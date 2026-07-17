//! droplist — **범용 드롭다운 선택** 커스텀 컨트롤(ctl 5호 — fontbox의 일반화.
//! 라이브러리 추상화 — 앱 비결합·comctl32 비의존).
//!
//! ## 계약(판매용 명세)
//! - 생성: [`create`] — 항목 라벨 목록(복사 소유)·초기 선택·[`Style`].
//! - 본체 = 현재 항목 + ▼ 표시(버튼형 — 편집 없음). 클릭/↓ = 팝업 목록
//!   (WS_EX_NOACTIVATE — 포커스 유지·hover 이동·클릭/Enter 확정·Esc/바깥 클릭 닫기 —
//!   fontbox에서 검증된 규약 재사용).
//! - 선택 확정 시 부모에 `WM_COMMAND(MAKEWPARAM(id, DL_CHANGED))`(lparam = 컨트롤).
//! - 조회/설정: [`DL_GETSEL`]/[`DL_SETSEL`](WM_USER+70/71 — SETSEL 통지 없음).

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, ClientToScreen, DrawTextW, EndPaint, InvalidateRect, SelectObject, SetBkMode,
    SetTextColor, DT_LEFT, DT_SINGLELINE, DT_VCENTER, HFONT, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetCursorPos, GetDlgCtrlID,
    GetParent, GetWindowLongPtrW, GetWindowRect, KillTimer, RegisterClassW, SendMessageW, SetTimer,
    SetWindowLongPtrW, SetWindowPos, GWLP_USERDATA, HMENU, HWND_TOPMOST, IDC_ARROW, SWP_NOACTIVATE,
    SWP_SHOWWINDOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_SETFONT, WM_TIMER, WNDCLASSW,
    WS_CHILD, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_TABSTOP, WS_VISIBLE,
};

use super::style::{fill, font_height, frame, Style};

/// 선택 확정 통지(WM_COMMAND HIWORD).
pub const DL_CHANGED: u32 = 1;
/// 현재 선택 조회.
pub const DL_GETSEL: u32 = 0x0400 + 70;
/// 선택 설정(wparam = 인덱스 — 통지 없음·범위 밖 무시).
pub const DL_SETSEL: u32 = 0x0400 + 71;

const TIMER_OUTSIDE: usize = 1;
/// 팝업 최대 가시 행.
const DROP_ROWS: i32 = 10;

struct DlState {
    items: Vec<String>,
    sel: usize,
    /// 팝업이 열릴 때의 임시 하이라이트(확정 전 — Esc/바깥 클릭이면 폐기).
    hot: usize,
    font: HFONT,
    style: Style,
    drop: Option<HWND>,
}

static REGISTER: std::sync::Once = std::sync::Once::new();
const CLASS: PCWSTR = w!("NexaDropList");
const POP_CLASS: PCWSTR = w!("NexaDropListPop");

/// 드롭다운 선택 생성 — `items` 라벨은 컨트롤이 복사 소유.
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
    let st = Box::new(DlState {
        items: items.iter().map(|s| s.to_string()).collect(),
        sel: selected.min(items.len().saturating_sub(1)),
        hot: 0,
        font,
        style,
        drop: None,
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

unsafe fn state(hwnd: HWND) -> *mut DlState {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut DlState
}

unsafe fn notify(hwnd: HWND) {
    if let Ok(parent) = GetParent(hwnd) {
        let id = GetDlgCtrlID(hwnd) as u32;
        SendMessageW(
            parent,
            WM_COMMAND,
            Some(WPARAM(((DL_CHANGED as usize) << 16) | id as usize)),
            Some(LPARAM(hwnd.0 as isize)),
        );
    }
}

unsafe fn row_h(hwnd: HWND, st: &DlState) -> i32 {
    font_height(hwnd, st.font) + 8
}

unsafe fn open_drop(hwnd: HWND, st: &mut DlState) {
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
    let drop = CreateWindowExW(
        WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
        POP_CLASS,
        w!(""),
        WS_POPUP,
        pt.x,
        pt.y + 1,
        rc.right.max(80),
        rh * visible + 2,
        Some(hwnd),
        None,
        None,
        None,
    )
    .unwrap_or_default();
    // owner 연결(QA 07-17 진범): WS_POPUP을 자식 컨트롤 owner로 만들면 시스템이
    // owner를 **최상위 창으로 승격** — GetParent(팝업)은 다이얼로그를 돌려주고
    // 그 GWLP_USERDATA를 DlState로 오독해 크래시. 팝업 자신의 USERDATA에 저장.
    SetWindowLongPtrW(drop, GWLP_USERDATA, hwnd.0 as isize);
    let _ = SetWindowPos(
        drop,
        Some(HWND_TOPMOST),
        pt.x,
        pt.y + 1,
        rc.right.max(80),
        rh * visible + 2,
        SWP_SHOWWINDOW | SWP_NOACTIVATE,
    );
    st.drop = Some(drop);
    let _ = SetTimer(Some(hwnd), TIMER_OUTSIDE, 60, None); // 바깥 클릭 감시(fontbox 규약)
}

unsafe fn close_drop(hwnd: HWND, st: &mut DlState) {
    if let Some(d) = st.drop.take() {
        let _ = KillTimer(Some(hwnd), TIMER_OUTSIDE);
        let _ = DestroyWindow(d);
    }
}

unsafe fn commit(hwnd: HWND, st: &mut DlState) {
    let changed = st.hot != st.sel;
    st.sel = st.hot;
    close_drop(hwnd, st);
    let _ = InvalidateRect(Some(hwnd), None, true);
    if changed {
        notify(hwnd);
    }
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
        m if m == DL_GETSEL => LRESULT(state(hwnd).as_ref().map_or(0, |s| s.sel as isize)),
        m if m == DL_SETSEL => {
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
            // 바깥 클릭 = 닫기(fontbox 검증 규약 — NOACTIVATE 보완)
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
                fill(dc, &rc, st.style.bg);
                frame(dc, &rc, st.style.border);
                let old = SelectObject(dc, st.font.into());
                SetBkMode(dc, TRANSPARENT);
                SetTextColor(dc, st.style.text);
                if let Some(label) = st.items.get(st.sel) {
                    let mut w16: Vec<u16> = label.encode_utf16().collect();
                    let mut trc = RECT {
                        left: rc.left + 6,
                        top: rc.top,
                        right: rc.right - 18,
                        bottom: rc.bottom,
                    };
                    DrawTextW(dc, &mut w16, &mut trc, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
                }
                SetTextColor(dc, st.style.text_dim);
                let mut arrow: Vec<u16> = "▾".encode_utf16().collect();
                let mut arc = RECT {
                    left: rc.right - 16,
                    top: rc.top,
                    right: rc.right - 4,
                    bottom: rc.bottom,
                };
                DrawTextW(
                    dc,
                    &mut arrow,
                    &mut arc,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE,
                );
                SelectObject(dc, old);
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// 팝업 목록 — 자기 그리기(오너드로 LISTBOX 대신 직접 — 스크롤은 hot 추종 창 이동).
/// 목록이 DROP_ROWS를 넘으면 hot 중심으로 표시 구간을 옮긴다(단순 가상화).
unsafe extern "system" fn pop_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // GetParent 금지(owner 승격 — open_drop 주석) — 자신의 USERDATA에서 owner 복원.
    let owner = HWND(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut core::ffi::c_void);
    match msg {
        0x0021 /* WM_MOUSEACTIVATE */ => LRESULT(3 /* MA_NOACTIVATE — fontbox 규약 */),
        WM_MOUSEMOVE => {
            if let Some(st) = state(owner).as_mut() {
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                let rh = row_h(owner, st);
                let first = first_visible(st);
                let hit = first + (y.max(0) / rh.max(1)) as usize;
                if hit < st.items.len() && hit != st.hot {
                    st.hot = hit;
                    let _ = InvalidateRect(Some(hwnd), None, true);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => LRESULT(0), // 포커스 강탈 방지(fontbox 진범 회피) — 확정은 UP
        WM_LBUTTONUP => {
            if let Some(st) = state(owner).as_mut() {
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                let rh = row_h(owner, st);
                let first = first_visible(st);
                let hit = first + (y.max(0) / rh.max(1)) as usize;
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
                frame(dc, &rc, st.style.border);
                let rh = row_h(owner, st);
                let first = first_visible(st);
                let old = SelectObject(dc, st.font.into());
                SetBkMode(dc, TRANSPARENT);
                for (row, idx) in (first..st.items.len()).enumerate() {
                    let top = rc.top + 1 + row as i32 * rh;
                    if top >= rc.bottom {
                        break;
                    }
                    let cell = RECT {
                        left: rc.left + 1,
                        top,
                        right: rc.right - 1,
                        bottom: (top + rh).min(rc.bottom - 1),
                    };
                    if idx == st.hot {
                        fill(dc, &cell, st.style.sel_bg);
                    }
                    SetTextColor(dc, st.style.text);
                    let mut w16: Vec<u16> = st.items[idx].encode_utf16().collect();
                    let mut trc = RECT {
                        left: cell.left + 6,
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

/// hot 중심의 첫 가시 인덱스(간단 가상화 — DROP_ROWS 창).
fn first_visible(st: &DlState) -> usize {
    let rows = DROP_ROWS as usize;
    if st.items.len() <= rows {
        0
    } else {
        st.hot.saturating_sub(rows / 2).min(st.items.len() - rows)
    }
}
