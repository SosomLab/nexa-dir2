//! 설정 창(S6 → X-7 전면 확장 → X-8 VS Code식 완성 — 원본 `PreferencesWindow`/docs/40 §8):
//! **VS Code식** = 상단 검색 + 좌측 카테고리 목록 + 우측 편집기(설정 레지스트리 구동)
//! + **즉시 적용**(저장 버튼 없음 — 체크박스·순환=클릭 즉시, 텍스트·숫자=포커스 이탈 시)
//! + **리사이즈 가능 창**(WS_THICKFRAME — 기본 크기가 최소).
//!
//! 네이티브 컨트롤(user32 STATIC/EDIT/BUTTON — comctl32 비의존)·모달·`Ctrl+,`.
//!
//! 원본 구조 계승: 영속 설정 전부를 [`Entry`] 목록(레지스트리)으로 등록 → 카테고리 렌더와
//! **검색**(제목 부분 일치)이 같은 원천. 항목 = 테마·언어·터미널/대화상자 글꼴·크기·
//! 파일 목록(숨김·닷파일)·하단 도크. 적용은 [`WM_APP_PREFS_APPLY`]로 소유자에 동기 통지.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{DeleteObject, HBRUSH, HFONT};
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetMessageW, GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, IsWindow, MoveWindow,
    RegisterClassW, SendMessageW, SetForegroundWindow, SetWindowLongPtrW, SetWindowTextW,
    TranslateMessage, BS_AUTOCHECKBOX, BS_PUSHBUTTON, ES_AUTOHSCROLL, ES_NUMBER, GWLP_USERDATA,
    HMENU, MINMAXINFO, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND,
    WM_GETMINMAXINFO, WM_SETFONT, WM_SIZE, WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CHILD,
    WS_MAXIMIZEBOX, WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_THICKFRAME, WS_VISIBLE,
};

/// 설정 변경 즉시 적용 통지(VS Code식 — X-8): lparam = `*const PrefValues`(통지 동안만 유효,
/// 같은 스레드 SendMessage = 소유자 wndproc 직접 호출이므로 수신 측은 즉시 복사).
pub const WM_APP_PREFS_APPLY: u32 = 0x8006; // WM_APP + 6 (win.rs 0x8001~0x8005 다음)

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
    /// 소유자(메인 창) — 즉시 적용 통지 대상(X-8).
    owner: HWND,
    font: HFONT,
    /// 현재 카테고리(빈 검색 시)·검색어(있으면 전 카테고리에서 필터).
    category: String,
    query: String,
    /// 상단 검색창(리사이즈 시 폭 추종).
    search: HWND,
    /// 현재 클라이언트 크기(리사이즈 추종 레이아웃 — X-8).
    cw: i32,
    ch: i32,
    /// 동적 생성한 우측 컨트롤들(카테고리/검색 변경 시 파괴·재생성).
    rows: Vec<HWND>,
    /// 각 편집 컨트롤 (field, hwnd) — 값 수거용.
    editors: Vec<(u32, HWND)>,
}

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

/// 값 정규화(빈 글꼴 폴백·크기 클램프) — 즉시 적용·닫기 공용.
fn sanitize(v: &mut PrefValues) {
    if v.term_font.trim().is_empty() {
        v.term_font = "Consolas".into();
    }
    if v.dlg_font.trim().is_empty() {
        v.dlg_font = "Segoe UI".into();
    }
    v.term_font_size = v.term_font_size.clamp(8, 32);
    v.dlg_font_size = v.dlg_font_size.clamp(7, 24);
}

impl PrefState {
    /// VS Code식 즉시 적용(X-8) — 정규화한 현재 값을 소유자에 동기 통지(포인터는 통지 동안만
    /// 유효 — 같은 스레드 SendMessage라 수신 측이 복사를 마친 뒤 반환된다).
    unsafe fn apply_now(&self) {
        let mut v = self.values.clone();
        sanitize(&mut v);
        SendMessageW(
            self.owner,
            WM_APP_PREFS_APPLY,
            Some(WPARAM(0)),
            Some(LPARAM(&v as *const PrefValues as isize)),
        );
    }

    /// 현재 카테고리/검색어에 맞는 항목만 우측에 (재)구성.
    unsafe fn rebuild(&mut self) {
        for h in self.rows.drain(..) {
            let _ = DestroyWindow(h);
        }
        self.editors.clear();
        let reg = registry();
        let q = self.query.to_lowercase();
        let x0 = PAD + CAT_W + PAD;
        // 편집 컨트롤 폭 = 잔여 클라이언트 폭(리사이즈 추종 — X-8, 기본 크기에서 CTRL_W).
        let ctrl_w = (self.cw - x0 - LABEL_W - 8 - PAD).max(80);
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
                    ctrl_w,
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
                    ctrl_w,
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
                        ctrl_w,
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
                    (*st).apply_now(); // VS Code식 즉시 적용(X-8)
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
                    (*st).apply_now();
                }
                // VS Code식 즉시 적용(X-8): 체크박스 클릭(BN_CLICKED=0)은 즉시,
                // EDIT(글꼴·크기)은 포커스 이탈(EN_KILLFOCUS=0x0200) 시 값 확정 후 적용
                // (키 입력마다 백엔드 재생성 방지 — EN_CHANGE는 무시).
                i if i >= ID_FIELD_BASE && (notify == 0 || notify == 0x0200) => {
                    (*st).harvest();
                    (*st).apply_now();
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_SIZE => {
            // 리사이즈 추종(X-8) — 검색창 폭·편집 컨트롤 폭 재배치(최소화는 무시).
            let (w, h) = ((lparam.0 & 0xFFFF) as i32, ((lparam.0 >> 16) & 0xFFFF) as i32);
            if w > 0 && h > 0 {
                (*st).cw = w;
                (*st).ch = h;
                let x0 = PAD + CAT_W + PAD;
                let _ = MoveWindow((*st).search, x0, PAD, (w - x0 - PAD).max(80), SEARCH_H, true);
                (*st).harvest(); // 재구성 전 편집 값 보존
                (*st).rebuild();
            }
            LRESULT(0)
        }
        WM_GETMINMAXINFO => {
            // 최소 크기 = 기본 클라이언트 크기(컨트롤 클리핑 방지 — X-8).
            let mut rc = RECT {
                right: CLIENT_W,
                bottom: CLIENT_H,
                ..Default::default()
            };
            let _ = AdjustWindowRectEx(&mut rc, PREFS_STYLE, false, WINDOW_EX_STYLE(0x00000001));
            let mmi = lparam.0 as *mut MINMAXINFO;
            if !mmi.is_null() {
                (*mmi).ptMinTrackSize.x = rc.right - rc.left;
                (*mmi).ptMinTrackSize.y = rc.bottom - rc.top;
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            (*st).harvest(); // 닫기 전 미확정 편집 값 수거(최종 적용은 show 반환 후 호스트)
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// 설정 창 스타일 — 리사이즈 가능(VS Code식 — X-8). 기본 크기가 최소 크기.
const PREFS_STYLE: WINDOW_STYLE = WINDOW_STYLE(
    WS_POPUP.0 | WS_CAPTION.0 | WS_SYSMENU.0 | WS_THICKFRAME.0 | WS_MAXIMIZEBOX.0,
);

// 명령 id 매칭용 상수(match arm 가드) — 실제 판별은 가드의 `id ==`.
const F_THEME_CMD: u32 = ID_FIELD_BASE + F_THEME;
const F_LANG_CMD: u32 = ID_FIELD_BASE + F_LANG;

/// 설정 창 표시(모달) — VS Code식 즉시 적용(X-8): 변경은 [`WM_APP_PREFS_APPLY`]로 소유자에
/// 실시간 통지되고, 닫기 시 최종 값을 반환(호스트가 최종 적용·영속 — 미이탈 편집 값 수거).
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
    let _ = AdjustWindowRectEx(&mut win, PREFS_STYLE, false, WINDOW_EX_STYLE(0x00000001));
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
        PREFS_STYLE | WS_VISIBLE,
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
        owner,
        font,
        category: "appearance".into(),
        query: String::new(),
        search: HWND::default(),
        cw: CLIENT_W,
        ch: CLIENT_H,
        rows: Vec::new(),
        editors: Vec::new(),
    });
    // 상단 검색창(리사이즈 시 폭 추종 — WM_SIZE)
    state.search = mk(
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
    // 하단 저장/취소 버튼 없음 — VS Code식 즉시 적용(X-8), 닫기 = 타이틀바 X.
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
    // 즉시 적용 방식(X-8) — 닫기 = 확정. 최종 값 반환(미이탈 편집 값 포함, WM_CLOSE에서 수거).
    let mut v = state.values.clone();
    sanitize(&mut v);
    Some(v)
}
