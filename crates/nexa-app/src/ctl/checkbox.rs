//! checkbox — **NxCheckBox** 체크 박스(ctl 8호 — macOS 시안 07-17: 미체크 =
//! 연회색 라운드 박스 · 체크 = accent 라운드 박스 + 흰 ✓.
//! 라이브러리 추상화 — 앱 비결합·comctl32 비의존).
//!
//! ## 계약(판매용 명세 — 클래스 `Nexa.NxCheckBox`)
//! - 생성: [`create`] — 라벨(빈 문자열 = 박스만·복사 소유)·초기 상태·[`Style`].
//!   **높이 규칙(콤보와 동일)**: `h <= 0` = 자동(글꼴 높이 + 상/하 최소 여백
//!   각 [`super::combobox::PAD_Y`]px). **박스는 컨트롤 높이와 분리**(사용자
//!   확정 07-17 — 기본이 커 보임): 글꼴 높이 − 2 정사각을 세로 중앙에 그린다 —
//!   클릭 영역·행 정렬은 그대로, 시각만 시안 비율. 라벨은 우측 세로 중앙.
//! - 클릭/Space = 토글 → 부모에 `WM_COMMAND(MAKEWPARAM(id, NXCHK_CHANGED))`.
//! - 조회/설정: [`NXCHK_GETCHECK`]/[`NXCHK_SETCHECK`](WM_USER+95/96 — SETCHECK
//!   통지 없음). 배경 = `style.bg`(호스트 배경과 일치시킬 것 — 카드 위 = 카드 bg).

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, InvalidateRect,
    LineTo, MoveToEx, RoundRect, SelectObject, SetBkMode, SetTextColor, DT_LEFT, DT_SINGLELINE,
    DT_VCENTER, HFONT, PAINTSTRUCT, PS_SOLID, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetClientRect, GetDlgCtrlID, GetParent, GetWindowLongPtrW,
    RegisterClassW, SendMessageW, SetWindowLongPtrW, GWLP_USERDATA, HMENU, IDC_ARROW,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_KEYDOWN, WM_LBUTTONDOWN,
    WM_PAINT, WM_SETFONT, WNDCLASSW, WS_CHILD, WS_TABSTOP, WS_VISIBLE,
};

use super::combobox::PAD_Y;
use super::style::{fill, font_height, Style};

/// 토글 통지(WM_COMMAND HIWORD).
pub const NXCHK_CHANGED: u32 = 1;
/// 체크 상태 조회(반환 = 0/1).
pub const NXCHK_GETCHECK: u32 = 0x0400 + 95;
/// 체크 상태 설정(wparam = 0/1 — 통지 없음).
pub const NXCHK_SETCHECK: u32 = 0x0400 + 96;

struct ChkState {
    label: String,
    checked: bool,
    font: HFONT,
    style: Style,
}

static REGISTER: std::sync::Once = std::sync::Once::new();
const CLASS: PCWSTR = w!("Nexa.NxCheckBox");

/// NxCheckBox 생성 — `label` 빈 문자열 = 박스만. `h <= 0` = 자동 높이.
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
    checked: bool,
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
    let auto_h = font_height(parent, font) + PAD_Y * 2;
    let h = if h <= 0 { auto_h } else { h };
    let w = if w <= 0 { h } else { w }; // 폭 생략 = 박스만(정사각)
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
    let st = Box::new(ChkState {
        label: label.to_string(),
        checked,
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

unsafe fn state(hwnd: HWND) -> *mut ChkState {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ChkState
}

unsafe fn toggle(hwnd: HWND, st: &mut ChkState) {
    st.checked = !st.checked;
    let _ = InvalidateRect(Some(hwnd), None, true);
    if let Ok(parent) = GetParent(hwnd) {
        let id = GetDlgCtrlID(hwnd) as u32;
        SendMessageW(
            parent,
            WM_COMMAND,
            Some(WPARAM(((NXCHK_CHANGED as usize) << 16) | id as usize)),
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
        m if m == NXCHK_GETCHECK => LRESULT(state(hwnd).as_ref().map_or(0, |s| s.checked as isize)),
        m if m == NXCHK_SETCHECK => {
            if let Some(st) = state(hwnd).as_mut() {
                st.checked = wparam.0 != 0;
                let _ = InvalidateRect(Some(hwnd), None, true);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            if let Some(st) = state(hwnd).as_mut() {
                toggle(hwnd, st);
                let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(hwnd));
            }
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 as u32 == 0x20 => {
            // Space = 토글(표준 체크박스 키 계약)
            if let Some(st) = state(hwnd).as_mut() {
                toggle(hwnd, st);
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            if let Some(st) = state(hwnd).as_ref() {
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                fill(dc, &rc, st.style.bg);
                // 박스 = 글꼴 높이 기준 정사각을 세로 중앙에(컨트롤 높이와 분리 —
                // 클릭 영역은 그대로, 시각만 시안 비율. 미체크 = sel_bg·체크 = accent)
                let ch = rc.bottom - rc.top;
                // 글꼴 높이 정사각(사용자 확정 07-17: −2에서 상/하 1px씩 증가)
                let side = font_height(hwnd, st.font).clamp(10, ch);
                let btop = rc.top + (ch - side) / 2;
                let radius = (side / 3).max(4);
                let box_color = if st.checked {
                    st.style.accent
                } else {
                    st.style.sel_bg
                };
                let brush = CreateSolidBrush(box_color);
                let pen = CreatePen(PS_SOLID, 1, box_color);
                let ob = SelectObject(dc, brush.into());
                let op = SelectObject(dc, pen.into());
                let _ = RoundRect(
                    dc,
                    rc.left,
                    btop,
                    rc.left + side,
                    btop + side,
                    radius,
                    radius,
                );
                SelectObject(dc, op);
                SelectObject(dc, ob);
                let _ = DeleteObject(pen.into());
                let _ = DeleteObject(brush.into());
                if st.checked {
                    // 흰 ✓ — 펜 폴리라인(시안)
                    let cpen = CreatePen(PS_SOLID, 2, st.style.bg);
                    let old = SelectObject(dc, cpen.into());
                    let (cx, cy) = (rc.left + side / 2, btop + side / 2);
                    let _ = MoveToEx(dc, cx - side / 4, cy, None);
                    let _ = LineTo(dc, cx - side / 12, cy + side / 4 - 1);
                    let _ = LineTo(dc, cx + side / 4, cy - side / 5);
                    SelectObject(dc, old);
                    let _ = DeleteObject(cpen.into());
                }
                // 라벨(우측 세로 중앙 — 빈 문자열 = 박스만)
                if !st.label.is_empty() {
                    let old = SelectObject(dc, st.font.into());
                    SetBkMode(dc, TRANSPARENT);
                    SetTextColor(dc, st.style.text);
                    let mut w16: Vec<u16> = st.label.encode_utf16().collect();
                    let mut trc = RECT {
                        left: rc.left + side + 6,
                        ..rc
                    };
                    DrawTextW(dc, &mut w16, &mut trc, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
                    SelectObject(dc, old);
                }
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
