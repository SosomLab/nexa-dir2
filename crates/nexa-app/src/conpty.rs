//! ConPTY(의사 콘솔) 세션(M4-3) — **원본 이식**: `Terminal/ConPtySession.cs`(BP-T1·docs/37).
//!
//! 셸(pwsh→powershell→cmd)을 Windows Pseudo Console로 구동하고 입출력을 잇는다.
//! 셸이 내보내는 VT 바이트 스트림을 읽기 스레드가 UTF-8 디코드(멀티바이트 경계 보존)해
//! 공유 버퍼에 쌓고 `PostMessage(msg, panel, gen)`로 통지 — VT 파싱(nexa-term)·렌더는
//! win.rs. 종료 통지는 wparam에 [`EXIT_FLAG`]를 실어 구분. Windows 10 1809+.

use std::sync::{Arc, Mutex};

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, WPARAM};
use windows::Win32::Storage::FileSystem::{ReadFile, WriteFile};
use windows::Win32::System::Console::{
    ClosePseudoConsole, CreatePseudoConsole, ResizePseudoConsole, COORD, HPCON,
};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, InitializeProcThreadAttributeList,
    TerminateProcess, UpdateProcThreadAttribute, WaitForSingleObject, EXTENDED_STARTUPINFO_PRESENT,
    INFINITE, LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION, STARTUPINFOEXW,
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

        let mut cmdline: Vec<u16> = default_shell()
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
            PCWSTR::null(),
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
                let mut pending: Vec<u8> = Vec::new(); // 잘린 멀티바이트 보관
                let mut buf = [0u8; 4096];
                loop {
                    let mut read = 0u32;
                    let ok = unsafe { ReadFile(handle, Some(&mut buf), Some(&mut read), None) };
                    if ok.is_err() || read == 0 {
                        break; // EOF = 셸 종료(대기 스레드가 통지)
                    }
                    pending.extend_from_slice(&buf[..read as usize]);
                    // 유효 UTF-8 접두사만 디코드, 잘린 꼬리는 다음 읽기와 합침
                    let valid = match std::str::from_utf8(&pending) {
                        Ok(_) => pending.len(),
                        Err(e) => e.valid_up_to(),
                    };
                    if valid > 0 {
                        let text = String::from_utf8_lossy(&pending[..valid]).into_owned();
                        pending.drain(..valid);
                        out.lock().unwrap().push_str(&text);
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
        // 종료 대기 스레드 — Exited 통지(원본 WaitForExitAsync)
        {
            let (hwnd_raw, proc_raw) = (hwnd.0 as isize, pi.hProcess.0 as isize);
            std::thread::spawn(move || {
                let hwnd = HWND(hwnd_raw as *mut core::ffi::c_void);
                unsafe {
                    WaitForSingleObject(HANDLE(proc_raw as *mut core::ffi::c_void), INFINITE);
                    let _ = PostMessageW(
                        Some(hwnd),
                        msg,
                        WPARAM(panel | EXIT_FLAG),
                        LPARAM(gen as isize),
                    );
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

/// 기본 셸 — pwsh → powershell → cmd 순(원본 DefaultShell).
fn default_shell() -> String {
    for exe in ["pwsh.exe", "powershell.exe"] {
        if let Some(paths) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&paths) {
                if dir.as_os_str().is_empty() {
                    continue;
                }
                if dir.join(exe).is_file() {
                    return exe.to_string();
                }
            }
        }
    }
    "cmd.exe".into()
}
