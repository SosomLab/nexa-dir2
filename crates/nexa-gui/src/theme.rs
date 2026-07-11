//! 시맨틱 테마 토큰 — 원본 nexa-dir docs/39 §4의 토큰 체계·팔레트를 차용.
//! 토큰 키는 안정 계약(원본 규약): rename 시 마이그레이션 표를 남긴다.
//! 기본값은 **다크**(디자인 규약 DR-5: 고밀도·다크·키보드 우선). 모드 선택·영속은 M2 테마 시스템.

/// sRGB 불투명 색. `from_hex(0xRRGGBB)`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn from_hex(rgb: u32) -> Self {
        Color {
            r: ((rgb >> 16) & 0xFF) as u8,
            g: ((rgb >> 8) & 0xFF) as u8,
            b: (rgb & 0xFF) as u8,
        }
    }
}

/// 시맨틱 색 토큰(원본 docs/39 §4 `Nexa*Brush` 대응 + 커스텀 드로잉에 필요한 텍스트·행 교대 토큰 신설).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Theme {
    /// 창 루트 (원본 `NexaWindowBackgroundBrush`)
    pub window_bg: Color,
    /// 메뉴·도구모음·런처 (`NexaChromeBackgroundBrush`)
    pub chrome_bg: Color,
    /// 파일 패널 (`NexaPanelBackgroundBrush`)
    pub panel_bg: Color,
    /// 행 교대 음영 (신설 — 커스텀 드로잉 리스트 전용)
    pub panel_bg_alt: Color,
    /// 탭 바 (`NexaTabBarBackgroundBrush`)
    pub tab_bar_bg: Color,
    /// 컬럼 헤더 (`NexaHeaderBackgroundBrush`)
    pub header_bg: Color,
    /// 경로바 등 입력 필드 (`NexaFieldBackgroundBrush`)
    pub field_bg: Color,
    /// 하단 도킹 (`NexaBottomDockBackgroundBrush`)
    pub bottom_dock_bg: Color,
    /// 상태바 (`NexaStatusBarBackgroundBrush`)
    pub status_bar_bg: Color,
    /// 영역 경계선·스플리터 (`NexaBorderBrush`)
    pub border: Color,
    /// 강조 — 활성 탭 줄·삽입 표시 (`NexaAccentBrush`)
    pub accent: Color,
    /// 본문 텍스트 (신설 — WinUI 기본값이던 것을 명시 토큰화)
    pub text: Color,
    /// 보조 텍스트 (신설)
    pub text_dim: Color,
}

impl Theme {
    /// 다크 팔레트 — 원본 docs/39 §4 Dark 열.
    pub const fn dark() -> Self {
        Theme {
            window_bg: Color::from_hex(0x14161A),
            chrome_bg: Color::from_hex(0x1E2228),
            panel_bg: Color::from_hex(0x191C21),
            panel_bg_alt: Color::from_hex(0x1F242B),
            tab_bar_bg: Color::from_hex(0x232830),
            header_bg: Color::from_hex(0x262B33),
            field_bg: Color::from_hex(0x262B33),
            bottom_dock_bg: Color::from_hex(0x1B1F25),
            status_bar_bg: Color::from_hex(0x1E2228),
            border: Color::from_hex(0x363C46),
            accent: Color::from_hex(0x3D8BFF),
            text: Color::from_hex(0xD6DAE0),
            text_dim: Color::from_hex(0x8A919C),
        }
    }

    /// 라이트 팔레트 — 원본 docs/39 §4 Light 열.
    pub const fn light() -> Self {
        Theme {
            window_bg: Color::from_hex(0xF6F7F9),
            chrome_bg: Color::from_hex(0xEEF1F5),
            panel_bg: Color::from_hex(0xFFFFFF),
            panel_bg_alt: Color::from_hex(0xF5F7FA),
            tab_bar_bg: Color::from_hex(0xE6EAF0),
            header_bg: Color::from_hex(0xE9ECF1),
            field_bg: Color::from_hex(0xFFFFFF),
            bottom_dock_bg: Color::from_hex(0xF1F3F6),
            status_bar_bg: Color::from_hex(0xEEF1F5),
            border: Color::from_hex(0xD5DAE1),
            accent: Color::from_hex(0x3D8BFF),
            text: Color::from_hex(0x1B1F26),
            text_dim: Color::from_hex(0x6B7280),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Theme::dark()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_hex_unpacks_channels() {
        let c = Color::from_hex(0x3D8BFF);
        assert_eq!((c.r, c.g, c.b), (0x3D, 0x8B, 0xFF));
    }

    #[test]
    fn default_is_dark() {
        assert_eq!(Theme::default(), Theme::dark());
    }
}
