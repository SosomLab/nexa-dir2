//! TAR 목록 — 512바이트 헤더 블록을 따라가며 읽는다(비압축 컨테이너라 코덱 불요).
//!
//! 지원: ustar/POSIX · GNU 확장(`L` 긴 이름·base-256 큰 수) · PAX(`x`/`g` —
//! `path`·`size`·`mtime` 키). `.tar.gz`처럼 **압축된 tar**은 이 리더가 아니라
//! [`super::stream`]이 받는다(내부 tar 목록은 코덱 필요 = 플러그인 담당).

use super::{
    decode_name, finish, normalize_path, read_exact_at, ArchiveEntry, ArchiveError, ArchiveFormat,
    ListOpts, Listing, ReadAt,
};

pub struct Tar;

const BLOCK: u64 = 512;

/// 8진수 필드(공백·NUL 종료) 또는 GNU base-256(최상위 비트) 해석.
fn numeric(field: &[u8]) -> Option<u64> {
    if field.first().is_some_and(|b| b & 0x80 != 0) {
        // base-256: 첫 바이트의 부호 비트를 제외한 빅엔디언
        let mut v: u64 = (field[0] & 0x7F) as u64;
        for b in &field[1..] {
            v = v.checked_mul(256)?.checked_add(*b as u64)?;
        }
        return Some(v);
    }
    let s: String = field
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| b as char)
        .collect();
    let s = s.trim();
    if s.is_empty() {
        return Some(0);
    }
    u64::from_str_radix(s, 8).ok()
}

/// NUL 종료 문자열 필드.
fn cstr(field: &[u8]) -> &[u8] {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    &field[..end]
}

/// 헤더 체크섬 검증(시그니처가 없는 구형 tar 판정용).
fn checksum_ok(h: &[u8]) -> bool {
    let Some(want) = numeric(&h[148..156]) else {
        return false;
    };
    let sum: u64 = h
        .iter()
        .enumerate()
        .map(|(i, &b)| {
            if (148..156).contains(&i) {
                32
            } else {
                b as u64
            }
        })
        .sum();
    let signed: i64 = h
        .iter()
        .enumerate()
        .map(|(i, &b)| {
            if (148..156).contains(&i) {
                32
            } else {
                b as i8 as i64
            }
        })
        .sum();
    sum == want || signed == want as i64
}

/// PAX 확장 레코드(`길이 키=값\n` 반복) 파싱.
///
/// 길이 필드는 아카이브가 준 **바이트 수**이므로 `&[u8]`로 자른다 — `&str`로 자르면
/// 손상 레코드의 길이가 멀티바이트 글자 중간에 떨어질 때 경계 panic이 난다(G7-02).
/// `len <= sp + 1`(빈 레코드·자기 길이보다 짧은 길이)이면 손상으로 보고 중단한다.
fn parse_pax(body: &[u8]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(sp) = rest.iter().position(|&b| b == b' ') {
        let Some(len) = std::str::from_utf8(&rest[..sp])
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
        else {
            break;
        };
        if len <= sp + 1 || len > rest.len() {
            break;
        }
        let rec = String::from_utf8_lossy(&rest[sp + 1..len]);
        if let Some((k, v)) = rec.split_once('=') {
            out.push((k.to_string(), v.trim_end_matches('\n').to_string()));
        }
        rest = &rest[len..];
    }
    out
}

impl ArchiveFormat for Tar {
    fn id(&self) -> &'static str {
        "tar"
    }
    fn label(&self) -> &'static str {
        "TAR"
    }
    fn exts(&self) -> &'static [&'static str] {
        &["tar"]
    }
    fn sniff(&self, head: &[u8], _src: &dyn ReadAt) -> bool {
        head.len() >= 512
            && (matches!(head.get(257..262), Some(b"ustar")) || checksum_ok(&head[..512]))
    }

    fn list(&self, src: &dyn ReadAt, opts: &ListOpts) -> Result<Listing, ArchiveError> {
        let mut out = Listing {
            format: self.id().into(),
            label: self.label().into(),
            ..Default::default()
        };
        let limit = opts.limit();
        let size = src.size();
        let mut off = 0u64;
        // 다음 항목에만 적용되는 확장 헤더 값(GNU L·PAX x)
        let mut pending_name: Option<String> = None;
        let mut pending_size: Option<u64> = None;
        let mut pending_mtime: Option<i64> = None;
        let mut zeros = 0;

        while off.checked_add(BLOCK).is_some_and(|end| end <= size) && out.entries.len() <= limit {
            let h = read_exact_at(src, off, BLOCK as usize)?;
            off += BLOCK;
            if h.iter().all(|&b| b == 0) {
                zeros += 1;
                if zeros >= 2 {
                    break; // 종료 표식(0 블록 2개)
                }
                continue;
            }
            zeros = 0;
            if !checksum_ok(&h) {
                if out.entries.is_empty() {
                    return Err(ArchiveError::Corrupt("TAR 헤더 체크섬 불일치".into()));
                }
                break; // 뒤쪽 잡음은 조용히 종료(패딩 관용)
            }
            let esize = numeric(&h[124..136]).unwrap_or(0);
            let mtime = numeric(&h[136..148]).map(|t| t as i64);
            let typeflag = h[156];
            // base-256 크기는 u64 전 범위라 `div_ceil * BLOCK`·`off +=`가 wrap할 수 있다 —
            // wrap하면 off가 같은 헤더로 되돌아가 UI 스레드가 무한 루프에 빠진다(G7-11).
            // checked 연산 실패 = 조작/손상 → 여기서 목록을 끝낸다.
            let Some(data_blocks) = esize.checked_next_multiple_of(BLOCK) else {
                break;
            };
            let Some(after_data) = off.checked_add(data_blocks) else {
                break;
            };

            match typeflag {
                b'L' | b'K' => {
                    // GNU 긴 이름/링크 — 본문이 다음 항목의 이름
                    let body = read_exact_at(src, off, esize.min(super::MAX_NAME as u64) as usize)?;
                    if typeflag == b'L' {
                        pending_name = Some(decode_name(cstr(&body), false));
                    }
                    off = after_data;
                    continue;
                }
                b'x' | b'g' => {
                    let body = read_exact_at(src, off, esize.min(64 * 1024) as usize)?;
                    for (k, v) in parse_pax(&body) {
                        match k.as_str() {
                            "path" => pending_name = Some(v),
                            "size" => pending_size = v.parse().ok(),
                            "mtime" => {
                                pending_mtime = v.split('.').next().and_then(|s| s.parse().ok())
                            }
                            _ => {}
                        }
                    }
                    off = after_data;
                    continue;
                }
                _ => {}
            }

            let raw = pending_name.take().unwrap_or_else(|| {
                let name = decode_name(cstr(&h[0..100]), false);
                let prefix = decode_name(cstr(&h[345..500]), false);
                if prefix.is_empty() {
                    name
                } else {
                    format!("{prefix}/{name}")
                }
            });
            let (path, suspicious) = normalize_path(&raw);
            let esize = pending_size.take().unwrap_or(esize);
            let is_dir = typeflag == b'5' || raw.ends_with('/');
            if !path.is_empty() {
                out.entries.push(ArchiveEntry {
                    path,
                    is_dir,
                    size: (!is_dir).then_some(esize),
                    packed: (!is_dir).then_some(esize), // 비압축 컨테이너
                    modified: pending_mtime.take().or(mtime).filter(|&t| t > 0),
                    time_is_local: false, // tar = Unix epoch(UTC)
                    encrypted: false,
                    method: if is_dir {
                        String::new()
                    } else {
                        "Store".into()
                    },
                    crc32: None,
                    suspicious,
                });
            }
            off = after_data;
        }
        if out.entries.is_empty() {
            return Err(ArchiveError::Corrupt("TAR 항목 없음".into()));
        }
        Ok(finish(out, limit))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::SliceSource;

    /// 512B 헤더 조립(체크섬 계산 포함).
    fn header(name: &str, size: u64, typeflag: u8, mtime: u64, prefix: &str) -> Vec<u8> {
        let mut h = vec![0u8; 512];
        h[..name.len()].copy_from_slice(name.as_bytes());
        h[100..107].copy_from_slice(b"0000644");
        h[124..135].copy_from_slice(format!("{size:011o}").as_bytes());
        h[136..147].copy_from_slice(format!("{mtime:011o}").as_bytes());
        h[156] = typeflag;
        h[257..263].copy_from_slice(b"ustar\0");
        h[263..265].copy_from_slice(b"00");
        h[345..345 + prefix.len()].copy_from_slice(prefix.as_bytes());
        let sum: u64 = h
            .iter()
            .enumerate()
            .map(|(i, &b)| {
                if (148..156).contains(&i) {
                    32
                } else {
                    b as u64
                }
            })
            .sum();
        h[148..154].copy_from_slice(format!("{sum:06o}").as_bytes());
        h[155] = b' ';
        h
    }

    /// PAX 레코드 조립 — 길이 필드가 자기 자신을 포함하므로 수렴시켜 계산한다.
    fn pax_record(kv: &str) -> String {
        let mut n = kv.len() + 3;
        loop {
            let s = format!(
                "{n} {kv}
"
            );
            if s.len() == n {
                return s;
            }
            n += 1;
        }
    }

    /// 크기 필드(124..136)를 GNU base-256으로 쓰고 체크섬을 다시 계산한다.
    fn with_base256_size(mut h: Vec<u8>, size: u64) -> Vec<u8> {
        h[124] = 0x80; // base-256 표식
        h[125..128].copy_from_slice(&[0, 0, 0]);
        h[128..136].copy_from_slice(&size.to_be_bytes());
        let sum: u64 = h
            .iter()
            .enumerate()
            .map(|(i, &b)| {
                if (148..156).contains(&i) {
                    32
                } else {
                    b as u64
                }
            })
            .sum();
        h[148..154].copy_from_slice(format!("{sum:06o}").as_bytes());
        h[155] = b' ';
        h
    }

    fn body(data: &[u8]) -> Vec<u8> {
        let mut v = data.to_vec();
        v.resize(data.len().div_ceil(512) * 512, 0);
        v
    }

    fn build(parts: &[(String, Vec<u8>)]) -> Vec<u8> {
        let mut out = Vec::new();
        for (_, b) in parts {
            out.extend_from_slice(b);
        }
        out.extend_from_slice(&[0u8; 1024]); // 종료 블록 2개
        out
    }

    #[test]
    fn lists_files_dirs_and_prefix_paths() {
        let mut v = Vec::new();
        v.extend(header("dir/", 0, b'5', 1_700_000_000, ""));
        v.extend(header("dir/a.txt", 5, b'0', 1_700_000_100, ""));
        v.extend(body(b"hello"));
        v.extend(header("b.txt", 3, b'0', 0, "long/prefix"));
        v.extend(body(b"abc"));
        let t = build(&[(String::new(), v)]);
        let l = Tar.list(&SliceSource(&t), &ListOpts::default()).unwrap();
        assert_eq!(l.entries.len(), 3);
        assert!(l.entries.iter().any(|e| e.path == "dir" && e.is_dir));
        let a = l.entries.iter().find(|e| e.path == "dir/a.txt").unwrap();
        assert_eq!((a.size, a.modified), (Some(5), Some(1_700_000_100)));
        assert!(l.entries.iter().any(|e| e.path == "long/prefix/b.txt"));
        assert!(Tar.sniff(&t[..512], &SliceSource(&t)));
    }

    #[test]
    fn gnu_long_name_and_pax_path_override() {
        let long = "a/".repeat(80) + "deep.txt";
        let mut v = Vec::new();
        v.extend(header("././@LongLink", long.len() as u64, b'L', 0, ""));
        v.extend(body(long.as_bytes()));
        v.extend(header("short.txt", 0, b'0', 0, ""));
        // PAX로 경로 재정의(레코드 = "<자기 길이 포함 총 길이> key=value\n")
        let rec = pax_record("path=pax/name.txt");
        v.extend(header("PaxHeader", rec.len() as u64, b'x', 0, ""));
        v.extend(body(rec.as_bytes()));
        v.extend(header("ignored.txt", 0, b'0', 0, ""));
        let t = build(&[(String::new(), v)]);
        let l = Tar.list(&SliceSource(&t), &ListOpts::default()).unwrap();
        let paths: Vec<&str> = l.entries.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.contains(&long.trim_end_matches('/')), "{paths:?}");
        assert!(paths.contains(&"pax/name.txt"), "{paths:?}");
    }

    #[test]
    fn pax_short_length_field_does_not_panic() {
        // 길이 1 = 자기 길이보다 짧음(len <= sp+1) → 빈 결과, 패닉 없음
        assert!(parse_pax(b"1 x=y\n").is_empty());
        // 길이 0 → 중단
        assert!(parse_pax(b"0 x=y\n").is_empty());
        // 길이 필드가 숫자가 아님 → 중단
        assert!(parse_pax(b"ab x=y\n").is_empty());
        // 길이가 본문보다 큼 → 중단
        assert!(parse_pax(b"99 x=y\n").is_empty());
    }

    #[test]
    fn pax_length_inside_multibyte_char_does_not_panic() {
        // "6 p=한\n"은 8바이트인데 길이 6은 '한'(4..7) 중간 — &str 슬라이스였다면 경계 panic
        let body = "6 p=한\n".as_bytes();
        let got = parse_pax(body);
        // 레코드 하나를 손실 변환으로 읽고 다음 토큰('\xe2'로 시작)엔 공백이 없어 종료
        assert!(got.len() <= 1);
        if let Some((k, _)) = got.first() {
            assert_eq!(k, "p");
        }
    }

    #[test]
    fn pax_well_formed_records_roundtrip() {
        let rec = pax_record("path=가/나.txt")
            + &pax_record("size=12")
            + &pax_record("mtime=1700000000.5");
        let got = parse_pax(rec.as_bytes());
        assert_eq!(
            got,
            vec![
                ("path".to_string(), "가/나.txt".to_string()),
                ("size".to_string(), "12".to_string()),
                ("mtime".to_string(), "1700000000.5".to_string()),
            ]
        );
    }

    #[test]
    fn pax_header_with_corrupt_length_lists_without_panic() {
        // 길이 필드를 1 작게 만든 손상 PAX 레코드 — Ok 또는 Corrupt이되 패닉 없음
        let rec = pax_record("path=가/나.txt");
        let n: usize = rec.split(' ').next().unwrap().parse().unwrap();
        let bad = format!("{} {}", n - 1, &rec[rec.find(' ').unwrap() + 1..]);
        let mut v = Vec::new();
        v.extend(header("PaxHeader", bad.len() as u64, b'x', 0, ""));
        v.extend(body(bad.as_bytes()));
        v.extend(header("after.txt", 0, b'0', 0, ""));
        let t = build(&[(String::new(), v)]);
        let r = Tar.list(&SliceSource(&t), &ListOpts::default());
        assert!(matches!(r, Ok(_) | Err(ArchiveError::Corrupt(_))));
    }

    #[test]
    fn base256_size_field_is_read() {
        let mut v = with_base256_size(header("big.bin", 0, b'0', 0, ""), 0x1_0000); // 64KB
        v.extend(vec![0u8; 0x1_0000]);
        v.extend_from_slice(&[0u8; 1024]);
        let l = Tar.list(&SliceSource(&v), &ListOpts::default()).unwrap();
        assert_eq!(l.entries[0].size, Some(0x1_0000));
    }

    /// 조작된 base-256 크기 — `off += data_blocks`가 wrap해 같은 헤더로 되돌아가던 경우.
    /// 'L' 헤더는 항목을 push하지 않아 limit에도 걸리지 않고 영원히 돌았다(G7-11).
    #[test]
    fn wrapping_base256_size_on_long_name_header_terminates() {
        // esize ∈ (2^64-1024, 2^64-512] → data_blocks = 2^64-512, off(512)+data_blocks wrap → 512
        let bogus = u64::MAX - 600;
        let mut v = with_base256_size(header("././@LongLink", 0, b'L', 0, ""), bogus);
        v.extend(vec![0u8; 4096]); // 헤더 뒤 본문 몇 KB만 있어도 read_exact_at 통과
        v.extend_from_slice(&[0u8; 1024]);
        let r = Tar.list(&SliceSource(&v), &ListOpts::default());
        // 항목 0 → Corrupt. 핵심은 즉시 반환(무한 루프·panic 없음)
        assert!(matches!(r, Err(ArchiveError::Corrupt(_))), "{r:?}");
    }

    #[test]
    fn wrapping_base256_size_on_regular_header_stops_after_prior_entries() {
        // 정상 항목 하나 뒤에 크기가 u64::MAX인 일반 헤더(next_multiple_of 자체가 overflow)
        let mut v = Vec::new();
        v.extend(header("ok.txt", 3, b'0', 0, ""));
        v.extend(body(b"abc"));
        v.extend(with_base256_size(
            header("huge.bin", 0, b'0', 0, ""),
            u64::MAX,
        ));
        v.extend(vec![0u8; 4096]);
        v.extend_from_slice(&[0u8; 1024]);
        let l = Tar.list(&SliceSource(&v), &ListOpts::default()).unwrap();
        // 조작 헤더에서 목록을 끝내고(중복 5만 개가 아닌) 앞 항목만 남는다
        assert_eq!(l.entries.len(), 1);
        assert_eq!(l.entries[0].path, "ok.txt");
        assert!(!l.truncated);
    }

    #[test]
    fn pax_header_with_wrapping_size_terminates() {
        // 'x' 헤더(push 없음) + off+data_blocks만 wrap하는 값(next_multiple_of는 성공)
        let bogus = u64::MAX - 1000; // next multiple = 2^64-512 ≤ u64::MAX
        let mut v = with_base256_size(header("PaxHeader", 0, b'x', 0, ""), bogus);
        v.extend(vec![0u8; 4096]);
        v.extend_from_slice(&[0u8; 1024]);
        let r = Tar.list(&SliceSource(&v), &ListOpts::default());
        assert!(matches!(r, Err(ArchiveError::Corrupt(_))), "{r:?}");
    }

    #[test]
    fn garbage_is_rejected_without_panic() {
        let junk = vec![0x41u8; 4096];
        assert!(matches!(
            Tar.list(&SliceSource(&junk), &ListOpts::default()),
            Err(ArchiveError::Corrupt(_))
        ));
        assert!(!Tar.sniff(&junk[..512], &SliceSource(&junk)));
    }
}
