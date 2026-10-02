//! ConPTY(의사 콘솔) 세션(M4-3) — **원본 이식**: `Terminal/ConPtySession.cs`(BP-T1·docs/37).
//!
//! 셸(pwsh→powershell→cmd)을 Windows Pseudo Console로 구동하고 입출력을 잇는다.
//! 셸이 내보내는 VT 바이트 스트림을 읽기 스레드가 UTF-8 디코드(멀티바이트 경계 보존)해
//! 공유 버퍼에 쌓고 `PostMessage(msg, panel, gen)`로 통지 — VT 파싱(nexa-term)·렌더는
//! win.rs. 종료 통지는 wparam에 [`EXIT_FLAG`]를 실어 구분. Windows 10 1809+.

use std::sync::{Arc, Mutex};

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, DuplicateHandle, DUPLICATE_SAME_ACCESS, HANDLE, HWND, LPARAM, WPARAM,
};
use windows::Win32::Storage::FileSystem::{ReadFile, WriteFile};
use windows::Win32::System::Console::{
    ClosePseudoConsole, CreatePseudoConsole, ResizePseudoConsole, COORD, HPCON,
};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, GetCurrentProcess,
    InitializeProcThreadAttributeList, TerminateProcess, UpdateProcThreadAttribute,
    WaitForSingleObject, EXTENDED_STARTUPINFO_PRESENT, INFINITE, LPPROC_THREAD_ATTRIBUTE_LIST,
    PROCESS_INFORMATION, STARTUPINFOEXW,
};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

/// 종료 통지 표식 — `wparam = panel | EXIT_FLAG`.
pub const EXIT_FLAG: usize = 0x100;
/// PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE(winbase.h) — windows-rs 미노출.
const ATTR_PSEUDOCONSOLE: usize = 0x0002_0016;

/// ConPTY 세션 1개(도크 터미널 1개) — drop 시 셸 종료·핸들 정리.
pub struct ConPty {
    hpc: isize,
    process: isize,
    thread: isize,
    /// 셸 stdin(우리가 쓰는 쪽).
    writer: isize,
    attr_list: *mut u8,
    attr_size: usize,
    pub gen: u64,
    /// 읽기 스레드가 쌓는 출력(UI가 드레인 후 VtScreen.feed).
    pub output: Arc<Mutex<String>>,
}

impl ConPty {
    /// 세션 시작 — `cols`×`rows` ConPTY로 셸 구동. 실패 시 None(도크에 오류 표시).
    ///
    /// # Safety
    /// UI 스레드에서 호출. `hwnd`는 프로세스 수명 동안 유효(세션은 State 소유 — 창보다 먼저 drop).
    pub unsafe fn start(
        hwnd: HWND,
        msg: u32,
        panel: usize,
        gen: u64,
        cwd: &std::path::Path,
        cols: i16,
        rows: i16,
    ) -> Option<ConPty> {
        use std::os::windows::ffi::OsStrExt;
        // 파이프: 셸 stdin ← input_read(ConPTY 소유) / 우리는 input_write.
        //         셸 stdout → output_write(ConPTY 소유) / 우리는 output_read.
        let (mut input_read, mut input_write) = (HANDLE::default(), HANDLE::default());
        let (mut output_read, mut output_write) = (HANDLE::default(), HANDLE::default());
        CreatePipe(&mut input_read, &mut input_write, None, 0).ok()?;
        if CreatePipe(&mut output_read, &mut output_write, None, 0).is_err() {
            let _ = CloseHandle(input_read);
            let _ = CloseHandle(input_write);
            return None;
        }
        let hpc = match CreatePseudoConsole(
            COORD {
                X: cols.max(2),
                Y: rows.max(2),
            },
            input_read,
            output_write,
            0,
        ) {
            Ok(h) => h,
            Err(_) => {
                for h in [input_read, input_write, output_read, output_write] {
                    let _ = CloseHandle(h);
                }
                return None;
            }
        };
        // ConPTY가 소유하는 끝은 우리 사본을 닫는다 → 셸 종료 시 output_read에 EOF 전파(원본).
        let _ = CloseHandle(input_read);
        let _ = CloseHandle(output_write);

        // STARTUPINFOEX + PSEUDOCONSOLE 속성으로 셸 기동
        let mut size = 0usize;
        let _ = InitializeProcThreadAttributeList(None, 1, None, &mut size);
        let attr_list = std::alloc::alloc(std::alloc::Layout::from_size_align(size, 8).ok()?);
        let list = LPPROC_THREAD_ATTRIBUTE_LIST(attr_list as *mut core::ffi::c_void);
        if InitializeProcThreadAttributeList(Some(list), 1, None, &mut size).is_err()
            || UpdateProcThreadAttribute(
                list,
                0,
                ATTR_PSEUDOCONSOLE,
                Some(hpc.0 as *const core::ffi::c_void),
                std::mem::size_of::<isize>(),
                None,
                None,
            )
            .is_err()
        {
            ClosePseudoConsole(hpc);
            return None;
        }
        let mut si = STARTUPINFOEXW::default();
        si.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
        si.lpAttributeList = list;

        // 셸은 **전체 경로**로 실행(점검 1차 #3 — 종전은 "pwsh.exe" 이름만 넘겨 NULL 앱 이름 검색
        // 규칙[앱 폴더·CWD 우선]에 노출 = 바이너리 플랜팅). lpApplicationName에 경로를, 명령줄에는
        // 인용한 같은 경로를 준다(argv[0]).
        let shell = default_shell();
        let app_w: Vec<u16> = shell
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut cmdline: Vec<u16> = format!("\"{}\"", shell.display())
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let cwd_w: Vec<u16> = cwd
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut pi = PROCESS_INFORMATION::default();
        if CreateProcessW(
            PCWSTR(app_w.as_ptr()),
            Some(PWSTR(cmdline.as_mut_ptr())),
            None,
            None,
            false,
            EXTENDED_STARTUPINFO_PRESENT,
            None,
            PCWSTR(cwd_w.as_ptr()),
            &si.StartupInfo,
            &mut pi,
        )
        .is_err()
        {
            ClosePseudoConsole(hpc);
            let _ = CloseHandle(input_write);
            let _ = CloseHandle(output_read);
            return None;
        }

        let output = Arc::new(Mutex::new(String::new()));
        // 읽기 스레드 — VT 바이트 → UTF-8 디코드(경계 보존) → 공유 버퍼 + 통지
        {
            let out = output.clone();
            let (hwnd_raw, read_raw) = (hwnd.0 as isize, output_read.0 as isize);
            std::thread::spawn(move || {
                let hwnd = HWND(hwnd_raw as *mut core::ffi::c_void);
                let handle = HANDLE(read_raw as *mut core::ffi::c_void);
                let mut chunker = Utf8Chunker::new(); // 잘린 멀티바이트 보관
                let mut buf = [0u8; 4096];
                loop {
                    let mut read = 0u32;
                    let ok = unsafe { ReadFile(handle, Some(&mut buf), Some(&mut read), None) };
                    if ok.is_err() || read == 0 {
                        break; // EOF = 셸 종료(대기 스레드가 통지)
                    }
                    // 디코드는 순수 타입(Utf8Chunker)이 맡는다 — 스레드는 호출만
                    if let Some(text) = chunker.push(&buf[..read as usize]) {
                        crate::win::plock(&out).push_str(&text);
                        unsafe {
                            let _ =
                                PostMessageW(Some(hwnd), msg, WPARAM(panel), LPARAM(gen as isize));
                        }
                    }
                }
                unsafe {
                    let _ = CloseHandle(handle);
                }
            });
        }
        // 종료 대기 스레드 — Exited 통지(원본 WaitForExitAsync).
        // 스레드에는 **복제 핸들**을 넘긴다(07-31 안정성 QA — watcher 동일 규약):
        // Drop이 원본 process 핸들을 닫은 뒤 핸들 값이 재활용되면 원시값을 든
        // 대기 스레드가 **무관 객체를 영원히 대기**(스레드 누수 + 낡은 통지)한다.
        // 복제 실패(사실상 불발)는 원시값 폴백 — 종전 동작 유지.
        {
            let mut proc_thread = HANDLE(pi.hProcess.0);
            let dup_ok = DuplicateHandle(
                GetCurrentProcess(),
                pi.hProcess,
                GetCurrentProcess(),
                &mut proc_thread,
                0,
                false,
                DUPLICATE_SAME_ACCESS,
            )
            .is_ok();
            let (hwnd_raw, proc_raw) = (hwnd.0 as isize, proc_thread.0 as isize);
            std::thread::spawn(move || {
                let hwnd = HWND(hwnd_raw as *mut core::ffi::c_void);
                let proc = HANDLE(proc_raw as *mut core::ffi::c_void);
                unsafe {
                    WaitForSingleObject(proc, INFINITE);
                    let _ = PostMessageW(
                        Some(hwnd),
                        msg,
                        WPARAM(panel | EXIT_FLAG),
                        LPARAM(gen as isize),
                    );
                    if dup_ok {
                        let _ = CloseHandle(proc); // 스레드 소유 복제분
                    }
                }
            });
        }

        Some(ConPty {
            hpc: hpc.0 as isize,
            process: pi.hProcess.0 as isize,
            thread: pi.hThread.0 as isize,
            writer: input_write.0 as isize,
            attr_list,
            attr_size: size,
            gen,
            output,
        })
    }

    /// 사용자 입력을 셸 stdin으로(UTF-8). 실패(셸 종료 등)는 격리.
    pub fn write(&self, text: &str) {
        if text.is_empty() {
            return;
        }
        let bytes = text.as_bytes();
        unsafe {
            let _ = WriteFile(
                HANDLE(self.writer as *mut core::ffi::c_void),
                Some(bytes),
                None,
                None,
            );
        }
    }

    /// 터미널 크기 변경 → ConPTY 리사이즈.
    pub fn resize(&self, cols: i16, rows: i16) {
        if cols <= 0 || rows <= 0 {
            return;
        }
        unsafe {
            let _ = ResizePseudoConsole(HPCON(self.hpc), COORD { X: cols, Y: rows });
        }
    }
}

impl Drop for ConPty {
    fn drop(&mut self) {
        unsafe {
            let process = HANDLE(self.process as *mut core::ffi::c_void);
            let _ = TerminateProcess(process, 0);
            let _ = CloseHandle(HANDLE(self.writer as *mut core::ffi::c_void));
            ClosePseudoConsole(HPCON(self.hpc));
            if !self.attr_list.is_null() {
                DeleteProcThreadAttributeList(LPPROC_THREAD_ATTRIBUTE_LIST(
                    self.attr_list as *mut core::ffi::c_void,
                ));
                if let Ok(layout) = std::alloc::Layout::from_size_align(self.attr_size, 8) {
                    std::alloc::dealloc(self.attr_list, layout);
                }
            }
            let _ = CloseHandle(HANDLE(self.thread as *mut core::ffi::c_void));
            let _ = CloseHandle(process);
        }
    }
}

/// 기본 셸 — pwsh → powershell → cmd 순(원본 DefaultShell). **전체 경로**를 돌려준다(점검 1차 #3):
/// PATH에서 찾은 경로 그대로 · 못 찾으면 `%SystemRoot%\System32\cmd.exe`(이름 검색에 기대지 않는다).
fn default_shell() -> std::path::PathBuf {
    for exe in ["pwsh.exe", "powershell.exe"] {
        if let Some(paths) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&paths) {
                if dir.as_os_str().is_empty() || dir.is_relative() {
                    continue; // 상대 경로 항목(".")은 CWD 기준 = 플랜팅 경로 — 제외
                }
                let cand = dir.join(exe);
                if cand.is_file() {
                    return cand;
                }
            }
        }
    }
    let sysroot = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    std::path::Path::new(&sysroot)
        .join("System32")
        .join("cmd.exe")
}

/// 읽기 스레드의 UTF-8 디코더 — **순수 타입**(점검 2차 G9-01·G9-11).
///
/// 바이트를 밀어 넣으면 지금 디코드할 수 있는 만큼을 `String`으로 돌려주고,
/// 멀티바이트 문자가 읽기 경계에서 잘린 **꼬리(≤3바이트)는 보존**해 다음 호출과 합친다.
/// 확정 **불량 바이트**(`Utf8Error::error_len() == Some`)는 U+FFFD로 치환하고 건너뛴다 —
/// 종전 코드는 `valid_up_to()`만 보고 `valid == 0`이면 아무것도 하지 않아, 불량 바이트 1개가
/// 선두에 오는 순간(위치 k>0이어도 앞 k바이트가 비워진 다음 라운드에 선두가 된다)
/// 이후 셸 출력 전부가 `pending`에만 쌓이고 화면은 영구 침묵·메모리는 무한 증가했다.
/// 유효 입력·경계 분할 입력의 출력 바이트열은 종전과 동일.
pub(crate) struct Utf8Chunker {
    pending: Vec<u8>,
}

impl Utf8Chunker {
    pub(crate) fn new() -> Self {
        Self {
            pending: Vec::new(),
        }
    }

    /// `bytes`를 덧붙이고 디코드 가능한 텍스트를 돌려준다. 돌려줄 것이 없으면(잘린 꼬리만) `None`.
    pub(crate) fn push(&mut self, bytes: &[u8]) -> Option<String> {
        self.pending.extend_from_slice(bytes);
        let mut out = String::new();
        let mut consumed = 0usize;
        loop {
            match std::str::from_utf8(&self.pending[consumed..]) {
                Ok(s) => {
                    out.push_str(s);
                    consumed = self.pending.len();
                    break;
                }
                Err(e) => {
                    let valid = e.valid_up_to();
                    // `valid_up_to()` 앞은 유효가 보장된 접두사 — 재검사 없이 그대로
                    out.push_str(
                        std::str::from_utf8(&self.pending[consumed..consumed + valid])
                            .unwrap_or(""),
                    );
                    consumed += valid;
                    match e.error_len() {
                        None => break, // 잘린 꼬리 — 다음 읽기와 합친다
                        Some(n) => {
                            out.push('\u{FFFD}'); // 확정 불량 바이트 — 치환하고 건너뛴다
                            consumed += n;
                        }
                    }
                }
            }
        }
        self.pending.drain(..consumed);
        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    }

    /// 아직 디코드하지 못한 꼬리 길이(항상 < 4).
    #[cfg(test)]
    pub(crate) fn pending_len(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::Utf8Chunker;

    #[test]
    fn default_shell_is_absolute_and_exists() {
        let p = super::default_shell();
        assert!(p.is_absolute(), "{p:?}");
        assert!(p.is_file(), "{p:?}");
    }

    #[test]
    fn chunker_valid_text_passes_through() {
        let mut c = Utf8Chunker::new();
        assert_eq!(
            c.push("abc 한글 \x1b[31mé".as_bytes()).as_deref(),
            Some("abc 한글 \x1b[31mé")
        );
        assert_eq!(c.pending_len(), 0);
        assert_eq!(c.push(b""), None);
    }

    #[test]
    fn chunker_invalid_leading_byte_is_replaced_not_stuck() {
        // G9-01 ①: 종전에는 valid_up_to()==0 → 영구 정지
        let mut c = Utf8Chunker::new();
        assert_eq!(c.push(&[0xFF, b'a']).as_deref(), Some("\u{FFFD}a"));
        assert_eq!(c.pending_len(), 0);
        // 이후 출력도 계속 흐른다
        assert_eq!(c.push(b"ok").as_deref(), Some("ok"));
    }

    #[test]
    fn chunker_invalid_byte_in_middle_is_replaced() {
        // 위치 k>0도 다음 라운드에 선두가 되어 같은 정지에 빠지던 경우
        let mut c = Utf8Chunker::new();
        assert_eq!(
            c.push(&[b'a', b'b', 0xFF, b'c']).as_deref(),
            Some("ab\u{FFFD}c")
        );
        assert_eq!(c.pending_len(), 0);
        // 불량 바이트 2개 연속·잘린 시퀀스 뒤에 ASCII(= error_len Some(1))
        assert_eq!(
            c.push(&[0xC3, b'x', 0xE1, 0x80, b'y']).as_deref(),
            Some("\u{FFFD}x\u{FFFD}y")
        );
        assert_eq!(c.pending_len(), 0);
    }

    #[test]
    fn chunker_split_multibyte_one_byte_at_a_time() {
        // G9-01 ②: "한" = ED 95 9C 를 1바이트씩
        let bytes = "한".as_bytes();
        let mut c = Utf8Chunker::new();
        assert_eq!(c.push(&bytes[0..1]), None);
        assert_eq!(c.pending_len(), 1);
        assert_eq!(c.push(&bytes[1..2]), None);
        assert_eq!(c.pending_len(), 2);
        assert_eq!(c.push(&bytes[2..3]).as_deref(), Some("한"));
        assert_eq!(c.pending_len(), 0);
    }

    #[test]
    fn chunker_keeps_tail_and_joins_with_next_read() {
        // G9-01 ③: [a, C3] → "a"·꼬리 [C3], 이어 [95] → "Õ"
        let mut c = Utf8Chunker::new();
        assert_eq!(c.push(&[b'a', 0xC3]).as_deref(), Some("a"));
        assert_eq!(c.pending_len(), 1);
        assert_eq!(c.push(&[0x95]).as_deref(), Some("Õ"));
        assert_eq!(c.pending_len(), 0);
    }

    #[test]
    fn chunker_split_valid_stream_equals_whole() {
        // 동작 불변: 유효 입력을 아무 경계로 잘라 넣어도 이어 붙인 출력은 원문과 같다
        let text = "줄1 한글 ✓ \x1b[1;32mé\x1b[0m 🙂 끝\r\n".repeat(37);
        let bytes = text.as_bytes();
        for step in [1usize, 2, 3, 5, 7, 11, 4096] {
            let mut c = Utf8Chunker::new();
            let mut joined = String::new();
            for chunk in bytes.chunks(step) {
                if let Some(s) = c.push(chunk) {
                    joined.push_str(&s);
                }
            }
            assert_eq!(joined, text, "step {step}");
            assert_eq!(c.pending_len(), 0, "step {step}");
        }
    }

    #[test]
    fn chunker_random_bytes_never_accumulate() {
        // G9-01 ④: 랜덤 100KB — 어떤 입력에도 꼬리는 4바이트 미만·출력은 멈추지 않는다
        let mut x = 0x9E37_79B9_7F4A_7C15u64; // 결정적 xorshift
        let mut c = Utf8Chunker::new();
        let mut total_out = 0usize;
        let mut fed = 0usize;
        while fed < 100 * 1024 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let len = (x % 97) as usize + 1;
            let chunk: Vec<u8> = (0..len)
                .map(|i| (x.rotate_left((i % 8) as u32 * 8) & 0xFF) as u8)
                .collect();
            fed += chunk.len();
            if let Some(s) = c.push(&chunk) {
                total_out += s.len();
            }
            assert!(c.pending_len() < 4, "pending {}", c.pending_len());
        }
        assert!(total_out > 0);
        // 마지막에 ASCII를 넣으면 남은 꼬리까지 전부 비워진다(치환 또는 포함)
        let _ = c.push(b"z");
        assert_eq!(c.pending_len(), 0);
    }
}
