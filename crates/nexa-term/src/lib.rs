//! nexa-term — 터미널 **VT/ANSI 파서 + 화면 버퍼**(M4-3). **원본 이식**:
//! `app/Nexa.App/Terminal/VtScreen.cs`(BP-T2 — docs/37).
//!
//! ConPTY가 내보내는 VT 시퀀스를 해석해 **셀 그리드**(문자·색·속성)로 유지한다.
//! 지원: 출력 문자·CR/LF/BS/HT, SGR(16/256/트루컬러·굵게·반전·faint), 커서 이동
//! (CUP/CUU·D·F·B/CHA/VPA/CNL/CPL), 지우기(ED/EL/ECH), 삽입/삭제(ICH/DCH/IL/DL),
//! 스크롤(SU/SD·DECSTBM 마진·스크롤백 보존), DECSC/DECRC. 렌더·ConPTY 배선은 앱(win.rs).
//! 플랫폼 중립 — 전 플랫폼 테스트.

/// 터미널 셀 하나 — 문자 + 전경/배경색(**기호 색** — [`TermPalette::resolve`]) + 굵게/반전/흐리게(faint).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TermCell {
    pub ch: char,
    /// 기호 색(u32 인코딩 — [`DEFAULT_FG`]/[`ANSI_TAG`]/트루컬러). 렌더 시 팔레트로 해석.
    pub fg: u32,
    pub bg: u32,
    pub bold: bool,
    pub reverse: bool,
    /// SGR 2 — PSReadLine 인라인 예측 등이 연한 회색으로 표시.
    pub faint: bool,
}

impl TermCell {
    fn blank(fg: u32, bg: u32) -> TermCell {
        TermCell {
            ch: ' ',
            fg,
            bg,
            bold: false,
            reverse: false,
            faint: false,
        }
    }
}

// ── 셀 색 인코딩(09-04 라이트 팔레트) ─────────────────────────────────────
// 셀은 **해석된 색이 아니라 기호 색**을 담는다. 그래야 테마 전환(F6) 순간 스크롤백까지
// 새 팔레트로 다시 칠해진다. `TermCell`을 16바이트로 유지하려고 u32의 알파 바이트를 태그로 쓴다:
//   0xFF_RRGGBB = 트루컬러(SGR 38;2 · 256색 큐브/그레이 — 테마 무관)
//   0x00_000000 / 0x00_000001 = 기본 전경/배경(SGR 39/49·리셋)
//   0x01_0000ii = ANSI 16색 인덱스 ii(SGR 30~37/90~97 · 256색 0~15)
/// 기본 전경(기호) — 팔레트 `fg`로 해석.
pub const DEFAULT_FG: u32 = 0x0000_0000;
/// 기본 배경(기호) — 팔레트 `bg`로 해석.
pub const DEFAULT_BG: u32 = 0x0000_0001;
/// ANSI 16색 태그(알파 바이트 0x01) — 하위 바이트가 인덱스.
pub const ANSI_TAG: u32 = 0x0100_0000;
const MAX_SCROLLBACK: usize = 800;

/// 터미널 색 팔레트 — 기본 전경/배경 + ANSI 16색(0xFFRRGGBB). 앱 테마(다크/라이트)가 고른다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TermPalette {
    pub fg: u32,
    pub bg: u32,
    pub ansi: [u32; 16],
}

impl TermPalette {
    /// 다크 — Campbell(Windows Terminal 기본) 16색 + 앱 다크 토큰보다 한 단 어두운 배경(M4-3 현행).
    pub const fn dark() -> Self {
        TermPalette {
            fg: 0xFFE6_E6E6,
            bg: 0xFF0C_0F12,
            ansi: [
                0xFF0C_0C0C,
                0xFFC5_0F1F,
                0xFF13_A10E,
                0xFFC1_9C00,
                0xFF00_37DA,
                0xFF88_1798,
                0xFF3A_96DD,
                0xFFCC_CCCC,
                0xFF76_7676,
                0xFFE7_4856,
                0xFF16_C60C,
                0xFFF9_F1A5,
                0xFF3B_78FF,
                0xFFB4_009E,
                0xFF61_D6D6,
                0xFFF2_F2F2,
            ],
        }
    }

    /// 라이트 — **GitHub Light(Primer `color.ansi.*` light)** 16색 + 앱 라이트 `text`/`panel_bg`.
    /// 선정 근거(09-04): ① 흰 배경에서 16색 전부 대비 **≥3:1**(Primer가 AA 목표로 설계 — 테스트로
    /// 고정) ② pwsh/PSReadLine 기본이 숫자·멤버에 **bright white(97)**·타입에 white(37)를 쓰므로
    /// 흰색 계열을 **진한 회색으로 매핑**해야 글자가 사라지지 않는다(One Half Light·Solarized Light·
    /// Tango Light는 white≈배경이라 탈락, VS Code Light+는 bright green/yellow 2.1:1이라 탈락)
    /// ③ 무채색(24292F·57606A·6E7781)이 앱 라이트 토큰(1B1F26·6B7280)과 같은 청회색 계열.
    pub const fn light() -> Self {
        TermPalette {
            fg: 0xFF1B_1F26,
            bg: 0xFFFF_FFFF,
            ansi: [
                0xFF24_292F, // black
                0xFFCF_222E, // red
                0xFF11_6329, // green
                0xFF4D_2D00, // yellow(갈색 — 흰 배경에서 노랑은 읽히지 않는다)
                0xFF09_69DA, // blue
                0xFF82_50DF, // magenta
                0xFF1B_7C83, // cyan
                0xFF6E_7781, // white → 중간 회색
                0xFF57_606A, // bright black
                0xFFA4_0E26, // bright red
                0xFF1A_7F37, // bright green
                0xFF63_3C01, // bright yellow
                0xFF21_8BFF, // bright blue
                0xFFA4_75F9, // bright magenta
                0xFF31_92AA, // bright cyan
                0xFF8C_959F, // bright white → 밝은 회색(가장 옅지만 3:1 유지)
            ],
        }
    }

    /// 기호 색 → 불투명 0xFFRRGGBB.
    pub fn resolve(&self, c: u32) -> u32 {
        match c >> 24 {
            0xFF => c,
            0x00 => {
                if c == DEFAULT_BG {
                    self.bg
                } else {
                    self.fg
                }
            }
            _ => self.ansi[(c & 0xF) as usize],
        }
    }
}

/// 이름 있는 터미널 색 스킴(09-04 — 사용자 요청 "라이트·다크·그외 추천"). `id`는 설정 값(안정 계약).
#[derive(Clone, Copy, Debug)]
pub struct TermScheme {
    pub id: &'static str,
    pub name: &'static str,
    /// 어두운 배경인가 — 설정 창 라벨 태그·다크/라이트 기본 후보 분류.
    pub dark: bool,
    pub palette: TermPalette,
}

const fn pal(fg: u32, bg: u32, ansi: [u32; 16]) -> TermPalette {
    TermPalette { fg, bg, ansi }
}

/// 다크 모드 기본 스킴 id(Campbell).
pub const DEFAULT_DARK_ID: &str = "campbell";
/// 라이트 모드 기본 스킴 id(GitHub Light).
pub const DEFAULT_LIGHT_ID: &str = "github-light";

/// 내장 스킴 — 다크 9 + 라이트 6. 값은 각 원전(Windows Terminal defaults.json · Dracula · Nord ·
/// Gruvbox · Catppuccin · Tokyo Night · Primer)의 공개 팔레트(전부 MIT) 그대로. 순서 = 설정 창 표시 순.
pub const SCHEMES: &[TermScheme] = &[
    TermScheme {
        id: "campbell",
        name: "Campbell",
        dark: true,
        palette: TermPalette::dark(),
    },
    TermScheme {
        id: "one-half-dark",
        name: "One Half Dark",
        dark: true,
        palette: pal(
            0xFFDC_DFE4,
            0xFF28_2C34,
            [
                0xFF28_2C34,
                0xFFE0_6C75,
                0xFF98_C379,
                0xFFE5_C07B,
                0xFF61_AFEF,
                0xFFC6_78DD,
                0xFF56_B6C2,
                0xFFDC_DFE4,
                0xFF5A_6374,
                0xFFE0_6C75,
                0xFF98_C379,
                0xFFE5_C07B,
                0xFF61_AFEF,
                0xFFC6_78DD,
                0xFF56_B6C2,
                0xFFDC_DFE4,
            ],
        ),
    },
    TermScheme {
        id: "solarized-dark",
        name: "Solarized Dark",
        dark: true,
        palette: pal(
            0xFF83_9496,
            0xFF00_2B36,
            [
                0xFF00_2B36,
                0xFFDC_322F,
                0xFF85_9900,
                0xFFB5_8900,
                0xFF26_8BD2,
                0xFFD3_3682,
                0xFF2A_A198,
                0xFFEE_E8D5,
                0xFF07_3642,
                0xFFCB_4B16,
                0xFF58_6E75,
                0xFF65_7B83,
                0xFF83_9496,
                0xFF6C_71C4,
                0xFF93_A1A1,
                0xFFFD_F6E3,
            ],
        ),
    },
    TermScheme {
        id: "tango-dark",
        name: "Tango Dark",
        dark: true,
        palette: pal(
            0xFFD3_D7CF,
            0xFF00_0000,
            [
                0xFF00_0000,
                0xFFCC_0000,
                0xFF4E_9A06,
                0xFFC4_A000,
                0xFF34_65A4,
                0xFF75_507B,
                0xFF06_989A,
                0xFFD3_D7CF,
                0xFF55_5753,
                0xFFEF_2929,
                0xFF8A_E234,
                0xFFFC_E94F,
                0xFF72_9FCF,
                0xFFAD_7FA8,
                0xFF34_E2E2,
                0xFFEE_EEEC,
            ],
        ),
    },
    TermScheme {
        id: "dracula",
        name: "Dracula",
        dark: true,
        palette: pal(
            0xFFF8_F8F2,
            0xFF28_2A36,
            [
                0xFF21_222C,
                0xFFFF_5555,
                0xFF50_FA7B,
                0xFFF1_FA8C,
                0xFFBD_93F9,
                0xFFFF_79C6,
                0xFF8B_E9FD,
                0xFFF8_F8F2,
                0xFF62_72A4,
                0xFFFF_6E6E,
                0xFF69_FF94,
                0xFFFF_FFA5,
                0xFFD6_ACFF,
                0xFFFF_92DF,
                0xFFA4_FFFF,
                0xFFFF_FFFF,
            ],
        ),
    },
    TermScheme {
        id: "nord",
        name: "Nord",
        dark: true,
        palette: pal(
            0xFFD8_DEE9,
            0xFF2E_3440,
            [
                0xFF3B_4252,
                0xFFBF_616A,
                0xFFA3_BE8C,
                0xFFEB_CB8B,
                0xFF81_A1C1,
                0xFFB4_8EAD,
                0xFF88_C0D0,
                0xFFE5_E9F0,
                0xFF4C_566A,
                0xFFBF_616A,
                0xFFA3_BE8C,
                0xFFEB_CB8B,
                0xFF81_A1C1,
                0xFFB4_8EAD,
                0xFF8F_BCBB,
                0xFFEC_EFF4,
            ],
        ),
    },
    TermScheme {
        id: "gruvbox-dark",
        name: "Gruvbox Dark",
        dark: true,
        palette: pal(
            0xFFEB_DBB2,
            0xFF28_2828,
            [
                0xFF28_2828,
                0xFFCC_241D,
                0xFF98_971A,
                0xFFD7_9921,
                0xFF45_8588,
                0xFFB1_6286,
                0xFF68_9D6A,
                0xFFA8_9984,
                0xFF92_8374,
                0xFFFB_4934,
                0xFFB8_BB26,
                0xFFFA_BD2F,
                0xFF83_A598,
                0xFFD3_869B,
                0xFF8E_C07C,
                0xFFEB_DBB2,
            ],
        ),
    },
    TermScheme {
        id: "catppuccin-mocha",
        name: "Catppuccin Mocha",
        dark: true,
        palette: pal(
            0xFFCD_D6F4,
            0xFF1E_1E2E,
            [
                0xFF45_475A,
                0xFFF3_8BA8,
                0xFFA6_E3A1,
                0xFFF9_E2AF,
                0xFF89_B4FA,
                0xFFF5_C2E7,
                0xFF94_E2D5,
                0xFFBA_C2DE,
                0xFF58_5B70,
                0xFFF3_8BA8,
                0xFFA6_E3A1,
                0xFFF9_E2AF,
                0xFF89_B4FA,
                0xFFF5_C2E7,
                0xFF94_E2D5,
                0xFFA6_ADC8,
            ],
        ),
    },
    TermScheme {
        id: "tokyo-night",
        name: "Tokyo Night",
        dark: true,
        palette: pal(
            0xFFC0_CAF5,
            0xFF1A_1B26,
            [
                0xFF15_161E,
                0xFFF7_768E,
                0xFF9E_CE6A,
                0xFFE0_AF68,
                0xFF7A_A2F7,
                0xFFBB_9AF7,
                0xFF7D_CFFF,
                0xFFA9_B1D6,
                0xFF41_4868,
                0xFFF7_768E,
                0xFF9E_CE6A,
                0xFFE0_AF68,
                0xFF7A_A2F7,
                0xFFBB_9AF7,
                0xFF7D_CFFF,
                0xFFC0_CAF5,
            ],
        ),
    },
    TermScheme {
        id: "github-light",
        name: "GitHub Light",
        dark: false,
        palette: TermPalette::light(),
    },
    TermScheme {
        id: "one-half-light",
        name: "One Half Light",
        dark: false,
        palette: pal(
            0xFF38_3A42,
            0xFFFA_FAFA,
            [
                0xFF38_3A42,
                0xFFE4_5649,
                0xFF50_A14F,
                0xFFC1_8401,
                0xFF01_84BC,
                0xFFA6_26A4,
                0xFF09_97B3,
                0xFFFA_FAFA,
                0xFF4F_525D,
                0xFFDF_6C75,
                0xFF98_C379,
                0xFFE4_C07A,
                0xFF61_AFEF,
                0xFFC5_77DD,
                0xFF56_B5C1,
                0xFFFF_FFFF,
            ],
        ),
    },
    TermScheme {
        id: "solarized-light",
        name: "Solarized Light",
        dark: false,
        palette: pal(
            0xFF65_7B83,
            0xFFFD_F6E3,
            [
                0xFF00_2B36,
                0xFFDC_322F,
                0xFF85_9900,
                0xFFB5_8900,
                0xFF26_8BD2,
                0xFFD3_3682,
                0xFF2A_A198,
                0xFFEE_E8D5,
                0xFF07_3642,
                0xFFCB_4B16,
                0xFF58_6E75,
                0xFF65_7B83,
                0xFF83_9496,
                0xFF6C_71C4,
                0xFF93_A1A1,
                0xFFFD_F6E3,
            ],
        ),
    },
    TermScheme {
        id: "tango-light",
        name: "Tango Light",
        dark: false,
        palette: pal(
            0xFF55_5753,
            0xFFFF_FFFF,
            [
                0xFF00_0000,
                0xFFCC_0000,
                0xFF4E_9A06,
                0xFFC4_A000,
                0xFF34_65A4,
                0xFF75_507B,
                0xFF06_989A,
                0xFFD3_D7CF,
                0xFF55_5753,
                0xFFEF_2929,
                0xFF8A_E234,
                0xFFFC_E94F,
                0xFF72_9FCF,
                0xFFAD_7FA8,
                0xFF34_E2E2,
                0xFFEE_EEEC,
            ],
        ),
    },
    TermScheme {
        id: "gruvbox-light",
        name: "Gruvbox Light",
        dark: false,
        palette: pal(
            0xFF3C_3836,
            0xFFFB_F1C7,
            [
                0xFFFB_F1C7,
                0xFFCC_241D,
                0xFF98_971A,
                0xFFD7_9921,
                0xFF45_8588,
                0xFFB1_6286,
                0xFF68_9D6A,
                0xFF7C_6F64,
                0xFF92_8374,
                0xFF9D_0006,
                0xFF79_740E,
                0xFFB5_7614,
                0xFF07_6678,
                0xFF8F_3F71,
                0xFF42_7B58,
                0xFF3C_3836,
            ],
        ),
    },
    TermScheme {
        id: "catppuccin-latte",
        name: "Catppuccin Latte",
        dark: false,
        palette: pal(
            0xFF4C_4F69,
            0xFFEF_F1F5,
            [
                0xFF5C_5F77,
                0xFFD2_0F39,
                0xFF40_A02B,
                0xFFDF_8E1D,
                0xFF1E_66F5,
                0xFFEA_76CB,
                0xFF17_9299,
                0xFFAC_B0BE,
                0xFF6C_6F85,
                0xFFD2_0F39,
                0xFF40_A02B,
                0xFFDF_8E1D,
                0xFF1E_66F5,
                0xFFEA_76CB,
                0xFF17_9299,
                0xFFBC_C0CC,
            ],
        ),
    },
];

/// id → 스킴(모르는 id = None — 호출자가 기본으로 폴백).
pub fn scheme(id: &str) -> Option<&'static TermScheme> {
    SCHEMES.iter().find(|s| s.id == id)
}

/// 터미널 테마 **선택 규칙**(09-04 — 사용자 요청 "다크 모드에서 라이트 터미널도 고를 수 있게"):
///
/// | `selector`(설정 `term_theme`) | 결과 |
/// | --- | --- |
/// | `system` | 앱 테마 추종 — 다크면 `dark_default`, 라이트면 `light_default` |
/// | `dark` / `light` | 앱 테마와 **무관하게** 다크/라이트 **기본 스킴** 강제 |
/// | 스킴 id | 앱 테마와 무관하게 그 스킴(라이트 앱 + 다크 스킴, 그 역도 허용) |
///
/// `dark_default`/`light_default`(설정 `term_theme_dark`/`term_theme_light`)가 모르는 id면 내장
/// 기본([`DEFAULT_DARK_ID`]/[`DEFAULT_LIGHT_ID`])으로, `selector`가 모르는 id면 `system`으로 폴백 —
/// 설정 파일을 손으로 고쳐도 터미널이 검은 화면이 되지 않는다.
pub fn resolve_scheme(
    selector: &str,
    dark_default: &str,
    light_default: &str,
    app_is_dark: bool,
) -> &'static TermScheme {
    let dark = scheme(dark_default).unwrap_or_else(|| scheme(DEFAULT_DARK_ID).unwrap());
    let light = scheme(light_default).unwrap_or_else(|| scheme(DEFAULT_LIGHT_ID).unwrap());
    let by_mode = || if app_is_dark { dark } else { light };
    match selector {
        "dark" => dark,
        "light" => light,
        "system" => by_mode(),
        id => scheme(id).unwrap_or_else(by_mode),
    }
}

/// 파서 상태.
#[derive(Clone, Copy, PartialEq, Eq)]
enum S {
    Ground,
    Esc,
    Csi,
    Osc,
}

/// VT/ANSI 파서 + 화면 버퍼 — 원본 VtScreen.
pub struct VtScreen {
    cols: usize,
    rows: usize,
    screen: Vec<Vec<TermCell>>,
    scrollback: Vec<Vec<TermCell>>,
    cx: usize,
    cy: usize,
    saved_cx: usize,
    saved_cy: usize,
    /// 스크롤 마진(DECSTBM, 포함 범위) — 기본 전체 화면.
    top: usize,
    bottom: usize,
    fg: u32,
    bg: u32,
    bold: bool,
    reverse: bool,
    faint: bool,
    state: S,
    /// CSI private 마커('?') — DECSET/DECRST(h/l) 판별(X-5 마우스 모드).
    private: bool,
    /// 마우스 추적 모드(DECSET 1000/1002/1003 — 0=꺼짐). TUI(Zellij 등)가 켠다.
    mouse_mode: u16,
    /// SGR 확장 마우스 인코딩(DECSET 1006).
    mouse_sgr: bool,
    pars: Vec<i32>,
    cur: i32, // 현재 파라미터 누적(-1=없음)
}

impl VtScreen {
    pub fn new(cols: usize, rows: usize) -> VtScreen {
        let mut s = VtScreen {
            cols: 0,
            rows: 0,
            screen: Vec::new(),
            scrollback: Vec::new(),
            cx: 0,
            cy: 0,
            saved_cx: 0,
            saved_cy: 0,
            top: 0,
            bottom: 0,
            fg: DEFAULT_FG,
            bg: DEFAULT_BG,
            bold: false,
            reverse: false,
            faint: false,
            state: S::Ground,
            private: false,
            mouse_mode: 0,
            mouse_sgr: false,
            pars: Vec::new(),
            cur: -1,
        };
        s.resize(cols, rows);
        s
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    /// 커서 열(0-기준, 가시 화면 좌표) — 렌더 캐럿용.
    pub fn cursor_col(&self) -> usize {
        self.cx
    }

    /// 커서 행(0-기준, 가시 화면 내). 절대 라인 = [`Self::scrollback_count`] + 이 값.
    pub fn cursor_row(&self) -> usize {
        self.cy
    }

    pub fn scrollback_count(&self) -> usize {
        self.scrollback.len()
    }

    /// 마우스 추적 모드(X-5 — TUI가 DECSET으로 켬): `(모드 1000/1002/1003, SGR 인코딩)`.
    /// 꺼져 있으면 `None` — 호스트는 로컬 선택/스크롤을 유지한다.
    pub fn mouse_mode(&self) -> Option<(u16, bool)> {
        if self.mouse_mode == 0 {
            None
        } else {
            Some((self.mouse_mode, self.mouse_sgr))
        }
    }

    /// 총 라인 수(스크롤백 + 현재 화면).
    pub fn line_count(&self) -> usize {
        self.scrollback.len() + self.rows
    }

    /// 절대 라인 인덱스의 셀들(스크롤백 → 화면 순) — 실체화 없는 참조(원본 감사 004 계승).
    pub fn line_at(&self, index: usize) -> &[TermCell] {
        if index < self.scrollback.len() {
            &self.scrollback[index]
        } else {
            &self.screen[index - self.scrollback.len()]
        }
    }

    /// 절대 라인 범위의 텍스트 추출(양끝 포함) — 마우스 선택 복사용.
    /// 전각 연속 셀('\0')은 건너뛰고 각 줄 우측 공백 트림, 줄 구분 CRLF.
    pub fn get_text(
        &self,
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
    ) -> String {
        let count = self.line_count();
        if count == 0 {
            return String::new();
        }
        let sl = start_line.min(count - 1);
        let el = end_line.min(count - 1);
        let mut out = String::new();
        for li in sl..=el {
            let row = self.line_at(li);
            let c0 = if li == sl { start_col } else { 0 };
            let c1 = if li == el {
                end_col.min(row.len().saturating_sub(1))
            } else {
                row.len().saturating_sub(1)
            };
            let mut line = String::new();
            for cell in row.iter().take(c1 + 1).skip(c0) {
                if cell.ch != '\0' {
                    line.push(cell.ch);
                }
            }
            out.push_str(line.trim_end());
            if li < el {
                out.push_str("\r\n");
            }
        }
        out
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        if cols == self.cols && rows == self.rows {
            return;
        }
        let mut next = Vec::with_capacity(rows);
        for r in 0..rows {
            let mut row = vec![TermCell::blank(DEFAULT_FG, DEFAULT_BG); cols];
            if r < self.rows && r < self.screen.len() {
                let copy = cols.min(self.cols);
                row[..copy].copy_from_slice(&self.screen[r][..copy]);
            }
            next.push(row);
        }
        self.screen = next;
        self.cols = cols;
        self.rows = rows;
        self.cx = self.cx.min(cols - 1);
        self.cy = self.cy.min(rows - 1);
        self.top = 0;
        self.bottom = rows - 1; // 리사이즈 시 스크롤 마진 리셋(DECSTBM 관례)
    }

    pub fn feed(&mut self, data: &str) {
        for ch in data.chars() {
            match self.state {
                S::Ground => self.ground(ch),
                S::Esc => self.escape(ch),
                S::Csi => self.csi(ch),
                S::Osc => self.osc(ch),
            }
        }
    }

    fn ground(&mut self, ch: char) {
        match ch {
            '\x1B' => self.state = S::Esc,
            '\r' => self.cx = 0,
            '\n' => self.line_feed(),
            '\x08' => self.cx = self.cx.saturating_sub(1),
            '\t' => self.cx = (self.cols - 1).min((self.cx / 8 + 1) * 8),
            '\x07' => {} // BEL
            _ => {
                if ch >= ' ' {
                    self.put(ch);
                }
            }
        }
    }

    fn escape(&mut self, ch: char) {
        self.state = S::Ground;
        match ch {
            '[' => {
                self.state = S::Csi;
                self.pars.clear();
                self.cur = -1;
                self.private = false;
            }
            ']' => self.state = S::Osc,
            '(' | ')' | '*' | '+' => {} // charset 지정 — 다음 1글자 무시(간이·원본 동일)
            'M' => self.reverse_index(),
            'D' => self.line_feed(), // IND
            'E' => {
                self.cx = 0;
                self.line_feed(); // NEL
            }
            '7' => {
                self.saved_cx = self.cx;
                self.saved_cy = self.cy; // DECSC
            }
            '8' => self.restore_cursor(), // DECRC
            '=' | '>' => {}
            'c' => self.full_reset(),
            _ => {}
        }
    }

    fn csi(&mut self, ch: char) {
        if ch == '?' {
            self.private = true; // private 마커(DECSET/DECRST — X-5 마우스 모드 추적)
            return;
        }
        if ch.is_ascii_digit() {
            self.cur = self.cur.max(0) * 10 + (ch as i32 - '0' as i32);
            return;
        }
        if ch == ';' {
            self.pars.push(self.cur.max(0));
            self.cur = -1;
            return;
        }
        // 중간 바이트 무시, 최종 바이트에서 디스패치
        if ('\x40'..='\x7E').contains(&ch) {
            self.pars.push(self.cur.max(0));
            self.dispatch(ch);
            self.state = S::Ground;
        }
    }

    fn osc(&mut self, ch: char) {
        // OSC 종료: BEL 또는 ST(ESC \) — 간이(창 제목 등 무시, 원본 동일)
        if ch == '\x07' {
            self.state = S::Ground;
        } else if ch == '\x1B' {
            self.state = S::Esc;
        }
    }

    fn par(&self, i: usize, def: usize) -> usize {
        match self.pars.get(i) {
            Some(&v) if v > 0 => v as usize,
            Some(&0) if def == 0 => 0,
            _ => def,
        }
    }

    fn dispatch(&mut self, fin: char) {
        // DECSET/DECRST(CSI ? Pm h/l) — 마우스 추적 모드만 추적(X-5), 그 외 private은 무시
        if self.private {
            self.private = false;
            if fin == 'h' || fin == 'l' {
                let on = fin == 'h';
                for &p in &self.pars {
                    match p {
                        1000 | 1002 | 1003 => {
                            self.mouse_mode = if on { p as u16 } else { 0 };
                        }
                        1006 => self.mouse_sgr = on,
                        _ => {}
                    }
                }
            }
            return;
        }
        let p0 = self.pars.first().copied().unwrap_or(0).max(0) as usize;
        let n1 = p0.max(1);
        match fin {
            'm' => self.sgr(),
            'H' | 'f' => {
                self.cy = (self.par(0, 1) - 1).min(self.rows - 1);
                self.cx = (self.par(1, 1) - 1).min(self.cols - 1);
            }
            'A' => self.cy = self.cy.saturating_sub(n1),
            'B' => self.cy = (self.cy + n1).min(self.rows - 1),
            'C' => self.cx = (self.cx + n1).min(self.cols - 1),
            'D' => self.cx = self.cx.saturating_sub(n1),
            'G' => self.cx = (n1 - 1).min(self.cols - 1),
            'd' => self.cy = (n1 - 1).min(self.rows - 1),
            'E' => {
                self.cy = (self.cy + n1).min(self.rows - 1);
                self.cx = 0; // CNL
            }
            'F' => {
                self.cy = self.cy.saturating_sub(n1);
                self.cx = 0; // CPL
            }
            'S' => self.scroll_up(n1),   // SU
            'T' => self.scroll_down(n1), // SD
            'r' => {
                // DECSTBM — 스크롤 마진(미구현 시 영역 스크롤 어긋남: ls 등)
                self.top = (self.par(0, 1) - 1).min(self.rows - 1);
                self.bottom = (self.par(1, self.rows) - 1).min(self.rows - 1);
                if self.bottom <= self.top {
                    self.top = 0;
                    self.bottom = self.rows - 1; // 무효 → 전체 화면
                }
                self.cx = 0;
                self.cy = 0; // DECSTBM은 커서 홈(스펙)
            }
            'J' => self.erase_display(p0),
            'K' => self.erase_line(p0),
            'L' => self.insert_lines(n1),
            'M' => self.delete_lines(n1),
            'P' => self.delete_chars(n1),
            '@' => self.insert_chars(n1),
            'X' => self.erase_chars(n1), // ECH — 잔상 방지 필수(원본 BUG 교훈)
            's' => {
                self.saved_cx = self.cx;
                self.saved_cy = self.cy;
            }
            'u' => self.restore_cursor(),
            _ => {} // 미지원 무시
        }
    }

    fn cell(&self, ch: char) -> TermCell {
        TermCell {
            ch,
            fg: self.fg,
            bg: self.bg,
            bold: self.bold,
            reverse: self.reverse,
            faint: self.faint,
        }
    }

    fn put(&mut self, ch: char) {
        // 셸(ConPTY)은 CJK 전각을 2칸으로 계산 — 버퍼도 동일 전진해야 커서가 맞는다.
        let w = if is_wide(ch) { 2 } else { 1 };
        if self.cx + w > self.cols {
            self.cx = 0;
            self.line_feed();
        }
        self.screen[self.cy][self.cx] = self.cell(ch);
        if w == 2 && self.cx + 1 < self.cols {
            self.screen[self.cy][self.cx + 1] = TermCell {
                ch: '\0', // 연속(continuation) 셀 — 렌더는 스킵
                ..self.cell('\0')
            };
        }
        self.cx += w;
    }

    fn line_feed(&mut self) {
        if self.cy == self.bottom {
            self.scroll_up(1); // 마진 하단 LF = 영역 스크롤(전체 마진이면 스크롤백 보존)
            return;
        }
        if self.cy < self.rows - 1 {
            self.cy += 1;
        }
    }

    /// SU — 전체 화면 마진이면 맨 위 줄을 스크롤백 보존, 부분 마진이면 영역만. 커서 불변.
    fn scroll_up(&mut self, n: usize) {
        let full = self.top == 0 && self.bottom == self.rows - 1;
        for _ in 0..n {
            let removed = std::mem::replace(
                &mut self.screen[self.top],
                vec![TermCell::blank(DEFAULT_FG, DEFAULT_BG); self.cols],
            );
            if full {
                self.scrollback.push(removed);
            }
            // top 줄을 빼내 빈 줄로 바꾼 뒤 아래로 회전 — 결과: 영역이 한 줄 위로
            self.screen[self.top..=self.bottom].rotate_left(1);
        }
        if full && self.scrollback.len() > MAX_SCROLLBACK {
            let excess = self.scrollback.len() - MAX_SCROLLBACK;
            self.scrollback.drain(0..excess);
        }
    }

    /// SD — 영역 위는 빈 줄, 맨 아래는 버림. 커서 불변.
    fn scroll_down(&mut self, n: usize) {
        for _ in 0..n {
            self.screen[self.top..=self.bottom].rotate_right(1);
            self.screen[self.top] = vec![TermCell::blank(DEFAULT_FG, DEFAULT_BG); self.cols];
        }
    }

    fn reverse_index(&mut self) {
        if self.cy == self.top {
            self.scroll_down(1);
        } else if self.cy > 0 {
            self.cy -= 1;
        }
    }

    fn blank_filled_row(&self) -> Vec<TermCell> {
        vec![TermCell::blank(self.fg, self.bg); self.cols]
    }

    fn erase_display(&mut self, mode: usize) {
        match mode {
            0 => {
                self.erase_line(0);
                for r in self.cy + 1..self.rows {
                    self.screen[r] = self.blank_filled_row();
                }
            }
            1 => {
                for r in 0..self.cy {
                    self.screen[r] = self.blank_filled_row();
                }
                self.erase_line(1);
            }
            2 => {
                for r in 0..self.rows {
                    self.screen[r] = self.blank_filled_row();
                }
            }
            3 => self.scrollback.clear(),
            _ => {}
        }
    }

    fn erase_line(&mut self, mode: usize) {
        let (from, to) = match mode {
            1 => (0, self.cx),
            2 => (0, self.cols - 1),
            _ => (self.cx, self.cols - 1),
        };
        let blank = TermCell::blank(self.fg, self.bg);
        for c in from..=to.min(self.cols - 1) {
            self.screen[self.cy][c] = blank;
        }
    }

    /// ECH — 커서부터 n칸 지움(커서 불이동). PSReadLine 백스페이스 재그리기 등.
    fn erase_chars(&mut self, n: usize) {
        let to = self.cols.min(self.cx + n);
        let blank = TermCell::blank(self.fg, self.bg);
        for c in self.cx..to {
            self.screen[self.cy][c] = blank;
        }
    }

    fn insert_lines(&mut self, n: usize) {
        for _ in 0..n {
            let row = self.blank_filled_row();
            self.screen[self.cy..self.rows].rotate_right(1);
            self.screen[self.cy] = row;
        }
    }

    fn delete_lines(&mut self, n: usize) {
        for _ in 0..n {
            self.screen[self.cy..self.rows].rotate_left(1);
            self.screen[self.rows - 1] = self.blank_filled_row();
        }
    }

    fn delete_chars(&mut self, n: usize) {
        let blank = TermCell::blank(self.fg, self.bg);
        let row = &mut self.screen[self.cy];
        for c in self.cx..self.cols {
            row[c] = if c + n < self.cols { row[c + n] } else { blank };
        }
    }

    fn insert_chars(&mut self, n: usize) {
        let blank = TermCell::blank(self.fg, self.bg);
        let row = &mut self.screen[self.cy];
        for c in (self.cx..self.cols).rev() {
            row[c] = if c >= self.cx + n { row[c - n] } else { blank };
        }
    }

    fn restore_cursor(&mut self) {
        self.cx = self.saved_cx.min(self.cols - 1);
        self.cy = self.saved_cy.min(self.rows - 1);
    }

    fn full_reset(&mut self) {
        self.fg = DEFAULT_FG;
        self.bg = DEFAULT_BG;
        self.bold = false;
        self.reverse = false;
        self.faint = false;
        self.cx = 0;
        self.cy = 0;
        self.top = 0;
        self.bottom = self.rows - 1;
        for r in 0..self.rows {
            self.screen[r] = vec![TermCell::blank(DEFAULT_FG, DEFAULT_BG); self.cols];
        }
    }

    // ── SGR (색/속성) ────────────────────────────────────────────
    fn sgr(&mut self) {
        if self.pars.is_empty() {
            self.pars.push(0);
        }
        let pars = std::mem::take(&mut self.pars);
        let mut i = 0;
        while i < pars.len() {
            let p = pars[i];
            match p {
                0 => {
                    self.fg = DEFAULT_FG;
                    self.bg = DEFAULT_BG;
                    self.bold = false;
                    self.reverse = false;
                    self.faint = false;
                }
                1 => self.bold = true,
                2 => self.faint = true,
                22 => {
                    self.bold = false;
                    self.faint = false;
                }
                7 => self.reverse = true,
                27 => self.reverse = false,
                39 => self.fg = DEFAULT_FG,
                49 => self.bg = DEFAULT_BG,
                38 | 48 => {
                    let mut color = None;
                    if i + 2 < pars.len() && pars[i + 1] == 5 {
                        color = Some(color256(pars[i + 2].clamp(0, 255) as usize));
                        i += 2;
                    } else if i + 4 < pars.len() && pars[i + 1] == 2 {
                        color = Some(rgb(pars[i + 2], pars[i + 3], pars[i + 4]));
                        i += 4;
                    }
                    if let Some(c) = color {
                        if p == 38 {
                            self.fg = c;
                        } else {
                            self.bg = c;
                        }
                    }
                }
                30..=37 => self.fg = ansi16((p - 30) as usize),
                40..=47 => self.bg = ansi16((p - 40) as usize),
                90..=97 => self.fg = ansi16((p - 90 + 8) as usize),
                100..=107 => self.bg = ansi16((p - 100 + 8) as usize),
                _ => {}
            }
            i += 1;
        }
        self.pars = pars;
        self.pars.clear();
    }
}

/// 전각(2칸) 문자인가 — wcwidth 근사(한글·CJK·전각 기호, BMP 주요 범위 — 원본 동일).
pub fn is_wide(ch: char) -> bool {
    let c = ch as u32;
    (0x1100..=0x115F).contains(&c)
        || (0x2E80..=0xA4CF).contains(&c)
        || (0xAC00..=0xD7A3).contains(&c)
        || (0xF900..=0xFAFF).contains(&c)
        || (0xFE30..=0xFE4F).contains(&c)
        || (0xFF00..=0xFF60).contains(&c)
        || (0xFFE0..=0xFFE6).contains(&c)
}

/// ANSI 16색 기호 값 — 팔레트 해석은 렌더 시([`TermPalette::resolve`]).
fn ansi16(i: usize) -> u32 {
    ANSI_TAG | (i.min(15) as u32)
}

fn color256(n: usize) -> u32 {
    if n < 16 {
        return ansi16(n);
    }
    if n < 232 {
        let c = n - 16;
        let (r, g, b) = (c / 36, (c % 36) / 6, c % 6);
        let conv = |v: usize| if v == 0 { 0 } else { 55 + v as i32 * 40 };
        return rgb(conv(r), conv(g), conv(b));
    }
    let gray = 8 + (n as i32 - 232) * 10;
    rgb(gray, gray, gray)
}

fn rgb(r: i32, g: i32, b: i32) -> u32 {
    0xFF00_0000 | (((r & 0xFF) as u32) << 16) | (((g & 0xFF) as u32) << 8) | ((b & 0xFF) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decset_mouse_modes_tracked() {
        // X-5: DECSET 1000/1002/1003(추적)·1006(SGR) — Zellij 등 TUI 마우스 전달용
        let mut s = VtScreen::new(10, 4);
        assert!(s.mouse_mode().is_none());
        s.feed("\x1b[?1000;1006h");
        assert_eq!(s.mouse_mode(), Some((1000, true)));
        s.feed("\x1b[?1002h");
        assert_eq!(s.mouse_mode(), Some((1002, true)));
        s.feed("\x1b[?1006l");
        assert_eq!(s.mouse_mode(), Some((1002, false)));
        s.feed("\x1b[?1002l");
        assert!(s.mouse_mode().is_none());
        // private 시퀀스가 일반 CSI에 영향 없음
        s.feed("\x1b[2;3H");
        assert_eq!((s.cursor_row(), s.cursor_col()), (1, 2));
    }

    fn text_of(s: &VtScreen, row: usize) -> String {
        s.line_at(s.scrollback_count() + row)
            .iter()
            .filter(|c| c.ch != '\0')
            .map(|c| c.ch)
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    #[test]
    fn put_wrap_and_scrollback() {
        let mut s = VtScreen::new(5, 2);
        s.feed("abcdefg"); // 5칸 랩 → 2행
        assert_eq!(text_of(&s, 0), "abcde");
        assert_eq!(text_of(&s, 1), "fg");
        s.feed("\r\nhij"); // 최하단 LF = 스크롤(첫 줄 스크롤백 보존)
        assert_eq!(s.scrollback_count(), 1);
        assert_eq!(text_of(&s, 1), "hij");
        assert_eq!(s.get_text(0, 0, 0, 4), "abcde", "스크롤백 텍스트 추출");
    }

    #[test]
    fn cup_and_erase() {
        let mut s = VtScreen::new(10, 3);
        s.feed("aaaaaaaaaa\r\nbbbbbbbbbb\r\ncccccccccc");
        s.feed("\x1B[2;3H"); // 2행 3열
        assert_eq!((s.cursor_row(), s.cursor_col()), (1, 2));
        s.feed("\x1B[K"); // EL 0 — 커서부터 끝
        assert_eq!(text_of(&s, 1), "bb");
        s.feed("\x1B[2J"); // ED 2 — 전체
        assert_eq!(text_of(&s, 0), "");
        assert_eq!(text_of(&s, 2), "");
    }

    #[test]
    fn sgr_colors_16_256_true() {
        let mut s = VtScreen::new(20, 1);
        s.feed("\x1B[31mR\x1B[38;5;196mX\x1B[38;2;1;2;3mT\x1B[0mn");
        let row = s.line_at(0);
        assert_eq!(row[0].fg, ANSI_TAG | 1, "ANSI 빨강 = 기호(인덱스 1)");
        assert_eq!(row[1].fg, 0xFFFF_0000, "256색 196 = 순빨강(트루컬러)");
        assert_eq!(row[2].fg, 0xFF01_0203, "트루컬러");
        assert_eq!(row[3].fg, DEFAULT_FG, "리셋");
        // 같은 셀이 팔레트에 따라 다르게 해석된다(테마 전환 시 재도장 근거)
        let (d, l) = (TermPalette::dark(), TermPalette::light());
        assert_eq!(d.resolve(row[0].fg), 0xFFC5_0F1F, "다크 = Campbell 빨강");
        assert_eq!(l.resolve(row[0].fg), 0xFFCF_222E, "라이트 = Primer 빨강");
        assert_eq!(d.resolve(row[1].fg), 0xFFFF_0000, "트루컬러는 테마 무관");
        assert_eq!(l.resolve(DEFAULT_BG), 0xFFFF_FFFF);
        assert_eq!(l.resolve(DEFAULT_FG), 0xFF1B_1F26);
    }

    /// WCAG 상대 휘도·대비율.
    fn contrast(a: u32, b: u32) -> f64 {
        fn lum(c: u32) -> f64 {
            let ch = |v: u32| {
                let s = (v & 0xFF) as f64 / 255.0;
                if s <= 0.03928 {
                    s / 12.92
                } else {
                    ((s + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * ch(c >> 16) + 0.7152 * ch(c >> 8) + 0.0722 * ch(c)
        }
        let (l1, l2) = (lum(a), lum(b));
        (l1.max(l2) + 0.05) / (l1.min(l2) + 0.05)
    }

    #[test]
    fn light_palette_is_legible_on_its_background() {
        // 09-04 라이트 팔레트 선정 기준을 테스트로 고정: 16색 전부 배경 대비 ≥3:1 —
        // 특히 pwsh가 숫자·멤버에 쓰는 bright white(15)·타입의 white(7)가 흰 배경에서 사라지면 안 된다.
        let p = TermPalette::light();
        for (i, c) in p.ansi.iter().enumerate() {
            let r = contrast(*c, p.bg);
            assert!(r >= 3.0, "라이트 ANSI {i} = {c:#010X} 대비 {r:.2} < 3.0");
        }
        assert!(contrast(p.fg, p.bg) >= 7.0, "기본 전경은 AAA");
        // 다크(Campbell)는 원전 그대로라 이 기준을 두지 않는다 — 진파랑(4)이 2.3:1인 것은
        // Windows Terminal 기본값의 알려진 성질(실측 09-04). 기본 전경만 확인.
        let d = TermPalette::dark();
        assert!(contrast(d.fg, d.bg) >= 7.0, "다크 기본 전경은 AAA");
    }

    #[test]
    fn schemes_are_well_formed() {
        // id 유일·기본 2종 존재·다크/라이트 분류가 배경 휘도와 일치·전 색 불투명
        let mut ids: Vec<&str> = SCHEMES.iter().map(|s| s.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), SCHEMES.len(), "스킴 id 중복");
        assert!(scheme(DEFAULT_DARK_ID).is_some_and(|s| s.dark));
        assert!(scheme(DEFAULT_LIGHT_ID).is_some_and(|s| !s.dark));
        for s in SCHEMES {
            let bg = s.palette.bg;
            let lum = |c: u32| {
                0.2126 * ((c >> 16) & 0xFF) as f64
                    + 0.7152 * ((c >> 8) & 0xFF) as f64
                    + 0.0722 * (c & 0xFF) as f64
            };
            assert_eq!((lum(bg) < 128.0), s.dark, "{}: dark 분류 ≠ 배경 휘도", s.id);
            assert!(
                s.palette.fg >> 24 == 0xFF && bg >> 24 == 0xFF,
                "{}: 불투명",
                s.id
            );
            assert!(
                s.palette.ansi.iter().all(|c| c >> 24 == 0xFF),
                "{}: ANSI 불투명",
                s.id
            );
            // 원전 그대로인 스킴은 대비를 강제하지 않는다(Solarized 전경은 설계상 4.3:1) —
            // 최소 가독선(3:1)만 지킨다
            assert!(
                contrast(s.palette.fg, bg) >= 3.0,
                "{}: 기본 전경 최소 가독",
                s.id
            );
        }
    }

    #[test]
    fn resolve_scheme_selector_rules() {
        // 시스템 = 앱 테마 추종(각 모드 기본)
        assert_eq!(
            resolve_scheme("system", "campbell", "github-light", true).id,
            "campbell"
        );
        assert_eq!(
            resolve_scheme("system", "campbell", "github-light", false).id,
            "github-light"
        );
        // 모드 기본을 바꾸면 시스템·다크/라이트 강제가 그 값을 따른다
        assert_eq!(
            resolve_scheme("system", "nord", "solarized-light", true).id,
            "nord"
        );
        assert_eq!(
            resolve_scheme("system", "nord", "solarized-light", false).id,
            "solarized-light"
        );
        assert_eq!(
            resolve_scheme("dark", "nord", "solarized-light", false).id,
            "nord",
            "라이트 앱 + 다크 강제"
        );
        assert_eq!(
            resolve_scheme("light", "nord", "solarized-light", true).id,
            "solarized-light",
            "다크 앱 + 라이트 강제"
        );
        // 개별 스킴 = 앱 테마 무관
        assert_eq!(
            resolve_scheme("gruvbox-light", "campbell", "github-light", true).id,
            "gruvbox-light"
        );
        assert_eq!(
            resolve_scheme("dracula", "campbell", "github-light", false).id,
            "dracula"
        );
        // 폴백: 모르는 기본 id → 내장 기본 · 모르는 선택 → system
        assert_eq!(
            resolve_scheme("dark", "nope", "github-light", false).id,
            DEFAULT_DARK_ID
        );
        assert_eq!(
            resolve_scheme("system", "campbell", "nope", false).id,
            DEFAULT_LIGHT_ID
        );
        assert_eq!(
            resolve_scheme("nope", "campbell", "github-light", true).id,
            "campbell"
        );
        assert_eq!(
            resolve_scheme("", "campbell", "github-light", false).id,
            "github-light"
        );
    }

    #[test]
    fn sgr_bold_faint_reverse() {
        let mut s = VtScreen::new(10, 1);
        s.feed("\x1B[1;7mA\x1B[22;27m\x1B[2mB");
        let row = s.line_at(0);
        assert!(row[0].bold && row[0].reverse && !row[0].faint);
        assert!(!row[1].bold && !row[1].reverse && row[1].faint);
    }

    #[test]
    fn ech_erases_without_moving_cursor() {
        let mut s = VtScreen::new(10, 1);
        s.feed("abcdef\x1B[1;2H\x1B[3X");
        assert_eq!(text_of(&s, 0), "a   ef", "b·c·d 3칸 지움");
        assert_eq!(s.cursor_col(), 1, "ECH는 커서 불이동");
    }

    #[test]
    fn decstbm_region_scroll_keeps_outside() {
        let mut s = VtScreen::new(5, 4);
        s.feed("111\r\n222\r\n333\r\n444");
        s.feed("\x1B[2;3r"); // 마진 2~3행
        s.feed("\x1B[2;1H\n\n"); // 마진 안에서 LF 2회 → 영역만 스크롤
        assert_eq!(text_of(&s, 0), "111", "마진 밖 위 불변");
        assert_eq!(text_of(&s, 3), "444", "마진 밖 아래 불변");
        assert_eq!(s.scrollback_count(), 0, "부분 마진 = 스크롤백 미보존");
    }

    #[test]
    fn insert_delete_chars_and_lines() {
        let mut s = VtScreen::new(6, 3);
        s.feed("abcdef\x1B[1;2H\x1B[2@"); // ICH 2 — b 앞에 2칸 삽입
        assert_eq!(text_of(&s, 0), "a  bcd");
        s.feed("\x1B[2P"); // DCH 2
        assert_eq!(text_of(&s, 0), "abcd");
        s.feed("\x1B[2;1Hxxx\x1B[1;1H\x1B[1L"); // IL — 1행 앞에 삽입
        assert_eq!(text_of(&s, 0), "");
        assert_eq!(text_of(&s, 1), "abcd");
        s.feed("\x1B[1M"); // DL
        assert_eq!(text_of(&s, 0), "abcd");
    }

    #[test]
    fn wide_char_takes_two_cells() {
        let mut s = VtScreen::new(6, 1);
        s.feed("한a");
        let row = s.line_at(0);
        assert_eq!(row[0].ch, '한');
        assert_eq!(row[1].ch, '\0', "연속 셀");
        assert_eq!(row[2].ch, 'a');
        assert_eq!(s.get_text(0, 0, 0, 5), "한a", "연속 셀은 추출에서 스킵");
    }

    #[test]
    fn save_restore_cursor_and_resize() {
        let mut s = VtScreen::new(10, 4);
        s.feed("\x1B[3;5H\x1B7\x1B[1;1H\x1B8");
        assert_eq!((s.cursor_row(), s.cursor_col()), (2, 4), "DECSC/DECRC");
        s.resize(8, 2);
        assert_eq!((s.cols(), s.rows()), (8, 2));
        assert!(s.cursor_row() < 2 && s.cursor_col() < 8, "커서 클램프");
    }
}
