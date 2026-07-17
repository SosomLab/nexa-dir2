//! textbox — **NxTextBox** 텍스트 입력(ctl 9호 — macOS 시안 07-17: 라운드
//! 테두리, 포커스 시 **accent 두꺼운 링**. 라이브러리 추상화 — 앱 비결합).
//!
//! ## 계약(판매용 명세 — 클래스 `Nexa.NxTextBox`)
//! - 생성: [`create`] — [`Style`] 인자. **높이 규칙(공통)**: `h <= 0` =
//!   [`super::style::auto_height`](글꼴 + 상/하 [`super::style::PAD_Y`]px) —
//!   전 Nx 컨트롤과 동일해 수정 없이 같은 row 배치가 반듯하다.
//! - 본체 = **AA 라운드 사각**(모서리 = `style.behind` 블렌드 — DrawCtx 백엔드
//!   경유, 07-17 규약) + 평시 1px border · **포커스 = accent 2px 링**.
//!   입력은 내부 무테두리 EDIT(글꼴 높이로 **세로 중앙** — searchbox 검증 규약).
//! - 텍스트 API 위임: `WM_SETTEXT`/`WM_GETTEXT`/`WM_GETTEXTLENGTH`/
//!   `EM_SETCUEBANNER` → 내부 EDIT. 내용 변경 = 부모에
//!   `WM_COMMAND(MAKEWPARAM(id, EN_CHANGE))` 재발행(포커스 in/out도 재발행).

use nexa_gui::DrawCtx;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, DeleteObject, EndPaint, InvalidateRect, HFONT, PAINTSTRUCT,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetFocus, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetClientRect, GetDlgCtrlID, GetParent, GetWindowLongPtrW,
    MoveWindow, RegisterClassW, SendMessageW, SetWindowLongPtrW, ES_AUTOHSCROLL, GWLP_USERDATA,
    HMENU, IDC_ARROW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_GETTEXT,
    WM_GETTEXTLENGTH, WM_LBUTTONDOWN, WM_PAINT, WM_SETFOCUS, WM_SETFONT, WM_SETTEXT, WM_SIZE,
    WNDCLASSW, WS_CHILD, WS_TABSTOP, WS_VISIBLE,
};

use super::gdipctx::{color, rect as gc_rect, GdipCtx};
use super::style::{fill, font_height, Style};

/// 내용 변경 재발행 코드(EDIT EN_CHANGE 그대로).
pub const EN_CHANGE: u32 = 0x0300;
const EN_SETFOCUS: u32 = 0x0100;
const EN_KILLFOCUS: u32 = 0x0200;
const EM_SETCUEBANNER: u32 = 0x1501;

/// 라운드 반경(px — 콤보와 동일 시안).
const RADIUS: i32 = 6;
/// 좌우 내부 여백(px).
const PAD_X: i32 = 8;
/// 내부 EDIT id(재발행 판별용 — 외부 비노출).
const EDIT_ID: u32 = 1;

struct TbState {
    edit: HWND,
    font: HFONT,
    style: Style,
    /// WM_CTLCOLOREDIT 응답용(1회 생성 — 메시지마다 생성 시 GDI 누수).
    bg_brush: windows::Win32::Graphics::Gdi::HBRUSH,
}

static REGISTER: std::sync::Once = std::sync::Once::new();
const CLASS: PCWSTR = w!("Nexa.NxTextBox");

/// NxTextBox 생성 — 반환 HWND에 텍스트 API를 그대로 쓸 수 있다(내부 위임).
#[allow(clippy::too_many_arguments)] // Win32 create 계열 관례
pub unsafe fn create(
    parent: HWND,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    id: u32,
    font: HFONT,
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
    // h<=0 = 공통 자동 높이(전 Nx 컨트롤 동일 — 반듯한 기본 배치, 07-17)
    let h = if h <= 0 {
        super::style::auto_height(parent, font)
    } else {
        h
    };
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        CLASS,
        w!(""),
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
    // 내부 무테두리 EDIT — 글꼴 높이로 세로 중앙(searchbox 규약)
    let edit = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("EDIT"),
        w!(""),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(WS_TABSTOP.0 | ES_AUTOHSCROLL as u32),
        0,
        0,
        0,
        0,
        Some(hwnd),
        Some(HMENU(EDIT_ID as usize as *mut core::ffi::c_void)),
        None,
        None,
    )
    .unwrap_or_default();
    let st = Box::new(TbState {
        edit,
        font,
        style,
        bg_brush: windows::Win32::Graphics::Gdi::CreateSolidBrush(style.bg),
    });
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(st) as isize);
    // 라운드 모서리 = behind 칠 + AA 도형(07-17 개정 — 리전 클립 폐기)
    SendMessageW(
        hwnd,
        WM_SETFONT,
        Some(WPARAM(font.0 as usize)),
        Some(LPARAM(1)),
    );
    layout(hwnd);
    hwnd
}

unsafe fn state(hwnd: HWND) -> *mut TbState {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TbState
}

/// 내부 EDIT 재배치 — 글꼴 높이(+2)로 세로 중앙(상/하 균등 여백).
unsafe fn layout(hwnd: HWND) {
    let Some(st) = state(hwnd).as_ref() else {
        return;
    };
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let eh = (font_height(hwnd, st.font) + 2).min(rc.bottom - rc.top - 4);
    let ey = rc.top + ((rc.bottom - rc.top) - eh) / 2;
    let _ = MoveWindow(
        st.edit,
        rc.left + PAD_X,
        ey,
        (rc.right - rc.left) - PAD_X * 2,
        eh,
        true,
    );
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => LRESULT(0),
        WM_DESTROY => {
            let p = state(hwnd);
            if !p.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                let st = Box::from_raw(p);
                let _ = DeleteObject(st.bg_brush.into());
            }
            LRESULT(0)
        }
        WM_SETFONT => {
            if let Some(st) = state(hwnd).as_mut() {
                st.font = HFONT(wparam.0 as *mut core::ffi::c_void);
                SendMessageW(st.edit, WM_SETFONT, Some(wparam), Some(lparam));
            }
            layout(hwnd);
            let _ = InvalidateRect(Some(hwnd), None, true);
            LRESULT(0)
        }
        WM_SIZE => {
            layout(hwnd);
            let _ = InvalidateRect(Some(hwnd), None, true);
            LRESULT(0)
        }
        // 텍스트 API 위임(드롭인 계약 — searchbox 규약)
        m if m == WM_SETTEXT || m == WM_GETTEXT || m == WM_GETTEXTLENGTH || m == EM_SETCUEBANNER => {
            match state(hwnd).as_ref() {
                Some(st) => SendMessageW(st.edit, m, Some(wparam), Some(lparam)),
                None => DefWindowProcW(hwnd, msg, wparam, lparam),
            }
        }
        WM_SETFOCUS | WM_LBUTTONDOWN => {
            if let Some(st) = state(hwnd).as_ref() {
                let _ = SetFocus(Some(st.edit)); // 착지 = 내부 에디트
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            // 내부 EDIT 통지 → 포커스 링 재도장 + 컨트롤 id로 부모에 재발행
            let src = (wparam.0 & 0xFFFF) as u32;
            let code = ((wparam.0 >> 16) & 0xFFFF) as u32;
            if src == EDIT_ID {
                if code == EN_SETFOCUS || code == EN_KILLFOCUS {
                    let _ = InvalidateRect(Some(hwnd), None, true);
                }
                if matches!(code, EN_CHANGE | EN_SETFOCUS | EN_KILLFOCUS) {
                    if let Ok(parent) = GetParent(hwnd) {
                        let id = GetDlgCtrlID(hwnd) as u32;
                        SendMessageW(
                            parent,
                            WM_COMMAND,
                            Some(WPARAM(((code as usize) << 16) | id as usize)),
                            Some(LPARAM(hwnd.0 as isize)),
                        );
                    }
                }
            }
            LRESULT(0)
        }
        0x0133 /* WM_CTLCOLOREDIT */ => {
            // 내부 EDIT 배경 = style.bg(본체와 일치 — 브러시는 상태 보관 1회)
            if let Some(st) = state(hwnd).as_ref() {
                let dc = windows::Win32::Graphics::Gdi::HDC(wparam.0 as *mut core::ffi::c_void);
                windows::Win32::Graphics::Gdi::SetBkColor(dc, st.style.bg);
                windows::Win32::Graphics::Gdi::SetTextColor(dc, st.style.text);
                return LRESULT(st.bg_brush.0 as isize);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            if let Some(st) = state(hwnd).as_ref() {
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                // 모서리 = behind(부모 배경) → AA 라운드 본체 + 테두리 블렌드
                fill(dc, &rc, st.style.behind);
                let focused = GetFocus() == st.edit;
                let (ring, width) = if focused {
                    (st.style.accent, 2.0)
                } else {
                    (st.style.border, 1.0)
                };
                // AA 도형 = DrawCtx 백엔드만(07-17 규약 — GDI+ 직접 호출 금지)
                let mut g = GdipCtx::new(dc);
                g.fill_round_rect(gc_rect(&rc), RADIUS, color(st.style.bg));
                g.stroke_round_rect(gc_rect(&rc), RADIUS, color(ring), width);
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
