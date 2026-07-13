//! nexa-term — 터미널 **VT/ANSI 파서 + 화면 버퍼**(M4-3). **원본 이식**:
//! `app/Nexa.App/Terminal/VtScreen.cs`(BP-T2 — docs/37).
//!
//! ConPTY가 내보내는 VT 시퀀스를 해석해 **셀 그리드**(문자·색·속성)로 유지한다.
//! 지원: 출력 문자·CR/LF/BS/HT, SGR(16/256/트루컬러·굵게·반전·faint), 커서 이동
//! (CUP/CUU·D·F·B/CHA/VPA/CNL/CPL), 지우기(ED/EL/ECH), 삽입/삭제(ICH/DCH/IL/DL),
//! 스크롤(SU/SD·DECSTBM 마진·스크롤백 보존), DECSC/DECRC. 렌더·ConPTY 배선은 앱(win.rs).
//! 플랫폼 중립 — 전 플랫폼 테스트.

/// 터미널 셀 하나 — 문자 + 전경/배경색(ARGB) + 굵게/반전/흐리게(faint).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TermCell {
    pub ch: char,
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

pub const DEFAULT_FG: u32 = 0xFFE6_E6E6;
pub const DEFAULT_BG: u32 = 0xFF0C_0F12;
const MAX_SCROLLBACK: usize = 800;

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
            return; // private 마커 — 미사용(?…h/l은 최종 바이트에서 무시)
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

// Campbell(Windows Terminal 기본) 16색 팔레트
const PALETTE16: [u32; 16] = [
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
];

fn ansi16(i: usize) -> u32 {
    PALETTE16[i.min(15)]
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
        assert_eq!(row[0].fg, 0xFFC5_0F1F, "ANSI 빨강");
        assert_eq!(row[1].fg, 0xFFFF_0000, "256색 196 = 순빨강");
        assert_eq!(row[2].fg, 0xFF01_0203, "트루컬러");
        assert_eq!(row[3].fg, DEFAULT_FG, "리셋");
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
