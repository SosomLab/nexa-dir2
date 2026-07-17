//! groupcard — **그룹 카드 컨테이너** 커스텀 컨트롤(ctl 6호 — PF Batch Rename의
//! 동작별 카드 대응. 라이브러리 추상화 — 앱 비결합·comctl32 비의존).
//!
//! ## 계약(판매용 명세)
//! - 생성: [`create`] — 폭 + **타이틀/본문 높이 각각 지정**(총 높이 = 합) +
//!   `corner`(0 = 각진 사각형, >0 = 라운드 반경 px — 창 리전 클립) + [`Style`].
//! - 타이틀 = **윈도우 텍스트 표준 위임**(WM_SETTEXT/WM_GETTEXT — 변경 시 재도장).
//! - 본문 = 호스트가 자식 컨트롤을 카드의 자식으로 배치 — [`body_rect`]가
//!   본문 클라이언트 좌표를 돌려준다(제목 밴드 아래 전체).
//! - **타이틀 영역도 자식 배치 가능**(사용자 확정 07-17 — PF처럼 타이틀 자리에
//!   콤보 선택기): [`title_rect`] 기준 배치, 창 텍스트를 비우면 라벨 생략.
//! - **자식 통지 투과**: WM_COMMAND·WM_NOTIFY·WM_DRAWITEM·WM_MEASUREITEM·
//!   WM_CTLCOLOR* 를 카드 부모로 그대로 전달 — 호스트는 카드 유무와 무관하게
//!   자식 컨트롤 id로 처리한다(중첩 투명성).
//! - 그리기: 타이틀 밴드 = sel_bg + 하단 1px border 구분선 + text 라벨(좌측 여백),
//!   본문 = bg, 외곽 = border 1px(라운드 = RoundRect 외곽선).
//! - 조회: [`GC_GETTITLEH`](WM_USER+80 — 타이틀 높이 px 반환).

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreatePen, CreateRoundRectRgn, DeleteObject, DrawTextW, EndPaint, GetStockObject,
    InvalidateRect, SelectObject, SetBkMode, SetTextColor, SetWindowRgn, DT_LEFT, DT_SINGLELINE,
    DT_VCENTER, HFONT, NULL_BRUSH, PAINTSTRUCT, PS_SOLID, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetClientRect, GetParent, GetWindowTextW, RegisterClassW,
    SendMessageW, SetWindowLongPtrW, GWLP_USERDATA, HMENU, IDC_ARROW, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_COMMAND, WM_CREATE, WM_CTLCOLOREDIT, WM_CTLCOLORSTATIC, WM_DESTROY,
    WM_DRAWITEM, WM_MEASUREITEM, WM_NOTIFY, WM_PAINT, WM_SETFONT, WM_SETTEXT, WM_SIZE, WNDCLASSW,
    WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_VISIBLE,
};

use super::style::{fill, Style};

/// 타이틀 높이 조회(반환 = px).
pub const GC_GETTITLEH: u32 = 0x0400 + 80;

/// 생성 옵션 — 모양·영역 크기(계약: 총 높이 = title_h + body_h).
#[derive(Clone, Copy)]
pub struct GroupCardOpts {
    /// 0 = 각진 사각형 · >0 = 라운드 반경(px).
    pub corner: i32,
    /// 타이틀 밴드 높이(px).
    pub title_h: i32,
    /// 본문 영역 높이(px).
    pub body_h: i32,
}

struct GcState {
    title_h: i32,
    corner: i32,
    font: HFONT,
    style: Style,
}

static REGISTER: std::sync::Once = std::sync::Once::new();
const CLASS: PCWSTR = w!("Nexa.NxGroupCard");

/// 그룹 카드 생성 — 타이틀은 윈도우 텍스트로 위임(복사 소유).
#[allow(clippy::too_many_arguments)] // Win32 create 계열 관례
pub unsafe fn create(
    parent: HWND,
    x: i32,
    y: i32,
    w: i32,
    id: u32,
    font: HFONT,
    title: &str,
    opts: GroupCardOpts,
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
    let h = opts.title_h + opts.body_h;
    let t16 = windows::core::HSTRING::from(title);
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE(0x0001_0000), // WS_EX_CONTROLPARENT — Tab 내비가 카드 안으로(07-18)
        CLASS,
        PCWSTR(t16.as_ptr()),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(WS_CLIPCHILDREN.0 | WS_CLIPSIBLINGS.0),
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
    let st = Box::new(GcState {
        title_h: opts.title_h,
        corner: opts.corner,
        font,
        style,
    });
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(st) as isize);
    apply_region(hwnd, opts.corner, w, h);
    SendMessageW(
        hwnd,
        WM_SETFONT,
        Some(WPARAM(font.0 as usize)),
        Some(LPARAM(1)),
    );
    hwnd
}

/// 타이틀 밴드 영역(클라이언트 좌표) — 타이틀 자리에 자식(콤보 등)을 배치할 때
/// 기준. 창 텍스트를 비우면("" — WM_SETTEXT) 텍스트 라벨은 그리지 않는다.
pub unsafe fn title_rect(hwnd: HWND) -> RECT {
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let th = SendMessageW(hwnd, GC_GETTITLEH, None, None).0 as i32;
    RECT {
        bottom: rc.top + th,
        ..rc
    }
}

/// 본문 영역(클라이언트 좌표) — 호스트의 자식 배치 기준.
pub unsafe fn body_rect(hwnd: HWND) -> RECT {
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let th = SendMessageW(hwnd, GC_GETTITLEH, None, None).0 as i32;
    RECT {
        top: rc.top + th,
        ..rc
    }
}

unsafe fn state(hwnd: HWND) -> *mut GcState {
    windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut GcState
}

/// 라운드 선택 시 창 리전 클립(각진 = 리전 해제). 크기 변경마다 재적용.
unsafe fn apply_region(hwnd: HWND, corner: i32, w: i32, h: i32) {
    if corner > 0 {
        let rgn = CreateRoundRectRgn(0, 0, w + 1, h + 1, corner * 2, corner * 2);
        let _ = SetWindowRgn(hwnd, Some(rgn), true); // 소유권 이전 — 삭제 불필요
    } else {
        let _ = SetWindowRgn(hwnd, None, true);
    }
}

/// 자식 통지 투과 — 카드 부모로 그대로 전달(반환값 포함: CTLCOLOR 브러시 등).
unsafe fn forward(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match GetParent(hwnd) {
        Ok(p) => SendMessageW(p, msg, Some(wparam), Some(lparam)),
        Err(_) => DefWindowProcW(hwnd, msg, wparam, lparam),
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
        WM_SETTEXT => {
            let r = DefWindowProcW(hwnd, msg, wparam, lparam);
            let _ = InvalidateRect(Some(hwnd), None, true);
            r
        }
        m if m == GC_GETTITLEH => LRESULT(state(hwnd).as_ref().map_or(0, |s| s.title_h as isize)),
        WM_SIZE => {
            if let Some(st) = state(hwnd).as_ref() {
                let w = (lparam.0 & 0xFFFF) as i16 as i32;
                let h = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                apply_region(hwnd, st.corner, w, h);
            }
            let _ = InvalidateRect(Some(hwnd), None, true);
            LRESULT(0)
        }
        // 자식 통지 투과(계약 — 중첩 투명성)
        WM_COMMAND | WM_NOTIFY | WM_DRAWITEM | WM_MEASUREITEM => forward(hwnd, msg, wparam, lparam),
        m if (WM_CTLCOLOREDIT..=WM_CTLCOLORSTATIC).contains(&m) => {
            forward(hwnd, msg, wparam, lparam)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            if let Some(st) = state(hwnd).as_ref() {
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                fill(dc, &rc, st.style.bg);
                // 타이틀 밴드 + 하단 구분선
                let band = RECT {
                    bottom: rc.top + st.title_h,
                    ..rc
                };
                fill(dc, &band, st.style.sel_bg);
                let sep = RECT {
                    top: band.bottom - 1,
                    ..band
                };
                fill(dc, &sep, st.style.border);
                // 타이틀 라벨(윈도우 텍스트)
                let mut buf = [0u16; 256];
                let n = GetWindowTextW(hwnd, &mut buf) as usize;
                if n > 0 {
                    let old = SelectObject(dc, st.font.into());
                    SetBkMode(dc, TRANSPARENT);
                    SetTextColor(dc, st.style.text);
                    let mut trc = RECT {
                        left: band.left + 10,
                        right: band.right - 10,
                        ..band
                    };
                    DrawTextW(
                        dc,
                        &mut buf[..n],
                        &mut trc,
                        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
                    );
                    SelectObject(dc, old);
                }
                // 외곽선 — 라운드는 리전과 같은 반경의 RoundRect, 각진은 사각 프레임
                if st.corner > 0 {
                    let pen = CreatePen(PS_SOLID, 1, st.style.border);
                    let old_p = SelectObject(dc, pen.into());
                    let old_b = SelectObject(dc, GetStockObject(NULL_BRUSH));
                    let _ = windows::Win32::Graphics::Gdi::RoundRect(
                        dc,
                        rc.left,
                        rc.top,
                        rc.right,
                        rc.bottom,
                        st.corner * 2,
                        st.corner * 2,
                    );
                    SelectObject(dc, old_b);
                    SelectObject(dc, old_p);
                    let _ = DeleteObject(pen.into());
                } else {
                    super::style::frame(dc, &rc, st.style.border);
                }
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
