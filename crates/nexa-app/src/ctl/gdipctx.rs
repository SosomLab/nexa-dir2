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
    FillModeAlternate, GdipAddPathArc, GdipClosePathFigure, GdipCreateFromHDC,
    GdipCreateHICONFromBitmap, GdipCreatePath, GdipCreatePen1, GdipCreateSolidFill,
    GdipDeleteBrush, GdipDeleteGraphics, GdipDeletePath, GdipDeletePen, GdipDisposeImage,
    GdipDrawImageRectI, GdipDrawLines, GdipDrawPath, GdipFillEllipse, GdipFillPath,
    GdipGetImageHeight, GdipGetImageWidth, GdipLoadImageFromStream, GdipSetInterpolationMode,
    GdipSetPenEndCap, GdipSetPenLineJoin, GdipSetPenStartCap, GdipSetSmoothingMode, GdiplusStartup,
    GdiplusStartupInput, GdiplusStartupOutput, GpBitmap, GpBrush, GpGraphics, GpImage, GpPath,
    GpPen, GpSolidFill, InterpolationModeHighQualityBicubic, LineCapRound, LineJoinRound, PointF,
    SmoothingModeAntiAlias, Unit,
};
use windows::Win32::UI::WindowsAndMessaging::HICON;

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

/// [SVG 서브셋 문서](crate::svg::Doc) → `HICON`(07-18 사용자 요청 "svg 방식도
/// 적용" — GDI+ 오프스크린 32bpp ARGB 비트맵에 스트로크 렌더 후 아이콘 변환).
/// `px` = 정사각 픽셀 크기(viewBox → px 균등 스케일) · `argb` = 잉크 색.
/// 반환 핸들은 호출자가 `DestroyIcon`으로 해제.
///
/// # Safety
/// GDI+ 초기화 전제(내부에서 보장). 실패 시 `None`(오류 격리 — 아이콘 공백).
pub(crate) unsafe fn svg_to_hicon(doc: &crate::svg::Doc, px: i32, argb: u32) -> Option<HICON> {
    use crate::svg::{Op, Seg};
    use windows::Win32::Graphics::GdiPlus::{
        GdipAddPathBezier, GdipAddPathEllipse, GdipAddPathLine, GdipAddPathString,
        GdipCreateBitmapFromScan0, GdipCreateFontFamilyFromName, GdipCreateStringFormat,
        GdipDeleteFontFamily, GdipDeleteStringFormat, GdipGetImageGraphicsContext,
        GdipSetStringFormatAlign, GdipStartPathFigure, GpFontFamily, GpStringFormat, RectF,
        StringAlignmentCenter,
    };
    if !ensure_startup() || px <= 0 {
        return None;
    }
    const PXF_32ARGB: i32 = 0x0026_200A; // PixelFormat32bppARGB
    let mut bmp: *mut GpBitmap = std::ptr::null_mut();
    // scan0 = None → 0 초기화(투명 배경)
    if GdipCreateBitmapFromScan0(px, px, 0, PXF_32ARGB, None, &mut bmp).0 != 0 || bmp.is_null() {
        return None;
    }
    let mut g: *mut GpGraphics = std::ptr::null_mut();
    let _ = GdipGetImageGraphicsContext(bmp as *mut GpImage, &mut g);
    let mut icon = None;
    if !g.is_null() {
        let _ = GdipSetSmoothingMode(g, SmoothingModeAntiAlias);
        let (vx, vy, vw, vh) = doc.viewbox;
        let scale = (px as f32 / vw).min(px as f32 / vh);
        let sx = |x: f32| (x - vx) * scale;
        let sy = |y: f32| (y - vy) * scale;
        {
            for el in &doc.ops {
                let op = &el.op;
                // 요소 색 오버라이드(RGB) — 알파는 잉크 것 유지(비활성 흐림
                // 이 오버라이드 색에도 적용, 07-19 패널 토글 accent 선)
                let el_argb = match el.color {
                    Some(rgb) => (argb & 0xFF00_0000) | rgb,
                    None => argb,
                };
                let mut path: *mut GpPath = std::ptr::null_mut();
                if GdipCreatePath(FillModeAlternate, &mut path).0 != 0 || path.is_null() {
                    continue;
                }
                match op {
                    Op::Rect { x, y, w, h, rx } => {
                        let (l, t, w2, h2) = (sx(*x), sy(*y), w * scale, h * scale);
                        let d = (rx * scale * 2.0).clamp(0.0, w2.min(h2));
                        if d > 0.0 {
                            let _ = GdipAddPathArc(path, l, t, d, d, 180.0, 90.0);
                            let _ = GdipAddPathArc(path, l + w2 - d, t, d, d, 270.0, 90.0);
                            let _ = GdipAddPathArc(path, l + w2 - d, t + h2 - d, d, d, 0.0, 90.0);
                            let _ = GdipAddPathArc(path, l, t + h2 - d, d, d, 90.0, 90.0);
                        } else {
                            let _ = GdipAddPathLine(path, l, t, l + w2, t);
                            let _ = GdipAddPathLine(path, l + w2, t, l + w2, t + h2);
                            let _ = GdipAddPathLine(path, l + w2, t + h2, l, t + h2);
                        }
                        let _ = GdipClosePathFigure(path);
                    }
                    Op::Circle { cx, cy, r } => {
                        let d = r * scale * 2.0;
                        let _ =
                            GdipAddPathEllipse(path, sx(*cx) - r * scale, sy(*cy) - r * scale, d, d);
                    }
                    Op::Line { x1, y1, x2, y2 } => {
                        let _ = GdipAddPathLine(path, sx(*x1), sy(*y1), sx(*x2), sy(*y2));
                    }
                    Op::Polyline(pts) => {
                        for w2 in pts.windows(2) {
                            let _ = GdipAddPathLine(
                                path,
                                sx(w2[0].0),
                                sy(w2[0].1),
                                sx(w2[1].0),
                                sy(w2[1].1),
                            );
                        }
                    }
                    Op::Path(segs) => {
                        let (mut cx, mut cy) = (0.0f32, 0.0f32);
                        for seg in segs {
                            match *seg {
                                Seg::MoveTo(x, y) => {
                                    let _ = GdipStartPathFigure(path);
                                    (cx, cy) = (x, y);
                                }
                                Seg::LineTo(x, y) => {
                                    let _ =
                                        GdipAddPathLine(path, sx(cx), sy(cy), sx(x), sy(y));
                                    (cx, cy) = (x, y);
                                }
                                Seg::CurveTo([c1, c2, e]) => {
                                    let _ = GdipAddPathBezier(
                                        path,
                                        sx(cx),
                                        sy(cy),
                                        sx(c1.0),
                                        sy(c1.1),
                                        sx(c2.0),
                                        sy(c2.1),
                                        sx(e.0),
                                        sy(e.1),
                                    );
                                    (cx, cy) = (e.0, e.1);
                                }
                                Seg::Arc {
                                    cx: acx,
                                    cy: acy,
                                    r,
                                    start,
                                    sweep,
                                } => {
                                    let d = r * scale * 2.0;
                                    let _ = GdipAddPathArc(
                                        path,
                                        sx(acx - r),
                                        sy(acy - r),
                                        d,
                                        d,
                                        start,
                                        sweep,
                                    );
                                    let end = (start + sweep).to_radians();
                                    (cx, cy) = (acx + r * end.cos(), acy + r * end.sin());
                                }
                                Seg::Close => {
                                    let _ = GdipClosePathFigure(path);
                                }
                            }
                        }
                    }
                    Op::Text {
                        x,
                        y,
                        size,
                        bold,
                        middle,
                        content,
                    } => {
                        // 텍스트 아웃라인 패스(GdipAddPathString) — 항상 채움.
                        // 글꼴 = 고정 산세리프(Arial — 인박스)·`y` = 베이스라인
                        // 근사(em 위로). 가족 로드 실패 시 건너뜀(오류 격리).
                        let mut family: *mut GpFontFamily = std::ptr::null_mut();
                        let _ = GdipCreateFontFamilyFromName(
                            windows::core::w!("Arial"),
                            std::ptr::null_mut(),
                            &mut family,
                        );
                        if !family.is_null() {
                            let mut fmt: *mut GpStringFormat = std::ptr::null_mut();
                            let _ = GdipCreateStringFormat(0, 0, &mut fmt);
                            let em = size * scale;
                            let width = vw * scale;
                            let mut left = sx(*x);
                            if *middle && !fmt.is_null() {
                                let _ = GdipSetStringFormatAlign(fmt, StringAlignmentCenter);
                                left = sx(*x) - width / 2.0;
                            }
                            let rect = RectF {
                                X: left,
                                Y: sy(*y) - em,
                                Width: width,
                                Height: em * 1.6,
                            };
                            let wide = windows::core::HSTRING::from(content.as_str());
                            let _ = GdipAddPathString(
                                path,
                                windows::core::PCWSTR(wide.as_ptr()),
                                content.encode_utf16().count() as i32,
                                family,
                                if *bold { 1 /* FontStyleBold */ } else { 0 },
                                em,
                                &rect,
                                fmt,
                            );
                            if !fmt.is_null() {
                                let _ = GdipDeleteStringFormat(fmt);
                            }
                            let _ = GdipDeleteFontFamily(family);
                        }
                    }
                }
                // 채색: 요소 fill 오버라이드 > 루트 fill 모드 · 텍스트 상시 채움
                let filled = matches!(op, Op::Text { .. }) || el.fill.unwrap_or(doc.fill);
                if filled {
                    let mut b: *mut GpSolidFill = std::ptr::null_mut();
                    let _ = GdipCreateSolidFill(el_argb, &mut b);
                    if !b.is_null() {
                        let _ = GdipFillPath(g, b as *mut GpBrush, path);
                        let _ = GdipDeleteBrush(b as *mut GpBrush);
                    }
                } else {
                    let mut pen: *mut GpPen = std::ptr::null_mut();
                    let _ = GdipCreatePen1(
                        el_argb,
                        (el.width.unwrap_or(doc.stroke_width) * scale).max(1.0),
                        Unit(2 /* UnitPixel */),
                        &mut pen,
                    );
                    if !pen.is_null() {
                        let _ = GdipSetPenStartCap(pen, LineCapRound);
                        let _ = GdipSetPenEndCap(pen, LineCapRound);
                        let _ = GdipSetPenLineJoin(pen, LineJoinRound);
                        let _ = GdipDrawPath(g, pen, path);
                        let _ = GdipDeletePen(pen);
                    }
                }
                let _ = GdipDeletePath(path);
            }
        }
        let _ = GdipDeleteGraphics(g);
        let mut h = HICON::default();
        if GdipCreateHICONFromBitmap(bmp, &mut h).0 == 0 && !h.is_invalid() {
            icon = Some(h);
        }
    }
    let _ = GdipDisposeImage(bmp as *mut GpImage);
    icon
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
