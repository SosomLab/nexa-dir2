//! nexa-core — Nexa Dir 코어 공용 타입/유틸.
//!
//! 모든 코어 크레이트(vfs/index/preview/ops/...)가 공유하는 기본 타입을 둔다.
//! 스캐폴딩 단계 — 후속 단위에서 점진 확장.

/// 코어 버전 (인터롭/플러그인 호환성 점검에 사용).
pub const CORE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 파일 항목 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    File,
    Dir,
    Symlink,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_set() {
        assert!(!CORE_VERSION.is_empty());
        let v: &str = CORE_VERSION;
        assert!(!v.is_empty());
    }

    #[test]
    fn file_kinds_distinct() {
        assert_ne!(FileKind::File, FileKind::Dir);
        assert_ne!(FileKind::Dir, FileKind::Symlink);
    }
}
