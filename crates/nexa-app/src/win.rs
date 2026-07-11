//! Win32 창·메시지 루프(M0-5) — M1-1부터 렌더·입력은 nexa-gui 위젯으로 위임.
//! 이 모듈의 책임: 창 수명·더블 버퍼(메모리 DC)·WM_* → [`nexa_gui::InputEvent`] 번역·
//! [`nexa_gui::Invalidations`] → `InvalidateRect` 번역. 렌더 모델(docs/01 §3)은 M0-7 계승.

use nexa_gui::widgets::{RowSource, VirtualRows};
use nexa_gui::{InputEvent, Invalidations, Key, Rect as GRect, Theme, Widget};
use windows::core::{w, Result};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
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
    VK_DOWN, VK_END, VK_HOME, VK_NEXT, VK_PRIOR, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetMessageW,
    GetWindowLongPtrW, LoadCursorW, PostQuitMessage, RegisterClassW, SetWindowLongPtrW,
    SetWindowPos, TranslateMessage, CW_USEDEFAULT, GWLP_USERDATA, IDC_ARROW, MSG, SWP_NOACTIVATE,
    SWP_NOZORDER, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_KEYDOWN, WM_MOUSEWHEEL, WM_NCCREATE,
    WM_NCDESTROY, WM_PAINT, WM_SIZE, WNDCLASSW, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

use crate::gdi::GdiCtx;

/// 스파이크 데이터셋 — M1-3에서 nexa-tree 평면 스트림으로 대체.
const TOTAL_ROWS: usize = 100_000;

struct SpikeRows;

impl RowSource for SpikeRows {
    fn len(&self) -> usize {
        TOTAL_ROWS
    }
    fn row_text(&self, row: usize) -> String {
        format!("{row:>6}  spike-entry-{row:06}.txt — nexa-gui VirtualRows 위젯 배선(M1-1)")
    }
}

/// 클라이언트 크기와 1:1인 오프스크린 버퍼. 크기 변경 시 재생성(ensure_backbuffer).
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

/// 창 단위 상태 — `GWLP_USERDATA`에 Box raw 포인터로 보관(WM_NCCREATE~WM_NCDESTROY).
struct State {
    rows: VirtualRows<SpikeRows>,
    theme: Theme,
    font: HFONT,
    back: Option<BackBuffer>,
}

impl State {
    unsafe fn new(dpi: u32) -> Self {
        let (font, row_h) = make_font(dpi);
        State {
            rows: VirtualRows::new(SpikeRows, row_h, pad_x(dpi)),
            theme: Theme::default(), // 다크(DR-5) — 모드 선택은 M2 테마 시스템
            font,
            back: None,
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
            w!("Nexa Dir 2 — M1-1 nexa-gui 배선 (100k 행)"),
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

/// 위젯을 백버퍼에 그린 뒤 화면으로 BitBlt — 가시 영역만(M0-7 계승).
unsafe fn paint(hwnd: HWND, st: &mut State) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let rc = client_rect(hwnd);
    ensure_backbuffer(st, hdc, rc.w, rc.h);
    let Some(back) = &st.back else {
        let _ = EndPaint(hwnd, &ps);
        return;
    };
    let dc = back.dc;

    let old_font = SelectObject(dc, st.font.into());
    let mut ctx = GdiCtx { dc };
    st.rows.paint(&mut ctx, &st.theme);

    let _ = BitBlt(hdc, 0, 0, rc.w, rc.h, Some(dc), 0, 0, SRCCOPY);
    SelectObject(dc, old_font);
    let _ = EndPaint(hwnd, &ps);
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
                if let Some(key) = vk_to_key(wparam.0 as u16) {
                    route_event(hwnd, st, InputEvent::Key(key));
                }
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            if let Some(st) = state_of(hwnd) {
                let dpi = (wparam.0 & 0xFFFF) as u32;
                let _ = DeleteObject(st.font.into());
                let (font, row_h) = make_font(dpi);
                st.font = font;
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
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
