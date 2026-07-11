//! Win32 창·메시지 루프(M0-5) + GDI 렌더 스파이크(M0-7).
//! 렌더 모델(docs/01 §3): 창 1개 · `WM_PAINT` 더블 버퍼(메모리 DC) · **가시 행만** 그리기.
//! 합성 10만 행 + 세로 스크롤(휠·키보드)로 M1 가상 리스트의 전제(행 = rect × draw)를 검증한다.
//! 텍스트는 1차 GDI `ExtTextOutW` — DirectWrite interop 전환은 ADR-0002(M1)에서 비교 확정.

use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::ExtTextOutW;
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreateSolidBrush,
    DeleteDC, DeleteObject, EndPaint, FillRect, InvalidateRect, SelectObject, SetBkColor,
    SetTextColor, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH,
    ETO_OPAQUE, FF_DONTCARE, FW_NORMAL, HBITMAP, HBRUSH, HDC, HFONT, HGDIOBJ, OUT_DEFAULT_PRECIS,
    PAINTSTRUCT, SRCCOPY,
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
    SetWindowPos, TranslateMessage, CREATESTRUCTW, CW_USEDEFAULT, GWLP_USERDATA, IDC_ARROW, MSG,
    SWP_NOACTIVATE, SWP_NOZORDER, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_KEYDOWN,
    WM_MOUSEWHEEL, WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_SIZE, WNDCLASSW, WS_OVERLAPPEDWINDOW,
    WS_VISIBLE,
};

/// 스파이크 데이터셋 — 원본 게이트(100k 첫 렌더 <150ms, TODO M1-9)와 동일 체급.
const TOTAL_ROWS: usize = 100_000;
/// 휠 1노치(WHEEL_DELTA=120)당 스크롤 행 수.
const WHEEL_LINES: i32 = 3;

/// 다크 팔레트(디자인 규약 DR-5: 고밀도·다크) — M2 테마 시스템 전까지 하드코딩.
const BG: COLORREF = COLORREF(0x001E1E1E);
const ROW_ALT: COLORREF = COLORREF(0x00242424);
const TEXT: COLORREF = COLORREF(0x00D4D4D4);

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
    scroll_row: usize,
    wheel_accum: i32,
    row_h: i32,
    font: HFONT,
    bg_brush: HBRUSH,
    back: Option<BackBuffer>,
}

impl State {
    unsafe fn new(dpi: u32) -> Self {
        let (font, row_h) = make_font(dpi);
        State {
            scroll_row: 0,
            wheel_accum: 0,
            row_h,
            font,
            bg_brush: CreateSolidBrush(BG),
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
            w!("Nexa Dir 2 — M0-7 렌더 스파이크 (100k 행)"),
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

unsafe fn client_size(hwnd: HWND) -> (i32, i32) {
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    (rc.right - rc.left, rc.bottom - rc.top)
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

/// 현재 클라이언트 높이에서 그릴 수 있는 행 수(부분 행 포함).
fn visible_rows(client_h: i32, row_h: i32) -> usize {
    ((client_h + row_h - 1) / row_h).max(0) as usize
}

fn max_scroll(client_h: i32, row_h: i32) -> usize {
    let full = (client_h / row_h).max(0) as usize; // 완전 가시 행 수
    TOTAL_ROWS.saturating_sub(full)
}

unsafe fn scroll_to(hwnd: HWND, st: &mut State, target: isize) {
    let (_, h) = client_size(hwnd);
    let clamped = target.clamp(0, max_scroll(h, st.row_h) as isize) as usize;
    if clamped != st.scroll_row {
        st.scroll_row = clamped;
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

/// 가시 행만 백버퍼에 그린 뒤 화면으로 BitBlt — 프레임당 작업량은 창 높이에만 비례.
unsafe fn paint(hwnd: HWND, st: &mut State) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let (w, h) = client_size(hwnd);
    ensure_backbuffer(st, hdc, w, h);
    let Some(back) = &st.back else {
        let _ = EndPaint(hwnd, &ps);
        return;
    };
    let dc = back.dc;

    let old_font = SelectObject(dc, st.font.into());
    SetTextColor(dc, TEXT);

    let first = st.scroll_row;
    let count = visible_rows(h, st.row_h).min(TOTAL_ROWS - first);
    let pad_x = (12 * GetDpiForWindow(hwnd) as i32) / 96;
    for i in 0..count {
        let row = first + i;
        let y = i as i32 * st.row_h;
        let rc = RECT {
            left: 0,
            top: y,
            right: w,
            bottom: y + st.row_h,
        };
        // ETO_OPAQUE로 행 배경+텍스트를 한 호출에 — 행 단위 FillRect 생략
        SetBkColor(dc, if row.is_multiple_of(2) { BG } else { ROW_ALT });
        let text: Vec<u16> =
            format!("{row:>6}  spike-entry-{row:06}.txt — 가시 영역만 그리는 더블 버퍼 스파이크")
                .encode_utf16()
                .collect();
        let _ = ExtTextOutW(
            dc,
            pad_x,
            y + (st.row_h - (st.row_h * 4) / 5) / 2,
            ETO_OPAQUE,
            Some(&rc),
            PCWSTR(text.as_ptr()),
            text.len() as u32,
            None,
        );
    }
    // 마지막 행 아래 잔여 영역
    let drawn_h = count as i32 * st.row_h;
    if drawn_h < h {
        let rest = RECT {
            left: 0,
            top: drawn_h,
            right: w,
            bottom: h,
        };
        FillRect(dc, &rest, st.bg_brush);
    }

    let _ = BitBlt(hdc, 0, 0, w, h, Some(dc), 0, 0, SRCCOPY);
    SelectObject(dc, old_font);
    let _ = EndPaint(hwnd, &ps);
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            // lpCreateParams 대신 창 생성 직후 DPI로 상태 구성(창 1개 전제)
            let _ = lparam.0 as *const CREATESTRUCTW;
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
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            if let Some(st) = state_of(hwnd) {
                let delta = (wparam.0 >> 16) as i16 as i32; // WHEEL_DELTA 단위(트랙패드는 분수 노치)
                st.wheel_accum += delta;
                let lines = st.wheel_accum * WHEEL_LINES / 120;
                if lines != 0 {
                    st.wheel_accum -= lines * 120 / WHEEL_LINES;
                    scroll_to(hwnd, st, st.scroll_row as isize - lines as isize);
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if let Some(st) = state_of(hwnd) {
                let (_, h) = client_size(hwnd);
                let page = (h / st.row_h).max(1) as isize;
                let cur = st.scroll_row as isize;
                match wparam.0 as u16 {
                    k if k == VK_UP.0 => scroll_to(hwnd, st, cur - 1),
                    k if k == VK_DOWN.0 => scroll_to(hwnd, st, cur + 1),
                    k if k == VK_PRIOR.0 => scroll_to(hwnd, st, cur - page),
                    k if k == VK_NEXT.0 => scroll_to(hwnd, st, cur + page),
                    k if k == VK_HOME.0 => scroll_to(hwnd, st, 0),
                    k if k == VK_END.0 => scroll_to(hwnd, st, isize::MAX / 2),
                    _ => {}
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
                st.row_h = row_h;
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
                let _ = InvalidateRect(Some(hwnd), None, false);
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
                let _ = DeleteObject(st.bg_brush.into());
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
