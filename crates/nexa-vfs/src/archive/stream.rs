//! 단일 스트림 압축(gzip·bzip2·xz·zstd·lz4·lzip) — **항목이 1개**인 포맷.
//!
//! 컨테이너가 아니므로 "목록"은 원 파일 1건이다. 이름은 헤더가 보관하면 그것을
//! (gzip `FNAME`), 아니면 아카이브 파일명에서 확장자를 벗겨 만든다
//! ([`ListOpts::name_hint`]). gzip은 꼬리 4바이트(`ISIZE`)로 원본 크기까지 알 수
//! 있다(4GB 모듈러 — 그 이상은 근사).
//!
//! `.tar.gz`/`.tgz`처럼 **tar을 감싼 경우**는 안쪽 tar 목록까지 보려면 압축을 풀어야
//! 하므로(코덱 필요) 여기서는 tar 1건으로 보이고, 상세 목록은 플러그인 담당이다.

use super::{
    decode_name, normalize_path, read_exact_at, u32le, ArchiveEntry, ArchiveError, ArchiveFormat,
    ListOpts, Listing, ReadAt,
};

pub struct Gzip;
pub struct SingleStream;

/// 아카이브 파일명에서 안쪽 이름 추정 — `.tgz` → `.tar`, 그 외는 확장자 제거.
fn inner_name(hint: &str, ext: &str) -> String {
    let base = hint.trim();
    if base.is_empty() {
        return format!("(안쪽 내용 — {ext})");
    }
    let lower = base.to_ascii_lowercase();
    for (suffix, rep) in [
        (".tgz", ".tar"),
        (".tbz2", ".tar"),
        (".tbz", ".tar"),
        (".txz", ".tar"),
        (".tzst", ".tar"),
        (".tlz", ".tar"),
    ] {
        if lower.ends_with(suffix) {
            return format!("{}{}", &base[..base.len() - suffix.len()], rep);
        }
    }
    match base.rfind('.') {
        Some(i) if i > 0 => base[..i].to_string(),
        _ => base.to_string(),
    }
}

/// 단일 항목 목록 생성 공통부 — 항목은 호출부가 만들고, 포장만 여기서.
fn single(format: &str, label: &str, name: String, mut entry: ArchiveEntry) -> Listing {
    let (path, suspicious) = normalize_path(&name);
    entry.path = path;
    entry.suspicious = suspicious;
    Listing {
        format: format.into(),
        label: label.into(),
        entries: vec![entry],
        ..Default::default()
    }
}

impl ArchiveFormat for Gzip {
    fn id(&self) -> &'static str {
        "gzip"
    }
    fn label(&self) -> &'static str {
        "GZIP"
    }
    fn exts(&self) -> &'static [&'static str] {
        &["gz", "gzip", "tgz"]
    }
    fn sniff(&self, head: &[u8], _src: &dyn ReadAt) -> bool {
        head.starts_with(&[0x1F, 0x8B, 0x08])
    }

    fn list(&self, src: &dyn ReadAt, opts: &ListOpts) -> Result<Listing, ArchiveError> {
        let size = src.size();
        let head = read_exact_at(src, 0, 512.min(size as usize))?;
        if !head.starts_with(&[0x1F, 0x8B]) {
            return Err(ArchiveError::NotArchive);
        }
        let flg = head.get(3).copied().unwrap_or(0);
        let mtime = u32le(&head, 4).filter(|&t| t != 0).map(|t| t as i64);
        let mut p = 10usize;
        if flg & 0x04 != 0 {
            // FEXTRA
            let xlen = super::u16le(&head, p).unwrap_or(0) as usize;
            p += 2 + xlen;
        }
        // FNAME(원 파일 이름 — NUL 종료 · 대개 로컬 코드페이지)
        let name = if flg & 0x08 != 0 {
            let end = head[p.min(head.len())..]
                .iter()
                .position(|&b| b == 0)
                .map(|i| p + i)
                .unwrap_or(head.len());
            let s = decode_name(head.get(p..end).unwrap_or(&[]), false);
            if s.is_empty() {
                inner_name(opts.name_hint, "gz")
            } else {
                s
            }
        } else {
            inner_name(opts.name_hint, "gz")
        };
        // 꼬리 ISIZE = 원본 크기(2^32 모듈러)
        let isize_ = if size >= 8 {
            let tail = read_exact_at(src, size - 4, 4)?;
            u32le(&tail, 0).map(|v| v as u64)
        } else {
            None
        };
        Ok(single(
            self.id(),
            self.label(),
            name,
            ArchiveEntry {
                size: isize_,
                packed: Some(size),
                modified: mtime,
                method: "Deflate".into(),
                ..Default::default()
            },
        ))
    }
}

/// 시그니처가 있는 나머지 단일 스트림 포맷 표(확장 지점 — 한 줄 추가).
const STREAMS: &[(&str, &[u8], &str, &[&str])] = &[
    ("BZIP2", b"BZh", "BZip2", &["bz2", "tbz2", "tbz", "bz"]),
    (
        "XZ",
        &[0xFD, b'7', b'z', b'X', b'Z', 0x00],
        "LZMA2",
        &["xz", "txz"],
    ),
    (
        "Zstandard",
        &[0x28, 0xB5, 0x2F, 0xFD],
        "Zstd",
        &["zst", "zstd", "tzst"],
    ),
    ("LZ4", &[0x04, 0x22, 0x4D, 0x18], "LZ4", &["lz4"]),
    ("lzip", b"LZIP", "LZMA", &["lz", "tlz"]),
    ("LZMA", &[0x5D, 0x00, 0x00], "LZMA", &["lzma"]),
    ("compress", &[0x1F, 0x9D], "LZW", &["z", "taz"]),
];

impl ArchiveFormat for SingleStream {
    fn id(&self) -> &'static str {
        "stream"
    }
    fn label(&self) -> &'static str {
        "단일 스트림"
    }
    fn exts(&self) -> &'static [&'static str] {
        &[
            "bz2", "tbz2", "tbz", "bz", "xz", "txz", "zst", "zstd", "tzst", "lz4", "lz", "tlz",
            "lzma", "z", "taz",
        ]
    }
    fn sniff(&self, head: &[u8], _src: &dyn ReadAt) -> bool {
        STREAMS.iter().any(|(_, sig, _, _)| head.starts_with(sig))
    }

    fn list(&self, src: &dyn ReadAt, opts: &ListOpts) -> Result<Listing, ArchiveError> {
        let head = super::read_head(src);
        let ext = opts
            .name_hint
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        // 시그니처 우선, 없으면 확장자로 결정(잘린 파일 대비)
        let hit = STREAMS
            .iter()
            .find(|(_, sig, _, _)| head.starts_with(sig))
            .or_else(|| {
                STREAMS
                    .iter()
                    .find(|(_, _, _, exts)| exts.contains(&ext.as_str()))
            })
            .ok_or(ArchiveError::NotArchive)?;
        Ok(single(
            self.id(),
            hit.0,
            inner_name(opts.name_hint, hit.0),
            ArchiveEntry {
                // 원본 크기는 헤더에 없다(포맷별 꼬리 해석은 코덱 영역)
                size: None,
                packed: Some(src.size()),
                method: hit.2.into(),
                ..Default::default()
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::SliceSource;

    #[test]
    fn gzip_reads_stored_name_time_and_isize() {
        let mut v = vec![0x1F, 0x8B, 0x08, 0x08];
        v.extend_from_slice(&1_700_000_000u32.to_le_bytes());
        v.extend_from_slice(&[0x00, 0x03]);
        v.extend_from_slice("보고서.txt".as_bytes());
        v.push(0);
        v.extend_from_slice(&[0xAA; 32]); // 압축 데이터 자리
        v.extend_from_slice(&0u32.to_le_bytes()); // CRC32
        v.extend_from_slice(&4096u32.to_le_bytes()); // ISIZE
        let opts = ListOpts {
            name_hint: "보고서.txt.gz",
            ..Default::default()
        };
        assert!(Gzip.sniff(&v, &SliceSource(&v)));
        let l = Gzip.list(&SliceSource(&v), &opts).unwrap();
        assert_eq!(l.entries.len(), 1);
        let e = &l.entries[0];
        assert_eq!(e.path, "보고서.txt");
        assert_eq!((e.size, e.method.as_str()), (Some(4096), "Deflate"));
        assert_eq!(e.modified, Some(1_700_000_000));
    }

    #[test]
    fn tgz_without_name_derives_inner_tar() {
        let mut v = vec![0x1F, 0x8B, 0x08, 0x00];
        v.extend_from_slice(&[0u8; 6]);
        v.extend_from_slice(&[0xAA; 16]);
        v.extend_from_slice(&[0u8; 8]);
        let opts = ListOpts {
            name_hint: "backup.tgz",
            ..Default::default()
        };
        let l = Gzip.list(&SliceSource(&v), &opts).unwrap();
        assert_eq!(l.entries[0].path, "backup.tar");
    }

    #[test]
    fn single_stream_formats_are_identified() {
        for (label, sig, method, _) in STREAMS {
            let mut v = sig.to_vec();
            v.extend_from_slice(&[0u8; 64]);
            let opts = ListOpts {
                name_hint: "data.bin.xx",
                ..Default::default()
            };
            assert!(SingleStream.sniff(&v, &SliceSource(&v)), "{label}");
            let l = SingleStream.list(&SliceSource(&v), &opts).unwrap();
            assert_eq!(l.label, *label);
            assert_eq!(l.entries[0].method, *method);
            assert_eq!(l.entries[0].path, "data.bin");
        }
    }

    #[test]
    fn unknown_stream_is_rejected() {
        let v = vec![0x11u8; 32];
        assert!(!SingleStream.sniff(&v, &SliceSource(&v)));
        assert_eq!(
            SingleStream.list(&SliceSource(&v), &ListOpts::default()),
            Err(ArchiveError::NotArchive)
        );
    }
}
