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
    /// 내용 라인 선택(드래그 — QA 07-15: Info/Preview 텍스트 복사). (앵커, 현재) 라인.
    sel: Option<(usize, usize)>,
    /// 선택 드래그 중(MouseDown 시작 → MouseUp 종료, 선택은 유지).
    sel_drag: bool,
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
            sel: None,
            sel_drag: false,
        }
    }

    /// 내용 라인 y→인덱스(이미지 미리보기·터미널 종류는 라인 없음 = None).
    fn line_at(&self, x: i32, y: i32) -> Option<usize> {
        if self.image.is_some() || self.lines.is_empty() {
            return None;
        }
        let top = self.bounds.y + 1 + self.row_h.min((self.bounds.h - 1).max(0));
        if !self.bounds.contains(Point { x, y }) || y < top {
            return None;
        }
        let i = ((y - top) / self.row_h) as usize;
        (i < self.lines.len()).then_some(i)
    }

    /// 선택 텍스트(라인 범위 — Ctrl+C 복사용, QA 07-15). 선택 없으면 `None`.
    pub fn selected_text(&self) -> Option<String> {
        let (a, b) = self.sel?;
        let (lo, hi) = (a.min(b), a.max(b).min(self.lines.len().saturating_sub(1)));
        Some(self.lines[lo..=hi].join("\r\n"))
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
        inv.push(self.bounds);
    }

    /// 표시 내용 교체(변경 시에만 무효화 — 선택 이동마다 호출돼도 무비용 유지).
    /// 내용이 바뀌면 텍스트 선택도 해제(범위 무효 — QA 07-15).
    pub fn set_lines(&mut self, lines: Vec<String>, inv: &mut Invalidations) {
        if self.lines != lines {
            self.lines = lines;
            self.sel = None;
            self.sel_drag = false;
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
                } else if let Some(i) = self.line_at(x, y) {
                    // 내용 라인 드래그 선택 시작(QA 07-15 — Info/Preview 텍스트 복사)
                    self.sel = Some((i, i));
                    self.sel_drag = true;
                    inv.push(self.bounds);
                } else {
                    self.clear_text_selection(inv);
                }
            }
            InputEvent::MouseMove { x, y } => {
                if self.sel_drag {
                    // 내용 영역 밖은 첫/끝 라인으로 클램프(엣지 드래그)
                    let top = self.bounds.y + 1 + self.row_h.min((self.bounds.h - 1).max(0));
                    let i = if y < top {
                        0
                    } else {
                        (((y - top) / self.row_h).max(0) as usize)
                            .min(self.lines.len().saturating_sub(1))
                    };
                    let _ = x;
                    if let Some((a, cur)) = self.sel {
                        if cur != i {
                            self.sel = Some((a, i));
                            inv.push(self.bounds);
                        }
                    }
                }
            }
            InputEvent::MouseUp { .. } => {
                self.sel_drag = false; // 선택은 유지(Ctrl+C 복사 — 터미널 규약 동일)
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
            return;
        }
        // 내용 라인들 — 드래그 선택 라인은 하이라이트(QA 07-15, Ctrl+C 복사 대상)
        let sel = self.sel.map(|(a, b2)| (a.min(b2), a.max(b2)));
        let mut y = strip.bottom();
        for (i, line) in self.lines.iter().enumerate() {
            if y >= b.bottom() {
                break;
            }
            let bg = if sel.is_some_and(|(lo, hi)| i >= lo && i <= hi) {
                theme.sel_bg
            } else {
                theme.panel_bg
            };
            let cell = Rect::new(b.x, y, b.w, self.row_h.min(b.bottom() - y));
            if bg != theme.panel_bg {
                ctx.fill_rect(cell, bg);
            }
            ctx.text_opaque(cell.x + self.pad_x, ty(cell), cell, line, theme.text, bg);
            y += self.row_h;
        }
        // 잔여 배경
        if y < b.bottom() {
            ctx.fill_rect(Rect::new(b.x, y, b.w, b.bottom() - y), theme.panel_bg);
        }
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
