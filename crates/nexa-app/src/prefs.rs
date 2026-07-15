//! 설정 창(S6 → X-7 → X-8 → X-9 원본 UI 재현 — 원본 `PreferencesWindow`/docs/40 §8):
//! **VS Code식** = 좌측 사이드바(검색+카테고리 목록·선택 하이라이트) + 우측 편집기
//! (섹션 제목 + 체크박스[라벨 일체]·라디오 그룹·입력 필드) — 원본 스크린샷 레이아웃 재현.
//! **즉시 적용**(저장 버튼 없음 — 체크박스·라디오=클릭 즉시, 텍스트·숫자=포커스 이탈 시)
//! + **리사이즈 가능 창**(WS_THICKFRAME — 기본 크기가 최소).
//!
//! 네이티브 컨트롤(user32 STATIC/EDIT/BUTTON — comctl32 비의존)·모달·`Ctrl+,`.
//! 원본 구조 계승: 영속 설정 전부를 [`Entry`] 목록(레지스트리)으로 등록 → 카테고리 렌더와
//! **검색**(제목 부분 일치)이 같은 원천. dir2에 존재하는 설정만 등록(없는 옵션 미등록).
//! 적용은 [`WM_APP_PREFS_APPLY`]로 소유자에 동기 통지.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, FillRect, GetSysColorBrush,
    InvalidateRect, SelectObject, SetBkMode, CLIP_DEFAULT_PRECIS, COLOR_WINDOW, DEFAULT_CHARSET,
    DEFAULT_QUALITY, DT_LEFT, DT_SINGLELINE, DT_VCENTER, FF_DONTCARE, FW_SEMIBOLD, HBRUSH, HFONT,
    OUT_DEFAULT_PRECIS, TRANSPARENT,
};
use windows::Win32::UI::Controls::DRAWITEMSTRUCT;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetMessageW, GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, IsWindow, MoveWindow,
    RegisterClassW, SendMessageW, SetForegroundWindow, SetWindowLongPtrW, SetWindowTextW,
    TranslateMessage, BS_AUTOCHECKBOX, BS_AUTORADIOBUTTON, ES_AUTOHSCROLL, ES_NUMBER,
    GWLP_USERDATA, HMENU, MINMAXINFO, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND,
    WM_CTLCOLORBTN, WM_CTLCOLOREDIT, WM_CTLCOLORSTATIC, WM_DRAWITEM, WM_GETMINMAXINFO, WM_SETFONT,
    WM_SIZE, WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CHILD, WS_GROUP, WS_MAXIMIZEBOX, WS_POPUP,
    WS_SYSMENU, WS_TABSTOP, WS_THICKFRAME, WS_VISIBLE, WS_VSCROLL,
};

use crate::dialog::DlgFont;
use crate::i18n::tr;

/// 설정 변경 즉시 적용 통지(VS Code식 — X-8): lparam = `*const PrefValues`(통지 동안만 유효,
/// 같은 스레드 SendMessage = 소유자 wndproc 직접 호출이므로 수신 측은 즉시 복사).
pub const WM_APP_PREFS_APPLY: u32 = 0x8006; // WM_APP + 6 (win.rs 0x8001~0x8005 다음)

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
    /// 정적 라디오 그룹 — (값, 라벨 키) 목록(원본 스크린샷: 캡션 + 세로 라디오).
    Radio(&'static [(&'static str, &'static str)]),
    /// 언어 라디오(동적 — system + 발견 언어).
    LangRadio,
    Text,     // 글꼴 이름(EDIT)
    Number,   // 글꼴 크기(EDIT ES_NUMBER)
    CheckBox, // 불리언(라벨 일체형 — 원본 스크린샷)
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

/// 사이드바 **계층 트리**(전면 개편 07-15 — 사용자 요청: 단일 컴포넌트 트리 + 클릭 시
/// 우측 세부): 정적 pre-order (key, 라벨 키, 깊이). 자식 여부 = 다음 노드 깊이로 판정.
/// 그룹 노드 클릭 = 펼침 토글 + 하위 카테고리 전체를 섹션으로 표시, leaf = 그 카테고리만.
const TREE: &[(&str, &str, i32)] = &[
    ("general", "pref.grp.general", 0),
    ("appearance", "pref.cat.appearance", 1),
    ("fonts", "pref.cat.fonts", 1),
    ("lang", "pref.cat.lang", 1),
    ("list", "pref.cat.list", 0),
    ("panel", "pref.grp.panel", 0),
    ("dock", "pref.cat.dock", 1),
    ("terminal", "pref.cat.terminal", 1),
];

fn tree_has_children(i: usize) -> bool {
    TREE.get(i + 1).is_some_and(|n| n.2 > TREE[i].2)
}

/// 노드가 커버하는 leaf 카테고리 목록 — (카테고리 key, 라벨 키). leaf면 자신 1개.
fn tree_cats(i: usize) -> Vec<(&'static str, &'static str)> {
    if !tree_has_children(i) {
        return vec![(TREE[i].0, TREE[i].1)];
    }
    let d = TREE[i].2;
    let mut out = Vec::new();
    for n in &TREE[i + 1..] {
        if n.2 <= d {
            break;
        }
        out.push((n.0, n.1));
    }
    out
}

fn tree_index(key: &str) -> Option<usize> {
    TREE.iter().position(|n| n.0 == key)
}

/// 펼침 상태 기준 가시 노드 인덱스(pre-order — 접힌 그룹의 하위는 생략).
fn tree_visible(expanded: &[bool]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut hide_deeper: Option<i32> = None;
    for (i, n) in TREE.iter().enumerate() {
        if let Some(d) = hide_deeper {
            if n.2 > d {
                continue;
            }
            hide_deeper = None;
        }
        out.push(i);
        if tree_has_children(i) && !expanded[i] {
            hide_deeper = Some(n.2);
        }
    }
    out
}

const THEME_OPTS: &[(&str, &str)] = &[
    ("system", "pref.theme.system"),
    ("light", "pref.theme.light"),
    ("dark", "pref.theme.dark"),
];

fn registry() -> Vec<Entry> {
    vec![
        Entry {
            cat: "appearance",
            label_key: "pref.theme",
            kind: Kind::Radio(THEME_OPTS),
            field: F_THEME,
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
            cat: "terminal",
            label_key: "pref.termFont",
            kind: Kind::Text,
            field: F_TERM_FONT,
        },
        Entry {
            cat: "terminal",
            label_key: "pref.termFontSize",
            kind: Kind::Number,
            field: F_TERM_SIZE,
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
            kind: Kind::LangRadio,
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
    /// 섹션 제목용 큰 글꼴(X-9 — 원본 스크린샷 "File List" 헤더).
    title_font: HFONT,
    /// 현재 선택 노드 key(빈 검색 시)·검색어(있으면 전 카테고리에서 필터).
    category: String,
    query: String,
    /// 사이드바 트리(오너드로 LISTBOX 단일 컴포넌트 — 전면 개편 07-15).
    tree: HWND,
    /// TREE 인덱스별 펼침 상태(기본 = 전부 펼침).
    expanded: Vec<bool>,
    /// 현재 가시 노드(트리 목록 행 → TREE 인덱스).
    visible: Vec<usize>,
    /// 상단 검색창(사이드바 상단 — 원본 스크린샷 위치).
    search: HWND,
    /// 사이드바/본문 세로 구분선(리사이즈 시 높이 추종).
    divider: HWND,
    /// 현재 클라이언트 크기(리사이즈 추종 레이아웃 — X-8).
    cw: i32,
    ch: i32,
    /// 동적 생성한 우측 컨트롤들(카테고리/검색 변경 시 파괴·재생성).
    rows: Vec<HWND>,
    /// 각 편집 컨트롤 (field, hwnd) — 값 수거용(체크박스·EDIT).
    editors: Vec<(u32, HWND)>,
    /// 라디오 옵션 (컨트롤 id, field, 값) — 클릭 즉시 반영(X-9).
    radios: Vec<(u32, u32, String)>,
}

const ID_SEARCH: u32 = 1002;
const ID_TREE: u32 = 1100; // 사이드바 트리(오너드로 LISTBOX)
const ID_FIELD_BASE: u32 = 1200; // +field(체크/EDIT 명령)
const ID_OPT_BASE: u32 = 1400; // +라디오 옵션 순번

static REGISTER: std::sync::Once = std::sync::Once::new();
const CLASS: PCWSTR = w!("NexaPrefs");
const PAD: i32 = 16;
const ROW_H: i32 = 30;
const CAT_W: i32 = 180;
const CAT_H: i32 = 32;
const SEARCH_H: i32 = 26;
const EDIT_W: i32 = 200;
const CLIENT_W: i32 = 760;
const CLIENT_H: i32 = 560;
/// 사이드바 선택 하이라이트(원본 스크린샷의 연회색 — 라이트 고정 네이티브 창).
const SEL_BGR: u32 = 0x00ECE7E4; // RGB(0xE4,0xE7,0xEC)의 BGR

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

    /// 우측 본문 x 시작(사이드바+구분선 이후).
    fn body_x(&self) -> i32 {
        PAD + CAT_W + PAD * 2
    }

    /// 현재 카테고리/검색어에 맞는 항목만 우측에 (재)구성 — 섹션 제목 + 항목 나열(X-9).
    unsafe fn rebuild(&mut self) {
        for h in self.rows.drain(..) {
            let _ = DestroyWindow(h);
        }
        self.editors.clear();
        self.radios.clear();
        let reg = registry();
        let q = self.query.to_lowercase();
        let x0 = self.body_x();
        let pane_w = (self.cw - x0 - PAD).max(120);
        // 섹션 제목(원본 스크린샷 "File List") — 검색 중이면 검색어 표기
        let node = tree_index(&self.category);
        let title = if q.is_empty() {
            node.map(|i| tr(TREE[i].1)).unwrap_or_default()
        } else {
            format!("\u{201C}{}\u{201D}", self.query)
        };
        // 섹션 구성(트리 개편 07-15): 검색 = 전 카테고리 매치 1섹션 ·
        // 그룹 노드 = 자식 카테고리별 부제목 섹션 · leaf = 그 카테고리 1섹션(부제목 없음)
        let sections: Vec<(Option<String>, Vec<&Entry>)> = if !q.is_empty() {
            vec![(
                None,
                reg.iter()
                    .filter(|e| tr(e.label_key).to_lowercase().contains(&q))
                    .collect(),
            )]
        } else if let Some(i) = node {
            let cats = tree_cats(i);
            let multi = cats.len() > 1;
            cats.into_iter()
                .map(|(key, lk)| {
                    (
                        multi.then(|| tr(lk)),
                        reg.iter().filter(|e| e.cat == key).collect(),
                    )
                })
                .collect()
        } else {
            Vec::new()
        };
        let th = mk(
            self.hwnd,
            self.title_font,
            w!("STATIC"),
            &title,
            0,
            x0,
            PAD,
            pane_w,
            28,
            0,
        );
        self.rows.push(th);
        let mut y = PAD + 40;
        let mut opt_seq = 0u32;
        for (sub, entries) in &sections {
            if entries.is_empty() {
                continue;
            }
            if let Some(sub) = sub {
                // 그룹 노드 = 자식 카테고리 부제목(하위 섹션 — 트리 개편 07-15)
                let sh = mk(
                    self.hwnd,
                    self.title_font,
                    w!("STATIC"),
                    sub,
                    0,
                    x0,
                    y,
                    pane_w,
                    24,
                    0,
                );
                self.rows.push(sh);
                y += 32;
            }
            for e in entries {
                let label = tr(e.label_key);
                match e.kind {
                    Kind::CheckBox => {
                        // 라벨 일체형 체크박스(원본 스크린샷) — 클릭 즉시 적용
                        let b = mk(
                            self.hwnd,
                            self.font,
                            w!("BUTTON"),
                            &label,
                            WS_TABSTOP.0 | BS_AUTOCHECKBOX as u32,
                            x0,
                            y,
                            pane_w,
                            24,
                            ID_FIELD_BASE + e.field,
                        );
                        let on = match e.field {
                            F_HIDDEN => self.values.show_hidden,
                            F_DOTFILES => self.values.show_dotfiles,
                            F_DOCK => self.values.dock,
                            _ => false,
                        };
                        SendMessageW(b, 0x00F1, Some(WPARAM(on as usize)), Some(LPARAM(0))); // BM_SETCHECK
                        self.rows.push(b);
                        self.editors.push((e.field, b));
                        y += ROW_H;
                    }
                    Kind::Radio(_) | Kind::LangRadio => {
                        // 캡션 + 세로 라디오 그룹(원본 스크린샷 "Where to show ..." 형식)
                        let cap = mk(
                            self.hwnd,
                            self.font,
                            w!("STATIC"),
                            &label,
                            0,
                            x0,
                            y,
                            pane_w,
                            20,
                            0,
                        );
                        self.rows.push(cap);
                        y += 26;
                        let opts: Vec<(String, String)> = match e.kind {
                            Kind::Radio(list) => {
                                list.iter().map(|(v, lk)| (v.to_string(), tr(lk))).collect()
                            }
                            _ => {
                                let mut o = vec![("system".to_string(), lang_label("system"))];
                                o.extend(
                                    self.values.langs.iter().map(|c| (c.clone(), lang_label(c))),
                                );
                                o
                            }
                        };
                        let cur = match e.field {
                            F_THEME => self.values.theme.clone(),
                            F_LANG => self.values.lang.clone(),
                            _ => String::new(),
                        };
                        for (gi, (val, olabel)) in opts.into_iter().enumerate() {
                            let id = ID_OPT_BASE + opt_seq;
                            opt_seq += 1;
                            let mut style = WS_TABSTOP.0 | BS_AUTORADIOBUTTON as u32;
                            if gi == 0 {
                                style |= WS_GROUP.0; // 라디오 그룹 경계
                            }
                            let r = mk(
                                self.hwnd,
                                self.font,
                                w!("BUTTON"),
                                &olabel,
                                style,
                                x0 + 8,
                                y,
                                pane_w - 8,
                                24,
                                id,
                            );
                            if val == cur {
                                SendMessageW(r, 0x00F1, Some(WPARAM(1)), Some(LPARAM(0)));
                            }
                            self.rows.push(r);
                            self.radios.push((id, e.field, val));
                            y += 28;
                        }
                        y += 8;
                    }
                    Kind::Text | Kind::Number => {
                        // [EDIT] [라벨] — 원본 스크린샷 "1000 ⌃⌄ Type-ahead input reset (ms)" 형식
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
                        let ed = mk(
                            self.hwnd,
                            self.font,
                            w!("EDIT"),
                            &val,
                            style,
                            x0,
                            y,
                            EDIT_W,
                            24,
                            ID_FIELD_BASE + e.field,
                        );
                        let lbl = mk(
                            self.hwnd,
                            self.font,
                            w!("STATIC"),
                            &label,
                            0,
                            x0 + EDIT_W + 8,
                            y + 3,
                            (pane_w - EDIT_W - 8).max(40),
                            20,
                            0,
                        );
                        self.rows.push(ed);
                        self.rows.push(lbl);
                        self.editors.push((e.field, ed));
                        y += ROW_H + 4;
                    }
                }
            }
            y += 10; // 섹션 간 여백
        }
        let _ = InvalidateRect(Some(self.hwnd), None, true);
    }

    /// 트리 목록 재적재(펼침 상태 반영) — 현재 선택 노드 유지.
    unsafe fn repopulate_tree(&mut self) {
        self.visible = tree_visible(&self.expanded);
        // 행 높이(LBS_OWNERDRAWFIXED — WM_MEASUREITEM은 상태 설정 전 도착이라 여기서)
        SendMessageW(
            self.tree,
            0x01A0, // LB_SETITEMHEIGHT
            Some(WPARAM(0)),
            Some(LPARAM((CAT_H - 4) as isize)),
        );
        SendMessageW(self.tree, 0x0184 /* LB_RESETCONTENT */, None, None);
        for &i in &self.visible {
            let w = windows::core::HSTRING::from(tr(TREE[i].1));
            SendMessageW(
                self.tree,
                0x0180, // LB_ADDSTRING
                None,
                Some(LPARAM(w.as_ptr() as isize)),
            );
        }
        if let Some(pos) =
            tree_index(&self.category).and_then(|i| self.visible.iter().position(|&v| v == i))
        {
            SendMessageW(
                self.tree,
                0x0186, /* LB_SETCURSEL */
                Some(WPARAM(pos)),
                None,
            );
        }
    }

    /// 편집 컨트롤 현재 값을 values에 흡수(적용/카테고리 전환 전).
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

/// 사이드바 트리 행 오너드로(전면 개편 07-15 — 단일 LISTBOX 컴포넌트): 들여쓰기 +
/// 그룹 ▸/▾ 마커 + 라벨, 선택 = 연회색 하이라이트(X-9 계승).
unsafe fn draw_tree_item(st: &PrefState, dis: &DRAWITEMSTRUCT) {
    let row = dis.itemID as usize;
    let Some(&node) = st.visible.get(row) else {
        // 목록 비었을 때의 -1 요청 — 배경만
        FillRect(dis.hDC, &dis.rcItem, GetSysColorBrush(COLOR_WINDOW));
        return;
    };
    let (key, label_key, depth) = TREE[node];
    let selected = st.category == key;
    if selected {
        let b = CreateSolidBrush(COLORREF(SEL_BGR));
        FillRect(dis.hDC, &dis.rcItem, b);
        let _ = DeleteObject(b.into());
    } else {
        FillRect(dis.hDC, &dis.rcItem, GetSysColorBrush(COLOR_WINDOW));
    }
    let old = SelectObject(dis.hDC, st.font.into());
    SetBkMode(dis.hDC, TRANSPARENT);
    // 그룹 = ▸(접힘)/▾(펼침) 마커, leaf = 마커 없음(트리 시각 규약 — rows.rs와 동일)
    let label = if tree_has_children(node) {
        format!(
            "{} {}",
            if st.expanded[node] { "▾" } else { "▸" },
            tr(label_key)
        )
    } else {
        tr(label_key)
    };
    let mut wide: Vec<u16> = label.encode_utf16().collect();
    let mut rc = RECT {
        left: dis.rcItem.left + 10 + depth * 14,
        top: dis.rcItem.top,
        right: dis.rcItem.right - 4,
        bottom: dis.rcItem.bottom,
    };
    DrawTextW(
        dis.hDC,
        &mut wide,
        &mut rc,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
    );
    SelectObject(dis.hDC, old);
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
                    // 사이드바 선택 표시 갱신(검색 중=선택 없음)
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                ID_TREE if notify == 1 => {
                    // LBN_SELCHANGE — 트리 노드 선택(전면 개편 07-15): 그룹 = 펼침 토글 +
                    // 하위 섹션 전체 표시, leaf = 그 카테고리 표시
                    let s = &mut *st;
                    let row = SendMessageW(s.tree, 0x0188 /* LB_GETCURSEL */, None, None).0;
                    let Some(&node) = usize::try_from(row).ok().and_then(|r| s.visible.get(r))
                    else {
                        return LRESULT(0);
                    };
                    s.harvest(); // 이동 전 현재 편집 값 보존
                    s.category = TREE[node].0.to_string();
                    if tree_has_children(node) {
                        s.expanded[node] = !s.expanded[node];
                        s.repopulate_tree();
                    }
                    s.query.clear();
                    set_text(s.search, ""); // 검색창 비우기
                    s.rebuild();
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                i if i >= ID_OPT_BASE => {
                    // 라디오 옵션 클릭(X-9) — 값 반영 + 즉시 적용
                    if let Some((_, field, val)) =
                        (*st).radios.iter().find(|(rid, _, _)| *rid == i).cloned()
                    {
                        match field {
                            F_THEME => (*st).values.theme = val,
                            F_LANG => (*st).values.lang = val,
                            _ => {}
                        }
                        (*st).harvest();
                        (*st).apply_now();
                    }
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
        WM_DRAWITEM => {
            let dis = &*(lparam.0 as *const DRAWITEMSTRUCT);
            if dis.CtlID == ID_TREE {
                draw_tree_item(&*st, dis);
                return LRESULT(1);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        // 라이트 고정 네이티브 창(원본 스크린샷) — 라벨·체크박스 배경을 창 배경과 일치
        m if m == WM_CTLCOLORSTATIC || m == WM_CTLCOLORBTN => {
            let hdc = windows::Win32::Graphics::Gdi::HDC(wparam.0 as *mut core::ffi::c_void);
            SetBkMode(hdc, TRANSPARENT);
            LRESULT(GetSysColorBrush(COLOR_WINDOW).0 as isize)
        }
        m if m == WM_CTLCOLOREDIT => DefWindowProcW(hwnd, msg, wparam, lparam),
        WM_SIZE => {
            // 리사이즈 추종(X-8) — 구분선 높이·본문 컨트롤 폭 재배치(최소화는 무시).
            let (w, h) = (
                (lparam.0 & 0xFFFF) as i32,
                ((lparam.0 >> 16) & 0xFFFF) as i32,
            );
            if w > 0 && h > 0 {
                (*st).cw = w;
                (*st).ch = h;
                let _ = MoveWindow(
                    (*st).divider,
                    PAD + CAT_W + PAD - 2,
                    PAD,
                    2,
                    (h - PAD * 2).max(0),
                    true,
                );
                // 트리 높이 추종(전면 개편 07-15)
                let ty = PAD + SEARCH_H + 10;
                let _ = MoveWindow((*st).tree, PAD, ty, CAT_W - 8, (h - ty - PAD).max(40), true);
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
const PREFS_STYLE: WINDOW_STYLE =
    WINDOW_STYLE(WS_POPUP.0 | WS_CAPTION.0 | WS_SYSMENU.0 | WS_THICKFRAME.0 | WS_MAXIMIZEBOX.0);

/// 섹션 제목용 글꼴(X-9) — 대화상자 글꼴 +5pt·세미볼드.
unsafe fn make_title_font(hwnd: HWND, spec: &DlgFont) -> HFONT {
    let dpi = GetDpiForWindow(hwnd).max(96);
    let h = -(((spec.size_pt + 5).clamp(9, 30) * dpi as i32) / 72);
    let face = windows::core::HSTRING::from(&*spec.family);
    CreateFontW(
        h,
        0,
        0,
        0,
        FW_SEMIBOLD.0 as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        DEFAULT_QUALITY,
        FF_DONTCARE.0 as u32,
        PCWSTR(face.as_ptr()),
    )
}

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
            // 라이트 고정(원본 스크린샷) — 사이드바·본문 모두 창 배경(白)
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
    let title_font = make_title_font(dlg, font_spec);
    let mut state = Box::new(PrefState {
        values,
        hwnd: dlg,
        owner,
        font,
        title_font,
        category: "general".into(), // 첫 화면 = 일반 그룹(하위 섹션 전체)
        query: String::new(),
        tree: HWND::default(),
        expanded: vec![true; TREE.len()], // 기본 = 전부 펼침
        visible: Vec::new(),
        search: HWND::default(),
        divider: HWND::default(),
        cw: CLIENT_W,
        ch: CLIENT_H,
        rows: Vec::new(),
        editors: Vec::new(),
        radios: Vec::new(),
    });
    // 사이드바 상단 검색창(원본 스크린샷 위치)
    state.search = mk(
        dlg,
        font,
        w!("EDIT"),
        "",
        (WS_BORDER | WS_TABSTOP).0 | ES_AUTOHSCROLL as u32,
        PAD,
        PAD,
        CAT_W - 8,
        SEARCH_H,
        ID_SEARCH,
    );
    // 검색창 플레이스홀더(EM_SETCUEBANNER — 미지원 환경은 무해한 no-op)
    {
        let cue: Vec<u16> = tr("pref.search.placeholder")
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        SendMessageW(
            state.search,
            0x1501,
            Some(WPARAM(1)),
            Some(LPARAM(cue.as_ptr() as isize)),
        );
    }
    // 좌측 계층 트리(전면 개편 07-15 — 오너드로 LISTBOX **단일 컴포넌트**):
    // 들여쓰기·▸/▾ 마커·선택 하이라이트, 클릭 = 우측 세부 표시(그룹=펼침 토글 겸)
    state.tree = mk(
        dlg,
        font,
        w!("LISTBOX"),
        "",
        (WS_TABSTOP | WS_VSCROLL).0
            | 0x0001 /* LBS_NOTIFY */
            | 0x0010 /* LBS_OWNERDRAWFIXED */
            | 0x0040 /* LBS_HASSTRINGS */
            | 0x0100, /* LBS_NOINTEGRALHEIGHT */
        PAD,
        PAD + SEARCH_H + 10,
        CAT_W - 8,
        CLIENT_H - (PAD + SEARCH_H + 10) - PAD,
        ID_TREE,
    );
    // 사이드바/본문 구분선
    state.divider = mk(
        dlg,
        font,
        w!("STATIC"),
        "",
        0x11, // SS_ETCHEDVERT
        PAD + CAT_W + PAD - 2,
        PAD,
        2,
        CLIENT_H - PAD * 2,
        0,
    );
    SetWindowLongPtrW(dlg, GWLP_USERDATA, &mut *state as *mut PrefState as isize);
    state.repopulate_tree();
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
    let _ = DeleteObject(title_font.into());
    // 즉시 적용 방식(X-8) — 닫기 = 확정. 최종 값 반환(미이탈 편집 값 포함, WM_CLOSE에서 수거).
    let mut v = state.values.clone();
    sanitize(&mut v);
    Some(v)
}
