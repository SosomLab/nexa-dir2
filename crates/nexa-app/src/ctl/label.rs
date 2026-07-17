//! label — **NxLabel** 폼 라벨(ctl 12호 — PF 카드 폼의 "Apply to:" 좌측 라벨.
//! 사용자 확정 07-17: "높이 등의 일관성을 위해 라벨 컨트롤도 별도로".
//! 라이브러리 추상화 — 앱 비결합).
//!
//! ## 계약(판매용 명세 — 클래스 `Nexa.NxLabel`)
//! - 생성: [`create`] — 텍스트(윈도우 텍스트 위임 — WM_SETTEXT 재도장)·정렬·
//!   [`Style`]. **높이 규칙(공통)**: `h <= 0` = auto_height — 같은 row의 콤보/
//!   글상자와 세로 중심이 자동 일치(1px 하향 보정 포함).
//! - 배경 = `style.behind`(부모 배경색 — 카드 위 = 카드 bg)·글자 = `style.text`.
//! - 통지 없음(순수 표시) · 마우스 투명(HTTRANSPARENT — 클릭은 부모로).

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, DrawTextW, EndPaint, InvalidateRect, SelectObject, SetBkMode, SetTextColor,
    DT_LEFT, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, HFONT, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetClientRect, GetWindowLongPtrW, GetWindowTextW,
    RegisterClassW, SendMessageW, SetWindowLongPtrW, GWLP_USERDATA, HMENU, IDC_ARROW,
    WINDOW_EX_STYLE, WM_CREATE, WM_DESTROY, WM_NCHITTEST, WM_PAINT, WM_SETFONT, WM_SETTEXT,
    WNDCLASSW, WS_CHILD, WS_VISIBLE,
};

use super::style::{fill, Style};

/// 텍스트 정렬(폼 라벨 = 보통 Right — 콜론이 컨트롤에 붙는 PF 구도).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LabelAlign {
    Left,
    #[default]
    Right,
}

struct LbState {
    align: LabelAlign,
    font: HFONT,
    style: Style,
}

static REGISTER: std::sync::Once = std::sync::Once::new();
const CLASS: PCWSTR = w!("Nexa.NxLabel");

/// NxLabel 생성 — 텍스트는 윈도우 텍스트로 위임(복사 소유). `h <= 0` = 자동.
#[allow(clippy::too_many_arguments)] // Win32 create 계열 관례
pub unsafe fn create(
    parent: HWND,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    id: u32,
    font: HFONT,
    text: &str,
    align: LabelAlign,
    style: Style,
) -> HWND {
    REGISTER.call_once(|| {
        let wc = WNDCLASSW {
            lpfnWndProc: Some(proc),
            lpszClassName: CLASS,
            hCursor: windows::Win32::UI::WindowsAndMessaging::LoadCursorW(None, IDC_ARROW)
                .unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassW(&wc);
    });
    let h = if h <= 0 {
        super::style::auto_height(parent, font)
    } else {
        h
    };
    let t16 = windows::core::HSTRING::from(text);
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        CLASS,
        PCWSTR(t16.as_ptr()),
        WS_CHILD | WS_VISIBLE,
        x,
        y,
        w,
        h,
        Some(parent),
        Some(HMENU(id as usize as *mut core::ffi::c_void)),
        None,
        None,
    )
    .unwrap_or_default();
    let st = Box::new(LbState { align, font, style });
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(st) as isize);
    SendMessageW(
        hwnd,
        WM_SETFONT,
        Some(WPARAM(font.0 as usize)),
        Some(LPARAM(1)),
    );
    hwnd
}

unsafe fn state(hwnd: HWND) -> *mut LbState {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut LbState
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => LRESULT(0),
        WM_DESTROY => {
            let p = state(hwnd);
            if !p.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(p));
            }
            LRESULT(0)
        }
        WM_SETFONT => {
            if let Some(st) = state(hwnd).as_mut() {
                st.font = HFONT(wparam.0 as *mut core::ffi::c_void);
            }
            let _ = InvalidateRect(Some(hwnd), None, true);
            LRESULT(0)
        }
        WM_SETTEXT => {
            let r = DefWindowProcW(hwnd, msg, wparam, lparam);
            let _ = InvalidateRect(Some(hwnd), None, true);
            r
        }
        WM_NCHITTEST => LRESULT(-1 /* HTTRANSPARENT — 클릭 투과 */),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            if let Some(st) = state(hwnd).as_ref() {
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                fill(dc, &rc, st.style.behind);
                let mut buf = [0u16; 256];
                let n = GetWindowTextW(hwnd, &mut buf) as usize;
                if n > 0 {
                    let old = SelectObject(dc, st.font.into());
                    SetBkMode(dc, TRANSPARENT);
                    SetTextColor(dc, st.style.text);
                    // 세로 중앙 + 1px 하향(콤보/글상자와 동일 보정 — 행 정렬)
                    let mut trc = RECT {
                        top: rc.top + 1,
                        bottom: rc.bottom + 1,
                        ..rc
                    };
                    let al = if st.align == LabelAlign::Right {
                        DT_RIGHT
                    } else {
                        DT_LEFT
                    };
                    DrawTextW(dc, &mut buf[..n], &mut trc, al | DT_VCENTER | DT_SINGLELINE);
                    SelectObject(dc, old);
                }
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
