//! nexa-tree → nexa-gui 배선(M1-3): 가시 노드 평면 스트림을 `RowSource`로 투영.
//! M1-4: 컬럼 셀 값(확장자·크기·수정한 날짜·종류)·헤더 정렬(`set_sort`) 배선 — 원본 docs/23.
//! 플랫폼 중립(비-Windows에서도 테스트) — 창/렌더와 무관한 순수 어댑터.

use crate::i18n::{tr, trf};
use nexa_core::FileKind;
use nexa_gui::widgets::{Marker, RowItem, RowSource, SelectOp};
use nexa_tree::{FindScope, SelectMode, SortKey, SortSpec, Tree};

/// 컬럼 key(Column.key ↔ 이 상수). 0 = 트리 컬럼 관례(nexa-gui).
pub const COL_NAME: u32 = 0;
pub const COL_EXT: u32 = 1;
pub const COL_SIZE: u32 = 2;
pub const COL_MODIFIED: u32 = 3;
pub const COL_KIND: u32 = 4;

/// 트리 한 그루를 행 스트림으로 노출. 클릭 토글 = 펼침/접힘(캐럿·선택은 M1-5).
pub struct TreeSource {
    tree: Tree,
    /// 수정한 날짜 표시용 로컬 타임존 오프셋(분, UTC 기준 동쪽 양수).
    tz_offset_min: i32,
    /// 폴더 우선 그룹핑(G-13 — 설정 `sort_folders_first`, 기본 true=탐색기 규약).
    folders_first: bool,
    /// 대소문자 구분 정렬(사용자 요청 07-15 — 기본 false).
    case_sensitive: bool,
    /// 타입어헤드 검색 범위(원본 docs/32 §5 — 설정 07-15).
    find_scope: FindScope,
    /// 마지막 정렬 키(폴더 우선 토글 시 재적용용).
    sort_keys: Vec<(SortKey, bool)>,
}

impl TreeSource {
    pub fn new(tree: Tree, tz_offset_min: i32) -> Self {
        TreeSource {
            tree,
            tz_offset_min,
            folders_first: true,
            case_sensitive: false,
            find_scope: FindScope::VisibleStream,
            // Tree 기본(name_asc)과 일치 — 옵션 토글 시 빈 키로 열거 순서 퇴행 방지(07-15)
            sort_keys: vec![(SortKey::Name, false)],
        }
    }

    /// 폴더 우선 그룹핑 토글(G-13) — 현재 정렬 키를 유지한 채 즉시 재정렬.
    pub fn set_folders_first(&mut self, on: bool) {
        if self.folders_first != on {
            self.folders_first = on;
            self.apply_sort();
        }
    }

    /// 타입어헤드 검색 범위(설정 — 07-15).
    pub fn set_find_scope(&mut self, scope: FindScope) {
        self.find_scope = scope;
    }

    /// 대소문자 구분 정렬 토글(사용자 요청 07-15) — 즉시 재정렬.
    pub fn set_case_sensitive(&mut self, on: bool) {
        if self.case_sensitive != on {
            self.case_sensitive = on;
            self.apply_sort();
        }
    }

    /// 현재 키+옵션으로 정렬 재적용(단일 원천).
    fn apply_sort(&mut self) {
        self.tree.set_sort(SortSpec {
            keys: self.sort_keys.clone(),
            folders_first: self.folders_first,
            case_sensitive: self.case_sensitive,
        });
    }

    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    /// 재로드 상태 복원(펼침·선택 — M3-6 무간섭 갱신 선행)용 가변 접근.
    pub fn tree_mut(&mut self) -> &mut Tree {
        &mut self.tree
    }
}

impl RowSource for TreeSource {
    fn len(&self) -> usize {
        self.tree.visible_len()
    }

    fn row(&self, index: usize) -> RowItem {
        match self.tree.row(index) {
            Some(r) => RowItem {
                text: display_name(r.name),
                depth: r.depth,
                marker: if r.has_children {
                    if r.expanded {
                        Marker::Expanded
                    } else {
                        Marker::Collapsed
                    }
                } else {
                    Marker::None
                },
            },
            // 페인트 중 행 수가 바뀌는 일은 없지만(단일 스레드) 방어적 빈 행
            None => RowItem {
                text: String::new(),
                depth: 0,
                marker: Marker::None,
            },
        }
    }

    fn cell(&self, index: usize, key: u32) -> String {
        let Some(r) = self.tree.row(index) else {
            return String::new();
        };
        let is_dir = r.kind == FileKind::Dir;
        match key {
            COL_EXT => {
                if is_dir {
                    String::new()
                } else {
                    ext_of(&r.name).to_string()
                }
            }
            COL_SIZE => {
                if is_dir {
                    String::new() // 폴더 크기는 OS 잡음값(코어 정렬도 0 정규화)
                } else {
                    human_size(r.size)
                }
            }
            COL_MODIFIED => fmt_datetime(r.modified_unix_ms, self.tz_offset_min),
            // 페인트 시점 tr() 조회 — 언어 전환 시 재그리기만으로 반영(M2-6, 원본 kind.* 키)
            COL_KIND => match r.kind {
                FileKind::Dir => tr("kind.folder"),
                FileKind::Symlink => tr("kind.link"),
                FileKind::File => {
                    let ext = ext_of(&r.name);
                    if ext.is_empty() {
                        tr("kind.file")
                    } else {
                        trf("kind.extFile", &[&ext.to_uppercase()])
                    }
                }
            },
            _ => String::new(),
        }
    }

    fn toggle(&mut self, index: usize) -> bool {
        let Some(id) = self.tree.visible_id(index) else {
            return false;
        };
        match self.tree.is_expanded(id) {
            Some(true) => self.tree.collapse(id).removed > 0,
            Some(false) => match self.tree.expand(id) {
                // 빈 폴더 펼침은 가시 변화 0이지만 마커(▸→▾)는 바뀜 — 다시 그린다
                Ok(_) => self.tree.is_expanded(id) == Some(true),
                Err(_) => false, // 접근 불가 폴더 — 조용히 무시(M3 watcher에서 UX 개선)
            },
            None => false,
        }
    }

    // ── 선택(원본 docs/07 — 코어 OrderedSet·anchor 모델에 위임) ──

    fn is_selected(&self, index: usize) -> bool {
        self.tree
            .visible_id(index)
            .is_some_and(|id| self.tree.is_selected(id))
    }

    fn select(&mut self, index: usize, op: SelectOp) -> bool {
        let Some(id) = self.tree.visible_id(index) else {
            return false;
        };
        match op {
            SelectOp::Single => self.tree.select(id, SelectMode::Single),
            SelectOp::Toggle => self.tree.select(id, SelectMode::Toggle),
            SelectOp::RangeTo => self.tree.select_range(id),
        }
        true
    }

    fn select_span(&mut self, lo: usize, hi: usize) -> bool {
        let (Some(a), Some(b)) = (self.tree.visible_id(lo), self.tree.visible_id(hi)) else {
            return false;
        };
        // 러버밴드 = anchor 없는 일회성 범위 — Single(a) + range(b)로 코어 범위 선택 재사용
        self.tree.select(a, SelectMode::Single);
        self.tree.select_range(b);
        true
    }

    fn select_all(&mut self) -> bool {
        self.tree.select_all_visible();
        true
    }

    fn clear_selection(&mut self) -> bool {
        if self.tree.selection_count() == 0 {
            return false;
        }
        self.tree.clear_selection();
        true
    }

    fn find_prefix(&self, caret: Option<usize>, prefix: &str) -> Option<usize> {
        // 범위 = 가시 스트림 위치상대 + wrap(C, 기본 — docs/32 §5). A/B 설정 노출은 M2.
        self.tree.find_prefix(caret, prefix, self.find_scope)
    }

    fn icon(&self, index: usize) -> Option<(String, String)> {
        let id = self.tree.visible_id(index)?;
        let path = self.tree.node_path(id)?.to_string_lossy().into_owned();
        let is_dir = self.tree.row(index)?.kind == FileKind::Dir;
        Some((crate::icons::icon_key(is_dir, &path), path))
    }

    fn set_sort(&mut self, keys: &[(u32, bool)]) -> bool {
        let mapped: Vec<(SortKey, bool)> = keys
            .iter()
            .filter_map(|&(k, desc)| {
                let key = match k {
                    COL_NAME => SortKey::Name,
                    COL_EXT => SortKey::Ext,
                    COL_SIZE => SortKey::Size,
                    COL_MODIFIED => SortKey::Modified,
                    COL_KIND => SortKey::Kind,
                    _ => return None,
                };
                Some((key, desc))
            })
            .collect();
        // 빈 목록 = 열거 순서. 폴더 우선은 설정(G-13, 기본 true=탐색기 규약)
        self.sort_keys = mapped;
        self.apply_sort();
        true
    }
}

/// 표시 이름 — 바로가기(.lnk)는 확장자를 숨긴다(탐색기 NeverShowExt 관례, QA 07-14).
/// 확장자 컬럼(COL_EXT)에는 lnk가 그대로 남는다(정보 유지).
fn display_name(name: String) -> String {
    let n = name.len();
    if n > 4 && name.is_char_boundary(n - 4) && name[n - 4..].eq_ignore_ascii_case(".lnk") {
        name[..n - 4].to_string()
    } else {
        name
    }
}

/// 파일명의 확장자(마지막 `.` 뒤). 선행 `.`만 있는 dotfile은 확장자 없음("").
fn ext_of(name: &str) -> &str {
    match name.rfind('.') {
        Some(i) if i > 0 => &name[i + 1..],
        _ => "",
    }
}

/// 사람이 읽는 크기 — B/KB/MB/…, 100 미만은 소수 1자리.
fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    const UNITS: [&str; 5] = ["KB", "MB", "GB", "TB", "PB"];
    let mut v = bytes as f64 / 1024.0;
    let mut i = 0;
    while v >= 1024.0 && i + 1 < UNITS.len() {
        v /= 1024.0;
        i += 1;
    }
    if v >= 100.0 {
        format!("{v:.0} {}", UNITS[i])
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// Unix epoch 일수 → (년, 월, 일) — Howard Hinnant civil_from_days(공용 알고리즘).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Unix ms → "yyyy-MM-dd HH:mm"(오프셋 반영). 없음(-1 등 음수 관례값)이면 빈 값.
fn fmt_datetime(unix_ms: i64, tz_offset_min: i32) -> String {
    if unix_ms < 0 {
        return String::new();
    }
    let secs = unix_ms.div_euclid(1000) + i64::from(tz_offset_min) * 60;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}",
        rem / 3600,
        (rem % 3600) / 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn display_name_hides_lnk_only() {
        assert_eq!(display_name("앱 바로가기.lnk".into()), "앱 바로가기");
        assert_eq!(display_name("UPPER.LNK".into()), "UPPER");
        assert_eq!(display_name("a.txt".into()), "a.txt");
        assert_eq!(display_name(".lnk".into()), ".lnk"); // 이름 전체가 확장자 — 유지
        assert_eq!(display_name("한글이름".into()), "한글이름");
    }

    /// base/{dirA/{x.txt,y.txt}, empty/, file1.txt}
    fn fixture(tag: &str) -> PathBuf {
        let base =
            std::env::temp_dir().join(format!("nexa_app_src_{}_{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("dirA")).unwrap();
        fs::create_dir_all(base.join("empty")).unwrap();
        fs::write(base.join("dirA/x.txt"), b"x").unwrap();
        fs::write(base.join("dirA/y.txt"), b"y").unwrap();
        fs::write(base.join("file1.txt"), b"f").unwrap();
        base
    }

    #[test]
    fn rows_project_marker_and_depth() {
        let base = fixture("proj");
        let mut s = TreeSource::new(Tree::open(&base).unwrap(), 0);
        // 기본 정렬: [dirA, empty, file1.txt]
        assert_eq!(s.len(), 3);
        assert_eq!(s.row(0).marker, Marker::Collapsed);
        assert_eq!(s.row(2).marker, Marker::None);

        assert!(s.toggle(0)); // dirA 펼침 → x.txt·y.txt 삽입
        fs::remove_dir_all(&base).unwrap();
        assert_eq!(s.len(), 5);
        assert_eq!(s.row(0).marker, Marker::Expanded);
        assert_eq!(s.row(1).text, "x.txt");
        assert_eq!(s.row(1).depth, 1);

        assert!(s.toggle(0)); // 접기
        assert_eq!(s.len(), 3);
    }

    #[test]
    fn toggle_file_is_noop_and_empty_dir_still_repaints() {
        let base = fixture("noop");
        let mut s = TreeSource::new(Tree::open(&base).unwrap(), 0);
        assert!(!s.toggle(2)); // file1.txt — 무변화
        assert!(s.toggle(1)); // empty 폴더 — 행 수 불변이지만 마커 갱신 필요 → true
        fs::remove_dir_all(&base).unwrap();
        assert_eq!(s.len(), 3);
        assert_eq!(s.row(1).marker, Marker::Expanded);
        assert!(!s.toggle(99)); // 범위 밖
    }

    #[test]
    fn cells_project_ext_size_kind_and_dirs_are_blank() {
        let base = fixture("cells");
        let s = TreeSource::new(Tree::open(&base).unwrap(), 0);
        // [dirA, empty, file1.txt]
        assert_eq!(s.cell(0, COL_SIZE), ""); // 폴더 크기 없음
        assert_eq!(s.cell(0, COL_KIND), "Folder"); // 활성 언어 기본 = 내장 en(i18n)
        assert_eq!(s.cell(0, COL_EXT), "");
        assert_eq!(s.cell(2, COL_SIZE), "1 B");
        assert_eq!(s.cell(2, COL_EXT), "txt");
        assert_eq!(s.cell(2, COL_KIND), "TXT file");
        assert!(!s.cell(2, COL_MODIFIED).is_empty()); // 방금 만든 파일 — 날짜 표시
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn set_sort_reorders_visible_rows() {
        let base = fixture("sort");
        fs::write(base.join("big.bin"), vec![0u8; 2048]).unwrap();
        let mut s = TreeSource::new(Tree::open(&base).unwrap(), 0);
        // 기본: [dirA, empty, big.bin, file1.txt] (폴더 우선·이름 오름)
        assert!(s.set_sort(&[(COL_SIZE, true)])); // 크기 내림
        assert_eq!(s.row(2).text, "big.bin"); // 폴더 2개 뒤 가장 큰 파일
        assert!(s.set_sort(&[])); // 없음 = 열거 순서(폴더 우선 유지)
        assert_eq!(s.len(), 4);
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn cross_folder_selection_via_widget_ops() {
        let base = fixture("xsel");
        let mut s = TreeSource::new(Tree::open(&base).unwrap(), 0);
        s.toggle(0); // dirA 펼침 → [dirA, x.txt, y.txt, empty, file1.txt]
        fs::remove_dir_all(&base).unwrap();
        assert_eq!(s.len(), 5);

        s.select(1, SelectOp::Single); // x.txt (dirA 자식)
        s.select(4, SelectOp::Toggle); // file1.txt (루트) — 교차폴더(AC2)
        assert!(s.is_selected(1) && s.is_selected(4));
        assert_eq!(s.tree().selection_count(), 2);
        assert_eq!(s.tree().selected_paths().len(), 2); // 작업 엔진 입력(혼합 부모)

        s.select(2, SelectOp::RangeTo); // anchor(file1=4)~2 가시 범위 → {2,3,4}
        assert_eq!(s.tree().selection_count(), 3);
        assert!(s.is_selected(3) && !s.is_selected(1));

        assert!(s.select_span(0, 1)); // 러버밴드 범위 대체
        assert!(s.is_selected(0) && s.is_selected(1) && !s.is_selected(4));

        assert!(s.select_all());
        assert_eq!(s.tree().selection_count(), 5);
        assert!(s.clear_selection());
        assert!(!s.clear_selection(), "이미 비어 있으면 false");
    }

    #[test]
    fn find_prefix_delegates_visible_stream() {
        let base = fixture("find");
        let s = TreeSource::new(Tree::open(&base).unwrap(), 0);
        fs::remove_dir_all(&base).unwrap();
        // [dirA, empty, file1.txt] — 캐럿 없음 → 처음부터
        assert_eq!(s.find_prefix(None, "fi"), Some(2));
        assert_eq!(s.find_prefix(Some(0), "e"), Some(1));
        assert_eq!(s.find_prefix(Some(2), "d"), Some(0), "끝이면 wrap");
        assert_eq!(s.find_prefix(None, "zzz"), None);
    }

    #[test]
    fn human_size_units_and_precision() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
        assert_eq!(human_size(1024), "1.0 KB");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(150 * 1024), "150 KB");
        assert_eq!(human_size(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn fmt_datetime_applies_offset_and_epoch_math() {
        assert_eq!(fmt_datetime(0, 0), "1970-01-01 00:00");
        assert_eq!(fmt_datetime(0, 540), "1970-01-01 09:00"); // KST
                                                              // 2026-07-12 00:00:00 UTC = 20646일 × 86400s = 1_783_814_400_000 ms
        assert_eq!(fmt_datetime(1_783_814_400_000, 0), "2026-07-12 00:00");
        assert_eq!(fmt_datetime(-1, 540), ""); // 없음
    }
}
