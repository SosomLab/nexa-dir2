//! 비밀 문자열(암호) 보관 — **평문 노출·기록 금지**를 타입 수준에서 강제한다
//! (X-46 압축 미리보기 암호 입력. 설계 SSOT = docs/28 §5).
//!
//! 규약(사용자 지시 08-24: "입력된 내용은 전달만 하고 기록되거나 Plain으로
//! 노출되지 않도록"):
//!
//! 1. **기록 금지** — 설정·로그·상태 파일 어디에도 쓰지 않는다. 이 타입은
//!    직렬화 수단을 제공하지 않으며, `Display`/`ToString`/`AsRef<str>`도 없다.
//!    (토큰처럼 DPAPI로 저장하는 경로와 의도적으로 분리 — 암호는 **세션 한정**.)
//! 2. **평문 노출 금지** — `Debug`는 항상 `Secret(***)`. 길이조차 흘리지 않는다.
//!    실제 바이트 접근은 [`Secret::expose`] 하나뿐이며, 이름 자체가 감사 지점이다.
//! 3. **폐기 시 소거** — `Drop`에서 `write_volatile`로 0을 덮는다(컴파일러 최적화
//!    제거 방지 = volatile + 컴파일러 펜스). 재할당으로 사본이 흩어지지 않도록
//!    용량을 미리 확보한 뒤에만 채운다.
//! 4. **경유 버퍼도 소거** — 입력 컨트롤에서 읽어온 임시 버퍼는 [`zeroize_bytes`]·
//!    [`zeroize_u16`]·[`zeroize_string`]으로 호출자가 즉시 지운다.
//!
//! 크레이트 0(DR-8) — `zeroize` 등 외부 crate 없이 표준 라이브러리만 쓴다.

use std::sync::atomic::{compiler_fence, Ordering};

/// 슬라이스를 0으로 덮는다(최적화 제거 방지 — volatile 쓰기 + 컴파일러 펜스).
pub fn zeroize_bytes(buf: &mut [u8]) {
    for b in buf.iter_mut() {
        unsafe { std::ptr::write_volatile(b, 0) };
    }
    compiler_fence(Ordering::SeqCst);
}

/// UTF-16 버퍼 소거(Win32 `WM_GETTEXT` 경유 버퍼 — 호출자가 즉시 호출).
pub fn zeroize_u16(buf: &mut [u16]) {
    for b in buf.iter_mut() {
        unsafe { std::ptr::write_volatile(b, 0) };
    }
    compiler_fence(Ordering::SeqCst);
}

/// `String` 내용 소거 후 비움(재할당 없이 그 자리 버퍼를 덮는다).
pub fn zeroize_string(s: &mut String) {
    // SAFETY: 0x00은 유효한 UTF-8 — 길이 유지한 채 바이트만 덮은 뒤 clear.
    unsafe { zeroize_bytes(s.as_mut_vec()) };
    s.clear();
}

/// 세션 한정 비밀(암호) — 사본이 남지 않도록 폐기 시 소거.
pub struct Secret {
    bytes: Vec<u8>,
}

impl Secret {
    /// 바이트 소유권을 받아 감싼다(원본 `Vec`은 이동되므로 사본이 남지 않는다).
    pub fn new(bytes: Vec<u8>) -> Self {
        Secret { bytes }
    }

    /// 문자열에서 만들고 **원본 버퍼까지 소거**한다(입력 컨트롤 → Secret 표준 경로).
    pub fn take_from_string(s: &mut String) -> Self {
        let mut bytes = Vec::with_capacity(s.len()); // 재할당 = 미소거 사본 → 선확보
        bytes.extend_from_slice(s.as_bytes());
        zeroize_string(s);
        Secret { bytes }
    }

    /// UTF-16(Win32 편집 컨트롤 원문) → Secret. `src`도 소거한다.
    pub fn take_from_u16(src: &mut [u16]) -> Self {
        let end = src.iter().position(|&c| c == 0).unwrap_or(src.len());
        let mut s = String::from_utf16_lossy(&src[..end]);
        let sec = Secret::take_from_string(&mut s);
        zeroize_u16(src);
        sec
    }

    /// 원문 바이트 — **감사 지점**. 전달 직후 사본을 남기지 말 것.
    pub fn expose(&self) -> &[u8] {
        &self.bytes
    }

    /// UTF-8 원문(비UTF-8이면 `None`) — 리더에 넘기는 용도.
    pub fn expose_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.bytes).ok()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// `Drop`이 실행하는 소거 그 자체. 분리해 둔 이유는 **테스트가 살아 있는 버퍼로
    /// 계약을 확인**할 수 있게 하기 위해서다 — 폐기된 메모리를 다시 읽는 검사는 UB이고,
    /// 할당자가 free 리스트 메타데이터를 그 자리에 쓰는 macOS·Linux에서 실제로 깨진다.
    fn zeroize(&mut self) {
        zeroize_bytes(&mut self.bytes);
    }
}

impl Clone for Secret {
    /// 세션 캐시(같은 아카이브 재조회) 용도 — 사본도 Drop에서 소거된다.
    fn clone(&self) -> Self {
        let mut bytes = Vec::with_capacity(self.bytes.len());
        bytes.extend_from_slice(&self.bytes);
        Secret { bytes }
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.zeroize();
    }
}

/// 어떤 경로로도 평문이 새지 않게 — 길이도 감춘다.
impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(***)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_reveals_plaintext_or_length() {
        let s = Secret::new(b"hunter2".to_vec());
        assert_eq!(format!("{s:?}"), "Secret(***)");
        assert!(!format!("{s:?}").contains("hunter"));
        assert!(!format!("{s:?}").contains('7'), "길이도 흘리지 않는다");
    }

    #[test]
    fn take_from_string_zeroizes_source() {
        let mut src = String::from("pass워드");
        let raw = src.as_ptr();
        let len = src.len();
        let sec = Secret::take_from_string(&mut src);
        assert_eq!(sec.expose_str(), Some("pass워드"));
        assert!(src.is_empty(), "원본은 비워진다");
        // 원본 버퍼(용량 유지)가 실제로 0으로 덮였는지 — clear는 길이만 줄인다
        let seen = unsafe { std::slice::from_raw_parts(raw, len) };
        assert!(seen.iter().all(|&b| b == 0), "원본 버퍼 소거");
    }

    #[test]
    fn take_from_u16_reads_until_nul_and_clears() {
        let mut buf: Vec<u16> = "pw\u{0}rest".encode_utf16().collect();
        let sec = Secret::take_from_u16(&mut buf);
        assert_eq!(sec.expose(), b"pw");
        assert!(buf.iter().all(|&c| c == 0), "경유 버퍼 소거");
    }

    #[test]
    fn clone_is_independent_and_drop_zeroizes() {
        let sec = Secret::new(b"abc".to_vec());
        let mut c = sec.clone();
        assert_eq!(c.expose(), b"abc");
        // 소거는 **살아 있는 버퍼**에서 확인한다. 종전 판본은 drop 후 원래 주소를
        // 다시 읽었는데(UB), 해제된 블록 앞머리에 free 리스트 포인터를 써 넣는
        // macOS·Linux 할당자에서 실제로 실패했다(Windows만 우연히 통과 —
        // 08-24~09-02 core 잡 적색의 원인).
        c.zeroize();
        assert!(c.expose().iter().all(|&b| b == 0), "Drop 소거");
        drop(c);
        assert_eq!(sec.expose(), b"abc", "사본 폐기가 원본에 영향 없음");
    }
}
