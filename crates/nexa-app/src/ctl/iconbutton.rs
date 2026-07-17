//! iconbutton — **NxIconButton** 도형 투명 이미지 버튼(ctl 10호 — macOS 시안
//! 07-17: 원형 +/− 버튼. 라이브러리 추상화 — 앱 비결합).
//!
//! ## 설계(사용자 질의 07-17 — "추상 클래스+도형별 vs PNG 1개")
//! **단일 컨트롤 + 아이콘 소스 enum**을 채택한다:
//! - Rust/Win32에는 클래스 상속이 없어 "추상 기반 + 도형별 서브클래스"는 창
//!   클래스·플러밍(상태·통지·히트테스트) 중복만 남는다 — 도형은 **데이터**로.
//! - [`Icon`] enum이 소스 추상화: 현재 = 벡터 글리프(펜 — DPI/색 자유·자산 0).
//!   **PNG 등 알파 이미지는 같은 enum의 확장 변형**(`Icon::Image(HBITMAP)` —
//!   32bpp premultiplied + msimg32 AlphaBlend[OS 인박스])으로 추가한다. 호스트
//!   API는 불변.
//! - **shape 투명** = 원형 창 리전 클립(모서리 밖 = 부모 배경 그대로).
//!
//! ## 계약(판매용 명세 — 클래스 `Nexa.NxIconButton`)
//! - 생성: [`create`] — `d <= 0` = 공통 자동 높이 지름(전 Nx 기본 정렬).
//! - 클릭(enabled일 때만) → `WM_COMMAND(MAKEWPARAM(id, NXIB_CLICK))`.
//! - [`NXIB_GETENABLE`]/[`NXIB_SETENABLE`](WM_USER+100/101): 비활성 =
//!   sel_bg 원 + 글리프(시안 — 삭제 대상이 자신뿐인 − 버튼), 클릭 무시.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateEllipticRgn, CreatePen, CreateSolidBrush, DeleteObject, Ellipse, EndPaint,
    InvalidateRect, LineTo, MoveToEx, SelectObject, SetWindowRgn, PAINTSTRUCT, PS_SOLID,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetDlgCtrlID, GetParent, GetWindowLongPtrW, RegisterClassW,
    SendMessageW, SetWindowLongPtrW, GWLP_USERDATA, HMENU, IDC_ARROW, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_LBUTTONDOWN, WM_PAINT, WNDCLASSW, WS_CHILD,
    WS_TABSTOP, WS_VISIBLE,
};

use super::style::Style;

/// 클릭 통지(WM_COMMAND HIWORD — enabled일 때만).
pub const NXIB_CLICK: u32 = 1;
/// 활성 상태 조회(반환 = 0/1).
pub const NXIB_GETENABLE: u32 = 0x0400 + 100;
/// 활성 상태 설정(wparam = 0/1 — 재도장).
pub const NXIB_SETENABLE: u32 = 0x0400 + 101;

/// 아이콘 소스 — 현재 벡터 글리프(펜). PNG/HBITMAP 알파 이미지는 확장 변형으로
/// 추가(모듈 설계 주석 — AlphaBlend 경로), 호스트 API 불변.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    /// ＋ (십자)
    Plus,
    /// − (수평선)
    Minus,
}

struct IbState {
    icon: Icon,
    enabled: bool,
    style: Style,
}

static REGISTER: std::sync::Once = std::sync::Once::new();
const CLASS: PCWSTR = w!("Nexa.NxIconButton");

/// NxIconButton 생성 — 원형(지름 `d`·`d <= 0` = 공통 자동 높이). shape 밖 투명.
#[allow(clippy::too_many_arguments)] // Win32 create 계열 관례
pub unsafe fn create(
    parent: HWND,
    x: i32,
    y: i32,
    d: i32,
    id: u32,
    font: windows::Win32::Graphics::Gdi::HFONT,
    icon: Icon,
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
    // d<=0 = 공통 자동 높이 지름(전 Nx 컨트롤 동일 — 반듯한 기본 배치)
    let d = if d <= 0 {
        super::style::auto_height(parent, font)
    } else {
        d
    };
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        CLASS,
        w!(""),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(WS_TABSTOP.0),
        x,
        y,
        d,
        d,
        Some(parent),
        Some(HMENU(id as usize as *mut core::ffi::c_void)),
        None,
        None,
    )
    .unwrap_or_default();
    let st = Box::new(IbState {
        icon,
        enabled,
        style,
    });
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(st) as isize);
    // shape 투명 — 원형 리전 클립(밖은 부모 배경)
    let rgn = CreateEllipticRgn(0, 0, d + 1, d + 1);
    let _ = SetWindowRgn(hwnd, Some(rgn), false);
    hwnd
}

unsafe fn state(hwnd: HWND) -> *mut IbState {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut IbState
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
        m if m == NXIB_GETENABLE => LRESULT(state(hwnd).as_ref().map_or(0, |s| s.enabled as isize)),
        m if m == NXIB_SETENABLE => {
            if let Some(st) = state(hwnd).as_mut() {
                st.enabled = wparam.0 != 0;
                let _ = InvalidateRect(Some(hwnd), None, true);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            if let Some(st) = state(hwnd).as_ref() {
                if st.enabled {
                    if let Ok(parent) = GetParent(hwnd) {
                        let id = GetDlgCtrlID(hwnd) as u32;
                        SendMessageW(
                            parent,
                            WM_COMMAND,
                            Some(WPARAM(((NXIB_CLICK as usize) << 16) | id as usize)),
                            Some(LPARAM(hwnd.0 as isize)),
                        );
                    }
                }
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            if let Some(st) = state(hwnd).as_ref() {
                let mut rc = RECT::default();
                let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rc);
                // 원판: 활성 = text_dim(진회색)·비활성 = sel_bg(연회색) — 시안
                let disc = if st.enabled {
                    st.style.text_dim
                } else {
                    st.style.sel_bg
                };
                let brush = CreateSolidBrush(disc);
                let pen = CreatePen(PS_SOLID, 1, disc);
                let ob = SelectObject(dc, brush.into());
                let op = SelectObject(dc, pen.into());
                let _ = Ellipse(dc, rc.left, rc.top, rc.right, rc.bottom);
                SelectObject(dc, op);
                SelectObject(dc, ob);
                let _ = DeleteObject(pen.into());
                let _ = DeleteObject(brush.into());
                // 글리프 = bg 색 펜 2px(흰 +/−)
                let d = rc.right - rc.left;
                let (cx, cy) = ((rc.left + rc.right) / 2, (rc.top + rc.bottom) / 2);
                let arm = (d / 4).max(3);
                let gpen = CreatePen(PS_SOLID, 2, st.style.bg);
                let old = SelectObject(dc, gpen.into());
                let _ = MoveToEx(dc, cx - arm, cy, None);
                let _ = LineTo(dc, cx + arm + 1, cy);
                if st.icon == Icon::Plus {
                    let _ = MoveToEx(dc, cx, cy - arm, None);
                    let _ = LineTo(dc, cx, cy + arm + 1);
                }
                SelectObject(dc, old);
                let _ = DeleteObject(gpen.into());
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
