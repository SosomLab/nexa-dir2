//! 압축 파일 미리보기 공급자(X-46 — 설계 SSOT [docs/28](../../../../docs/28-archive-preview.md)).
//!
//! 목록 읽기는 [`nexa_vfs::archive`](압축 해제 없음)에 위임하고, 여기서는 **호스트
//! 역할**만 한다:
//!
//! - 확장자 선언 = 레지스트리 [`nexa_vfs::archive::all_exts`] 파생(포맷 추가 시 자동)
//! - 결과를 [`PreviewDoc::Archive`]로 올려 **하단 도크 = 요약 텍스트** ·
//!   **독립 창 = 그리드**([`crate::archivewnd`])가 같은 자료를 다르게 표시
//! - 암호는 **세션 메모리에만**([`pw`]) — 설정·로그·디스크 어디에도 기록하지 않는다
//! - 이름 코드페이지 디코더(CP949 등) 주입 — 구형 zip의 한글 이름 대응

use std::path::{Path, PathBuf};

use nexa_core::secret::Secret;
use nexa_vfs::archive::{self, ArchiveError, Listing};

use super::{PreviewDoc, PreviewProvider};
use crate::i18n::{tr, trf};

/// 미리보기 상태 — 실패 사유를 사용자 행동(암호 입력·플러그인 설치)으로 번역한다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchiveStatus {
    Ok,
    /// 목록 자체가 암호화 — 그리드 창에서 암호를 받는다.
    NeedPassword,
    /// 코덱이 필요해 내장이 못 읽는다(포맷 표시명, 코덱 표시명).
    NeedPlugin(String, String),
    /// 그 밖의 실패(손상·입출력) — 사용자 표시 문구.
    Failed(String),
}

/// 압축 미리보기 문서 — 도크·독립 창·플러그인 결과가 공유하는 형태.
#[derive(Debug, Clone)]
pub struct ArchiveDoc {
    /// 대상 아카이브 경로(그리드 창 제목·암호 캐시 키).
    #[allow(dead_code)] // 사용처 = 그리드 창(X-46 2차)
    pub path: PathBuf,
    pub listing: Listing,
    pub status: ArchiveStatus,
    /// 이 결과를 만든 공급자 id(`builtin.archive` 또는 플러그인 id).
    #[allow(dead_code)] // 사용처 = 그리드 창 상태 표시(X-46 2차)
    pub provider: String,
}

impl ArchiveDoc {
    /// 목록을 실제로 보여줄 수 있는가.
    #[allow(dead_code)] // 사용처 = 그리드 창(archivewnd — X-46 2차)
    pub fn is_ok(&self) -> bool {
        self.status == ArchiveStatus::Ok
    }
}

/// 세션 한정 암호 보관 — **메모리에만**, 프로세스가 끝나면 사라진다.
///
/// 사용자 지시(08-24): "입력된 내용은 전달만 하고 기록되거나 Plain으로 노출되지
/// 않도록". 따라서 여기에는 파일 저장 경로가 아예 없고(토큰용 DPAPI 경로와 의도적
/// 분리 — [`crate::secret`]), 값은 [`Secret`](Drop 소거)로만 보관한다.
/// UI 스레드 전용(`thread_local`) — 워커로 새지 않는다.
pub mod pw {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    thread_local! {
        static CACHE: RefCell<HashMap<PathBuf, Secret>> = RefCell::new(HashMap::new());
    }

    /// 이 아카이브에 대해 세션 중 입력받은 암호(없으면 `None`).
    pub fn get(path: &Path) -> Option<Secret> {
        CACHE.with(|c| c.borrow().get(path).cloned())
    }

    /// 성공한 암호를 세션 동안 기억(같은 파일 재조회 시 재입력 방지).
    #[allow(dead_code)] // 사용처 = 암호 프롬프트(X-46 2차)
    pub fn remember(path: &Path, secret: Secret) {
        CACHE.with(|c| {
            c.borrow_mut().insert(path.to_path_buf(), secret);
        });
    }

    /// 한 건 폐기(암호 실패·사용자 요청).
    #[allow(dead_code)] // 사용처 = 암호 프롬프트(X-46 2차)
    pub fn forget(path: &Path) {
        CACHE.with(|c| {
            c.borrow_mut().remove(path);
        });
    }

    /// 전부 폐기(앱 종료·"암호 기억 지우기") — Drop이 각 값을 0으로 덮는다.
    #[allow(dead_code)] // 사용처 = 종료 정리·설정(X-46 2차)
    pub fn forget_all() {
        CACHE.with(|c| c.borrow_mut().clear());
    }

    /// 보관 중인 건수(진단·테스트용 — 값은 노출하지 않는다).
    #[allow(dead_code)] // 진단·테스트 전용
    pub fn len() -> usize {
        CACHE.with(|c| c.borrow().len())
    }
}

/// **활성 암호 주입**(호스트 → 공급자) — [`super::set_dark`]와 같은 규약.
///
/// 공급자 트레이트(`preview(path)`)는 인자가 경로 하나뿐이라, 암호는 호출 직전에
/// 이 슬롯으로 주입하고 호출 직후 비운다(Drop = 소거). 내장 리더와 WASM 플러그인이
/// **같은 경로**로 암호를 받는다 — 저장은 어디에도 하지 않는다.
mod active {
    use super::Secret;
    use std::cell::RefCell;

    thread_local! {
        static ACTIVE: RefCell<Option<Secret>> = const { RefCell::new(None) };
    }

    /// 주입 후 `f` 실행, 종료 시 반드시 비운다(패닉 경로 포함 — 스코프 가드).
    pub fn scoped<R>(pw: Option<Secret>, f: impl FnOnce() -> R) -> R {
        struct Guard;
        impl Drop for Guard {
            fn drop(&mut self) {
                ACTIVE.with(|a| *a.borrow_mut() = None); // Secret Drop = 0 덮기
            }
        }
        ACTIVE.with(|a| *a.borrow_mut() = pw);
        let _g = Guard;
        f()
    }

    /// 활성 암호를 빌려 쓴다(사본을 만들지 않는다).
    pub fn with<R>(f: impl FnOnce(Option<&Secret>) -> R) -> R {
        ACTIVE.with(|a| f(a.borrow().as_ref()))
    }
}

/// 활성 암호 열람(런타임 호스트 API `password` 구현 — wasm.rs).
pub(crate) fn with_active_password<R>(f: impl FnOnce(Option<&Secret>) -> R) -> R {
    active::with(f)
}

/// 활성 암호를 건 채 `f` 실행(호출이 끝나면 슬롯은 비워지고 값은 소거된다).
/// 공급자를 직접 부르는 경로(플러그인 런타임·테스트)가 쓴다.
pub(crate) fn with_password_scope<R>(password: Option<Secret>, f: impl FnOnce() -> R) -> R {
    active::scoped(password, f)
}

/// 공급자 경유 목록 읽기 — **플러그인 우선**(설정 `preview_map`·사용 여부 반영),
/// 없으면 내장. 암호는 활성 슬롯으로 주입해 어느 공급자든 같은 방식으로 받는다.
pub fn read_via(
    path: &Path,
    preview_map: &str,
    disabled: &str,
    password: Option<Secret>,
) -> ArchiveDoc {
    with_password_scope(password, || {
        match super::preview_for(path, preview_map, disabled) {
            PreviewDoc::Archive(doc) => *doc,
            // 압축이 아닌 공급자로 매핑된 경우(사용자 오버라이드) — 내장으로 판단만 전달
            _ => ArchiveDoc {
                path: path.to_path_buf(),
                listing: Listing::default(),
                status: ArchiveStatus::Failed(tr("archive.notArchive")),
                provider: String::new(),
            },
        }
    })
}

/// 목록 읽기 — 세션 암호가 있으면 함께 넘긴다(호출자가 명시 암호를 줄 수도 있다).
pub fn read(path: &Path, password: Option<&Secret>) -> ArchiveDoc {
    // 우선순위: 명시 인자 > 활성 슬롯(호스트 주입) > 세션 캐시
    let injected = password.is_none().then(|| active::with(|p| p.cloned())).flatten();
    let session = password.is_none() && injected.is_none();
    let session = session.then(|| pw::get(path)).flatten();
    let pass = password.or(injected.as_ref()).or(session.as_ref());
    let opts = archive::ListOpts {
        password: pass,
        ..Default::default()
    };
    let (listing, status) = match archive::list_path(path, &opts) {
        Ok(l) => (l, ArchiveStatus::Ok),
        Err(ArchiveError::PasswordRequired) | Err(ArchiveError::WrongPassword) => {
            (Listing::default(), ArchiveStatus::NeedPassword)
        }
        Err(ArchiveError::NeedsCodec(fmt, codec)) => {
            (Listing::default(), ArchiveStatus::NeedPlugin(fmt, codec))
        }
        Err(ArchiveError::NotArchive) => (
            Listing::default(),
            ArchiveStatus::Failed(tr("archive.notArchive")),
        ),
        Err(ArchiveError::Corrupt(why)) | Err(ArchiveError::Io(why)) => {
            (Listing::default(), ArchiveStatus::Failed(why))
        }
    };
    ArchiveDoc {
        path: path.to_path_buf(),
        listing,
        status,
        provider: "builtin.archive".into(),
    }
}

/// 도크(축약 뷰)용 텍스트 — 요약 3줄 + 항목 미리보기. 그리드는 독립 창이 그린다.
pub fn summary_lines(doc: &ArchiveDoc, tz_offset_min: i32, max_rows: usize) -> Vec<String> {
    let mut out = Vec::new();
    match &doc.status {
        ArchiveStatus::NeedPassword => {
            out.push(tr("archive.needPassword"));
            out.push(tr("archive.openHint"));
            return out;
        }
        ArchiveStatus::NeedPlugin(fmt, codec) => {
            out.push(trf("archive.needPlugin", &[fmt, codec]));
            return out;
        }
        ArchiveStatus::Failed(why) => {
            out.push(trf("archive.failed", &[why]));
            return out;
        }
        ArchiveStatus::Ok => {}
    }
    let l = &doc.listing;
    let (files, dirs) = l.counts();
    let (size, packed) = l.totals();
    out.push(trf(
        "archive.summary",
        &[&l.label, &files.to_string(), &dirs.to_string()],
    ));
    let saved = if size > 0 {
        format!("{}%", 100 - (packed.min(size) * 100 / size.max(1)))
    } else {
        "-".into()
    };
    out.push(trf(
        "archive.sizes",
        &[
            &crate::source::human_size(size),
            &crate::source::human_size(packed),
            &saved,
        ],
    ));
    for (flag, key) in [
        (l.has_encrypted, "archive.encrypted"),
        (l.solid, "archive.solid"),
        (l.multivolume, "archive.multivolume"),
        (l.truncated, "archive.truncated"),
    ] {
        if flag {
            out.push(tr(key));
        }
    }
    if let Some(c) = &l.comment {
        out.push(trf("archive.comment", &[c]));
    }
    out.push(tr("archive.openHint"));
    out.push(String::new());
    // 항목 미리보기(도크는 평문 — 정렬은 그리드 창 담당)
    for e in l.entries.iter().take(max_rows) {
        let size = match (e.is_dir, e.size) {
            (true, _) => String::new(),
            (_, Some(s)) => crate::source::human_size(s),
            _ => "-".into(),
        };
        let when = e
            .modified
            .map(|t| fmt_entry_time(t, e.time_is_local, tz_offset_min))
            .unwrap_or_default();
        let lock = if e.encrypted { "🔒 " } else { "" };
        out.push(format!(
            "{lock}{}{}  {size}  {when}",
            e.path,
            if e.is_dir { "/" } else { "" }
        ));
    }
    if l.entries.len() > max_rows {
        out.push(trf(
            "archive.more",
            &[&(l.entries.len() - max_rows).to_string()],
        ));
    }
    out
}

/// 항목 시각 표시 — DOS 계열(현지 벽시계 그대로)은 보정하지 않는다(이중 보정 방지).
pub fn fmt_entry_time(unix_secs: i64, time_is_local: bool, tz_offset_min: i32) -> String {
    let off = if time_is_local { 0 } else { tz_offset_min };
    crate::source::fmt_datetime(unix_secs.saturating_mul(1000), off)
}

/// 내장 압축 공급자 — 확장자는 레지스트리에서 파생(포맷 추가 시 자동 확장).
pub struct BuiltinArchive {
    pub exts: Vec<String>,
}

impl BuiltinArchive {
    pub fn new() -> Self {
        BuiltinArchive {
            exts: archive::all_exts()
                .into_iter()
                .map(str::to_string)
                .collect(),
        }
    }
}

impl PreviewProvider for BuiltinArchive {
    fn id(&self) -> &str {
        "builtin.archive"
    }
    fn exts(&self) -> &[String] {
        &self.exts
    }
    fn preview(&self, path: &Path) -> PreviewDoc {
        PreviewDoc::Archive(Box::new(read(path, None)))
    }
}

/// OS 코드페이지 이름 디코더 주입(구형 zip의 CP949/CP932 이름) — 시작 시 1회.
#[cfg(windows)]
pub fn install_name_decoder() {
    archive::set_name_decoder(|bytes| {
        use windows::Win32::Globalization::{MultiByteToWideChar, CP_ACP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS};
        if bytes.is_empty() {
            return Some(String::new());
        }
        unsafe {
            let n = MultiByteToWideChar(CP_ACP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0), bytes, None);
            if n <= 0 {
                return None;
            }
            let mut buf = vec![0u16; n as usize];
            let n = MultiByteToWideChar(CP_ACP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0), bytes, Some(&mut buf));
            if n <= 0 {
                return None;
            }
            Some(String::from_utf16_lossy(&buf[..n as usize]))
        }
    });
}

#[cfg(not(windows))]
pub fn install_name_decoder() {}

#[cfg(test)]
mod tests {
    use super::*;

    /// 테스트용 최소 ZIP(항목 1개 — 중앙 디렉터리 규약만 충족).
    fn zip_bytes(name: &str) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        out.extend_from_slice(b"PK\x03\x04");
        out.extend_from_slice(&[0u8; 26]);
        let cd_off = out.len() as u32;
        let mut cd: Vec<u8> = Vec::new();
        cd.extend_from_slice(b"PK\x01\x02");
        cd.extend_from_slice(&[0u8; 4]);
        cd.extend_from_slice(&0x800u16.to_le_bytes()); // UTF-8 이름
        cd.extend_from_slice(&[0u8; 10]);
        cd.extend_from_slice(&7u32.to_le_bytes()); // 압축 크기
        cd.extend_from_slice(&10u32.to_le_bytes()); // 원본 크기
        cd.extend_from_slice(&(name.len() as u16).to_le_bytes());
        cd.extend_from_slice(&[0u8; 12]);
        cd.extend_from_slice(&0u32.to_le_bytes()); // 로컬 오프셋
        cd.extend_from_slice(name.as_bytes());
        let cd_size = cd.len() as u32;
        out.extend_from_slice(&cd);
        out.extend_from_slice(b"PK\x05\x06");
        out.extend_from_slice(&[0u8; 4]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_off.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    fn tmp(name: &str, bytes: &[u8]) -> PathBuf {
        let p = std::env::temp_dir().join(format!("nexa_arc_{}_{name}", std::process::id()));
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn archive_ext_routes_to_builtin_provider() {
        let p = tmp("a.zip", &zip_bytes("hello.txt"));
        match super::super::preview_for(&p, "", "") {
            PreviewDoc::Archive(doc) => {
                assert!(doc.is_ok());
                assert_eq!(doc.listing.entries[0].path, "hello.txt");
                assert_eq!(doc.provider, "builtin.archive");
            }
            other => panic!("압축 확장자는 압축 공급자여야 함: {other:?}"),
        }
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn summary_lines_lead_with_format_and_counts() {
        let p = tmp("b.zip", &zip_bytes("docs/readme.md"));
        let doc = read(&p, None);
        let lines = summary_lines(&doc, 540, 10);
        assert!(lines[0].contains("ZIP"), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("docs/readme.md")));
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn failure_status_maps_to_user_action() {
        let p = tmp("c.zip", b"not a zip at all");
        let doc = read(&p, None);
        assert!(matches!(doc.status, ArchiveStatus::Failed(_)));
        assert!(!summary_lines(&doc, 0, 5).is_empty());
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn password_cache_is_memory_only_and_forgettable() {
        let p = PathBuf::from("X:/only-a-key.zip");
        assert!(pw::get(&p).is_none());
        pw::remember(&p, Secret::new(b"pw".to_vec()));
        assert_eq!(pw::get(&p).map(|s| s.expose().to_vec()), Some(b"pw".to_vec()));
        pw::forget(&p);
        assert!(pw::get(&p).is_none());
        pw::remember(&p, Secret::new(b"pw".to_vec()));
        pw::forget_all();
        assert_eq!(pw::len(), 0);
    }

    #[test]
    fn dos_times_are_not_shifted_twice() {
        // 현지 벽시계 그대로인 DOS 시각은 오프셋을 적용하지 않는다
        let t = nexa_vfs::archive::ymd_hms_to_unix(2026, 8, 24, 13, 45, 0);
        assert_eq!(fmt_entry_time(t, true, 540), "2026-08-24 13:45");
        assert_eq!(fmt_entry_time(t, false, 540), "2026-08-24 22:45");
    }
}
