//! 셸 컨텍스트 메뉴 **격리 재현기 + 단계별 타이밍**(10-02 — 우클릭 즉사·지연 분석용).
//! 인수로 받은 경로의 `IContextMenu`를 `shellmenu::show_inner`/`run_menu`와 같은 순서로
//! 취득해 `QueryContextMenu` → verb 조회(GetCommandString ×N) → "새로 만들기" 병합
//! (CLSID_NewMenu)까지 수행한다(메뉴 표시 없음). 메뉴 확장(서드파티 DLL)이 프로세스
//! 내부에서 로드·실행되는 구간을 그대로 밟으므로 종료 코드로 즉사 여부를, 각 단계의
//! 소요 ms로 지연 원인을 판정한다. **2회 반복** = 콜드(DLL 로드·JIT 포함) vs 웜.
//!
//! ```text
//! cargo run --release -p nexa-app --example ctxmenu_probe -- "<경로>" [반복수]
//! ```
//! 종료 코드: 0 = 정상 · 0xC0000409 = fast-fail(CET 등) · 그 외 = 예외.

#[cfg(windows)]
fn main() {
    win::run();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Windows 전용(셸 컨텍스트 메뉴 재현기)");
}

#[cfg(windows)]
mod win {
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    use windows::core::{Interface, PCWSTR, PSTR};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
    };
    use windows::Win32::UI::Shell::Common::ITEMIDLIST;
    use windows::Win32::UI::Shell::{
        IContextMenu, IShellExtInit, IShellFolder, SHBindToParent, SHParseDisplayName,
        CMF_NORMAL, GCS_VERBW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreatePopupMenu, DestroyMenu, GetMenuItemCount, GetMenuItemID,
    };

    fn ms(t: Instant) -> f64 {
        t.elapsed().as_secs_f64() * 1000.0
    }

    unsafe fn parse(path: &Path) -> *mut ITEMIDLIST {
        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
        SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut pidl, 0, None)
            .expect("SHParseDisplayName");
        pidl
    }

    unsafe fn once(path: &Path, round: usize, flags: u32) {
        let total = Instant::now();
        let t = Instant::now();
        let pidl = parse(path);
        let t_parse = ms(t);
        let t = Instant::now();
        let mut child: *mut ITEMIDLIST = std::ptr::null_mut();
        let folder =
            SHBindToParent::<IShellFolder>(pidl, Some(&mut child)).expect("SHBindToParent");
        let t_bind = ms(t);
        let t = Instant::now();
        let children = [child as *const ITEMIDLIST];
        let icm = folder
            .GetUIObjectOf::<IContextMenu>(HWND::default(), &children, None)
            .expect("GetUIObjectOf(IContextMenu)");
        let t_uiobj = ms(t);
        let hmenu = CreatePopupMenu().expect("CreatePopupMenu");
        let t = Instant::now();
        let r = icm.QueryContextMenu(hmenu, 0, 1, 0x6FFF, flags);
        let t_query = ms(t);
        let n = GetMenuItemCount(Some(hmenu));
        // run_menu 2-0) 제자리 대체를 위한 verb 조회 — 항목마다 GetCommandString
        let t = Instant::now();
        let mut verbs = 0;
        for pos in 0..n.max(0) {
            let id = GetMenuItemID(hmenu, pos);
            if !(1..=0x6FFF).contains(&id) {
                continue;
            }
            let mut buf = [0u16; 512];
            if icm
                .GetCommandString(
                    (id - 1) as usize,
                    GCS_VERBW,
                    None,
                    PSTR(buf.as_mut_ptr() as *mut u8),
                    buf.len() as u32,
                )
                .is_ok()
            {
                verbs += 1;
            }
        }
        let t_verbs = ms(t);
        // run_menu 2-2) "새로 만들기" 병합 — CLSID_NewMenu를 부모 폴더로 초기화 + Query
        let t = Instant::now();
        let mut t_new = 0.0;
        let mut new_items = 0;
        if let Some(dir) = path.parent() {
            const CLSID_NEW_MENU: windows::core::GUID =
                windows::core::GUID::from_u128(0xD969A300_E7FF_11D0_A93B_00A0C90F2719);
            if let Ok(unk) = CoCreateInstance::<_, windows::core::IUnknown>(
                &CLSID_NEW_MENU,
                None,
                CLSCTX_INPROC_SERVER,
            ) {
                if let Ok(init) = unk.cast::<IShellExtInit>() {
                    let dp = parse(dir);
                    let ok = init.Initialize(Some(dp as *const _), None, None).is_ok();
                    CoTaskMemFree(Some(dp as *const core::ffi::c_void));
                    if ok {
                        if let Ok(nicm) = unk.cast::<IContextMenu>() {
                            let pos = GetMenuItemCount(Some(hmenu)).max(0) as u32;
                            if nicm
                                .QueryContextMenu(hmenu, pos, 0x7000, 0x7FFF, CMF_NORMAL)
                                .is_ok()
                            {
                                new_items = GetMenuItemCount(Some(hmenu)) - n;
                            }
                        }
                    }
                }
            }
            t_new = ms(t);
        }
        let _ = DestroyMenu(hmenu);
        // 재사용 측정(10-02 — 우클릭 가속 검토): 같은 IContextMenu로 새 HMENU에 재질의
        let t = Instant::now();
        let hm2 = CreatePopupMenu().expect("CreatePopupMenu");
        let r2 = icm.QueryContextMenu(hm2, 0, 1, 0x6FFF, flags);
        let n2 = GetMenuItemCount(Some(hm2));
        let _ = DestroyMenu(hm2);
        println!("  reuse same IContextMenu: {:.1} ms ({r2:?}, {n2} items)", ms(t));
        drop(icm);
        drop(folder);
        CoTaskMemFree(Some(pidl as *const core::ffi::c_void));
        println!(
            "round {round}: total {:.1} ms | parse {:.1} · bind {:.1} · GetUIObjectOf {:.1} · \
             QueryContextMenu {:.1} ({:?}, {n} items) · verbs {:.1} ({verbs}) · NewMenu {:.1} (+{new_items})",
            ms(total),
            t_parse,
            t_bind,
            t_uiobj,
            t_query,
            r,
            t_verbs,
            t_new
        );
    }

    pub fn run() {
        let path = PathBuf::from(
            std::env::args_os()
                .nth(1)
                .expect("usage: ctxmenu_probe <path> [rounds]"),
        );
        let rounds: usize = std::env::args()
            .nth(2)
            .and_then(|s| s.parse().ok())
            .unwrap_or(2);
        // 3번째 인수 = QueryContextMenu 플래그(16진) — 탐색기 = 0x8414
        // (CMF_EXPLORE 0x4 | CMF_CANRENAME 0x10 | CMF_ASYNCVERBSTATE 0x400 | CMF_ITEMMENU 0x8000)
        let flags: u32 = std::env::args()
            .nth(3)
            .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
            .unwrap_or(CMF_NORMAL);
        unsafe {
            let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
            println!("probe: {} (flags 0x{flags:x})", path.display());
            for round in 1..=rounds {
                once(&path, round, flags);
            }
            if hr.is_ok() {
                CoUninitialize();
            }
        }
        println!("OK");
    }
}
