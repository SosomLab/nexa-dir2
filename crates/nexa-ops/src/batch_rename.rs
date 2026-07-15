//! 일괄 이름변경(M5-1 — 원본 docs/25 이식·**원본도 설계만 존재해 최초 구현**):
//! **순서형 동작 파이프라인**(사용자가 블록을 순서대로 배치 — 위→아래, 각 단계 출력이
//! 다음 입력. docs/25 §3 블록 스택) + 미리보기 + 충돌 검출 4종 + **프리셋 직렬화**
//! (docs/25 §3 "Save Renaming Sequence" — 파일 I/O는 앱 계층).
//!
//! 플랫폼 중립 순수 로직(파일시스템 접근은 `exists` 콜백 주입) — 맥 `cargo test` 대상.
//! 적용·Undo(배치 전체 = 트랜잭션 1건, docs/25 §7 B-13u)는 앱 계층이
//! [`history::MoveBatchOp`]로 수행한다.
//!
//! 동작 대상: 기본 = **이름부(stem)**(확장자 보존·폴더는 전체가 이름부),
//! [`RenameOp::ChangeExt`]만 확장자 대상(폴더는 no-op).
//! 정규식 = `regex-lite`(이진 크기 최적화 — DR-8 원장 docs/10 §1-2).

use regex_lite::Regex;

/// 대소문자 변경(docs/25 §2 동작 4 — UPPER/lower/Title/Sentence).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CaseMode {
    Upper,
    Lower,
    /// 단어 첫 글자 대문자(공백·`-`·`_`·`.` 뒤).
    Title,
    /// 첫 글자만 대문자, 나머지 소문자.
    Sentence,
}

impl CaseMode {
    fn as_str(self) -> &'static str {
        match self {
            CaseMode::Upper => "upper",
            CaseMode::Lower => "lower",
            CaseMode::Title => "title",
            CaseMode::Sentence => "sentence",
        }
    }
    fn from_str(s: &str) -> Option<CaseMode> {
        Some(match s {
            "upper" => CaseMode::Upper,
            "lower" => CaseMode::Lower,
            "title" => CaseMode::Title,
            "sentence" => CaseMode::Sentence,
            _ => return None,
        })
    }
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

/// 파이프라인 동작 1블록 — 사용자가 순서대로 배치(위→아래 순차 적용).
#[derive(Clone, PartialEq, Debug)]
pub enum RenameOp {
    /// 텍스트/정규식 치환(docs/25 §2 동작 1·2). `regex`면 `with`에 `$1` 캡처 참조 가능,
    /// `match_case`=false는 `(?i)` 접두(정규식)/문자 단위 비교(일반).
    Replace {
        find: String,
        with: String,
        match_case: bool,
        regex: bool,
    },
    /// 대소문자 변경(동작 4).
    Case(CaseMode),
    /// 텍스트 삽입(동작 3 α — 접두/접미).
    Insert { text: String, suffix: bool },
    /// 연번(동작 5) — 항목 순서 기준.
    Number(NumberSpec),
    /// 구간 이동(사용자 요청 07-15 — "중간 N자리를 잘라 맨 앞/뒤로"): `start` = 1기준
    /// 문자 위치, `len` 문자 수. 범위 밖은 가능한 만큼만(없으면 no-op).
    Move {
        start: usize,
        len: usize,
        to_front: bool,
    },
    /// 확장자 변경(사용자 요청 07-15 — 예: cfg→config). `from` 빈 값 = 모든 확장자,
    /// 매치는 대소문자 무시. `to` 빈 값 = 확장자 제거. 폴더는 no-op.
    ChangeExt { from: String, to: String },
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
fn replace_plain(s: &str, find: &str, with: &str, match_case: bool) -> String {
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

/// 정규식 패턴 구성(`match_case`=false → `(?i)`).
fn regex_of(find: &str, match_case: bool) -> Result<Regex, String> {
    let pat = if match_case {
        find.to_string()
    } else {
        format!("(?i){find}")
    };
    Regex::new(&pat).map_err(|e| e.to_string())
}

/// 파이프라인 검증 — 정규식 블록의 패턴 오류를 (블록 순번, 메시지)로 반환.
pub fn validate(ops: &[RenameOp]) -> Result<(), (usize, String)> {
    for (i, op) in ops.iter().enumerate() {
        if let RenameOp::Replace {
            find,
            match_case,
            regex: true,
            ..
        } = op
        {
            if find.is_empty() {
                return Err((i, "empty pattern".into()));
            }
            regex_of(find, *match_case).map_err(|e| (i, e))?;
        }
    }
    Ok(())
}

/// 동작 1블록 적용 — `stem`/`ext`(선행 `.` 포함)를 제자리 갱신. `idx` = 연번용 순번.
fn apply(op: &RenameOp, stem: &mut String, ext: &mut String, idx: usize, is_dir: bool) {
    match op {
        RenameOp::Replace {
            find,
            with,
            match_case,
            regex,
        } => {
            if *regex {
                if let Ok(re) = regex_of(find, *match_case) {
                    *stem = re.replace_all(stem, with.as_str()).into_owned();
                }
                // 패턴 오류는 validate가 사전 차단 — 방어적으로 무변경
            } else {
                *stem = replace_plain(stem, find, with, *match_case);
            }
        }
        RenameOp::Case(mode) => *stem = apply_case(stem, *mode),
        RenameOp::Insert { text, suffix } => {
            if *suffix {
                stem.push_str(text);
            } else {
                *stem = format!("{text}{stem}");
            }
        }
        RenameOp::Number(n) => {
            let val = n.start + n.step * idx as i64;
            let num = if val < 0 {
                format!("-{:0width$}", -val, width = n.pad.max(1))
            } else {
                format!("{:0width$}", val, width = n.pad.max(1))
            };
            if n.suffix {
                stem.push_str(&num);
            } else {
                *stem = format!("{num}{stem}");
            }
        }
        RenameOp::Move {
            start,
            len,
            to_front,
        } => {
            let cs: Vec<char> = stem.chars().collect();
            let st = start.saturating_sub(1).min(cs.len());
            let ln = (*len).min(cs.len() - st);
            if ln == 0 {
                return;
            }
            let cut: String = cs[st..st + ln].iter().collect();
            let rest: String = cs[..st].iter().chain(&cs[st + ln..]).collect();
            *stem = if *to_front {
                format!("{cut}{rest}")
            } else {
                format!("{rest}{cut}")
            };
        }
        RenameOp::ChangeExt { from, to } => {
            if is_dir {
                return; // 폴더는 확장자 개념 없음
            }
            let cur = ext.strip_prefix('.').unwrap_or("");
            let hit = from.is_empty() || cur.eq_ignore_ascii_case(from);
            if hit && !cur.is_empty() {
                *ext = if to.is_empty() {
                    String::new()
                } else {
                    format!(".{to}")
                };
            }
        }
    }
}

/// 미리보기 — `items` = (현재 이름, 폴더 여부), 파이프라인을 순서대로 적용한 새 이름.
/// 연번은 목록 순서(호출자가 정렬 책임 — α: 선택 순서).
pub fn preview(items: &[(String, bool)], ops: &[RenameOp]) -> Vec<String> {
    items
        .iter()
        .enumerate()
        .map(|(idx, (name, is_dir))| {
            let (stem, ext) = split_stem(name, *is_dir);
            let mut s = stem.to_string();
            let mut e = ext.to_string();
            for op in ops {
                apply(op, &mut s, &mut e, idx, *is_dir);
            }
            format!("{}{}", s.trim(), e)
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

// ── 프리셋 직렬화(docs/25 §3 — key=value 라인·관용 파싱) ─────────────────

/// 필드 값 이스케이프 — 구분자 `|`·`\`·개행.
fn esc(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('\n', "\\n")
}

fn unesc(s: &str) -> String {
    let mut out = String::new();
    let mut ch = s.chars();
    while let Some(c) = ch.next() {
        if c == '\\' {
            match ch.next() {
                Some('|') => out.push('|'),
                Some('n') => out.push('\n'),
                Some('\\') => out.push('\\'),
                Some(o) => {
                    out.push('\\');
                    out.push(o);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// 이스케이프(`\|`)를 보존하며 `|`로 분리 — 필드는 unesc 전 원문.
fn split_fields(line: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut esc_next = false;
    for c in line.chars() {
        if esc_next {
            let cur = out.last_mut().unwrap();
            cur.push('\\');
            cur.push(c);
            esc_next = false;
        } else if c == '\\' {
            esc_next = true;
        } else if c == '|' {
            out.push(String::new());
        } else {
            out.last_mut().unwrap().push(c);
        }
    }
    if esc_next {
        out.last_mut().unwrap().push('\\');
    }
    out
}

/// 파이프라인 → 프리셋 텍스트(라인 순서 = 적용 순서). 파일 I/O는 앱(`data\renames\`).
pub fn serialize_ops(ops: &[RenameOp]) -> String {
    let mut out = String::from("# nexa-dir2 rename preset v1\n");
    for op in ops {
        let line = match op {
            RenameOp::Replace {
                find,
                with,
                match_case,
                regex,
            } => format!(
                "op=replace|find={}|with={}|case={}|regex={}",
                esc(find),
                esc(with),
                u8::from(*match_case),
                u8::from(*regex)
            ),
            RenameOp::Case(m) => format!("op=case|mode={}", m.as_str()),
            RenameOp::Insert { text, suffix } => format!(
                "op=insert|text={}|pos={}",
                esc(text),
                if *suffix { "suffix" } else { "prefix" }
            ),
            RenameOp::Number(n) => format!(
                "op=number|start={}|step={}|pad={}|pos={}",
                n.start,
                n.step,
                n.pad,
                if n.suffix { "suffix" } else { "prefix" }
            ),
            RenameOp::Move {
                start,
                len,
                to_front,
            } => format!(
                "op=move|start={start}|len={len}|dest={}",
                if *to_front { "front" } else { "end" }
            ),
            RenameOp::ChangeExt { from, to } => {
                format!("op=ext|from={}|to={}", esc(from), esc(to))
            }
        };
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// 프리셋 텍스트 → 파이프라인(관용 파싱 — 손상 라인·미지 종류는 무시, 상한 64블록).
pub fn parse_ops(text: &str) -> Vec<RenameOp> {
    let mut ops = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || ops.len() >= 64 {
            continue;
        }
        let fields = split_fields(line);
        let get = |key: &str| -> Option<String> {
            fields.iter().find_map(|f| {
                f.strip_prefix(key)
                    .and_then(|r| r.strip_prefix('='))
                    .map(unesc)
            })
        };
        let Some(kind) = get("op") else { continue };
        let op = match kind.as_str() {
            "replace" => RenameOp::Replace {
                find: get("find").unwrap_or_default(),
                with: get("with").unwrap_or_default(),
                match_case: get("case").as_deref() == Some("1"),
                regex: get("regex").as_deref() == Some("1"),
            },
            "case" => match get("mode").as_deref().and_then(CaseMode::from_str) {
                Some(m) => RenameOp::Case(m),
                None => continue,
            },
            "insert" => RenameOp::Insert {
                text: get("text").unwrap_or_default(),
                suffix: get("pos").as_deref() != Some("prefix"),
            },
            "number" => RenameOp::Number(NumberSpec {
                start: get("start").and_then(|v| v.parse().ok()).unwrap_or(1),
                step: get("step").and_then(|v| v.parse().ok()).unwrap_or(1),
                pad: get("pad").and_then(|v| v.parse().ok()).unwrap_or(3),
                suffix: get("pos").as_deref() != Some("prefix"),
            }),
            "move" => RenameOp::Move {
                start: get("start").and_then(|v| v.parse().ok()).unwrap_or(1),
                len: get("len").and_then(|v| v.parse().ok()).unwrap_or(0),
                to_front: get("dest").as_deref() == Some("front"),
            },
            "ext" => RenameOp::ChangeExt {
                from: get("from").unwrap_or_default(),
                to: get("to").unwrap_or_default(),
            },
            _ => continue,
        };
        ops.push(op);
    }
    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(names: &[&str]) -> Vec<(String, bool)> {
        names.iter().map(|n| (n.to_string(), false)).collect()
    }

    #[test]
    fn pipeline_applies_in_user_order() {
        // 사용자 예시(07-15): ① 연번 앞 ② 중간 2자 맨 뒤로 ③ 확장자 cfg→config
        let ops = vec![
            RenameOp::Number(NumberSpec {
                start: 1,
                step: 1,
                pad: 2,
                suffix: false,
            }),
            RenameOp::Move {
                start: 3,
                len: 2,
                to_front: false,
            },
            RenameOp::ChangeExt {
                from: "cfg".into(),
                to: "config".into(),
            },
        ];
        let out = preview(&files(&["settings.cfg", "session.cfg"]), &ops);
        // "settings" → "01settings" → 3번째부터 2자("se")를 맨 뒤로 → "01ttingsse" + .config
        assert_eq!(out, vec!["01ttingsse.config", "02ssionse.config"]);
    }

    #[test]
    fn op_order_matters() {
        let a = vec![
            RenameOp::Insert {
                text: "X".into(),
                suffix: false,
            },
            RenameOp::Case(CaseMode::Lower),
        ];
        let b = vec![
            RenameOp::Case(CaseMode::Lower),
            RenameOp::Insert {
                text: "X".into(),
                suffix: false,
            },
        ];
        assert_eq!(preview(&files(&["A.txt"]), &a), vec!["xa.txt"]);
        assert_eq!(preview(&files(&["A.txt"]), &b), vec!["Xa.txt"]);
    }

    #[test]
    fn regex_replace_with_captures_and_case_insensitive() {
        let ops = vec![RenameOp::Replace {
            find: r"img_(\d+)".into(),
            with: "photo-$1".into(),
            match_case: false,
            regex: true,
        }];
        assert_eq!(
            preview(&files(&["IMG_042.jpg"]), &ops),
            vec!["photo-042.jpg"]
        );
        // 검증 — 잘못된 패턴은 (블록 순번, 메시지)
        let bad = vec![RenameOp::Replace {
            find: "(".into(),
            with: "".into(),
            match_case: true,
            regex: true,
        }];
        assert!(matches!(validate(&bad), Err((0, _))));
        assert!(validate(&ops).is_ok());
    }

    #[test]
    fn move_and_ext_edge_cases() {
        // 범위 밖 이동 = 가능한 만큼(없으면 no-op)
        let mv = vec![RenameOp::Move {
            start: 10,
            len: 2,
            to_front: true,
        }];
        assert_eq!(preview(&files(&["ab.txt"]), &mv), vec!["ab.txt"]);
        // 확장자: from 불일치 = 유지·폴더 = no-op·to 빈 값 = 제거
        let ext = vec![RenameOp::ChangeExt {
            from: "cfg".into(),
            to: "config".into(),
        }];
        assert_eq!(preview(&files(&["a.txt"]), &ext), vec!["a.txt"]);
        assert_eq!(
            preview(&[("dir.cfg".into(), true)], &ext),
            vec!["dir.cfg"],
            "폴더는 확장자 변경 없음"
        );
        let strip = vec![RenameOp::ChangeExt {
            from: String::new(),
            to: String::new(),
        }];
        assert_eq!(preview(&files(&["a.bak"]), &strip), vec!["a"]);
    }

    #[test]
    fn preview_case_modes_and_dir_whole_name() {
        let t = vec![RenameOp::Case(CaseMode::Title)];
        assert_eq!(
            preview(&[("my file-name.txt".into(), false)], &t),
            vec!["My File-Name.txt"]
        );
        assert_eq!(
            preview(&[("archive.old".into(), true)], &t),
            vec!["Archive.Old"]
        );
        let s = vec![RenameOp::Case(CaseMode::Sentence)];
        assert_eq!(
            preview(&files(&["hELLO wORLD.md"]), &s),
            vec!["Hello world.md"]
        );
    }

    #[test]
    fn preset_round_trip_with_escapes() {
        let ops = vec![
            RenameOp::Replace {
                find: "a|b\\c".into(),
                with: "x".into(),
                match_case: true,
                regex: false,
            },
            RenameOp::Case(CaseMode::Title),
            RenameOp::Insert {
                text: "pre|fix".into(),
                suffix: false,
            },
            RenameOp::Number(NumberSpec {
                start: 5,
                step: 2,
                pad: 4,
                suffix: true,
            }),
            RenameOp::Move {
                start: 2,
                len: 3,
                to_front: true,
            },
            RenameOp::ChangeExt {
                from: "cfg".into(),
                to: "config".into(),
            },
        ];
        let text = serialize_ops(&ops);
        assert_eq!(parse_ops(&text), ops, "직렬화 왕복(이스케이프 포함)");
        // 손상 라인 관용
        assert!(parse_ops("op=unknown|x=1\ngarbage\n").is_empty());
    }

    #[test]
    fn conflict_detection_four_kinds() {
        let exists = |_: &str, n: &str| n == "taken.txt";
        let items: Vec<(String, String, String)> = vec![
            ("C:\\d".into(), "a.txt".into(), ".txt".into()),
            ("C:\\d".into(), "b.txt".into(), "b?.txt".into()),
            ("C:\\d".into(), "c1.txt".into(), "same.txt".into()),
            ("C:\\d".into(), "c2.txt".into(), "SAME.txt".into()),
            ("C:\\d".into(), "e.txt".into(), "taken.txt".into()),
            ("C:\\d".into(), "f.txt".into(), "f.txt".into()),
            ("C:\\d".into(), "g.txt".into(), "G.txt".into()),
        ];
        let out = conflicts(&items, &exists);
        assert_eq!(
            out,
            vec![
                Conflict::None,
                Conflict::Invalid,
                Conflict::Duplicate,
                Conflict::Duplicate,
                Conflict::Exists,
                Conflict::None,
                Conflict::None,
            ]
        );
        assert_eq!(
            conflicts(&[("p".into(), "x".into(), "  ".into())], &exists),
            vec![Conflict::Empty]
        );
    }
}
