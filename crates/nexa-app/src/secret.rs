//! 토큰 보관(X-37 — [ADR-0006](../../../docs/27-adr-0006-cloud-oauth.md) §2-3).
//!
//! **DPAPI**(`CryptProtectData` — crypt32 인박스, crate 0)로 암호화해
//! `data\secrets\cloud<N>.tok`에 둔다. 키는 **현재 사용자+머신에 바인딩**되므로
//! `data\`를 다른 PC로 옮기면 복호가 실패한다 — 재로그인이 필요하지만 **USB에 평문
//! 토큰이 남지 않는다**(의도된 포터블 안전 특성, ADR-0006 §2-3).
//!
//! 저장 형식 = DPAPI blob의 **소문자 hex 1줄**(설정 파일과 같은 텍스트 규율 — 바이너리
//! 파일을 늘리지 않아 포터블 진단이 쉽다).

use std::path::PathBuf;

/// 연결 N번의 토큰 파일 경로.
fn tok_path(idx: usize) -> PathBuf {
    crate::config::data_dir()
        .join("secrets")
        .join(format!("cloud{idx}.tok"))
}

/// refresh 토큰 저장(암호화). 실패는 `false` — 호출자는 "재로그인 필요"로 저하.
pub fn save_token(idx: usize, refresh: &str) -> bool {
    let Some(blob) = protect(refresh.as_bytes()) else {
        return false;
    };
    let hex: String = blob.iter().map(|b| format!("{b:02x}")).collect();
    let p = tok_path(idx);
    let Some(dir) = p.parent() else { return false };
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    std::fs::write(&p, hex).is_ok()
}

/// refresh 토큰 로드(복호). 부재·복호 실패(타 PC·타 사용자) = `None`.
#[allow(dead_code)] // 사용처 = ADR-0006 §3 2차(탐색) — 저장된 refresh 로드
pub fn load_token(idx: usize) -> Option<String> {
    let hex = std::fs::read_to_string(tok_path(idx)).ok()?;
    let hex = hex.trim();
    if hex.len() % 2 != 0 || hex.is_empty() {
        return None;
    }
    let blob: Option<Vec<u8>> = (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok())
        .collect();
    let plain = unprotect(&blob?)?;
    String::from_utf8(plain).ok()
}

/// 연결 해제 시 토큰 파일 제거(흔적 정리).
#[allow(dead_code)] // clear_from이 현 경로 — 단건 폐기는 2차 재인증에서 사용
pub fn clear_token(idx: usize) {
    let _ = std::fs::remove_file(tok_path(idx));
}

/// 연결 목록이 줄었을 때 인덱스 재배치 후 남는 꼬리 파일 정리.
pub fn clear_from(idx: usize) {
    for i in idx..32 {
        let p = tok_path(i);
        if p.exists() {
            let _ = std::fs::remove_file(p);
        }
    }
}

#[cfg(windows)]
fn protect(data: &[u8]) -> Option<Vec<u8>> {
    use windows::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB};
    unsafe {
        let input = CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        };
        let mut out = CRYPT_INTEGER_BLOB::default();
        CryptProtectData(&input, None, None, None, None, 0, &mut out).ok()?;
        let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        let _ = windows::Win32::Foundation::LocalFree(Some(
            windows::Win32::Foundation::HLOCAL(out.pbData as *mut core::ffi::c_void),
        ));
        Some(v)
    }
}

#[cfg(windows)]
fn unprotect(blob: &[u8]) -> Option<Vec<u8>> {
    use windows::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};
    unsafe {
        let input = CRYPT_INTEGER_BLOB {
            cbData: blob.len() as u32,
            pbData: blob.as_ptr() as *mut u8,
        };
        let mut out = CRYPT_INTEGER_BLOB::default();
        CryptUnprotectData(&input, None, None, None, None, 0, &mut out).ok()?;
        let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        let _ = windows::Win32::Foundation::LocalFree(Some(
            windows::Win32::Foundation::HLOCAL(out.pbData as *mut core::ffi::c_void),
        ));
        Some(v)
    }
}

#[cfg(not(windows))]
fn protect(_data: &[u8]) -> Option<Vec<u8>> {
    None // 비Windows 빌드는 클라우드 인증 미지원
}
#[cfg(not(windows))]
fn unprotect(_blob: &[u8]) -> Option<Vec<u8>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DPAPI 왕복 — 저장 후 같은 사용자/PC에서 복호되어야 한다.
    #[cfg(windows)]
    #[test]
    fn token_roundtrip_and_clear() {
        let idx = 31; // 실사용 인덱스와 충돌하지 않는 꼬리 슬롯
        let secret = "refresh-token-테스트-\u{1F510}";
        assert!(save_token(idx, secret), "저장 성공");
        assert_eq!(load_token(idx).as_deref(), Some(secret), "복호 일치");
        clear_token(idx);
        assert_eq!(load_token(idx), None, "제거 후 부재");
    }

    /// 손상된 hex·부재 파일은 panic 없이 None.
    #[test]
    fn corrupt_or_missing_is_none() {
        assert_eq!(load_token(30), None); // 미저장 슬롯
    }
}
