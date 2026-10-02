//! 셸 컨텍스트 메뉴 **핸들러별 프로파일러**(10-02 — 우클릭 지연 원인 분리).
//! 인수 = 파일 경로 + 핸들러 CLSID 목록. 각 CLSID를 탐색기와 같은 순서로
//! `CoCreateInstance → IShellExtInit::Initialize(부모 pidl, IDataObject) → QueryContextMenu`
//! 하고 단계별 ms를 찍는다. CLSID 목록은 레지스트리(HKCR\*·AllFilesystemObjects·
//! <progid>·SystemFileAssociations\<ext> 의 shellex\ContextMenuHandlers)에서 호출자가 모은다.
//!
//! ```text
//! cargo run --release -p nexa-app --example ctxmenu_handlers -- "<경로>" {CLSID} {CLSID} …
//! ```

#[cfg(windows)]
fn main() {
    win::run();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Windows 전용");
}

#[cfg(windows)]
mod win {
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    use windows::core::{Interface, GUID, PCWSTR};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IDataObject,
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
    };
    use windows::Win32::UI::Shell::Common::ITEMIDLIST;
    use windows::Win32::UI::Shell::{
        IContextMenu, IShellExtInit, IShellFolder, SHBindToParent, SHParseDisplayName,
        CMF_NORMAL,
    };
    use windows::Win32::UI::WindowsAndMessaging::{CreatePopupMenu, DestroyMenu, GetMenuItemCount};

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

    fn parse_guid(s: &str) -> Option<GUID> {
        let s = s.trim().trim_start_matches('{').trim_end_matches('}');
        let p: Vec<&str> = s.split('-').collect();
        if p.len() != 5 {
            return None;
        }
        let d1 = u32::from_str_radix(p[0], 16).ok()?;
        let d2 = u16::from_str_radix(p[1], 16).ok()?;
        let d3 = u16::from_str_radix(p[2], 16).ok()?;
        let d4h = u16::from_str_radix(p[3], 16).ok()?;
        let d4l = u64::from_str_radix(p[4], 16).ok()?;
        let mut d4 = [0u8; 8];
        d4[0] = (d4h >> 8) as u8;
        d4[1] = d4h as u8;
        for i in 0..6 {
            d4[2 + i] = (d4l >> (8 * (5 - i))) as u8;
        }
        Some(GUID::from_values(d1, d2, d3, d4))
    }

    pub fn run() {
        let mut args = std::env::args();
        let _ = args.next();
        let path = PathBuf::from(args.next().expect("usage: ctxmenu_handlers <path> <clsid>…"));
        let clsids: Vec<String> = args.collect();
        unsafe {
            let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
            let pidl = parse(&path);
            let mut child: *mut ITEMIDLIST = std::ptr::null_mut();
            let folder =
                SHBindToParent::<IShellFolder>(pidl, Some(&mut child)).expect("SHBindToParent");
            let children = [child as *const ITEMIDLIST];
            let dobj = folder
                .GetUIObjectOf::<IDataObject>(HWND::default(), &children, None)
                .expect("IDataObject");
            let parent_pidl = parse(path.parent().unwrap_or(&path));
            println!("file: {}", path.display());
            for c in &clsids {
                let Some(g) = parse_guid(c) else {
                    println!("{c}: bad guid");
                    continue;
                };
                let total = Instant::now();
                let t = Instant::now();
                let unk = match CoCreateInstance::<_, windows::core::IUnknown>(
                    &g,
                    None,
                    CLSCTX_INPROC_SERVER,
                ) {
                    Ok(u) => u,
                    Err(e) => {
                        println!("{c}: CoCreateInstance 실패 {e:?} ({:.1} ms)", ms(t));
                        continue;
                    }
                };
                let t_create = ms(t);
                let t = Instant::now();
                let init_ok = match unk.cast::<IShellExtInit>() {
                    Ok(init) => init
                        .Initialize(Some(parent_pidl as *const _), Some(&dobj), None)
                        .is_ok(),
                    Err(_) => false,
                };
                let t_init = ms(t);
                let t = Instant::now();
                let (q, items) = match unk.cast::<IContextMenu>() {
                    Ok(icm) => {
                        let hmenu = CreatePopupMenu().unwrap();
                        let r = icm.QueryContextMenu(hmenu, 0, 1, 0x6FFF, CMF_NORMAL);
                        let n = GetMenuItemCount(Some(hmenu));
                        let _ = DestroyMenu(hmenu);
                        (format!("{r:?}"), n)
                    }
                    Err(_) => ("no IContextMenu".into(), 0),
                };
                let t_query = ms(t);
                println!(
                    "{c}: total {:.1} ms | create {:.1} · init {:.1} (ok={init_ok}) · query {:.1} ({q}, +{items})",
                    ms(total),
                    t_create,
                    t_init,
                    t_query
                );
            }
            CoTaskMemFree(Some(parent_pidl as *const core::ffi::c_void));
            drop(dobj);
            drop(folder);
            CoTaskMemFree(Some(pidl as *const core::ffi::c_void));
            if hr.is_ok() {
                CoUninitialize();
            }
        }
    }
}
