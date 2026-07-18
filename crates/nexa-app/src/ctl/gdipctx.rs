//! gdipctx — **GDI+ DrawCtx 백엔드**(07-17 raster QA — 사용자 지시:
//! "GDI+를 쓰되 DrawCtx 백엔드로만, ctl에서 직접 호출 금지").
//!
//! **이 모듈이 코드베이스의 유일한 GDI+ 접점**이다. 컨트롤/위젯은
//! [`nexa_gui::DrawCtx`]의 AA 도형 프리미티브(fill_ellipse/fill_round_rect/
//! stroke_round_rect/polyline)만 호출한다 — 래스터라이저 교체(예: Direct2D
//! 승격) 시 이 모듈만 바꾼다. gdiplus.dll = OS 인박스(B3 게이트 통과)·
//! 텍스트는 GDI/DirectWrite 유지(사용자 확정 — GDI+ 텍스트 부적합).
//!
//! [`GdipCtx`]는 `BeginPaint` HDC를 감싸는 일회성 컨텍스트: 도형 = GDI+
//! (SmoothingMode AA — DC의 기존 픽셀과 블렌드), fill_rect/텍스트 = GDI
//! (ctl은 텍스트를 DrawTextW로 직접 그림 — 여기 텍스트 구현은 최소 폴백).

use std::sync::OnceLock;

use nexa_gui::{Color, DrawCtx, Rect};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::HDC;
use windows::Win32::Graphics::GdiPlus::{
    FillModeAlternate, GdipAddPathArc, GdipClosePathFigure, GdipCreateFromHDC, GdipCreatePath,
    GdipCreatePen1, GdipCreateSolidFill, GdipDeleteBrush, GdipDeleteGraphics, GdipDeletePath,
    GdipDeletePen, GdipDisposeImage, GdipDrawImageRectI, GdipDrawLines, GdipDrawPath,
    GdipFillEllipse, GdipFillPath, GdipGetImageHeight, GdipGetImageWidth,
    GdipLoadImageFromStream, GdipSetInterpolationMode,
    GdipSetPenEndCap, GdipSetPenLineJoin, GdipSetPenStartCap, GdipSetSmoothingMode, GdiplusStartup,
    GdiplusStartupInput, GdiplusStartupOutput, GpBrush, GpGraphics, GpImage, GpPath, GpPen,
    GpSolidFill, InterpolationModeHighQualityBicubic, LineCapRound, LineJoinRound, PointF,
    SmoothingModeAntiAlias, Unit,
};

/// COLORREF → [`Color`](DrawCtx 인자 변환 — ctl 편의).
pub(crate) fn color(c: COLORREF) -> Color {
    Color {
        r: (c.0 & 0xFF) as u8,
        g: ((c.0 >> 8) & 0xFF) as u8,
        b: ((c.0 >> 16) & 0xFF) as u8,
    }
}

/// RECT → [`Rect`](DrawCtx 인자 변환 — ctl 편의).
pub(crate) fn rect(rc: &windows::Win32::Foundation::RECT) -> Rect {
    Rect::new(rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top)
}

fn c_argb(c: Color) -> u32 {
    0xFF00_0000 | ((c.r as u32) << 16) | ((c.g as u32) << 8) | c.b as u32
}

/// PNG 등 이미지 바이트를 GDI+ 이미지로 디코드(알파 보존 — 07-18 리네임 ± 버튼).
/// 실패 시 널. 반환 이미지는 [`dispose_image`]로 해제(호스트가 1회 디코드·캐시).
///
/// # Safety
/// `bytes`는 유효한 이미지(PNG/BMP 등) 인코딩이어야 하며, GDI+ 초기화 전제.
pub(crate) unsafe fn decode_png(bytes: &[u8]) -> *mut GpImage {
    if !ensure_startup() {
        return std::ptr::null_mut();
    }
    // SHCreateMemStream = 바이트 복사 스트림(shlwapi 인박스) → 호출 후 bytes 해제 무관
    let Some(stream) = windows::Win32::UI::Shell::SHCreateMemStream(Some(bytes)) else {
        return std::ptr::null_mut();
    };
    let mut img: *mut GpImage = std::ptr::null_mut();
    let _ = GdipLoadImageFromStream(&stream, &mut img);
    img
}

/// [`decode_png`] 이미지의 원본 픽셀 크기(폭, 높이). 널/실패 = (0, 0).
///
/// # Safety
/// `img`는 [`decode_png`]가 반환한 유효/널 포인터여야 한다.
pub(crate) unsafe fn image_size(img: *mut GpImage) -> (i32, i32) {
    if img.is_null() {
        return (0, 0);
    }
    let (mut w, mut h) = (0u32, 0u32);
    let _ = GdipGetImageWidth(img, &mut w);
    let _ = GdipGetImageHeight(img, &mut h);
    (w as i32, h as i32)
}

/// [`decode_png`] 이미지 해제(호스트 컨트롤 파괴 시).
///
/// # Safety
/// `img`는 [`decode_png`]가 반환한 유효/널 포인터여야 한다.
pub(crate) unsafe fn dispose_image(img: *mut GpImage) {
    if !img.is_null() {
        let _ = GdipDisposeImage(img);
    }
}

/// GDI+ 1회 초기화(프로세스 수명 — 종료 시 OS 회수).
unsafe fn ensure_startup() -> bool {
    static TOKEN: OnceLock<usize> = OnceLock::new();
    *TOKEN.get_or_init(|| {
        let input = GdiplusStartupInput {
            GdiplusVersion: 1,
            ..Default::default()
        };
        let mut token = 0usize;
        let mut output = GdiplusStartupOutput::default();
        let st = GdiplusStartup(&mut token, &input, &mut output);
        if st.0 == 0 {
            token
        } else {
            0
        }
    }) != 0
}

/// HDC를 감싸는 AA 도형 컨텍스트 — [`DrawCtx`] 구현.
pub struct GdipCtx {
    dc: HDC,
    g: *mut GpGraphics,
}

impl GdipCtx {
    /// `BeginPaint`/메모리 DC 위에 생성. GDI+ 초기화 실패 시에도 안전(도형 no-op).
    pub unsafe fn new(dc: HDC) -> Self {
        let mut g: *mut GpGraphics = std::ptr::null_mut();
        if ensure_startup() {
            let _ = GdipCreateFromHDC(dc, &mut g);
            if !g.is_null() {
                let _ = GdipSetSmoothingMode(g, SmoothingModeAntiAlias);
            }
        }
        GdipCtx { dc, g }
    }

    /// [`decode_png`] 이미지를 `rect`에 고품질 스케일로 그린다(알파 블렌드 —
    /// 리네임 ± 버튼 raster). 초기화·널 이미지 시 no-op.
    ///
    /// # Safety
    /// `img`는 [`decode_png`] 반환 포인터(현재 컨텍스트와 같은 GDI+ 세션).
    pub(crate) unsafe fn draw_image(&mut self, img: *mut GpImage, rect: Rect) {
        if self.g.is_null() || img.is_null() {
            return;
        }
        let _ = GdipSetInterpolationMode(self.g, InterpolationModeHighQualityBicubic);
        let _ = GdipDrawImageRectI(self.g, img, rect.x, rect.y, rect.w, rect.h);
    }

    unsafe fn brush(&self, color: Color) -> *mut GpSolidFill {
        let mut b: *mut GpSolidFill = std::ptr::null_mut();
        let _ = GdipCreateSolidFill(c_argb(color), &mut b);
        b
    }

    unsafe fn pen(&self, color: Color, width: f32) -> *mut GpPen {
        let mut p: *mut GpPen = std::ptr::null_mut();
        let _ = GdipCreatePen1(c_argb(color), width, Unit(2 /* UnitPixel */), &mut p);
        if !p.is_null() {
            let _ = GdipSetPenStartCap(p, LineCapRound);
            let _ = GdipSetPenEndCap(p, LineCapRound);
            let _ = GdipSetPenLineJoin(p, LineJoinRound);
        }
        p
    }

    /// 라운드 사각 경로(4 모서리 호 + 닫기) — fill/stroke 공용.
    unsafe fn round_path(&self, r: Rect, radius: i32, inset: f32) -> *mut GpPath {
        let mut path: *mut GpPath = std::ptr::null_mut();
        if GdipCreatePath(FillModeAlternate, &mut path).0 != 0 {
            return std::ptr::null_mut();
        }
        let (l, t) = (r.x as f32 + inset, r.y as f32 + inset);
        let (w, h) = (r.w as f32 - inset * 2.0, r.h as f32 - inset * 2.0);
        let d = (radius as f32 * 2.0).min(w).min(h);
        let _ = GdipAddPathArc(path, l, t, d, d, 180.0, 90.0);
        let _ = GdipAddPathArc(path, l + w - d, t, d, d, 270.0, 90.0);
        let _ = GdipAddPathArc(path, l + w - d, t + h - d, d, d, 0.0, 90.0);
        let _ = GdipAddPathArc(path, l, t + h - d, d, d, 90.0, 90.0);
        let _ = GdipClosePathFigure(path);
        path
    }
}

impl Drop for GdipCtx {
    fn drop(&mut self) {
        if !self.g.is_null() {
            unsafe {
                let _ = GdipDeleteGraphics(self.g);
            }
        }
    }
}

impl DrawCtx for GdipCtx {
    fn fill_rect(&mut self, rect: Rect, color: Color) {
        // 직사각형은 GDI로 충분(AA 불필요 — 축 정렬)
        unsafe {
            let rc = windows::Win32::Foundation::RECT {
                left: rect.x,
                top: rect.y,
                right: rect.right(),
                bottom: rect.y + rect.h,
            };
            super::style::fill(
                self.dc,
                &rc,
                COLORREF(((color.b as u32) << 16) | ((color.g as u32) << 8) | color.r as u32),
            );
        }
    }

    fn text_opaque(&mut self, _x: i32, _y: i32, _clip: Rect, _text: &str, _fg: Color, _bg: Color) {
        // 도형 전용 백엔드 — ctl 텍스트는 GDI DrawTextW 직접(계약 명세)
    }

    fn text_width(&mut self, _text: &str) -> i32 {
        0
    }

    fn fill_ellipse(&mut self, rect: Rect, color: Color) {
        if self.g.is_null() {
            return;
        }
        unsafe {
            let b = self.brush(color);
            if !b.is_null() {
                let _ = GdipFillEllipse(
                    self.g,
                    b as *mut GpBrush,
                    rect.x as f32,
                    rect.y as f32,
                    rect.w as f32,
                    rect.h as f32,
                );
                let _ = GdipDeleteBrush(b as *mut GpBrush);
            }
        }
    }

    fn fill_round_rect(&mut self, rect: Rect, radius: i32, color: Color) {
        if self.g.is_null() {
            return;
        }
        unsafe {
            let path = self.round_path(rect, radius, 0.0);
            let b = self.brush(color);
            if !path.is_null() && !b.is_null() {
                let _ = GdipFillPath(self.g, b as *mut GpBrush, path);
            }
            if !b.is_null() {
                let _ = GdipDeleteBrush(b as *mut GpBrush);
            }
            if !path.is_null() {
                let _ = GdipDeletePath(path);
            }
        }
    }

    fn stroke_round_rect(&mut self, rect: Rect, radius: i32, color: Color, width: f32) {
        if self.g.is_null() {
            return;
        }
        unsafe {
            // 펜 중심선이 rect 안쪽에 오도록 폭/2 + 0.5(픽셀 경계) 인셋
            let path = self.round_path(rect, radius, width / 2.0 + 0.5);
            let p = self.pen(color, width);
            if !path.is_null() && !p.is_null() {
                let _ = GdipDrawPath(self.g, p, path);
            }
            if !p.is_null() {
                let _ = GdipDeletePen(p);
            }
            if !path.is_null() {
                let _ = GdipDeletePath(path);
            }
        }
    }

    fn polyline(&mut self, pts: &[(i32, i32)], color: Color, width: f32) {
        if self.g.is_null() || pts.len() < 2 {
            return;
        }
        unsafe {
            let p = self.pen(color, width);
            if !p.is_null() {
                let fpts: Vec<PointF> = pts
                    .iter()
                    .map(|&(x, y)| PointF {
                        X: x as f32,
                        Y: y as f32,
                    })
                    .collect();
                let _ = GdipDrawLines(self.g, p, fpts.as_ptr(), fpts.len() as i32);
                let _ = GdipDeletePen(p);
            }
        }
    }
}
