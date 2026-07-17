//! searchbox — **내장 ✕ 지우개 검색 입력** 커스텀 컨트롤(사용자 요청 07-16).
//!
//! 자기완결 설계: 컨트롤(이 창 클래스)이 테두리·✕ 영역을 자기 클라이언트에 직접 그리고,
//! 실입력은 **자식 EDIT**(무테두리)가 담당한다. ✕가 겹친 형제가 아니라 컨트롤 소유
//! 영역이므로 z-순서/덮어 그리기 문제가 구조적으로 없다(QA 07-16 z-순서 진범의 근본 해소).
//!
//! 동작 규약(사용자 확정):
//! - 입력이 있을 때만 ✕ 표시, 클릭 = **전체 지우기 + 포커스 복귀**(EN_CHANGE 경유 통지).
//! - 실시간 검색 전제 — 검색 버튼 없음.
//! - 텍스트는 **세로 중앙 정렬**: 위/아래 여백 동일, 컨트롤이 커져도 동일 유지
//!   (내부 EDIT를 글꼴 높이로 재배치 — WM_SIZE/WM_SETFONT에서 재계산).
//!
//! 호스트 계약(드롭인): `WM_SETTEXT`/`WM_GETTEXT`/`WM_GETTEXTLENGTH`는 내부 EDIT로
//! 위임되고, 내용 변경은 **호스트에 `WM_COMMAND(MAKEWPARAM(컨트롤 id, EN_CHANGE))`**
//! (lparam = 컨트롤 HWND)로 전달된다 — 기존 EDIT 배선을 그대로 재사용할 수 있다.
//! `EM_SETCUEBANNER`도 위임. 라이트 고정(설정 창 규약 — 필요 시 테마 인자화는 후속).

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect, GetSysColorBrush,
    InvalidateRect, SelectObject, SetBkMode, SetTextColor, COLOR_WINDOW, DT_CENTER, DT_SINGLELINE,
    DT_VCENTER, HFONT, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetClientRect, GetDlgCtrlID, GetWindowLongPtrW, MoveWindow,
    RegisterClassW, SendMessageW, SetWindowLongPtrW, ES_AUTOHSCROLL, GWLP_USERDATA, HMENU,
    IDC_ARROW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_COMMAND, WM_CREATE, WM_CTLCOLOREDIT, WM_DESTROY,
    WM_GETTEXT, WM_GETTEXTLENGTH, WM_LBUTTONDOWN, WM_PAINT, WM_SETFOCUS, WM_SETFONT, WM_SETTEXT,
    WM_SIZE, WNDCLASSW, WS_CHILD, WS_TABSTOP, WS_VISIBLE,
};

/// 내부 EDIT의 컨트롤-로컬 id(외부와 무관 — 통지는 컨트롤 id로 재발행).
const EDIT_ID: u32 = 1;
/// ✕ 영역 폭(px, 96dpi 기준 — 컨트롤 오른쪽 끝).
const CLEAR_W: i32 = 20;
/// 좌측 텍스트 여백.
const PAD_X: i32 = 4;
/// EN_CHANGE(부모 재발행에 그대로 사용).
const EN_CHANGE: u32 = 0x0300;
/// 테두리 회색(설정 창 라이트 고정 팔레트).
const BORDER_BGR: u32 = 0x00AC_A8A4;
/// ✕ 회색(설정 창 설명 텍스트와 동일).
const GLYPH_BGR: u32 = 0x0078_6E68;

struct SbState {
    edit: HWND,
    font: HFONT,
}

static REGISTER: std::sync::Once = std::sync::Once::new();
const CLASS: PCWSTR = w!("Nexa.NxSearchBox");

/// 검색박스 생성 — 반환 HWND에 `WM_SETTEXT`/`WM_GETTEXT`/`EM_SETCUEBANNER`를 그대로
/// 쓸 수 있다. 내용 변경 시 부모가 `WM_COMMAND(id, EN_CHANGE)`를 받는다.
pub unsafe fn create(parent: HWND, x: i32, y: i32, w: i32, h: i32, id: u32, font: HFONT) -> HWND {
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
    SendMessageW(
        hwnd,
        WM_SETFONT,
        Some(WPARAM(font.0 as usize)),
        Some(LPARAM(1)),
    );
    hwnd
}

unsafe fn state(hwnd: HWND) -> *mut SbState {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut SbState
}

/// 내부 EDIT 재배치 — **세로 중앙**(사용자 확정: 위/아래 여백 동일, 높이가 커져도 유지).
/// EDIT 높이 = 글꼴 높이 + 2(클리핑 방지) — 컨트롤 높이와 무관하게 글꼴 기준.
unsafe fn layout(hwnd: HWND, st: &SbState) {
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let (cw, ch) = (rc.right - rc.left, rc.bottom - rc.top);
    // 한글 상단 붙음(사용자 QA 07-16): 단일행 EDIT는 텍스트를 위 정렬한다 —
    // EDIT 자체를 글꼴 높이(+한글 폴백 글리프 여유 4px)로 줄여 컨트롤 중앙에 배치
    // = 위/아래 여백 동일, 컨트롤이 커져도 유지.
    let font_h = font_height(hwnd, st.font);
    let eh = (font_h + 4).min((ch - 4).max(8));
    let ey = (ch - eh) / 2; // 상하 여백 동일(중앙 정렬)
    let ew = (cw - PAD_X - CLEAR_W).max(10);
    let _ = MoveWindow(st.edit, PAD_X, ey, ew, eh, true);
}

/// 글꼴 픽셀 높이(tmHeight) — 중앙 정렬 기준.
unsafe fn font_height(hwnd: HWND, font: HFONT) -> i32 {
    use windows::Win32::Graphics::Gdi::{GetDC, GetTextMetricsW, ReleaseDC, TEXTMETRICW};
    let dc = GetDC(Some(hwnd));
    let old = SelectObject(dc, font.into());
    let mut tm = TEXTMETRICW::default();
    let _ = GetTextMetricsW(dc, &mut tm);
    SelectObject(dc, old);
    ReleaseDC(Some(hwnd), dc);
    tm.tmHeight.max(12)
}

/// ✕ 히트 영역(클라이언트 좌표).
unsafe fn clear_rect(hwnd: HWND) -> RECT {
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    RECT {
        left: rc.right - CLEAR_W,
        top: rc.top,
        right: rc.right,
        bottom: rc.bottom,
    }
}

unsafe fn has_text(st: &SbState) -> bool {
    SendMessageW(st.edit, WM_GETTEXTLENGTH, None, None).0 > 0
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            // 무테두리 자식 EDIT — 테두리는 컨트롤이 그린다(중앙 정렬 배치와 분리)
            let edit = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("EDIT"),
                w!(""),
                WS_CHILD | WS_VISIBLE | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
                PAD_X,
                2,
                10,
                10,
                Some(hwnd),
                Some(HMENU(EDIT_ID as usize as *mut core::ffi::c_void)),
                None,
                None,
            )
            .unwrap_or_default();
            let st = Box::new(SbState {
                edit,
                font: HFONT::default(),
            });
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(st) as isize);
            LRESULT(0)
        }
        WM_DESTROY => {
            let p = state(hwnd);
            if !p.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(p));
            }
            LRESULT(0)
        }
        WM_SETFONT => {
            let p = state(hwnd);
            if let Some(st) = p.as_mut() {
                st.font = HFONT(wparam.0 as *mut core::ffi::c_void);
                SendMessageW(st.edit, WM_SETFONT, Some(wparam), Some(lparam));
                layout(hwnd, st);
            }
            LRESULT(0)
        }
        WM_SIZE => {
            if let Some(st) = state(hwnd).as_ref() {
                layout(hwnd, st); // 높이가 커져도 상하 여백 동일(중앙)
            }
            let _ = InvalidateRect(Some(hwnd), None, true);
            LRESULT(0)
        }
        // 텍스트 API 위임(드롭인 계약) — set_text/get_text가 컨트롤에 그대로 동작
        m if m == WM_SETTEXT || m == WM_GETTEXT || m == WM_GETTEXTLENGTH || m == 0x1501 => {
            match state(hwnd).as_ref() {
                Some(st) => SendMessageW(st.edit, m, Some(wparam), Some(lparam)),
                None => LRESULT(0),
            }
        }
        WM_SETFOCUS => {
            if let Some(st) = state(hwnd).as_ref() {
                let _ = SetFocus(Some(st.edit)); // 탭 이동 착지 = 내부 에디트
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            // 내부 EDIT 통지 → ✕ 영역 재도장 + **컨트롤 id로 부모에 재발행**(EN_CHANGE)
            let notify = (wparam.0 >> 16) as u32;
            let src = (wparam.0 & 0xFFFF) as u32;
            if src == EDIT_ID && notify == EN_CHANGE {
                let rc = clear_rect(hwnd);
                let _ = InvalidateRect(Some(hwnd), Some(&rc), true);
                if let Ok(parent) = windows::Win32::UI::WindowsAndMessaging::GetParent(hwnd) {
                    let id = GetDlgCtrlID(hwnd) as u32;
                    SendMessageW(
                        parent,
                        WM_COMMAND,
                        Some(WPARAM(((EN_CHANGE as usize) << 16) | id as usize)),
                        Some(LPARAM(hwnd.0 as isize)),
                    );
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            // ✕ 클릭 = 전체 지우기 + 포커스 복귀(사용자 확정). 에디트 밖(=컨트롤 영역)
            // 클릭은 전부 여기로 오므로 z-순서 이슈가 없다.
            let (x, y) = (
                (lparam.0 & 0xFFFF) as i16 as i32,
                ((lparam.0 >> 16) & 0xFFFF) as i16 as i32,
            );
            if let Some(st) = state(hwnd).as_ref() {
                let rc = clear_rect(hwnd);
                if has_text(st) && x >= rc.left && x < rc.right && y >= rc.top && y < rc.bottom {
                    let _ =
                        SendMessageW(st.edit, WM_SETTEXT, None, Some(LPARAM(w!("").0 as isize)));
                }
                let _ = SetFocus(Some(st.edit));
            }
            LRESULT(0)
        }
        m if m == WM_CTLCOLOREDIT => DefWindowProcW(hwnd, msg, wparam, lparam),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            // 배경 + 1px 테두리(라이트 고정)
            FillRect(dc, &rc, GetSysColorBrush(COLOR_WINDOW));
            let border = CreateSolidBrush(COLORREF(BORDER_BGR));
            for (l, t, r, b) in [
                (rc.left, rc.top, rc.right, rc.top + 1),
                (rc.left, rc.bottom - 1, rc.right, rc.bottom),
                (rc.left, rc.top, rc.left + 1, rc.bottom),
                (rc.right - 1, rc.top, rc.right, rc.bottom),
            ] {
                let e = RECT {
                    left: l,
                    top: t,
                    right: r,
                    bottom: b,
                };
                FillRect(dc, &e, border);
            }
            let _ = DeleteObject(border.into());
            // ✕ — 입력이 있을 때만(사용자 확정), 세로 중앙
            if let Some(st) = state(hwnd).as_ref() {
                if has_text(st) {
                    let old = SelectObject(dc, st.font.into());
                    SetBkMode(dc, TRANSPARENT);
                    SetTextColor(dc, COLORREF(GLYPH_BGR));
                    let mut glyph: Vec<u16> = "✕".encode_utf16().collect();
                    let mut zone = clear_rect(hwnd);
                    DrawTextW(
                        dc,
                        &mut glyph,
                        &mut zone,
                        DT_SINGLELINE | DT_CENTER | DT_VCENTER,
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
