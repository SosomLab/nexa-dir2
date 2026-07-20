//! 일괄 이름변경 v2(M5-1 → X-22 — 원본 docs/25 + **Path Finder 6동작 전수 대조**
//! [docs/22-batch-rename-v2.md] 기반 확장):
//! **순서형 동작 파이프라인**(블록 위→아래, 각 단계 출력이 다음 입력) + 미리보기 +
//! 충돌 검출 4종 + **프리셋 직렬화**(구 v1 프리셋 하위호환 파싱).
//!
//! v2 확장(07-17): ① **적용 스코프**(이름/전체/확장자/점 포함 확장자 — PF Apply to)
//! ② **임의 삽입 위치**(오프셋+방향·초과 클램프 — Insert/Number/Date 공용 PF Position)
//! ③ 치환 **Mode**(모든/첫/마지막 매치·전체 교체 — 일반 텍스트 전용, 정규식은 PF도 All)
//! ④ Number **감싸기**(Prefix/Suffix 텍스트) ⑤ **Add Date**(수정/생성일·토큰 포맷).
//!
//! 플랫폼 중립 순수 로직(파일시스템 접근은 `exists` 콜백 주입·날짜는 입력 전달) —
//! 맥 `cargo test` 대상. 적용·Undo(배치 = 트랜잭션 1건)는 앱 계층 `MoveBatchOp`.
//! 정규식 = `regex-lite`(DR-8 원장 docs/10 §1-2).

use regex_lite::Regex;

// ── 공통 타입(v2) ─────────────────────────────────────────────────

/// 적용 스코프(PF Apply to — 동작별 필드, 기본 `Name` = v1과 동일).
/// **폴더는 항상 전체가 이름부**(확장자 개념 없음)라 스코프와 무관하게 Name 취급.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Scope {
    #[default]
    Name,
    /// 확장자 포함 전체(적용 후 마지막 `.` 기준 재분해 — 탐색기 규약).
    NameExt,
    /// 확장자만(선행 `.` 제외 텍스트).
    Ext,
    /// 점 포함 확장자(`.md` — `.tar.gz`류 케이스용).
    ExtDot,
}

impl Scope {
    fn as_str(self) -> &'static str {
        match self {
            Scope::Name => "name",
            Scope::NameExt => "nameext",
            Scope::Ext => "ext",
            Scope::ExtDot => "extdot",
        }
    }
    fn from_str(s: &str) -> Scope {
        match s {
            "nameext" => Scope::NameExt,
            "ext" => Scope::Ext,
            "extdot" => Scope::ExtDot,
            _ => Scope::Name, // 생략/미지 = v1 동작
        }
    }
}

/// 삽입 위치(PF Position — Insert/Number/Date 공용): 선택한 끝에서 `offset` 문자
/// 지점, **범위 초과는 반대편으로 클램프**(관대 규약 — PF 미리보기로 확정, Move 동일).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct InsertAt {
    pub offset: usize,
    pub from_end: bool,
}

impl InsertAt {
    /// v1 `suffix: bool` 대응(앞 = {0,false} · 뒤 = {0,true}).
    pub fn edge(suffix: bool) -> InsertAt {
        InsertAt {
            offset: 0,
            from_end: suffix,
        }
    }
}

/// 문자 단위 삽입(UTF-8 안전) — [`InsertAt`] 규약.
fn insert_at(s: &str, at: InsertAt, ins: &str) -> String {
    let cs: Vec<char> = s.chars().collect();
    let off = at.offset.min(cs.len());
    let idx = if at.from_end { cs.len() - off } else { off };
    let mut out = String::new();
    out.extend(&cs[..idx]);
    out.push_str(ins);
    out.extend(&cs[idx..]);
    out
}

/// 치환 범위(PF Mode — **일반 텍스트 전용**. 정규식은 항상 All — PF 규약 동일,
/// 위치는 앵커 `^`/`$`로 표현).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ReplaceMode {
    #[default]
    All,
    First,
    Last,
    /// 매치가 있으면 **전체를 with로 교체**(find 빈 값 = 무조건 교체).
    Entire,
}

impl ReplaceMode {
    fn as_str(self) -> &'static str {
        match self {
            ReplaceMode::All => "all",
            ReplaceMode::First => "first",
            ReplaceMode::Last => "last",
            ReplaceMode::Entire => "entire",
        }
    }
    fn from_str(s: &str) -> ReplaceMode {
        match s {
            "first" => ReplaceMode::First,
            "last" => ReplaceMode::Last,
            "entire" => ReplaceMode::Entire,
            _ => ReplaceMode::All,
        }
    }
}

/// 대소문자 변경(docs/25 §2 동작 4 — PF Change Case 4모드와 1:1).
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

/// 연번(PF Add Number Sequence) — 시작·증가·0패딩 + **위치·감싸기**(v2).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NumberSpec {
    pub start: i64,
    pub step: i64,
    /// 0패딩 자릿수(1 = 패딩 없음).
    pub pad: usize,
    pub at: InsertAt,
    /// 연번을 감싸는 텍스트(PF Prefix/Suffix — `PRE{n}SUF` 한 덩어리로 삽입).
    pub prefix: String,
    pub suffix: String,
}

/// 날짜 원천(PF Add Date Type).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DateKind {
    #[default]
    Modified,
    Created,
}

impl DateKind {
    fn as_str(self) -> &'static str {
        match self {
            DateKind::Modified => "modified",
            DateKind::Created => "created",
        }
    }
    fn from_str(s: &str) -> DateKind {
        if s == "created" {
            DateKind::Created
        } else {
            DateKind::Modified
        }
    }
}

/// 날짜 삽입(PF Add Date — v2 신설) — 포맷은 토큰 문자열(드래그 빌더 대체,
/// [docs/22 §2-3]): `yyyy`/`yy`·`MMM`/`MM`/`M`·`ddd`·`dd`/`d`·`HH`·`mm`·`ss` + 리터럴.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DateSpec {
    pub kind: DateKind,
    pub format: String,
    pub at: InsertAt,
    pub prefix: String,
    pub suffix: String,
}

/// 파이프라인 동작 1블록 — 사용자가 순서대로 배치(위→아래 순차 적용).
#[derive(Clone, PartialEq, Debug)]
pub enum RenameOp {
    /// 텍스트/정규식 치환. `regex`면 `with`에 `$1` 캡처 참조, `mode`는 일반 텍스트 전용.
    Replace {
        scope: Scope,
        find: String,
        with: String,
        match_case: bool,
        regex: bool,
        mode: ReplaceMode,
    },
    /// 대소문자 변경.
    Case { scope: Scope, mode: CaseMode },
    /// 텍스트 삽입(임의 위치 — v2).
    Insert {
        scope: Scope,
        text: String,
        at: InsertAt,
    },
    /// 연번 — 항목 순서 기준.
    Number { scope: Scope, spec: NumberSpec },
    /// 날짜 삽입(v2 신설) — 파일별 수정/생성일.
    Date { scope: Scope, spec: DateSpec },
    /// 구간 이동(dir2 고유 — "중간 N자리를 잘라 맨 앞/뒤로"): `start` = 1기준.
    Move {
        start: usize,
        len: usize,
        to_front: bool,
    },
    /// 확장자 변경(dir2 고유 — 예: cfg→config). `from` 빈 값 = 모든 확장자.
    ChangeExt { from: String, to: String },
}

/// 미리보기 입력 1건(v2 — Date가 파일별 메타데이터 요구, [docs/22 §2-4]).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RenameInput {
    pub name: String,
    pub is_dir: bool,
    /// 수정/생성 시각(unix ms — 미상 0 = Date 결과 빈 문자열로 격리).
    pub modified_ms: i64,
    pub created_ms: i64,
}

impl RenameInput {
    /// 이름만으로 구성(테스트·날짜 무관 파이프라인용).
    pub fn plain(name: &str, is_dir: bool) -> RenameInput {
        RenameInput {
            name: name.into(),
            is_dir,
            modified_ms: 0,
            created_ms: 0,
        }
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
    /// 개명 예정인 **조상 폴더가 같은 배치에 선택**됨(X-25 1차 — 07-20): 조상이 먼저
    /// 개명되면 이 항목의 old_path가 디스크에서 소멸(순서 비결정 = undo·경로 추적
    /// 부정합) → 감지 후 적용 차단. 경로 rebase 완전 지원(2차)은 후속.
    Nested,
}

// ── 날짜 포맷(순수 — 외부 crate 0) ─────────────────────────────────

/// unix ms + TZ 오프셋(분) → (연, 월, 일, 시, 분, 초, 요일 0=일).
/// civil-from-days(Howard Hinnant 알고리즘 — fmt_datetime과 동일 계열).
fn civil(ms: i64, tz_min: i32) -> (i64, u32, u32, u32, u32, u32, u32) {
    let secs = ms.div_euclid(1000) + tz_min as i64 * 60;
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (h, mi, s) = (tod / 3600, (tod % 3600) / 60, tod % 60);
    // 1970-01-01 = 목(4). 요일 0=일요일.
    let weekday = ((days % 7 + 7) % 7 + 4) % 7;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d, h as u32, mi as u32, s as u32, weekday as u32)
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const MONTHS_FULL: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// 기본 날짜 포맷(`${}` 문법 — 사용자 확정 07-17).
pub const DEFAULT_DATE_FORMAT: &str = "${YYYY}-${MM}-${DD}";

/// 날짜 포맷(개정 07-17 — 사용자 확정: **텍스트 `${토큰}` 문법**):
/// 년 `${YYYY}`=2026·`${YY}`=26 · 월 `${MMMM}`=July·`${MMM}`=Jul·`${MM}`=07·
/// `${M}`=7 · 일 `${DD}`=07·`${D}`=7 · 요일 `${DDD}`=Tue · 시 `${HH}`/`${H}` ·
/// 분 `${mm}`/`${m}` · 초 `${ss}`/`${s}`. 미지 토큰·그 외 문자 = 리터럴 보존.
/// `ms == 0`(미상)이면 빈 문자열(오류 격리 — 무변경에 수렴).
pub fn format_date(fmt: &str, ms: i64, tz_min: i32) -> String {
    if ms == 0 {
        return String::new();
    }
    let (y, mo, d, h, mi, s, wd) = civil(ms, tz_min);
    let expand = |t: &str| -> Option<String> {
        Some(match t {
            "YYYY" => format!("{y:04}"),
            "YY" => format!("{:02}", y.rem_euclid(100)),
            "MMMM" => MONTHS_FULL[(mo - 1) as usize].to_string(),
            "MMM" => MONTHS[(mo - 1) as usize].to_string(),
            "MM" => format!("{mo:02}"),
            "M" => mo.to_string(),
            "DDD" => WEEKDAYS[wd as usize].to_string(),
            "DD" => format!("{d:02}"),
            "D" => d.to_string(),
            "HH" => format!("{h:02}"),
            "H" => h.to_string(),
            "mm" => format!("{mi:02}"),
            "m" => mi.to_string(),
            "ss" => format!("{s:02}"),
            "s" => s.to_string(),
            _ => return None,
        })
    };
    let mut out = String::new();
    let mut rest = fmt;
    while let Some(p) = rest.find("${") {
        out.push_str(&rest[..p]);
        let after = &rest[p + 2..];
        match after.find('}') {
            Some(e) => {
                match expand(&after[..e]) {
                    Some(v) => out.push_str(&v),
                    None => out.push_str(&rest[p..p + 2 + e + 1]), // 미지 토큰 = 리터럴
                }
                rest = &after[e + 1..];
            }
            None => {
                out.push_str(&rest[p..]); // 닫힘 없음 = 리터럴
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// 구식(v2 초기 — `yyyy-MM-dd` 계열) 포맷 → `${}` 문법 이행(프리셋 하위호환).
/// `${`가 이미 있으면 그대로.
pub fn migrate_date_format(fmt: &str) -> String {
    if fmt.contains("${") {
        return fmt.to_string();
    }
    let mut out = String::new();
    let cs: Vec<char> = fmt.chars().collect();
    let mut i = 0;
    let tok = |i: usize, t: &str| -> bool { cs[i..].iter().take(t.len()).collect::<String>() == t };
    // (구식, 신식) — 긴 토큰 우선(기존 매칭 규칙 그대로)
    const MAP: [(&str, &str); 11] = [
        ("yyyy", "${YYYY}"),
        ("yy", "${YY}"),
        ("MMM", "${MMM}"),
        ("MM", "${MM}"),
        ("M", "${M}"),
        ("ddd", "${DDD}"),
        ("dd", "${DD}"),
        ("d", "${D}"),
        ("HH", "${HH}"),
        ("mm", "${mm}"),
        ("ss", "${ss}"),
    ];
    'outer: while i < cs.len() {
        for (old, new) in MAP {
            if tok(i, old) {
                out.push_str(new);
                i += old.len();
                continue 'outer;
            }
        }
        out.push(cs[i]);
        i += 1;
    }
    out
}

// ── 치환/케이스 유틸 ───────────────────────────────────────────────

/// 대소문자 무시 치환(문자 단위 — UTF-8 길이 변화 안전). `limit_first`/`only_last` =
/// Mode(First/Last — v2). `match_case`면 정확 일치.
fn replace_plain(s: &str, find: &str, with: &str, match_case: bool, mode: ReplaceMode) -> String {
    if find.is_empty() {
        return match mode {
            ReplaceMode::Entire => with.to_string(), // 빈 find + Entire = 무조건 교체
            _ => s.to_string(),
        };
    }
    let sc: Vec<char> = s.chars().collect();
    let fc: Vec<char> = find.chars().collect();
    let eq = |a: char, b: char| {
        if match_case {
            a == b
        } else {
            a.to_lowercase().eq(b.to_lowercase())
        }
    };
    // 매치 시작 인덱스 수집(비중첩 — 왼쪽부터)
    let mut hits = Vec::new();
    let mut i = 0;
    while i + fc.len() <= sc.len() {
        if sc[i..i + fc.len()].iter().zip(&fc).all(|(a, b)| eq(*a, *b)) {
            hits.push(i);
            i += fc.len();
        } else {
            i += 1;
        }
    }
    if hits.is_empty() {
        return s.to_string();
    }
    match mode {
        ReplaceMode::Entire => return with.to_string(), // 매치 존재 = 전체 교체
        ReplaceMode::First => hits.truncate(1),
        ReplaceMode::Last => hits = vec![*hits.last().unwrap()],
        ReplaceMode::All => {}
    }
    let mut out = String::new();
    let mut pos = 0;
    for h in hits {
        out.extend(&sc[pos..h]);
        out.push_str(with);
        pos = h + fc.len();
    }
    out.extend(&sc[pos..]);
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

/// 스코프 적용(v2 — [docs/22 §2-1]): 작업 문자열 선택 → 변환 → 재조립.
/// 폴더(`is_dir`)는 확장자 개념이 없다 — Ext/ExtDot 스코프 = **무변경**(no-op),
/// NameExt = 전체 이름부(Name)로 수렴.
fn scoped(
    stem: &mut String,
    ext: &mut String,
    scope: Scope,
    is_dir: bool,
    f: impl FnOnce(&str) -> String,
) {
    let scope = if is_dir {
        match scope {
            Scope::Ext | Scope::ExtDot => return, // 폴더 = 대상 텍스트 없음
            _ => Scope::Name,
        }
    } else {
        scope
    };
    match scope {
        Scope::Name => *stem = f(stem),
        Scope::NameExt => {
            let joined = format!("{stem}{ext}");
            let r = f(&joined);
            let (s2, e2) = split_stem(&r, false); // 마지막 '.' 기준 재분해(탐색기 규약)
            let (s2, e2) = (s2.to_string(), e2.to_string());
            *stem = s2;
            *ext = e2;
        }
        Scope::Ext => {
            let cur = ext.strip_prefix('.').unwrap_or("");
            let r = f(cur);
            *ext = if r.is_empty() {
                String::new()
            } else {
                format!(".{r}")
            };
        }
        Scope::ExtDot => {
            // 점 포함 텍스트가 대상 = **결과를 그대로 채택**(사용자 확정 07-18 —
            // 기존 "점 유실 시 복원"이 앞삽입 결과 "INS.txt"에 점을 또 붙여
            // ".INS.txt" 이중 점 유발). 전체 이름 기준 재분해로 후속
            // 파이프라인 스코프 일관성 유지(NameExt 규약 동일).
            let r = f(ext);
            let joined = format!("{stem}{r}");
            let (s2, e2) = split_stem(&joined, false);
            let (s2, e2) = (s2.to_string(), e2.to_string());
            *stem = s2;
            *ext = e2;
        }
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

/// 동작 1블록 적용 — `stem`/`ext`(선행 `.` 포함) 제자리 갱신. `idx` = 연번 순번.
fn apply(
    op: &RenameOp,
    stem: &mut String,
    ext: &mut String,
    item: &RenameInput,
    idx: usize,
    tz_min: i32,
) {
    let is_dir = item.is_dir;
    match op {
        RenameOp::Replace {
            scope,
            find,
            with,
            match_case,
            regex,
            mode,
        } => scoped(stem, ext, *scope, is_dir, |s| {
            if *regex {
                match regex_of(find, *match_case) {
                    Ok(re) => re.replace_all(s, with.as_str()).into_owned(),
                    Err(_) => s.to_string(), // validate가 사전 차단 — 방어적 무변경
                }
            } else {
                replace_plain(s, find, with, *match_case, *mode)
            }
        }),
        RenameOp::Case { scope, mode } => {
            scoped(stem, ext, *scope, is_dir, |s| apply_case(s, *mode))
        }
        RenameOp::Insert { scope, text, at } => {
            scoped(stem, ext, *scope, is_dir, |s| insert_at(s, *at, text))
        }
        RenameOp::Number { scope, spec } => {
            let val = spec.start + spec.step * idx as i64;
            let num = if val < 0 {
                format!("-{:0width$}", -val, width = spec.pad.max(1))
            } else {
                format!("{:0width$}", val, width = spec.pad.max(1))
            };
            let ins = format!("{}{}{}", spec.prefix, num, spec.suffix); // 감싸기 일체(v2)
            scoped(stem, ext, *scope, is_dir, |s| insert_at(s, spec.at, &ins));
        }
        RenameOp::Date { scope, spec } => {
            let ms = match spec.kind {
                DateKind::Modified => item.modified_ms,
                DateKind::Created => item.created_ms,
            };
            let txt = format_date(&spec.format, ms, tz_min);
            if txt.is_empty() {
                return; // 시각 미상 = 무변경(오류 격리)
            }
            let ins = format!("{}{}{}", spec.prefix, txt, spec.suffix);
            scoped(stem, ext, *scope, is_dir, |s| insert_at(s, spec.at, &ins));
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

/// 미리보기(v2) — 파이프라인 순차 적용 결과. 연번은 목록 순서(호출자 정렬 책임).
/// `tz_min` = 날짜 표기 TZ 오프셋(분 — 앱의 fmt_datetime과 동일 값 전달).
pub fn preview(items: &[RenameInput], ops: &[RenameOp], tz_min: i32) -> Vec<String> {
    items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let (stem, ext) = split_stem(&item.name, item.is_dir);
            let mut s = stem.to_string();
            let mut e = ext.to_string();
            for op in ops {
                apply(op, &mut s, &mut e, item, idx, tz_min);
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
            // 조상-자손 동시 선택(X-25 1차): 개명 예정(new≠old)인 다른 항목의 전체
            // 경로(부모+현재 이름)가 이 항목 부모의 경로 접두이면 자손 = Nested.
            // 문자열 동등만 보던 기존 검사로는 미감지(경로 계층 비교 — 대소문자 무시).
            let norm = |s: &str| s.replace('/', "\\").trim_end_matches('\\').to_lowercase();
            let parent_n = norm(parent);
            let nested = items.iter().enumerate().any(|(j, (p, o, n))| {
                if j == i || n == o {
                    return false;
                }
                let full = format!("{}\\{}", norm(p), lower(o));
                parent_n == full || parent_n.starts_with(&format!("{full}\\"))
            });
            if nested {
                return Conflict::Nested;
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
            if lower(new) != lower(old) && exists(parent, new) {
                return Conflict::Exists;
            }
            Conflict::None
        })
        .collect()
}

// ── 프리셋 직렬화(v1 하위호환 — 생략 필드 = 기본) ────────────────────

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

fn at_fields(at: InsertAt) -> String {
    format!(
        "off={}|dir={}",
        at.offset,
        if at.from_end { "end" } else { "start" }
    )
}

/// 파이프라인 → 프리셋 텍스트(라인 순서 = 적용 순서). 파일 I/O는 앱(`data\renames\`).
pub fn serialize_ops(ops: &[RenameOp]) -> String {
    let mut out = String::from("# nexa-dir rename preset v2\n");
    for op in ops {
        let line = match op {
            RenameOp::Replace {
                scope,
                find,
                with,
                match_case,
                regex,
                mode,
            } => format!(
                "op=replace|scope={}|find={}|with={}|case={}|regex={}|mode={}",
                scope.as_str(),
                esc(find),
                esc(with),
                u8::from(*match_case),
                u8::from(*regex),
                mode.as_str()
            ),
            RenameOp::Case { scope, mode } => {
                format!("op=case|scope={}|mode={}", scope.as_str(), mode.as_str())
            }
            RenameOp::Insert { scope, text, at } => format!(
                "op=insert|scope={}|text={}|{}",
                scope.as_str(),
                esc(text),
                at_fields(*at)
            ),
            RenameOp::Number { scope, spec } => format!(
                "op=number|scope={}|start={}|step={}|pad={}|{}|pre={}|suf={}",
                scope.as_str(),
                spec.start,
                spec.step,
                spec.pad,
                at_fields(spec.at),
                esc(&spec.prefix),
                esc(&spec.suffix)
            ),
            RenameOp::Date { scope, spec } => format!(
                "op=date|scope={}|kind={}|fmt={}|{}|pre={}|suf={}",
                scope.as_str(),
                spec.kind.as_str(),
                esc(&spec.format),
                at_fields(spec.at),
                esc(&spec.prefix),
                esc(&spec.suffix)
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

/// 프리셋 텍스트 → 파이프라인(관용 파싱 — 손상 라인·미지 종류 무시, 상한 64블록).
/// **v1 하위호환**: `scope`/`mode`/`off`/`dir`/`pre`/`suf` 생략 = 기본,
/// 구 `pos=prefix|suffix`는 [`InsertAt::edge`]로 매핑.
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
        let scope = Scope::from_str(get("scope").as_deref().unwrap_or(""));
        // 위치: v2 off/dir → 없으면 v1 pos=prefix|suffix → 기본 suffix(뒤)
        let at = || -> InsertAt {
            if let Some(off) = get("off").and_then(|v| v.parse().ok()) {
                InsertAt {
                    offset: off,
                    from_end: get("dir").as_deref() != Some("start"),
                }
            } else {
                InsertAt::edge(get("pos").as_deref() != Some("prefix"))
            }
        };
        let Some(kind) = get("op") else { continue };
        let op = match kind.as_str() {
            "replace" => RenameOp::Replace {
                scope,
                find: get("find").unwrap_or_default(),
                with: get("with").unwrap_or_default(),
                match_case: get("case").as_deref() == Some("1"),
                regex: get("regex").as_deref() == Some("1"),
                mode: ReplaceMode::from_str(get("mode").as_deref().unwrap_or("")),
            },
            "case" => match get("mode").as_deref().and_then(CaseMode::from_str) {
                Some(mode) => RenameOp::Case { scope, mode },
                None => continue,
            },
            "insert" => RenameOp::Insert {
                scope,
                text: get("text").unwrap_or_default(),
                at: at(),
            },
            "number" => RenameOp::Number {
                scope,
                spec: NumberSpec {
                    start: get("start").and_then(|v| v.parse().ok()).unwrap_or(1),
                    step: get("step").and_then(|v| v.parse().ok()).unwrap_or(1),
                    pad: get("pad").and_then(|v| v.parse().ok()).unwrap_or(3),
                    at: at(),
                    prefix: get("pre").unwrap_or_default(),
                    suffix: get("suf").unwrap_or_default(),
                },
            },
            "date" => RenameOp::Date {
                scope,
                spec: DateSpec {
                    kind: DateKind::from_str(get("kind").as_deref().unwrap_or("")),
                    // 구식(yyyy-MM-dd) 프리셋 = ${} 문법으로 자동 이행(07-17)
                    format: migrate_date_format(
                        &get("fmt").unwrap_or_else(|| DEFAULT_DATE_FORMAT.into()),
                    ),
                    at: at(),
                    prefix: get("pre").unwrap_or_default(),
                    suffix: get("suf").unwrap_or_default(),
                },
            },
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

    fn files(names: &[&str]) -> Vec<RenameInput> {
        names.iter().map(|n| RenameInput::plain(n, false)).collect()
    }

    fn pv(items: &[RenameInput], ops: &[RenameOp]) -> Vec<String> {
        preview(items, ops, 0)
    }

    #[test]
    fn pipeline_applies_in_user_order() {
        // 사용자 예시(07-15): ① 연번 앞 ② 중간 2자 맨 뒤로 ③ 확장자 cfg→config
        let ops = vec![
            RenameOp::Number {
                scope: Scope::Name,
                spec: NumberSpec {
                    start: 1,
                    step: 1,
                    pad: 2,
                    at: InsertAt::edge(false),
                    prefix: String::new(),
                    suffix: String::new(),
                },
            },
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
        let out = pv(&files(&["settings.cfg", "session.cfg"]), &ops);
        assert_eq!(out, vec!["01ttingsse.config", "02ssionse.config"]);
    }

    #[test]
    fn scope_variants_select_working_text() {
        // PF Apply to(v2): Name/NameExt/Ext/ExtDot — 같은 치환의 스코프별 결과
        let mk = |scope| RenameOp::Replace {
            scope,
            find: "md".into(),
            with: "XX".into(),
            match_case: false,
            regex: false,
            mode: ReplaceMode::All,
        };
        let f = files(&["md-file.md"]);
        assert_eq!(pv(&f, &[mk(Scope::Name)]), vec!["XX-file.md"]);
        assert_eq!(pv(&f, &[mk(Scope::NameExt)]), vec!["XX-file.XX"]);
        assert_eq!(pv(&f, &[mk(Scope::Ext)]), vec!["md-file.XX"]);
        // ExtDot — 점 포함 텍스트가 대상, 결과 그대로 채택(사용자 확정 07-18 —
        // 자동 점 복원 폐지: ".tar.gz"를 원하면 점까지 입력)
        let dot = |with: &str| RenameOp::Replace {
            scope: Scope::ExtDot,
            find: ".md".into(),
            with: with.into(),
            match_case: false,
            regex: false,
            mode: ReplaceMode::All,
        };
        assert_eq!(pv(&f, &[dot(".tar.gz")]), vec!["md-file.tar.gz"]);
        assert_eq!(pv(&f, &[dot("tar")]), vec!["md-filetar"]); // 점 제거 = 문자 그대로
        assert_eq!(pv(&f, &[dot("")]), vec!["md-file"]); // 점까지 제거 = 확장자 없음
                                                         // 폴더 = 스코프 무관 전체 이름부
        assert_eq!(
            pv(&[RenameInput::plain("md.dir", true)], &[mk(Scope::Ext)]),
            vec!["md.dir"]
        );
    }

    #[test]
    fn replace_modes_first_last_entire() {
        let m = |mode| RenameOp::Replace {
            scope: Scope::Name,
            find: "a".into(),
            with: "X".into(),
            match_case: false,
            regex: false,
            mode,
        };
        let f = files(&["banana.txt"]);
        assert_eq!(pv(&f, &[m(ReplaceMode::All)]), vec!["bXnXnX.txt"]);
        assert_eq!(pv(&f, &[m(ReplaceMode::First)]), vec!["bXnana.txt"]);
        assert_eq!(pv(&f, &[m(ReplaceMode::Last)]), vec!["bananX.txt"]);
        assert_eq!(pv(&f, &[m(ReplaceMode::Entire)]), vec!["X.txt"]);
        // Entire + 매치 없음 = 무변경 · 빈 find + Entire = 무조건 교체
        let none = RenameOp::Replace {
            scope: Scope::Name,
            find: "zz".into(),
            with: "X".into(),
            match_case: false,
            regex: false,
            mode: ReplaceMode::Entire,
        };
        assert_eq!(pv(&f, &[none]), vec!["banana.txt"]);
        let always = RenameOp::Replace {
            scope: Scope::Name,
            find: String::new(),
            with: "N".into(),
            match_case: false,
            regex: false,
            mode: ReplaceMode::Entire,
        };
        assert_eq!(pv(&f, &[always]), vec!["N.txt"]);
    }

    #[test]
    fn extdot_insert_front_no_double_dot() {
        // 사용자 QA 07-18: Insert(ExtDot, 앞 0, "INS")가 ".INS.txt" 이중 점 —
        // 결과 그대로 채택 규약으로 "faaa052INS.txt"가 정답. 같은 scoped 경로를
        // 쓰는 연번/날짜 앞삽입도 전수 확인.
        let ins = RenameOp::Insert {
            scope: Scope::ExtDot,
            text: "INS".into(),
            at: InsertAt {
                offset: 0,
                from_end: false,
            },
        };
        assert_eq!(pv(&files(&["faaa052.txt"]), &[ins]), vec!["faaa052INS.txt"]);
        let num = RenameOp::Number {
            scope: Scope::ExtDot,
            spec: NumberSpec {
                start: 7,
                step: 1,
                pad: 2,
                prefix: String::new(),
                suffix: "_".into(),
                at: InsertAt {
                    offset: 0,
                    from_end: false,
                },
            },
        };
        assert_eq!(pv(&files(&["a.txt"]), &[num]), vec!["a07_.txt"]);
        // Case는 점 보존 경로(무영향) — 회귀 가드
        let up = RenameOp::Case {
            scope: Scope::ExtDot,
            mode: CaseMode::Upper,
        };
        assert_eq!(pv(&files(&["a.txt"]), &[up]), vec!["a.TXT"]);
    }

    #[test]
    fn insert_at_arbitrary_position_with_clamp() {
        // PF 미리보기로 확정한 시맨틱: 끝기준 2 → 2자 앞에 삽입 · 초과 = 반대편 클램프
        let ins = |offset, from_end| RenameOp::Insert {
            scope: Scope::Name,
            text: "aa".into(),
            at: InsertAt { offset, from_end },
        };
        assert_eq!(pv(&files(&["a.md"]), &[ins(2, true)]), vec!["aaa.md"]); // 클램프
        assert_eq!(
            pv(&files(&["개발순서-상세.md"]), &[ins(2, true)]),
            vec!["개발순서-aa상세.md"]
        );
        assert_eq!(
            pv(&files(&["subXY.md"]), &[ins(2, false)]),
            vec!["suaabXY.md"]
        );
    }

    #[test]
    fn number_with_wrapping_and_position() {
        // PF 예제 재현: Position 2(앞) · start 3 · step 3 · pad 2 · PRE/SUF 감싸기
        let ops = vec![RenameOp::Number {
            scope: Scope::Name,
            spec: NumberSpec {
                start: 3,
                step: 3,
                pad: 2,
                at: InsertAt {
                    offset: 2,
                    from_end: false,
                },
                prefix: "PRE".into(),
                suffix: "SUF".into(),
            },
        }];
        let out = pv(&files(&["a.md", "sublime.md"]), &ops);
        assert_eq!(out, vec!["aPRE03SUF.md", "suPRE06SUFblime.md"]);
    }

    #[test]
    fn date_format_tokens_and_missing_time() {
        // 2026-07-01 12:34:56 UTC = 1782909296000ms — ${} 토큰 조합(개정 07-17)
        let ms = 1_782_909_296_000i64;
        assert_eq!(format_date("${YYYY}-${MM}-${DD}", ms, 0), "2026-07-01");
        assert_eq!(format_date("${YY}.${M}.${D}", ms, 0), "26.7.1");
        assert_eq!(format_date("${MMM} ${D} ${DDD}", ms, 0), "Jul 1 Wed");
        assert_eq!(format_date("${MMMM}", ms, 0), "July");
        assert_eq!(format_date("${HH}${mm}${ss}", ms, 0), "123456");
        assert_eq!(format_date("${H}:${m}:${s}", ms, 0), "12:34:56");
        // 미지 토큰·불완전 = 리터럴 보존
        assert_eq!(format_date("${XX}-${YYYY", ms, 0), "${XX}-${YYYY");
        // TZ 오프셋(+9h) — 날짜 경계 이동
        assert_eq!(format_date("${DD} ${HH}", ms, 540), "01 21");
        // 미상(0) = 빈 문자열 → Date 동작은 무변경
        assert_eq!(format_date("${YYYY}", 0, 0), "");
        // 구식 포맷 이행(프리셋 하위호환)
        assert_eq!(migrate_date_format("yyyy-MM-dd"), "${YYYY}-${MM}-${DD}");
        assert_eq!(migrate_date_format("MMM d ddd"), "${MMM} ${D} ${DDD}");
        assert_eq!(
            migrate_date_format("${YYYY}-${MM}"), // 신식 = 그대로
            "${YYYY}-${MM}"
        );
        let d = RenameOp::Date {
            scope: Scope::Name,
            spec: DateSpec {
                kind: DateKind::Modified,
                format: "${YYYY}-${MM}-${DD}".into(),
                at: InsertAt {
                    offset: 2,
                    from_end: false,
                },
                prefix: "PRE".into(),
                suffix: "SUF".into(),
            },
        };
        let mut item = RenameInput::plain("sublime.md", false);
        item.modified_ms = ms;
        assert_eq!(
            preview(&[item], std::slice::from_ref(&d), 0),
            vec!["suPRE2026-07-01SUFblime.md"] // PF 예제 재현
        );
        // 시각 미상 = 무변경(오류 격리)
        assert_eq!(
            preview(&[RenameInput::plain("a.md", false)], &[d], 0),
            vec!["a.md"]
        );
    }

    #[test]
    fn op_order_matters() {
        let ins = RenameOp::Insert {
            scope: Scope::Name,
            text: "X".into(),
            at: InsertAt::edge(false),
        };
        let case = RenameOp::Case {
            scope: Scope::Name,
            mode: CaseMode::Lower,
        };
        assert_eq!(
            pv(&files(&["A.txt"]), &[ins.clone(), case.clone()]),
            vec!["xa.txt"]
        );
        assert_eq!(pv(&files(&["A.txt"]), &[case, ins]), vec!["Xa.txt"]);
    }

    #[test]
    fn regex_replace_with_captures_and_case_insensitive() {
        let ops = vec![RenameOp::Replace {
            scope: Scope::Name,
            find: r"img_(\d+)".into(),
            with: "photo-$1".into(),
            match_case: false,
            regex: true,
            mode: ReplaceMode::All,
        }];
        assert_eq!(pv(&files(&["IMG_042.jpg"]), &ops), vec!["photo-042.jpg"]);
        let bad = vec![RenameOp::Replace {
            scope: Scope::Name,
            find: "(".into(),
            with: "".into(),
            match_case: true,
            regex: true,
            mode: ReplaceMode::All,
        }];
        assert!(matches!(validate(&bad), Err((0, _))));
        assert!(validate(&ops).is_ok());
    }

    #[test]
    fn move_and_ext_edge_cases() {
        let mv = vec![RenameOp::Move {
            start: 10,
            len: 2,
            to_front: true,
        }];
        assert_eq!(pv(&files(&["ab.txt"]), &mv), vec!["ab.txt"]);
        let ext = vec![RenameOp::ChangeExt {
            from: "cfg".into(),
            to: "config".into(),
        }];
        assert_eq!(pv(&files(&["a.txt"]), &ext), vec!["a.txt"]);
        assert_eq!(
            pv(&[RenameInput::plain("dir.cfg", true)], &ext),
            vec!["dir.cfg"],
            "폴더는 확장자 변경 없음"
        );
        let strip = vec![RenameOp::ChangeExt {
            from: String::new(),
            to: String::new(),
        }];
        assert_eq!(pv(&files(&["a.bak"]), &strip), vec!["a"]);
    }

    #[test]
    fn preview_case_modes_and_dir_whole_name() {
        let t = vec![RenameOp::Case {
            scope: Scope::Name,
            mode: CaseMode::Title,
        }];
        assert_eq!(
            pv(&[RenameInput::plain("my file-name.txt", false)], &t),
            vec!["My File-Name.txt"]
        );
        assert_eq!(
            pv(&[RenameInput::plain("archive.old", true)], &t),
            vec!["Archive.Old"]
        );
        let s = vec![RenameOp::Case {
            scope: Scope::Name,
            mode: CaseMode::Sentence,
        }];
        assert_eq!(pv(&files(&["hELLO wORLD.md"]), &s), vec!["Hello world.md"]);
    }

    #[test]
    fn preset_round_trip_v2_and_v1_compat() {
        let ops = vec![
            RenameOp::Replace {
                scope: Scope::NameExt,
                find: "a|b\\c".into(),
                with: "x".into(),
                match_case: true,
                regex: false,
                mode: ReplaceMode::Last,
            },
            RenameOp::Case {
                scope: Scope::Ext,
                mode: CaseMode::Title,
            },
            RenameOp::Insert {
                scope: Scope::Name,
                text: "pre|fix".into(),
                at: InsertAt {
                    offset: 3,
                    from_end: true,
                },
            },
            RenameOp::Number {
                scope: Scope::Name,
                spec: NumberSpec {
                    start: 5,
                    step: 2,
                    pad: 4,
                    at: InsertAt::edge(true),
                    prefix: "P|".into(),
                    suffix: "S".into(),
                },
            },
            RenameOp::Date {
                scope: Scope::Name,
                spec: DateSpec {
                    kind: DateKind::Created,
                    format: "${YYYY}-${MM}-${DD}".into(),
                    at: InsertAt::edge(false),
                    prefix: String::new(),
                    suffix: "_".into(),
                },
            },
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
        assert_eq!(parse_ops(&text), ops, "v2 왕복(이스케이프 포함)");
        // v1 프리셋 하위호환 — 생략 필드 = 기본·pos=prefix/suffix 매핑
        let v1 = "op=replace|find=a|with=b|case=0|regex=0\n\
                  op=case|mode=title\n\
                  op=insert|text=X|pos=prefix\n\
                  op=number|start=1|step=1|pad=3|pos=suffix\n";
        let parsed = parse_ops(v1);
        assert_eq!(
            parsed[0],
            RenameOp::Replace {
                scope: Scope::Name,
                find: "a".into(),
                with: "b".into(),
                match_case: false,
                regex: false,
                mode: ReplaceMode::All,
            }
        );
        assert_eq!(
            parsed[2],
            RenameOp::Insert {
                scope: Scope::Name,
                text: "X".into(),
                at: InsertAt::edge(false),
            }
        );
        match &parsed[3] {
            RenameOp::Number { spec, .. } => {
                assert_eq!(spec.at, InsertAt::edge(true));
                assert!(spec.prefix.is_empty() && spec.suffix.is_empty());
            }
            _ => panic!("number"),
        }
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

    #[test]
    fn nested_selection_blocks_descendants() {
        // X-25 1차(07-20): 조상 폴더와 자손을 함께 선택 — 조상이 개명 예정이면
        // 자손 = Nested(적용 차단). 접두 유사 형제(parents)는 오탐 없음.
        let exists = |_: &str, _: &str| false;
        let items: Vec<(String, String, String)> = vec![
            ("C:\\d".into(), "parent".into(), "parent2".into()),
            ("C:\\d\\parent".into(), "a.txt".into(), "b.txt".into()),
            ("C:\\d\\Parent\\sub".into(), "c.txt".into(), "d.txt".into()),
            ("C:\\d\\parents".into(), "e.txt".into(), "f.txt".into()),
        ];
        assert_eq!(
            conflicts(&items, &exists),
            vec![
                Conflict::None,
                Conflict::Nested, // 직계 자손(대소문자 무시)
                Conflict::Nested, // 깊은 자손
                Conflict::None,   // 접두 유사 형제
            ]
        );
        // 조상이 무변경이면 자손은 안전 — 충돌 없음
        let safe: Vec<(String, String, String)> = vec![
            ("C:\\d".into(), "parent".into(), "parent".into()),
            ("C:\\d\\parent".into(), "a.txt".into(), "b.txt".into()),
        ];
        assert_eq!(
            conflicts(&safe, &exists),
            vec![Conflict::None, Conflict::None]
        );
    }
}
