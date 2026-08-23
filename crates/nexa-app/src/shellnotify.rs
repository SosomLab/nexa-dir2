//! 셸 변경 통지 구독(X-44 5차 — 사용자 확정 08-23): 클라우드 동기화가 디스크에
//! 반영되는 순간을 **이벤트로** 받는다.
//!
//! ## 왜 필요한가 (08-23 실측)
//!
//! OneDrive가 원격 생성분을 내려놓는 플레이스홀더 생성(CfCreatePlaceholders)은
//! **부모 폴더 mtime도 RDCW 통지도 남기지 않는다** — 파일시스템 계층에서는 완전한
//! 침묵이다. 그런데 탐색기는 즉시 갱신된다: OneDrive가 **셸에는 `SHChangeNotify`로
//! 직접 알리기** 때문. 같은 채널(`SHChangeNotifyRegister` — shell32 인박스·관리자
//! 권한 불요)을 구독하면 우리도 그 순간을 받는다.
//!
//! ## 규약
//!
//! - 패널 루트(활성 탭)를 **재귀** 구독 — 하위 전체의 셸 이벤트가 창 메시지로 온다.
//!   pidl 필터는 등록 시 셸이 수행하므로 수신 측은 내용 해석 없이 **디바운스 재로드
//!   경로에 합류**만 한다(폭주는 300ms 디바운스 + 1s 상한이 흡수 — watcher 동일).
//! - 이 채널은 "제공자가 쏴 줄 때만" 온다(OneDrive·탐색기 = 확실, 일부 도구 = 미보장)
//!   → RDCW watcher·프로브 스윕은 **보험으로 존치**(스윕은 감속 — win.rs).
//! - `SHCNRF_NewDelivery` 사용 — 수신 시 Lock/Unlock으로 페이로드를 해제해야 한다
//!   (내용은 안 쓰지만 잠금 해제가 자원 반납이다). 등록 해제는 Drop.

use std::path::{Path, PathBuf};

use windows::core::HSTRING;
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::UI::Shell::{
    ILFree, SHChangeNotification_Lock, SHChangeNotification_Unlock, SHChangeNotifyDeregister,
    SHChangeNotifyEntry, SHChangeNotifyRegister, SHParseDisplayName, SHCNE_DISKEVENTS,
    SHCNRF_NewDelivery, SHCNRF_ShellLevel,
};

/// 패널 1개의 셸 변경 구독 — 경로가 바뀌면 재등록(sync는 win.rs 길목 몫).
pub struct ShellWatch {
    /// 구독 중인 루트(재등록 판정용).
    pub path: PathBuf,
    id: u32,
}

impl ShellWatch {
    /// `path`(실폴더)를 재귀 구독. 가상 루트·클라우드 센티널 등 셸 네임스페이스로
    /// 해석 불가한 경로는 `None`(그 패널은 watcher·프로브 보험만).
    ///
    /// # Safety
    /// `hwnd`는 프로세스 수명 동안 유효한 자기 창. UI 스레드에서 호출.
    pub unsafe fn register(hwnd: HWND, msg: u32, path: &Path) -> Option<ShellWatch> {
        let mut pidl = std::ptr::null_mut();
        SHParseDisplayName(
            &HSTRING::from(path.as_os_str()),
            None,
            &mut pidl,
            0,
            None,
        )
        .ok()?;
        let entry = SHChangeNotifyEntry {
            pidl,
            fRecursive: true.into(),
        };
        let id = SHChangeNotifyRegister(
            hwnd,
            SHCNRF_ShellLevel | SHCNRF_NewDelivery,
            SHCNE_DISKEVENTS.0 as i32,
            msg,
            1,
            &entry,
        );
        ILFree(Some(pidl)); // 등록이 엔트리를 복사한다 — 원본 해제
        (id != 0).then(|| ShellWatch {
            path: path.to_path_buf(),
            id,
        })
    }
}

impl Drop for ShellWatch {
    fn drop(&mut self) {
        unsafe {
            let _ = SHChangeNotifyDeregister(self.id);
        }
    }
}

/// 수신 메시지의 페이로드 잠금 해제(NewDelivery 자원 반납) — 내용은 쓰지 않는다
/// (등록 pidl이 이미 범위를 필터했고, 앱은 전체 재열거로 수렴한다 — watcher 동일).
///
/// # Safety
/// `wparam`/`lparam`은 해당 등록 메시지의 원시 인자 그대로.
pub unsafe fn release_payload(wparam: usize, lparam: isize) {
    let hlock = SHChangeNotification_Lock(HANDLE(wparam as *mut _), lparam as u32, None, None);
    if !hlock.is_invalid() {
        let _ = SHChangeNotification_Unlock(hlock);
    }
}
