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
use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use windows::core::{Interface, PCWSTR, PSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    IContextMenu, IContextMenu2, IContextMenu3, IShellExtInit, IShellFolder, SHBindToParent,
    SHGetDesktopFolder, SHParseDisplayName, CMF_EXTENDEDVERBS, CMF_NORMAL, CMINVOKECOMMANDINFO,
    CMINVOKECOMMANDINFOEX, GCS_VERBW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, PostMessageW, SetForegroundWindow,
    TrackPopupMenuEx, MF_GRAYED, MF_SEPARATOR, MF_STRING, SW_SHOWNORMAL, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, WM_DRAWITEM, WM_INITMENUPOPUP, WM_MEASUREITEM, WM_MENUCHAR, WM_NULL,
};

/// 셸 명령 ID 대역 — 고유 항목은 [`ID_CUSTOM_FIRST`]+(ADR-0005 대역 분리).
const ID_SHELL_FIRST: u32 = 1;
const ID_SHELL_LAST: u32 = 0x6FFF;
/// 셸 "새로 만들기" 확장(CLSID_NewMenu) 전용 대역(07-27) — 항목 메뉴 병합 서브메뉴.
const ID_NEW_FIRST: u32 = 0x7000;
const ID_NEW_LAST: u32 = 0x7FFF;
/// 고유(호스트) 항목 ID 시작 — 셸 대역과 겹치지 않는다.
pub const ID_CUSTOM_FIRST: u32 = 0x8000;

/// 항목 메뉴에 병합할 "새로 만들기" 서브메뉴 요청(07-27 사용자) — 셸은 항목 메뉴에 New를
/// 넣지 않으므로(배경 메뉴 전용) 셸 New 확장(CLSID_NewMenu)을 `dir` 대상으로 직접 호스팅.
/// 배경 메뉴와 동일한 전체 ShellNew 템플릿(폴더·바로가기·txt·docx…)이 나온다.
#[derive(Debug, Clone)]
pub struct NewSpec {
    /// 생성 대상 폴더 — 파일 항목=부모·폴더 항목=자신(호출자 결정).
    pub dir: PathBuf,
    /// 최상위 항목 라벨 — 앱 언어 추종(QA 07-14 규약. 서브메뉴 항목은 OS 로케일).
    pub label: String,
}

/// 병합할 고유 메뉴 항목(원본 CustomItem — 서브메뉴는 후속). `id`는 [`ID_CUSTOM_FIRST`] 이상.
#[derive(Debug, Clone)]
pub struct CustomItem {
    pub id: u32,
    pub label: String,
    pub enabled: bool,
    /// 이 ID를 가진 기존 항목 **바로 아래**에 삽입(제자리 대체된 항목 포함 — 예: 경로 복사
    /// 아래 이름 복사). 대상 부재 시 하단 고유 섹션으로 폴백.
    pub after_id: Option<u32>,
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
    /// 셸 실행으로 항목 1개가 새로 생성됨(새로 만들기 — 07-27 전후 diff 감지) —
    /// 호출자가 재로드 후 캐럿 이동·인라인 리네임 진입(탐색기 관례).
    Created(PathBuf),
    /// 앱 통합이 필요한 동사를 가로챔(delete·rename 등) — 호출자가 자체 경로로 실행
    /// (undo 기록·인라인 리네임 합류. 원본 verbInterceptor 계승 — 콜백 대신 반환값).
    Verb(String),
    /// 고유 병합 항목 선택(0x8000+) — 호출자가 id로 분기(원본 CustomItem.Invoke 대응).
    Custom(u32),
}

/// 활성 셸 메뉴 핸들러(주 메뉴 ICM/호스팅 New 확장) + 메시지 라우팅 정보.
/// QA 07-27: 전 핸들러 브로드캐스트는 CNewMenu가 **주 메뉴** WM_INITMENUPOPUP에
/// 반응해 템플릿을 주 메뉴에 평탄 삽입하는 오동작 유발 → 소유 서브메뉴 핸들·명령
/// 대역으로 **선별 라우팅**(Explorer++ 등 다중 ICM 호스트 관례).
struct MenuHost {
    icm2: Option<IContextMenu2>,
    icm3: Option<IContextMenu3>,
    /// 이 핸들러가 소유한 서브메뉴 핸들(HMENU 값). 0 = 주 메뉴(매치 실패 폴백 대상).
    submenu: isize,
    /// 이 핸들러의 명령 ID 대역 — WM_DRAWITEM/WM_MEASUREITEM itemID 라우팅.
    first: u32,
    last: u32,
}

// 메뉴 표시 구간의 활성 핸들러 목록 — wndproc 포워딩용(**메뉴를 표시하는 스레드** 전용 —
// thread_local. 10-02 X-61: 전용 메뉴 스레드가 track하면 그 스레드의 숨은 창 wndproc이 포워딩).
thread_local! {
    static ACTIVE: RefCell<Vec<MenuHost>> = const { RefCell::new(Vec::new()) };
}

/// 우클릭 지연 계측(10-02 — 탐색기 대비 분석). `NEXA_CTX_TIMING=1`일 때만 동작:
/// 단계 시각 + 메뉴 메시지 포워딩 누적을 임시 폴더 `nexa-ctxmenu-timing.log`에 한 줄 추가.
/// 꺼져 있으면 `mark`/`fwd` = 분기 하나(무비용).
mod timing {
    use std::cell::RefCell;
    use std::time::Instant;
    pub struct T {
        pub t0: Instant,
        pub marks: Vec<(&'static str, f64)>,
        pub fwd_ms: f64,
        pub fwd_n: u32,
        pub first_fwd: Option<f64>,
    }
    thread_local! {
        static CUR: RefCell<Option<T>> = const { RefCell::new(None) };
    }
    pub fn enabled() -> bool {
        static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ON.get_or_init(|| std::env::var_os("NEXA_CTX_TIMING").is_some_and(|v| v == "1"))
    }
    pub fn start() {
        if enabled() {
            CUR.set(Some(T {
                t0: Instant::now(),
                marks: Vec::new(),
                fwd_ms: 0.0,
                fwd_n: 0,
                first_fwd: None,
            }));
        }
    }
    pub fn mark(name: &'static str) {
        if enabled() {
            CUR.with_borrow_mut(|c| {
                if let Some(t) = c {
                    let ms = t.t0.elapsed().as_secs_f64() * 1000.0;
                    t.marks.push((name, ms));
                }
            });
        }
    }
    pub fn fwd(began: Option<Instant>) {
        if let Some(b) = began {
            CUR.with_borrow_mut(|c| {
                if let Some(t) = c {
                    t.fwd_ms += b.elapsed().as_secs_f64() * 1000.0;
                    t.fwd_n += 1;
                    if t.first_fwd.is_none() {
                        t.first_fwd = Some(b.duration_since(t.t0).as_secs_f64() * 1000.0);
                    }
                }
            });
        }
    }
    pub fn finish(label: &str) {
        if !enabled() {
            return;
        }
        let Some(t) = CUR.take() else { return };
        let mut line = format!("{label}:");
        let mut prev = 0.0;
        for (n, ms) in &t.marks {
            line.push_str(&format!(" {n} +{:.1}", ms - prev));
            prev = *ms;
        }
        line.push_str(&format!(
            " | fwd {} msgs {:.1} ms (first at {:.1}) | total {:.1} ms\n",
            t.fwd_n,
            t.fwd_ms,
            t.first_fwd.unwrap_or(-1.0),
            t.t0.elapsed().as_secs_f64() * 1000.0
        ));
        let p = std::env::temp_dir().join("nexa-ctxmenu-timing.log");
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)
        {
            use std::io::Write;
            let _ = f.write_all(line.as_bytes());
        }
    }
}

impl MenuHost {
    /// 메뉴 메시지 1건 포워딩 — 확장 예외는 HRESULT로 격리(메뉴 그리기 실패 무시, 원본 동일).
    fn handle(&self, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let mut result = LRESULT(0);
        if let Some(icm3) = &self.icm3 {
            unsafe {
                let _ = icm3.HandleMenuMsg2(msg, wparam, lparam, Some(&mut result));
            }
        } else if let Some(icm2) = &self.icm2 {
            unsafe {
                let _ = icm2.HandleMenuMsg(msg, wparam, lparam);
            }
        }
        result
    }
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
    let began = timing::enabled().then(std::time::Instant::now);
    let r = ACTIVE.with_borrow(|active| {
        if active.is_empty() {
            return None;
        }
        // MENUCHAR(니모닉)만 전 핸들러 순회 — 첫 유효 응답 채택
        if msg == WM_MENUCHAR {
            let mut out = LRESULT(0);
            for m in active {
                let r = m.handle(msg, wparam, lparam);
                if out.0 == 0 {
                    out = r;
                }
            }
            return Some(out);
        }
        // 선별 라우팅(QA 07-27) — INITMENUPOPUP: 열리는 HMENU의 소유 핸들러에만 /
        // DRAW·MEASUREITEM: itemID 대역 소유 핸들러에만. 미매치 = 주 메뉴 핸들러.
        let target = match msg {
            WM_INITMENUPOPUP => {
                let h = wparam.0 as isize;
                active.iter().find(|m| m.submenu == h)
            }
            _ => {
                use windows::Win32::UI::Controls::{DRAWITEMSTRUCT, MEASUREITEMSTRUCT};
                let id = unsafe {
                    if msg == WM_DRAWITEM {
                        (*(lparam.0 as *const DRAWITEMSTRUCT)).itemID
                    } else {
                        (*(lparam.0 as *const MEASUREITEMSTRUCT)).itemID
                    }
                };
                active.iter().find(|m| (m.first..=m.last).contains(&id))
            }
        };
        let target = target.or_else(|| active.iter().find(|m| m.submenu == 0))?;
        target.handle(msg, wparam, lparam);
        Some(LRESULT(0))
    });
    timing::fwd(began);
    r
}

/// 셸 메뉴 표시. `paths`는 **같은 부모 폴더**의 파일/폴더들.
/// `extended_verbs`=Shift(확장 동사). `intercept`: 이 canonical verb들은 셸 실행 대신
/// [`Outcome::Verb`]로 반환(앱 통합 — delete=휴지통 undo·rename=인라인).
/// `custom`: 고유 병합 항목(구분자 아래 0x8000+, ADR-0005) — 선택 시 [`Outcome::Custom`].
/// `new_menu`: `Some`=하단에 "새로 만들기" 서브메뉴 병합(07-27 — [`NewSpec`] 대상 폴더에
/// 생성. 생성 감지 시 [`Outcome::Created`]).
/// `at`: 표시 화면 좌표 — `None`=커서 위치(우클릭)·`Some`=지정 위치(Apps/Shift+F10).
///
/// # Safety
/// UI 스레드에서 호출. `hwnd`는 유효한 자기 창(모달 메뉴 펌프 동안 wndproc 재진입 —
/// 호출자는 State 가변 참조를 넘기지 말 것).
#[allow(clippy::too_many_arguments)]
pub unsafe fn show(
    hwnd: HWND,
    paths: &[PathBuf],
    extended_verbs: bool,
    intercept: &[&str],
    hide: &[(&str, u32, String)],
    custom: &[CustomItem],
    new_menu: Option<&NewSpec>,
    at: Option<POINT>,
) -> Outcome {
    if paths.is_empty() {
        return Outcome::Cancelled;
    }
    timing::start();
    let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
    let out = show_inner(
        hwnd,
        paths,
        extended_verbs,
        intercept,
        hide,
        custom,
        new_menu,
        at,
    );
    if hr.is_ok() {
        CoUninitialize();
    }
    timing::finish("item");
    out
}

#[allow(clippy::too_many_arguments)]
unsafe fn show_inner(
    hwnd: HWND,
    paths: &[PathBuf],
    extended_verbs: bool,
    intercept: &[&str],
    hide: &[(&str, u32, String)],
    custom: &[CustomItem],
    new_menu: Option<&NewSpec>,
    at: Option<POINT>,
) -> Outcome {
    match prepare_items(hwnd, paths, extended_verbs, hide, custom, new_menu) {
        Some(p) => track(p, hwnd, intercept, None, at),
        None => Outcome::Cancelled,
    }
}

/// 항목 메뉴 **구축**(10-02 X-61 — 표시 없이 `QueryContextMenu`까지): 경로 → PIDL → 공통
/// 부모 `IShellFolder` → `IContextMenu` → HMENU(셸 대역 + 제자리 대체 + 고유 병합 + New).
/// 결과 [`Prepared`]는 **만든 스레드(STA)에서만** [`track`]할 수 있다(COM 객체·PIDL 보유).
/// 전용 메뉴 스레드가 선택 직후 미리 부르고, 우클릭 때 `track`만 하면 확장 실행 시간
/// (실측 0.6~1.4 s — docs/audit/20261002-ultracode/ctxmenu-latency.md)이 클릭 경로에서 빠진다.
///
/// # Safety
/// COM 초기화된 STA 스레드. `hwnd`는 메뉴 소유 창(대화상자 부모).
pub unsafe fn prepare_items(
    hwnd: HWND,
    paths: &[PathBuf],
    extended_verbs: bool,
    hide: &[(&str, u32, String)],
    custom: &[CustomItem],
    new_menu: Option<&NewSpec>,
) -> Option<Prepared> {
    use std::os::windows::ffi::OsStrExt;

    // 1) 경로 → full PIDL → 공통 부모 IShellFolder + child PIDL 목록.
    //    child는 full 내부를 가리킴 → full을 메뉴 종료까지 유지(원본 동일 — Prepared가 보유).
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
    let free = |pidls: &[*mut ITEMIDLIST]| {
        for &pidl in pidls {
            CoTaskMemFree(Some(pidl as *const core::ffi::c_void)); // ILFree 동등
        }
    };
    let Some(folder) = folder else {
        free(&full_pidls);
        return None;
    };
    if children.is_empty() {
        free(&full_pidls);
        return None;
    }
    timing::mark("parse+bind");
    // 2) IContextMenu 취득 → 공용 구축.
    let Ok(icm) = folder.GetUIObjectOf::<IContextMenu>(hwnd, &children, None) else {
        free(&full_pidls);
        return None;
    };
    timing::mark("GetUIObjectOf");
    match build(icm, extended_verbs, hide, custom, new_menu) {
        Some(mut p) => {
            p.pidls = full_pidls;
            p.folder = Some(folder);
            Some(p)
        }
        None => {
            free(&full_pidls);
            None
        }
    }
}

/// 폴더 **배경** 셸 메뉴 표시(원본 ADR-0005 S2) — `CreateViewObject(IID_IContextMenu)`.
/// 새로 만들기 서브메뉴·붙여넣기·속성 등 탐색기 빈 영역 메뉴와 동일. 파라미터 규약은 [`show`].
/// 셸 실행으로 `dir`에 항목 1개가 새로 생기면 [`Outcome::Created`](07-27 — 새로 만들기
/// 리네임 진입용 diff 감지. 셸은 New 선택 여부를 안 알려줘 실행 전반에 적용하는 휴리스틱).
///
/// # Safety
/// [`show`]와 동일.
pub unsafe fn show_background(
    hwnd: HWND,
    dir: &std::path::Path,
    extended_verbs: bool,
    intercept: &[&str],
    hide: &[(&str, u32, String)],
    custom: &[CustomItem],
    at: Option<POINT>,
) -> Outcome {
    let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
    let out = show_background_inner(hwnd, dir, extended_verbs, intercept, hide, custom, at);
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
    hide: &[(&str, u32, String)],
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
        run_menu(
            hwnd,
            &icm,
            extended_verbs,
            intercept,
            hide,
            custom,
            None,
            Some(dir),
            at,
        )
    })();
    CoTaskMemFree(Some(pidl as *const core::ffi::c_void));
    outcome
}

/// 구축된 메뉴(10-02 X-61) — `build`/`prepare_items`가 만들고 [`track`]이 소비한다.
/// COM 객체·HMENU·PIDL을 보유하므로 **만든 STA 스레드 전용**(Send 아님). 미표시로
/// 버려지면 Drop이 HMENU·PIDL을 해제한다.
pub struct Prepared {
    hmenu: windows::Win32::UI::WindowsAndMessaging::HMENU,
    icm: IContextMenu,
    /// 메시지 포워딩 대상(주 메뉴 ICM + 호스팅 New 확장) — track이 ACTIVE에 설치.
    hosts: Vec<MenuHost>,
    new_icm: Option<(IContextMenu, windows::Win32::UI::WindowsAndMessaging::HMENU)>,
    /// New 서브메뉴 대상 폴더(생성 diff 감지용).
    new_dir: Option<PathBuf>,
    /// 항목 메뉴: child PIDL이 참조하는 full PIDL(메뉴 종료까지 유지) + 부모 폴더.
    pidls: Vec<*mut ITEMIDLIST>,
    folder: Option<IShellFolder>,
}

impl Drop for Prepared {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyMenu(self.hmenu);
            for &pidl in &self.pidls {
                CoTaskMemFree(Some(pidl as *const core::ffi::c_void));
            }
        }
    }
}

/// HMENU 구축(항목/배경 공용 — 10-02 X-61 분리): 셸 대역 `QueryContextMenu` → 제자리 대체 →
/// 고유 병합 → New 서브메뉴. 표시는 [`track`].
unsafe fn build(
    icm: IContextMenu,
    extended_verbs: bool,
    hide: &[(&str, u32, String)],
    custom: &[CustomItem],
    new_menu: Option<&NewSpec>,
) -> Option<Prepared> {
    let hmenu = CreatePopupMenu().ok()?;
    let mut p = Prepared {
        hmenu,
        hosts: vec![MenuHost {
            icm2: icm.cast().ok(),
            icm3: icm.cast().ok(),
            submenu: 0,
            first: ID_SHELL_FIRST,
            last: ID_SHELL_LAST,
        }],
        icm,
        new_icm: None,
        new_dir: new_menu.map(|s| s.dir.clone()),
        pidls: Vec::new(),
        folder: None,
    };
    let flags = if extended_verbs {
        CMF_EXTENDEDVERBS
    } else {
        CMF_NORMAL
    };
    if p.icm
        .QueryContextMenu(hmenu, 0, ID_SHELL_FIRST, ID_SHELL_LAST, flags)
        .is_err()
    {
        return None; // Drop이 HMENU 회수
    }
    timing::mark("QueryContextMenu");
    // 2-0) 셸 항목 **제자리 대체**(원본 VerbReplacement — QA 07-14): 대상 verb의 메뉴
    // 항목 ID만 고유 ID로 바꿔치기 — 위치·라벨(=윈도우 기본 다국어) 그대로, 선택 시
    // Outcome::Custom으로 우리 경로 실행(단일 부모 한계 우회).
    if !hide.is_empty() {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetMenuItemCount, GetMenuItemID, SetMenuItemInfoW, MENUITEMINFOW, MIIM_ID, MIIM_STRING,
        };
        let n = GetMenuItemCount(Some(hmenu));
        for pos in 0..n.max(0) {
            let id = GetMenuItemID(hmenu, pos);
            if !(ID_SHELL_FIRST..=ID_SHELL_LAST).contains(&id) {
                continue;
            }
            if let Some(verb) = get_verb(&p.icm, id - ID_SHELL_FIRST) {
                if let Some((_, custom_id, label)) =
                    hide.iter().find(|(v, _, _)| verb.eq_ignore_ascii_case(v))
                {
                    // 라벨 = 앱 언어(i18n — QA 07-14: 셸 OS 라벨 대신 앱 언어 추종)
                    let mut wide: Vec<u16> =
                        label.encode_utf16().chain(std::iter::once(0)).collect();
                    let mii = MENUITEMINFOW {
                        cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
                        fMask: MIIM_ID | MIIM_STRING,
                        wID: *custom_id,
                        dwTypeData: windows::core::PWSTR(wide.as_mut_ptr()),
                        ..Default::default()
                    };
                    let _ = SetMenuItemInfoW(hmenu, pos as u32, true, &mii);
                }
            }
        }
    }
    timing::mark("verbs");
    // 2-1) 고유 항목 병합(0x8000+) — 앵커(`after_id`) 지정은 그 항목 바로 아래 삽입,
    // 나머지는 구분자로 섹션 분리 후 하단(ADR-0005. 셸 제공 동사는 중복 금지).
    if !custom.is_empty() {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetMenuItemCount, GetMenuItemID, InsertMenuW, MF_BYPOSITION,
        };
        let mut bottom: Vec<&CustomItem> = Vec::new();
        for c in custom {
            let mut flags = MF_STRING;
            if !c.enabled {
                flags |= MF_GRAYED;
            }
            let label = windows::core::HSTRING::from(&*c.label);
            let anchor = c.after_id.and_then(|aid| {
                let n = GetMenuItemCount(Some(hmenu));
                (0..n.max(0)).find(|&pos| GetMenuItemID(hmenu, pos) == aid)
            });
            if let Some(pos) = anchor {
                let _ = InsertMenuW(
                    hmenu,
                    (pos + 1) as u32,
                    flags | MF_BYPOSITION,
                    c.id as usize,
                    PCWSTR(label.as_ptr()),
                );
            } else {
                bottom.push(c);
            }
        }
        if !bottom.is_empty() {
            let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, None);
            for c in bottom {
                let mut flags = MF_STRING;
                if !c.enabled {
                    flags |= MF_GRAYED;
                }
                let label = windows::core::HSTRING::from(&*c.label);
                let _ = AppendMenuW(hmenu, flags, c.id as usize, PCWSTR(label.as_ptr()));
            }
        }
    }

    // 2-2) "새로 만들기" 서브메뉴 병합(07-27 사용자) — 셸 New 확장(CLSID_NewMenu)을
    // 대상 폴더로 직접 초기화해 하단 섹션에 삽입. 실패는 조용히 생략(메뉴는 정상 표시).
    p.new_icm = new_menu.and_then(|spec| attach_new_menu(hmenu, spec));
    timing::mark("custom+NewMenu");
    if let Some((icm, sub)) = &p.new_icm {
        // 서브메뉴 lazy 채움(WM_INITMENUPOPUP)을 위해 포워딩 대상에 추가 —
        // 소유 서브메뉴 핸들·New 대역으로 선별 라우팅(QA 07-27 평탄 삽입 방지)
        p.hosts.push(MenuHost {
            icm2: icm.cast().ok(),
            icm3: icm.cast().ok(),
            submenu: sub.0 as isize,
            first: ID_NEW_FIRST,
            last: ID_NEW_LAST,
        });
    }
    Some(p)
}

/// 구축된 메뉴 **표시·선택 분기**(10-02 X-61 분리). `hwnd` = 메뉴 소유 창(이 스레드 소유 —
/// 메뉴 메시지가 그 wndproc → [`forward_menu_msg`]로 온다). `probe_dir`: `Some`=셸 대역
/// 실행 후 이 폴더의 신규 항목 diff → [`Outcome::Created`](배경 메뉴 — 셸 자체 New 감지).
///
/// # Safety
/// `p`를 만든 STA 스레드에서 호출. 모달 메뉴 펌프 동안 `hwnd`의 wndproc이 재진입한다.
pub unsafe fn track(
    mut p: Prepared,
    hwnd: HWND,
    intercept: &[&str],
    probe_dir: Option<&Path>,
    at: Option<POINT>,
) -> Outcome {
    ACTIVE.set(std::mem::take(&mut p.hosts));
    let hmenu = p.hmenu;
    let out = (|| {
        // 3) 표시 — 모달 메뉴 펌프(메뉴 메시지는 wndproc → forward_menu_msg).
        let pt = at.unwrap_or_else(|| {
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            pt
        });
        timing::mark("pre-track");
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
        timing::mark("TrackPopupMenuEx(user)");
        // 메뉴가 닫혔으니 포워딩 즉시 해제(10-02 G8-20) — 아래 InvokeCommand가 띄우는 모달 UI
        // (연결 프로그램·보내기 등)가 이 hwnd를 owner로 메뉴를 열면 WM_INITMENUPOPUP 등이 이
        // wndproc에 오는데, 그 **남의 HMENU**가 주 메뉴 핸들러(submenu==0 폴백)로 흘러가면
        // 안 된다. 닫힌 뒤에는 포워딩이 필요한 메시지가 없다(동적 서브메뉴·아이콘은 표시 중만).
        // invoke용 `p.icm`·`p.new_icm`은 별도 소유라 영향 없음.
        ACTIVE.set(Vec::new());
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        if sel >= ID_CUSTOM_FIRST {
            return Outcome::Custom(sel); // 고유 병합 항목 — 호출자 분기
        }
        if (ID_NEW_FIRST..=ID_NEW_LAST).contains(&sel) {
            // 호스팅한 New 서브메뉴 선택(07-27) — New 확장 ICM으로 invoke 후 생성 diff
            let (Some(dir), Some((new_icm, _))) = (&p.new_dir, &p.new_icm) else {
                return Outcome::Cancelled;
            };
            let before = dir_names(dir);
            return match invoke(new_icm, hwnd, sel - ID_NEW_FIRST, pt) {
                // 생성은 결정적 트리거(New 선택) — 넉넉한 재시도(20ms×10)
                Ok(()) => match detect_created(dir, &before, 10) {
                    Some(p) => Outcome::Created(p),
                    None => Outcome::Shell, // 바로가기 마법사 등 비동기 — 재로드만
                },
                Err(_) => Outcome::Cancelled,
            };
        }
        if !(ID_SHELL_FIRST..=ID_SHELL_LAST).contains(&sel) {
            return Outcome::Cancelled; // 취소(0)
        }

        // 4) 앱 통합 동사 가로채기(원본 verbInterceptor) — undo 기록 등 자체 경로로.
        let offset = sel - ID_SHELL_FIRST;
        if let Some(verb) = get_verb(&p.icm, offset) {
            if intercept.iter().any(|v| verb.eq_ignore_ascii_case(v)) {
                return Outcome::Verb(verb);
            }
        }

        // 5) 셸 실행 — lpVerb = MAKEINTRESOURCE(선택 오프셋).
        let before = probe_dir.map(dir_names);
        match invoke(&p.icm, hwnd, offset, pt) {
            Ok(()) => {
                // 배경 메뉴 생성 감지(07-27 휴리스틱) — 항목 1개 신규면 리네임 진입 후보.
                // 짧은 재시도(20ms×2)만 — 미생성 명령(속성 등)의 지연 최소화
                if let (Some(dir), Some(before)) = (probe_dir, &before) {
                    if let Some(p) = detect_created(dir, before, 2) {
                        return Outcome::Created(p);
                    }
                }
                Outcome::Shell
            }
            Err(_) => Outcome::Cancelled, // 확장 실패 격리(ADR-0005 위험 1)
        }
    })();
    ACTIVE.set(Vec::new()); // 멱등 — 위 해제의 보호선
    drop(p); // DestroyMenu + PIDL 해제
    out
}

/// 공용 메뉴 흐름(배경 메뉴·동기 폴백) = [`build`] + [`track`].
#[allow(clippy::too_many_arguments)]
unsafe fn run_menu(
    hwnd: HWND,
    icm: &IContextMenu,
    extended_verbs: bool,
    intercept: &[&str],
    hide: &[(&str, u32, String)],
    custom: &[CustomItem],
    new_menu: Option<&NewSpec>,
    probe_dir: Option<&Path>,
    at: Option<POINT>,
) -> Outcome {
    match build(icm.clone(), extended_verbs, hide, custom, new_menu) {
        Some(p) => track(p, hwnd, intercept, probe_dir, at),
        None => Outcome::Cancelled,
    }
}

/// InvokeCommand 공용 래퍼 — lpVerb = MAKEINTRESOURCE(대역 내 오프셋).
unsafe fn invoke(
    icm: &IContextMenu,
    hwnd: HWND,
    offset: u32,
    pt: POINT,
) -> windows::core::Result<()> {
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
    // "열기"·"연결 프로그램"·"편집" 등이 **이미 떠 있는** 프로그램에 위임될 때도
    // 그 창이 앞으로 오도록 포그라운드 권한을 넘긴다(win.rs 주석 참조).
    crate::win::allow_foreground_handoff();
    icm.InvokeCommand(&inv as *const _ as *const CMINVOKECOMMANDINFO)
}

/// 셸 New 확장(CLSID_NewMenu)을 `spec.dir`로 초기화해 `hmenu` 하단에 "새로 만들기"
/// 서브메뉴로 병합(07-27). 성공 시 (invoke용 IContextMenu, 소유 서브메뉴 핸들 —
/// WM_INITMENUPOPUP 선별 라우팅용) 반환 — 실패는 None(생략).
unsafe fn attach_new_menu(
    hmenu: windows::Win32::UI::WindowsAndMessaging::HMENU,
    spec: &NewSpec,
) -> Option<(IContextMenu, windows::Win32::UI::WindowsAndMessaging::HMENU)> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetMenuItemCount, GetSubMenu, RemoveMenu, SetMenuItemInfoW, MENUITEMINFOW, MF_BYPOSITION,
        MIIM_STRING,
    };
    // CLSID_NewMenu — shobjidl에 windows-rs 미노출(탐색기 배경 메뉴의 New 제공자)
    const CLSID_NEW_MENU: windows::core::GUID =
        windows::core::GUID::from_u128(0xD969A300_E7FF_11D0_A93B_00A0C90F2719);
    let unk: windows::core::IUnknown =
        CoCreateInstance(&CLSID_NEW_MENU, None, CLSCTX_INPROC_SERVER).ok()?;
    let init: IShellExtInit = unk.cast().ok()?;
    let wide: Vec<u16> = spec
        .dir
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
    SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut pidl, 0, None).ok()?;
    // Initialize는 pidl을 복제(ILClone) — 즉시 해제 가능
    let init_ok = init.Initialize(Some(pidl as *const _), None, None).is_ok();
    CoTaskMemFree(Some(pidl as *const core::ffi::c_void));
    if !init_ok {
        return None;
    }
    let icm: IContextMenu = unk.cast().ok()?;
    // 하단 구분자 + New 항목(서브메뉴는 표시 시 lazy 채움) 삽입 → 라벨 앱 언어 교체
    let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, None);
    let pos = GetMenuItemCount(Some(hmenu)).max(0) as u32;
    if icm
        .QueryContextMenu(hmenu, pos, ID_NEW_FIRST, ID_NEW_LAST, CMF_NORMAL)
        .is_err()
    {
        let _ = RemoveMenu(hmenu, pos.saturating_sub(1), MF_BYPOSITION); // 구분자 회수
        return None;
    }
    // 라벨 = 앱 언어(QA 07-14 — 셸 OS 라벨 대신 앱 언어 추종. 서브메뉴 핸들은 유지)
    let mut label: Vec<u16> = spec
        .label
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mii = MENUITEMINFOW {
        cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
        fMask: MIIM_STRING,
        dwTypeData: windows::core::PWSTR(label.as_mut_ptr()),
        ..Default::default()
    };
    let _ = SetMenuItemInfoW(hmenu, pos, true, &mii);
    let sub = GetSubMenu(hmenu, pos as i32); // 소유 서브메뉴 — INITMENUPOPUP 라우팅 키
    Some((icm, sub))
}

/// 폴더의 항목 이름 스냅샷(생성 감지용 07-27) — 셸 invoke는 생성 파일명을 반환하지 않아
/// 전후 diff로 식별(탐색기는 IFolderView 사이트 통보를 받지만 우리는 뷰 미노출).
fn dir_names(dir: impl AsRef<Path>) -> HashSet<OsString> {
    std::fs::read_dir(dir)
        .map(|it| it.flatten().map(|e| e.file_name()).collect())
        .unwrap_or_default()
}

/// invoke 후 신규 항목 감지 — **정확히 1개**일 때만 경로 반환(압축 해제 등 다건 오탐 방지).
/// `retries`×20ms 재시도(생성이 invoke 직후 FS에 늦게 보이는 경우 대비).
fn detect_created(dir: &Path, before: &HashSet<OsString>, retries: u32) -> Option<PathBuf> {
    for i in 0..=retries {
        if i > 0 {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let now = dir_names(dir);
        let mut fresh = now.difference(before);
        if let Some(first) = fresh.next() {
            if fresh.next().is_none() {
                return Some(dir.join(first));
            }
            return None; // 2개 이상 — 새로 만들기 아님
        }
    }
    None
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

#[cfg(test)]
mod tests {
    use super::*;

    /// ICM 없는 가짜 호스트 — 라우팅 선택만 검증(handle은 icm2/icm3 None이면 무동작).
    fn fake_host(submenu: isize, first: u32, last: u32) -> MenuHost {
        MenuHost {
            icm2: None,
            icm3: None,
            submenu,
            first,
            last,
        }
    }

    /// G8-20: ACTIVE가 비면(메뉴 닫힌 뒤) 메뉴 메시지는 포워딩되지 않고 None(DefWindowProc).
    #[test]
    fn forward_stops_once_active_cleared() {
        ACTIVE.set(vec![fake_host(0, ID_SHELL_FIRST, ID_SHELL_LAST)]);
        // 표시 중: 모르는 HMENU도 주 메뉴 핸들러로 폴백 소비(Some(0)).
        assert_eq!(
            forward_menu_msg(WM_INITMENUPOPUP, WPARAM(0x1234), LPARAM(0)),
            Some(LRESULT(0))
        );
        // 메뉴 닫힘 = track이 TrackPopupMenuEx 직후 비움 → 이후 메시지는 미소비.
        ACTIVE.set(Vec::new());
        assert_eq!(
            forward_menu_msg(WM_INITMENUPOPUP, WPARAM(0x1234), LPARAM(0)),
            None
        );
        assert_eq!(forward_menu_msg(WM_MENUCHAR, WPARAM(0), LPARAM(0)), None);
    }

    /// 메뉴 메시지가 아닌 것은 ACTIVE와 무관하게 None.
    #[test]
    fn non_menu_messages_pass_through() {
        ACTIVE.set(vec![fake_host(0, ID_SHELL_FIRST, ID_SHELL_LAST)]);
        assert_eq!(forward_menu_msg(WM_NULL, WPARAM(0), LPARAM(0)), None);
        ACTIVE.set(Vec::new());
    }
}
