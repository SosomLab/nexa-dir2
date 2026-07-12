//! Win32 창·메시지 루프 — 렌더·입력은 nexa-gui 위젯, 패널 로직은 panel.rs(플랫폼 중립).
//! 이 모듈의 책임: 창 수명·백버퍼(DW)·**듀얼 패널 배치(스플리터)·활성 패널 라우팅**(M2-2)·
//! WM_* → [`nexa_gui::InputEvent`] 번역·[`nexa_gui::Invalidations`] → `InvalidateRect` 번역.
//! F3 = 활성 패널 200프레임 스크롤 벤치. 타이틀바 = 활성 패널 경로·행/선택 수·페인트 시간.

use std::path::PathBuf;
use std::time::Instant;

use nexa_gui::widgets::RowSource;
use nexa_gui::{Column, InputEvent, Invalidations, Key, Rect as GRect, Theme};
use nexa_tree::Tree;
use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, EndPaint, InvalidateRect, UpdateWindow, PAINTSTRUCT, SRCCOPY,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
use windows::Win32::UI::HiDpi::{
    GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, VK_CONTROL, VK_DOWN, VK_END, VK_ESCAPE, VK_F3,
    VK_HOME, VK_LEFT, VK_NEXT, VK_OEM_PERIOD, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_SPACE,
    VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetMessageW,
    GetWindowLongPtrW, KillTimer, LoadCursorW, PostQuitMessage, RegisterClassW, SetTimer,
    SetWindowLongPtrW, SetWindowPos, SetWindowTextW, TranslateMessage, CREATESTRUCTW, CS_DBLCLKS,
    CW_USEDEFAULT, GWLP_USERDATA, IDC_ARROW, MSG, SWP_NOACTIVATE, SWP_NOZORDER, WM_CHAR,
    WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCREATE, WM_NCDESTROY, WM_PAINT,
    WM_RBUTTONDOWN, WM_SIZE, WM_SYSKEYDOWN, WM_TIMER, WM_XBUTTONDOWN, WNDCLASSW,
    WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

use crate::dw::{DwBackend, DwCtx};
use crate::icons::shell::ShellIcons;
use crate::panel::{NavCtx, Panel, PanelMetrics};
use crate::source::{COL_EXT, COL_KIND, COL_MODIFIED, COL_NAME, COL_SIZE};

/// wParam 마우스 수식키 비트(winuser.h MK_SHIFT/MK_CONTROL).
const MK_SHIFT: usize = 0x0004;
const MK_CONTROL: usize = 0x0008;

/// F3 스크롤 벤치 프레임 수(게이트 관측).
const BENCH_FRAMES: usize = 200;
/// 타입어헤드 타임아웃 점검 타이머 id·주기(ms).
const TIMER_TYPEAHEAD: usize = 1;
const TIMER_TICK_MS: u32 = 250;
/// 셸 아이콘 로딩 큐 타이머 id(주기 = icons::shell::TICK_MS — 원본 A-4 속도 제한).
const TIMER_ICONS: usize = 2;
/// 패널 최소 폭(논리 px)·스플리터 히트 존 반폭.
const MIN_PANEL: i32 = 200;
const SPLIT_HALF: i32 = 3;

/// 단조 시각(ms) — 타입어헤드 버퍼 타임아웃 판정용(프로세스 기동 기준).
fn now_ms() -> u64 {
    use std::sync::OnceLock;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// 평균 페인트 시간(µs) 누적 — 벤치 시 리셋. `first_render_ms` = 기동→첫 페인트(게이트).
#[derive(Default)]
struct PaintStats {
    total_us: u64,
    frames: u32,
    first_render_ms: Option<u64>,
}

impl PaintStats {
    fn add(&mut self, us: u64) {
        self.total_us += us;
        self.frames += 1;
    }
    fn avg_us(&self) -> u64 {
        if self.frames == 0 {
            0
        } else {
            self.total_us / self.frames as u64
        }
    }
    fn reset(&mut self) {
        let first = self.first_render_ms;
        *self = PaintStats::default();
        self.first_render_ms = first;
    }
}

/// 창 단위 상태 — `GWLP_USERDATA`에 Box raw 포인터로 보관(WM_NCCREATE~WM_NCDESTROY).
struct State {
    /// 듀얼 패널(0=좌 주, 1=우 — docs/20 §2). 우 패널 숨김 토글은 후속.
    panels: [Panel; 2],
    /// 활성 패널(키보드·타이틀 기준). 클릭·Tab으로 전환.
    active: usize,
    /// 좌 패널 폭 비율(스플리터 드래그).
    split: f32,
    split_drag: bool,
    theme: Theme,
    dpi: u32,
    dw: Option<DwBackend>,
    icons: std::cell::RefCell<ShellIcons>,
    stats: PaintStats,
    tz: i32,
    /// 가시성 필터(전역 ViewOptions — 영속은 M2-5).
    show_hidden: bool,
    show_dotfiles: bool,
}

impl State {
    fn nav_ctx(&self) -> NavCtx {
        NavCtx {
            show_hidden: self.show_hidden,
            show_dotfiles: self.show_dotfiles,
            tz: self.tz,
        }
    }
    fn active_panel(&mut self) -> &mut Panel {
        &mut self.panels[self.active]
    }
    /// 좌표가 속한 패널 인덱스(스플리터 존이면 `None`).
    fn panel_at(&self, x: i32) -> Option<usize> {
        if x < self.panels[0].bounds().right() {
            Some(0)
        } else if x >= self.panels[1].bounds().x {
            Some(1)
        } else {
            None // 스플리터 존
        }
    }
}

/// DPI 의존 지표(고밀도 규약 20px 행 @96dpi).
fn panel_metrics(dpi: u32) -> PanelMetrics {
    let s = |v: i32| (v * dpi as i32) / 96;
    PanelMetrics {
        row_h: s(20).max(14),
        pad_x: s(6),
        indent_w: s(16),
        tab_h: s(22),
        bar_h: s(24),
    }
}

/// 기본 5컬럼(원본 docs/23 §2-1). 폭은 dpi 스케일.
fn columns(dpi: u32) -> Vec<Column> {
    let s = |v: i32| (v * dpi as i32) / 96;
    vec![
        Column::new(COL_NAME, "이름", s(340)),
        Column::new(COL_EXT, "확장자", s(64)),
        Column::new(COL_SIZE, "크기", s(96)).right_aligned(),
        Column::new(COL_MODIFIED, "수정한 날짜", s(140)),
        Column::new(COL_KIND, "종류", s(110)),
    ]
}

/// 로컬 타임존 오프셋(분, UTC 동쪽 양수).
unsafe fn tz_offset_min() -> i32 {
    let mut tzi = TIME_ZONE_INFORMATION::default();
    let code = GetTimeZoneInformation(&mut tzi);
    let bias = tzi.Bias
        + match code {
            1 => tzi.StandardBias,
            2 => tzi.DaylightBias,
            _ => 0,
        };
    -bias
}

/// 시작 루트 경로: argv[1] → %USERPROFILE% → C:\.
fn root_path() -> PathBuf {
    std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("C:\\"))
}

fn open_start_tree(path: &std::path::Path) -> Tree {
    Tree::open(path).unwrap_or_else(|e| {
        eprintln!("{} 열기 실패({e}) — C:\\ 로 대체", path.display());
        Tree::open("C:\\").expect("C:\\ 열기 실패")
    })
}

pub fn run() -> Result<()> {
    let _ = now_ms(); // 기동 시각 고정 — 첫 렌더 계측 기준
    let path = root_path();
    let tz = unsafe { tz_offset_min() };
    let ctx = NavCtx {
        show_hidden: true,
        show_dotfiles: true,
        tz,
    };
    let m = panel_metrics(96); // 실제 DPI는 WM_NCCREATE에서 반영
                               // 좌/우 패널 — 같은 시작 경로의 독립 트리(원본 docs/20: 듀얼 기본, 숨김 토글 후속)
    let left = Panel::new(open_start_tree(&path), ctx, m, columns(96));
    let right = Panel::new(open_start_tree(&path), ctx, m, columns(96));
    let state = Box::new(State {
        panels: [left, right],
        active: 0,
        split: 0.5,
        split_drag: false,
        theme: Theme::default(), // 다크(DR-5) — 모드 선택은 M2-4
        dpi: 96,
        dw: None,
        icons: std::cell::RefCell::new(ShellIcons::new()),
        stats: PaintStats::default(),
        tz,
        show_hidden: true,
        show_dotfiles: true,
    });

    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let hinstance = GetModuleHandleW(None)?;
        let class_name = w!("NexaDir2Main");
        let wc = WNDCLASSW {
            style: CS_DBLCLKS,
            hInstance: hinstance.into(),
            lpszClassName: class_name,
            lpfnWndProc: Some(wndproc),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        let atom = RegisterClassW(&wc);
        debug_assert_ne!(atom, 0, "RegisterClassW 실패");

        CreateWindowExW(
            Default::default(),
            class_name,
            w!("Nexa Dir 2"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1400,
            800,
            None,
            None,
            Some(hinstance.into()),
            Some(Box::into_raw(state) as *const core::ffi::c_void),
        )?;

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

unsafe fn state_of<'a>(hwnd: HWND) -> Option<&'a mut State> {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
    ptr.as_mut()
}

unsafe fn client_rect(hwnd: HWND) -> GRect {
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    GRect::new(0, 0, rc.right - rc.left, rc.bottom - rc.top)
}

unsafe fn flush_invalidations(hwnd: HWND, inv: &mut Invalidations) {
    for r in inv.drain() {
        let rc = RECT {
            left: r.x,
            top: r.y,
            right: r.right(),
            bottom: r.bottom(),
        };
        let _ = InvalidateRect(Some(hwnd), Some(&rc), false);
    }
}

/// 스플리터 x(클라이언트) — split 비율에서 계산(패널 최소 폭 클램프).
unsafe fn splitter_x(hwnd: HWND, st: &State) -> i32 {
    let rc = client_rect(hwnd);
    let s = |v: i32| (v * st.dpi as i32) / 96;
    let min = s(MIN_PANEL);
    ((rc.w as f32 * st.split) as i32).clamp(min.min(rc.w / 2), (rc.w - min).max(rc.w / 2))
}

/// 듀얼 패널 레이아웃(좌 ║ 우 — docs/20 §1).
unsafe fn layout(hwnd: HWND, st: &mut State, inv: &mut Invalidations) {
    let rc = client_rect(hwnd);
    let sx = splitter_x(hwnd, st);
    let s = |v: i32| (v * st.dpi as i32) / 96;
    let half = s(SPLIT_HALF).max(1);
    st.panels[0].set_bounds(GRect::new(0, 0, (sx - half).max(0), rc.h), inv);
    st.panels[1].set_bounds(
        GRect::new(sx + half, 0, (rc.w - sx - half).max(0), rc.h),
        inv,
    );
}

/// 타이틀바 — 활성 패널 기준 경로·행/선택 수·페인트 시간.
unsafe fn update_title(hwnd: HWND, st: &State, note: &str) {
    let p = &st.panels[st.active];
    let sel = p.rows().source().tree().selection_count();
    let sel_txt = if sel > 0 {
        format!(" · 선택 {sel}")
    } else {
        String::new()
    };
    let first_txt = st
        .stats
        .first_render_ms
        .map(|ms| format!(" · 첫렌더 {ms}ms"))
        .unwrap_or_default();
    let side = if st.active == 0 { "좌" } else { "우" };
    let text = format!(
        "Nexa Dir 2 — [{side}] {} [{}행{} · 탭 {}/{} · 평균 {}µs{}]{}\0",
        p.root_path().display(),
        p.rows().source().len(),
        sel_txt,
        p.active_index() + 1,
        p.tab_count(),
        st.stats.avg_us(),
        first_txt,
        note,
    );
    let wtext: Vec<u16> = text.encode_utf16().collect();
    let _ = SetWindowTextW(hwnd, PCWSTR(wtext.as_ptr()));
}

unsafe fn ensure_dw(st: &mut State, hdc: windows::Win32::Graphics::Gdi::HDC, w: i32, h: i32) {
    match &mut st.dw {
        None => match DwBackend::new(hdc, w, h, st.dpi) {
            Ok(b) => st.dw = Some(b),
            Err(e) => eprintln!("DirectWrite 초기화 실패: {e}"),
        },
        Some(b) => {
            if b.size() != (w, h) {
                let _ = b.resize(w, h);
            }
        }
    }
}

/// 두 패널 + 스플리터를 DW 백버퍼에 그린 뒤 BitBlt.
unsafe fn paint(hwnd: HWND, st: &mut State) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let rc = client_rect(hwnd);
    let t0 = Instant::now();

    ensure_dw(st, hdc, rc.w, rc.h);
    if let Some(back) = &st.dw {
        let mut ctx = DwCtx {
            back,
            icons: &st.icons,
        };
        st.panels[0].paint(&mut ctx, &st.theme);
        st.panels[1].paint(&mut ctx, &st.theme);
        // 스플리터(드래그 중 accent)
        use nexa_gui::DrawCtx;
        let lx = st.panels[0].bounds().right();
        let w = st.panels[1].bounds().x - lx;
        let color = if st.split_drag {
            st.theme.accent
        } else {
            st.theme.border
        };
        ctx.fill_rect(GRect::new(lx, 0, w, rc.h), color);
        let _ = BitBlt(hdc, 0, 0, rc.w, rc.h, Some(back.memory_dc()), 0, 0, SRCCOPY);
    }

    st.stats.add(t0.elapsed().as_micros() as u64);
    if st.stats.first_render_ms.is_none() {
        st.stats.first_render_ms = Some(now_ms());
    }
    let _ = EndPaint(hwnd, &ps);

    if st.icons.borrow().has_pending() {
        SetTimer(Some(hwnd), TIMER_ICONS, crate::icons::shell::TICK_MS, None);
    }
    if st.stats.frames == 1 || st.stats.frames.is_multiple_of(20) {
        update_title(hwnd, st, "");
    }
}

/// 활성 패널 전환(클릭·Tab) — 탭 바 accent로 시각화.
unsafe fn set_active(hwnd: HWND, st: &mut State, idx: usize) {
    if st.active != idx {
        st.active = idx;
        let mut inv = Invalidations::default();
        st.panels[0].set_focused(idx == 0, &mut inv);
        st.panels[1].set_focused(idx == 1, &mut inv);
        flush_invalidations(hwnd, &mut inv);
        update_title(hwnd, st, "");
    }
}

/// F3 — 활성 패널 200프레임 스크롤 벤치.
unsafe fn bench(hwnd: HWND, st: &mut State) {
    let ctx_ev = InputEvent::Key {
        key: Key::Home,
        shift: false,
        ctrl: false,
    };
    let mut inv = Invalidations::default();
    st.active_panel().on_event(&ctx_ev, &mut inv);
    flush_invalidations(hwnd, &mut inv);
    let _ = UpdateWindow(hwnd);
    st.stats.reset();
    for _ in 0..BENCH_FRAMES {
        if let Some(s) = state_of(hwnd) {
            let mut inv = Invalidations::default();
            s.active_panel()
                .on_event(&InputEvent::Wheel { delta: -120 }, &mut inv);
            flush_invalidations(hwnd, &mut inv);
        }
        let _ = UpdateWindow(hwnd);
    }
    if let Some(s) = state_of(hwnd) {
        update_title(hwnd, s, " · 벤치 완료");
    }
}

fn vk_to_key(vk: u16) -> Option<Key> {
    match vk {
        k if k == VK_UP.0 => Some(Key::Up),
        k if k == VK_DOWN.0 => Some(Key::Down),
        k if k == VK_PRIOR.0 => Some(Key::PageUp),
        k if k == VK_NEXT.0 => Some(Key::PageDown),
        k if k == VK_HOME.0 => Some(Key::Home),
        k if k == VK_END.0 => Some(Key::End),
        k if k == VK_RIGHT.0 => Some(Key::Right),
        k if k == VK_LEFT.0 => Some(Key::Left),
        k if k == VK_SPACE.0 => Some(Key::Space),
        _ => None,
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let cs = &*(lparam.0 as *const CREATESTRUCTW);
            let ptr = cs.lpCreateParams as *mut State;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize);
            if let Some(st) = ptr.as_mut() {
                st.dpi = GetDpiForWindow(hwnd);
                let mut inv = Invalidations::default();
                let m = panel_metrics(st.dpi);
                let cols = columns(st.dpi);
                st.panels[0].set_metrics(m, cols.clone(), &mut inv);
                st.panels[1].set_metrics(m, cols, &mut inv);
                st.panels[1].set_focused(false, &mut inv);
                update_title(hwnd, st, "");
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_PAINT => {
            if let Some(st) = state_of(hwnd) {
                paint(hwnd, st);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_SIZE => {
            if let Some(st) = state_of(hwnd) {
                let mut inv = Invalidations::default();
                layout(hwnd, st, &mut inv);
                flush_invalidations(hwnd, &mut inv);
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            if let Some(st) = state_of(hwnd) {
                let delta = (wparam.0 >> 16) as i16 as i32;
                let ev = if wparam.0 & MK_SHIFT != 0 {
                    InputEvent::HWheel { delta: -delta }
                } else {
                    InputEvent::Wheel { delta }
                };
                let mut inv = Invalidations::default();
                st.active_panel().on_event(&ev, &mut inv);
                flush_invalidations(hwnd, &mut inv);
            }
            LRESULT(0)
        }
        WM_MOUSEHWHEEL => {
            if let Some(st) = state_of(hwnd) {
                let delta = (wparam.0 >> 16) as i16 as i32;
                let mut inv = Invalidations::default();
                st.active_panel()
                    .on_event(&InputEvent::HWheel { delta }, &mut inv);
                flush_invalidations(hwnd, &mut inv);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            if let Some(st) = state_of(hwnd) {
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                let shift = wparam.0 & MK_SHIFT != 0;
                let ctrl = wparam.0 & MK_CONTROL != 0;
                let mut inv = Invalidations::default();
                SetCapture(hwnd); // 스플리터·리사이즈·러버밴드 드래그 공용
                let sx = splitter_x(hwnd, st);
                let half = (SPLIT_HALF * st.dpi as i32) / 96;
                if st.active_panel().pathbar.is_editing() {
                    // 포커스아웃 = 편집 취소(docs/27 §2)
                    st.active_panel().pathbar.cancel_edit(&mut inv);
                } else if (x - sx).abs() <= half.max(1) {
                    st.split_drag = true;
                    let _ = InvalidateRect(Some(hwnd), None, false);
                } else if let Some(idx) = st.panel_at(x) {
                    set_active(hwnd, st, idx);
                    let ev = InputEvent::MouseDown { x, y, shift, ctrl };
                    st.panels[idx].on_event(&ev, &mut inv);
                    let ctx = st.nav_ctx();
                    st.panels[idx].drain_actions(ctx, &mut inv);
                }
                flush_invalidations(hwnd, &mut inv);
                update_title(hwnd, st, "");
            }
            LRESULT(0)
        }
        WM_RBUTTONDOWN => {
            if let Some(st) = state_of(hwnd) {
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                if let Some(idx) = st.panel_at(x) {
                    set_active(hwnd, st, idx);
                    let mut inv = Invalidations::default();
                    st.panels[idx].on_event(&InputEvent::RightDown { x, y }, &mut inv);
                    flush_invalidations(hwnd, &mut inv);
                }
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if let Some(st) = state_of(hwnd) {
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                let mut inv = Invalidations::default();
                if st.split_drag {
                    let rc = client_rect(hwnd);
                    if rc.w > 0 {
                        st.split = (x as f32 / rc.w as f32).clamp(0.1, 0.9);
                        layout(hwnd, st, &mut inv);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                } else {
                    let ev = InputEvent::MouseMove { x, y };
                    st.panels[0].on_event(&ev, &mut inv);
                    st.panels[1].on_event(&ev, &mut inv);
                }
                flush_invalidations(hwnd, &mut inv);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            if let Some(st) = state_of(hwnd) {
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                if st.split_drag {
                    st.split_drag = false;
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                let ev = InputEvent::MouseUp { x, y };
                let mut inv = Invalidations::default();
                st.panels[0].on_event(&ev, &mut inv);
                st.panels[1].on_event(&ev, &mut inv);
                flush_invalidations(hwnd, &mut inv);
            }
            let _ = ReleaseCapture();
            LRESULT(0)
        }
        WM_XBUTTONDOWN => {
            if let Some(st) = state_of(hwnd) {
                let mut inv = Invalidations::default();
                let ctx = st.nav_ctx();
                match (wparam.0 >> 16) & 0xFFFF {
                    1 => st.active_panel().nav_back(ctx, &mut inv),
                    2 => st.active_panel().nav_forward(ctx, &mut inv),
                    _ => {}
                }
                flush_invalidations(hwnd, &mut inv);
                update_title(hwnd, st, "");
            }
            LRESULT(0)
        }
        WM_LBUTTONDBLCLK => {
            if let Some(st) = state_of(hwnd) {
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                if let Some(idx) = st.panel_at(x) {
                    set_active(hwnd, st, idx);
                    let rows = st.panels[idx].rows();
                    let hit = (!rows.marker_hit(x, y))
                        .then(|| rows.row_at(x, y))
                        .flatten();
                    if let Some(row) = hit {
                        let mut inv = Invalidations::default();
                        let ctx = st.nav_ctx();
                        st.panels[idx].activate_row(row, ctx, &mut inv);
                        flush_invalidations(hwnd, &mut inv);
                        update_title(hwnd, st, "");
                    }
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if let Some(st) = state_of(hwnd) {
                let vk = wparam.0 as u16;
                let shift = GetKeyState(VK_SHIFT.0 as i32) < 0;
                let ctrl = GetKeyState(VK_CONTROL.0 as i32) < 0;
                let ctx = st.nav_ctx();
                let mut inv = Invalidations::default();
                if st.active_panel().pathbar.is_editing() {
                    if vk == VK_RETURN.0 {
                        st.active_panel().pathbar.submit_edit(&mut inv);
                        st.active_panel().drain_actions(ctx, &mut inv);
                    } else if vk == VK_ESCAPE.0 {
                        st.active_panel().pathbar.cancel_edit(&mut inv);
                    }
                    flush_invalidations(hwnd, &mut inv);
                    update_title(hwnd, st, "");
                    return LRESULT(0);
                }
                if vk == VK_F3.0 {
                    bench(hwnd, st);
                } else if vk == VK_TAB.0 {
                    if ctrl {
                        st.active_panel().next_tab(&mut inv); // Ctrl+Tab = 다음 탭
                    } else {
                        let next = 1 - st.active;
                        set_active(hwnd, st, next); // Tab = 패널 전환(커맨더 규약)
                    }
                } else if vk == b'T' as u16 && ctrl {
                    st.active_panel().new_tab(ctx, &mut inv); // Ctrl+T = 새 탭
                } else if vk == b'W' as u16 && ctrl {
                    let i = st.active_panel().active_index();
                    st.active_panel().close_tab(i, &mut inv); // Ctrl+W = 탭 닫기
                } else if vk == b'A' as u16 && ctrl {
                    st.active_panel().on_event(&InputEvent::SelectAll, &mut inv);
                } else if vk == VK_RETURN.0 {
                    if let Some(c) = st.active_panel().rows().caret() {
                        st.active_panel().activate_row(c, ctx, &mut inv);
                    }
                } else if vk == b'H' as u16 && ctrl {
                    st.show_hidden = !st.show_hidden;
                    let ctx = st.nav_ctx();
                    st.active_panel().reopen_filtered(ctx, &mut inv);
                } else if vk == VK_OEM_PERIOD.0 && ctrl {
                    st.show_dotfiles = !st.show_dotfiles;
                    let ctx = st.nav_ctx();
                    st.active_panel().reopen_filtered(ctx, &mut inv);
                } else if let Some(key) = vk_to_key(vk) {
                    st.active_panel()
                        .on_event(&InputEvent::Key { key, shift, ctrl }, &mut inv);
                }
                flush_invalidations(hwnd, &mut inv);
                update_title(hwnd, st, "");
            }
            LRESULT(0)
        }
        WM_SYSKEYDOWN => {
            let vk = wparam.0 as u16;
            if let Some(st) = state_of(hwnd) {
                let ctx = st.nav_ctx();
                let mut inv = Invalidations::default();
                let handled = if vk == VK_LEFT.0 {
                    st.active_panel().nav_back(ctx, &mut inv);
                    true
                } else if vk == VK_RIGHT.0 {
                    st.active_panel().nav_forward(ctx, &mut inv);
                    true
                } else if vk == VK_UP.0 {
                    st.active_panel().nav_up(ctx, &mut inv);
                    true
                } else {
                    false
                };
                if handled {
                    flush_invalidations(hwnd, &mut inv);
                    update_title(hwnd, st, "");
                    return LRESULT(0);
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_CHAR => {
            if let Some(st) = state_of(hwnd) {
                if let Some(c) = char::from_u32(wparam.0 as u32) {
                    let mut inv = Invalidations::default();
                    if st.active_panel().pathbar.is_editing() {
                        if c == '\u{8}' || !c.is_control() {
                            st.active_panel().pathbar.edit_char(c, &mut inv);
                        }
                    } else if GetKeyState(VK_CONTROL.0 as i32) >= 0
                        && (c == '\u{8}' || (!c.is_control() && c != ' '))
                    {
                        st.active_panel().on_event(
                            &InputEvent::Char {
                                c,
                                now_ms: now_ms(),
                            },
                            &mut inv,
                        );
                        if !st.active_panel().rows().typeahead_text().is_empty() {
                            SetTimer(Some(hwnd), TIMER_TYPEAHEAD, TIMER_TICK_MS, None);
                        }
                    }
                    flush_invalidations(hwnd, &mut inv);
                    update_title(hwnd, st, "");
                }
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == TIMER_TYPEAHEAD {
                if let Some(st) = state_of(hwnd) {
                    let mut inv = Invalidations::default();
                    let now = now_ms();
                    st.panels[0].rows_mut().tick(now, &mut inv);
                    st.panels[1].rows_mut().tick(now, &mut inv);
                    flush_invalidations(hwnd, &mut inv);
                    if st.panels[0].rows().typeahead_text().is_empty()
                        && st.panels[1].rows().typeahead_text().is_empty()
                    {
                        let _ = KillTimer(Some(hwnd), TIMER_TYPEAHEAD);
                    }
                }
            } else if wparam.0 == TIMER_ICONS {
                if let Some(st) = state_of(hwnd) {
                    if st.icons.borrow_mut().tick() {
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                    if !st.icons.borrow().has_pending() {
                        let _ = KillTimer(Some(hwnd), TIMER_ICONS);
                    }
                }
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            if let Some(st) = state_of(hwnd) {
                let dpi = (wparam.0 & 0xFFFF) as u32;
                st.dpi = dpi;
                if let Some(dw) = &mut st.dw {
                    let _ = dw.set_dpi(dpi);
                }
                let mut inv = Invalidations::default();
                let m = panel_metrics(dpi);
                let cols = columns(dpi);
                st.panels[0].set_metrics(m, cols.clone(), &mut inv);
                st.panels[1].set_metrics(m, cols, &mut inv);
                let rc = &*(lparam.0 as *const RECT);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    rc.left,
                    rc.top,
                    rc.right - rc.left,
                    rc.bottom - rc.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                layout(hwnd, st, &mut inv);
                flush_invalidations(hwnd, &mut inv);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_NCDESTROY => {
            let ptr = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) as *mut State;
            if !ptr.is_null() {
                drop(Box::from_raw(ptr));
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
