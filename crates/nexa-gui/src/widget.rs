//! 위젯 trait·무효화 수집 — 창 1개 + 논리 위젯 모델(docs/01 §3: 자식 HWND 최소화).
//! 위젯은 상태 변화 시 더러워진 영역을 [`Invalidations`]에 밀어 넣고,
//! 루프 소유자(nexa-app)가 이를 OS 무효화(`InvalidateRect`)로 번역한다.

use crate::draw::DrawCtx;
use crate::event::InputEvent;
use crate::geom::Rect;
use crate::theme::Theme;

/// 프레임당 무효화 rect 수집기. 교차하는 rect는 union으로 병합해 과분할을 막는다.
#[derive(Default, Debug)]
pub struct Invalidations {
    rects: Vec<Rect>,
}

impl Invalidations {
    pub fn push(&mut self, rect: Rect) {
        if rect.is_empty() {
            return;
        }
        // 교차분은 병합 — rect 수는 프레임당 소수라는 전제(가시 영역 한정)
        if let Some(hit) = self.rects.iter_mut().find(|r| r.intersects(&rect)) {
            *hit = hit.union(&rect);
            return;
        }
        self.rects.push(rect);
    }

    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }

    pub fn drain(&mut self) -> impl Iterator<Item = Rect> + '_ {
        self.rects.drain(..)
    }
}

/// 논리 위젯 — HWND 없는 그리기·입력 단위. 좌표는 전부 창 클라이언트 좌표계.
pub trait Widget {
    fn bounds(&self) -> Rect;

    /// 레이아웃 결과 반영. 위젯은 필요한 무효화를 스스로 push한다.
    fn set_bounds(&mut self, bounds: Rect, inv: &mut Invalidations);

    /// 입력 라우팅 — 상태가 바뀌면 더러워진 영역을 push.
    fn on_event(&mut self, ev: &InputEvent, inv: &mut Invalidations);

    /// 자신의 bounds 안을 그린다(가시 영역만 — docs/01 §3).
    fn paint(&self, ctx: &mut dyn DrawCtx, theme: &Theme);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_merges_intersecting_rects() {
        let mut inv = Invalidations::default();
        inv.push(Rect::new(0, 0, 10, 10));
        inv.push(Rect::new(5, 5, 10, 10)); // 교차 → 병합
        inv.push(Rect::new(100, 0, 5, 5)); // 분리 → 별도
        let rects: Vec<_> = inv.drain().collect();
        assert_eq!(
            rects,
            vec![Rect::new(0, 0, 15, 15), Rect::new(100, 0, 5, 5)]
        );
    }

    #[test]
    fn empty_rect_is_ignored() {
        let mut inv = Invalidations::default();
        inv.push(Rect::new(0, 0, 0, 10));
        assert!(inv.is_empty());
    }
}
