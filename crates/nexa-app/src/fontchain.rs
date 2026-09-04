//! 글꼴 폴백 체인(09-04 — 사용자 요청 "두부 방지"): 설정의 **쉼표 목록 = 폴백 체인** 규약
//! (fontbox·config 주석)을 실제 렌더에 적용한다. 그동안 체인은 터미널(X-3)에서만 살아 있었고
//! UI 슬롯·대화상자·미리보기는 **문자열 전체를 얼굴 이름으로** 넘겨 1순위조차 못 맞추고 있었다.
//!
//! - [`families`]: 체인 파싱. [`first_installed`]: 체인에서 **설치된 첫 패밀리**(GDI
//!   `CreateFont`는 "A, B"를 얼굴 이름으로 받아 매칭 실패 → 기본 글꼴 대체되던 결함 정정).
//!   [`fallbacks`]: 1순위를 뺀 설치 패밀리(DW `IDWriteFontFallback`·GDI 런 공용 목록).
//! - [`GdiChain`]: GDI 커스텀 드로잉(미리보기 창)용 **글리프 단위 폴백** — 1순위 글꼴에 없는
//!   문자는 체인의 다음 글꼴로 런을 나눠 그린다. 측정([`GdiChain::offsets`])도 같은 런 규칙이라
//!   선택 경계가 그리기와 일치한다. 어느 글꼴에도 없는 문자는 1순위에 남겨 OS 글꼴 연결
//!   (FontLink)에 마지막 기회를 준다.
//! - DirectWrite 경로([dw.rs](crate::dw))는 슬롯별 `IDWriteFontFallback`으로 같은 체인을 적용.
//! - 네이티브 컨트롤(STATIC/EDIT 등)은 글리프 단위 개입이 불가 — 패밀리 선택만 정정하고
//!   나머지는 OS 글꼴 연결에 맡긴다(한중일은 인박스 연결로 충분).

use windows::core::PCWSTR;
use windows::Win32::Foundation::{RECT, SIZE};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, DeleteObject, ExtTextOutW, GetGlyphIndicesW, GetTextExtentPoint32W, SelectObject,
    CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_QUALITY, ETO_OPTIONS, HDC, HFONT,
    OUT_DEFAULT_PRECIS,
};

/// GetGlyphIndicesW: 미보유 글리프를 0xFFFF로 표시.
const GGI_MARK_NONEXISTING_GLYPHS: u32 = 1;
const MISSING: u16 = 0xFFFF;

/// 쉼표 체인 → 패밀리 목록(공백 정리·빈 항목 제거).
pub fn families(chain: &str) -> Vec<String> {
    chain
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn is_installed(name: &str) -> bool {
    crate::ctl::fontbox::families()
        .iter()
        .any(|f| f.eq_ignore_ascii_case(name))
}

/// 체인에서 **설치된 첫 패밀리**. 하나도 없으면 `default`(인박스 이름).
pub fn first_installed(chain: &str, default: &str) -> String {
    families(chain)
        .into_iter()
        .find(|f| is_installed(f))
        .unwrap_or_else(|| default.to_string())
}

/// 1순위(`primary`)를 뺀 체인의 설치 패밀리(순서 보존) — 폴백 목록.
pub fn fallbacks(chain: &str, primary: &str) -> Vec<String> {
    families(chain)
        .into_iter()
        .filter(|f| !f.eq_ignore_ascii_case(primary) && is_installed(f))
        .collect()
}

unsafe fn mk_font(face: &str, height: i32, weight: i32, pitch_family: u32) -> HFONT {
    let name = windows::core::HSTRING::from(face);
    CreateFontW(
        height,
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        DEFAULT_QUALITY,
        pitch_family,
        PCWSTR(name.as_ptr()),
    )
}

/// GDI 글꼴 체인 — `[0]` = 1순위, 이후 = 폴백(설치된 것만). 같은 높이·굵기·피치.
pub struct GdiChain {
    fonts: Vec<HFONT>,
}

impl GdiChain {
    /// `chain`의 설치된 첫 패밀리(없으면 `default`)를 1순위로, 나머지 설치 패밀리를 폴백으로.
    /// `height` = `lfHeight`(음수 = 글자 높이 px) · `pitch_family` = `lfPitchAndFamily`.
    ///
    /// # Safety
    /// GDI 객체 생성 — [`GdiChain::delete`]로 해제할 것.
    pub unsafe fn new(
        chain: &str,
        default: &str,
        height: i32,
        weight: i32,
        pitch_family: u32,
    ) -> Self {
        let primary = first_installed(chain, default);
        let mut names = vec![primary.clone()];
        names.extend(fallbacks(chain, &primary));
        let fonts = names
            .iter()
            .map(|n| mk_font(n, height, weight, pitch_family))
            .collect();
        GdiChain { fonts }
    }

    /// 1순위 글꼴(행 높이 측정·기존 SelectObject 경로용).
    pub fn primary(&self) -> HFONT {
        self.fonts[0]
    }

    /// # Safety
    /// 이후 이 체인의 HFONT를 쓰지 말 것.
    pub unsafe fn delete(&self) {
        for f in &self.fonts {
            let _ = DeleteObject((*f).into());
        }
    }

    /// 문자(코드 포인트)별 글꼴 인덱스 — BMP만 판정(서러게이트 쌍은 1순위에 두고 OS 연결에
    /// 맡긴다: `GetGlyphIndicesW`가 UTF-16 단위라 비BMP는 판정 불가).
    unsafe fn assign(&self, hdc: HDC, chars: &[char]) -> Vec<usize> {
        let mut idx = vec![0usize; chars.len()];
        if self.fonts.len() < 2 || chars.is_empty() {
            return idx;
        }
        let bmp: Vec<(usize, u16)> = chars
            .iter()
            .enumerate()
            .filter(|(_, c)| (**c as u32) < 0x1_0000)
            .map(|(i, c)| (i, *c as u16))
            .collect();
        if bmp.is_empty() {
            return idx;
        }
        let units: Vec<u16> = bmp.iter().map(|(_, u)| *u).collect();
        let mut unresolved = vec![true; bmp.len()];
        let mut gi = vec![0u16; units.len()];
        for (fi, &f) in self.fonts.iter().enumerate() {
            let old = SelectObject(hdc, f.into());
            let n = GetGlyphIndicesW(
                hdc,
                PCWSTR(units.as_ptr()),
                units.len() as i32,
                gi.as_mut_ptr(),
                GGI_MARK_NONEXISTING_GLYPHS,
            );
            SelectObject(hdc, old);
            if n == u32::MAX {
                continue; // GDI_ERROR
            }
            let mut left = 0;
            for (k, g) in gi.iter().enumerate() {
                if !unresolved[k] {
                    continue;
                }
                if *g != MISSING {
                    idx[bmp[k].0] = fi;
                    unresolved[k] = false;
                } else {
                    left += 1;
                }
            }
            if left == 0 {
                break;
            }
        }
        idx
    }

    /// 런 = (글꼴 idx, 문자 범위 `[start, end)`) — 코드 포인트 인덱스.
    unsafe fn runs(&self, hdc: HDC, chars: &[char]) -> Vec<(usize, usize, usize)> {
        let idx = self.assign(hdc, chars);
        let mut runs = Vec::new();
        let mut start = 0;
        for i in 1..=idx.len() {
            if i == idx.len() || idx[i] != idx[start] {
                runs.push((idx[start], start, i));
                start = i;
            }
        }
        runs
    }

    /// 문자 경계 x 오프셋 `[0, w1, w1+w2, …]`(도크/미리보기 offsets 규약) — 런별 글꼴로 측정.
    ///
    /// # Safety
    /// 유효한 HDC.
    pub unsafe fn offsets(&self, hdc: HDC, text: &str) -> Vec<i32> {
        let chars: Vec<char> = text.chars().collect();
        let mut offs = vec![0i32];
        let mut x = 0;
        for (fi, s, e) in self.runs(hdc, &chars) {
            let old = SelectObject(hdc, self.fonts[fi].into());
            let mut buf: Vec<u16> = Vec::new();
            for c in &chars[s..e] {
                let mut u = [0u16; 2];
                buf.extend_from_slice(c.encode_utf16(&mut u));
                let mut sz = SIZE::default();
                let _ = GetTextExtentPoint32W(hdc, &buf, &mut sz);
                offs.push(x + sz.cx);
            }
            x = *offs.last().unwrap_or(&0);
            SelectObject(hdc, old);
        }
        offs
    }

    /// 텍스트 폭(px) — 런 합.
    ///
    /// # Safety
    /// 유효한 HDC.
    pub unsafe fn width(&self, hdc: HDC, text: &str) -> i32 {
        self.offsets(hdc, text).last().copied().unwrap_or(0)
    }

    /// 런별 `ExtTextOutW` — `opts`는 ETO_CLIPPED 등(행 불투명 채움은 호출자가 먼저).
    /// 배경 모드·색은 호출자 설정 그대로.
    ///
    /// # Safety
    /// 유효한 HDC.
    pub unsafe fn draw(
        &self,
        hdc: HDC,
        x: i32,
        y: i32,
        opts: ETO_OPTIONS,
        clip: Option<&RECT>,
        text: &str,
    ) {
        let chars: Vec<char> = text.chars().collect();
        let mut x = x;
        for (fi, s, e) in self.runs(hdc, &chars) {
            let old = SelectObject(hdc, self.fonts[fi].into());
            let seg: String = chars[s..e].iter().collect();
            let w: Vec<u16> = seg.encode_utf16().collect();
            let _ = ExtTextOutW(
                hdc,
                x,
                y,
                opts,
                clip.map(|r| r as *const RECT),
                PCWSTR(w.as_ptr()),
                w.len() as u32,
                None,
            );
            let mut sz = SIZE::default();
            let _ = GetTextExtentPoint32W(hdc, &w, &mut sz);
            x += sz.cx;
            SelectObject(hdc, old);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::families;

    #[test]
    fn families_trims_and_drops_empty() {
        assert_eq!(
            families(" D2Coding , JetBrainsMono Nerd Font,, "),
            vec![
                "D2Coding".to_string(),
                "JetBrainsMono Nerd Font".to_string()
            ]
        );
        assert!(families("").is_empty());
    }
}
