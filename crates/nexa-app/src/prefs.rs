//! 설정 창(S6 → X-7 전면 확장 — 원본 `PreferencesWindow`/docs/40 §8 이식):
//! **VS Code식** = 상단 검색 + 좌측 카테고리 목록 + 우측 편집기(설정 레지스트리 구동).
//! 네이티브 컨트롤(user32 STATIC/EDIT/BUTTON — comctl32 비의존)·모달·`Ctrl+,`.
//!
//! 원본 구조 계승: 영속 설정 전부를 [`Entry`] 목록(레지스트리)으로 등록 → 카테고리 렌더와
//! **검색**(제목 부분 일치)이 같은 원천. 항목 = 테마·언어·터미널/대화상자 글꼴·크기·
//! 파일 목록(숨김·닷파일)·하단 도크. [저장]=수정 값 반환(적용·영속은 호스트).

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{DeleteObject, HBRUSH, HFONT};
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetMessageW, GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, IsWindow, RegisterClassW,
    SendMessageW, SetForegroundWindow, SetWindowLongPtrW, SetWindowTextW, TranslateMessage,
    BS_AUTOCHECKBOX, BS_PUSHBUTTON, ES_AUTOHSCROLL, ES_NUMBER, GWLP_USERDATA, HMENU, MSG,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_SETFONT, WNDCLASSW, WS_BORDER,
    WS_CAPTION, WS_CHILD, WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
};

use crate::dialog::DlgFont;
use crate::i18n::tr;

/// 설정 창 입력/결과 — 호스트(win.rs)가 현재 값을 넣고, 저장 시 수정본을 돌려받는다.
#[derive(Clone)]
pub struct PrefValues {
    pub theme: String, // "system"|"light"|"dark"
    pub lang: String,  // "system"|코드
    pub langs: Vec<String>,
    pub term_font: String,
    pub term_font_size: i32,
    pub dlg_font: String,
    pub dlg_font_size: i32,
    pub show_hidden: bool,
    pub show_dotfiles: bool,
    pub dock: bool,
}

/// 설정 항목 종류(편집 컨트롤 형태) — 레지스트리 최소 단위.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    ThemeCycle,
    LangCycle,
    Text,     // 글꼴 이름(EDIT)
    Number,   // 글꼴 크기(EDIT ES_NUMBER)
    CheckBox, // 불리언
}

/// 설정 항목(레지스트리) — 카테고리·라벨키·종류·대상 필드 id.
struct Entry {
    cat: &'static str,
    label_key: &'static str,
    kind: Kind,
    field: u32,
}

// 필드 id(값 라우팅) — 컨트롤 명령 id로도 사용.
const F_THEME: u32 = 1;
const F_LANG: u32 = 2;
const F_TERM_FONT: u32 = 3;
const F_TERM_SIZE: u32 = 4;
const F_DLG_FONT: u32 = 5;
const F_DLG_SIZE: u32 = 6;
const F_HIDDEN: u32 = 7;
const F_DOTFILES: u32 = 8;
const F_DOCK: u32 = 9;

/// 카테고리(좌측 목록 순서). (키, 라벨키).
const CATEGORIES: &[(&str, &str)] = &[
    ("appearance", "pref.cat.appearance"),
    ("fonts", "pref.cat.fonts"),
    ("list", "pref.cat.list"),
    ("dock", "pref.cat.dock"),
    ("lang", "pref.cat.lang"),
];

fn registry() -> Vec<Entry> {
    vec![
        Entry {
            cat: "appearance",
            label_key: "pref.theme",
            kind: Kind::ThemeCycle,
            field: F_THEME,
        },
        Entry {
            cat: "fonts",
            label_key: "pref.termFont",
            kind: Kind::Text,
            field: F_TERM_FONT,
        },
        Entry {
            cat: "fonts",
            label_key: "pref.termFontSize",
            kind: Kind::Number,
            field: F_TERM_SIZE,
        },
        Entry {
            cat: "fonts",
            label_key: "pref.dlgFont",
            kind: Kind::Text,
            field: F_DLG_FONT,
        },
        Entry {
            cat: "fonts",
            label_key: "pref.dlgFontSize",
            kind: Kind::Number,
            field: F_DLG_SIZE,
        },
        Entry {
            cat: "list",
            label_key: "pref.showHidden",
            kind: Kind::CheckBox,
            field: F_HIDDEN,
        },
        Entry {
            cat: "list",
            label_key: "pref.showDotfiles",
            kind: Kind::CheckBox,
            field: F_DOTFILES,
        },
        Entry {
            cat: "dock",
            label_key: "pref.dock",
            kind: Kind::CheckBox,
            field: F_DOCK,
        },
        Entry {
            cat: "lang",
            label_key: "pref.lang",
            kind: Kind::LangCycle,
            field: F_LANG,
        },
    ]
}

struct PrefState {
    values: PrefValues,
    hwnd: HWND,
    font: HFONT,
    /// 현재 카테고리(빈 검색 시)·검색어(있으면 전 카테고리에서 필터).
    category: String,
    query: String,
    /// 동적 생성한 우측 컨트롤들(카테고리/검색 변경 시 파괴·재생성).
    rows: Vec<HWND>,
    /// 각 편집 컨트롤 (field, hwnd) — 값 수거용.
    editors: Vec<(u32, HWND)>,
    saved: bool,
}

const ID_SAVE: u32 = 1000;
const ID_CANCEL: u32 = 1001;
const ID_SEARCH: u32 = 1002;
const ID_CAT_BASE: u32 = 1100; // +카테고리 인덱스
const ID_FIELD_BASE: u32 = 1200; // +field(순환/체크 버튼 명령)

static REGISTER: std::sync::Once = std::sync::Once::new();
const CLASS: PCWSTR = w!("NexaPrefs");
const PAD: i32 = 12;
const ROW_H: i32 = 32;
const CAT_W: i32 = 150;
const SEARCH_H: i32 = 26;
const LABEL_W: i32 = 170;
const CTRL_W: i32 = 220;
const CLIENT_W: i32 = PAD + CAT_W + PAD + LABEL_W + 8 + CTRL_W + PAD;
const CLIENT_H: i32 = PAD + SEARCH_H + PAD + ROW_H * 10 + PAD + 30;

unsafe fn set_text(hwnd: HWND, text: &str) {
    let w = windows::core::HSTRING::from(text);
    let _ = SetWindowTextW(hwnd, PCWSTR(w.as_ptr()));
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

fn lang_label(code: &str) -> String {
    if code == "system" {
        tr("pref.lang.system")
    } else {
        code.to_string()
    }
}

fn theme_label(t: &str) -> String {
    tr(&format!("pref.theme.{t}"))
}

/// 편집 컨트롤 생성 헬퍼.
#[allow(clippy::too_many_arguments)] // Win32 CreateWindow 인자 전달(래핑 이득 없음)
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

impl PrefState {
    /// 현재 카테고리/검색어에 맞는 항목만 우측에 (재)구성.
    unsafe fn rebuild(&mut self) {
        for h in self.rows.drain(..) {
            let _ = DestroyWindow(h);
        }
        self.editors.clear();
        let reg = registry();
        let q = self.query.to_lowercase();
        let x0 = PAD + CAT_W + PAD;
        let mut y = PAD + SEARCH_H + PAD;
        for e in &reg {
            let label = tr(e.label_key);
            let visible = if q.is_empty() {
                e.cat == self.category
            } else {
                label.to_lowercase().contains(&q)
            };
            if !visible {
                continue;
            }
            // 라벨
            let lbl = mk(
                self.hwnd,
                self.font,
                w!("STATIC"),
                &label,
                0,
                x0,
                y + 6,
                LABEL_W,
                20,
                0,
            );
            self.rows.push(lbl);
            let cx = x0 + LABEL_W + 8;
            let ctrl = match e.kind {
                Kind::ThemeCycle => mk(
                    self.hwnd,
                    self.font,
                    w!("BUTTON"),
                    &theme_label(&self.values.theme),
                    WS_TABSTOP.0 | BS_PUSHBUTTON as u32,
                    cx,
                    y + 2,
                    CTRL_W,
                    26,
                    ID_FIELD_BASE + e.field,
                ),
                Kind::LangCycle => mk(
                    self.hwnd,
                    self.font,
                    w!("BUTTON"),
                    &lang_label(&self.values.lang),
                    WS_TABSTOP.0 | BS_PUSHBUTTON as u32,
                    cx,
                    y + 2,
                    CTRL_W,
                    26,
                    ID_FIELD_BASE + e.field,
                ),
                Kind::CheckBox => {
                    let b = mk(
                        self.hwnd,
                        self.font,
                        w!("BUTTON"),
                        "",
                        WS_TABSTOP.0 | BS_AUTOCHECKBOX as u32,
                        cx,
                        y + 4,
                        22,
                        20,
                        ID_FIELD_BASE + e.field,
                    );
                    let on = match e.field {
                        F_HIDDEN => self.values.show_hidden,
                        F_DOTFILES => self.values.show_dotfiles,
                        F_DOCK => self.values.dock,
                        _ => false,
                    };
                    // BM_SETCHECK
                    SendMessageW(b, 0x00F1, Some(WPARAM(on as usize)), Some(LPARAM(0)));
                    b
                }
                Kind::Text | Kind::Number => {
                    let val = match e.field {
                        F_TERM_FONT => self.values.term_font.clone(),
                        F_TERM_SIZE => self.values.term_font_size.to_string(),
                        F_DLG_FONT => self.values.dlg_font.clone(),
                        F_DLG_SIZE => self.values.dlg_font_size.to_string(),
                        _ => String::new(),
                    };
                    let mut style = (WS_BORDER | WS_TABSTOP).0 | ES_AUTOHSCROLL as u32;
                    if e.kind == Kind::Number {
                        style |= ES_NUMBER as u32;
                    }
                    mk(
                        self.hwnd,
                        self.font,
                        w!("EDIT"),
                        &val,
                        style,
                        cx,
                        y + 2,
                        CTRL_W,
                        24,
                        ID_FIELD_BASE + e.field,
                    )
                }
            };
            self.rows.push(ctrl);
            if matches!(e.kind, Kind::Text | Kind::Number | Kind::CheckBox) {
                self.editors.push((e.field, ctrl));
            }
            y += ROW_H;
        }
        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(self.hwnd), None, true);
    }

    /// 편집 컨트롤 현재 값을 values에 흡수(저장/카테고리 전환 전).
    unsafe fn harvest(&mut self) {
        for &(field, hw) in &self.editors {
            match field {
                F_TERM_FONT => self.values.term_font = get_text(hw),
                F_TERM_SIZE => {
                    self.values.term_font_size = get_text(hw).trim().parse().unwrap_or(12)
                }
                F_DLG_FONT => self.values.dlg_font = get_text(hw),
                F_DLG_SIZE => self.values.dlg_font_size = get_text(hw).trim().parse().unwrap_or(9),
                F_HIDDEN | F_DOTFILES | F_DOCK => {
                    let on = SendMessageW(hw, 0x00F0, None, None).0 == 1; // BM_GETCHECK
                    match field {
                        F_HIDDEN => self.values.show_hidden = on,
                        F_DOTFILES => self.values.show_dotfiles = on,
                        F_DOCK => self.values.dock = on,
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
}

unsafe extern "system" fn prefs_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let st = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut PrefState;
    if st.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    match msg {
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as u32;
            let notify = (wparam.0 >> 16) as u32;
            match id {
                ID_SAVE => {
                    (*st).harvest();
                    (*st).saved = true;
                    let _ = DestroyWindow(hwnd);
                }
                ID_CANCEL => {
                    let _ = DestroyWindow(hwnd);
                }
                ID_SEARCH if notify == 0x0300 => {
                    // EN_CHANGE — 검색어 갱신·재구성
                    let text = get_text(HWND(lparam.0 as *mut core::ffi::c_void));
                    (*st).query = text;
                    (*st).rebuild();
                }
                i if (ID_CAT_BASE..ID_CAT_BASE + CATEGORIES.len() as u32).contains(&i) => {
                    (*st).harvest(); // 카테고리 이동 전 현재 편집 값 보존
                    (*st).category = CATEGORIES[(i - ID_CAT_BASE) as usize].0.to_string();
                    (*st).query.clear();
                    // 검색창 비우기
                    (*st).rebuild();
                }
                F_THEME_CMD if id == ID_FIELD_BASE + F_THEME => {
                    (*st).values.theme = match (*st).values.theme.as_str() {
                        "system" => "light",
                        "light" => "dark",
                        _ => "system",
                    }
                    .to_string();
                    let lbl = theme_label(&(*st).values.theme);
                    set_text(HWND(lparam.0 as *mut core::ffi::c_void), &lbl);
                }
                F_LANG_CMD if id == ID_FIELD_BASE + F_LANG => {
                    let mut opts = vec!["system".to_string()];
                    opts.extend((*st).values.langs.clone());
                    let cur = opts
                        .iter()
                        .position(|c| *c == (*st).values.lang)
                        .unwrap_or(0);
                    (*st).values.lang = opts[(cur + 1) % opts.len()].clone();
                    let lbl = lang_label(&(*st).values.lang);
                    set_text(HWND(lparam.0 as *mut core::ffi::c_void), &lbl);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// 명령 id 매칭용 상수(match arm 가드) — 실제 판별은 가드의 `id ==`.
const F_THEME_CMD: u32 = ID_FIELD_BASE + F_THEME;
const F_LANG_CMD: u32 = ID_FIELD_BASE + F_LANG;

/// 설정 창 표시(모달) — 저장 시 수정 값, 취소/닫기 시 `None`.
///
/// # Safety
/// UI 스레드에서 호출(모달 루프 동안 wndproc 재진입 — 호출자는 State 참조를 끊을 것).
pub unsafe fn show(owner: HWND, values: PrefValues, font_spec: &DlgFont) -> Option<PrefValues> {
    REGISTER.call_once(|| {
        let wc = WNDCLASSW {
            lpszClassName: CLASS,
            lpfnWndProc: Some(prefs_proc),
            hInstance: windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
                .unwrap_or_default()
                .into(),
            hbrBackground: HBRUSH(
                (windows::Win32::Graphics::Gdi::COLOR_BTNFACE.0 + 1) as isize
                    as *mut core::ffi::c_void,
            ),
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
    let font = crate::dialog::make_font_pub(owner, font_spec);
    let mut win = RECT {
        right: CLIENT_W,
        bottom: CLIENT_H,
        ..Default::default()
    };
    let _ = AdjustWindowRectEx(
        &mut win,
        WS_POPUP | WS_CAPTION | WS_SYSMENU,
        false,
        WINDOW_EX_STYLE(0x00000001),
    );
    let (w_, h_) = (win.right - win.left, win.bottom - win.top);
    let mut orc = RECT::default();
    let _ = windows::Win32::UI::WindowsAndMessaging::GetWindowRect(owner, &mut orc);
    let (cx, cy) = (
        orc.left + ((orc.right - orc.left) - w_) / 2,
        orc.top + ((orc.bottom - orc.top) - h_) / 2,
    );
    let title = windows::core::HSTRING::from(tr("pref.title"));
    let Ok(dlg) = CreateWindowExW(
        WINDOW_EX_STYLE(0x00000001),
        CLASS,
        PCWSTR(title.as_ptr()),
        WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
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
    let mut state = Box::new(PrefState {
        values,
        hwnd: dlg,
        font,
        category: "appearance".into(),
        query: String::new(),
        rows: Vec::new(),
        editors: Vec::new(),
        saved: false,
    });
    // 상단 검색창
    mk(
        dlg,
        font,
        w!("EDIT"),
        "",
        (WS_BORDER | WS_TABSTOP).0 | ES_AUTOHSCROLL as u32,
        PAD + CAT_W + PAD,
        PAD,
        LABEL_W + 8 + CTRL_W,
        SEARCH_H,
        ID_SEARCH,
    );
    // 좌측 카테고리 버튼
    for (i, (_, label_key)) in CATEGORIES.iter().enumerate() {
        mk(
            dlg,
            font,
            w!("BUTTON"),
            &tr(label_key),
            WS_TABSTOP.0 | BS_PUSHBUTTON as u32,
            PAD,
            PAD + i as i32 * (ROW_H - 2),
            CAT_W,
            ROW_H - 6,
            ID_CAT_BASE + i as u32,
        );
    }
    // 하단 저장/취소
    let by = CLIENT_H - PAD - 26;
    mk(
        dlg,
        font,
        w!("BUTTON"),
        &tr("pref.save"),
        WS_TABSTOP.0 | BS_PUSHBUTTON as u32,
        CLIENT_W - PAD - 90 - 8 - 90,
        by,
        90,
        26,
        ID_SAVE,
    );
    mk(
        dlg,
        font,
        w!("BUTTON"),
        &tr("pref.cancel"),
        WS_TABSTOP.0 | BS_PUSHBUTTON as u32,
        CLIENT_W - PAD - 90,
        by,
        90,
        26,
        ID_CANCEL,
    );
    SetWindowLongPtrW(dlg, GWLP_USERDATA, &mut *state as *mut PrefState as isize);
    state.rebuild();

    let _ = EnableWindow(owner, false);
    let _ = SetForegroundWindow(dlg);
    let mut msg = MSG::default();
    while IsWindow(Some(dlg)).as_bool() && GetMessageW(&mut msg, None, 0, 0).as_bool() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    let _ = EnableWindow(owner, true);
    let _ = SetForegroundWindow(owner);
    let _ = DeleteObject(font.into());
    if state.saved {
        let mut v = state.values.clone();
        if v.term_font.trim().is_empty() {
            v.term_font = "Consolas".into();
        }
        if v.dlg_font.trim().is_empty() {
            v.dlg_font = "Segoe UI".into();
        }
        v.term_font_size = v.term_font_size.clamp(8, 32);
        v.dlg_font_size = v.dlg_font_size.clamp(7, 24);
        Some(v)
    } else {
        None
    }
}
