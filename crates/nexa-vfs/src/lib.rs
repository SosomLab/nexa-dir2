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
    /// 링크형 항목의 실제 대상 경로(X-36 — 클라우드 연결 등 **표시명 ≠ 경로**).
    /// `Some`이면 트리 노드 경로는 `parent.join(name)` 대신 이 값을 쓴다.
    /// 일반 열거(`read_dir_entries`)·드라이브 항목은 `None`.
    pub target: Option<String>,
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
            target: None,
        })
    });
    Ok(iter)
}

/// 가상 최상위 "내 PC"의 **센티널 경로**(X-17). 콜론이 파일명에 불가한 문자라
/// 실제 경로와 충돌하지 않는다. 이 경로를 루트로 열면 드라이브 목록이 열거되고,
/// 항목 이름이 `C:\` 형태(절대 경로)라 `join` 시 부모를 대체 — 진입이 실 경로가 된다.
pub const MY_PC: &str = "::PC::";

/// `path`가 가상 최상위(내 PC)인가.
pub fn is_virtual_root(path: impl AsRef<Path>) -> bool {
    path.as_ref().as_os_str() == MY_PC
}

/// 존재하는 드라이브 루트 열거(X-17 — std만: `A:\`~`Z:\` metadata 프로브,
/// Win32 API 불요 = 크레이트 플랫폼 중립 유지. 비Windows에선 자연히 빈 목록).
/// 이름 = `C:\`(절대 경로 형태 — [`MY_PC`] 문서 참조). 볼륨명·용량 데코는 β(Win32).
pub fn drive_entries() -> Vec<Entry> {
    let mut out = Vec::new();
    for c in b'A'..=b'Z' {
        let root = format!("{}:\\", c as char);
        if fs::metadata(&root).is_ok() {
            out.push(Entry {
                name: root,
                kind: FileKind::Dir,
                size: 0,
                modified: None, // 드라이브는 수정일 개념 없음 — 표시층에서 빈 셀
                attrs: 0,
                target: None,
            });
        }
    }
    out
}

/// 가상 최상위에 합류할 **추가 루트**(X-36 — 클라우드 연결 등 앱 정의 항목) 전역 등록부.
/// (표시명, 실경로) 쌍 — 표시명은 사용자 라벨, 진입은 [`Entry::target`] 경유 실경로.
/// 앱(nexa-app)이 설정 로드/변경 시 갱신하고, [`MY_PC`] 열거가 드라이브 뒤에 합류한다.
/// 탐지·직렬화는 앱 소관(이 크레이트는 플랫폼 중립 유지 — 검토서 26 §2-4).
static EXTRA_ROOTS: std::sync::RwLock<Vec<(String, String)>> = std::sync::RwLock::new(Vec::new());

/// 추가 루트 목록 교체(전량) — 실존 프로브는 호출자 몫.
pub fn set_extra_roots(roots: Vec<(String, String)>) {
    *EXTRA_ROOTS
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = roots;
}

/// 등록된 추가 루트를 Entry로 열거(가상 최상위 전용 — 드라이브 목록 뒤 합류).
pub fn extra_root_entries() -> Vec<Entry> {
    EXTRA_ROOTS
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .map(|(label, path)| Entry {
            name: label.clone(),
            kind: FileKind::Dir,
            size: 0,
            modified: None,
            attrs: 0,
            target: Some(path.clone()),
        })
        .collect()
}

/// 저장소 공급자 추상화. (로컬/SFTP/S3/클라우드)
///
/// 후속 단위에서 `list`/`stat`/`read`/`watch` 등을 추가한다.
pub trait Provider {
    /// 공급자 스킴 식별자 (예: "local", "sftp", "s3").
    fn scheme(&self) -> &str;
}

/// 클라우드 API 연결 경로의 **센티널 접두사**(X-37 2차 — ADR-0006 §3).
/// 형식 = `::CLOUD:<연결 인덱스>::<클라우드 내부 경로>`
/// (예: `::CLOUD:0::` = 그 연결의 루트 · `::CLOUD:0::/Documents`).
/// [`MY_PC`]와 같은 규약 — 콜론은 파일명에 불가해 실경로와 충돌하지 않는다.
pub const CLOUD_PREFIX: &str = "::CLOUD:";

/// `path`가 클라우드 API 경로면 `(연결 인덱스, 내부 경로)`. 아니면 `None`.
/// 내부 경로는 빈 문자열(루트) 또는 `/`로 시작하는 경로.
pub fn cloud_parts(path: impl AsRef<Path>) -> Option<(usize, String)> {
    let s = path.as_ref().to_str()?;
    let rest = s.strip_prefix(CLOUD_PREFIX)?;
    let (idx, tail) = rest.split_once("::")?;
    Some((idx.parse().ok()?, tail.to_string()))
}

/// 클라우드 연결 루트 경로 문자열.
pub fn cloud_root(idx: usize) -> String {
    format!("{CLOUD_PREFIX}{idx}::")
}

/// 클라우드 하위 경로(부모 + 자식 이름) — 트리 `join` 대체용.
pub fn cloud_child(idx: usize, parent_inner: &str, name: &str) -> String {
    format!("{CLOUD_PREFIX}{idx}::{parent_inner}/{name}")
}

/// 클라우드 연결의 **표시 라벨**(등록된 추가 루트에서 조회 — 예 "OneDrive – a@b.com").
/// 미등록이면 `None`.
pub fn cloud_label(idx: usize) -> Option<String> {
    let root = cloud_root(idx);
    EXTRA_ROOTS
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .find(|(_, p)| *p == root)
        .map(|(label, _)| label.clone())
}

/// 사람이 읽는 경로 표기(X-37 — 센티널 노출 금지).
/// `::CLOUD:0::/Docs/a.txt` → `OneDrive – a@b.com\Docs\a.txt`.
/// 클라우드 경로가 아니면 `None`(호출자가 원래 표기를 쓴다).
pub fn cloud_display(path: impl AsRef<Path>) -> Option<String> {
    let (idx, inner) = cloud_parts(path)?;
    let label = cloud_label(idx).unwrap_or_else(|| format!("Cloud {idx}"));
    if inner.is_empty() {
        return Some(label);
    }
    Some(format!("{label}{}", inner.replace('/', "\\")))
}

/// 클라우드 경로의 **마지막 세그먼트**(탭 제목용). 루트면 연결 라벨.
pub fn cloud_leaf(path: impl AsRef<Path>) -> Option<String> {
    let (idx, inner) = cloud_parts(path)?;
    match inner.rsplit('/').next().filter(|s| !s.is_empty()) {
        Some(name) => Some(name.to_string()),
        None => Some(cloud_label(idx).unwrap_or_else(|| format!("Cloud {idx}"))),
    }
}

/// 클라우드 경로의 **부모**(상위 이동). 루트의 부모 = 내 PC.
pub fn cloud_parent(path: impl AsRef<Path>) -> Option<String> {
    let (idx, inner) = cloud_parts(path)?;
    if inner.is_empty() {
        return Some(MY_PC.to_string());
    }
    let cut = inner.rfind('/').unwrap_or(0);
    Some(format!("{CLOUD_PREFIX}{idx}::{}", &inner[..cut]))
}

/// 클라우드 열거 콜백 — 앱(nexa-app)이 등록한다. 이 크레이트는 네트워크를 모른다.
/// 반환 `None` = 아직 로딩 중(빈 목록으로 표시하고 완료 통지가 재로드).
type CloudLister = Box<dyn Fn(usize, &str) -> Option<Vec<Entry>> + Send + Sync>;
static CLOUD_LISTER: std::sync::RwLock<Option<CloudLister>> = std::sync::RwLock::new(None);

/// 클라우드 열거 콜백 등록(앱 기동 시 1회).
pub fn set_cloud_lister(f: CloudLister) {
    *CLOUD_LISTER
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(f);
}

/// 클라우드 경로 열거 — 등록된 콜백에 위임. 클라우드 경로가 아니면 `None`.
/// 콜백 미등록·로딩 중이면 **빈 목록**(트리는 정상 동작하고 완료 후 재로드된다).
pub fn cloud_entries(path: impl AsRef<Path>) -> Option<Vec<Entry>> {
    let (idx, inner) = cloud_parts(path)?;
    let guard = CLOUD_LISTER
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    Some(match guard.as_ref() {
        Some(f) => f(idx, &inner).unwrap_or_default(),
        None => Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_root_and_drive_entries() {
        assert!(is_virtual_root(MY_PC));
        assert!(!is_virtual_root("C:\\"));
        // 드라이브 항목: 이름 = `X:\`(절대) → 센티널과 join하면 부모가 대체된다
        #[cfg(windows)]
        {
            let drives = drive_entries();
            assert!(!drives.is_empty(), "Windows에는 드라이브 1개 이상");
            for d in &drives {
                assert!(d.name.len() == 3 && d.name.ends_with(":\\"), "{}", d.name);
                assert_eq!(d.kind, FileKind::Dir);
                assert_eq!(
                    Path::new(MY_PC).join(&d.name),
                    Path::new(&d.name),
                    "절대 이름 join = 실 드라이브 경로"
                );
            }
        }
    }

    #[test]
    fn entry_holds_kind() {
        let e = Entry {
            name: "a.txt".into(),
            kind: FileKind::File,
            size: 5,
            modified: None,
            attrs: 0,
            target: None,
        };
        assert_eq!(e.kind, FileKind::File);
        assert_eq!(e.name, "a.txt");
        assert_eq!(e.size, 5);
    }

    /// X-37 2차: 클라우드 센티널 경로 분해·조립.
    #[test]
    fn cloud_path_parts_and_build() {
        assert_eq!(cloud_parts("::CLOUD:0::"), Some((0, String::new())));
        assert_eq!(
            cloud_parts("::CLOUD:2::/Docs/a.txt"),
            Some((2, "/Docs/a.txt".into()))
        );
        assert_eq!(cloud_parts("C:\\Users"), None);
        assert_eq!(cloud_parts(MY_PC), None);
        assert_eq!(cloud_root(3), "::CLOUD:3::");
        assert_eq!(cloud_child(1, "", "Docs"), "::CLOUD:1::/Docs");
        assert_eq!(cloud_child(1, "/Docs", "a.txt"), "::CLOUD:1::/Docs/a.txt");
        // 조립 → 분해 왕복
        let p = cloud_child(4, "/x", "y");
        assert_eq!(cloud_parts(&p), Some((4, "/x/y".into())));
    }

    /// X-37 5차: 표시명·leaf·부모 — **센티널이 UI에 노출되면 안 된다**.
    #[test]
    fn cloud_display_leaf_and_parent() {
        set_extra_roots(vec![("OneDrive – a@b.com".into(), cloud_root(0))]);
        assert_eq!(cloud_label(0).as_deref(), Some("OneDrive – a@b.com"));
        assert_eq!(cloud_display("::CLOUD:0::").as_deref(), Some("OneDrive – a@b.com"));
        assert_eq!(
            cloud_display("::CLOUD:0::/Docs/a.txt").as_deref(),
            Some("OneDrive – a@b.com\\Docs\\a.txt")
        );
        assert!(cloud_display("C:\\x").is_none(), "일반 경로는 원래 표기 유지");
        // 탭 제목 = 마지막 세그먼트, 루트는 연결 라벨
        assert_eq!(cloud_leaf("::CLOUD:0::/Docs").as_deref(), Some("Docs"));
        assert_eq!(cloud_leaf("::CLOUD:0::").as_deref(), Some("OneDrive – a@b.com"));
        // 상위 이동: 하위 → 부모, 루트 → 내 PC
        assert_eq!(cloud_parent("::CLOUD:0::/Docs/x").as_deref(), Some("::CLOUD:0::/Docs"));
        assert_eq!(cloud_parent("::CLOUD:0::/Docs").as_deref(), Some("::CLOUD:0::"));
        assert_eq!(cloud_parent("::CLOUD:0::").as_deref(), Some(MY_PC));
        assert!(cloud_parent("D:\\a").is_none());
        // 미등록 연결도 패닉 없이 폴백 라벨
        assert_eq!(cloud_display("::CLOUD:9::").as_deref(), Some("Cloud 9"));
        set_extra_roots(Vec::new());
    }

    /// 콜백 미등록이어도 클라우드 경로는 빈 목록(패닉 없음) · 비클라우드는 None.
    #[test]
    fn cloud_entries_without_lister_is_empty() {
        assert_eq!(cloud_entries("::CLOUD:9::").map(|v| v.len()), Some(0));
        assert!(cloud_entries("D:\\tmp").is_none());
    }

    /// X-36: 추가 루트 등록 → 표시명·target 실경로 Entry로 열거.
    #[test]
    fn extra_roots_roundtrip() {
        set_extra_roots(vec![("OneDrive – Test".into(), "C:\\Users\\t\\OneDrive".into())]);
        let e = extra_root_entries();
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].name, "OneDrive – Test");
        assert_eq!(e[0].target.as_deref(), Some("C:\\Users\\t\\OneDrive"));
        assert_eq!(e[0].kind, FileKind::Dir);
        set_extra_roots(Vec::new());
        assert!(extra_root_entries().is_empty());
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
