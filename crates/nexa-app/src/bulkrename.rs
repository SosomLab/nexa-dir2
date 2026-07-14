//! 일괄 이름변경 다이얼로그(M5-1 — 원본 docs/25 α: 좌측 동작 폼 + 우측 실시간 미리보기).
//! 원본은 설계만 존재(구현 0) — docs/25 스펙이 SSOT. 순수 로직은
//! [`nexa_ops::batch_rename`](nexa-ops), 이 모듈은 네이티브 컨트롤 UI만.
//!
//! α 범위: 동작 4종(치환·대소문자·삽입·연번 — **고정 순서 파이프라인**, 블록 재배열 없음)
//! · 충돌 4종 하이라이트·적용 차단 · 적용 = 앱 계층(MoveBatchOp 트랜잭션 1건 — §7 B-13u).
//! 정규식·날짜·토큰 언어·프리셋은 β 이후(docs/25 §8).
//!
//! prefs.rs와 동일 규약: user32 네이티브 컨트롤(comctl32 비의존 — B3 게이트)·자체 모달 루프.

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
    SendMessageW, SetForegroundWindow, SetWindowLongPtrW, TranslateMessage, BS_AUTOCHECKBOX,
    BS_AUTORADIOBUTTON, ES_AUTOHSCROLL, ES_NUMBER, GWLP_USERDATA, HMENU, MSG, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_CTLCOLORBTN, WM_CTLCOLORSTATIC, WM_SETFONT, WNDCLASSW,
    WS_BORDER, WS_CAPTION, WS_CHILD, WS_GROUP, WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
    WS_VSCROLL,
};

use crate::dialog::DlgFont;
use crate::i18n::tr;
use nexa_ops::batch_rename::{conflicts, preview, BatchSpec, CaseMode, Conflict, NumberSpec};

const CLASS: PCWSTR = w!("NexaBulkRename");
static REGISTER: std::sync::Once = std::sync::Once::new();

const PAD: i32 = 12;
const ROW: i32 = 26;
const FORM_W: i32 = 260;
const CLIENT_W: i32 = 760;
const CLIENT_H: i32 = 460;
const STYLE: WINDOW_STYLE = WINDOW_STYLE(WS_POPUP.0 | WS_CAPTION.0 | WS_SYSMENU.0);

// 컨트롤 id
const ID_FIND: u32 = 1;
const ID_WITH: u32 = 2;
const ID_MATCHCASE: u32 = 3;
const ID_INSERT: u32 = 4;
const ID_INS_PRE: u32 = 5;
const ID_INS_SUF: u32 = 6;
/// 대소문자 라디오 5종(없음·UPPER·lower·Title·Sentence) — ID_CASE_BASE + idx.
const ID_CASE_BASE: u32 = 10;
const ID_NUM: u32 = 20;
const ID_START: u32 = 21;
const ID_STEP: u32 = 22;
const ID_PAD_D: u32 = 23;
const ID_NUM_PRE: u32 = 24;
const ID_NUM_SUF: u32 = 25;
const ID_APPLY: u32 = 30;
const ID_CANCEL: u32 = 31;
const ID_LIST: u32 = 32;

// LISTBOX 메시지(winuser.h)
const LB_ADDSTRING: u32 = 0x0180;
const LB_RESETCONTENT: u32 = 0x0184;

struct BrState {
    hwnd: HWND,
    font: HFONT,
    /// 대상 = (부모 경로, 현재 이름, 폴더 여부) — 선택 순서 보존(연번 기준).
    items: Vec<(String, String, bool)>,
    list: HWND,
    apply: HWND,
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

/// 컨트롤 → 동작 묶음(α 고정 순서). 빈 필드 = 그 동작 비활성.
unsafe fn spec_of(dlg: HWND) -> BatchSpec {
    let find = get_text(ctl(dlg, ID_FIND));
    let insert = get_text(ctl(dlg, ID_INSERT));
    let case = (1..5)
        .find(|i| checked(dlg, ID_CASE_BASE + i))
        .map(|i| match i {
            1 => CaseMode::Upper,
            2 => CaseMode::Lower,
            3 => CaseMode::Title,
            _ => CaseMode::Sentence,
        });
    let number = checked(dlg, ID_NUM).then(|| NumberSpec {
        start: get_text(ctl(dlg, ID_START)).parse().unwrap_or(1),
        step: get_text(ctl(dlg, ID_STEP)).parse().unwrap_or(1),
        pad: get_text(ctl(dlg, ID_PAD_D)).parse().unwrap_or(3),
        suffix: !checked(dlg, ID_NUM_PRE),
    });
    BatchSpec {
        replace: (!find.is_empty()).then(|| {
            (
                find,
                get_text(ctl(dlg, ID_WITH)),
                checked(dlg, ID_MATCHCASE),
            )
        }),
        case,
        insert: (!insert.is_empty()).then(|| (insert, !checked(dlg, ID_INS_PRE))),
        number,
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

/// 미리보기·충돌 재계산 → 리스트 갱신 + [적용] 활성 판정.
unsafe fn refresh(st: &mut BrState) {
    let spec = spec_of(st.hwnd);
    let names: Vec<(String, bool)> = st.items.iter().map(|(_, n, d)| (n.clone(), *d)).collect();
    let new_names = preview(&names, &spec);
    let triples: Vec<(String, String, String)> = st
        .items
        .iter()
        .zip(&new_names)
        .map(|((p, o, _), n)| (p.clone(), o.clone(), n.clone()))
        .collect();
    let confs = conflicts(&triples, &|parent, name| {
        Path::new(parent).join(name).exists()
    });
    SendMessageW(st.list, LB_RESETCONTENT, None, None);
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
        let w = windows::core::HSTRING::from(line);
        SendMessageW(
            st.list,
            LB_ADDSTRING,
            None,
            Some(LPARAM(w.as_ptr() as isize)),
        );
    }
    let ok = !spec.is_empty() && changed > 0 && confs.iter().all(|c| *c == Conflict::None);
    let _ = EnableWindow(st.apply, ok);
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
            match id {
                ID_APPLY if notify == 0 => {
                    // 확정 — 충돌 없음은 refresh가 보장([적용] 활성 조건)
                    let spec = spec_of(hwnd);
                    let names: Vec<(String, bool)> = (*st)
                        .items
                        .iter()
                        .map(|(_, n, d)| (n.clone(), *d))
                        .collect();
                    let new_names = preview(&names, &spec);
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
                ID_CANCEL if notify == 0 => {
                    let _ = DestroyWindow(hwnd);
                }
                // 그 외 컨트롤: 체크/라디오 클릭(0)·EDIT 변경(EN_CHANGE=0x300) → 실시간 미리보기
                _ if notify == 0 || notify == 0x0300 => refresh(&mut *st),
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

    // ── 좌측 동작 폼(α 고정 순서: 치환 → 대소문자 → 삽입 → 연번) ──
    let x = PAD;
    let mut y = PAD;
    let lbl = |dlg, text: &str, y: i32| mk(dlg, font, w!("STATIC"), text, 0, x, y, FORM_W, 18, 0);
    let half = (FORM_W - 6) / 2;
    lbl(dlg, &tr("bulk.find"), y);
    y += 20;
    mk(
        dlg,
        font,
        w!("EDIT"),
        "",
        (WS_BORDER | WS_TABSTOP).0 | ES_AUTOHSCROLL as u32,
        x,
        y,
        FORM_W,
        22,
        ID_FIND,
    );
    y += ROW;
    lbl(dlg, &tr("bulk.with"), y);
    y += 20;
    mk(
        dlg,
        font,
        w!("EDIT"),
        "",
        (WS_BORDER | WS_TABSTOP).0 | ES_AUTOHSCROLL as u32,
        x,
        y,
        FORM_W,
        22,
        ID_WITH,
    );
    y += ROW;
    mk(
        dlg,
        font,
        w!("BUTTON"),
        &tr("bulk.matchCase"),
        WS_TABSTOP.0 | BS_AUTOCHECKBOX as u32,
        x,
        y,
        FORM_W,
        20,
        ID_MATCHCASE,
    );
    y += ROW + 4;

    lbl(dlg, &tr("bulk.case"), y);
    y += 20;
    let case_keys = [
        "bulk.case.none",
        "bulk.case.upper",
        "bulk.case.lower",
        "bulk.case.title",
        "bulk.case.sentence",
    ];
    for (i, key) in case_keys.iter().enumerate() {
        let style = WS_TABSTOP.0 | BS_AUTORADIOBUTTON as u32 | if i == 0 { WS_GROUP.0 } else { 0 };
        let (cx2, cy2) = (x + (i as i32 % 3) * (FORM_W / 3), y + (i as i32 / 3) * 22);
        mk(
            dlg,
            font,
            w!("BUTTON"),
            &tr(key),
            style,
            cx2,
            cy2,
            FORM_W / 3,
            20,
            ID_CASE_BASE + i as u32,
        );
    }
    set_check(dlg, ID_CASE_BASE, true); // 기본 = 변경 없음
    y += 22 * 2 + 8;

    lbl(dlg, &tr("bulk.insert"), y);
    y += 20;
    mk(
        dlg,
        font,
        w!("EDIT"),
        "",
        (WS_BORDER | WS_TABSTOP).0 | ES_AUTOHSCROLL as u32,
        x,
        y,
        FORM_W,
        22,
        ID_INSERT,
    );
    y += ROW;
    mk(
        dlg,
        font,
        w!("BUTTON"),
        &tr("bulk.posPrefix"),
        WS_TABSTOP.0 | WS_GROUP.0 | BS_AUTORADIOBUTTON as u32,
        x,
        y,
        half,
        20,
        ID_INS_PRE,
    );
    mk(
        dlg,
        font,
        w!("BUTTON"),
        &tr("bulk.posSuffix"),
        WS_TABSTOP.0 | BS_AUTORADIOBUTTON as u32,
        x + half + 6,
        y,
        half,
        20,
        ID_INS_SUF,
    );
    set_check(dlg, ID_INS_SUF, true);
    y += ROW + 4;

    mk(
        dlg,
        font,
        w!("BUTTON"),
        &tr("bulk.number"),
        WS_TABSTOP.0 | BS_AUTOCHECKBOX as u32,
        x,
        y,
        FORM_W,
        20,
        ID_NUM,
    );
    y += 24;
    let third = (FORM_W - 12) / 3;
    for (i, (key, id, init)) in [
        ("bulk.start", ID_START, "1"),
        ("bulk.step", ID_STEP, "1"),
        ("bulk.pad", ID_PAD_D, "3"),
    ]
    .iter()
    .enumerate()
    {
        let cx2 = x + i as i32 * (third + 6);
        mk(dlg, font, w!("STATIC"), &tr(key), 0, cx2, y, third, 16, 0);
        mk(
            dlg,
            font,
            w!("EDIT"),
            init,
            (WS_BORDER | WS_TABSTOP).0 | (ES_NUMBER | ES_AUTOHSCROLL) as u32,
            cx2,
            y + 18,
            third,
            22,
            *id,
        );
    }
    y += 18 + ROW;
    mk(
        dlg,
        font,
        w!("BUTTON"),
        &tr("bulk.posPrefix"),
        WS_TABSTOP.0 | WS_GROUP.0 | BS_AUTORADIOBUTTON as u32,
        x,
        y,
        half,
        20,
        ID_NUM_PRE,
    );
    mk(
        dlg,
        font,
        w!("BUTTON"),
        &tr("bulk.posSuffix"),
        WS_TABSTOP.0 | BS_AUTORADIOBUTTON as u32,
        x + half + 6,
        y,
        half,
        20,
        ID_NUM_SUF,
    );
    set_check(dlg, ID_NUM_SUF, true);

    // 하단 버튼
    let by = CLIENT_H - PAD - 26;
    let apply = mk(
        dlg,
        font,
        w!("BUTTON"),
        &tr("bulk.apply"),
        WS_TABSTOP.0,
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

    // ── 우측 미리보기(원본 → 새 이름·충돌 하이라이트) ──
    let lx = PAD + FORM_W + PAD;
    let list = mk(
        dlg,
        font,
        w!("LISTBOX"),
        "",
        (WS_BORDER | WS_VSCROLL).0 | 0x1000 /* LBS_NOSEL */ | 0x0040, /* LBS_NOINTEGRALHEIGHT */
        lx,
        PAD,
        CLIENT_W - lx - PAD,
        CLIENT_H - PAD * 2,
        ID_LIST,
    );

    let mut state = Box::new(BrState {
        hwnd: dlg,
        font,
        items,
        list,
        apply,
        result: None,
    });
    SetWindowLongPtrW(dlg, GWLP_USERDATA, &mut *state as *mut BrState as isize);
    refresh(&mut state);

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
