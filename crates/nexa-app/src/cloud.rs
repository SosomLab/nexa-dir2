//! 클라우드 동기화 클라이언트 탐지(X-36 — [검토서 26 §2](../../../docs/26-cloud-integration-study.md)).
//!
//! 설치된 동기화 클라이언트(OneDrive 개인/비즈니스·Google Drive·Dropbox)의 **로컬
//! 폴더를 탐지**해 후보로 돌려준다 — 네트워크 0·전부 로컬 읽기(레지스트리·파일·볼륨
//! 라벨). 연결(영속)은 설정 `cloudN`([`crate::config::CloudConn`]) 소관, 내 PC 노출은
//! `nexa_vfs::set_extra_roots` 경유. 탐지 실패·잔재 레지스트리는 **실존 프로브**로 방어.

use std::path::PathBuf;
/// 드라이브 루트 판정(`detect_googledrive`)에만 쓰여 Windows 전용이다.
#[cfg(windows)]
use std::path::Path;

/// 탐지된 클라우드 후보 — 아직 "연결"은 아니다(연결 추가 메뉴·This PC 우클릭의 소스).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloudCandidate {
    /// 종류 식별자: `"onedrive"` | `"googledrive"` | `"dropbox"` (설정 kind와 동일).
    pub kind: &'static str,
    pub label: String,
    pub path: PathBuf,
}

/// 종류별 웹 URL(온라인 보기·URL 복사 — 검토서 26 §2). 로그인은 **브라우저 세션**
/// 소관(사용자 요청 08-01: 프라이빗 창에서 다른 계정 접속용으로 URL 복사 제공).
pub fn web_url(kind: &str) -> &'static str {
    match kind {
        "onedrive" => "https://onedrive.live.com/",
        "googledrive" => "https://drive.google.com/",
        "dropbox" => "https://www.dropbox.com/home",
        _ => "",
    }
}

/// 전체 탐지 — 실존하는 폴더만(제거 잔재 방어). 순서 = OneDrive → Google Drive → Dropbox.
pub fn detect() -> Vec<CloudCandidate> {
    // 타입 명시 — 비Windows에서는 아래 cfg 블록이 통째로 사라져 추론 근거가 없다
    // (08-02 CI: E0282. Windows에서는 detect_* 인자로 추론돼 드러나지 않았다).
    let mut out: Vec<CloudCandidate> = Vec::new();
    #[cfg(windows)]
    {
        unsafe { detect_onedrive(&mut out) };
        unsafe { detect_googledrive(&mut out) };
        detect_dropbox(&mut out);
    }
    out.retain(|c| c.path.is_dir());
    out
}

/// OneDrive 개인/비즈니스 — 레지스트리 `HKCU\Software\Microsoft\OneDrive\Accounts\*`
/// (`UserFolder` + 표기용 `DisplayName`/`UserEmail`). BusinessN 키 열거 = 다계정 자연
/// 지원. 계정 키가 없으면 env(`OneDrive`) 폴백.
#[cfg(windows)]
unsafe fn detect_onedrive(out: &mut Vec<CloudCandidate>) {
    use windows::core::w;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
    };
    let mut hkey = HKEY::default();
    let opened = RegOpenKeyExW(
        HKEY_CURRENT_USER,
        w!("Software\\Microsoft\\OneDrive\\Accounts"),
        Some(0),
        KEY_READ,
        &mut hkey,
    )
    .is_ok();
    let before = out.len();
    if opened {
        for i in 0.. {
            let mut name = [0u16; 128];
            let mut len = name.len() as u32;
            if RegEnumKeyExW(
                hkey,
                i,
                Some(windows::core::PWSTR(name.as_mut_ptr())),
                &mut len,
                None,
                None,
                None,
                None,
            )
            .is_err()
            {
                break;
            }
            let sub = String::from_utf16_lossy(&name[..len as usize]);
            let base = format!("Software\\Microsoft\\OneDrive\\Accounts\\{sub}");
            let Some(folder) = reg_str(&base, "UserFolder") else {
                continue;
            };
            // 라벨 = 비즈니스 조직명 > 계정 이메일 > 키 이름(Personal 등)
            let who = reg_str(&base, "DisplayName")
                .or_else(|| reg_str(&base, "UserEmail"))
                .unwrap_or_else(|| sub.clone());
            out.push(CloudCandidate {
                kind: "onedrive",
                label: format!("OneDrive – {who}"),
                path: PathBuf::from(folder),
            });
        }
        let _ = RegCloseKey(hkey);
    }
    if out.len() == before {
        // 계정 키 부재(구버전 등) — 환경 변수 폴백
        if let Some(p) = std::env::var_os("OneDrive") {
            out.push(CloudCandidate {
                kind: "onedrive",
                label: "OneDrive".into(),
                path: PathBuf::from(p),
            });
        }
    }
    // 중복 경로 제거(env 폴백·키 중복 방어)
    let mut seen: Vec<PathBuf> = Vec::new();
    out.retain(|c| {
        if seen.iter().any(|p| p == &c.path) {
            false
        } else {
            seen.push(c.path.clone());
            true
        }
    });
}

/// HKCU 문자열 값 읽기(REG_SZ) — 실패 = None.
#[cfg(windows)]
unsafe fn reg_str(subkey: &str, value: &str) -> Option<String> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
    let sub = HSTRING::from(subkey);
    let val = HSTRING::from(value);
    let mut buf = [0u16; 512];
    let mut size = (buf.len() * 2) as u32;
    RegGetValueW(
        HKEY_CURRENT_USER,
        PCWSTR(sub.as_ptr()),
        PCWSTR(val.as_ptr()),
        RRF_RT_REG_SZ,
        None,
        Some(buf.as_mut_ptr() as *mut core::ffi::c_void),
        Some(&mut size),
    )
    .is_ok()
    .then(|| {
        let n = (size as usize / 2).saturating_sub(1); // 종단 NUL 제외
        String::from_utf16_lossy(&buf[..n.min(buf.len())])
    })
    .filter(|s| !s.is_empty())
}

/// Google Drive(DriveFS) — 마운트 드라이브의 **볼륨 라벨 "Google Drive"** 프로브
/// (레지스트리 마운트 키는 버전별로 형태가 달라 라벨 스캔이 견고 — 검토서 26 §2-1).
#[cfg(windows)]
unsafe fn detect_googledrive(out: &mut Vec<CloudCandidate>) {
    use windows::core::HSTRING;
    use windows::Win32::Storage::FileSystem::GetVolumeInformationW;
    for c in b'A'..=b'Z' {
        let root = format!("{}:\\", c as char);
        if !Path::new(&root).is_dir() {
            continue;
        }
        let wide = HSTRING::from(root.as_str());
        let mut label = [0u16; 64];
        if GetVolumeInformationW(
            windows::core::PCWSTR(wide.as_ptr()),
            Some(&mut label),
            None,
            None,
            None,
            None,
        )
        .is_err()
        {
            continue;
        }
        let n = label.iter().position(|&u| u == 0).unwrap_or(0);
        if String::from_utf16_lossy(&label[..n]) == "Google Drive" {
            out.push(CloudCandidate {
                kind: "googledrive",
                label: format!("Google Drive ({}:)", c as char),
                path: PathBuf::from(root),
            });
        }
    }
}

/// Dropbox — `%APPDATA%\Dropbox\info.json`(폴백 `%LOCALAPPDATA%`)의
/// `personal`/`business` 키(공식 문서화된 위치 — JSON 구조 자체가 다계정).
#[cfg(windows)]
fn detect_dropbox(out: &mut Vec<CloudCandidate>) {
    let files = ["APPDATA", "LOCALAPPDATA"].iter().filter_map(|env| {
        std::env::var_os(env).map(|d| PathBuf::from(d).join("Dropbox").join("info.json"))
    });
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else {
            continue;
        };
        for (key, tag) in [("personal", "Personal"), ("business", "Business")] {
            if let Some(path) = dropbox_path(&text, key) {
                let path = PathBuf::from(path);
                if !out.iter().any(|c: &CloudCandidate| c.path == path) {
                    out.push(CloudCandidate {
                        kind: "dropbox",
                        label: format!("Dropbox – {tag}"),
                        path,
                    });
                }
            }
        }
        break; // 첫 파일만(APPDATA 우선)
    }
}

/// info.json에서 `"<account>": { … "path": "…" }`의 path 값 추출 — **최소 수제 파서**
/// (crate 0 — DR-8). 계정 키 뒤 첫 `"path"` 문자열을 escape 해제(`\\`·`\/`·`\"`)해
/// 돌려준다. 형식 밖 입력은 None(관용).
fn dropbox_path(json: &str, account: &str) -> Option<String> {
    let akey = format!("\"{account}\"");
    let rest = &json[json.find(&akey)? + akey.len()..];
    let rest = &rest[rest.find("\"path\"")? + "\"path\"".len()..];
    let rest = &rest[rest.find(':')? + 1..];
    let start = rest.find('"')? + 1;
    let mut outs = String::new();
    let mut esc = false;
    for ch in rest[start..].chars() {
        if esc {
            outs.push(match ch {
                '\\' => '\\',
                '/' => '/',
                '"' => '"',
                other => other, // \uXXXX 등은 경로에 비출현 — 관용 통과
            });
            esc = false;
        } else if ch == '\\' {
            esc = true;
        } else if ch == '"' {
            return (!outs.is_empty()).then_some(outs);
        } else {
            outs.push(ch);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropbox_info_json_paths() {
        let json = r#"{
  "personal": {"path": "C:\\Users\\me\\Dropbox", "host": 123},
  "business": {"path": "D:\\Work\\Dropbox (Acme)", "host": 456}
}"#;
        assert_eq!(
            dropbox_path(json, "personal").as_deref(),
            Some("C:\\Users\\me\\Dropbox")
        );
        assert_eq!(
            dropbox_path(json, "business").as_deref(),
            Some("D:\\Work\\Dropbox (Acme)")
        );
        assert_eq!(dropbox_path(json, "team"), None);
        assert_eq!(dropbox_path("{}", "personal"), None);
    }

    #[test]
    fn web_urls_per_kind() {
        assert!(web_url("onedrive").contains("onedrive"));
        assert!(web_url("googledrive").contains("google"));
        assert!(web_url("dropbox").contains("dropbox"));
        assert_eq!(web_url("unknown"), "");
    }
}
