//! **전용 메뉴 스레드**(10-02 X-61 — 우클릭 가속 설계 A, docs/audit/20261002-ultracode/ctxmenu-latency.md).
//!
//! 우클릭 지연의 95%는 등록된 셸 확장 19개가 `QueryContextMenu`에서 실행되는 시간(0.6~1.4 s)이고,
//! `IContextMenu`는 프록시가 없어 만든 아파트(스레드) 밖으로 넘길 수 없다. 그래서 **STA 스레드
//! 하나가 메뉴의 전 생애**(구축 → 표시 → 서브메뉴 포워딩 → 명령 실행)를 맡는다:
//!
//! - 선택이 머물면 UI 스레드가 [`MenuThread::prepare`]로 **미리 구축**을 시킨다(결과는 이 스레드에
//!   보관). 우클릭 때 [`MenuThread::show`] — 준비된 메뉴가 같은 대상이면 즉시 `track`(수 ms), 아니면
//!   그때 구축 후 표시. 어느 쪽이든 **UI 스레드는 막히지 않는다**.
//! - 숨은 소유자 창(이 스레드 소유)이 `TrackPopupMenuEx`의 owner — 메뉴 메시지(WM_INITMENUPOPUP·
//!   DRAWITEM·MEASUREITEM·MENUCHAR)가 그 wndproc으로 와 [`shellmenu::forward_menu_msg`]로 간다
//!   (`ACTIVE`는 thread_local이라 이 스레드 안에서 완결).
//! - 표시 전 `AttachThreadInput(메뉴, UI)`로 입력 큐를 합쳐 전경·키보드 상태를 공유(다른 스레드가
//!   띄운 메뉴가 바깥 클릭에 닫히지 않는 고전 문제의 표준 해법). 표시 후 분리.
//! - 결과는 `WM_APP_CTXMENU_RESULT`(lparam = Box<(gen, MenuReq, Outcome)>)로 주 창에 — 세대 번호가
//!   맞을 때만 반영(선택이 바뀐 뒤 늦게 닫힌 메뉴 무시).

use std::path::PathBuf;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, PostMessageW, RegisterClassW,
    TranslateMessage, CW_USEDEFAULT, MSG, WM_APP, WM_DRAWITEM, WM_INITMENUPOPUP, WM_MEASUREITEM,
    WM_MENUCHAR, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
};

use crate::shellmenu::{self, CustomItem, NewSpec, Outcome, Prepared};

/// 메뉴 스레드 내부 메시지(숨은 창으로 PostMessage — lparam = Box 포인터).
const WM_MT_PREPARE: u32 = WM_APP + 0x40;
const WM_MT_SHOW: u32 = WM_APP + 0x41;
const WM_MT_INVALIDATE: u32 = WM_APP + 0x42;

/// 항목 메뉴 요청(Send — 경로·문자열만). UI 스레드가 만들고, 결과 처리 때 그대로 돌려받는다.
#[derive(Debug, Clone)]
pub struct MenuReq {
    /// 메뉴 대상(같은 부모 폴더로 축소된 경로들).
    pub targets: Vec<PathBuf>,
    /// 축소 전 전체 선택(교차 폴더 포함) — copy/cut/경로 복사 가로채기용.
    pub full: Vec<PathBuf>,
    /// Shift(확장 동사).
    pub extended: bool,
    pub intercept: Vec<String>,
    pub hide: Vec<(String, u32, String)>,
    pub custom: Vec<CustomItem>,
    pub new_spec: Option<NewSpec>,
    /// "여기에 붙여넣기" 대상(단일 폴더 선택).
    pub paste_dir: Option<PathBuf>,
}

impl MenuReq {
    /// 준비된 메뉴를 재사용할 수 있는가 — 대상·확장 동사·고유 항목·New 대상이 같을 때.
    fn same_menu(&self, other: &MenuReq) -> bool {
        self.targets == other.targets
            && self.extended == other.extended
            && self.new_spec.as_ref().map(|n| &n.dir) == other.new_spec.as_ref().map(|n| &n.dir)
            && self.custom.len() == other.custom.len()
            && self
                .custom
                .iter()
                .zip(&other.custom)
                .all(|(a, b)| a.id == b.id && a.label == b.label && a.enabled == b.enabled)
            && self.hide == other.hide
    }
}

struct ShowReq {
    req: MenuReq,
    at: Option<POINT>,
}

/// 스레드 상태(thread_local — 메뉴 스레드 전용). 준비된 메뉴는 1개만 보관(최신 대상).
struct ThreadState {
    main_hwnd: isize,
    main_tid: u32,
    result_msg: u32,
    prepared: Option<(u64, MenuReq, Prepared)>,
}

thread_local! {
    static TS: std::cell::RefCell<Option<ThreadState>> = const { std::cell::RefCell::new(None) };
}

/// UI 스레드가 보유하는 핸들(숨은 창 hwnd). 창 소멸은 프로세스 종료와 함께(메뉴 스레드는 데몬).
pub struct MenuThread {
    hwnd: isize,
}

impl MenuThread {
    /// 메뉴 스레드 기동(기동 1회 — WM_CREATE). `result_msg` = 결과를 주 창에 보낼 WM_APP 번호.
    /// 창 생성이 끝날 때까지 기다린다(실패 시 None — 호출자는 동기 경로 폴백).
    pub fn spawn(main_hwnd: isize, main_tid: u32, result_msg: u32) -> Option<MenuThread> {
        let (tx, rx) = std::sync::mpsc::channel::<isize>();
        std::thread::Builder::new()
            .name("nexa-ctxmenu".into())
            .spawn(move || unsafe {
                use windows::Win32::System::Com::{
                    CoInitializeEx, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
                };
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
                TS.set(Some(ThreadState {
                    main_hwnd,
                    main_tid,
                    result_msg,
                    prepared: None,
                }));
                let hwnd = create_host_window();
                let _ = tx.send(hwnd.0 as isize);
                if hwnd.0.is_null() {
                    return;
                }
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            })
            .ok()?;
        let hwnd = rx.recv().ok()?;
        (hwnd != 0).then_some(MenuThread { hwnd })
    }

    fn post(&self, msg: u32, w: usize, l: isize) -> bool {
        unsafe {
            PostMessageW(
                Some(HWND(self.hwnd as *mut core::ffi::c_void)),
                msg,
                WPARAM(w),
                LPARAM(l),
            )
            .is_ok()
        }
    }

    /// 미리 구축(선택이 머물 때). 이전에 준비된 메뉴는 버려진다.
    pub fn prepare(&self, gen: u64, req: MenuReq) {
        let p = Box::into_raw(Box::new(req)) as isize;
        if !self.post(WM_MT_PREPARE, gen as usize, p) {
            drop(unsafe { Box::from_raw(p as *mut MenuReq) });
        }
    }

    /// 표시 요청(우클릭). 준비된 메뉴가 같은 대상이면 즉시, 아니면 구축 후 표시. 결과는
    /// `result_msg`로 주 창에(wparam = gen, lparam = Box<(MenuReq, Outcome)>).
    pub fn show(&self, gen: u64, req: MenuReq, at: Option<POINT>) -> bool {
        let p = Box::into_raw(Box::new(ShowReq { req, at })) as isize;
        if self.post(WM_MT_SHOW, gen as usize, p) {
            true
        } else {
            drop(unsafe { Box::from_raw(p as *mut ShowReq) });
            false
        }
    }

    /// 준비된 메뉴 폐기(폴더 변경·선택 해제).
    pub fn invalidate(&self) {
        let _ = self.post(WM_MT_INVALIDATE, 0, 0);
    }
}

unsafe fn create_host_window() -> HWND {
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    static REGISTER: std::sync::Once = std::sync::Once::new();
    let class = windows::core::w!("NexaDirMenuHost");
    REGISTER.call_once(|| {
        let wc = WNDCLASSW {
            lpfnWndProc: Some(host_proc),
            hInstance: GetModuleHandleW(None).unwrap_or_default().into(),
            lpszClassName: class,
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);
    });
    // 보이지 않는 소유자 창(도구 창 — 작업 표시줄·Alt+Tab에 안 나옴). 메뉴 메시지만 받는다.
    CreateWindowExW(
        WS_EX_TOOLWINDOW,
        class,
        PCWSTR::null(),
        WS_POPUP,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        0,
        0,
        None,
        None,
        None,
        None,
    )
    .unwrap_or_default()
}

unsafe extern "system" fn host_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        m if m == WM_MT_PREPARE => {
            let req = *Box::from_raw(lparam.0 as *mut MenuReq);
            let gen = wparam.0 as u64;
            TS.with_borrow_mut(|ts| {
                if let Some(ts) = ts {
                    ts.prepared = None; // 이전 준비분 폐기(Drop = HMENU·PIDL 해제)
                    if let Some(p) = build_from(hwnd, &req) {
                        ts.prepared = Some((gen, req, p));
                    }
                }
            });
            LRESULT(0)
        }
        m if m == WM_MT_SHOW => {
            let ShowReq { req, at } = *Box::from_raw(lparam.0 as *mut ShowReq);
            let gen = wparam.0 as u64;
            // 준비분 재사용 판정 — RefCell 차용은 track 전에 끝낸다(모달 펌프 중 재진입 대비)
            let (main_hwnd, main_tid, result_msg, ready) = TS.with_borrow_mut(|ts| {
                let ts = ts.as_mut().expect("menu thread state");
                let ready = match ts.prepared.take() {
                    Some((_, preq, p)) if preq.same_menu(&req) => Some(p),
                    _ => None,
                };
                (ts.main_hwnd, ts.main_tid, ts.result_msg, ready)
            });
            let prepared = ready.or_else(|| build_from(hwnd, &req));
            let outcome = match prepared {
                Some(p) => {
                    let intercept: Vec<&str> = req.intercept.iter().map(String::as_str).collect();
                    show_attached(hwnd, main_hwnd, main_tid, || {
                        shellmenu::track(p, hwnd, &intercept, None, at)
                    })
                }
                None => Outcome::Cancelled,
            };
            let payload = Box::into_raw(Box::new((req, outcome))) as isize;
            let main = HWND(main_hwnd as *mut core::ffi::c_void);
            if PostMessageW(
                Some(main),
                result_msg,
                WPARAM(gen as usize),
                LPARAM(payload),
            )
            .is_err()
            {
                drop(Box::from_raw(payload as *mut (MenuReq, Outcome)));
            }
            LRESULT(0)
        }
        m if m == WM_MT_INVALIDATE => {
            TS.with_borrow_mut(|ts| {
                if let Some(ts) = ts {
                    ts.prepared = None;
                }
            });
            LRESULT(0)
        }
        // 셸 메뉴 표시 구간 — IContextMenu2/3 메시지 포워딩(동적 서브메뉴·아이콘)
        m if matches!(
            m,
            WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR
        ) =>
        {
            match shellmenu::forward_menu_msg(m, wparam, lparam) {
                Some(r) => r,
                None => DefWindowProcW(hwnd, msg, wparam, lparam),
            }
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn build_from(hwnd: HWND, req: &MenuReq) -> Option<Prepared> {
    let hide: Vec<(&str, u32, String)> = req
        .hide
        .iter()
        .map(|(v, id, l)| (v.as_str(), *id, l.clone()))
        .collect();
    shellmenu::prepare_items(
        hwnd,
        &req.targets,
        req.extended,
        &hide,
        &req.custom,
        req.new_spec.as_ref(),
    )
}

/// 입력 큐를 UI 스레드와 합친 채 메뉴를 띄운다(전경·키보드 공유). 끝나면 분리하고 전경을 주
/// 창으로 되돌린다.
unsafe fn show_attached<F: FnOnce() -> Outcome>(
    _host: HWND,
    main_hwnd: isize,
    main_tid: u32,
    f: F,
) -> Outcome {
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
    let me = GetCurrentThreadId();
    let attached = AttachThreadInput(me, main_tid, true).as_bool();
    let main = HWND(main_hwnd as *mut core::ffi::c_void);
    let _ = SetForegroundWindow(main); // 바깥 클릭 시 닫힘 + 키보드 내비(합쳐진 큐)
    let out = f();
    if attached {
        let _ = SetFocus(Some(main));
        let _ = AttachThreadInput(me, main_tid, false);
    }
    out
}


#[cfg(test)]
mod tests {
    use super::*;

    fn req(targets: &[&str], ext: bool) -> MenuReq {
        MenuReq {
            targets: targets.iter().map(PathBuf::from).collect(),
            full: Vec::new(),
            extended: ext,
            intercept: vec!["delete".into()],
            hide: vec![("copyaspath".into(), 0x8001, "경로 복사".into())],
            custom: vec![CustomItem {
                id: 0x8002,
                label: "이름 복사".into(),
                enabled: true,
                after_id: Some(0x8001),
            }],
            new_spec: Some(NewSpec {
                dir: PathBuf::from("C:/x"),
                label: "새로 만들기".into(),
            }),
            paste_dir: None,
        }
    }

    #[test]
    fn same_menu_requires_identical_targets_flags_and_merged_items() {
        let a = req(&["C:/x/a.txt"], false);
        assert!(a.same_menu(&req(&["C:/x/a.txt"], false)));
        assert!(!a.same_menu(&req(&["C:/x/b.txt"], false)), "대상 다름");
        assert!(
            !a.same_menu(&req(&["C:/x/a.txt"], true)),
            "Shift 확장 동사 다름"
        );
        let mut c = req(&["C:/x/a.txt"], false);
        c.custom[0].enabled = false;
        assert!(!a.same_menu(&c), "고유 항목 상태 다름");
        let mut d = req(&["C:/x/a.txt"], false);
        d.new_spec = None;
        assert!(!a.same_menu(&d), "New 대상 다름");
    }
}
