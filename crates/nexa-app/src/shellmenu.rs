//! 클래식 **셸 컨텍스트 메뉴 호스팅**(M3-4, ADR-0003 — 원본 ADR-0005 계승).
//! **원본 이식**: `app/Nexa.App/ShellContextMenu.cs` — 항목들의 `IContextMenu`를 HMENU로 받아
//! `TrackPopupMenuEx`로 표시(탐색기 "더 많은 옵션"과 동일: 7-Zip·Git·보내기·열기 방법·속성).
//!
//! 원본과의 차이(ADR-0003 §특이점): 자기 wndproc 보유 → comctl32 서브클래스 불요 —
//! wndproc이 [`forward_menu_msg`]로 `WM_INITMENUPOPUP`/`WM_DRAWITEM`/`WM_MEASUREITEM`/
//! `WM_MENUCHAR`를 활성 메뉴의 `IContextMenu2/3`에 직접 포워딩(동적 서브메뉴·아이콘).
//! COM 인터페이스는 windows-rs 제공(수동 vtable 선언 0).
//!
//! 다중 선택은 **같은 부모 폴더** 항목만(호출자가 축소 보장 — ADR-0003 §다중 선택 규칙).
//! 고유 항목 병합(0x8000+)은 S2에서 이 모듈에 추가.

use std::cell::RefCell;
use std::path::PathBuf;

use windows::core::{Interface, PCWSTR, PSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::Com::{
    CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    IContextMenu, IContextMenu2, IContextMenu3, IShellFolder, SHBindToParent, SHGetDesktopFolder,
    SHParseDisplayName, CMF_EXTENDEDVERBS, CMF_NORMAL, CMINVOKECOMMANDINFO, CMINVOKECOMMANDINFOEX,
    GCS_VERBW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, PostMessageW, SetForegroundWindow,
    TrackPopupMenuEx, MF_GRAYED, MF_SEPARATOR, MF_STRING, SW_SHOWNORMAL, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, WM_DRAWITEM, WM_INITMENUPOPUP, WM_MEASUREITEM, WM_MENUCHAR, WM_NULL,
};

/// 셸 명령 ID 대역(1~0x7FFF) — 고유 항목은 [`ID_CUSTOM_FIRST`]+(ADR-0005 대역 분리).
const ID_SHELL_FIRST: u32 = 1;
const ID_SHELL_LAST: u32 = 0x7FFF;
/// 고유(호스트) 항목 ID 시작 — 셸 대역과 겹치지 않는다.
pub const ID_CUSTOM_FIRST: u32 = 0x8000;

/// 병합할 고유 메뉴 항목(원본 CustomItem — 서브메뉴는 후속). `id`는 [`ID_CUSTOM_FIRST`] 이상.
pub struct CustomItem {
    pub id: u32,
    pub label: String,
    pub enabled: bool,
}
/// CMINVOKECOMMANDINFOEX.fMask — windows-rs 미노출 상수(shellapi.h).
const CMIC_MASK_UNICODE: u32 = 0x4000;
const CMIC_MASK_PTINVOKE: u32 = 0x2000_0000;

/// 표시 결과 — 호출자가 후처리(재로드·앱 통합 동사 실행)를 판단한다.
pub enum Outcome {
    /// 취소(선택 없음/실패). 후처리 불요.
    Cancelled,
    /// 셸이 실행함(InvokeCommand) — FS가 바뀌었을 수 있어 재로드 필요.
    Shell,
    /// 앱 통합이 필요한 동사를 가로챔(delete·rename 등) — 호출자가 자체 경로로 실행
    /// (undo 기록·인라인 리네임 합류. 원본 verbInterceptor 계승 — 콜백 대신 반환값).
    Verb(String),
    /// 고유 병합 항목 선택(0x8000+) — 호출자가 id로 분기(원본 CustomItem.Invoke 대응).
    Custom(u32),
}

// 메뉴 표시 구간의 활성 IContextMenu2/3 — wndproc 포워딩용(UI 스레드 전용).
thread_local! {
    static ACTIVE: RefCell<Option<(Option<IContextMenu2>, Option<IContextMenu3>)>> =
        const { RefCell::new(None) };
}

/// wndproc 훅 — 활성 셸 메뉴가 있으면 메뉴 메시지를 IContextMenu2/3로 포워딩.
/// 반환 `Some(lresult)` = 소비됨(원본 SubclassProc 대응 — 서브클래스 없이 자기 wndproc).
pub fn forward_menu_msg(msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    if !matches!(
        msg,
        WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR
    ) {
        return None;
    }
    ACTIVE.with_borrow(|active| {
        let (icm2, icm3) = active.as_ref()?;
        // 확장 예외는 HRESULT로 격리 — 메뉴 그리기 실패는 무시(원본 동일)
        if let Some(icm3) = icm3 {
            let mut result = LRESULT(0);
            unsafe {
                let _ = icm3.HandleMenuMsg2(msg, wparam, lparam, Some(&mut result));
            }
            return Some(if msg == WM_MENUCHAR {
                result
            } else {
                LRESULT(0)
            });
        }
        if let Some(icm2) = icm2 {
            unsafe {
                let _ = icm2.HandleMenuMsg(msg, wparam, lparam);
            }
            return Some(LRESULT(0));
        }
        None
    })
}

/// 셸 메뉴 표시. `paths`는 **같은 부모 폴더**의 파일/폴더들.
/// `extended_verbs`=Shift(확장 동사). `intercept`: 이 canonical verb들은 셸 실행 대신
/// [`Outcome::Verb`]로 반환(앱 통합 — delete=휴지통 undo·rename=인라인).
/// `custom`: 고유 병합 항목(구분자 아래 0x8000+, ADR-0005) — 선택 시 [`Outcome::Custom`].
/// `at`: 표시 화면 좌표 — `None`=커서 위치(우클릭)·`Some`=지정 위치(Apps/Shift+F10).
///
/// # Safety
/// UI 스레드에서 호출. `hwnd`는 유효한 자기 창(모달 메뉴 펌프 동안 wndproc 재진입 —
/// 호출자는 State 가변 참조를 넘기지 말 것).
pub unsafe fn show(
    hwnd: HWND,
    paths: &[PathBuf],
    extended_verbs: bool,
    intercept: &[&str],
    custom: &[CustomItem],
    at: Option<POINT>,
) -> Outcome {
    if paths.is_empty() {
        return Outcome::Cancelled;
    }
    let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
    let out = show_inner(hwnd, paths, extended_verbs, intercept, custom, at);
    if hr.is_ok() {
        CoUninitialize();
    }
    out
}

unsafe fn show_inner(
    hwnd: HWND,
    paths: &[PathBuf],
    extended_verbs: bool,
    intercept: &[&str],
    custom: &[CustomItem],
    at: Option<POINT>,
) -> Outcome {
    use std::os::windows::ffi::OsStrExt;

    // 1) 경로 → full PIDL → 공통 부모 IShellFolder + child PIDL 목록.
    //    child는 full 내부를 가리킴 → full을 메뉴 종료까지 유지(원본 동일).
    let mut full_pidls: Vec<*mut ITEMIDLIST> = Vec::new();
    let mut children: Vec<*const ITEMIDLIST> = Vec::new();
    let mut folder: Option<IShellFolder> = None;
    for p in paths {
        let wide: Vec<u16> = p
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
        if SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut pidl, 0, None).is_err() {
            continue; // 접근 불가 항목은 제외(격리)
        }
        full_pidls.push(pidl);
        let mut child: *mut ITEMIDLIST = std::ptr::null_mut();
        let Ok(f) = SHBindToParent::<IShellFolder>(pidl, Some(&mut child)) else {
            continue;
        };
        folder.get_or_insert(f); // 같은 부모 — 첫 폴더만 유지(호출자 보장)
        children.push(child as *const ITEMIDLIST);
    }
    let outcome = (|| {
        let Some(folder) = &folder else {
            return Outcome::Cancelled;
        };
        if children.is_empty() {
            return Outcome::Cancelled;
        }

        // 2) IContextMenu 취득 → 공용 메뉴 흐름.
        let Ok(icm) = folder.GetUIObjectOf::<IContextMenu>(hwnd, &children, None) else {
            return Outcome::Cancelled;
        };
        run_menu(hwnd, &icm, extended_verbs, intercept, custom, at)
    })();
    for pidl in full_pidls {
        CoTaskMemFree(Some(pidl as *const core::ffi::c_void)); // ILFree 동등
    }
    outcome
}

/// 폴더 **배경** 셸 메뉴 표시(원본 ADR-0005 S2) — `CreateViewObject(IID_IContextMenu)`.
/// 새로 만들기 서브메뉴·붙여넣기·속성 등 탐색기 빈 영역 메뉴와 동일. 파라미터 규약은 [`show`].
///
/// # Safety
/// [`show`]와 동일.
pub unsafe fn show_background(
    hwnd: HWND,
    dir: &std::path::Path,
    extended_verbs: bool,
    intercept: &[&str],
    custom: &[CustomItem],
    at: Option<POINT>,
) -> Outcome {
    let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
    let out = show_background_inner(hwnd, dir, extended_verbs, intercept, custom, at);
    if hr.is_ok() {
        CoUninitialize();
    }
    out
}

unsafe fn show_background_inner(
    hwnd: HWND,
    dir: &std::path::Path,
    extended_verbs: bool,
    intercept: &[&str],
    custom: &[CustomItem],
    at: Option<POINT>,
) -> Outcome {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = dir
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
    if SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut pidl, 0, None).is_err() {
        return Outcome::Cancelled;
    }
    let outcome = (|| {
        let Ok(desktop) = SHGetDesktopFolder() else {
            return Outcome::Cancelled;
        };
        let Ok(folder) = desktop.BindToObject::<_, IShellFolder>(pidl, None) else {
            return Outcome::Cancelled;
        };
        let Ok(icm) = folder.CreateViewObject::<IContextMenu>(hwnd) else {
            return Outcome::Cancelled;
        };
        run_menu(hwnd, &icm, extended_verbs, intercept, custom, at)
    })();
    CoTaskMemFree(Some(pidl as *const core::ffi::c_void));
    outcome
}

/// 공용 메뉴 흐름 — HMENU 구성(셸 대역+고유 병합)·표시·선택 분기(항목/배경 메뉴 공용).
unsafe fn run_menu(
    hwnd: HWND,
    icm: &IContextMenu,
    extended_verbs: bool,
    intercept: &[&str],
    custom: &[CustomItem],
    at: Option<POINT>,
) -> Outcome {
    let Ok(hmenu) = CreatePopupMenu() else {
        return Outcome::Cancelled;
    };
    ACTIVE.set(Some((icm.cast().ok(), icm.cast().ok())));
    let flags = if extended_verbs {
        CMF_EXTENDEDVERBS
    } else {
        CMF_NORMAL
    };
    let out = (|| {
        if icm
            .QueryContextMenu(hmenu, 0, ID_SHELL_FIRST, ID_SHELL_LAST, flags)
            .is_err()
        {
            return Outcome::Cancelled;
        }
        // 2-1) 고유 항목 병합(0x8000+) — 구분자로 섹션 분리(ADR-0005. 셸 제공 동사는 중복 금지).
        if !custom.is_empty() {
            let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, None);
            for c in custom {
                let mut flags = MF_STRING;
                if !c.enabled {
                    flags |= MF_GRAYED;
                }
                let label = windows::core::HSTRING::from(&*c.label);
                let _ = AppendMenuW(hmenu, flags, c.id as usize, PCWSTR(label.as_ptr()));
            }
        }

        // 3) 표시 — 모달 메뉴 펌프(메뉴 메시지는 wndproc → forward_menu_msg).
        let pt = at.unwrap_or_else(|| {
            let mut p = POINT::default();
            let _ = GetCursorPos(&mut p);
            p
        });
        let _ = SetForegroundWindow(hwnd); // 메뉴 밖 클릭 시 정상 닫힘(표준 관례)
        let sel = TrackPopupMenuEx(
            hmenu,
            (TPM_RETURNCMD | TPM_RIGHTBUTTON).0,
            pt.x,
            pt.y,
            hwnd,
            None,
        )
        .0 as u32;
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        if sel >= ID_CUSTOM_FIRST {
            return Outcome::Custom(sel); // 고유 병합 항목 — 호출자 분기
        }
        if !(ID_SHELL_FIRST..=ID_SHELL_LAST).contains(&sel) {
            return Outcome::Cancelled; // 취소(0)
        }

        // 4) 앱 통합 동사 가로채기(원본 verbInterceptor) — undo 기록 등 자체 경로로.
        let offset = sel - ID_SHELL_FIRST;
        if let Some(verb) = get_verb(icm, offset) {
            if intercept.iter().any(|v| verb.eq_ignore_ascii_case(v)) {
                return Outcome::Verb(verb);
            }
        }

        // 5) 셸 실행 — lpVerb = MAKEINTRESOURCE(선택 오프셋).
        let inv = CMINVOKECOMMANDINFOEX {
            cbSize: std::mem::size_of::<CMINVOKECOMMANDINFOEX>() as u32,
            fMask: CMIC_MASK_UNICODE | CMIC_MASK_PTINVOKE,
            hwnd,
            lpVerb: windows::core::PCSTR(offset as usize as *const u8),
            lpVerbW: PCWSTR(offset as usize as *const u16),
            nShow: SW_SHOWNORMAL.0,
            ptInvoke: pt,
            ..Default::default()
        };
        match icm.InvokeCommand(&inv as *const _ as *const CMINVOKECOMMANDINFO) {
            Ok(()) => Outcome::Shell,
            Err(_) => Outcome::Cancelled, // 확장 실패 격리(ADR-0005 위험 1)
        }
    })();
    ACTIVE.set(None);
    let _ = DestroyMenu(hmenu);
    out
}

/// 선택된 셸 명령의 canonical verb(언어 무관 식별자, 예: "delete"/"copy").
/// GCS_VERBW는 PSTR 버퍼에 **wide 문자열**을 쓴다(원본 동일 — u16 버퍼로 수신).
unsafe fn get_verb(icm: &IContextMenu, id_offset: u32) -> Option<String> {
    let mut buf = [0u16; 512];
    icm.GetCommandString(
        id_offset as usize,
        GCS_VERBW,
        None,
        PSTR(buf.as_mut_ptr() as *mut u8),
        buf.len() as u32,
    )
    .ok()?; // 일부 확장은 미구현/실패 → 식별 불가 — 가로채기 없이 셸 실행
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}
