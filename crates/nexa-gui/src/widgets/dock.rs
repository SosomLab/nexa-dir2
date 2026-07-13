//! 하단 도크 — **정보 뷰**(M4-1, 원본 BottomDockView의 Info 종류·docs/20 하단 도크 대원칙:
//! 듀얼=좌↔좌·우↔우 — 각 패널 하단에 1개). 미리보기(M4-2)·터미널(M4-3)은 종류 스왑으로 확장.
//! 플랫폼 중립 — 텍스트 라인 렌더 + 상단 경계선.

use crate::draw::DrawCtx;
use crate::event::InputEvent;
use crate::geom::Rect;
use crate::theme::Theme;
use crate::widget::{Invalidations, Widget};

/// 하단 도크(정보 뷰) — 호스트가 [`set_lines`](InfoDock::set_lines)로 내용을 공급한다
/// (원본 InfoText 대응 — 선택 항목 속성·현재 폴더).
pub struct InfoDock {
    bounds: Rect,
    row_h: i32,
    pad_x: i32,
    /// 종류 라벨(α = "정보" 고정 — M4-2에서 토글 스트립로 확장).
    title: String,
    lines: Vec<String>,
}

impl InfoDock {
    pub fn new(title: impl Into<String>, row_h: i32, pad_x: i32) -> Self {
        InfoDock {
            bounds: Rect::default(),
            row_h: row_h.max(1),
            pad_x,
            title: title.into(),
            lines: Vec::new(),
        }
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

    /// i18n 전환 등 — 종류 라벨 갱신(변경 시에만 무효화).
    pub fn set_title(&mut self, title: impl Into<String>, inv: &mut Invalidations) {
        let title = title.into();
        if self.title != title {
            self.title = title;
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

    fn on_event(&mut self, _ev: &InputEvent, _inv: &mut Invalidations) {
        // α: 표시 전용(종류 토글·리사이즈 드래그는 M4-2/S2)
    }

    fn paint(&self, ctx: &mut dyn DrawCtx, theme: &Theme) {
        let b = self.bounds;
        if b.h <= 0 {
            return;
        }
        ctx.fill_rect(b, theme.panel_bg);
        // 상단 경계선(리스트와 구분 — docs/39 §2 경계선+명도 차)
        ctx.fill_rect(Rect::new(b.x, b.y, b.w, 1), theme.border);
        // 종류 라벨 스트립(헤더 톤)
        let strip = Rect::new(b.x, b.y + 1, b.w, self.row_h.min(b.h - 1));
        let ty = |cell: Rect| cell.y + (cell.h - (cell.h * 4) / 5) / 2;
        ctx.text_opaque(
            strip.x + self.pad_x,
            ty(strip),
            strip,
            &self.title,
            theme.text_dim,
            theme.header_bg,
        );
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
