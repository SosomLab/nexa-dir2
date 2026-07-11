//! 가상화 행 리스트 — 스크롤·가시 행(M1-1) → 트리 표시(M1-3) → **컬럼 시스템(M1-4)**.
//! 원본 docs/23 계승: 헤더 행(정렬 3상태 ▲/▼ 앞·다중 순번 ①② 뒤·Shift 다중열·드래그 리사이즈)·
//! 가로 스크롤. 컬럼 의미는 모른다 — 셀 값·정렬은 [`RowSource`]에 위임(key 불투명).

use crate::columns::{order_badge, Align, Column};
use crate::draw::DrawCtx;
use crate::event::{InputEvent, Key, WheelAccum};
use crate::geom::{Point, Rect};
use crate::theme::Theme;
use crate::widget::{Invalidations, Widget};

/// 휠 1노치당 스크롤 행 수(M0-7 계승).
const WHEEL_LINES: i32 = 3;
/// 가로 휠 1"행"당 픽셀.
const HSCROLL_PX: i32 = 16;
/// 리사이즈 핸들 판정 폭 — 컬럼 오른쪽 경계 기준 [right-6, right+2).
const RESIZE_ZONE_L: i32 = 6;
const RESIZE_ZONE_R: i32 = 2;

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

/// 트리 컬럼(key 0) 한 행의 표시 데이터.
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
    /// 트리 컬럼(key 0)의 행 데이터.
    fn row(&self, index: usize) -> RowItem;
    /// 그 외 컬럼의 셀 텍스트(key = Column.key). 기본 = 빈 값.
    fn cell(&self, index: usize, key: u32) -> String {
        let _ = (index, key);
        String::new()
    }
    /// 행 활성화(클릭) — 목록 구조가 바뀌었으면 `true`(위젯이 전체 무효화).
    fn toggle(&mut self, index: usize) -> bool {
        let _ = index;
        false
    }
    /// 정렬 적용(우선순위 순 `(key, desc)`, 빈 목록 = 열거 순서). 반영했으면 `true`.
    fn set_sort(&mut self, keys: &[(u32, bool)]) -> bool {
        let _ = keys;
        false
    }
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// 리사이즈 드래그 상태.
#[derive(Clone, Copy, Debug)]
struct ResizeDrag {
    col: usize,
    start_x: i32,
    start_w: i32,
}

/// 세로 가상화 행 리스트 + 컬럼 헤더 — 프레임 비용은 bounds 높이에만 비례(docs/01 §3).
pub struct VirtualRows<S> {
    src: S,
    bounds: Rect,
    scroll_row: usize,
    scroll_x: i32,
    row_h: i32,
    pad_x: i32,
    /// 트리 깊이 1단계의 가로 들여쓰기(px). 마커 폭도 이 값을 쓴다.
    indent_w: i32,
    wheel: WheelAccum,
    hwheel: WheelAccum,
    /// 컬럼 정의(비면 헤더 없는 단일 트리 컬럼 — M1-3 호환).
    columns: Vec<Column>,
    /// 정렬 상태(우선순위 순). 빈 목록 = 소스 기본 정렬.
    sort: Vec<(u32, bool)>,
    resize: Option<ResizeDrag>,
}

impl<S: RowSource> VirtualRows<S> {
    pub fn new(src: S, row_h: i32, pad_x: i32, indent_w: i32) -> Self {
        VirtualRows {
            src,
            bounds: Rect::default(),
            scroll_row: 0,
            scroll_x: 0,
            row_h: row_h.max(1),
            pad_x,
            indent_w: indent_w.max(1),
            wheel: WheelAccum::default(),
            hwheel: WheelAccum::default(),
            columns: Vec::new(),
            sort: Vec::new(),
            resize: None,
        }
    }

    pub fn scroll_row(&self) -> usize {
        self.scroll_row
    }

    pub fn scroll_x(&self) -> i32 {
        self.scroll_x
    }

    /// 데이터 공급자 접근(호스트가 트리 상태를 조회할 때).
    pub fn source(&self) -> &S {
        &self.src
    }

    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    /// 현재 정렬 상태(우선순위 순).
    pub fn sort(&self) -> &[(u32, bool)] {
        &self.sort
    }

    pub fn set_columns(&mut self, columns: Vec<Column>, inv: &mut Invalidations) {
        self.columns = columns;
        self.clamp_scroll_x();
        inv.push(self.bounds);
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

    // ── 기하 ─────────────────────────────────────────────────────

    /// 헤더 높이(컬럼 없으면 0 — M1-3 호환).
    fn header_h(&self) -> i32 {
        if self.columns.is_empty() {
            0
        } else {
            self.row_h
        }
    }

    fn body_top(&self) -> i32 {
        self.bounds.y + self.header_h()
    }

    fn body_h(&self) -> i32 {
        (self.bounds.h - self.header_h()).max(0)
    }

    /// 전체 컬럼 폭 합(컬럼 없으면 위젯 폭).
    fn total_w(&self) -> i32 {
        if self.columns.is_empty() {
            self.bounds.w
        } else {
            self.columns.iter().map(|c| c.width).sum()
        }
    }

    /// 컬럼 `i`의 x 시작(스크롤 반영).
    fn col_x(&self, i: usize) -> i32 {
        let before: i32 = self.columns[..i].iter().map(|c| c.width).sum();
        self.bounds.x - self.scroll_x + before
    }

    /// 현재 높이에서 그릴 행 수(부분 행 포함).
    fn visible_rows(&self) -> usize {
        ((self.body_h() + self.row_h - 1) / self.row_h).max(0) as usize
    }

    /// 스크롤 상한 = 전체 - 완전 가시 행 수.
    fn max_scroll(&self) -> usize {
        let full = (self.body_h() / self.row_h).max(0) as usize;
        self.src.len().saturating_sub(full)
    }

    fn clamp_scroll(&mut self) {
        self.scroll_row = self.scroll_row.min(self.max_scroll());
        self.clamp_scroll_x();
    }

    fn clamp_scroll_x(&mut self) {
        let max_x = (self.total_w() - self.bounds.w).max(0);
        self.scroll_x = self.scroll_x.clamp(0, max_x);
    }

    fn scroll_to(&mut self, target: isize, inv: &mut Invalidations) {
        let clamped = target.clamp(0, self.max_scroll() as isize) as usize;
        if clamped != self.scroll_row {
            self.scroll_row = clamped;
            inv.push(self.bounds); // 전 행 이동 — 위젯 영역 전체 무효화
        }
    }

    /// 클라이언트 좌표 → 본문 행 인덱스(범위 밖이면 `None`).
    fn row_at(&self, x: i32, y: i32) -> Option<usize> {
        if !self.bounds.contains(Point { x, y }) || y < self.body_top() {
            return None;
        }
        let row = self.scroll_row + ((y - self.body_top()) / self.row_h) as usize;
        (row < self.src.len()).then_some(row)
    }

    /// 헤더 명중 판정: `Some((컬럼 인덱스, 리사이즈 핸들 여부))`.
    fn header_hit(&self, x: i32, y: i32) -> Option<(usize, bool)> {
        if self.columns.is_empty() || !self.bounds.contains(Point { x, y }) || y >= self.body_top()
        {
            return None;
        }
        // 핸들 우선: 컬럼 오른쪽 경계 [right-6, right+2)
        for i in 0..self.columns.len() {
            let right = self.col_x(i) + self.columns[i].width;
            if self.columns[i].resizable && x >= right - RESIZE_ZONE_L && x < right + RESIZE_ZONE_R
            {
                return Some((i, true));
            }
        }
        for i in 0..self.columns.len() {
            let cx = self.col_x(i);
            if x >= cx && x < cx + self.columns[i].width {
                return Some((i, false));
            }
        }
        None
    }

    // ── 정렬 (원본 docs/23 §4: 3상태 순환·Shift 다중열) ────────────

    fn dir_of(&self, key: u32) -> Option<bool> {
        self.sort.iter().find(|(k, _)| *k == key).map(|(_, d)| *d)
    }

    /// 헤더 클릭 정렬. 단순 클릭 = 단일 정렬 리셋 + 3상태 순환(없음→▲→▼→없음).
    /// Shift+클릭 & 기존 정렬 ≥1 = 키 추가/방향 순환/제거(다중열).
    fn apply_sort(&mut self, key: u32, shift: bool, inv: &mut Invalidations) {
        let cur = self.dir_of(key);
        if shift && !self.sort.is_empty() {
            match cur {
                None => self.sort.push((key, false)), // 추가 = 오름
                Some(false) => {
                    if let Some(e) = self.sort.iter_mut().find(|(k, _)| *k == key) {
                        e.1 = true;
                    }
                }
                Some(true) => self.sort.retain(|(k, _)| *k != key), // 없음 = 제거(순번 당김)
            }
        } else {
            self.sort = match cur {
                None => vec![(key, false)],
                Some(false) => vec![(key, true)],
                Some(true) => Vec::new(), // 없음 = 열거 순서
            };
        }
        let keys = self.sort.clone();
        self.src.set_sort(&keys);
        self.clamp_scroll(); // 정렬로 행 수는 불변이지만 방어
        inv.push(self.bounds); // 헤더 글리프 + 본문 전체
    }

    // ── 페인트 보조 ──────────────────────────────────────────────

    /// 트리 컬럼(마커+들여쓰기+이름)을 `cell` 안에 그린다.
    fn paint_tree_cell(
        &self,
        ctx: &mut dyn DrawCtx,
        theme: &Theme,
        item: &RowItem,
        cell: Rect,
        ty: i32,
        bg: crate::theme::Color,
    ) {
        let indent = cell.x + self.pad_x + item.depth as i32 * self.indent_w;
        ctx.text_opaque(indent, ty, cell, item.marker.glyph(), theme.text_dim, bg);
        let name_x = indent + self.indent_w;
        if name_x < cell.right() {
            let name_rc = Rect::new(name_x, cell.y, cell.right() - name_x, cell.h);
            ctx.text_opaque(name_x, ty, name_rc, &item.text, theme.text, bg);
        }
    }

    /// 헤더 셀 제목: ▲/▼는 이름 앞, 다중 정렬 순번(①②…)은 이름 뒤(원본 docs/23 §4).
    fn header_label(&self, col: &Column) -> String {
        let mut s = String::new();
        if let Some(desc) = self.dir_of(col.key) {
            s.push_str(if desc { "▼ " } else { "▲ " });
        }
        s.push_str(&col.title);
        if self.sort.len() > 1 {
            if let Some(order) = self.sort.iter().position(|(k, _)| *k == col.key) {
                s.push(' ');
                s.push_str(order_badge(order));
            }
        }
        s
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
        let page = (self.body_h() / self.row_h).max(1) as isize;
        match *ev {
            InputEvent::Wheel { delta } => {
                let lines = self.wheel.add(delta, WHEEL_LINES) as isize;
                if lines != 0 {
                    self.scroll_to(cur - lines, inv);
                }
            }
            InputEvent::HWheel { delta } => {
                let lines = self.hwheel.add(delta, WHEEL_LINES);
                if lines != 0 {
                    let old = self.scroll_x;
                    self.scroll_x += lines * HSCROLL_PX;
                    self.clamp_scroll_x();
                    if self.scroll_x != old {
                        inv.push(self.bounds);
                    }
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
            InputEvent::MouseDown { x, y, shift } => {
                if let Some((i, handle)) = self.header_hit(x, y) {
                    if handle {
                        self.resize = Some(ResizeDrag {
                            col: i,
                            start_x: x,
                            start_w: self.columns[i].width,
                        });
                    } else if self.columns[i].sortable {
                        let key = self.columns[i].key;
                        self.apply_sort(key, shift, inv);
                    }
                } else if let Some(row) = self.row_at(x, y) {
                    if self.src.toggle(row) {
                        // 목록 구조 변경(펼침/접힘) — 행 수가 줄었을 수 있으니 재클램프
                        self.clamp_scroll();
                        inv.push(self.bounds);
                    }
                }
            }
            InputEvent::MouseMove { x, y: _ } => {
                if let Some(drag) = self.resize {
                    let w =
                        (drag.start_w + (x - drag.start_x)).max(self.columns[drag.col].min_width);
                    if w != self.columns[drag.col].width {
                        self.columns[drag.col].width = w;
                        self.clamp_scroll_x();
                        inv.push(self.bounds);
                    }
                }
            }
            InputEvent::MouseUp { .. } => {
                self.resize = None;
            }
        }
    }

    fn paint(&self, ctx: &mut dyn DrawCtx, theme: &Theme) {
        let b = self.bounds;
        let first = self.scroll_row;
        let count = self
            .visible_rows()
            .min(self.src.len().saturating_sub(first));
        let body_top = self.body_top();

        // ── 본문 행 ──
        for i in 0..count {
            let row = first + i;
            let y = body_top + i as i32 * self.row_h;
            let bg = if row.is_multiple_of(2) {
                theme.panel_bg
            } else {
                theme.panel_bg_alt
            };
            // 텍스트 세로 위치: 행 높이의 4/5를 글자 높이로 보고 중앙 정렬(M0-7 계승)
            let ty = y + (self.row_h - (self.row_h * 4) / 5) / 2;

            if self.columns.is_empty() {
                // M1-3 호환: 단일 트리 컬럼이 전체 폭
                let item = self.src.row(row);
                let rc = Rect::new(b.x, y, b.w, self.row_h);
                self.paint_tree_cell(ctx, theme, &item, rc, ty, bg);
                continue;
            }

            for (ci, col) in self.columns.iter().enumerate() {
                let cx = self.col_x(ci);
                if cx >= b.right() || cx + col.width <= b.x {
                    continue; // 가로 스크롤로 화면 밖
                }
                let cell = Rect::new(cx, y, col.width, self.row_h);
                if col.key == 0 {
                    let item = self.src.row(row);
                    self.paint_tree_cell(ctx, theme, &item, cell, ty, bg);
                } else {
                    let text = self.src.cell(row, col.key);
                    let tx = match col.align {
                        Align::Left => cell.x + self.pad_x,
                        Align::Right => {
                            let w = ctx.text_width(&text);
                            (cell.right() - self.pad_x - w).max(cell.x + self.pad_x)
                        }
                    };
                    ctx.text_opaque(tx, ty, cell, &text, theme.text, bg);
                }
            }
            // 마지막 컬럼 오른쪽 잔여
            let cols_right =
                self.col_x(self.columns.len() - 1) + self.columns.last().map_or(0, |c| c.width);
            if cols_right < b.right() {
                ctx.fill_rect(
                    Rect::new(cols_right, y, b.right() - cols_right, self.row_h),
                    bg,
                );
            }
        }

        // 마지막 행 아래 잔여 영역
        let drawn_h = count as i32 * self.row_h;
        if body_top + drawn_h < b.bottom() {
            ctx.fill_rect(
                Rect::new(
                    b.x,
                    body_top + drawn_h,
                    b.w,
                    b.bottom() - (body_top + drawn_h),
                ),
                theme.panel_bg,
            );
        }

        // ── 헤더(본문 위에 그려 스크롤과 무관하게 고정) ──
        if !self.columns.is_empty() {
            let hy = b.y;
            let hty = hy + (self.row_h - (self.row_h * 4) / 5) / 2;
            for (ci, col) in self.columns.iter().enumerate() {
                let cx = self.col_x(ci);
                if cx >= b.right() || cx + col.width <= b.x {
                    continue;
                }
                let cell = Rect::new(cx, hy, col.width, self.row_h);
                ctx.text_opaque(
                    cell.x + self.pad_x,
                    hty,
                    cell,
                    &self.header_label(col),
                    theme.text,
                    theme.header_bg,
                );
                // 컬럼 경계선(헤더 안, 오른쪽 1px)
                let sep_x = cell.right() - 1;
                if sep_x >= b.x && sep_x < b.right() {
                    ctx.fill_rect(Rect::new(sep_x, hy, 1, self.row_h), theme.border);
                }
            }
            let cols_right =
                self.col_x(self.columns.len() - 1) + self.columns.last().map_or(0, |c| c.width);
            if cols_right < b.right() {
                ctx.fill_rect(
                    Rect::new(cols_right, hy, b.right() - cols_right, self.row_h),
                    theme.header_bg,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Color;
    use std::cell::RefCell;

    /// 정적 N행 소스(토글 없음) + set_sort 기록.
    struct Rows {
        n: usize,
        sorts: RefCell<Vec<Vec<(u32, bool)>>>,
    }
    impl Rows {
        fn new(n: usize) -> Rows {
            Rows {
                n,
                sorts: RefCell::new(Vec::new()),
            }
        }
    }
    impl RowSource for Rows {
        fn len(&self) -> usize {
            self.n
        }
        fn row(&self, index: usize) -> RowItem {
            RowItem {
                text: format!("row-{index}"),
                depth: 0,
                marker: Marker::None,
            }
        }
        fn cell(&self, index: usize, key: u32) -> String {
            format!("c{key}-{index}")
        }
        fn set_sort(&mut self, keys: &[(u32, bool)]) -> bool {
            self.sorts.borrow_mut().push(keys.to_vec());
            true
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
        let mut v = VirtualRows::new(Rows::new(total), 20, 12, 16);
        v.set_bounds(Rect::new(0, 0, 400, h), &mut inv);
        inv.drain().for_each(drop);
        (v, inv)
    }

    fn cols() -> Vec<Column> {
        vec![
            Column::new(0, "이름", 200),
            Column::new(2, "크기", 100).right_aligned(),
            Column::new(3, "수정한 날짜", 150),
        ]
    }

    fn list_with_cols(total: usize, h: i32) -> (VirtualRows<Rows>, Invalidations) {
        let (mut v, mut inv) = list(total, h);
        v.set_columns(cols(), &mut inv);
        inv.drain().for_each(drop);
        (v, inv)
    }

    fn down(v: &mut VirtualRows<Rows>, inv: &mut Invalidations, x: i32, y: i32, shift: bool) {
        v.on_event(&InputEvent::MouseDown { x, y, shift }, inv);
    }

    // ── M1-3 계승(컬럼 없음 = 헤더 없음) ──

    #[test]
    fn scroll_clamps_to_total_minus_full_rows() {
        let (mut v, mut inv) = list(100, 200); // 완전 가시 10행
        v.on_event(&InputEvent::Key(Key::End), &mut inv);
        assert_eq!(v.scroll_row(), 90);
        v.on_event(&InputEvent::Key(Key::Home), &mut inv);
        assert_eq!(v.scroll_row(), 0);
    }

    #[test]
    fn click_toggles_row_without_columns() {
        let mut inv = Invalidations::default();
        let mut v = VirtualRows::new(Expandable { expanded: false }, 20, 12, 16);
        v.set_bounds(Rect::new(0, 0, 400, 200), &mut inv);
        inv.drain().for_each(drop);
        v.on_event(
            &InputEvent::MouseDown {
                x: 10,
                y: 5,
                shift: false,
            },
            &mut inv,
        ); // 헤더 없음 → y=5는 0행
        assert_eq!(v.source().len(), 6);
    }

    // ── 헤더·본문 오프셋 ──

    #[test]
    fn header_shifts_body_rows_down() {
        let (mut v, mut inv) = list_with_cols(100, 220); // 헤더 20 + 본문 200 = 완전 가시 10행
        assert_eq!(v.max_scroll(), 90);
        // y=5 → 헤더(정렬 클릭), y=25 → 0행
        assert_eq!(v.row_at(10, 5), None);
        assert_eq!(v.row_at(10, 25), Some(0));
        v.on_event(&InputEvent::Key(Key::End), &mut inv);
        assert_eq!(v.scroll_row(), 90);
    }

    // ── 정렬: 3상태 순환·단일 리셋·Shift 다중열(원본 docs/23 §4) ──

    #[test]
    fn header_click_cycles_three_states() {
        let (mut v, mut inv) = list_with_cols(10, 220);
        let name_x = 50; // 이름 컬럼(0..200)
        down(&mut v, &mut inv, name_x, 5, false);
        assert_eq!(v.sort(), &[(0, false)]); // ▲
        down(&mut v, &mut inv, name_x, 5, false);
        assert_eq!(v.sort(), &[(0, true)]); // ▼
        down(&mut v, &mut inv, name_x, 5, false);
        assert_eq!(v.sort(), &[]); // 없음(열거)
        assert_eq!(
            *v.source().sorts.borrow(),
            vec![vec![(0, false)], vec![(0, true)], vec![]]
        );
    }

    #[test]
    fn plain_click_resets_to_single_sort() {
        let (mut v, mut inv) = list_with_cols(10, 220);
        down(&mut v, &mut inv, 50, 5, false); // 이름 ▲
        down(&mut v, &mut inv, 250, 5, true); // Shift+크기 → 다중 [이름▲, 크기▲]
        assert_eq!(v.sort(), &[(0, false), (2, false)]);
        down(&mut v, &mut inv, 250, 5, false); // 단순 클릭 = 단일 리셋 + 크기의 3상태(▲→▼)
        assert_eq!(v.sort(), &[(2, true)]);
    }

    #[test]
    fn shift_click_adds_cycles_and_removes_keys() {
        let (mut v, mut inv) = list_with_cols(10, 220);
        down(&mut v, &mut inv, 50, 5, true); // 정렬 없음 + Shift = 단일로 동작
        assert_eq!(v.sort(), &[(0, false)]);
        down(&mut v, &mut inv, 250, 5, true); // 크기 추가(오름)
        down(&mut v, &mut inv, 350, 5, true); // 날짜 추가(오름 — 날짜 컬럼 300..450 중 가시 범위)
        assert_eq!(v.sort(), &[(0, false), (2, false), (3, false)]);
        down(&mut v, &mut inv, 250, 5, true); // 크기 방향 순환 ▲→▼
        assert_eq!(v.sort(), &[(0, false), (2, true), (3, false)]);
        down(&mut v, &mut inv, 250, 5, true); // 크기 ▼→없음(제거, 뒤 순번 당김)
        assert_eq!(v.sort(), &[(0, false), (3, false)]);
    }

    #[test]
    fn header_label_shows_arrow_before_and_badge_after() {
        let (mut v, mut inv) = list_with_cols(10, 220);
        down(&mut v, &mut inv, 50, 5, false); // 이름 ▲
        assert_eq!(v.header_label(&v.columns()[0]), "▲ 이름"); // 단일 = 순번 없음
        down(&mut v, &mut inv, 250, 5, true); // + 크기
        assert_eq!(v.header_label(&v.columns()[0]), "▲ 이름 ①");
        assert_eq!(v.header_label(&v.columns()[1]), "▲ 크기 ②");
        assert_eq!(v.header_label(&v.columns()[2]), "수정한 날짜");
    }

    // ── 리사이즈 드래그 ──

    #[test]
    fn drag_handle_resizes_column_with_min_width() {
        let (mut v, mut inv) = list_with_cols(10, 220);
        // 이름 컬럼 오른쪽 경계 x=200 → 핸들 [194, 202)
        down(&mut v, &mut inv, 197, 5, false);
        assert!(v.sort().is_empty(), "핸들 클릭은 정렬이 아님");
        v.on_event(&InputEvent::MouseMove { x: 257, y: 5 }, &mut inv);
        assert_eq!(v.columns()[0].width, 260); // +60
        v.on_event(&InputEvent::MouseMove { x: -500, y: 5 }, &mut inv);
        assert_eq!(v.columns()[0].width, 40); // min_width 고정
        v.on_event(&InputEvent::MouseUp { x: -500, y: 5 }, &mut inv);
        v.on_event(&InputEvent::MouseMove { x: 300, y: 5 }, &mut inv);
        assert_eq!(v.columns()[0].width, 40, "업 이후엔 리사이즈 없음");
    }

    // ── 가로 스크롤 ──

    #[test]
    fn hwheel_scrolls_and_clamps_to_total_width() {
        let (mut v, mut inv) = list_with_cols(10, 220); // 총폭 450, 위젯 400 → max 50
        v.on_event(&InputEvent::HWheel { delta: 120 }, &mut inv); // 3행 × 16px = 48
        assert_eq!(v.scroll_x(), 48);
        v.on_event(&InputEvent::HWheel { delta: 120 }, &mut inv);
        assert_eq!(v.scroll_x(), 50); // 클램프
        v.on_event(&InputEvent::HWheel { delta: -1200 }, &mut inv);
        assert_eq!(v.scroll_x(), 0);
    }

    #[test]
    fn widening_bounds_reclamps_scroll_x() {
        let (mut v, mut inv) = list_with_cols(10, 220);
        v.on_event(&InputEvent::HWheel { delta: 120 }, &mut inv);
        assert_eq!(v.scroll_x(), 48);
        v.set_bounds(Rect::new(0, 0, 1000, 220), &mut inv); // 총폭 450 < 1000 → 0
        assert_eq!(v.scroll_x(), 0);
    }

    // ── 페인트 ──

    #[test]
    fn paint_draws_header_cells_and_right_aligned_size() {
        struct Probe {
            texts: Vec<(i32, i32, String)>,
            fills: Vec<Rect>,
        }
        impl DrawCtx for Probe {
            fn fill_rect(&mut self, rect: Rect, _color: Color) {
                self.fills.push(rect);
            }
            fn text_opaque(
                &mut self,
                x: i32,
                y: i32,
                _clip: Rect,
                text: &str,
                _fg: Color,
                _bg: Color,
            ) {
                self.texts.push((x, y, text.to_string()));
            }
            fn text_width(&mut self, text: &str) -> i32 {
                text.chars().count() as i32 * 8
            }
        }
        let (v, _) = list_with_cols(1, 220);
        let mut p = Probe {
            texts: vec![],
            fills: vec![],
        };
        v.paint(&mut p, &Theme::dark());
        // 본문 0행: 트리(마커+이름), 크기(우측 정렬), 날짜 — 이후 헤더 3개
        let texts: Vec<&str> = p.texts.iter().map(|(_, _, t)| t.as_str()).collect();
        assert!(texts.contains(&"row-0"));
        assert!(texts.contains(&"이름") && texts.contains(&"크기"));
        // 크기 셀 "c2-0"(폭 8*4=32): x = 300(right) - 12(pad) - 32 = 256
        let size_cell = p.texts.iter().find(|(_, _, t)| t == "c2-0").unwrap();
        assert_eq!(size_cell.0, 256);
        // 헤더는 y=0행에 그려짐
        let hdr = p.texts.iter().find(|(_, _, t)| t == "이름").unwrap();
        assert!(hdr.1 < 20);
    }
}
