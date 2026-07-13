//! 가상화 행 리스트 — 스크롤·가시 행(M1-1) → 트리 표시(M1-3) → **컬럼 시스템(M1-4)**.
//! 원본 docs/23 계승: 헤더 행(정렬 3상태 ▲/▼ 앞·다중 순번 ①② 뒤·Shift 다중열·드래그 리사이즈)·
//! 가로 스크롤. 컬럼 의미는 모른다 — 셀 값·정렬은 [`RowSource`]에 위임(key 불투명).

use crate::columns::{order_badge, Align, Column};
use crate::draw::DrawCtx;
use crate::event::{InputEvent, Key, WheelAccum};
use crate::geom::{Point, Rect};
use crate::theme::Theme;
use crate::typeahead::{TypeAhead, TYPEAHEAD_TIMEOUT_MS};
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

/// 클릭 선택 방식(원본 docs/07 §1-2 — 교차폴더 다중 선택).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SelectOp {
    /// 단일 선택(기존 해제) + anchor 갱신.
    Single,
    /// Ctrl — 비연속 토글.
    Toggle,
    /// Shift — anchor~행 가시 범위.
    RangeTo,
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
    /// 행 활성화(펼침 마커 클릭) — 목록 구조가 바뀌었으면 `true`(위젯이 전체 무효화).
    fn toggle(&mut self, index: usize) -> bool {
        let _ = index;
        false
    }
    /// 정렬 적용(우선순위 순 `(key, desc)`, 빈 목록 = 열거 순서). 반영했으면 `true`.
    fn set_sort(&mut self, keys: &[(u32, bool)]) -> bool {
        let _ = keys;
        false
    }
    // ── 선택(기본 = 선택 없음 소스) ──
    fn is_selected(&self, index: usize) -> bool {
        let _ = index;
        false
    }
    fn select(&mut self, index: usize, op: SelectOp) -> bool {
        let _ = (index, op);
        false
    }
    /// 가시 범위 `lo..=hi`로 선택을 대체(러버밴드).
    fn select_span(&mut self, lo: usize, hi: usize) -> bool {
        let _ = (lo, hi);
        false
    }
    fn select_all(&mut self) -> bool {
        false
    }
    /// 선택 전체 해제(빈 영역 클릭). 해제했으면 `true`.
    fn clear_selection(&mut self) -> bool {
        false
    }
    /// 타입어헤드 매칭(원본 docs/32 §6 — 가시 스트림 위치상대 starts-with + wrap).
    /// `caret` 다음부터 검색(코어 `find_prefix` 규약). 기본 = 매치 없음.
    fn find_prefix(&self, caret: Option<usize>, prefix: &str) -> Option<usize> {
        let _ = (caret, prefix);
        None
    }
    /// 행 아이콘 `(키, 로드 힌트)` — DrawCtx가 해석(M1-7 셸 아이콘). 기본 = 아이콘 없음.
    fn icon(&self, index: usize) -> Option<(String, String)> {
        let _ = index;
        None
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

/// 러버밴드 드래그 상태(본문 빈 영역에서 시작).
#[derive(Clone, Copy, Debug)]
struct BandDrag {
    ox: i32,
    oy: i32,
    cx: i32,
    cy: i32,
}

impl BandDrag {
    fn rect(&self) -> Rect {
        let x = self.ox.min(self.cx);
        let y = self.oy.min(self.cy);
        Rect::new(x, y, (self.ox - self.cx).abs(), (self.oy - self.cy).abs())
    }
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
    band: Option<BandDrag>,
    /// 캐럿(키보드 네비 기준 행 — docs/07 §8·docs/32).
    caret: Option<usize>,
    typeahead: TypeAhead,
    /// 인라인 이름변경(M3-2, 원본 B-6) — Some((행, 편집 상태)). 캐럿·선택은 edit.rs 공용 모델.
    rename: Option<(usize, crate::edit::EditState)>,
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
            band: None,
            caret: None,
            typeahead: TypeAhead::new(TYPEAHEAD_TIMEOUT_MS),
            rename: None,
        }
    }

    // ── 인라인 이름변경(M3-2, 원본 B-6) ─────────────────────────────

    pub fn is_renaming(&self) -> bool {
        self.rename.is_some()
    }

    /// 편집 상태 (행, 현재 버퍼) — IME 배치·호스트 표시용.
    pub fn rename_state(&self) -> Option<(usize, String)> {
        self.rename.as_ref().map(|(r, e)| (*r, e.text()))
    }

    /// 인라인 이름변경 시작 — 캐럿을 그 행으로, 가시 범위로 스크롤.
    /// 초기 선택 = 파일이면 **이름부**(마지막 `.` 앞), 폴더면 전체(탐색기 관례 — QA 07-13).
    pub fn begin_rename(&mut self, row: usize, initial: &str, inv: &mut Invalidations) {
        if row >= self.src.len() {
            return;
        }
        self.caret = Some(row);
        self.scroll_into_view(row);
        let is_dir = self.src.row(row).marker != Marker::None;
        let sel_to = if is_dir {
            initial.chars().count()
        } else {
            match initial.rfind('.') {
                Some(i) if i > 0 => initial[..i].chars().count(),
                _ => initial.chars().count(),
            }
        };
        self.rename = Some((
            row,
            crate::edit::EditState::with_selection_to(initial, sel_to),
        ));
        inv.push(self.bounds);
    }

    /// 편집 문자 입력(`'\u{8}'` = Backspace, 그 외 제어 문자 무시) — 선택은 대체/삭제.
    pub fn rename_char(&mut self, c: char, inv: &mut Invalidations) {
        let Some((_, es)) = &mut self.rename else {
            return;
        };
        if c == '\u{8}' {
            es.backspace();
        } else if !c.is_control() {
            es.insert(c);
        } else {
            return;
        }
        inv.push(self.bounds);
    }

    /// 편집 키(←→/Home/End/Shift 선택·Ctrl+A·Delete — 실기 QA 07-13).
    pub fn rename_key(&mut self, k: crate::edit::EditKey, shift: bool, inv: &mut Invalidations) {
        if let Some((_, es)) = &mut self.rename {
            es.key(k, shift);
            inv.push(self.bounds);
        }
    }

    /// 편집 취소(Esc·외부 클릭) — 입력 무시.
    pub fn cancel_rename(&mut self, inv: &mut Invalidations) {
        if self.rename.take().is_some() {
            inv.push(self.bounds);
        }
    }

    /// 편집 제출(Enter) — (행, 새 이름) 반환. 실제 rename·재로드는 호스트 책임.
    pub fn submit_rename(&mut self, inv: &mut Invalidations) -> Option<(usize, String)> {
        let taken = self.rename.take().map(|(r, e)| (r, e.text()));
        if taken.is_some() {
            inv.push(self.bounds);
        }
        taken
    }

    /// 타입어헤드 버퍼(HUD·타이머 판단용). 빈 값 = 비활성.
    pub fn typeahead_text(&self) -> &str {
        self.typeahead.text()
    }

    /// 주기 점검(WM_TIMER) — 타입어헤드 타임아웃 소거.
    pub fn tick(&mut self, now_ms: u64, inv: &mut Invalidations) {
        if self.typeahead.tick(now_ms) {
            inv.push(self.bounds);
        }
    }

    pub fn caret(&self) -> Option<usize> {
        self.caret
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

    /// 행이 보이도록 세로 스크롤 조정(원본 ScrollIndexIntoView 대응).
    fn scroll_into_view(&mut self, row: usize) {
        let full = ((self.body_h() / self.row_h).max(1)) as usize;
        if row < self.scroll_row {
            self.scroll_row = row;
        } else if row >= self.scroll_row + full {
            self.scroll_row = row + 1 - full;
        }
        self.scroll_row = self.scroll_row.min(self.max_scroll());
    }

    /// 캐럿 이동 + 선택 규약(탐색기): 평이동=단일 선택, Shift=범위, Ctrl=캐럿만.
    fn move_caret(&mut self, target: usize, shift: bool, ctrl: bool, inv: &mut Invalidations) {
        self.caret = Some(target);
        if shift {
            self.src.select(target, SelectOp::RangeTo);
        } else if !ctrl {
            self.src.select(target, SelectOp::Single);
        }
        self.scroll_into_view(target);
        inv.push(self.bounds);
    }

    /// 가시 목록에서 `row`의 부모 행(더 얕은 깊이의 직전 행). 최상위면 `None`.
    fn parent_row(&self, row: usize) -> Option<usize> {
        let depth = self.src.row(row).depth;
        if depth == 0 {
            return None;
        }
        (0..row).rev().find(|&i| self.src.row(i).depth < depth)
    }

    /// 타입어헤드 검색 실행 — 매치 시 단일 선택+캐럿+스크롤(원본 docs/32 §6).
    fn typeahead_find(&mut self, prefix: &str, include_caret: bool, inv: &mut Invalidations) {
        // find_prefix는 caret "다음"부터 — 현재 행 포함 재평가는 caret-1 기준
        let base = if include_caret {
            self.caret.and_then(|c| c.checked_sub(1))
        } else {
            self.caret
        };
        if let Some(idx) = self.src.find_prefix(base, prefix) {
            self.caret = Some(idx);
            self.src.select(idx, SelectOp::Single);
            self.scroll_into_view(idx);
        }
        inv.push(self.bounds); // 매치 없어도 HUD(버퍼) 갱신
    }

    /// 소스 가변 접근 — 재로드 상태 복원(펼침·선택) 등 호스트 주도 변형용(M3-6 선행).
    pub fn source_mut(&mut self) -> &mut S {
        &mut self.src
    }

    /// 재로드 후 뷰 상태 복원(M3-6 무간섭 갱신 선행) — 캐럿·스크롤(범위 밖은 clamp).
    /// 선택·펼침은 소스([`Self::source_mut`])가 복원한다.
    pub fn restore_view(
        &mut self,
        caret: Option<usize>,
        scroll_row: usize,
        scroll_x: i32,
        inv: &mut Invalidations,
    ) {
        self.caret = caret.filter(|&c| c < self.src.len());
        self.scroll_row = scroll_row;
        self.scroll_x = scroll_x;
        self.clamp_scroll();
        inv.push(self.bounds);
    }

    /// 데이터 공급자 교체(네비게이션 — M1-8). 스크롤·캐럿·타입어헤드는 리셋,
    /// 컬럼·정렬 상태는 유지하고 새 소스에 재적용(원본 PanelView.SortKeys 지속 규약).
    pub fn replace_source(&mut self, src: S, inv: &mut Invalidations) {
        self.src = src;
        self.scroll_row = 0;
        self.scroll_x = 0;
        self.caret = None;
        self.band = None;
        self.typeahead.clear();
        let keys = self.sort.clone();
        self.src.set_sort(&keys);
        self.clamp_scroll();
        inv.push(self.bounds);
    }

    /// 클라이언트 좌표 → 본문 행 인덱스(범위 밖이면 `None`). 호스트의 더블클릭 진입 판정에도 사용.
    pub fn row_at(&self, x: i32, y: i32) -> Option<usize> {
        if !self.bounds.contains(Point { x, y }) || y < self.body_top() {
            return None;
        }
        let row = self.scroll_row + ((y - self.body_top()) / self.row_h) as usize;
        (row < self.src.len()).then_some(row)
    }

    /// 행의 클라이언트 앵커 좌표(가시 범위 내일 때만) — 키보드 컨텍스트 메뉴 위치(M3-4 Apps 키).
    pub fn row_anchor(&self, row: usize) -> Option<Point> {
        if row < self.scroll_row || row >= self.src.len() {
            return None;
        }
        let y = self.body_top() + ((row - self.scroll_row) as i32) * self.row_h;
        (y + self.row_h <= self.bounds.bottom()).then_some(Point {
            x: self.bounds.x + self.pad_x,
            y: y + self.row_h / 2,
        })
    }

    /// 좌표가 본문(헤더 아래) 영역인가 — 호스트의 빈 영역 판정(M3-4 배경 셸 메뉴).
    pub fn in_body(&self, x: i32, y: i32) -> bool {
        self.bounds.contains(Point { x, y }) && y >= self.body_top()
    }

    /// 좌표가 펼침 마커 위인가 — 호스트가 더블클릭 진입과 마커 토글을 구분할 때 사용.
    pub fn marker_hit(&self, x: i32, y: i32) -> bool {
        self.row_at(x, y)
            .is_some_and(|row| self.in_marker_zone(row, x))
    }

    /// 클릭 x가 해당 행의 펼침 마커 영역인가(트리 컬럼 안 들여쓰기 자리·마커 있는 행만).
    fn in_marker_zone(&self, row: usize, x: i32) -> bool {
        let (tc_x, tc_w) = if self.columns.is_empty() {
            (self.bounds.x, self.bounds.w)
        } else {
            match self.columns.iter().position(|c| c.key == 0) {
                Some(i) => (self.col_x(i), self.columns[i].width),
                None => return false,
            }
        };
        let item = self.src.row(row);
        if item.marker == Marker::None {
            return false;
        }
        let indent = tc_x + self.pad_x + item.depth as i32 * self.indent_w;
        x >= indent && x < (indent + self.indent_w).min(tc_x + tc_w)
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

    /// 트리 컬럼(마커+들여쓰기+아이콘+이름)을 `cell` 안에 그린다.
    fn paint_tree_cell(
        &self,
        ctx: &mut dyn DrawCtx,
        theme: &Theme,
        item: &RowItem,
        icon: Option<&(String, String)>,
        cell: Rect,
        bg: crate::theme::Color,
    ) {
        // 텍스트 세로 위치: 행 높이의 4/5를 글자 높이로 보고 중앙 정렬(M0-7 계승)
        let ty = cell.y + (cell.h - (cell.h * 4) / 5) / 2;
        let indent = cell.x + self.pad_x + item.depth as i32 * self.indent_w;
        ctx.text_opaque(indent, ty, cell, item.marker.glyph(), theme.text_dim, bg);
        let mut name_x = indent + self.indent_w;
        if let Some((key, hint)) = icon {
            // 아이콘 크기 = 들여쓰기 폭(16px@96dpi) — 셸 스몰 아이콘 규격
            let isz = self.indent_w;
            let iy = cell.y + (cell.h - isz) / 2;
            ctx.draw_icon(name_x, iy, isz, key, hint);
            name_x += isz + self.pad_x / 2;
        }
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
        // 인라인 이름변경 중: 문자는 버퍼로, 키 네비는 차단, 필드 안 클릭=캐럿 배치·
        // 밖 클릭=취소 후 정상 처리(M3-2·QA 07-13)
        if self.is_renaming() {
            match *ev {
                InputEvent::Char { c, .. } => {
                    self.rename_char(c, inv);
                    return;
                }
                InputEvent::Key { .. } => return, // Enter/Esc·편집 키는 호스트가 라우팅
                InputEvent::MouseDown { x, y, .. } => {
                    if let Some((_, es)) = &mut self.rename {
                        if es.hit(x, y) {
                            es.click(x);
                            inv.push(self.bounds);
                            return;
                        }
                    }
                    self.cancel_rename(inv);
                }
                _ => {}
            }
        }
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
            InputEvent::Key { key, shift, ctrl } => {
                let len = self.src.len();
                if len == 0 {
                    return;
                }
                let caret = self.caret.unwrap_or(self.scroll_row).min(len - 1);
                match key {
                    // 캐럿 이동(탐색기 규약: 평이동=단일 선택·Shift=범위·Ctrl=캐럿만)
                    Key::Up | Key::Down | Key::PageUp | Key::PageDown | Key::Home | Key::End => {
                        let cur = caret as isize;
                        let target = match key {
                            Key::Up => cur - 1,
                            Key::Down => cur + 1,
                            Key::PageUp => cur - page,
                            Key::PageDown => cur + page,
                            Key::Home => 0,
                            _ => len as isize - 1, // End
                        }
                        .clamp(0, len as isize - 1) as usize;
                        self.move_caret(target, shift, ctrl, inv);
                    }
                    // → = 펼침, 이미 펼침이면 첫 자식으로(docs/07 §8)
                    Key::Right => {
                        let item = self.src.row(caret);
                        match item.marker {
                            Marker::Collapsed => {
                                if self.src.toggle(caret) {
                                    self.caret = Some(caret);
                                    self.clamp_scroll();
                                    inv.push(self.bounds);
                                }
                            }
                            Marker::Expanded => {
                                if caret + 1 < len && self.src.row(caret + 1).depth > item.depth {
                                    self.move_caret(caret + 1, shift, ctrl, inv);
                                }
                            }
                            Marker::None => {}
                        }
                    }
                    // ← = 접힘, 접힘/파일이면 부모로(docs/07 §8)
                    Key::Left => {
                        if self.src.row(caret).marker == Marker::Expanded {
                            if self.src.toggle(caret) {
                                self.caret = Some(caret);
                                self.clamp_scroll();
                                inv.push(self.bounds);
                            }
                        } else if let Some(parent) = self.parent_row(caret) {
                            self.move_caret(parent, shift, ctrl, inv);
                        }
                    }
                    // Space/Ctrl+Space = 캐럿 행 선택 토글(docs/32 §7 결정 1)
                    Key::Space => {
                        self.caret = Some(caret);
                        self.src.select(caret, SelectOp::Toggle);
                        inv.push(self.bounds);
                    }
                }
            }
            InputEvent::Char { c, now_ms } => {
                if c == '\u{8}' {
                    // Backspace — 접두사 축소, 비면 HUD 소거
                    match self.typeahead.backspace(now_ms) {
                        Some(q) => self.typeahead_find(&q.prefix, q.include_caret, inv),
                        None => inv.push(self.bounds),
                    }
                } else if !c.is_control() && c != ' ' {
                    let q = self.typeahead.push(c, now_ms);
                    self.typeahead_find(&q.prefix, q.include_caret, inv);
                }
            }
            InputEvent::SelectAll => {
                if self.src.select_all() {
                    inv.push(self.bounds);
                }
            }
            InputEvent::MouseDown { x, y, shift, ctrl } => {
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
                    if self.in_marker_zone(row, x) {
                        // 삼각형 = 인라인 펼침/접힘(docs/07 §1-1) — 선택과 분리
                        if self.src.toggle(row) {
                            self.clamp_scroll();
                            inv.push(self.bounds);
                        }
                    } else {
                        let op = if shift {
                            SelectOp::RangeTo
                        } else if ctrl {
                            SelectOp::Toggle
                        } else {
                            SelectOp::Single
                        };
                        self.src.select(row, op);
                        self.caret = Some(row);
                        inv.push(self.bounds); // 하이라이트·캐럿 갱신
                    }
                } else if y >= self.body_top() && self.bounds.contains(Point { x, y }) {
                    // 빈 본문 영역 — 러버밴드 시작(기존 선택 해제)
                    self.band = Some(BandDrag {
                        ox: x,
                        oy: y,
                        cx: x,
                        cy: y,
                    });
                    self.src.clear_selection();
                    inv.push(self.bounds);
                }
            }
            InputEvent::MouseMove { x, y } => {
                if let Some(drag) = self.resize {
                    let w =
                        (drag.start_w + (x - drag.start_x)).max(self.columns[drag.col].min_width);
                    if w != self.columns[drag.col].width {
                        self.columns[drag.col].width = w;
                        self.clamp_scroll_x();
                        inv.push(self.bounds);
                    }
                } else if let Some(mut band) = self.band {
                    band.cx = x;
                    band.cy = y;
                    self.band = Some(band);
                    // 밴드 세로 범위와 교차하는 가시 행 범위로 선택 대체
                    let r = band.rect();
                    let top = r.y.max(self.body_top());
                    let bot = r.bottom().min(self.bounds.bottom());
                    if bot > top && !self.src.is_empty() {
                        let lo = self.scroll_row + ((top - self.body_top()) / self.row_h) as usize;
                        let hi = (self.scroll_row
                            + ((bot - 1 - self.body_top()) / self.row_h) as usize)
                            .min(self.src.len() - 1);
                        if lo <= hi && lo < self.src.len() {
                            self.src.select_span(lo, hi);
                        } else {
                            self.src.clear_selection();
                        }
                    } else {
                        self.src.clear_selection();
                    }
                    inv.push(self.bounds);
                }
            }
            InputEvent::MouseUp { .. } => {
                self.resize = None;
                if self.band.take().is_some() {
                    inv.push(self.bounds); // 밴드 사각형 지우기
                }
            }
            InputEvent::RightDown { x, y } => {
                // 선택 규약만 처리(탐색기: 미선택 행=단독 선택·선택 행=유지) — 메뉴 표시는 호스트(M3-4).
                if let Some(row) = self.row_at(x, y) {
                    if !self.in_marker_zone(row, x) {
                        if !self.src.is_selected(row) {
                            self.src.select(row, SelectOp::Single);
                        }
                        self.caret = Some(row);
                        inv.push(self.bounds);
                    }
                } else if y >= self.body_top() && self.bounds.contains(Point { x, y }) {
                    // 빈 본문 영역 = 선택 해제(배경 셸 메뉴는 S3)
                    self.src.clear_selection();
                    inv.push(self.bounds);
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
        let body_top = self.body_top();

        // ── 본문 행 ──
        for i in 0..count {
            let row = first + i;
            let y = body_top + i as i32 * self.row_h;
            // 선택 하이라이트 > 교대 음영(docs/07 §7 다중·비연속·교차폴더 하이라이트)
            let bg = if self.src.is_selected(row) {
                theme.sel_bg
            } else if row.is_multiple_of(2) {
                theme.panel_bg
            } else {
                theme.panel_bg_alt
            };
            // 텍스트 세로 위치: 행 높이의 4/5를 글자 높이로 보고 중앙 정렬(M0-7 계승)
            let ty = y + (self.row_h - (self.row_h * 4) / 5) / 2;

            if self.columns.is_empty() {
                // M1-3 호환: 단일 트리 컬럼이 전체 폭
                let item = self.src.row(row);
                let icon = self.src.icon(row);
                let rc = Rect::new(b.x, y, b.w, self.row_h);
                self.paint_tree_cell(ctx, theme, &item, icon.as_ref(), rc, bg);
            } else {
                for (ci, col) in self.columns.iter().enumerate() {
                    let cx = self.col_x(ci);
                    if cx >= b.right() || cx + col.width <= b.x {
                        continue; // 가로 스크롤로 화면 밖
                    }
                    let cell = Rect::new(cx, y, col.width, self.row_h);
                    if col.key == 0 {
                        let item = self.src.row(row);
                        let icon = self.src.icon(row);
                        self.paint_tree_cell(ctx, theme, &item, icon.as_ref(), cell, bg);
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

            // 캐럿 행 테두리(1px accent) — 선택과 독립(키보드 기준점 표시)
            if self.caret == Some(row) {
                ctx.fill_rect(Rect::new(b.x, y, b.w, 1), theme.accent);
                ctx.fill_rect(Rect::new(b.x, y + self.row_h - 1, b.w, 1), theme.accent);
                ctx.fill_rect(Rect::new(b.x, y, 1, self.row_h), theme.accent);
                ctx.fill_rect(Rect::new(b.right() - 1, y, 1, self.row_h), theme.accent);
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

        // ── 러버밴드 외곽선(드래그 중) ──
        if let Some(band) = self.band {
            let r = band.rect();
            if r.w > 0 && r.h > 0 {
                ctx.fill_rect(Rect::new(r.x, r.y, r.w, 1), theme.accent);
                ctx.fill_rect(Rect::new(r.x, r.bottom() - 1, r.w, 1), theme.accent);
                ctx.fill_rect(Rect::new(r.x, r.y, 1, r.h), theme.accent);
                ctx.fill_rect(Rect::new(r.right() - 1, r.y, 1, r.h), theme.accent);
            }
        }

        // ── 인라인 이름변경 필드(M3-2) — 트리 컬럼 이름부 위 오버레이,
        //    공용 필드 페인트(선택 하이라이트·세로바 캐럿·캐럿 가시 정렬) ──
        if let Some((row, es)) = &self.rename {
            if *row >= first && *row < first + count {
                let y = body_top + (*row - first) as i32 * self.row_h;
                let (tc_x, tc_w) = if self.columns.is_empty() {
                    (b.x, b.w)
                } else {
                    (self.col_x(0), self.columns[0].width)
                };
                let item = self.src.row(*row);
                let mut fx = tc_x + self.pad_x + item.depth as i32 * self.indent_w + self.indent_w;
                if self.src.icon(*row).is_some() {
                    fx += self.indent_w + self.pad_x / 2;
                }
                let fw = (tc_x + tc_w - fx).max(self.indent_w * 3);
                let rc = Rect::new(fx, y, fw, self.row_h);
                es.paint_field(ctx, rc, 2, theme);
            }
        }

        // ── 타입어헤드 HUD(본문 좌하단 플로팅 배지 — 원본 docs/32 §7-A) ──
        if !self.typeahead.text().is_empty() {
            let label = format!("찾기: {}", self.typeahead.text());
            let tw = ctx.text_width(&label);
            let hud = Rect::new(
                b.x + self.pad_x,
                b.bottom() - self.row_h - self.pad_x,
                tw + self.pad_x * 2,
                self.row_h,
            );
            let hty = hud.y + (self.row_h - (self.row_h * 4) / 5) / 2;
            ctx.text_opaque(
                hud.x + self.pad_x,
                hty,
                hud,
                &label,
                theme.text,
                theme.header_bg,
            );
            ctx.fill_rect(Rect::new(hud.x, hud.y, hud.w, 1), theme.accent);
            ctx.fill_rect(Rect::new(hud.x, hud.bottom() - 1, hud.w, 1), theme.accent);
            ctx.fill_rect(Rect::new(hud.x, hud.y, 1, hud.h), theme.accent);
            ctx.fill_rect(Rect::new(hud.right() - 1, hud.y, 1, hud.h), theme.accent);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Color;
    use std::cell::RefCell;

    #[test]
    fn inline_rename_flow_and_key_block() {
        let mut v = VirtualRows::new(
            Rows {
                n: 5,
                sorts: RefCell::new(Vec::new()),
            },
            20,
            6,
            16,
        );
        let mut inv = Invalidations::default();
        v.set_bounds(Rect::new(0, 0, 300, 200), &mut inv);
        v.begin_rename(2, "행 2", &mut inv);
        assert_eq!(v.rename_state(), Some((2, "행 2".to_string())));
        assert_eq!(v.caret(), Some(2), "편집 행으로 캐럿 이동");
        // 시작 = 선택 상태(확장자 없음 → 전체) — End로 접어 끝 편집으로 전환
        v.rename_key(crate::edit::EditKey::End, false, &mut inv);
        // 문자 입력·Backspace — Char 이벤트 경유(타입어헤드 대신 버퍼로)
        v.on_event(
            &InputEvent::Char {
                c: '\u{8}',
                now_ms: 0,
            },
            &mut inv,
        );
        v.on_event(&InputEvent::Char { c: '3', now_ms: 0 }, &mut inv);
        assert_eq!(v.rename_state(), Some((2, "행 3".to_string())));
        assert_eq!(v.typeahead_text(), "", "편집 중 타입어헤드 미동작");
        // 키 네비 차단
        v.on_event(
            &InputEvent::Key {
                key: Key::Down,
                shift: false,
                ctrl: false,
            },
            &mut inv,
        );
        assert_eq!(v.caret(), Some(2), "편집 중 캐럿 이동 차단");
        // 제출 — (행, 새 이름), 편집 종료
        assert_eq!(v.submit_rename(&mut inv), Some((2, "행 3".to_string())));
        assert!(!v.is_renaming());
        // 취소 경로
        v.begin_rename(1, "x", &mut inv);
        v.cancel_rename(&mut inv);
        assert!(!v.is_renaming());
        assert_eq!(v.submit_rename(&mut inv), None);
    }

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
        v.on_event(
            &InputEvent::MouseDown {
                x,
                y,
                shift,
                ctrl: false,
            },
            inv,
        );
    }

    fn key(k: Key) -> InputEvent {
        InputEvent::Key {
            key: k,
            shift: false,
            ctrl: false,
        }
    }

    // ── M1-3 계승(컬럼 없음 = 헤더 없음) ──

    #[test]
    fn scroll_clamps_to_total_minus_full_rows() {
        let (mut v, mut inv) = list(100, 200); // 완전 가시 10행
        v.on_event(&key(Key::End), &mut inv);
        assert_eq!(v.scroll_row(), 90); // 캐럿 99가 보이도록 스크롤 추적
        assert_eq!(v.caret(), Some(99));
        v.on_event(&key(Key::Home), &mut inv);
        assert_eq!(v.scroll_row(), 0);
        assert_eq!(v.caret(), Some(0));
    }

    #[test]
    fn marker_click_toggles_row_without_columns() {
        let mut inv = Invalidations::default();
        let mut v = VirtualRows::new(Expandable { expanded: false }, 20, 12, 16);
        v.set_bounds(Rect::new(0, 0, 400, 200), &mut inv);
        inv.drain().for_each(drop);
        // 헤더 없음 → y=5는 0행. 마커 존 = [12, 28)
        v.on_event(
            &InputEvent::MouseDown {
                x: 15,
                y: 5,
                shift: false,
                ctrl: false,
            },
            &mut inv,
        );
        assert_eq!(v.source().len(), 6);
        // 마커 존 밖 클릭은 펼침이 아니라 선택(캐럿만 이동)
        v.on_event(
            &InputEvent::MouseDown {
                x: 100,
                y: 5,
                shift: false,
                ctrl: false,
            },
            &mut inv,
        );
        assert_eq!(v.source().len(), 6, "본문 클릭은 토글 아님");
        assert_eq!(v.caret(), Some(0));
    }

    // ── 헤더·본문 오프셋 ──

    #[test]
    fn header_shifts_body_rows_down() {
        let (mut v, mut inv) = list_with_cols(100, 220); // 헤더 20 + 본문 200 = 완전 가시 10행
        assert_eq!(v.max_scroll(), 90);
        // y=5 → 헤더(정렬 클릭), y=25 → 0행
        assert_eq!(v.row_at(10, 5), None);
        assert_eq!(v.row_at(10, 25), Some(0));
        v.on_event(&key(Key::End), &mut inv);
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

    // ── 선택(원본 docs/07 §1-2·§8): 단일·Ctrl 토글·Shift 범위·Ctrl+A·러버밴드 ──

    struct SelRows {
        n: usize,
        sel: std::collections::HashSet<usize>,
        anchor: usize,
        names: Vec<String>,
    }
    impl SelRows {
        fn new(n: usize) -> SelRows {
            SelRows {
                n,
                sel: Default::default(),
                anchor: 0,
                names: Vec::new(),
            }
        }
    }
    impl SelRows {
        fn named(names: &[&str]) -> SelRows {
            let mut s = SelRows::new(names.len());
            s.names = names.iter().map(|n| n.to_string()).collect();
            s
        }
        fn name(&self, i: usize) -> String {
            self.names
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("row-{i}"))
        }
    }
    impl RowSource for SelRows {
        fn len(&self) -> usize {
            self.n
        }
        fn row(&self, index: usize) -> RowItem {
            RowItem {
                text: self.name(index),
                depth: 0,
                marker: Marker::None,
            }
        }
        fn find_prefix(&self, caret: Option<usize>, prefix: &str) -> Option<usize> {
            // 코어 find_prefix(VisibleStream) 축약 모사: caret+1부터 + wrap, 대소문자 무시
            if prefix.is_empty() || self.n == 0 {
                return None;
            }
            let lower = prefix.to_lowercase();
            let start = caret.filter(|&c| c < self.n).map_or(0, |c| c + 1);
            (start..self.n)
                .chain(0..start)
                .find(|&i| self.name(i).to_lowercase().starts_with(&lower))
        }
        fn is_selected(&self, index: usize) -> bool {
            self.sel.contains(&index)
        }
        fn select(&mut self, index: usize, op: SelectOp) -> bool {
            match op {
                SelectOp::Single => {
                    self.sel.clear();
                    self.sel.insert(index);
                    self.anchor = index;
                }
                SelectOp::Toggle => {
                    if !self.sel.remove(&index) {
                        self.sel.insert(index);
                    }
                    self.anchor = index;
                }
                SelectOp::RangeTo => {
                    let (lo, hi) = if self.anchor <= index {
                        (self.anchor, index)
                    } else {
                        (index, self.anchor)
                    };
                    self.sel = (lo..=hi).collect();
                }
            }
            true
        }
        fn select_span(&mut self, lo: usize, hi: usize) -> bool {
            self.sel = (lo..=hi).collect();
            true
        }
        fn select_all(&mut self) -> bool {
            self.sel = (0..self.n).collect();
            true
        }
        fn clear_selection(&mut self) -> bool {
            let had = !self.sel.is_empty();
            self.sel.clear();
            had
        }
    }

    fn sel_list(n: usize, h: i32) -> (VirtualRows<SelRows>, Invalidations) {
        let mut inv = Invalidations::default();
        let mut v = VirtualRows::new(SelRows::new(n), 20, 12, 16);
        v.set_bounds(Rect::new(0, 0, 400, h), &mut inv);
        inv.drain().for_each(drop);
        (v, inv)
    }

    fn sdown(
        v: &mut VirtualRows<SelRows>,
        inv: &mut Invalidations,
        y: i32,
        shift: bool,
        ctrl: bool,
    ) {
        v.on_event(
            &InputEvent::MouseDown {
                x: 100,
                y,
                shift,
                ctrl,
            },
            inv,
        );
    }

    #[test]
    fn click_selects_single_ctrl_toggles_shift_ranges() {
        let (mut v, mut inv) = sel_list(10, 200); // 헤더 없음 — 행 y = i*20
        sdown(&mut v, &mut inv, 5, false, false); // 0행 단일
        assert!(v.source().is_selected(0) && v.source().sel.len() == 1);
        assert_eq!(v.caret(), Some(0));
        sdown(&mut v, &mut inv, 45, false, true); // Ctrl+2행 토글 → {0,2} (비연속)
        assert_eq!(v.source().sel.len(), 2);
        assert!(v.source().is_selected(2));
        sdown(&mut v, &mut inv, 85, true, false); // Shift+4행 → anchor(2)~4 범위
        assert_eq!(
            v.source().sel,
            [2usize, 3, 4].into_iter().collect(),
            "가시 순서 범위 선택"
        );
        assert_eq!(v.caret(), Some(4));
        sdown(&mut v, &mut inv, 45, false, true); // Ctrl 토글 해제
        assert!(!v.source().is_selected(2));
    }

    #[test]
    fn right_down_selects_unselected_keeps_selection_clears_on_empty() {
        let (mut v, mut inv) = sel_list(5, 200);
        // 미선택 행 우클릭 = 단독 선택 + 캐럿(탐색기 규약, M3-4)
        v.on_event(&InputEvent::RightDown { x: 30, y: 45 }, &mut inv); // 2행
        assert!(v.source().is_selected(2) && v.source().sel.len() == 1);
        assert_eq!(v.caret(), Some(2));
        // 기존 다중 선택 위 우클릭 = 선택 유지(축소 안 함)
        sdown(&mut v, &mut inv, 5, false, false); // 0행 단일
        sdown(&mut v, &mut inv, 45, true, false); // Shift+2행 → {0,1,2}
        v.on_event(&InputEvent::RightDown { x: 30, y: 25 }, &mut inv); // 선택된 1행
        assert_eq!(v.source().sel.len(), 3, "선택 유지");
        assert_eq!(v.caret(), Some(1));
        // 빈 본문 영역 우클릭 = 선택 해제(배경 메뉴는 S3)
        v.on_event(&InputEvent::RightDown { x: 30, y: 150 }, &mut inv);
        assert!(v.source().sel.is_empty());
    }

    #[test]
    fn ctrl_a_selects_all_visible() {
        let (mut v, mut inv) = sel_list(7, 200);
        v.on_event(&InputEvent::SelectAll, &mut inv);
        assert_eq!(v.source().sel.len(), 7);
        assert!(!inv.is_empty());
    }

    #[test]
    fn rubber_band_selects_intersecting_rows_and_ends_on_up() {
        let (mut v, mut inv) = sel_list(3, 200); // 행 3개(0..60), 아래 빈 영역
        sdown(&mut v, &mut inv, 5, false, false); // 미리 선택해 둔 0행이
        sdown(&mut v, &mut inv, 100, false, false); // 빈 영역 클릭 → 밴드 시작 + 해제
        assert!(v.source().sel.is_empty());
        v.on_event(&InputEvent::MouseMove { x: 50, y: 30 }, &mut inv); // 위로 드래그: 30..100
        assert_eq!(
            v.source().sel,
            [1usize, 2].into_iter().collect(),
            "밴드 세로 범위와 교차하는 행"
        );
        v.on_event(&InputEvent::MouseUp { x: 50, y: 30 }, &mut inv);
        assert_eq!(v.source().sel.len(), 2, "업 후 선택 유지");
        // 업 이후 이동은 밴드 아님
        v.on_event(&InputEvent::MouseMove { x: 50, y: 5 }, &mut inv);
        assert_eq!(v.source().sel.len(), 2);
    }

    #[test]
    fn right_left_keys_toggle_expansion_at_caret() {
        let mut inv = Invalidations::default();
        let mut v = VirtualRows::new(Expandable { expanded: false }, 20, 12, 16);
        v.set_bounds(Rect::new(0, 0, 400, 200), &mut inv);
        v.on_event(
            &InputEvent::MouseDown {
                x: 100,
                y: 5,
                shift: false,
                ctrl: false,
            },
            &mut inv,
        ); // 본문 클릭 → 캐럿 0
        assert_eq!(v.caret(), Some(0));
        v.on_event(&key(Key::Right), &mut inv);
        assert_eq!(v.source().len(), 6, "→ = 인라인 펼침");
        v.on_event(&key(Key::Right), &mut inv);
        assert_eq!(v.caret(), Some(1), "펼침 상태에서 → = 첫 자식으로");
        v.on_event(&key(Key::Left), &mut inv);
        assert_eq!(v.caret(), Some(0), "자식(파일)에서 ← = 부모로");
        v.on_event(&key(Key::Left), &mut inv);
        assert_eq!(v.source().len(), 1, "펼친 부모에서 ← = 접힘");
    }

    // ── 캐럿 키보드 네비(M1-6, 탐색기 규약) ──

    #[test]
    fn caret_moves_select_and_scroll_follows() {
        let (mut v, mut inv) = sel_list(100, 200); // 완전 가시 10행
        v.on_event(&key(Key::Down), &mut inv); // 캐럿 없음 → scroll_row(0) 기준 +1
        assert_eq!(v.caret(), Some(1));
        assert!(v.source().is_selected(1) && v.source().sel.len() == 1);
        v.on_event(&key(Key::PageDown), &mut inv);
        assert_eq!(v.caret(), Some(11));
        assert_eq!(v.scroll_row(), 2, "캐럿이 보이도록 스크롤 추적");
        v.on_event(&key(Key::Home), &mut inv);
        assert_eq!((v.caret(), v.scroll_row()), (Some(0), 0));
    }

    #[test]
    fn shift_moves_range_and_ctrl_moves_caret_only() {
        let (mut v, mut inv) = sel_list(20, 200);
        sdown(&mut v, &mut inv, 25, false, false); // 1행 클릭(anchor)
        v.on_event(
            &InputEvent::Key {
                key: Key::Down,
                shift: true,
                ctrl: false,
            },
            &mut inv,
        );
        v.on_event(
            &InputEvent::Key {
                key: Key::Down,
                shift: true,
                ctrl: false,
            },
            &mut inv,
        );
        assert_eq!(v.source().sel, [1usize, 2, 3].into_iter().collect());
        v.on_event(
            &InputEvent::Key {
                key: Key::Down,
                shift: false,
                ctrl: true,
            },
            &mut inv,
        ); // Ctrl+↓ = 캐럿만
        assert_eq!(v.caret(), Some(4));
        assert_eq!(v.source().sel.len(), 3, "Ctrl 이동은 선택 불변");
        v.on_event(
            &InputEvent::Key {
                key: Key::Space,
                shift: false,
                ctrl: true,
            },
            &mut inv,
        ); // Ctrl+Space = 토글
        assert_eq!(v.source().sel.len(), 4);
        assert!(v.source().is_selected(4));
    }

    // ── 타입어헤드(M1-6, 원본 docs/32 §6) ──

    #[test]
    fn typeahead_jumps_cycles_and_accumulates() {
        let mut inv = Invalidations::default();
        let mut v = VirtualRows::new(
            SelRows::named(&["apple", "apricot", "banana", "aardvark"]),
            20,
            12,
            16,
        );
        v.set_bounds(Rect::new(0, 0, 400, 200), &mut inv);
        v.on_event(&InputEvent::Char { c: 'a', now_ms: 0 }, &mut inv);
        assert_eq!(v.caret(), Some(0), "첫 'a' = apple");
        assert!(v.source().is_selected(0));
        v.on_event(
            &InputEvent::Char {
                c: 'a',
                now_ms: 300,
            },
            &mut inv,
        );
        assert_eq!(v.caret(), Some(1), "반복 'a' = 다음 매치 cycle(apricot)");
        v.on_event(
            &InputEvent::Char {
                c: 'a',
                now_ms: 600,
            },
            &mut inv,
        );
        assert_eq!(v.caret(), Some(3), "banana 건너뛰고 aardvark");
        v.on_event(
            &InputEvent::Char {
                c: 'p',
                now_ms: 900,
            },
            &mut inv,
        );
        assert_eq!(v.typeahead_text(), "ap");
        assert_eq!(v.caret(), Some(0), "누적 'ap' = wrap 후 apple");
        // Backspace → "a", 현재 행 포함 재평가 → apple 유지
        v.on_event(
            &InputEvent::Char {
                c: '\u{8}',
                now_ms: 1100,
            },
            &mut inv,
        );
        assert_eq!(v.typeahead_text(), "a");
        assert_eq!(v.caret(), Some(0));
    }

    #[test]
    fn typeahead_times_out_via_tick_and_space_is_excluded() {
        let mut inv = Invalidations::default();
        let mut v = VirtualRows::new(SelRows::named(&["alpha", "beta"]), 20, 12, 16);
        v.set_bounds(Rect::new(0, 0, 400, 200), &mut inv);
        v.on_event(&InputEvent::Char { c: 'b', now_ms: 0 }, &mut inv);
        assert_eq!(v.typeahead_text(), "b");
        v.tick(500, &mut inv);
        assert_eq!(v.typeahead_text(), "b", "타임아웃 전 유지");
        v.tick(1200, &mut inv);
        assert_eq!(v.typeahead_text(), "", "1000ms 경과 → 버퍼 소거");
        v.on_event(
            &InputEvent::Char {
                c: ' ',
                now_ms: 1300,
            },
            &mut inv,
        );
        assert_eq!(
            v.typeahead_text(),
            "",
            "Space는 타입어헤드 제외(docs/32 §7)"
        );
    }

    // ── 소스 교체(M1-8 네비게이션) ──

    #[test]
    fn replace_source_resets_view_but_keeps_sort() {
        let (mut v, mut inv) = list_with_cols(100, 220);
        down(&mut v, &mut inv, 50, 5, false); // 이름 ▲ 정렬
        v.on_event(&key(Key::End), &mut inv); // 스크롤·캐럿 이동
        assert!(v.scroll_row() > 0 && v.caret().is_some());

        v.replace_source(Rows::new(5), &mut inv); // "다른 폴더 진입"
        assert_eq!((v.scroll_row(), v.caret()), (0, None), "뷰 상태 리셋");
        assert_eq!(v.sort(), &[(0, false)], "정렬 상태 유지");
        assert_eq!(
            *v.source().sorts.borrow(),
            vec![vec![(0, false)]],
            "새 소스에 정렬 재적용"
        );
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
