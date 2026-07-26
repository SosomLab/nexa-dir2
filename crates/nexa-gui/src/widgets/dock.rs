//! 하단 도크 — **정보 뷰**(M4-1, 원본 BottomDockView의 Info 종류·docs/20 하단 도크 대원칙:
//! 듀얼=좌↔좌·우↔우 — 각 패널 하단에 1개). 미리보기(M4-2)·터미널(M4-3)은 종류 스왑으로 확장.
//! 플랫폼 중립 — 텍스트 라인 렌더 + 상단 경계선.

use crate::draw::DrawCtx;
use crate::event::InputEvent;
use crate::geom::{Point, Rect};
use crate::theme::Theme;
use crate::widget::{Invalidations, Widget};

/// 하단 도크 — 호스트가 [`set_lines`](InfoDock::set_lines)로 내용을 공급한다
/// (원본 InfoText/PreviewPath 대응). 종류 스트립(정보|미리보기 — M4-2)은 클릭 전환,
/// 내용 해석은 호스트 몫(위젯은 종류 인덱스만 보유 — 원본 Kind 스왑).
pub struct InfoDock {
    bounds: Rect,
    row_h: i32,
    pad_x: i32,
    /// 종류 라벨들(예: ["정보", "미리보기"]) — 스트립에 나열, 클릭으로 전환.
    kinds: Vec<String>,
    active: usize,
    /// 스트립 라벨 x 범위 캐시(히트 테스트 — 텍스트 측정은 paint에서만).
    ranges: std::cell::RefCell<Vec<(i32, i32)>>,
    lines: Vec<String>,
    /// 이미지 미리보기 경로(Some = 라인 대신 이미지 — M4-2).
    image: Option<String>,
    /// 터미널 "폴더로 이동"(→) 클릭 통지(QA 07-14 — 원본 '터미널에서 열기'). 1회성.
    pending_goto: bool,
    /// → 버튼 x 범위 캐시(paint가 채움 — 마지막 종류[터미널] 옆에 부착).
    goto_range: std::cell::Cell<(i32, i32)>,
    /// 호스트 패널 포커스 — 비활성 패널은 강조색(accent·sel_bg)을 무채색으로 낮춘다.
    focused: bool,
    /// 내용 첫 가시 라인(세로 스크롤 — 미리보기 07-26. 내용 교체 시 0).
    scroll: usize,
    /// 우상단 "크게"(↗) 오버레이 버튼 표시(호스트가 미리보기 종류일 때 켬 — 07-26).
    popout_on: bool,
    /// ↗ 클릭 통지(1회성 — 호스트가 독립 미리보기 창을 연다).
    pending_popout: bool,
    /// ↗ 버튼 히트 존 캐시(paint가 채움).
    popout_range: std::cell::Cell<Rect>,
    /// ↗ hover(색 입힘 — X-27 sel_bg 토큰)·pressed(누름 효과 — accent 블렌드 +
    /// 아이콘 1px 오프셋, MouseUp이 버튼 안이면 발화 — 07-26 이미지 버튼 개편).
    popout_hover: bool,
    popout_pressed: bool,
    /// 내용 텍스트 선택(드래그 — QA 07-15 라인 → **문자 단위**로 보완 07-20:
    /// Info/Preview 영역 선택 복사). (앵커, 현재) = (라인, 문자 경계).
    sel: Option<((usize, usize), (usize, usize))>,
    /// 선택 드래그 중(MouseDown 시작 → MouseUp 종료, 선택은 유지).
    sel_drag: bool,
    /// paint 캐시: 그려진 라인별 문자 경계 x 오프셋(원점 = bounds.x+pad_x 접두 폭 —
    /// edit.rs 캐시 규약). 내용/지표/크기 변경 시 비움(paint가 재계산). 폭 초과
    /// 문자는 측정 중단(클릭 가능 영역 상한 = 보이는 폭).
    offsets: std::cell::RefCell<Vec<Vec<i32>>>,
}

/// 클릭 x(원점 상대) → 최근접 문자 경계 인덱스(edit.rs index_at 규약).
fn nearest_boundary(offs: &[i32], rel: i32) -> usize {
    let mut best = 0usize;
    let mut bd = i32::MAX;
    for (i, o) in offs.iter().enumerate() {
        let d = (o - rel).abs();
        if d < bd {
            bd = d;
            best = i;
        }
    }
    best
}

impl InfoDock {
    pub fn new(title: impl Into<String>, row_h: i32, pad_x: i32) -> Self {
        InfoDock {
            bounds: Rect::default(),
            row_h: row_h.max(1),
            pad_x,
            kinds: vec![title.into()],
            active: 0,
            ranges: std::cell::RefCell::new(Vec::new()),
            lines: Vec::new(),
            image: None,
            pending_goto: false,
            goto_range: std::cell::Cell::new((0, 0)),
            focused: false,
            scroll: 0,
            popout_on: false,
            pending_popout: false,
            popout_range: std::cell::Cell::new(Rect::default()),
            popout_hover: false,
            popout_pressed: false,
            sel: None,
            sel_drag: false,
            offsets: std::cell::RefCell::new(Vec::new()),
        }
    }

    /// 가시 내용 행 수(스트립 제외 — 스크롤 상한 산정).
    fn visible_rows(&self) -> usize {
        let strip_h = 1 + self.row_h.min((self.bounds.h - 1).max(0));
        ((self.bounds.h - strip_h).max(0) / self.row_h.max(1)) as usize
    }

    /// 스크롤 상한(마지막 페이지가 화면을 채우는 지점).
    fn max_scroll(&self) -> usize {
        self.lines.len().saturating_sub(self.visible_rows().max(1))
    }

    /// 세로 스크롤 이동(휠·드래그 자동 스크롤 — 07-26). 변경 시 오프셋 캐시 무효.
    fn scroll_to(&mut self, to: usize, inv: &mut Invalidations) -> bool {
        let to = to.min(self.max_scroll());
        if to != self.scroll {
            self.scroll = to;
            self.offsets.borrow_mut().clear();
            inv.push(self.bounds);
            return true;
        }
        false
    }

    /// 내용 라인 y→**절대** 인덱스(스크롤 반영. 이미지·터미널 종류는 None).
    fn line_at(&self, x: i32, y: i32) -> Option<usize> {
        if self.image.is_some() || self.lines.is_empty() {
            return None;
        }
        let top = self.bounds.y + 1 + self.row_h.min((self.bounds.h - 1).max(0));
        if !self.bounds.contains(Point { x, y }) || y < top {
            return None;
        }
        let i = self.scroll + ((y - top) / self.row_h) as usize;
        (i < self.lines.len()).then_some(i)
    }

    /// 선택 텍스트(문자 단위 영역 — Ctrl+C 복사용, QA 07-15 → 07-20 보완).
    /// 선택 없음·빈 범위 = `None`.
    pub fn selected_text(&self) -> Option<String> {
        let (a, c) = self.sel?;
        let (lo, hi) = if a <= c { (a, c) } else { (c, a) };
        if lo == hi || lo.0 >= self.lines.len() {
            return None;
        }
        let chars_of = |l: usize| self.lines[l].chars().collect::<Vec<char>>();
        let (ll, lc) = lo;
        let (hl, hc) = (hi.0.min(self.lines.len() - 1), hi.1);
        if ll == hl {
            let cs = chars_of(ll);
            let (a, b) = (lc.min(cs.len()), hc.min(cs.len()));
            return (b > a).then(|| cs[a..b].iter().collect());
        }
        let mut parts = Vec::with_capacity(hl - ll + 1);
        let f = chars_of(ll);
        parts.push(f[lc.min(f.len())..].iter().collect::<String>());
        for l in ll + 1..hl {
            parts.push(self.lines[l].clone());
        }
        let t = chars_of(hl);
        parts.push(t[..hc.min(t.len())].iter().collect::<String>());
        Some(parts.join("\r\n"))
    }

    /// 클릭 좌표 → (절대 라인, 최근접 문자 경계) — paint 오프셋 캐시(가시 행 단위)
    /// 역참조(paint 전 = None).
    fn char_at(&self, x: i32, y: i32) -> Option<(usize, usize)> {
        let i = self.line_at(x, y)?;
        let offs = self.offsets.borrow();
        let line = offs.get(i - self.scroll)?;
        if line.is_empty() {
            return Some((i, 0));
        }
        Some((i, nearest_boundary(line, x - (self.bounds.x + self.pad_x))))
    }

    /// 우상단 "크게"(↗) 오버레이 표시 여부(호스트 — 미리보기 종류일 때만).
    pub fn set_popout(&mut self, on: bool, inv: &mut Invalidations) {
        if self.popout_on != on {
            self.popout_on = on;
            if !on {
                self.popout_range.set(Rect::default());
            }
            inv.push(self.bounds);
        }
    }

    /// ↗ 클릭 수거(1회성) — 호스트가 독립 미리보기 창(모달)을 연다.
    pub fn take_popout(&mut self) -> bool {
        std::mem::take(&mut self.pending_popout)
    }

    /// 우상단 "크게"(↗) **이미지 버튼** 그리기(내용/이미지 위 — paint 마지막 호출,
    /// 07-26 개편). 3상태 배경: pressed = accent 38% 블렌드(툴바 켜짐 규약) >
    /// hover = sel_bg(X-27 토큰) > 기본 header_bg. 누름 효과 = 아이콘 1px 우하 오프셋.
    /// 아이콘 = `emb:popout`(SVG — 잉크 = 테마 본문색·다크 신호), 미로드 폴백 = ↗ 글리프.
    fn draw_popout(&self, ctx: &mut dyn DrawCtx, theme: &Theme, content_top: i32) {
        if !self.popout_on {
            self.popout_range.set(Rect::default());
            return;
        }
        let b = self.bounds;
        let side = (self.row_h + 4).min((b.bottom() - content_top - 4).max(0));
        let cell = Rect::new(b.right() - side - self.pad_x, content_top + 2, side, side);
        if cell.w <= 0 || cell.h <= 0 {
            self.popout_range.set(Rect::default());
            return;
        }
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * 0.38) as u8;
        let bg = if self.popout_pressed {
            crate::theme::Color {
                r: mix(theme.panel_bg.r, theme.accent.r),
                g: mix(theme.panel_bg.g, theme.accent.g),
                b: mix(theme.panel_bg.b, theme.accent.b),
            }
        } else if self.popout_hover {
            theme.sel_bg
        } else {
            theme.header_bg
        };
        ctx.fill_rect(cell, bg);
        let off = i32::from(self.popout_pressed); // 누름 효과 1px 우하
        let isz = (side - 8).max(8);
        let ink =
            ((theme.text.r as u32) << 16) | ((theme.text.g as u32) << 8) | theme.text.b as u32;
        let dk = if theme.is_dark { "#dark" } else { "" };
        let key = format!("emb:popout{dk}#{ink:06X}");
        let drew = ctx.draw_icon(
            cell.x + (side - isz) / 2 + off,
            cell.y + (side - isz) / 2 + off,
            isz,
            &key,
            "",
        );
        if !drew {
            // 미로드/추출 실패 폴백 = ↗ 글리프(bg는 이미 채움)
            let label = "↗";
            let tw = ctx.text_width(label);
            let ty = cell.y + (cell.h - (cell.h * 4) / 5) / 2 + off;
            let fg = if self.focused { theme.text } else { theme.text_dim };
            ctx.text(cell.x + (cell.w - tw).max(0) / 2 + off, ty, cell, label, fg);
        }
        self.popout_range.set(cell);
    }

    /// 선택 해제(다른 영역 클릭 시 호스트가 호출).
    pub fn clear_text_selection(&mut self, inv: &mut Invalidations) {
        self.sel_drag = false;
        if self.sel.take().is_some() {
            inv.push(self.bounds);
        }
    }

    /// 도크 키 포커스 상태 반영(호스트=터미널 포커스와 동기 — QA 07-15) —
    /// 활성 종류·→ 버튼 강조색만 바뀐다.
    pub fn set_focused(&mut self, focused: bool, inv: &mut Invalidations) {
        if self.focused != focused {
            self.focused = focused;
            inv.push(self.bounds);
        }
    }

    /// 터미널 "폴더로 이동"(→) 클릭 수거(1회성 — 호스트가 cd 전송·종류 전환).
    pub fn take_goto(&mut self) -> bool {
        std::mem::take(&mut self.pending_goto)
    }

    /// 종류 라벨 목록 교체(i18n 전환 포함) — 활성 인덱스는 범위로 클램프.
    pub fn set_kinds(&mut self, kinds: Vec<String>, inv: &mut Invalidations) {
        if self.kinds != kinds {
            self.kinds = kinds;
            self.active = self.active.min(self.kinds.len().saturating_sub(1));
            inv.push(self.bounds);
        }
    }

    /// 활성 종류 인덱스(호스트가 내용 공급 분기 — 0=정보·1=미리보기·2=터미널).
    pub fn active_kind(&self) -> usize {
        self.active
    }

    /// 종류 스트립 아래 내용 영역(터미널 등 호스트 직접 렌더용 — M4-3).
    pub fn content_rect(&self) -> Rect {
        let strip_h = 1 + self.row_h.min((self.bounds.h - 1).max(0));
        Rect::new(
            self.bounds.x,
            self.bounds.y + strip_h,
            self.bounds.w,
            (self.bounds.h - strip_h).max(0),
        )
    }

    pub fn set_metrics(&mut self, row_h: i32, pad_x: i32, inv: &mut Invalidations) {
        self.row_h = row_h.max(1);
        self.pad_x = pad_x;
        self.offsets.borrow_mut().clear(); // 폰트/지표 변경 = 문자 폭 캐시 무효(07-20)
        inv.push(self.bounds);
    }

    /// 표시 내용 교체(변경 시에만 무효화 — 선택 이동마다 호출돼도 무비용 유지).
    /// 내용이 바뀌면 텍스트 선택·스크롤도 초기화(범위 무효 — QA 07-15).
    pub fn set_lines(&mut self, lines: Vec<String>, inv: &mut Invalidations) {
        if self.lines != lines {
            self.lines = lines;
            self.sel = None;
            self.sel_drag = false;
            self.scroll = 0;
            self.offsets.borrow_mut().clear();
            inv.push(self.bounds);
        }
    }

    /// 이미지 미리보기 대상(M4-2 — Some이면 라인 대신 이미지 표시. 렌더는 draw_image 백엔드).
    pub fn set_image(&mut self, image: Option<String>, inv: &mut Invalidations) {
        if self.image != image {
            self.image = image;
            inv.push(self.bounds);
        }
    }
}

impl Widget for InfoDock {
    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect, inv: &mut Invalidations) {
        if self.bounds != bounds {
            let old = self.bounds;
            self.bounds = bounds;
            self.scroll = self.scroll.min(self.max_scroll()); // 높이 변경 = 상한 재클램프
            self.offsets.borrow_mut().clear(); // 폭 변경 = 측정 상한 무효(07-20)
            inv.push(old.union(&bounds));
        }
    }

    fn on_event(&mut self, ev: &InputEvent, inv: &mut Invalidations) {
        match *ev {
            // 종류 스트립 클릭 = 전환(M4-2 — 원본 SyncToggles 대응). 내용 갱신은 호스트.
            InputEvent::MouseDown { x, y, .. } => {
                let strip_bottom = self.bounds.y + 1 + self.row_h;
                if y >= self.bounds.y && y < strip_bottom {
                    // → 버튼(터미널 옆 부착 — QA 07-14 '폴더로 이동')
                    let (glo, ghi) = self.goto_range.get();
                    if ghi > glo && x >= glo && x < ghi {
                        self.pending_goto = true;
                        if self.active + 1 != self.kinds.len() {
                            self.active = self.kinds.len().saturating_sub(1); // 터미널로 전환
                        }
                        inv.push(self.bounds);
                        return;
                    }
                    let hit = self
                        .ranges
                        .borrow()
                        .iter()
                        .position(|&(lo, hi)| x >= lo && x < hi);
                    if let Some(i) = hit {
                        if self.active != i {
                            self.active = i;
                            self.sel = None; // 종류 전환 = 내용 교체
                            self.sel_drag = false;
                            inv.push(self.bounds);
                        }
                    }
                } else if self.popout_on && self.popout_range.get().contains(Point { x, y }) {
                    // 우상단 ↗ "크게" 이미지 버튼(07-26) — 프레스 시각만, 발화는 MouseUp
                    self.popout_pressed = true;
                    inv.push(self.popout_range.get());
                } else if let Some(pos) = self.char_at(x, y) {
                    // 내용 드래그 선택 시작(QA 07-15 → 07-20 **문자 단위** 앵커)
                    self.sel = Some((pos, pos));
                    self.sel_drag = true;
                    inv.push(self.bounds);
                } else {
                    self.clear_text_selection(inv);
                }
            }
            InputEvent::MouseMove { x, y } => {
                // ↗ hover 색 입힘(07-26 — X-27 sel_bg 토큰. 변경 시에만 무효화)
                let hp = self.popout_on
                    && !self.sel_drag
                    && self.popout_range.get().contains(Point { x, y });
                if hp != self.popout_hover {
                    self.popout_hover = hp;
                    inv.push(self.popout_range.get());
                }
                if self.sel_drag {
                    let top = self.bounds.y + 1 + self.row_h.min((self.bounds.h - 1).max(0));
                    // 드래그 자동 스크롤(07-26): 내용 영역 위/아래 = 1행씩 이동
                    if y < top {
                        let s = self.scroll.saturating_sub(1);
                        let _ = self.scroll_to(s, inv);
                    } else if y >= self.bounds.bottom() {
                        let _ = self.scroll_to(self.scroll + 1, inv);
                    }
                    // 내용 영역 밖은 첫/끝 라인·문자 경계로 클램프(엣지 드래그 — 07-20)
                    let offs = self.offsets.borrow();
                    let last_line = self.lines.len().saturating_sub(1);
                    let (li, ci) = if offs.is_empty() {
                        // 스크롤 직후 = 캐시 무효 — 라인만 갱신(경계는 다음 paint 후 정밀화)
                        let li = if y < top {
                            self.scroll
                        } else {
                            (self.scroll + (((y - top) / self.row_h).max(0) as usize))
                                .min(last_line)
                        };
                        (li, 0)
                    } else {
                        let row = if y < top {
                            0
                        } else {
                            (((y - top) / self.row_h).max(0) as usize)
                                .min(offs.len().saturating_sub(1))
                        };
                        let li = (self.scroll + row).min(last_line);
                        let ci = if offs[row].is_empty() {
                            0
                        } else {
                            nearest_boundary(&offs[row], x - (self.bounds.x + self.pad_x))
                        };
                        (li, ci)
                    };
                    drop(offs);
                    if let Some((a, cur)) = self.sel {
                        if cur != (li, ci) {
                            self.sel = Some((a, (li, ci)));
                            inv.push(self.bounds);
                        }
                    }
                }
            }
            InputEvent::Wheel { delta } => {
                // 미리보기 내용 스크롤(07-26 — 3줄/노치·터미널 규약 동일). 호스트가
                // 내용 영역 hover일 때만 라우팅한다.
                if self.image.is_none() && !self.lines.is_empty() {
                    let step = delta / crate::WHEEL_DELTA * 3;
                    let to = (self.scroll as i32 - step).max(0) as usize;
                    let _ = self.scroll_to(to, inv);
                }
            }
            InputEvent::MouseUp { x, y } => {
                // ↗ 버튼 의미론(07-26): 누른 채 버튼 **안에서 뗄 때만** 발화
                if self.popout_pressed {
                    self.popout_pressed = false;
                    if self.popout_range.get().contains(Point { x, y }) {
                        self.pending_popout = true;
                    }
                    inv.push(self.popout_range.get());
                }
                self.sel_drag = false; // 선택은 유지(Ctrl+C 복사 — 터미널 규약 동일)
                if let Some((a, c)) = self.sel {
                    if a == c {
                        self.sel = None; // 이동 없는 단순 클릭 = 선택 없음(edit.rs 규약)
                    }
                }
            }
            _ => {}
        }
    }

    fn paint(&self, ctx: &mut dyn DrawCtx, theme: &Theme) {
        ctx.select_font(crate::FontSlot::Base, false, false); // 폰트 슬롯(X-12)
        let b = self.bounds;
        if b.h <= 0 {
            return;
        }
        ctx.fill_rect(b, theme.panel_bg);
        // 상단 경계선(리스트와 구분 — docs/39 §2 경계선+명도 차)
        ctx.fill_rect(Rect::new(b.x, b.y, b.w, 1), theme.border);
        // 종류 스트립(정보|미리보기 — 활성 강조·클릭 전환. 범위 캐시)
        let strip = Rect::new(b.x, b.y + 1, b.w, self.row_h.min(b.h - 1));
        let ty = |cell: Rect| cell.y + (cell.h - (cell.h * 4) / 5) / 2;
        ctx.fill_rect(strip, theme.header_bg);
        let mut ranges = Vec::with_capacity(self.kinds.len());
        let mut x = strip.x + self.pad_x;
        let last = self.kinds.len().saturating_sub(1);
        self.goto_range.set((0, 0));
        for (i, label) in self.kinds.iter().enumerate() {
            let w = ctx.text_width(label) + self.pad_x * 2;
            let cell = Rect::new(x, strip.y, w.min((strip.right() - x).max(0)), strip.h);
            let active = i == self.active;
            let (fg, bg) = if active && self.focused {
                (theme.text, theme.sel_bg)
            } else if active {
                // 비활성 패널 — 활성 종류는 무채색으로만 표시(활성 패널과 구분)
                (theme.text, theme.sel_bg_inactive)
            } else {
                (theme.text_dim, theme.header_bg)
            };
            if cell.w > 0 {
                ctx.text_opaque(cell.x + self.pad_x, ty(cell), cell, label, fg, bg);
            }
            ranges.push((cell.x, cell.x + w));
            x += w;
            if i == last && self.kinds.len() > 1 {
                // 터미널 옆 "폴더로 이동"(→) — 한 몸 버튼(QA 07-14, 원본 '터미널에서 열기').
                // 활성=accent 배경(단, 패널 비활성이면 무채색 — 활성 영역과 구분), 비활성=무색
                let gw = ctx.text_width("→") + self.pad_x * 2;
                let gcell = Rect::new(x, strip.y, gw.min((strip.right() - x).max(0)), strip.h);
                let (gfg, gbg) = if active && self.focused {
                    (theme.text, theme.accent)
                } else if active {
                    (theme.text, theme.sel_bg_inactive)
                } else {
                    (theme.text_dim, theme.header_bg)
                };
                if gcell.w > 0 {
                    ctx.text_opaque(gcell.x + self.pad_x, ty(gcell), gcell, "→", gfg, gbg);
                }
                self.goto_range.set((gcell.x, gcell.x + gw));
                x += gw;
            }
            x += self.pad_x;
        }
        *self.ranges.borrow_mut() = ranges;
        // 이미지 미리보기(M4-2) — 내용 영역 전체에 비율 유지 가운데 표시
        if let Some(img) = &self.image {
            let area = Rect::new(
                b.x + self.pad_x,
                strip.bottom() + 2,
                b.w - self.pad_x * 2,
                (b.bottom() - strip.bottom() - 4).max(0),
            );
            ctx.draw_image(area, img);
            self.draw_popout(ctx, theme, strip.bottom());
            return;
        }
        // 내용 라인들(스크롤 반영 — 07-26) — 드래그 선택 **문자 구간** 하이라이트
        // (QA 07-15 → 07-20 보완, Ctrl+C 복사 대상). 문자 경계 오프셋은 **가시 행**
        // 단위로 캐시(히트 테스트 역참조 — edit.rs paint_field 규약. 무효화 시 재측정)
        let sel = self.sel.map(|(a, c)| if a <= c { (a, c) } else { (c, a) });
        let rebuild = self.offsets.borrow().is_empty() && !self.lines.is_empty();
        let x0 = b.x + self.pad_x;
        let max_w = (b.w - self.pad_x).max(0);
        let mut y = strip.bottom();
        for (i, line) in self.lines.iter().enumerate().skip(self.scroll) {
            if y >= b.bottom() {
                break;
            }
            if rebuild {
                let mut offs = vec![0];
                let mut prefix = String::new();
                for c in line.chars() {
                    prefix.push(c);
                    let w = ctx.text_width(&prefix);
                    offs.push(w);
                    if w > max_w {
                        break; // 보이는 폭 밖 = 클릭 불가(측정 상한)
                    }
                }
                self.offsets.borrow_mut().push(offs);
            }
            let cell = Rect::new(b.x, y, b.w, self.row_h.min(b.bottom() - y));
            if let Some(((ll, lc), (hl, hc))) = sel.filter(|&((ll, _), (hl, _))| ll <= i && i <= hl)
            {
                let offs = self.offsets.borrow();
                if let Some(o) = offs.get(i - self.scroll) {
                    let last = o.len().saturating_sub(1);
                    let cs = if i == ll { lc.min(last) } else { 0 };
                    let ce = if i == hl { hc.min(last) } else { last };
                    if ce > cs {
                        ctx.fill_rect(
                            Rect::new(x0 + o[cs], cell.y, o[ce] - o[cs], cell.h),
                            theme.sel_bg,
                        );
                    }
                }
            }
            ctx.text(cell.x + self.pad_x, ty(cell), cell, line, theme.text);
            y += self.row_h;
        }
        // 잔여 배경
        if y < b.bottom() {
            ctx.fill_rect(Rect::new(b.x, y, b.w, b.bottom() - y), theme.panel_bg);
        }
        self.draw_popout(ctx, theme, strip.bottom());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Color;

    struct Probe;
    impl DrawCtx for Probe {
        fn fill_rect(&mut self, _r: Rect, _c: Color) {}
        fn text_opaque(&mut self, _x: i32, _y: i32, _c: Rect, _t: &str, _f: Color, _b: Color) {}
        fn text_width(&mut self, text: &str) -> i32 {
            text.chars().count() as i32 * 8
        }
    }

    #[test]
    fn drag_selects_char_region_and_click_alone_clears() {
        // 문자 단위 영역 선택(07-20 — 미리보기 텍스트 복사 보완). Probe = 8px/문자.
        let mut inv = Invalidations::default();
        let mut d = InfoDock::new("정보", 20, 6);
        d.set_bounds(Rect::new(0, 100, 400, 120), &mut inv);
        d.set_lines(vec!["abcdef".into(), "01234".into()], &mut inv);
        d.paint(&mut Probe, &Theme::dark());
        // 내용 top = 100+1+20 = 121. 라인0 문자경계2(x=6+16) 프레스 →
        // 라인1 문자경계3(x=6+24)까지 드래그
        d.on_event(
            &InputEvent::MouseDown {
                x: 6 + 16,
                y: 121,
                shift: false,
                ctrl: false,
            },
            &mut inv,
        );
        d.on_event(&InputEvent::MouseMove { x: 6 + 24, y: 141 }, &mut inv);
        d.on_event(&InputEvent::MouseUp { x: 6 + 24, y: 141 }, &mut inv);
        assert_eq!(
            d.selected_text().as_deref(),
            Some("cdef\r\n012"),
            "문자 단위 다중 라인 영역"
        );
        // 같은 라인 안 부분 선택
        d.on_event(
            &InputEvent::MouseDown {
                x: 6 + 8,
                y: 121,
                shift: false,
                ctrl: false,
            },
            &mut inv,
        );
        d.on_event(&InputEvent::MouseMove { x: 6 + 32, y: 121 }, &mut inv);
        d.on_event(&InputEvent::MouseUp { x: 6 + 32, y: 121 }, &mut inv);
        assert_eq!(d.selected_text().as_deref(), Some("bcd"));
        // 이동 없는 단순 클릭 = 선택 없음(edit.rs 규약)
        d.on_event(
            &InputEvent::MouseDown {
                x: 6 + 16,
                y: 121,
                shift: false,
                ctrl: false,
            },
            &mut inv,
        );
        d.on_event(&InputEvent::MouseUp { x: 6 + 16, y: 121 }, &mut inv);
        assert_eq!(d.selected_text(), None);
    }

    #[test]
    fn wheel_scrolls_and_selection_maps_absolute_lines() {
        // 07-26 미리보기 스크롤: h=120 → 스트립 21 + 내용 4행(row_h 20)·12줄 = 상한 8.
        let mut inv = Invalidations::default();
        let mut d = InfoDock::new("정보", 20, 6);
        d.set_bounds(Rect::new(0, 100, 400, 120), &mut inv);
        d.set_lines((0..12).map(|i| format!("l{i}")).collect(), &mut inv);
        d.paint(&mut Probe, &Theme::dark());
        d.on_event(&InputEvent::Wheel { delta: -120 }, &mut inv); // 1노치 아래 = 3줄
        d.paint(&mut Probe, &Theme::dark()); // 오프셋 재측정(가시 행 기준)
        // 첫 가시 행 클릭 = 절대 라인 3 — "l3"의 문자 1~2 선택 = "3"
        d.on_event(
            &InputEvent::MouseDown {
                x: 6 + 8,
                y: 121,
                shift: false,
                ctrl: false,
            },
            &mut inv,
        );
        d.on_event(&InputEvent::MouseMove { x: 6 + 16, y: 121 }, &mut inv);
        d.on_event(&InputEvent::MouseUp { x: 6 + 16, y: 121 }, &mut inv);
        assert_eq!(d.selected_text().as_deref(), Some("3"), "스크롤 반영 절대 라인");
        // 상한 클램프(대량 휠)
        d.on_event(&InputEvent::Wheel { delta: -120 * 50 }, &mut inv);
        assert_eq!(d.scroll, 8, "max_scroll 클램프");
        d.on_event(&InputEvent::Wheel { delta: 120 * 50 }, &mut inv);
        assert_eq!(d.scroll, 0, "0 클램프");
    }

    #[test]
    fn popout_button_hover_press_release_semantics() {
        // 07-26 이미지 버튼 개편: hover 색·press 시각·**안에서 릴리스할 때만** 발화.
        let mut inv = Invalidations::default();
        let mut d = InfoDock::new("정보", 20, 6);
        d.set_bounds(Rect::new(0, 100, 400, 120), &mut inv);
        d.set_lines(vec!["abc".into()], &mut inv);
        d.set_popout(true, &mut inv);
        d.paint(&mut Probe, &Theme::dark());
        // 버튼 정사각 side = row_h+4 = 24 → cell x=400-24-6=370, y=123
        let _ = inv.drain().count();
        d.on_event(&InputEvent::MouseMove { x: 380, y: 130 }, &mut inv);
        assert!(d.popout_hover, "hover 색 입힘");
        assert_eq!(inv.drain().count(), 1, "hover 진입 = 무효화 1회");
        d.on_event(&InputEvent::MouseMove { x: 380, y: 130 }, &mut inv);
        assert_eq!(inv.drain().count(), 0, "동일 hover = 무비용");
        // 프레스 = 시각만(발화 없음)
        d.on_event(
            &InputEvent::MouseDown {
                x: 380,
                y: 130,
                shift: false,
                ctrl: false,
            },
            &mut inv,
        );
        assert!(d.popout_pressed && !d.take_popout(), "press = 발화 전");
        // 밖에서 릴리스 = 취소
        d.on_event(&InputEvent::MouseUp { x: 10, y: 130 }, &mut inv);
        assert!(!d.popout_pressed && !d.take_popout(), "밖 릴리스 = 취소");
        // 안에서 릴리스 = 1회성 발화
        d.on_event(
            &InputEvent::MouseDown {
                x: 380,
                y: 130,
                shift: false,
                ctrl: false,
            },
            &mut inv,
        );
        d.on_event(&InputEvent::MouseUp { x: 380, y: 130 }, &mut inv);
        assert!(d.take_popout(), "안 릴리스 = 발화");
        assert!(!d.take_popout(), "수거 후 소진");
        assert_eq!(d.selected_text(), None, "버튼 조작은 선택 시작 아님");
        // 꺼진 상태에서는 히트 없음
        d.set_popout(false, &mut inv);
        d.paint(&mut Probe, &Theme::dark());
        d.on_event(
            &InputEvent::MouseDown {
                x: 380,
                y: 130,
                shift: false,
                ctrl: false,
            },
            &mut inv,
        );
        d.on_event(&InputEvent::MouseUp { x: 380, y: 130 }, &mut inv);
        assert!(!d.take_popout());
    }

    #[test]
    fn drag_below_content_autoscrolls() {
        let mut inv = Invalidations::default();
        let mut d = InfoDock::new("정보", 20, 6);
        d.set_bounds(Rect::new(0, 100, 400, 120), &mut inv);
        d.set_lines((0..12).map(|i| format!("l{i}")).collect(), &mut inv);
        d.paint(&mut Probe, &Theme::dark());
        d.on_event(
            &InputEvent::MouseDown {
                x: 6,
                y: 121,
                shift: false,
                ctrl: false,
            },
            &mut inv,
        );
        // 하단 밖 드래그 = 1행씩 자동 스크롤(이동할 때마다 — 07-26)
        d.on_event(&InputEvent::MouseMove { x: 6, y: 400 }, &mut inv);
        assert_eq!(d.scroll, 1);
        d.paint(&mut Probe, &Theme::dark());
        d.on_event(&InputEvent::MouseMove { x: 6, y: 400 }, &mut inv);
        assert_eq!(d.scroll, 2);
        // 선택 끝은 문서 끝 방향으로 확장 중(앵커 = 라인 0)
        d.on_event(&InputEvent::MouseUp { x: 6, y: 400 }, &mut inv);
        assert!(d.selected_text().is_some());
    }

    #[test]
    fn set_lines_invalidates_only_on_change() {
        let mut inv = Invalidations::default();
        let mut d = InfoDock::new("정보", 20, 6);
        d.set_bounds(Rect::new(0, 100, 400, 120), &mut inv);
        let _ = inv.drain().count();
        d.set_lines(vec!["a.txt".into(), "크기: 10 B".into()], &mut inv);
        assert_eq!(inv.drain().count(), 1, "내용 변경 = 무효화");
        d.set_lines(vec!["a.txt".into(), "크기: 10 B".into()], &mut inv);
        assert_eq!(inv.drain().count(), 0, "동일 내용 = 무비용");
        d.paint(&mut Probe, &Theme::dark()); // 렌더 스모크
    }
}
