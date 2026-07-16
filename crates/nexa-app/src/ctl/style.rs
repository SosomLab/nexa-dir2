//! style — ctl 공통 팔레트·그리기 유틸(라이브러리 추상화 — 사용자 확정 07-17:
//! 차후 **독립 라이브러리 판매**를 겨냥해 앱 비결합으로 설계).
//!
//! 계약: 모든 ctl 컨트롤은 색을 하드코딩하지 않고 [`Style`]을 받는다(생성 인자).
//! 기본값 = 설정 창 라이트 팔레트. 다크/커스텀은 호스트가 값만 바꿔 전달.

use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::{CreateSolidBrush, DeleteObject, FillRect, HDC};

/// ctl 팔레트(COLORREF — 0x00BBGGRR). Copy로 컨트롤 상태에 저장.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Style {
    /// 배경(입력 필드·팝업).
    pub bg: COLORREF,
    /// 1px 테두리.
    pub border: COLORREF,
    /// 본문 글자.
    pub text: COLORREF,
    /// 보조 글자(플레이스홀더·✕ 등).
    pub text_dim: COLORREF,
    /// 강조(선택 테두리·활성 세그먼트).
    pub accent: COLORREF,
    /// 선택/hover 배경.
    pub sel_bg: COLORREF,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            bg: COLORREF(0x00FF_FFFF),
            border: COLORREF(0x00AC_A8A4),
            text: COLORREF(0x0020_2020),
            text_dim: COLORREF(0x0078_6E68),
            accent: COLORREF(0x00D4_7800),
            sel_bg: COLORREF(0x00EC_E7E4),
        }
    }
}

/// 사각형 채우기(단색 브러시 1회).
pub(crate) unsafe fn fill(dc: HDC, rc: &windows::Win32::Foundation::RECT, color: COLORREF) {
    let b = CreateSolidBrush(color);
    FillRect(dc, rc, b);
    let _ = DeleteObject(b.into());
}

/// 1px 테두리.
pub(crate) unsafe fn frame(dc: HDC, rc: &windows::Win32::Foundation::RECT, color: COLORREF) {
    use windows::Win32::Foundation::RECT;
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
        fill(dc, &e, color);
    }
}

/// 글꼴 픽셀 높이(tmHeight) — 세로 중앙 배치 공용.
pub(crate) unsafe fn font_height(
    hwnd: windows::Win32::Foundation::HWND,
    font: windows::Win32::Graphics::Gdi::HFONT,
) -> i32 {
    use windows::Win32::Graphics::Gdi::{
        GetDC, GetTextMetricsW, ReleaseDC, SelectObject, TEXTMETRICW,
    };
    let dc = GetDC(Some(hwnd));
    let old = SelectObject(dc, font.into());
    let mut tm = TEXTMETRICW::default();
    let _ = GetTextMetricsW(dc, &mut tm);
    SelectObject(dc, old);
    ReleaseDC(Some(hwnd), dc);
    tm.tmHeight.max(12)
}
