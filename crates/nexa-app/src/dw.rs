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
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_STRIKETHROUGH, DWRITE_TEXT_METRICS, DWRITE_TRIMMING,
    DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_UNDERLINE, DWRITE_WORD_WRAPPING_NO_WRAP,
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

/// DirectWrite interop 백엔드 — 창 1개 기준 수명(창 크기 변경 시 Resize).
pub struct DwBackend {
    factory: IDWriteFactory,
    brt: IDWriteBitmapRenderTarget,
    format: IDWriteTextFormat,
    renderer: IDWriteTextRenderer,
    color: Rc<Cell<COLORREF>>,
    /// 컬럼 트리밍(말줄임표) 기호 — 레이아웃마다 SetTrimming으로 부착.
    ellipsis: IDWriteInlineObject,
    /// (텍스트, 최대 폭 px) → 레이아웃 캐시. 폭이 트리밍을 결정하므로 키에 포함. DPI 변경 시 비움.
    layouts: RefCell<HashMap<(String, i32), IDWriteTextLayout>>,
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
            ellipsis,
            layouts: RefCell::new(HashMap::new()),
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
}

/// 프레임 단위 드로잉 컨텍스트 — DrawCtx의 DirectWrite interop 구현.
pub struct DwCtx<'a> {
    pub back: &'a DwBackend,
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
}
