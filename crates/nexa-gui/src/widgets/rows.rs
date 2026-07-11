//! 가상화 행 리스트 — M0-7 스파이크의 스크롤·가시 행 로직을 위젯으로 이식(M1-1),
//! M1-3에서 트리 표시(들여쓰기·펼침 마커·클릭 토글)로 확장.
//! 데이터는 [`RowSource`]로 추상화 — nexa-tree 평면 스트림이 nexa-app 어댑터로 배선된다.

use crate::draw::DrawCtx;
use crate::event::{InputEvent, Key, WheelAccum};
use crate::geom::Rect;
use crate::theme::Theme;
use crate::widget::{Invalidations, Widget};

/// 휠 1노치당 스크롤 행 수(M0-7 계승).
const WHEEL_LINES: i32 = 3;

/// 행 왼쪽의 펼침 상태 마커.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Marker {
    /// 펼칠 수 없음(파일) — 마커 없음, 자리는 유지(정렬).
    None,
    /// 접힌 디렉터리(▸).
    Collapsed,
    /// 펼친 디렉터리(▾).
    Expanded,
}

impl Marker {
    fn glyph(self) -> &'static str {
        match self {
            Marker::None => "",
            Marker::Collapsed => "▸",
            Marker::Expanded => "▾",
        }
    }
}

/// 한 행의 표시 데이터(코어 → 위젯 투영).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RowItem {
    pub text: String,
    /// 트리 깊이(들여쓰기 단위 수).
    pub depth: u32,
    pub marker: Marker,
}

/// 행 데이터 공급자. 위젯은 가시 행에 대해서만 호출한다.
pub trait RowSource {
    fn len(&self) -> usize;
    fn row(&self, index: usize) -> RowItem;
    /// 행 활성화(클릭) — 목록 구조가 바뀌었으면 `true`(위젯이 전체 무효화).
    /// 기본 구현 = 아무 일 없음(정적 목록).
    fn toggle(&mut self, index: usize) -> bool {
        let _ = index;
        false
    }
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
    /// 트리 깊이 1단계의 가로 들여쓰기(px). 마커 폭도 이 값을 쓴다.
    indent_w: i32,
    wheel: WheelAccum,
}

impl<S: RowSource> VirtualRows<S> {
    pub fn new(src: S, row_h: i32, pad_x: i32, indent_w: i32) -> Self {
        VirtualRows {
            src,
            bounds: Rect::default(),
            scroll_row: 0,
            row_h: row_h.max(1),
            pad_x,
            indent_w: indent_w.max(1),
            wheel: WheelAccum::default(),
        }
    }

    pub fn scroll_row(&self) -> usize {
        self.scroll_row
    }

    /// 데이터 공급자 접근(호스트가 트리 상태를 조회할 때).
    pub fn source(&self) -> &S {
        &self.src
    }

    /// DPI 변화 등에 따른 행 높이·패딩·들여쓰기 갱신(WM_DPICHANGED 경로).
    pub fn set_metrics(&mut self, row_h: i32, pad_x: i32, indent_w: i32, inv: &mut Invalidations) {
        let row_h = row_h.max(1);
        let indent_w = indent_w.max(1);
        if self.row_h != row_h || self.pad_x != pad_x || self.indent_w != indent_w {
            self.row_h = row_h;
            self.pad_x = pad_x;
            self.indent_w = indent_w;
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

    /// 클라이언트 좌표 → 행 인덱스(범위 밖이면 `None`).
    fn row_at(&self, x: i32, y: i32) -> Option<usize> {
        if !self.bounds.contains(crate::geom::Point { x, y }) {
            return None;
        }
        let row = self.scroll_row + ((y - self.bounds.y) / self.row_h) as usize;
        (row < self.src.len()).then_some(row)
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
            InputEvent::MouseDown { x, y } => {
                if let Some(row) = self.row_at(x, y) {
                    if self.src.toggle(row) {
                        // 목록 구조 변경(펼침/접힘) — 행 수가 줄었을 수 있으니 재클램프
                        self.clamp_scroll();
                        inv.push(self.bounds);
                    }
                }
            }
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
            let item = self.src.row(row);
            let y = b.y + i as i32 * self.row_h;
            let rc = Rect::new(b.x, y, b.w, self.row_h);
            let bg = if row.is_multiple_of(2) {
                theme.panel_bg
            } else {
                theme.panel_bg_alt
            };
            // 텍스트 세로 위치: 행 높이의 4/5를 글자 높이로 보고 중앙 정렬(M0-7 계승)
            let ty = y + (self.row_h - (self.row_h * 4) / 5) / 2;
            let indent = b.x + self.pad_x + item.depth as i32 * self.indent_w;
            // 마커: 행 전체 배경을 채우며 그리고(빈 문자열이어도 배경은 채움),
            // 이름: 마커 자리(indent_w) 뒤에서 시작 — 남은 영역만 다시 채워 이중 그리기 최소화
            ctx.text_opaque(indent, ty, rc, item.marker.glyph(), theme.text_dim, bg);
            let name_x = indent + self.indent_w;
            let name_rc = Rect::new(name_x, y, b.right() - name_x, self.row_h);
            ctx.text_opaque(name_x, ty, name_rc, &item.text, theme.text, bg);
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

    /// 정적 N행 소스(토글 없음).
    struct Rows(usize);
    impl RowSource for Rows {
        fn len(&self) -> usize {
            self.0
        }
        fn row(&self, index: usize) -> RowItem {
            RowItem {
                text: format!("row-{index}"),
                depth: 0,
                marker: Marker::None,
            }
        }
    }

    /// 토글 가능한 소스 — index 0을 토글하면 5행이 늘었다 줄었다 한다(트리 펼침 모사).
    struct Expandable {
        expanded: bool,
    }
    impl RowSource for Expandable {
        fn len(&self) -> usize {
            if self.expanded {
                6
            } else {
                1
            }
        }
        fn row(&self, index: usize) -> RowItem {
            RowItem {
                text: format!("row-{index}"),
                depth: u32::from(index > 0),
                marker: if index == 0 {
                    if self.expanded {
                        Marker::Expanded
                    } else {
                        Marker::Collapsed
                    }
                } else {
                    Marker::None
                },
            }
        }
        fn toggle(&mut self, index: usize) -> bool {
            if index == 0 {
                self.expanded = !self.expanded;
                true
            } else {
                false
            }
        }
    }

    fn list(total: usize, h: i32) -> (VirtualRows<Rows>, Invalidations) {
        let mut inv = Invalidations::default();
        let mut v = VirtualRows::new(Rows(total), 20, 12, 16);
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
    fn click_toggles_row_and_invalidates() {
        let mut inv = Invalidations::default();
        let mut v = VirtualRows::new(Expandable { expanded: false }, 20, 12, 16);
        v.set_bounds(Rect::new(0, 0, 400, 200), &mut inv);
        inv.drain().for_each(drop);

        assert_eq!(v.source().len(), 1);
        v.on_event(&InputEvent::MouseDown { x: 10, y: 5 }, &mut inv); // 0행 클릭 → 펼침
        assert_eq!(v.source().len(), 6);
        assert!(!inv.is_empty());
        inv.drain().for_each(drop);

        v.on_event(&InputEvent::MouseDown { x: 10, y: 30 }, &mut inv); // 1행(파일) → 무변화
        assert_eq!(v.source().len(), 6);
        assert!(inv.is_empty());

        v.on_event(&InputEvent::MouseDown { x: 10, y: 5 }, &mut inv); // 다시 접기
        assert_eq!(v.source().len(), 1);
    }

    #[test]
    fn click_outside_rows_or_bounds_is_ignored() {
        let (mut v, mut inv) = list(3, 200); // 행 3개(60px), 나머지 빈 영역
        v.on_event(&InputEvent::MouseDown { x: 10, y: 100 }, &mut inv); // 빈 영역
        v.on_event(&InputEvent::MouseDown { x: -5, y: 5 }, &mut inv); // bounds 밖
        assert!(inv.is_empty());
    }

    #[test]
    fn toggle_that_shrinks_list_reclamps_scroll() {
        let mut inv = Invalidations::default();
        let mut v = VirtualRows::new(Expandable { expanded: true }, 20, 12, 16);
        v.set_bounds(Rect::new(0, 0, 400, 40), &mut inv); // 완전 가시 2행
        inv.drain().for_each(drop);
        v.on_event(&InputEvent::Key(Key::End), &mut inv);
        assert_eq!(v.scroll_row(), 4); // 6행 - 2
                                       // 화면 최상단(스크롤 4 → 0행이 아닌 4행)이지만 클릭 y=5는 4행 → 토글 아님(파일).
                                       // Home으로 올라가 0행(디렉터리)을 접으면 목록 1행 → 스크롤 재클램프 0.
        v.on_event(&InputEvent::Key(Key::Home), &mut inv);
        v.on_event(&InputEvent::MouseDown { x: 10, y: 5 }, &mut inv);
        assert_eq!(v.source().len(), 1);
        assert_eq!(v.scroll_row(), 0);
    }

    #[test]
    fn paint_visits_only_visible_rows_with_indent_and_marker() {
        struct Probe {
            texts: Vec<(i32, String)>,
            fills: Vec<Rect>,
        }
        impl DrawCtx for Probe {
            fn fill_rect(&mut self, rect: Rect, _color: Color) {
                self.fills.push(rect);
            }
            fn text_opaque(
                &mut self,
                x: i32,
                _y: i32,
                _clip: Rect,
                text: &str,
                _fg: Color,
                _bg: Color,
            ) {
                self.texts.push((x, text.to_string()));
            }
        }
        let mut inv = Invalidations::default();
        let mut v = VirtualRows::new(Expandable { expanded: true }, 20, 12, 16);
        v.set_bounds(Rect::new(0, 0, 400, 45), &mut inv); // 부분 행 포함 3행
        let mut p = Probe {
            texts: vec![],
            fills: vec![],
        };
        v.paint(&mut p, &Theme::dark());
        // 행당 2회(마커+이름) × 3행
        assert_eq!(p.texts.len(), 6);
        assert_eq!(p.texts[0], (12, "▾".into())); // 0행 마커(depth 0 → pad_x)
        assert_eq!(p.texts[1], (28, "row-0".into())); // 이름 = 마커 + indent_w
        assert_eq!(p.texts[2], (28, "".into())); // 1행(depth 1) 마커 없음, 들여쓰기 1단
        assert_eq!(p.texts[3], (44, "row-1".into()));
        assert!(p.fills.is_empty()); // 행이 영역을 다 덮음
    }
}
