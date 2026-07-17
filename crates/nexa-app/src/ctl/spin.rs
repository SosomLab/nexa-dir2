//! spin — **숫자 스피너** 커스텀 컨트롤(ctl 4호 — PF Position/Start/Step 스테퍼 대응.
//! 라이브러리 추상화 — 앱 비결합).
//!
//! ## 계약(판매용 명세)
//! - 생성: [`create`] — 초기값·범위(min..=max)·[`Style`]. 내부 = 숫자 EDIT(세로 중앙,
//!   searchbox 규약) + 우측 ▲▼ 버튼 존(컨트롤 소유 영역 — z-순서 이슈 없음).
//! - 값 변경(타이핑·▲▼·↑/↓ 키) 시 부모에 `WM_COMMAND(MAKEWPARAM(id, EN_CHANGE))`.
//!   포커스 이탈 시 `EN_KILLFOCUS` 재발행(범위 클램프 확정) — 기존 EDIT 배선 호환.
//! - 조회/설정: [`SPIN_GETVAL`]/[`SPIN_SETVAL`](WM_USER+60/61) ·
//!   `WM_GETTEXT`/`WM_SETTEXT` 위임(드롭인).

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, DrawTextW, EndPaint, InvalidateRect, SelectObject, SetBkMode, SetTextColor,
    DT_CENTER, DT_SINGLELINE, DT_VCENTER, HFONT, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, CreateWindowExW, DefWindowProcW, GetClientRect, GetDlgCtrlID, GetParent,
    GetWindowLongPtrW, MoveWindow, RegisterClassW, SendMessageW, SetWindowLongPtrW, ES_AUTOHSCROLL,
    ES_NUMBER, GWLP_USERDATA, GWLP_WNDPROC, HMENU, IDC_ARROW, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_COMMAND, WM_CREATE, WM_DESTROY, WM_GETTEXT, WM_GETTEXTLENGTH, WM_KEYDOWN, WM_KILLFOCUS,
    WM_LBUTTONDOWN, WM_PAINT, WM_SETFOCUS, WM_SETFONT, WM_SETTEXT, WM_SIZE, WNDCLASSW, WS_CHILD,
    WS_TABSTOP, WS_VISIBLE,
};

use super::style::{fill, font_height, frame, Style};

/// 값 조회(반환 = 클램프된 현재 값).
pub const SPIN_GETVAL: u32 = 0x0400 + 60;
/// 값 설정(wparam = i64 as usize — 통지 없음·클램프).
pub const SPIN_SETVAL: u32 = 0x0400 + 61;

const EDIT_ID: u32 = 1;
const EN_CHANGE: u32 = 0x0300;
const EN_KILLFOCUS: u32 = 0x0200;
/// ▲▼ 버튼 존 폭.
const BTN_W: i32 = 16;

struct SpinState {
    edit: HWND,
    font: HFONT,
    style: Style,
    min: i64,
    max: i64,
    edit_proc: isize,
}

static REGISTER: std::sync::Once = std::sync::Once::new();
const CLASS: PCWSTR = w!("Nexa.NxSpin");

/// 숫자 스피너 생성.
#[allow(clippy::too_many_arguments)] // Win32 create 계열 관례
pub unsafe fn create(
    parent: HWND,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    id: u32,
    font: HFONT,
    value: i64,
    min: i64,
    max: i64,
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
    if let Some(st) = state(hwnd).as_mut() {
        st.min = min;
        st.max = max;
        st.style = style;
    }
    SendMessageW(
        hwnd,
        WM_SETFONT,
        Some(WPARAM(font.0 as usize)),
        Some(LPARAM(1)),
    );
    SendMessageW(hwnd, SPIN_SETVAL, Some(WPARAM(value as usize)), None);
    hwnd
}

unsafe fn state(hwnd: HWND) -> *mut SpinState {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut SpinState
}

unsafe fn layout(hwnd: HWND, st: &SpinState) {
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let eh = (font_height(hwnd, st.font) + 4).min((rc.bottom - 4).max(8));
    let _ = MoveWindow(
        st.edit,
        4,
        (rc.bottom - eh) / 2,
        (rc.right - 4 - BTN_W - 2).max(10),
        eh,
        true,
    );
}

unsafe fn cur_val(st: &SpinState) -> i64 {
    let len = SendMessageW(st.edit, WM_GETTEXTLENGTH, None, None).0;
    let mut buf = vec![0u16; len as usize + 1];
    let n = SendMessageW(
        st.edit,
        WM_GETTEXT,
        Some(WPARAM(buf.len())),
        Some(LPARAM(buf.as_mut_ptr() as isize)),
    )
    .0;
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
        .trim()
        .parse()
        .unwrap_or(0)
}

unsafe fn set_val(hwnd: HWND, st: &SpinState, v: i64, fire: bool) {
    let v = v.clamp(st.min, st.max);
    let w16: Vec<u16> = v
        .to_string()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    SendMessageW(
        st.edit,
        WM_SETTEXT,
        None,
        Some(LPARAM(w16.as_ptr() as isize)),
    ); // EN_CHANGE는 에디트가 발화 → 재발행 경로 공용
    if fire {
        notify(hwnd, EN_CHANGE);
    }
    let _ = InvalidateRect(Some(hwnd), None, true);
}

unsafe fn notify(hwnd: HWND, code: u32) {
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

/// 내부 EDIT 서브클래스 — ↑/↓ = 증감, 포커스 이탈 = 클램프 확정+재발행.
unsafe extern "system" fn edit_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let ctl = GetParent(hwnd).unwrap_or_default();
    let Some(st) = state(ctl).as_mut() else {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    };
    let orig = st.edit_proc;
    match msg {
        WM_KEYDOWN => match wparam.0 as u32 {
            0x26 => {
                set_val(ctl, st, cur_val(st) + 1, false);
                return LRESULT(0);
            }
            0x28 => {
                set_val(ctl, st, cur_val(st) - 1, false);
                return LRESULT(0);
            }
            _ => {}
        },
        WM_KILLFOCUS => {
            set_val(ctl, st, cur_val(st), false); // 범위 클램프 확정
            notify(ctl, EN_KILLFOCUS);
        }
        _ => {}
    }
    CallWindowProcW(
        Some(std::mem::transmute::<
            isize,
            unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
        >(orig)),
        hwnd,
        msg,
        wparam,
        lparam,
    )
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            let edit = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("EDIT"),
                w!(""),
                WS_CHILD | WS_VISIBLE | WINDOW_STYLE(ES_AUTOHSCROLL as u32 | ES_NUMBER as u32),
                4,
                2,
                10,
                10,
                Some(hwnd),
                Some(HMENU(EDIT_ID as usize as *mut core::ffi::c_void)),
                None,
                None,
            )
            .unwrap_or_default();
            let orig = SetWindowLongPtrW(edit, GWLP_WNDPROC, edit_proc as *const () as isize);
            let st = Box::new(SpinState {
                edit,
                font: HFONT::default(),
                style: Style::default(),
                min: i64::MIN,
                max: i64::MAX,
                edit_proc: orig,
            });
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(st) as isize);
            LRESULT(0)
        }
        WM_DESTROY => {
            let p = state(hwnd);
            if let Some(st) = p.as_ref() {
                SetWindowLongPtrW(st.edit, GWLP_WNDPROC, st.edit_proc);
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
                SendMessageW(st.edit, WM_SETFONT, Some(wparam), Some(lparam));
                layout(hwnd, st);
            }
            LRESULT(0)
        }
        WM_SIZE => {
            if let Some(st) = state(hwnd).as_ref() {
                layout(hwnd, st);
            }
            let _ = InvalidateRect(Some(hwnd), None, true);
            LRESULT(0)
        }
        m if m == SPIN_GETVAL => LRESULT(
            state(hwnd)
                .as_ref()
                .map_or(0, |st| cur_val(st).clamp(st.min, st.max)) as isize,
        ),
        m if m == SPIN_SETVAL => {
            if let Some(st) = state(hwnd).as_ref() {
                set_val(hwnd, st, wparam.0 as i64, false);
            }
            LRESULT(0)
        }
        m if m == WM_SETTEXT || m == WM_GETTEXT || m == WM_GETTEXTLENGTH => {
            match state(hwnd).as_ref() {
                Some(st) => SendMessageW(st.edit, m, Some(wparam), Some(lparam)),
                None => LRESULT(0),
            }
        }
        WM_SETFOCUS => {
            if let Some(st) = state(hwnd).as_ref() {
                let _ = SetFocus(Some(st.edit));
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            // 내부 EDIT EN_CHANGE → 컨트롤 id로 재발행(드롭인 계약)
            let notify_code = (wparam.0 >> 16) as u32;
            if (wparam.0 & 0xFFFF) as u32 == EDIT_ID && notify_code == EN_CHANGE {
                notify(hwnd, EN_CHANGE);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            if let Some(st) = state(hwnd).as_mut() {
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                if x >= rc.right - BTN_W {
                    let up = y < rc.bottom / 2; // 상단 ▲ / 하단 ▼
                    let delta = if up { 1 } else { -1 };
                    set_val(hwnd, st, cur_val(st) + delta, false);
                }
                let _ = SetFocus(Some(st.edit));
            }
            LRESULT(0)
        }
        m if m == windows::Win32::UI::WindowsAndMessaging::WM_CTLCOLOREDIT => {
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            if let Some(st) = state(hwnd).as_ref() {
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                fill(dc, &rc, st.style.bg);
                frame(dc, &rc, st.style.border);
                // ▲▼ 버튼 존(우측 — 컨트롤 소유 영역)
                let bx = rc.right - BTN_W;
                let sep = RECT {
                    left: bx,
                    top: rc.top + 1,
                    right: bx + 1,
                    bottom: rc.bottom - 1,
                };
                fill(dc, &sep, st.style.border);
                let old = SelectObject(dc, st.font.into());
                SetBkMode(dc, TRANSPARENT);
                SetTextColor(dc, st.style.text_dim);
                for (glyph, top, bottom) in [
                    ("▲", rc.top, rc.bottom / 2),
                    ("▼", rc.bottom / 2, rc.bottom),
                ] {
                    let mut w16: Vec<u16> = glyph.encode_utf16().collect();
                    let mut zone = RECT {
                        left: bx + 1,
                        top,
                        right: rc.right - 1,
                        bottom,
                    };
                    DrawTextW(
                        dc,
                        &mut w16,
                        &mut zone,
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                    );
                }
                SelectObject(dc, old);
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
