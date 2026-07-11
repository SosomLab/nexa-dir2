//! 가상화 행 리스트 — M0-7 스파이크의 스크롤·가시 행 로직을 위젯으로 이식(M1-1).
//! 데이터는 [`RowSource`]로 추상화 — M1-3에서 nexa-tree 평면 스트림이 이 자리에 배선된다.

use crate::draw::DrawCtx;
use crate::event::{InputEvent, Key, WheelAccum};
use crate::geom::Rect;
use crate::theme::Theme;
use crate::widget::{Invalidations, Widget};

/// 휠 1노치당 스크롤 행 수(M0-7 계승).
const WHEEL_LINES: i32 = 3;

/// 행 데이터 공급자. 위젯은 가시 행에 대해서만 호출한다.
pub trait RowSource {
    fn len(&self) -> usize;
    fn row_text(&self, row: usize) -> String;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// 세로 가상화 행 리스트 — 프레임 비용은 bounds 높이에만 비례(docs/01 §3).
pub struct VirtualRows<S> {
    src: S,
    bounds: Rect,
    scroll_row: usize,
    row_h: i32,
    pad_x: i32,
    wheel: WheelAccum,
}

impl<S: RowSource> VirtualRows<S> {
    pub fn new(src: S, row_h: i32, pad_x: i32) -> Self {
        VirtualRows {
            src,
            bounds: Rect::default(),
            scroll_row: 0,
            row_h: row_h.max(1),
            pad_x,
            wheel: WheelAccum::default(),
        }
    }

    pub fn scroll_row(&self) -> usize {
        self.scroll_row
    }

    /// DPI 변화 등에 따른 행 높이·패딩 갱신(WM_DPICHANGED 경로).
    pub fn set_metrics(&mut self, row_h: i32, pad_x: i32, inv: &mut Invalidations) {
        let row_h = row_h.max(1);
        if self.row_h != row_h || self.pad_x != pad_x {
            self.row_h = row_h;
            self.pad_x = pad_x;
            self.clamp_scroll();
            inv.push(self.bounds);
        }
    }

    /// 현재 높이에서 그릴 행 수(부분 행 포함).
    fn visible_rows(&self) -> usize {
        ((self.bounds.h + self.row_h - 1) / self.row_h).max(0) as usize
    }

    /// 스크롤 상한 = 전체 - 완전 가시 행 수.
    fn max_scroll(&self) -> usize {
        let full = (self.bounds.h / self.row_h).max(0) as usize;
        self.src.len().saturating_sub(full)
    }

    fn clamp_scroll(&mut self) {
        self.scroll_row = self.scroll_row.min(self.max_scroll());
    }

    fn scroll_to(&mut self, target: isize, inv: &mut Invalidations) {
        let clamped = target.clamp(0, self.max_scroll() as isize) as usize;
        if clamped != self.scroll_row {
            self.scroll_row = clamped;
            inv.push(self.bounds); // 전 행 이동 — 위젯 영역 전체 무효화
        }
    }
}

impl<S: RowSource> Widget for VirtualRows<S> {
    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect, inv: &mut Invalidations) {
        if self.bounds != bounds {
            let old = self.bounds;
            self.bounds = bounds;
            self.clamp_scroll();
            inv.push(old.union(&bounds));
        }
    }

    fn on_event(&mut self, ev: &InputEvent, inv: &mut Invalidations) {
        let cur = self.scroll_row as isize;
        let page = (self.bounds.h / self.row_h).max(1) as isize;
        match *ev {
            InputEvent::Wheel { delta } => {
                let lines = self.wheel.add(delta, WHEEL_LINES) as isize;
                if lines != 0 {
                    self.scroll_to(cur - lines, inv);
                }
            }
            InputEvent::Key(k) => match k {
                Key::Up => self.scroll_to(cur - 1, inv),
                Key::Down => self.scroll_to(cur + 1, inv),
                Key::PageUp => self.scroll_to(cur - page, inv),
                Key::PageDown => self.scroll_to(cur + page, inv),
                Key::Home => self.scroll_to(0, inv),
                Key::End => self.scroll_to(isize::MAX / 2, inv),
            },
        }
    }

    fn paint(&self, ctx: &mut dyn DrawCtx, theme: &Theme) {
        let b = self.bounds;
        let first = self.scroll_row;
        let count = self
            .visible_rows()
            .min(self.src.len().saturating_sub(first));

        for i in 0..count {
            let row = first + i;
            let y = b.y + i as i32 * self.row_h;
            let rc = Rect::new(b.x, y, b.w, self.row_h);
            let bg = if row.is_multiple_of(2) {
                theme.panel_bg
            } else {
                theme.panel_bg_alt
            };
            // 텍스트 세로 위치: 행 높이의 4/5를 글자 높이로 보고 중앙 정렬(M0-7 계승)
            let ty = y + (self.row_h - (self.row_h * 4) / 5) / 2;
            ctx.text_opaque(
                b.x + self.pad_x,
                ty,
                rc,
                &self.src.row_text(row),
                theme.text,
                bg,
            );
        }

        // 마지막 행 아래 잔여 영역
        let drawn_h = count as i32 * self.row_h;
        if drawn_h < b.h {
            ctx.fill_rect(
                Rect::new(b.x, b.y + drawn_h, b.w, b.h - drawn_h),
                theme.panel_bg,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Color;

    struct Rows(usize);
    impl RowSource for Rows {
        fn len(&self) -> usize {
            self.0
        }
        fn row_text(&self, row: usize) -> String {
            format!("row-{row}")
        }
    }

    fn list(total: usize, h: i32) -> (VirtualRows<Rows>, Invalidations) {
        let mut inv = Invalidations::default();
        let mut v = VirtualRows::new(Rows(total), 20, 12);
        v.set_bounds(Rect::new(0, 0, 400, h), &mut inv);
        inv.drain().for_each(drop);
        (v, inv)
    }

    #[test]
    fn scroll_clamps_to_total_minus_full_rows() {
        let (mut v, mut inv) = list(100, 200); // 완전 가시 10행
        v.on_event(&InputEvent::Key(Key::End), &mut inv);
        assert_eq!(v.scroll_row(), 90);
        v.on_event(&InputEvent::Key(Key::Down), &mut inv);
        assert_eq!(v.scroll_row(), 90); // 상한 고정
        v.on_event(&InputEvent::Key(Key::Home), &mut inv);
        assert_eq!(v.scroll_row(), 0);
        v.on_event(&InputEvent::Key(Key::Up), &mut inv);
        assert_eq!(v.scroll_row(), 0); // 하한 고정
    }

    #[test]
    fn short_list_never_scrolls() {
        let (mut v, mut inv) = list(5, 200);
        v.on_event(&InputEvent::Key(Key::End), &mut inv);
        assert_eq!(v.scroll_row(), 0);
        assert!(inv.is_empty()); // 이동 없음 = 무효화 없음
    }

    #[test]
    fn wheel_scrolls_and_invalidates_bounds() {
        let (mut v, mut inv) = list(100, 200);
        v.on_event(&InputEvent::Wheel { delta: -120 }, &mut inv); // 아래로 1노치 = 3행
        assert_eq!(v.scroll_row(), 3);
        assert_eq!(
            inv.drain().collect::<Vec<_>>(),
            vec![Rect::new(0, 0, 400, 200)]
        );
    }

    #[test]
    fn page_keys_move_by_full_visible_rows() {
        let (mut v, mut inv) = list(100, 205); // 완전 가시 10행(부분 행 1 제외)
        v.on_event(&InputEvent::Key(Key::PageDown), &mut inv);
        assert_eq!(v.scroll_row(), 10);
        v.on_event(&InputEvent::Key(Key::PageUp), &mut inv);
        assert_eq!(v.scroll_row(), 0);
    }

    #[test]
    fn shrinking_bounds_reclamps_scroll() {
        let (mut v, mut inv) = list(100, 200);
        v.on_event(&InputEvent::Key(Key::End), &mut inv);
        assert_eq!(v.scroll_row(), 90);
        v.set_bounds(Rect::new(0, 0, 400, 2000), &mut inv); // 완전 가시 100행 → 상한 0
        assert_eq!(v.scroll_row(), 0);
    }

    #[test]
    fn paint_visits_only_visible_rows() {
        struct Probe {
            texts: Vec<String>,
            fills: Vec<Rect>,
        }
        impl DrawCtx for Probe {
            fn fill_rect(&mut self, rect: Rect, _color: Color) {
                self.fills.push(rect);
            }
            fn text_opaque(
                &mut self,
                _x: i32,
                _y: i32,
                _clip: Rect,
                text: &str,
                _fg: Color,
                _bg: Color,
            ) {
                self.texts.push(text.to_string());
            }
        }
        let (mut v, mut inv) = list(100_000, 205); // 부분 행 포함 11행
        v.on_event(&InputEvent::Wheel { delta: -120 }, &mut inv);
        let mut p = Probe {
            texts: vec![],
            fills: vec![],
        };
        v.paint(&mut p, &Theme::dark());
        assert_eq!(p.texts.len(), 11);
        assert_eq!(p.texts[0], "row-3");
        assert!(p.fills.is_empty()); // 행이 영역을 다 덮으면 잔여 채우기 없음
    }
}
