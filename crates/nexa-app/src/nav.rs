//! 네비게이션 히스토리(M1-8) — 뒤로/앞으로 스택. 순수 로직(전 플랫폼 테스트).
//! 탐색기 규약: 새 진입은 현재 위치 이후의 "앞으로" 기록을 버린다.

use std::path::{Path, PathBuf};

pub struct History {
    entries: Vec<PathBuf>,
    pos: usize,
}

impl History {
    pub fn new(start: PathBuf) -> Self {
        History {
            entries: vec![start],
            pos: 0,
        }
    }

    pub fn current(&self) -> &Path {
        &self.entries[self.pos]
    }

    /// 새 위치 진입 — 앞으로 기록 절단 후 push. 현재와 같은 경로면 무시.
    pub fn push(&mut self, path: PathBuf) {
        if self.current() == path.as_path() {
            return;
        }
        self.entries.truncate(self.pos + 1);
        self.entries.push(path);
        self.pos += 1;
    }

    /// 현재 항목을 교체(필터 토글 재열기 등 — 히스토리 이동 없음).
    pub fn replace(&mut self, path: PathBuf) {
        self.entries[self.pos] = path;
    }

    pub fn can_back(&self) -> bool {
        self.pos > 0
    }

    pub fn can_forward(&self) -> bool {
        self.pos + 1 < self.entries.len()
    }

    pub fn back(&mut self) -> Option<&Path> {
        if !self.can_back() {
            return None;
        }
        self.pos -= 1;
        Some(self.current())
    }

    pub fn forward(&mut self) -> Option<&Path> {
        if !self.can_forward() {
            return None;
        }
        self.pos += 1;
        Some(self.current())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn push_back_forward_roundtrip() {
        let mut h = History::new(p("C:\\a"));
        h.push(p("C:\\a\\b"));
        h.push(p("C:\\a\\b\\c"));
        assert_eq!(h.back(), Some(p("C:\\a\\b").as_path()));
        assert_eq!(h.back(), Some(p("C:\\a").as_path()));
        assert_eq!(h.back(), None); // 바닥
        assert_eq!(h.forward(), Some(p("C:\\a\\b").as_path()));
        assert!(h.can_forward());
    }

    #[test]
    fn push_truncates_forward_branch() {
        let mut h = History::new(p("a"));
        h.push(p("b"));
        h.push(p("c"));
        h.back(); // → b
        h.push(p("d")); // c 절단
        assert_eq!(h.current(), p("d").as_path());
        assert!(!h.can_forward());
        assert_eq!(h.back(), Some(p("b").as_path()));
    }

    #[test]
    fn push_same_path_is_noop_and_replace_keeps_position() {
        let mut h = History::new(p("a"));
        h.push(p("a")); // 무시
        assert!(!h.can_back());
        h.push(p("b"));
        h.replace(p("b2")); // 토글 재열기 — 이동 없음
        assert_eq!(h.current(), p("b2").as_path());
        assert_eq!(h.back(), Some(p("a").as_path()));
    }
}
