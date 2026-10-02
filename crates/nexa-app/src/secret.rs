//! 토큰 보관(X-37 — [ADR-0006](../../../docs/27-adr-0006-cloud-oauth.md) §2-3).
//!
//! **DPAPI**(`CryptProtectData` — crypt32 인박스, crate 0)로 암호화해
//! `data\secrets\cloud<N>.tok`에 둔다. 키는 **현재 사용자+머신에 바인딩**되므로
//! `data\`를 다른 PC로 옮기면 복호가 실패한다 — 재로그인이 필요하지만 **USB에 평문
//! 토큰이 남지 않는다**(의도된 포터블 안전 특성, ADR-0006 §2-3).
//!
//! 저장 형식 = DPAPI blob의 **소문자 hex 1줄**(설정 파일과 같은 텍스트 규율 — 바이너리
//! 파일을 늘리지 않아 포터블 진단이 쉽다).

use std::path::{Path, PathBuf};

/// 토큰 파일 폴더(`data\secrets`).
fn secrets_dir() -> PathBuf {
    crate::config::data_dir().join("secrets")
}

/// 연결 N번의 토큰 파일명.
fn tok_name(idx: usize) -> String {
    format!("cloud{idx}.tok")
}

/// refresh 토큰 저장(암호화). 실패는 `false` — 호출자는 "재로그인 필요"로 저하.
pub fn save_token(idx: usize, refresh: &str) -> bool {
    save_token_in(&secrets_dir(), idx, refresh)
}

/// refresh 토큰 로드(복호). 부재·복호 실패(타 PC·타 사용자) = `None`.
pub fn load_token(idx: usize) -> Option<String> {
    load_token_in(&secrets_dir(), idx)
}

/// 연결 해제 시 토큰 파일 제거(흔적 정리).
pub fn clear_token(idx: usize) {
    clear_token_in(&secrets_dir(), idx);
}

/// 연결 목록이 줄었을 때 인덱스 재배치 후 남는 꼬리 파일 정리 — 단건 폐기 [`clear_token`] 재사용.
pub fn clear_from(idx: usize) {
    clear_tail(idx, clear_token);
}

/// 저장 본체 — [`crate::config::save`](temp+`sync_all`+rename)로 **원자적**. 쓰기 중 크래시·
/// 전원 차단에도 0바이트/반쪽 hex가 남아 재로그인을 강요하지 않는다(G10-17 — 재구현 금지).
fn save_token_in(dir: &Path, idx: usize, refresh: &str) -> bool {
    let Some(blob) = protect(refresh.as_bytes()) else {
        return false;
    };
    let hex: String = blob.iter().map(|b| format!("{b:02x}")).collect();
    crate::config::save(dir, &tok_name(idx), &hex).is_ok()
}

fn load_token_in(dir: &Path, idx: usize) -> Option<String> {
    let hex = std::fs::read_to_string(dir.join(tok_name(idx))).ok()?;
    let blob = decode_hex(&hex)?;
    let plain = unprotect(&blob)?;
    String::from_utf8(plain).ok()
}

/// 부재·실패 무시 — `remove_file`이 부재를 오류로 돌리므로 선검사 없이 결과를 버린다.
fn clear_token_in(dir: &Path, idx: usize) {
    let _ = std::fs::remove_file(dir.join(tok_name(idx)));
}

/// 슬롯 상한 — `clear_from`이 훑는 꼬리 범위(연결 수 상한과 같은 32).
const MAX_SLOTS: usize = 32;

/// 꼬리 범위 순회 — 제거 동작은 주입(실사용 = [`clear_token`], 테스트 = 임시 폴더).
fn clear_tail(idx: usize, rm: impl FnMut(usize)) {
    (idx..MAX_SLOTS).for_each(rm);
}

/// 소문자 hex 1줄 → 바이트. 앞뒤 공백 허용. **ASCII가 아니거나**(BOM·멀티바이트 — 편집기
/// 저장 흔적) 홀수 길이·빈 문자열·비hex 문자는 `None` — 바이트 슬라이싱이 문자 경계를
/// 가르며 panic 하던 손상 파일 경로를 막는다(G10-04).
fn decode_hex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if s.is_empty() || !s.is_ascii() || !s.len().is_multiple_of(2) {
        return None;
    }
    s.as_bytes()
        .chunks(2)
        .map(|c| u8::from_str_radix(std::str::from_utf8(c).ok()?, 16).ok())
        .collect()
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
        let _ = windows::Win32::Foundation::LocalFree(Some(windows::Win32::Foundation::HLOCAL(
            out.pbData as *mut core::ffi::c_void,
        )));
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
        let _ = windows::Win32::Foundation::LocalFree(Some(windows::Win32::Foundation::HLOCAL(
            out.pbData as *mut core::ffi::c_void,
        )));
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

    /// 테스트 전용 임시 secrets 폴더 — 실사용 `data\secrets`를 건드리지 않는다. Drop 시 제거.
    struct TempSecrets(PathBuf);
    impl TempSecrets {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "nexa-secret-{tag}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            Self(dir)
        }
        fn path(&self) -> &Path {
            &self.0
        }
        /// 폴더 안 파일명 목록(정렬) — `.tmp` 잔존·꼬리 정리 단언용.
        fn names(&self) -> Vec<String> {
            let mut v: Vec<String> = std::fs::read_dir(&self.0)
                .map(|rd| {
                    rd.filter_map(|e| e.ok())
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default();
            v.sort();
            v
        }
    }
    impl Drop for TempSecrets {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// DPAPI 왕복 — 저장 후 같은 사용자/PC에서 복호되어야 한다. 폴더는 임시 디렉터리이고
    /// 저장은 원자적(G10-17) — 임시 `.tmp`가 남지 않아야 한다.
    #[cfg(windows)]
    #[test]
    fn token_roundtrip_and_clear() {
        let t = TempSecrets::new("roundtrip");
        let idx = 31;
        let secret = "refresh-token-테스트-\u{1F510}";
        assert!(
            save_token_in(t.path(), idx, secret),
            "저장 성공(폴더 자동 생성)"
        );
        assert_eq!(
            t.names(),
            vec![tok_name(idx)],
            ".tmp 잔존 없이 토큰 파일 하나"
        );
        assert_eq!(
            load_token_in(t.path(), idx).as_deref(),
            Some(secret),
            "복호 일치"
        );
        // 덮어쓰기 — 기존 파일이 있어도 원자 교체(rename REPLACE_EXISTING).
        assert!(save_token_in(t.path(), idx, "second"));
        assert_eq!(load_token_in(t.path(), idx).as_deref(), Some("second"));
        assert_eq!(t.names(), vec![tok_name(idx)]);
        clear_token_in(t.path(), idx);
        assert_eq!(load_token_in(t.path(), idx), None, "제거 후 부재");
        assert!(t.names().is_empty());
        clear_token_in(t.path(), idx); // 부재 재제거 — panic 없음
    }

    /// clear_from — 꼬리 슬롯(idx 이상)만 제거하고 앞 슬롯은 보존. 부재 폴더에서도 panic 없음.
    #[cfg(windows)]
    #[test]
    fn clear_from_removes_tail_only() {
        let t = TempSecrets::new("clearfrom");
        clear_tail(0, |i| clear_token_in(t.path(), i)); // 폴더 자체가 없어도 조용히
        for i in [0, 1, 2, MAX_SLOTS - 1] {
            assert!(save_token_in(t.path(), i, &format!("tok{i}")));
        }
        clear_tail(2, |i| clear_token_in(t.path(), i));
        assert_eq!(t.names(), vec![tok_name(0), tok_name(1)]);
        assert_eq!(load_token_in(t.path(), 1).as_deref(), Some("tok1"));
        assert_eq!(load_token_in(t.path(), 2), None);
        assert_eq!(load_token_in(t.path(), MAX_SLOTS - 1), None);
    }

    /// 손상된 hex·부재 파일은 panic 없이 None.
    #[test]
    fn corrupt_or_missing_is_none() {
        let t = TempSecrets::new("missing");
        assert_eq!(load_token_in(t.path(), 30), None, "폴더 부재");
        assert_eq!(load_token(30), None, "실사용 미저장 슬롯");
    }

    /// G10-04 — 비ASCII(멀티바이트·BOM)는 문자 경계 panic 없이 None, 정상 hex는 바이트.
    #[test]
    fn decode_hex_rejects_non_ascii_and_odd() {
        assert_eq!(
            decode_hex("a\u{e9}a"),
            None,
            "멀티바이트 — 바이트 길이 4(짝수)지만 거부"
        );
        assert_eq!(decode_hex("\u{feff}00ff"), None, "BOM 선두");
        assert_eq!(decode_hex("00ff"), Some(vec![0x00, 0xff]));
        assert_eq!(
            decode_hex("  00FF\r\n"),
            Some(vec![0x00, 0xff]),
            "앞뒤 공백·대문자 허용"
        );
        assert_eq!(decode_hex(""), None);
        assert_eq!(decode_hex("0"), None, "홀수 길이");
        assert_eq!(decode_hex("0g"), None, "비hex 문자");
    }

    /// 손상 토큰 파일(멀티바이트 섞임)을 실제로 써 두고 load_token이 None인지 — 기동 복원 경로.
    #[test]
    fn corrupt_token_file_is_none() {
        let t = TempSecrets::new("corrupt");
        let idx = 29;
        std::fs::create_dir_all(t.path()).expect("임시 폴더");
        std::fs::write(t.path().join(tok_name(idx)), "a\u{e9}a").expect("손상 파일 작성");
        assert_eq!(load_token_in(t.path(), idx), None);
    }
}
