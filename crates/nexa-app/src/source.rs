//! nexa-tree → nexa-gui 배선(M1-3): 가시 노드 평면 스트림을 `RowSource`로 투영.
//! 플랫폼 중립(비-Windows에서도 테스트) — 창/렌더와 무관한 순수 어댑터.

use nexa_gui::widgets::{Marker, RowItem, RowSource};
use nexa_tree::Tree;

/// 트리 한 그루를 행 스트림으로 노출. 클릭 토글 = 펼침/접힘(캐럿·선택은 M1-5).
pub struct TreeSource {
    tree: Tree,
}

impl TreeSource {
    pub fn new(tree: Tree) -> Self {
        TreeSource { tree }
    }

    pub fn tree(&self) -> &Tree {
        &self.tree
    }
}

impl RowSource for TreeSource {
    fn len(&self) -> usize {
        self.tree.visible_len()
    }

    fn row(&self, index: usize) -> RowItem {
        match self.tree.row(index) {
            Some(r) => RowItem {
                text: r.name,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

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
        let mut s = TreeSource::new(Tree::open(&base).unwrap());
        // 기본 정렬: [dirA, empty, file1.txt]
        assert_eq!(s.len(), 3);
        assert_eq!(s.row(0).marker, Marker::Collapsed);
        assert_eq!(s.row(2).marker, Marker::None);
        assert_eq!(s.row(0).depth, 0);

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
        let mut s = TreeSource::new(Tree::open(&base).unwrap());
        assert!(!s.toggle(2)); // file1.txt — 무변화
        assert!(s.toggle(1)); // empty 폴더 — 행 수 불변이지만 마커 갱신 필요 → true
        fs::remove_dir_all(&base).unwrap();
        assert_eq!(s.len(), 3);
        assert_eq!(s.row(1).marker, Marker::Expanded);
        assert!(!s.toggle(99)); // 범위 밖
    }
}
