//! 폴더 **자동 갱신 watcher**(M3-6) — 원본 `FolderWatcher.cs`(B-12w 1차·docs/33) 이식.
//!
//! 성능 규약(원본 동일): **비재귀**(패널 현재 폴더만) · 변경은 win.rs가 **300ms 디바운스**
//! 코얼레싱해 1회 재로드(무간섭 — 펼침·선택·캐럿·스크롤 보존 `reopen_filtered`) ·
//! 감시 실패(권한 등)는 무시하고 수동 F5 폴백.
//!
//! 구현(QA 07-14 개정): `ReadDirectoryChangesW` **OVERLAPPED** + 중지 이벤트 스레드.
//! 초판(동기 호출 + drop=CloseHandle)은 **UI 프리즈 결함** — 비 OVERLAPPED 핸들은
//! 파일 객체 잠금이 직렬화되어, 대기 중인 동기 ReadDirectoryChangesW가 끝날 때까지
//! CloseHandle이 블로킹된다(= 이전 폴더에 변경이 생길 때까지 이동이 멈춤 — 조용한
//! OneDrive 폴더에서 수십 초). 지금은 **drop=SetEvent(논블로킹)**, 핸들 정리는 스레드
//! 자신이 수행. 낡은 스레드의 통지는 **세대 가드**로 무시(원본 A-1 계승).
//! 한계(α): 펼친 하위 폴더 변경은 비감시(재귀 금지 규약 — F5).

use std::path::{Path, PathBuf};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{
    CloseHandle, DuplicateHandle, DUPLICATE_SAME_ACCESS, HANDLE, HWND, LPARAM, WAIT_OBJECT_0,
    WPARAM,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadDirectoryChangesW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OVERLAPPED,
    FILE_LIST_DIRECTORY, FILE_NOTIFY_CHANGE_ATTRIBUTES, FILE_NOTIFY_CHANGE_DIR_NAME,
    FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Threading::{
    CreateEventW, GetCurrentProcess, SetEvent, WaitForMultipleObjects, INFINITE,
};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

/// 폴더 1개 감시(패널당 1개) — drop 시 중지 이벤트만 신호(논블로킹, 정리는 스레드 몫).
pub struct DirWatcher {
    pub path: PathBuf,
    pub gen: u64,
    /// 중지 이벤트(수동 리셋) 원시값 — 소유는 이 구조체(drop에서 SetEvent 후 CloseHandle).
    stop: isize,
}

impl DirWatcher {
    /// `path` 감시 시작. 실패(권한·소실 등) 시 `None` — 수동 F5 폴백(원본 규약).
    ///
    /// # Safety
    /// `hwnd`는 프로세스 수명 동안 유효한 자기 창(파괴 후 통지는 PostMessage 실패로 무해).
    pub unsafe fn start(
        hwnd: HWND,
        msg: u32,
        panel: usize,
        gen: u64,
        path: &Path,
    ) -> Option<DirWatcher> {
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let dir = CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_LIST_DIRECTORY.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED, // 디렉터리 + 비동기(정지 가능)
            None,
        )
        .ok()?;
        let stop = match CreateEventW(None, true, false, None) {
            Ok(h) => h,
            Err(_) => {
                let _ = CloseHandle(dir);
                return None;
            }
        };
        let io = match CreateEventW(None, false, false, None) {
            Ok(h) => h,
            Err(_) => {
                let _ = CloseHandle(dir);
                let _ = CloseHandle(stop);
                return None;
            }
        };
        // 스레드에는 **복제 핸들**을 넘긴다 — drop이 원본을 닫아도 스레드 참조가 이벤트
        // 객체를 유지(핸들 값 재활용으로 무관 객체를 기다리는 경쟁 차단).
        let mut stop_thread = HANDLE::default();
        if DuplicateHandle(
            GetCurrentProcess(),
            stop,
            GetCurrentProcess(),
            &mut stop_thread,
            0,
            false,
            DUPLICATE_SAME_ACCESS,
        )
        .is_err()
        {
            let _ = CloseHandle(dir);
            let _ = CloseHandle(stop);
            let _ = CloseHandle(io);
            return None;
        }
        let hwnd_raw = hwnd.0 as isize;
        let (dir_raw, stop_raw, io_raw) = (dir.0 as isize, stop_thread.0 as isize, io.0 as isize);
        std::thread::spawn(move || {
            let hwnd = HWND(hwnd_raw as *mut core::ffi::c_void);
            let dir = HANDLE(dir_raw as *mut core::ffi::c_void);
            let io = HANDLE(io_raw as *mut core::ffi::c_void);
            let stop = HANDLE(stop_raw as *mut core::ffi::c_void);
            let mut buf = vec![0u8; 8 * 1024];
            loop {
                let mut ov = OVERLAPPED {
                    hEvent: io,
                    ..Default::default()
                };
                let queued = unsafe {
                    ReadDirectoryChangesW(
                        dir,
                        buf.as_mut_ptr() as *mut core::ffi::c_void,
                        buf.len() as u32,
                        false, // 비재귀(성능 규약 — docs/33)
                        FILE_NOTIFY_CHANGE_FILE_NAME
                            | FILE_NOTIFY_CHANGE_DIR_NAME
                            | FILE_NOTIFY_CHANGE_SIZE
                            | FILE_NOTIFY_CHANGE_LAST_WRITE
                            | FILE_NOTIFY_CHANGE_ATTRIBUTES,
                        None,
                        Some(&mut ov),
                        None,
                    )
                };
                if queued.is_err() {
                    break; // 핸들 무효·권한 상실 등 — 종료
                }
                // 중지 신호 또는 변경 도착까지 대기(WAIT_FAILED도 종료 경로 공용)
                let wait = unsafe { WaitForMultipleObjects(&[stop, io], false, INFINITE) };
                if wait.0 != WAIT_OBJECT_0.0 + 1 {
                    // 중지(또는 실패) — 진행 중 IO 취소 후 종료
                    unsafe {
                        let _ = CancelIoEx(dir, Some(&ov));
                        let mut n = 0u32;
                        let _ = GetOverlappedResult(dir, &ov, &mut n, true);
                    }
                    break;
                }
                let mut got = 0u32;
                if unsafe { GetOverlappedResult(dir, &ov, &mut got, false) }.is_err() {
                    break;
                }
                // 개별 엔트리는 해석하지 않는다 — 디바운스 후 전체 재열거로 수렴(원본 동일).
                // got==0(버퍼 오버플로·대량 변경)도 같은 경로.
                unsafe {
                    let _ = PostMessageW(Some(hwnd), msg, WPARAM(panel), LPARAM(gen as isize));
                }
            }
            unsafe {
                let _ = CloseHandle(dir);
                let _ = CloseHandle(io);
                let _ = CloseHandle(stop); // 스레드 소유 복제분
            }
        });
        Some(DirWatcher {
            path: path.to_path_buf(),
            gen,
            stop: stop.0 as isize,
        })
    }
}

impl Drop for DirWatcher {
    fn drop(&mut self) {
        unsafe {
            let stop = HANDLE(self.stop as *mut core::ffi::c_void);
            let _ = SetEvent(stop); // 논블로킹 — 스레드가 IO 취소·핸들 정리(QA 07-14 프리즈 수정)
            let _ = CloseHandle(stop);
        }
    }
}
