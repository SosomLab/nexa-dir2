//! 하단 도크 — **정보 뷰**(M4-1, 원본 BottomDockView의 Info 종류·docs/20 하단 도크 대원칙:
//! 듀얼=좌↔좌·우↔우 — 각 패널 하단에 1개). 미리보기(M4-2)·터미널(M4-3)은 종류 스왑으로 확장.
//! 플랫폼 중립 — 텍스트 라인 렌더 + 상단 경계선.

use crate::draw::DrawCtx;
use crate::event::InputEvent;
use crate::geom::Rect;
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
        }
    }

    /// 종류 라벨 목록 교체(i18n 전환 포함) — 활성 인덱스는 범위로 클램프.
    pub fn set_kinds(&mut self, kinds: Vec<String>, inv: &mut Invalidations) {
        if self.kinds != kinds {
            self.kinds = kinds;
            self.active = self.active.min(self.kinds.len().saturating_sub(1));
            inv.push(self.bounds);
        }
    }

    /// 활성 종류 인덱스(호스트가 내용 공급 분기 — 0=정보·1=미리보기).
    pub fn active_kind(&self) -> usize {
        self.active
    }

    pub fn set_metrics(&mut self, row_h: i32, pad_x: i32, inv: &mut Invalidations) {
        self.row_h = row_h.max(1);
        self.pad_x = pad_x;
        inv.push(self.bounds);
    }

    /// 표시 내용 교체(변경 시에만 무효화 — 선택 이동마다 호출돼도 무비용 유지).
    pub fn set_lines(&mut self, lines: Vec<String>, inv: &mut Invalidations) {
        if self.lines != lines {
            self.lines = lines;
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
        // 종류 스트립 클릭 = 전환(M4-2 — 원본 SyncToggles 대응). 내용 갱신은 호스트.
        if let InputEvent::MouseDown { x, y, .. } = *ev {
            let strip_bottom = self.bounds.y + 1 + self.row_h;
            if y >= self.bounds.y && y < strip_bottom {
                let hit = self
                    .ranges
                    .borrow()
                    .iter()
                    .position(|&(lo, hi)| x >= lo && x < hi);
                if let Some(i) = hit {
                    if self.active != i {
                        self.active = i;
                        inv.push(self.bounds);
                    }
                }
            }
        }
    }

    fn paint(&self, ctx: &mut dyn DrawCtx, theme: &Theme) {
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
        for (i, label) in self.kinds.iter().enumerate() {
            let w = ctx.text_width(label) + self.pad_x * 2;
            let cell = Rect::new(x, strip.y, w.min((strip.right() - x).max(0)), strip.h);
            let (fg, bg) = if i == self.active {
                (theme.text, theme.sel_bg)
            } else {
                (theme.text_dim, theme.header_bg)
            };
            if cell.w > 0 {
                ctx.text_opaque(cell.x + self.pad_x, ty(cell), cell, label, fg, bg);
            }
            ranges.push((cell.x, cell.x + w));
            x += w + self.pad_x;
        }
        *self.ranges.borrow_mut() = ranges;
        // 내용 라인들
        let mut y = strip.bottom();
        for line in &self.lines {
            if y >= b.bottom() {
                break;
            }
            let cell = Rect::new(b.x, y, b.w, self.row_h.min(b.bottom() - y));
            ctx.text_opaque(
                cell.x + self.pad_x,
                ty(cell),
                cell,
                line,
                theme.text,
                theme.panel_bg,
            );
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
