//! 경로 입력 해석·자동완성(편의 UX ① — 원본 `PathInterpreter.cs`·`PathSuggestions.cs`
//! [PATH-SUG] 이식). **순수 로직**(IO 주입·regex 등 외부 crate 0) — 전 플랫폼 테스트.
//!
//! - [`expand_env`]: CMD `%VAR%` · PowerShell `$env:VAR`/`${env:VAR}` 확장 + 감싸는
//!   따옴표/공백 제거. **미정의 변수는 원문 유지**(경로 검증에서 자연 실패 — 원본 규약).
//! - [`suggest_folders`]: 입력을 "베이스 폴더 + 마지막 세그먼트 접두사"로 분해해
//!   일치하는 하위 폴더 전체 경로 목록(탐색기식 — 구분자 입력=전체 목록, 타이핑=필터).

/// 환경변수 확장 — 원본 PathInterpreter.Expand 대응.
pub fn expand_env(input: &str) -> String {
    let mut s = input.trim().to_string();
    // 붙여넣기 시 흔한 감싸는 따옴표 제거("..." 또는 '...')
    let b = s.as_bytes();
    if s.len() >= 2
        && ((b[0] == b'"' && b[s.len() - 1] == b'"') || (b[0] == b'\'' && b[s.len() - 1] == b'\''))
    {
        s = s[1..s.len() - 1].to_string();
    }
    // PowerShell ${env:NAME}(중괄호 안 특수문자 허용)을 먼저 — 더 구체적
    s = replace_between(&s, "${env:", "}", |name| std::env::var(name).ok());
    // PowerShell $env:NAME — 이름은 단어문자만(PowerShell 파싱 동일)
    s = replace_ps_bare(&s);
    // CMD %NAME%
    s = replace_between(&s, "%", "%", |name| {
        if name.is_empty() {
            None // "%%"는 원문 유지
        } else {
            std::env::var(name).ok()
        }
    });
    s.trim().to_string()
}

/// `hay[from..]`에서 ASCII 대소문자 무시로 `needle`을 찾아 **`hay` 기준 바이트 오프셋**을
/// 반환. `to_lowercase()`는 길이를 보존하지 않아('İ' 2B→3B · 'K' 3B→1B) 그 오프셋을
/// 원문에 쓰면 문자 경계 panic(G13-01) — 토큰(`%`·`$env:`·`${env:`)은 전부 ASCII이므로
/// 바이트 단위 길이 보존 비교로 충분하다. `needle`은 ASCII·비어 있지 않아야 한다.
/// 반환 오프셋은 `needle`이 ASCII라 항상 문자 경계.
fn find_ascii_ci(hay: &str, from: usize, needle: &str) -> Option<usize> {
    debug_assert!(needle.is_ascii() && !needle.is_empty());
    let n = needle.as_bytes();
    hay.as_bytes()
        .get(from..)?
        .windows(n.len())
        .position(|w| w.eq_ignore_ascii_case(n))
        .map(|rel| from + rel)
}

/// `open`…`close` 사이 이름을 `lookup`으로 치환(대소문자 무시 open 매칭·미정의=원문 유지).
/// 인덱스는 전부 `s` 하나에서만 계산한다([`find_ascii_ci`]).
fn replace_between(
    s: &str,
    open: &str,
    close: &str,
    lookup: impl Fn(&str) -> Option<String>,
) -> String {
    let mut out = String::with_capacity(s.len());
    let mut i = 0usize;
    while i < s.len() {
        match find_ascii_ci(s, i, open) {
            Some(start) => {
                out.push_str(&s[i..start]);
                let name_start = start + open.len();
                match s[name_start..].find(close) {
                    Some(nrel) => {
                        let name = &s[name_start..name_start + nrel];
                        let end = name_start + nrel + close.len();
                        match lookup(name) {
                            Some(v) => out.push_str(&v),
                            None => out.push_str(&s[start..end]), // 미정의 — 원문 유지
                        }
                        i = end;
                    }
                    None => {
                        out.push_str(&s[start..]);
                        i = s.len();
                    }
                }
            }
            None => {
                out.push_str(&s[i..]);
                i = s.len();
            }
        }
    }
    out
}

/// PowerShell `$env:NAME`(중괄호 없음 — 이름은 `[A-Za-z0-9_]+`) 치환.
fn replace_ps_bare(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut i = 0usize;
    while i < s.len() {
        match find_ascii_ci(s, i, "$env:") {
            Some(start) => {
                out.push_str(&s[i..start]);
                let name_start = start + "$env:".len();
                let name_len = s[name_start..]
                    .bytes()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == b'_')
                    .count();
                if name_len == 0 {
                    out.push_str(&s[start..name_start]);
                    i = name_start;
                    continue;
                }
                let name = &s[name_start..name_start + name_len];
                match std::env::var(name) {
                    Ok(v) => out.push_str(&v),
                    Err(_) => out.push_str(&s[start..name_start + name_len]),
                }
                i = name_start + name_len;
            }
            None => {
                out.push_str(&s[i..]);
                i = s.len();
            }
        }
    }
    out
}

/// 폴더 제안(원본 PathSuggestions.SuggestFolders 대응) — `text`를 마지막 구분자에서
/// 분해해 베이스 폴더의 하위 폴더 중 접두사 일치(대소문자 무시)를 최대 `max`개.
/// `enum_dirs(base)` = 하위 폴더 **전체 경로** 열거(실패=빈 목록 — 팝업 닫힘).
pub fn suggest_folders(
    text: &str,
    enum_dirs: impl Fn(&str) -> Vec<String>,
    max: usize,
) -> Vec<String> {
    let mut out = Vec::new();
    if text.trim().is_empty() || max == 0 {
        return out;
    }
    let Some(last_sep) = text.rfind(['\\', '/']) else {
        return out; // "C:" 등 구분자 이전(드라이브 제안)은 후속
    };
    let base = &text[..last_sep + 1]; // 구분자 포함("C:\Users\")
    let prefix = text[last_sep + 1..].to_lowercase();
    for dir in enum_dirs(base) {
        if last_name(&dir).to_lowercase().starts_with(&prefix) {
            out.push(dir);
            if out.len() >= max {
                break;
            }
        }
    }
    out
}

/// 실제 파일시스템 열거자 — 베이스 폴더의 하위 **폴더** 전체 경로(오류=빈 목록).
pub fn fs_dirs(base: &str) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(base) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in rd.flatten() {
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            out.push(e.path().to_string_lossy().into_owned());
        }
    }
    out
}

/// 경로의 마지막 세그먼트(끝 구분자 무시·`\`/`/` 공통).
fn last_name(path: &str) -> &str {
    let t = path.trim_end_matches(['\\', '/']);
    match t.rfind(['\\', '/']) {
        Some(i) => &t[i + 1..],
        None => t,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_cmd_ps_quotes_and_undefined() {
        std::env::set_var("NEXA_T1", "C:\\Base");
        assert_eq!(expand_env("%NEXA_T1%\\sub"), "C:\\Base\\sub");
        assert_eq!(expand_env("$env:NEXA_T1/x"), "C:\\Base/x");
        assert_eq!(expand_env("${env:NEXA_T1}\\y"), "C:\\Base\\y");
        assert_eq!(expand_env("\"C:\\a b\""), "C:\\a b", "감싸는 따옴표 제거");
        assert_eq!(
            expand_env("%NEXA_NOPE%\\z"),
            "%NEXA_NOPE%\\z",
            "미정의 원문 유지"
        );
        assert_eq!(expand_env("  C:\\t  "), "C:\\t");
    }

    /// G13-01: `to_lowercase()`가 길이를 바꾸는 문자('İ' 2B→3B · 'K'(U+212A) 3B→1B ·
    /// 'ǅ') 뒤의 토큰에서 바이트 오프셋이 어긋나 문자 경계 panic·엉뚱한 구간 치환이 났다.
    #[test]
    fn expand_env_non_ascii_case_change_no_panic() {
        std::env::set_var("NEXA_T2", "V");
        // 미정의 변수 — panic 없이 원문 유지
        for s in [
            "İ%한%",
            "K%NEXA_NOPE%",
            "KK%NEXA_NOPE%",
            "ǅ%X%",
            "İ$env:NEXA_NOPE",
        ] {
            assert_eq!(expand_env(s), s, "{s}");
        }
        // 정의된 변수 — 앞의 비ASCII 문자는 보존되고 토큰만 치환
        assert_eq!(expand_env("C:\\İstanbul\\%NEXA_T2%"), "C:\\İstanbul\\V");
        assert_eq!(expand_env("K%NEXA_T2%"), "KV");
        assert_eq!(expand_env("KK%NEXA_T2%\\x"), "KKV\\x");
        assert_eq!(expand_env("İ$env:NEXA_T2/x"), "İV/x");
        assert_eq!(expand_env("İ${env:NEXA_T2}"), "İV");
        // 토큰 대소문자 무시(ASCII)는 그대로
        assert_eq!(expand_env("$ENV:NEXA_T2"), "V");
        assert_eq!(expand_env("${ENV:NEXA_T2}"), "V");
    }

    #[test]
    fn find_ascii_ci_offsets_are_in_haystack() {
        assert_eq!(find_ascii_ci("abc$ENV:x", 0, "$env:"), Some(3));
        assert_eq!(find_ascii_ci("İ%한%", 0, "%"), Some(2));
        assert_eq!(find_ascii_ci("İ%한%", 3, "%"), Some(6));
        assert_eq!(find_ascii_ci("abc", 0, "%"), None);
        assert_eq!(find_ascii_ci("abc", 10, "a"), None, "범위 밖 from");
    }

    fn dirs(base: &str) -> Vec<String> {
        if base == "C:\\U\\" {
            vec![
                "C:\\U\\Alpha".to_string(),
                "C:\\U\\alBum".to_string(),
                "C:\\U\\Beta".to_string(),
            ]
        } else {
            vec![]
        }
    }

    #[test]
    fn suggest_splits_base_and_prefix_ci() {
        // 구분자 직후 = 전체 목록
        assert_eq!(suggest_folders("C:\\U\\", dirs, 20).len(), 3);
        // 접두사 필터(대소문자 무시)
        assert_eq!(
            suggest_folders("C:\\U\\al", dirs, 20),
            vec!["C:\\U\\Alpha".to_string(), "C:\\U\\alBum".to_string()]
        );
        // 상한
        assert_eq!(suggest_folders("C:\\U\\", dirs, 1).len(), 1);
        // 구분자 없음·빈 입력·실패 베이스 = 빈 목록
        assert!(suggest_folders("C:", dirs, 20).is_empty());
        assert!(suggest_folders("  ", dirs, 20).is_empty());
        assert!(suggest_folders("D:\\none\\x", dirs, 20).is_empty());
    }
}
