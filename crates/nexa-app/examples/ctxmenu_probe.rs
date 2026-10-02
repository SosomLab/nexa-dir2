//! 셸 컨텍스트 메뉴 **격리 재현기**(10-02 — 우클릭 즉사 분석용).
//! 인수로 받은 경로의 `IContextMenu`를 `shellmenu::show_inner`와 같은 순서로 취득해
//! `QueryContextMenu`까지만 수행한다(메뉴 표시 없음). 메뉴 확장(서드파티 DLL)이
//! 프로세스 내부에서 로드·실행되는 구간을 그대로 밟으므로, 종료 코드로 즉사 여부를 판정한다.
//!
//! ```text
//! cargo run --release -p nexa-app --example ctxmenu_probe -- "<경로>"
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
    use std::path::PathBuf;

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_APARTMENTTHREADED,
        COINIT_DISABLE_OLE1DDE,
    };
    use windows::Win32::UI::Shell::Common::ITEMIDLIST;
    use windows::Win32::UI::Shell::{
        IContextMenu, IShellFolder, SHBindToParent, SHParseDisplayName, CMF_NORMAL,
    };
    use windows::Win32::UI::WindowsAndMessaging::{CreatePopupMenu, DestroyMenu, GetMenuItemCount};

    pub fn run() {
        let path = PathBuf::from(
            std::env::args_os()
                .nth(1)
                .expect("usage: ctxmenu_probe <path>"),
        );
        unsafe {
            let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
            let wide: Vec<u16> = path
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
            SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut pidl, 0, None)
                .expect("SHParseDisplayName");
            let mut child: *mut ITEMIDLIST = std::ptr::null_mut();
            let folder =
                SHBindToParent::<IShellFolder>(pidl, Some(&mut child)).expect("SHBindToParent");
            let children = [child as *const ITEMIDLIST];
            let icm = folder
                .GetUIObjectOf::<IContextMenu>(HWND::default(), &children, None)
                .expect("GetUIObjectOf(IContextMenu)");
            let hmenu = CreatePopupMenu().expect("CreatePopupMenu");
            println!("QueryContextMenu start ({})", path.display());
            let r = icm.QueryContextMenu(hmenu, 0, 1, 0x6FFF, CMF_NORMAL);
            println!(
                "QueryContextMenu -> {:?}, items={}",
                r,
                GetMenuItemCount(Some(hmenu))
            );
            let _ = DestroyMenu(hmenu);
            drop(icm);
            drop(folder);
            CoTaskMemFree(Some(pidl as *const core::ffi::c_void));
            if hr.is_ok() {
                CoUninitialize();
            }
        }
        println!("OK");
    }
}
