//! ZIP 계열 목록 — **중앙 디렉터리(Central Directory)만 읽는다**(압축 해제 없음).
//!
//! 구조(APPNOTE 6.3.x): `[로컬 헤더+데이터]... [중앙 디렉터리]... [EOCD]`.
//! EOCD를 꼬리에서 역탐색 → (Zip64면 Zip64 EOCD로 승격) → 중앙 디렉터리 레코드
//! 순회. 이름·크기·시각·CRC·암호화 플래그가 전부 평문이라 **암호 없이도 목록은
//! 보인다**(WinZip AES·ZipCrypto 모두 이름은 노출 — 내용만 암호 필요).
//!
//! 예외: **중앙 디렉터리 암호화**(강한 암호화 — 플래그 비트 13)는 목록 자체가
//! 암호화되어 [`ArchiveError::PasswordRequired`]로 올린다.
//!
//! SFX(자기추출 exe 등)처럼 앞에 다른 데이터가 붙은 파일은 EOCD가 가리키는
//! 오프셋이 실제 위치와 어긋나므로 **델타 보정**한다(널리 쓰이는 관용 처리).

use super::{
    decode_name, dos_to_unix, filetime_to_unix, finish, normalize_path, read_exact_at, u16le,
    u32le, u64le, ArchiveEntry, ArchiveError, ArchiveFormat, ListOpts, Listing, ReadAt, MAX_CHUNK,
};

/// EOCD 최소 크기(주석 0) + 주석 최대(64KB).
const EOCD_MIN: usize = 22;
const EOCD_SEARCH: usize = EOCD_MIN + 0xFFFF;

const SIG_EOCD: u32 = 0x0605_4B50;
const SIG_EOCD64: u32 = 0x0606_4B50;
const SIG_LOC64: u32 = 0x0706_4B50;
const SIG_CD: u32 = 0x0201_4B50;

pub struct Zip;

/// 압축 방식 표시명(APPNOTE 4.4.5).
fn method_name(m: u16) -> &'static str {
    match m {
        0 => "Store",
        1 => "Shrink",
        2..=5 => "Reduce",
        6 => "Implode",
        8 => "Deflate",
        9 => "Deflate64",
        12 => "BZip2",
        14 => "LZMA",
        16 => "CMPSC",
        18 => "Terse",
        19 => "LZ77",
        20 | 93 => "Zstd",
        94 => "MP3",
        95 => "XZ",
        96 => "JPEG",
        97 => "WavPack",
        98 => "PPMd",
        99 => "AES",
        _ => "?",
    }
}

/// 확장 필드 해석 결과(중앙 디렉터리 레코드 1개분).
#[derive(Default)]
struct Extra {
    size: Option<u64>,
    packed: Option<u64>,
    /// AES(WinZip) 실제 압축 방식·강도.
    aes: Option<(u16, u8)>,
    /// NTFS·확장 타임스탬프에서 얻은 수정 시각(DOS 시각보다 정밀).
    mtime: Option<i64>,
}

/// 확장 필드 순회 — `0x0001` Zip64 · `0x9901` AES · `0x000A` NTFS · `0x5455` UT.
fn parse_extra(buf: &[u8], need_size: bool, need_packed: bool) -> Extra {
    let mut out = Extra::default();
    let mut p = 0usize;
    while p + 4 <= buf.len() {
        let (Some(id), Some(len)) = (u16le(buf, p), u16le(buf, p + 2)) else {
            break;
        };
        let body = match buf.get(p + 4..p + 4 + len as usize) {
            Some(b) => b,
            None => break,
        };
        match id {
            0x0001 => {
                // Zip64: 0xFFFFFFFF였던 필드만 순서대로 u64로 이어진다
                let mut q = 0usize;
                if need_size {
                    out.size = u64le(body, q);
                    q += 8;
                }
                if need_packed {
                    out.packed = u64le(body, q);
                }
            }
            0x9901 => {
                // AES: ver u16 · "AE" u16 · strength u8 · 실제 method u16
                if let (Some(strength), Some(m)) = (body.get(4).copied(), u16le(body, 5)) {
                    out.aes = Some((m, strength));
                }
            }
            0x000A => {
                // NTFS: reserved u32 · tag u16(1) · size u16(24) · mtime FILETIME
                if u16le(body, 4) == Some(1) {
                    out.mtime = u64le(body, 8).and_then(filetime_to_unix);
                }
            }
            // 확장 타임스탬프: flags u8(비트 0 = mtime 있음) · mtime i32
            0x5455 if body.first().is_some_and(|f| f & 1 != 0) => {
                out.mtime = u32le(body, 1).map(|t| t as i32 as i64);
            }
            _ => {}
        }
        p += 4 + len as usize;
    }
    out
}

/// 꼬리에서 EOCD를 역탐색 — (버퍼, 버퍼 내 위치, 파일 절대 위치).
fn find_eocd(src: &dyn ReadAt) -> Result<(Vec<u8>, usize, u64), ArchiveError> {
    let size = src.size();
    let want = EOCD_SEARCH.min(size as usize);
    let start = size - want as u64;
    let mut buf = vec![0u8; want];
    let n = src.read_at(start, &mut buf)?;
    buf.truncate(n);
    if buf.len() < EOCD_MIN {
        return Err(ArchiveError::Corrupt("ZIP 꼬리 부족".into()));
    }
    for i in (0..=buf.len() - EOCD_MIN).rev() {
        if u32le(&buf, i) == Some(SIG_EOCD) {
            // 주석 길이 정합 확인(우연한 시그니처 배제)
            let clen = u16le(&buf, i + 20).unwrap_or(0) as usize;
            if i + EOCD_MIN + clen <= buf.len() {
                return Ok((buf, i, start + i as u64));
            }
        }
    }
    Err(ArchiveError::Corrupt("EOCD 없음".into()))
}

impl ArchiveFormat for Zip {
    fn id(&self) -> &'static str {
        "zip"
    }
    fn label(&self) -> &'static str {
        "ZIP"
    }
    fn exts(&self) -> &'static [&'static str] {
        // ZIP 컨테이너를 쓰는 실사용 확장자 — 라우팅은 여기서 파생된다
        &[
            "zip", "zipx", "jar", "war", "ear", "apk", "aar", "docx", "xlsx", "pptx", "odt", "ods",
            "odp", "epub", "whl", "crx", "xpi", "nupkg", "appx", "msix", "vsix", "ipa", "kmz",
            "cbz", "sar", "pk3",
        ]
    }
    fn sniff(&self, head: &[u8], _src: &dyn ReadAt) -> bool {
        // 로컬 헤더 · 빈 아카이브(EOCD 선두) · 분할 표식
        matches!(head.get(..4), Some(b"PK\x03\x04") | Some(b"PK\x05\x06") | Some(b"PK\x07\x08"))
    }

    fn list(&self, src: &dyn ReadAt, opts: &ListOpts) -> Result<Listing, ArchiveError> {
        let (tail, pos, eocd_abs) = find_eocd(src)?;
        let e = &tail[pos..];
        let mut total = u16le(e, 10).unwrap_or(0) as u64;
        let mut cd_size = u32le(e, 12).unwrap_or(0) as u64;
        let mut cd_off = u32le(e, 16).unwrap_or(0) as u64;
        let clen = u16le(e, 20).unwrap_or(0) as usize;
        let comment = (clen > 0)
            .then(|| decode_name(e.get(EOCD_MIN..EOCD_MIN + clen).unwrap_or(&[]), false))
            .filter(|c| !c.trim().is_empty());
        let multivolume = u16le(e, 4).unwrap_or(0) != 0;

        // Zip64 승격 — 32비트 필드가 포화(0xFFFF/0xFFFFFFFF)면 로케이터를 따라간다
        if total == 0xFFFF || cd_size == 0xFFFF_FFFF || cd_off == 0xFFFF_FFFF {
            if let Some(loc_at) = pos.checked_sub(20) {
                if u32le(&tail, loc_at) == Some(SIG_LOC64) {
                    let z64 = u64le(&tail, loc_at + 8).unwrap_or(0);
                    let hdr = read_exact_at(src, z64, 56)?;
                    if u32le(&hdr, 0) == Some(SIG_EOCD64) {
                        total = u64le(&hdr, 32).unwrap_or(total);
                        cd_size = u64le(&hdr, 40).unwrap_or(cd_size);
                        cd_off = u64le(&hdr, 48).unwrap_or(cd_off);
                    }
                }
            }
        }
        if cd_size as usize > MAX_CHUNK {
            return Err(ArchiveError::Corrupt("중앙 디렉터리 과대".into()));
        }
        // SFX 등 앞에 데이터가 붙은 파일 보정 — EOCD 직전이 CD 끝이어야 한다
        if cd_off + cd_size != eocd_abs {
            if let Some(delta) = eocd_abs.checked_sub(cd_size) {
                cd_off = delta;
            }
        }
        let cd = read_exact_at(src, cd_off, cd_size as usize)?;

        let mut out = Listing {
            format: self.id().into(),
            label: self.label().into(),
            comment,
            multivolume,
            ..Default::default()
        };
        let limit = opts.limit();
        let mut p = 0usize;
        let mut seen = 0u64;
        while p + 46 <= cd.len() && out.entries.len() <= limit {
            if u32le(&cd, p) != Some(SIG_CD) {
                break;
            }
            let flags = u16le(&cd, p + 8).unwrap_or(0);
            let method = u16le(&cd, p + 10).unwrap_or(0);
            let dos_time = u16le(&cd, p + 12).unwrap_or(0);
            let dos_date = u16le(&cd, p + 14).unwrap_or(0);
            let crc = u32le(&cd, p + 16).unwrap_or(0);
            let packed = u32le(&cd, p + 20).unwrap_or(0) as u64;
            let size = u32le(&cd, p + 24).unwrap_or(0) as u64;
            let nlen = u16le(&cd, p + 28).unwrap_or(0) as usize;
            let xlen = u16le(&cd, p + 30).unwrap_or(0) as usize;
            let clen = u16le(&cd, p + 32).unwrap_or(0) as usize;
            let attrs = u32le(&cd, p + 38).unwrap_or(0);
            let name_b = cd.get(p + 46..p + 46 + nlen).unwrap_or(&[]);
            let extra_b = cd.get(p + 46 + nlen..p + 46 + nlen + xlen).unwrap_or(&[]);

            // 비트 13 = 중앙 디렉터리 암호화(이름조차 암호문) → 목록 불가
            if flags & 0x2000 != 0 {
                return Err(ArchiveError::PasswordRequired);
            }
            let ex = parse_extra(extra_b, size == 0xFFFF_FFFF, packed == 0xFFFF_FFFF);
            let raw = decode_name(name_b, flags & 0x800 != 0);
            let (path, suspicious) = normalize_path(&raw);
            let is_dir = raw.ends_with('/') || raw.ends_with('\\') || (attrs & 0x10 != 0 && size == 0);
            let method_label = match ex.aes {
                Some((inner, strength)) => format!(
                    "AES-{} + {}",
                    match strength {
                        1 => "128",
                        2 => "192",
                        _ => "256",
                    },
                    method_name(inner)
                ),
                None => method_name(method).to_string(),
            };
            if !path.is_empty() {
                out.entries.push(ArchiveEntry {
                    path,
                    is_dir,
                    size: Some(ex.size.unwrap_or(size)),
                    packed: Some(ex.packed.unwrap_or(packed)),
                    modified: ex.mtime.or_else(|| dos_to_unix(dos_date, dos_time)),
                    encrypted: flags & 1 != 0,
                    method: method_label,
                    crc32: (crc != 0).then_some(crc),
                    suspicious,
                });
            }
            p += 46 + nlen + xlen + clen;
            seen += 1;
        }
        if out.entries.is_empty() && total > 0 && seen == 0 {
            return Err(ArchiveError::Corrupt("중앙 디렉터리 레코드 없음".into()));
        }
        Ok(finish(out, limit))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::SliceSource;

    /// 최소 ZIP 조립기(테스트 픽스처 — 실제 도구 없이 바이트 규약만으로 구성).
    /// 항목 = (이름, 내용, 암호화 플래그, 방식).
    fn build_zip(items: &[(&str, &[u8], bool, u16)], comment: &[u8]) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        let mut cd: Vec<u8> = Vec::new();
        let (date, time) = ((46u16 << 9) | (8 << 5) | 24, (13u16 << 11) | (45 << 5) | 15);
        for (name, data, enc, method) in items {
            let off = out.len() as u32;
            let flags: u16 = if *enc { 1 } else { 0 } | 0x800; // UTF-8 이름
            out.extend_from_slice(b"PK\x03\x04");
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&flags.to_le_bytes());
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&time.to_le_bytes());
            out.extend_from_slice(&date.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes()); // crc
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(data);

            cd.extend_from_slice(b"PK\x01\x02");
            cd.extend_from_slice(&20u16.to_le_bytes());
            cd.extend_from_slice(&20u16.to_le_bytes());
            cd.extend_from_slice(&flags.to_le_bytes());
            cd.extend_from_slice(&method.to_le_bytes());
            cd.extend_from_slice(&time.to_le_bytes());
            cd.extend_from_slice(&date.to_le_bytes());
            cd.extend_from_slice(&0x1234_5678u32.to_le_bytes()); // crc
            cd.extend_from_slice(&(data.len() as u32).to_le_bytes());
            cd.extend_from_slice(&(data.len() as u32).to_le_bytes());
            cd.extend_from_slice(&(name.len() as u16).to_le_bytes());
            cd.extend_from_slice(&0u16.to_le_bytes()); // extra
            cd.extend_from_slice(&0u16.to_le_bytes()); // comment
            cd.extend_from_slice(&0u16.to_le_bytes()); // disk
            cd.extend_from_slice(&0u16.to_le_bytes()); // internal attr
            cd.extend_from_slice(&(if name.ends_with('/') { 0x10u32 } else { 0 }).to_le_bytes());
            cd.extend_from_slice(&off.to_le_bytes());
            cd.extend_from_slice(name.as_bytes());
        }
        let cd_off = out.len() as u32;
        let cd_size = cd.len() as u32;
        out.extend_from_slice(&cd);
        out.extend_from_slice(b"PK\x05\x06");
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(items.len() as u16).to_le_bytes());
        out.extend_from_slice(&(items.len() as u16).to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_off.to_le_bytes());
        out.extend_from_slice(&(comment.len() as u16).to_le_bytes());
        out.extend_from_slice(comment);
        out
    }

    #[test]
    fn lists_entries_with_sizes_time_and_encryption() {
        let z = build_zip(
            &[
                ("docs/", b"", false, 0),
                ("docs/a.txt", b"hello world", false, 8),
                ("secret.bin", b"xxxx", true, 99),
            ],
            "메모".as_bytes(),
        );
        let l = Zip.list(&SliceSource(&z), &ListOpts::default()).unwrap();
        assert_eq!(l.format, "zip");
        assert_eq!(l.entries.len(), 3);
        assert_eq!(l.comment.as_deref(), Some("메모"));
        let a = l.entries.iter().find(|e| e.path == "docs/a.txt").unwrap();
        assert_eq!((a.size, a.packed, a.method.as_str()), (Some(11), Some(11), "Deflate"));
        assert_eq!(
            a.modified,
            Some(crate::archive::ymd_hms_to_unix(2026, 8, 24, 13, 45, 30))
        );
        assert!(l.entries.iter().find(|e| e.path == "docs").unwrap().is_dir);
        let s = l.entries.iter().find(|e| e.path == "secret.bin").unwrap();
        assert!(s.encrypted && l.has_encrypted, "암호화 항목 집계");
        assert_eq!(s.method, "AES"); // 확장 필드 없는 method 99 = AES 표기
    }

    #[test]
    fn sniff_matches_signature_and_detect_routes_by_ext() {
        let z = build_zip(&[("a", b"1", false, 0)], b"");
        assert!(Zip.sniff(&z[..4], &SliceSource(&z)));
        assert!(!Zip.sniff(b"not a zip", &SliceSource(&z)));
        let f = crate::archive::detect(&SliceSource(&z), "").unwrap();
        assert_eq!(f.id(), "zip");
    }

    #[test]
    fn sfx_prefix_is_corrected_by_delta() {
        let mut z = vec![0xCCu8; 4096]; // 앞에 붙은 실행 코드 흉내
        z.extend_from_slice(&build_zip(&[("a.txt", b"1", false, 0)], b""));
        let l = Zip.list(&SliceSource(&z), &ListOpts::default()).unwrap();
        assert_eq!(l.entries.len(), 1, "앞 데이터가 붙어도 목록 복구");
    }

    #[test]
    fn central_directory_encryption_asks_for_password() {
        let mut z = build_zip(&[("a.txt", b"1", false, 0)], b"");
        // 중앙 디렉터리 레코드의 플래그(오프셋 +8)에 비트 13을 세운다
        let pos = z.windows(4).position(|w| w == b"PK\x01\x02").unwrap();
        let f = u16le(&z, pos + 8).unwrap() | 0x2000;
        z[pos + 8..pos + 10].copy_from_slice(&f.to_le_bytes());
        assert_eq!(
            Zip.list(&SliceSource(&z), &ListOpts::default()),
            Err(ArchiveError::PasswordRequired)
        );
    }

    #[test]
    fn zip_slip_paths_are_contained_and_flagged() {
        let z = build_zip(&[("../../evil.sh", b"1", false, 0)], b"");
        let l = Zip.list(&SliceSource(&z), &ListOpts::default()).unwrap();
        assert_eq!(l.entries[0].path, "evil.sh");
        assert!(l.entries[0].suspicious);
    }

    #[test]
    fn truncated_tail_is_reported_not_panicking() {
        let z = build_zip(&[("a.txt", b"1", false, 0)], b"");
        let cut = &z[..z.len() - 10];
        assert!(matches!(
            Zip.list(&SliceSource(cut), &ListOpts::default()),
            Err(ArchiveError::Corrupt(_))
        ));
    }
}
