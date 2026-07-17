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

/// 폰트 슬롯 사양(X-12 — 사용자 요청 07-16): (패밀리, 크기 DIP).
/// 콘솔(mono)·대화상자(GDI DlgFont)는 기존 경로 유지.
#[derive(Clone, PartialEq, Debug)]
pub struct FontSpec {
    pub base: (String, f32),
    pub list: (String, f32),
    pub status: (String, f32),
}

/// 스타일 id = 슬롯×4 + bold + 2×italic (nexa-gui FontSlot ↔ 여기 매핑).
fn style_id(slot: nexa_gui::FontSlot, bold: bool, italic: bool) -> u8 {
    let s = match slot {
        nexa_gui::FontSlot::Base => 0u8,
        nexa_gui::FontSlot::List => 1,
        nexa_gui::FontSlot::Status => 2,
    };
    s * 4 + u8::from(bold) + 2 * u8::from(italic)
}

/// DirectWrite interop 백엔드 — 창 1개 기준 수명(창 크기 변경 시 Resize).
pub struct DwBackend {
    factory: IDWriteFactory,
    brt: IDWriteBitmapRenderTarget,
    /// 폰트 슬롯 포맷(X-12) — 스타일 id → (포맷, 말줄임 기호). 기본/목록(굵게·이탤릭
    /// 장식 포함)/상태바. 조회 실패 = 기본(0) 폴백.
    formats: HashMap<u8, (IDWriteTextFormat, IDWriteInlineObject)>,
    /// 현재 선택 스타일(DrawCtx::select_font — 위젯이 페인트 시작에 지정).
    cur_style: std::cell::Cell<u8>,
    renderer: IDWriteTextRenderer,
    color: Rc<Cell<COLORREF>>,
    /// 큰 글리프(버튼 화살표 등) 포맷 — 15 DIP·가로 중앙 정렬.
    icon_format: IDWriteTextFormat,
    /// Segoe MDL2 Assets 글리프 포맷(07-18 — 원본 내비/디스클로저 규약:
    /// U+E700 대역 PUA는 이 포맷으로 라우팅. 인박스 아이콘 폰트).
    mdl2_format: IDWriteTextFormat,
    /// 터미널 모노스페이스 포맷(M4-3 — 기본 Consolas 12 DIP, 셀 그리드 정렬.
    /// 설정 `term_font`로 교체 — 미설치 글리프는 DWrite 시스템 폴백이 해석).
    mono_format: IDWriteTextFormat,
    /// 명시 폴백 체인(X-3 — term_font 쉼표 목록 2순위 이후·시스템 폴백 연결).
    mono_fallback: Option<windows::Win32::Graphics::DirectWrite::IDWriteFontFallback>,
    /// 터미널 단일 글리프 레이아웃 캐시(QA 07-14 — 셀 단위 렌더의 프레임당 생성 비용 제거).
    mono_glyphs: RefCell<HashMap<char, IDWriteTextLayout>>,
    /// (텍스트, 최대 폭 px) → 레이아웃 캐시. 폭이 트리밍을 결정하므로 키에 포함. DPI 변경 시 비움.
    /// 레이아웃 캐시 — 외측 키 = 폭(px, 아이콘 포맷은 음수 네임스페이스), 내측 = 텍스트.
    /// 중첩 맵인 이유(X-16): 튜플 키 `(String, i32)`는 조회마다 `to_owned()` 할당이 필요
    /// 하지만, 내측 `HashMap<String, _>`은 `&str`로 무할당 조회가 된다(Borrow<str>).
    layouts: RefCell<HashMap<i32, HashMap<String, IDWriteTextLayout>>>,
    /// 캐시 총 항목 수(중첩 맵 합산 대신 카운터 — 상한 도달 시 전체 비움).
    layout_count: std::cell::Cell<usize>,
    /// 미리보기 이미지 캐시(M4-2) — (경로, 맞춤 폭, 높이) → 디코드 결과. 상한 초과 시 비움.
    /// 백엔드는 상주 트림에서 통째로 해제되므로 캐시도 함께 소멸(M2-8 규율 부합).
    images: RefCell<HashMap<ImageKey, DecodedImage>>,
    wic: RefCell<Option<windows::Win32::Graphics::Imaging::IWICImagingFactory>>,
    w: i32,
    h: i32,
}

impl DwBackend {
    /// `hdc`와 호환되는 비트맵 렌더 타깃을 만든다(9pt = 12 DIP, DPI는 PixelsPerDip로 반영).
    pub unsafe fn new(
        hdc: HDC,
        w: i32,
        h: i32,
        dpi: u32,
        fonts: &FontSpec,
        mono_font: &str,
        mono_size: f32,
    ) -> Result<Self> {
        let factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let interop: IDWriteGdiInterop = factory.GetGdiInterop()?;
        let brt = interop.CreateBitmapRenderTarget(Some(hdc), w.max(1) as u32, h.max(1) as u32)?;
        brt.SetPixelsPerDip(dpi as f32 / 96.0)?;
        let params = factory.CreateRenderingParams()?;

        // 폰트 슬롯 포맷(X-12): 기본·상태바 = 보통, 목록 = 보통/굵게/이탤릭/굵은이탤릭
        // (폴더 이름 굵게·헤더 장식). 미설치 패밀리는 DWrite가 시스템 폴백으로 해석.
        let mk = |family: &str,
                  size: f32,
                  bold: bool,
                  italic: bool|
         -> Result<(IDWriteTextFormat, IDWriteInlineObject)> {
            use windows::Win32::Graphics::DirectWrite::{
                DWRITE_FONT_STYLE_ITALIC, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
            };
            let name: Vec<u16> = family.encode_utf16().chain(std::iter::once(0)).collect();
            let f = factory.CreateTextFormat(
                windows::core::PCWSTR(name.as_ptr()),
                None,
                if bold {
                    DWRITE_FONT_WEIGHT_SEMI_BOLD
                } else {
                    DWRITE_FONT_WEIGHT_NORMAL
                },
                if italic {
                    DWRITE_FONT_STYLE_ITALIC
                } else {
                    DWRITE_FONT_STYLE_NORMAL
                },
                windows::Win32::Graphics::DirectWrite::DWRITE_FONT_STRETCH_NORMAL,
                size.clamp(8.0, 32.0),
                w!("ko-kr"),
            )?;
            f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            // 행 rect 안 세로 중앙 정렬(레이아웃 maxheight = 행 높이)
            f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            let e = factory.CreateEllipsisTrimmingSign(&f)?;
            Ok((f, e))
        };
        let mut formats: HashMap<u8, (IDWriteTextFormat, IDWriteInlineObject)> = HashMap::new();
        formats.insert(0, mk(&fonts.base.0, fonts.base.1, false, false)?);
        for (b, i) in [(false, false), (true, false), (false, true), (true, true)] {
            formats.insert(
                4 + u8::from(b) + 2 * u8::from(i),
                mk(&fonts.list.0, fonts.list.1, b, i)?,
            );
        }
        formats.insert(8, mk(&fonts.status.0, fonts.status.1, false, false)?);

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

        // Segoe MDL2 Assets(07-18 — 원본 내비 버튼 11px 규약·인박스 아이콘 폰트)
        let mdl2_format = factory.CreateTextFormat(
            w!("Segoe MDL2 Assets"),
            None,
            windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT_NORMAL,
            windows::Win32::Graphics::DirectWrite::DWRITE_FONT_STYLE_NORMAL,
            windows::Win32::Graphics::DirectWrite::DWRITE_FONT_STRETCH_NORMAL,
            11.0,
            w!("ko-kr"),
        )?;
        mdl2_format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        mdl2_format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        mdl2_format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;

        // 터미널 모노스페이스(M4-3) — 기본 Consolas(비스타+ 인박스)·랩 없음·세로 중앙.
        // 설정 `term_font`(QA 07-14): **쉼표 목록 = 폴백 체인**(WT식 "D2Coding,
        // JetBrainsMono Nerd Font") — 1순위 = 텍스트 포맷 패밀리, 2순위 이후는
        // IDWriteFontFallback(X-3)으로 미보유 글리프 해석 → 시스템 폴백 체인 연결.
        let families: Vec<String> = mono_font
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let primary = families
            .first()
            .cloned()
            .unwrap_or_else(|| "Consolas".to_string());
        let mono_name: Vec<u16> = primary.encode_utf16().chain(std::iter::once(0)).collect();
        let size = mono_size.clamp(8.0, 32.0);
        let mono_format = factory
            .CreateTextFormat(
                windows::core::PCWSTR(mono_name.as_ptr()),
                None,
                windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT_NORMAL,
                windows::Win32::Graphics::DirectWrite::DWRITE_FONT_STYLE_NORMAL,
                windows::Win32::Graphics::DirectWrite::DWRITE_FONT_STRETCH_NORMAL,
                size,
                w!("ko-kr"),
            )
            .or_else(|_| {
                factory.CreateTextFormat(
                    w!("Consolas"),
                    None,
                    windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT_NORMAL,
                    windows::Win32::Graphics::DirectWrite::DWRITE_FONT_STYLE_NORMAL,
                    windows::Win32::Graphics::DirectWrite::DWRITE_FONT_STRETCH_NORMAL,
                    size,
                    w!("ko-kr"),
                )
            })?;
        mono_format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        mono_format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        // 명시 폴백 체인(X-3 — 2순위 이후) — 실패는 조용히 시스템 폴백만 사용
        let mono_fallback: Option<windows::Win32::Graphics::DirectWrite::IDWriteFontFallback> =
            if families.len() > 1 {
                (|| {
                    use windows::core::Interface;
                    use windows::Win32::Graphics::DirectWrite::{
                        IDWriteFactory2, DWRITE_UNICODE_RANGE,
                    };
                    let f2: IDWriteFactory2 = factory.cast().ok()?;
                    let builder = f2.CreateFontFallbackBuilder().ok()?;
                    let range = DWRITE_UNICODE_RANGE {
                        first: 0x0,
                        last: 0x10FFFF,
                    };
                    let names: Vec<windows::core::HSTRING> = families[1..]
                        .iter()
                        .map(windows::core::HSTRING::from)
                        .collect();
                    let ptrs: Vec<*const u16> = names.iter().map(|h| h.as_ptr()).collect();
                    builder
                        .AddMapping(&[range], &ptrs, None, None, None, 1.0)
                        .ok()?;
                    let sys = f2.GetSystemFontFallback().ok()?;
                    builder.AddMappings(&sys).ok()?;
                    builder.CreateFontFallback().ok()
                })()
            } else {
                None
            };

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
            formats,
            cur_style: std::cell::Cell::new(0),
            renderer,
            color,
            icon_format,
            mdl2_format,
            mono_format,
            mono_fallback,
            mono_glyphs: RefCell::new(HashMap::new()),
            layouts: RefCell::new(HashMap::new()),
            layout_count: std::cell::Cell::new(0),
            images: RefCell::new(HashMap::new()),
            wic: RefCell::new(None),
            w,
            h,
        })
    }

    /// 현재 스타일의 (포맷, 말줄임 기호) — 미등록 스타일은 기본(0) 폴백.
    fn cur_format(&self) -> &(IDWriteTextFormat, IDWriteInlineObject) {
        self.formats
            .get(&self.cur_style.get())
            .or_else(|| self.formats.get(&0))
            .expect("기본 포맷은 항상 등록")
    }

    /// `(text, max_w_px)`의 레이아웃(캐시 히트 시 재사용) — 폭 초과분은 말줄임표 트리밍.
    /// `max_h_dip` = 행 높이(세로 중앙 정렬 기준). 캐시 외측 키 = 폭 + **스타일 상위
    /// 비트**(X-12 — 슬롯/장식별 레이아웃 분리. |폭| < 2^20 전제·아이콘 음수 네임스페이스 보존).
    fn layout_for(&self, text: &str, max_w_px: i32, max_h_dip: f32) -> Option<IDWriteTextLayout> {
        let style = self.cur_style.get();
        let key_w = max_w_px + ((style as i32) << 21);
        // 히트 경로 무할당(X-16) — &str 그대로 내측 맵 조회
        if let Some(l) = self.layouts.borrow().get(&key_w).and_then(|m| m.get(text)) {
            return Some(l.clone());
        }
        let ppd = self.pixels_per_dip();
        let wtext: Vec<u16> = text.encode_utf16().collect();
        let (format, ellipsis) = self.cur_format();
        let layout = unsafe {
            let layout = self
                .factory
                .CreateTextLayout(&wtext, format, max_w_px as f32 / ppd, max_h_dip)
                .ok()?;
            let trim = DWRITE_TRIMMING {
                granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                delimiter: 0,
                delimiterCount: 0,
            };
            let _ = layout.SetTrimming(&trim, ellipsis);
            layout
        };
        let mut cache = self.layouts.borrow_mut();
        if self.layout_count.get() >= LAYOUT_CACHE_CAP {
            cache.clear();
            self.layout_count.set(0);
        }
        cache
            .entry(key_w)
            .or_default()
            .insert(text.to_owned(), layout.clone());
        self.layout_count.set(self.layout_count.get() + 1);
        Some(layout)
    }

    pub fn size(&self) -> (i32, i32) {
        (self.w, self.h)
    }

    /// 터미널 레이아웃에 명시 폴백 체인 적용(X-3 — 없으면 무동작).
    unsafe fn apply_mono_fallback(&self, layout: &IDWriteTextLayout) {
        if let Some(fb) = &self.mono_fallback {
            use windows::core::Interface;
            if let Ok(l2) =
                layout.cast::<windows::Win32::Graphics::DirectWrite::IDWriteTextLayout2>()
            {
                let _ = l2.SetFontFallback(fb);
            }
        }
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
        self.mono_glyphs.borrow_mut().clear();
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
    fn select_font(&mut self, slot: nexa_gui::FontSlot, bold: bool, italic: bool) {
        // 폰트 슬롯 선택(X-12) — 이후 레이아웃 생성/조회가 이 스타일 포맷을 쓴다
        self.back.cur_style.set(style_id(slot, bold, italic));
    }

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
        // 모노스페이스(M4-3) — 배경 채움 + 글리프. 단일 문자(셀 단위 렌더, QA 07-14)는
        // 레이아웃 캐시로 프레임당 생성 비용 제거, 여러 글자(안내문 등)는 즉석 생성.
        self.fill_rect(clip, bg);
        if text.is_empty() || x >= clip.right() {
            return;
        }
        unsafe {
            let ppd = self.back.pixels_per_dip();
            let mut chars = text.chars();
            let (first, rest) = (chars.next(), chars.next());
            let layout = if let (Some(c), None) = (first, rest) {
                let mut cache = self.back.mono_glyphs.borrow_mut();
                if !cache.contains_key(&c) {
                    let wtext: Vec<u16> = text.encode_utf16().collect();
                    let Ok(l) = self.back.factory.CreateTextLayout(
                        &wtext,
                        &self.back.mono_format,
                        100.0,
                        clip.h as f32 / ppd,
                    ) else {
                        return;
                    };
                    self.back.apply_mono_fallback(&l); // 폴백 체인(X-3)
                    if cache.len() > 2048 {
                        cache.clear(); // 폭주 방지(비정상 출력) — 정상 사용은 수백 자
                    }
                    cache.insert(c, l);
                }
                cache.get(&c).unwrap().clone()
            } else {
                let wtext: Vec<u16> = text.encode_utf16().collect();
                let Ok(l) = self.back.factory.CreateTextLayout(
                    &wtext,
                    &self.back.mono_format,
                    (clip.right() - x) as f32 / ppd,
                    clip.h as f32 / ppd,
                ) else {
                    return;
                };
                self.back.apply_mono_fallback(&l); // 폴백 체인(X-3)
                l
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
            // 아이콘 포맷 캐시 = **음수 폭 네임스페이스**(본문 캐시와 분리 — clip.w > 0
            // 보장이므로 -clip.w는 충돌 없음). 히트 경로 무할당(X-16).
            let layout = self
                .back
                .layouts
                .borrow()
                .get(&-clip.w)
                .and_then(|m| m.get(text))
                .cloned();
            let layout = match layout {
                Some(l) => l,
                None => {
                    let wtext: Vec<u16> = text.encode_utf16().collect();
                    // U+E700 대역 PUA = Segoe MDL2 Assets(원본 내비/디스클로저 규약)
                    let fmt = if text
                        .chars()
                        .next()
                        .is_some_and(|c| ('\u{E700}'..='\u{E8FF}').contains(&c))
                    {
                        &self.back.mdl2_format
                    } else {
                        &self.back.icon_format
                    };
                    let Ok(l) = self.back.factory.CreateTextLayout(
                        &wtext,
                        fmt,
                        clip.w as f32 / ppd,
                        clip.h as f32 / ppd,
                    ) else {
                        return;
                    };
                    let mut cache = self.back.layouts.borrow_mut();
                    if self.back.layout_count.get() >= LAYOUT_CACHE_CAP {
                        cache.clear();
                        self.back.layout_count.set(0);
                    }
                    cache
                        .entry(-clip.w)
                        .or_default()
                        .insert(text.to_owned(), l.clone());
                    self.back.layout_count.set(self.back.layout_count.get() + 1);
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

    fn draw_icon(&mut self, x: i32, y: i32, size: i32, key: &str, hint: &str) -> bool {
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
            return true;
        }
        // 미로드 시 공백 유지 — 큐잉됐으므로 win.rs 아이콘 타이머가 로드 후 다시 그린다
        false
    }
}
