//! DirectWrite GDI interop 백엔드 — ADR-0002 채택 구현(docs/07).
//! `IDWriteBitmapRenderTarget`(자체 메모리 DC = 더블 버퍼)에 텍스트 레이아웃을 그리고
//! 창 DC로 BitBlt — GPU 스왑체인 없음(docs/01 §3 금지 사항 준수).
//! 배경 채우기는 같은 메모리 DC에 GDI(ETO_OPAQUE) — 행 = fill + DrawGlyphRun.
//! 텍스트 레이아웃은 텍스트 키 캐시(가시 행은 프레임 간 동일 — ADR-0002 §5 최적화).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use nexa_gui::{Color, DrawCtx, Rect};
use windows::core::{implement, w, Ref, Result, BOOL};
use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteBitmapRenderTarget, IDWriteFactory, IDWriteGdiInterop,
    IDWriteInlineObject, IDWritePixelSnapping_Impl, IDWriteRenderingParams, IDWriteTextFormat,
    IDWriteTextLayout, IDWriteTextRenderer, IDWriteTextRenderer_Impl, DWRITE_FACTORY_TYPE_SHARED,
    DWRITE_GLYPH_RUN, DWRITE_GLYPH_RUN_DESCRIPTION, DWRITE_MATRIX, DWRITE_MEASURING_MODE,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_STRIKETHROUGH, DWRITE_TEXT_ALIGNMENT_CENTER,
    DWRITE_TEXT_METRICS, DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_UNDERLINE,
    DWRITE_WORD_WRAPPING_NO_WRAP,
};
use windows::Win32::Graphics::Gdi::{ExtTextOutW, SetBkColor, ETO_OPAQUE, HDC};

/// [`Color`] → GDI `COLORREF`(0x00BBGGRR).
pub fn colorref(c: Color) -> COLORREF {
    COLORREF(((c.b as u32) << 16) | ((c.g as u32) << 8) | c.r as u32)
}

/// 레이아웃 캐시 상한 — 초과 시 전체 비움(가시 행 수백 개 대비 충분한 여유).
const LAYOUT_CACHE_CAP: usize = 4096;
/// 측정 전용 레이아웃의 가상 최대 폭(px) — 트리밍이 걸리지 않는 충분히 큰 값.
const MEASURE_W: i32 = 1 << 20;

/// 텍스트 레이아웃의 글리프 런을 비트맵 렌더 타깃에 그리는 콜백.
/// 색은 [`DwBackend`]와 공유하는 `Cell`로 전달(그리기 직전에 설정).
#[implement(IDWriteTextRenderer)]
struct BrtRenderer {
    brt: IDWriteBitmapRenderTarget,
    params: IDWriteRenderingParams,
    color: Rc<Cell<COLORREF>>,
}

impl IDWritePixelSnapping_Impl for BrtRenderer_Impl {
    fn IsPixelSnappingDisabled(&self, _ctx: *const core::ffi::c_void) -> Result<BOOL> {
        Ok(BOOL(0))
    }

    fn GetCurrentTransform(
        &self,
        _ctx: *const core::ffi::c_void,
        transform: *mut DWRITE_MATRIX,
    ) -> Result<()> {
        unsafe { self.brt.GetCurrentTransform(transform) }
    }

    fn GetPixelsPerDip(&self, _ctx: *const core::ffi::c_void) -> Result<f32> {
        Ok(unsafe { self.brt.GetPixelsPerDip() })
    }
}

impl IDWriteTextRenderer_Impl for BrtRenderer_Impl {
    fn DrawGlyphRun(
        &self,
        _ctx: *const core::ffi::c_void,
        baseline_x: f32,
        baseline_y: f32,
        measuring_mode: DWRITE_MEASURING_MODE,
        glyph_run: *const DWRITE_GLYPH_RUN,
        _desc: *const DWRITE_GLYPH_RUN_DESCRIPTION,
        _effect: Ref<windows::core::IUnknown>,
    ) -> Result<()> {
        unsafe {
            self.brt.DrawGlyphRun(
                baseline_x,
                baseline_y,
                measuring_mode,
                glyph_run,
                &self.params,
                self.color.get(),
                None,
            )
        }
    }

    fn DrawUnderline(
        &self,
        _ctx: *const core::ffi::c_void,
        _baseline_x: f32,
        _baseline_y: f32,
        _underline: *const DWRITE_UNDERLINE,
        _effect: Ref<windows::core::IUnknown>,
    ) -> Result<()> {
        Ok(())
    }

    fn DrawStrikethrough(
        &self,
        _ctx: *const core::ffi::c_void,
        _baseline_x: f32,
        _baseline_y: f32,
        _strikethrough: *const DWRITE_STRIKETHROUGH,
        _effect: Ref<windows::core::IUnknown>,
    ) -> Result<()> {
        Ok(())
    }

    fn DrawInlineObject(
        &self,
        _ctx: *const core::ffi::c_void,
        _origin_x: f32,
        _origin_y: f32,
        _inline_object: Ref<IDWriteInlineObject>,
        _is_sideways: BOOL,
        _is_rtl: BOOL,
        _effect: Ref<windows::core::IUnknown>,
    ) -> Result<()> {
        Ok(())
    }
}

/// 미리보기 이미지 캐시 키 — (경로, 맞춤 폭, 높이).
type ImageKey = (String, i32, i32);
/// 디코드 결과 — (실제 w, h, 32bpp BGRA top-down).
type DecodedImage = (i32, i32, Vec<u8>);

/// DirectWrite interop 백엔드 — 창 1개 기준 수명(창 크기 변경 시 Resize).
pub struct DwBackend {
    factory: IDWriteFactory,
    brt: IDWriteBitmapRenderTarget,
    format: IDWriteTextFormat,
    renderer: IDWriteTextRenderer,
    color: Rc<Cell<COLORREF>>,
    /// 큰 글리프(버튼 화살표 등) 포맷 — 15 DIP·가로 중앙 정렬.
    icon_format: IDWriteTextFormat,
    /// 컬럼 트리밍(말줄임표) 기호 — 레이아웃마다 SetTrimming으로 부착.
    ellipsis: IDWriteInlineObject,
    /// 터미널 모노스페이스 포맷(M4-3 — Consolas 12 DIP, 셀 그리드 정렬).
    mono_format: IDWriteTextFormat,
    /// (텍스트, 최대 폭 px) → 레이아웃 캐시. 폭이 트리밍을 결정하므로 키에 포함. DPI 변경 시 비움.
    layouts: RefCell<HashMap<(String, i32), IDWriteTextLayout>>,
    /// 미리보기 이미지 캐시(M4-2) — (경로, 맞춤 폭, 높이) → 디코드 결과. 상한 초과 시 비움.
    /// 백엔드는 상주 트림에서 통째로 해제되므로 캐시도 함께 소멸(M2-8 규율 부합).
    images: RefCell<HashMap<ImageKey, DecodedImage>>,
    wic: RefCell<Option<windows::Win32::Graphics::Imaging::IWICImagingFactory>>,
    w: i32,
    h: i32,
}

impl DwBackend {
    /// `hdc`와 호환되는 비트맵 렌더 타깃을 만든다(9pt = 12 DIP, DPI는 PixelsPerDip로 반영).
    pub unsafe fn new(hdc: HDC, w: i32, h: i32, dpi: u32) -> Result<Self> {
        let factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let interop: IDWriteGdiInterop = factory.GetGdiInterop()?;
        let brt = interop.CreateBitmapRenderTarget(Some(hdc), w.max(1) as u32, h.max(1) as u32)?;
        brt.SetPixelsPerDip(dpi as f32 / 96.0)?;
        let params = factory.CreateRenderingParams()?;

        let format = factory.CreateTextFormat(
            w!("Segoe UI"),
            None,
            windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT_NORMAL,
            windows::Win32::Graphics::DirectWrite::DWRITE_FONT_STYLE_NORMAL,
            windows::Win32::Graphics::DirectWrite::DWRITE_FONT_STRETCH_NORMAL,
            12.0, // 9pt (GDI 백엔드와 동일 크기)
            w!("ko-kr"),
        )?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        // 행 rect 안 세로 중앙 정렬(레이아웃 maxheight = 행 높이)
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;

        // 큰 글리프 포맷(네비 화살표 등) — 15 DIP·상하/좌우 중앙(글리프 가시성)
        let icon_format = factory.CreateTextFormat(
            w!("Segoe UI"),
            None,
            windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT_NORMAL,
            windows::Win32::Graphics::DirectWrite::DWRITE_FONT_STYLE_NORMAL,
            windows::Win32::Graphics::DirectWrite::DWRITE_FONT_STRETCH_NORMAL,
            15.0,
            w!("ko-kr"),
        )?;
        icon_format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        icon_format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        icon_format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;

        // 터미널 모노스페이스(M4-3) — Consolas(비스타+ 인박스)·랩 없음·세로 중앙
        let mono_format = factory.CreateTextFormat(
            w!("Consolas"),
            None,
            windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT_NORMAL,
            windows::Win32::Graphics::DirectWrite::DWRITE_FONT_STYLE_NORMAL,
            windows::Win32::Graphics::DirectWrite::DWRITE_FONT_STRETCH_NORMAL,
            12.0,
            w!("ko-kr"),
        )?;
        mono_format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        mono_format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;

        let color = Rc::new(Cell::new(COLORREF(0)));
        let renderer: IDWriteTextRenderer = BrtRenderer {
            brt: brt.clone(),
            params,
            color: color.clone(),
        }
        .into();
        let ellipsis = factory.CreateEllipsisTrimmingSign(&format)?;

        Ok(DwBackend {
            factory,
            brt,
            format,
            renderer,
            color,
            icon_format,
            ellipsis,
            mono_format,
            layouts: RefCell::new(HashMap::new()),
            images: RefCell::new(HashMap::new()),
            wic: RefCell::new(None),
            w,
            h,
        })
    }

    /// `(text, max_w_px)`의 레이아웃(캐시 히트 시 재사용) — 폭 초과분은 말줄임표 트리밍.
    /// `max_h_dip` = 행 높이(세로 중앙 정렬 기준).
    fn layout_for(&self, text: &str, max_w_px: i32, max_h_dip: f32) -> Option<IDWriteTextLayout> {
        let key = (text.to_owned(), max_w_px);
        if let Some(l) = self.layouts.borrow().get(&key) {
            return Some(l.clone());
        }
        let ppd = self.pixels_per_dip();
        let wtext: Vec<u16> = text.encode_utf16().collect();
        let layout = unsafe {
            let layout = self
                .factory
                .CreateTextLayout(&wtext, &self.format, max_w_px as f32 / ppd, max_h_dip)
                .ok()?;
            let trim = DWRITE_TRIMMING {
                granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                delimiter: 0,
                delimiterCount: 0,
            };
            let _ = layout.SetTrimming(&trim, &self.ellipsis);
            layout
        };
        let mut cache = self.layouts.borrow_mut();
        if cache.len() >= LAYOUT_CACHE_CAP {
            cache.clear();
        }
        cache.insert(key, layout.clone());
        Some(layout)
    }

    pub fn size(&self) -> (i32, i32) {
        (self.w, self.h)
    }

    pub unsafe fn resize(&mut self, w: i32, h: i32) -> Result<()> {
        self.brt.Resize(w.max(1) as u32, h.max(1) as u32)?;
        self.w = w;
        self.h = h;
        Ok(())
    }

    pub unsafe fn set_dpi(&mut self, dpi: u32) -> Result<()> {
        // 행 높이(px)가 바뀌므로 캐시된 maxheight가 무효 — 비움
        self.layouts.borrow_mut().clear();
        self.brt.SetPixelsPerDip(dpi as f32 / 96.0)
    }

    /// 백버퍼(비트맵 렌더 타깃)의 메모리 DC — BitBlt 원본.
    pub unsafe fn memory_dc(&self) -> HDC {
        self.brt.GetMemoryDC()
    }

    fn pixels_per_dip(&self) -> f32 {
        unsafe { self.brt.GetPixelsPerDip() }
    }

    /// 미리보기 이미지 디코드(M4-2) — WIC로 `(max_w, max_h)` 안에 비율 유지 스케일한
    /// 32bpp BGRA를 반환(캐시). 확대는 안 함(원본 크기 이하 표시). 실패 = None(표시 생략).
    fn image_scaled(&self, path: &str, max_w: i32, max_h: i32) -> Option<DecodedImage> {
        use windows::core::PCWSTR;
        use windows::Win32::Foundation::GENERIC_READ;
        use windows::Win32::Graphics::Imaging::{
            CLSID_WICImagingFactory, GUID_WICPixelFormat32bppBGRA, IWICImagingFactory,
            WICBitmapDitherTypeNone, WICBitmapInterpolationModeFant, WICBitmapPaletteTypeCustom,
            WICDecodeMetadataCacheOnDemand,
        };
        use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
        if max_w <= 0 || max_h <= 0 {
            return None;
        }
        let key = (path.to_owned(), max_w, max_h);
        if let Some(v) = self.images.borrow().get(&key) {
            return Some(v.clone());
        }
        unsafe {
            if self.wic.borrow().is_none() {
                // UI 스레드는 기동 시 OleInitialize(STA — M3-5) 완료 상태
                *self.wic.borrow_mut() = CoCreateInstance::<_, IWICImagingFactory>(
                    &CLSID_WICImagingFactory,
                    None,
                    CLSCTX_INPROC_SERVER,
                )
                .ok();
            }
            let wic_ref = self.wic.borrow();
            let wic = wic_ref.as_ref()?;
            let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
            let dec = wic
                .CreateDecoderFromFilename(
                    PCWSTR(wide.as_ptr()),
                    None,
                    GENERIC_READ,
                    WICDecodeMetadataCacheOnDemand,
                )
                .ok()?;
            let frame = dec.GetFrame(0).ok()?;
            let (mut iw, mut ih) = (0u32, 0u32);
            frame.GetSize(&mut iw, &mut ih).ok()?;
            if iw == 0 || ih == 0 {
                return None;
            }
            let scale = (max_w as f32 / iw as f32)
                .min(max_h as f32 / ih as f32)
                .min(1.0);
            let tw = ((iw as f32 * scale) as u32).max(1);
            let th = ((ih as f32 * scale) as u32).max(1);
            let scaler = wic.CreateBitmapScaler().ok()?;
            scaler
                .Initialize(&frame, tw, th, WICBitmapInterpolationModeFant)
                .ok()?;
            let conv = wic.CreateFormatConverter().ok()?;
            conv.Initialize(
                &scaler,
                &GUID_WICPixelFormat32bppBGRA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeCustom,
            )
            .ok()?;
            let stride = tw * 4;
            let mut buf = vec![0u8; (stride * th) as usize];
            conv.CopyPixels(std::ptr::null(), stride, &mut buf).ok()?;
            drop(wic_ref);
            let val = (tw as i32, th as i32, buf);
            let mut cache = self.images.borrow_mut();
            if cache.len() >= 8 {
                cache.clear(); // 소형 상한 — 초과 시 전체 비움(상주 규율)
            }
            cache.insert(key, val.clone());
            Some(val)
        }
    }
}

/// 프레임 단위 드로잉 컨텍스트 — DrawCtx의 DirectWrite interop 구현.
/// 아이콘은 [`crate::icons::shell::ShellIcons`] 캐시(미로드 시 큐잉 — win.rs 타이머가 로드).
pub struct DwCtx<'a> {
    pub back: &'a DwBackend,
    pub icons: &'a std::cell::RefCell<crate::icons::shell::ShellIcons>,
}

impl DrawCtx for DwCtx<'_> {
    fn fill_rect(&mut self, rect: Rect, color: Color) {
        unsafe {
            let dc = self.back.memory_dc();
            SetBkColor(dc, colorref(color));
            let rc = RECT {
                left: rect.x,
                top: rect.y,
                right: rect.right(),
                bottom: rect.bottom(),
            };
            let _ = ExtTextOutW(dc, rect.x, rect.y, ETO_OPAQUE, Some(&rc), w!(""), 0, None);
        }
    }

    fn text_opaque(&mut self, x: i32, _y: i32, clip: Rect, text: &str, fg: Color, bg: Color) {
        // 배경은 GDI로 불투명 채우기, 글리프는 DirectWrite로 — 행 = fill + Draw 1회
        self.fill_rect(clip, bg);
        if text.is_empty() || x >= clip.right() {
            return;
        }
        unsafe {
            let ppd = self.back.pixels_per_dip();
            let max_w = clip.right() - x;
            let Some(layout) = self.back.layout_for(text, max_w, clip.h as f32 / ppd) else {
                return;
            };
            self.back.color.set(colorref(fg));
            let _ = layout.Draw(
                None,
                &self.back.renderer,
                x as f32 / ppd,
                clip.y as f32 / ppd,
            );
        }
    }

    fn term_cell_w(&mut self) -> i32 {
        // Consolas 12 DIP "0" 폭(px) — 모노라 전 반각 문자 동일(M4-3)
        unsafe {
            let ppd = self.back.pixels_per_dip();
            let wtext: Vec<u16> = "0".encode_utf16().collect();
            let Ok(layout) =
                self.back
                    .factory
                    .CreateTextLayout(&wtext, &self.back.mono_format, 1000.0, 100.0)
            else {
                return 8;
            };
            let mut m = DWRITE_TEXT_METRICS::default();
            if layout.GetMetrics(&mut m).is_err() {
                return 8;
            }
            ((m.widthIncludingTrailingWhitespace * ppd).ceil() as i32).max(1)
        }
    }

    fn term_text(&mut self, x: i32, _y: i32, clip: Rect, text: &str, fg: Color, bg: Color) {
        // 모노스페이스 런(M4-3) — 배경 채움 + Consolas 레이아웃(캐시 없음: 터미널 출력은
        // 프레임마다 변함·가시 행 수십 개 수준)
        self.fill_rect(clip, bg);
        if text.is_empty() || x >= clip.right() {
            return;
        }
        unsafe {
            let ppd = self.back.pixels_per_dip();
            let wtext: Vec<u16> = text.encode_utf16().collect();
            let Ok(layout) = self.back.factory.CreateTextLayout(
                &wtext,
                &self.back.mono_format,
                (clip.right() - x) as f32 / ppd,
                clip.h as f32 / ppd,
            ) else {
                return;
            };
            self.back.color.set(colorref(fg));
            let _ = layout.Draw(
                None,
                &self.back.renderer,
                x as f32 / ppd,
                clip.y as f32 / ppd,
            );
        }
    }

    fn draw_image(&mut self, rect: Rect, hint: &str) {
        // WIC 디코드(캐시) → 백버퍼 메모리 DC에 StretchDIBits(가운데·비율 유지, M4-2)
        if rect.w <= 2 || rect.h <= 2 {
            return;
        }
        let Some((w, h, bits)) = self.back.image_scaled(hint, rect.w - 2, rect.h - 2) else {
            return;
        };
        unsafe {
            use windows::Win32::Graphics::Gdi::{
                StretchDIBits, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, SRCCOPY,
            };
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h, // top-down
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let dx = rect.x + (rect.w - w) / 2;
            let dy = rect.y + (rect.h - h) / 2;
            StretchDIBits(
                self.back.memory_dc(),
                dx,
                dy,
                w,
                h,
                0,
                0,
                w,
                h,
                Some(bits.as_ptr() as *const core::ffi::c_void),
                &bmi,
                DIB_RGB_COLORS,
                SRCCOPY,
            );
        }
    }

    fn text(&mut self, x: i32, _y: i32, clip: Rect, text: &str, fg: Color) {
        // 배경 없이 글리프만(편집 필드 — 선택 하이라이트 위 1회 겹쳐 그리기, QA 07-13)
        if text.is_empty() || x >= clip.right() {
            return;
        }
        unsafe {
            let ppd = self.back.pixels_per_dip();
            let max_w = clip.right() - x;
            let Some(layout) = self.back.layout_for(text, max_w, clip.h as f32 / ppd) else {
                return;
            };
            self.back.color.set(colorref(fg));
            let _ = layout.Draw(
                None,
                &self.back.renderer,
                x as f32 / ppd,
                clip.y as f32 / ppd,
            );
        }
    }

    fn text_width(&mut self, text: &str) -> i32 {
        if text.is_empty() {
            return 0;
        }
        unsafe {
            let ppd = self.back.pixels_per_dip();
            let Some(layout) = self.back.layout_for(text, MEASURE_W, 1000.0) else {
                return 0;
            };
            let mut m = DWRITE_TEXT_METRICS::default();
            if layout.GetMetrics(&mut m).is_err() {
                return 0;
            }
            (m.widthIncludingTrailingWhitespace * ppd).ceil() as i32
        }
    }

    fn glyph_opaque(
        &mut self,
        clip: nexa_gui::Rect,
        text: &str,
        fg: nexa_gui::Color,
        bg: nexa_gui::Color,
    ) {
        self.fill_rect(clip, bg);
        if text.is_empty() || clip.w <= 0 {
            return;
        }
        unsafe {
            let ppd = self.back.pixels_per_dip();
            // 아이콘 포맷 전용 캐시 키(제어문자 접두사로 본문 캐시와 분리)
            let key = format!("\u{1}{text}");
            let layout = self
                .back
                .layouts
                .borrow()
                .get(&(key.clone(), clip.w))
                .cloned();
            let layout = match layout {
                Some(l) => l,
                None => {
                    let wtext: Vec<u16> = text.encode_utf16().collect();
                    let Ok(l) = self.back.factory.CreateTextLayout(
                        &wtext,
                        &self.back.icon_format,
                        clip.w as f32 / ppd,
                        clip.h as f32 / ppd,
                    ) else {
                        return;
                    };
                    let mut cache = self.back.layouts.borrow_mut();
                    if cache.len() >= LAYOUT_CACHE_CAP {
                        cache.clear();
                    }
                    cache.insert((key, clip.w), l.clone());
                    l
                }
            };
            self.back.color.set(colorref(fg));
            let _ = layout.Draw(
                None,
                &self.back.renderer,
                clip.x as f32 / ppd,
                clip.y as f32 / ppd,
            );
        }
    }

    fn draw_icon(&mut self, x: i32, y: i32, size: i32, key: &str, hint: &str) {
        if let Some(icon) = self.icons.borrow_mut().get_or_request(key, hint) {
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::DrawIconEx(
                    self.back.memory_dc(),
                    x,
                    y,
                    icon,
                    size,
                    size,
                    0,
                    None,
                    windows::Win32::UI::WindowsAndMessaging::DI_NORMAL,
                );
            }
        }
        // 미로드 시 공백 유지 — 큐잉됐으므로 win.rs 아이콘 타이머가 로드 후 다시 그린다
    }
}
