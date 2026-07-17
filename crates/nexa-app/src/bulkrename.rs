//! 일괄 이름변경 다이얼로그(M5-1 → 07-15 확장 — 원본 docs/25 §1~3 블록 스택):
//! **순서형 파이프라인 편집기** = 동작 종류 선택 + 파라미터 폼 → [블록 추가] →
//! 파이프라인 목록(선택·▲▼ 재배치·삭제) + 우측 실시간 미리보기(충돌 ⚠·적용 차단) +
//! **프리셋 저장/불러오기**(`data\renames\*.cfg` — docs/25 §3 "Save Renaming Sequence").
//!
//! 순수 로직·직렬화 = [`nexa_ops::batch_rename`], 이 모듈은 네이티브 컨트롤 UI만.
//! prefs.rs와 동일 규약: user32 컨트롤(COMBOBOX/LISTBOX 포함 — comctl32 비의존·B3 게이트)·
//! 자체 모달 루프. 블록 편집은 삭제 후 재추가(α — 제자리 편집은 후속).

use std::path::{Path, PathBuf};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetSysColorBrush, SetBkMode, COLOR_WINDOW, HBRUSH, HFONT, TRANSPARENT,
};
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetMessageW, GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, IsWindow, RegisterClassW,
    SendMessageW, SetForegroundWindow, SetWindowLongPtrW, ShowWindow, TranslateMessage,
    BS_AUTOCHECKBOX, BS_AUTORADIOBUTTON, ES_AUTOHSCROLL, ES_NUMBER, GWLP_USERDATA, HMENU, MSG,
    SW_HIDE, SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_CTLCOLORBTN,
    WM_CTLCOLORSTATIC, WM_SETFONT, WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CHILD, WS_GROUP, WS_POPUP,
    WS_SYSMENU, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
};

use crate::ctl::combobox::{NXCB_GETSEL, NXCB_SETSEL};
use crate::ctl::segmented::SEG_GETSEL;
use crate::ctl::spin::SPIN_GETVAL;
use crate::ctl::style::Style;
use crate::dialog::DlgFont;
use crate::i18n::tr;
use nexa_ops::batch_rename::{
    conflicts, parse_ops, preview, serialize_ops, validate, CaseMode, Conflict, DateKind, DateSpec,
    InsertAt, NumberSpec, RenameInput, RenameOp, ReplaceMode, Scope,
};

const CLASS: PCWSTR = w!("NexaBulkRename");
static REGISTER: std::sync::Once = std::sync::Once::new();

const PAD: i32 = 12;
const FORM_W: i32 = 300;
const CLIENT_W: i32 = 880;
const CLIENT_H: i32 = 620; // v2 — 파라미터 4행·스코프 행 추가분
const STYLE: WINDOW_STYLE = WINDOW_STYLE(WS_POPUP.0 | WS_CAPTION.0 | WS_SYSMENU.0);

// 컨트롤 id — 파이프라인 편집
const ID_KIND: u32 = 1;
/// 적용 스코프(v2 — PF Apply to, ctl::combobox). Move/Ext 종류에선 숨김.
const ID_SCOPE: u32 = 7;
const ID_ADD: u32 = 2;
const ID_UP: u32 = 3;
const ID_DOWN: u32 = 4;
const ID_DEL: u32 = 5;
const ID_PIPE: u32 = 6;
// 파라미터(종류별 패널)
const ID_FIND: u32 = 10;
const ID_WITH: u32 = 11;
const ID_MC: u32 = 12;
const ID_RX: u32 = 13;
/// 치환 Mode(v2 — 모든/첫/마지막/전체, ctl::combobox — 정규식 체크 시 숨김).
const ID_RX_MODE: u32 = 14;
const ID_CASE_BASE: u32 = 20; // +0..3 = upper/lower/title/sentence
const ID_INS_TEXT: u32 = 30;
const ID_INS_OFF: u32 = 33; // 위치 오프셋(ctl::spin)
const ID_INS_DIR: u32 = 34; // 앞/뒤(ctl::segmented)
const ID_NUM_START: u32 = 40;
const ID_NUM_STEP: u32 = 41;
const ID_NUM_PAD: u32 = 42;
const ID_NUM_OFF: u32 = 45; // 위치(ctl::spin)
const ID_NUM_DIR: u32 = 46; // 앞/뒤(ctl::segmented)
const ID_NUM_WPRE: u32 = 47; // 감싸기 Prefix(v2)
const ID_NUM_WSUF: u32 = 48; // 감싸기 Suffix(v2)
                             // 날짜(v2 신설 — PF Add Date)
const ID_DT_KIND: u32 = 90; // 수정/생성(ctl::combobox)
const ID_DT_FMT: u32 = 91;
const ID_DT_OFF: u32 = 92;
const ID_DT_DIR: u32 = 93;
const ID_DT_PRE: u32 = 94;
const ID_DT_SUF: u32 = 95;
/// 변경 건수("N개 항목이 변경됩니다" — PF 카운트 대응).
const ID_COUNT: u32 = 82;
const ID_MV_START: u32 = 50;
const ID_MV_LEN: u32 = 51;
const ID_MV_FRONT: u32 = 52;
const ID_MV_END: u32 = 53;
const ID_EXT_FROM: u32 = 60;
const ID_EXT_TO: u32 = 61;
// 프리셋·확정
const ID_PRESET_NAME: u32 = 70;
const ID_PRESET_SAVE: u32 = 71;
const ID_PRESET_COMBO: u32 = 72;
const ID_PRESET_LOAD: u32 = 73;
const ID_APPLY: u32 = 80;
const ID_CANCEL: u32 = 81;

// user32 컨트롤 메시지(winuser.h)
const LB_ADDSTRING: u32 = 0x0180;
const LB_RESETCONTENT: u32 = 0x0184;
const LB_SETCURSEL: u32 = 0x0186;
const LB_GETCURSEL: u32 = 0x0188;
const CB_ADDSTRING: u32 = 0x0143;
const CB_GETCURSEL: u32 = 0x0147;
const CB_RESETCONTENT: u32 = 0x014B;
const CB_SETCURSEL: u32 = 0x014E;
const CB_GETLBTEXT: u32 = 0x0148;
const CB_GETLBTEXTLEN: u32 = 0x0149;
const EM_SETCUEBANNER: u32 = 0x1501;

/// 동작 종류(콤보 순서) — i18n 라벨 키. v2: 날짜(4) 신설(PF Add Date).
const KINDS: [&str; 7] = [
    "bulk.kind.replace",
    "bulk.kind.case",
    "bulk.kind.insert",
    "bulk.kind.number",
    "bulk.kind.date",
    "bulk.kind.move",
    "bulk.kind.ext",
];
/// 스코프 콤보 항목 순서 ↔ [`Scope`] 매핑.
const SCOPES: [Scope; 4] = [Scope::Name, Scope::NameExt, Scope::Ext, Scope::ExtDot];

struct BrState {
    font: HFONT,
    /// 대상 = (부모, 현재 이름, 폴더, 수정 ms, 생성 ms) — 선택 순서 보존(연번 기준).
    /// 시각은 Date 동작용(v2 — 미상 0 = 무변경 격리).
    items: Vec<(String, String, bool, i64, i64)>,
    /// 날짜 표기 TZ 오프셋(분 — 호스트 전달).
    tz_min: i32,
    /// 파이프라인(위→아래 순차 적용) — UI의 단일 원천.
    ops: Vec<RenameOp>,
    /// 종류별 파라미터 컨트롤(show/hide 스왑).
    panels: [Vec<HWND>; 7],
    /// 공통 스코프 droplist(Move/Ext 종류에선 숨김 — show_kind).
    scope: HWND,
    pipe: HWND,
    prev: HWND,
    err: HWND,
    apply: HWND,
    preset_combo: HWND,
    count: HWND,
    /// 확정 결과 — (부모, 현재 이름, 새 이름)의 변경 항목만(적용 클릭 시 채움).
    result: Option<Vec<(String, String, String)>>,
}

unsafe fn get_text(hwnd: HWND) -> String {
    let len = GetWindowTextLengthW(hwnd);
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; len as usize + 1];
    let got = GetWindowTextW(hwnd, &mut buf);
    String::from_utf16_lossy(&buf[..got.max(0) as usize])
}

unsafe fn set_text(hwnd: HWND, text: &str) {
    let t = windows::core::HSTRING::from(text);
    let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowTextW(hwnd, PCWSTR(t.as_ptr()));
}

#[allow(clippy::too_many_arguments)] // Win32 CreateWindow 인자 전달(prefs.rs 동일)
unsafe fn mk(
    parent: HWND,
    font: HFONT,
    class: PCWSTR,
    text: &str,
    style: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    id: u32,
) -> HWND {
    let t = windows::core::HSTRING::from(text);
    let hw = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        class,
        PCWSTR(t.as_ptr()),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(style),
        x,
        y,
        w,
        h,
        Some(parent),
        Some(HMENU(id as usize as *mut core::ffi::c_void)),
        None,
        None,
    )
    .unwrap_or_default();
    SendMessageW(
        hw,
        WM_SETFONT,
        Some(WPARAM(font.0 as usize)),
        Some(LPARAM(1)),
    );
    hw
}

unsafe fn cue(hwnd: HWND, text: &str) {
    let w: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    SendMessageW(
        hwnd,
        EM_SETCUEBANNER,
        Some(WPARAM(1)),
        Some(LPARAM(w.as_ptr() as isize)),
    );
}

unsafe fn ctl(dlg: HWND, id: u32) -> HWND {
    windows::Win32::UI::WindowsAndMessaging::GetDlgItem(Some(dlg), id as i32).unwrap_or_default()
}

unsafe fn checked(dlg: HWND, id: u32) -> bool {
    SendMessageW(ctl(dlg, id), 0x00F0 /* BM_GETCHECK */, None, None).0 == 1
}

unsafe fn set_check(dlg: HWND, id: u32, on: bool) {
    SendMessageW(
        ctl(dlg, id),
        0x00F1, // BM_SETCHECK
        Some(WPARAM(usize::from(on))),
        None,
    );
}

unsafe fn num_of(dlg: HWND, id: u32, default: i64) -> i64 {
    get_text(ctl(dlg, id)).trim().parse().unwrap_or(default)
}

/// 프리셋 폴더(`data\renames\` — docs/25 §3, settings와 별도).
fn preset_dir() -> PathBuf {
    crate::config::data_dir().join("renames")
}

/// 폼의 공통 스코프(droplist 선택 → [`Scope`]).
unsafe fn scope_of(dlg: HWND) -> Scope {
    let i = SendMessageW(ctl(dlg, ID_SCOPE), NXCB_GETSEL, None, None).0 as usize;
    SCOPES.get(i).copied().unwrap_or(Scope::Name)
}

/// 위치 폼(spin 오프셋 + segmented 방향) → [`InsertAt`].
unsafe fn at_of(dlg: HWND, off_id: u32, dir_id: u32) -> InsertAt {
    InsertAt {
        offset: SendMessageW(ctl(dlg, off_id), SPIN_GETVAL, None, None)
            .0
            .max(0) as usize,
        from_end: SendMessageW(ctl(dlg, dir_id), SEG_GETSEL, None, None).0 == 1,
    }
}

/// 현재 파라미터 폼 → 동작 1블록(선택된 종류 기준). 유효하지 않으면 None.
unsafe fn op_from_form(dlg: HWND, kind: usize) -> Option<RenameOp> {
    let scope = scope_of(dlg);
    match kind {
        0 => {
            let find = get_text(ctl(dlg, ID_FIND));
            let regex = checked(dlg, ID_RX);
            let mode_i = SendMessageW(ctl(dlg, ID_RX_MODE), NXCB_GETSEL, None, None).0;
            let mode = if regex {
                ReplaceMode::All // 정규식 = 항상 All(PF 규약 — 앵커로 대체)
            } else {
                [
                    ReplaceMode::All,
                    ReplaceMode::First,
                    ReplaceMode::Last,
                    ReplaceMode::Entire,
                ][(mode_i.max(0) as usize).min(3)]
            };
            // Entire + 빈 find = 무조건 교체 허용 — 그 외 빈 find는 무효
            (!find.is_empty() || mode == ReplaceMode::Entire).then(|| RenameOp::Replace {
                scope,
                find,
                with: get_text(ctl(dlg, ID_WITH)),
                match_case: checked(dlg, ID_MC),
                regex,
                mode,
            })
        }
        1 => {
            let sel = SendMessageW(ctl(dlg, ID_CASE_BASE), SEG_GETSEL, None, None).0;
            Some(RenameOp::Case {
                scope,
                mode: [
                    CaseMode::Upper,
                    CaseMode::Title,
                    CaseMode::Sentence,
                    CaseMode::Lower,
                ][(sel.max(0) as usize).min(3)], // 세그 순서 = PF "AB CD|Ab Cd|Ab cd|ab cd"
            })
        }
        2 => {
            let text = get_text(ctl(dlg, ID_INS_TEXT));
            (!text.is_empty()).then(|| RenameOp::Insert {
                scope,
                text,
                at: at_of(dlg, ID_INS_OFF, ID_INS_DIR),
            })
        }
        3 => Some(RenameOp::Number {
            scope,
            spec: NumberSpec {
                start: SendMessageW(ctl(dlg, ID_NUM_START), SPIN_GETVAL, None, None).0 as i64,
                step: SendMessageW(ctl(dlg, ID_NUM_STEP), SPIN_GETVAL, None, None).0 as i64,
                pad: (SendMessageW(ctl(dlg, ID_NUM_PAD), SPIN_GETVAL, None, None).0 as i64)
                    .clamp(1, 12) as usize,
                at: at_of(dlg, ID_NUM_OFF, ID_NUM_DIR),
                prefix: get_text(ctl(dlg, ID_NUM_WPRE)),
                suffix: get_text(ctl(dlg, ID_NUM_WSUF)),
            },
        }),
        4 => {
            let format = get_text(ctl(dlg, ID_DT_FMT));
            let format = if format.trim().is_empty() {
                "yyyy-MM-dd".to_string()
            } else {
                format
            };
            let kind_i = SendMessageW(ctl(dlg, ID_DT_KIND), NXCB_GETSEL, None, None).0;
            Some(RenameOp::Date {
                scope,
                spec: DateSpec {
                    kind: if kind_i == 1 {
                        DateKind::Created
                    } else {
                        DateKind::Modified
                    },
                    format,
                    at: at_of(dlg, ID_DT_OFF, ID_DT_DIR),
                    prefix: get_text(ctl(dlg, ID_DT_PRE)),
                    suffix: get_text(ctl(dlg, ID_DT_SUF)),
                },
            })
        }
        5 => {
            let len = num_of(dlg, ID_MV_LEN, 0).max(0) as usize;
            (len > 0).then(|| RenameOp::Move {
                start: num_of(dlg, ID_MV_START, 1).max(1) as usize,
                len,
                to_front: checked(dlg, ID_MV_FRONT),
            })
        }
        _ => {
            let to = get_text(ctl(dlg, ID_EXT_TO));
            let from = get_text(ctl(dlg, ID_EXT_FROM));
            let strip = |s: String| s.trim().trim_start_matches('.').to_string();
            (!to.trim().is_empty() || !from.trim().is_empty()).then(|| RenameOp::ChangeExt {
                from: strip(from),
                to: strip(to),
            })
        }
    }
}

/// 스코프 표기(Name = 생략 — v1 시각과 동일).
fn scope_tag(scope: Scope) -> String {
    match scope {
        Scope::Name => String::new(),
        Scope::NameExt => format!(" [{}]", tr("bulk.scope.nameext")),
        Scope::Ext => format!(" [{}]", tr("bulk.scope.ext")),
        Scope::ExtDot => format!(" [{}]", tr("bulk.scope.extdot")),
    }
}

/// 위치 표기 — "@N앞/뒤"(모서리 0은 기존 앞/뒤 라벨).
fn at_tag(at: InsertAt) -> String {
    let dir = tr(if at.from_end {
        "bulk.posSuffix"
    } else {
        "bulk.posPrefix"
    });
    if at.offset == 0 {
        format!("({dir})")
    } else {
        format!("(@{} {dir})", at.offset)
    }
}

/// 파이프라인 목록 한 줄 표기.
fn op_label(op: &RenameOp) -> String {
    match op {
        RenameOp::Replace {
            scope,
            find,
            with,
            regex,
            mode,
            ..
        } => format!(
            "{}{} \"{find}\" → \"{with}\"{}{}",
            tr("bulk.kind.replace"),
            if *regex { " (regex)" } else { "" },
            match mode {
                ReplaceMode::All => String::new(),
                ReplaceMode::First => format!(" ({})", tr("bulk.mode.first")),
                ReplaceMode::Last => format!(" ({})", tr("bulk.mode.last")),
                ReplaceMode::Entire => format!(" ({})", tr("bulk.mode.entire")),
            },
            scope_tag(*scope)
        ),
        RenameOp::Case { scope, mode } => format!(
            "{}: {}{}",
            tr("bulk.kind.case"),
            tr(match mode {
                CaseMode::Upper => "bulk.case.upper",
                CaseMode::Lower => "bulk.case.lower",
                CaseMode::Title => "bulk.case.title",
                CaseMode::Sentence => "bulk.case.sentence",
            }),
            scope_tag(*scope)
        ),
        RenameOp::Insert { scope, text, at } => format!(
            "{} \"{text}\" {}{}",
            tr("bulk.kind.insert"),
            at_tag(*at),
            scope_tag(*scope)
        ),
        RenameOp::Number { scope, spec } => format!(
            "{} {}+{}×{} {}{}{}",
            tr("bulk.kind.number"),
            spec.start,
            spec.step,
            spec.pad,
            at_tag(spec.at),
            if spec.prefix.is_empty() && spec.suffix.is_empty() {
                String::new()
            } else {
                format!(" \"{}n{}\"", spec.prefix, spec.suffix)
            },
            scope_tag(*scope)
        ),
        RenameOp::Date { scope, spec } => format!(
            "{} {} {} {}{}",
            tr("bulk.kind.date"),
            tr(match spec.kind {
                DateKind::Modified => "bulk.date.modified",
                DateKind::Created => "bulk.date.created",
            }),
            spec.format,
            at_tag(spec.at),
            scope_tag(*scope)
        ),
        RenameOp::Move {
            start,
            len,
            to_front,
        } => format!(
            "{} {start}..{} → {}",
            tr("bulk.kind.move"),
            start + len.saturating_sub(1),
            tr(if *to_front {
                "bulk.destFront"
            } else {
                "bulk.destEnd"
            })
        ),
        RenameOp::ChangeExt { from, to } => format!(
            "{} .{} → .{}",
            tr("bulk.kind.ext"),
            if from.is_empty() { "*" } else { from },
            to
        ),
    }
}

fn conflict_label(c: Conflict) -> String {
    match c {
        Conflict::None => String::new(),
        Conflict::Empty => tr("bulk.conflict.empty"),
        Conflict::Invalid => tr("bulk.conflict.invalid"),
        Conflict::Duplicate => tr("bulk.conflict.dup"),
        Conflict::Exists => tr("bulk.conflict.exists"),
    }
}

unsafe fn lb_add(list: HWND, line: &str) {
    let w = windows::core::HSTRING::from(line);
    SendMessageW(list, LB_ADDSTRING, None, Some(LPARAM(w.as_ptr() as isize)));
}

/// 파이프라인 목록 갱신(선택 유지).
unsafe fn refresh_pipe(st: &BrState, select: Option<usize>) {
    SendMessageW(st.pipe, LB_RESETCONTENT, None, None);
    for (i, op) in st.ops.iter().enumerate() {
        lb_add(st.pipe, &format!("{}. {}", i + 1, op_label(op)));
    }
    if let Some(i) = select {
        SendMessageW(st.pipe, LB_SETCURSEL, Some(WPARAM(i)), None);
    }
}

/// 미리보기·충돌 재계산 → 우측 목록·오류 표시·[적용] 활성 판정.
unsafe fn refresh_preview(st: &mut BrState) {
    // 정규식 검증 — 오류는 상단 STATIC에 (블록 순번) 메시지
    let err = validate(&st.ops).err();
    set_text(
        st.err,
        &err.as_ref()
            .map(|(i, m)| format!("⚠ #{}: {m}", i + 1))
            .unwrap_or_default(),
    );
    let inputs: Vec<RenameInput> = st
        .items
        .iter()
        .map(|(_, n, d, m, c)| RenameInput {
            name: n.clone(),
            is_dir: *d,
            modified_ms: *m,
            created_ms: *c,
        })
        .collect();
    let new_names = preview(&inputs, &st.ops, st.tz_min);
    let triples: Vec<(String, String, String)> = st
        .items
        .iter()
        .zip(&new_names)
        .map(|((p, o, _, _, _), n)| (p.clone(), o.clone(), n.clone()))
        .collect();
    let confs = conflicts(&triples, &|parent, name| {
        Path::new(parent).join(name).exists()
    });
    SendMessageW(st.prev, LB_RESETCONTENT, None, None);
    let mut changed = 0usize;
    for (i, (_, old, new)) in triples.iter().enumerate() {
        // 변경 항목 = ✓ 마커·무변경 = 들여쓰기 유지(PF 미리보기 규약 — v2)
        let line = if confs[i] != Conflict::None {
            format!("⚠ {old} → {new} ({})", conflict_label(confs[i]))
        } else if new != old {
            changed += 1;
            format!("✓ {old} → {new}")
        } else {
            format!("   {old}")
        };
        lb_add(st.prev, &line);
    }
    // "N개 항목이 변경됩니다"(PF 카운트 — v2)
    set_text(
        st.count,
        &crate::i18n::trf("bulk.count", &[&changed.to_string()]),
    );
    let ok = !st.ops.is_empty()
        && changed > 0
        && err.is_none()
        && confs.iter().all(|c| *c == Conflict::None);
    let _ = EnableWindow(st.apply, ok);
}

/// 종류 패널 전환(콤보 선택) — 해당 종류 컨트롤만 표시.
/// 스코프 droplist는 Move/Ext(자체 대상 고정)에서 숨김(v2).
unsafe fn show_kind(st: &BrState, kind: usize) {
    for (i, panel) in st.panels.iter().enumerate() {
        for h in panel {
            let _ = ShowWindow(*h, if i == kind { SW_SHOW } else { SW_HIDE });
        }
    }
    let _ = ShowWindow(st.scope, if kind >= 5 { SW_HIDE } else { SW_SHOW });
}

/// 프리셋 콤보 재적재(`data\renames\*.cfg`).
unsafe fn refresh_presets(st: &BrState) {
    SendMessageW(st.preset_combo, CB_RESETCONTENT, None, None);
    if let Ok(rd) = std::fs::read_dir(preset_dir()) {
        let mut names: Vec<String> = rd
            .flatten()
            .filter_map(|e| {
                let n = e.file_name().to_string_lossy().into_owned();
                n.strip_suffix(".cfg").map(str::to_string)
            })
            .collect();
        names.sort();
        for n in names.iter().take(64) {
            let w = windows::core::HSTRING::from(n.as_str());
            SendMessageW(
                st.preset_combo,
                CB_ADDSTRING,
                None,
                Some(LPARAM(w.as_ptr() as isize)),
            );
        }
    }
}

unsafe fn combo_sel_text(combo: HWND) -> Option<String> {
    let idx = SendMessageW(combo, CB_GETCURSEL, None, None).0;
    if idx < 0 {
        return None;
    }
    let len = SendMessageW(combo, CB_GETLBTEXTLEN, Some(WPARAM(idx as usize)), None).0;
    if len <= 0 {
        return None;
    }
    let mut buf = vec![0u16; len as usize + 1];
    let got = SendMessageW(
        combo,
        CB_GETLBTEXT,
        Some(WPARAM(idx as usize)),
        Some(LPARAM(buf.as_mut_ptr() as isize)),
    )
    .0;
    (got > 0).then(|| String::from_utf16_lossy(&buf[..got as usize]))
}

unsafe extern "system" fn br_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let st = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut BrState;
    if st.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    match msg {
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as u32;
            let notify = ((wparam.0 >> 16) & 0xFFFF) as u32;
            let sel = || SendMessageW((*st).pipe, LB_GETCURSEL, None, None).0;
            match (id, notify) {
                (ID_RX, 0) => {
                    // 정규식 체크 = Mode 숨김(PF 규약 — 정규식은 항상 All)
                    let rx = checked(hwnd, ID_RX);
                    let _ = ShowWindow(ctl(hwnd, ID_RX_MODE), if rx { SW_HIDE } else { SW_SHOW });
                }
                (ID_KIND, 1 /* CBN_SELCHANGE */) => {
                    let k = SendMessageW(ctl(hwnd, ID_KIND), CB_GETCURSEL, None, None)
                        .0
                        .max(0);
                    show_kind(&*st, k as usize);
                }
                (ID_ADD, 0) => {
                    let k = SendMessageW(ctl(hwnd, ID_KIND), CB_GETCURSEL, None, None)
                        .0
                        .max(0);
                    if let Some(op) = op_from_form(hwnd, k as usize) {
                        (*st).ops.push(op);
                        refresh_pipe(&*st, Some((*st).ops.len() - 1));
                        refresh_preview(&mut *st);
                    }
                }
                (ID_DEL, 0) => {
                    let i = sel();
                    if i >= 0 && (i as usize) < (*st).ops.len() {
                        (*st).ops.remove(i as usize);
                        let n = (*st).ops.len();
                        refresh_pipe(&*st, (n > 0).then(|| (i as usize).min(n - 1)));
                        refresh_preview(&mut *st);
                    }
                }
                (ID_UP, 0) | (ID_DOWN, 0) => {
                    let i = sel();
                    let j = if id == ID_UP { i - 1 } else { i + 1 };
                    if i >= 0 && j >= 0 && (j as usize) < (*st).ops.len() {
                        (*st).ops.swap(i as usize, j as usize);
                        refresh_pipe(&*st, Some(j as usize));
                        refresh_preview(&mut *st);
                    }
                }
                (ID_PRESET_SAVE, 0) => {
                    // 이름 정제(금지 문자 제거) — 빈 이름·빈 파이프라인은 무시
                    let name: String = get_text(ctl(hwnd, ID_PRESET_NAME))
                        .chars()
                        .filter(|c| !"<>:\"/\\|?*".contains(*c))
                        .collect();
                    let name = name.trim().to_string();
                    if !name.is_empty() && !(*st).ops.is_empty() {
                        let _ = crate::config::save(
                            &preset_dir(),
                            &format!("{name}.cfg"),
                            &serialize_ops(&(*st).ops),
                        );
                        refresh_presets(&*st);
                    }
                }
                (ID_PRESET_LOAD, 0) => {
                    if let Some(name) = combo_sel_text((*st).preset_combo) {
                        if let Some(text) =
                            crate::config::load(&preset_dir(), &format!("{name}.cfg"))
                        {
                            (*st).ops = parse_ops(&text);
                            refresh_pipe(&*st, None);
                            refresh_preview(&mut *st);
                        }
                    }
                }
                (ID_APPLY, 0) => {
                    // 확정 — 충돌 0·검증 통과는 refresh_preview가 보장([적용] 활성 조건)
                    let inputs: Vec<RenameInput> = (*st)
                        .items
                        .iter()
                        .map(|(_, n, d, m, c)| RenameInput {
                            name: n.clone(),
                            is_dir: *d,
                            modified_ms: *m,
                            created_ms: *c,
                        })
                        .collect();
                    let new_names = preview(&inputs, &(*st).ops, (*st).tz_min);
                    let out: Vec<(String, String, String)> = (*st)
                        .items
                        .iter()
                        .zip(&new_names)
                        .filter(|((_, old, _, _, _), new)| *new != old)
                        .map(|((p, o, _, _, _), n)| (p.clone(), o.clone(), n.clone()))
                        .collect();
                    (*st).result = Some(out);
                    let _ = DestroyWindow(hwnd);
                }
                (ID_CANCEL, 0) => {
                    let _ = DestroyWindow(hwnd);
                }
                _ => {}
            }
            LRESULT(0)
        }
        // 네이티브 라이트 창(prefs.rs 동일) — 라벨·체크박스 배경을 창 배경과 일치
        m if m == WM_CTLCOLORSTATIC || m == WM_CTLCOLORBTN => {
            let hdc = windows::Win32::Graphics::Gdi::HDC(wparam.0 as *mut core::ffi::c_void);
            SetBkMode(hdc, TRANSPARENT);
            LRESULT(GetSysColorBrush(COLOR_WINDOW).0 as isize)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// 다이얼로그 표시(모달) — `targets` = 선택 항목(전체 경로, 폴더 여부 — 선택 순서 = 연번).
/// 반환 = 적용 확정된 (전체 경로, 새 이름) 목록(취소·무변경 = None).
///
/// # Safety
/// UI 스레드에서 호출(모달 루프 동안 wndproc 재진입 — 호출자는 State 참조를 끊을 것).
pub unsafe fn show(
    owner: HWND,
    targets: &[(PathBuf, bool)],
    font_spec: &DlgFont,
    tz_min: i32,
) -> Option<Vec<(PathBuf, String)>> {
    if targets.is_empty() {
        return None;
    }
    REGISTER.call_once(|| {
        let wc = WNDCLASSW {
            lpszClassName: CLASS,
            lpfnWndProc: Some(br_proc),
            hInstance: windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
                .unwrap_or_default()
                .into(),
            hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as isize as *mut core::ffi::c_void),
            hCursor: windows::Win32::UI::WindowsAndMessaging::LoadCursorW(
                None,
                windows::Win32::UI::WindowsAndMessaging::IDC_ARROW,
            )
            .unwrap_or_default(),
            hIcon: crate::icon::load(32).unwrap_or_default(),
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);
    });
    // 수정/생성 시각(v2 Date용 — 실패 = 0: Date 동작이 무변경으로 격리)
    let ms_of = |t: std::io::Result<std::time::SystemTime>| -> i64 {
        t.ok()
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    };
    let items: Vec<(String, String, bool, i64, i64)> = targets
        .iter()
        .filter_map(|(p, d)| {
            let meta = std::fs::metadata(p).ok();
            let (m, c) = meta
                .map(|md| (ms_of(md.modified()), ms_of(md.created())))
                .unwrap_or((0, 0));
            Some((
                p.parent()?.to_string_lossy().into_owned(),
                p.file_name()?.to_string_lossy().into_owned(),
                *d,
                m,
                c,
            ))
        })
        .collect();
    if items.is_empty() {
        return None;
    }

    let font = crate::dialog::make_font_pub(owner, font_spec);
    let mut win = RECT {
        right: CLIENT_W,
        bottom: CLIENT_H,
        ..Default::default()
    };
    let _ = AdjustWindowRectEx(&mut win, STYLE, false, WINDOW_EX_STYLE(0x00000001));
    let (w_, h_) = (win.right - win.left, win.bottom - win.top);
    let mut orc = RECT::default();
    let _ = windows::Win32::UI::WindowsAndMessaging::GetWindowRect(owner, &mut orc);
    let (cx, cy) = (
        orc.left + ((orc.right - orc.left) - w_) / 2,
        orc.top + ((orc.bottom - orc.top) - h_) / 2,
    );
    let title = windows::core::HSTRING::from(tr("bulk.title"));
    let Ok(dlg) = CreateWindowExW(
        WINDOW_EX_STYLE(0x00000001),
        CLASS,
        PCWSTR(title.as_ptr()),
        STYLE | WS_VISIBLE,
        cx,
        cy,
        w_,
        h_,
        Some(owner),
        None,
        None,
        None,
    ) else {
        let _ = DeleteObject(font.into());
        return None;
    };

    let x = PAD;
    let ed = (WS_BORDER | WS_TABSTOP).0 | ES_AUTOHSCROLL as u32;
    let ed_num = ed | ES_NUMBER as u32;
    let rb = WS_TABSTOP.0 | BS_AUTORADIOBUTTON as u32;
    let cb = WS_TABSTOP.0 | BS_AUTOCHECKBOX as u32;

    // ── 동작 종류 콤보 + [블록 추가] ──
    let combo = mk(
        dlg,
        font,
        w!("COMBOBOX"),
        "",
        (WS_TABSTOP | WS_VSCROLL).0 | 0x0003, /* CBS_DROPDOWNLIST */
        x,
        PAD,
        FORM_W - 90,
        200,
        ID_KIND,
    );
    for key in KINDS {
        let w = windows::core::HSTRING::from(tr(key));
        SendMessageW(combo, CB_ADDSTRING, None, Some(LPARAM(w.as_ptr() as isize)));
    }
    SendMessageW(combo, CB_SETCURSEL, Some(WPARAM(0)), None);
    mk(
        dlg,
        font,
        w!("BUTTON"),
        &tr("bulk.add"),
        WS_TABSTOP.0 | WS_GROUP.0,
        x + FORM_W - 84,
        PAD,
        84,
        24,
        ID_ADD,
    );

    // ── 공통 스코프(v2 — PF Apply to): 종류 콤보 아래·패널 위 ──
    let style2 = Style::default();
    let scope_items = [
        tr("bulk.scope.name"),
        tr("bulk.scope.nameext"),
        tr("bulk.scope.ext"),
        tr("bulk.scope.extdot"),
    ];
    let scope_refs: Vec<&str> = scope_items.iter().map(String::as_str).collect();
    let scope = crate::ctl::combobox::create(
        dlg,
        x,
        PAD + 30,
        FORM_W,
        24,
        ID_SCOPE,
        font,
        &scope_refs,
        0,
        style2,
    );

    // ── 종류별 파라미터 패널(겹침 배치 — 콤보 선택으로 스왑) ──
    let py = PAD + 62;
    let half = (FORM_W - 6) / 2;
    let third = (FORM_W - 12) / 3;
    let mut panels: [Vec<HWND>; 7] = Default::default();
    // 0: 치환(찾기/바꾸기/대소문자/정규식)
    {
        let f = mk(
            dlg,
            font,
            w!("EDIT"),
            "",
            ed | WS_GROUP.0,
            x,
            py,
            FORM_W,
            22,
            ID_FIND,
        );
        cue(f, &tr("bulk.find"));
        let wch = mk(
            dlg,
            font,
            w!("EDIT"),
            "",
            ed,
            x,
            py + 26,
            FORM_W,
            22,
            ID_WITH,
        );
        cue(wch, &tr("bulk.with"));
        let mc = mk(
            dlg,
            font,
            w!("BUTTON"),
            &tr("bulk.matchCase"),
            cb,
            x,
            py + 52,
            half,
            20,
            ID_MC,
        );
        let rx = mk(
            dlg,
            font,
            w!("BUTTON"),
            &tr("bulk.regex"),
            cb,
            x + half + 6,
            py + 52,
            half,
            20,
            ID_RX,
        );
        // 치환 Mode(v2 — 모든/첫/마지막/전체. 정규식 체크 시 숨김 — PF 규약)
        let mode_items = [
            tr("bulk.mode.all"),
            tr("bulk.mode.first"),
            tr("bulk.mode.last"),
            tr("bulk.mode.entire"),
        ];
        let mode_refs: Vec<&str> = mode_items.iter().map(String::as_str).collect();
        let mode = crate::ctl::combobox::create(
            dlg,
            x,
            py + 76,
            FORM_W,
            24,
            ID_RX_MODE,
            font,
            &mode_refs,
            0,
            style2,
        );
        panels[0] = vec![f, wch, mc, rx, mode];
    }
    // 1: 대소문자 — ctl::segmented(PF 결과 표기 라벨 "AB CD|Ab Cd|Ab cd|ab cd")
    {
        let seg = crate::ctl::segmented::create(
            dlg,
            x,
            py,
            FORM_W,
            26,
            ID_CASE_BASE,
            font,
            &["AB CD", "Ab Cd", "Ab cd", "ab cd"],
            0,
            crate::ctl::segmented::SegOpts::default(),
            style2,
        );
        panels[1] = vec![seg];
    }
    // 2: 삽입(텍스트·앞/뒤)
    {
        let t = mk(
            dlg,
            font,
            w!("EDIT"),
            "",
            ed | WS_GROUP.0,
            x,
            py,
            FORM_W,
            22,
            ID_INS_TEXT,
        );
        cue(t, &tr("bulk.insert"));
        // 위치(v2 — PF Position): 오프셋 spin + 앞/뒤 segmented(기본 뒤·초과 클램프)
        let off = crate::ctl::spin::create(
            dlg,
            x,
            py + 26,
            half,
            24,
            ID_INS_OFF,
            font,
            0,
            0,
            999,
            style2,
        );
        let dir = crate::ctl::segmented::create(
            dlg,
            x + half + 6,
            py + 26,
            half,
            24,
            ID_INS_DIR,
            font,
            &[&tr("bulk.dirStart"), &tr("bulk.dirEnd")],
            1,
            crate::ctl::segmented::SegOpts::default(),
            style2,
        );
        panels[2] = vec![t, off, dir];
    }
    // 3: 연번(시작/증가/자릿수 spin·위치·감싸기 — v2)
    {
        let mut v = Vec::new();
        for (i, (id, init)) in [(ID_NUM_START, 1i64), (ID_NUM_STEP, 1), (ID_NUM_PAD, 3)]
            .iter()
            .enumerate()
        {
            let cx2 = x + i as i32 * (third + 6);
            let (lo, hi) = if *id == ID_NUM_PAD {
                (1, 12)
            } else {
                (-9999, 9999)
            };
            v.push(crate::ctl::spin::create(
                dlg, cx2, py, third, 24, *id, font, *init, lo, hi, style2,
            ));
        }
        let off = crate::ctl::spin::create(
            dlg,
            x,
            py + 28,
            half,
            24,
            ID_NUM_OFF,
            font,
            0,
            0,
            999,
            style2,
        );
        let dir = crate::ctl::segmented::create(
            dlg,
            x + half + 6,
            py + 28,
            half,
            24,
            ID_NUM_DIR,
            font,
            &[&tr("bulk.dirStart"), &tr("bulk.dirEnd")],
            1,
            crate::ctl::segmented::SegOpts::default(),
            style2,
        );
        let wp = mk(
            dlg,
            font,
            w!("EDIT"),
            "",
            ed,
            x,
            py + 56,
            half,
            22,
            ID_NUM_WPRE,
        );
        cue(wp, &tr("bulk.wrapPre"));
        let ws2 = mk(
            dlg,
            font,
            w!("EDIT"),
            "",
            ed,
            x + half + 6,
            py + 56,
            half,
            22,
            ID_NUM_WSUF,
        );
        cue(ws2, &tr("bulk.wrapSuf"));
        v.extend([off, dir, wp, ws2]);
        panels[3] = v;
    }
    // 4: 날짜(v2 신설 — PF Add Date: 원천·토큰 포맷·위치·감싸기)
    {
        let kind_items = [tr("bulk.date.modified"), tr("bulk.date.created")];
        let kind_refs: Vec<&str> = kind_items.iter().map(String::as_str).collect();
        let dk = crate::ctl::combobox::create(
            dlg, x, py, half, 24, ID_DT_KIND, font, &kind_refs, 0, style2,
        );
        let fmt = mk(
            dlg,
            font,
            w!("EDIT"),
            "yyyy-MM-dd",
            ed,
            x + half + 6,
            py,
            half,
            22,
            ID_DT_FMT,
        );
        cue(fmt, &tr("bulk.date.fmt"));
        let off = crate::ctl::spin::create(
            dlg,
            x,
            py + 28,
            half,
            24,
            ID_DT_OFF,
            font,
            0,
            0,
            999,
            style2,
        );
        let dir = crate::ctl::segmented::create(
            dlg,
            x + half + 6,
            py + 28,
            half,
            24,
            ID_DT_DIR,
            font,
            &[&tr("bulk.dirStart"), &tr("bulk.dirEnd")],
            1,
            crate::ctl::segmented::SegOpts::default(),
            style2,
        );
        let dp = mk(
            dlg,
            font,
            w!("EDIT"),
            "",
            ed,
            x,
            py + 56,
            half,
            22,
            ID_DT_PRE,
        );
        cue(dp, &tr("bulk.wrapPre"));
        let ds2 = mk(
            dlg,
            font,
            w!("EDIT"),
            "",
            ed,
            x + half + 6,
            py + 56,
            half,
            22,
            ID_DT_SUF,
        );
        cue(ds2, &tr("bulk.wrapSuf"));
        panels[4] = vec![dk, fmt, off, dir, dp, ds2];
    }
    // 4: 구간 이동(시작 위치/길이·맨 앞/맨 뒤)
    {
        let st_e = mk(
            dlg,
            font,
            w!("EDIT"),
            "1",
            ed_num | WS_GROUP.0,
            x,
            py,
            half,
            22,
            ID_MV_START,
        );
        cue(st_e, &tr("bulk.moveStart"));
        let ln_e = mk(
            dlg,
            font,
            w!("EDIT"),
            "2",
            ed_num,
            x + half + 6,
            py,
            half,
            22,
            ID_MV_LEN,
        );
        cue(ln_e, &tr("bulk.moveLen"));
        let f = mk(
            dlg,
            font,
            w!("BUTTON"),
            &tr("bulk.destFront"),
            rb | WS_GROUP.0,
            x,
            py + 26,
            half,
            20,
            ID_MV_FRONT,
        );
        let e = mk(
            dlg,
            font,
            w!("BUTTON"),
            &tr("bulk.destEnd"),
            rb,
            x + half + 6,
            py + 26,
            half,
            20,
            ID_MV_END,
        );
        set_check(dlg, ID_MV_END, true);
        panels[5] = vec![st_e, ln_e, f, e];
    }
    // 5: 확장자 변경(기존/새)
    {
        let f = mk(
            dlg,
            font,
            w!("EDIT"),
            "",
            ed | WS_GROUP.0,
            x,
            py,
            half,
            22,
            ID_EXT_FROM,
        );
        cue(f, &tr("bulk.extFrom"));
        let t = mk(
            dlg,
            font,
            w!("EDIT"),
            "",
            ed,
            x + half + 6,
            py,
            half,
            22,
            ID_EXT_TO,
        );
        cue(t, &tr("bulk.extTo"));
        panels[6] = vec![f, t];
    }

    // ── 파이프라인 목록 + 재배치 ──
    let ly = py + 110; // v2 — 파라미터 4행
    mk(
        dlg,
        font,
        w!("STATIC"),
        &tr("bulk.pipeline"),
        WS_GROUP.0,
        x,
        ly,
        FORM_W,
        18,
        0,
    );
    let pipe = mk(
        dlg,
        font,
        w!("LISTBOX"),
        "",
        (WS_BORDER | WS_VSCROLL | WS_TABSTOP).0 | 0x0001 /* LBS_NOTIFY */ | 0x0040, /* LBS_NOINTEGRALHEIGHT */
        x,
        ly + 20,
        FORM_W,
        170,
        ID_PIPE,
    );
    let by1 = ly + 20 + 176;
    mk(
        dlg,
        font,
        w!("BUTTON"),
        "▲",
        WS_TABSTOP.0 | WS_GROUP.0,
        x,
        by1,
        third,
        24,
        ID_UP,
    );
    mk(
        dlg,
        font,
        w!("BUTTON"),
        "▼",
        WS_TABSTOP.0,
        x + third + 6,
        by1,
        third,
        24,
        ID_DOWN,
    );
    mk(
        dlg,
        font,
        w!("BUTTON"),
        "✕",
        WS_TABSTOP.0,
        x + (third + 6) * 2,
        by1,
        third,
        24,
        ID_DEL,
    );

    // ── 프리셋 저장/불러오기(docs/25 §3) ──
    let sy = by1 + 34;
    let pn = mk(
        dlg,
        font,
        w!("EDIT"),
        "",
        ed | WS_GROUP.0,
        x,
        sy,
        FORM_W - 90,
        22,
        ID_PRESET_NAME,
    );
    cue(pn, &tr("bulk.preset.name"));
    mk(
        dlg,
        font,
        w!("BUTTON"),
        &tr("bulk.preset.save"),
        WS_TABSTOP.0,
        x + FORM_W - 84,
        sy,
        84,
        22,
        ID_PRESET_SAVE,
    );
    let pc = mk(
        dlg,
        font,
        w!("COMBOBOX"),
        "",
        (WS_TABSTOP | WS_VSCROLL).0 | 0x0003, /* CBS_DROPDOWNLIST */
        x,
        sy + 28,
        FORM_W - 90,
        200,
        ID_PRESET_COMBO,
    );
    mk(
        dlg,
        font,
        w!("BUTTON"),
        &tr("bulk.preset.load"),
        WS_TABSTOP.0,
        x + FORM_W - 84,
        sy + 28,
        84,
        22,
        ID_PRESET_LOAD,
    );

    // ── 하단 확정 버튼 ──
    let by = CLIENT_H - PAD - 26;
    let apply = mk(
        dlg,
        font,
        w!("BUTTON"),
        &tr("bulk.apply"),
        WS_TABSTOP.0 | WS_GROUP.0,
        x,
        by,
        half,
        26,
        ID_APPLY,
    );
    mk(
        dlg,
        font,
        w!("BUTTON"),
        &tr("bulk.cancel"),
        WS_TABSTOP.0,
        x + half + 6,
        by,
        half,
        26,
        ID_CANCEL,
    );

    // ── 우측: 검증 오류 + 미리보기 ──
    let lx = PAD + FORM_W + PAD;
    let err = mk(
        dlg,
        font,
        w!("STATIC"),
        "",
        0,
        lx,
        PAD,
        CLIENT_W - lx - PAD,
        18,
        0,
    );
    let prev = mk(
        dlg,
        font,
        w!("LISTBOX"),
        "",
        (WS_BORDER | WS_VSCROLL).0 | 0x1000 /* LBS_NOSEL */ | 0x0040, /* LBS_NOINTEGRALHEIGHT */
        lx,
        PAD + 22,
        CLIENT_W - lx - PAD,
        CLIENT_H - PAD * 2 - 22 - 24,
        0,
    );
    // 변경 건수(PF "N items will be renamed" — v2)
    let count = mk(
        dlg,
        font,
        w!("STATIC"),
        "",
        0,
        lx,
        CLIENT_H - PAD - 20,
        CLIENT_W - lx - PAD,
        18,
        ID_COUNT,
    );

    let mut state = Box::new(BrState {
        font,
        items,
        tz_min,
        ops: Vec::new(),
        panels,
        scope,
        pipe,
        prev,
        err,
        apply,
        preset_combo: pc,
        count,
        result: None,
    });
    let _ = SendMessageW(scope, NXCB_SETSEL, Some(WPARAM(0)), None);
    SetWindowLongPtrW(dlg, GWLP_USERDATA, &mut *state as *mut BrState as isize);
    show_kind(&state, 0);
    refresh_presets(&state);
    refresh_preview(&mut state);

    let _ = EnableWindow(owner, false);
    let _ = SetForegroundWindow(dlg);
    let mut msg = MSG::default();
    while IsWindow(Some(dlg)).as_bool() && GetMessageW(&mut msg, None, 0, 0).as_bool() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    let _ = EnableWindow(owner, true);
    let _ = SetForegroundWindow(owner);
    let _ = DeleteObject(state.font.into());

    let out: Vec<(PathBuf, String)> = state
        .result
        .take()?
        .into_iter()
        .map(|(parent, old, new)| (Path::new(&parent).join(old), new))
        .collect();
    (!out.is_empty()).then_some(out)
}
