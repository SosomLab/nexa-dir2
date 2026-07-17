//! menubutton — **NxMenuButton** 오버플로 메뉴 버튼(ctl 13호 — 시안 07-17:
//! `… ⌄` 필 버튼 + 액션 드롭다운. 관용 명칭 = **메뉴 버튼/오버플로 버튼**
//! (WinUI DropDownButton·macOS pull-down 대응). 라이브러리 추상화 — 앱 비결합).
//!
//! ## 계약(판매용 명세 — 클래스 `Nexa.NxMenuButton`)
//! - 목적: **좁은 자리**에 여러 기능을 접어 두고, 항목 클릭 = 기능 실행
//!   (콤보와 달리 **선택 상태가 없다** — 순수 액션 메뉴).
//! - 생성: [`create`] — 액션 라벨 목록(복사 소유)·[`Style`].
//!   `h <= 0` = 공통 자동 높이 · `w <= 0` = `…`+⌄ 최소 폭.
//! - 항목 실행 시 부모에 `WM_COMMAND(MAKEWPARAM(id, NXMB_PICK))`(lparam =
//!   컨트롤) → 호스트는 [`NXMB_GETPICK`]으로 **마지막 실행 인덱스** 조회.
//! - 팝업 규약 = NxComboBox와 동일(NOACTIVATE·hover·클릭/Enter 실행·Esc/바깥
//!   클릭 닫기·owner = 팝업 USERDATA[승격 함정 회피])·✓ 없음.
//! - 항목 `"-"` = **구분선**(07-18 시안 — 프리셋들 위·고정 액션 아래):
//!   hover/클릭/통지 없음, 수평선만 그린다.

use nexa_gui::DrawCtx;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, ClientToScreen, CreateRoundRectRgn, DrawTextW, EndPaint, InvalidateRect,
    SelectObject, SetBkMode, SetTextColor, SetWindowRgn, DT_CENTER, DT_LEFT, DT_SINGLELINE,
    DT_VCENTER, HFONT, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetCursorPos, GetDlgCtrlID,
    GetParent, GetWindowLongPtrW, GetWindowRect, KillTimer, RegisterClassW, SendMessageW, SetTimer,
    SetWindowLongPtrW, SetWindowPos, GWLP_USERDATA, HMENU, HWND_TOPMOST, IDC_ARROW, SWP_NOACTIVATE,
    SWP_SHOWWINDOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_SETFONT, WM_TIMER, WNDCLASSW,
    WS_CHILD, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_TABSTOP, WS_VISIBLE,
};

use super::gdipctx::{color, rect as gc_rect, GdipCtx};
use super::style::{fill, font_height, Style};

/// 항목 실행 통지(WM_COMMAND HIWORD) — 인덱스는 [`NXMB_GETPICK`]으로 조회.
pub const NXMB_PICK: u32 = 1;
/// 마지막 실행 인덱스 조회(실행 전 = -1).
pub const NXMB_GETPICK: u32 = 0x0400 + 110;

const TIMER_OUTSIDE: usize = 1;
/// 팝업 최대 가시 행.
const DROP_ROWS: i32 = 12;
/// 라운드 반경(px — 콤보 시안 계열).
const RADIUS: i32 = 6;

struct MbState {
    items: Vec<String>,
    /// 마지막 실행 인덱스(-1 = 없음).
    picked: isize,
    /// 팝업 hover(실행 전 하이라이트).
    hot: usize,
    font: HFONT,
    style: Style,
    drop: Option<HWND>,
}

static REGISTER: std::sync::Once = std::sync::Once::new();
const CLASS: PCWSTR = w!("Nexa.NxMenuButton");
const POP_CLASS: PCWSTR = w!("Nexa.NxMenuButtonPop");

/// NxMenuButton 생성 — `items` = 액션 라벨(복사 소유). `w/h <= 0` = 자동.
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
    let h = if h <= 0 {
        super::style::auto_height(parent, font)
    } else {
        h
    };
    let w = if w <= 0 { 48 } else { w }; // …+⌄ 최소 폭(좁은 자리 목적)
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
    let st = Box::new(MbState {
        items: items.iter().map(|s| s.to_string()).collect(),
        picked: -1,
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

unsafe fn state(hwnd: HWND) -> *mut MbState {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut MbState
}

unsafe fn row_h(hwnd: HWND, st: &MbState) -> i32 {
    font_height(hwnd, st.font) + super::style::PAD_Y * 2 + 2
}

unsafe fn open_drop(hwnd: HWND, st: &mut MbState) {
    if st.drop.is_some() || st.items.is_empty() {
        return;
    }
    st.hot = 0;
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let mut pt = POINT { x: 0, y: rc.bottom };
    let _ = ClientToScreen(hwnd, &mut pt);
    let rh = row_h(hwnd, st);
    let visible = (st.items.len() as i32).min(DROP_ROWS);
    // 액션 라벨 폭 실측(좁은 본체보다 넓은 팝업 — 시안)
    let tw = st
        .items
        .iter()
        .map(|s| super::style::text_width(hwnd, st.font, s))
        .max()
        .unwrap_or(80);
    let (w, h) = ((tw + 28).max(rc.right), rh * visible + 6);
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
    // owner 연결(승격 함정 회피 — 콤보 규약)
    SetWindowLongPtrW(drop, GWLP_USERDATA, hwnd.0 as isize);
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

unsafe fn close_drop(hwnd: HWND, st: &mut MbState) {
    if let Some(d) = st.drop.take() {
        let _ = KillTimer(Some(hwnd), TIMER_OUTSIDE);
        let _ = DestroyWindow(d);
    }
}

/// 항목 실행 — 인덱스 기록 + 통지(선택 상태 없음 — 액션 1회성). 구분선 무시.
unsafe fn pick(hwnd: HWND, st: &mut MbState, idx: usize) {
    if st.items.get(idx).is_some_and(|s| s == "-") {
        return; // 구분선 = 실행 불가
    }
    st.picked = idx as isize;
    close_drop(hwnd, st);
    if let Ok(parent) = GetParent(hwnd) {
        let id = GetDlgCtrlID(hwnd) as u32;
        SendMessageW(
            parent,
            WM_COMMAND,
            Some(WPARAM(((NXMB_PICK as usize) << 16) | id as usize)),
            Some(LPARAM(hwnd.0 as isize)),
        );
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
        m if m == NXMB_GETPICK => LRESULT(state(hwnd).as_ref().map_or(-1, |s| s.picked)),
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
                    // 구분선("-")은 건너뛴다(키보드 탐색)
                    let step = |from: usize, up: bool, items: &[String]| -> usize {
                        let mut i = from as isize;
                        loop {
                            i += if up { -1 } else { 1 };
                            if i < 0 || i as usize >= items.len() {
                                return from;
                            }
                            if items[i as usize] != "-" {
                                return i as usize;
                            }
                        }
                    };
                    match vk {
                        0x26 => st.hot = step(st.hot, true, &st.items),  // ↑
                        0x28 => st.hot = step(st.hot, false, &st.items), // ↓
                        0x0D => {
                            let i = st.hot;
                            pick(hwnd, st, i);
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
                // 본체 = 라운드 필 + 외곽선(콤보 규약 — 동색 배경 구별)
                fill(dc, &rc, st.style.behind);
                {
                    let mut g = GdipCtx::new(dc);
                    g.fill_round_rect(gc_rect(&rc), RADIUS, color(st.style.sel_bg));
                    g.stroke_round_rect(gc_rect(&rc), RADIUS, color(st.style.border), 1.0);
                    // 우측 ⌄ 셰브론(AA)
                    let cx = rc.right - 12;
                    let cy = (rc.top + rc.bottom) / 2;
                    g.polyline(
                        &[(cx - 4, cy - 2), (cx, cy + 2), (cx + 4, cy - 2)],
                        color(st.style.text),
                        1.4,
                    );
                } // GDI 텍스트 전에 Graphics 해제(HDC 혼용 규약)
                  // `…` 라벨(GDI — 텍스트 규약)
                let old = SelectObject(dc, st.font.into());
                SetBkMode(dc, TRANSPARENT);
                SetTextColor(dc, st.style.text);
                let mut w16: Vec<u16> = "…".encode_utf16().collect();
                let mut trc = RECT {
                    left: rc.left,
                    top: rc.top + 1,
                    right: rc.right - 18,
                    bottom: rc.bottom + 1,
                };
                DrawTextW(
                    dc,
                    &mut w16,
                    &mut trc,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
                SelectObject(dc, old);
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// 팝업 — hover 강조 + 클릭/Enter 실행(✓ 없음 — 액션 메뉴).
unsafe extern "system" fn pop_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let owner = HWND(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut core::ffi::c_void);
    match msg {
        0x0021 /* WM_MOUSEACTIVATE */ => LRESULT(3 /* MA_NOACTIVATE */),
        WM_MOUSEMOVE => {
            if let Some(st) = state(owner).as_mut() {
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                let rh = row_h(owner, st);
                let hit = ((y - 3).max(0) / rh.max(1)) as usize;
                let sep = st.items.get(hit).is_some_and(|s| s == "-");
                if hit < st.items.len() && hit != st.hot && !sep {
                    st.hot = hit;
                    let _ = InvalidateRect(Some(hwnd), None, true);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => LRESULT(0), // 포커스 강탈 방지 — 실행은 UP
        WM_LBUTTONUP => {
            if let Some(st) = state(owner).as_mut() {
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                let rh = row_h(owner, st);
                let hit = ((y - 3).max(0) / rh.max(1)) as usize;
                if hit < st.items.len() {
                    pick(owner, st, hit);
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
                let rh = row_h(owner, st);
                let cell_of = |row: usize| -> RECT {
                    let top = rc.top + 3 + row as i32 * rh;
                    RECT {
                        left: rc.left + 3,
                        top,
                        right: rc.right - 3,
                        bottom: (top + rh).min(rc.bottom - 3),
                    }
                };
                {
                    let mut g = GdipCtx::new(dc);
                    g.stroke_round_rect(gc_rect(&rc), RADIUS, color(st.style.border), 1.0);
                    if st.hot < st.items.len() {
                        g.fill_round_rect(
                            gc_rect(&cell_of(st.hot)),
                            RADIUS - 2,
                            color(st.style.accent),
                        );
                    }
                } // GDI 텍스트 전에 Graphics 해제
                let old = SelectObject(dc, st.font.into());
                SetBkMode(dc, TRANSPARENT);
                for (i, label) in st.items.iter().enumerate() {
                    let cell = cell_of(i);
                    if cell.top >= rc.bottom - 3 {
                        break;
                    }
                    if label == "-" {
                        // 구분선(07-18) — 수평선만
                        let mid = (cell.top + cell.bottom) / 2;
                        let line = RECT {
                            left: cell.left + 6,
                            top: mid,
                            right: cell.right - 6,
                            bottom: mid + 1,
                        };
                        fill(dc, &line, st.style.border);
                        continue;
                    }
                    SetTextColor(
                        dc,
                        if i == st.hot {
                            st.style.bg
                        } else {
                            st.style.text
                        },
                    );
                    let mut w16: Vec<u16> = label.encode_utf16().collect();
                    let mut trc = RECT {
                        left: cell.left + 10,
                        top: cell.top + 1,
                        right: cell.right,
                        bottom: cell.bottom + 1,
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
