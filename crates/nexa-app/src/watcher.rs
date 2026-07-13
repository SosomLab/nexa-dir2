//! 폴더 **자동 갱신 watcher**(M3-6) — 원본 `FolderWatcher.cs`(B-12w 1차·docs/33) 이식.
//!
//! 성능 규약(원본 동일): **비재귀**(패널 현재 폴더만) · 변경은 win.rs가 **300ms 디바운스**
//! 코얼레싱해 1회 재로드(무간섭 — 펼침·선택·캐럿·스크롤 보존 `reopen_filtered`) ·
//! 감시 실패(권한 등)는 무시하고 수동 F5 폴백.
//!
//! 구현: `ReadDirectoryChangesW` 동기 루프 스레드(패널당 1개) — 변경 시
//! `PostMessage(msg, panel, gen)`. **중지 = 디렉터리 핸들 닫기**(대기 중 호출이 오류로
//! 풀려 스레드 자연 종료 — Drop에서 수행). 낡은 스레드의 통지는 **세대 가드**로 무시
//! (원본 A-1 계승). 한계(α): 펼친 하위 폴더 변경은 비감시(재귀 금지 규약 — F5).

use std::path::{Path, PathBuf};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, WPARAM};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadDirectoryChangesW, FILE_FLAG_BACKUP_SEMANTICS, FILE_LIST_DIRECTORY,
    FILE_NOTIFY_CHANGE_ATTRIBUTES, FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME,
    FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

/// 폴더 1개 감시(패널당 1개) — drop 시 핸들을 닫아 스레드를 종료시킨다.
pub struct DirWatcher {
    pub path: PathBuf,
    pub gen: u64,
    /// 디렉터리 핸들 원시값(스레드와 공유 — 닫기 = 중지 신호).
    handle: isize,
}

impl DirWatcher {
    /// `path` 감시 시작. 실패(권한·소실 등) 시 `None` — 수동 F5 폴백(원본 규약).
    ///
    /// # Safety
    /// `hwnd`는 프로세스 수명 동안 유효한 자기 창(파괴 전 watcher가 먼저 drop — State 소유).
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
        let handle = CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_LIST_DIRECTORY.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS, // 디렉터리 핸들
            None,
        )
        .ok()?;
        let hwnd_raw = hwnd.0 as isize;
        let h_raw = handle.0 as isize;
        std::thread::spawn(move || {
            let hwnd = HWND(hwnd_raw as *mut core::ffi::c_void);
            let handle = HANDLE(h_raw as *mut core::ffi::c_void);
            let mut buf = vec![0u8; 8 * 1024];
            loop {
                let mut got = 0u32;
                let r = unsafe {
                    ReadDirectoryChangesW(
                        handle,
                        buf.as_mut_ptr() as *mut core::ffi::c_void,
                        buf.len() as u32,
                        false, // 비재귀(성능 규약 — docs/33)
                        FILE_NOTIFY_CHANGE_FILE_NAME
                            | FILE_NOTIFY_CHANGE_DIR_NAME
                            | FILE_NOTIFY_CHANGE_SIZE
                            | FILE_NOTIFY_CHANGE_LAST_WRITE
                            | FILE_NOTIFY_CHANGE_ATTRIBUTES,
                        Some(&mut got),
                        None,
                        None,
                    )
                };
                if r.is_err() {
                    break; // 핸들 닫힘(중지) 또는 오류 — 스레드 종료
                }
                // 개별 엔트리는 해석하지 않는다 — 디바운스 후 전체 재열거로 수렴(원본 동일).
                // got==0(버퍼 오버플로·대량 변경)도 같은 경로.
                unsafe {
                    let _ = PostMessageW(Some(hwnd), msg, WPARAM(panel), LPARAM(gen as isize));
                }
            }
        });
        Some(DirWatcher {
            path: path.to_path_buf(),
            gen,
            handle: h_raw,
        })
    }
}

impl Drop for DirWatcher {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(HANDLE(self.handle as *mut core::ffi::c_void));
        }
    }
}
