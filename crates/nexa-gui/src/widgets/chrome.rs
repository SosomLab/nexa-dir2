//! 도구 모음·상태바 — 원본 docs/20 §2의 고정 높이 크롬 위젯.
//! Toolbar: 버튼 id 통지(네비 ←→↑⟳ 등 — 실행은 호스트). StatusBar: 좌/우 텍스트 표시 전용.

use std::cell::RefCell;

use crate::draw::DrawCtx;
use crate::event::InputEvent;
use crate::geom::{Point, Rect};
use crate::theme::Theme;
use crate::widget::{Invalidations, Widget};

/// 도구 버튼(글리프 텍스트 + 명령 id).
#[derive(Clone, Debug)]
pub struct ToolButton {
    pub id: u32,
    pub glyph: String,
    /// 그룹 구분선(QA 07-14 — 원본 도구 모음 그룹화 PR#10). 클릭 불가.
    pub separator: bool,
    /// 토글 상태 표시(하단 accent 줄 — 숨김/닷파일 보기 등).
    pub checked: bool,
}

impl ToolButton {
    pub fn new(id: u32, glyph: impl Into<String>) -> Self {
        ToolButton {
            id,
            glyph: glyph.into(),
            separator: false,
            checked: false,
        }
    }

    /// 그룹 구분선.
    pub fn sep() -> Self {
        ToolButton {
            id: 0,
            glyph: String::new(),
            separator: true,
            checked: false,
        }
    }

    pub fn toggled(mut self, on: bool) -> Self {
        self.checked = on;
        self
    }
}

pub struct Toolbar {
    buttons: Vec<ToolButton>,
    bounds: Rect,
    row_h: i32,
    pad_x: i32,
    /// 고정 버튼 폭(px). `None` = 글리프 폭 기반. 패널 네비 버튼처럼
    /// 레이아웃이 폭을 미리 알아야 할 때 사용(버튼은 간격 없이 연속 배치).
    button_w: Option<i32>,
    hover: Option<usize>,
    pending: Option<u32>,
    ranges: RefCell<Vec<(i32, i32)>>,
}

impl Toolbar {
    pub fn new(buttons: Vec<ToolButton>, row_h: i32, pad_x: i32) -> Self {
        Toolbar {
            buttons,
            bounds: Rect::default(),
            row_h: row_h.max(1),
            pad_x,
            button_w: None,
            hover: None,
            pending: None,
            ranges: RefCell::new(Vec::new()),
        }
    }

    /// 고정 버튼 폭 모드(선두 여백 없이 bounds 시작부터 연속 배치).
    pub fn with_button_width(mut self, w: i32) -> Self {
        self.button_w = Some(w.max(1));
        self
    }

    pub fn set_button_width(&mut self, w: Option<i32>, inv: &mut Invalidations) {
        self.button_w = w.map(|v| v.max(1));
        inv.push(self.bounds);
    }

    pub fn take_command(&mut self) -> Option<u32> {
        self.pending.take()
    }

    /// 토글 버튼 상태 동기(QA 07-14 — 메뉴 체크와 동일 흐름).
    pub fn set_checked(&mut self, id: u32, on: bool, inv: &mut Invalidations) {
        for b in &mut self.buttons {
            if !b.separator && b.id == id && b.checked != on {
                b.checked = on;
                inv.push(self.bounds);
            }
        }
    }

    pub fn set_metrics(&mut self, row_h: i32, pad_x: i32, inv: &mut Invalidations) {
        self.row_h = row_h.max(1);
        self.pad_x = pad_x;
        inv.push(self.bounds);
    }

    fn button_at(&self, x: i32, y: i32) -> Option<usize> {
        if !self.bounds.contains(Point { x, y }) {
            return None;
        }
        self.ranges
            .borrow()
            .iter()
            .position(|&(lo, hi)| x >= lo && x < hi)
    }
}

impl Widget for Toolbar {
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
                if let Some(i) = self.button_at(x, y) {
                    if !self.buttons[i].separator {
                        self.pending = Some(self.buttons[i].id);
                        inv.push(self.bounds);
                    }
                }
            }
            InputEvent::MouseMove { x, y } => {
                let hover = self.button_at(x, y);
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
        ctx.fill_rect(b, theme.chrome_bg);
        let mut ranges = Vec::with_capacity(self.buttons.len());
        // 고정 폭 모드 = bounds 시작부터, 글리프 폭 모드 = 선두 여백. 버튼은 간격 없이 연속.
        let mut x = if self.button_w.is_some() {
            b.x
        } else {
            b.x + self.pad_x
        };
        for (i, btn) in self.buttons.iter().enumerate() {
            if btn.separator {
                // 그룹 구분선(QA 07-14) — 세로 1px + 양측 여백
                let w = self.pad_x.max(4);
                let lx = x + w / 2;
                ctx.fill_rect(Rect::new(lx, b.y + 3, 1, (b.h - 6).max(0)), theme.border);
                ranges.push((x, x)); // 히트 없음(빈 범위)
                x += w;
                continue;
            }
            let w = self
                .button_w
                .unwrap_or_else(|| ctx.text_width(&btn.glyph) + self.pad_x * 2);
            let cell = Rect::new(x, b.y, w.min((b.right() - x).max(0)), b.h);
            let bg = if self.hover == Some(i) {
                theme.header_bg
            } else {
                theme.chrome_bg
            };
            if cell.w > 0 {
                if self.button_w.is_some() {
                    // 고정 폭(네비 버튼) — 큰 글리프를 셀 중앙에(방향 가시성, 사용자 지시)
                    ctx.glyph_opaque(cell, &btn.glyph, theme.text, bg);
                } else {
                    ctx.text_opaque(cell.x + self.pad_x, ty, cell, &btn.glyph, theme.text, bg);
                }
                if btn.checked {
                    // 토글 켜짐 = 하단 accent 줄(메뉴 체크 대응 시각화)
                    ctx.fill_rect(Rect::new(cell.x, b.bottom() - 2, cell.w, 2), theme.accent);
                }
            }
            ranges.push((cell.x, cell.x + w));
            x += w;
        }
        ctx.fill_rect(Rect::new(b.x, b.bottom() - 1, b.w, 1), theme.border);
        *self.ranges.borrow_mut() = ranges;
    }
}

/// 상태바 — 좌(선택/항목 정보)·우(보조 정보) 텍스트 표시 전용(원본 docs/20 §2).
pub struct StatusBar {
    left: String,
    right: String,
    bounds: Rect,
    row_h: i32,
    pad_x: i32,
}

impl StatusBar {
    pub fn new(row_h: i32, pad_x: i32) -> Self {
        StatusBar {
            left: String::new(),
            right: String::new(),
            bounds: Rect::default(),
            row_h: row_h.max(1),
            pad_x,
        }
    }

    pub fn set_text(
        &mut self,
        left: impl Into<String>,
        right: impl Into<String>,
        inv: &mut Invalidations,
    ) {
        let (l, r) = (left.into(), right.into());
        if l != self.left || r != self.right {
            self.left = l;
            self.right = r;
            inv.push(self.bounds);
        }
    }

    pub fn set_metrics(&mut self, row_h: i32, pad_x: i32, inv: &mut Invalidations) {
        self.row_h = row_h.max(1);
        self.pad_x = pad_x;
        inv.push(self.bounds);
    }
}

impl Widget for StatusBar {
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

    fn on_event(&mut self, _ev: &InputEvent, _inv: &mut Invalidations) {}

    fn paint(&self, ctx: &mut dyn DrawCtx, theme: &Theme) {
        let b = self.bounds;
        let ty = b.y + (b.h - (b.h * 4) / 5) / 2;
        ctx.fill_rect(b, theme.status_bar_bg);
        ctx.text_opaque(
            b.x + self.pad_x,
            ty,
            b,
            &self.left,
            theme.text,
            theme.status_bar_bg,
        );
        if !self.right.is_empty() {
            let rw = ctx.text_width(&self.right);
            let rx = (b.right() - self.pad_x - rw).max(b.x + self.pad_x);
            ctx.text_opaque(
                rx,
                ty,
                Rect::new(rx, b.y, b.right() - rx, b.h),
                &self.right,
                theme.text_dim,
                theme.status_bar_bg,
            );
        }
        ctx.fill_rect(Rect::new(b.x, b.y, b.w, 1), theme.border);
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
    fn toolbar_click_emits_button_id() {
        let mut inv = Invalidations::default();
        let mut t = Toolbar::new(
            vec![ToolButton::new(100, "←"), ToolButton::new(101, "→")],
            20,
            6,
        );
        t.set_bounds(Rect::new(0, 0, 400, 24), &mut inv);
        t.paint(&mut Probe, &Theme::dark());
        // 버튼0: [6, 6+8+12=26) · 버튼1: [29, 49)
        t.on_event(
            &InputEvent::MouseDown {
                x: 10,
                y: 5,
                shift: false,
                ctrl: false,
            },
            &mut inv,
        );
        assert_eq!(t.take_command(), Some(100));
        t.on_event(
            &InputEvent::MouseDown {
                x: 35,
                y: 5,
                shift: false,
                ctrl: false,
            },
            &mut inv,
        );
        assert_eq!(t.take_command(), Some(101));
        t.on_event(
            &InputEvent::MouseDown {
                x: 300,
                y: 5,
                shift: false,
                ctrl: false,
            },
            &mut inv,
        );
        assert_eq!(t.take_command(), None);
    }

    #[test]
    fn statusbar_set_text_invalidates_only_on_change() {
        let mut inv = Invalidations::default();
        let mut s = StatusBar::new(20, 6);
        s.set_bounds(Rect::new(0, 400, 600, 22), &mut inv);
        inv.drain().for_each(drop);
        s.set_text("3개 항목", "RSS", &mut inv);
        assert!(!inv.is_empty());
        inv.drain().for_each(drop);
        s.set_text("3개 항목", "RSS", &mut inv);
        assert!(inv.is_empty(), "동일 텍스트는 무효화 없음");
    }
}
