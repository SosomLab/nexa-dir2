//! 압축 파일 **목록 읽기**(X-46 — 설계 SSOT [docs/28](../../../../docs/28-archive-preview.md)).
//!
//! 미리보기는 **압축을 풀지 않는다** — 대부분의 포맷은 목록(중앙 디렉터리·헤더
//! 테이블)이 비압축 평문이라, 코덱 없이도 항목·크기·시각·암호화 여부를 읽을 수
//! 있다. 이 모듈은 그 "목록 계층"만 담당하며 결과는 [`Listing`] 하나로 통일된다
//! (표시·그리드·플러그인 ABI가 모두 이 모델을 공유).
//!
//! ## 확장 규약(사용자 지시 08-24 — "설계상 확장이 용이하도록")
//!
//! 새 포맷 추가 = **파일 1개 + [`FORMATS`] 한 줄**. 그 외 어떤 곳도 고치지 않는다:
//!
//! ```text
//! archive/myfmt.rs  →  impl ArchiveFormat for MyFmt { id/label/exts/sniff/list }
//! archive/mod.rs    →  FORMATS 배열에 &myfmt::MyFmt 추가
//! ```
//!
//! 확장자 목록·판정(sniff)·미리보기 라우팅·그리드 컬럼·플러그인 폴백은 전부
//! 레지스트리에서 파생되므로 자동으로 따라온다. 내장이 못 읽는 포맷(코덱 필요 =
//! 7z 압축 헤더 등)은 [`ArchiveError::NeedsCodec`]로 **플러그인 담당**임을 알린다.
//!
//! ## 격리·상한
//!
//! 손상·악의적 파일 방어: 항목 수 [`MAX_ENTRIES`]·이름 [`MAX_NAME`]·1회 읽기
//! [`MAX_CHUNK`] 상한, 오프셋은 전부 검사 후 접근(패닉 없는 `Option` 경로),
//! 경로는 [`normalize_path`]로 탈출(`..`·절대·드라이브)을 차단하고 `suspicious`로
//! 표시한다(표시만 — 추출은 별도 기능).

use nexa_core::secret::Secret;
use std::path::Path;

pub mod cab;
pub mod rar;
pub mod sevenz;
pub mod stream;
pub mod tar;
pub mod zip;

/// 목록 항목 상한(초과 = `truncated` 표시 — 창/그리드 보호).
pub const MAX_ENTRIES: usize = 50_000;
/// 항목 이름 바이트 상한.
pub const MAX_NAME: usize = 4096;
/// 1회 읽기 상한(중앙 디렉터리 등 — 64MB).
pub const MAX_CHUNK: usize = 64 * 1024 * 1024;

/// 압축 항목 1개 — 포맷 중립 모델(그리드 컬럼·플러그인 ABI 공통).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ArchiveEntry {
    /// `/` 구분 상대 경로(정규화됨 — [`normalize_path`]).
    pub path: String,
    pub is_dir: bool,
    /// 원본 크기(모르면 `None` — 단일 스트림 포맷 등).
    pub size: Option<u64>,
    /// 압축 후 크기.
    pub packed: Option<u64>,
    /// 수정 시각(Unix 초, UTC). 모르면 `None`.
    pub modified: Option<i64>,
    /// 이 항목이 암호화되어 있는가(내용 — 이름은 목록에 보인다).
    pub encrypted: bool,
    /// 압축 방식 표시명(`Store`·`Deflate`·`LZMA`…). 모르면 빈 문자열.
    pub method: String,
    pub crc32: Option<u32>,
    /// 경로 탈출 시도(`..`·절대 경로·드라이브)를 정규화로 막은 항목 — 표시 경고용.
    pub suspicious: bool,
}

impl ArchiveEntry {
    /// 압축률(%) — 원본 대비 절감 비율. 크기 미상·0바이트는 `None`.
    pub fn ratio(&self) -> Option<u32> {
        let (s, p) = (self.size?, self.packed?);
        if s == 0 {
            return None;
        }
        Some(100 - (p.min(s) as f64 / s as f64 * 100.0).round() as u32)
    }

    /// 표시용 이름(마지막 경로 요소).
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    /// 상위 폴더 경로(최상위면 빈 문자열).
    pub fn parent(&self) -> &str {
        match self.path.rfind('/') {
            Some(i) => &self.path[..i],
            None => "",
        }
    }
}

/// 압축 파일 1개의 목록 결과.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Listing {
    /// 포맷 id(레지스트리 — `zip`·`tar`…).
    pub format: String,
    /// 포맷 표시명(`ZIP`·`RAR 5`…).
    pub label: String,
    pub entries: Vec<ArchiveEntry>,
    /// 상한 초과로 잘렸는가.
    pub truncated: bool,
    /// 아카이브 주석(있으면).
    pub comment: Option<String>,
    /// 항목이 하나라도 암호화되어 있는가.
    pub has_encrypted: bool,
    /// 목록(헤더) 자체가 암호화되어 암호 없이는 못 읽는 아카이브였는가.
    pub header_encrypted: bool,
    /// 분할(멀티볼륨) 아카이브.
    pub multivolume: bool,
    /// 솔리드 압축(항목 단위 추출 불가).
    pub solid: bool,
}

impl Listing {
    /// 총 원본/압축 크기 합(모르는 항목은 0으로 취급).
    pub fn totals(&self) -> (u64, u64) {
        self.entries
            .iter()
            .filter(|e| !e.is_dir)
            .fold((0, 0), |(s, p), e| {
                (s + e.size.unwrap_or(0), p + e.packed.unwrap_or(0))
            })
    }

    /// (파일 수, 폴더 수).
    pub fn counts(&self) -> (usize, usize) {
        let d = self.entries.iter().filter(|e| e.is_dir).count();
        (self.entries.len() - d, d)
    }
}

/// 목록 실패 사유 — 호스트가 사용자 안내(암호 입력·플러그인 필요)로 번역한다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchiveError {
    /// 알려진 압축 포맷이 아니다(시그니처·확장자 모두 불일치).
    NotArchive,
    /// 목록 자체가 암호화 — **암호 입력 필요**(호스트가 프롬프트 후 재시도).
    PasswordRequired,
    /// 암호가 틀렸다(복호 검증 실패).
    WrongPassword,
    /// 포맷은 알지만 목록을 읽으려면 코덱이 필요하다(플러그인 담당).
    /// `.0` = 포맷 표시명, `.1` = 필요한 코덱 표시명.
    NeedsCodec(String, String),
    /// 구조 손상.
    Corrupt(String),
    /// 입출력 실패.
    Io(String),
}

/// 임의 위치 읽기 원본 — 파일·메모리·(후속) 원격 스트림을 같은 계약으로 다룬다.
pub trait ReadAt {
    fn size(&self) -> u64;
    /// `off`부터 `buf`를 채운다(파일 끝이면 짧게). 반환 = 실제 읽은 바이트.
    fn read_at(&self, off: u64, buf: &mut [u8]) -> Result<usize, ArchiveError>;
}

/// 로컬 파일 원본.
pub struct FileSource {
    f: std::cell::RefCell<std::fs::File>,
    size: u64,
}

impl FileSource {
    pub fn open(path: &Path) -> Result<Self, ArchiveError> {
        let f = std::fs::File::open(path).map_err(|e| ArchiveError::Io(e.to_string()))?;
        let size = f.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(FileSource {
            f: std::cell::RefCell::new(f),
            size,
        })
    }
}

impl ReadAt for FileSource {
    fn size(&self) -> u64 {
        self.size
    }
    fn read_at(&self, off: u64, buf: &mut [u8]) -> Result<usize, ArchiveError> {
        use std::io::{Read, Seek, SeekFrom};
        if off >= self.size {
            return Ok(0);
        }
        let mut f = self.f.borrow_mut();
        f.seek(SeekFrom::Start(off))
            .map_err(|e| ArchiveError::Io(e.to_string()))?;
        let want = buf.len().min((self.size - off) as usize);
        let mut got = 0;
        while got < want {
            match f.read(&mut buf[got..want]) {
                Ok(0) => break,
                Ok(n) => got += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(ArchiveError::Io(e.to_string())),
            }
        }
        Ok(got)
    }
}

/// 메모리 원본(테스트·이미 읽어 둔 버퍼).
pub struct SliceSource<'a>(pub &'a [u8]);

impl ReadAt for SliceSource<'_> {
    fn size(&self) -> u64 {
        self.0.len() as u64
    }
    fn read_at(&self, off: u64, buf: &mut [u8]) -> Result<usize, ArchiveError> {
        let off = off.min(self.0.len() as u64) as usize;
        let n = buf.len().min(self.0.len() - off);
        buf[..n].copy_from_slice(&self.0[off..off + n]);
        Ok(n)
    }
}

/// 정확히 `len`바이트 읽기(부족 = `Corrupt`). 상한 [`MAX_CHUNK`].
pub fn read_exact_at(src: &dyn ReadAt, off: u64, len: usize) -> Result<Vec<u8>, ArchiveError> {
    if len > MAX_CHUNK {
        return Err(ArchiveError::Corrupt("읽기 상한 초과".into()));
    }
    let mut buf = vec![0u8; len];
    let n = src.read_at(off, &mut buf)?;
    if n != len {
        return Err(ArchiveError::Corrupt(format!("{off}+{len} 범위 부족")));
    }
    Ok(buf)
}

/// 파일 앞부분(시그니처 판정용 — 최대 512B).
pub fn read_head(src: &dyn ReadAt) -> Vec<u8> {
    let mut buf = vec![0u8; 512];
    let n = src.read_at(0, &mut buf).unwrap_or(0);
    buf.truncate(n);
    buf
}

// ── 리틀엔디언 안전 읽기(범위 밖 = None — 손상 파일에서 패닉 금지) ──

pub(crate) fn u16le(b: &[u8], off: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(off..off + 2)?.try_into().ok()?))
}
pub(crate) fn u32le(b: &[u8], off: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(off..off + 4)?.try_into().ok()?))
}
pub(crate) fn u64le(b: &[u8], off: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(off..off + 8)?.try_into().ok()?))
}

/// 이름 바이트 디코더 훅(호스트 주입) — UTF-8이 아닌 레거시 이름
/// (CP949·CP932·CP437 등)은 OS 코드페이지 변환이 정확하다. 미주입 시
/// UTF-8 → CP437 순으로 폴백한다([`decode_name`]).
static NAME_DECODER: std::sync::OnceLock<NameDecoder> = std::sync::OnceLock::new();

/// 이름 디코더 시그니처 — 실패(코드페이지 변환 불가) = `None`.
pub type NameDecoder = fn(&[u8]) -> Option<String>;

/// 호스트(Windows = `MultiByteToWideChar(CP_ACP)`)가 시작 시 1회 주입.
pub fn set_name_decoder(f: NameDecoder) {
    let _ = NAME_DECODER.set(f);
}

/// CP437 상위 128자(내장 폴백 — 서구권 구형 zip 이름).
const CP437_HIGH: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å', 'É', 'æ', 'Æ',
    'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ', 'á', 'í', 'ó', 'ú', 'ñ', 'Ñ',
    'ª', 'º', '¿', '⌐', '¬', '½', '¼', '¡', '«', '»', '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕',
    '╣', '║', '╗', '╝', '╜', '╛', '┐', '└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦',
    '╠', '═', '╬', '╧', '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐',
    '▀', 'α', 'ß', 'Γ', 'π', 'Σ', 'σ', 'µ', 'τ', 'Φ', 'Θ', 'Ω', 'δ', '∞', 'φ', 'ε', '∩', '≡', '±',
    '≥', '≤', '⌠', '⌡', '÷', '≈', '°', '∙', '·', '√', 'ⁿ', '²', '■', '\u{a0}',
];

/// 항목 이름 디코드 — `utf8` 플래그가 서면 UTF-8 확정, 아니면
/// UTF-8 검증 → 호스트 디코더 → CP437 순.
pub fn decode_name(bytes: &[u8], utf8: bool) -> String {
    if utf8 {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    if let Some(f) = NAME_DECODER.get() {
        if let Some(s) = f(bytes) {
            return s;
        }
    }
    bytes
        .iter()
        .map(|&b| {
            if b < 0x80 {
                b as char
            } else {
                CP437_HIGH[(b - 0x80) as usize]
            }
        })
        .collect()
}

/// 경로 정규화 — `\` → `/`, 절대·드라이브·`..` 제거(zip slip 차단).
/// 반환 = (정규 경로, 탈출 시도 여부).
pub fn normalize_path(raw: &str) -> (String, bool) {
    let mut suspicious = raw.starts_with('/') || raw.starts_with('\\');
    let mut s = raw.replace('\\', "/");
    if s.len() >= 3 && s.as_bytes()[0].is_ascii_alphabetic() && s[1..3] == *":/" {
        s = s[3..].to_string(); // "C:/..." 드라이브 절대 경로
        suspicious = true;
    }
    let mut parts: Vec<&str> = Vec::new();
    for part in s.split('/') {
        match part {
            "" | "." => {}
            // 상위 탈출은 흡수(표시만 — 구조는 아카이브 내부에 가둔다)
            ".." => suspicious = true,
            p => parts.push(p),
        }
    }
    let mut out = parts.join("/");
    if out.len() > MAX_NAME {
        out.truncate(MAX_NAME);
    }
    (out, suspicious)
}

// ── 시각 변환 ──

/// (y, m, d, h, mi, s) → Unix 초(UTC 해석 — 아카이브 포맷 대부분이 시간대를
/// 남기지 않으므로 값 그대로 옮긴다).
pub fn ymd_hms_to_unix(y: i64, m: i64, d: i64, h: i64, mi: i64, s: i64) -> i64 {
    // days_from_civil(Howard Hinnant) — 1970-01-01 기준 일수
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = if y2 >= 0 { y2 } else { y2 - 399 } / 400;
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    days * 86_400 + h * 3600 + mi * 60 + s
}

/// DOS 날짜/시간(zip·cab·rar4) → Unix 초. `date`·`time` = 16비트 필드.
pub fn dos_to_unix(date: u16, time: u16) -> Option<i64> {
    if date == 0 {
        return None;
    }
    let (y, m, d) = (
        1980 + (date >> 9) as i64,
        ((date >> 5) & 0xF) as i64,
        (date & 0x1F) as i64,
    );
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(ymd_hms_to_unix(
        y,
        m,
        d,
        (time >> 11) as i64,
        ((time >> 5) & 0x3F) as i64,
        ((time & 0x1F) * 2) as i64,
    ))
}

/// Windows FILETIME(100ns, 1601 기준) → Unix 초.
pub fn filetime_to_unix(ft: u64) -> Option<i64> {
    if ft == 0 {
        return None;
    }
    Some((ft / 10_000_000) as i64 - 11_644_473_600)
}

/// 목록 옵션 — 암호는 **전달만** 하고 어디에도 남기지 않는다(Secret 규약).
#[derive(Default)]
pub struct ListOpts<'a> {
    pub password: Option<&'a Secret>,
    /// 항목 상한(0 = [`MAX_ENTRIES`]).
    pub limit: usize,
    /// 아카이브 파일명(확장자 포함) — 단일 스트림 포맷이 안쪽 이름을 만드는 데 쓴다.
    pub name_hint: &'a str,
}

impl ListOpts<'_> {
    pub(crate) fn limit(&self) -> usize {
        if self.limit == 0 {
            MAX_ENTRIES
        } else {
            self.limit.min(MAX_ENTRIES)
        }
    }
}

/// 포맷 리더 계약 — **새 포맷 = 이 트레이트 구현 1개 + [`FORMATS`] 한 줄**.
pub trait ArchiveFormat: Sync {
    /// 안정 식별자(설정·플러그인 매핑 키 — 개명 금지).
    fn id(&self) -> &'static str;
    /// 표시명(`ZIP`·`RAR 5`…).
    fn label(&self) -> &'static str;
    /// 담당 확장자(소문자·점 없음). 미리보기 라우팅이 이 목록에서 파생된다.
    fn exts(&self) -> &'static [&'static str];
    /// 시그니처 판정(확장자보다 우선) — `head` = 파일 앞 512B.
    fn sniff(&self, head: &[u8], src: &dyn ReadAt) -> bool;
    /// 목록 읽기.
    fn list(&self, src: &dyn ReadAt, opts: &ListOpts) -> Result<Listing, ArchiveError>;
}

/// 등록된 포맷(판정 순서 = 이 순서). **확장 지점** — 새 포맷은 여기 한 줄.
pub static FORMATS: &[&dyn ArchiveFormat] = &[
    &zip::Zip,
    &sevenz::SevenZ,
    &rar::Rar,
    &cab::Cab,
    &tar::Tar,
    &stream::Gzip,
    &stream::SingleStream,
];

/// id로 포맷 조회.
pub fn format_by_id(id: &str) -> Option<&'static dyn ArchiveFormat> {
    FORMATS.iter().copied().find(|f| f.id() == id)
}

/// 전 포맷의 확장자 합집합(미리보기 공급자 선언용 — 중복 제거·정렬).
pub fn all_exts() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = FORMATS
        .iter()
        .flat_map(|f| f.exts().iter().copied())
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// 확장자가 알려진 압축 포맷인가(라우팅 1차 판정).
pub fn is_archive_ext(ext: &str) -> bool {
    let e = ext.trim_start_matches('.').to_ascii_lowercase();
    FORMATS.iter().any(|f| f.exts().contains(&e.as_str()))
}

/// 원본에 맞는 포맷 결정 — **시그니처 우선**, 실패 시 확장자(`ext_hint`) 폴백.
pub fn detect(src: &dyn ReadAt, ext_hint: &str) -> Option<&'static dyn ArchiveFormat> {
    let head = read_head(src);
    if let Some(f) = FORMATS.iter().copied().find(|f| f.sniff(&head, src)) {
        return Some(f);
    }
    let e = ext_hint.trim_start_matches('.').to_ascii_lowercase();
    FORMATS
        .iter()
        .copied()
        .find(|f| f.exts().contains(&e.as_str()))
}

/// 목록 읽기 진입점(원본 임의 위치 읽기 계약 — 로컬 파일·메모리 공용).
pub fn list_from(
    src: &dyn ReadAt,
    ext_hint: &str,
    opts: &ListOpts,
) -> Result<Listing, ArchiveError> {
    let f = detect(src, ext_hint).ok_or(ArchiveError::NotArchive)?;
    f.list(src, opts)
}

/// 로컬 경로 목록 읽기(호스트 표준 경로) — 파일명을 `name_hint`로 이어 준다.
pub fn list_path(path: &Path, opts: &ListOpts) -> Result<Listing, ArchiveError> {
    let src = FileSource::open(path)?;
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let o = ListOpts {
        password: opts.password,
        limit: opts.limit,
        name_hint: &name,
    };
    list_from(&src, &ext, &o)
}

/// 목록 마감 공통 처리 — 상한 절단·경로 정렬·중복 제거 + 암호화 집계.
pub(crate) fn finish(mut l: Listing, limit: usize) -> Listing {
    if l.entries.len() > limit {
        l.entries.truncate(limit);
        l.truncated = true;
    }
    l.entries.sort_by(|a, b| a.path.cmp(&b.path));
    l.entries
        .dedup_by(|a, b| a.path == b.path && a.is_dir == b.is_dir);
    l.has_encrypted = l.entries.iter().any(|e| e.encrypted);
    l
}

/// 항목 경로에서 **암시된 폴더**를 채운다(zip/tar은 폴더 항목이 없을 수 있다 —
/// 트리 그리드가 상위 노드를 만들 수 있게).
pub fn with_implied_dirs(entries: &[ArchiveEntry]) -> Vec<ArchiveEntry> {
    let have: std::collections::HashSet<&str> = entries
        .iter()
        .filter(|e| e.is_dir)
        .map(|e| e.path.as_str())
        .collect();
    let mut added: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut extra: Vec<ArchiveEntry> = Vec::new();
    for e in entries {
        let mut parent = e.parent();
        while !parent.is_empty() {
            if have.contains(parent) || added.contains(parent) {
                break;
            }
            added.insert(parent.to_string());
            extra.push(ArchiveEntry {
                path: parent.to_string(),
                is_dir: true,
                ..Default::default()
            });
            parent = match parent.rfind('/') {
                Some(i) => &parent[..i],
                None => "",
            };
        }
    }
    let mut out = entries.to_vec();
    out.extend(extra);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_blocks_escape_and_flags_it() {
        assert_eq!(normalize_path("a/b.txt"), ("a/b.txt".into(), false));
        assert_eq!(normalize_path("a\\b\\c"), ("a/b/c".into(), false));
        assert_eq!(
            normalize_path("../../etc/passwd"),
            ("etc/passwd".into(), true)
        );
        assert_eq!(normalize_path("/abs/x"), ("abs/x".into(), true));
        assert_eq!(normalize_path("C:/win/x"), ("win/x".into(), true));
        assert_eq!(normalize_path("./a/./b"), ("a/b".into(), false));
    }

    #[test]
    fn dos_time_matches_known_values() {
        // 1980-01-01 00:00:00 = date 0x0021, time 0
        assert_eq!(dos_to_unix(0x0021, 0), Some(315_532_800));
        let date = (46u16 << 9) | (8 << 5) | 24; // 2026-08-24
        let time = (13u16 << 11) | (45 << 5) | 15; // 13:45:30(2초 단위)
        assert_eq!(
            dos_to_unix(date, time),
            Some(ymd_hms_to_unix(2026, 8, 24, 13, 45, 30))
        );
        assert_eq!(dos_to_unix(0, 0), None);
    }

    #[test]
    fn unix_epoch_anchors() {
        assert_eq!(ymd_hms_to_unix(1970, 1, 1, 0, 0, 0), 0);
        assert_eq!(ymd_hms_to_unix(2000, 3, 1, 0, 0, 0), 951_868_800);
        assert_eq!(filetime_to_unix(116_444_736_000_000_000), Some(0));
        assert_eq!(filetime_to_unix(0), None);
    }

    #[test]
    fn ratio_and_name_helpers() {
        let e = ArchiveEntry {
            path: "docs/readme.md".into(),
            size: Some(1000),
            packed: Some(250),
            ..Default::default()
        };
        assert_eq!(e.ratio(), Some(75));
        assert_eq!(e.name(), "readme.md");
        assert_eq!(e.parent(), "docs");
    }

    #[test]
    fn implied_dirs_are_filled_once() {
        let e = vec![
            ArchiveEntry {
                path: "a/b/c.txt".into(),
                ..Default::default()
            },
            ArchiveEntry {
                path: "a/b/d.txt".into(),
                ..Default::default()
            },
        ];
        let out = with_implied_dirs(&e);
        let dirs: Vec<&str> = out
            .iter()
            .filter(|x| x.is_dir)
            .map(|x| x.path.as_str())
            .collect();
        assert_eq!(dirs, ["a", "a/b"]);
        assert_eq!(out.len(), 4);
    }

    #[test]
    fn registry_exts_are_unique_and_routable() {
        let exts = all_exts();
        let mut sorted = exts.clone();
        sorted.dedup();
        assert_eq!(exts.len(), sorted.len(), "확장자 중복 등록 금지");
        assert!(is_archive_ext("zip") && is_archive_ext(".ZIP"));
        assert!(!is_archive_ext("txt"));
        for f in FORMATS {
            assert!(!f.exts().is_empty(), "{}: 확장자 선언 필요", f.id());
            assert!(format_by_id(f.id()).is_some());
        }
    }

    #[test]
    fn name_decode_falls_back_to_cp437() {
        assert_eq!(decode_name("한글.txt".as_bytes(), true), "한글.txt");
        assert_eq!(decode_name(&[0x41, 0x80, 0x42], false), "AÇB");
    }
}
