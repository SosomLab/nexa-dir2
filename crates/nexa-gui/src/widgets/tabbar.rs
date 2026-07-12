//! 패널 탭 바 — 원본 PanelTab(docs/20 §2) 대응 위젯. 탭 표시·전환·닫기·새 탭.
//! 도메인 비종속: 제목 목록만 알고, 동작은 [`TabAction`]으로 호스트에 통지(take_action).
//! 드래그 재배열·잠금/고정·멀티라인은 후속(원본 탭 UX 세부).

use std::cell::RefCell;

use crate::draw::DrawCtx;
use crate::event::InputEvent;
use crate::geom::{Point, Rect};
use crate::theme::Theme;
use crate::widget::{Invalidations, Widget};

/// 호스트가 수행할 탭 동작.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TabAction {
    Switch(usize),
    Close(usize),
    New,
}

/// 탭 바 위젯(높이 1줄). 활성 탭 = 패널 배경 + 상단 accent 줄(원본 규약).
pub struct TabBar {
    titles: Vec<String>,
    active: usize,
    /// 활성 패널 여부 — 비활성 패널은 accent 줄을 흐리게(테두리 색).
    focused: bool,
    bounds: Rect,
    row_h: i32,
    pad_x: i32,
    hover: Option<usize>,
    pending: Option<TabAction>,
    /// 페인트 시 캐시한 탭 x 범위 + [+] 버튼 범위.
    ranges: RefCell<HitRanges>,
}

/// (탭별 x 범위, [+] 버튼 x 범위) — 페인트가 채우고 히트 테스트가 읽는다.
type HitRanges = (Vec<(i32, i32)>, (i32, i32));

/// 닫기(×) 히트 존 폭 배수(pad_x 기준).
const CLOSE_ZONE_PADS: i32 = 3;

impl TabBar {
    pub fn new(row_h: i32, pad_x: i32) -> Self {
        TabBar {
            titles: Vec::new(),
            active: 0,
            focused: true,
            bounds: Rect::default(),
            row_h: row_h.max(1),
            pad_x,
            hover: None,
            pending: None,
            ranges: RefCell::new((Vec::new(), (0, 0))),
        }
    }

    pub fn set_tabs(&mut self, titles: Vec<String>, active: usize, inv: &mut Invalidations) {
        self.titles = titles;
        self.active = active.min(self.titles.len().saturating_sub(1));
        self.hover = None;
        inv.push(self.bounds);
    }

    pub fn set_focused(&mut self, focused: bool, inv: &mut Invalidations) {
        if self.focused != focused {
            self.focused = focused;
            inv.push(self.bounds);
        }
    }

    pub fn set_metrics(&mut self, row_h: i32, pad_x: i32, inv: &mut Invalidations) {
        self.row_h = row_h.max(1);
        self.pad_x = pad_x;
        inv.push(self.bounds);
    }

    /// 호스트가 수거할 탭 동작(1회성).
    pub fn take_action(&mut self) -> Option<TabAction> {
        self.pending.take()
    }

    fn tab_at(&self, x: i32, y: i32) -> Option<(usize, bool)> {
        if !self.bounds.contains(Point { x, y }) {
            return None;
        }
        let (tabs, _) = &*self.ranges.borrow();
        for (i, &(lo, hi)) in tabs.iter().enumerate() {
            if x >= lo && x < hi {
                // 오른쪽 닫기(×) 존
                let close = x >= hi - self.pad_x * CLOSE_ZONE_PADS;
                return Some((i, close));
            }
        }
        None
    }

    fn plus_at(&self, x: i32, y: i32) -> bool {
        if !self.bounds.contains(Point { x, y }) {
            return false;
        }
        let (_, (lo, hi)) = *self.ranges.borrow();
        x >= lo && x < hi
    }
}

impl Widget for TabBar {
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
            InputEvent::MouseDown { x, y, .. } => {
                if let Some((i, close)) = self.tab_at(x, y) {
                    self.pending = Some(if close && self.titles.len() > 1 {
                        TabAction::Close(i) // 마지막 탭은 닫기 불가(패널은 항상 ≥1 탭)
                    } else {
                        TabAction::Switch(i)
                    });
                    inv.push(self.bounds);
                } else if self.plus_at(x, y) {
                    self.pending = Some(TabAction::New);
                    inv.push(self.bounds);
                }
            }
            InputEvent::MouseMove { x, y } => {
                let hover = self.tab_at(x, y).map(|(i, _)| i);
                if hover != self.hover {
                    self.hover = hover;
                    inv.push(self.bounds);
                }
            }
            _ => {}
        }
    }

    fn paint(&self, ctx: &mut dyn DrawCtx, theme: &Theme) {
        let b = self.bounds;
        let ty = b.y + (b.h - (b.h * 4) / 5) / 2;
        ctx.fill_rect(b, theme.tab_bar_bg);

        let mut tabs = Vec::with_capacity(self.titles.len());
        let mut x = b.x;
        let close_w = self.pad_x * CLOSE_ZONE_PADS;
        for (i, title) in self.titles.iter().enumerate() {
            let text_w = ctx.text_width(title);
            let w = (text_w + self.pad_x * 2 + close_w).min((b.right() - x).max(0));
            if w <= 0 {
                tabs.push((x, x));
                continue;
            }
            let cell = Rect::new(x, b.y, w, b.h);
            let active = i == self.active;
            let bg = if active {
                theme.panel_bg
            } else if self.hover == Some(i) {
                theme.header_bg
            } else {
                theme.tab_bar_bg
            };
            ctx.text_opaque(cell.x + self.pad_x, ty, cell, title, theme.text, bg);
            // 닫기 × (탭 오른쪽)
            let close_x = cell.right() - close_w + self.pad_x;
            ctx.text_opaque(
                close_x,
                ty,
                Rect::new(cell.right() - close_w, b.y, close_w, b.h),
                "×",
                theme.text_dim,
                bg,
            );
            if active {
                // 활성 탭 상단 accent 줄(비활성 패널은 흐리게 — 활성 패널 시각화)
                let line = if self.focused {
                    theme.accent
                } else {
                    theme.border
                };
                ctx.fill_rect(Rect::new(cell.x, b.y, cell.w, 2), line);
            }
            tabs.push((cell.x, cell.x + w));
            x += w;
        }
        // [+] 새 탭 버튼
        let plus_w = ctx.text_width("+") + self.pad_x * 2;
        let plus = (x, (x + plus_w).min(b.right()));
        if plus.1 > plus.0 {
            ctx.text_opaque(
                x + self.pad_x,
                ty,
                Rect::new(x, b.y, plus.1 - plus.0, b.h),
                "+",
                theme.text_dim,
                theme.tab_bar_bg,
            );
        }
        // 하단 경계선
        ctx.fill_rect(Rect::new(b.x, b.bottom() - 1, b.w, 1), theme.border);
        *self.ranges.borrow_mut() = (tabs, plus);
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

    fn bar(titles: &[&str], active: usize) -> (TabBar, Invalidations) {
        let mut inv = Invalidations::default();
        let mut t = TabBar::new(20, 6);
        t.set_bounds(Rect::new(0, 0, 600, 22), &mut inv);
        t.set_tabs(
            titles.iter().map(|s| s.to_string()).collect(),
            active,
            &mut inv,
        );
        t.paint(&mut Probe, &Theme::dark());
        (t, inv)
    }

    fn down(t: &mut TabBar, inv: &mut Invalidations, x: i32) {
        t.on_event(
            &InputEvent::MouseDown {
                x,
                y: 5,
                shift: false,
                ctrl: false,
            },
            inv,
        );
    }

    #[test]
    fn click_switches_close_zone_closes_and_plus_creates() {
        let (mut t, mut inv) = bar(&["alpha", "beta"], 0);
        // 탭0: 폭 = 5*8+12+18 = 70 → [0,70). 본문 클릭 = 전환
        down(&mut t, &mut inv, 10);
        assert_eq!(t.take_action(), Some(TabAction::Switch(0)));
        // 탭1 [70,132): 닫기 존 = [132-18, 132)
        down(&mut t, &mut inv, 120);
        assert_eq!(t.take_action(), Some(TabAction::Close(1)));
        // [+] = [132, 132+20)
        down(&mut t, &mut inv, 140);
        assert_eq!(t.take_action(), Some(TabAction::New));
        // 빈 영역 무시
        down(&mut t, &mut inv, 500);
        assert_eq!(t.take_action(), None);
    }

    #[test]
    fn last_tab_close_zone_switches_instead() {
        let (mut t, mut inv) = bar(&["only"], 0);
        // 탭0 [0, 4*8+12+18=62), 닫기 존 클릭
        down(&mut t, &mut inv, 55);
        assert_eq!(
            t.take_action(),
            Some(TabAction::Switch(0)),
            "마지막 탭은 닫기 불가"
        );
    }

    #[test]
    fn hover_tracks_tabs() {
        let (mut t, mut inv) = bar(&["alpha", "beta"], 0);
        t.on_event(&InputEvent::MouseMove { x: 80, y: 5 }, &mut inv);
        assert_eq!(t.hover, Some(1));
        t.on_event(&InputEvent::MouseMove { x: 80, y: 50 }, &mut inv);
        assert_eq!(t.hover, None);
    }
}
