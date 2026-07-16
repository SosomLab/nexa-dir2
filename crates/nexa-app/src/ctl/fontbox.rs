//! fontbox — **글꼴 입력** 커스텀 컨트롤(사용자 요청 07-16, ctl 2호 — WT 글꼴 피커 참조).
//!
//! 구성: 자식 EDIT(입력) + **드롭다운 목록**(설치 글꼴 — 각 항목을 **그 글꼴로 렌더**,
//! 스크롤·마우스 hover·키보드 ↑/↓/PgUp/PgDn/Enter/Esc). 입력 중 목록은 접두 매칭
//! 위치로 자동 이동(커서 옆 HUD는 사용자 확정으로 제거 — 에디트가 곧 입력 표시).
//!
//! 선택 반영 규칙(사용자 확정 — 쉼표 = 폴백 체인 규약):
//! - 마지막 `,` 뒤 조각이 선택 글꼴의 **접두사**(= 검색 중 입력)면 그 조각을 교체.
//! - 아니면(완결된 이름/빈 입력 아님) `, ` 구분자로 **뒤에 추가**.
//! - 빈 입력 = 선택 글꼴 그대로.
//!
//! 호스트 계약: WM_SETTEXT/GETTEXT/GETTEXTLENGTH 위임 · 내용 변경 =
//! `WM_COMMAND(id, EN_CHANGE)` · **확정**(선택/포커스 이탈) = `WM_COMMAND(id,
//! EN_KILLFOCUS)` 재발행 — prefs의 기존 즉시 적용 배선(0x0200)을 그대로 탄다.

use std::collections::HashMap;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint,
    EnumFontFamiliesExW, FillRect, GetDC, GetSysColorBrush, GetTextMetricsW, InvalidateRect,
    ReleaseDC, SelectObject, SetBkMode, SetTextColor, CLIP_DEFAULT_PRECIS, COLOR_WINDOW,
    DEFAULT_CHARSET, DEFAULT_QUALITY, DT_LEFT, DT_SINGLELINE, DT_VCENTER, FF_DONTCARE, HFONT,
    LOGFONTW, OUT_DEFAULT_PRECIS, PAINTSTRUCT, TEXTMETRICW, TRANSPARENT,
};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetCursorPos,
    GetDlgCtrlID, GetParent, GetWindowLongPtrW, MoveWindow, RegisterClassW, SendMessageW,
    SetWindowLongPtrW, SetWindowPos, ES_AUTOHSCROLL, GWLP_USERDATA, GWLP_WNDPROC, HMENU,
    HWND_TOPMOST, IDC_ARROW, SWP_NOACTIVATE, SWP_SHOWWINDOW, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_CHAR, WM_COMMAND, WM_CREATE, WM_CTLCOLOREDIT, WM_DESTROY, WM_DRAWITEM, WM_GETTEXT,
    WM_GETTEXTLENGTH, WM_KEYDOWN, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    WM_PAINT, WM_SETFOCUS, WM_SETFONT, WM_SETTEXT, WM_SIZE, WNDCLASSW, WS_CHILD, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
};

const EDIT_ID: u32 = 1;
const LIST_ID: u32 = 2;
const PAD_X: i32 = 4;
const EN_CHANGE: u32 = 0x0300;
const EN_KILLFOCUS: u32 = 0x0200;
const BORDER_BGR: u32 = 0x00AC_A8A4;
const SEL_BGR: u32 = 0x00EC_E7E4;
/// 드롭다운 최대 가시 행.
const DROP_ROWS: i32 = 12;

// ── 설치 글꼴 열거(프로세스 1회 — EnumFontFamiliesExW, '@'(세로쓰기) 제외) ──

static FAMILIES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();

unsafe extern "system" fn enum_cb(
    lf: *const LOGFONTW,
    _tm: *const TEXTMETRICW,
    _ty: u32,
    lparam: LPARAM,
) -> i32 {
    let out = &mut *(lparam.0 as *mut Vec<String>);
    let lf = &*lf;
    let end = lf
        .lfFaceName
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(lf.lfFaceName.len());
    let name = String::from_utf16_lossy(&lf.lfFaceName[..end]);
    if !name.starts_with('@') {
        out.push(name);
    }
    1
}

fn families() -> &'static [String] {
    FAMILIES.get_or_init(|| unsafe {
        let mut v: Vec<String> = Vec::new();
        let dc = GetDC(None);
        let lf = LOGFONTW {
            lfCharSet: DEFAULT_CHARSET,
            ..Default::default()
        };
        EnumFontFamiliesExW(dc, &lf, Some(enum_cb), LPARAM(&mut v as *mut _ as isize), 0);
        ReleaseDC(None, dc);
        v.sort_by_key(|a| a.to_lowercase());
        v.dedup();
        v
    })
}

// ── 상태 ─────────────────────────────────────────────────────────

struct FbState {
    edit: HWND,
    font: HFONT,
    /// 열린 드롭다운(컨테이너 popup) — 없으면 None.
    drop: Option<HWND>,
    /// 에디트 원래 wndproc(서브클래스 복원용).
    edit_proc: isize,
}

/// 드롭다운 컨테이너 상태 — 항목별 미리보기 HFONT 캐시(지연 생성·파괴 시 해제).
struct DropState {
    list: HWND,
    owner: HWND, // fontbox 컨트롤
    item_fonts: HashMap<usize, HFONT>,
    row_h: i32,
    list_proc: isize,
}

static REGISTER: std::sync::Once = std::sync::Once::new();
const CLASS: PCWSTR = w!("NexaFontBox");
const DROP_CLASS: PCWSTR = w!("NexaFontDrop");

/// 글꼴 입력 컨트롤 생성 — searchbox와 동일한 드롭인 텍스트 계약.
pub unsafe fn create(parent: HWND, x: i32, y: i32, w: i32, h: i32, id: u32, font: HFONT) -> HWND {
    REGISTER.call_once(|| {
        for (class, p) in [
            (CLASS, fb_proc as unsafe extern "system" fn(_, _, _, _) -> _),
            (DROP_CLASS, drop_proc),
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
    SendMessageW(
        hwnd,
        WM_SETFONT,
        Some(WPARAM(font.0 as usize)),
        Some(LPARAM(1)),
    );
    hwnd
}

unsafe fn state(hwnd: HWND) -> *mut FbState {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut FbState
}

unsafe fn font_height(hwnd: HWND, font: HFONT) -> i32 {
    let dc = GetDC(Some(hwnd));
    let old = SelectObject(dc, font.into());
    let mut tm = TEXTMETRICW::default();
    let _ = GetTextMetricsW(dc, &mut tm);
    SelectObject(dc, old);
    ReleaseDC(Some(hwnd), dc);
    tm.tmHeight.max(12)
}

/// 내부 EDIT 재배치 — 세로 중앙(searchbox 규약 공유).
unsafe fn layout(hwnd: HWND, st: &FbState) {
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let (cw, ch) = (rc.right, rc.bottom);
    let eh = (font_height(hwnd, st.font) + 4).min((ch - 4).max(8));
    let _ = MoveWindow(
        st.edit,
        PAD_X,
        (ch - eh) / 2,
        (cw - PAD_X * 2).max(10),
        eh,
        true,
    );
}

unsafe fn edit_text(st: &FbState) -> String {
    let len = SendMessageW(st.edit, WM_GETTEXTLENGTH, None, None).0;
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; len as usize + 1];
    let n = SendMessageW(
        st.edit,
        WM_GETTEXT,
        Some(WPARAM(buf.len())),
        Some(LPARAM(buf.as_mut_ptr() as isize)),
    )
    .0;
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// 현재 검색 조각 = 마지막 `,` 뒤(없으면 전체) 트림.
fn segment(text: &str) -> &str {
    text.rsplit(',').next().unwrap_or("").trim()
}

/// 선택 반영(사용자 확정 규칙) — 새 전체 텍스트를 돌려준다.
fn apply_pick(text: &str, pick: &str) -> String {
    let seg = segment(text);
    let is_prefix = !seg.is_empty() && pick.to_lowercase().starts_with(&seg.to_lowercase());
    match text.rfind(',') {
        Some(i) => {
            if is_prefix || seg.is_empty() {
                format!("{}, {}", text[..i].trim_end(), pick) // 조각 교체/구분자 뒤 추가
            } else {
                format!("{}, {}", text.trim_end(), pick) // 완결 이름 뒤에 체인 추가
            }
        }
        None => {
            if is_prefix || seg.is_empty() {
                pick.to_string() // 검색 중 입력 교체(빈 입력 포함)
            } else {
                format!("{}, {}", text.trim_end(), pick) // 구분자 없음 = 뒤에 붙이기
            }
        }
    }
}

/// 부모에 통지 재발행(컨트롤 id 기준).
unsafe fn notify_parent(hwnd: HWND, code: u32) {
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

// ── 드롭다운 열기/닫기/탐색 ──────────────────────────────────────

const TIMER_OUTSIDE: usize = 1;

unsafe fn open_drop(hwnd: HWND, st: &mut FbState) {
    if st.drop.is_some() {
        return;
    }
    let _ = windows::Win32::UI::WindowsAndMessaging::SetTimer(Some(hwnd), TIMER_OUTSIDE, 60, None);
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let mut pt = POINT { x: 0, y: rc.bottom };
    let _ = windows::Win32::Graphics::Gdi::ClientToScreen(hwnd, &mut pt);
    let row_h = font_height(hwnd, st.font) + 10;
    let h = row_h * DROP_ROWS + 4;
    let drop = CreateWindowExW(
        WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
        DROP_CLASS,
        w!(""),
        WS_POPUP,
        pt.x,
        pt.y + 1,
        (rc.right - rc.left).max(180),
        h,
        Some(hwnd), // 소유 관계(최상위로 승격) — 통지는 컨테이너가 중계
        None,
        None,
        None,
    )
    .unwrap_or_default();
    // 컨테이너 상태(리스트는 WM_CREATE에서) — owner 연결
    let ds = GetWindowLongPtrW(drop, GWLP_USERDATA) as *mut DropState;
    if let Some(ds) = ds.as_mut() {
        ds.owner = hwnd;
        ds.row_h = row_h;
        SendMessageW(
            ds.list,
            0x01A0, // LB_SETITEMHEIGHT
            Some(WPARAM(0)),
            Some(LPARAM(row_h as isize)),
        );
        SendMessageW(
            ds.list,
            WM_SETFONT,
            Some(WPARAM(st.font.0 as usize)),
            Some(LPARAM(0)),
        );
        for name in families() {
            let w16: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
            SendMessageW(
                ds.list,
                0x0180, // LB_ADDSTRING
                None,
                Some(LPARAM(w16.as_ptr() as isize)),
            );
        }
    }
    let _ = SetWindowPos(
        drop,
        Some(HWND_TOPMOST),
        pt.x,
        pt.y + 1,
        (rc.right - rc.left).max(180),
        h,
        SWP_SHOWWINDOW | SWP_NOACTIVATE,
    );
    st.drop = Some(drop);
    sync_match(st); // 현재 조각으로 초기 위치
}

unsafe fn close_drop(st: &mut FbState) {
    if let Some(d) = st.drop.take() {
        if let Ok(owner) = GetParent(d) {
            let _ = windows::Win32::UI::WindowsAndMessaging::KillTimer(Some(owner), TIMER_OUTSIDE);
        }
        let _ = DestroyWindow(d);
    }
}

unsafe fn drop_list(st: &FbState) -> Option<HWND> {
    let d = st.drop?;
    let ds = GetWindowLongPtrW(d, GWLP_USERDATA) as *mut DropState;
    ds.as_ref().map(|s| s.list)
}

/// 현재 조각과 **접두 매칭**되는 첫 글꼴로 목록 이동(타입어헤드) + HUD 갱신.
unsafe fn sync_match(st: &mut FbState) {
    let Some(list) = drop_list(st) else { return };
    let seg = segment(&edit_text(st)).to_lowercase();
    if !seg.is_empty() {
        if let Some(i) = families()
            .iter()
            .position(|f| f.to_lowercase().starts_with(&seg))
        {
            SendMessageW(list, 0x0186 /* LB_SETCURSEL */, Some(WPARAM(i)), None);
            SendMessageW(
                list,
                0x0197, /* LB_SETTOPINDEX */
                Some(WPARAM(i.saturating_sub(2))),
                None,
            );
        }
    }
}

/// 목록 현재 선택을 확정 — 조각 규칙 반영 + 통지(EN_CHANGE·EN_KILLFOCUS=적용) + 닫기.
unsafe fn commit_sel(hwnd: HWND, st: &mut FbState) {
    let Some(list) = drop_list(st) else { return };
    let sel = SendMessageW(list, 0x0188 /* LB_GETCURSEL */, None, None).0;
    if sel < 0 {
        close_drop(st);
        return;
    }
    let Some(pick) = families().get(sel as usize) else {
        close_drop(st);
        return;
    };
    let new_text = apply_pick(&edit_text(st), pick);
    let w16: Vec<u16> = new_text.encode_utf16().chain(std::iter::once(0)).collect();
    SendMessageW(
        st.edit,
        WM_SETTEXT,
        None,
        Some(LPARAM(w16.as_ptr() as isize)),
    );
    close_drop(st);
    notify_parent(hwnd, EN_KILLFOCUS); // 확정 = 즉시 적용 경로(prefs 0x0200)
    let _ = SetFocus(Some(st.edit));
}

// ── 내부 EDIT 서브클래스(user32 원시 — 키 라우팅) ─────────────────

unsafe extern "system" fn edit_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let ctl = GetParent(hwnd).unwrap_or_default();
    let stp = state(ctl);
    let Some(st) = stp.as_mut() else {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    };
    let orig = st.edit_proc;
    match msg {
        WM_KEYDOWN => {
            let vk = wparam.0 as u32;
            if st.drop.is_some() {
                if let Some(list) = drop_list(st) {
                    let count = families().len() as isize;
                    let cur = SendMessageW(list, 0x0188, None, None).0.max(0);
                    let target = match vk {
                        0x26 => Some(cur - 1),                  // ↑
                        0x28 => Some(cur + 1),                  // ↓
                        0x21 => Some(cur - DROP_ROWS as isize), // PgUp
                        0x22 => Some(cur + DROP_ROWS as isize), // PgDn
                        _ => None,
                    };
                    if let Some(t) = target {
                        let t = t.clamp(0, count - 1) as usize;
                        SendMessageW(list, 0x0186, Some(WPARAM(t)), None);
                        SendMessageW(list, 0x0197, Some(WPARAM(t.saturating_sub(2))), None);
                        return LRESULT(0);
                    }
                    if vk == 0x0D {
                        commit_sel(ctl, st); // Enter = 선택 확정
                        return LRESULT(0);
                    }
                    if vk == 0x1B {
                        close_drop(st); // Esc = 닫기
                        return LRESULT(0);
                    }
                }
            } else if matches!(wparam.0 as u32, 0x28) {
                open_drop(ctl, st); // 닫힘 상태 ↓ = 열기
                return LRESULT(0);
            }
        }
        WM_CHAR if wparam.0 == 0x0D || wparam.0 == 0x1B => {
            if st.drop.is_none() && wparam.0 == 0x0D {
                // 닫힘 상태 Enter는 호스트(모달 펌프)가 적용 — 비프만 억제
            }
            return LRESULT(0);
        }
        WM_LBUTTONDOWN => {
            if st.drop.is_none() {
                open_drop(ctl, st); // 사용자 확정: 창 클릭 = 목록 표시
            }
        }
        WM_KILLFOCUS => {
            // 포커스 이탈 = 닫기 + 적용 통지(기존 EDIT kill-focus 계약 유지).
            // 단, 새 포커스가 **드롭다운 내부**면 유지(방어 — 목록 조작 중 오폐쇄 방지).
            let new_focus = HWND(wparam.0 as *mut core::ffi::c_void);
            let into_drop = st
                .drop
                .is_some_and(|d| new_focus == d || GetParent(new_focus).ok() == Some(d));
            if !into_drop {
                close_drop(st);
                notify_parent(ctl, EN_KILLFOCUS);
            }
        }
        WM_DESTROY => {
            let orig = st.edit_proc;
            SetWindowLongPtrW(hwnd, GWLP_WNDPROC, orig);
            return CallWindowProcW(
                Some(std::mem::transmute::<
                    isize,
                    unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
                >(orig)),
                hwnd,
                msg,
                wparam,
                lparam,
            );
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

// ── 컨트롤 본체 ──────────────────────────────────────────────────

unsafe extern "system" fn fb_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
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
            let orig = SetWindowLongPtrW(edit, GWLP_WNDPROC, edit_proc as *const () as isize);
            let st = Box::new(FbState {
                edit,
                font: HFONT::default(),
                drop: None,
                edit_proc: orig,
            });
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(st) as isize);
            LRESULT(0)
        }
        WM_DESTROY => {
            let p = state(hwnd);
            if let Some(st) = p.as_mut() {
                close_drop(st);
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
        m if m == WM_SETTEXT || m == WM_GETTEXT || m == WM_GETTEXTLENGTH || m == 0x1501 => {
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
            let notify = (wparam.0 >> 16) as u32;
            let src = (wparam.0 & 0xFFFF) as u32;
            if src == EDIT_ID && notify == EN_CHANGE {
                if let Some(st) = state(hwnd).as_mut() {
                    if st.drop.is_some() {
                        sync_match(st); // 타입어헤드 — 매칭 위치 이동 + HUD
                    }
                }
                notify_parent(hwnd, EN_CHANGE);
            }
            LRESULT(0)
        }
        0x0113 /* WM_TIMER */ if wparam.0 == TIMER_OUTSIDE => {
            // 바깥 클릭 = 닫기(사용자 확정 QA 07-16) — 좌버튼이 눌린 순간 커서가
            // 컨트롤/팝업 밖이면 닫는다(빈 영역·타이틀바·타 앱 전부 커버).
            if let Some(st) = state(hwnd).as_mut() {
                if let Some(drop) = st.drop {
                    let pressed = windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(
                        0x01, // VK_LBUTTON
                    ) < 0;
                    if pressed {
                        let mut pt = POINT::default();
                        let _ = GetCursorPos(&mut pt);
                        let inside = |w: HWND| -> bool {
                            let mut rc = RECT::default();
                            if windows::Win32::UI::WindowsAndMessaging::GetWindowRect(w, &mut rc)
                                .is_err()
                            {
                                return false;
                            }
                            pt.x >= rc.left && pt.x < rc.right && pt.y >= rc.top && pt.y < rc.bottom
                        };
                        if !inside(drop) && !inside(hwnd) {
                            close_drop(st);
                        }
                    }
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            // 에디트 밖(테두리 여백) 클릭도 목록 토글 — "창을 클릭하면 목록"(사용자 확정)
            if let Some(st) = state(hwnd).as_mut() {
                if st.drop.is_some() {
                    close_drop(st);
                } else {
                    open_drop(hwnd, st);
                }
                let _ = SetFocus(Some(st.edit));
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
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
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        m if m == WM_CTLCOLOREDIT => DefWindowProcW(hwnd, msg, wparam, lparam),
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// ── 드롭다운 컨테이너 + 목록(오너드로 — 각 항목을 그 글꼴로) ─────

unsafe fn drop_state(hwnd: HWND) -> *mut DropState {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut DropState
}

unsafe extern "system" fn list_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let container = GetParent(hwnd).unwrap_or_default();
    let dsp = drop_state(container);
    let Some(ds) = dsp.as_ref() else {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    };
    let orig = ds.list_proc;
    match msg {
        WM_MOUSEMOVE => {
            // hover = 선택 이동(이미지 규약 — 마우스 탐색)
            let r = SendMessageW(hwnd, 0x01A9 /* LB_ITEMFROMPOINT */, None, Some(lparam)).0;
            if (r >> 16) == 0 {
                SendMessageW(hwnd, 0x0186, Some(WPARAM((r & 0xFFFF) as usize)), None);
            }
        }
        WM_LBUTTONDOWN => {
            // [QA 07-16 진범] LISTBOX 기본 DOWN이 **SetFocus(자신)** → 에디트
            // WM_KILLFOCUS → 커밋 전에 팝업 파괴. DOWN을 삼키고 선택만 직접 반영
            // (포커스는 에디트 유지 — 확정은 UP에서).
            let r = SendMessageW(hwnd, 0x01A9, None, Some(lparam)).0;
            if (r >> 16) == 0 {
                SendMessageW(hwnd, 0x0186, Some(WPARAM((r & 0xFFFF) as usize)), None);
            }
            return LRESULT(0);
        }
        WM_LBUTTONUP => {
            // 클릭 = 확정(마우스 선택) — CURSEL 의존 대신 **UP 좌표로 항목 직접 계산**
            // (QA 07-16: 캡처/트래킹 상태에 따라 CURSEL이 비어 미반영되던 결함)
            let r = SendMessageW(hwnd, 0x01A9 /* LB_ITEMFROMPOINT */, None, Some(lparam)).0;
            if (r >> 16) == 0 {
                SendMessageW(hwnd, 0x0186, Some(WPARAM((r & 0xFFFF) as usize)), None);
            }
            if let Some(st) = state(ds.owner).as_mut() {
                commit_sel(ds.owner, st);
            }
            return LRESULT(0);
        }
        0x0021 /* WM_MOUSEACTIVATE */ => {
            return LRESULT(3); // MA_NOACTIVATE — 에디트 포커스 유지
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

unsafe extern "system" fn drop_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        // [QA 07-16 진범] 클릭 활성화 차단 — 기본(MA_ACTIVATE)이면 프리페스가
        // 비활성화되며 EDIT WM_KILLFOCUS → 팝업이 **클릭 처리 전에 파괴**돼
        // 목록 클릭이 반영되지 않았다. NOACTIVATE = 포커스 유지 + 클릭 정상 전달.
        0x0021 /* WM_MOUSEACTIVATE */ => LRESULT(3 /* MA_NOACTIVATE */),
        WM_CREATE => {
            let list = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("LISTBOX"),
                w!(""),
                WS_CHILD
                    | WS_VISIBLE
                    | WS_VSCROLL
                    | WINDOW_STYLE(
                        0x0010 /* LBS_OWNERDRAWFIXED */
                            | 0x0040 /* LBS_HASSTRINGS */
                            | 0x0100, /* LBS_NOINTEGRALHEIGHT — 하단 회색 띠(QA) 방지 */
                    ),
                1,
                1,
                10,
                10,
                Some(hwnd),
                Some(HMENU(LIST_ID as usize as *mut core::ffi::c_void)),
                None,
                None,
            )
            .unwrap_or_default();
            let orig = SetWindowLongPtrW(list, GWLP_WNDPROC, list_proc as *const () as isize);
            let ds = Box::new(DropState {
                list,
                owner: HWND::default(),
                item_fonts: HashMap::new(),
                row_h: 22,
                list_proc: orig,
            });
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(ds) as isize);
            LRESULT(0)
        }
        WM_SIZE => {
            if let Some(ds) = drop_state(hwnd).as_ref() {
                let (w, h) = (
                    (lparam.0 & 0xFFFF) as i32,
                    ((lparam.0 >> 16) & 0xFFFF) as i32,
                );
                let _ = MoveWindow(ds.list, 1, 1, w - 2, h - 2, true);
            }
            LRESULT(0)
        }
        WM_DRAWITEM => {
            let dis = &*(lparam.0 as *const windows::Win32::UI::Controls::DRAWITEMSTRUCT);
            if let Some(ds) = drop_state(hwnd).as_mut() {
                draw_font_item(ds, dis);
                return LRESULT(1);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            let p = drop_state(hwnd);
            if let Some(ds) = p.as_mut() {
                for (_, f) in ds.item_fonts.drain() {
                    let _ = DeleteObject(f.into());
                }
                SetWindowLongPtrW(ds.list, GWLP_WNDPROC, ds.list_proc);
            }
            if !p.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(p));
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let border = CreateSolidBrush(COLORREF(BORDER_BGR));
            FillRect(dc, &rc, border);
            let _ = DeleteObject(border.into());
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// 항목 오너드로 — **그 글꼴로 미리보기**(이미지 규약). HFONT는 항목별 지연 생성·캐시.
unsafe fn draw_font_item(ds: &mut DropState, dis: &windows::Win32::UI::Controls::DRAWITEMSTRUCT) {
    let idx = dis.itemID as usize;
    let Some(name) = families().get(idx) else {
        FillRect(dis.hDC, &dis.rcItem, GetSysColorBrush(COLOR_WINDOW));
        return;
    };
    let selected = (dis.itemState.0 & 0x0001) != 0; // ODS_SELECTED
    if selected {
        let b = CreateSolidBrush(COLORREF(SEL_BGR));
        FillRect(dis.hDC, &dis.rcItem, b);
        let _ = DeleteObject(b.into());
    } else {
        FillRect(dis.hDC, &dis.rcItem, GetSysColorBrush(COLOR_WINDOW));
    }
    let font = *ds.item_fonts.entry(idx).or_insert_with(|| {
        let w16: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        CreateFontW(
            -(ds.row_h - 8),
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            DEFAULT_QUALITY,
            FF_DONTCARE.0.into(),
            PCWSTR(w16.as_ptr()),
        )
    });
    let old = SelectObject(dis.hDC, font.into());
    SetBkMode(dis.hDC, TRANSPARENT);
    SetTextColor(dis.hDC, COLORREF(0x0020_2020));
    let mut w16: Vec<u16> = name.encode_utf16().collect();
    let mut rc = dis.rcItem;
    rc.left += 8;
    DrawTextW(
        dis.hDC,
        &mut w16,
        &mut rc,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER,
    );
    SelectObject(dis.hDC, old);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_pick_segment_rules() {
        // 검색 중 접두사 = 조각 교체
        assert_eq!(apply_pick("D2C", "D2Coding"), "D2Coding");
        assert_eq!(
            apply_pick("D2Coding, JetB", "JetBrainsMono Nerd Font"),
            "D2Coding, JetBrainsMono Nerd Font"
        );
        // 구분자 뒤 빈 조각 = 뒤에 추가
        assert_eq!(apply_pick("D2Coding,", "Consolas"), "D2Coding, Consolas");
        assert_eq!(apply_pick("D2Coding, ", "Consolas"), "D2Coding, Consolas");
        // 구분자 없음 + 완결 이름 = 뒤에 붙이기(사용자 확정)
        assert_eq!(apply_pick("D2Coding", "Consolas"), "D2Coding, Consolas");
        // 빈 입력 = 그대로
        assert_eq!(apply_pick("", "Consolas"), "Consolas");
    }
}
