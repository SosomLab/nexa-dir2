//! nexa-vfs — 가상 파일시스템 추상화. 모든 저장소를 통일 인터페이스로 다룬다.
//!
//! 로컬 **스트리밍 열거**(FR-A1) 초안 + 저장소 공급자 추상화(스텁).

use std::fs;
use std::io;
use std::path::Path;
use std::time::SystemTime;

use nexa_core::FileKind;

/// 디렉터리 항목. 이름·종류 + 기본 메타데이터(크기·수정시각·속성).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub kind: FileKind,
    pub size: u64,
    pub modified: Option<SystemTime>,
    /// Windows 파일 속성 비트(FILE_ATTRIBUTE_*). Windows 외에는 0.
    /// 열거 시 이미 조회한 메타데이터에서 꺼내므로 추가 syscall이 없다(숨김 필터의 무료 원천).
    pub attrs: u32,
}

/// 열거 메타데이터에서 Windows 파일 속성 비트를 꺼낸다(비Windows=0).
#[cfg(windows)]
fn file_attrs(m: &fs::Metadata) -> u32 {
    use std::os::windows::fs::MetadataExt;
    m.file_attributes()
}

#[cfg(not(windows))]
fn file_attrs(_m: &fs::Metadata) -> u32 {
    0
}

/// 로컬 디렉터리를 **스트리밍 열거**한다 — 엔트리를 도착하는 대로 순차 산출.
///
/// 전체 스캔을 기다리지 않고 점진 처리(가상화 렌더·인라인 트리 펼침의 기반, FR-A1).
/// 반환 이터레이터의 각 항목은 개별 `Result` — 한 엔트리의 실패가 전체 열거를 막지 않는다.
/// 메타데이터 조회 실패(권한 등)는 격리하여 엔트리는 산출하되 크기/시각만 기본값으로 둔다.
pub fn read_dir_entries(
    path: impl AsRef<Path>,
) -> io::Result<impl Iterator<Item = io::Result<Entry>>> {
    let iter = fs::read_dir(path)?.map(|res| {
        let dirent = res?;
        let file_type = dirent.file_type()?;
        let kind = if file_type.is_symlink() {
            FileKind::Symlink
        } else if file_type.is_dir() {
            FileKind::Dir
        } else {
            FileKind::File
        };
        let (size, modified, attrs) = match dirent.metadata() {
            Ok(m) => (m.len(), m.modified().ok(), file_attrs(&m)),
            Err(_) => (0, None, 0),
        };
        Ok(Entry {
            name: dirent.file_name().to_string_lossy().into_owned(),
            kind,
            size,
            modified,
            attrs,
        })
    });
    Ok(iter)
}

/// 저장소 공급자 추상화. (로컬/SFTP/S3/클라우드)
///
/// 후속 단위에서 `list`/`stat`/`read`/`watch` 등을 추가한다.
pub trait Provider {
    /// 공급자 스킴 식별자 (예: "local", "sftp", "s3").
    fn scheme(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_holds_kind() {
        let e = Entry {
            name: "a.txt".into(),
            kind: FileKind::File,
            size: 5,
            modified: None,
            attrs: 0,
        };
        assert_eq!(e.kind, FileKind::File);
        assert_eq!(e.name, "a.txt");
        assert_eq!(e.size, 5);
    }

    #[test]
    fn read_dir_entries_streams_local() {
        // 격리된 임시 디렉터리 생성(파일 1 + 하위 폴더 1)
        let base = std::env::temp_dir().join(format!("nexa_vfs_stream_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join("a.txt"), b"hello").unwrap();
        fs::create_dir(base.join("sub")).unwrap();

        let mut entries: Vec<Entry> = read_dir_entries(&base)
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        // 정리(assert 전에 수행 → 실패해도 임시폴더 잔류 방지)
        fs::remove_dir_all(&base).unwrap();

        assert_eq!(entries.len(), 2);
        let file = entries.iter().find(|e| e.name == "a.txt").unwrap();
        assert_eq!(file.kind, FileKind::File);
        assert_eq!(file.size, 5);
        let sub = entries.iter().find(|e| e.name == "sub").unwrap();
        assert_eq!(sub.kind, FileKind::Dir);
    }

    #[test]
    fn read_dir_entries_missing_path_errors() {
        let missing = std::env::temp_dir().join("nexa_vfs_does_not_exist_zzz");
        assert!(read_dir_entries(missing).is_err());
    }
}
