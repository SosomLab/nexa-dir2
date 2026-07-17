//! button — **NxButton** 푸시 버튼(ctl 11호 — macOS 시안 07-17: Cancel = 기본
//! 연회색 라운드·Rename = **Default 파랑**. 라이브러리 추상화 — 앱 비결합).
//!
//! ## 계약(판매용 명세 — 클래스 `Nexa.NxButton`)
//! - 생성: [`create`] — 라벨(복사 소유)·[`ButtonKind`]·[`Style`].
//!   `h <= 0` = **글꼴 + 상/하 2px**(컴팩트 — 사용자 확정: 공통 auto보다 낮음) ·
//!   `w <= 0` = 라벨 폭 + 좌우 여백(자동).
//! - 상태 3종(사용자 확정):
//!   기본 = sel_bg 라운드 필 + text 글자 ·
//!   **Default** = accent 필 + bg 글자(대화상자 기본 동작 버튼) ·
//!   **Disabled** = 연한 필 + text_dim 글자·클릭 무시.
//! - 클릭(Space/Enter 포함, enabled일 때만) → 부모에
//!   `WM_COMMAND(MAKEWPARAM(id, NXBTN_CLICK))`(lparam = 컨트롤).
//! - 조회/설정: [`NXBTN_GETENABLE`]/[`NXBTN_SETENABLE`](WM_USER+105/106) ·
//!   [`NXBTN_SETDEFAULT`](WM_USER+107 — wparam 0/1) · 라벨 = 윈도우 텍스트
//!   표준 위임(WM_SETTEXT 재도장).
//! - 도형 = AA(DrawCtx 백엔드 — GDI+ 직접 호출 금지) · 모서리 = `style.behind`.

use nexa_gui::DrawCtx;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, DrawTextW, EndPaint, InvalidateRect, SelectObject, SetBkMode, SetTextColor,
    DT_CENTER, DT_SINGLELINE, DT_VCENTER, HFONT, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetClientRect, GetDlgCtrlID, GetParent, GetWindowLongPtrW,
    GetWindowTextW, RegisterClassW, SendMessageW, SetWindowLongPtrW, GWLP_USERDATA, HMENU,
    IDC_ARROW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_PAINT, WM_SETFONT, WM_SETTEXT, WNDCLASSW, WS_CHILD, WS_TABSTOP, WS_VISIBLE,
};

use super::gdipctx::{color, rect as gc_rect, GdipCtx};
use super::style::{fill, Style};

/// 클릭 통지(WM_COMMAND HIWORD — enabled일 때만).
pub const NXBTN_CLICK: u32 = 1;
/// 활성 상태 조회(반환 = 0/1).
pub const NXBTN_GETENABLE: u32 = 0x0400 + 105;
/// 활성 상태 설정(wparam = 0/1 — 재도장).
pub const NXBTN_SETENABLE: u32 = 0x0400 + 106;
/// Default(기본 동작) 설정(wparam = 0/1 — 회색 ↔ accent 전환·재도장).
pub const NXBTN_SETDEFAULT: u32 = 0x0400 + 107;

/// 버튼 종류(사용자 확정 — Default 여부로 색 전환).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ButtonKind {
    /// 기본(연회색 필 + text 글자 — 시안 Cancel).
    #[default]
    Normal,
    /// 대화상자 기본 동작(accent 필 + bg 글자 — 시안 Rename).
    Default,
}

/// 라운드 반경(px — 시안 필 형태).
const RADIUS: i32 = 6;
/// 좌우 내부 여백(px — `w <= 0` 자동 폭 계산).
const PAD_X: i32 = 16;

struct BtnState {
    kind: ButtonKind,
    enabled: bool,
    font: HFONT,
    style: Style,
}

static REGISTER: std::sync::Once = std::sync::Once::new();
const CLASS: PCWSTR = w!("Nexa.NxButton");

/// NxButton 생성 — 라벨은 윈도우 텍스트로 위임(복사 소유). `w/h <= 0` = 자동.
#[allow(clippy::too_many_arguments)] // Win32 create 계열 관례
pub unsafe fn create(
    parent: HWND,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    id: u32,
    font: HFONT,
    label: &str,
    kind: ButtonKind,
    enabled: bool,
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
    // 기본 높이 = 글꼴 + 상/하 2px(사용자 확정 07-17 — 공통 auto_height보다
    // 컴팩트: 버튼은 텍스트에 딱 맞는 시안 비율)
    let h = if h <= 0 {
        super::style::font_height(parent, font) + 4
    } else {
        h
    };
    // w<=0 = 라벨 폭 + 좌우 여백(GDI 측정 — 텍스트는 GDI 소관)
    let w = if w <= 0 {
        label_width(parent, font, label) + PAD_X * 2
    } else {
        w
    };
    let t16 = windows::core::HSTRING::from(label);
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        CLASS,
        PCWSTR(t16.as_ptr()),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(WS_TABSTOP.0),
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
    let st = Box::new(BtnState {
        kind,
        enabled,
        font,
        style,
    });
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(st) as isize);
    SendMessageW(
        hwnd,
        WM_SETFONT,
        Some(WPARAM(font.0 as usize)),
        Some(LPARAM(1)),
    );
    hwnd
}

/// 라벨 렌더 폭(px — GDI GetTextExtentPoint32W).
unsafe fn label_width(hwnd: HWND, font: HFONT, label: &str) -> i32 {
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::{GetDC, GetTextExtentPoint32W, ReleaseDC, SelectObject};
    let dc = GetDC(Some(hwnd));
    let old = SelectObject(dc, font.into());
    let w16: Vec<u16> = label.encode_utf16().collect();
    let mut sz = SIZE::default();
    let _ = GetTextExtentPoint32W(dc, &w16, &mut sz);
    SelectObject(dc, old);
    ReleaseDC(Some(hwnd), dc);
    sz.cx
}

unsafe fn state(hwnd: HWND) -> *mut BtnState {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut BtnState
}

unsafe fn click(hwnd: HWND) {
    if let Ok(parent) = GetParent(hwnd) {
        let id = GetDlgCtrlID(hwnd) as u32;
        SendMessageW(
            parent,
            WM_COMMAND,
            Some(WPARAM(((NXBTN_CLICK as usize) << 16) | id as usize)),
            Some(LPARAM(hwnd.0 as isize)),
        );
    }
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
        m if m == NXBTN_GETENABLE => {
            LRESULT(state(hwnd).as_ref().map_or(0, |s| s.enabled as isize))
        }
        m if m == NXBTN_SETENABLE => {
            if let Some(st) = state(hwnd).as_mut() {
                st.enabled = wparam.0 != 0;
                let _ = InvalidateRect(Some(hwnd), None, true);
            }
            LRESULT(0)
        }
        m if m == NXBTN_SETDEFAULT => {
            if let Some(st) = state(hwnd).as_mut() {
                st.kind = if wparam.0 != 0 {
                    ButtonKind::Default
                } else {
                    ButtonKind::Normal
                };
                let _ = InvalidateRect(Some(hwnd), None, true);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            if let Some(st) = state(hwnd).as_ref() {
                if st.enabled {
                    let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(hwnd));
                    click(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN if matches!(wparam.0 as u32, 0x20 | 0x0D) => {
            // Space/Enter = 클릭(표준 버튼 키 계약)
            if let Some(st) = state(hwnd).as_ref() {
                if st.enabled {
                    click(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            if let Some(st) = state(hwnd).as_ref() {
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                // 모서리 = behind(부모 배경) → AA 라운드 필 블렌드
                fill(dc, &rc, st.style.behind);
                // 필/글자 색: 기본 = sel_bg/text · Default = accent/bg ·
                // Disabled = sel_bg/text_dim(사용자 확정 3상태)
                let (fill_c, text_c) = if !st.enabled {
                    (st.style.sel_bg, st.style.text_dim)
                } else if st.kind == ButtonKind::Default {
                    (st.style.accent, st.style.bg)
                } else {
                    (st.style.sel_bg, st.style.text)
                };
                {
                    let mut g = GdipCtx::new(dc);
                    g.fill_round_rect(gc_rect(&rc), RADIUS, color(fill_c));
                } // GDI 텍스트 전에 Graphics 해제(HDC 혼용 규약)
                let mut buf = [0u16; 256];
                let n = GetWindowTextW(hwnd, &mut buf) as usize;
                if n > 0 {
                    let old = SelectObject(dc, st.font.into());
                    SetBkMode(dc, TRANSPARENT);
                    SetTextColor(dc, text_c);
                    // 세로 중앙 + 1px 하향(콤보/글상자와 동일 보정)
                    let mut trc = RECT {
                        top: rc.top + 1,
                        bottom: rc.bottom + 1,
                        ..rc
                    };
                    DrawTextW(
                        dc,
                        &mut buf[..n],
                        &mut trc,
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                    );
                    SelectObject(dc, old);
                }
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
