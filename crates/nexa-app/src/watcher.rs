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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{
    CloseHandle, DuplicateHandle, GetLastError, DUPLICATE_SAME_ACCESS, ERROR_NOTIFY_ENUM_DIR,
    HANDLE, HWND, LPARAM, WAIT_OBJECT_0, WPARAM,
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
    /// 스레드 생존 신호(07-31 안정성 QA — OneDrive 저장 시 간헐 무갱신 보고).
    /// 스레드가 오류로 종료하면 false — [`sync_watchers`]가 죽은 항목을 감지해
    /// 재구독한다(이전에는 경로만 비교해 죽은 watcher가 "감시 중"으로 남아
    /// 그 폴더가 영구 무감시 = F5만 듣는 상태가 됐다).
    alive: Arc<AtomicBool>,
}

impl DirWatcher {
    /// 감시 스레드가 아직 살아 있는가 — false = 재구독 필요(자가 치유는 sync_watchers 몫).
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }
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
        let alive = Arc::new(AtomicBool::new(true));
        let alive_thread = alive.clone();
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
                    // 07-31 안정성 QA: ERROR_NOTIFY_ENUM_DIR = 변경 폭주로 통지 버퍼가
                    // 넘쳤으니 **전체 재열거하라**는 복구 가능 신호(엑셀 저장의 임시 파일
                    // 연쇄 + OneDrive 동기화가 겹치면 발생). 앱은 어차피 개별 엔트리를
                    // 해석하지 않고 전체 재열거로 수렴하므로 통지 1회 후 감시를 계속한다
                    // — 이전에는 무조건 break로 스레드가 죽어 그 폴더가 영구 무갱신이 됐다.
                    if unsafe { GetLastError() } == ERROR_NOTIFY_ENUM_DIR {
                        unsafe {
                            let _ =
                                PostMessageW(Some(hwnd), msg, WPARAM(panel), LPARAM(gen as isize));
                        }
                        continue;
                    }
                    break; // 폴더 소실·핸들 무효 등 — 종료(재구독은 sync_watchers 몫)
                }
                // 개별 엔트리는 해석하지 않는다 — 디바운스 후 전체 재열거로 수렴(원본 동일).
                // got==0(버퍼 오버플로·대량 변경)도 같은 경로.
                unsafe {
                    let _ = PostMessageW(Some(hwnd), msg, WPARAM(panel), LPARAM(gen as isize));
                }
            }
            alive_thread.store(false, Ordering::Relaxed); // 죽음 공표 → 재구독 대상
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
            alive,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 07-31 안정성 QA 계약: 감시 폴더가 소실되면 스레드가 죽고, 그 죽음이
    /// `is_alive()=false`로 **관측 가능**해야 한다(sync_watchers 재구독의 전제 —
    /// 이전에는 죽음이 은폐돼 그 폴더가 영구 무갱신이 됐다).
    #[test]
    fn watcher_death_is_observable_when_dir_removed() {
        let base = std::env::temp_dir().join(format!("nexa_watch_alive_{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let w = unsafe { DirWatcher::start(HWND::default(), 0x8002, 0, 1, &base) }
            .expect("임시 폴더 감시 시작");
        assert!(w.is_alive(), "시작 직후는 생존");
        // FILE_SHARE_DELETE로 열었으므로 삭제 가능 — 진행 중 RDC가 오류로 완료 → 종료
        std::fs::remove_dir(&base).unwrap();
        for _ in 0..100 {
            if !w.is_alive() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        panic!("폴더 소실 5초 후에도 is_alive=true — 죽음이 은폐됨");
    }
}
