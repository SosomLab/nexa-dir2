//! combobox — **NxComboBox** 팝업 선택 버튼(ctl 7호 — macOS 팝업 버튼 스타일,
//! 사용자 시안 07-17. 라이브러리 추상화 — 앱 비결합·comctl32 비의존).
//!
//! ## 계약(판매용 명세 — 클래스 `Nexa.NxComboBox`)
//! - 생성: [`create`] — 항목 라벨 목록(복사 소유)·초기 선택·[`Style`].
//!   **높이 규칙(사용자 확정)**: `h <= 0` = 자동(글꼴 높이 + 상/하 최소 여백
//!   각 [`PAD_Y`]px), 더 크게 주면 텍스트는 상/하 **균등 여백** 세로 중앙.
//! - 본체 = 라운드 필(sel_bg) + 현재 항목 라벨 + 우측 이중 셰브론(⌃/⌄ 펜 그리기).
//! - 클릭/↓ = 팝업(✓ = 현재 선택 표기·hover 강조·클릭/Enter 확정·Esc/바깥 클릭
//!   닫기 — NOACTIVATE·owner는 팝업 USERDATA 저장[승격 함정 회피]).
//! - 선택 확정 시 부모에 `WM_COMMAND(MAKEWPARAM(id, NXCB_CHANGED))`(lparam = 컨트롤).
//! - 조회/설정: [`NXCB_GETSEL`]/[`NXCB_SETSEL`](WM_USER+90/91 — SETSEL 통지 없음).

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, ClientToScreen, CreatePen, CreateRoundRectRgn, CreateSolidBrush, DeleteObject,
    DrawTextW, EndPaint, GetStockObject, InvalidateRect, LineTo, MoveToEx, RoundRect, SelectObject,
    SetBkMode, SetTextColor, SetWindowRgn, DT_LEFT, DT_SINGLELINE, DT_VCENTER, HDC, HFONT,
    NULL_BRUSH, PAINTSTRUCT, PS_SOLID, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetCursorPos, GetDlgCtrlID,
    GetParent, GetWindowLongPtrW, GetWindowRect, KillTimer, RegisterClassW, SendMessageW, SetTimer,
    SetWindowLongPtrW, SetWindowPos, GWLP_USERDATA, HMENU, HWND_TOPMOST, IDC_ARROW, SWP_NOACTIVATE,
    SWP_SHOWWINDOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_SETFONT, WM_TIMER, WNDCLASSW,
    WS_CHILD, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_TABSTOP, WS_VISIBLE,
};

use super::style::{fill, font_height, Style};

/// 선택 확정 통지(WM_COMMAND HIWORD).
pub const NXCB_CHANGED: u32 = 1;
/// 현재 선택 조회.
pub const NXCB_GETSEL: u32 = 0x0400 + 90;
/// 선택 설정(wparam = 인덱스 — 통지 없음·범위 밖 무시).
pub const NXCB_SETSEL: u32 = 0x0400 + 91;

/// 텍스트 상/하 최소 여백 — 공통 규약([`super::style::PAD_Y`]) 재노출(하위호환).
pub use super::style::PAD_Y;

const TIMER_OUTSIDE: usize = 1;
/// 팝업 최대 가시 행.
const DROP_ROWS: i32 = 12;
/// 본체/팝업 라운드 반경(px — 시안: 필 형태).
const RADIUS: i32 = 6;
/// ✓ 열 폭(px).
const CHECK_W: i32 = 22;

struct CbState {
    items: Vec<String>,
    sel: usize,
    /// 팝업 임시 하이라이트(확정 전 — Esc/바깥 클릭 폐기).
    hot: usize,
    font: HFONT,
    style: Style,
    drop: Option<HWND>,
}

static REGISTER: std::sync::Once = std::sync::Once::new();
const CLASS: PCWSTR = w!("Nexa.NxComboBox");
const POP_CLASS: PCWSTR = w!("Nexa.NxComboBoxPop");

/// NxComboBox 생성 — `items` 라벨은 컨트롤이 복사 소유. `h <= 0` = 자동 높이.
#[allow(clippy::too_many_arguments)] // Win32 create 계열 관례
pub unsafe fn create(
    parent: HWND,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    id: u32,
    font: HFONT,
    items: &[&str],
    selected: usize,
    style: Style,
) -> HWND {
    REGISTER.call_once(|| {
        for (class, p) in [
            (
                CLASS,
                ctl_proc as unsafe extern "system" fn(_, _, _, _) -> _,
            ),
            (POP_CLASS, pop_proc),
        ] {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(p),
                lpszClassName: class,
                hCursor: windows::Win32::UI::WindowsAndMessaging::LoadCursorW(None, IDC_ARROW)
                    .unwrap_or_default(),
                ..Default::default()
            };
            RegisterClassW(&wc);
        }
    });
    // 높이 규칙: 자동 = 공통 auto_height(전 Nx 컨트롤 동일 — 반듯한 기본 배치)
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
    let st = Box::new(CbState {
        items: items.iter().map(|s| s.to_string()).collect(),
        sel: selected.min(items.len().saturating_sub(1)),
        hot: 0,
        font,
        style,
        drop: None,
    });
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(st) as isize);
    // 본체 = 라운드 필 — 창 리전 클립(모서리 밖은 부모 배경 그대로 — 배경색 비의존)
    let rgn = CreateRoundRectRgn(0, 0, w + 1, h + 1, RADIUS * 2, RADIUS * 2);
    let _ = SetWindowRgn(hwnd, Some(rgn), false);
    SendMessageW(
        hwnd,
        WM_SETFONT,
        Some(WPARAM(font.0 as usize)),
        Some(LPARAM(1)),
    );
    hwnd
}

unsafe fn state(hwnd: HWND) -> *mut CbState {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut CbState
}

unsafe fn notify(hwnd: HWND) {
    if let Ok(parent) = GetParent(hwnd) {
        let id = GetDlgCtrlID(hwnd) as u32;
        SendMessageW(
            parent,
            WM_COMMAND,
            Some(WPARAM(((NXCB_CHANGED as usize) << 16) | id as usize)),
            Some(LPARAM(hwnd.0 as isize)),
        );
    }
}

/// 팝업 행 높이 — 본체와 같은 규칙(글꼴 + 상/하 균등 여백).
unsafe fn row_h(hwnd: HWND, st: &CbState) -> i32 {
    font_height(hwnd, st.font) + PAD_Y * 2 + 2
}

unsafe fn open_drop(hwnd: HWND, st: &mut CbState) {
    if st.drop.is_some() || st.items.is_empty() {
        return;
    }
    st.hot = st.sel;
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let mut pt = POINT { x: 0, y: rc.bottom };
    let _ = ClientToScreen(hwnd, &mut pt);
    let rh = row_h(hwnd, st);
    let visible = (st.items.len() as i32).min(DROP_ROWS);
    let (w, h) = (rc.right.max(80), rh * visible + 6);
    let drop = CreateWindowExW(
        WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
        POP_CLASS,
        w!(""),
        WS_POPUP,
        pt.x,
        pt.y + 2,
        w,
        h,
        Some(hwnd),
        None,
        None,
        None,
    )
    .unwrap_or_default();
    // owner 연결(droplist 진범 교훈): WS_POPUP owner는 최상위로 승격 —
    // GetParent 금지, 팝업 자신의 USERDATA에 저장.
    SetWindowLongPtrW(drop, GWLP_USERDATA, hwnd.0 as isize);
    // 팝업도 라운드(시안 — 모서리 잘림)
    let rgn = CreateRoundRectRgn(0, 0, w + 1, h + 1, RADIUS * 2, RADIUS * 2);
    let _ = SetWindowRgn(drop, Some(rgn), false);
    let _ = SetWindowPos(
        drop,
        Some(HWND_TOPMOST),
        pt.x,
        pt.y + 2,
        w,
        h,
        SWP_SHOWWINDOW | SWP_NOACTIVATE,
    );
    st.drop = Some(drop);
    let _ = SetTimer(Some(hwnd), TIMER_OUTSIDE, 60, None); // 바깥 클릭 감시
}

unsafe fn close_drop(hwnd: HWND, st: &mut CbState) {
    if let Some(d) = st.drop.take() {
        let _ = KillTimer(Some(hwnd), TIMER_OUTSIDE);
        let _ = DestroyWindow(d);
    }
}

unsafe fn commit(hwnd: HWND, st: &mut CbState) {
    let changed = st.hot != st.sel;
    st.sel = st.hot;
    close_drop(hwnd, st);
    let _ = InvalidateRect(Some(hwnd), None, true);
    if changed {
        notify(hwnd);
    }
}

/// 이중 셰브론(⌃/⌄) — 우측 존에 펜으로 그린다(글리프 폰트 의존 제거).
unsafe fn draw_chevrons(dc: HDC, zone: &RECT, color: COLORREF) {
    let pen = CreatePen(PS_SOLID, 1, color);
    let old = SelectObject(dc, pen.into());
    let cx = (zone.left + zone.right) / 2;
    let cy = (zone.top + zone.bottom) / 2;
    let (hw, hh, gap) = (3, 3, 2); // 셰브론 반폭·높이·중심 간격
    for (dir, base) in [(-1, cy - gap), (1, cy + gap)] {
        // dir=-1: ⌃(꼭짓점 위), dir=1: ⌄(꼭짓점 아래)
        let tip = base + dir * hh;
        let _ = MoveToEx(dc, cx - hw, base, None);
        let _ = LineTo(dc, cx, tip);
        let _ = LineTo(dc, cx + hw + 1, base - dir);
    }
    SelectObject(dc, old);
    let _ = DeleteObject(pen.into());
}

/// ✓ 마크 — 펜 폴리라인(팝업 선택 표기).
unsafe fn draw_check(dc: HDC, x: i32, cy: i32, color: COLORREF) {
    let pen = CreatePen(PS_SOLID, 2, color);
    let old = SelectObject(dc, pen.into());
    let _ = MoveToEx(dc, x, cy, None);
    let _ = LineTo(dc, x + 3, cy + 3);
    let _ = LineTo(dc, x + 9, cy - 4);
    SelectObject(dc, old);
    let _ = DeleteObject(pen.into());
}

unsafe extern "system" fn ctl_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => LRESULT(0),
        WM_DESTROY => {
            let p = state(hwnd);
            if let Some(st) = p.as_mut() {
                close_drop(hwnd, st);
            }
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
        m if m == NXCB_GETSEL => LRESULT(state(hwnd).as_ref().map_or(0, |s| s.sel as isize)),
        m if m == NXCB_SETSEL => {
            if let Some(st) = state(hwnd).as_mut() {
                if wparam.0 < st.items.len() {
                    st.sel = wparam.0;
                    let _ = InvalidateRect(Some(hwnd), None, true);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            if let Some(st) = state(hwnd).as_mut() {
                if st.drop.is_some() {
                    close_drop(hwnd, st);
                } else {
                    open_drop(hwnd, st);
                }
                let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(hwnd));
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if let Some(st) = state(hwnd).as_mut() {
                let vk = wparam.0 as u32;
                if st.drop.is_some() {
                    match vk {
                        0x26 => st.hot = st.hot.saturating_sub(1),             // ↑
                        0x28 => st.hot = (st.hot + 1).min(st.items.len() - 1), // ↓
                        0x0D => {
                            commit(hwnd, st);
                            return LRESULT(0);
                        }
                        0x1B => {
                            close_drop(hwnd, st);
                            return LRESULT(0);
                        }
                        _ => return LRESULT(0),
                    }
                    if let Some(d) = st.drop {
                        let _ = InvalidateRect(Some(d), None, true);
                    }
                } else if vk == 0x28 {
                    open_drop(hwnd, st); // 닫힘 상태 ↓ = 열기
                }
            }
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == TIMER_OUTSIDE => {
            if let Some(st) = state(hwnd).as_mut() {
                if let Some(drop) = st.drop {
                    let pressed =
                        windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x01) < 0;
                    if pressed {
                        let mut pt = POINT::default();
                        let _ = GetCursorPos(&mut pt);
                        let inside = |w: HWND| -> bool {
                            let mut rc = RECT::default();
                            if GetWindowRect(w, &mut rc).is_err() {
                                return false;
                            }
                            pt.x >= rc.left && pt.x < rc.right && pt.y >= rc.top && pt.y < rc.bottom
                        };
                        if !inside(drop) && !inside(hwnd) {
                            close_drop(hwnd, st);
                        }
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
                let _ = GetClientRect(hwnd, &mut rc);
                // 본체 필 — 리전이 라운드 클립하므로 전체 채움이면 충분
                fill(dc, &rc, st.style.sel_bg);
                // 현재 항목 라벨(세로 중앙 — 상/하 균등 여백)
                let old = SelectObject(dc, st.font.into());
                SetBkMode(dc, TRANSPARENT);
                SetTextColor(dc, st.style.text);
                if let Some(label) = st.items.get(st.sel) {
                    let mut w16: Vec<u16> = label.encode_utf16().collect();
                    let mut trc = RECT {
                        left: rc.left + 10,
                        top: rc.top,
                        right: rc.right - 22,
                        bottom: rc.bottom,
                    };
                    DrawTextW(dc, &mut w16, &mut trc, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
                }
                SelectObject(dc, old);
                // 우측 이중 셰브론
                let zone = RECT {
                    left: rc.right - 20,
                    top: rc.top,
                    right: rc.right - 6,
                    bottom: rc.bottom,
                };
                draw_chevrons(dc, &zone, st.style.text);
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// 팝업 목록 — 자기 그리기(✓ = 현재 선택·hover = accent). 목록이 DROP_ROWS를
/// 넘으면 hot 중심 표시 구간 이동(간단 가상화 — droplist 규약).
unsafe extern "system" fn pop_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // owner = 팝업 USERDATA(승격 함정 회피 — droplist 교훈)
    let owner = HWND(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut core::ffi::c_void);
    match msg {
        0x0021 /* WM_MOUSEACTIVATE */ => LRESULT(3 /* MA_NOACTIVATE */),
        WM_MOUSEMOVE => {
            if let Some(st) = state(owner).as_mut() {
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                let rh = row_h(owner, st);
                let first = first_visible(st);
                let hit = first + ((y - 3).max(0) / rh.max(1)) as usize;
                if hit < st.items.len() && hit != st.hot {
                    st.hot = hit;
                    let _ = InvalidateRect(Some(hwnd), None, true);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => LRESULT(0), // 포커스 강탈 방지 — 확정은 UP
        WM_LBUTTONUP => {
            if let Some(st) = state(owner).as_mut() {
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                let rh = row_h(owner, st);
                let first = first_visible(st);
                let hit = first + ((y - 3).max(0) / rh.max(1)) as usize;
                if hit < st.items.len() {
                    st.hot = hit;
                    commit(owner, st);
                }
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            if let Some(st) = state(owner).as_ref() {
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                fill(dc, &rc, st.style.bg);
                // 라운드 외곽선
                let pen = CreatePen(PS_SOLID, 1, st.style.border);
                let old_p = SelectObject(dc, pen.into());
                let old_b = SelectObject(dc, GetStockObject(NULL_BRUSH));
                let _ = RoundRect(
                    dc,
                    rc.left,
                    rc.top,
                    rc.right,
                    rc.bottom,
                    RADIUS * 2,
                    RADIUS * 2,
                );
                SelectObject(dc, old_b);
                SelectObject(dc, old_p);
                let _ = DeleteObject(pen.into());
                let rh = row_h(owner, st);
                let first = first_visible(st);
                let old = SelectObject(dc, st.font.into());
                SetBkMode(dc, TRANSPARENT);
                for (row, idx) in (first..st.items.len()).enumerate() {
                    let top = rc.top + 3 + row as i32 * rh;
                    if top >= rc.bottom - 3 {
                        break;
                    }
                    let cell = RECT {
                        left: rc.left + 3,
                        top,
                        right: rc.right - 3,
                        bottom: (top + rh).min(rc.bottom - 3),
                    };
                    let hot = idx == st.hot;
                    if hot {
                        // hover = accent 라운드 필 + bg 글자(시안)
                        let brush = CreateSolidBrush(st.style.accent);
                        let hpen = CreatePen(PS_SOLID, 1, st.style.accent);
                        let ob = SelectObject(dc, brush.into());
                        let op = SelectObject(dc, hpen.into());
                        let _ = RoundRect(
                            dc,
                            cell.left,
                            cell.top,
                            cell.right,
                            cell.bottom,
                            RADIUS,
                            RADIUS,
                        );
                        SelectObject(dc, op);
                        SelectObject(dc, ob);
                        let _ = DeleteObject(hpen.into());
                        let _ = DeleteObject(brush.into());
                    }
                    let fg = if hot { st.style.bg } else { st.style.text };
                    if idx == st.sel {
                        draw_check(dc, cell.left + 6, (cell.top + cell.bottom) / 2, fg);
                    }
                    SetTextColor(dc, fg);
                    let mut w16: Vec<u16> = st.items[idx].encode_utf16().collect();
                    let mut trc = RECT {
                        left: cell.left + CHECK_W,
                        ..cell
                    };
                    DrawTextW(dc, &mut w16, &mut trc, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
                }
                SelectObject(dc, old);
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// hot 중심 첫 가시 인덱스(DROP_ROWS 창 — droplist 규약).
fn first_visible(st: &CbState) -> usize {
    let rows = DROP_ROWS as usize;
    if st.items.len() <= rows {
        0
    } else {
        st.hot.saturating_sub(rows / 2).min(st.items.len() - rows)
    }
}
