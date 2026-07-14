//! 일괄 이름변경(M5-1 — 원본 docs/25 α 단계 이식·**원본도 설계만 존재해 최초 구현**):
//! 동작 파이프라인(치환→대소문자→삽입→연번, α 고정 순서 — 블록 재배열은 β) + 미리보기 +
//! 충돌 검출 4종(빈 이름·금지 문자·배치 내 중복·기존 파일 존재).
//!
//! 플랫폼 중립 순수 로직(파일시스템 접근은 `exists` 콜백 주입) — 맥 `cargo test` 대상.
//! 적용·Undo(배치 전체 = 트랜잭션 1건, docs/25 §7 B-13u)는 앱 계층이 [`history::MoveBatchOp`]
//! 로 수행한다. 동작은 **이름부(stem)에만** 적용, 확장자는 보존(폴더는 전체가 이름부).

/// 대소문자 변경(docs/25 §2 동작 4 — UPPER/lower/Title/Sentence).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CaseMode {
    Upper,
    Lower,
    /// 단어 첫 글자 대문자(공백·`-`·`_` 뒤).
    Title,
    /// 첫 글자만 대문자, 나머지 소문자.
    Sentence,
}

/// 연번 삽입(docs/25 §2 동작 5) — 시작값·증가폭·0패딩 자릿수·위치.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NumberSpec {
    pub start: i64,
    pub step: i64,
    /// 0패딩 자릿수(1 = 패딩 없음).
    pub pad: usize,
    /// true = 이름 뒤(suffix), false = 앞(prefix).
    pub suffix: bool,
}

/// 동작 묶음(α — 활성 동작만 Some). 적용 순서 고정: 치환 → 대소문자 → 삽입 → 연번.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct BatchSpec {
    /// (찾기, 바꾸기, 대소문자 일치) — 전체 일치 치환(docs/25 §2 동작 1).
    pub replace: Option<(String, String, bool)>,
    pub case: Option<CaseMode>,
    /// (텍스트, 뒤에 삽입) — 접두/접미(docs/25 §2 동작 3 α).
    pub insert: Option<(String, bool)>,
    pub number: Option<NumberSpec>,
}

impl BatchSpec {
    pub fn is_empty(&self) -> bool {
        self.replace.is_none()
            && self.case.is_none()
            && self.insert.is_none()
            && self.number.is_none()
    }
}

/// 충돌 종류(docs/25 §7 — 미리보기 하이라이트·적용 차단).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Conflict {
    None,
    /// 결과가 빈 이름(전체 빈 이름 방지 규약).
    Empty,
    /// Windows 금지 문자 포함.
    Invalid,
    /// 배치 안에서 같은 부모의 결과 이름 중복.
    Duplicate,
    /// 대상 경로에 파일이 이미 존재(자기 자신 제외).
    Exists,
}

/// 대소문자 무시 치환(문자 단위 — UTF-8 길이 변화 안전). `match_case`면 정확 일치.
fn replace_all(s: &str, find: &str, with: &str, match_case: bool) -> String {
    if find.is_empty() {
        return s.to_string();
    }
    if match_case {
        return s.replace(find, with);
    }
    let sc: Vec<char> = s.chars().collect();
    let fc: Vec<char> = find.chars().collect();
    let eq = |a: char, b: char| a.to_lowercase().eq(b.to_lowercase());
    let mut out = String::new();
    let mut i = 0;
    while i < sc.len() {
        if i + fc.len() <= sc.len() && sc[i..i + fc.len()].iter().zip(&fc).all(|(a, b)| eq(*a, *b))
        {
            out.push_str(with);
            i += fc.len();
        } else {
            out.push(sc[i]);
            i += 1;
        }
    }
    out
}

fn apply_case(s: &str, mode: CaseMode) -> String {
    match mode {
        CaseMode::Upper => s.to_uppercase(),
        CaseMode::Lower => s.to_lowercase(),
        CaseMode::Sentence => {
            let mut ch = s.chars();
            match ch.next() {
                Some(f) => f
                    .to_uppercase()
                    .chain(ch.flat_map(char::to_lowercase))
                    .collect(),
                None => String::new(),
            }
        }
        CaseMode::Title => {
            let mut out = String::new();
            let mut at_word = true; // 시작·구분자 뒤 = 단어 첫 글자
            for c in s.chars() {
                if at_word && c.is_alphabetic() {
                    out.extend(c.to_uppercase());
                    at_word = false;
                } else {
                    out.extend(c.to_lowercase());
                    if matches!(c, ' ' | '-' | '_' | '.') {
                        at_word = true;
                    }
                }
            }
            out
        }
    }
}

/// 이름부/확장자 분리 — 파일만 분리(숨김 파일 `.x`는 전체가 이름부, 폴더는 항상 전체).
fn split_stem(name: &str, is_dir: bool) -> (&str, &str) {
    if is_dir {
        return (name, "");
    }
    match name.rfind('.') {
        Some(i) if i > 0 => name.split_at(i),
        _ => (name, ""),
    }
}

/// 미리보기 — `items` = (현재 이름, 폴더 여부), 반환 = 새 이름(순서 보존).
/// 연번은 목록 순서(호출자가 정렬 책임 — α: 선택 순서).
pub fn preview(items: &[(String, bool)], spec: &BatchSpec) -> Vec<String> {
    items
        .iter()
        .enumerate()
        .map(|(idx, (name, is_dir))| {
            let (stem, ext) = split_stem(name, *is_dir);
            let mut s = stem.to_string();
            if let Some((find, with, mc)) = &spec.replace {
                s = replace_all(&s, find, with, *mc);
            }
            if let Some(mode) = spec.case {
                s = apply_case(&s, mode);
            }
            if let Some((text, suffix)) = &spec.insert {
                if *suffix {
                    s.push_str(text);
                } else {
                    s = format!("{text}{s}");
                }
            }
            if let Some(n) = &spec.number {
                let val = n.start + n.step * idx as i64;
                let num = if val < 0 {
                    format!("-{:0width$}", -val, width = n.pad.max(1))
                } else {
                    format!("{:0width$}", val, width = n.pad.max(1))
                };
                if n.suffix {
                    s.push_str(&num);
                } else {
                    s = format!("{num}{s}");
                }
            }
            format!("{}{}", s.trim(), ext)
        })
        .collect()
}

const INVALID_CHARS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// 충돌 검출 — `items` = (부모 경로 문자열, 현재 이름, 새 이름). `exists(부모, 새 이름)` =
/// 파일시스템 존재 확인(앱 주입 — 테스트는 가짜). 비변경 항목은 충돌 아님(적용에서 제외).
pub fn conflicts(
    items: &[(String, String, String)],
    exists: &dyn Fn(&str, &str) -> bool,
) -> Vec<Conflict> {
    let lower = |s: &str| s.to_lowercase();
    items
        .iter()
        .enumerate()
        .map(|(i, (parent, old, new))| {
            if new == old {
                return Conflict::None; // 무변경 — 적용 제외 대상
            }
            if new.trim().is_empty() {
                return Conflict::Empty;
            }
            if new.contains(INVALID_CHARS) || new.ends_with('.') || new.ends_with(' ') {
                return Conflict::Invalid;
            }
            // 배치 내 중복(같은 부모·대소문자 무시) — 자신 제외
            let dup = items.iter().enumerate().any(|(j, (p, _, n))| {
                j != i && lower(p) == lower(parent) && lower(n) == lower(new)
            });
            if dup {
                return Conflict::Duplicate;
            }
            // 기존 파일 존재 — 대소문자만 변경(자기 자신)은 허용(원본 CommitRename 규약).
            // 배치 내 다른 항목이 비켜줄 자리도 α에선 보수적으로 충돌 처리(순서 의존 회피).
            if lower(new) != lower(old) && exists(parent, new) {
                return Conflict::Exists;
            }
            Conflict::None
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(names: &[&str]) -> Vec<(String, bool)> {
        names.iter().map(|n| (n.to_string(), false)).collect()
    }

    #[test]
    fn preview_pipeline_order_and_ext_preserved() {
        // 치환 → 대소문자 → 삽입 → 연번(고정 순서), 확장자 보존
        let spec = BatchSpec {
            replace: Some(("img".into(), "photo".into(), false)),
            case: Some(CaseMode::Upper),
            insert: Some(("2026-".into(), false)),
            number: Some(NumberSpec {
                start: 1,
                step: 1,
                pad: 3,
                suffix: true,
            }),
        };
        let out = preview(&files(&["IMG_a.jpg", "img_b.jpg"]), &spec);
        assert_eq!(out, vec!["2026-PHOTO_A001.jpg", "2026-PHOTO_B002.jpg"]);
    }

    #[test]
    fn preview_case_modes_and_dir_whole_name() {
        let spec = BatchSpec {
            case: Some(CaseMode::Title),
            ..Default::default()
        };
        assert_eq!(
            preview(&[("my file-name.txt".into(), false)], &spec),
            vec!["My File-Name.txt"]
        );
        // 폴더는 전체가 이름부 — 확장자 분리 없음
        assert_eq!(
            preview(&[("archive.old".into(), true)], &spec),
            vec!["Archive.Old"]
        );
        let sent = BatchSpec {
            case: Some(CaseMode::Sentence),
            ..Default::default()
        };
        assert_eq!(
            preview(&[("hELLO wORLD.md".into(), false)], &sent),
            vec!["Hello world.md"]
        );
    }

    #[test]
    fn preview_number_padding_and_step() {
        let spec = BatchSpec {
            number: Some(NumberSpec {
                start: 8,
                step: 2,
                pad: 2,
                suffix: false,
            }),
            ..Default::default()
        };
        assert_eq!(
            preview(&files(&["a.txt", "b.txt", "c.txt"]), &spec),
            vec!["08a.txt", "10b.txt", "12c.txt"]
        );
    }

    #[test]
    fn conflict_detection_four_kinds() {
        let exists = |_: &str, n: &str| n == "taken.txt";
        // (부모, 현재, 새) — Empty·Invalid·Duplicate·Exists·None·무변경
        let items: Vec<(String, String, String)> = vec![
            ("C:\\d".into(), "a.txt".into(), ".txt".into()), // trim 후 stem 없음 → 그대로 비교
            ("C:\\d".into(), "b.txt".into(), "b?.txt".into()),
            ("C:\\d".into(), "c1.txt".into(), "same.txt".into()),
            ("C:\\d".into(), "c2.txt".into(), "SAME.txt".into()), // 대소문자 무시 중복
            ("C:\\d".into(), "e.txt".into(), "taken.txt".into()),
            ("C:\\d".into(), "f.txt".into(), "f.txt".into()), // 무변경
            ("C:\\d".into(), "g.txt".into(), "G.txt".into()), // 대소문자만 — 허용
        ];
        let out = conflicts(&items, &exists);
        assert_eq!(
            out,
            vec![
                Conflict::None, // ".txt"는 빈 이름 아님(숨김 파일 관례) — 존재 검사만
                Conflict::Invalid,
                Conflict::Duplicate,
                Conflict::Duplicate,
                Conflict::Exists,
                Conflict::None,
                Conflict::None,
            ]
        );
        // 빈 이름
        assert_eq!(
            conflicts(&[("p".into(), "x".into(), "  ".into())], &exists),
            vec![Conflict::Empty]
        );
    }
}
