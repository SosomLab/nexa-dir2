//! 일괄 이름변경 다이얼로그(M5-1 → X-23 카드 재편 07-17 — PF 카드 모델):
//! **카드 스택 = 파이프라인** — NxGroupCard 1장 = 동작 1블록(타이틀 = 동작 콤보 +
//! `±`: + = 아래에 카드 추가·− = 해당 카드 삭제·**마지막 1장 삭제 불가**), 카드
//! 폼 편집 = **실시간 미리보기**(우측 — 충돌 ⚠·적용 차단·변경 건수) +
//! **프리셋 저장/불러오기**(`data\renames\*.cfg` — 카드 스택 복원).
//!
//! 순수 로직·직렬화 = [`nexa_ops::batch_rename`], 이 모듈은 Nx 컨트롤 UI만
//! (콤보/체크/글상자/스핀/세그먼트 = ctl — comctl32 비의존·B3 게이트)·자체 모달
//! 루프. 카드 스택 스크롤(카드 다수)·▲▼ 재배치는 후속.

use std::path::{Path, PathBuf};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetSysColorBrush, SetBkMode, COLOR_WINDOW, HBRUSH, HFONT, TRANSPARENT,
};
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetMessageW, GetParent, GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, IsWindow,
    RegisterClassW, SendMessageW, SetForegroundWindow, SetWindowLongPtrW, TranslateMessage,
    GWLP_USERDATA, HMENU, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_CTLCOLORBTN,
    WM_CTLCOLORSTATIC, WM_SETFONT, WNDCLASSW, WS_CAPTION, WS_CHILD, WS_POPUP, WS_SYSMENU,
    WS_VISIBLE,
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
/// 카드 폭(X-23 라벨 열 확정 후 320 — 한 줄 복수 라벨 행[Start/Prefix] 공간).
const FORM_W: i32 = 320;
const CLIENT_W: i32 = 880;
const CLIENT_H: i32 = 620; // v2 — 파라미터 4행·스코프 행 추가분
const STYLE: WINDOW_STYLE = WINDOW_STYLE(WS_POPUP.0 | WS_CAPTION.0 | WS_SYSMENU.0);

// 컨트롤 id — 파이프라인 편집
const ID_KIND: u32 = 1;
/// 적용 스코프(v2 — PF Apply to, ctl::combobox). Move/Ext 종류에선 숨김.
const ID_SCOPE: u32 = 7;
const ID_ADD: u32 = 2;
const ID_DEL: u32 = 5;
// 파라미터(종류별 패널)
const ID_FIND: u32 = 10;
const ID_WITH: u32 = 11;
const ID_MC: u32 = 12;
/// 치환 Mode(모든/첫/마지막/전체 — Replace Text 카드 전용, X-23 분리).
const ID_RX_MODE: u32 = 14;
/// Replace RegEx 카드(X-23 — 치환 텍스트/정규식 분리).
const ID_RXF_MC: u32 = 13;
const ID_RXF_FIND: u32 = 15;
const ID_RXF_WITH: u32 = 16;
/// 날짜 포맷 도움말 ? 버튼(X-23 — ${} 토큰 안내).
const ID_DT_HELP: u32 = 96;
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
const ID_EXT_FROM: u32 = 60;
const ID_EXT_TO: u32 = 61;
/// 미리보기 그리드(NxGrid — 적용 열 토글 통지, 07-18).
const ID_PREV: u32 = 84;
/// 프리셋 `…` 메뉴(07-18 v2 — 프리셋들·구분선·Save/Edit. 이름은 Save 팝업).
const ID_PRESET_MENU: u32 = 71;
const ID_APPLY: u32 = 80;
const ID_CANCEL: u32 = 81;

// user32 컨트롤 메시지(winuser.h)
const EM_SETCUEBANNER: u32 = 0x1501;

/// 동작 종류(카드 타이틀 콤보 순서 — X-23 재편: PF 카드 순서·치환 텍스트/정규식 분리).
/// 날짜(5) = 보류(사용자 07-17 — 새 시안 대기, 기존 폼 유지). 이동/확장자 = 우리 고유.
const KINDS: [&str; 8] = [
    "bulk.kind.replace",
    "bulk.kind.replaceRx",
    "bulk.kind.insert",
    "bulk.kind.case",
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
    /// 파이프라인 캐시(카드 스택에서 파생 — refresh_preview가 갱신).
    ops: Vec<RenameOp>,
    /// **카드 스택 = 파이프라인**(X-23 PF 모델 — 카드 1장 = 동작 1블록,
    /// 위→아래 순서 적용. + = 아래에 카드 추가·− = 해당 카드 삭제,
    /// **마지막 1장은 삭제 불가** — 사용자 확정 07-17).
    cards: Vec<HWND>,
    /// 미리보기 NxGrid(적용/이전/이후 — 07-18).
    prev: HWND,
    /// 행별 적용 제외(그리드 적용 열 체크 해제 — items와 같은 길이).
    excluded: Vec<bool>,
    err: HWND,
    apply: HWND,
    /// 프리셋 `…` 메뉴버튼(항목 갱신 = 재생성 — X-23 PF 시안).
    preset_menu: HWND,
    /// 메뉴 항목 1..에 대응하는 저장된 프리셋 이름(0 = 저장 액션).
    preset_names: Vec<String>,
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

/// 컨트롤 조회 — 직속 자식 우선, 없으면 **카드(컨테이너) 내부까지 탐색**
/// (X-23: 파라미터 컨트롤은 NxGroupCard의 자식 — GetDlgItem은 직속만 본다).
unsafe fn ctl(dlg: HWND, id: u32) -> HWND {
    use windows::Win32::UI::WindowsAndMessaging::{GetDlgItem, GetWindow, GW_CHILD, GW_HWNDNEXT};
    if let Ok(h) = GetDlgItem(Some(dlg), id as i32) {
        return h;
    }
    let mut c = GetWindow(dlg, GW_CHILD).unwrap_or_default();
    while !c.is_invalid() {
        if let Ok(h) = GetDlgItem(Some(c), id as i32) {
            return h;
        }
        c = GetWindow(c, GW_HWNDNEXT).unwrap_or_default();
    }
    HWND::default()
}

/// NxCheckBox 상태(X-23 — user32 BM_GETCHECK 대체).
unsafe fn nx_checked(dlg: HWND, id: u32) -> bool {
    SendMessageW(
        ctl(dlg, id),
        crate::ctl::checkbox::NXCHK_GETCHECK,
        None,
        None,
    )
    .0 != 0
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

/// 현재 파라미터 폼 → 동작 1블록(선택된 종류 기준 — X-23 카드 순서).
/// 유효하지 않으면 None.
unsafe fn op_from_form(dlg: HWND, kind: usize) -> Option<RenameOp> {
    let scope = scope_of(dlg);
    match kind {
        // Replace Text(PF 카드 — Mode·대소문자)
        0 => {
            let find = get_text(ctl(dlg, ID_FIND));
            let mode_i = SendMessageW(ctl(dlg, ID_RX_MODE), NXCB_GETSEL, None, None).0;
            let mode = [
                ReplaceMode::All,
                ReplaceMode::First,
                ReplaceMode::Last,
                ReplaceMode::Entire,
            ][(mode_i.max(0) as usize).min(3)];
            // Entire + 빈 find = 무조건 교체 허용 — 그 외 빈 find는 무효
            (!find.is_empty() || mode == ReplaceMode::Entire).then(|| RenameOp::Replace {
                scope,
                find,
                with: get_text(ctl(dlg, ID_WITH)),
                match_case: nx_checked(dlg, ID_MC),
                regex: false,
                mode,
            })
        }
        // Replace RegEx(별도 카드 — 정규식 = 항상 All, PF 규약)
        1 => {
            let find = get_text(ctl(dlg, ID_RXF_FIND));
            (!find.is_empty()).then(|| RenameOp::Replace {
                scope,
                find,
                with: get_text(ctl(dlg, ID_RXF_WITH)),
                match_case: nx_checked(dlg, ID_RXF_MC),
                regex: true,
                mode: ReplaceMode::All,
            })
        }
        // Insert Text
        2 => {
            let text = get_text(ctl(dlg, ID_INS_TEXT));
            (!text.is_empty()).then(|| RenameOp::Insert {
                scope,
                text,
                at: at_of(dlg, ID_INS_OFF, ID_INS_DIR),
            })
        }
        // Change Case
        3 => {
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
        // Add Number Sequence(Padding = 콤보 프리셋 — 인덱스+1 자릿수)
        4 => Some(RenameOp::Number {
            scope,
            spec: NumberSpec {
                start: SendMessageW(ctl(dlg, ID_NUM_START), SPIN_GETVAL, None, None).0 as i64,
                step: SendMessageW(ctl(dlg, ID_NUM_STEP), SPIN_GETVAL, None, None).0 as i64,
                pad: (SendMessageW(ctl(dlg, ID_NUM_PAD), NXCB_GETSEL, None, None)
                    .0
                    .max(0) as usize)
                    .clamp(0, 5)
                    + 1,
                at: at_of(dlg, ID_NUM_OFF, ID_NUM_DIR),
                prefix: get_text(ctl(dlg, ID_NUM_WPRE)),
                suffix: get_text(ctl(dlg, ID_NUM_WSUF)),
            },
        }),
        // Add Date(포맷 = ${} 텍스트 문법 — 사용자 확정 07-17, 구식 입력 자동 이행)
        5 => {
            let format = get_text(ctl(dlg, ID_DT_FMT));
            let format = if format.trim().is_empty() {
                nexa_ops::batch_rename::DEFAULT_DATE_FORMAT.to_string()
            } else {
                nexa_ops::batch_rename::migrate_date_format(&format)
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
        // 구간 이동(우리 고유)
        6 => {
            let len = SendMessageW(ctl(dlg, ID_MV_LEN), SPIN_GETVAL, None, None)
                .0
                .max(0) as usize;
            (len > 0).then(|| RenameOp::Move {
                start: SendMessageW(ctl(dlg, ID_MV_START), SPIN_GETVAL, None, None)
                    .0
                    .max(1) as usize,
                len,
                to_front: SendMessageW(ctl(dlg, ID_MV_FRONT), SEG_GETSEL, None, None).0 == 0,
            })
        }
        // 확장자 변경(우리 고유)
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

fn conflict_label(c: Conflict) -> String {
    match c {
        Conflict::None => String::new(),
        Conflict::Empty => tr("bulk.conflict.empty"),
        Conflict::Invalid => tr("bulk.conflict.invalid"),
        Conflict::Duplicate => tr("bulk.conflict.dup"),
        Conflict::Exists => tr("bulk.conflict.exists"),
    }
}

/// 카드의 현재 kind(타이틀 콤보 선택).
unsafe fn card_kind(card: HWND) -> usize {
    SendMessageW(ctl(card, ID_KIND), NXCB_GETSEL, None, None)
        .0
        .max(0) as usize
}

/// 카드 스택 → 파이프라인(위→아래 — 무효 폼은 건너뜀).
unsafe fn harvest(st: &BrState) -> Vec<RenameOp> {
    st.cards
        .iter()
        .filter_map(|c| op_from_form(*c, card_kind(*c)))
        .collect()
}

/// 카드 세로 재배치(추가/삭제 후) + − 버튼 활성 동기(마지막 1장 = 삭제 불가).
unsafe fn relayout_cards(st: &BrState) {
    use windows::Win32::UI::WindowsAndMessaging::MoveWindow;
    let one = st.cards.len() <= 1;
    for (i, c) in st.cards.iter().enumerate() {
        let _ = MoveWindow(
            *c,
            PAD,
            PAD + i as i32 * (CARD_H + CARD_GAP),
            FORM_W,
            CARD_H,
            true,
        );
        SendMessageW(
            ctl(*c, ID_DEL),
            crate::ctl::iconbutton::NXIB_SETENABLE,
            Some(WPARAM(usize::from(!one))),
            None,
        );
    }
}

/// 미리보기·충돌 재계산 → 우측 목록·오류 표시·[적용] 활성 판정.
/// 파이프라인 = 카드 스택에서 파생(X-23 — 폼 편집 즉시 반영).
unsafe fn refresh_preview(st: &mut BrState) {
    st.ops = harvest(st);
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
    // 미리보기 그리드(NxGrid — 사용자 확정 07-18): 적용(변경 행만 체크 마크 —
    // 클릭 = 행별 제외) / 이전 / 이후. 충돌 행 = 마크 없음 + 사유 표기.
    let mut changed = 0usize;
    let mut rows = Vec::with_capacity(triples.len());
    for (i, (_, old, new)) in triples.iter().enumerate() {
        let (check, after) = if confs[i] != Conflict::None {
            (None, format!("⚠ {new} ({})", conflict_label(confs[i])))
        } else if new != old {
            let on = !st.excluded.get(i).copied().unwrap_or(false);
            if on {
                changed += 1;
            }
            (Some(on), new.clone())
        } else {
            (None, String::new()) // 무변경 = 이후 빈 칸(변경 없음이 한눈에)
        };
        rows.push(crate::ctl::grid::GridRow {
            check,
            cells: vec![old.clone(), after],
        });
    }
    crate::ctl::grid::set_rows(st.prev, rows);
    // "N개 항목이 변경됩니다"(PF 카운트 — 체크 해제 행 제외)
    set_text(
        st.count,
        &crate::i18n::trf("bulk.count", &[&changed.to_string()]),
    );
    let ok = !st.ops.is_empty()
        && changed > 0
        && err.is_none()
        && confs.iter().all(|c| *c == Conflict::None);
    // NxButton 활성은 내부 상태(NXBTN_SETENABLE) — EnableWindow는 그리기 미반영
    SendMessageW(
        st.apply,
        crate::ctl::button::NXBTN_SETENABLE,
        Some(WPARAM(usize::from(ok))),
        None,
    );
}

/// 동작 → kind 인덱스(카드 콤보 순서 — 프리셋 복원용).
fn kind_index_of(op: &RenameOp) -> usize {
    match op {
        RenameOp::Replace { regex: false, .. } => 0,
        RenameOp::Replace { regex: true, .. } => 1,
        RenameOp::Insert { .. } => 2,
        RenameOp::Case { .. } => 3,
        RenameOp::Number { .. } => 4,
        RenameOp::Date { .. } => 5,
        RenameOp::Move { .. } => 6,
        RenameOp::ChangeExt { .. } => 7,
    }
}

/// 카드 크기(타이틀 34 + 본문 168)·간격.
const CARD_H: i32 = 34 + 168;
const CARD_GAP: i32 = 8;

/// 카드 생성(X-23 PF 모델) — 타이틀 = 동작 콤보 + ±(+ = 아래 카드 추가·
/// − = 이 카드 삭제[마지막 1장 비활성]) + kind 본문. 위치는 relayout_cards가 확정.
unsafe fn make_card(dlg: HWND, font: HFONT, kind: usize) -> HWND {
    let style2 = Style::default();
    let card = crate::ctl::groupcard::create(
        dlg,
        PAD,
        PAD,
        FORM_W,
        0,
        font,
        "",
        crate::ctl::groupcard::GroupCardOpts {
            corner: 8,
            title_h: 34,
            body_h: CARD_H - 34,
        },
        style2,
    );
    let trc = crate::ctl::groupcard::title_rect(card);
    // 타이틀 밴드 위 컨트롤(QA 07-17): behind = 밴드 색 + 필 = 한 단계 진한
    // 회색(밴드와 동색이라 묻히던 문제 — 외곽선과 함께 위계 형성)
    let st_band = Style {
        behind: style2.sel_bg,
        sel_bg: windows::Win32::Foundation::COLORREF(0x00DC_D6D2),
        ..style2
    };
    let ib = crate::ctl::style::font_height(dlg, font).max(10);
    let kind_items: Vec<String> = KINDS.iter().map(|k| tr(k)).collect();
    let kind_refs: Vec<&str> = kind_items.iter().map(String::as_str).collect();
    let cb_h = 24;
    crate::ctl::combobox::create(
        card,
        trc.left + 8,
        trc.top + (trc.bottom - trc.top - cb_h) / 2,
        170,
        cb_h,
        ID_KIND,
        font,
        &kind_refs,
        kind,
        st_band,
    );
    let iy = trc.top + (trc.bottom - trc.top - ib) / 2;
    crate::ctl::iconbutton::create(
        card,
        trc.right - 8 - ib * 2 - 6,
        iy,
        ib,
        ID_ADD,
        font,
        crate::ctl::iconbutton::Icon::Plus,
        true,
        st_band,
    );
    crate::ctl::iconbutton::create(
        card,
        trc.right - 8 - ib,
        iy,
        ib,
        ID_DEL,
        font,
        crate::ctl::iconbutton::Icon::Minus,
        true,
        st_band,
    );
    build_card_body(card, kind, font);
    card
}

/// 카드 본문 재구성(kind 변경·생성·프리셋 복원) — 타이틀(콤보·±) 외 자식을
/// 파괴 후 해당 kind 컨트롤만 생성(패널 스왑 대신 동적 구성 — 카드 N장 대응).
/// 행 구도(PF 시안·사용자 확정): **좌측 정렬 NxLabel 열 + 좌측 정렬 컨트롤 열**,
/// 연번 카드 = 한 줄 복수 쌍(Start value:[spin] Prefix:[box]).
unsafe fn build_card_body(card: HWND, kind: usize, font: HFONT) {
    use windows::Win32::UI::WindowsAndMessaging::{GetWindow, GW_CHILD, GW_HWNDNEXT};
    // 기존 본문 파괴(수집 후 — 파괴 중 열거 금지)
    let mut kill = Vec::new();
    let mut c = GetWindow(card, GW_CHILD).unwrap_or_default();
    while !c.is_invalid() {
        let id = windows::Win32::UI::WindowsAndMessaging::GetDlgCtrlID(c) as u32;
        if !matches!(id, ID_KIND | ID_ADD | ID_DEL) {
            kill.push(c);
        }
        c = GetWindow(c, GW_HWNDNEXT).unwrap_or_default();
    }
    for k in kill {
        let _ = DestroyWindow(k);
    }
    let style2 = Style::default();
    let body = crate::ctl::groupcard::body_rect(card);
    let bx = body.left + 10;
    let bw = (body.right - body.left) - 20;
    let y0 = body.top + 8;
    let row = |k: i32| y0 + 28 * k;
    // 라벨 열 폭 = **현재 언어 라벨 실측 최대치**(i18n 변경에도 정렬 유지 —
    // 사용자 확정 07-17. 전 kind 공통 집합으로 재어 카드 간 열 위치도 일치)
    let measure = |key: &str| crate::ctl::style::text_width(card, font, &tr(key));
    let lbl_w = [
        "bulk.lbl.applyTo",
        "bulk.lbl.mode",
        "bulk.lbl.matchCase",
        "bulk.lbl.find",
        "bulk.lbl.findRx",
        "bulk.lbl.with",
        "bulk.lbl.pos",
        "bulk.lbl.text",
        "bulk.lbl.caseTo",
        "bulk.lbl.padding",
        "bulk.lbl.start",
        "bulk.lbl.step",
        "bulk.lbl.type",
        "bulk.lbl.fmt",
        "bulk.lbl.prefix",
        "bulk.lbl.range",
        "bulk.lbl.dest",
        "bulk.lbl.ext",
    ]
    .iter()
    .map(|k| measure(k))
    .max()
    .unwrap_or(64)
    .clamp(56, 140)
        + 6;
    let lbl2_w = measure("bulk.lbl.prefix")
        .max(measure("bulk.lbl.suffix"))
        .clamp(28, 90)
        + 6;
    // 컨트롤 열(라벨 오른쪽 고정 x — 두 열 모두 좌측 정렬)
    let cx = bx + lbl_w + 6;
    let cw = bw - lbl_w - 6;
    let chalf = (cw - 6) / 2;
    let lbl = |key: &str, x: i32, w: i32, r: i32| {
        crate::ctl::label::create(
            card,
            x,
            row(r),
            w,
            0,
            0,
            font,
            &tr(key),
            // 우측 정렬(사용자 확정 07-17 재개정 — 라벨은 콜론이 컨트롤에 붙는 구도)
            crate::ctl::label::LabelAlign::Right,
            style2,
        );
    };
    // 공통 스코프(PF Apply to) — Move/Ext(자체 대상 고정)에선 생략
    if kind < 6 {
        lbl("bulk.lbl.applyTo", bx, lbl_w, 0);
        let scope_items = [
            tr("bulk.scope.name"),
            tr("bulk.scope.nameext"),
            tr("bulk.scope.ext"),
            tr("bulk.scope.extdot"),
        ];
        let scope_refs: Vec<&str> = scope_items.iter().map(String::as_str).collect();
        crate::ctl::combobox::create(
            card,
            cx,
            row(0),
            cw,
            0,
            ID_SCOPE,
            font,
            &scope_refs,
            0,
            style2,
        );
    }
    match kind {
        // Replace Text(PF 카드 — Mode·대소문자·찾기/바꾸기)
        0 => {
            lbl("bulk.lbl.mode", bx, lbl_w, 1);
            let mode_items = [
                tr("bulk.mode.all"),
                tr("bulk.mode.first"),
                tr("bulk.mode.last"),
                tr("bulk.mode.entire"),
            ];
            let mode_refs: Vec<&str> = mode_items.iter().map(String::as_str).collect();
            crate::ctl::combobox::create(
                card,
                cx,
                row(1),
                cw,
                0,
                ID_RX_MODE,
                font,
                &mode_refs,
                0,
                style2,
            );
            lbl("bulk.lbl.matchCase", bx, lbl_w, 2);
            crate::ctl::checkbox::create(card, cx, row(2), 0, 0, ID_MC, font, "", false, style2);
            lbl("bulk.lbl.find", bx, lbl_w, 3);
            crate::ctl::textbox::create(card, cx, row(3), cw, 0, ID_FIND, font, style2);
            lbl("bulk.lbl.with", bx, lbl_w, 4);
            crate::ctl::textbox::create(card, cx, row(4), cw, 0, ID_WITH, font, style2);
        }
        // Replace RegEx(분리 카드 — 정규식 = 항상 All, PF 규약)
        1 => {
            lbl("bulk.lbl.matchCase", bx, lbl_w, 1);
            crate::ctl::checkbox::create(
                card,
                cx,
                row(1),
                0,
                0,
                ID_RXF_MC,
                font,
                "",
                false,
                style2,
            );
            lbl("bulk.lbl.findRx", bx, lbl_w, 2);
            crate::ctl::textbox::create(card, cx, row(2), cw, 0, ID_RXF_FIND, font, style2);
            lbl("bulk.lbl.with", bx, lbl_w, 3);
            crate::ctl::textbox::create(card, cx, row(3), cw, 0, ID_RXF_WITH, font, style2);
        }
        // Insert Text(Position = spin + →abc/←abc 세그먼트 — PF 시안)
        2 => {
            lbl("bulk.lbl.pos", bx, lbl_w, 1);
            crate::ctl::spin::create(card, cx, row(1), 70, 0, ID_INS_OFF, font, 0, 0, 999, style2);
            crate::ctl::segmented::create(
                card,
                cx + 76,
                row(1),
                cw - 76,
                0,
                ID_INS_DIR,
                font,
                &["→ abc", "← abc"],
                1,
                crate::ctl::segmented::SegOpts::default(),
                style2,
            );
            lbl("bulk.lbl.text", bx, lbl_w, 2);
            crate::ctl::textbox::create(card, cx, row(2), cw, 0, ID_INS_TEXT, font, style2);
        }
        // Change Case — segmented(PF "AB CD|Ab Cd|Ab cd|ab cd")
        3 => {
            lbl("bulk.lbl.caseTo", bx, lbl_w, 1);
            crate::ctl::segmented::create(
                card,
                cx,
                row(1),
                cw,
                0,
                ID_CASE_BASE,
                font,
                &["AB CD", "Ab Cd", "Ab cd", "ab cd"],
                0,
                crate::ctl::segmented::SegOpts::default(),
                style2,
            );
        }
        // Add Number Sequence(PF 카드 — 한 줄 복수 쌍: Start/Prefix·Step/Suffix)
        4 => {
            lbl("bulk.lbl.padding", bx, lbl_w, 1);
            let pad_items = [
                "1, 2, 3, 4…",
                "01, 02, 03…",
                "001, 002, 003…",
                "0001, 0002…",
                "00001…",
                "000001…",
            ];
            crate::ctl::combobox::create(
                card,
                cx,
                row(1),
                cw,
                0,
                ID_NUM_PAD,
                font,
                &pad_items,
                2,
                style2,
            );
            lbl("bulk.lbl.pos", bx, lbl_w, 2);
            crate::ctl::spin::create(card, cx, row(2), 70, 0, ID_NUM_OFF, font, 0, 0, 999, style2);
            crate::ctl::segmented::create(
                card,
                cx + 76,
                row(2),
                cw - 76,
                0,
                ID_NUM_DIR,
                font,
                &["→ abc", "← abc"],
                1,
                crate::ctl::segmented::SegOpts::default(),
                style2,
            );
            // 한 줄 복수 쌍(PF): Start value:[spin]  Prefix:[box]
            let x2 = cx + 76;
            let bw2 = cw - 76 - lbl2_w - 6;
            lbl("bulk.lbl.start", bx, lbl_w, 3);
            crate::ctl::spin::create(
                card,
                cx,
                row(3),
                70,
                0,
                ID_NUM_START,
                font,
                1,
                -9999,
                9999,
                style2,
            );
            lbl("bulk.lbl.prefix", x2, lbl2_w, 3);
            crate::ctl::textbox::create(
                card,
                x2 + lbl2_w + 6,
                row(3),
                bw2,
                0,
                ID_NUM_WPRE,
                font,
                style2,
            );
            lbl("bulk.lbl.step", bx, lbl_w, 4);
            crate::ctl::spin::create(
                card,
                cx,
                row(4),
                70,
                0,
                ID_NUM_STEP,
                font,
                1,
                -9999,
                9999,
                style2,
            );
            lbl("bulk.lbl.suffix", x2, lbl2_w, 4);
            crate::ctl::textbox::create(
                card,
                x2 + lbl2_w + 6,
                row(4),
                bw2,
                0,
                ID_NUM_WSUF,
                font,
                style2,
            );
        }
        // Add Date(PF 카드 + Format = ${} 텍스트·? 도움말 — 사용자 확정 07-17)
        5 => {
            lbl("bulk.lbl.type", bx, lbl_w, 1);
            let kind_items = [tr("bulk.date.modified"), tr("bulk.date.created")];
            let kind_refs: Vec<&str> = kind_items.iter().map(String::as_str).collect();
            crate::ctl::combobox::create(
                card,
                cx,
                row(1),
                cw,
                0,
                ID_DT_KIND,
                font,
                &kind_refs,
                0,
                style2,
            );
            lbl("bulk.lbl.pos", bx, lbl_w, 2);
            crate::ctl::spin::create(card, cx, row(2), 70, 0, ID_DT_OFF, font, 0, 0, 999, style2);
            crate::ctl::segmented::create(
                card,
                cx + 76,
                row(2),
                cw - 76,
                0,
                ID_DT_DIR,
                font,
                &["→ abc", "← abc"],
                1,
                crate::ctl::segmented::SegOpts::default(),
                style2,
            );
            // 한 줄 복수 쌍(PF): Prefix:[box]  Suffix:[box]
            let x2 = cx + 76;
            let bw2 = cw - 76 - lbl2_w - 6;
            lbl("bulk.lbl.prefix", bx, lbl_w, 3);
            crate::ctl::textbox::create(card, cx, row(3), 70, 0, ID_DT_PRE, font, style2);
            lbl("bulk.lbl.suffix", x2, lbl2_w, 3);
            crate::ctl::textbox::create(
                card,
                x2 + lbl2_w + 6,
                row(3),
                bw2,
                0,
                ID_DT_SUF,
                font,
                style2,
            );
            lbl("bulk.lbl.fmt", bx, lbl_w, 4);
            let ib = crate::ctl::style::font_height(card, font).max(10);
            let fmt = crate::ctl::textbox::create(
                card,
                cx,
                row(4),
                cw - ib - 6,
                0,
                ID_DT_FMT,
                font,
                style2,
            );
            set_text(fmt, nexa_ops::batch_rename::DEFAULT_DATE_FORMAT);
            let auto_h = crate::ctl::style::font_height(card, font) + 8;
            crate::ctl::iconbutton::create(
                card,
                cx + cw - ib,
                row(4) + (auto_h - ib) / 2,
                ib,
                ID_DT_HELP,
                font,
                crate::ctl::iconbutton::Icon::Help,
                true,
                style2,
            );
        }
        // 구간 이동(우리 고유 — 구간:[시작][길이]·대상 세그먼트)
        6 => {
            lbl("bulk.lbl.range", bx, lbl_w, 1);
            crate::ctl::spin::create(
                card,
                cx,
                row(1),
                70,
                0,
                ID_MV_START,
                font,
                1,
                1,
                999,
                style2,
            );
            crate::ctl::spin::create(
                card,
                cx + 76,
                row(1),
                70,
                0,
                ID_MV_LEN,
                font,
                2,
                0,
                999,
                style2,
            );
            lbl("bulk.lbl.dest", bx, lbl_w, 2);
            crate::ctl::segmented::create(
                card,
                cx,
                row(2),
                cw,
                0,
                ID_MV_FRONT,
                font,
                &[&tr("bulk.destFront"), &tr("bulk.destEnd")],
                1,
                crate::ctl::segmented::SegOpts::default(),
                style2,
            );
        }
        // 확장자 변경(우리 고유 — 확장자:[기존][새])
        _ => {
            lbl("bulk.lbl.ext", bx, lbl_w, 1);
            let f =
                crate::ctl::textbox::create(card, cx, row(1), chalf, 0, ID_EXT_FROM, font, style2);
            cue(f, &tr("bulk.extFrom"));
            let t = crate::ctl::textbox::create(
                card,
                cx + chalf + 6,
                row(1),
                cw - chalf - 6,
                0,
                ID_EXT_TO,
                font,
                style2,
            );
            cue(t, &tr("bulk.extTo"));
        }
    }
}

/// 프리셋 복원 — 동작 값을 카드 폼에 주입(kind 본문이 이미 구성된 상태 전제).
unsafe fn set_form_from_op(card: HWND, op: &RenameOp) {
    let set_cb = |id: u32, i: usize| {
        SendMessageW(ctl(card, id), NXCB_SETSEL, Some(WPARAM(i)), None);
    };
    let set_seg = |id: u32, i: usize| {
        SendMessageW(
            ctl(card, id),
            crate::ctl::segmented::SEG_SETSEL,
            Some(WPARAM(i)),
            None,
        );
    };
    let set_spin = |id: u32, v: i64| {
        SendMessageW(
            ctl(card, id),
            crate::ctl::spin::SPIN_SETVAL,
            Some(WPARAM(v as usize)),
            None,
        );
    };
    let set_chk = |id: u32, on: bool| {
        SendMessageW(
            ctl(card, id),
            crate::ctl::checkbox::NXCHK_SETCHECK,
            Some(WPARAM(usize::from(on))),
            None,
        );
    };
    let scope_sel = |s: Scope| SCOPES.iter().position(|x| *x == s).unwrap_or(0);
    match op {
        RenameOp::Replace {
            scope,
            find,
            with,
            match_case,
            regex,
            mode,
        } => {
            set_cb(ID_SCOPE, scope_sel(*scope));
            if *regex {
                set_chk(ID_RXF_MC, *match_case);
                set_text(ctl(card, ID_RXF_FIND), find);
                set_text(ctl(card, ID_RXF_WITH), with);
            } else {
                set_cb(
                    ID_RX_MODE,
                    match mode {
                        ReplaceMode::All => 0,
                        ReplaceMode::First => 1,
                        ReplaceMode::Last => 2,
                        ReplaceMode::Entire => 3,
                    },
                );
                set_chk(ID_MC, *match_case);
                set_text(ctl(card, ID_FIND), find);
                set_text(ctl(card, ID_WITH), with);
            }
        }
        RenameOp::Insert { scope, text, at } => {
            set_cb(ID_SCOPE, scope_sel(*scope));
            set_spin(ID_INS_OFF, at.offset as i64);
            set_seg(ID_INS_DIR, usize::from(at.from_end));
            set_text(ctl(card, ID_INS_TEXT), text);
        }
        RenameOp::Case { scope, mode } => {
            set_cb(ID_SCOPE, scope_sel(*scope));
            set_seg(
                ID_CASE_BASE,
                match mode {
                    CaseMode::Upper => 0,
                    CaseMode::Title => 1,
                    CaseMode::Sentence => 2,
                    CaseMode::Lower => 3,
                },
            );
        }
        RenameOp::Number { scope, spec } => {
            set_cb(ID_SCOPE, scope_sel(*scope));
            set_cb(ID_NUM_PAD, spec.pad.clamp(1, 6) - 1);
            set_spin(ID_NUM_OFF, spec.at.offset as i64);
            set_seg(ID_NUM_DIR, usize::from(spec.at.from_end));
            set_spin(ID_NUM_START, spec.start);
            set_spin(ID_NUM_STEP, spec.step);
            set_text(ctl(card, ID_NUM_WPRE), &spec.prefix);
            set_text(ctl(card, ID_NUM_WSUF), &spec.suffix);
        }
        RenameOp::Date { scope, spec } => {
            set_cb(ID_SCOPE, scope_sel(*scope));
            set_cb(
                ID_DT_KIND,
                usize::from(matches!(spec.kind, DateKind::Created)),
            );
            set_spin(ID_DT_OFF, spec.at.offset as i64);
            set_seg(ID_DT_DIR, usize::from(spec.at.from_end));
            set_text(ctl(card, ID_DT_PRE), &spec.prefix);
            set_text(ctl(card, ID_DT_SUF), &spec.suffix);
            set_text(ctl(card, ID_DT_FMT), &spec.format);
        }
        RenameOp::Move {
            start,
            len,
            to_front,
        } => {
            set_spin(ID_MV_START, *start as i64);
            set_spin(ID_MV_LEN, *len as i64);
            set_seg(ID_MV_FRONT, usize::from(!*to_front));
        }
        RenameOp::ChangeExt { from, to } => {
            set_text(ctl(card, ID_EXT_FROM), from);
            set_text(ctl(card, ID_EXT_TO), to);
        }
    }
}

/// 카드 스택을 파이프라인으로 재구성(프리셋 불러오기 — 기존 카드 전부 교체).
unsafe fn rebuild_cards(dlg: HWND, st: &mut BrState, ops: &[RenameOp]) {
    for c in st.cards.drain(..) {
        let _ = DestroyWindow(c);
    }
    let ops: &[RenameOp] = if ops.is_empty() {
        &[] // 빈 프리셋 = 기본 카드 1장
    } else {
        ops
    };
    if ops.is_empty() {
        st.cards.push(make_card(dlg, st.font, 0));
    } else {
        for op in ops {
            let card = make_card(dlg, st.font, kind_index_of(op));
            set_form_from_op(card, op);
            st.cards.push(card);
        }
    }
    relayout_cards(st);
}

/// 저장된 프리셋 이름 목록(`data\renames\*.cfg` — 정렬).
fn preset_names() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(preset_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.strip_suffix(".cfg").map(str::to_string)
        })
        .collect();
    names.sort();
    names.truncate(64);
    names
}

/// 프리셋 `…` 메뉴 재구성(저장/삭제 후·초기) — 순서(사용자 확정 07-18):
/// 저장된 프리셋들 → 구분선 → Save/Edit Renaming Sequence…(NxMenuButton은
/// 항목 불변 규약 — 재생성으로 갱신).
unsafe fn rebuild_preset_menu(dlg: HWND, st: &mut BrState) {
    if !st.preset_menu.is_invalid() {
        let _ = DestroyWindow(st.preset_menu);
    }
    st.preset_names = preset_names();
    let save_label = tr("bulk.preset.saveSeq");
    let edit_label = tr("bulk.preset.editSeq");
    let mut items: Vec<&str> = st.preset_names.iter().map(String::as_str).collect();
    if !items.is_empty() {
        items.push("-"); // 구분선(프리셋 위쪽·고정 액션 아래쪽)
    }
    items.push(save_label.as_str());
    items.push(edit_label.as_str());
    // 하단 좌측(구 [적용] 자리 — 사용자 확정 07-18) [… ⌄] + 오른쪽 건수 라벨
    st.preset_menu = crate::ctl::menubutton::create(
        dlg,
        PAD,
        CLIENT_H - PAD - 26 + 2,
        48,
        0,
        ID_PRESET_MENU,
        st.font,
        &items,
        Style::default(),
    );
}

// ── 프리셋 이름 입력 팝업(Save Renaming Sequence… — 사용자 확정 07-18) ──

const PROMPT_CLASS: PCWSTR = w!("NexaPromptName");
static PROMPT_REGISTER: std::sync::Once = std::sync::Once::new();

struct PromptState {
    edit: HWND,
    result: Option<String>,
}

unsafe extern "system" fn prompt_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let st = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut PromptState;
    match msg {
        WM_COMMAND if !st.is_null() => {
            let id = (wparam.0 & 0xFFFF) as u32;
            let code = ((wparam.0 >> 16) & 0xFFFF) as u32;
            if code == 1 {
                if id == 2 {
                    // 확인 = 이름 확정(빈 값·금지 문자 정제)
                    let name: String = get_text((*st).edit)
                        .chars()
                        .filter(|c| !"<>:\"/\\|?*".contains(*c))
                        .collect();
                    let name = name.trim().to_string();
                    if !name.is_empty() {
                        (*st).result = Some(name);
                    }
                    let _ = DestroyWindow(hwnd);
                } else if id == 3 {
                    let _ = DestroyWindow(hwnd);
                }
            }
            LRESULT(0)
        }
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

/// 파일 목록 그리드 행 높이 동일화(사용자 확정 07-18 — win.rs panel_metrics
/// 고밀도 규약 20px @96dpi와 같은 산식).
unsafe fn file_row_h(hwnd: HWND) -> i32 {
    let dpi = windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd) as i32;
    (20 * dpi.max(96) / 96).max(14)
}

/// 무캐션 라운드 팝업(macOS 시안 07-18) — Win11 DWM 라운드 코너(Win10 = 각).
unsafe fn round_popup(hwnd: HWND) {
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    };
    let pref = DWMWCP_ROUND;
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_WINDOW_CORNER_PREFERENCE,
        &pref as *const _ as *const core::ffi::c_void,
        std::mem::size_of_val(&pref) as u32,
    );
}

/// 버튼 2개(취소·OK)를 우하단 정렬(w=0 자동 폭 생성 후 실측 재배치) — OK 최우측.
unsafe fn place_buttons_br(ok: HWND, cancel: HWND, w0: i32, by: i32, pad: i32) {
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOSIZE, SWP_NOZORDER};
    let width = |h: HWND| {
        let mut rc = RECT::default();
        let _ = windows::Win32::UI::WindowsAndMessaging::GetWindowRect(h, &mut rc);
        rc.right - rc.left
    };
    let (ow, cw) = (width(ok), width(cancel));
    let ox = w0 - pad - ow;
    let _ = SetWindowPos(ok, None, ox, by, 0, 0, SWP_NOSIZE | SWP_NOZORDER);
    let _ = SetWindowPos(
        cancel,
        None,
        ox - 8 - cw,
        by,
        0,
        0,
        SWP_NOSIZE | SWP_NOZORDER,
    );
}

/// 이름 입력 모달(macOS 시안 07-18 — 무캐션 라운드·상단 라벨·기본 이름 전체
/// 선택·우하단 [취소][OK]). 취소/빈 값 = None.
unsafe fn prompt_name(owner: HWND, font: HFONT) -> Option<String> {
    PROMPT_REGISTER.call_once(|| {
        let wc = WNDCLASSW {
            lpszClassName: PROMPT_CLASS,
            lpfnWndProc: Some(prompt_proc),
            hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as isize as *mut core::ffi::c_void),
            hCursor: windows::Win32::UI::WindowsAndMessaging::LoadCursorW(
                None,
                windows::Win32::UI::WindowsAndMessaging::IDC_ARROW,
            )
            .unwrap_or_default(),
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);
    });
    const PAD: i32 = 20;
    let (w0, h0) = (360, 150);
    let mut orc = RECT::default();
    let _ = windows::Win32::UI::WindowsAndMessaging::GetWindowRect(owner, &mut orc);
    let (cx, cy) = (
        orc.left + ((orc.right - orc.left) - w0) / 2,
        orc.top + ((orc.bottom - orc.top) - h0) / 2,
    );
    let dlg = CreateWindowExW(
        WINDOW_EX_STYLE(0x00000001),
        PROMPT_CLASS,
        w!(""),
        WINDOW_STYLE(WS_POPUP.0 | windows::Win32::UI::WindowsAndMessaging::WS_BORDER.0)
            | WS_VISIBLE,
        cx,
        cy,
        w0,
        h0,
        Some(owner),
        None,
        None,
        None,
    )
    .ok()?;
    round_popup(dlg);
    let style2 = Style::default();
    let default_name = tr("bulk.preset.savedSeq");
    crate::ctl::label::create(
        dlg,
        PAD,
        PAD,
        w0 - PAD * 2,
        0,
        4,
        font,
        &default_name,
        crate::ctl::label::LabelAlign::Left,
        style2,
    );
    let edit = crate::ctl::textbox::create(dlg, PAD, PAD + 28, w0 - PAD * 2, 0, 1, font, style2);
    set_text(edit, &default_name);
    // 전체 선택(EM_SETSEL=0x00B1 — 시안: 기본 이름이 선택된 채 열림)
    SendMessageW(edit, 0x00B1, Some(WPARAM(0)), Some(LPARAM(-1)));
    let by = h0 - PAD - 24;
    let ok = crate::ctl::button::create(
        dlg,
        0,
        by,
        0,
        0,
        2,
        font,
        &tr("bulk.preset.ok"),
        crate::ctl::button::ButtonKind::Default,
        true,
        style2,
    );
    let cancel = crate::ctl::button::create(
        dlg,
        0,
        by,
        0,
        0,
        3,
        font,
        &tr("bulk.preset.cancel"),
        crate::ctl::button::ButtonKind::Normal,
        true,
        style2,
    );
    place_buttons_br(ok, cancel, w0, by, PAD);
    let mut pst = Box::new(PromptState { edit, result: None });
    SetWindowLongPtrW(dlg, GWLP_USERDATA, &mut *pst as *mut PromptState as isize);
    let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(edit));
    let _ = EnableWindow(owner, false);
    let mut msg = MSG::default();
    while IsWindow(Some(dlg)).as_bool() && GetMessageW(&mut msg, None, 0, 0).as_bool() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    let _ = EnableWindow(owner, true);
    let _ = SetForegroundWindow(owner);
    pst.result.take()
}

// ── 프리셋 관리 팝업(Edit Renaming Sequences… — macOS 시안 07-18:
//    무헤더 지브라 목록 + 행별 빨간 ⊖, 삭제는 OK까지 스테이징) ──

const MANAGE_CLASS: PCWSTR = w!("NexaManagePresets");
static MANAGE_REGISTER: std::sync::Once = std::sync::Once::new();

struct ManageState {
    grid: HWND,
    /// 화면 목록(⊖ 클릭 즉시 제거 — 스테이징 반영 상태).
    names: Vec<String>,
    /// OK 확정 시 실제 파일 삭제 목록(취소 = 폐기).
    deleted: Vec<String>,
    font: HFONT,
}

unsafe fn manage_fill(st: &mut ManageState) {
    let rows = st
        .names
        .iter()
        .map(|n| crate::ctl::grid::GridRow {
            check: Some(false), // Minus 마크 표시 조건(Some — 값은 미사용)
            cells: vec![n.clone()],
        })
        .collect();
    crate::ctl::grid::set_rows(st.grid, rows);
}

unsafe extern "system" fn manage_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let st = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ManageState;
    match msg {
        WM_COMMAND if !st.is_null() => {
            let id = (wparam.0 & 0xFFFF) as u32;
            let code = ((wparam.0 >> 16) & 0xFFFF) as u32;
            let stm = &mut *st;
            if id == 1 && code == crate::ctl::grid::NXGR_TOGGLE {
                // ⊖ 클릭 = 목록에서 즉시 제거(스테이징 — 시안 동작)
                let idx = SendMessageW(stm.grid, crate::ctl::grid::NXGR_GETROW, None, None).0;
                if idx >= 0 && (idx as usize) < stm.names.len() {
                    let name = stm.names.remove(idx as usize);
                    stm.deleted.push(name);
                    manage_fill(stm);
                }
            } else if code == 1 && id == 2 {
                // OK = 스테이징된 삭제 확정(파일 제거)
                for name in &stm.deleted {
                    let _ = std::fs::remove_file(preset_dir().join(format!("{name}.cfg")));
                }
                let _ = DestroyWindow(hwnd);
            } else if code == 1 && id == 3 {
                let _ = DestroyWindow(hwnd); // 취소 = 스테이징 폐기
            }
            LRESULT(0)
        }
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

/// 프리셋 관리 모달(macOS 시안 07-18) — 무헤더 지브라 목록 + 행별 ⊖,
/// 우하단 [취소][OK]. OK = 삭제 확정, 취소 = 폐기. 닫은 뒤 호출부가 메뉴 재구성.
unsafe fn manage_presets(owner: HWND, font: HFONT) {
    MANAGE_REGISTER.call_once(|| {
        let wc = WNDCLASSW {
            lpszClassName: MANAGE_CLASS,
            lpfnWndProc: Some(manage_proc),
            hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as isize as *mut core::ffi::c_void),
            hCursor: windows::Win32::UI::WindowsAndMessaging::LoadCursorW(
                None,
                windows::Win32::UI::WindowsAndMessaging::IDC_ARROW,
            )
            .unwrap_or_default(),
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);
    });
    const PAD: i32 = 20;
    let (w0, h0) = (340, 360);
    let mut orc = RECT::default();
    let _ = windows::Win32::UI::WindowsAndMessaging::GetWindowRect(owner, &mut orc);
    let (cx, cy) = (
        orc.left + ((orc.right - orc.left) - w0) / 2,
        orc.top + ((orc.bottom - orc.top) - h0) / 2,
    );
    let Ok(dlg) = CreateWindowExW(
        WINDOW_EX_STYLE(0x00000001),
        MANAGE_CLASS,
        w!(""),
        WINDOW_STYLE(WS_POPUP.0 | windows::Win32::UI::WindowsAndMessaging::WS_BORDER.0)
            | WS_VISIBLE,
        cx,
        cy,
        w0,
        h0,
        Some(owner),
        None,
        None,
        None,
    ) else {
        return;
    };
    round_popup(dlg);
    let style2 = Style::default();
    let by = h0 - PAD - 24;
    let list_w = w0 - PAD * 2;
    let cols = [("", list_w)];
    let grid = crate::ctl::grid::create(
        dlg,
        PAD,
        PAD,
        list_w,
        by - PAD * 2,
        1,
        font,
        &cols,
        crate::ctl::grid::GridOpts {
            no_header: true,
            zebra: true,
            outline: true,
            row_h: file_row_h(dlg),
            mark: crate::ctl::grid::Mark::Minus,
        },
        style2,
    );
    let ok = crate::ctl::button::create(
        dlg,
        0,
        by,
        0,
        0,
        2,
        font,
        &tr("bulk.preset.ok"),
        crate::ctl::button::ButtonKind::Default,
        true,
        style2,
    );
    let cancel = crate::ctl::button::create(
        dlg,
        0,
        by,
        0,
        0,
        3,
        font,
        &tr("bulk.preset.cancel"),
        crate::ctl::button::ButtonKind::Normal,
        true,
        style2,
    );
    place_buttons_br(ok, cancel, w0, by, PAD);
    let mut mst = Box::new(ManageState {
        grid,
        names: preset_names(),
        deleted: Vec::new(),
        font,
    });
    manage_fill(&mut mst);
    SetWindowLongPtrW(dlg, GWLP_USERDATA, &mut *mst as *mut ManageState as isize);
    let _ = EnableWindow(owner, false);
    let mut msg = MSG::default();
    while IsWindow(Some(dlg)).as_bool() && GetMessageW(&mut msg, None, 0, 0).as_bool() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    let _ = EnableWindow(owner, true);
    let _ = SetForegroundWindow(owner);
    let _ = mst.font; // 수명 명시(폰트는 호출부 소유)
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
            // 카드 자식 통지 = lparam(컨트롤) → 부모 = 카드(X-23 — 카드별 지역 id)
            let src = HWND(lparam.0 as *mut core::ffi::c_void);
            let src_card = || GetParent(src).unwrap_or_default();
            match (id, notify) {
                (ID_KIND, 1 /* NXCB_CHANGED */) => {
                    // 해당 카드만 본문 재구성(동적 — 패널 스왑 대신)
                    let card = src_card();
                    let k = SendMessageW(src, NXCB_GETSEL, None, None).0.max(0) as usize;
                    build_card_body(card, k, (*st).font);
                    refresh_preview(&mut *st);
                }
                (ID_DT_HELP, 1 /* NXIB_CLICK */) => {
                    // 날짜 포맷 ${} 토큰 안내(사용자 확정 07-17 — ? 버튼).
                    // 토큰 예시는 언어 중립 — 마지막 줄만 i18n.
                    let body = format!(
                        "${{YYYY}} = 2026    ${{YY}} = 26\n\
                         ${{MMMM}} = July    ${{MMM}} = Jul\n\
                         ${{MM}} = 07    ${{M}} = 7\n\
                         ${{DD}} = 07    ${{D}} = 7    ${{DDD}} = Tue\n\
                         ${{HH}} = 09    ${{H}} = 9\n\
                         ${{mm}} = 05    ${{m}} = 5\n\
                         ${{ss}} = 07    ${{s}} = 7\n\n\
                         {}",
                        tr("bulk.date.fmtHelpNote")
                    );
                    let text = windows::core::HSTRING::from(body);
                    let title = windows::core::HSTRING::from(tr("bulk.date.fmtHelpTitle"));
                    let _ = windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
                        Some(hwnd),
                        PCWSTR(text.as_ptr()),
                        PCWSTR(title.as_ptr()),
                        windows::Win32::UI::WindowsAndMessaging::MB_OK,
                    );
                }
                (
                    ID_ADD,
                    1, /* NXIB_CLICK — 카드 + = 아래에 새 카드(사용자 확정) */
                ) => {
                    let card = src_card();
                    let at = (*st)
                        .cards
                        .iter()
                        .position(|c| *c == card)
                        .map_or((*st).cards.len(), |i| i + 1);
                    let new_card = make_card(hwnd, (*st).font, 0);
                    (*st).cards.insert(at, new_card);
                    relayout_cards(&*st);
                    refresh_preview(&mut *st);
                }
                (
                    ID_DEL,
                    1, /* NXIB_CLICK — 카드 − = 이 카드 삭제(마지막 1장 불가) */
                ) => {
                    let card = src_card();
                    if (*st).cards.len() > 1 {
                        if let Some(i) = (*st).cards.iter().position(|c| *c == card) {
                            let _ = DestroyWindow((*st).cards.remove(i));
                            relayout_cards(&*st);
                            refresh_preview(&mut *st);
                        }
                    }
                }
                (ID_PRESET_MENU, 1 /* NXMB_PICK — 프리셋 메뉴(07-18 v2) */) => {
                    let stm = &mut *st;
                    let idx = SendMessageW(
                        stm.preset_menu,
                        crate::ctl::menubutton::NXMB_GETPICK,
                        None,
                        None,
                    )
                    .0;
                    let n = stm.preset_names.len() as isize;
                    let sep = if n > 0 { 1 } else { 0 };
                    if idx >= 0 && idx < n {
                        // 프리셋 클릭 = 불러오기(카드 스택 재구성)
                        if let Some(name) = {
                            let names: &Vec<String> = &stm.preset_names;
                            names.get(idx as usize).cloned()
                        } {
                            if let Some(text) =
                                crate::config::load(&preset_dir(), &format!("{name}.cfg"))
                            {
                                let ops = parse_ops(&text);
                                rebuild_cards(hwnd, stm, &ops);
                                refresh_preview(stm);
                            }
                        }
                    } else if idx == n + sep {
                        // Save Renaming Sequence… = 이름 입력 팝업(사용자 확정)
                        if !stm.ops.is_empty() {
                            if let Some(name) = prompt_name(hwnd, stm.font) {
                                let _ = crate::config::save(
                                    &preset_dir(),
                                    &format!("{name}.cfg"),
                                    &serialize_ops(&stm.ops),
                                );
                                rebuild_preset_menu(hwnd, stm);
                            }
                        }
                    } else if idx == n + sep + 1 {
                        // Edit Renaming Sequences… = 관리 팝업(체크 삭제)
                        manage_presets(hwnd, stm.font);
                        rebuild_preset_menu(hwnd, stm);
                    }
                }
                (ID_PREV, 1 /* NXGR_TOGGLE — 적용 열 체크(행별 제외) */) => {
                    let stm = &mut *st; // 명시 재차용(원시 포인터 autoref lint)
                    let idx = SendMessageW(stm.prev, crate::ctl::grid::NXGR_GETROW, None, None).0;
                    if idx >= 0 {
                        let checked =
                            crate::ctl::grid::row_check(stm.prev, idx as usize).unwrap_or(true);
                        if let Some(e) = stm.excluded.get_mut(idx as usize) {
                            *e = !checked;
                        }
                        refresh_preview(stm); // 카운트·[적용] 활성 재계산
                    }
                }
                (ID_APPLY, 1 /* NXBTN_CLICK — Rename NxButton */) => {
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
                    let excluded = &(*st).excluded;
                    let out: Vec<(String, String, String)> = (*st)
                        .items
                        .iter()
                        .zip(&new_names)
                        .enumerate()
                        .filter(|(i, ((_, old, _, _, _), new))| {
                            // 그리드 적용 열 체크 해제 행 = 제외(07-18)
                            *new != old && !excluded.get(*i).copied().unwrap_or(false)
                        })
                        .map(|(_, ((p, o, _, _, _), n))| (p.clone(), o.clone(), n.clone()))
                        .collect();
                    (*st).result = Some(out);
                    let _ = DestroyWindow(hwnd);
                }
                (ID_CANCEL, 1 /* NXBTN_CLICK */) => {
                    let _ = DestroyWindow(hwnd);
                }
                _ => {
                    // 카드 폼 편집(텍스트·콤보·세그·체크·스핀) = 실시간 미리보기
                    // (X-23 — 블록 추가 없이 카드 자체가 파이프라인)
                    if matches!(notify, 1 | 0x0300 /* NX*_CHANGED | EN_CHANGE */) {
                        refresh_preview(&mut *st);
                    }
                }
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

    // ── 카드 스택(X-23 PF 모델): 초기 = Replace Text 카드 1장 ──
    let card0 = make_card(dlg, font, 0);

    // ── 하단 행(사용자 확정 07-18): 좌 [… ⌄]+건수 · 중 검증 오류 ·
    //    우 [취소][Rename NxButton Default] ──
    let by = CLIENT_H - PAD - 26;
    let apply = crate::ctl::button::create(
        dlg,
        0,
        by + 2,
        0,
        0,
        ID_APPLY,
        font,
        &tr("bulk.rename"),
        crate::ctl::button::ButtonKind::Default,
        true,
        Style::default(),
    );
    let cancel_btn = crate::ctl::button::create(
        dlg,
        0,
        by + 2,
        0,
        0,
        ID_CANCEL,
        font,
        &tr("bulk.cancel"),
        crate::ctl::button::ButtonKind::Normal,
        true,
        Style::default(),
    );
    place_buttons_br(apply, cancel_btn, CLIENT_W, by + 2, PAD);

    // ── 우측: 미리보기(전고 — 프리셋 행 제거분 확장) · 하단 검증 오류 ──
    let lx = PAD + FORM_W + PAD;
    let err = mk(
        dlg,
        font,
        w!("STATIC"),
        "",
        0,
        lx,
        by + 6,
        CLIENT_W - lx - PAD - 220,
        18,
        0,
    );
    // 미리보기 = NxGrid(적용/이전/이후 — 컬럼 리사이즈·적용 열 체크 토글, 07-18)
    let pw = CLIENT_W - lx - PAD;
    let apply_w = 56;
    let half_w = (pw - apply_w) / 2;
    let grid_cols = [
        (tr("bulk.grid.apply"), apply_w),
        (tr("bulk.grid.before"), half_w),
        (tr("bulk.grid.after"), half_w),
    ];
    let grid_refs: Vec<(&str, i32)> = grid_cols.iter().map(|(t, w)| (t.as_str(), *w)).collect();
    let prev = crate::ctl::grid::create(
        dlg,
        lx,
        PAD,
        pw,
        by - 8 - PAD,
        ID_PREV,
        font,
        &grid_refs,
        crate::ctl::grid::GridOpts {
            mark: crate::ctl::grid::Mark::Check,
            row_h: file_row_h(dlg),
            ..Default::default()
        },
        Style::default(),
    );
    // 변경 건수(PF "N items will be renamed") — 하단 [… ⌄] 오른쪽 동일 행
    let count = mk(
        dlg,
        font,
        w!("STATIC"),
        "",
        0,
        PAD + 48 + 10,
        by + 6,
        FORM_W - 48 - 10,
        18,
        ID_COUNT,
    );

    let items_len = items.len();
    let mut state = Box::new(BrState {
        font,
        items,
        tz_min,
        ops: Vec::new(),
        excluded: vec![false; items_len],
        cards: vec![card0],
        prev,
        err,
        apply,
        preset_menu: HWND::default(),
        preset_names: Vec::new(),
        count,
        result: None,
    });
    SetWindowLongPtrW(dlg, GWLP_USERDATA, &mut *state as *mut BrState as isize);
    relayout_cards(&state); // 초기 카드 1장 = − 비활성(마지막 카드 삭제 불가)
    rebuild_preset_menu(dlg, &mut state);
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
