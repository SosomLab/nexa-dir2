//! shellpath — **`shell:` 특수 폴더 스킴** 해석(사용자 요청 07-17 — 탐색기 동일).
//!
//! `shell:startup`·`shell:common startup`·`shell:downloads`·`shell:sendto`·
//! `shell:::{GUID}` 등 **탐색기가 받는 전체 이름**을 그대로 지원한다 — 수동 테이블
//! 대신 셸의 정식 해석기 `SHParseDisplayName`(shell32 인박스, B3 무변)에 위임하므로
//! KnownFolders 레지스트리에 등록된 이름 전부와 향후 OS 추가분까지 자동 커버.
//!
//! 파일시스템 경로가 없는 가상 폴더(예: `shell:::{휴지통}`)는 해석 실패로 두고
//! **원문을 반환** — 기존 "열기 실패 = 위치 유지" 격리 경로가 그대로 처리한다.

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::UI::Shell::{Common::ITEMIDLIST, SHGetPathFromIDListEx, SHParseDisplayName};

/// `shell:` 스킴이면 실제 폴더 경로로 해석(아니면·실패하면 원문 그대로).
/// 경로 바 제출 파이프라인(expand_env 뒤)에서 호출 — UI 스레드(COM 초기화됨).
pub fn resolve(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.len() < 6 || !trimmed[..6].eq_ignore_ascii_case("shell:") {
        return input.to_string();
    }
    unsafe {
        let w = HSTRING::from(trimmed);
        let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
        if SHParseDisplayName(PCWSTR(w.as_ptr()), None, &mut pidl, 0, None).is_err() {
            return input.to_string(); // 미지 이름 — 원문 유지(열기 실패로 자연 격리)
        }
        // 긴 경로 여유(GPFIDL_DEFAULT — 표준 FS 경로)
        let mut buf = [0u16; 1024];
        let ok = SHGetPathFromIDListEx(pidl, &mut buf, windows::Win32::UI::Shell::GPFIDL_DEFAULT)
            .as_bool();
        windows::Win32::System::Com::CoTaskMemFree(Some(pidl as *const core::ffi::c_void));
        if !ok {
            return input.to_string(); // 가상 폴더(FS 경로 없음) — 원문 유지
        }
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SHParseDisplayName은 호출 스레드 COM 초기화 필요(런타임 = UI 스레드 OLE ✓,
    /// 테스트 스레드는 직접 초기화 — S_FALSE(중복)도 무해).
    fn init_com() {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
            );
        }
    }

    #[test]
    fn resolves_known_shell_names() {
        init_com();
        // 대표 이름들 — 탐색기와 동일 해석(레지스트리 KnownFolders 위임)
        let startup = resolve("shell:startup");
        assert!(
            startup.to_lowercase().ends_with("startup") && !startup.starts_with("shell:"),
            "{startup}"
        );
        let common = resolve("shell:common startup");
        assert!(
            common.to_lowercase().contains("programdata")
                && common.to_lowercase().ends_with("startup"),
            "{common}"
        );
        let dl = resolve("Shell:Downloads"); // 대소문자 무시
        assert!(dl.to_lowercase().contains("downloads"), "{dl}");
    }

    #[test]
    fn passthrough_and_failure_keep_input() {
        assert_eq!(resolve("C:\\Windows"), "C:\\Windows"); // 스킴 아님 = 원문
        assert_eq!(
            resolve("shell:no-such-folder-xyz"),
            "shell:no-such-folder-xyz"
        );
        assert_eq!(resolve("shel"), "shel"); // 6자 미만 방어
    }
}
