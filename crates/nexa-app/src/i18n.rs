//! i18n(M2-6) — 원본 docs/42 이식: 외부 `.lang`(properties 스타일 §3)·키 단위 병합·en 폴백.
//! 원본과의 차이(결정 기록: docs/journal): ① JSON 포맷 심 비이관 — crate 0(DR-8) 유지,
//! 원본도 properties를 권장안으로 명시. ② 오버라이드 폴더 = %APPDATA% 대신 **exe 옆
//! `data\lang\`**(DR-3 포터블). ③ **동적 전환** — 커스텀 드로잉은 매 프레임 문자열을 다시
//! 그리므로 테이블 스왑+메뉴/컬럼 재구성+재그리기로 재시작 없이 반영(원본 PREF-9 확인창 불요).
//!
//! 활성 테이블은 thread_local(UI 단일 스레드) — 셀·상태바는 페인트 시점 [`tr`] 조회.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;

/// 내장 언어(빌드 산출물에 임베드 — `lang/` 폴더 없이도 붕괴하지 않는 안전망, 원본 §2).
const BUILTIN_EN: &str = include_str!("../lang/en.lang");
const BUILTIN_KO: &str = include_str!("../lang/ko.lang");
/// 일본어(사용자 요청 07-21) — en/ko와 동일한 전 키 번역·파리티 테스트 대상.
const BUILTIN_JA: &str = include_str!("../lang/ja.lang");

fn builtin(code: &str) -> Option<&'static str> {
    match code {
        "en" => Some(BUILTIN_EN),
        "ko" => Some(BUILTIN_KO),
        "ja" => Some(BUILTIN_JA),
        _ => None,
    }
}

/// `.lang` 파일 1개의 파싱 결과 — `@` 메타 + 문자열 테이블.
#[derive(Default, Debug)]
pub struct LangFile {
    pub meta: HashMap<String, String>,
    pub strings: HashMap<String, String>,
}

/// 값 이스케이프 해석: `\n`·`\t`·`\\`(원본 §3). 그 외 시퀀스는 리터럴 유지.
fn unescape(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// 원본 §3 파싱 규칙: BOM 스킵·`#` 주석·빈 줄 무시·첫 `=` 분리·키/값 trim·중복은 마지막 승리.
/// 깨진 줄(`=` 없음)은 스킵(라인 격리 — §5). `@` 메타 값의 후행 `# 주석`은 제거(§3 예시 규약).
pub fn parse(text: &str) -> LangFile {
    let mut f = LangFile::default();
    for line in text.strip_prefix('\u{feff}').unwrap_or(text).lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue; // 파손 줄 스킵
        };
        let (k, v) = (k.trim(), v.trim());
        if let Some(mk) = k.strip_prefix('@') {
            let v = v.split('#').next().unwrap_or("").trim(); // 메타 후행 주석 허용
            f.meta.insert(mk.trim().to_string(), v.to_string());
        } else {
            f.strings.insert(k.to_string(), unescape(v));
        }
    }
    f
}

/// 활성 언어 — 현재 테이블 + en 폴백 테이블(원본 Localizer 폴백 체인: 현재 → en → 키).
pub struct Lang {
    table: HashMap<String, String>,
    fallback: HashMap<String, String>,
}

impl Lang {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.table
            .get(key)
            .or_else(|| self.fallback.get(key))
            .map(String::as_str)
    }
}

/// 코드의 테이블 = 내장(있으면) 위에 사용자 `data\lang\{code}.lang`을 **키 단위** 덮어쓰기(§2).
fn merged_table(code: &str, data_dir: &Path) -> HashMap<String, String> {
    let mut t = builtin(code).map(|s| parse(s).strings).unwrap_or_default();
    let user = data_dir.join("lang").join(format!("{code}.lang"));
    if let Ok(text) = std::fs::read_to_string(&user) {
        for (k, v) in parse(&text).strings {
            t.insert(k, v);
        }
    }
    t
}

/// 언어 로드 — 폴백은 en(기준 언어, 원본 §7). en 자신은 폴백 없음.
pub fn load(code: &str, data_dir: &Path) -> Lang {
    Lang {
        table: merged_table(code, data_dir),
        fallback: if code == "en" {
            HashMap::new()
        } else {
            merged_table("en", data_dir)
        },
    }
}

/// 사용 가능한 언어 발견: 내장 + `data\lang\*.lang`(파일명 = 코드, `@name` 표기 — 원본 §4-1).
/// 반환 (code, 자기 언어 표기). 내장이 앞, 추가 발견분은 코드순.
pub fn discover(data_dir: &Path) -> Vec<(String, String)> {
    let mut out = vec![
        ("en".to_string(), "English".to_string()),
        ("ko".to_string(), "한국어".to_string()),
        ("ja".to_string(), "日本語".to_string()),
    ];
    let mut extra: Vec<(String, String)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(data_dir.join("lang")) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_none_or(|x| x != "lang") {
                continue;
            }
            let Some(code) = p.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if builtin(code).is_some() {
                continue; // 내장 오버라이드는 목록 중복 없이 병합만
            }
            let name = std::fs::read_to_string(&p)
                .ok()
                .and_then(|t| parse(&t).meta.get("name").cloned())
                .unwrap_or_else(|| code.to_string());
            extra.push((code.to_string(), name));
        }
    }
    extra.sort();
    out.extend(extra);
    out
}

/// 설정값 → 실제 코드. "system" = OS 언어의 1차 서브태그(ko-KR → ko), 미보유 시 en(기준).
pub fn resolve_code(setting: &str, system: &str, available: &[(String, String)]) -> String {
    let want = if setting == "system" {
        system.split(['-', '_']).next().unwrap_or("en")
    } else {
        setting
    };
    if available.iter().any(|(c, _)| c == want) {
        want.to_string()
    } else {
        "en".to_string()
    }
}

thread_local! {
    /// 활성 테이블(UI 스레드 전용). 기본 = 내장 en — 테스트·기동 초기에도 빈 키 붕괴 없음.
    static ACTIVE: RefCell<Lang> = RefCell::new(Lang {
        table: parse(BUILTIN_EN).strings,
        fallback: HashMap::new(),
    });
}

/// 언어 전환 — 테이블 스왑. 호출 측이 메뉴/컬럼 재구성 + 전체 재그리기를 수행한다.
pub fn activate(lang: Lang) {
    ACTIVE.with(|a| *a.borrow_mut() = lang);
}

/// 키 → 문자열(현재 → en 폴백 → 키 그대로 — 원본 폴백 체인).
pub fn tr(key: &str) -> String {
    ACTIVE.with(|a| {
        a.borrow()
            .get(key)
            .map(str::to_string)
            .unwrap_or_else(|| key.to_string())
    })
}

/// `{0}`·`{1}`… 자리표 치환(원본 status.* 패턴 규약).
pub fn trf(key: &str, args: &[&str]) -> String {
    let mut s = tr(key);
    for (i, a) in args.iter().enumerate() {
        s = s.replace(&format!("{{{i}}}"), a);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rules_meta_comment_escape_dup() {
        let f = parse(
            "\u{feff}# 주석\n@code = ko   # 후행 주석\n@name= 한국어\n\na.b = 값1\nbroken line\na.b = 값2\nesc = 줄\\n바꿈\\t탭\\\\역슬래시\nlit = 그대로\\x\n",
        );
        assert_eq!(f.meta["code"], "ko");
        assert_eq!(f.meta["name"], "한국어");
        assert_eq!(f.strings["a.b"], "값2", "중복 키 = 마지막 승리");
        assert_eq!(f.strings["esc"], "줄\n바꿈\t탭\\역슬래시");
        assert_eq!(f.strings["lit"], "그대로\\x", "미지 이스케이프는 리터럴");
        assert_eq!(f.strings.len(), 4 - 1, "파손 줄 스킵"); // a.b·esc·lit
    }

    #[test]
    fn builtin_langs_parse_and_key_parity() {
        let en = parse(BUILTIN_EN);
        assert_eq!(en.meta["code"], "en");
        assert!(!en.strings.is_empty());
        // 번역 파리티 — 신규 키 누락 방지(en = 기준 언어). 내장 전 언어 공통.
        for (code, text) in [("ko", BUILTIN_KO), ("ja", BUILTIN_JA)] {
            let l = parse(text);
            assert_eq!(l.meta["code"], code);
            for k in en.strings.keys() {
                assert!(l.strings.contains_key(k), "{code}.lang에 키 누락: {k}");
            }
            for k in l.strings.keys() {
                assert!(en.strings.contains_key(k), "en.lang에 키 누락({code}): {k}");
            }
        }
    }

    #[test]
    fn merge_override_fallback_and_resolve() {
        let dir = std::env::temp_dir().join(format!("nexa_lang_{}", std::process::id()));
        let sub = dir.join("lang");
        std::fs::create_dir_all(&sub).unwrap();
        // 사용자 오버라이드: ko의 일부 키만 교체 + 신규 언어 fr(ja는 내장 승격 — 07-21)
        std::fs::write(sub.join("ko.lang"), "menu.file = 화일\n").unwrap();
        std::fs::write(
            sub.join("fr.lang"),
            "@code = fr\n@name = Français\nmenu.file = Fichier\n",
        )
        .unwrap();

        let ko = load("ko", &dir);
        assert_eq!(
            ko.get("menu.file"),
            Some("화일"),
            "사용자 키 단위 오버라이드"
        );
        assert_eq!(ko.get("menu.view"), Some("보기"), "나머지는 내장 유지");
        let ja = load("ja", &dir);
        assert_eq!(ja.get("menu.file"), Some("ファイル"), "내장 일본어(07-21)");
        let fr = load("fr", &dir);
        assert_eq!(fr.get("menu.file"), Some("Fichier"));
        assert_eq!(fr.get("menu.view"), Some("View"), "누락 키 = en 폴백");
        assert_eq!(fr.get("no.such.key"), None);

        let avail = discover(&dir);
        assert_eq!(avail[0].0, "en");
        assert!(avail.iter().any(|(c, n)| c == "ja" && n == "日本語"));
        assert!(avail.iter().any(|(c, n)| c == "fr" && n == "Français"));
        assert_eq!(
            avail.iter().filter(|(c, _)| c == "ko").count(),
            1,
            "오버라이드는 중복 미등재"
        );

        assert_eq!(resolve_code("system", "ko-KR", &avail), "ko");
        assert_eq!(
            resolve_code("system", "ja-JP", &avail),
            "ja",
            "OS 일본어 추종"
        );
        assert_eq!(resolve_code("system", "de-DE", &avail), "en", "미보유 = en");
        assert_eq!(resolve_code("fr", "ko-KR", &avail), "fr");
        assert_eq!(resolve_code("zz", "ko-KR", &avail), "en");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn trf_placeholders() {
        assert_eq!(trf("status.tab", &["2", "3"]), "Tab 2/3");
        assert_eq!(trf("kind.extFile", &["TXT"]), "TXT file");
        assert_eq!(tr("no.such.key"), "no.such.key", "최후 폴백 = 키");
    }
}
