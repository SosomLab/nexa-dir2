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
    GetDlgCtrlID, GetMessageW, GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, IsWindow,
    MoveWindow, RegisterClassW, SendMessageW, SetForegroundWindow, SetWindowLongPtrW,
    SetWindowTextW, TranslateMessage, BS_AUTOCHECKBOX, BS_AUTORADIOBUTTON, BS_OWNERDRAW,
    ES_AUTOHSCROLL, ES_NUMBER, GWLP_USERDATA, HMENU, MINMAXINFO, MSG, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_CTLCOLORBTN, WM_CTLCOLOREDIT, WM_CTLCOLORSTATIC,
    WM_DRAWITEM, WM_GETMINMAXINFO, WM_SETFONT, WM_SIZE, WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CHILD,
    WS_GROUP, WS_MAXIMIZEBOX, WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_THICKFRAME, WS_VISIBLE,
    WS_VSCROLL,
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
    pub term_wrap: bool,
    pub term_cols: i32,
    /// 컬럼 auto-fit 최대 폭(px @96dpi — 07-19).
    pub col_autofit_max: i32,
    pub dlg_font: String,
    pub dlg_font_size: i32,
    /// 폰트 슬롯(X-12 — 07-16): 기본/우클릭 메뉴/상태바/파일 목록 + 목록 장식 3종.
    pub base_font: String,
    pub base_font_size: i32,
    pub ctx_font: String,
    pub ctx_font_size: i32,
    pub status_font: String,
    pub status_font_size: i32,
    pub list_font: String,
    pub list_font_size: i32,
    pub list_folder_bold: bool,
    pub header_bold: bool,
    pub header_italic: bool,
    pub show_hidden: bool,
    pub show_dotfiles: bool,
    pub dock: bool,
    /// 폴더 우선 정렬(G-13).
    pub sort_folders_first: bool,
    /// 대소문자 구분 정렬(07-15).
    pub sort_case_sensitive: bool,
    /// Alt+↑ 자동 선택 배치("top"|"center"|"bottom" — 07-15).
    pub nav_up_align: String,
    /// 탭 더블클릭 동작("close"|"pin"|"lock" — 07-15).
    pub tab_dblclick: String,
    /// 타입어헤드(원본 docs/32 §7 — 07-15).
    pub typeahead_scope: String,
    pub typeahead_reset_ms: i32,
    pub typeahead_pos: i32,
    pub typeahead_special: bool,
    pub typeahead_space: bool,
    pub typeahead_backspace: bool,
}

/// 설정 항목 종류(편집 컨트롤 형태) — 레지스트리 최소 단위.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// 정적 라디오 그룹 — (값, 라벨 키) 목록(원본 스크린샷: 캡션 + 세로 라디오).
    Radio(&'static [(&'static str, &'static str)]),
    /// 언어 라디오(동적 — system + 발견 언어).
    LangRadio,
    /// 3×3 위치 피커(오너드로 이미지 버튼 — 원본 §7-A, QA 07-15).
    PosGrid,
    /// 자유 텍스트(EDIT) — X-12에서 글꼴이 Font 행으로 이관돼 현재 미사용(향후 텍스트 설정용).
    #[allow(dead_code)]
    Text,
    Number, // 숫자(EDIT ES_NUMBER — 리셋 ms·열 수 등)
    /// 폰트 행(X-12 — 원본 스크린샷): 캡션 + [패밀리 EDIT][크기 EDIT] **한 줄**.
    /// Entry.field = 패밀리, 인자 = 크기 필드 id.
    Font(u32),
    CheckBox, // 불리언(라벨 일체형 — 원본 스크린샷)
}

/// 설정 항목(레지스트리) — 카테고리·라벨키·설명키·종류·대상 필드 id.
struct Entry {
    cat: &'static str,
    label_key: &'static str,
    /// 설명 문장(X-10 ③ — 제목 아래 회색 한 줄). `라벨키.desc` 규약.
    desc_key: &'static str,
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
const F_FOLDERS_FIRST: u32 = 10;
const F_TERM_WRAP: u32 = 11;
const F_TERM_COLS: u32 = 12;
const F_CASE_SORT: u32 = 13;
const F_NAV_UP: u32 = 14;
const F_TAB_DBL: u32 = 15;
const F_TA_SCOPE: u32 = 16;
const F_TA_RESET: u32 = 17;
const F_TA_POS: u32 = 18;
const F_TA_SPECIAL: u32 = 19;
const F_TA_SPACE: u32 = 20;
const F_TA_BS: u32 = 21;
// 폰트 슬롯(X-12 — 07-16)
const F_BASE_FONT: u32 = 23;
const F_BASE_SIZE: u32 = 24;
const F_CTX_FONT: u32 = 25;
const F_CTX_SIZE: u32 = 26;
const F_STATUS_FONT: u32 = 27;
const F_STATUS_SIZE: u32 = 28;
const F_LIST_FONT: u32 = 29;
const F_LIST_SIZE: u32 = 30;
const F_FOLDER_BOLD: u32 = 31;
const F_HDR_BOLD: u32 = 32;
const F_HDR_ITALIC: u32 = 33;
const F_COL_AUTOFIT: u32 = 34;

/// 사이드바 **계층 트리**(전면 개편 07-15 — 사용자 요청: 단일 컴포넌트 트리 + 클릭 시
/// 우측 세부): 정적 pre-order (key, 라벨 키, 깊이). 자식 여부 = 다음 노드 깊이로 판정.
/// 그룹 노드 클릭 = 펼침 토글 + **하위 메뉴 링크 페이지**(세부는 하위 선택 시 — 드릴다운
/// 개편 07-15), leaf = 그 카테고리 항목(검색 중엔 검색어 매치 항목만).
const TREE: &[(&str, &str, i32)] = &[
    ("general", "pref.grp.general", 0),
    ("appearance", "pref.cat.appearance", 1),
    ("fonts", "pref.cat.fonts", 1),
    ("lang", "pref.cat.lang", 1),
    ("filelist", "pref.cat.list", 0),
    ("list", "pref.cat.listGeneral", 1),
    ("typeahead", "pref.cat.typeahead", 1),
    ("tabs", "pref.cat.tabs", 0),
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

/// 검색어 → 소문자 토큰(X-10 ② — 공백 구분 **AND 매칭**, VS Code 규약).
fn q_tokens(q: &str) -> Vec<String> {
    q.to_lowercase()
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// 라벨이 전 토큰을 포함하는가(AND).
fn label_hits(label: &str, tokens: &[String]) -> bool {
    let l = label.to_lowercase();
    tokens.iter().all(|t| l.contains(t))
}

/// 카테고리의 상세 설정 중 토큰 매치 항목 수(X-10 ① 매치 수·필터 공용).
fn cat_match_count(key: &str, tokens: &[String], reg: &[Entry]) -> usize {
    reg.iter()
        .filter(|e| e.cat == key && label_hits(&tr(e.label_key), tokens))
        .count()
}

/// 카테고리 매치(검색 기준 — 트리 필터·그룹 페이지 링크 공용): **라벨 매치** 또는
/// **하위 상세 설정(항목 라벨) 매치**.
fn cat_matches(key: &str, label_key: &str, tokens: &[String], reg: &[Entry]) -> bool {
    label_hits(&tr(label_key), tokens) || cat_match_count(key, tokens, reg) > 0
}

/// 검색 중 트리 필터(X-10 ① — 사용자 요청 07-15): **노드 라벨에 검색어가 있거나**,
/// 라벨엔 없어도 **하위 상세 설정(항목 라벨)에 검색어가 있는** 노드만 표시.
/// 매치 노드의 조상(경로)은 유지, 그룹 라벨 자체 매치면 하위 전체 표시.
fn tree_visible_search(tokens: &[String], reg: &[Entry]) -> Vec<usize> {
    let leaf_detail_hit = |key: &str| cat_match_count(key, tokens, reg) > 0;
    let mut keep = vec![false; TREE.len()];
    for i in 0..TREE.len() {
        let name_hit = label_hits(&tr(TREE[i].1), tokens);
        let detail_hit = tree_cats(i).iter().any(|(k, _)| leaf_detail_hit(k));
        if !(name_hit || detail_hit) {
            continue;
        }
        keep[i] = true;
        // 조상 경로 유지(트리 문맥 보존)
        let mut d = TREE[i].2;
        for j in (0..i).rev() {
            if TREE[j].2 < d {
                keep[j] = true;
                d = TREE[j].2;
            }
        }
        // 그룹 라벨 자체가 매치 = 하위 전체가 대상(카테고리 검색 의미)
        if name_hit && tree_has_children(i) {
            let base = TREE[i].2;
            for j in i + 1..TREE.len() {
                if TREE[j].2 <= base {
                    break;
                }
                keep[j] = true;
            }
        }
    }
    (0..TREE.len()).filter(|&i| keep[i]).collect()
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

/// 타입어헤드 검색 범위(원본 docs/32 §5 — 07-15).
const TA_SCOPE_OPTS: &[(&str, &str)] = &[
    ("global", "pref.taScope.global"),
    ("level", "pref.taScope.level"),
    ("visible", "pref.taScope.visible"),
];

/// 탭 더블클릭 동작(사용자 요청 07-15 — 기본 닫기·옵션 추가 예정).
const TAB_DBL_OPTS: &[(&str, &str)] = &[
    ("close", "pref.tabDbl.close"),
    ("pin", "pref.tabDbl.pin"),
    ("lock", "pref.tabDbl.lock"),
];

/// Alt+↑ 자동 선택 배치 옵션(07-15 — 상단/중단/하단).
const NAV_UP_OPTS: &[(&str, &str)] = &[
    ("top", "pref.align.top"),
    ("center", "pref.align.center"),
    ("bottom", "pref.align.bottom"),
];

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
            desc_key: "pref.theme.desc",
            kind: Kind::Radio(THEME_OPTS),
            field: F_THEME,
        },
        Entry {
            cat: "fonts",
            label_key: "pref.baseFont",
            desc_key: "pref.baseFont.desc",
            kind: Kind::Font(F_BASE_SIZE),
            field: F_BASE_FONT,
        },
        Entry {
            cat: "fonts",
            label_key: "pref.termFont",
            desc_key: "pref.consoleFont.desc",
            kind: Kind::Font(F_TERM_SIZE),
            field: F_TERM_FONT,
        },
        Entry {
            cat: "fonts",
            label_key: "pref.ctxFont",
            desc_key: "pref.ctxFont.desc",
            kind: Kind::Font(F_CTX_SIZE),
            field: F_CTX_FONT,
        },
        Entry {
            cat: "fonts",
            label_key: "pref.statusFont",
            desc_key: "pref.statusFont.desc",
            kind: Kind::Font(F_STATUS_SIZE),
            field: F_STATUS_FONT,
        },
        Entry {
            cat: "fonts",
            label_key: "pref.listFont",
            desc_key: "pref.listFont.desc",
            kind: Kind::Font(F_LIST_SIZE),
            field: F_LIST_FONT,
        },
        Entry {
            cat: "fonts",
            label_key: "pref.folderBold",
            desc_key: "pref.folderBold.desc",
            kind: Kind::CheckBox,
            field: F_FOLDER_BOLD,
        },
        Entry {
            cat: "fonts",
            label_key: "pref.hdrBold",
            desc_key: "pref.hdrBold.desc",
            kind: Kind::CheckBox,
            field: F_HDR_BOLD,
        },
        Entry {
            cat: "fonts",
            label_key: "pref.hdrItalic",
            desc_key: "pref.hdrItalic.desc",
            kind: Kind::CheckBox,
            field: F_HDR_ITALIC,
        },
        Entry {
            cat: "fonts",
            label_key: "pref.dlgFont",
            desc_key: "pref.dlgFont.desc",
            kind: Kind::Font(F_DLG_SIZE),
            field: F_DLG_FONT,
        },
        Entry {
            cat: "list",
            label_key: "pref.showHidden",
            desc_key: "pref.showHidden.desc",
            kind: Kind::CheckBox,
            field: F_HIDDEN,
        },
        Entry {
            cat: "list",
            label_key: "pref.showDotfiles",
            desc_key: "pref.showDotfiles.desc",
            kind: Kind::CheckBox,
            field: F_DOTFILES,
        },
        Entry {
            cat: "list",
            label_key: "pref.colAutofitMax",
            desc_key: "pref.colAutofitMax.desc",
            kind: Kind::Number,
            field: F_COL_AUTOFIT,
        },
        Entry {
            cat: "list",
            label_key: "pref.sortFoldersFirst",
            desc_key: "pref.sortFoldersFirst.desc",
            kind: Kind::CheckBox,
            field: F_FOLDERS_FIRST,
        },
        Entry {
            cat: "list",
            label_key: "pref.sortCaseSensitive",
            desc_key: "pref.sortCaseSensitive.desc",
            kind: Kind::CheckBox,
            field: F_CASE_SORT,
        },
        Entry {
            cat: "list",
            label_key: "pref.navUpAlign",
            desc_key: "pref.navUpAlign.desc",
            kind: Kind::Radio(NAV_UP_OPTS),
            field: F_NAV_UP,
        },
        Entry {
            cat: "tabs",
            label_key: "pref.tabDblclick",
            desc_key: "pref.tabDblclick.desc",
            kind: Kind::Radio(TAB_DBL_OPTS),
            field: F_TAB_DBL,
        },
        Entry {
            cat: "typeahead",
            label_key: "pref.taScope",
            desc_key: "pref.taScope.desc",
            kind: Kind::Radio(TA_SCOPE_OPTS),
            field: F_TA_SCOPE,
        },
        Entry {
            cat: "typeahead",
            label_key: "pref.taReset",
            desc_key: "pref.taReset.desc",
            kind: Kind::Number,
            field: F_TA_RESET,
        },
        Entry {
            cat: "typeahead",
            label_key: "pref.taSpecial",
            desc_key: "pref.taSpecial.desc",
            kind: Kind::CheckBox,
            field: F_TA_SPECIAL,
        },
        Entry {
            cat: "typeahead",
            label_key: "pref.taSpace",
            desc_key: "pref.taSpace.desc",
            kind: Kind::CheckBox,
            field: F_TA_SPACE,
        },
        Entry {
            cat: "typeahead",
            label_key: "pref.taBackspace",
            desc_key: "pref.taBackspace.desc",
            kind: Kind::CheckBox,
            field: F_TA_BS,
        },
        Entry {
            cat: "typeahead",
            label_key: "pref.taPos",
            desc_key: "pref.taPos.desc",
            kind: Kind::PosGrid,
            field: F_TA_POS,
        },
        Entry {
            cat: "terminal",
            label_key: "pref.termWrap",
            desc_key: "pref.termWrap.desc",
            kind: Kind::CheckBox,
            field: F_TERM_WRAP,
        },
        Entry {
            cat: "terminal",
            label_key: "pref.termCols",
            desc_key: "pref.termCols.desc",
            kind: Kind::Number,
            field: F_TERM_COLS,
        },
        Entry {
            cat: "dock",
            label_key: "pref.dock",
            desc_key: "pref.dock.desc",
            kind: Kind::CheckBox,
            field: F_DOCK,
        },
        Entry {
            cat: "lang",
            label_key: "pref.lang",
            desc_key: "pref.lang.desc",
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
    /// 트리 디스클로저 글리프 폰트(Segoe MDL2 — 파일 목록과 동일 규약, 07-18).
    icon_font: HFONT,
    /// 현재 선택 노드 key(빈 검색 시)·검색어(있으면 전 카테고리에서 필터).
    category: String,
    query: String,
    /// 사이드바 트리(오너드로 LISTBOX 단일 컴포넌트 — 전면 개편 07-15).
    tree: HWND,
    /// TREE 인덱스별 펼침 상태(기본 = 전부 펼침).
    expanded: Vec<bool>,
    /// 현재 가시 노드(트리 목록 행 → TREE 인덱스).
    visible: Vec<usize>,
    /// 검색 중 TREE 인덱스별 매치 항목 수(X-10 ① — 트리 행 "(N)" 표기).
    search_counts: Vec<usize>,
    /// 수정됨 바(X-10 ④) 브러시 — 창 수명 동안 재사용(WM_CTLCOLORSTATIC).
    accent_brush: windows::Win32::Graphics::Gdi::HBRUSH,
    /// 상단 검색창(사이드바 상단 — 원본 스크린샷 위치).
    search: HWND,
    /// 사이드바/본문 세로 구분선(리사이즈 시 높이 추종).
    divider: HWND,
    /// 현재 클라이언트 크기(리사이즈 추종 레이아웃 — X-8).
    cw: i32,
    ch: i32,
    /// 본문 세로 스크롤(QA 07-15 — 항목이 창보다 길 때 휠 스크롤). 재구성 시 오프셋.
    scroll_y: i32,
    /// 마지막 재구성의 콘텐츠 전체 높이(스크롤 상한 계산).
    content_h: i32,
    /// 동적 생성한 우측 컨트롤들(카테고리/검색 변경 시 파괴·재생성).
    rows: Vec<HWND>,
    /// 각 편집 컨트롤 (field, hwnd) — 값 수거용(체크박스·EDIT).
    editors: Vec<(u32, HWND)>,
    /// 라디오 옵션 (컨트롤 id, field, 값) — 클릭 즉시 반영(X-9).
    radios: Vec<(u32, u32, String)>,
}

const ID_SEARCH: u32 = 1002; // 검색박스(ctl::searchbox — 내장 ✕는 컨트롤 소관, 07-16)
/// 수정됨 표시 바(X-10 ④ — 기본값과 다른 항목 좌측 세로 accent). 여러 컨트롤 공유 id.
const ID_MODBAR: u32 = 1997;
/// 설명 문장(X-10 ③ — 회색 텍스트). 여러 컨트롤 공유 id.
const ID_DESC: u32 = 1998;
const ID_TREE: u32 = 1100; // 사이드바 트리(오너드로 LISTBOX)
const ID_FIELD_BASE: u32 = 1200; // +field(체크/EDIT 명령)
const ID_OPT_BASE: u32 = 1400; // +라디오 옵션 순번
/// 그룹 페이지의 하위 메뉴 링크(드릴다운 개편 07-15) — +TREE 인덱스.
const ID_NAV_BASE: u32 = 1600;
/// 타입어헤드 위치 3×3 피커 셀(오너드로 — QA 07-15) — +0..9.
const ID_POS_BASE: u32 = 1900;

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
    v.term_cols = v.term_cols.clamp(80, 1000);
    v.col_autofit_max = v.col_autofit_max.clamp(50, 2000);
    v.typeahead_reset_ms = v.typeahead_reset_ms.clamp(200, 10_000);
    v.typeahead_pos = v.typeahead_pos.clamp(0, 8);
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
        // 수확 목록을 **파괴 전에** 비운다(QA 07-16 진범): 포커스 EDIT의 DestroyWindow가
        // EN_KILLFOCUS를 동기 발화 → 재진입 harvest가 파괴된 컨트롤에서 빈 문자열을
        // 수확해 values를 덮던 결함(스크롤 시 폰트 이름 공백). 비워두면 재진입 무해.
        self.editors.clear();
        self.radios.clear();
        for h in self.rows.drain(..) {
            let _ = DestroyWindow(h);
        }
        let reg = registry();
        let x0 = self.body_x();
        let pane_w = (self.cw - x0 - PAD).max(120);
        // 표시 모드(드릴다운 개편 07-15 — 사용자 QA): **그룹 = 하위 메뉴 목록만**
        // (세부는 하위 메뉴 선택 시), leaf = 그 카테고리 항목(검색 중엔 **검색어 매치
        // 항목만**), 검색 중 미선택(category 빈 값) = 전 카테고리 매치 목록.
        let node = tree_index(&self.category);
        let is_group = node.is_some_and(tree_has_children);
        let tokens = q_tokens(&self.query);
        // 전역 검색 페이지 제목 = “검색어” — N개 일치(X-10 ② 결과 수)
        let title = if let Some(i) = node {
            tr(TREE[i].1)
        } else {
            let n = reg
                .iter()
                .filter(|e| label_hits(&tr(e.label_key), &tokens))
                .count();
            format!(
                "\u{201C}{}\u{201D} — {}",
                self.query,
                crate::i18n::trf("pref.search.results", &[&n.to_string()])
            )
        };
        let th = mk(
            self.hwnd,
            self.title_font,
            w!("STATIC"),
            &title,
            0,
            x0,
            PAD - self.scroll_y,
            pane_w,
            28,
            0,
        );
        self.rows.push(th);
        let mut y = PAD + 40 - self.scroll_y;
        let mut opt_seq = 0u32;
        if is_group {
            // 그룹 페이지 = 하위 메뉴 링크(클릭 = 그 메뉴로 이동 — SS_NOTIFY).
            // 제목보다 **작은 본문 폰트**(QA 07-16 — 제목과 위계 구분). 검색 중엔
            // 매치 기준(라벨/상세 — 트리 필터와 동일) 자식만.
            for (key, lk) in tree_cats(node.unwrap_or_default()) {
                if key == self.category {
                    continue; // 그룹 자신(직속 상세는 아래 항목 목록이 담당)
                }
                if !tokens.is_empty() && !cat_matches(key, lk, &tokens, &reg) {
                    continue;
                }
                let Some(ti) = tree_index(key) else { continue };
                let link = mk(
                    self.hwnd,
                    self.font,
                    w!("STATIC"),
                    &tr(lk),
                    0x0100, // SS_NOTIFY — STN_CLICKED로 이동
                    x0,
                    y,
                    pane_w,
                    20,
                    ID_NAV_BASE + ti as u32,
                );
                self.rows.push(link);
                y += 28;
            }
            y += 6; // 링크 ↔ 직속 상세 간격(QA 07-16)
        }
        {
            // 항목 목록: **그룹 = 직속 상세**(cat == 그룹 key — 있을 때만 링크 아래,
            // QA 07-16) · 검색 미선택 = 전 카테고리 매치 · leaf = 그 카테고리 항목
            // (검색 중 = 라벨 매치만 — 메뉴명 매치로 진입해 상세 매치가 0이면 전체 표시)
            let list: Vec<&Entry> = if is_group {
                reg.iter()
                    .filter(|e| {
                        e.cat == self.category
                            && (tokens.is_empty() || label_hits(&tr(e.label_key), &tokens))
                    })
                    .collect()
            } else if node.is_none() {
                reg.iter()
                    .filter(|e| label_hits(&tr(e.label_key), &tokens))
                    .collect()
            } else {
                let matched: Vec<&Entry> = reg
                    .iter()
                    .filter(|e| {
                        e.cat == self.category
                            && (tokens.is_empty() || label_hits(&tr(e.label_key), &tokens))
                    })
                    .collect();
                if matched.is_empty() && !tokens.is_empty() {
                    reg.iter().filter(|e| e.cat == self.category).collect()
                } else {
                    matched
                }
            };
            for e in list {
                // 전역 검색 결과 = "카테고리: 항목" 접두(X-10 ⑤ — VS Code 규약)
                let label = if node.is_none() {
                    let cat_label = tree_index(e.cat).map(|i| tr(TREE[i].1)).unwrap_or_default();
                    format!("{cat_label}: {}", tr(e.label_key))
                } else {
                    tr(e.label_key)
                };
                // 수정됨 표시(X-10 ④) — 기본값과 다른 항목 좌측 세로 accent 바
                let y0 = y;
                match e.kind {
                    Kind::PosGrid => {
                        // 3×3 이미지 피커(원본 §7-A — QA 07-15 라디오 9종 대체)
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
                        y += 24;
                        for gi in 0..9u32 {
                            let (col, row_i) = ((gi % 3) as i32, (gi / 3) as i32);
                            let b = mk(
                                self.hwnd,
                                self.font,
                                w!("BUTTON"),
                                "",
                                WS_TABSTOP.0 | BS_OWNERDRAW as u32,
                                x0 + col * 30,
                                y + row_i * 30,
                                26,
                                26,
                                ID_POS_BASE + gi,
                            );
                            self.rows.push(b);
                        }
                        y += 3 * 30 + 6;
                    }
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
                            F_FOLDERS_FIRST => self.values.sort_folders_first,
                            F_TERM_WRAP => self.values.term_wrap,
                            F_CASE_SORT => self.values.sort_case_sensitive,
                            F_TA_SPECIAL => self.values.typeahead_special,
                            F_TA_SPACE => self.values.typeahead_space,
                            F_TA_BS => self.values.typeahead_backspace,
                            F_FOLDER_BOLD => self.values.list_folder_bold,
                            F_HDR_BOLD => self.values.header_bold,
                            F_HDR_ITALIC => self.values.header_italic,
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
                            F_NAV_UP => self.values.nav_up_align.clone(),
                            F_TAB_DBL => self.values.tab_dblclick.clone(),
                            F_TA_SCOPE => self.values.typeahead_scope.clone(),
                            F_TA_POS => self.values.typeahead_pos.to_string(),
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
                    Kind::Font(size_field) => {
                        // 폰트 행(X-12 — 사용자 확정: 이름+크기 **한 줄**): 캡션 →
                        // [패밀리 EDIT 넓게][크기 EDIT 좁게(숫자)]
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
                        y += 24;
                        let fam = self.font_value(e.field);
                        // 패밀리 = ctl::fontbox(사용자 요청 07-16): 클릭 = 설치 글꼴
                        // 드롭다운(자기 글꼴 렌더)·타입어헤드 HUD·쉼표 체인 선택 규칙.
                        // 확정(선택/포커스 이탈) = EN_KILLFOCUS 재발행 → 기존 즉시 적용.
                        let ed = crate::ctl::fontbox::create(
                            self.hwnd,
                            x0,
                            y,
                            EDIT_W,
                            24,
                            ID_FIELD_BASE + e.field,
                            self.font,
                        );
                        set_text(ed, &fam);
                        let sz = self.font_value(size_field);
                        // 크기 = **입력 가능한 콤보**(사용자 확정 07-16): 프리셋 + 직접 입력,
                        // 선택/Enter = 즉시 적용(CBN_SELCHANGE·모달 펌프 VK_RETURN).
                        let ed2 = mk(
                            self.hwnd,
                            self.font,
                            w!("COMBOBOX"),
                            "",
                            WS_TABSTOP.0 | WS_VSCROLL.0 | 0x0002, /* CBS_DROPDOWN */
                            x0 + EDIT_W + 8,
                            y,
                            64,
                            240, // 닫힘+드롭다운 목록 높이
                            ID_FIELD_BASE + size_field,
                        );
                        for v in [8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 32] {
                            let w16: Vec<u16> = v
                                .to_string()
                                .encode_utf16()
                                .chain(std::iter::once(0))
                                .collect();
                            SendMessageW(
                                ed2,
                                0x0143, // CB_ADDSTRING
                                None,
                                Some(LPARAM(w16.as_ptr() as isize)),
                            );
                        }
                        set_text(ed2, &sz);
                        self.rows.push(ed);
                        self.rows.push(ed2);
                        self.editors.push((e.field, ed));
                        self.editors.push((size_field, ed2));
                        y += ROW_H + 4;
                    }
                    Kind::Text | Kind::Number => {
                        // [EDIT] [라벨] — 원본 스크린샷 "1000 ⌃⌄ Type-ahead input reset (ms)" 형식
                        let val = match e.field {
                            F_TERM_FONT => self.values.term_font.clone(),
                            F_TERM_SIZE => self.values.term_font_size.to_string(),
                            F_TERM_COLS => self.values.term_cols.to_string(),
                            F_COL_AUTOFIT => self.values.col_autofit_max.to_string(),
                            F_TA_RESET => self.values.typeahead_reset_ms.to_string(),
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
                // 설명 문장(X-10 ③) — 제목/컨트롤 아래 회색 한 줄(ID_DESC 색 분기)
                let desc = tr(e.desc_key);
                if desc != e.desc_key {
                    let d = mk(
                        self.hwnd,
                        self.font,
                        w!("STATIC"),
                        &desc,
                        0,
                        x0 + 2,
                        y,
                        pane_w - 2,
                        18,
                        ID_DESC,
                    );
                    self.rows.push(d);
                    y += 26;
                }
                // 수정됨 표시(X-10 ④) — 기본값과 다른 항목 좌측 세로 accent 바
                if self.is_modified(e.field) {
                    let bar = mk(
                        self.hwnd,
                        self.font,
                        w!("STATIC"),
                        "",
                        0,
                        x0 - 8,
                        y0 + 2,
                        3,
                        (y - y0 - 8).max(18),
                        ID_MODBAR,
                    );
                    self.rows.push(bar);
                }
            }
        }
        self.content_h = y + self.scroll_y + PAD; // 스크롤 상한 계산용(QA 07-15)
        let _ = InvalidateRect(Some(self.hwnd), None, true);
    }

    /// 폰트 행 필드의 현재 표시값(X-12 — 패밀리/크기 공용).
    fn font_value(&self, field: u32) -> String {
        let v = &self.values;
        match field {
            F_BASE_FONT => v.base_font.clone(),
            F_BASE_SIZE => v.base_font_size.to_string(),
            F_CTX_FONT => v.ctx_font.clone(),
            F_CTX_SIZE => v.ctx_font_size.to_string(),
            F_STATUS_FONT => v.status_font.clone(),
            F_STATUS_SIZE => v.status_font_size.to_string(),
            F_LIST_FONT => v.list_font.clone(),
            F_LIST_SIZE => v.list_font_size.to_string(),
            F_TERM_FONT => v.term_font.clone(),
            F_TERM_SIZE => v.term_font_size.to_string(),
            F_DLG_FONT => v.dlg_font.clone(),
            F_DLG_SIZE => v.dlg_font_size.to_string(),
            _ => String::new(),
        }
    }

    /// 항목이 기본값과 다른가(X-10 ④ — config::Settings::default 단일 원천).
    fn is_modified(&self, field: u32) -> bool {
        let v = &self.values;
        let d = crate::config::Settings::default();
        match field {
            F_THEME => v.theme != d.theme,
            F_LANG => v.lang != d.lang,
            F_TERM_FONT => v.term_font != d.term_font,
            F_TERM_SIZE => v.term_font_size != d.term_font_size,
            F_DLG_FONT => v.dlg_font != d.dlg_font,
            F_DLG_SIZE => v.dlg_font_size != d.dlg_font_size,
            F_HIDDEN => v.show_hidden != d.show_hidden,
            F_DOTFILES => v.show_dotfiles != d.show_dotfiles,
            F_DOCK => v.dock != d.dock,
            F_FOLDERS_FIRST => v.sort_folders_first != d.sort_folders_first,
            F_TERM_WRAP => v.term_wrap != d.term_wrap,
            F_TERM_COLS => v.term_cols != d.term_cols,
            F_COL_AUTOFIT => v.col_autofit_max != d.col_autofit_max,
            F_CASE_SORT => v.sort_case_sensitive != d.sort_case_sensitive,
            F_NAV_UP => v.nav_up_align != d.nav_up_align,
            F_TAB_DBL => v.tab_dblclick != d.tab_dblclick,
            F_TA_SCOPE => v.typeahead_scope != d.typeahead_scope,
            F_TA_RESET => v.typeahead_reset_ms != d.typeahead_reset_ms,
            F_TA_POS => v.typeahead_pos != d.typeahead_pos,
            F_BASE_FONT => v.base_font != d.base_font,
            F_BASE_SIZE => v.base_font_size != d.base_font_size,
            F_CTX_FONT => v.ctx_font != d.ctx_font,
            F_CTX_SIZE => v.ctx_font_size != d.ctx_font_size,
            F_STATUS_FONT => v.status_font != d.status_font,
            F_STATUS_SIZE => v.status_font_size != d.status_font_size,
            F_LIST_FONT => v.list_font != d.list_font,
            F_LIST_SIZE => v.list_font_size != d.list_font_size,
            F_FOLDER_BOLD => v.list_folder_bold != d.list_folder_bold,
            F_HDR_BOLD => v.header_bold != d.header_bold,
            F_HDR_ITALIC => v.header_italic != d.header_italic,
            F_TA_SPECIAL => v.typeahead_special != d.typeahead_special,
            F_TA_SPACE => v.typeahead_space != d.typeahead_space,
            F_TA_BS => v.typeahead_backspace != d.typeahead_backspace,
            _ => false,
        }
    }

    /// 트리 목록 재적재 — 검색 중 = 매치 필터(라벨/상세 — X-10 ①), 아니면 펼침 상태.
    /// 현재 선택 노드가 가시 목록에 있으면 선택 유지. 검색 중 매치 수(N)도 함께 계산.
    unsafe fn repopulate_tree(&mut self) {
        let reg = registry();
        let tokens = q_tokens(&self.query);
        self.visible = if tokens.is_empty() {
            self.search_counts = vec![0; TREE.len()];
            tree_visible(&self.expanded)
        } else {
            // 노드별 매치 수 = 커버 카테고리들의 매치 항목 합(그룹=하위 합)
            self.search_counts = (0..TREE.len())
                .map(|i| {
                    tree_cats(i)
                        .iter()
                        .map(|(k, _)| cat_match_count(k, &tokens, &reg))
                        .sum()
                })
                .collect();
            tree_visible_search(&tokens, &reg)
        };
        // 행 높이(LBS_OWNERDRAWFIXED — WM_MEASUREITEM은 상태 설정 전 도착이라 여기서)
        SendMessageW(
            self.tree,
            0x01A0, // LB_SETITEMHEIGHT
            Some(WPARAM(0)),
            Some(LPARAM((CAT_H - 4) as isize)),
        );
        SendMessageW(self.tree, 0x0184 /* LB_RESETCONTENT */, None, None);
        for &i in &self.visible {
            // 검색 중 매치 수 "(N)"은 저장 문자열에도 포함(오너드로와 일치 — 접근성/판독)
            let mut label = tr(TREE[i].1);
            if !tokens.is_empty() {
                if let Some(n) = self.search_counts.get(i).filter(|n| **n > 0) {
                    label.push_str(&format!(" ({n})"));
                }
            }
            let w = windows::core::HSTRING::from(label);
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
                F_TERM_COLS => self.values.term_cols = get_text(hw).trim().parse().unwrap_or(240),
                F_COL_AUTOFIT => {
                    self.values.col_autofit_max = get_text(hw).trim().parse().unwrap_or(400)
                }
                F_TA_RESET => {
                    self.values.typeahead_reset_ms = get_text(hw).trim().parse().unwrap_or(1000)
                }
                F_DLG_FONT => self.values.dlg_font = get_text(hw),
                F_DLG_SIZE => self.values.dlg_font_size = get_text(hw).trim().parse().unwrap_or(9),
                F_BASE_FONT => self.values.base_font = get_text(hw),
                F_BASE_SIZE => {
                    self.values.base_font_size = get_text(hw).trim().parse().unwrap_or(12)
                }
                F_CTX_FONT => self.values.ctx_font = get_text(hw),
                F_CTX_SIZE => self.values.ctx_font_size = get_text(hw).trim().parse().unwrap_or(12),
                F_STATUS_FONT => self.values.status_font = get_text(hw),
                F_STATUS_SIZE => {
                    self.values.status_font_size = get_text(hw).trim().parse().unwrap_or(12)
                }
                F_LIST_FONT => self.values.list_font = get_text(hw),
                F_LIST_SIZE => {
                    self.values.list_font_size = get_text(hw).trim().parse().unwrap_or(12)
                }
                F_HIDDEN | F_DOTFILES | F_DOCK | F_FOLDERS_FIRST | F_TERM_WRAP | F_CASE_SORT
                | F_TA_SPECIAL | F_TA_SPACE | F_TA_BS | F_FOLDER_BOLD | F_HDR_BOLD
                | F_HDR_ITALIC => {
                    let on = SendMessageW(hw, 0x00F0, None, None).0 == 1; // BM_GETCHECK
                    match field {
                        F_HIDDEN => self.values.show_hidden = on,
                        F_DOTFILES => self.values.show_dotfiles = on,
                        F_DOCK => self.values.dock = on,
                        F_FOLDERS_FIRST => self.values.sort_folders_first = on,
                        F_TERM_WRAP => self.values.term_wrap = on,
                        F_CASE_SORT => self.values.sort_case_sensitive = on,
                        F_TA_SPECIAL => self.values.typeahead_special = on,
                        F_TA_SPACE => self.values.typeahead_space = on,
                        F_TA_BS => self.values.typeahead_backspace = on,
                        F_FOLDER_BOLD => self.values.list_folder_bold = on,
                        F_HDR_BOLD => self.values.header_bold = on,
                        F_HDR_ITALIC => self.values.header_italic = on,
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
    // 그룹 디스클로저 = **파일 목록과 동일 MDL2 셰브론**(E76C 접힘/E70D 펼침 —
    // 사용자 확정 07-18: 텍스트 ▸/▾ 폐기). 검색 중 = 강제 펼침 표시.
    // 마커 존 = **고정 폭 상시 예약**(사용자 확정 07-18 — 파일뷰 규약 동일):
    // 하위 유무와 무관하게 같은 레벨의 라벨 x가 일치한다(그룹만 글리프 표시).
    let base_left = dis.rcItem.left + 10 + depth * 14;
    let text_left = base_left + 14;
    if tree_has_children(node) {
        let glyph = if st.expanded[node] || !st.query.is_empty() {
            "\u{E70D}" // ChevronDown(펼침)
        } else {
            "\u{E76C}" // ChevronRight(접힘)
        };
        let prev = SelectObject(dis.hDC, st.icon_font.into());
        let mut g16: Vec<u16> = glyph.encode_utf16().collect();
        let mut grc = RECT {
            left: base_left,
            top: dis.rcItem.top,
            right: base_left + 12,
            bottom: dis.rcItem.bottom,
        };
        DrawTextW(
            dis.hDC,
            &mut g16,
            &mut grc,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
        SelectObject(dis.hDC, prev);
    }
    let mut label = tr(label_key);
    if !st.query.is_empty() {
        if let Some(n) = st.search_counts.get(node).filter(|n| **n > 0) {
            label.push_str(&format!(" ({n})"));
        }
    }
    let mut wide: Vec<u16> = label.encode_utf16().collect();
    let mut rc = RECT {
        left: text_left,
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

/// 3×3 위치 피커 셀 오너드로(QA 07-15) — 선택 = accent 테두리+점, 비선택 = 회색.
unsafe fn draw_pos_cell(st: &PrefState, dis: &DRAWITEMSTRUCT) {
    let idx = (dis.CtlID - ID_POS_BASE) as i32;
    let selected = st.values.typeahead_pos == idx;
    FillRect(dis.hDC, &dis.rcItem, GetSysColorBrush(COLOR_WINDOW));
    let border = CreateSolidBrush(COLORREF(if selected { 0x00D4_7800 } else { 0x00C8_C8C8 }));
    let r = dis.rcItem;
    let t = if selected { 2 } else { 1 };
    // 테두리(두께 t)
    for (x, y, w, h) in [
        (r.left, r.top, r.right - r.left, t),
        (r.left, r.bottom - t, r.right - r.left, t),
        (r.left, r.top, t, r.bottom - r.top),
        (r.right - t, r.top, t, r.bottom - r.top),
    ] {
        let rc = RECT {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        };
        FillRect(dis.hDC, &rc, border);
    }
    // 중앙 점(선택 = accent·비선택 = 회색)
    let (cx, cy) = ((r.left + r.right) / 2, (r.top + r.bottom) / 2);
    let dot = RECT {
        left: cx - 3,
        top: cy - 3,
        right: cx + 3,
        bottom: cy + 3,
    };
    FillRect(dis.hDC, &dot, border);
    let _ = DeleteObject(border.into());
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
                    // EN_CHANGE — 검색어 갱신·트리 필터(X-10 ①)·우측 재구성.
                    // 입력/변경 = 선택 해제(전역 매치 페이지) · 명시적 비움 = 기본 노드 복귀.
                    let s = &mut *st;
                    s.query = get_text(HWND(lparam.0 as *mut core::ffi::c_void));
                    s.category = if s.query.is_empty() {
                        "general".into()
                    } else {
                        String::new()
                    };
                    s.scroll_y = 0;
                    s.repopulate_tree();
                    s.rebuild();
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
                    s.scroll_y = 0;
                    // 검색어는 **메뉴 탐색 중 유지**(사용자 요청 07-15 — 명시적 삭제만
                    // 지움). 펼침 토글은 일반 모드만(검색 중 = 필터가 하위 강제 표시).
                    if s.query.is_empty() && tree_has_children(node) {
                        s.expanded[node] = !s.expanded[node];
                        s.repopulate_tree();
                    }
                    s.rebuild();
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                i if (ID_NAV_BASE..ID_NAV_BASE + TREE.len() as u32).contains(&i) && notify == 0 => {
                    // 그룹 페이지의 하위 메뉴 링크 클릭(STN_CLICKED) — 그 메뉴로 이동
                    // (검색어 유지 — 트리 클릭과 동일 규약)
                    let s = &mut *st;
                    let ti = (i - ID_NAV_BASE) as usize;
                    s.harvest();
                    s.category = TREE[ti].0.to_string();
                    s.scroll_y = 0;
                    if s.query.is_empty() {
                        // 조상 펼침(선택 노드 가시화)
                        let mut d = TREE[ti].2;
                        for j in (0..ti).rev() {
                            if TREE[j].2 < d {
                                s.expanded[j] = true;
                                d = TREE[j].2;
                            }
                        }
                    }
                    s.repopulate_tree();
                    s.rebuild();
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                i if (ID_POS_BASE..ID_POS_BASE + 9).contains(&i) && notify == 0 => {
                    // 3×3 피커 클릭(QA 07-15) — 값 반영 + 즉시 적용 + 셀 재도장
                    (*st).values.typeahead_pos = (i - ID_POS_BASE) as i32;
                    (*st).harvest();
                    (*st).apply_now();
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
                            F_NAV_UP => (*st).values.nav_up_align = val,
                            F_TAB_DBL => (*st).values.tab_dblclick = val,
                            F_TA_SCOPE => (*st).values.typeahead_scope = val,
                            F_TA_POS => (*st).values.typeahead_pos = val.parse().unwrap_or(6),
                            _ => {}
                        }
                        (*st).harvest();
                        (*st).apply_now();
                    }
                }
                // 크기 콤보(X-12 QA): 드롭다운 선택 = **즉시 적용** — CBN_SELCHANGE
                // 시점엔 에디트가 아직 이전 값이라 선택 항목 텍스트를 직접 반영.
                i if i >= ID_FIELD_BASE && notify == 1 => {
                    let combo = HWND(lparam.0 as *mut core::ffi::c_void);
                    let sel = SendMessageW(combo, 0x0147 /* CB_GETCURSEL */, None, None).0;
                    if sel >= 0 {
                        let mut buf = [0u16; 16];
                        let n = SendMessageW(
                            combo,
                            0x0148, // CB_GETLBTEXT
                            Some(WPARAM(sel as usize)),
                            Some(LPARAM(buf.as_mut_ptr() as isize)),
                        )
                        .0;
                        if n > 0 {
                            let t = String::from_utf16_lossy(&buf[..n as usize]);
                            set_text(combo, &t); // 에디트부 확정 → harvest가 새 값 수확
                        }
                    }
                    (*st).harvest();
                    (*st).apply_now();
                }
                // VS Code식 즉시 적용(X-8): 체크박스 클릭(BN_CLICKED=0)은 즉시,
                // EDIT(글꼴·크기)은 포커스 이탈(EN_KILLFOCUS=0x0200 — 콤보는
                // CBN_KILLFOCUS=4) 시 값 확정 후 적용(EN_CHANGE는 무시).
                i if i >= ID_FIELD_BASE && (notify == 0 || notify == 0x0200 || notify == 4) => {
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
            if (ID_POS_BASE..ID_POS_BASE + 9).contains(&dis.CtlID) {
                draw_pos_cell(&*st, dis);
                return LRESULT(1);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        // 본문 휠 스크롤(QA 07-15 — 항목이 창보다 길 때)
        0x020A /* WM_MOUSEWHEEL */ => {
            let delta = (wparam.0 >> 16) as i16 as i32;
            let s = &mut *st;
            let max = (s.content_h - s.ch).max(0);
            let ny = (s.scroll_y - delta / 120 * 48).clamp(0, max);
            if ny != s.scroll_y {
                s.scroll_y = ny;
                s.harvest();
                s.rebuild();
            }
            LRESULT(0)
        }
        // 라이트 고정 네이티브 창(원본 스크린샷) — 라벨·체크박스 배경을 창 배경과 일치.
        // ID_MODBAR = accent 채움(수정됨 바 — X-10 ④) · ID_DESC = 회색 텍스트(설명 — ③).
        m if m == WM_CTLCOLORSTATIC || m == WM_CTLCOLORBTN => {
            let hdc = windows::Win32::Graphics::Gdi::HDC(wparam.0 as *mut core::ffi::c_void);
            let child = HWND(lparam.0 as *mut core::ffi::c_void);
            let id = GetDlgCtrlID(child) as u32;
            if id == ID_MODBAR {
                return LRESULT((*st).accent_brush.0 as isize);
            }
            SetBkMode(hdc, TRANSPARENT);
            if id == ID_DESC {
                windows::Win32::Graphics::Gdi::SetTextColor(hdc, COLORREF(0x0078_6E68));
            }
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
/// 디스클로저 글리프 폰트(Segoe MDL2 Assets 9px — 파일 목록 mdl2_small과
/// 동일 크기 규약, 사용자 확정 07-18).
unsafe fn make_icon_font() -> HFONT {
    let name: Vec<u16> = "Segoe MDL2 Assets\0".encode_utf16().collect();
    let mut lf = windows::Win32::Graphics::Gdi::LOGFONTW {
        lfHeight: -9,
        ..Default::default()
    };
    lf.lfFaceName[..name.len().min(32)].copy_from_slice(&name[..name.len().min(32)]);
    windows::Win32::Graphics::Gdi::CreateFontIndirectW(&lf)
}

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
    let icon_font = make_icon_font();
    let mut state = Box::new(PrefState {
        values,
        hwnd: dlg,
        owner,
        font,
        title_font,
        icon_font,
        category: "general".into(), // 첫 화면 = 일반 그룹(하위 섹션 전체)
        query: String::new(),
        tree: HWND::default(),
        expanded: vec![true; TREE.len()], // 기본 = 전부 펼침
        visible: Vec::new(),
        search_counts: Vec::new(),
        // 수정됨 바(X-10 ④) — Windows accent 근사색(라이트 고정 창)
        accent_brush: CreateSolidBrush(COLORREF(0x00D4_7800)),
        search: HWND::default(),
        divider: HWND::default(),
        cw: CLIENT_W,
        ch: CLIENT_H,
        scroll_y: 0,
        content_h: 0,
        rows: Vec::new(),
        editors: Vec::new(),
        radios: Vec::new(),
    });
    // 검색박스 = **자기완결 커스텀 컨트롤**(사용자 요청 07-16 — ctl::searchbox):
    // 내장 ✕(입력 시만·클릭 전체 지우기)·세로 중앙 정렬(한글 상단 붙음 해소)·
    // EN_CHANGE 재발행 계약이라 기존 ID_SEARCH 배선 그대로.
    state.search =
        crate::ctl::searchbox::create(dlg, PAD, PAD, CAT_W - 8, SEARCH_H, ID_SEARCH, font);
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
        // Enter = 편집 중 값 **즉시 적용**(사용자 확정 07-16 — 폰트 이름 EDIT·크기 콤보).
        // 포커스(콤보는 내부 에디트 → 부모로 승격)가 수확 대상이면 harvest+apply.
        if msg.message == 0x0100 /* WM_KEYDOWN */ && msg.wParam.0 == 0x0D {
            let st = GetWindowLongPtrW(dlg, GWLP_USERDATA) as *mut PrefState;
            if !st.is_null() {
                let focus = windows::Win32::UI::Input::KeyboardAndMouse::GetFocus();
                let owner_ctl = if (*st).editors.iter().any(|&(_, h)| h == focus) {
                    Some(focus)
                } else {
                    // 콤보 내부 EDIT — 부모(콤보)가 수확 대상인지
                    let parent = windows::Win32::UI::WindowsAndMessaging::GetParent(focus)
                        .unwrap_or_default();
                    (*st)
                        .editors
                        .iter()
                        .any(|&(_, h)| h == parent)
                        .then_some(parent)
                };
                if let Some(ctl) = owner_ctl {
                    // fontbox 드롭다운이 열려 있으면 Enter = 목록 확정(컨트롤 몫 —
                    // QA 07-16: 펌프가 가로채 Enter 선택이 죽던 진범). 닫혀 있으면
                    // 기존대로 즉시 적용.
                    let drop_open =
                        SendMessageW(ctl, crate::ctl::fontbox::FBM_HAS_DROP, None, None).0 == 1;
                    if !drop_open {
                        (*st).harvest();
                        (*st).apply_now();
                        continue; // 대화상자 기본 Enter 처리(비프) 억제
                    }
                }
            }
        }
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    let _ = EnableWindow(owner, true);
    let _ = SetForegroundWindow(owner);
    let _ = DeleteObject(font.into());
    let _ = DeleteObject(title_font.into());
    let _ = DeleteObject(icon_font.into());
    let _ = DeleteObject(state.accent_brush.into());
    // 즉시 적용 방식(X-8) — 닫기 = 확정. 최종 값 반환(미이탈 편집 값 포함, WM_CLOSE에서 수거).
    let mut v = state.values.clone();
    sanitize(&mut v);
    Some(v)
}
