//! segmented — **세그먼트 라디오** 커스텀 컨트롤(ctl 3호 — PF `AB CD | Ab Cd` /
//! `→abc | ←abc` 세그먼트 대응. 라이브러리 추상화 — 앱 비결합).
//!
//! ## 계약(판매용 명세)
//! - 생성: [`create`] — 항목 라벨 목록(컨트롤이 복사 소유)·초기 선택·[`Style`].
//! - 선택 변경(클릭·←/→ 키) 시 부모에 `WM_COMMAND(MAKEWPARAM(id, SEG_CHANGED))`
//!   (lparam = 컨트롤 HWND).
//! - 조회/설정: [`SEG_GETSEL`]/[`SEG_SETSEL`](WM_USER+50/51 — SETSEL은 통지 없음).
//! - 그리기: 균등 폭 세그먼트, 선택 = accent 배경+bg 글자, 비선택 = bg+text.
//!   폰트는 WM_SETFONT.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, DrawTextW, EndPaint, InvalidateRect, SelectObject, SetBkMode, SetTextColor,
    DT_CENTER, DT_SINGLELINE, DT_VCENTER, HFONT, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetClientRect, GetDlgCtrlID, GetParent, GetWindowLongPtrW,
    RegisterClassW, SendMessageW, SetWindowLongPtrW, GWLP_USERDATA, HMENU, IDC_ARROW,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_KEYDOWN, WM_LBUTTONDOWN,
    WM_PAINT, WM_SETFONT, WM_SIZE, WNDCLASSW, WS_CHILD, WS_TABSTOP, WS_VISIBLE,
};

use super::style::{fill, frame, Style};

/// 선택 변경 통지(WM_COMMAND HIWORD).
pub const SEG_CHANGED: u32 = 1;
/// 현재 선택 조회(반환 = 인덱스).
pub const SEG_GETSEL: u32 = 0x0400 + 50;
/// 선택 설정(wparam = 인덱스 — 통지 없음·범위 밖 무시).
pub const SEG_SETSEL: u32 = 0x0400 + 51;

struct SegState {
    items: Vec<String>,
    sel: usize,
    font: HFONT,
    style: Style,
}

static REGISTER: std::sync::Once = std::sync::Once::new();
const CLASS: PCWSTR = w!("Nexa.NxSegmented");

/// 세그먼트 라디오 생성 — `items` 라벨은 컨트롤이 복사 소유.
#[allow(clippy::too_many_arguments)] // Win32 create 계열 관례(좌표+정의)
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
    let st = Box::new(SegState {
        items: items.iter().map(|s| s.to_string()).collect(),
        sel: selected.min(items.len().saturating_sub(1)),
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

unsafe fn state(hwnd: HWND) -> *mut SegState {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut SegState
}

unsafe fn notify(hwnd: HWND) {
    if let Ok(parent) = GetParent(hwnd) {
        let id = GetDlgCtrlID(hwnd) as u32;
        SendMessageW(
            parent,
            WM_COMMAND,
            Some(WPARAM(((SEG_CHANGED as usize) << 16) | id as usize)),
            Some(LPARAM(hwnd.0 as isize)),
        );
    }
}

unsafe fn set_sel(hwnd: HWND, st: &mut SegState, sel: usize, fire: bool) {
    if sel >= st.items.len() || sel == st.sel {
        return;
    }
    st.sel = sel;
    let _ = InvalidateRect(Some(hwnd), None, true);
    if fire {
        notify(hwnd);
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
                st.font = windows::Win32::Graphics::Gdi::HFONT(wparam.0 as *mut core::ffi::c_void);
            }
            let _ = InvalidateRect(Some(hwnd), None, true);
            LRESULT(0)
        }
        m if m == SEG_GETSEL => LRESULT(state(hwnd).as_ref().map_or(0, |s| s.sel as isize)),
        m if m == SEG_SETSEL => {
            if let Some(st) = state(hwnd).as_mut() {
                set_sel(hwnd, st, wparam.0, false);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            if let Some(st) = state(hwnd).as_mut() {
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                let n = st.items.len().max(1) as i32;
                let seg_w = (rc.right - rc.left) / n;
                let hit = ((x / seg_w.max(1)) as usize).min(st.items.len() - 1);
                set_sel(hwnd, st, hit, true);
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if let Some(st) = state(hwnd).as_mut() {
                let cur = st.sel;
                match wparam.0 as u32 {
                    0x25 => set_sel(hwnd, st, cur.saturating_sub(1), true), // ←
                    0x27 => set_sel(hwnd, st, (cur + 1).min(st.items.len() - 1), true), // →
                    _ => {}
                }
            }
            LRESULT(0)
        }
        WM_SIZE => {
            let _ = InvalidateRect(Some(hwnd), None, true);
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
                let n = st.items.len().max(1) as i32;
                let seg_w = (rc.right - rc.left) / n;
                let old = SelectObject(dc, st.font.into());
                SetBkMode(dc, TRANSPARENT);
                for (i, label) in st.items.iter().enumerate() {
                    let cell = RECT {
                        left: rc.left + seg_w * i as i32,
                        top: rc.top,
                        right: if i + 1 == st.items.len() {
                            rc.right
                        } else {
                            rc.left + seg_w * (i as i32 + 1)
                        },
                        bottom: rc.bottom,
                    };
                    let selected = i == st.sel;
                    if selected {
                        let inner = RECT {
                            left: cell.left + 1,
                            top: cell.top + 1,
                            right: cell.right - 1,
                            bottom: cell.bottom - 1,
                        };
                        fill(dc, &inner, st.style.accent);
                        SetTextColor(dc, st.style.bg);
                    } else {
                        SetTextColor(dc, st.style.text);
                    }
                    let mut w16: Vec<u16> = label.encode_utf16().collect();
                    let mut trc = cell;
                    DrawTextW(
                        dc,
                        &mut w16,
                        &mut trc,
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                    );
                    // 세그먼트 경계선
                    if i + 1 < st.items.len() {
                        let sep = RECT {
                            left: cell.right,
                            top: cell.top + 2,
                            right: cell.right + 1,
                            bottom: cell.bottom - 2,
                        };
                        fill(dc, &sep, st.style.border);
                    }
                }
                SelectObject(dc, old);
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
