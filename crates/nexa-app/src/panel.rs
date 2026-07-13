//! 패널 — 탭 바 + 경로 바 + 파일 리스트 묶음(원본 PanelView·docs/20 §2).
//! 탭 = 독립 뷰 상태 + **탭별 독립 히스토리**(docs/20 §3). 플랫폼 중립 — 전 플랫폼 테스트.
//! 창/렌더 배선(win.rs)은 패널 2개를 스플리터로 배치하고 활성 패널에 키보드를 라우팅한다.

use std::path::{Path, PathBuf};

use nexa_core::FileKind;
use nexa_gui::widgets::{PathBar, TabAction, TabBar, ToolButton, Toolbar, VirtualRows};
use nexa_gui::{Column, DrawCtx, InputEvent, Invalidations, Rect, Theme, Widget};
use nexa_tree::Tree;

use crate::nav::History;
use crate::source::TreeSource;

/// 네비게이션 컨텍스트 — 가시성 필터(전역 ViewOptions)·타임존(호스트 소유).
#[derive(Clone, Copy, Debug)]
pub struct NavCtx {
    pub show_hidden: bool,
    pub show_dotfiles: bool,
    pub tz: i32,
}

/// 탭 = 리스트 뷰 상태 + 독립 back/forward 히스토리.
pub struct Tab {
    pub rows: VirtualRows<TreeSource>,
    pub nav: History,
}

impl Tab {
    fn title(&self) -> String {
        let p = self.rows.source().tree().root_path();
        match p.file_name() {
            Some(n) => n.to_string_lossy().into_owned(),
            None => p.to_string_lossy().into_owned(), // 드라이브 루트 등
        }
    }
}

/// 패널 지표(DPI 스케일 — 호스트가 계산).
#[derive(Clone, Copy, Debug)]
pub struct PanelMetrics {
    pub row_h: i32,
    pub pad_x: i32,
    pub indent_w: i32,
    /// 탭 바·경로 바 높이.
    pub tab_h: i32,
    pub bar_h: i32,
}

/// 패널 네비 버튼 id(원본 docs/20 §2 네비게이션 바 — 탭 하단 [←][→][↑]).
const BTN_BACK: u32 = 1;
const BTN_FORWARD: u32 = 2;
const BTN_UP: u32 = 3;

pub struct Panel {
    pub tabbar: TabBar,
    /// 경로바 왼쪽의 [←][→][↑] — **이 패널의 활성 탭** 히스토리에 동작(사용자 지시 2026-07-12).
    navbtns: Toolbar,
    pub pathbar: PathBar,
    tabs: Vec<Tab>,
    active: usize,
    bounds: Rect,
    m: PanelMetrics,
}

impl Panel {
    /// 첫 탭을 `tree`로 시작.
    pub fn new(tree: Tree, ctx: NavCtx, m: PanelMetrics, columns: Vec<Column>) -> Panel {
        let root = tree.root_path().to_path_buf();
        let mut inv = Invalidations::default();
        let mut rows =
            VirtualRows::new(TreeSource::new(tree, ctx.tz), m.row_h, m.pad_x, m.indent_w);
        rows.set_columns(columns, &mut inv);
        let mut p = Panel {
            tabbar: TabBar::new(m.row_h, m.pad_x),
            navbtns: Toolbar::new(nav_buttons(), m.row_h, m.pad_x).with_button_width(nav_btn_w(&m)),
            pathbar: PathBar::new(root.to_string_lossy(), m.row_h, m.pad_x),
            tabs: vec![Tab {
                rows,
                nav: History::new(root),
            }],
            active: 0,
            bounds: Rect::default(),
            m,
        };
        p.sync_chrome(&mut inv);
        p
    }

    /// 세션 복원 — 경로 목록으로 탭들을 연다(열기 실패 탭은 건너뜀·전부 실패면 `fallback`).
    pub fn restore(
        paths: &[PathBuf],
        active: usize,
        fallback: &Path,
        ctx: NavCtx,
        m: PanelMetrics,
        columns: Vec<Column>,
    ) -> Panel {
        let mut valid: Vec<Tree> = paths
            .iter()
            .filter_map(|p| Tree::open_filtered(p, ctx.show_hidden, ctx.show_dotfiles).ok())
            .collect();
        if valid.is_empty() {
            valid.push(
                Tree::open_filtered(fallback, ctx.show_hidden, ctx.show_dotfiles)
                    .or_else(|_| Tree::open("C:\\"))
                    .expect("C:\\ 열기 실패"),
            );
        }
        let first = valid.remove(0);
        let mut p = Panel::new(first, ctx, m, columns.clone());
        let mut inv = Invalidations::default();
        for tree in valid {
            let root = tree.root_path().to_path_buf();
            let mut rows =
                VirtualRows::new(TreeSource::new(tree, ctx.tz), m.row_h, m.pad_x, m.indent_w);
            rows.set_columns(columns.clone(), &mut inv);
            p.tabs.push(Tab {
                rows,
                nav: History::new(root),
            });
        }
        p.active = active.min(p.tabs.len() - 1);
        p.sync_chrome(&mut inv);
        p
    }

    /// 세션 스냅샷 — (탭 경로들, 활성 탭 인덱스).
    pub fn session(&self) -> (Vec<PathBuf>, usize) {
        let tabs = self
            .tabs
            .iter()
            .map(|t| t.rows.source().tree().root_path().to_path_buf())
            .collect();
        (tabs, self.active)
    }

    // ── 접근 ───────────────────────────────────────────────────

    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn rows(&self) -> &VirtualRows<TreeSource> {
        &self.tabs[self.active].rows
    }

    pub fn rows_mut(&mut self) -> &mut VirtualRows<TreeSource> {
        &mut self.tabs[self.active].rows
    }

    pub fn root_path(&self) -> PathBuf {
        self.rows().source().tree().root_path().to_path_buf()
    }

    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    // ── 레이아웃·표시 ───────────────────────────────────────────

    /// 탭 바(상단) → 네비 바([←][→][↑] + 경로 바) → 리스트(잔여) 수직 배치(docs/20 §1·§2).
    pub fn set_bounds(&mut self, bounds: Rect, inv: &mut Invalidations) {
        self.bounds = bounds;
        let tab_h = self.m.tab_h.min(bounds.h);
        let bar_h = self.m.bar_h.min((bounds.h - tab_h).max(0));
        self.tabbar
            .set_bounds(Rect::new(bounds.x, bounds.y, bounds.w, tab_h), inv);
        // 네비 버튼 3개 = 간격 없이 연속, 경로 바도 바로 이어 붙임(사용자 지시 —
        // 이전 4px 틈은 미도색 영역이 검게 비치던 것이라 제거)
        let nav_w = (nav_btn_w(&self.m) * 3).min(bounds.w);
        self.navbtns
            .set_bounds(Rect::new(bounds.x, bounds.y + tab_h, nav_w, bar_h), inv);
        self.pathbar.set_bounds(
            Rect::new(
                bounds.x + nav_w,
                bounds.y + tab_h,
                (bounds.w - nav_w).max(0),
                bar_h,
            ),
            inv,
        );
        let list_y = bounds.y + tab_h + bar_h;
        for tab in &mut self.tabs {
            tab.rows.set_bounds(
                Rect::new(
                    bounds.x,
                    list_y,
                    bounds.w,
                    (bounds.bottom() - list_y).max(0),
                ),
                inv,
            );
        }
    }

    pub fn set_metrics(&mut self, m: PanelMetrics, columns: Vec<Column>, inv: &mut Invalidations) {
        self.m = m;
        self.tabbar.set_metrics(m.row_h, m.pad_x, inv);
        self.navbtns.set_metrics(m.row_h, m.pad_x, inv);
        self.navbtns.set_button_width(Some(nav_btn_w(&m)), inv);
        self.pathbar.set_metrics(m.row_h, m.pad_x, inv);
        for tab in &mut self.tabs {
            tab.rows.set_metrics(m.row_h, m.pad_x, m.indent_w, inv);
            tab.rows.set_columns(columns.clone(), inv);
        }
        self.set_bounds(self.bounds, inv);
    }

    pub fn set_focused(&mut self, focused: bool, inv: &mut Invalidations) {
        self.tabbar.set_focused(focused, inv);
    }

    pub fn paint(&self, ctx: &mut dyn DrawCtx, theme: &Theme) {
        self.rows().paint(ctx, theme);
        self.navbtns.paint(ctx, theme);
        self.pathbar.paint(ctx, theme);
        self.tabbar.paint(ctx, theme);
    }

    /// 탭 바·경로 바를 활성 탭 상태와 동기화.
    fn sync_chrome(&mut self, inv: &mut Invalidations) {
        let titles = self.tabs.iter().map(|t| t.title()).collect();
        self.tabbar.set_tabs(titles, self.active, inv);
        let path = self.root_path().to_string_lossy().into_owned();
        self.pathbar.set_path(path, inv);
    }

    // ── 탭 관리(원본 F20: 패널별 탭) ───────────────────────────

    /// 현재 경로를 복제한 새 탭(Ctrl+T·[+]).
    pub fn new_tab(&mut self, ctx: NavCtx, inv: &mut Invalidations) {
        let path = self.root_path();
        let Some(src) = open_source(&path, ctx) else {
            return;
        };
        let mut rows = VirtualRows::new(src, self.m.row_h, self.m.pad_x, self.m.indent_w);
        rows.set_columns(self.rows().columns().to_vec(), inv);
        self.tabs.push(Tab {
            rows,
            nav: History::new(path),
        });
        self.active = self.tabs.len() - 1;
        self.set_bounds(self.bounds, inv); // 새 탭 리스트 bounds 반영
        self.sync_chrome(inv);
    }

    /// 탭 닫기(Ctrl+W·×) — 패널은 항상 ≥1 탭.
    pub fn close_tab(&mut self, i: usize, inv: &mut Invalidations) {
        if self.tabs.len() <= 1 || i >= self.tabs.len() {
            return;
        }
        self.tabs.remove(i);
        if self.active >= self.tabs.len() || (self.active > i) {
            self.active = self.active.saturating_sub(1).min(self.tabs.len() - 1);
        }
        self.sync_chrome(inv);
        inv.push(self.bounds);
    }

    pub fn switch_tab(&mut self, i: usize, inv: &mut Invalidations) {
        if i < self.tabs.len() && i != self.active {
            self.active = i;
            self.sync_chrome(inv);
            inv.push(self.bounds);
        }
    }

    /// 다음 탭으로 순환(Ctrl+Tab).
    pub fn next_tab(&mut self, inv: &mut Invalidations) {
        if self.tabs.len() > 1 {
            let next = (self.active + 1) % self.tabs.len();
            self.switch_tab(next, inv);
        }
    }

    // ── 네비게이션(활성 탭 — docs/20 §3 탭별 독립) ──────────────

    fn apply_source(&mut self, src: TreeSource, inv: &mut Invalidations) {
        // 펼침 상태 이월(사용자 지시 07-13): 하위 진입 = 새 루트 아래의 기존 펼침 유지 ·
        // 상위 이동 = 직전 루트(와 그 하위 펼침)를 펼친 채 표시. 새 루트 밖 경로는
        // expand_path가 무시하므로 방향 구분 불요.
        let mut expanded: Vec<String> = vec![self.root_path().to_string_lossy().into_owned()];
        {
            let tree = self.rows().source().tree();
            for i in 0..tree.visible_len() {
                if let Some(r) = tree.row(i) {
                    if r.expanded {
                        if let Some(p) = tree.node_path(r.id) {
                            expanded.push(p.to_string_lossy().into_owned()); // 부모 우선 순
                        }
                    }
                }
            }
        }
        self.tabs[self.active].rows.replace_source(src, inv);
        let tree = self.rows_mut().source_mut().tree_mut();
        for p in &expanded {
            let _ = tree.expand_path(p);
        }
        self.sync_chrome(inv);
    }

    /// 새 경로 진입(히스토리 push — 앞으로 절단). 열기 실패 시 현 위치 유지.
    pub fn navigate_to(&mut self, path: PathBuf, ctx: NavCtx, inv: &mut Invalidations) {
        let Some(src) = open_source(&path, ctx) else {
            self.sync_chrome(inv); // 편집 제출 실패 시 브레드크럼 복귀
            return;
        };
        self.tabs[self.active].nav.push(path);
        self.apply_source(src, inv);
    }

    pub fn nav_back(&mut self, ctx: NavCtx, inv: &mut Invalidations) {
        let Some(p) = self.tabs[self.active].nav.back().map(Path::to_path_buf) else {
            return;
        };
        match open_source(&p, ctx) {
            Some(src) => self.apply_source(src, inv),
            None => {
                let _ = self.tabs[self.active].nav.forward(); // 실패 — 위치 복원
            }
        }
    }

    pub fn nav_forward(&mut self, ctx: NavCtx, inv: &mut Invalidations) {
        let Some(p) = self.tabs[self.active].nav.forward().map(Path::to_path_buf) else {
            return;
        };
        match open_source(&p, ctx) {
            Some(src) => self.apply_source(src, inv),
            None => {
                let _ = self.tabs[self.active].nav.back();
            }
        }
    }

    /// 위로(부모 폴더) — Alt+↑.
    pub fn nav_up(&mut self, ctx: NavCtx, inv: &mut Invalidations) {
        if let Some(parent) = self.root_path().parent().map(Path::to_path_buf) {
            self.navigate_to(parent, ctx, inv);
        }
    }

    /// 행 활성화(더블클릭·Enter) — 폴더면 진입. 파일 실행은 M3.
    pub fn activate_row(&mut self, row: usize, ctx: NavCtx, inv: &mut Invalidations) {
        let tree = self.rows().source().tree();
        let (Some(r), Some(id)) = (tree.row(row), tree.visible_id(row)) else {
            return;
        };
        if r.kind != FileKind::Dir {
            return;
        }
        if let Some(p) = tree.node_path(id).map(Path::to_path_buf) {
            self.navigate_to(p, ctx, inv);
        }
    }

    /// 가시성 필터 토글·파일 조작 후 현 위치 재열기(히스토리 무이동).
    /// **무간섭 갱신**(원본 NAV-UPFOCUS 계승 — M3-6 선행): 펼침·선택·캐럿·스크롤을
    /// 스냅샷 후 새 소스에 복원한다(소실 항목은 개별 무시 — 외부 변경).
    pub fn reopen_filtered(&mut self, ctx: NavCtx, inv: &mut Invalidations) {
        let path = self.root_path();
        let Some(src) = open_source(&path, ctx) else {
            return;
        };
        // 1) 스냅샷(경로 기준 — 재열기 후 인덱스/ID는 무효)
        let (expanded, selected, caret_path, scroll_row, scroll_x) = {
            let rows = self.rows();
            let tree = rows.source().tree();
            let mut expanded = Vec::new();
            for i in 0..tree.visible_len() {
                if let Some(r) = tree.row(i) {
                    if r.expanded {
                        if let Some(p) = tree.node_path(r.id) {
                            expanded.push(p.to_string_lossy().into_owned()); // 가시 순 = 부모 우선
                        }
                    }
                }
            }
            let selected: Vec<String> = tree
                .selected_paths()
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect();
            let caret_path = rows
                .caret()
                .and_then(|c| tree.visible_id(c))
                .and_then(|id| tree.node_path(id))
                .map(|p| p.to_string_lossy().into_owned());
            (
                expanded,
                selected,
                caret_path,
                rows.scroll_row(),
                rows.scroll_x(),
            )
        };
        // 2) 재열기
        self.tabs[self.active].nav.replace(path);
        self.apply_source(src, inv);
        // 3) 복원 — 펼침(부모 우선 순서)·선택·캐럿·스크롤
        let rows = self.rows_mut();
        let tree = rows.source_mut().tree_mut();
        for p in &expanded {
            let _ = tree.expand_path(p); // 소실 폴더 무시
        }
        for p in &selected {
            if let Some(id) = tree.index_of_path(p).and_then(|i| tree.visible_id(i)) {
                if !tree.is_selected(id) {
                    tree.select(id, nexa_tree::SelectMode::Toggle);
                }
            }
        }
        let caret = caret_path.as_deref().and_then(|p| tree.index_of_path(p));
        rows.restore_view(caret, scroll_row, scroll_x, inv);
    }

    // ── 이벤트 ─────────────────────────────────────────────────

    /// 패널 내부 y-라우팅(마우스 계열). 키보드·편집은 호스트가 활성 패널에 직접.
    pub fn on_event(&mut self, ev: &InputEvent, inv: &mut Invalidations) {
        match *ev {
            InputEvent::MouseDown { x, y, .. } | InputEvent::RightDown { x, y } => {
                if y < self.pathbar.bounds().y {
                    self.tabbar.on_event(ev, inv);
                } else if y < self.rows().bounds().y {
                    // 네비 바 행: [←][→][↑] | 경로 바
                    if x < self.pathbar.bounds().x {
                        self.navbtns.on_event(ev, inv);
                    } else {
                        self.pathbar.on_event(ev, inv);
                    }
                } else {
                    self.tabs[self.active].rows.on_event(ev, inv);
                }
            }
            InputEvent::MouseMove { .. } => {
                self.tabbar.on_event(ev, inv);
                self.navbtns.on_event(ev, inv);
                self.pathbar.on_event(ev, inv);
                self.tabs[self.active].rows.on_event(ev, inv);
            }
            InputEvent::MouseUp { .. } => {
                // 경로바 편집 드래그 선택 종료(QA 07-13) + 리스트 밴드/리사이즈 종료
                self.pathbar.on_event(ev, inv);
                self.tabs[self.active].rows.on_event(ev, inv);
            }
            _ => self.tabs[self.active].rows.on_event(ev, inv),
        }
    }

    /// 위젯들이 쌓아 둔 동작 수거 — 탭·경로 바·**패널 네비 버튼**(이 패널의 활성 탭 대상).
    pub fn drain_actions(&mut self, ctx: NavCtx, inv: &mut Invalidations) {
        if let Some(action) = self.tabbar.take_action() {
            match action {
                TabAction::Switch(i) => self.switch_tab(i, inv),
                TabAction::Close(i) => self.close_tab(i, inv),
                TabAction::New => self.new_tab(ctx, inv),
            }
        }
        if let Some(p) = self.pathbar.take_navigation() {
            self.navigate_to(PathBuf::from(p), ctx, inv);
        }
        if let Some(btn) = self.navbtns.take_command() {
            match btn {
                BTN_BACK => self.nav_back(ctx, inv),
                BTN_FORWARD => self.nav_forward(ctx, inv),
                BTN_UP => self.nav_up(ctx, inv),
                _ => {}
            }
        }
    }
}

/// 네비 버튼 1개 폭(고정 — 레이아웃이 측정 없이 계산).
fn nav_btn_w(m: &PanelMetrics) -> i32 {
    m.row_h + m.pad_x
}

/// 패널 네비 버튼 정의([←][→][↑] — 원본 docs/20 §2 네비게이션 바).
fn nav_buttons() -> Vec<ToolButton> {
    [(BTN_BACK, "←"), (BTN_FORWARD, "→"), (BTN_UP, "↑")]
        .into_iter()
        .map(|(id, g)| ToolButton {
            id,
            glyph: g.into(),
        })
        .collect()
}

/// 현재 필터로 경로를 연다. 실패(권한 등) 시 `None`(오류 격리 — 호출자가 위치 유지).
fn open_source(path: &Path, ctx: NavCtx) -> Option<TreeSource> {
    match Tree::open_filtered(path, ctx.show_hidden, ctx.show_dotfiles) {
        Ok(t) => Some(TreeSource::new(t, ctx.tz)),
        Err(e) => {
            eprintln!("{} 열기 실패: {e}", path.display());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn ctx() -> NavCtx {
        NavCtx {
            show_hidden: true,
            show_dotfiles: true,
            tz: 0,
        }
    }

    fn metrics() -> PanelMetrics {
        PanelMetrics {
            row_h: 20,
            pad_x: 6,
            indent_w: 16,
            tab_h: 22,
            bar_h: 24,
        }
    }

    fn fixture(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("nexa_panel_{}_{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("sub")).unwrap();
        fs::write(base.join("a.txt"), b"a").unwrap();
        base
    }

    fn panel(base: &Path) -> (Panel, Invalidations) {
        let mut inv = Invalidations::default();
        let mut p = Panel::new(Tree::open(base).unwrap(), ctx(), metrics(), Vec::new());
        p.set_bounds(Rect::new(0, 0, 400, 400), &mut inv);
        (p, inv)
    }

    #[test]
    fn navigation_carries_expansion_down_and_up() {
        let base = fixture("navexp");
        fs::create_dir_all(base.join("sub").join("inner")).unwrap();
        let (mut p, mut inv) = panel(&base);
        let sub = base.join("sub");
        let inner = sub.join("inner");
        // base 뷰에서 sub·sub/inner 펼침
        {
            let tree = p.rows_mut().source_mut().tree_mut();
            tree.expand_path(&sub.to_string_lossy()).unwrap();
            tree.expand_path(&inner.to_string_lossy()).unwrap();
        }
        // 하위 진입(sub) — inner 펼침 유지
        p.navigate_to(sub.clone(), ctx(), &mut inv);
        {
            let tree = p.rows().source().tree();
            let i = tree.index_of_path(&inner.to_string_lossy()).unwrap();
            let id = tree.visible_id(i).unwrap();
            assert_eq!(tree.is_expanded(id), Some(true), "하위 진입 시 펼침 이월");
        }
        // 상위 이동(base) — 직전 루트(sub)와 그 하위(inner) 펼침 유지
        p.navigate_to(base.clone(), ctx(), &mut inv);
        {
            let tree = p.rows().source().tree();
            for q in [&sub, &inner] {
                let i = tree.index_of_path(&q.to_string_lossy()).unwrap();
                let id = tree.visible_id(i).unwrap();
                assert_eq!(tree.is_expanded(id), Some(true), "상위 이동 시 펼침 이월");
            }
        }
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn reopen_preserves_expanded_selection_caret_scroll() {
        let base = fixture("reopen");
        fs::create_dir_all(base.join("sub").join("inner")).unwrap();
        fs::write(base.join("sub").join("x.txt"), b"x").unwrap();
        let (mut p, mut inv) = panel(&base);
        // 펼침(sub) + 선택/캐럿(sub/x.txt) 구성
        let sub = base.join("sub");
        {
            let tree = p.rows_mut().source_mut().tree_mut();
            tree.expand_path(&sub.to_string_lossy()).unwrap();
            let xi = tree
                .index_of_path(&base.join("sub").join("x.txt").to_string_lossy())
                .expect("펼침 후 x.txt 가시");
            let id = tree.visible_id(xi).unwrap();
            tree.select(id, nexa_tree::SelectMode::Single);
        }
        let caret_row = p
            .rows()
            .source()
            .tree()
            .index_of_path(&base.join("sub").join("x.txt").to_string_lossy())
            .unwrap();
        p.rows_mut().restore_view(Some(caret_row), 0, 0, &mut inv);
        // 외부 변경(파일 추가) 후 재열기 — 무간섭 갱신(M3-6 선행)
        fs::write(base.join("new.txt"), b"n").unwrap();
        p.reopen_filtered(ctx(), &mut inv);
        let tree = p.rows().source().tree();
        let sub_i = tree.index_of_path(&sub.to_string_lossy()).unwrap();
        let sub_id = tree.visible_id(sub_i).unwrap();
        assert_eq!(tree.is_expanded(sub_id), Some(true), "펼침 보존");
        assert_eq!(
            tree.selected_paths(),
            vec![base.join("sub").join("x.txt").as_path()],
            "선택 보존"
        );
        let caret = p.rows().caret().expect("캐럿 보존");
        assert_eq!(
            tree.visible_id(caret).and_then(|id| tree.node_path(id)),
            Some(base.join("sub").join("x.txt").as_path()),
            "캐럿 = 같은 경로"
        );
        assert!(
            tree.index_of_path(&base.join("new.txt").to_string_lossy())
                .is_some(),
            "새 항목 반영"
        );
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn layout_stacks_tabbar_navbar_rows() {
        let base = fixture("layout");
        let (p, _) = panel(&base);
        fs::remove_dir_all(&base).unwrap();
        assert_eq!(p.tabbar.bounds(), Rect::new(0, 0, 400, 22));
        // 네비 바 행 = [←][→][↑] 연속 3×26=78 + 경로 바 바로 이어 붙임(사용자 지시)
        assert_eq!(p.navbtns.bounds(), Rect::new(0, 22, 78, 24));
        assert_eq!(p.pathbar.bounds(), Rect::new(78, 22, 322, 24));
        assert_eq!(p.rows().bounds(), Rect::new(0, 46, 400, 354));
    }

    #[test]
    fn panel_nav_buttons_drive_this_panels_tab_history() {
        let base = fixture("navbtn");
        let (mut p, mut inv) = panel(&base);
        p.navigate_to(base.join("sub"), ctx(), &mut inv); // 히스토리: base → sub
        p.paint(&mut PaintProbe, &Theme::dark()); // 버튼 히트 범위 캐시
                                                  // [←] 클릭(첫 버튼 영역) → 뒤로
        p.on_event(
            &InputEvent::MouseDown {
                x: 10,
                y: 30,
                shift: false,
                ctrl: false,
            },
            &mut inv,
        );
        p.drain_actions(ctx(), &mut inv);
        assert_eq!(p.root_path(), base, "[←] = 이 패널 활성 탭의 뒤로");
        fs::remove_dir_all(&base).unwrap();
    }

    struct PaintProbe;
    impl nexa_gui::DrawCtx for PaintProbe {
        fn fill_rect(&mut self, _r: Rect, _c: nexa_gui::Color) {}
        fn text_opaque(
            &mut self,
            _x: i32,
            _y: i32,
            _c: Rect,
            _t: &str,
            _f: nexa_gui::Color,
            _b: nexa_gui::Color,
        ) {
        }
        fn text_width(&mut self, text: &str) -> i32 {
            text.chars().count() as i32 * 8
        }
    }

    #[test]
    fn tabs_open_switch_close_keep_at_least_one() {
        let base = fixture("tabs");
        let (mut p, mut inv) = panel(&base);
        p.new_tab(ctx(), &mut inv); // 현재 경로 복제
        assert_eq!((p.tab_count(), p.active_index()), (2, 1));
        // 탭별 독립 네비: 탭1만 sub로 진입
        p.navigate_to(base.join("sub"), ctx(), &mut inv);
        assert!(p.root_path().ends_with("sub"));
        p.switch_tab(0, &mut inv);
        assert!(!p.root_path().ends_with("sub"), "탭0은 원 경로 유지");
        assert_eq!(p.pathbar.path(), p.root_path().to_string_lossy());
        // 닫기 — 최소 1개 유지
        p.close_tab(1, &mut inv);
        assert_eq!(p.tab_count(), 1);
        p.close_tab(0, &mut inv);
        assert_eq!(p.tab_count(), 1, "마지막 탭은 닫기 불가");
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn per_tab_history_is_independent() {
        let base = fixture("hist");
        let (mut p, mut inv) = panel(&base);
        p.navigate_to(base.join("sub"), ctx(), &mut inv); // 탭0: base → sub
        p.new_tab(ctx(), &mut inv); // 탭1: sub에서 시작(히스토리 새로)
        p.nav_back(ctx(), &mut inv); // 탭1은 back 불가(스택 1)
        assert!(p.root_path().ends_with("sub"));
        p.switch_tab(0, &mut inv);
        p.nav_back(ctx(), &mut inv); // 탭0은 base로 back
        assert_eq!(p.root_path(), base);
        p.nav_forward(ctx(), &mut inv);
        assert!(p.root_path().ends_with("sub"));
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn navigate_failure_keeps_position() {
        let base = fixture("fail");
        let (mut p, mut inv) = panel(&base);
        let before = p.root_path();
        p.navigate_to(PathBuf::from("Z:\\no\\such\\dir"), ctx(), &mut inv);
        assert_eq!(p.root_path(), before);
        assert_eq!(
            p.pathbar.path(),
            before.to_string_lossy(),
            "브레드크럼 복귀"
        );
        fs::remove_dir_all(&base).unwrap();
    }
}
