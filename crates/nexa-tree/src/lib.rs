//! nexa-tree — 인라인 트리 + 교차 선택 모델(가시 노드 평면 스트림, C1).
//!
//! 트리를 **가시 노드의 평면 스트림**(`VisibleRow`)으로 투영해 가상화 렌더 + 빠른 선택을
//! 동시에 달성한다(설계: docs/07 · ADR-0004 docs/29). UI 비종속 순수 로직 → 맥 단위테스트.
//!
//! 슬라이스 1(이 크레이트): open/expand/collapse/가시행/선택(OrderedSet). ABI·앱은 후속 슬라이스.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use nexa_core::FileKind;
use nexa_vfs::read_dir_entries;

/// 트리 세션 내 안정 식별자(삽입 순번, 회수하지 않음 → arena 인덱스와 동일).
pub type NodeId = u64;

/// 트리 노드(arena 저장). `children`은 `loaded == true`일 때만 유효(정렬된 순서).
#[derive(Debug, Clone)]
struct Node {
    id: NodeId,
    /// 부모(최상위는 `None`). 타입어헤드 `CurrentLevel`(형제 스코프)·경로변동 추적에 사용.
    parent: Option<NodeId>,
    path: PathBuf,
    name: String,
    kind: FileKind,
    depth: u32,
    size: u64,
    modified_unix_ms: i64,
    attrs: u32,
    expanded: bool,
    loaded: bool,
    children: Vec<NodeId>,
}

impl Node {
    fn is_dir(&self) -> bool {
        self.kind == FileKind::Dir
    }
}

/// UI로 흘려보내는 가시 행 단위(코어→호스트).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibleRow {
    pub id: NodeId,
    pub depth: u32,
    pub kind: FileKind,
    pub name: String,
    pub size: u64,
    pub modified_unix_ms: i64,
    pub attrs: u32,
    pub expanded: bool,
    /// 펼칠 수 있는가(디렉터리). 심링크는 슬라이스 1에서 펼침 대상 아님.
    pub has_children: bool,
}

/// 펼침/접힘으로 인한 가시 목록 변경 구간(호스트가 행 삽입/삭제에 사용).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RangeChange {
    pub start: usize,
    pub removed: usize,
    pub inserted: usize,
}

impl RangeChange {
    /// 변경 없음.
    pub const NONE: RangeChange = RangeChange {
        start: 0,
        removed: 0,
        inserted: 0,
    };
}

/// 선택 갱신 방식.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectMode {
    /// 단일 선택(기존 해제) + anchor 갱신.
    Single,
    /// 비연속 토글(다중) + anchor 갱신.
    Toggle,
}

/// 정렬 키(컬럼) — **실제 필드**로 비교(표시 텍스트가 아님: 크기/날짜는 숫자).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Ext,
    Size,
    Modified,
    Kind,
    /// 정렬 없음 = 원래 **열거 순서**(children id 오름차순 복원).
    None,
}

/// 정렬 사양(패널별 독립, docs/23 §3-1). `keys`는 1차→2차… 우선순위이며 각 키에 `desc`.
/// `folders_first`면 방향과 무관하게 **폴더를 앞에 모은다**(탐색기 규약).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortSpec {
    /// (키, 내림차순 여부). 빈 목록 또는 `None` 키 = 열거 순서.
    pub keys: Vec<(SortKey, bool)>,
    pub folders_first: bool,
    /// 대소문자 구분 이름/확장자 비교(07-15 — 기본 false. 코드포인트 순 = 대문자 그룹 상단).
    pub case_sensitive: bool,
}

impl SortSpec {
    /// 기본: **폴더 우선 + 이름 오름차순**(기존 고정 동작과 동일).
    pub fn name_asc() -> SortSpec {
        SortSpec {
            keys: vec![(SortKey::Name, false)],
            folders_first: true,
            case_sensitive: false,
        }
    }
}

impl Default for SortSpec {
    fn default() -> Self {
        SortSpec::name_asc()
    }
}

/// 타입어헤드 찾기 범위(docs/32 §5·§8). 하나의 매칭 함수를 파라미터화한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindScope {
    /// A: 가시 스트림 처음부터 첫 매치(캐럿 무시).
    GlobalFirst,
    /// B: 캐럿의 같은 부모(형제)인 가시 행만, 위치상대+wrap.
    CurrentLevel,
    /// C(기본): 캐럿 다음부터 가시 스트림 starts-with, 끝이면 wrap.
    VisibleStream,
}

/// Windows 숨김 속성 비트(FILE_ATTRIBUTE_HIDDEN).
const ATTR_HIDDEN: u32 = 0x2;

/// 가시성 필터(숨김 속성·점 파일). 앱 `ViewOptions`와 동일 개념(둘 다 "보기").
/// 열거 시 적용 — 걸러진 항목은 트리에 아예 생성하지 않는다.
#[derive(Debug, Clone, Copy)]
struct Filter {
    show_hidden: bool,
    show_dotfiles: bool,
}

impl Filter {
    fn allows(&self, name: &str, attrs: u32) -> bool {
        if !self.show_dotfiles && name.starts_with('.') {
            return false;
        }
        if !self.show_hidden && (attrs & ATTR_HIDDEN) != 0 {
            return false;
        }
        true
    }
}

/// 인라인 트리 + 선택 상태. 임의 부모의 노드를 함께 선택할 수 있다(교차 선택).
#[derive(Debug)]
pub struct Tree {
    nodes: Vec<Node>,
    roots: Vec<NodeId>,
    visible: Vec<NodeId>,
    sel_order: Vec<NodeId>, // OrderedSet: 삽입 순서 보존
    sel_set: HashSet<NodeId>,
    anchor: Option<NodeId>,
    root_path: PathBuf,
    filter: Filter,
    sort: SortSpec,
}

impl Tree {
    /// `path`를 열어 최상위(depth 0) 항목을 열거한 트리를 만든다(펼침 없음, **모두 표시**).
    pub fn open(path: impl AsRef<Path>) -> io::Result<Tree> {
        Tree::open_filtered(path, true, true)
    }

    /// 가시성 필터를 적용해 연다. `show_hidden`=Windows 숨김 속성, `show_dotfiles`=점(.) 파일.
    /// 걸러진 항목은 트리에 생성되지 않으므로 펼침 시 자식도 동일 필터가 적용된다.
    pub fn open_filtered(
        path: impl AsRef<Path>,
        show_hidden: bool,
        show_dotfiles: bool,
    ) -> io::Result<Tree> {
        let root_path = path.as_ref().to_path_buf();
        let mut tree = Tree {
            nodes: Vec::new(),
            roots: Vec::new(),
            visible: Vec::new(),
            sel_order: Vec::new(),
            sel_set: HashSet::new(),
            anchor: None,
            root_path: root_path.clone(),
            filter: Filter {
                show_hidden,
                show_dotfiles,
            },
            sort: SortSpec::default(),
        };
        let roots = tree.enumerate(&root_path, None, 0)?;
        tree.roots.clone_from(&roots);
        tree.visible = roots;
        Ok(tree)
    }

    /// 열린 루트 경로.
    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    /// 최상위 항목 수(펼침과 무관).
    pub fn root_count(&self) -> usize {
        self.roots.len()
    }

    // ── 열거 ──────────────────────────────────────────────────

    /// `dir`의 자식을 열거해 arena에 추가하고 정렬된 id 목록을 반환한다.
    /// 엔트리 단위 오류는 격리(해당 항목만 건너뜀).
    fn enumerate(
        &mut self,
        dir: &Path,
        parent: Option<NodeId>,
        depth: u32,
    ) -> io::Result<Vec<NodeId>> {
        let mut ids = Vec::new();
        for entry in read_dir_entries(dir)? {
            let Ok(e) = entry else { continue };
            if !self.filter.allows(&e.name, e.attrs) {
                continue; // 숨김/점 파일 필터(트리에 아예 생성 안 함)
            }
            let id = self.nodes.len() as NodeId;
            let path = dir.join(&e.name);
            self.nodes.push(Node {
                id,
                parent,
                path,
                name: e.name,
                kind: e.kind,
                depth,
                size: e.size,
                modified_unix_ms: to_unix_ms(e.modified),
                attrs: e.attrs,
                expanded: false,
                loaded: false,
                children: Vec::new(),
            });
            ids.push(id);
        }
        self.sort_ids(&mut ids);
        Ok(ids)
    }

    /// 현재 `self.sort` 사양으로 id 슬라이스를 정렬(폴더 우선 → 키 순서 → 이름·열거 tie-break).
    fn sort_ids(&self, ids: &mut [NodeId]) {
        ids.sort_by(|&a, &b| self.cmp_nodes(a, b));
    }

    /// 두 노드의 정렬 순서. `folders_first` 그룹핑 → 키 순차 비교(각 asc/desc) →
    /// 남으면 이름·id로 안정 tie-break. 빈 키/`None` 키 = **열거 순서(id)**.
    fn cmp_nodes(&self, a: NodeId, b: NodeId) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        let na = &self.nodes[a as usize];
        let nb = &self.nodes[b as usize];
        if self.sort.folders_first {
            let g = nb.is_dir().cmp(&na.is_dir()); // 폴더(true) 먼저
            if g != Ordering::Equal {
                return g;
            }
        }
        if self.sort.keys.is_empty() {
            return a.cmp(&b); // 정렬 없음 = 열거(id) 순서
        }
        for &(key, desc) in &self.sort.keys {
            if key == SortKey::None {
                return a.cmp(&b); // 열거 순서(방향 무시)
            }
            // 대소문자 구분 옵션(07-15) — 알파벳 순서는 유지하되 같은 이름은 대문자 우선
            let cmp_name = |x: &str, y: &str| {
                if self.sort.case_sensitive {
                    cmp_cs_upper_first(x, y)
                } else {
                    cmp_ci(x, y)
                }
            };
            let ord = match key {
                SortKey::Name => cmp_name(&na.name, &nb.name),
                SortKey::Ext => cmp_name(ext_of(&na.name), ext_of(&nb.name)),
                // 폴더 크기는 OS 잡음값 → 정렬상 0으로 정규화(폴더는 이름 tie-break로, 탐색기와 동일).
                SortKey::Size => {
                    let sa = if na.is_dir() { 0 } else { na.size };
                    let sb = if nb.is_dir() { 0 } else { nb.size };
                    sa.cmp(&sb)
                }
                SortKey::Modified => na.modified_unix_ms.cmp(&nb.modified_unix_ms),
                // "종류" = 파일 타입. kind_rank만으론 folders_first 뒤 파일이 전부 동률(File)→asc=desc가 됨.
                // 셸 타입 문자열이 없으므로 확장자를 타입 대용으로 2차 비교(파일 간 변별력 + 방향 반영).
                SortKey::Kind => kind_rank(na.kind)
                    .cmp(&kind_rank(nb.kind))
                    .then_with(|| cmp_name(ext_of(&na.name), ext_of(&nb.name))),
                SortKey::None => Ordering::Equal, // 위에서 처리(도달 안 함)
            };
            let ord = if desc { ord.reverse() } else { ord };
            if ord != Ordering::Equal {
                return ord;
            }
        }
        // 모든 키 동률 → 이름, 그다음 id(안정·열거 순서)
        cmp_ci(&na.name, &nb.name).then(a.cmp(&b))
    }

    /// 현재 정렬 사양(호스트/테스트 조회용).
    pub fn sort_spec(&self) -> &SortSpec {
        &self.sort
    }

    /// 정렬 사양을 바꾸고 **로드된 모든 폴더의 자식 + 가시 목록을 재구성**한다(펼침 상태 보존).
    /// children id는 열거 순서로 보존되므로 `None`이면 그 순서가 그대로 복원된다(docs/23 §4-1 COL-2a).
    pub fn set_sort(&mut self, spec: SortSpec) {
        self.sort = spec;
        // 루트 재정렬.
        let mut roots = std::mem::take(&mut self.roots);
        self.sort_ids(&mut roots);
        self.roots = roots;
        // 로드된 각 폴더의 children 재정렬(2개 미만은 순서 불변).
        for i in 0..self.nodes.len() {
            if self.nodes[i].loaded && self.nodes[i].children.len() > 1 {
                let mut ch = std::mem::take(&mut self.nodes[i].children);
                self.sort_ids(&mut ch);
                self.nodes[i].children = ch;
            }
        }
        self.rebuild_visible();
    }

    /// 현재 펼침 상태를 따라 roots→DFS로 가시 목록을 다시 만든다(정렬 변경 후 호출).
    fn rebuild_visible(&mut self) {
        let roots = std::mem::take(&mut self.roots); // 빌림 회피 — clone 대신 이동 후 복귀
        let mut vis = Vec::with_capacity(self.visible.len());
        for &r in &roots {
            vis.push(r);
            let n = &self.nodes[r as usize];
            if n.is_dir() && n.expanded {
                self.collect_subtree(r, &mut vis);
            }
        }
        self.roots = roots;
        self.visible = vis;
    }

    // ── 타입어헤드 찾기 (docs/32) ───────────────────────────────

    /// 타입어헤드 접두사 매칭 — `prefix`(대소문자 무시)로 시작하는 **가시 행**의 인덱스(없으면 `None`).
    /// 이름 starts-with만(경로 아님, 정렬 `cmp_ci`와 동일 규약). `caret`=현재 캐럿 가시 인덱스(범위 밖/`None`=처음부터).
    /// - `VisibleStream`(C): `caret+1`부터 앞으로, 끝이면 `0..=caret`로 wrap(계속 입력 시 다음 매치로 cycle).
    /// - `GlobalFirst`(A): 0부터 첫 매치(캐럿 무시).
    /// - `CurrentLevel`(B): 캐럿과 **같은 부모(형제)** 인 가시 행만 대상으로 C 규칙.
    pub fn find_prefix(
        &self,
        caret: Option<usize>,
        prefix: &str,
        scope: FindScope,
    ) -> Option<usize> {
        let n = self.visible.len();
        if prefix.is_empty() || n == 0 {
            return None;
        }
        let lower = prefix.to_lowercase();
        // 노드 이름을 통째로 소문자화(힙 할당)하지 않고 접두사 길이만큼만 문자 단위 비교 —
        // 키 입력마다 가시 행 전체를 스캔하는 인터랙티브 경로라 할당 제로가 중요.
        let starts = |idx: usize| -> bool {
            let id = self.visible[idx];
            let mut name = self.nodes[id as usize]
                .name
                .chars()
                .flat_map(char::to_lowercase);
            lower.chars().all(|pc| name.next() == Some(pc))
        };
        let caret = caret.filter(|&c| c < n);
        match scope {
            FindScope::GlobalFirst => (0..n).find(|&i| starts(i)),
            FindScope::VisibleStream => {
                let start = caret.map_or(0, |c| c + 1); // caret+1..n → 0..start(=0..=caret) wrap
                (start..n).chain(0..start).find(|&i| starts(i))
            }
            FindScope::CurrentLevel => {
                // 캐럿의 부모(없으면 최상위=None) 형제만. C 규칙(caret+1부터 wrap).
                let parent = caret.and_then(|c| self.nodes[self.visible[c] as usize].parent);
                let start = caret.map_or(0, |c| c + 1);
                (start..n)
                    .chain(0..start)
                    .find(|&i| self.nodes[self.visible[i] as usize].parent == parent && starts(i))
            }
        }
    }

    // ── 가시 스트림 ─────────────────────────────────────────────

    /// 현재 가시 행 수.
    pub fn visible_len(&self) -> usize {
        self.visible.len()
    }

    /// 가시 목록이 비었는가.
    pub fn is_empty(&self) -> bool {
        self.visible.is_empty()
    }

    /// 가시 인덱스의 행. 범위 밖이면 `None`.
    pub fn row(&self, index: usize) -> Option<VisibleRow> {
        let id = *self.visible.get(index)?;
        let n = &self.nodes[id as usize];
        Some(VisibleRow {
            id: n.id,
            depth: n.depth,
            kind: n.kind,
            name: n.name.clone(),
            size: n.size,
            modified_unix_ms: n.modified_unix_ms,
            attrs: n.attrs,
            expanded: n.expanded,
            has_children: n.is_dir(),
        })
    }

    /// 가시 목록에서 `id`의 인덱스(선형 탐색). 단일 Vec 스캔이라 마샬/실체화 없이 빠름
    /// (10만 노드 ≈20µs, 슬라이스 4-1 벤치) → 호스트가 행 재실체화 없이 조회하도록 공개.
    pub fn index_of(&self, id: NodeId) -> Option<usize> {
        self.visible.iter().position(|&x| x == id)
    }

    /// 가시 인덱스의 노드 id(범위 밖이면 `None`) — `row()`와 달리 이름 클론 없이 id만.
    /// 경로/아이콘 조회처럼 id만 필요한 인터롭 경량 경로용.
    pub fn visible_id(&self, index: usize) -> Option<NodeId> {
        self.visible.get(index).copied()
    }

    /// 가시 목록에서 경로가 일치하는 행 인덱스(끝 구분자·대소문자 무시). 없으면 `None`.
    /// 호스트가 행별 경로를 P/Invoke·문자열 복사로 왕복하지 않고 코어에서 직접 매칭하도록 공개
    /// (슬라이스 4-3+ / 감사 P3). 대소문자는 ASCII 무시(Windows 경로 관례, 앱 `OrdinalIgnoreCase`와 정합).
    pub fn index_of_path(&self, target: &str) -> Option<usize> {
        let want = target.trim_end_matches(['\\', '/']);
        self.visible.iter().position(|&id| {
            let p = self.nodes[id as usize].path.to_string_lossy();
            p.trim_end_matches(['\\', '/']).eq_ignore_ascii_case(want)
        })
    }

    /// `id`의 펼침 하위(자식과 그 펼친 후손)를 가시 순서(DFS)로 `out`에 모은다.
    fn collect_subtree(&self, id: NodeId, out: &mut Vec<NodeId>) {
        for &c in &self.nodes[id as usize].children {
            out.push(c);
            let cn = &self.nodes[c as usize];
            if cn.is_dir() && cn.expanded {
                self.collect_subtree(c, out);
            }
        }
    }

    // ── 펼침 / 접힘 ─────────────────────────────────────────────

    /// `id`(디렉터리)를 펼친다. 최초면 지연 열거. 이미 펼쳤거나 디렉터리가 아니거나
    /// 가시 상태가 아니면 무변경(`RangeChange::NONE`). 이전에 접힌 하위의 펼침 상태는 복원한다.
    pub fn expand(&mut self, id: NodeId) -> io::Result<RangeChange> {
        match self.nodes.get(id as usize) {
            Some(n) if n.is_dir() && !n.expanded => {}
            _ => return Ok(RangeChange::NONE),
        }
        let Some(vis) = self.index_of(id) else {
            return Ok(RangeChange::NONE);
        };
        if !self.nodes[id as usize].loaded {
            let path = self.nodes[id as usize].path.clone();
            let depth = self.nodes[id as usize].depth + 1;
            let children = self.enumerate(&path, Some(id), depth)?;
            self.nodes[id as usize].children = children;
            self.nodes[id as usize].loaded = true;
        }
        self.nodes[id as usize].expanded = true;

        let mut sub = Vec::new();
        self.collect_subtree(id, &mut sub);
        let start = vis + 1;
        let inserted = sub.len();
        self.visible.splice(start..start, sub);
        Ok(RangeChange {
            start,
            removed: 0,
            inserted,
        })
    }

    /// 경로로 지정한 가시 폴더를 펼친다(F18 펼침 복원용, 감사 P3). 경로가 가시 목록에 없거나
    /// 이미 펼침/파일이면 `RangeChange::NONE`. 호스트의 O(경로수×가시행) per-row 마샬을 대체.
    pub fn expand_path(&mut self, target: &str) -> io::Result<RangeChange> {
        match self.index_of_path(target) {
            Some(i) => {
                let id = self.visible[i];
                self.expand(id)
            }
            None => Ok(RangeChange::NONE),
        }
    }

    /// `id`를 접는다. 펼침 상태가 아니거나 가시 상태가 아니면 무변경. 하위의 펼침 상태는 보존.
    pub fn collapse(&mut self, id: NodeId) -> RangeChange {
        match self.nodes.get(id as usize) {
            Some(n) if n.expanded => {}
            _ => return RangeChange::NONE,
        }
        let Some(vis) = self.index_of(id) else {
            return RangeChange::NONE;
        };
        let base_depth = self.nodes[id as usize].depth;
        self.nodes[id as usize].expanded = false;

        let start = vis + 1;
        let mut count = 0;
        while let Some(&nid) = self.visible.get(start + count) {
            if self.nodes[nid as usize].depth > base_depth {
                count += 1;
            } else {
                break;
            }
        }
        self.visible.drain(start..start + count);
        RangeChange {
            start,
            removed: count,
            inserted: 0,
        }
    }

    /// `id`의 펼침 여부(범위 밖이면 `None`).
    pub fn is_expanded(&self, id: NodeId) -> Option<bool> {
        self.nodes.get(id as usize).map(|n| n.expanded)
    }

    // ── 선택 (OrderedSet, 교차 폴더 허용) ────────────────────────

    /// 단일/토글 선택 + anchor 갱신.
    pub fn select(&mut self, id: NodeId, mode: SelectMode) {
        match mode {
            SelectMode::Single => {
                self.clear_selection();
                self.add_sel(id);
            }
            SelectMode::Toggle => {
                if self.sel_set.contains(&id) {
                    self.remove_sel(id);
                } else {
                    self.add_sel(id);
                }
            }
        }
        self.anchor = Some(id);
    }

    /// anchor~`id`의 가시 범위 선택(anchor 없으면 단일). anchor는 유지.
    pub fn select_range(&mut self, id: NodeId) {
        let Some(anchor) = self.anchor else {
            self.select(id, SelectMode::Single);
            return;
        };
        let (Some(ia), Some(ib)) = (self.index_of(anchor), self.index_of(id)) else {
            self.select(id, SelectMode::Single);
            return;
        };
        let (lo, hi) = if ia <= ib { (ia, ib) } else { (ib, ia) };
        self.clear_selection();
        for idx in lo..=hi {
            let nid = self.visible[idx];
            self.add_sel(nid);
        }
    }

    /// 현재 가시 노드 전체 선택.
    pub fn select_all_visible(&mut self) {
        self.clear_selection();
        for i in 0..self.visible.len() {
            let nid = self.visible[i];
            self.add_sel(nid);
        }
        self.anchor = self.visible.first().copied();
    }

    /// 선택 해제(anchor는 유지).
    pub fn clear_selection(&mut self) {
        self.sel_order.clear();
        self.sel_set.clear();
    }

    fn add_sel(&mut self, id: NodeId) {
        // FFI 경계 방어: 호스트가 준 범위 밖 id는 무시 — 이후 selected_paths() 등의
        // nodes[id] 직접 인덱싱이 패닉으로 extern "C" 밖까지 unwind(abort)하는 것을 차단.
        if (id as usize) >= self.nodes.len() {
            return;
        }
        if self.sel_set.insert(id) {
            self.sel_order.push(id);
        }
    }

    fn remove_sel(&mut self, id: NodeId) {
        if self.sel_set.remove(&id) {
            if let Some(pos) = self.sel_order.iter().position(|&x| x == id) {
                self.sel_order.remove(pos);
            }
        }
    }

    /// `id`가 선택됐는가.
    pub fn is_selected(&self, id: NodeId) -> bool {
        self.sel_set.contains(&id)
    }

    /// 선택 노드 id(삽입 순서).
    pub fn selected_ids(&self) -> &[NodeId] {
        &self.sel_order
    }

    /// 선택 수.
    pub fn selection_count(&self) -> usize {
        self.sel_order.len()
    }

    /// 선택 노드의 경로(삽입 순서) — 작업 엔진 입력(혼합 부모 허용).
    pub fn selected_paths(&self) -> Vec<&Path> {
        self.sel_order
            .iter()
            .map(|&id| self.nodes[id as usize].path.as_path())
            .collect()
    }

    /// 선택(삽입 순서) `index`번째 경로(범위 밖이면 `None`).
    /// 호스트가 N개 경로를 인덱스로 순회할 때 `selected_paths()` Vec을 매번 재구성(O(N²))하지 않도록.
    pub fn selected_path(&self, index: usize) -> Option<&Path> {
        let id = *self.sel_order.get(index)?;
        self.nodes.get(id as usize).map(|n| n.path.as_path())
    }

    /// 현재 anchor.
    pub fn anchor(&self) -> Option<NodeId> {
        self.anchor
    }

    /// 노드 경로(범위 밖이면 `None`). ABI/작업 엔진용.
    pub fn node_path(&self, id: NodeId) -> Option<&Path> {
        self.nodes.get(id as usize).map(|n| n.path.as_path())
    }
}

/// `SystemTime` → Unix epoch 밀리초(없으면 -1). 인터롭 표기와 동일.
fn to_unix_ms(t: Option<SystemTime>) -> i64 {
    t.and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(-1, |d| d.as_millis() as i64)
}

/// 대소문자 무시 이름 비교(정렬 규약: 표시와 별개, 안정적 순서).
/// 소문자화 이터레이터를 직접 비교 — `to_lowercase()` 문자열 2개를 비교마다 힙 할당하지 않음
/// (O(n log n) 정렬 핫패스). UTF-8 바이트 순서=코드포인트 순서라 기존 결과와 동일.
fn cmp_ci(a: &str, b: &str) -> std::cmp::Ordering {
    a.chars()
        .flat_map(char::to_lowercase)
        .cmp(b.chars().flat_map(char::to_lowercase))
}

/// 대소문자 **구분** 비교(사용자 확정 07-15 — 스크린샷 QA): **코드포인트 순** —
/// ASCII에서 대문자 블록(65~90)이 소문자 블록(97~122)보다 앞이므로 **대문자 시작
/// 항목이 최상단 그룹**이 된다(`Abc.txt` < `a c.txt` < `abb.txt`, C 로케일 `ls` 동일).
/// NTFS의 디렉터리 B-tree(대소문자 무시 upcase 순)는 저장 구조일 뿐 — 표시 정렬은
/// 앱 메모리 정렬이라 제약 없음. (이전 "알파벳 유지+동률 대문자 우선" 규칙은
/// 대문자가 그룹으로 모이지 않아 사용자 의도와 달라 교체.)
fn cmp_cs_upper_first(a: &str, b: &str) -> std::cmp::Ordering {
    a.cmp(b)
}

/// 파일명의 확장자(마지막 `.` 뒤). 선행 `.`만 있는 dotfile은 확장자 없음("").
fn ext_of(name: &str) -> &str {
    match name.rfind('.') {
        Some(i) if i > 0 => &name[i + 1..],
        _ => "",
    }
}

/// 종류(Kind) 정렬 순위: 폴더 → 파일 → 심링크.
fn kind_rank(k: FileKind) -> u8 {
    match k {
        FileKind::Dir => 0,
        FileKind::File => 1,
        FileKind::Symlink => 2,
    }
}

#[cfg(test)]
impl Tree {
    /// 파일시스템 없이 합성 노드로 채운 트리(벤치/스케일 테스트 전용).
    /// 최상위 `dirs`개 폴더 × 각 `per_dir`개 파일 자식(모두 `loaded`, 접힘 상태).
    /// 최상위만 가시(dirs행). 실제 열거 비용을 제거하고 순수 트리 연산만 측정.
    fn synthetic(dirs: usize, per_dir: usize) -> Tree {
        let mut nodes: Vec<Node> = Vec::with_capacity(dirs * (per_dir + 1));
        let mut roots = Vec::with_capacity(dirs);
        for d in 0..dirs {
            let dir_id = nodes.len() as NodeId;
            let dir_name = format!("dir{d:05}");
            let dir_path = PathBuf::from(&dir_name);
            let mut children = Vec::with_capacity(per_dir);
            // 자식 먼저 예약할 수 없으니 부모 push 후 자식 push, children는 나중에 세팅.
            nodes.push(Node {
                id: dir_id,
                parent: None,
                path: dir_path.clone(),
                name: dir_name,
                kind: FileKind::Dir,
                depth: 0,
                size: 0,
                modified_unix_ms: -1,
                attrs: 0,
                expanded: false,
                loaded: true,
                children: Vec::new(),
            });
            for f in 0..per_dir {
                let cid = nodes.len() as NodeId;
                let cname = format!("f{f:05}.txt");
                nodes.push(Node {
                    id: cid,
                    parent: Some(dir_id),
                    path: dir_path.join(&cname),
                    name: cname,
                    kind: FileKind::File,
                    depth: 1,
                    size: 0,
                    modified_unix_ms: -1,
                    attrs: 0,
                    expanded: false,
                    loaded: true,
                    children: Vec::new(),
                });
                children.push(cid);
            }
            nodes[dir_id as usize].children = children;
            roots.push(dir_id);
        }
        Tree {
            nodes,
            visible: roots.clone(),
            roots,
            sel_order: Vec::new(),
            sel_set: HashSet::new(),
            anchor: None,
            root_path: PathBuf::from("<synthetic>"),
            filter: Filter {
                show_hidden: true,
                show_dotfiles: true,
            },
            sort: SortSpec::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Instant;

    /// AC5 벤치(NFR-P1/P2, docs/07·29) — 10만 가시 노드에서 트리 연산이 UI 프레임 예산 안인지.
    /// 타이밍 단언은 CI 머신 편차로 불안정하므로 하지 않고(`--ignored`로 수동 측정),
    /// 대신 **연산이 완료됨**(무한/이차 폭주 방지)만 확인한다. `-- --ignored --nocapture`로 수치 확인.
    #[test]
    #[ignore = "수동 벤치: cargo test -p nexa-tree -- --ignored --nocapture"]
    fn bench_100k_visible() {
        let dirs = 100;
        let per_dir = 1000; // 100 × 1000 = 100,000 파일 + 100 폴더
        let build = Instant::now();
        let mut t = Tree::synthetic(dirs, per_dir);
        eprintln!(
            "[bench] synthetic build: {:?} ({} nodes)",
            build.elapsed(),
            t.nodes.len()
        );

        // 전체 펼침 → 100,100 가시 행. 각 expand는 splice(꼬리 이동)를 포함.
        let expand = Instant::now();
        let root_ids: Vec<NodeId> = t.roots.clone();
        for id in &root_ids {
            t.expand(*id).unwrap();
        }
        let vis = t.visible_len();
        eprintln!(
            "[bench] expand {dirs} dirs → {vis} visible rows: {:?}",
            expand.elapsed()
        );
        assert_eq!(vis, dirs + dirs * per_dir);

        // 무작위 위치 10,000회 index_of 조회(현재 O(n) 선형) — 병목 후보 측정.
        let lookups = 10_000usize;
        let probe = Instant::now();
        let mut acc = 0usize;
        for k in 0..lookups {
            let target = t.visible[(k * 7919) % vis]; // 흩뿌린 인덱스
            acc += t.index_of(target).unwrap();
        }
        eprintln!(
            "[bench] {lookups}× index_of: {:?} (acc={acc})",
            probe.elapsed()
        );

        // 행 조회 전체 순회(호스트 마샬 전 코어 비용).
        let rows = Instant::now();
        for i in 0..vis {
            let _ = t.row(i).unwrap();
        }
        eprintln!("[bench] row() × {vis}: {:?}", rows.elapsed());

        // 전체 선택 + 접힘.
        let sel = Instant::now();
        t.select_all_visible();
        eprintln!(
            "[bench] select_all_visible ({}): {:?}",
            t.selection_count(),
            sel.elapsed()
        );
        let col = Instant::now();
        for id in &root_ids {
            t.collapse(*id);
        }
        eprintln!(
            "[bench] collapse {dirs} dirs → {} visible: {:?}",
            t.visible_len(),
            col.elapsed()
        );
    }

    /// 스케일 가드(CI 상시) — 10만 노드에서 핵심 연산이 정상 완료(이차 폭주·패닉 없음).
    #[test]
    fn large_tree_scale_ops_complete() {
        let mut t = Tree::synthetic(20, 5000); // 20 × 5000 = 100,000 + 20
        for id in t.roots.clone() {
            t.expand(id).unwrap();
        }
        let vis = t.visible_len();
        assert_eq!(vis, 20 + 20 * 5000);
        // 경계 행 조회.
        assert!(t.row(0).unwrap().has_children);
        assert!(t.row(vis - 1).is_some());
        assert!(t.row(vis).is_none());
        // 위치 조회(끝 근처) + 선택.
        let last = t.visible[vis - 1];
        assert_eq!(t.index_of(last), Some(vis - 1));
        t.select(last, SelectMode::Single);
        assert!(t.is_selected(last));
        // 첫 폴더 접기 → 5000 제거.
        let removed = t.collapse(t.roots[0]).removed;
        assert_eq!(removed, 5000);
        assert_eq!(t.visible_len(), vis - 5000);
    }

    /// 격리된 임시 트리: base/{dirA/{x.txt,y.txt}, dirA/dirB/z.txt, file1.txt}.
    fn make_fixture(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("nexa_tree_{}_{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("dirA/dirB")).unwrap();
        fs::write(base.join("dirA/x.txt"), b"x").unwrap();
        fs::write(base.join("dirA/y.txt"), b"yy").unwrap();
        fs::write(base.join("dirA/dirB/z.txt"), b"zzz").unwrap();
        fs::write(base.join("file1.txt"), b"f").unwrap();
        base
    }

    fn names(t: &Tree) -> Vec<String> {
        (0..t.visible_len())
            .map(|i| t.row(i).unwrap().name)
            .collect()
    }

    #[test]
    fn open_lists_top_level_folders_first() {
        let base = make_fixture("open");
        let t = Tree::open(&base).unwrap();
        fs::remove_dir_all(&base).unwrap();

        assert_eq!(t.visible_len(), 2);
        assert_eq!(names(&t), vec!["dirA", "file1.txt"]);
        assert!(t.row(0).unwrap().has_children); // dir
        assert!(!t.row(0).unwrap().expanded);
        assert!(!t.row(1).unwrap().has_children); // file
    }

    #[test]
    fn expand_and_collapse_roundtrip() {
        let base = make_fixture("expcol");
        let mut t = Tree::open(&base).unwrap();
        let dir_a = t.row(0).unwrap().id;

        let c = t.expand(dir_a).unwrap();
        // dirA 자식 3개(dirB·x.txt·y.txt)가 삽입됨
        assert_eq!(
            c,
            RangeChange {
                start: 1,
                removed: 0,
                inserted: 3
            }
        );
        // dirA / dirB / x.txt / y.txt / file1.txt  (dirB=폴더 우선)
        assert_eq!(
            names(&t),
            vec!["dirA", "dirB", "x.txt", "y.txt", "file1.txt"]
        );
        assert_eq!(t.row(1).unwrap().depth, 1);
        assert!(t.row(0).unwrap().expanded);

        let c2 = t.collapse(dir_a);
        assert_eq!(
            c2,
            RangeChange {
                start: 1,
                removed: 3,
                inserted: 0
            }
        );
        fs::remove_dir_all(&base).unwrap();
        assert_eq!(names(&t), vec!["dirA", "file1.txt"]);
    }

    #[test]
    fn reexpand_restores_nested_expansion() {
        let base = make_fixture("nested");
        let mut t = Tree::open(&base).unwrap();
        let dir_a = t.row(0).unwrap().id;
        t.expand(dir_a).unwrap();
        let dir_b = t.row(1).unwrap().id; // dirB
        assert_eq!(t.row(1).unwrap().name, "dirB");
        t.expand(dir_b).unwrap();
        assert_eq!(
            names(&t),
            vec!["dirA", "dirB", "z.txt", "x.txt", "y.txt", "file1.txt"]
        );

        t.collapse(dir_a);
        assert_eq!(names(&t), vec!["dirA", "file1.txt"]);
        // 재펼침 시 dirB의 펼침 상태(z.txt)가 복원돼야 함
        t.expand(dir_a).unwrap();
        fs::remove_dir_all(&base).unwrap();
        assert_eq!(
            names(&t),
            vec!["dirA", "dirB", "z.txt", "x.txt", "y.txt", "file1.txt"]
        );
    }

    #[test]
    fn index_of_path_and_expand_path() {
        let base = make_fixture("bypath");
        let mut t = Tree::open(&base).unwrap();
        let dir_a_path = base.join("dirA");
        // 최상위에서 경로로 인덱스 조회(끝 구분자·대소문자 무시)
        assert_eq!(t.index_of_path(&dir_a_path.to_string_lossy()), Some(0));
        let mut trailing = dir_a_path.to_string_lossy().into_owned();
        trailing.push('\\');
        assert_eq!(t.index_of_path(&trailing), Some(0)); // 끝 구분자 무시
        let upper = dir_a_path.to_string_lossy().to_uppercase();
        assert_eq!(t.index_of_path(&upper), Some(0)); // 대소문자 무시
        assert_eq!(t.index_of_path("nope"), None);

        // 경로로 펼침 → dirA 자식 삽입, 이후 dirB도 보이면 경로로 조회 가능
        let c = t.expand_path(&dir_a_path.to_string_lossy()).unwrap();
        assert_eq!((c.start, c.removed, c.inserted), (1, 0, 3));
        assert_eq!(
            names(&t),
            vec!["dirA", "dirB", "x.txt", "y.txt", "file1.txt"]
        );
        // 플랫폼 구분자로 조립(Windows에서 내부 '/'는 노드 경로 '\\'와 불일치하므로 join 중첩)
        assert_eq!(
            t.index_of_path(&base.join("dirA").join("dirB").to_string_lossy()),
            Some(1)
        );
        // 이미 펼침 → NONE, 없는 경로 → NONE
        assert_eq!(
            t.expand_path(&dir_a_path.to_string_lossy()).unwrap(),
            RangeChange::NONE
        );
        assert_eq!(t.expand_path("no/such").unwrap(), RangeChange::NONE);
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn expand_is_noop_on_file_or_twice() {
        let base = make_fixture("noop");
        let mut t = Tree::open(&base).unwrap();
        let file1 = t.row(1).unwrap().id;
        assert_eq!(t.expand(file1).unwrap(), RangeChange::NONE); // 파일

        let dir_a = t.row(0).unwrap().id;
        t.expand(dir_a).unwrap();
        assert_eq!(t.expand(dir_a).unwrap(), RangeChange::NONE); // 이미 펼침
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn cross_folder_selection_ordered() {
        let base = make_fixture("sel");
        let mut t = Tree::open(&base).unwrap();
        let dir_a = t.row(0).unwrap().id;
        t.expand(dir_a).unwrap(); // dirA/dirB/x.txt/y.txt/file1.txt

        let x_id = t.row(2).unwrap().id; // x.txt (dirA 자식)
        let file1 = t.row(4).unwrap().id; // file1.txt (루트)
        assert_eq!(t.row(2).unwrap().name, "x.txt");
        assert_eq!(t.row(4).unwrap().name, "file1.txt");

        t.select(x_id, SelectMode::Single);
        t.select(file1, SelectMode::Toggle); // 서로 다른 부모 동시 선택
        assert!(t.is_selected(x_id) && t.is_selected(file1));
        assert_eq!(t.selected_ids(), &[x_id, file1]); // 삽입 순서
        assert_eq!(t.selection_count(), 2);

        t.select(file1, SelectMode::Toggle); // 토글 해제
        assert!(!t.is_selected(file1));
        assert_eq!(t.selected_ids(), &[x_id]);
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn range_and_select_all() {
        let base = make_fixture("range");
        let mut t = Tree::open(&base).unwrap();
        let dir_a = t.row(0).unwrap().id;
        t.expand(dir_a).unwrap(); // 5 rows

        let first = t.row(1).unwrap().id;
        let fourth = t.row(3).unwrap().id;
        t.select(first, SelectMode::Single); // anchor=first(row1)
        t.select_range(fourth); // row1..row3
        assert_eq!(t.selection_count(), 3);
        assert!(t.is_selected(t.row(1).unwrap().id));
        assert!(t.is_selected(t.row(3).unwrap().id));
        assert!(!t.is_selected(t.row(4).unwrap().id));

        t.select_all_visible();
        assert_eq!(t.selection_count(), t.visible_len());
        t.clear_selection();
        assert_eq!(t.selection_count(), 0);
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn open_filtered_excludes_dotfiles() {
        let base = std::env::temp_dir().join(format!("nexa_tree_filter_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join(".hidden"), b"h").unwrap();
        fs::write(base.join("visible.txt"), b"v").unwrap();

        let all = Tree::open(&base).unwrap();
        let no_dot = Tree::open_filtered(&base, true, false).unwrap();
        fs::remove_dir_all(&base).unwrap();

        assert_eq!(all.visible_len(), 2); // 기본 open = 모두 표시
        assert_eq!(no_dot.visible_len(), 1); // .hidden 제외
        assert_eq!(no_dot.row(0).unwrap().name, "visible.txt");
    }

    #[test]
    fn open_missing_path_errors() {
        let missing = std::env::temp_dir().join("nexa_tree_missing_zzz_does_not_exist");
        assert!(Tree::open(&missing).is_err());
    }

    // ── 정렬(COL-2a) ────────────────────────────────────────────────

    /// 정렬 검증용 픽스처: 폴더 adir(inner.txt 1개)·zdir, 파일 a.log(3)·m.txt(1)·z.md(2).
    /// 이름/크기/확장자 정렬 결과가 서로 달라 구분력이 있다.
    fn make_sort_fixture(tag: &str) -> PathBuf {
        let base =
            std::env::temp_dir().join(format!("nexa_tree_sort_{}_{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("adir")).unwrap();
        fs::create_dir_all(base.join("zdir")).unwrap();
        fs::write(base.join("adir/inner.txt"), b"i").unwrap();
        fs::write(base.join("a.log"), b"333").unwrap(); // size 3
        fs::write(base.join("m.txt"), b"1").unwrap(); //   size 1
        fs::write(base.join("z.md"), b"22").unwrap(); //   size 2
        base
    }

    fn spec(key: SortKey, desc: bool, folders_first: bool) -> SortSpec {
        SortSpec {
            keys: vec![(key, desc)],
            folders_first,
            case_sensitive: false,
        }
    }

    #[test]
    fn case_sensitive_groups_uppercase_first() {
        use std::cmp::Ordering;
        // 코드포인트 순(사용자 확정 07-15) — 대문자 시작 항목이 최상단 그룹
        assert_eq!(cmp_cs_upper_first("Apple", "apple"), Ordering::Less);
        assert_eq!(cmp_cs_upper_first("README", "readme"), Ordering::Less);
        assert_eq!(
            cmp_cs_upper_first("Zebra", "apple"),
            Ordering::Less,
            "대문자 블록 전체가 소문자보다 앞(C 로케일 ls 동일)"
        );
        // 사용자 스크린샷 시나리오: Abc.txt가 a c.txt·abb.txt 위
        assert_eq!(cmp_cs_upper_first("Abc.txt", "a c.txt"), Ordering::Less);
        assert_eq!(cmp_cs_upper_first("Abc.txt", "abb.txt"), Ordering::Less);
        assert_eq!(
            cmp_cs_upper_first(".claude.json", "Abc.txt"),
            Ordering::Less
        );
        assert_eq!(cmp_cs_upper_first("ab", "abc"), Ordering::Less);
    }

    #[test]
    fn sort_default_is_folders_first_name_asc() {
        let base = make_sort_fixture("def");
        let t = Tree::open(&base).unwrap();
        fs::remove_dir_all(&base).unwrap();
        // 기본(변경 전) = 폴더 우선 + 이름 오름
        assert_eq!(names(&t), vec!["adir", "zdir", "a.log", "m.txt", "z.md"]);
    }

    #[test]
    fn sort_by_size_asc_and_desc() {
        let base = make_sort_fixture("size");
        let mut t = Tree::open(&base).unwrap();
        fs::remove_dir_all(&base).unwrap();
        t.set_sort(spec(SortKey::Size, false, true));
        // 폴더 우선(이름) → 파일 크기 오름: m(1) z(2) a(3)
        assert_eq!(names(&t), vec!["adir", "zdir", "m.txt", "z.md", "a.log"]);
        t.set_sort(spec(SortKey::Size, true, true));
        // 파일 크기 내림: a(3) z(2) m(1). 폴더는 여전히 앞(folders_first).
        assert_eq!(names(&t), vec!["adir", "zdir", "a.log", "z.md", "m.txt"]);
    }

    #[test]
    fn sort_by_name_desc_keeps_folders_first() {
        let base = make_sort_fixture("named");
        let mut t = Tree::open(&base).unwrap();
        fs::remove_dir_all(&base).unwrap();
        t.set_sort(spec(SortKey::Name, true, true));
        // 폴더 우선(내림: zdir,adir) → 파일 내림: z,m,a
        assert_eq!(names(&t), vec!["zdir", "adir", "z.md", "m.txt", "a.log"]);
    }

    #[test]
    fn sort_by_ext_asc() {
        let base = make_sort_fixture("ext");
        let mut t = Tree::open(&base).unwrap();
        fs::remove_dir_all(&base).unwrap();
        t.set_sort(spec(SortKey::Ext, false, true));
        // 폴더(확장자 없음, 이름 tie) → 파일 확장자: log(a) < md(z) < txt(m)
        assert_eq!(names(&t), vec!["adir", "zdir", "a.log", "z.md", "m.txt"]);
    }

    #[test]
    fn sort_by_kind_asc_desc_differ_by_ext() {
        let base = make_sort_fixture("kind");
        let mut t = Tree::open(&base).unwrap();
        fs::remove_dir_all(&base).unwrap();
        // 종류(=확장자 타입) 오름: 폴더 우선 → 파일 확장자 log(a) < md(z) < txt(m)
        t.set_sort(spec(SortKey::Kind, false, true));
        let asc = names(&t);
        assert_eq!(asc, vec!["adir", "zdir", "a.log", "z.md", "m.txt"]);
        // 내림: 파일 확장자 역순 txt(m) > md(z) > log(a). 폴더는 folders_first로 여전히 앞.
        t.set_sort(spec(SortKey::Kind, true, true));
        let desc = names(&t);
        assert_eq!(desc, vec!["adir", "zdir", "m.txt", "z.md", "a.log"]);
        assert_ne!(asc, desc, "종류 오름/내림 결과가 달라야 함");
    }

    #[test]
    fn sort_none_restores_enumeration_order() {
        let base = make_sort_fixture("none");
        let mut t = Tree::open(&base).unwrap();
        fs::remove_dir_all(&base).unwrap();
        // folders_first=off + None → 순수 열거(id) 순서 = id 오름차순(엄격 증가).
        t.set_sort(spec(SortKey::None, false, false));
        let ids: Vec<NodeId> = (0..t.visible_len()).map(|i| t.row(i).unwrap().id).collect();
        assert!(
            ids.windows(2).all(|w| w[0] < w[1]),
            "None은 열거(id) 순서여야 함: {ids:?}"
        );
    }

    #[test]
    fn set_sort_preserves_expansion() {
        let base = make_sort_fixture("exp");
        let mut t = Tree::open(&base).unwrap();
        // adir 펼침(inner.txt 표시).
        let adir = t.row(0).unwrap().id;
        assert_eq!(t.row(0).unwrap().name, "adir");
        t.expand(adir).unwrap();
        assert!(names(&t).contains(&"inner.txt".to_string()));
        // 정렬을 크기 내림으로 바꿔도 adir는 여전히 펼쳐져 있고 inner.txt가 그 아래.
        t.set_sort(spec(SortKey::Size, true, true));
        fs::remove_dir_all(&base).unwrap();
        assert_eq!(t.is_expanded(adir), Some(true));
        let ns = names(&t);
        let ai = ns.iter().position(|n| n == "adir").unwrap();
        assert_eq!(
            ns[ai + 1],
            "inner.txt",
            "펼친 자식이 부모 바로 뒤에 유지: {ns:?}"
        );
    }

    // ── 타입어헤드 find_prefix (docs/32) ────────────────────────────

    /// 타입어헤드 픽스처: 최상위 apple/apricot/banana.txt + 폴더 sub{avocado.txt}.
    /// 기본 정렬(폴더우선·이름오름) 가시(접힘): [sub, apple.txt, apricot.txt, banana.txt].
    fn make_ta_fixture(tag: &str) -> PathBuf {
        let base =
            std::env::temp_dir().join(format!("nexa_tree_ta_{}_{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("sub")).unwrap();
        fs::write(base.join("sub/avocado.txt"), b"a").unwrap();
        fs::write(base.join("apple.txt"), b"a").unwrap();
        fs::write(base.join("apricot.txt"), b"a").unwrap();
        fs::write(base.join("banana.txt"), b"b").unwrap();
        base
    }

    #[test]
    fn find_prefix_visible_stream_wrap_and_cycle() {
        let base = make_ta_fixture("c");
        let t = Tree::open(&base).unwrap();
        fs::remove_dir_all(&base).unwrap();
        // [sub(0), apple(1), apricot(2), banana(3)]
        assert_eq!(
            names(&t),
            vec!["sub", "apple.txt", "apricot.txt", "banana.txt"]
        );
        // 캐럿 없음 → 처음부터 첫 'a' = apple(1)
        assert_eq!(t.find_prefix(None, "a", FindScope::VisibleStream), Some(1));
        // apple(1)에서 다음 'a' = apricot(2)
        assert_eq!(
            t.find_prefix(Some(1), "a", FindScope::VisibleStream),
            Some(2)
        );
        // apricot(2)에서 다음 'a' → banana 아님, wrap → apple(1)
        assert_eq!(
            t.find_prefix(Some(2), "a", FindScope::VisibleStream),
            Some(1)
        );
        // 'ap' 접두사 → apple(1)
        assert_eq!(t.find_prefix(None, "ap", FindScope::VisibleStream), Some(1));
        // 'b' → banana(3), 대소문자 무시
        assert_eq!(t.find_prefix(None, "B", FindScope::VisibleStream), Some(3));
        // 없는 접두사 → None, 빈 접두사 → None
        assert_eq!(t.find_prefix(None, "z", FindScope::VisibleStream), None);
        assert_eq!(t.find_prefix(Some(1), "", FindScope::VisibleStream), None);
    }

    #[test]
    fn find_prefix_global_first_ignores_caret() {
        let base = make_ta_fixture("a");
        let t = Tree::open(&base).unwrap();
        fs::remove_dir_all(&base).unwrap();
        // 캐럿 무관하게 항상 0부터 첫 'a' = apple(1)
        assert_eq!(t.find_prefix(None, "a", FindScope::GlobalFirst), Some(1));
        assert_eq!(t.find_prefix(Some(2), "a", FindScope::GlobalFirst), Some(1));
        assert_eq!(t.find_prefix(Some(3), "a", FindScope::GlobalFirst), Some(1));
    }

    #[test]
    fn find_prefix_current_level_siblings_only() {
        let base = make_ta_fixture("b");
        let mut t = Tree::open(&base).unwrap();
        // sub 펼침 → [sub(0), avocado(1), apple(2), apricot(3), banana(4)]
        let sub = t.row(0).unwrap().id;
        t.expand(sub).unwrap();
        fs::remove_dir_all(&base).unwrap();
        assert_eq!(
            names(&t),
            vec![
                "sub",
                "avocado.txt",
                "apple.txt",
                "apricot.txt",
                "banana.txt"
            ]
        );
        // 최상위 apple(2)에서 'a' 형제 검색 → avocado(자식) 건너뛰고 apricot(3)
        assert_eq!(
            t.find_prefix(Some(2), "a", FindScope::CurrentLevel),
            Some(3)
        );
        // avocado(1, sub 자식)에서 'a' 형제 → 형제는 avocado뿐 → wrap으로 자신(1)
        assert_eq!(
            t.find_prefix(Some(1), "a", FindScope::CurrentLevel),
            Some(1)
        );
        // 비교: 같은 상황 C(가시 스트림)는 형제 무시하고 apricot(3)
        assert_eq!(
            t.find_prefix(Some(1), "a", FindScope::VisibleStream),
            Some(2)
        ); // apple(2)
    }

    #[test]
    fn set_sort_none_then_name_roundtrip() {
        let base = make_sort_fixture("rt");
        let mut t = Tree::open(&base).unwrap();
        fs::remove_dir_all(&base).unwrap();
        let default_order = names(&t);
        t.set_sort(spec(SortKey::Size, true, true)); // 흐트러뜨림
        assert_ne!(names(&t), default_order);
        t.set_sort(SortSpec::name_asc()); // 기본 복귀
        assert_eq!(names(&t), default_order);
    }
}
