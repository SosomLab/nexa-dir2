//! tip — 도구 모음 툴팁 팝업(07-18 사용자 요청: "도구 모음에 tool-tip을
//! i18n 적용"). 커스텀 무캐션 팝업(WS_EX_TOOLWINDOW·NOACTIVATE·TOPMOST) —
//! 텍스트는 호출자가 [`crate::i18n::tr`]로 넘긴다(이 모듈은 표시만).
//!
//! 수명: [`show`]가 생성, [`hide`]가 파괴 — 호스트(win.rs)가 hover 추적
//! 타이머로 관리(창당 1개). 폰트는 여기서 생성·`WM_NCDESTROY`에서 해제.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, GetDC, ReleaseDC,
    SelectObject, SetBkMode, SetTextColor, DT_CALCRECT, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE,
    DT_VCENTER, FillRect, FrameRect, HFONT, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetWindowLongPtrW, IsWindow,
    RegisterClassW, SetWindowLongPtrW, ShowWindow, GWLP_USERDATA, SW_SHOWNOACTIVATE, WINDOW_EX_STYLE,
    WM_NCDESTROY, WM_PAINT, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use crate::dialog::DlgFont;

const CLASS: PCWSTR = w!("NexaTip");
static REGISTER: std::sync::Once = std::sync::Once::new();

/// 좌우/상하 안쪽 여백(px @96dpi 기준 — 창 크기 산정과 페인트 공용).
const PAD_X: i32 = 8;
const PAD_Y: i32 = 4;

/// 툴팁 색(호출자가 테마에서 변환 — [`crate::dw::colorref`]).
#[derive(Clone, Copy)]
pub struct TipStyle {
    pub fg: COLORREF,
    pub bg: COLORREF,
    pub border: COLORREF,
}

struct TipState {
    text: Vec<u16>,
    font: HFONT,
    style: TipStyle,
}

/// 툴팁 표시 — `(sx, sy)` = **화면 좌표** 좌상단 기준(호출자가 버튼 하단 계산).
/// 빈 텍스트는 표시하지 않음(`None`). 반환 창은 [`hide`]로 파괴.
pub unsafe fn show(
    owner: HWND,
    sx: i32,
    sy: i32,
    text: &str,
    font_spec: &DlgFont,
    style: TipStyle,
) -> Option<HWND> {
    if text.is_empty() {
        return None; // 빈 Vec→DrawTextW = AV(원장) — 진입 차단
    }
    REGISTER.call_once(|| {
        let wc = WNDCLASSW {
            lpszClassName: CLASS,
            lpfnWndProc: Some(tip_proc),
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);
    });
    let font = crate::dialog::make_font_pub(owner, font_spec);
    // 측정(단일 행)
    let hdc = GetDC(None);
    let old = SelectObject(hdc, font.into());
    let mut buf: Vec<u16> = text.encode_utf16().collect();
    let mut rc = RECT::default();
    DrawTextW(hdc, &mut buf, &mut rc, DT_CALCRECT | DT_LEFT | DT_NOPREFIX);
    SelectObject(hdc, old);
    ReleaseDC(None, hdc);
    let (w, h) = (
        (rc.right - rc.left) + PAD_X * 2,
        (rc.bottom - rc.top) + PAD_Y * 2,
    );
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE(WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0 | WS_EX_TOPMOST.0),
        CLASS,
        w!(""),
        WS_POPUP,
        sx.max(0),
        sy,
        w,
        h,
        Some(owner),
        None,
        None,
        None,
    )
    .ok()?;
    let st = Box::new(TipState {
        text: buf,
        font,
        style,
    });
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(st) as isize);
    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    Some(hwnd)
}

/// 툴팁 파괴(널/이미 파괴 안전).
pub unsafe fn hide(hwnd: &mut Option<HWND>) {
    if let Some(h) = hwnd.take() {
        if IsWindow(Some(h)).as_bool() {
            let _ = DestroyWindow(h);
        }
    }
}

unsafe extern "system" fn tip_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let st = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TipState;
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            if let Some(st) = st.as_mut() {
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                let bg = CreateSolidBrush(st.style.bg);
                FillRect(hdc, &rc, bg);
                let _ = DeleteObject(bg.into());
                let bd = CreateSolidBrush(st.style.border);
                FrameRect(hdc, &rc, bd);
                let _ = DeleteObject(bd.into());
                if !st.text.is_empty() {
                    let old = SelectObject(hdc, st.font.into());
                    SetBkMode(hdc, TRANSPARENT);
                    SetTextColor(hdc, st.style.fg);
                    let mut trc = RECT {
                        left: rc.left + PAD_X,
                        top: rc.top,
                        right: rc.right - PAD_X,
                        bottom: rc.bottom,
                    };
                    DrawTextW(
                        hdc,
                        &mut st.text,
                        &mut trc,
                        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
                    );
                    SelectObject(hdc, old);
                }
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_NCDESTROY => {
            if !st.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                let st = Box::from_raw(st);
                let _ = DeleteObject(st.font.into());
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        // 클릭 투과 방지 자체가 불필요(NOACTIVATE·마우스 이탈 시 호스트가 파괴)
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
