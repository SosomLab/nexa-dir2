//! Microsoft Cabinet(CAB) 목록 — CFHEADER → CFFILE 테이블(평문)만 읽는다.
//!
//! 구조(MS-CAB): `CFHEADER · CFFOLDER[] · CFFILE[] · CFDATA[]`. 파일 테이블은
//! 압축되지 않으므로 이름·크기·시각·속성을 코덱 없이 얻는다(압축 방식은 폴더가
//! 보유 — MSZIP/Quantum/LZX).

use super::{
    decode_name, dos_to_unix, finish, normalize_path, read_exact_at, u16le, u32le, ArchiveEntry,
    ArchiveError, ArchiveFormat, ListOpts, Listing, ReadAt,
};

pub struct Cab;

/// CFHEADER 고정부 크기.
const HDR: usize = 36;

/// 폴더 압축 방식 표시명(typeCompress 하위 4비트).
fn folder_method(t: u16) -> &'static str {
    match t & 0x000F {
        0 => "Store",
        1 => "MSZIP",
        2 => "Quantum",
        3 => "LZX",
        _ => "?",
    }
}

impl ArchiveFormat for Cab {
    fn id(&self) -> &'static str {
        "cab"
    }
    fn label(&self) -> &'static str {
        "CAB"
    }
    fn exts(&self) -> &'static [&'static str] {
        &["cab"]
    }
    fn sniff(&self, head: &[u8], _src: &dyn ReadAt) -> bool {
        head.starts_with(b"MSCF") && u32le(head, 4) == Some(0)
    }

    fn list(&self, src: &dyn ReadAt, opts: &ListOpts) -> Result<Listing, ArchiveError> {
        let h = read_exact_at(src, 0, HDR.min(src.size() as usize))?;
        if !h.starts_with(b"MSCF") {
            return Err(ArchiveError::NotArchive);
        }
        let coff_files = u32le(&h, 16).unwrap_or(0) as u64;
        let n_folders = u16le(&h, 26).unwrap_or(0) as usize;
        let n_files = u16le(&h, 28).unwrap_or(0) as usize;
        let flags = u16le(&h, 30).unwrap_or(0);

        // 예약 영역(cfhdrRESERVE_PRESENT) — 폴더 항목 크기·헤더 뒤 오프셋에 영향
        let mut cur = HDR as u64;
        let mut folder_reserve = 0u64;
        if flags & 0x0004 != 0 {
            let r = read_exact_at(src, cur, 4)?;
            let cb_header = u16le(&r, 0).unwrap_or(0) as u64;
            folder_reserve = r[2] as u64;
            cur += 4 + cb_header;
        }
        // 이전/다음 캐비닛 이름(분할) — NUL 종료 문자열 2개씩
        let multivolume = flags & 0x0003 != 0;
        for bit in [0x0001u16, 0x0002] {
            if flags & bit != 0 {
                for _ in 0..2 {
                    cur = skip_cstr(src, cur)?;
                }
            }
        }
        // CFFOLDER[] — 압축 방식 표시용
        let mut methods: Vec<&'static str> = Vec::new();
        for _ in 0..n_folders.min(4096) {
            let f = read_exact_at(src, cur, 8)?;
            methods.push(folder_method(u16le(&f, 6).unwrap_or(0)));
            cur += 8 + folder_reserve;
        }

        let mut out = Listing {
            format: self.id().into(),
            label: self.label().into(),
            multivolume,
            ..Default::default()
        };
        let limit = opts.limit();
        let mut off = coff_files;
        for _ in 0..n_files.min(limit) {
            let f = read_exact_at(src, off, 16)?;
            let size = u32le(&f, 0).unwrap_or(0) as u64;
            let ifolder = u16le(&f, 8).unwrap_or(0) as usize;
            let date = u16le(&f, 10).unwrap_or(0);
            let time = u16le(&f, 12).unwrap_or(0);
            let attribs = u16le(&f, 14).unwrap_or(0);
            let (name, next) = read_cstr(src, off + 16)?;
            off = next;
            // _A_NAME_IS_UTF(0x80) = 이름이 UTF-8
            let raw = decode_name(&name, attribs & 0x80 != 0);
            let (path, suspicious) = normalize_path(&raw);
            if path.is_empty() {
                continue;
            }
            out.entries.push(ArchiveEntry {
                path,
                is_dir: false, // CAB은 폴더 항목이 없다(경로가 이름에 포함)
                size: Some(size),
                packed: None, // 폴더 단위 압축 — 항목별 압축 크기 없음
                modified: dos_to_unix(date, time),
                encrypted: false,
                method: methods.get(ifolder).copied().unwrap_or("").to_string(),
                crc32: None,
                suspicious,
            });
        }
        if out.entries.is_empty() && n_files > 0 {
            return Err(ArchiveError::Corrupt("CFFILE 테이블 읽기 실패".into()));
        }
        Ok(finish(out, limit))
    }
}

/// NUL 종료 문자열 읽기 — (바이트, 다음 오프셋). 상한 = [`super::MAX_NAME`].
fn read_cstr(src: &dyn ReadAt, off: u64) -> Result<(Vec<u8>, u64), ArchiveError> {
    let mut buf = vec![0u8; 256.min(super::MAX_NAME)];
    let mut out: Vec<u8> = Vec::new();
    let mut cur = off;
    loop {
        let n = src.read_at(cur, &mut buf)?;
        if n == 0 {
            return Err(ArchiveError::Corrupt("이름 종료 없음".into()));
        }
        if let Some(i) = buf[..n].iter().position(|&b| b == 0) {
            out.extend_from_slice(&buf[..i]);
            return Ok((out, cur + i as u64 + 1));
        }
        out.extend_from_slice(&buf[..n]);
        cur += n as u64;
        if out.len() > super::MAX_NAME {
            return Err(ArchiveError::Corrupt("이름 과대".into()));
        }
    }
}

fn skip_cstr(src: &dyn ReadAt, off: u64) -> Result<u64, ArchiveError> {
    Ok(read_cstr(src, off)?.1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::SliceSource;

    /// 최소 CAB 조립기(헤더 + 폴더 1 + 파일 N).
    fn build(files: &[(&str, u32)]) -> Vec<u8> {
        let mut out = vec![0u8; HDR];
        out[..4].copy_from_slice(b"MSCF");
        out[24] = 3; // versionMinor
        out[25] = 1; // versionMajor
        out[26..28].copy_from_slice(&1u16.to_le_bytes()); // cFolders
        out[28..30].copy_from_slice(&(files.len() as u16).to_le_bytes());
        // CFFOLDER 1개(LZX)
        let folder_off = out.len();
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&3u16.to_le_bytes());
        let _ = folder_off;
        let coff_files = out.len() as u32;
        out[16..20].copy_from_slice(&coff_files.to_le_bytes());
        let (date, time) = ((46u16 << 9) | (8 << 5) | 24, (9u16 << 11) | (30 << 5));
        for (name, size) in files {
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // iFolder
            out.extend_from_slice(&date.to_le_bytes());
            out.extend_from_slice(&time.to_le_bytes());
            out.extend_from_slice(&0x80u16.to_le_bytes()); // UTF 이름
            out.extend_from_slice(name.as_bytes());
            out.push(0);
        }
        out
    }

    #[test]
    fn lists_files_with_folder_method_and_time() {
        let c = build(&[("setup.exe", 1024), ("bin\\app.dll", 2048)]);
        assert!(Cab.sniff(&c, &SliceSource(&c)));
        let l = Cab.list(&SliceSource(&c), &ListOpts::default()).unwrap();
        assert_eq!(l.entries.len(), 2);
        let d = l.entries.iter().find(|e| e.path == "bin/app.dll").unwrap();
        assert_eq!((d.size, d.method.as_str()), (Some(2048), "LZX"));
        assert_eq!(
            d.modified,
            Some(crate::archive::ymd_hms_to_unix(2026, 8, 24, 9, 30, 0))
        );
    }

    #[test]
    fn non_cab_is_rejected() {
        let junk = vec![0u8; 64];
        assert!(!Cab.sniff(&junk, &SliceSource(&junk)));
        assert_eq!(
            Cab.list(&SliceSource(&junk), &ListOpts::default()),
            Err(ArchiveError::NotArchive)
        );
    }
}
