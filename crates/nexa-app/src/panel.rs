//! 패널 — 탭 바 + 경로 바 + 파일 리스트 묶음(원본 PanelView·docs/20 §2).
//! 탭 = 독립 뷰 상태 + **탭별 독립 히스토리**(docs/20 §3). 플랫폼 중립 — 전 플랫폼 테스트.
//! 창/렌더 배선(win.rs)은 패널 2개를 스플리터로 배치하고 활성 패널에 키보드를 라우팅한다.

use std::path::{Path, PathBuf};

use nexa_core::FileKind;
use nexa_gui::widgets::{InfoDock, PathBar, TabAction, TabBar, ToolButton, Toolbar, VirtualRows};
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

/// 탭 = 리스트 뷰 상태 + 독립 back/forward 히스토리 + **영속 펼침 집합**(원본 F18 — X-4).
pub struct Tab {
    pub rows: VirtualRows<TreeSource>,
    pub nav: History,
    /// 펼침 경로 집합(키=소문자 정규화·값=원 표기, BTreeMap=사전순 → **부모 우선** 재적용).
    /// 현재 트리와 별개로 유지 — 다른 폴더로 이동해도 펼침 상태가 소실되지 않는다.
    expanded: std::collections::BTreeMap<String, PathBuf>,
    /// 탭 잠금(닫기 제외 — 원본 TAB-MENU, 편의 UX ②). 세션 영속.
    locked: bool,
    /// 탭 고정(📌 핀 그룹 앞 정렬 — 사용자 요청 07-15). 세션 영속.
    pinned: bool,
}

/// F18 펼침 키 — 대소문자 무시·후행 구분자 제거(원본 OrdinalIgnoreCase HashSet 대응).
fn expand_key(p: &Path) -> String {
    p.to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_lowercase()
}

impl Tab {
    fn title(&self) -> String {
        let p = self.rows.source().tree().root_path();
        if nexa_vfs::is_virtual_root(p) {
            return crate::i18n::tr("nav.mypc"); // 가상 최상위(X-17)
        }
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
    /// 하단 도크(M4-1, 원본 대원칙: 듀얼=좌↔좌·우↔우 — 패널별 1개). 표시 여부는 호스트 전역.
    pub dock: InfoDock,
    dock_visible: bool,
    /// Alt+↑ 자동 선택의 뷰 배치(사용자 QA 07-15 — 설정 `nav_up_align`).
    nav_up_align: nexa_gui::widgets::ScrollAlign,
    /// 정렬 옵션(G-13·07-15) — **새 소스(탐색·재로드·새 탭)에도 재적용**하기 위해 보관.
    sort_folders_first: bool,
    sort_case: bool,
    /// 타입어헤드 옵션(07-15): (범위, 리셋 ms, 특수문자, 공백, Backspace, HUD 위치).
    ta_opts: (nexa_tree::FindScope, u64, bool, bool, bool, u8),
    /// 폰트 장식(X-12): (폴더 굵게, 헤더 굵게, 헤더 이탤릭) — 새 탭에도 재적용.
    font_decor: (bool, bool, bool),
    /// 호스트가 준 기본 컬럼(내 PC 드라이브 컬럼 전환의 복귀 원본 — X-17).
    base_columns: Vec<Column>,
    /// 세션 저장 요청 플래그(사용자 요청 07-15 — 탭/경로 변경 시 표시만, 저장은
    /// 호스트가 디바운스 flush: 폭주해도 마지막 상태 1회만 기록).
    session_dirty: bool,
    /// 도크 높이 비율(리스트+도크 영역 대비 — S2 드래그·영속. 전역 공유 값이 양 패널에 적용).
    dock_ratio: f32,
    tabs: Vec<Tab>,
    active: usize,
    bounds: Rect,
    m: PanelMetrics,
    /// 탭 우클릭 메뉴 요청(편의 UX ② — 표시는 호스트 몫, 1회성 수거).
    pending_tab_menu: Option<usize>,
    /// 패널 포커스(활성/비활성) — 새 탭 생성 시 리스트에 물려준다.
    focused: bool,
}

impl Panel {
    /// 첫 탭을 `tree`로 시작.
    pub fn new(tree: Tree, ctx: NavCtx, m: PanelMetrics, columns: Vec<Column>) -> Panel {
        let root = tree.root_path().to_path_buf();
        let mut inv = Invalidations::default();
        let mut rows =
            VirtualRows::new(TreeSource::new(tree, ctx.tz), m.row_h, m.pad_x, m.indent_w);
        rows.set_columns(columns.clone(), &mut inv);
        let mut p = Panel {
            tabbar: TabBar::new(m.row_h, m.pad_x),
            navbtns: Toolbar::new(nav_buttons(), m.row_h, m.pad_x).with_button_width(nav_btn_w(&m)),
            pathbar: PathBar::new(root.to_string_lossy(), m.row_h, m.pad_x),
            dock: InfoDock::new("", m.row_h, m.pad_x),
            dock_visible: false,
            nav_up_align: nexa_gui::widgets::ScrollAlign::default(),
            sort_folders_first: true,
            sort_case: false,
            ta_opts: (
                nexa_tree::FindScope::VisibleStream,
                1000,
                true,
                true,
                true,
                6,
            ),
            base_columns: columns,
            font_decor: (false, false, false),
            session_dirty: false,
            dock_ratio: 0.3,
            tabs: vec![Tab {
                rows,
                nav: History::new(root),
                expanded: Default::default(),
                locked: false,
                pinned: false,
            }],
            active: 0,
            bounds: Rect::default(),
            m,
            pending_tab_menu: None,
            focused: true,
        };
        p.sync_chrome(&mut inv);
        p
    }

    /// 세션 복원 — 경로 목록으로 탭들을 연다(열기 실패 탭은 건너뜀·전부 실패면 `fallback`).
    /// `expanded` = 탭별 펼침 경로 목록(F18 — X-4, `paths`와 인덱스 정렬·부족분 허용).
    pub fn restore(
        paths: &[PathBuf],
        active: usize,
        expanded: &[Vec<PathBuf>],
        fallback: &Path,
        ctx: NavCtx,
        m: PanelMetrics,
        columns: Vec<Column>,
    ) -> Panel {
        let mut valid: Vec<(Tree, Vec<PathBuf>)> = paths
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                let tree = Tree::open_filtered(p, ctx.show_hidden, ctx.show_dotfiles).ok()?;
                Some((tree, expanded.get(i).cloned().unwrap_or_default()))
            })
            .collect();
        if valid.is_empty() {
            valid.push((
                Tree::open_filtered(fallback, ctx.show_hidden, ctx.show_dotfiles)
                    .or_else(|_| Tree::open("C:\\"))
                    .expect("C:\\ 열기 실패"),
                Vec::new(),
            ));
        }
        let (first, first_exp) = valid.remove(0);
        let mut p = Panel::new(first, ctx, m, columns.clone());
        let mut inv = Invalidations::default();
        Self::seed_expanded(&mut p.tabs[0], &first_exp);
        for (tree, exp) in valid {
            let root = tree.root_path().to_path_buf();
            let mut rows =
                VirtualRows::new(TreeSource::new(tree, ctx.tz), m.row_h, m.pad_x, m.indent_w);
            rows.set_columns(columns.clone(), &mut inv);
            let mut tab = Tab {
                rows,
                nav: History::new(root),
                expanded: Default::default(),
                locked: false,
                pinned: false,
            };
            Self::seed_expanded(&mut tab, &exp);
            p.tabs.push(tab);
        }
        p.active = active.min(p.tabs.len() - 1);
        p.sync_chrome(&mut inv);
        p
    }

    /// 세션 펼침 목록을 탭 집합에 시드하고 트리에 적용(부모 우선 — BTreeMap 키 사전순).
    fn seed_expanded(tab: &mut Tab, exp: &[PathBuf]) {
        for p in exp {
            tab.expanded.insert(expand_key(p), p.clone());
        }
        let entries: Vec<PathBuf> = tab.expanded.values().cloned().collect();
        let tree = tab.rows.source_mut().tree_mut();
        for p in &entries {
            let _ = tree.expand_path(&p.to_string_lossy()); // 소실 폴더 무시
        }
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

    /// 세션 펼침 스냅샷(F18 — X-4) — 탭별 펼침 경로(현재 트리와 동기 후·상한 200/탭).
    pub fn session_expanded(&mut self) -> Vec<Vec<PathBuf>> {
        let saved = self.active;
        let mut out = Vec::with_capacity(self.tabs.len());
        for i in 0..self.tabs.len() {
            self.active = i;
            self.sync_expanded(); // 각 탭의 집합을 그 탭 트리와 동기
            out.push(self.tabs[i].expanded.values().take(200).cloned().collect());
        }
        self.active = saved;
        out
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
        // 도크는 패널 밖 **전폭 밴드**(X-6 — 호스트 layout이 dock.set_bounds 직접 지정.
        // 파일 좌/우와 도크 좌/우의 폭이 독립) — 리스트가 패널 전체 높이를 쓴다.
        let list_h = (bounds.bottom() - list_y).max(0);
        for tab in &mut self.tabs {
            tab.rows
                .set_bounds(Rect::new(bounds.x, list_y, bounds.w, list_h), inv);
        }
        // 자동완성 팝업 하한 = 리스트 바닥(도크/터미널 침범 금지 — PATH-SUG)
        self.pathbar.set_overlay_bottom(list_y + list_h);
    }

    /// 하단 도크 표시 토글(호스트 전역 Ctrl+` — 원본 대원칙: 듀얼=패널별 아래).
    pub fn set_dock_visible(&mut self, on: bool, inv: &mut Invalidations) {
        if self.dock_visible != on {
            self.dock_visible = on;
            self.set_bounds(self.bounds, inv);
            inv.push(self.bounds);
        }
    }

    pub fn dock_visible(&self) -> bool {
        self.dock_visible
    }

    /// 도크 높이 비율 적용(드래그·설정 복원 — 0.15~0.5 클램프) 후 재배치.
    pub fn set_dock_ratio(&mut self, ratio: f32, inv: &mut Invalidations) {
        let r = ratio.clamp(0.15, 0.5);
        if (self.dock_ratio - r).abs() > f32::EPSILON {
            self.dock_ratio = r;
            self.set_bounds(self.bounds, inv);
            inv.push(self.bounds);
        }
    }

    pub fn dock_ratio(&self) -> f32 {
        self.dock_ratio
    }

    pub fn set_metrics(&mut self, m: PanelMetrics, columns: Vec<Column>, inv: &mut Invalidations) {
        self.m = m;
        self.base_columns = columns.clone();
        self.tabbar.set_metrics(m.row_h, m.pad_x, inv);
        self.navbtns.set_metrics(m.row_h, m.pad_x, inv);
        self.navbtns.set_button_width(Some(nav_btn_w(&m)), inv);
        self.pathbar.set_metrics(m.row_h, m.pad_x, inv);
        self.dock.set_metrics(m.row_h, m.pad_x, inv);
        for tab in &mut self.tabs {
            tab.rows.set_metrics(m.row_h, m.pad_x, m.indent_w, inv);
            tab.rows.set_columns(columns.clone(), inv);
        }
        self.set_bounds(self.bounds, inv);
    }

    /// 패널(탭 바+리스트) 키 포커스 표시 — QA 07-15: 터미널 포커스 중엔 활성 패널도
    /// 전부 비활성 색(실제 포커스 영역만 강조). 도크 스트립 강조는 호스트가
    /// `dock.set_focused`로 터미널 포커스에 맞춰 직접 동기한다.
    pub fn set_focused(&mut self, focused: bool, inv: &mut Invalidations) {
        self.focused = focused;
        self.tabbar.set_focused(focused, inv);
        for tab in &mut self.tabs {
            tab.rows.set_focused(focused, inv);
        }
    }

    pub fn paint(&self, ctx: &mut dyn DrawCtx, theme: &Theme) {
        self.rows().paint(ctx, theme);
        if self.dock_visible {
            self.dock.paint(ctx, theme);
        }
        self.navbtns.paint(ctx, theme);
        self.pathbar.paint(ctx, theme);
        self.tabbar.paint(ctx, theme);
        // 자동완성 팝업(PATH-SUG) — 리스트 위 오버레이라 마지막에
        self.pathbar.paint_suggest(ctx, theme);
    }

    /// 탭 바·경로 바를 활성 탭 상태와 동기화.
    fn sync_chrome(&mut self, inv: &mut Invalidations) {
        let titles = self.tabs.iter().map(|t| t.title()).collect();
        self.tabbar.set_tabs(titles, self.active, inv);
        self.tabbar
            .set_locked(self.tabs.iter().map(|t| t.locked).collect(), inv);
        self.tabbar
            .set_pinned(self.tabs.iter().map(|t| t.pinned).collect(), inv);
        // 가상 최상위(X-17)는 사람이 읽는 라벨로 — 단일(=현재) 세그먼트라 클릭 무동작
        let root = self.root_path();
        let path = if nexa_vfs::is_virtual_root(&root) {
            crate::i18n::tr("nav.mypc")
        } else {
            root.to_string_lossy().into_owned()
        };
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
        rows.set_focused(self.focused, inv);
        // 새 탭 = 현재 탭 복제 — 보기 모드도 계승(07-16 탭별 규약)
        rows.set_view_mode(self.active_view_mode(), inv);
        self.tabs.push(Tab {
            rows,
            nav: History::new(path),
            expanded: Default::default(),
            locked: false,
            pinned: false,
        });
        self.active = self.tabs.len() - 1;
        self.apply_sort_opts(self.active); // 정렬 옵션 전파(07-15 — 새 탭 유지)
        self.session_dirty = true;
        self.set_bounds(self.bounds, inv); // 새 탭 리스트 bounds 반영
        self.sync_chrome(inv);
    }

    /// 탭 재정렬(드래그 — 편의 UX ②): `from` 탭을 `to` 위치로. 활성 탭 추종.
    pub fn move_tab(&mut self, from: usize, to: usize, inv: &mut Invalidations) {
        self.session_dirty = true;
        if from >= self.tabs.len() || to >= self.tabs.len() || from == to {
            return;
        }
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        // 활성 인덱스 추종(잡은 탭이 활성일 수도, 사이 탭이 밀릴 수도)
        self.active = if self.active == from {
            to
        } else if from < self.active && self.active <= to {
            self.active - 1
        } else if to <= self.active && self.active < from {
            self.active + 1
        } else {
            self.active
        };
        self.sync_chrome(inv);
        inv.push(self.bounds);
    }

    /// 탭 복제(우클릭 메뉴 — 원본 TAB-MENU): 같은 경로 + 펼침 집합 복사, 바로 옆에 삽입.
    pub fn duplicate_tab(&mut self, i: usize, ctx: NavCtx, inv: &mut Invalidations) {
        if i >= self.tabs.len() {
            return;
        }
        let path = self.tabs[i].rows.source().tree().root_path().to_path_buf();
        let Some(src) = open_source(&path, ctx) else {
            return;
        };
        let mut rows = VirtualRows::new(src, self.m.row_h, self.m.pad_x, self.m.indent_w);
        rows.set_columns(self.rows().columns().to_vec(), inv);
        rows.set_focused(self.focused, inv);
        let mut tab = Tab {
            rows,
            nav: History::new(path),
            expanded: self.tabs[i].expanded.clone(),
            locked: false,
            pinned: false, // 복제본은 잠금 해제 상태(원본 동일)
        };
        let entries: Vec<PathBuf> = tab.expanded.values().cloned().collect();
        let tree = tab.rows.source_mut().tree_mut();
        for p in &entries {
            let _ = tree.expand_path(&p.to_string_lossy());
        }
        self.tabs.insert(i + 1, tab);
        self.active = i + 1;
        self.set_bounds(self.bounds, inv);
        self.sync_chrome(inv);
    }

    /// 탭 잠금 토글(우클릭 메뉴 — 닫기 제외, 원본 TAB-MENU).
    pub fn toggle_tab_lock(&mut self, i: usize, inv: &mut Invalidations) {
        self.session_dirty = true;
        if let Some(t) = self.tabs.get_mut(i) {
            t.locked = !t.locked;
            self.sync_chrome(inv);
        }
    }

    pub fn tab_locked(&self, i: usize) -> bool {
        self.tabs.get(i).is_some_and(|t| t.locked)
    }

    /// 탭 고정 토글(사용자 요청 07-15) — 고정 시 핀 그룹 끝으로, 해제 시 그룹 밖으로 이동.
    pub fn toggle_tab_pin(&mut self, i: usize, inv: &mut Invalidations) {
        self.session_dirty = true;
        let Some(t) = self.tabs.get_mut(i) else {
            return;
        };
        t.pinned = !t.pinned;
        // 핀 그룹(앞쪽) 경계 = i 제외 고정 탭 수 — 고정=그룹 끝으로, 해제=그룹 직후로
        // (둘 다 경계 위치와 일치)
        let target = self
            .tabs
            .iter()
            .enumerate()
            .filter(|(j, t)| *j != i && t.pinned)
            .count();
        if target != i {
            self.move_tab(i, target, inv);
        }
        self.sync_chrome(inv);
    }

    pub fn tab_pinned(&self, i: usize) -> bool {
        self.tabs.get(i).is_some_and(|t| t.pinned)
    }

    pub fn session_pinned(&self) -> Vec<bool> {
        self.tabs.iter().map(|t| t.pinned).collect()
    }

    /// 세션 고정 시드(부족분 false — locked와 동일 규약).
    pub fn seed_pinned(&mut self, pinned: &[bool], inv: &mut Invalidations) {
        for (t, p) in self.tabs.iter_mut().zip(pinned) {
            t.pinned = *p;
        }
        self.sync_chrome(inv);
    }

    /// 탭 우클릭 메뉴 요청 수거(1회성 — 호스트가 네이티브 팝업 표시).
    pub fn take_tab_menu(&mut self) -> Option<usize> {
        self.pending_tab_menu.take()
    }

    /// 세션 잠금 스냅샷(원본 TabSession.Locked).
    pub fn session_locked(&self) -> Vec<bool> {
        self.tabs.iter().map(|t| t.locked).collect()
    }

    /// 세션 잠금 복원 — restore 후 호출(탭 인덱스 정렬·부족분 무시).
    pub fn seed_locked(&mut self, locked: &[bool], inv: &mut Invalidations) {
        for (i, l) in locked.iter().enumerate() {
            if let Some(t) = self.tabs.get_mut(i) {
                t.locked = *l;
            }
        }
        self.sync_chrome(inv);
    }

    /// 탭 닫기(Ctrl+W·×) — 패널은 항상 ≥1 탭·잠긴 탭은 닫지 않음(원본 TAB-MENU).
    pub fn close_tab(&mut self, i: usize, inv: &mut Invalidations) {
        self.session_dirty = true;
        if self.tabs.len() <= 1 || i >= self.tabs.len() || self.tabs[i].locked {
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
        self.session_dirty = true;
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

    /// 활성 탭 펼침 집합을 현재 트리와 동기화(F18 — X-4): 가시 폴더 행 기준
    /// **펼침=등재·접힘=말소**. 비가시(부모 접힘) 엔트리는 보존 — 부모를 다시 펼치면
    /// 하위 펼침까지 복원된다(expand_path가 가시 경로만 처리하므로 부활 부작용 없음).
    fn sync_expanded(&mut self) {
        let mut add: Vec<PathBuf> = Vec::new();
        let mut del: Vec<String> = Vec::new();
        {
            let tree = self.rows().source().tree();
            for i in 0..tree.visible_len() {
                if let Some(r) = tree.row(i) {
                    if !r.has_children {
                        continue; // 폴더(펼침 가능) 행만
                    }
                    if let Some(p) = tree.node_path(r.id) {
                        if r.expanded {
                            add.push(p.to_path_buf());
                        } else {
                            del.push(expand_key(p));
                        }
                    }
                }
            }
        }
        let set = &mut self.tabs[self.active].expanded;
        for k in del {
            set.remove(&k);
        }
        for p in add {
            set.insert(expand_key(&p), p);
        }
    }

    fn apply_source(&mut self, src: TreeSource, inv: &mut Invalidations) {
        // 펼침 상태 유지(원본 F18 — X-4): 경계에서 집합 동기 후 새 루트 아래 엔트리 재적용.
        // **방문 ≠ 확장**(사용자 지시 07-14): 더블클릭 진입은 확장 버튼과 동일 취급하지
        // 않는다 — 직전 루트 자동 등재 없음. 마커로 명시 펼침한 폴더만 집합에 남는다.
        // 새 루트 밖·부모 접힘 경로는 expand_path가 무시하므로 방향 구분 불요.
        self.sync_expanded();
        self.tabs[self.active].rows.replace_source(src, inv);
        // 내 PC(X-17): 드라이브 컬럼(이름·종류·전체·여유) ↔ 일반 컬럼 전환.
        // 전환 시점에만 교체(평시 set_columns 재호출로 사용자 폭 리셋 방지).
        if !self.base_columns.is_empty() {
            use crate::source::COL_TOTAL;
            let virt =
                nexa_vfs::is_virtual_root(self.tabs[self.active].rows.source().tree().root_path());
            let has_drive = self.tabs[self.active]
                .rows
                .columns()
                .iter()
                .any(|c| c.key == COL_TOTAL);
            if virt != has_drive {
                let cols = if virt {
                    self.drive_columns()
                } else {
                    self.base_columns.clone()
                };
                self.tabs[self.active].rows.set_columns(cols, inv);
            }
        }
        self.apply_sort_opts(self.active); // 정렬 옵션 전파(07-15 — 탐색/재로드 유지)
        self.session_dirty = true; // 경로/구성 변경 — 디바운스 세션 저장(07-15)
        let entries: Vec<PathBuf> = self.tabs[self.active].expanded.values().cloned().collect();
        let tree = self.rows_mut().source_mut().tree_mut();
        for p in &entries {
            let _ = tree.expand_path(&p.to_string_lossy()); // 키 사전순 = 부모 우선
        }
        self.sync_chrome(inv);
    }

    /// 이름변경을 펼침 집합에 반영(원본 UpdateExpandedPaths) — 폴더 자신+하위 접두사 치환.
    pub fn rename_expanded(&mut self, old: &Path, new: &Path) {
        let ok = expand_key(old);
        let new_str = new
            .to_string_lossy()
            .trim_end_matches(['\\', '/'])
            .to_string();
        let old_len = old.to_string_lossy().trim_end_matches(['\\', '/']).len();
        let set = &mut self.tabs[self.active].expanded;
        let affected: Vec<String> = set
            .keys()
            .filter(|k| {
                k.as_str() == ok
                    || k.starts_with(&format!("{ok}\\"))
                    || k.starts_with(&format!("{ok}/"))
            })
            .cloned()
            .collect();
        for k in affected {
            let Some(v) = set.remove(&k) else { continue };
            let vs = v.to_string_lossy().into_owned();
            let tail = vs.get(old_len..).unwrap_or("");
            let nv = PathBuf::from(format!("{new_str}{tail}"));
            set.insert(expand_key(&nv), nv);
        }
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

    /// 위로(부모 폴더) — Alt+↑. **떠난 폴더를 자동 선택**(G-7 — 원본 F13-1 이식):
    /// 부모 목록에서 방금 떠난 폴더가 캐럿+단일 선택되어 위치 감각을 보존한다.
    /// 드라이브 루트(`C:\`)에서는 **가상 최상위 "내 PC"**로 올라간다(X-17 —
    /// 드라이브 간 이동). 내 PC가 최상위(더 위 없음).
    pub fn nav_up(&mut self, ctx: NavCtx, inv: &mut Invalidations) {
        let left = self.root_path();
        if nexa_vfs::is_virtual_root(&left) {
            return; // 내 PC = 최상위
        }
        match left.parent().map(Path::to_path_buf) {
            Some(parent) if !parent.as_os_str().is_empty() => {
                self.navigate_to(parent, ctx, inv);
                self.select_path(&left, inv);
            }
            _ => {
                self.navigate_to(PathBuf::from(nexa_vfs::MY_PC), ctx, inv);
                self.select_path(&left, inv); // 떠난 드라이브 선택(G-7 일관)
            }
        }
    }

    /// Alt+↑ 자동 선택의 뷰 배치(사용자 QA 07-15 — 설정 `nav_up_align`).
    pub fn set_nav_up_align(&mut self, align: nexa_gui::widgets::ScrollAlign) {
        self.nav_up_align = align;
    }

    /// 폴더 우선 정렬 토글(G-13) — 전 탭 소스에 전파·즉시 재정렬(+새 소스용 보관).
    pub fn set_folders_first(&mut self, on: bool, inv: &mut Invalidations) {
        self.sort_folders_first = on;
        for t in &mut self.tabs {
            t.rows.source_mut().set_folders_first(on);
            inv.push(t.rows.bounds());
        }
    }

    /// 대소문자 구분 정렬 토글(사용자 요청 07-15) — 전 탭 소스에 전파·즉시 재정렬(+보관).
    pub fn set_sort_case(&mut self, on: bool, inv: &mut Invalidations) {
        self.sort_case = on;
        for t in &mut self.tabs {
            t.rows.source_mut().set_case_sensitive(on);
            inv.push(t.rows.bounds());
        }
    }

    /// 세션 저장 요청 수거(1회성 — 호스트 update_status가 폴링해 디바운스 타이머 무장).
    pub fn take_session_dirty(&mut self) -> bool {
        std::mem::take(&mut self.session_dirty)
    }

    /// 새로 만든 소스에 보관된 정렬·타입어헤드 옵션 적용(탐색·재로드·새 탭 공통 — 07-15).
    fn apply_sort_opts(&mut self, tab: usize) {
        let (sf, sc) = (self.sort_folders_first, self.sort_case);
        let (scope, reset, special, space, bs, hud) = self.ta_opts;
        let mut inv = Invalidations::default();
        {
            let src = self.tabs[tab].rows.source_mut();
            src.set_folders_first(sf);
            src.set_case_sensitive(sc);
            src.set_find_scope(scope);
        }
        self.tabs[tab]
            .rows
            .set_typeahead_opts(reset, special, space, bs, hud, &mut inv);
        let (fb, hb, hi) = self.font_decor;
        self.tabs[tab].rows.set_font_decor(fb, hb, hi, &mut inv); // X-12 — 새 탭 계승
                                                                  // 보기 모드는 재적용 불요(07-16 개정) — VirtualRows가 탭별로 보존.
    }

    /// 폰트 장식 적용(X-12 — 설정): 전 탭 + 보관(새 탭 계승).
    pub fn set_font_decor(
        &mut self,
        folder_bold: bool,
        hdr_bold: bool,
        hdr_italic: bool,
        inv: &mut Invalidations,
    ) {
        self.font_decor = (folder_bold, hdr_bold, hdr_italic);
        for t in &mut self.tabs {
            t.rows
                .set_font_decor(folder_bold, hdr_bold, hdr_italic, inv);
        }
    }

    /// 보기 모드 전환(사용자 요청 07-16 개정: **탭별 설정**) — **활성 탭에만** 적용.
    /// 트리 이탈 시 펼침을 접어 목록을 현재 폴더로 평탄화(일반/타일 의미론).
    /// 모드는 VirtualRows(탭별 위젯)에 저장 — 탐색/재로드에도 자연 유지.
    pub fn set_view_mode(&mut self, mode: nexa_gui::widgets::ViewMode, inv: &mut Invalidations) {
        Self::apply_tab_view_mode(&mut self.tabs[self.active], mode, inv);
        self.session_dirty = true; // 탭별 모드 = 세션 영속(07-16)
    }

    fn apply_tab_view_mode(
        tab: &mut Tab,
        mode: nexa_gui::widgets::ViewMode,
        inv: &mut Invalidations,
    ) {
        if tab.rows.view_mode() == mode {
            return;
        }
        if mode != nexa_gui::widgets::ViewMode::Tree {
            tab.rows.source_mut().tree_mut().collapse_all();
        }
        tab.rows.set_view_mode(mode, inv);
        inv.push(tab.rows.bounds());
    }

    /// 활성 탭의 보기 모드(도구 모음/메뉴 라디오 동기 — 07-16).
    pub fn active_view_mode(&self) -> nexa_gui::widgets::ViewMode {
        self.tabs[self.active].rows.view_mode()
    }

    /// 세션 스냅샷 — 탭별 보기 모드 문자열(07-16).
    pub fn session_modes(&self) -> Vec<String> {
        self.tabs
            .iter()
            .map(|t| {
                match t.rows.view_mode() {
                    nexa_gui::widgets::ViewMode::Flat => "flat",
                    nexa_gui::widgets::ViewMode::Tiles => "tiles",
                    nexa_gui::widgets::ViewMode::Tree => "tree",
                }
                .to_string()
            })
            .collect()
    }

    /// 세션 복원 — 탭 인덱스 정렬(부족분 = 기본 유지). `fallback` = 세션에 없을 때
    /// 전 탭 기본(settings.view_mode — 구세션/신규 탭 호환).
    pub fn seed_modes(
        &mut self,
        modes: &[String],
        fallback: nexa_gui::widgets::ViewMode,
        inv: &mut Invalidations,
    ) {
        for (i, t) in self.tabs.iter_mut().enumerate() {
            let m = match modes.get(i).map(String::as_str) {
                Some("flat") => nexa_gui::widgets::ViewMode::Flat,
                Some("tiles") => nexa_gui::widgets::ViewMode::Tiles,
                Some(_) => nexa_gui::widgets::ViewMode::Tree,
                None => fallback,
            };
            Self::apply_tab_view_mode(t, m, inv);
        }
    }

    /// 내 PC 드라이브 컬럼 세트(X-17 — 탐색기: 이름·종류·전체 크기·사용 가능한 공간).
    /// 폭은 기본 컬럼에서 차용(이름=기본 이름 폭·나머지=크기 폭 계열).
    fn drive_columns(&self) -> Vec<Column> {
        use crate::i18n::tr;
        use crate::source::{COL_FREE, COL_KIND, COL_NAME, COL_TOTAL};
        let w_of = |key: u32, fallback: i32| {
            self.base_columns
                .iter()
                .find(|c| c.key == key)
                .map(|c| c.width)
                .unwrap_or(fallback)
        };
        vec![
            Column::new(COL_NAME, tr("col.name"), w_of(COL_NAME, 340)),
            Column::new(COL_KIND, tr("col.kind"), w_of(COL_KIND, 110)),
            Column::new(COL_TOTAL, tr("col.total"), 110).right_aligned(),
            Column::new(COL_FREE, tr("col.free"), 130).right_aligned(),
        ]
    }

    /// 타입어헤드 옵션 적용(설정 — 전 탭 + 새 소스용 보관, 07-15).
    #[allow(clippy::too_many_arguments)] // 설정 6값 전달(구조체화는 후속)
    pub fn set_typeahead_opts(
        &mut self,
        scope: nexa_tree::FindScope,
        reset_ms: u64,
        special: bool,
        space: bool,
        backspace: bool,
        hud_pos: u8,
        inv: &mut Invalidations,
    ) {
        self.ta_opts = (scope, reset_ms, special, space, backspace, hud_pos);
        for t in &mut self.tabs {
            t.rows.source_mut().set_find_scope(scope);
            t.rows
                .set_typeahead_opts(reset_ms, special, space, backspace, hud_pos, inv);
        }
    }

    /// 가시 목록에서 경로가 일치하는 행을 캐럿+단일 선택(G-7 — 최상위 자식은 항상 가시).
    fn select_path(&mut self, path: &Path, inv: &mut Invalidations) {
        let row = {
            let tree = self.rows().source().tree();
            (0..tree.visible_len()).find(|&i| {
                tree.visible_id(i)
                    .and_then(|id| tree.node_path(id))
                    .is_some_and(|p| p == path)
            })
        };
        if let Some(i) = row {
            let align = self.nav_up_align;
            self.rows_mut().select_program_aligned(
                i,
                nexa_gui::widgets::SelectOp::Single,
                align,
                inv,
            );
        }
    }

    /// 행 활성화(더블클릭·Enter·Alt+↓ — 원본 F19): 폴더=진입, **파일=경로 반환**
    /// (실행은 호스트 몫 — ShellExecute는 플랫폼 종속·위젯/패널은 중립 유지).
    pub fn activate_row(
        &mut self,
        row: usize,
        ctx: NavCtx,
        inv: &mut Invalidations,
    ) -> Option<PathBuf> {
        let tree = self.rows().source().tree();
        let (Some(r), Some(id)) = (tree.row(row), tree.visible_id(row)) else {
            return None;
        };
        let path = tree.node_path(id).map(Path::to_path_buf)?;
        if r.kind == FileKind::Dir {
            self.navigate_to(path, ctx, inv);
            None
        } else {
            Some(path) // 파일 — 호스트가 실행(QA 07-14)
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
        // 1) 스냅샷(경로 기준 — 재열기 후 인덱스/ID는 무효). 펼침은 탭 영속 집합(F18)이
        //    apply_source에서 동기·복원하므로 여기선 선택·캐럿·스크롤만.
        let (selected, caret_path, scroll_row, scroll_x) = {
            let rows = self.rows();
            let tree = rows.source().tree();
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
            (selected, caret_path, rows.scroll_row(), rows.scroll_x())
        };
        // 2) 재열기(펼침 복원 포함)
        self.tabs[self.active].nav.replace(path);
        self.apply_source(src, inv);
        // 3) 복원 — 선택·캐럿·스크롤
        let rows = self.rows_mut();
        let tree = rows.source_mut().tree_mut();
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
                // 도크 밖 클릭 = 도크 텍스트 선택 해제(QA 07-15 — Ctrl+C 우선순위 복원)
                if self.dock_visible && y < self.dock.bounds().y {
                    self.dock.clear_text_selection(inv);
                }
                if y < self.pathbar.bounds().y {
                    self.tabbar.on_event(ev, inv);
                } else if y < self.rows().bounds().y {
                    // 네비 바 행: [←][→][↑] | 경로 바
                    if x < self.pathbar.bounds().x {
                        self.navbtns.on_event(ev, inv);
                    } else {
                        self.pathbar.on_event(ev, inv);
                    }
                } else if self.dock_visible && y >= self.dock.bounds().y {
                    self.dock.on_event(ev, inv); // 종류 스트립 전환(M4-2)
                } else {
                    self.tabs[self.active].rows.on_event(ev, inv);
                }
            }
            InputEvent::MouseMove { .. } => {
                self.tabbar.on_event(ev, inv);
                self.navbtns.on_event(ev, inv);
                self.pathbar.on_event(ev, inv);
                self.tabs[self.active].rows.on_event(ev, inv);
                if self.dock_visible {
                    self.dock.on_event(ev, inv); // 내용 라인 드래그 선택(QA 07-15)
                }
            }
            InputEvent::MouseUp { .. } => {
                // 경로바 편집 드래그 선택 종료(QA 07-13) + 리스트 밴드/리사이즈 종료 +
                // 탭 드래그 재정렬 종료(QA 07-14 — 미전달 시 드래그 상태 잔존 결함)
                self.tabbar.on_event(ev, inv);
                self.pathbar.on_event(ev, inv);
                self.tabs[self.active].rows.on_event(ev, inv);
                if self.dock_visible {
                    self.dock.on_event(ev, inv); // 도크 선택 드래그 종료(QA 07-15)
                }
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
                TabAction::Move { from, to } => self.move_tab(from, to, inv),
                TabAction::Context(i) => self.pending_tab_menu = Some(i),
            }
        }
        if let Some(p) = self.pathbar.take_navigation() {
            // 환경변수 해석(원본 PathInterpreter — %VAR%·$env:VAR·따옴표, PATH-SUG 동반)
            let p = crate::pathinput::expand_env(&p);
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
        .map(|(id, g)| ToolButton::new(id, g))
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

    #[test]
    fn sort_opts_propagate_to_new_sources() {
        // 07-15 전파 수정: 대소문자 정렬이 토글 즉시·재로드·새 탭에서 유지되는가
        let base = std::env::temp_dir().join(format!("nexa_panel_case_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join("abb.txt"), b"x").unwrap();
        fs::write(base.join("Abc.txt"), b"x").unwrap();
        let (mut p, mut inv) = panel(&base);
        let first = |p: &Panel| p.rows().source().tree().row(0).unwrap().name;
        assert_eq!(first(&p), "abb.txt", "기본 = 대소문자 무시(abb < abc)");
        p.set_sort_case(true, &mut inv);
        assert_eq!(first(&p), "Abc.txt", "토글 즉시 대문자 그룹 상단");
        p.reopen_filtered(ctx(), &mut inv);
        assert_eq!(first(&p), "Abc.txt", "재로드 후 유지");
        p.new_tab(ctx(), &mut inv);
        assert_eq!(first(&p), "Abc.txt", "새 탭에도 유지");
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn session_dirty_flag_on_tab_ops() {
        // 07-15 세션 자동 저장: 탭 변경 = 플래그 1회성(디바운스 flush는 호스트 몫)
        let base = fixture("dirty");
        let (mut p, mut inv) = panel(&base);
        assert!(!p.take_session_dirty(), "초기 = 깨끗");
        p.new_tab(ctx(), &mut inv);
        assert!(p.take_session_dirty(), "새 탭 = 더티");
        assert!(!p.take_session_dirty(), "수거 후 = 리셋(1회성)");
        p.switch_tab(0, &mut inv);
        assert!(p.take_session_dirty(), "탭 전환 = 더티");
        p.toggle_tab_pin(0, &mut inv);
        assert!(p.take_session_dirty(), "고정 = 더티");
        p.close_tab(1, &mut inv);
        assert!(p.take_session_dirty(), "닫기 = 더티");
        fs::remove_dir_all(&base).unwrap();
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

    #[cfg(windows)]
    #[test]
    fn nav_up_from_drive_root_enters_my_pc() {
        // X-17: 드라이브 루트에서 위로 = 가상 최상위 "내 PC"·떠난 드라이브 선택(G-7)
        let (mut p, mut inv) = panel(Path::new("C:\\"));
        p.nav_up(ctx(), &mut inv);
        assert!(
            nexa_vfs::is_virtual_root(p.root_path()),
            "root = {:?}",
            p.root_path()
        );
        let tree = p.rows().source().tree();
        assert!(tree.visible_len() >= 1, "드라이브 목록");
        let i = tree.index_of_path("C:\\").expect("C: 드라이브 행");
        let id = tree.visible_id(i).unwrap();
        assert!(tree.is_selected(id), "떠난 드라이브 자동 선택(G-7)");
        // 내 PC에서 다시 위로 = 무동작(최상위)
        p.nav_up(ctx(), &mut inv);
        assert!(nexa_vfs::is_virtual_root(p.root_path()));
        // 뒤로 = 드라이브 루트 복귀(히스토리 경유)
        p.nav_back(ctx(), &mut inv);
        assert_eq!(p.root_path(), PathBuf::from("C:\\"));
    }

    #[test]
    fn view_mode_propagates_and_flattens() {
        // 보기 모드(07-16): 타일/일반 진입 = 펼침 평탄화·새 소스에도 모드 계승
        use nexa_gui::widgets::ViewMode;
        let base = fixture("viewmode");
        fs::create_dir_all(base.join("sub").join("inner")).unwrap();
        let (mut p, mut inv) = panel(&base);
        p.rows_mut()
            .source_mut()
            .tree_mut()
            .expand_path(&base.join("sub").to_string_lossy())
            .unwrap();
        let before = p.rows().source().tree().visible_len();
        p.set_view_mode(ViewMode::Tiles, &mut inv);
        assert!(
            p.rows().source().tree().visible_len() < before,
            "타일 진입 = 펼침 평탄화"
        );
        assert_eq!(p.rows().view_mode(), ViewMode::Tiles);
        // 새 소스(탐색)에도 모드 계승(apply_sort_opts 경유)
        p.navigate_to(base.join("sub"), ctx(), &mut inv);
        assert_eq!(p.rows().view_mode(), ViewMode::Tiles, "탐색 후 모드 유지");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn view_mode_is_per_tab() {
        // 07-16 개정: 보기 모드는 탭별 — 새 탭 계승·탭 간 독립·세션 스냅샷
        use nexa_gui::widgets::ViewMode;
        let base = fixture("viewmode_tab");
        let (mut p, mut inv) = panel(&base);
        p.set_view_mode(ViewMode::Tiles, &mut inv);
        p.new_tab(ctx(), &mut inv); // 새 탭(활성) = 현재 탭 계승
        assert_eq!(p.active_view_mode(), ViewMode::Tiles, "새 탭 계승");
        p.set_view_mode(ViewMode::Flat, &mut inv); // 탭1만 변경
        assert_eq!(p.active_view_mode(), ViewMode::Flat);
        p.switch_tab(0, &mut inv);
        assert_eq!(p.active_view_mode(), ViewMode::Tiles, "탭0 독립 유지");
        assert_eq!(
            p.session_modes(),
            vec!["tiles".to_string(), "flat".to_string()],
            "세션 스냅샷 = 탭별"
        );
        // 세션 복원 경로 — 부족분은 fallback
        let mut q = panel(&base).0;
        q.new_tab(ctx(), &mut inv);
        q.seed_modes(&["flat".into()], ViewMode::Tiles, &mut inv);
        assert_eq!(
            q.session_modes(),
            vec!["flat".to_string(), "tiles".to_string()],
            "복원: 세션 값 우선·부족분 fallback"
        );
        let _ = fs::remove_dir_all(&base);
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
    fn sibling_expansion_survives_enter_and_return() {
        // 사용자 QA 07-14: A·B 모두 펼침 → B 진입 → 상위 복귀 시 A 펼침이 소실되던 결함
        // (가시 트리 스냅샷 한계) — F18 탭별 영속 집합으로 유지(X-4).
        let base = fixture("sibexp");
        fs::create_dir_all(base.join("A000").join("a1")).unwrap();
        fs::create_dir_all(base.join("B000").join("b1")).unwrap();
        let (mut p, mut inv) = panel(&base);
        let (a, b) = (base.join("A000"), base.join("B000"));
        {
            let tree = p.rows_mut().source_mut().tree_mut();
            tree.expand_path(&a.to_string_lossy()).unwrap();
            tree.expand_path(&b.to_string_lossy()).unwrap();
        }
        p.navigate_to(b.clone(), ctx(), &mut inv); // B 진입(A는 트리 밖)
        p.nav_back(ctx(), &mut inv); // 상위 복귀
        let tree = p.rows().source().tree();
        for q in [&a, &b] {
            let i = tree
                .index_of_path(&q.to_string_lossy())
                .unwrap_or_else(|| panic!("{} 가시", q.display()));
            let id = tree.visible_id(i).unwrap();
            assert_eq!(
                tree.is_expanded(id),
                Some(true),
                "{} 펼침 유지",
                q.display()
            );
        }
        // 접기 후 이동 왕복 = 접힘 유지(말소 동작 — 부활 없음)
        {
            let tree = p.rows_mut().source_mut().tree_mut();
            let i = tree.index_of_path(&a.to_string_lossy()).unwrap();
            let id = tree.visible_id(i).unwrap();
            tree.collapse(id);
        }
        p.navigate_to(b, ctx(), &mut inv);
        p.nav_back(ctx(), &mut inv);
        let tree = p.rows().source().tree();
        let i = tree.index_of_path(&a.to_string_lossy()).unwrap();
        let id = tree.visible_id(i).unwrap();
        assert_eq!(
            tree.is_expanded(id),
            Some(false),
            "접은 폴더는 접힌 채 유지"
        );
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn restore_seeds_expanded_from_session() {
        // F18 세션 영속 — restore(expanded)가 탭 트리에 재적용(부모 우선)
        let base = fixture("sessexp");
        fs::create_dir_all(base.join("A000").join("a1").join("a2")).unwrap();
        let a = base.join("A000");
        let a1 = a.join("a1");
        let p = Panel::restore(
            std::slice::from_ref(&base),
            0,
            &[vec![a.clone(), a1.clone()]],
            &base,
            ctx(),
            metrics(),
            Vec::new(),
        );
        let tree = p.rows().source().tree();
        for q in [&a, &a1] {
            let i = tree
                .index_of_path(&q.to_string_lossy())
                .unwrap_or_else(|| panic!("{} 가시", q.display()));
            let id = tree.visible_id(i).unwrap();
            assert_eq!(
                tree.is_expanded(id),
                Some(true),
                "{} 세션 복원 펼침",
                q.display()
            );
        }
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn tab_move_lock_duplicate() {
        // 편의 UX ② — 드래그 재정렬(활성 추종)·잠금=닫기 거부·복제=경로+펼침 복사
        let base = fixture("tabux");
        let (mut p, mut inv) = panel(&base);
        p.new_tab(ctx(), &mut inv); // 탭 2개, 활성=1
        p.navigate_to(base.join("sub"), ctx(), &mut inv); // 탭1 = sub
        p.move_tab(1, 0, &mut inv); // 활성 탭을 맨 앞으로
        assert_eq!(p.active_index(), 0, "이동한 활성 탭 추종");
        assert!(p.root_path().ends_with("sub"));
        p.toggle_tab_lock(0, &mut inv);
        assert!(p.tab_locked(0));
        p.close_tab(0, &mut inv);
        assert_eq!(p.tab_count(), 2, "잠긴 탭은 닫기 거부");
        assert_eq!(p.session_locked(), vec![true, false]);
        p.duplicate_tab(0, ctx(), &mut inv);
        assert_eq!(p.tab_count(), 3);
        assert_eq!(p.active_index(), 1, "복제본이 바로 옆·활성");
        assert!(p.root_path().ends_with("sub"), "복제 = 같은 경로");
        assert!(!p.tab_locked(1), "복제본은 잠금 해제");
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
