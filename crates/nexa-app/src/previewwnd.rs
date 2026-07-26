//! 독립 미리보기 창(사용자 요청 07-26 — 미리보기의 **기본 제공 뷰**).
//! 하단 도크와 같은 `lines`를 **콘솔 폰트 문자 그리드 + 스크롤**로 크게 표시 —
//! 플러그인 개발의 **기준 캔버스**(박스 드로잉·표 정렬은 이 창 기준. 도크는 축약 뷰).
//! 모덜리스·단일 인스턴스(재호출 = 내용 교체) — F3(파일 목록)에서 연다.
//! user32/gdi32만 사용(B3 임포트 게이트) — 이미지 미리보기는 도크(WIC) 담당,
//! 이 창은 텍스트 전용(호출측이 안내 1줄로 폴백).

use std::cell::Cell;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, DeleteObject, EndPaint, ExtTextOutW, GetDC, GetTextExtentPoint32W,
    InvalidateRect, ReleaseDC, SelectObject, SetBkColor, SetTextColor, CLIP_DEFAULT_PRECIS,
    DEFAULT_CHARSET, DEFAULT_QUALITY, ETO_CLIPPED, ETO_OPAQUE, FF_DONTCARE, FIXED_PITCH, FW_NORMAL,
    HBRUSH, HFONT, OUT_DEFAULT_PRECIS, PAINTSTRUCT,
};
use windows::Win32::UI::Controls::SetScrollInfo;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetWindowLongPtrW,
    GetWindowRect, LoadCursorW, RegisterClassW, SetForegroundWindow, SetWindowLongPtrW,
    SetWindowTextW, ShowWindow, GWLP_USERDATA, IDC_ARROW, SB_VERT, SCROLLINFO, SIF_PAGE, SIF_POS,
    SIF_RANGE, SW_SHOWNORMAL, WINDOW_EX_STYLE, WM_CLOSE, WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN,
    WM_MOUSEWHEEL, WM_PAINT, WM_SIZE, WM_VSCROLL, WNDCLASSW, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
    WS_VSCROLL,
};

const CLASS: PCWSTR = w!("NexaPreviewWnd");

struct PvState {
    font: HFONT,
    lines: Vec<Vec<u16>>,
    line_h: i32,
    /// 첫 가시 라인(세로 스크롤 위치).
    top: i32,
    dark: bool,
}

thread_local! {
    /// 단일 인스턴스 창 핸들(0 = 없음) — 재호출 시 내용 교체·전면.
    static OPEN: Cell<isize> = const { Cell::new(0) };
}

/// 테마색(BGR COLORREF) — nexa-gui Theme 다크/라이트 토큰과 동일 값.
fn colors(dark: bool) -> (COLORREF, COLORREF) {
    if dark {
        (COLORREF(0x00211C19), COLORREF(0x00E0DAD6)) // panel_bg 0x191C21 · text 0xD6DAE0
    } else {
        (COLORREF(0x00FFFFFF), COLORREF(0x00261F1B)) // panel_bg 0xFFFFFF · text 0x1B1F26
    }
}

static REGISTER: std::sync::Once = std::sync::Once::new();

unsafe fn ensure_class() {
    REGISTER.call_once(|| {
        let wc = WNDCLASSW {
            lpszClassName: CLASS,
            lpfnWndProc: Some(pv_proc),
            hInstance: windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
                .unwrap_or_default()
                .into(),
            hbrBackground: HBRUSH(std::ptr::null_mut()), // 배경은 WM_PAINT가 불투명 도장
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hIcon: crate::icon::load(32).unwrap_or_default(),
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);
    });
}

/// 콘솔 폰트 생성 — 설정 `term_font` 1순위 패밀리 + 크기(DPI 반영·FIXED_PITCH 힌트).
unsafe fn make_mono(hwnd: HWND, family: &str, size_pt: i32) -> HFONT {
    let dpi = GetDpiForWindow(hwnd).max(96);
    let h = -((size_pt.clamp(8, 32) * dpi as i32) / 72);
    let first = family.split(',').next().unwrap_or("Consolas").trim();
    let face = windows::core::HSTRING::from(first);
    CreateFontW(
        h,
        0,
        0,
        0,
        FW_NORMAL.0 as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        DEFAULT_QUALITY,
        (FIXED_PITCH.0 | FF_DONTCARE.0) as u32,
        PCWSTR(face.as_ptr()),
    )
}

unsafe fn visible_rows(hwnd: HWND, st: &PvState) -> i32 {
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    ((rc.bottom - rc.top) / st.line_h.max(1)).max(1)
}

unsafe fn clamp_scroll(hwnd: HWND, st: &mut PvState) {
    let vis = visible_rows(hwnd, st);
    let max_top = (st.lines.len() as i32 - vis).max(0);
    st.top = st.top.clamp(0, max_top);
    let si = SCROLLINFO {
        cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
        fMask: SIF_PAGE | SIF_POS | SIF_RANGE,
        nMin: 0,
        nMax: (st.lines.len() as i32 - 1).max(0),
        nPage: vis as u32,
        nPos: st.top,
        ..Default::default()
    };
    let _ = SetScrollInfo(hwnd, SB_VERT, &si, true);
}

unsafe fn scroll_to(hwnd: HWND, st: &mut PvState, top: i32) {
    let before = st.top;
    st.top = top;
    clamp_scroll(hwnd, st);
    if st.top != before {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

unsafe extern "system" fn pv_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut PvState;
    match msg {
        WM_ERASEBKGND => LRESULT(1), // 전면 도장(깜빡임 방지)
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            if !state.is_null() {
                let st = &*state;
                let (bg, fg) = colors(st.dark);
                let old = SelectObject(hdc, st.font.into());
                SetBkColor(hdc, bg);
                SetTextColor(hdc, fg);
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                let pad = 8;
                let vis = visible_rows(hwnd, st);
                let mut y = pad / 2;
                for i in st.top..(st.top + vis + 1).min(st.lines.len() as i32) {
                    let row = RECT {
                        left: rc.left,
                        top: y,
                        right: rc.right,
                        bottom: y + st.line_h,
                    };
                    let text = &st.lines[i as usize];
                    let _ = ExtTextOutW(
                        hdc,
                        pad,
                        y,
                        ETO_OPAQUE | ETO_CLIPPED,
                        Some(&row),
                        PCWSTR(text.as_ptr()),
                        (text.len() - 1) as u32, // NUL 종료 제외
                        None,
                    );
                    y += st.line_h;
                }
                // 잔여 배경(마지막 라인 아래 + 상단 패드)
                for r in [
                    RECT {
                        left: rc.left,
                        top: 0,
                        right: rc.right,
                        bottom: pad / 2,
                    },
                    RECT {
                        left: rc.left,
                        top: y,
                        right: rc.right,
                        bottom: rc.bottom,
                    },
                ] {
                    if r.bottom > r.top {
                        SetBkColor(hdc, bg);
                        let _ = ExtTextOutW(hdc, 0, 0, ETO_OPAQUE, Some(&r), w!(""), 0, None);
                    }
                }
                SelectObject(hdc, old);
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_SIZE => {
            if !state.is_null() {
                clamp_scroll(hwnd, &mut *state);
            }
            LRESULT(0)
        }
        WM_VSCROLL => {
            if !state.is_null() {
                let st = &mut *state;
                let vis = visible_rows(hwnd, st);
                let code = (wparam.0 & 0xFFFF) as u32;
                let pos = (wparam.0 >> 16) as i32;
                let top = match code {
                    0 => st.top - 1,        // SB_LINEUP
                    1 => st.top + 1,        // SB_LINEDOWN
                    2 => st.top - vis,      // SB_PAGEUP
                    3 => st.top + vis,      // SB_PAGEDOWN
                    4 | 5 => pos,           // SB_THUMB*
                    6 => 0,                 // SB_TOP
                    7 => i32::MAX,          // SB_BOTTOM
                    _ => st.top,
                };
                scroll_to(hwnd, st, top);
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            if !state.is_null() {
                let st = &mut *state;
                let delta = ((wparam.0 >> 16) as i16) as i32;
                scroll_to(hwnd, st, st.top - delta / 40); // 120 = 3행
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if !state.is_null() {
                let st = &mut *state;
                let vis = visible_rows(hwnd, st);
                match wparam.0 as u32 {
                    0x1B => {
                        let _ = DestroyWindow(hwnd); // ESC
                    }
                    0x26 => scroll_to(hwnd, st, st.top - 1),        // ↑
                    0x28 => scroll_to(hwnd, st, st.top + 1),        // ↓
                    0x21 => scroll_to(hwnd, st, st.top - vis),      // PgUp
                    0x22 => scroll_to(hwnd, st, st.top + vis),      // PgDn
                    0x24 => scroll_to(hwnd, st, 0),                 // Home
                    0x23 => scroll_to(hwnd, st, i32::MAX),          // End
                    _ => {}
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            if !state.is_null() {
                let st = Box::from_raw(state);
                let _ = DeleteObject(st.font.into());
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            OPEN.with(|c| c.set(0));
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// lines → UTF-16(NUL 종료) 변환(탭 4칸 — 그리드 규약).
fn to_wide(lines: Vec<String>) -> Vec<Vec<u16>> {
    lines
        .into_iter()
        .map(|l| {
            l.replace('\t', "    ")
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect()
        })
        .collect()
}

/// 독립 미리보기 창 열기/갱신(모덜리스 — 단일 인스턴스: 이미 열려 있으면 내용 교체).
/// `mono` = (설정 term_font, 크기 pt) — 플러그인 기준 캔버스는 콘솔 폰트 그리드.
pub unsafe fn show(owner: HWND, title: &str, lines: Vec<String>, mono: (&str, i32), dark: bool) {
    ensure_class();
    let title_w = windows::core::HSTRING::from(format!("{title} — Preview"));
    let existing = OPEN.with(|c| c.get());
    if existing != 0 {
        let hwnd = HWND(existing as *mut core::ffi::c_void);
        let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut PvState;
        if !state.is_null() {
            let st = &mut *state;
            st.lines = to_wide(lines);
            st.top = 0;
            st.dark = dark;
            clamp_scroll(hwnd, st);
            let _ = SetWindowTextW(hwnd, PCWSTR(title_w.as_ptr()));
            let _ = InvalidateRect(Some(hwnd), None, false);
            let _ = SetForegroundWindow(hwnd);
            return;
        }
    }
    // 소유자 중앙·80×32 셀 근사 크기(이후 리사이즈 자유)
    let (mut w, mut h) = (900, 640);
    let (mut x, mut y) = (120, 120);
    let mut rc = RECT::default();
    if GetWindowRect(owner, &mut rc).is_ok() {
        w = ((rc.right - rc.left) * 3 / 4).clamp(480, 1400);
        h = ((rc.bottom - rc.top) * 3 / 4).clamp(360, 1000);
        x = rc.left + ((rc.right - rc.left) - w) / 2;
        y = rc.top + ((rc.bottom - rc.top) - h) / 2;
    }
    let Ok(hwnd) = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        CLASS,
        PCWSTR(title_w.as_ptr()),
        WS_OVERLAPPEDWINDOW | WS_VISIBLE | WS_VSCROLL,
        x,
        y,
        w,
        h,
        Some(owner), // 소유 모덜리스(주 창 위 — 입력은 차단 안 함)
        None,
        None,
        None,
    ) else {
        return;
    };
    let font = make_mono(hwnd, mono.0, mono.1);
    // 행 높이 실측(폰트 기준)
    let line_h = {
        let hdc = GetDC(Some(hwnd));
        let old = SelectObject(hdc, font.into());
        let mut sz = SIZE::default();
        let probe: Vec<u16> = "Ag".encode_utf16().collect();
        let _ = GetTextExtentPoint32W(hdc, &probe, &mut sz);
        SelectObject(hdc, old);
        ReleaseDC(Some(hwnd), hdc);
        (sz.cy).max(12) + 2
    };
    let mut state = Box::new(PvState {
        font,
        lines: to_wide(lines),
        line_h,
        top: 0,
        dark,
    });
    clamp_scroll(hwnd, &mut state);
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
    OPEN.with(|c| c.set(hwnd.0 as isize));
    let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
}
