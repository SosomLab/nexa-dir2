//! 7z 판정 — 7z는 **헤더 자체가 압축(LZMA)** 되는 것이 기본값이라 목록을 읽으려면
//! 코덱이 필요하다. 내장은 여기까지를 **정직하게** 알린다:
//!
//! - 헤더가 AES로 암호화(`-mhe=on`)면 → [`ArchiveError::PasswordRequired`]
//! - 그 외 압축 헤더 → [`ArchiveError::NeedsCodec`]("7z", "LZMA") = **플러그인 담당**
//!   (호스트가 "플러그인 필요" 안내로 번역 — docs/28 §4).
//!
//! 시작 헤더 구조: `signature(6) · version(2) · startHeaderCRC(4) ·
//! nextHeaderOffset(8) · nextHeaderSize(8) · nextHeaderCRC(4)` → 다음 헤더는
//! 파일 오프셋 `32 + nextHeaderOffset`. 첫 바이트 = `0x01`(kHeader, 평문) 또는
//! `0x17`(kEncodedHeader, 압축/암호화).
//!
//! 확장 지점: 평문 헤더(`0x01`) 파싱이나 LZMA 디코더가 들어오면 이 파일만 고치면
//! 된다(레지스트리·라우팅·표시는 그대로).

use super::{
    read_exact_at, u64le, ArchiveError, ArchiveFormat, ListOpts, Listing, ReadAt,
};

pub struct SevenZ;

const SIG: &[u8] = b"7z\xbc\xaf\x27\x1c";
/// AES-256 + SHA-256 코더 ID(7z 폴더 코더 — 헤더 암호화 판정).
const CODER_AES: [u8; 4] = [0x06, 0xF1, 0x07, 0x01];

impl ArchiveFormat for SevenZ {
    fn id(&self) -> &'static str {
        "7z"
    }
    fn label(&self) -> &'static str {
        "7z"
    }
    fn exts(&self) -> &'static [&'static str] {
        &["7z"]
    }
    fn sniff(&self, head: &[u8], _src: &dyn ReadAt) -> bool {
        head.starts_with(SIG)
    }

    fn list(&self, src: &dyn ReadAt, _opts: &ListOpts) -> Result<Listing, ArchiveError> {
        let start = read_exact_at(src, 0, 32.min(src.size() as usize))?;
        if !start.starts_with(SIG) {
            return Err(ArchiveError::NotArchive);
        }
        let next_off = u64le(&start, 12).unwrap_or(0);
        let next_size = u64le(&start, 20).unwrap_or(0);
        if next_size == 0 {
            // 빈 아카이브 — 목록이 없다는 사실 자체는 정상 결과
            return Ok(Listing {
                format: self.id().into(),
                label: self.label().into(),
                ..Default::default()
            });
        }
        let at = 32u64.saturating_add(next_off);
        let n = next_size.min(64 * 1024) as usize;
        let hdr = read_exact_at(src, at, n.min(src.size().saturating_sub(at) as usize))?;
        match hdr.first() {
            // 압축/암호화 헤더 — AES 코더가 보이면 암호가 필요한 아카이브
            Some(0x17) if hdr.windows(4).any(|w| w == CODER_AES) => {
                Err(ArchiveError::PasswordRequired)
            }
            Some(0x17) => Err(ArchiveError::NeedsCodec("7z".into(), "LZMA".into())),
            // 평문 헤더도 스트림 구조 해석에 코덱 지식이 필요 — 플러그인 경로로
            Some(0x01) => Err(ArchiveError::NeedsCodec("7z".into(), "LZMA".into())),
            _ => Err(ArchiveError::Corrupt("7z 다음 헤더 손상".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::SliceSource;

    fn build(next: &[u8]) -> Vec<u8> {
        let mut v = vec![0u8; 32];
        v[..6].copy_from_slice(SIG);
        v[6] = 0;
        v[7] = 4;
        v[12..20].copy_from_slice(&0u64.to_le_bytes()); // nextHeaderOffset
        v[20..28].copy_from_slice(&(next.len() as u64).to_le_bytes());
        v.extend_from_slice(next);
        v
    }

    #[test]
    fn encoded_header_reports_codec_need() {
        let v = build(&[0x17, 0x06, 0x00, 0x01]);
        assert!(SevenZ.sniff(&v, &SliceSource(&v)));
        assert_eq!(
            SevenZ.list(&SliceSource(&v), &ListOpts::default()),
            Err(ArchiveError::NeedsCodec("7z".into(), "LZMA".into()))
        );
    }

    #[test]
    fn aes_coder_in_header_asks_for_password() {
        let mut next = vec![0x17u8, 0x06, 0x01, 0x09];
        next.extend_from_slice(&CODER_AES);
        let v = build(&next);
        assert_eq!(
            SevenZ.list(&SliceSource(&v), &ListOpts::default()),
            Err(ArchiveError::PasswordRequired)
        );
    }

    #[test]
    fn empty_archive_lists_nothing() {
        let v = build(&[]);
        let l = SevenZ.list(&SliceSource(&v), &ListOpts::default()).unwrap();
        assert!(l.entries.is_empty() && l.format == "7z");
    }
}
