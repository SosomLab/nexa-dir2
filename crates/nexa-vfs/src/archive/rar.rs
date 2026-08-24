//! RAR 목록 — RAR 5(`Rar!\x1A\x07\x01\x00`)와 RAR 4(`Rar!\x1A\x07\x00`) 모두
//! **파일 헤더가 평문**이므로 코덱 없이 목록을 읽는다(내용만 압축·암호화).
//!
//! 헤더 암호화 아카이브(RAR5 = 암호화 헤더 블록 `type 4` 선두 · RAR4 = 메인
//! 헤더 플래그 `0x0080`)는 목록 자체가 암호문이라 [`ArchiveError::PasswordRequired`]
//! 를 올린다 — 호스트가 암호를 받아 재시도한다(복호 구현은 플러그인 담당).

use super::{
    decode_name, dos_to_unix, finish, normalize_path, read_exact_at, u16le, u32le, ArchiveEntry,
    ArchiveError, ArchiveFormat, ListOpts, Listing, ReadAt,
};

pub struct Rar;

const SIG5: &[u8] = b"Rar!\x1a\x07\x01\x00";
const SIG4: &[u8] = b"Rar!\x1a\x07\x00";

/// RAR5 가변 정수(7비트 리틀엔디언·최상위 비트 = 계속) — (값, 소비 바이트).
fn vint(b: &[u8], mut p: usize) -> Option<(u64, usize)> {
    let start = p;
    let mut v = 0u64;
    let mut shift = 0u32;
    loop {
        let byte = *b.get(p)?;
        p += 1;
        v |= ((byte & 0x7F) as u64) << shift;
        if byte & 0x80 == 0 {
            return Some((v, p - start));
        }
        shift += 7;
        if shift > 63 {
            return None;
        }
    }
}

/// RAR5 압축 방식 표시명(compression_info 비트 7-9).
fn rar5_method(info: u64) -> &'static str {
    match (info >> 7) & 0x7 {
        0 => "Store",
        1 => "Fastest",
        2 => "Fast",
        3 => "Normal",
        4 => "Good",
        _ => "Best",
    }
}

/// 확장 영역(extra area) 훑기 — 암호화 레코드(type 1) 유무만 본다.
fn rar5_extra_has_crypt(extra: &[u8]) -> bool {
    let mut p = 0usize;
    while p < extra.len() {
        let Some((size, n1)) = vint(extra, p) else {
            return false;
        };
        let Some((rtype, _)) = vint(extra, p + n1) else {
            return false;
        };
        if rtype == 1 {
            return true; // FHEXTRA_CRYPT
        }
        p += n1 + size as usize;
        if size == 0 {
            return false;
        }
    }
    false
}

/// RAR 5 목록.
fn list_v5(src: &dyn ReadAt, opts: &ListOpts, out: &mut Listing) -> Result<(), ArchiveError> {
    let limit = opts.limit();
    let size = src.size();
    let mut off = SIG5.len() as u64;
    out.label = "RAR 5".into();
    // 최소 블록 = CRC(4) + vint 헤더 크기(1) + 타입(1) + 플래그(1)
    while off + 7 <= size && out.entries.len() <= limit {
        // CRC(4) + vint 헤더 크기 → 헤더 본문
        let head = read_exact_at(src, off, 64.min((size - off) as usize))?;
        let Some((hsize, n)) = vint(&head, 4) else {
            break;
        };
        let body_at = off + 4 + n as u64;
        if hsize == 0 || hsize > super::MAX_CHUNK as u64 {
            break;
        }
        let body = read_exact_at(src, body_at, (hsize as usize).min((size - body_at) as usize))?;
        let mut p = 0usize;
        let Some((htype, n1)) = vint(&body, p) else {
            break;
        };
        p += n1;
        let Some((hflags, n2)) = vint(&body, p) else {
            break;
        };
        p += n2;
        let mut extra_size = 0u64;
        if hflags & 0x0001 != 0 {
            let Some((v, n)) = vint(&body, p) else { break };
            extra_size = v;
            p += n;
        }
        let mut data_size = 0u64;
        if hflags & 0x0002 != 0 {
            let Some((v, n)) = vint(&body, p) else { break };
            data_size = v;
            p += n;
        }
        match htype {
            4 => return Err(ArchiveError::PasswordRequired), // 암호화 헤더
            1 => {
                // 메인 헤더 — 볼륨 플래그
                if let Some((flags, _)) = vint(&body, p) {
                    out.multivolume = flags & 0x0001 != 0;
                }
            }
            2 | 3 => {
                let Some((fflags, n)) = vint(&body, p) else { break };
                p += n;
                let Some((usize_, n)) = vint(&body, p) else { break };
                p += n;
                let Some((_attrs, n)) = vint(&body, p) else { break };
                p += n;
                let mut mtime = None;
                if fflags & 0x0002 != 0 {
                    mtime = u32le(&body, p).map(|t| t as i64);
                    p += 4;
                }
                let mut crc = None;
                if fflags & 0x0004 != 0 {
                    crc = u32le(&body, p);
                    p += 4;
                }
                let Some((cinfo, n)) = vint(&body, p) else { break };
                p += n;
                let Some((_host, n)) = vint(&body, p) else { break };
                p += n;
                let Some((nlen, n)) = vint(&body, p) else { break };
                p += n;
                let name_b = body.get(p..p + nlen as usize).unwrap_or(&[]);
                p += nlen as usize;
                let encrypted = extra_size > 0
                    && rar5_extra_has_crypt(body.get(p..p + extra_size as usize).unwrap_or(&[]));
                let is_dir = fflags & 0x0001 != 0;
                let (path, suspicious) = normalize_path(&decode_name(name_b, true));
                if cinfo & 0x40 != 0 {
                    out.solid = true;
                }
                if htype == 2 && !path.is_empty() {
                    out.entries.push(ArchiveEntry {
                        path,
                        is_dir,
                        size: (!is_dir).then_some(usize_),
                        packed: (!is_dir).then_some(data_size),
                        modified: mtime.filter(|&t| t > 0),
                        time_is_local: false, // RAR5 = Unix epoch(UTC)
                        encrypted,
                        method: if is_dir {
                            String::new()
                        } else {
                            rar5_method(cinfo).into()
                        },
                        crc32: crc,
                        suspicious,
                    });
                }
            }
            5 => break, // 끝 표식
            _ => {}
        }
        let next = body_at + hsize + data_size;
        if next <= off {
            break; // 전진 없음 = 손상
        }
        off = next;
    }
    Ok(())
}

/// RAR 4 목록.
fn list_v4(src: &dyn ReadAt, opts: &ListOpts, out: &mut Listing) -> Result<(), ArchiveError> {
    let limit = opts.limit();
    let size = src.size();
    let mut off = SIG4.len() as u64;
    out.label = "RAR 4".into();
    while off + 7 <= size && out.entries.len() <= limit {
        let h = read_exact_at(src, off, 7)?;
        let htype = h[2];
        let hflags = u16le(&h, 3).unwrap_or(0);
        let hsize = u16le(&h, 5).unwrap_or(0) as u64;
        if hsize < 7 {
            break;
        }
        let full = read_exact_at(src, off, (hsize as usize).min((size - off) as usize))?;
        let add = if hflags & 0x8000 != 0 {
            u32le(&full, 7).unwrap_or(0) as u64
        } else {
            0
        };
        match htype {
            0x73 => {
                // 메인 헤더 — 0x0080 = 헤더 암호화, 0x0001 = 볼륨
                if hflags & 0x0080 != 0 {
                    return Err(ArchiveError::PasswordRequired);
                }
                out.multivolume = hflags & 0x0001 != 0;
                out.solid = hflags & 0x0008 != 0;
            }
            0x74 => {
                let packed = u32le(&full, 7).unwrap_or(0) as u64;
                let unpacked = u32le(&full, 11).unwrap_or(0) as u64;
                let crc = u32le(&full, 16);
                let ftime = u32le(&full, 20).unwrap_or(0);
                let method = full.get(25).copied().unwrap_or(0x30);
                let nlen = u16le(&full, 26).unwrap_or(0) as usize;
                let mut p = 32usize;
                let (mut packed, mut unpacked) = (packed, unpacked);
                if hflags & 0x0100 != 0 {
                    // 64비트 확장(HIGH_PACK_SIZE·HIGH_UNP_SIZE)
                    packed |= (u32le(&full, p).unwrap_or(0) as u64) << 32;
                    unpacked |= (u32le(&full, p + 4).unwrap_or(0) as u64) << 32;
                    p += 8;
                }
                let name_b = full.get(p..p + nlen).unwrap_or(&[]);
                // 유니코드 플래그(0x0200)면 `ascii\0<압축 유니코드>` — 앞부분만 취한다
                let name_b = match (hflags & 0x0200 != 0, name_b.iter().position(|&b| b == 0)) {
                    (true, Some(i)) => &name_b[..i],
                    _ => name_b,
                };
                let is_dir = hflags & 0x00E0 == 0x00E0;
                let (path, suspicious) = normalize_path(&decode_name(name_b, false));
                if !path.is_empty() {
                    out.entries.push(ArchiveEntry {
                        path,
                        is_dir,
                        size: (!is_dir).then_some(unpacked),
                        packed: (!is_dir).then_some(packed),
                        modified: dos_to_unix((ftime >> 16) as u16, ftime as u16),
                        time_is_local: true, // RAR4 = DOS 시각
                        encrypted: hflags & 0x0004 != 0,
                        method: match method {
                            0x30 => "Store",
                            0x31 => "Fastest",
                            0x32 => "Fast",
                            0x33 => "Normal",
                            0x34 => "Good",
                            _ => "Best",
                        }
                        .into(),
                        crc32: crc,
                        suspicious,
                    });
                }
            }
            0x7B => break, // 끝 표식
            _ => {}
        }
        let next = off + hsize + add;
        if next <= off {
            break;
        }
        off = next;
    }
    Ok(())
}

impl ArchiveFormat for Rar {
    fn id(&self) -> &'static str {
        "rar"
    }
    fn label(&self) -> &'static str {
        "RAR"
    }
    fn exts(&self) -> &'static [&'static str] {
        &["rar", "r00", "rev"]
    }
    fn sniff(&self, head: &[u8], _src: &dyn ReadAt) -> bool {
        head.starts_with(SIG5) || head.starts_with(SIG4)
    }

    fn list(&self, src: &dyn ReadAt, opts: &ListOpts) -> Result<Listing, ArchiveError> {
        let head = super::read_head(src);
        let mut out = Listing {
            format: self.id().into(),
            label: self.label().into(),
            ..Default::default()
        };
        if head.starts_with(SIG5) {
            list_v5(src, opts, &mut out)?;
        } else if head.starts_with(SIG4) {
            list_v4(src, opts, &mut out)?;
        } else {
            return Err(ArchiveError::NotArchive);
        }
        if out.entries.is_empty() {
            return Err(ArchiveError::Corrupt("RAR 항목 없음".into()));
        }
        Ok(finish(out, opts.limit()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::SliceSource;

    fn put_vint(out: &mut Vec<u8>, mut v: u64) {
        loop {
            let b = (v & 0x7F) as u8;
            v >>= 7;
            out.push(if v > 0 { b | 0x80 } else { b });
            if v == 0 {
                break;
            }
        }
    }

    /// RAR5 블록 조립 — (타입, 본문, 데이터부).
    fn block(htype: u64, body_tail: &[u8], data: &[u8], hflags: u64, extra: &[u8]) -> Vec<u8> {
        let mut body = Vec::new();
        put_vint(&mut body, htype);
        put_vint(&mut body, hflags);
        if hflags & 1 != 0 {
            put_vint(&mut body, extra.len() as u64);
        }
        if hflags & 2 != 0 {
            put_vint(&mut body, data.len() as u64);
        }
        body.extend_from_slice(body_tail);
        body.extend_from_slice(extra);
        let mut out = Vec::new();
        out.extend_from_slice(&0u32.to_le_bytes()); // CRC(검사 안 함)
        put_vint(&mut out, body.len() as u64);
        out.extend_from_slice(&body);
        out.extend_from_slice(data);
        out
    }

    fn file5(name: &str, usize_: u64, data: &[u8], dir: bool, crypt: bool) -> Vec<u8> {
        let mut tail = Vec::new();
        put_vint(&mut tail, if dir { 0x0003 } else { 0x0002 }); // file_flags(+mtime)
        put_vint(&mut tail, usize_);
        put_vint(&mut tail, 0x20); // attributes
        tail.extend_from_slice(&1_700_000_500u32.to_le_bytes()); // mtime
        put_vint(&mut tail, 0x03 << 7); // compression_info(Normal)
        put_vint(&mut tail, 0); // host os
        put_vint(&mut tail, name.len() as u64);
        tail.extend_from_slice(name.as_bytes());
        let mut extra = Vec::new();
        if crypt {
            let mut rec = Vec::new();
            put_vint(&mut rec, 1); // type = FHEXTRA_CRYPT
            rec.extend_from_slice(&[0u8; 4]);
            let mut sized = Vec::new();
            put_vint(&mut sized, rec.len() as u64);
            sized.extend_from_slice(&rec);
            extra = sized;
        }
        let flags = 0x0002 | if crypt { 0x0001 } else { 0 };
        block(2, &tail, data, flags, &extra)
    }

    #[test]
    fn rar5_lists_files_dirs_and_encrypted_flag() {
        let mut v = SIG5.to_vec();
        v.extend(block(1, &[0], &[], 0, &[])); // 메인 헤더
        v.extend(file5("dir", 0, &[], true, false));
        v.extend(file5("dir/a.txt", 100, &[0u8; 40], false, false));
        v.extend(file5("secret.txt", 10, &[0u8; 8], false, true));
        assert!(Rar.sniff(&v, &SliceSource(&v)));
        let l = Rar.list(&SliceSource(&v), &ListOpts::default()).unwrap();
        assert_eq!(l.label, "RAR 5");
        assert_eq!(l.entries.len(), 3);
        let a = l.entries.iter().find(|e| e.path == "dir/a.txt").unwrap();
        assert_eq!((a.size, a.packed, a.method.as_str()), (Some(100), Some(40), "Normal"));
        assert_eq!(a.modified, Some(1_700_000_500));
        assert!(l.entries.iter().find(|e| e.path == "dir").unwrap().is_dir);
        assert!(l.has_encrypted, "암호화 항목 집계");
    }

    #[test]
    fn rar5_encrypted_header_requires_password() {
        let mut v = SIG5.to_vec();
        v.extend(block(4, &[0], &[], 0, &[])); // 암호화 헤더 블록
        assert_eq!(
            Rar.list(&SliceSource(&v), &ListOpts::default()),
            Err(ArchiveError::PasswordRequired)
        );
    }

    #[test]
    fn rar4_lists_file_headers() {
        let mut v = SIG4.to_vec();
        // 메인 헤더(0x73)
        let mut main = vec![0u8; 13];
        main[2] = 0x73;
        main[3..5].copy_from_slice(&0u16.to_le_bytes());
        main[5..7].copy_from_slice(&13u16.to_le_bytes());
        v.extend(main);
        // 파일 헤더(0x74)
        let name = "old.txt";
        let hsize = 32 + name.len();
        let mut f = vec![0u8; hsize];
        f[2] = 0x74;
        f[3..5].copy_from_slice(&0x8000u16.to_le_bytes()); // ADD_SIZE 있음
        f[5..7].copy_from_slice(&(hsize as u16).to_le_bytes());
        f[7..11].copy_from_slice(&50u32.to_le_bytes()); // PACK_SIZE
        f[11..15].copy_from_slice(&120u32.to_le_bytes()); // UNP_SIZE
        f[16..20].copy_from_slice(&0xABCDu32.to_le_bytes()); // CRC
        let dt = (((46u32 << 9) | (8 << 5) | 24) << 16) | ((9u32 << 11) | (30 << 5));
        f[20..24].copy_from_slice(&dt.to_le_bytes());
        f[25] = 0x33; // Normal
        f[26..28].copy_from_slice(&(name.len() as u16).to_le_bytes());
        f[32..32 + name.len()].copy_from_slice(name.as_bytes());
        v.extend(f);
        v.extend(vec![0u8; 50]); // 데이터부
        let l = Rar.list(&SliceSource(&v), &ListOpts::default()).unwrap();
        assert_eq!(l.label, "RAR 4");
        assert_eq!(l.entries.len(), 1);
        let e = &l.entries[0];
        assert_eq!((e.path.as_str(), e.size, e.packed), ("old.txt", Some(120), Some(50)));
        assert_eq!(e.method, "Normal");
        assert_eq!(
            e.modified,
            Some(crate::archive::ymd_hms_to_unix(2026, 8, 24, 9, 30, 0))
        );
    }

    #[test]
    fn rar4_encrypted_headers_require_password() {
        let mut v = SIG4.to_vec();
        let mut main = vec![0u8; 13];
        main[2] = 0x73;
        main[3..5].copy_from_slice(&0x0080u16.to_le_bytes());
        main[5..7].copy_from_slice(&13u16.to_le_bytes());
        v.extend(main);
        assert_eq!(
            Rar.list(&SliceSource(&v), &ListOpts::default()),
            Err(ArchiveError::PasswordRequired)
        );
    }
}
