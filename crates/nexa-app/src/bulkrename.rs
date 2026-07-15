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

use crate::dialog::DlgFont;
use crate::i18n::tr;
use nexa_ops::batch_rename::{
    conflicts, parse_ops, preview, serialize_ops, validate, CaseMode, Conflict, NumberSpec,
    RenameOp,
};

const CLASS: PCWSTR = w!("NexaBulkRename");
static REGISTER: std::sync::Once = std::sync::Once::new();

const PAD: i32 = 12;
const FORM_W: i32 = 300;
const CLIENT_W: i32 = 880;
const CLIENT_H: i32 = 560;
const STYLE: WINDOW_STYLE = WINDOW_STYLE(WS_POPUP.0 | WS_CAPTION.0 | WS_SYSMENU.0);

// 컨트롤 id — 파이프라인 편집
const ID_KIND: u32 = 1;
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
const ID_CASE_BASE: u32 = 20; // +0..3 = upper/lower/title/sentence
const ID_INS_TEXT: u32 = 30;
const ID_INS_PRE: u32 = 31;
const ID_INS_SUF: u32 = 32;
const ID_NUM_START: u32 = 40;
const ID_NUM_STEP: u32 = 41;
const ID_NUM_PAD: u32 = 42;
const ID_NUM_PRE: u32 = 43;
const ID_NUM_SUF: u32 = 44;
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

/// 동작 종류(콤보 순서) — i18n 라벨 키.
const KINDS: [&str; 6] = [
    "bulk.kind.replace",
    "bulk.kind.case",
    "bulk.kind.insert",
    "bulk.kind.number",
    "bulk.kind.move",
    "bulk.kind.ext",
];

struct BrState {
    font: HFONT,
    /// 대상 = (부모 경로, 현재 이름, 폴더 여부) — 선택 순서 보존(연번 기준).
    items: Vec<(String, String, bool)>,
    /// 파이프라인(위→아래 순차 적용) — UI의 단일 원천.
    ops: Vec<RenameOp>,
    /// 종류별 파라미터 컨트롤(show/hide 스왑).
    panels: [Vec<HWND>; 6],
    pipe: HWND,
    prev: HWND,
    err: HWND,
    apply: HWND,
    preset_combo: HWND,
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

/// 현재 파라미터 폼 → 동작 1블록(선택된 종류 기준). 유효하지 않으면 None.
unsafe fn op_from_form(dlg: HWND, kind: usize) -> Option<RenameOp> {
    match kind {
        0 => {
            let find = get_text(ctl(dlg, ID_FIND));
            (!find.is_empty()).then(|| RenameOp::Replace {
                find,
                with: get_text(ctl(dlg, ID_WITH)),
                match_case: checked(dlg, ID_MC),
                regex: checked(dlg, ID_RX),
            })
        }
        1 => {
            let mode = (0..4).find(|i| checked(dlg, ID_CASE_BASE + i))?;
            Some(RenameOp::Case(match mode {
                0 => CaseMode::Upper,
                1 => CaseMode::Lower,
                2 => CaseMode::Title,
                _ => CaseMode::Sentence,
            }))
        }
        2 => {
            let text = get_text(ctl(dlg, ID_INS_TEXT));
            (!text.is_empty()).then(|| RenameOp::Insert {
                text,
                suffix: !checked(dlg, ID_INS_PRE),
            })
        }
        3 => Some(RenameOp::Number(NumberSpec {
            start: num_of(dlg, ID_NUM_START, 1),
            step: num_of(dlg, ID_NUM_STEP, 1),
            pad: num_of(dlg, ID_NUM_PAD, 3).clamp(1, 12) as usize,
            suffix: !checked(dlg, ID_NUM_PRE),
        })),
        4 => {
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

/// 파이프라인 목록 한 줄 표기.
fn op_label(op: &RenameOp) -> String {
    match op {
        RenameOp::Replace {
            find, with, regex, ..
        } => format!(
            "{}{} \"{find}\" → \"{with}\"",
            tr("bulk.kind.replace"),
            if *regex { " (regex)" } else { "" }
        ),
        RenameOp::Case(m) => format!(
            "{}: {}",
            tr("bulk.kind.case"),
            tr(match m {
                CaseMode::Upper => "bulk.case.upper",
                CaseMode::Lower => "bulk.case.lower",
                CaseMode::Title => "bulk.case.title",
                CaseMode::Sentence => "bulk.case.sentence",
            })
        ),
        RenameOp::Insert { text, suffix } => format!(
            "{} \"{text}\" ({})",
            tr("bulk.kind.insert"),
            tr(if *suffix {
                "bulk.posSuffix"
            } else {
                "bulk.posPrefix"
            })
        ),
        RenameOp::Number(n) => format!(
            "{} {}+{}×{} ({})",
            tr("bulk.kind.number"),
            n.start,
            n.step,
            n.pad,
            tr(if n.suffix {
                "bulk.posSuffix"
            } else {
                "bulk.posPrefix"
            })
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
    let names: Vec<(String, bool)> = st.items.iter().map(|(_, n, d)| (n.clone(), *d)).collect();
    let new_names = preview(&names, &st.ops);
    let triples: Vec<(String, String, String)> = st
        .items
        .iter()
        .zip(&new_names)
        .map(|((p, o, _), n)| (p.clone(), o.clone(), n.clone()))
        .collect();
    let confs = conflicts(&triples, &|parent, name| {
        Path::new(parent).join(name).exists()
    });
    SendMessageW(st.prev, LB_RESETCONTENT, None, None);
    let mut changed = 0usize;
    for (i, (_, old, new)) in triples.iter().enumerate() {
        let line = if confs[i] != Conflict::None {
            format!("⚠ {old} → {new} ({})", conflict_label(confs[i]))
        } else if new != old {
            changed += 1;
            format!("{old} → {new}")
        } else {
            old.clone()
        };
        lb_add(st.prev, &line);
    }
    let ok = !st.ops.is_empty()
        && changed > 0
        && err.is_none()
        && confs.iter().all(|c| *c == Conflict::None);
    let _ = EnableWindow(st.apply, ok);
}

/// 종류 패널 전환(콤보 선택) — 해당 종류 컨트롤만 표시.
unsafe fn show_kind(st: &BrState, kind: usize) {
    for (i, panel) in st.panels.iter().enumerate() {
        for h in panel {
            let _ = ShowWindow(*h, if i == kind { SW_SHOW } else { SW_HIDE });
        }
    }
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
                    let names: Vec<(String, bool)> = (*st)
                        .items
                        .iter()
                        .map(|(_, n, d)| (n.clone(), *d))
                        .collect();
                    let new_names = preview(&names, &(*st).ops);
                    let out: Vec<(String, String, String)> = (*st)
                        .items
                        .iter()
                        .zip(&new_names)
                        .filter(|((_, old, _), new)| *new != old)
                        .map(|((p, o, _), n)| (p.clone(), o.clone(), n.clone()))
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
    let items: Vec<(String, String, bool)> = targets
        .iter()
        .filter_map(|(p, d)| {
            Some((
                p.parent()?.to_string_lossy().into_owned(),
                p.file_name()?.to_string_lossy().into_owned(),
                *d,
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

    // ── 종류별 파라미터 패널(겹침 배치 — 콤보 선택으로 스왑) ──
    let py = PAD + 34;
    let half = (FORM_W - 6) / 2;
    let third = (FORM_W - 12) / 3;
    let mut panels: [Vec<HWND>; 6] = Default::default();
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
        panels[0] = vec![f, wch, mc, rx];
    }
    // 1: 대소문자(라디오 4)
    {
        let keys = [
            "bulk.case.upper",
            "bulk.case.lower",
            "bulk.case.title",
            "bulk.case.sentence",
        ];
        let mut v = Vec::new();
        for (i, key) in keys.iter().enumerate() {
            let style = rb | if i == 0 { WS_GROUP.0 } else { 0 };
            let (cx2, cy2) = (x + (i as i32 % 2) * half, py + (i as i32 / 2) * 24);
            v.push(mk(
                dlg,
                font,
                w!("BUTTON"),
                &tr(key),
                style,
                cx2,
                cy2,
                half,
                20,
                ID_CASE_BASE + i as u32,
            ));
        }
        set_check(dlg, ID_CASE_BASE, true);
        panels[1] = v;
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
        let p = mk(
            dlg,
            font,
            w!("BUTTON"),
            &tr("bulk.posPrefix"),
            rb | WS_GROUP.0,
            x,
            py + 26,
            half,
            20,
            ID_INS_PRE,
        );
        let s = mk(
            dlg,
            font,
            w!("BUTTON"),
            &tr("bulk.posSuffix"),
            rb,
            x + half + 6,
            py + 26,
            half,
            20,
            ID_INS_SUF,
        );
        set_check(dlg, ID_INS_SUF, true);
        panels[2] = vec![t, p, s];
    }
    // 3: 연번(시작/증가/자릿수·앞/뒤)
    {
        let mut v = Vec::new();
        for (i, (key, id, init)) in [
            ("bulk.start", ID_NUM_START, "1"),
            ("bulk.step", ID_NUM_STEP, "1"),
            ("bulk.pad", ID_NUM_PAD, "3"),
        ]
        .iter()
        .enumerate()
        {
            let cx2 = x + i as i32 * (third + 6);
            let style = ed_num | if i == 0 { WS_GROUP.0 } else { 0 };
            let e = mk(dlg, font, w!("EDIT"), init, style, cx2, py, third, 22, *id);
            cue(e, &tr(key));
            v.push(e);
        }
        let p = mk(
            dlg,
            font,
            w!("BUTTON"),
            &tr("bulk.posPrefix"),
            rb | WS_GROUP.0,
            x,
            py + 26,
            half,
            20,
            ID_NUM_PRE,
        );
        let s = mk(
            dlg,
            font,
            w!("BUTTON"),
            &tr("bulk.posSuffix"),
            rb,
            x + half + 6,
            py + 26,
            half,
            20,
            ID_NUM_SUF,
        );
        set_check(dlg, ID_NUM_SUF, true);
        v.extend([p, s]);
        panels[3] = v;
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
        panels[4] = vec![st_e, ln_e, f, e];
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
        panels[5] = vec![f, t];
    }

    // ── 파이프라인 목록 + 재배치 ──
    let ly = py + 84;
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
        CLIENT_H - PAD * 2 - 22,
        0,
    );

    let mut state = Box::new(BrState {
        font,
        items,
        ops: Vec::new(),
        panels,
        pipe,
        prev,
        err,
        apply,
        preset_combo: pc,
        result: None,
    });
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
