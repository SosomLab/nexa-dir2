//! Win32 창·메시지 루프(M0-5) — M1-1부터 렌더·입력은 nexa-gui 위젯으로 위임.
//! 이 모듈의 책임: 창 수명·더블 버퍼(메모리 DC)·WM_* → [`nexa_gui::InputEvent`] 번역·
//! [`nexa_gui::Invalidations`] → `InvalidateRect` 번역. 렌더 모델(docs/01 §3)은 M0-7 계승.
//!
//! **ADR-0002(M1-2, Accepted)**: 텍스트 렌더링 = DirectWrite GDI interop(기본).
//! GDI 백엔드·F2 전환·F3 벤치는 육안 비교용으로 유지 — M1-3(실제 리스트 배선)에서 제거.
//! 시작 백엔드 강제: `NEXA_RENDER=gdi|dw`.

use std::time::Instant;

use nexa_gui::widgets::{RowSource, VirtualRows};
use nexa_gui::{InputEvent, Invalidations, Key, Rect as GRect, Theme, Widget};
use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::UpdateWindow;
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, DeleteDC,
    DeleteObject, EndPaint, InvalidateRect, SelectObject, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS,
    DEFAULT_CHARSET, DEFAULT_PITCH, FF_DONTCARE, FW_NORMAL, HBITMAP, HDC, HFONT, HGDIOBJ,
    OUT_DEFAULT_PRECIS, PAINTSTRUCT, SRCCOPY,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VK_DOWN, VK_END, VK_F2, VK_F3, VK_HOME, VK_NEXT, VK_PRIOR, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetMessageW,
    GetWindowLongPtrW, LoadCursorW, PostQuitMessage, RegisterClassW, SetWindowLongPtrW,
    SetWindowPos, SetWindowTextW, TranslateMessage, CW_USEDEFAULT, GWLP_USERDATA, IDC_ARROW, MSG,
    SWP_NOACTIVATE, SWP_NOZORDER, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_KEYDOWN,
    WM_MOUSEWHEEL, WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_SIZE, WNDCLASSW, WS_OVERLAPPEDWINDOW,
    WS_VISIBLE,
};

use crate::dw::{DwBackend, DwCtx};
use crate::gdi::GdiCtx;

/// 스파이크 데이터셋 — M1-3에서 nexa-tree 평면 스트림으로 대체.
const TOTAL_ROWS: usize = 100_000;
/// F3 스크롤 벤치 프레임 수.
const BENCH_FRAMES: usize = 200;

struct SpikeRows;

impl RowSource for SpikeRows {
    fn len(&self) -> usize {
        TOTAL_ROWS
    }
    fn row_text(&self, row: usize) -> String {
        format!("{row:>6}  spike-entry-{row:06}.txt — 한글 텍스트 렌더 품질 비교 Quality 0Oo 1lI")
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BackendKind {
    Gdi,
    DirectWrite,
}

impl BackendKind {
    fn label(self) -> &'static str {
        match self {
            BackendKind::Gdi => "GDI",
            BackendKind::DirectWrite => "DirectWrite interop",
        }
    }
}

/// 클라이언트 크기와 1:1인 오프스크린 버퍼(GDI 백엔드). 크기 변경 시 재생성.
struct BackBuffer {
    dc: HDC,
    bmp: HBITMAP,
    old_bmp: HGDIOBJ,
    w: i32,
    h: i32,
}

impl BackBuffer {
    unsafe fn free(self) {
        SelectObject(self.dc, self.old_bmp);
        let _ = DeleteObject(self.bmp.into());
        let _ = DeleteDC(self.dc);
    }
}

/// 평균 페인트 시간(µs) 누적 — 백엔드 전환·벤치 시 리셋.
#[derive(Default)]
struct PaintStats {
    total_us: u64,
    frames: u32,
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
        *self = PaintStats::default();
    }
}

/// 창 단위 상태 — `GWLP_USERDATA`에 Box raw 포인터로 보관(WM_NCCREATE~WM_NCDESTROY).
struct State {
    rows: VirtualRows<SpikeRows>,
    theme: Theme,
    font: HFONT,
    dpi: u32,
    backend: BackendKind,
    back: Option<BackBuffer>,
    dw: Option<DwBackend>,
    stats: PaintStats,
}

impl State {
    unsafe fn new(dpi: u32) -> Self {
        let (font, row_h) = make_font(dpi);
        // ADR-0002 채택 경로 = DirectWrite interop. GDI는 육안 비교용(M1-3에서 제거)
        let backend = match std::env::var("NEXA_RENDER").as_deref() {
            Ok("gdi") => BackendKind::Gdi,
            _ => BackendKind::DirectWrite,
        };
        State {
            rows: VirtualRows::new(SpikeRows, row_h, pad_x(dpi)),
            theme: Theme::default(), // 다크(DR-5) — 모드 선택은 M2 테마 시스템
            font,
            dpi,
            backend,
            back: None,
            dw: None,
            stats: PaintStats::default(),
        }
    }
}

/// DPI에 맞춘 UI 폰트(Segoe UI 9pt)와 행 높이.
unsafe fn make_font(dpi: u32) -> (HFONT, i32) {
    let px_per_pt = dpi as i32; // height(px) = pt * dpi / 72
    let height = -(9 * px_per_pt) / 72;
    let font = CreateFontW(
        height,
        0,
        0,
        0,
        FW_NORMAL.0 as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY,
        DEFAULT_PITCH.0 as u32 | FF_DONTCARE.0 as u32,
        w!("Segoe UI"),
    );
    // 고밀도 행(디자인 규약): 텍스트 높이 + 여백 ≈ 20px@96dpi
    let row_h = (20 * dpi as i32) / 96;
    (font, row_h.max(14))
}

fn pad_x(dpi: u32) -> i32 {
    (12 * dpi as i32) / 96
}

pub fn run() -> Result<()> {
    unsafe {
        // PerMonitorV2 DPI — 매니페스트 도입 전까지 코드로 선언(docs/01 §3)
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let hinstance = GetModuleHandleW(None)?;
        let class_name = w!("NexaDir2Main");
        let wc = WNDCLASSW {
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
            w!("Nexa Dir 2 — ADR-0002 렌더 스파이크"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1200,
            800,
            None,
            None,
            Some(hinstance.into()),
            None,
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

/// 위젯이 수집한 무효화 rect를 OS 무효화로 번역.
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

unsafe fn route_event(hwnd: HWND, st: &mut State, ev: InputEvent) {
    let mut inv = Invalidations::default();
    st.rows.on_event(&ev, &mut inv);
    flush_invalidations(hwnd, &mut inv);
}

/// 백버퍼를 클라이언트 크기와 일치시킨다(불일치 시에만 재생성).
unsafe fn ensure_backbuffer(st: &mut State, hdc: HDC, w: i32, h: i32) {
    if let Some(b) = &st.back {
        if b.w == w && b.h == h {
            return;
        }
    }
    if let Some(old) = st.back.take() {
        old.free();
    }
    let dc = CreateCompatibleDC(Some(hdc));
    let bmp = CreateCompatibleBitmap(hdc, w.max(1), h.max(1));
    let old_bmp = SelectObject(dc, bmp.into());
    st.back = Some(BackBuffer {
        dc,
        bmp,
        old_bmp,
        w,
        h,
    });
}

/// DW 렌더 타깃을 클라이언트 크기와 일치시킨다.
unsafe fn ensure_dw(st: &mut State, hdc: HDC, w: i32, h: i32) {
    match &mut st.dw {
        None => match DwBackend::new(hdc, w, h, st.dpi) {
            Ok(b) => st.dw = Some(b),
            Err(e) => {
                // interop 초기화 실패 — GDI로 폴백(스파이크 한정 처리)
                eprintln!("DirectWrite 초기화 실패, GDI 폴백: {e}");
                st.backend = BackendKind::Gdi;
            }
        },
        Some(b) => {
            if b.size() != (w, h) {
                let _ = b.resize(w, h);
            }
        }
    }
}

/// 타이틀바에 백엔드·평균 페인트 시간 표시(비교 실측 가시화).
unsafe fn update_title(hwnd: HWND, st: &State, note: &str) {
    let text = format!(
        "Nexa Dir 2 — ADR-0002 [{}] 평균 {}µs/{}프레임{} (F2 전환·F3 벤치)\0",
        st.backend.label(),
        st.stats.avg_us(),
        st.stats.frames,
        note,
    );
    let wtext: Vec<u16> = text.encode_utf16().collect();
    let _ = SetWindowTextW(hwnd, PCWSTR(wtext.as_ptr()));
}

/// 위젯을 백버퍼에 그린 뒤 화면으로 BitBlt — 가시 영역만(M0-7 계승). 페인트 시간을 누적.
unsafe fn paint(hwnd: HWND, st: &mut State) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let rc = client_rect(hwnd);
    let t0 = Instant::now();

    let src_dc = match st.backend {
        BackendKind::Gdi => {
            ensure_backbuffer(st, hdc, rc.w, rc.h);
            let Some(back) = &st.back else {
                let _ = EndPaint(hwnd, &ps);
                return;
            };
            let dc = back.dc;
            let old_font = SelectObject(dc, st.font.into());
            let mut ctx = GdiCtx { dc };
            st.rows.paint(&mut ctx, &st.theme);
            SelectObject(dc, old_font);
            dc
        }
        BackendKind::DirectWrite => {
            ensure_dw(st, hdc, rc.w, rc.h);
            match &st.dw {
                Some(back) if st.backend == BackendKind::DirectWrite => {
                    let mut ctx = DwCtx { back };
                    st.rows.paint(&mut ctx, &st.theme);
                    back.memory_dc()
                }
                _ => {
                    // DW 폴백 직후 프레임 — GDI 경로로 재진입
                    let _ = EndPaint(hwnd, &ps);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    return;
                }
            }
        }
    };

    let _ = BitBlt(hdc, 0, 0, rc.w, rc.h, Some(src_dc), 0, 0, SRCCOPY);
    st.stats.add(t0.elapsed().as_micros() as u64);
    let _ = EndPaint(hwnd, &ps);

    if st.stats.frames.is_multiple_of(20) {
        update_title(hwnd, st, "");
    }
}

/// F3 — 200프레임 연속 스크롤 벤치(동기 UpdateWindow). 결과는 타이틀바.
unsafe fn bench(hwnd: HWND, st: &mut State) {
    route_event(hwnd, st, InputEvent::Key(Key::Home));
    let _ = UpdateWindow(hwnd);
    st.stats.reset();
    for _ in 0..BENCH_FRAMES {
        // 휠 1노치(3행)씩 아래로 — 실사용 스크롤 패턴
        if let Some(s) = state_of(hwnd) {
            route_event(hwnd, s, InputEvent::Wheel { delta: -120 });
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
        _ => None,
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let st = Box::new(State::new(GetDpiForWindow(hwnd)));
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(st) as isize);
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_PAINT => {
            if let Some(st) = state_of(hwnd) {
                paint(hwnd, st);
            }
            LRESULT(0)
        }
        // 더블 버퍼가 전체를 덮으므로 배경 지우기 생략(깜빡임 제거)
        WM_ERASEBKGND => LRESULT(1),
        WM_SIZE => {
            if let Some(st) = state_of(hwnd) {
                let mut inv = Invalidations::default();
                st.rows.set_bounds(client_rect(hwnd), &mut inv);
                flush_invalidations(hwnd, &mut inv);
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            if let Some(st) = state_of(hwnd) {
                let delta = (wparam.0 >> 16) as i16 as i32; // WHEEL_DELTA 단위(트랙패드는 분수 노치)
                route_event(hwnd, st, InputEvent::Wheel { delta });
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if let Some(st) = state_of(hwnd) {
                let vk = wparam.0 as u16;
                if vk == VK_F2.0 {
                    // 백엔드 전환 — 통계 리셋 후 전체 다시 그리기
                    st.backend = match st.backend {
                        BackendKind::Gdi => BackendKind::DirectWrite,
                        BackendKind::DirectWrite => BackendKind::Gdi,
                    };
                    st.stats.reset();
                    update_title(hwnd, st, " · 전환");
                    let _ = InvalidateRect(Some(hwnd), None, false);
                } else if vk == VK_F3.0 {
                    bench(hwnd, st);
                } else if let Some(key) = vk_to_key(vk) {
                    route_event(hwnd, st, InputEvent::Key(key));
                }
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            if let Some(st) = state_of(hwnd) {
                let dpi = (wparam.0 & 0xFFFF) as u32;
                st.dpi = dpi;
                let _ = DeleteObject(st.font.into());
                let (font, row_h) = make_font(dpi);
                st.font = font;
                if let Some(dw) = &mut st.dw {
                    let _ = dw.set_dpi(dpi);
                }
                let mut inv = Invalidations::default();
                st.rows.set_metrics(row_h, pad_x(dpi), &mut inv);
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
                let st = Box::from_raw(ptr);
                if let Some(b) = st.back {
                    b.free();
                }
                let _ = DeleteObject(st.font.into());
                // st.dw(COM 참조)는 drop으로 해제
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
