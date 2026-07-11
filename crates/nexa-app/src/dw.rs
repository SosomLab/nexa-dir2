//! DirectWrite GDI interop 백엔드 — ADR-0002(M1-2) 비교 후보.
//! `IDWriteBitmapRenderTarget`(자체 메모리 DC = 더블 버퍼)에 텍스트 레이아웃을 그리고
//! 창 DC로 BitBlt — GPU 스왑체인 없음(docs/01 §3 금지 사항 준수).
//! 배경 채우기는 같은 메모리 DC에 GDI(ETO_OPAQUE)로 — gdi.rs와 동일 모델.

use std::cell::Cell;
use std::rc::Rc;

use nexa_gui::{Color, DrawCtx, Rect};
use windows::core::{implement, w, Ref, Result, BOOL};
use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteBitmapRenderTarget, IDWriteFactory, IDWriteGdiInterop,
    IDWriteInlineObject, IDWritePixelSnapping_Impl, IDWriteRenderingParams, IDWriteTextFormat,
    IDWriteTextRenderer, IDWriteTextRenderer_Impl, DWRITE_FACTORY_TYPE_SHARED, DWRITE_GLYPH_RUN,
    DWRITE_GLYPH_RUN_DESCRIPTION, DWRITE_MATRIX, DWRITE_MEASURING_MODE,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_STRIKETHROUGH, DWRITE_UNDERLINE,
    DWRITE_WORD_WRAPPING_NO_WRAP,
};
use windows::Win32::Graphics::Gdi::{ExtTextOutW, SetBkColor, ETO_OPAQUE, HDC};

use crate::gdi::colorref;

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

        Ok(DwBackend {
            factory,
            brt,
            format,
            renderer,
            color,
            w,
            h,
        })
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
        unsafe {
            let ppd = self.back.pixels_per_dip();
            let wtext: Vec<u16> = text.encode_utf16().collect();
            let Ok(layout) = self.back.factory.CreateTextLayout(
                &wtext,
                &self.back.format,
                (clip.right() - x) as f32 / ppd,
                clip.h as f32 / ppd, // 세로 중앙 정렬 기준 높이 = 행 높이
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
}
