//! Win32 창·메시지 루프 — 렌더·입력은 nexa-gui 위젯, 패널 로직은 panel.rs(플랫폼 중립).
//! 이 모듈의 책임: 창 수명·백버퍼(DW)·**듀얼 패널 배치(스플리터)·활성 패널 라우팅**(M2-2)·
//! WM_* → [`nexa_gui::InputEvent`] 번역·[`nexa_gui::Invalidations`] → `InvalidateRect` 번역.
//! F3 = 활성 패널 200프레임 스크롤 벤치. 타이틀바 = 활성 패널 경로·행/선택 수·페인트 시간.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use nexa_gui::widgets::{Menu, MenuBar, MenuItem, RowSource, StatusBar, ToolButton, Toolbar};
use nexa_gui::{Column, InputEvent, Invalidations, Key, Rect as GRect, Theme, Widget};
use nexa_tree::Tree;
use windows::core::{w, Result, HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, EndPaint, InvalidateRect, UpdateWindow, PAINTSTRUCT, SRCCOPY,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
use windows::Win32::UI::HiDpi::{
    GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetDoubleClickTime, GetKeyState, ReleaseCapture, SetCapture, VK_APPS, VK_CONTROL, VK_DELETE,
    VK_DOWN, VK_END, VK_ESCAPE, VK_F10, VK_F2, VK_F3, VK_F5, VK_F6, VK_HOME, VK_LEFT, VK_NEXT,
    VK_OEM_3, VK_OEM_COMMA, VK_OEM_PERIOD, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_SPACE,
    VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetMessageW,
    GetWindowLongPtrW, KillTimer, LoadCursorW, PostMessageW, PostQuitMessage, RegisterClassW,
    SetTimer, SetWindowLongPtrW, SetWindowPos, SetWindowTextW, TranslateMessage, CREATESTRUCTW,
    CS_DBLCLKS, CW_USEDEFAULT, GWLP_USERDATA, IDC_ARROW, MSG, SWP_NOACTIVATE, SWP_NOZORDER,
    WM_CAPTURECHANGED, WM_CHAR, WM_DESTROY, WM_DPICHANGED, WM_DRAWITEM, WM_ERASEBKGND,
    WM_GETOBJECT, WM_IME_COMPOSITION, WM_IME_STARTCOMPOSITION, WM_INITMENUPOPUP, WM_KEYDOWN,
    WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MEASUREITEM, WM_MENUCHAR, WM_MOUSEHWHEEL,
    WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_RBUTTONDOWN, WM_RBUTTONUP,
    WM_SETTINGCHANGE, WM_SIZE, WM_SYSKEYDOWN, WM_TIMER, WM_XBUTTONDOWN, WNDCLASSW,
    WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

use crate::config::{self, PanelSession, Session, Settings, SESSION_FILE, SETTINGS_FILE};
use crate::dw::{DwBackend, DwCtx};
use crate::i18n::{self, tr, trf};
use crate::icons::shell::ShellIcons;
use crate::panel::{NavCtx, Panel, PanelMetrics};
use crate::source::{COL_EXT, COL_KIND, COL_MODIFIED, COL_NAME, COL_SIZE};

/// 기선택 항목 재클릭 → 이름 바꾸기 최소 간격(ms) — 이보다 짧으면 더블클릭 시도로 무시.
const SLOW_CLICK_RENAME_MS: u64 = 1_000;
/// 스플리터 공통 두께(px @96dpi — 파일 좌/우·도크 좌/우·가로 분리선 통일, QA 07-14).
const SPLIT_TH: i32 = 3;
/// 스플리터 자석 스냅 임계(px @96dpi — 창 50%·반대편 구분선. Alt=해제, 원본 F9).
const SNAP_PX: i32 = 20;

/// wParam 마우스 수식키 비트(winuser.h MK_LBUTTON/MK_SHIFT/MK_CONTROL).
const MK_LBUTTON: usize = 0x0001;
const MK_SHIFT: usize = 0x0004;
const MK_CONTROL: usize = 0x0008;

/// F3 스크롤 벤치 프레임 수(게이트 관측).
const BENCH_FRAMES: usize = 200;
/// 타입어헤드 타임아웃 점검 타이머 id·주기(ms).
const TIMER_TYPEAHEAD: usize = 1;
const TIMER_TICK_MS: u32 = 250;
/// 셸 아이콘 로딩 큐 타이머 id(주기 = icons::shell::TICK_MS — 원본 A-4 속도 제한).
const TIMER_ICONS: usize = 2;
/// 전송 잡 통지(워커 → UI, M3-1) — wparam=세대(원본 A-1 가드 계승), lparam: 0=진행/1=완료.
const WM_APP_TRANSFER: u32 = 0x8001; // WM_APP + 1
/// 상주 자니터 타이머 id·점검 주기·유휴 트림 임계(M2-8 — 원본 01 §5-1·NFR-M3).
/// 활성 중에만 저빈도로 돌고, 트림 후엔 스스로 꺼진다(유휴 백그라운드 0%).
const TIMER_JANITOR: usize = 3;
/// 느린 재클릭 리네임 지연 타이머(QA 07-14) — 더블클릭 시간 내 두 번째 클릭이 오면 취소
/// (더블클릭=열기 우선, 탐색기 관례). 주기 = GetDoubleClickTime().
const TIMER_RENAME: usize = 4;
/// 터미널 선택 엣지 자동 스크롤 반복(QA 07-14) — 그리드 밖에서 버튼 유지 시 60ms 간격.
const TIMER_TERM_SEL: usize = 5;
/// 터미널 캐럿 깜빡임(QA 07-14) — 키 포커스 동안 GetCaretBlinkTime() 간격 토글.
const TIMER_TERM_CARET: usize = 6;
/// 전송 완료 진행 창 자동 닫기(원본 PROG-WIN 2초 — 사용자 요청 07-15).
const TIMER_PROG_CLOSE: usize = 7;
/// 세션 디바운스 자동 저장(사용자 요청 07-15 — 탭/경로 변경 폭주 시 마지막 상태 1회만).
const TIMER_SESSION_SAVE: usize = 8;
const SESSION_SAVE_DEBOUNCE_MS: u32 = 1_000;
const JANITOR_TICK_MS: u32 = 10_000;
const IDLE_TRIM_MS: u64 = 60_000;
/// 폴더 watcher 통지(M3-6) — wparam=패널, lparam=세대(낡은 스레드 무시). 디바운스 타이머는
/// 패널별(TIMER_WATCH_BASE+패널), 300ms 코얼레싱(원본 FolderWatcher 동일).
const WM_APP_FSCHANGE: u32 = 0x8002; // WM_APP + 2
const TIMER_WATCH_BASE: usize = 10;
const WATCH_DEBOUNCE_MS: u32 = 300;
/// 터미널 출력/종료 통지(M4-3) — wparam=패널(|EXIT_FLAG), lparam=세대.
const WM_APP_TERM: u32 = 0x8003; // WM_APP + 3
/// 파일별 아이콘 워커 결과(M4 QA 07-14 — WPARAM=Box<icons::shell::LoadResult>).
const WM_APP_ICON: u32 = 0x8004; // WM_APP + 4
/// 설정 창 열기 지연 실행(QA 07-14 — run_command의 State 차용과 모달 재진입 분리).
const WM_APP_PREFS: u32 = 0x8005; // WM_APP + 5
/// 일괄 이름변경 창 열기 지연 실행(M5-1 — WM_APP_PREFS와 동일 재진입 규약).
/// 0x8006=prefs 적용·0x8007=UIA 선택(uia.rs) 다음.
const WM_APP_BULK: u32 = 0x8008; // WM_APP + 8
/// ctl 갤러리(개발 검증 전용 — 메뉴 비노출, 주입으로만 연다. ctldemo.rs).
const WM_APP_CTLDEMO: u32 = 0x8009; // WM_APP + 9
/// 패널 최소 폭(논리 px)·스플리터 히트 존 반폭.
const MIN_PANEL: i32 = 200;
const SPLIT_HALF: i32 = 3;

/// 명령 id(메뉴·도구 모음 공용 — docs/20 §2).
const CMD_NEW_TAB: u32 = 1;
const CMD_CLOSE_TAB: u32 = 2;
const CMD_EXIT: u32 = 3;
const CMD_NEW_FOLDER: u32 = 4;
const CMD_NEW_FILE: u32 = 5;
const CMD_UNDO: u32 = 6;
const CMD_REDO: u32 = 7;
const CMD_TOGGLE_DOCK: u32 = 8;
/// 퀵 런처 바 표시 토글(M5-1 — 원본 ShowLauncher).
const CMD_TOGGLE_LAUNCHER: u32 = 9;
const CMD_TOGGLE_HIDDEN: u32 = 10;
const CMD_TOGGLE_DOTFILES: u32 = 11;
const CMD_REFRESH: u32 = 12;
/// 보기 모드 라디오(사용자 요청 07-16 — 트리 폴더/일반 폴더/타일).
const CMD_VIEW_TREE: u32 = 13;
const CMD_VIEW_FLAT: u32 = 14;
const CMD_VIEW_TILES: u32 = 15;
/// 패널/정보 모드 라디오(사용자 요청 07-16 — 원본 FR-C1 단일↔듀얼).
const CMD_PANEL_SINGLE: u32 = 16;
const CMD_PANEL_DUAL: u32 = 17;
const CMD_INFO_SINGLE: u32 = 18;
const CMD_INFO_DUAL: u32 = 19;
const CMD_NAV_BACK: u32 = 20;
const CMD_NAV_FORWARD: u32 = 21;
const CMD_NAV_UP: u32 = 22;
const CMD_THEME_SYSTEM: u32 = 30;
const CMD_THEME_LIGHT: u32 = 31;
const CMD_THEME_DARK: u32 = 32;
/// 언어 라디오 — 40 = 시스템, 41+idx = State.langs[idx](발견 목록 순).
const CMD_LANG_SYSTEM: u32 = 40;
const CMD_LANG_BASE: u32 = 41;
/// 설정 창(S6 — Ctrl+, / 메뉴·도구모음, QA 07-14).
const CMD_PREFS: u32 = 60;
/// 일괄 이름변경(M5-1 — 원본 docs/25, Ctrl+Shift+R).
const CMD_BULK_RENAME: u32 = 61;
/// ctl 갤러리(GroupCard 검증 — **임시** 도구 모음 버튼, 사용자 요청 07-17.
/// 카드 재편(X-23) 완료 시 버튼 제거).
const CMD_CTLDEMO: u32 = 62;
/// 컬럼 넓이 동기화 토글(사용자 확정 07-18 — 보기 메뉴 패널 모드 4종 하단).
const CMD_COLW_SYNC: u32 = 63;
/// 퀵 런처 항목(M5-1) — 200 + 항목 인덱스(항목 수 상한 32 — config.rs 파싱 방어와 동일).
const CMD_LAUNCHER_BASE: u32 = 200;

/// 테마 모드(원본 docs/39 §3 — System/Light/Dark). 영속은 M2-5.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ThemeMode {
    System,
    Light,
    Dark,
}

/// OS 앱 테마가 라이트인가 — 레지스트리 `AppsUseLightTheme`(없으면 라이트 간주).
unsafe fn os_uses_light_theme() -> bool {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let mut val: u32 = 1;
    let mut size = std::mem::size_of::<u32>() as u32;
    let ok = RegGetValueW(
        HKEY_CURRENT_USER,
        w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
        w!("AppsUseLightTheme"),
        RRF_RT_REG_DWORD,
        None,
        Some(&mut val as *mut u32 as *mut core::ffi::c_void),
        Some(&mut size),
    );
    ok.is_ok() && val != 0 || ok.is_err() // 조회 실패 = 라이트 기본
}

/// 모드 → 실효 테마(System = OS 설정 추종 — docs/39 §3).
unsafe fn resolve_theme(mode: ThemeMode) -> Theme {
    match mode {
        ThemeMode::Light => Theme::light(),
        ThemeMode::Dark => Theme::dark(),
        ThemeMode::System => {
            if os_uses_light_theme() {
                Theme::light()
            } else {
                Theme::dark()
            }
        }
    }
}

/// OS UI 언어(BCP-47, 예: "ko-KR") — i18n "system" 해석용(1차 서브태그만 사용).
unsafe fn system_ui_lang() -> String {
    use windows::Win32::Globalization::GetUserDefaultLocaleName;
    let mut buf = [0u16; 85]; // LOCALE_NAME_MAX_LENGTH
    let n = GetUserDefaultLocaleName(&mut buf);
    if n > 1 {
        String::from_utf16_lossy(&buf[..n as usize - 1]) // 종단 NUL 제외
    } else {
        "en".into()
    }
}

/// 타이틀바 다크 모드(DWMWA_USE_IMMERSIVE_DARK_MODE) — 본문 테마와 일치.
unsafe fn apply_titlebar_theme(hwnd: HWND, dark: bool) {
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
    let on: i32 = if dark { 1 } else { 0 };
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_USE_IMMERSIVE_DARK_MODE,
        &on as *const i32 as *const core::ffi::c_void,
        std::mem::size_of::<i32>() as u32,
    );
}

impl ThemeMode {
    fn as_str(self) -> &'static str {
        match self {
            ThemeMode::System => "system",
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
        }
    }
    fn from_str(s: &str) -> ThemeMode {
        match s {
            "system" => ThemeMode::System,
            "light" => ThemeMode::Light,
            _ => ThemeMode::Dark,
        }
    }
}

/// 메뉴 정의(파일·보기 — 도구/도움말은 후속 M5). 라벨 = i18n(언어 전환 시 재호출·set_menus).
/// `langs` = 발견된 언어 (code, 표기) — 라디오는 시스템 + 각 언어(원본 docs/42 §4-3 동적 목록).
#[allow(clippy::too_many_arguments)] // 전역 보기 상태 전달(구조체화는 후속)
fn build_menus(
    show_hidden: bool,
    show_dotfiles: bool,
    dock: bool,
    launcher: bool,
    mode: ThemeMode,
    lang_setting: &str,
    langs: &[(String, String)],
    view_mode: &str,
    panel_mode: &str,
    info_single_eff: bool,
    col_width_sync: bool,
) -> Vec<Menu> {
    let mut view_items = vec![
        // 보기 모드 라디오(07-16 — 원본 FR-A4 1차: 계층/일반/타일)
        MenuItem::new(CMD_VIEW_TREE, tr("menu.view.modeTree"), "").checked(view_mode == "tree"),
        MenuItem::new(CMD_VIEW_FLAT, tr("menu.view.modeFlat"), "").checked(view_mode == "flat"),
        MenuItem::new(CMD_VIEW_TILES, tr("menu.view.modeTiles"), "").checked(view_mode == "tiles"),
        MenuItem::separator(),
        // 패널/정보 모드 라디오(07-16 — 원본 FR-C1). 정보 라디오 표시 = **효과 기준**
        // (싱글 패널 = 싱글 고정 — 클릭 시 상태바 안내, 메뉴 비활성 미지원 α).
        MenuItem::new(CMD_PANEL_DUAL, tr("menu.view.panelDual"), "").checked(panel_mode == "dual"),
        MenuItem::new(CMD_PANEL_SINGLE, tr("menu.view.panelSingle"), "")
            .checked(panel_mode == "single"),
        MenuItem::new(CMD_INFO_DUAL, tr("menu.view.infoDual"), "").checked(!info_single_eff),
        MenuItem::new(CMD_INFO_SINGLE, tr("menu.view.infoSingle"), "").checked(info_single_eff),
        // 컬럼 넓이 동기화(사용자 확정 07-18 — 모드 4종 하단·기본 on·영속)
        MenuItem::new(CMD_COLW_SYNC, tr("menu.view.colWidthSync"), "").checked(col_width_sync),
        MenuItem::separator(),
        MenuItem::new(CMD_TOGGLE_HIDDEN, tr("menu.view.hidden"), "Ctrl+H").checked(show_hidden),
        MenuItem::new(CMD_TOGGLE_DOTFILES, tr("menu.view.dot"), "Ctrl+.").checked(show_dotfiles),
        MenuItem::new(CMD_TOGGLE_DOCK, tr("menu.view.dock"), "Ctrl+`").checked(dock),
        MenuItem::new(CMD_TOGGLE_LAUNCHER, tr("menu.view.launcher"), "").checked(launcher),
        MenuItem::separator(),
        MenuItem::new(CMD_REFRESH, tr("menu.view.refresh"), "F5"),
        MenuItem::separator(),
        // 테마 라디오(원본 docs/39 §3) — 설정 복원값 반영, F6 = 순환
        MenuItem::new(CMD_THEME_SYSTEM, tr("menu.view.theme.system"), "F6")
            .checked(mode == ThemeMode::System),
        MenuItem::new(CMD_THEME_LIGHT, tr("menu.view.theme.light"), "F6")
            .checked(mode == ThemeMode::Light),
        MenuItem::new(CMD_THEME_DARK, tr("menu.view.theme.dark"), "F6")
            .checked(mode == ThemeMode::Dark),
        MenuItem::separator(),
        MenuItem::new(CMD_LANG_SYSTEM, tr("menu.view.lang.system"), "")
            .checked(lang_setting == "system"),
    ];
    for (i, (code, name)) in langs.iter().enumerate() {
        view_items
            .push(MenuItem::new(CMD_LANG_BASE + i as u32, name, "").checked(lang_setting == code));
    }
    vec![
        Menu {
            title: tr("menu.file"),
            items: vec![
                MenuItem::new(CMD_NEW_TAB, tr("menu.file.newTab"), "Ctrl+T"),
                MenuItem::new(CMD_CLOSE_TAB, tr("menu.file.closeTab"), "Ctrl+W"),
                MenuItem::separator(),
                MenuItem::new(CMD_NEW_FOLDER, tr("menu.file.newFolder"), "Ctrl+Shift+N"),
                MenuItem::new(CMD_NEW_FILE, tr("menu.file.newFile"), ""),
                MenuItem::separator(),
                MenuItem::new(CMD_PREFS, tr("menu.file.prefs"), "Ctrl+,"),
                MenuItem::separator(),
                MenuItem::new(CMD_EXIT, tr("menu.file.exit"), ""),
            ],
        },
        Menu {
            title: tr("menu.edit"),
            items: vec![
                // 활성/비활성 표시는 후속(메뉴 위젯 enabled 미지원) — 없으면 상태바로 알림(M3-3)
                MenuItem::new(CMD_UNDO, tr("menu.edit.undo"), "Ctrl+Z"),
                MenuItem::new(CMD_REDO, tr("menu.edit.redo"), "Ctrl+Y"),
                MenuItem::separator(),
                MenuItem::new(CMD_BULK_RENAME, tr("menu.edit.bulkRename"), "Ctrl+Shift+R"),
            ],
        },
        Menu {
            title: tr("menu.view"),
            items: view_items,
        },
    ]
}

/// 도구 모음 버튼 — 새로고침만(사용자 지시 07-13: 네비 ←→↑는 패널별 네비 바가 전담,
/// 전역 도구 모음의 이전/다음 오동작 보고에 따라 중복 제거).
fn build_toolbar(show_hidden: bool, show_dotfiles: bool, view_mode: &str) -> Vec<ToolButton> {
    // 그룹화(QA 07-14 — 원본 PR#10): [새로고침] | [설정] | [숨김·닷파일 토글] |
    // [보기 모드 라디오 3종 — 07-16: 트리/일반/타일, 활성 탭 기준 1개만 켜짐]
    vec![
        ToolButton::new(CMD_VIEW_TREE, "├─").toggled(view_mode == "tree"),
        ToolButton::new(CMD_VIEW_FLAT, "☰").toggled(view_mode == "flat"),
        ToolButton::new(CMD_VIEW_TILES, "▦").toggled(view_mode == "tiles"),
        ToolButton::sep(),
        ToolButton::new(CMD_REFRESH, "⟳"),
        ToolButton::sep(),
        // 설정 = MDL2 Settings 톱니바퀴(사용자 확정 07-18 - U+2699는 꽃처럼 렌더)
        ToolButton::new(CMD_PREFS, ""),
        ToolButton::sep(),
        ToolButton::new(CMD_TOGGLE_HIDDEN, "👁").toggled(show_hidden),
        ToolButton::new(CMD_TOGGLE_DOTFILES, "…").toggled(show_dotfiles),
        // ctl 갤러리 🃏 버튼 숨김(사용자 확정 07-18 — 개발 검증은
        // WM_APP_CTLDEMO(0x8009) 주입 경로만 유지)
    ]
}

/// 퀵 런처 바 버튼(M5-1) — **exe 셸 아이콘 16×16 정사각 버튼**(원본 썸네일 대응 —
/// 미로드/실패 시 라벨 앞 2자 폴백) + 그룹 구분선(`launcherN=-` — 도구 모음 그룹화 대응).
fn build_launcherbar(items: &[crate::config::LauncherItem]) -> Vec<ToolButton> {
    items
        .iter()
        .enumerate()
        .map(|(i, it)| {
            if it.is_separator() {
                ToolButton::sep()
            } else {
                ToolButton::new(CMD_LAUNCHER_BASE + i as u32, it.label.clone())
                    .with_icon(crate::icons::icon_key(false, &it.exe), it.exe.clone())
            }
        })
        .collect()
}

/// 단조 시각(ms) — 타입어헤드 버퍼 타임아웃 판정용(프로세스 기동 기준).
fn now_ms() -> u64 {
    use std::sync::OnceLock;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// 평균 페인트 시간(µs) 누적 — 벤치 시 리셋. `first_render_ms` = 기동→첫 페인트(게이트).
#[derive(Default)]
struct PaintStats {
    total_us: u64,
    frames: u32,
    first_render_ms: Option<u64>,
}

impl PaintStats {
    fn add(&mut self, us: u64) {
        self.total_us += us;
        self.frames += 1;
    }
    fn avg_us(&self) -> u64 {
        if self.frames == 0 {
            0
        } else {
            self.total_us / self.frames as u64
        }
    }
    fn reset(&mut self) {
        let first = self.first_render_ms;
        *self = PaintStats::default();
        self.first_render_ms = first;
    }
}

/// 창 단위 상태 — `GWLP_USERDATA`에 Box raw 포인터로 보관(WM_NCCREATE~WM_NCDESTROY).
struct State {
    menubar: MenuBar,
    toolbar: Toolbar,
    /// 퀵 런처 바(M5-1 — 원본 docs/44 Row 2): 사용자 정의 외부 프로그램 버튼.
    /// 항목 라벨 = 버튼 글리프. 숨김이거나 항목 0이면 레이아웃 높이 0.
    launcherbar: Toolbar,
    launcher_visible: bool,
    launcher_items: Vec<crate::config::LauncherItem>,
    /// 보기 모드 "tree"|"flat"|"tiles"(사용자 요청 07-16 — 영속).
    view_mode: String,
    /// 패널 모드 "single"|"dual"(07-16 — 원본 FR-C1). 싱글 = 우 패널 숨김(상태 보존).
    panel_mode: String,
    /// 정보(도크) 모드 **선호값** "single"|"dual" — 효과는 [`single_info`](싱글 패널 강제).
    info_mode: String,
    /// 폰트 슬롯(X-12): 기본/우클릭 메뉴/상태바/파일 목록 + 목록 장식 3종.
    base_font: String,
    base_font_size: i32,
    ctx_font: String,
    ctx_font_size: i32,
    status_font: String,
    status_font_size: i32,
    list_font: String,
    list_font_size: i32,
    list_folder_bold: bool,
    header_bold: bool,
    header_italic: bool,
    /// 컬럼 넓이 동기화(07-18) — on = 좌/우 패널 실시간 동기(영속).
    col_width_sync: bool,
    /// 세션 복원 컬럼 폭(07-18) — DPI set_metrics(기본 폭 리셋) **이후** 적용
    /// 하려고 보류(WM_NCCREATE에서 소비).
    pending_colw: [Vec<i32>; 2],
    statusbar: StatusBar,
    /// 듀얼 패널(0=좌 주, 1=우 — docs/20 §2). 우 패널 숨김 토글은 후속.
    panels: [Panel; 2],
    /// 활성 패널(키보드·타이틀 기준). 클릭·Tab으로 전환.
    active: usize,
    /// 좌 패널 폭 비율(스플리터 드래그).
    split: f32,
    split_drag: bool,
    theme: Theme,
    theme_mode: ThemeMode,
    dpi: u32,
    dw: Option<DwBackend>,
    icons: std::cell::RefCell<ShellIcons>,
    stats: PaintStats,
    tz: i32,
    /// 가시성 필터(전역 ViewOptions — 영속은 M2-5).
    show_hidden: bool,
    show_dotfiles: bool,
    /// 폴더 우선 정렬(G-13 — 설정 영속·전 탭 전파).
    sort_folders_first: bool,
    /// 대소문자 구분 정렬·Alt+↑ 자동 선택 배치(사용자 요청 07-15 — 설정 영속).
    sort_case_sensitive: bool,
    nav_up_align: String,
    /// 탭 더블클릭 동작("close"|"pin"|"lock" — 사용자 요청 07-15).
    tab_dblclick: String,
    /// 타입어헤드 설정(원본 docs/32 §7 — 07-15): 범위·리셋 ms·HUD 위치·체크 3종.
    ta_scope: String,
    ta_reset_ms: i32,
    ta_pos: i32,
    ta_special: bool,
    ta_space: bool,
    ta_backspace: bool,
    /// 언어 설정값("system"|코드)·발견 목록(메뉴 라디오 CMD_LANG_BASE+idx 매핑) — M2-6.
    lang_setting: String,
    langs: Vec<(String, String)>,
    /// 터미널 글꼴 설정(QA 07-14 — data\settings term_font, 백엔드 생성 시 적용).
    term_font: String,
    term_font_size: i32,
    /// 터미널 줄 바꿈·고정 열(X-3 — 비줄바꿈이면 term_cols 고정+가로 스크롤).
    term_wrap: bool,
    term_cols: i32,
    /// 대화상자 글꼴(확인창·진행 창 — dialog.rs 공유).
    dlg_font: crate::dialog::DlgFont,
    /// 상주 자니터(M2-8): 마지막 입력 활동 시각(now_ms 기준)·트림 완료 플래그.
    last_activity_ms: u64,
    trimmed: bool,
    /// UIA 포커스 이벤트 중복 억제(M2-7): 마지막 통지한 (활성 패널, 캐럿).
    uia_caret: Option<(usize, Option<usize>)>,
    /// UIA 구조 이벤트 중복 억제(M5-3): 마지막 통지한 (활성 패널, 경로, 행 수).
    uia_struct: Option<(usize, String, usize)>,
    /// 진행 중 전송 잡(M3-1) — 동시 1개(α). 세대 번호로 낡은 워커 통지 무시.
    transfer: Option<TransferJob>,
    /// 완료 후 자동 닫기 대기 중인 진행 창(2초 — TIMER_PROG_CLOSE가 drop).
    transfer_close: Option<crate::dialog::Progress>,
    transfer_gen: u64,
    /// 행 위 왼쪽 버튼 누름 좌표(M3-5 S4) — 임계 이동 시 OLE 드래그 발신 시작.
    drag_press: Option<(i32, i32)>,
    /// 직전 행 클릭 (패널, 경로, 시각) — 기선택 항목 느린 재클릭=이름 바꾸기 판정(탐색기 관례).
    slow_click: Option<(usize, String, u64)>,
    /// 느린 재클릭 리네임 예약 — **MouseUp에서 진입**(드래그가 시작되면 취소 = DnD 우선).
    rename_on_up: bool,
    /// 패널별 폴더 watcher(M3-6) — 활성 탭 현재 폴더 감시(비재귀). 경로 변경 시 재구독.
    watchers: [Option<crate::watcher::DirWatcher>; 2],
    watch_gen: u64,
    /// 도크 상단 경계 드래그 중(M4-1 S2) — 패널 인덱스.
    dock_drag: Option<usize>,
    /// 도크 밴드 좌/우 스플리터 드래그 중(X-6 — 파일 스플리터와 독립).
    dock_split_drag: bool,
    /// 도크 밴드 좌/우 분할 비율(X-6 — 영속).
    dock_split: f32,
    /// 패널별 ConPTY 터미널(M4-3) — 도크 종류 '터미널' 최초 전환 시 지연 시작.
    terms: [Option<TermState>; 2],
    term_gen: u64,
    /// 터미널 키 입력 포커스(도크 터미널 클릭 시) — 리스트 단축키 차단.
    term_focus: Option<usize>,
    /// 터미널 마우스 선택 드래그 중(QA 07-14) — 패널 인덱스.
    term_drag: Option<usize>,
    /// 터미널 마우스 모드 전달 중 눌린 버튼(X-5) — (패널, SGR 버튼 코드).
    term_mouse_btn: Option<(usize, u8)>,
    /// 터미널 캐럿 깜빡임 위상(QA 07-14) — 포커스 중 타이머가 토글, 입력 시 true 리셋.
    term_caret_on: bool,
    /// 파일 작업 undo/redo 히스토리(M3-3, 원본 B-13u) — 세션 한정.
    history: nexa_ops::history::OperationHistory,
}

/// 도크 터미널 1개(M4-3) — ConPTY 세션 + VT 화면 버퍼.
struct TermState {
    pty: crate::conpty::ConPty,
    screen: nexa_term::VtScreen,
    exited: bool,
    /// 스크롤백 보기 오프셋(0=하단 라이브, 최대=scrollback_count) — 휠·선택 자동 스크롤(QA 07-14).
    view_off: usize,
    /// 마우스 선택(절대 라인, 열): (앵커, 끝) — 버튼 업 후에도 유지(Ctrl+C 복사).
    sel: Option<((usize, usize), (usize, usize))>,
    /// 페인트가 캐시한 셀 그리드: (내용 rect, cell_w, cell_h) — 마우스 히트 테스트용.
    grid: (nexa_gui::Rect, i32, i32),
    /// 가로 보기 오프셋(열 — X-3 비줄바꿈 고정 열 모드의 가로 스크롤).
    view_x: usize,
}

impl TermState {
    fn new(pty: crate::conpty::ConPty, cols: usize, rows: usize) -> Self {
        TermState {
            pty,
            screen: nexa_term::VtScreen::new(cols, rows),
            exited: false,
            view_off: 0,
            sel: None,
            grid: (nexa_gui::Rect::default(), 8, 16),
            view_x: 0,
        }
    }

    /// 가로 보기 이동(X-3 — 비줄바꿈 고정 열). 양수=오른쪽. 변화가 있었으면 `true`.
    fn scroll_view_x(&mut self, delta_cols: i32) -> bool {
        let (rc, cw, _) = self.grid;
        let vis = ((rc.w - 4) / cw.max(1)).max(1) as usize;
        let max_x = self.screen.cols().saturating_sub(vis);
        let nx = (self.view_x as i64 + delta_cols as i64).clamp(0, max_x as i64) as usize;
        if nx != self.view_x {
            self.view_x = nx;
            true
        } else {
            false
        }
    }

    /// 정렬된 선택 범위(시작 ≤ 끝). 앵커=끝(클릭만)이면 `None`.
    fn sel_norm(&self) -> Option<((usize, usize), (usize, usize))> {
        let (a, b) = self.sel?;
        if a == b {
            return None;
        }
        Some(if a.0 < b.0 || (a.0 == b.0 && a.1 <= b.1) {
            (a, b)
        } else {
            (b, a)
        })
    }

    /// 클라이언트 좌표 → (절대 라인, 열) — 그리드 캐시 기준(범위 밖은 가장자리로 클램프).
    fn cell_at(&self, x: i32, y: i32) -> (usize, usize) {
        let (rc, cw, ch) = self.grid;
        let col =
            (self.view_x + ((x - rc.x - 2) / cw.max(1)).max(0) as usize).min(self.screen.cols());
        let row = (((y - rc.y - 1) / ch.max(1)).max(0) as usize)
            .min(self.screen.rows().saturating_sub(1));
        let sb = self.screen.scrollback_count();
        let top = sb - self.view_off.min(sb);
        (top + row, col)
    }

    /// 스크롤백 보기 이동(양수=위로) — 변화가 있었으면 `true`.
    fn scroll_view(&mut self, lines: i32) -> bool {
        let sb = self.screen.scrollback_count();
        let cur = self.view_off.min(sb) as i64;
        let next = (cur + lines as i64).clamp(0, sb as i64) as usize;
        if next != self.view_off {
            self.view_off = next;
            true
        } else {
            false
        }
    }
}

/// 전송 워커와 UI 스레드가 공유하는 상태(원자/뮤텍스 — 워커는 State 접근 금지).
struct TransferShared {
    cancel: AtomicBool,
    done_bytes: AtomicU64,
    total_bytes: AtomicU64,
    outcome: Mutex<Option<nexa_ops::Outcome>>,
}

struct TransferJob {
    shared: Arc<TransferShared>,
    gen: u64,
    /// 완료 시 히스토리 기록용(M3-3) — Move/Copy에 따라 역연산이 다르다.
    op: nexa_ops::Op,
    /// 진행 창(QA 07-14 — 커스텀 프로그레스·취소 버튼). Drop=창 닫기.
    progress: Option<crate::dialog::Progress>,
}

impl State {
    fn nav_ctx(&self) -> NavCtx {
        NavCtx {
            show_hidden: self.show_hidden,
            show_dotfiles: self.show_dotfiles,
            tz: self.tz,
        }
    }
    fn active_panel(&mut self) -> &mut Panel {
        &mut self.panels[self.active]
    }
    /// 좌표가 속한 패널(도크 밴드 인지 — X-6: 밴드 안은 dock_split 기준). 스플리터 존=None.
    fn panel_at_pt(&self, x: i32, y: i32) -> Option<usize> {
        if self.panels[0].dock_visible() {
            let band = self.panels[0].dock.bounds();
            if band.h > 0 && y >= band.y {
                let right = self.panels[1].dock.bounds();
                return if x < band.right() {
                    Some(0)
                } else if x >= right.x {
                    Some(1)
                } else {
                    None // 도크 스플리터 존
                };
            }
        }
        self.panel_at(x)
    }

    /// 좌표가 속한 패널 인덱스(스플리터 존이면 `None`).
    fn panel_at(&self, x: i32) -> Option<usize> {
        if x < self.panels[0].bounds().right() {
            Some(0)
        } else if x >= self.panels[1].bounds().x {
            Some(1)
        } else {
            None // 스플리터 존
        }
    }
}

/// DPI 의존 지표(고밀도 규약 20px 행 @96dpi).
fn panel_metrics(dpi: u32) -> PanelMetrics {
    let s = |v: i32| (v * dpi as i32) / 96;
    PanelMetrics {
        row_h: s(20).max(14),
        pad_x: s(6),
        indent_w: s(16),
        tab_h: s(22),
        bar_h: s(24),
    }
}

/// 기본 5컬럼(원본 docs/23 §2-1). 폭은 dpi 스케일, 제목 = i18n(원본 col.* 키).
fn columns(dpi: u32) -> Vec<Column> {
    let s = |v: i32| (v * dpi as i32) / 96;
    vec![
        Column::new(COL_NAME, tr("col.name"), s(340)),
        Column::new(COL_EXT, tr("col.ext"), s(64)),
        Column::new(COL_SIZE, tr("col.size"), s(96)).right_aligned(),
        Column::new(COL_MODIFIED, tr("col.modified"), s(140)),
        Column::new(COL_KIND, tr("col.kind"), s(110)),
    ]
}

/// 로컬 타임존 오프셋(분, UTC 동쪽 양수).
unsafe fn tz_offset_min() -> i32 {
    let mut tzi = TIME_ZONE_INFORMATION::default();
    let code = GetTimeZoneInformation(&mut tzi);
    let bias = tzi.Bias
        + match code {
            1 => tzi.StandardBias,
            2 => tzi.DaylightBias,
            _ => 0,
        };
    -bias
}

/// 시작 루트 경로: argv[1] → %USERPROFILE% → C:\.
fn root_path() -> PathBuf {
    std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("C:\\"))
}

fn open_start_tree(path: &std::path::Path) -> Tree {
    Tree::open(path).unwrap_or_else(|e| {
        eprintln!("{} 열기 실패({e}) — C:\\ 로 대체", path.display());
        Tree::open("C:\\").expect("C:\\ 열기 실패")
    })
}

pub fn run() -> Result<()> {
    let _ = now_ms(); // 기동 시각 고정 — 첫 렌더 계측 기준
    let tz = unsafe { tz_offset_min() };
    // 설정/세션 로드(data\ — 없으면 기본값. M2-5. 구 .txt는 1회성 마이그레이션 폴백)
    let data = config::data_dir();
    let settings = config::load_migrated(&data, SETTINGS_FILE, config::SETTINGS_FILE_OLD)
        .map(|t| Settings::parse(&t))
        .unwrap_or_default();
    let session = config::load_migrated(&data, SESSION_FILE, config::SESSION_FILE_OLD)
        .map(|t| Session::parse(&t))
        .unwrap_or_default();
    let theme_mode = ThemeMode::from_str(&settings.theme);
    // i18n 활성화(M2-6) — 패널·메뉴 생성 전에(컬럼 제목·라벨이 tr() 경유)
    let langs = i18n::discover(&data);
    let code = i18n::resolve_code(&settings.lang, &unsafe { system_ui_lang() }, &langs);
    i18n::activate(i18n::load(&code, &data));
    let ctx = NavCtx {
        show_hidden: settings.show_hidden,
        show_dotfiles: settings.show_dotfiles,
        tz,
    };
    let m = panel_metrics(96); // 실제 DPI는 WM_NCCREATE에서 반영
    let fallback = root_path();
    let arg_given = std::env::args_os().nth(1).is_some();
    // argv 경로 = 명시 의도 → 세션 대신 그 경로로 시작. 그 외 = 세션 복원(원본 SESS)
    let (left, right, active_panel) = if arg_given {
        (
            Panel::new(open_start_tree(&fallback), ctx, m, columns(96)),
            Panel::new(open_start_tree(&fallback), ctx, m, columns(96)),
            0,
        )
    } else {
        (
            Panel::restore(
                &session.panels[0].tabs,
                session.panels[0].active,
                &session.panels[0].expanded,
                &fallback,
                ctx,
                m,
                columns(96),
            ),
            Panel::restore(
                &session.panels[1].tabs,
                session.panels[1].active,
                &session.panels[1].expanded,
                &fallback,
                ctx,
                m,
                columns(96),
            ),
            session.active_panel,
        )
    };
    let (mut left, mut right) = (left, right);
    // 탭 잠금 복원(편의 UX ② — 원본 TabSession.Locked)
    {
        let mut inv = Invalidations::default();
        left.seed_locked(&session.panels[0].locked, &mut inv);
        right.seed_locked(&session.panels[1].locked, &mut inv);
        left.seed_pinned(&session.panels[0].pinned, &mut inv);
        right.seed_pinned(&session.panels[1].pinned, &mut inv);
    }
    {
        // 하단 도크 복원(M4-1 — 원본 세션 저장 계승: 표시·비율)
        let mut inv = Invalidations::default();
        left.set_dock_ratio(settings.dock_ratio, &mut inv);
        right.set_dock_ratio(settings.dock_ratio, &mut inv);
        if settings.dock {
            left.set_dock_visible(true, &mut inv);
            right.set_dock_visible(true, &mut inv);
        }
    }
    // 정렬·Alt+↑ 배치 설정 — 기본이 아니면 기동 시 전 탭 적용
    {
        let mut inv = Invalidations::default();
        if !settings.sort_folders_first {
            left.set_folders_first(false, &mut inv);
            right.set_folders_first(false, &mut inv);
        }
        if settings.sort_case_sensitive {
            left.set_sort_case(true, &mut inv);
            right.set_sort_case(true, &mut inv);
        }
        let align = align_of(&settings.nav_up_align);
        left.set_nav_up_align(align);
        right.set_nav_up_align(align);
        // 폰트 장식 복원(X-12)
        if settings.list_folder_bold || settings.header_bold || settings.header_italic {
            left.set_font_decor(
                settings.list_folder_bold,
                settings.header_bold,
                settings.header_italic,
                &mut inv,
            );
            right.set_font_decor(
                settings.list_folder_bold,
                settings.header_bold,
                settings.header_italic,
                &mut inv,
            );
        }
        // 보기 모드 복원(07-16 개정: **탭별**) — 세션 값 우선, 없으면 설정 기본
        let fallback = mode_of(&settings.view_mode);
        left.seed_modes(&session.panels[0].modes, fallback, &mut inv);
        right.seed_modes(&session.panels[1].modes, fallback, &mut inv);
        // 타입어헤드 옵션(07-15) — 항상 적용(리셋 ms 등 기본값 아님 가능)
        let scope = scope_of(&settings.typeahead_scope);
        for p in [&mut left, &mut right] {
            p.set_typeahead_opts(
                scope,
                settings.typeahead_reset_ms.max(1) as u64,
                settings.typeahead_special,
                settings.typeahead_space,
                settings.typeahead_backspace,
                settings.typeahead_pos.clamp(0, 8) as u8,
                &mut inv,
            );
        }
        let _ = inv; // 창 생성 전 — 첫 페인트가 대체
    }
    // 퀵 런처 항목(M5-1) — 키 부재(첫 실행)면 시드(VS Code│pwsh·cmd — v2).
    // 다음 저장부터 launcher_count로 확정되어 "비움"이 존중된다.
    // 시드 버전이 낮으면 신규 시드(cmd·pwsh — 사용자 요청 07-15)만 1회 추가.
    let mut launcher_items = settings
        .launcher_items
        .clone()
        .unwrap_or_else(crate::launcher::seed);
    if settings.launcher_seed < crate::launcher::SEED_VERSION {
        crate::launcher::seed_missing(&mut launcher_items);
    }
    let state = Box::new(State {
        menubar: MenuBar::new(
            build_menus(
                settings.show_hidden,
                settings.show_dotfiles,
                settings.dock,
                settings.launcher,
                theme_mode,
                &settings.lang,
                &langs,
                &settings.view_mode,
                &settings.panel_mode,
                settings.panel_mode == "single" || settings.info_mode == "single",
                settings.col_width_sync,
            ),
            m.row_h,
            m.pad_x,
        ),
        toolbar: Toolbar::new(
            build_toolbar(
                settings.show_hidden,
                settings.show_dotfiles,
                &settings.view_mode,
            ),
            m.row_h,
            m.pad_x,
        ),
        launcherbar: Toolbar::new(build_launcherbar(&launcher_items), m.row_h, m.pad_x),
        launcher_visible: settings.launcher,
        launcher_items,
        view_mode: settings.view_mode.clone(),
        panel_mode: settings.panel_mode.clone(),
        info_mode: settings.info_mode.clone(),
        base_font: settings.base_font.clone(),
        base_font_size: settings.base_font_size,
        ctx_font: settings.ctx_font.clone(),
        ctx_font_size: settings.ctx_font_size,
        status_font: settings.status_font.clone(),
        status_font_size: settings.status_font_size,
        list_font: settings.list_font.clone(),
        list_font_size: settings.list_font_size,
        list_folder_bold: settings.list_folder_bold,
        header_bold: settings.header_bold,
        col_width_sync: settings.col_width_sync,
        pending_colw: [
            session.panels[0].col_widths.clone(),
            session.panels[1].col_widths.clone(),
        ],
        header_italic: settings.header_italic,
        statusbar: StatusBar::new(m.row_h, m.pad_x),
        panels: [left, right],
        active: active_panel,
        split: settings.split,
        split_drag: false,
        theme: unsafe { resolve_theme(theme_mode) },
        theme_mode,
        dpi: 96,
        dw: None,
        icons: std::cell::RefCell::new(ShellIcons::new()),
        stats: PaintStats::default(),
        tz,
        show_hidden: settings.show_hidden,
        show_dotfiles: settings.show_dotfiles,
        sort_folders_first: settings.sort_folders_first,
        sort_case_sensitive: settings.sort_case_sensitive,
        nav_up_align: settings.nav_up_align.clone(),
        tab_dblclick: settings.tab_dblclick.clone(),
        ta_scope: settings.typeahead_scope.clone(),
        ta_reset_ms: settings.typeahead_reset_ms,
        ta_pos: settings.typeahead_pos,
        ta_special: settings.typeahead_special,
        ta_space: settings.typeahead_space,
        ta_backspace: settings.typeahead_backspace,
        lang_setting: settings.lang,
        langs,
        term_font: settings.term_font,
        term_font_size: settings.term_font_size,
        term_wrap: settings.term_wrap,
        term_cols: settings.term_cols,
        dlg_font: crate::dialog::DlgFont {
            family: settings.dlg_font,
            size_pt: settings.dlg_font_size,
        },
        last_activity_ms: 0,
        trimmed: false,
        uia_caret: None,
        uia_struct: None,
        transfer: None,
        transfer_close: None,
        transfer_gen: 0,
        drag_press: None,
        slow_click: None,
        rename_on_up: false,
        watchers: [None, None],
        watch_gen: 0,
        dock_drag: None,
        dock_split_drag: false,
        dock_split: settings.dock_split,
        terms: [None, None],
        term_gen: 0,
        term_focus: None,
        term_drag: None,
        term_mouse_btn: None,
        term_caret_on: true,
        history: nexa_ops::history::OperationHistory::default(),
    });

    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let hinstance = GetModuleHandleW(None)?;
        let class_name = w!("NexaDir2Main");
        let wc = WNDCLASSW {
            style: CS_DBLCLKS,
            hInstance: hinstance.into(),
            lpszClassName: class_name,
            lpfnWndProc: Some(wndproc),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            // 원본 앱 아이콘(QA 07-14 — Alt+Tab·타이틀바. 작은 아이콘은 WM_SETICON)
            hIcon: crate::icon::load(32).unwrap_or_default(),
            ..Default::default()
        };
        let atom = RegisterClassW(&wc);
        debug_assert_ne!(atom, 0, "RegisterClassW 실패");

        // OLE DnD 수신(M3-5 S3) — OleInitialize는 RegisterDragDrop 전제(STA·UI 스레드)
        let _ = windows::Win32::System::Ole::OleInitialize(None);

        let hwnd = CreateWindowExW(
            Default::default(),
            class_name,
            w!("Nexa Dir 2"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1400,
            800,
            None,
            None,
            Some(hinstance.into()),
            Some(Box::into_raw(state) as *const core::ffi::c_void),
        )?;
        // 작은 아이콘(타이틀바·작업표시줄 — QA 07-14 원본 아이콘 공통 적용)
        if let Some(small) = crate::icon::load(16) {
            use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, ICON_SMALL, WM_SETICON};
            SendMessageW(
                hwnd,
                WM_SETICON,
                Some(WPARAM(ICON_SMALL as usize)),
                Some(LPARAM(small.0 as isize)),
            );
        }

        // 외부(탐색기 등)→앱 드롭 수신 등록 — 수명은 OLE가 보유(AddRef), 해제는 WM_NCDESTROY
        let drop_target: windows::Win32::System::Ole::IDropTarget = crate::dnd::DropTarget::new(
            hwnd,
            crate::dnd::DropHooks {
                dest_at: drop_dest_at,
                drop: handle_external_drop,
            },
        )
        .into();
        if let Err(e) = windows::Win32::System::Ole::RegisterDragDrop(hwnd, &drop_target) {
            eprintln!("DnD 수신 등록 실패(계속 진행): {e}");
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

/// lparam → 클라이언트 좌표 (x, y) — WM_MOUSE* 공용(부호 확장 포함, X-16 중복 제거).
fn mouse_xy(lparam: LPARAM) -> (i32, i32) {
    (
        (lparam.0 & 0xFFFF) as i16 as i32,
        ((lparam.0 >> 16) & 0xFFFF) as i16 as i32,
    )
}

unsafe fn state_of<'a>(hwnd: HWND) -> Option<&'a mut State> {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
    ptr.as_mut()
}

unsafe fn client_rect(hwnd: HWND) -> GRect {
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    GRect::new(0, 0, rc.right - rc.left, rc.bottom - rc.top)
}

unsafe fn flush_invalidations(hwnd: HWND, inv: &mut Invalidations) {
    for r in inv.drain() {
        let rc = RECT {
            left: r.x,
            top: r.y,
            right: r.right(),
            bottom: r.bottom(),
        };
        let _ = InvalidateRect(Some(hwnd), Some(&rc), false);
    }
}

/// 스플리터 x(클라이언트) — split 비율에서 계산(패널 최소 폭 클램프).
unsafe fn splitter_x(hwnd: HWND, st: &State) -> i32 {
    let rc = client_rect(hwnd);
    let s = |v: i32| (v * st.dpi as i32) / 96;
    let min = s(MIN_PANEL);
    ((rc.w as f32 * st.split) as i32).clamp(min.min(rc.w / 2), (rc.w - min).max(rc.w / 2))
}

/// 전체 레이아웃(docs/20 §1): 메뉴 / 도구 모음 / 퀵 런처 바(M5-1) / [좌 ║ 우 패널] / 상태바.
unsafe fn layout(hwnd: HWND, st: &mut State, inv: &mut Invalidations) {
    let rc = client_rect(hwnd);
    let s = |v: i32| (v * st.dpi as i32) / 96;
    let (menu_h, tool_h, status_h) = (s(22), s(24), s(22));
    st.menubar
        .set_bounds(GRect::new(0, 0, rc.w, menu_h.min(rc.h)), inv);
    st.toolbar
        .set_bounds(GRect::new(0, menu_h, rc.w, tool_h), inv);
    // 퀵 런처 바(원본 Row 2) — 숨김·실행 항목 0(구분선뿐 포함)이면 높이 0
    let has_items = st.launcher_items.iter().any(|i| !i.is_separator());
    let launch_h = if st.launcher_visible && has_items {
        tool_h
    } else {
        0
    };
    st.launcherbar
        .set_bounds(GRect::new(0, menu_h + tool_h, rc.w, launch_h), inv);
    let top = menu_h + tool_h + launch_h;
    let bottom = (rc.h - status_h).max(top);
    st.statusbar
        .set_bounds(GRect::new(0, bottom, rc.w, rc.h - bottom), inv);

    let sx = splitter_x(hwnd, st);
    // 스플리터 두께 **통일**(QA 07-14 — 파일 좌/우·도크 좌/우·가로 분리선 전부 동일)
    let gap = s(SPLIT_TH).max(2);
    let g2 = gap / 2;
    let area_h = (bottom - top).max(0);
    // 하단 도크 = **전폭 밴드**(X-6, 원본 BottomLeftCol/Splitter/RightCol) — 파일 영역과
    // 가로 분리·도크 좌/우는 파일 좌/우와 **독립 스플리터**(dock_split, 영속).
    let m = panel_metrics(st.dpi);
    let band_h = if st.panels[0].dock_visible() {
        ((area_h as f32 * st.panels[0].dock_ratio()) as i32)
            .clamp((m.row_h * 3).min(area_h / 2), area_h / 2)
    } else {
        0
    };
    let ph = (area_h - band_h).max(0);
    if single_panel(st) {
        // 싱글 패널(07-16): 좌 = 전폭, 우 = 0(상태 보존 — 세션/탭 그대로)
        st.panels[0].set_bounds(GRect::new(0, top, rc.w, ph), inv);
        st.panels[1].set_bounds(GRect::default(), inv);
    } else {
        st.panels[0].set_bounds(GRect::new(0, top, (sx - g2).max(0), ph), inv);
        st.panels[1].set_bounds(
            GRect::new(sx - g2 + gap, top, (rc.w - sx + g2 - gap).max(0), ph),
            inv,
        );
    }
    if band_h > 0 {
        let band_y = top + ph;
        let dsx = ((rc.w as f32 * st.dock_split) as i32).clamp(rc.w / 8, rc.w * 7 / 8);
        // 가로 분리선도 같은 두께(gap) — 도크 내용은 그 아래부터
        let dock_y = band_y + gap;
        let dock_h = (band_h - gap).max(0);
        if single_info(st) {
            // 싱글 정보(07-16): 전폭 공유 도크 1개(내용 = 활성 패널 추종 —
            // update_dock_info). 우 도크 = 0(스플리터 히트도 자연 소멸).
            st.panels[0]
                .dock
                .set_bounds(GRect::new(0, dock_y, rc.w, dock_h), inv);
            st.panels[1].dock.set_bounds(GRect::default(), inv);
        } else {
            st.panels[0]
                .dock
                .set_bounds(GRect::new(0, dock_y, (dsx - g2).max(0), dock_h), inv);
            st.panels[1].dock.set_bounds(
                GRect::new(
                    dsx - g2 + gap,
                    dock_y,
                    (rc.w - dsx + g2 - gap).max(0),
                    dock_h,
                ),
                inv,
            );
        }
    } else {
        st.panels[0].dock.set_bounds(GRect::default(), inv);
        st.panels[1].dock.set_bounds(GRect::default(), inv);
    }
}

/// 타이틀바 — 활성 패널 기준 경로·행/선택 수·페인트 시간.
unsafe fn update_title(hwnd: HWND, st: &State, note: &str) {
    let p = &st.panels[st.active];
    let sel = p.rows().source().tree().selection_count();
    let sel_txt = if sel > 0 {
        format!(" · {}", trf("status.selectedCount", &[&sel.to_string()]))
    } else {
        String::new()
    };
    let first_txt = st
        .stats
        .first_render_ms
        .map(|ms| format!(" · {}", trf("status.firstRender", &[&ms.to_string()])))
        .unwrap_or_default();
    let side = tr(if st.active == 0 {
        "panel.left"
    } else {
        "panel.right"
    });
    // 가상 최상위(X-17)는 타이틀에도 사람이 읽는 라벨
    let root = p.root_path();
    let root_disp = if nexa_vfs::is_virtual_root(&root) {
        tr("nav.mypc")
    } else {
        root.display().to_string()
    };
    let text = format!(
        "Nexa Dir 2 — [{side}] {} [{}{} · {} · {}{}]{}\0",
        root_disp,
        trf("status.itemCount", &[&p.rows().source().len().to_string()]),
        sel_txt,
        trf(
            "status.tab",
            &[
                &(p.active_index() + 1).to_string(),
                &p.tab_count().to_string(),
            ],
        ),
        trf("status.avg", &[&st.stats.avg_us().to_string()]),
        first_txt,
        note,
    );
    let wtext: Vec<u16> = text.encode_utf16().collect();
    let _ = SetWindowTextW(hwnd, PCWSTR(wtext.as_ptr()));
}

unsafe fn ensure_dw(st: &mut State, hdc: windows::Win32::Graphics::Gdi::HDC, w: i32, h: i32) {
    let fonts = crate::dw::FontSpec {
        base: (st.base_font.clone(), st.base_font_size as f32),
        list: (st.list_font.clone(), st.list_font_size as f32),
        status: (st.status_font.clone(), st.status_font_size as f32),
    };
    match &mut st.dw {
        None => match DwBackend::new(
            hdc,
            w,
            h,
            st.dpi,
            &fonts,
            &st.term_font,
            st.term_font_size as f32,
        ) {
            Ok(b) => st.dw = Some(b),
            Err(e) => eprintln!("DirectWrite 초기화 실패: {e}"),
        },
        Some(b) => {
            if b.size() != (w, h) {
                let _ = b.resize(w, h);
            }
        }
    }
}

/// 유휴 트림 판정(순수) — 마지막 활동 후 [`IDLE_TRIM_MS`] 경과 && 아직 미트림.
fn should_trim(now: u64, last_activity: u64, trimmed: bool) -> bool {
    !trimmed && now.saturating_sub(last_activity) >= IDLE_TRIM_MS
}

/// 상주 트림(M2-8 — 원본 01 §5-1 "유휴/최소화 시 작업집합 트림" 이식):
/// DW 백엔드(백버퍼 비트맵·레이아웃 캐시)·셸 아이콘 캐시(HICON) 해제 후 작업집합 반납.
/// 화면 무효화는 하지 않는다 — 다음 실제 페인트에서 지연 재적재(ensure_dw 재생성·아이콘 재요청).
unsafe fn trim_resident(st: &mut State) {
    st.dw = None;
    st.icons.borrow_mut().trim();
    use windows::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};
    // (-1, -1) = 작업집합 반납 — OS가 미사용 페이지 회수(RSS 즉시 반영)
    let _ = SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
}

/// 입력 활동 기록(M2-8) — 트림 상태 해제(재적재는 페인트가 담당)·자니터 재가동.
unsafe fn note_activity(hwnd: HWND, st: &mut State) {
    st.last_activity_ms = now_ms();
    if st.trimmed {
        st.trimmed = false;
        SetTimer(Some(hwnd), TIMER_JANITOR, JANITOR_TICK_MS, None);
    }
}

/// 활성 패널 가시 행의 UIA 스냅샷(M2-7) — 화면 좌표로 변환한 불변 공유 데이터.
/// UIA 콜백은 임의 스레드에서 오므로 프로바이더는 이 스냅샷만 읽는다(uia.rs 참조).
unsafe fn uia_snapshot(hwnd: HWND, st: &State) -> std::sync::Arc<crate::uia::Snap> {
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    let p = &st.panels[st.active];
    let rows = p.rows();
    let src = rows.source();
    let b = rows.bounds();
    let mut origin = windows::Win32::Foundation::POINT { x: 0, y: 0 };
    let _ = ClientToScreen(hwnd, &mut origin);
    let m = panel_metrics(st.dpi);
    let header = m.row_h; // 컬럼 헤더 행 근사(1차)
    let first = rows.scroll_row();
    let visible =
        ((((b.h - header) / m.row_h).max(0)) as usize).min(src.len().saturating_sub(first));
    let caret = rows.caret();
    let mut out = Vec::with_capacity(visible);
    for k in 0..visible {
        let i = first + k;
        let y = b.y + header + (k as i32) * m.row_h;
        out.push(crate::uia::RowSnap {
            name: src.row(i).text,
            selected: src.is_selected(i),
            focused: caret == Some(i),
            rect: (origin.x + b.x, origin.y + y, b.w, m.row_h),
        });
    }
    std::sync::Arc::new(crate::uia::Snap {
        name: p.root_path().display().to_string(),
        rect: (origin.x + b.x, origin.y + b.y, b.w, b.h),
        first_row: first,
        rows: out,
    })
}

/// 캐럿·구조 변경 시 UIA 이벤트 발행(M2-7 → M5-3) — 클라이언트가 붙어 있을 때만.
/// 구조 = (활성 패널, 경로, 행 수) 서명 — 재로드·펼침/접힘·이동을 포착해 자식 무효화
/// 이벤트를 발행(1차 한계였던 "트리 갱신 이벤트 미발행" 해소. 동수 개명은 미포착 — α).
unsafe fn uia_notify(hwnd: HWND, st: &mut State) {
    let p = &st.panels[st.active];
    let cur = (st.active, p.rows().caret());
    let sig = (
        st.active,
        p.root_path().display().to_string(),
        p.rows().source().len(),
    );
    let caret_changed = st.uia_caret != Some(cur);
    let struct_changed = st.uia_struct.as_ref() != Some(&sig);
    if !caret_changed && !struct_changed {
        return;
    }
    st.uia_caret = Some(cur);
    st.uia_struct = Some(sig);
    if !crate::uia::listening() {
        return;
    }
    let snap = uia_snapshot(hwnd, st);
    if struct_changed {
        crate::uia::raise_structure_changed(hwnd, snap.clone());
    }
    if caret_changed && cur.1.is_some() {
        crate::uia::raise_focus(hwnd, snap);
    }
}

/// 활성 패널 선택 → 클립보드 항목(M3-1). 선택이 없으면 `None`(클립보드 유지).
fn clip_from_selection(st: &mut State, op: nexa_ops::Op) -> Option<(Vec<PathBuf>, nexa_ops::Op)> {
    let paths: Vec<PathBuf> = st
        .active_panel()
        .rows()
        .source()
        .tree()
        .selected_paths()
        .into_iter()
        .map(|p| p.to_path_buf())
        .collect();
    (!paths.is_empty()).then_some((paths, op))
}

/// 도크 정보 뷰 내용(M4-1, 원본 DockInfo 이식) — 다중 선택=개수·단일=속성·없음=현재 폴더.
fn dock_info(p: &Panel) -> Vec<String> {
    use nexa_gui::widgets::RowSource;
    let rows = p.rows();
    let tree = rows.source().tree();
    let sel = tree.selection_count();
    if sel >= 2 {
        return vec![trf("info.selected", &[&sel.to_string()])];
    }
    if sel == 1 {
        if let Some(i) = tree
            .selected_ids()
            .first()
            .and_then(|&id| tree.index_of(id))
        {
            if let Some(r) = tree.row(i) {
                let mut lines = vec![
                    r.name.clone(),
                    trf("info.kind", &[&rows.source().cell(i, COL_KIND)]),
                ];
                if r.kind != nexa_core::FileKind::Dir {
                    lines.push(trf("info.size", &[&r.size.to_string()])); // 원시 바이트(원본)
                }
                let modified = rows.source().cell(i, COL_MODIFIED);
                if !modified.is_empty() {
                    lines.push(trf("info.modified", &[&modified]));
                }
                if let Some(path) = tree.node_path(r.id) {
                    lines.push(trf("info.path", &[&path.to_string_lossy()]));
                }
                return lines;
            }
        }
    }
    vec![trf(
        "info.currentFolder",
        &[&p.root_path().to_string_lossy()],
    )]
}

/// WIC가 인박스로 디코드하는 이미지 확장자(원본 docs/35 이미지 공급자 대응).
// webp(G-12) = OS WIC 확장 코덱 의존 — 미설치면 디코드 실패로 텍스트/이진 판정 폴백(무해)
const IMAGE_EXTS: [&str; 9] = [
    "png", "jpg", "jpeg", "bmp", "gif", "ico", "tif", "tiff", "webp",
];

/// 단일 선택 파일의 미리보기(M4-2 — 원본 docs/35 텍스트·이미지 공급자 대응).
/// 반환 (텍스트 라인들, 이미지 경로) — 이미지 확장자면 WIC 렌더(draw_image), 그 외 텍스트
/// 첫 16KB(대용량 안전·첫 1KB NUL=이진 판정).
fn preview_content(p: &Panel) -> (Vec<String>, Option<String>) {
    use std::io::Read;
    let tree = p.rows().source().tree();
    if tree.selection_count() != 1 {
        return (vec![tr("preview.none")], None);
    }
    let Some(path) = tree.selected_path(0).map(std::path::Path::to_path_buf) else {
        return (vec![tr("preview.none")], None);
    };
    if path.is_dir() {
        return (vec![tr("preview.none")], None);
    }
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if IMAGE_EXTS.contains(&ext.as_str()) {
        return (Vec::new(), Some(path.to_string_lossy().into_owned()));
    }
    let Ok(mut f) = std::fs::File::open(&path) else {
        return (vec![tr("preview.fail")], None);
    };
    let mut buf = vec![0u8; 16 * 1024];
    let n = f.read(&mut buf).unwrap_or(0);
    buf.truncate(n);
    if n == 0 {
        return (vec![tr("preview.empty")], None);
    }
    if buf[..n.min(1024)].contains(&0) {
        return (vec![tr("preview.binary")], None);
    }
    let lines = String::from_utf8_lossy(&buf)
        .lines()
        .take(200) // 도크 높이 초과분은 그리지 않음 — 여유 상한
        .map(|l| l.replace('\t', "    "))
        .collect();
    (lines, None)
}

/// 양 패널 도크 내용 갱신(표시 중일 때만 — set_lines는 변경 시에만 무효화).
/// 활성 종류: 0=정보(원본 DockInfo) · 1=미리보기(M4-2).
fn update_dock_info(st: &mut State, inv: &mut Invalidations) {
    let si = single_info(st);
    for i in 0..2 {
        if st.panels[i].dock_visible() {
            if si && i == 1 {
                continue; // 싱글 정보 — 우 도크는 숨김(bounds 0)
            }
            // 싱글 정보(07-16): 공유 도크(좌 위젯)의 내용 원천 = **활성 패널**
            // (터미널 종류는 좌 세션 고정 — α 규약, paint가 인덱스 0으로 그림)
            let src = if si { st.active } else { i };
            st.panels[i].dock.set_kinds(
                vec![tr("dock.info"), tr("dock.preview"), tr("dock.terminal")],
                inv,
            );
            let (lines, image) = match st.panels[i].dock.active_kind() {
                1 => preview_content(&st.panels[src]),
                2 => (Vec::new(), None), // 터미널은 paint에서 직접 그림(M4-3)
                _ => (dock_info(&st.panels[src]), None),
            };
            st.panels[i].dock.set_lines(lines, inv);
            st.panels[i].dock.set_image(image, inv);
        }
    }
}

/// 도크 터미널 렌더(M4-3, paint 경유) — 지연 시작·크기 동기·셀 그리드·캐럿·종료 안내.
/// 스크롤백 표시는 α 생략(가시 화면만 — 원본 β 기능).
#[allow(clippy::too_many_arguments)] // paint의 State 필드 분해 전달(차용 분리)
unsafe fn term_paint(
    ctx: &mut DwCtx,
    hwnd: HWND,
    panel: usize,
    term: &mut Option<TermState>,
    term_gen: &mut u64,
    rc: nexa_gui::Rect,
    cwd: &std::path::Path,
    dpi: u32,
    font_px: i32,
    theme: &nexa_gui::Theme,
    caret_on: bool,
    wrap: bool,
    cols_setting: i32,
) {
    use nexa_gui::{Color, DrawCtx, Rect};
    let cell_w = ctx.term_cell_w();
    let cell_h = ((font_px * 4 / 3 * dpi as i32) / 96).max(12); // 줄 높이 ≈ 1.33×(X-3 크기 설정)
                                                                // X-3: 줄 바꿈 = 뷰 폭 열(현행) · 비줄바꿈 = 고정 열(term_cols)+가로 스크롤
    let vis_cols = ((rc.w - 4) / cell_w.max(1)) as usize;
    let cols = if wrap {
        vis_cols
    } else {
        (cols_setting.clamp(80, 1000)) as usize
    };
    let rows = ((rc.h - 2) / cell_h) as usize;
    if vis_cols < 2 || rows < 2 {
        return;
    }
    if term.is_none() {
        *term_gen += 1;
        match crate::conpty::ConPty::start(
            hwnd,
            WM_APP_TERM,
            panel,
            *term_gen,
            cwd,
            cols as i16,
            rows as i16,
        ) {
            Some(pty) => {
                *term = Some(TermState::new(pty, cols, rows));
            }
            None => {
                ctx.term_text(
                    rc.x + 2,
                    rc.y,
                    Rect::new(rc.x, rc.y, rc.w, cell_h),
                    &tr("term.fail"),
                    theme.text_dim,
                    theme.panel_bg,
                );
                return;
            }
        }
    }
    let t = term.as_mut().unwrap();
    if t.screen.cols() != cols || t.screen.rows() != rows {
        t.screen.resize(cols, rows);
        t.pty.resize(cols as i16, rows as i16);
    }
    t.grid = (rc, cell_w, cell_h); // 마우스 히트 테스트용 캐시(QA 07-14)
    t.view_x = t.view_x.min(cols.saturating_sub(vis_cols)); // 래핑 전환·리사이즈 방어
    let argb = |c: u32| Color {
        r: (c >> 16) as u8,
        g: (c >> 8) as u8,
        b: c as u8,
    };
    let sb = t.screen.scrollback_count();
    let view = t.view_off.min(sb);
    let top = sb - view; // 가시 첫 절대 라인(0=스크롤백 최상단)
    let sel = t.sel_norm();
    let in_sel = |line: usize, col: usize| match sel {
        Some(((sl, sc), (el, ec))) => {
            (line > sl || (line == sl && col >= sc)) && (line < el || (line == el && col <= ec))
        }
        None => false,
    };
    for r in 0..rows {
        let y = rc.y + 1 + r as i32 * cell_h;
        let row_h = cell_h.min(rc.bottom() - y);
        if row_h <= 0 {
            break;
        }
        let abs = top + r;
        let line = t.screen.line_at(abs);
        // 유효 (fg,bg): reverse 스왑 → 선택이면 다시 스왑(반전 하이라이트).
        let eff = |c: usize| -> (u32, u32, bool) {
            let cell = &line[c];
            let (mut fg, mut bg) = (cell.fg, cell.bg);
            if cell.reverse {
                std::mem::swap(&mut fg, &mut bg);
            }
            if in_sel(abs, c) {
                std::mem::swap(&mut fg, &mut bg);
            }
            (fg, bg, cell.faint)
        };
        // 동일 (fg,bg) 런 = 배경 채움 단위. 문자는 **셀 x에 개별 배치** — 런 단위
        // 레이아웃은 폴백 글꼴(한글·아이콘) 전진폭이 셀 그리드와 어긋나 열이 밀림
        // (QA 07-14 — ls 이름 컬럼 깨짐).
        let c0 = t.view_x;
        let c_end = cols.min(line.len()).min(c0 + vis_cols);
        let mut c = c0.min(c_end);
        while c < c_end {
            let (fg, bg, faint) = eff(c);
            let start = c;
            while c < c_end {
                let (cf, cb, _) = eff(c);
                if cf != fg || cb != bg {
                    break;
                }
                c += 1;
            }
            let x = rc.x + 2 + (start - c0) as i32 * cell_w;
            let clip = Rect::new(x, y, (c - start) as i32 * cell_w, row_h);
            ctx.fill_rect(clip, argb(bg));
            let mut fgc = argb(fg);
            if faint {
                // faint = 배경 쪽으로 절반 블렌드(원본 PSReadLine 예측 표시 대응)
                let bgc = argb(bg);
                fgc = Color {
                    r: ((fgc.r as u16 + bgc.r as u16) / 2) as u8,
                    g: ((fgc.g as u16 + bgc.g as u16) / 2) as u8,
                    b: ((fgc.b as u16 + bgc.b as u16) / 2) as u8,
                };
            }
            for i in start..c {
                let ch = line[i].ch;
                if ch == '\0' || ch == ' ' {
                    continue; // 전각 연속 셀·공백 = 배경만
                }
                let wide = line.get(i + 1).is_some_and(|n| n.ch == '\0');
                let cx = rc.x + 2 + (i - c0) as i32 * cell_w;
                let cclip = Rect::new(cx, y, if wide { 2 } else { 1 } * cell_w, row_h);
                let mut buf = [0u8; 4];
                ctx.term_text(cx, y, cclip, ch.encode_utf8(&mut buf), fgc, argb(bg));
            }
        }
    }
    // 캐럿(세로바 — Windows Terminal 기본 bar 동일, QA 07-14) — 종료 시엔 안내
    if t.exited {
        let msg = tr("term.exited");
        let y = rc.bottom() - cell_h - 1;
        ctx.term_text(
            rc.x + 2,
            y,
            Rect::new(rc.x + 2, y, rc.w - 4, cell_h),
            &msg,
            theme.accent,
            theme.panel_bg,
        );
    } else {
        // 스크롤백 보기 중엔 캐럿 절대 라인이 화면 밖일 수 있음. 깜빡임 오프 프레임은 생략
        let cr = sb + t.screen.cursor_row();
        let cc = t.screen.cursor_col();
        if caret_on && cr >= top && cr < top + rows && cc >= t.view_x && cc < t.view_x + vis_cols {
            let cx = rc.x + 2 + (cc - t.view_x) as i32 * cell_w;
            let cy = rc.y + 1 + (cr - top) as i32 * cell_h;
            if cy + cell_h <= rc.bottom() {
                let w = (dpi as i32 / 96).max(1);
                // 밝은 회색 고정(QA 07-14) — 셀 배경이 어두워 theme.text로는 비가시
                ctx.fill_rect(Rect::new(cx, cy + 1, w, cell_h - 2), argb(0x00CC_CCCC));
            }
        }
    }
}

/// 키보드 조작 대상(M3-2, 원본 KeyboardTargets) — 선택(있으면) 아니면 캐럿 행.
fn keyboard_targets(st: &mut State) -> Vec<PathBuf> {
    let rows = st.active_panel().rows();
    let tree = rows.source().tree();
    let sel: Vec<PathBuf> = tree
        .selected_paths()
        .into_iter()
        .map(|p| p.to_path_buf())
        .collect();
    if !sel.is_empty() {
        return sel;
    }
    rows.caret()
        .and_then(|c| tree.visible_id(c))
        .and_then(|id| tree.node_path(id))
        .map(|p| vec![p.to_path_buf()])
        .unwrap_or_default()
}

/// **표시 순서** 선택 경로(QA 07-14 — selected_paths는 선택 삽입 순서라 화면 순서와 다름):
/// 가시 행을 위에서부터 순회해 선택 항목을 수집. 선택 없으면 캐럿 폴백(keyboard_targets).
fn display_order_targets(st: &mut State) -> Vec<PathBuf> {
    let out: Vec<PathBuf> = {
        let rows = st.active_panel().rows();
        let tree = rows.source().tree();
        (0..tree.visible_len())
            .filter_map(|i| {
                let id = tree.visible_id(i)?;
                if !tree.is_selected(id) {
                    return None;
                }
                tree.node_path(id).map(|p| p.to_path_buf())
            })
            .collect()
    };
    if out.is_empty() {
        return keyboard_targets(st);
    }
    out
}

/// 우클릭 컨텍스트 대상(M3-4) — 선택(없으면 캐럿)을 **캐럿 항목의 부모 폴더 기준으로 축소**
/// (ADR-0003 §다중 선택 규칙: GetUIObjectOf는 단일 부모만 표현. 교차 부모 확장은 후속).
fn context_targets(st: &mut State) -> Vec<PathBuf> {
    let targets = keyboard_targets(st);
    let caret_path = {
        let rows = st.active_panel().rows();
        let tree = rows.source().tree();
        rows.caret()
            .and_then(|c| tree.visible_id(c))
            .and_then(|id| tree.node_path(id))
            .map(|p| p.to_path_buf())
    };
    let Some(caret_path) = caret_path else {
        return targets;
    };
    let Some(parent) = caret_path.parent() else {
        return targets;
    };
    targets
        .into_iter()
        .filter(|p| p.parent() == Some(parent))
        .collect()
}

/// 고유 병합 항목 ID(0x8000+, ADR-0005 대역 분리) — 원본 §7 레지스트리는 후속(M5).
const CTX_DELETE_PERMANENT: u32 = crate::shellmenu::ID_CUSTOM_FIRST;
const CTX_PASTE_INTO: u32 = crate::shellmenu::ID_CUSTOM_FIRST + 1;
const CTX_UNDO: u32 = crate::shellmenu::ID_CUSTOM_FIRST + 2;
const CTX_REDO: u32 = crate::shellmenu::ID_CUSTOM_FIRST + 3;
const CTX_PASTE_BG: u32 = crate::shellmenu::ID_CUSTOM_FIRST + 4;
/// 경로 복사(QA 07-14 — 원본 §7-5 Copy as path): **교차 폴더 전체 선택**을 텍스트로.
const CTX_COPY_PATH: u32 = crate::shellmenu::ID_CUSTOM_FIRST + 5;
/// 이름 복사(QA 07-14 — 원본 PR#10 Copy as name): 경로 제외 **파일 이름만**.
const CTX_COPY_NAME: u32 = crate::shellmenu::ID_CUSTOM_FIRST + 6;

/// 빈 본문 우클릭 = 폴더 **배경** 셸 메뉴(M3-4 S3, 원본 ADR-0005 S2) — 새로 만들기 서브메뉴·
/// 속성 등 + 고유 Undo/Redo 병합(원본 빈영역 메뉴 항목 계승). paste 동사는 내부 클립보드로
/// 가로챔(OS 클립보드 상호운용은 M3-5).
unsafe fn show_background_context_menu(hwnd: HWND) {
    use crate::shellmenu::{self, CustomItem};
    // 1단계: State에서 요청 데이터 추출 — 모달 메뉴 펌프 전 참조 종료(ADR-0003 재진입 안전)
    let req = state_of(hwnd).map(|st| {
        let dir = st.active_panel().root_path();
        let undo_label = match st.history.undo_description() {
            Some(d) => trf("ctx.undoOf", &[d]),
            None => tr("menu.edit.undo"),
        };
        let redo_label = match st.history.redo_description() {
            Some(d) => trf("ctx.redoOf", &[d]),
            None => tr("menu.edit.redo"),
        };
        let custom = vec![
            // 고유 붙여넣기 — 셸 배경 메뉴가 paste 동사를 안 내는 환경 대비(QA 07-13:
            // 복사 후 빈영역 우클릭에 붙여넣기 부재). 클립보드에 파일 있을 때만 활성
            CustomItem {
                id: CTX_PASTE_BG,
                label: tr("ctx.paste"),
                enabled: crate::clipboard::has_files(),
                after_id: None,
            },
            CustomItem {
                id: CTX_UNDO,
                label: undo_label,
                enabled: st.history.can_undo(),
                after_id: None,
            },
            CustomItem {
                id: CTX_REDO,
                label: redo_label,
                enabled: st.history.can_redo(),
                after_id: None,
            },
        ];
        (dir, GetKeyState(VK_SHIFT.0 as i32) < 0, custom)
    });
    let Some((dir, shift, custom)) = req else {
        return;
    };
    let outcome = shellmenu::show_background(hwnd, &dir, shift, &["paste"], &[], &custom, None);
    let Some(st) = state_of(hwnd) else { return };
    match outcome {
        shellmenu::Outcome::Shell => reload_both(hwnd, st, ""), // 새로 만들기 등 FS 변경 가능
        shellmenu::Outcome::Verb(v) if v.eq_ignore_ascii_case("paste") => {
            // OS 클립보드 붙여넣기 합류(M3-5 — 전송 엔진 경유 = undo 기록 포함)
            if let Some((paths, op)) = crate::clipboard::read_file_list() {
                if op == nexa_ops::Op::Move {
                    crate::clipboard::clear(hwnd);
                }
                start_transfer(hwnd, st, paths, dir, op);
            }
        }
        shellmenu::Outcome::Custom(CTX_PASTE_BG) => {
            // 고유 붙여넣기 — paste 동사 가로채기와 동일 경로(전송 엔진 = undo 기록)
            if let Some((paths, op)) = crate::clipboard::read_file_list() {
                if op == nexa_ops::Op::Move {
                    crate::clipboard::clear(hwnd);
                }
                start_transfer(hwnd, st, paths, dir, op);
            }
        }
        shellmenu::Outcome::Custom(CTX_UNDO) => do_undo_redo(hwnd, st, false),
        shellmenu::Outcome::Custom(CTX_REDO) => do_undo_redo(hwnd, st, true),
        _ => {}
    }
}

/// 캐럿/선택 기준 셸 컨텍스트 메뉴 표시(M3-4 — 우클릭·Apps/Shift+F10 공용).
/// `at_caret`=true면 캐럿 행 앵커 위치(키보드), false면 커서 위치(마우스).
unsafe fn show_row_context_menu(hwnd: HWND, at_caret: bool) {
    use crate::shellmenu::{self, CustomItem};
    // 1단계: State에서 요청 데이터 추출 — 모달 메뉴 펌프 전 참조 종료(ADR-0003 재진입 안전)
    struct Req {
        targets: Vec<PathBuf>,
        /// 축소 전 전체 선택(교차 폴더 포함) — copy/cut/경로 복사 가로채기용(QA 07-14).
        full: Vec<PathBuf>,
        shift: bool,
        custom: Vec<CustomItem>,
        paste_dir: Option<PathBuf>,
        at: Option<windows::Win32::Foundation::POINT>,
    }
    let req = state_of(hwnd).and_then(|st| {
        let full = display_order_targets(st); // 표시 순서(QA 07-14 — 경로 복사 순서 정합)
        let targets = context_targets(st);
        if targets.is_empty() {
            return None;
        }
        let caret_path = {
            let rows = st.active_panel().rows();
            let tree = rows.source().tree();
            rows.caret()
                .and_then(|c| tree.visible_id(c))
                .and_then(|id| tree.node_path(id))
                .map(|p| p.to_path_buf())
        };
        let at = if at_caret {
            let anchor = st
                .active_panel()
                .rows()
                .caret()
                .and_then(|c| st.active_panel().rows().row_anchor(c))?;
            let mut pt = windows::Win32::Foundation::POINT {
                x: anchor.x,
                y: anchor.y,
            };
            let _ = windows::Win32::Graphics::Gdi::ClientToScreen(hwnd, &mut pt);
            Some(pt)
        } else {
            None
        };
        // 고유 병합 항목(원본 S1: 셸이 제공하는 동사는 중복 금지) — 완전 삭제·폴더에 붙여넣기
        let paste_dir = caret_path.filter(|p| targets.len() == 1 && p.is_dir());
        // "경로 복사"는 셸 항목 제자리 대체(hide→CTX_COPY_PATH — 앱 언어 라벨·윈도우 위치)
        let mut custom = vec![
            CustomItem {
                id: CTX_DELETE_PERMANENT,
                label: tr("ctx.deletePermanent"),
                enabled: true,
                after_id: None,
            },
            // 이름 복사(QA 07-14 — 원본 Copy as name): 경로 제외 파일 이름만.
            // 위치 = "경로 복사"(제자리 대체) 바로 아래(사용자 지시 2026-07-15).
            CustomItem {
                id: CTX_COPY_NAME,
                label: tr("ctx.copyName"),
                enabled: true,
                after_id: Some(CTX_COPY_PATH),
            },
        ];
        if paste_dir.is_some() && crate::clipboard::has_files() {
            custom.push(CustomItem {
                id: CTX_PASTE_INTO,
                label: tr("ctx.pasteInto"),
                enabled: true,
                after_id: None,
            });
        }
        Some(Req {
            targets,
            full,
            shift: GetKeyState(VK_SHIFT.0 as i32) < 0,
            custom,
            paste_dir,
            at,
        })
    });
    let Some(req) = req else { return };
    let outcome = shellmenu::show(
        hwnd,
        &req.targets,
        req.shift,
        &["delete", "rename", "copy", "cut"],
        &[("copyaspath", CTX_COPY_PATH, tr("ctx.copyPath"))],
        &req.custom,
        req.at,
    );
    let Some(st) = state_of(hwnd) else { return };
    match outcome {
        shellmenu::Outcome::Shell => reload_both(hwnd, st, ""), // 셸이 FS 변경했을 수 있음
        shellmenu::Outcome::Verb(v) if v.eq_ignore_ascii_case("delete") => {
            // 앱 경로 합류 — undo 기록(M3-3). Shift 열림 = 완전 삭제(확인창 방어)
            do_delete(hwnd, st, req.shift);
        }
        shellmenu::Outcome::Verb(v) if v.eq_ignore_ascii_case("rename") => {
            begin_rename_caret(hwnd, st); // 인라인 리네임 합류(M3-2)
        }
        shellmenu::Outcome::Verb(v)
            if v.eq_ignore_ascii_case("copy") || v.eq_ignore_ascii_case("cut") =>
        {
            // 복사/잘라내기 가로채기(QA 07-14) — 셸 데이터 객체는 단일 부모만 표현
            // (ADR-0003) → **교차 폴더 전체 선택**을 앱 클립보드 경로로(Ctrl+C/X 동일)
            let op = if v.eq_ignore_ascii_case("cut") {
                nexa_ops::Op::Move
            } else {
                nexa_ops::Op::Copy
            };
            crate::clipboard::write_file_list(hwnd, &req.full, op);
        }
        shellmenu::Outcome::Custom(CTX_COPY_PATH) => {
            // 경로 복사 — 전체 선택(교차 폴더)을 줄바꿈 구분 텍스트로(원본 §7-5)
            let text = req
                .full
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("\r\n");
            crate::clipboard::write_text(hwnd, &text);
        }
        shellmenu::Outcome::Custom(CTX_COPY_NAME) => {
            // 이름 복사(QA 07-14) — 경로 제외 파일/폴더 이름만(표시 순서)
            let text = req
                .full
                .iter()
                .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                .collect::<Vec<_>>()
                .join("\r\n");
            crate::clipboard::write_text(hwnd, &text);
        }
        shellmenu::Outcome::Custom(CTX_DELETE_PERMANENT) => do_delete(hwnd, st, true),
        shellmenu::Outcome::Custom(CTX_PASTE_INTO) => {
            if let (Some(dir), Some((paths, op))) =
                (req.paste_dir, crate::clipboard::read_file_list())
            {
                if op == nexa_ops::Op::Move {
                    crate::clipboard::clear(hwnd); // 잘라내기는 1회성(탐색기 관례)
                }
                start_transfer(hwnd, st, paths, dir, op);
            }
        }
        _ => {}
    }
}

/// 휴지통 삭제 — `SHFileOperationW`(FO_DELETE+FOF_ALLOWUNDO, 배치). α 채택 근거:
/// COM 초기화 불요·인박스(shell32). 원본 문서의 IFileOperation은 M3-3(휴지통 복원)과 함께 재검토.
unsafe fn delete_to_recycle_bin(paths: &[PathBuf]) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::UI::Shell::{
        SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_SILENT, FO_DELETE, SHFILEOPSTRUCTW,
    };
    let mut list: Vec<u16> = Vec::new();
    for p in paths {
        list.extend(p.as_os_str().encode_wide());
        list.push(0);
    }
    list.push(0); // 이중 NUL 종단
    let mut op = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: PCWSTR(list.as_ptr()),
        fFlags: (FOF_ALLOWUNDO.0 | FOF_NOCONFIRMATION.0 | FOF_SILENT.0) as u16,
        ..Default::default()
    };
    SHFileOperationW(&mut op) == 0 && !op.fAnyOperationsAborted.as_bool()
}

/// 화면 좌표의 드롭 대상 폴더(M3-5 S3, dnd.rs 훅) — 폴더 행=그 폴더·그 외=좌표 패널의 루트.
unsafe fn drop_dest_at(hwnd: HWND, sx: i32, sy: i32) -> Option<PathBuf> {
    let mut pt = windows::Win32::Foundation::POINT { x: sx, y: sy };
    let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut pt);
    let st = state_of(hwnd)?;
    let idx = st.panel_at(pt.x)?;
    let rows = st.panels[idx].rows();
    if let Some(row) = rows.row_at(pt.x, pt.y) {
        let tree = rows.source().tree();
        if let Some(p) = tree.visible_id(row).and_then(|id| tree.node_path(id)) {
            let p = p.to_path_buf();
            if p.is_dir() {
                return Some(p); // 폴더 행 = 그 폴더로 드롭
            }
        }
    }
    Some(st.panels[idx].root_path()) // 파일 행/빈 본문 = 패널 현재 폴더
}

/// 외부 드롭 확정(M3-5 S3, dnd.rs 훅) — 전송 엔진 합류(진행·취소·undo 기록·양쪽 재로드).
unsafe fn handle_external_drop(hwnd: HWND, paths: Vec<PathBuf>, dest: PathBuf, op: nexa_ops::Op) {
    if let Some(st) = state_of(hwnd) {
        start_transfer(hwnd, st, paths, dest, op);
    }
}

/// 휴지통 삭제 배치(원본 DeleteBatchOp — RecycleBin.cs와 동일 배치) — undo: 셸 undelete로
/// 원래 위치 복원 / redo: 다시 휴지통 삭제. 셸 COM 의존이라 앱 계층(연산 계약은 nexa-ops).
struct DeleteBatchOp {
    paths: Vec<PathBuf>,
    description: String,
}

impl nexa_ops::history::ReversibleOp for DeleteBatchOp {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&mut self) -> std::result::Result<(), nexa_ops::history::OpError> {
        let restored = crate::recycle::restore_by_original_paths(&self.paths);
        if restored < self.paths.len() {
            return Err(nexa_ops::history::OpError::Failed(
                self.paths.len() - restored,
            ));
        }
        Ok(())
    }

    fn redo(&mut self) -> std::result::Result<(), nexa_ops::history::OpError> {
        let existing: Vec<PathBuf> = self
            .paths
            .iter()
            .filter(|p| nexa_ops::exists(p))
            .cloned()
            .collect();
        if existing.is_empty() {
            return Ok(());
        }
        if unsafe { delete_to_recycle_bin(&existing) } {
            Ok(())
        } else {
            Err(nexa_ops::history::OpError::Failed(existing.len()))
        }
    }
}

/// 단일 경로 휴지통 삭제 — undo용 사본/생성물 제거 주입(원본 FileOps.DeleteToRecycleBin 대응).
fn recycle_delete_one(p: &std::path::Path) -> std::io::Result<()> {
    if unsafe { delete_to_recycle_bin(&[p.to_path_buf()]) } {
        Ok(())
    } else {
        Err(std::io::Error::other("휴지통 삭제 실패"))
    }
}

/// [`nexa_ops::history::OpError`] → i18n 문구(코어는 구조화 사유만 반환 — 문구는 앱 책임).
fn op_error_text(e: &nexa_ops::history::OpError) -> String {
    use nexa_ops::history::OpError;
    match e {
        OpError::Failed(n) => trf("history.failedItems", &[&n.to_string()]),
        OpError::MissingSource(name) => trf("history.missingSource", &[name]),
        OpError::NameExists(name) => trf("history.nameExists", &[name]),
    }
}

/// Ctrl+Z/Y·편집 메뉴(M3-3, 원본 UndoLastOperation/RedoLastOperation) — 실행 후 양쪽 재로드.
/// 실패한 연산은 스택에서 소실(무결성 우선 — docs/33)·상태는 타이틀 노트로 알림.
unsafe fn do_undo_redo(hwnd: HWND, st: &mut State, redo: bool) {
    let desc = if redo {
        st.history.redo_description()
    } else {
        st.history.undo_description()
    }
    .map(str::to_owned);
    let result = if redo {
        st.history.redo()
    } else {
        st.history.undo()
    };
    let Some(result) = result else {
        // 되돌릴/재실행할 작업 없음 — FS 무변경이라 재로드 생략
        let note = tr(if redo { "redo.none" } else { "undo.none" });
        update_title(hwnd, st, &format!(" · {note}"));
        return;
    };
    let desc = desc.unwrap_or_default();
    let note = match result {
        Ok(()) => trf(if redo { "redo.done" } else { "undo.done" }, &[&desc]),
        Err(e) => trf(
            if redo { "redo.fail" } else { "undo.fail" },
            &[&desc, &op_error_text(&e)],
        ),
    };
    // 실패여도 일부 항목은 수행됐을 수 있다 — 항상 재로드
    reload_both(hwnd, st, &format!(" · {note}"));
}

/// 패널별 watcher를 현재 폴더와 동기화(M3-6) — 경로 무변경이면 무비용(문자열 비교 2회).
/// 네비게이션·탭 전환 등 경로가 바뀔 수 있는 모든 경로가 update_status를 지나므로 그곳에서 호출.
unsafe fn sync_watchers(hwnd: HWND, st: &mut State) {
    for i in 0..2 {
        let want = st.panels[i].root_path();
        if st.watchers[i].as_ref().is_some_and(|w| w.path == want) {
            continue;
        }
        st.watch_gen += 1; // 낡은 스레드 통지 무시(세대 가드 — 원본 A-1)
        st.watchers[i] =
            crate::watcher::DirWatcher::start(hwnd, WM_APP_FSCHANGE, i, st.watch_gen, &want);
        // 실패(권한 등) = None — 수동 F5 폴백(원본 규약)
    }
}

/// 양쪽 패널 재로드(원본 ReloadBothPanels — watcher(M3-6) 전 임시) + 타이틀/상태 갱신.
unsafe fn reload_both(hwnd: HWND, st: &mut State, note: &str) {
    let ctx = st.nav_ctx();
    let mut inv = Invalidations::default();
    st.panels[0].reopen_filtered(ctx, &mut inv);
    st.panels[1].reopen_filtered(ctx, &mut inv);
    flush_invalidations(hwnd, &mut inv);
    update_title(hwnd, st, note);
    update_status(hwnd, st);
}

/// 삭제(M3-2, 원본 DeletePaths): Del=휴지통(확인 없음)·Shift+Del=완전(확인창 방어, 기본=취소).
/// 완전 삭제는 항목별 개별 격리, 휴지통은 셸 배치.
unsafe fn do_delete(hwnd: HWND, st: &mut State, permanent: bool) {
    let targets = keyboard_targets(st);
    if targets.is_empty() {
        return;
    }
    if permanent {
        use windows::Win32::UI::WindowsAndMessaging::{
            MessageBoxW, IDYES, MB_DEFBUTTON2, MB_ICONWARNING, MB_YESNO,
        };
        let text = HSTRING::from(trf("del.confirm", &[&targets.len().to_string()]));
        let title = HSTRING::from(tr("del.title"));
        let r = MessageBoxW(
            Some(hwnd),
            PCWSTR(text.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2,
        );
        if r != IDYES {
            return;
        }
    }
    let (mut ok, mut fail) = (0usize, 0usize);
    if permanent {
        for p in &targets {
            match nexa_ops::delete_permanent(p) {
                Ok(()) => ok += 1,
                Err(_) => fail += 1, // 개별 격리(원본)
            }
        }
    } else if delete_to_recycle_bin(&targets) {
        ok = targets.len();
        // undo 기록(B-13u S2) — undo=휴지통 복원·redo=재삭제. 완전 삭제는 설계상 제외(확인창 방어).
        st.history.push(Box::new(DeleteBatchOp {
            description: trf("del.recycleOp", &[&ok.to_string()]),
            paths: targets,
        }));
    } else {
        fail = targets.len();
    }
    let kind = tr(if permanent {
        "del.kindPermanent"
    } else {
        "del.kindRecycle"
    });
    let mut note = trf("del.done", &[&kind, &ok.to_string()]);
    if fail > 0 {
        note = format!("{note} · {}", trf("ops.errors", &[&fail.to_string()]));
    }
    reload_both(hwnd, st, &format!(" · {note}"));
}

/// F2 — 캐럿 행 인라인 이름변경 시작(원본 B-6).
unsafe fn begin_rename_caret(hwnd: HWND, st: &mut State) {
    let target = {
        let rows = st.active_panel().rows();
        rows.caret().map(|c| (c, rows.source().row(c).text))
    };
    let Some((row, name)) = target else { return };
    let mut inv = Invalidations::default();
    st.active_panel()
        .rows_mut()
        .begin_rename(row, &name, &mut inv);
    flush_invalidations(hwnd, &mut inv);
}

/// 인라인 이름변경 확정(M3-2) — 행 경로 조회 → `nexa_ops::rename` → 양쪽 재로드.
unsafe fn apply_rename(hwnd: HWND, st: &mut State, row: usize, new_name: &str) {
    let path = {
        let tree = st.active_panel().rows().source().tree();
        tree.visible_id(row)
            .and_then(|id| tree.node_path(id))
            .map(|p| p.to_path_buf())
    };
    let Some(path) = path else { return };
    // 바로가기 확장자 숨김(QA 07-14 — 탐색기 NeverShowExt): 표시/편집은 이름만,
    // 실제 리네임에는 원래 .lnk를 복원
    let mut new_name = new_name.to_string();
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("lnk"))
        && !new_name.to_ascii_lowercase().ends_with(".lnk")
    {
        new_name.push_str(".lnk");
    }
    let new_name = new_name.as_str();
    let note = match nexa_ops::rename(&path, new_name) {
        Ok(new_path) => {
            if new_path != path {
                // 펼침 집합 접두사 치환(F18 — 원본 UpdateExpandedPaths)
                st.active_panel().rename_expanded(&path, &new_path);
                // undo 기록(B-13u) — 동일 이름 무동작은 제외
                let desc = trf("rename.done", &[&nexa_ops::leaf_name(&path), new_name]);
                st.history.push(Box::new(nexa_ops::history::RenameOp::new(
                    path.clone(),
                    new_path,
                    desc,
                )));
            }
            String::new()
        }
        Err(e) => format!(" · {}", trf("rename.fail", &[&e.to_string()])),
    };
    reload_both(hwnd, st, &note);
}

/// 새로 만들기(M3-2, 원본 BG-N1/N2) — 생성 → 재로드 → 그 행 즉시 인라인 이름변경(RevealAndRename).
unsafe fn create_new(hwnd: HWND, st: &mut State, folder: bool) {
    let dir = st.active_panel().root_path();
    let created = if folder {
        nexa_ops::create_new_dir(&dir, &tr("new.folderBase"))
    } else {
        nexa_ops::create_new_file(&dir, &format!("{}.txt", tr("new.fileBase")))
    };
    match created {
        Err(e) => {
            update_title(
                hwnd,
                st,
                &format!(" · {}", trf("new.fail", &[&e.to_string()])),
            );
        }
        Ok(path) => {
            // undo 기록(B-13u) — undo=휴지통 삭제·redo=재생성(원본 CreateOp 주입 동일)
            let desc = tr(if folder { "new.folderOp" } else { "new.fileOp" });
            let recreate: nexa_ops::history::RecreateFn = {
                let p = path.clone();
                if folder {
                    Box::new(move || std::fs::create_dir(&p))
                } else {
                    Box::new(move || {
                        std::fs::OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(&p)
                            .map(|_| ())
                    })
                }
            };
            st.history.push(Box::new(nexa_ops::history::CreateOp::new(
                path.clone(),
                desc,
                Box::new(recycle_delete_one),
                recreate,
            )));
            reload_both(hwnd, st, "");
            let row = st
                .active_panel()
                .rows()
                .source()
                .tree()
                .index_of_path(&path.to_string_lossy());
            if let Some(row) = row {
                let name = nexa_ops::leaf_name(&path);
                let mut inv = Invalidations::default();
                st.active_panel()
                    .rows_mut()
                    .begin_rename(row, &name, &mut inv);
                flush_invalidations(hwnd, &mut inv);
            }
        }
    }
}

/// 전송 시작(M3-1, 원본 TransferPathsInto의 UI측) — 워커 스레드 + PostMessage 통지.
/// 충돌은 α 정책 = 전부 건너뜀(확인 모달은 후속 — 원본 확인창 자리). 동시 1잡.
unsafe fn start_transfer(
    hwnd: HWND,
    st: &mut State,
    sources: Vec<PathBuf>,
    dest: PathBuf,
    op: nexa_ops::Op,
) {
    if sources.is_empty() || st.transfer.is_some() {
        return;
    }
    st.transfer_gen += 1;
    let gen = st.transfer_gen;
    let shared = Arc::new(TransferShared {
        cancel: AtomicBool::new(false),
        done_bytes: AtomicU64::new(0),
        total_bytes: AtomicU64::new(0),
        outcome: Mutex::new(None),
    });
    let sh = shared.clone();
    let hwnd_raw = hwnd.0 as isize; // HWND는 !Send — 원시값으로 워커에 전달
                                    // 충돌 확인 문구는 UI 스레드에서 선확정(i18n 전역을 워커에서 조회하지 않음)
    let ow_title = tr("ops.overwriteTitle");
    let ow_text = tr("ops.overwrite");
    let dlg_font = st.dlg_font.clone();
    let (b_yes, b_yes_all, b_skip, b_cancel) = (
        tr("ops.yes"),
        tr("ops.yesAll"),
        tr("ops.skip"),
        tr("ops.cancel"),
    );
    std::thread::spawn(move || {
        let hwnd = HWND(hwnd_raw as *mut core::ffi::c_void);
        // 충돌 확인(QA 07-14 개정 — 원본 4버튼): **덮어쓰기/모두 덮어쓰기/건너뛰기/취소**
        // 커스텀 대화상자(dialog.rs — MessageBox 대체). "모두 덮어쓰기"만 이후 무확인,
        // 그 외에는 파일별로 다시 묻는다. 1건뿐이면 자연히 1회 질문.
        let mut decided: Option<nexa_ops::Conflict> = None;
        let out = nexa_ops::transfer(
            &sources,
            &dest,
            op,
            // 워커 스레드 모달(자체 메시지 루프) — UI 스레드는 계속 펌프(진행 표시 유지).
            &mut |conflict| {
                if let Some(choice) = decided {
                    return choice;
                }
                let text = ow_text.replace("{0}", &nexa_ops::leaf_name(conflict));
                let buttons = [
                    crate::dialog::DlgButton {
                        id: 1,
                        label: b_yes.clone(),
                    },
                    crate::dialog::DlgButton {
                        id: 2,
                        label: b_yes_all.clone(),
                    },
                    crate::dialog::DlgButton {
                        id: 3,
                        label: b_skip.clone(),
                    },
                    crate::dialog::DlgButton {
                        id: 4,
                        label: b_cancel.clone(),
                    },
                ];
                match unsafe {
                    crate::dialog::show_buttons(hwnd, &ow_title, &text, &buttons, &dlg_font)
                } {
                    1 => nexa_ops::Conflict::Overwrite, // 이 파일만 — 다음 충돌 재질문
                    2 => {
                        decided = Some(nexa_ops::Conflict::Overwrite); // 모두 덮어쓰기
                        nexa_ops::Conflict::Overwrite
                    }
                    3 => nexa_ops::Conflict::Skip, // 이 파일만 건너뜀
                    _ => {
                        sh.cancel.store(true, Ordering::Relaxed); // 취소/Esc = 전체 중단
                        nexa_ops::Conflict::Skip
                    }
                }
            },
            &mut |p, _| {
                sh.done_bytes.store(p.done_bytes, Ordering::Relaxed);
                sh.total_bytes.store(p.total_bytes, Ordering::Relaxed);
                // 4MB 청크 단위 통지 — 저빈도라 스로틀 불요
                unsafe {
                    let _ =
                        PostMessageW(Some(hwnd), WM_APP_TRANSFER, WPARAM(gen as usize), LPARAM(0));
                }
            },
            &sh.cancel,
        );
        *sh.outcome.lock().unwrap() = Some(out);
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_APP_TRANSFER, WPARAM(gen as usize), LPARAM(1));
        }
    });
    // 진행 창(QA 07-14 — 커스텀 프로그레스 컨트롤·[취소]) — 비모달, 완료 시 자동 닫힘(Drop)
    let progress = crate::dialog::Progress::open(
        hwnd,
        &tr("ops.progressTitle"),
        &tr("ops.progressLabel"),
        &st.dlg_font,
    );
    st.transfer = Some(TransferJob {
        shared,
        gen,
        op,
        progress,
    });
    update_title(hwnd, st, &format!(" · {}", trf("ops.progress", &["0"])));
}

/// 전송 통지 처리(WM_APP_TRANSFER) — 세대 불일치(낡은 워커)는 무시(원본 A-1 계승).
unsafe fn on_transfer_message(hwnd: HWND, st: &mut State, gen: u64, done_phase: bool) {
    let Some(job) = &st.transfer else { return };
    if job.gen != gen {
        return; // 낡은 워커 통지
    }
    if !done_phase {
        let (d, t) = (
            job.shared.done_bytes.load(Ordering::Relaxed),
            job.shared.total_bytes.load(Ordering::Relaxed),
        );
        let pct = (d * 100).checked_div(t).unwrap_or(100);
        // 진행 창 갱신 + [취소]/X 폴링(QA 07-14) — 취소 요청은 워커 cancel 플래그로
        let job = st.transfer.as_mut().unwrap();
        if let Some(p) = &mut job.progress {
            p.update(d, t);
            if p.cancelled() {
                job.shared.cancel.store(true, Ordering::Relaxed);
            }
        }
        update_title(
            hwnd,
            st,
            &format!(" · {}", trf("ops.progress", &[&pct.to_string()])),
        );
        return;
    }
    let mut job = st.transfer.take().unwrap();
    // 진행 창 = 완료 표기 + [닫기 (2)] 카운트다운(창 자체가 닫힘 — 사용자 요청 07-15).
    // 호스트 타이머는 백스톱: 구조체 지연 해제(창은 이미 닫혔어도 무해).
    if let Some(mut p) = job.progress.take() {
        p.set_done(&tr("ops.doneClosing"));
        st.transfer_close = Some(p);
        SetTimer(Some(hwnd), TIMER_PROG_CLOSE, 2_500, None);
    }
    let out = job
        .shared
        .outcome
        .lock()
        .unwrap()
        .take()
        .unwrap_or_default();
    // undo 기록(M3-3, 원본 B-13u) — 수행된 (원본, 최종 대상) 쌍만. 취소돼도 수행분은 기록.
    if !out.transferred.is_empty() {
        let n = out.transferred.len().to_string();
        let op: Box<dyn nexa_ops::history::ReversibleOp> = match job.op {
            nexa_ops::Op::Move => Box::new(nexa_ops::history::MoveBatchOp::new(
                out.transferred.clone(),
                trf("op.moveCount", &[&n]),
            )),
            nexa_ops::Op::Copy => Box::new(nexa_ops::history::CopyBatchOp::new(
                out.transferred.clone(),
                trf("op.copyCount", &[&n]),
                Box::new(recycle_delete_one),
            )),
        };
        st.history.push(op);
    }
    // 완료 후 양쪽 재로드(원본 TRANSFER-ENGINE 규약 — watcher는 M3-6)
    let ctx = st.nav_ctx();
    let mut inv = Invalidations::default();
    st.panels[0].reopen_filtered(ctx, &mut inv);
    st.panels[1].reopen_filtered(ctx, &mut inv);
    flush_invalidations(hwnd, &mut inv);
    let mut parts = vec![trf("ops.done", &[&out.transferred.len().to_string()])];
    if !out.skipped.is_empty() {
        parts.push(trf("ops.skipped", &[&out.skipped.len().to_string()]));
    }
    if !out.errors.is_empty() {
        parts.push(trf("ops.errors", &[&out.errors.len().to_string()]));
    }
    if out.canceled {
        parts.push(tr("ops.canceled"));
    }
    update_title(hwnd, st, &format!(" · {}", parts.join(" · ")));
    update_status(hwnd, st);
}

/// IME 조합 창을 편집 캐럿 옆에 배치(M2-7 근접 조합 — 원본 NFR-A1).
/// 대상 = 편집 중인 경로바(활성 패널 우선) → 인라인 이름변경 필드(M5-3 — M3-2 α 한계
/// 해소). 없으면 IME 기본 위치에 둔다.
/// 결과 문자열은 기존 WM_CHAR 경로로 수신(DefWindowProc의 WM_IME_CHAR 변환).
unsafe fn position_ime(hwnd: HWND, st: &mut State) {
    use windows::Win32::UI::Input::Ime::{
        ImmGetContext, ImmReleaseContext, ImmSetCompositionWindow, CFS_POINT, COMPOSITIONFORM,
    };
    // (캐럿 앞 텍스트, 필드, pad) — 캐럿 위치 기준 조합 창 배치(edit.rs 캐럿 모델)
    let Some((buf, field, pad)) = [st.active, 1 - st.active]
        .into_iter()
        .find_map(|i| st.panels[i].pathbar.edit_info())
        .or_else(|| st.panels[st.active].rows().rename_edit_info())
    else {
        return;
    };
    let Some(back) = &st.dw else { return };
    let mut ctx = DwCtx {
        back,
        icons: &st.icons,
    };
    let caret_x = field.x + pad + nexa_gui::DrawCtx::text_width(&mut ctx, &buf);
    let himc = ImmGetContext(hwnd);
    if himc.is_invalid() {
        return;
    }
    let form = COMPOSITIONFORM {
        dwStyle: CFS_POINT,
        ptCurrentPos: windows::Win32::Foundation::POINT {
            x: caret_x.min(field.right() - pad),
            y: field.y + 2,
        },
        ..Default::default()
    };
    let _ = ImmSetCompositionWindow(himc, &form);
    let _ = ImmReleaseContext(hwnd, himc);
}

/// 두 패널 + 스플리터를 DW 백버퍼에 그린 뒤 BitBlt.
unsafe fn paint(hwnd: HWND, st: &mut State) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let rc = client_rect(hwnd);
    let t0 = Instant::now();

    ensure_dw(st, hdc, rc.w, rc.h);
    if let Some(back) = &st.dw {
        let mut ctx = DwCtx {
            back,
            icons: &st.icons,
        };
        st.panels[0].paint(&mut ctx, &st.theme);
        if st.panels[1].bounds().w > 0 {
            st.panels[1].paint(&mut ctx, &st.theme); // 싱글 패널이면 숨김(07-16)
        }
        // 싱글 정보(X-20): 전폭 공유 도크(좌 위젯)를 **우 패널 뒤에 재도장** —
        // 우 리스트의 마지막 부분 행이 도크 위를 침범하던 것(원래는 우 도크가 덮음)
        if single_info(st) && st.panels[0].dock_visible() && st.panels[0].dock.bounds().h > 0 {
            st.panels[0].dock.paint(&mut ctx, &st.theme);
        }
        // 도크 터미널(M4-3) — 종류=터미널인 패널의 내용 영역에 셀 그리드 직접 렌더
        for i in 0..2 {
            if st.panels[i].dock_visible()
                && st.panels[i].dock.bounds().h > 0 // 싱글 정보 — 숨은 도크 PTY 기동 방지
                && st.panels[i].dock.active_kind() == 2
            {
                let trc = st.panels[i].dock.content_rect();
                let cwd = st.panels[i].root_path();
                // 깜빡임(QA 07-14)은 키 포커스일 때만 — 비포커스는 상시 표시(원본 규약)
                let caret_on = st.term_focus != Some(i) || st.term_caret_on;
                term_paint(
                    &mut ctx,
                    hwnd,
                    i,
                    &mut st.terms[i],
                    &mut st.term_gen,
                    trc,
                    &cwd,
                    st.dpi,
                    st.term_font_size,
                    &st.theme,
                    caret_on,
                    st.term_wrap,
                    st.term_cols,
                );
            }
        }
        // 스플리터(패널 영역 한정·드래그 중 accent). 싱글 패널(X-20)은 우 패널이
        // 0-rect라 w가 **음수** — GDI ETO_OPAQUE가 뒤집힌 rect를 정규화해 패널
        // 전체를 border 색으로 덮어칠하던 진범(QA 07-17 공백 화면). 양수만 그린다.
        use nexa_gui::DrawCtx;
        let lx = st.panels[0].bounds().right();
        let pb = st.panels[0].bounds();
        let w = st.panels[1].bounds().x - lx;
        if w > 0 {
            let color = if st.split_drag {
                st.theme.accent
            } else {
                st.theme.border
            };
            ctx.fill_rect(GRect::new(lx, pb.y, w, pb.h), color);
        }
        // 도크 밴드 스플리터 + 가로 분리선(X-6 — 통일 두께·QA 07-14)
        if st.panels[0].dock_visible() {
            let db = st.panels[0].dock.bounds();
            if db.h > 0 {
                let gap = ((SPLIT_TH * st.dpi as i32) / 96).max(2);
                let dx = db.right();
                let dw = st.panels[1].dock.bounds().x - dx;
                if dw > 0 {
                    // 싱글 정보(X-20)는 dw 음수 — 동일 정규화 덮어칠 방지(QA 07-17)
                    let dcolor = if st.dock_split_drag {
                        st.theme.accent
                    } else {
                        st.theme.border
                    };
                    ctx.fill_rect(GRect::new(dx, db.y, dw, db.h), dcolor);
                }
                // 파일↔정보 가로 분리선 — 동일 두께·다크 식별(text_dim)·높이 드래그=accent
                let hcolor = if st.dock_drag.is_some() {
                    st.theme.accent
                } else {
                    st.theme.text_dim
                };
                ctx.fill_rect(GRect::new(0, db.y - gap, rc.w, gap), hcolor);
            }
        }
        st.toolbar.paint(&mut ctx, &st.theme);
        if st.launcherbar.bounds().h > 0 {
            st.launcherbar.paint(&mut ctx, &st.theme); // 퀵 런처 바(M5-1)
        }
        st.statusbar.paint(&mut ctx, &st.theme);
        st.menubar.paint(&mut ctx, &st.theme); // 마지막 — 드롭다운 오버레이가 위에
        let _ = BitBlt(hdc, 0, 0, rc.w, rc.h, Some(back.memory_dc()), 0, 0, SRCCOPY);
    }

    st.stats.add(t0.elapsed().as_micros() as u64);
    if st.stats.first_render_ms.is_none() {
        st.stats.first_render_ms = Some(now_ms());
    }
    let _ = EndPaint(hwnd, &ps);

    if st.icons.borrow().has_pending() {
        SetTimer(Some(hwnd), TIMER_ICONS, crate::icons::shell::TICK_MS, None);
    }
    if st.stats.frames == 1 || st.stats.frames.is_multiple_of(20) {
        update_title(hwnd, st, "");
    }
}

/// 상태바 갱신 — 좌: 활성 패널 항목/선택/탭, 우: 필터·페인트 관측(docs/20 §2).
unsafe fn update_status(hwnd: HWND, st: &mut State) {
    let p = &st.panels[st.active];
    let sel = p.rows().source().tree().selection_count();
    let side = tr(if st.active == 0 {
        "panel.left"
    } else {
        "panel.right"
    });
    let left = format!(
        "[{side}] {}{} · {}",
        trf("status.itemCount", &[&p.rows().source().len().to_string()]),
        if sel > 0 {
            format!(" · {}", trf("status.selectedCount", &[&sel.to_string()]))
        } else {
            String::new()
        },
        trf(
            "status.tab",
            &[
                &(p.active_index() + 1).to_string(),
                &p.tab_count().to_string(),
            ],
        ),
    );
    let onoff = |on: bool| tr(if on { "status.show" } else { "status.hide" });
    let right = format!(
        "{} · {}",
        trf(
            "status.filters",
            &[&onoff(st.show_hidden), &onoff(st.show_dotfiles)],
        ),
        trf("status.avg", &[&st.stats.avg_us().to_string()]),
    );
    let mut inv = Invalidations::default();
    st.statusbar.set_text(left, right, &mut inv);
    update_dock_info(st, &mut inv); // 선택 변경 → 도크 정보(M4-1 — 변경 시에만 무효화)
    uia_notify(hwnd, st); // 캐럿 변경 시 스크린리더 통지(M2-7)
    sync_watchers(hwnd, st); // 경로 변경 시 watcher 재구독(M3-6 — 무변경이면 무비용)
                             // 세션 자동 저장(07-15): 탭/경로 변경 플래그 → 디바운스 재무장(변경 폭주 = 타이머
                             // 연장 = 중간 상태 무효화, 조용해진 뒤 마지막 상태만 1회 flush — 원본 SESS 코얼레싱)
                             // 보기 모드 라디오 동기(07-16 — 탭별): 활성 패널·활성 탭 기준. 탭 전환/네비/명령
                             // 등 모든 상호작용이 이 길목을 지나므로 별도 훅 없이 항상 일치(set_checked = 무변 무시).
    {
        use nexa_gui::widgets::ViewMode;
        let cur = match st.panels[st.active].active_view_mode() {
            ViewMode::Flat => "flat",
            ViewMode::Tiles => "tiles",
            ViewMode::Tree => "tree",
        };
        let mut vinv = Invalidations::default();
        for (c, m2) in [
            (CMD_VIEW_TREE, "tree"),
            (CMD_VIEW_FLAT, "flat"),
            (CMD_VIEW_TILES, "tiles"),
        ] {
            st.menubar.set_checked(c, m2 == cur, &mut vinv);
            st.toolbar.set_checked(c, m2 == cur, &mut vinv);
        }
        flush_invalidations(hwnd, &mut vinv);
    }
    if st.panels[0].take_session_dirty() | st.panels[1].take_session_dirty() {
        SetTimer(
            Some(hwnd),
            TIMER_SESSION_SAVE,
            SESSION_SAVE_DEBOUNCE_MS,
            None,
        );
    }
    flush_invalidations(hwnd, &mut inv);
}

/// 명령 실행(메뉴·도구 모음 공용).
unsafe fn run_command(hwnd: HWND, st: &mut State, id: u32) {
    let ctx = st.nav_ctx();
    let mut inv = Invalidations::default();
    match id {
        CMD_NEW_TAB => st.active_panel().new_tab(ctx, &mut inv),
        CMD_CLOSE_TAB => {
            let i = st.active_panel().active_index();
            st.active_panel().close_tab(i, &mut inv);
        }
        CMD_EXIT => PostQuitMessage(0),
        CMD_TOGGLE_HIDDEN => {
            st.show_hidden = !st.show_hidden;
            let on = st.show_hidden;
            st.menubar.set_checked(CMD_TOGGLE_HIDDEN, on, &mut inv);
            st.toolbar.set_checked(CMD_TOGGLE_HIDDEN, on, &mut inv); // 토글 그룹 동기
            let ctx = st.nav_ctx();
            st.active_panel().reopen_filtered(ctx, &mut inv);
            persist_settings(st);
        }
        CMD_TOGGLE_DOTFILES => {
            st.show_dotfiles = !st.show_dotfiles;
            let on = st.show_dotfiles;
            st.menubar.set_checked(CMD_TOGGLE_DOTFILES, on, &mut inv);
            st.toolbar.set_checked(CMD_TOGGLE_DOTFILES, on, &mut inv);
            let ctx = st.nav_ctx();
            st.active_panel().reopen_filtered(ctx, &mut inv);
            persist_settings(st);
        }
        CMD_NEW_FOLDER | CMD_NEW_FILE => {
            create_new(hwnd, st, id == CMD_NEW_FOLDER);
        }
        CMD_VIEW_TREE | CMD_VIEW_FLAT | CMD_VIEW_TILES => {
            // 보기 모드 라디오(07-16 개정: **탭별**) — 활성 패널의 활성 탭에만 적용.
            // 라디오(메뉴·도구 모음) 동기는 update_status의 단일 동기 지점이 수행.
            let mode_str = match id {
                CMD_VIEW_FLAT => "flat",
                CMD_VIEW_TILES => "tiles",
                _ => "tree",
            };
            st.view_mode = mode_str.into(); // 마지막 선택 = 새 세션 기본(설정 영속)
            let mode = mode_of(mode_str);
            st.active_panel().set_view_mode(mode, &mut inv);
            let settings = current_settings(st);
            let _ = config::save(&config::data_dir(), SETTINGS_FILE, &settings.serialize());
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
        CMD_COLW_SYNC => {
            // 컬럼 넓이 동기화 토글(사용자 확정 07-18 — 영속·on 전환 시 활성
            // 패널 폭으로 반대 패널 즉시 정렬)
            st.col_width_sync = !st.col_width_sync;
            st.menubar
                .set_checked(CMD_COLW_SYNC, st.col_width_sync, &mut inv);
            if st.col_width_sync {
                let w = st.panels[st.active].col_widths();
                st.panels[1 - st.active].apply_col_widths(&w, &mut inv);
            }
            let settings = current_settings(st);
            let _ = config::save(&config::data_dir(), SETTINGS_FILE, &settings.serialize());
        }
        CMD_PANEL_SINGLE | CMD_PANEL_DUAL => {
            // 패널 모드(07-16 — 원본 FR-C1): 싱글 = 우 패널 숨김(**상태 보존** — 탭/
            // 세션 그대로, 복귀 시 원복). 싱글 진입 시 활성 = 좌 강제.
            let want = if id == CMD_PANEL_SINGLE {
                "single"
            } else {
                "dual"
            };
            if st.panel_mode != want {
                st.panel_mode = want.into();
                if single_panel(st) && st.active == 1 {
                    set_active(hwnd, st, 0);
                }
                st.menubar
                    .set_checked(CMD_PANEL_SINGLE, want == "single", &mut inv);
                st.menubar
                    .set_checked(CMD_PANEL_DUAL, want == "dual", &mut inv);
                // 정보 라디오 = 효과 기준(싱글 패널이면 싱글 표시·선호값은 보존)
                let ie = single_info(st);
                st.menubar.set_checked(CMD_INFO_SINGLE, ie, &mut inv);
                st.menubar.set_checked(CMD_INFO_DUAL, !ie, &mut inv);
                layout(hwnd, st, &mut inv);
                update_dock_info(st, &mut inv);
                let settings = current_settings(st);
                let _ = config::save(&config::data_dir(), SETTINGS_FILE, &settings.serialize());
                let _ = InvalidateRect(Some(hwnd), None, false);
                update_status(hwnd, st);
            }
        }
        CMD_INFO_SINGLE | CMD_INFO_DUAL => {
            // 정보(도크) 모드(07-16): 싱글 = 전폭 공유(활성 패널 추종).
            // **싱글 패널에서는 변경 불가**(싱글 고정 — 사용자 확정 규칙).
            if single_panel(st) {
                update_title(hwnd, st, &tr("status.infoLocked"));
            } else {
                let want = if id == CMD_INFO_SINGLE {
                    "single"
                } else {
                    "dual"
                };
                if st.info_mode != want {
                    st.info_mode = want.into();
                    st.menubar
                        .set_checked(CMD_INFO_SINGLE, want == "single", &mut inv);
                    st.menubar
                        .set_checked(CMD_INFO_DUAL, want == "dual", &mut inv);
                    layout(hwnd, st, &mut inv);
                    update_dock_info(st, &mut inv);
                    let settings = current_settings(st);
                    let _ = config::save(&config::data_dir(), SETTINGS_FILE, &settings.serialize());
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    update_status(hwnd, st);
                }
            }
        }
        CMD_TOGGLE_DOCK => {
            // 하단 도크 토글(M4-1, Ctrl+` — 원본 대원칙: 듀얼=좌↔좌·우↔우 동시)
            let on = !st.panels[0].dock_visible();
            st.panels[0].set_dock_visible(on, &mut inv);
            st.panels[1].set_dock_visible(on, &mut inv);
            st.menubar.set_checked(CMD_TOGGLE_DOCK, on, &mut inv);
            layout(hwnd, st, &mut inv); // 도크는 전폭 밴드 — 호스트 재배치(X-6)
            update_dock_info(st, &mut inv);
            persist_settings(st);
        }
        CMD_TOGGLE_LAUNCHER => {
            // 퀵 런처 바 토글(M5-1 — 원본 ShowLauncher, settings 영속)
            st.launcher_visible = !st.launcher_visible;
            st.menubar
                .set_checked(CMD_TOGGLE_LAUNCHER, st.launcher_visible, &mut inv);
            layout(hwnd, st, &mut inv);
            let _ = InvalidateRect(Some(hwnd), None, false);
            persist_settings(st);
        }
        id if (CMD_LAUNCHER_BASE..CMD_LAUNCHER_BASE + st.launcher_items.len() as u32)
            .contains(&id) =>
        {
            // 퀵 런처 항목 실행(M5-1) — %path% = 활성 패널 현재 폴더, 실패는 상태바 격리
            let item = st.launcher_items[(id - CMD_LAUNCHER_BASE) as usize].clone();
            if item.is_separator() {
                return; // 구분선 — 히트 불가지만 방어
            }
            let folder = st.active_panel().root_path();
            let ok = crate::launcher::launch(hwnd, &item, &folder);
            let key = if ok {
                "launcher.ran"
            } else {
                "launcher.failed"
            };
            update_title(hwnd, st, &format!(" · {}", trf(key, &[&item.label])));
        }
        CMD_PREFS => {
            // 모달 설정 창은 State 차용과 분리해 지연 실행(재진입 규약 — WM_APP_PREFS)
            let _ = PostMessageW(Some(hwnd), WM_APP_PREFS, WPARAM(0), LPARAM(0));
        }
        CMD_BULK_RENAME => {
            // 모달 창 — 설정 창과 동일 재진입 규약(M5-1)
            let _ = PostMessageW(Some(hwnd), WM_APP_BULK, WPARAM(0), LPARAM(0));
        }
        CMD_CTLDEMO => {
            // 임시(07-17): ctl 갤러리(GroupCard 검증) — X-23 재편 완료 시 제거
            let _ = PostMessageW(Some(hwnd), WM_APP_CTLDEMO, WPARAM(0), LPARAM(0));
        }
        CMD_UNDO | CMD_REDO => {
            do_undo_redo(hwnd, st, id == CMD_REDO);
            return; // 결과 노트 보존(말미 update_title("")이 지우지 않도록)
        }
        CMD_REFRESH => st.active_panel().reopen_filtered(ctx, &mut inv),
        CMD_NAV_BACK => st.active_panel().nav_back(ctx, &mut inv),
        CMD_NAV_FORWARD => st.active_panel().nav_forward(ctx, &mut inv),
        CMD_NAV_UP => st.active_panel().nav_up(ctx, &mut inv),
        CMD_THEME_SYSTEM | CMD_THEME_LIGHT | CMD_THEME_DARK => {
            st.theme_mode = match id {
                CMD_THEME_LIGHT => ThemeMode::Light,
                CMD_THEME_DARK => ThemeMode::Dark,
                _ => ThemeMode::System,
            };
            apply_theme(hwnd, st, &mut inv);
            persist_settings(st);
        }
        CMD_LANG_SYSTEM => {
            st.lang_setting = "system".into();
            apply_lang(hwnd, st, &mut inv);
            persist_settings(st);
        }
        i if i >= CMD_LANG_BASE && ((i - CMD_LANG_BASE) as usize) < st.langs.len() => {
            st.lang_setting = st.langs[(i - CMD_LANG_BASE) as usize].0.clone();
            apply_lang(hwnd, st, &mut inv);
            persist_settings(st);
        }
        _ => {}
    }
    flush_invalidations(hwnd, &mut inv);
    update_title(hwnd, st, "");
    update_status(hwnd, st);
}

/// 테마 모드 적용 — 실효 테마 재해석·메뉴 라디오 동기·타이틀바·전체 다시 그리기(docs/39 §3).
unsafe fn apply_theme(hwnd: HWND, st: &mut State, inv: &mut Invalidations) {
    st.theme = resolve_theme(st.theme_mode);
    st.menubar
        .set_checked(CMD_THEME_SYSTEM, st.theme_mode == ThemeMode::System, inv);
    st.menubar
        .set_checked(CMD_THEME_LIGHT, st.theme_mode == ThemeMode::Light, inv);
    st.menubar
        .set_checked(CMD_THEME_DARK, st.theme_mode == ThemeMode::Dark, inv);
    apply_titlebar_theme(hwnd, st.theme == Theme::dark());
    let _ = InvalidateRect(Some(hwnd), None, false);
}

/// 언어 전환(M2-6) — 재시작 없음: 테이블 스왑 + 메뉴/컬럼 라벨 재구성 + 전체 재그리기.
/// 행 셀(종류)·상태바는 페인트 시점 tr() 조회라 재그리기만으로 반영.
/// 한계(α): 컬럼 폭이 기본값으로 재설정된다(제목 재구성이 set_metrics 경유).
unsafe fn apply_lang(hwnd: HWND, st: &mut State, inv: &mut Invalidations) {
    let data = config::data_dir();
    st.langs = i18n::discover(&data);
    let code = i18n::resolve_code(&st.lang_setting, &system_ui_lang(), &st.langs);
    i18n::activate(i18n::load(&code, &data));
    st.menubar.set_menus(
        build_menus(
            st.show_hidden,
            st.show_dotfiles,
            st.panels[0].dock_visible(),
            st.launcher_visible,
            st.theme_mode,
            &st.lang_setting,
            &st.langs,
            &st.view_mode,
            &st.panel_mode,
            single_info(st),
            st.col_width_sync,
        ),
        inv,
    );
    let m = panel_metrics(st.dpi);
    let cols = columns(st.dpi);
    st.panels[0].set_metrics(m, cols.clone(), inv);
    st.panels[1].set_metrics(m, cols, inv);
    let _ = InvalidateRect(Some(hwnd), None, false);
}

/// 활성 패널 전환(클릭·Tab) — 탭 바 accent로 시각화.
unsafe fn set_active(hwnd: HWND, st: &mut State, idx: usize) {
    let idx = if single_panel(st) { 0 } else { idx }; // 싱글 패널 = 좌 고정(07-16)
    if st.active != idx {
        st.active = idx;
        let mut inv = Invalidations::default();
        sync_focus_visuals(st, &mut inv);
        flush_invalidations(hwnd, &mut inv);
        update_title(hwnd, st, "");
    }
}

/// 포커스 시각 동기(QA 07-15) — **실제 키 포커스가 있는 영역만 강조**:
/// 탭 바·리스트 선택색=활성 패널이면서 터미널 비포커스(터미널 포커스 중엔 패널 전부
/// 비활성 표시 — 사용자 지시) · 도크 스트립(활성 종류 라벨·[→])=그 패널 터미널이
/// 키 포커스일 때만 accent.
unsafe fn sync_focus_visuals(st: &mut State, inv: &mut Invalidations) {
    for i in 0..2 {
        st.panels[i].set_focused(st.active == i && st.term_focus.is_none(), inv);
        st.panels[i].dock.set_focused(st.term_focus == Some(i), inv);
    }
}

/// F3 — 활성 패널 200프레임 스크롤 벤치.
unsafe fn bench(hwnd: HWND, st: &mut State) {
    let ctx_ev = InputEvent::Key {
        key: Key::Home,
        shift: false,
        ctrl: false,
    };
    let mut inv = Invalidations::default();
    st.active_panel().on_event(&ctx_ev, &mut inv);
    flush_invalidations(hwnd, &mut inv);
    let _ = UpdateWindow(hwnd);
    st.stats.reset();
    for _ in 0..BENCH_FRAMES {
        if let Some(s) = state_of(hwnd) {
            let mut inv = Invalidations::default();
            s.active_panel()
                .on_event(&InputEvent::Wheel { delta: -120 }, &mut inv);
            flush_invalidations(hwnd, &mut inv);
        }
        let _ = UpdateWindow(hwnd);
    }
    if let Some(s) = state_of(hwnd) {
        update_title(hwnd, s, " · 벤치 완료");
    }
}

/// 터미널 비문자 키 → VT 시퀀스(M4-3 — 문자·Enter·Backspace·Ctrl+문자는 WM_CHAR 경로).
fn term_key_seq(vk: u16) -> Option<&'static str> {
    match vk {
        k if k == VK_UP.0 => Some("\x1b[A"),
        k if k == VK_DOWN.0 => Some("\x1b[B"),
        k if k == VK_RIGHT.0 => Some("\x1b[C"),
        k if k == VK_LEFT.0 => Some("\x1b[D"),
        k if k == VK_HOME.0 => Some("\x1b[H"),
        k if k == VK_END.0 => Some("\x1b[F"),
        k if k == VK_DELETE.0 => Some("\x1b[3~"),
        k if k == VK_PRIOR.0 => Some("\x1b[5~"),
        k if k == VK_NEXT.0 => Some("\x1b[6~"),
        _ => None,
    }
}

/// 스플리터 자석 스냅(원본 F9 — QA 07-14): 창 50% 및 `other`(반대편 구분선) 위치에
/// [`SNAP_PX`] 이내로 근접하면 정렬. **Alt 유지 = 스냅 해제**(정밀 배치).
unsafe fn snap_split_x(hwnd: HWND, st: &State, x: i32, other: i32) -> i32 {
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_MENU;
    if GetKeyState(VK_MENU.0 as i32) < 0 {
        return x; // Alt = 스냅 없음
    }
    let rc = client_rect(hwnd);
    let snap = (SNAP_PX * st.dpi as i32) / 96;
    let mid = rc.w / 2;
    if (x - mid).abs() <= snap {
        return mid;
    }
    if st.panels[0].dock_visible() && (x - other).abs() <= snap {
        return other;
    }
    x
}

/// 휠 대상 패널·클라이언트 좌표(QA 07-14) — WM_MOUSEWHEEL lparam은 **화면 좌표**.
/// 마우스 아래 패널로 라우팅(스플리터 존/패널 밖이면 활성 패널 폴백) — hover 스크롤.
unsafe fn wheel_target(hwnd: HWND, st: &State, lparam: LPARAM) -> (usize, i32, i32) {
    let mut pt = windows::Win32::Foundation::POINT {
        x: (lparam.0 & 0xFFFF) as i16 as i32,
        y: ((lparam.0 >> 16) & 0xFFFF) as i16 as i32,
    };
    let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut pt);
    (st.panel_at_pt(pt.x, pt.y).unwrap_or(st.active), pt.x, pt.y)
}

/// 시스템 캐럿 깜빡임 주기(ms) — 0/비활성 값은 표준 530ms로 보정.
fn caret_blink_ms() -> u32 {
    let t = unsafe { windows::Win32::UI::WindowsAndMessaging::GetCaretBlinkTime() };
    if t == 0 || t == u32::MAX {
        530
    } else {
        t
    }
}

/// 터미널 마우스 이벤트 전달(X-5 — Zellij 등 TUI): 마우스 모드(DECSET)가 켜져 있고
/// SGR(1006) 인코딩일 때 `ESC[<b;col;row M/m`을 stdin으로. 전송했으면 `true`
/// (호스트는 로컬 선택/스크롤을 억제). 레거시(비SGR) 인코딩은 바이트 >127이라
/// 미지원 — 현대 TUI는 전부 1006(α).
fn term_send_mouse(t: &TermState, x: i32, y: i32, btn: u8, press: bool) -> bool {
    let Some((_, sgr)) = t.screen.mouse_mode() else {
        return false;
    };
    if !sgr {
        return false;
    }
    let (rc, cw, ch) = t.grid;
    if !rc.contains(nexa_gui::Point { x, y }) {
        return false;
    }
    let col = ((x - rc.x - 2) / cw.max(1)).max(0) as usize + 1;
    let row = ((y - rc.y - 1) / ch.max(1)).max(0) as usize + 1;
    t.pty.write(&format!(
        "\x1b[<{btn};{col};{row}{}",
        if press { 'M' } else { 'm' }
    ));
    true
}

/// 터미널 선택 끝점 갱신(QA 07-14) — 그리드 위/아래로 벗어나 있으면 1줄 자동 스크롤 후
/// 클램프한 좌표의 셀로 확장(엣지 자동 스크롤 — MOUSEMOVE·타이머 공용).
fn term_drag_extend(t: &mut TermState, x: i32, y: i32) {
    let (rc, _, _) = t.grid;
    if y < rc.y {
        t.scroll_view(1);
    } else if y >= rc.bottom() {
        t.scroll_view(-1);
    }
    let cell = t.cell_at(
        x.clamp(rc.x, (rc.right() - 1).max(rc.x)),
        y.clamp(rc.y, (rc.bottom() - 1).max(rc.y)),
    );
    if let Some((a, _)) = t.sel {
        t.sel = Some((a, cell));
    }
}

/// 좌표가 패널의 도크 터미널 그리드 위인가(QA 07-14 — 휠 스크롤백·선택 히트).
fn term_hit(st: &State, panel: usize, x: i32, y: i32) -> bool {
    st.panels[panel].dock_visible()
        && st.panels[panel].dock.active_kind() == 2
        && st.terms[panel]
            .as_ref()
            .is_some_and(|t| t.grid.0.contains(nexa_gui::Point { x, y }))
}

/// 도크 rect 무효화(터미널 스크롤/선택 갱신용).
unsafe fn invalidate_dock(hwnd: HWND, st: &State, panel: usize) {
    let r = st.panels[panel].dock.bounds();
    let rc = RECT {
        left: r.x,
        top: r.y,
        right: r.right(),
        bottom: r.bottom(),
    };
    let _ = InvalidateRect(Some(hwnd), Some(&rc), false);
}

/// 설정 창 열기(S6 — Ctrl+, 원본 docs/40): 현재 값 스냅샷 → 모달(State 참조 차단 —
/// 재진입 규약) → 저장 시 적용(테마/언어=기존 명령 경로 재사용·글꼴=백엔드 재생성)
/// + settings.cfg 즉시 저장(원본 PREF 영속 규율).
unsafe fn open_prefs(hwnd: HWND) {
    let req = state_of(hwnd).map(|st| {
        (
            crate::prefs::PrefValues {
                theme: st.theme_mode.as_str().into(),
                lang: st.lang_setting.clone(),
                langs: st.langs.iter().map(|(c, _)| c.clone()).collect(),
                term_font: st.term_font.clone(),
                term_font_size: st.term_font_size,
                term_wrap: st.term_wrap,
                term_cols: st.term_cols,
                dlg_font: st.dlg_font.family.clone(),
                dlg_font_size: st.dlg_font.size_pt,
                base_font: st.base_font.clone(),
                base_font_size: st.base_font_size,
                ctx_font: st.ctx_font.clone(),
                ctx_font_size: st.ctx_font_size,
                status_font: st.status_font.clone(),
                status_font_size: st.status_font_size,
                list_font: st.list_font.clone(),
                list_font_size: st.list_font_size,
                list_folder_bold: st.list_folder_bold,
                header_bold: st.header_bold,
                header_italic: st.header_italic,
                show_hidden: st.show_hidden,
                show_dotfiles: st.show_dotfiles,
                dock: st.panels[0].dock_visible(),
                sort_folders_first: st.sort_folders_first,
                sort_case_sensitive: st.sort_case_sensitive,
                nav_up_align: st.nav_up_align.clone(),
                tab_dblclick: st.tab_dblclick.clone(),
                typeahead_scope: st.ta_scope.clone(),
                typeahead_reset_ms: st.ta_reset_ms,
                typeahead_pos: st.ta_pos,
                typeahead_special: st.ta_special,
                typeahead_space: st.ta_space,
                typeahead_backspace: st.ta_backspace,
            },
            st.dlg_font.clone(),
        )
    });
    let Some((vals, dfont)) = req else { return };
    // VS Code식 즉시 적용(X-8): 변경은 WM_APP_PREFS_APPLY 통지로 실시간 반영되고,
    // 닫기 시 최종 값으로 한 번 더 적용·영속(포커스 이탈 전 편집 값 수거분 — 멱등).
    let Some(v) = crate::prefs::show(hwnd, vals, &dfont) else {
        return;
    };
    apply_prefs(hwnd, &v);
}

/// 일괄 이름변경(M5-1 — 원본 docs/25 §7): 선택 항목 → 모달 다이얼로그(미리보기·충돌) →
/// 확정 시 순차 rename + **MoveBatchOp 트랜잭션 1건**(B-13u — Ctrl+Z로 배치 전체 되돌림).
/// 실패 항목은 개별 격리(성공분만 undo 기록 — 무결성 우선, MoveBatchOp 규약과 동일).
unsafe fn open_bulk_rename(hwnd: HWND) {
    let req = state_of(hwnd).map(|st| {
        let targets: Vec<(PathBuf, bool)> = keyboard_targets(st)
            .into_iter()
            .map(|p| {
                let is_dir = p.is_dir();
                (p, is_dir)
            })
            .collect();
        (targets, st.dlg_font.clone())
    });
    let Some((targets, dfont)) = req else { return };
    if targets.is_empty() {
        if let Some(st) = state_of(hwnd) {
            update_title(hwnd, st, &format!(" · {}", tr("bulk.noSelection")));
        }
        return;
    }
    // 모달(State 차용 없음 — prefs와 동일 재진입 규약)
    let tz = tz_offset_min();
    let Some(pairs) = crate::bulkrename::show(hwnd, &targets, &dfont, tz) else {
        return;
    };
    let mut done: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut errors = 0usize;
    for (old_path, new_name) in &pairs {
        match nexa_ops::rename(old_path, new_name) {
            Ok(new_path) => done.push((old_path.clone(), new_path)),
            Err(_) => errors += 1,
        }
    }
    let Some(st) = state_of(hwnd) else { return };
    if !done.is_empty() {
        for (old, new) in &done {
            st.active_panel().rename_expanded(old, new); // F18 접두사 치환
        }
        let desc = trf("bulk.done", &[&done.len().to_string()]);
        st.history
            .push(Box::new(nexa_ops::history::MoveBatchOp::new(
                done.clone(),
                desc,
            )));
    }
    let mut note = format!(" · {}", trf("bulk.done", &[&done.len().to_string()]));
    if errors > 0 {
        note.push_str(&format!(" · {}", trf("bulk.fail", &[&errors.to_string()])));
    }
    reload_both(hwnd, st, &note);
}

/// 설정 값 적용 + 즉시 영속(X-8 — 설정 창 실시간 통지·닫기 공용, 멱등).
unsafe fn apply_prefs(hwnd: HWND, v: &crate::prefs::PrefValues) {
    let Some(st) = state_of(hwnd) else { return };
    // 테마/언어 — 기존 명령 경로 재사용(라디오 동기·동적 전환 포함). 무변경 시 생략(즉시
    // 적용이 잦아 불필요한 전체 재도장 방지).
    if v.theme != st.theme_mode.as_str() {
        match v.theme.as_str() {
            "light" => run_command(hwnd, st, CMD_THEME_LIGHT),
            "dark" => run_command(hwnd, st, CMD_THEME_DARK),
            _ => run_command(hwnd, st, CMD_THEME_SYSTEM),
        }
    }
    let Some(st) = state_of(hwnd) else { return };
    if v.lang != st.lang_setting {
        if v.lang == "system" {
            run_command(hwnd, st, CMD_LANG_SYSTEM);
        } else if let Some(i) = st.langs.iter().position(|(c, _)| *c == v.lang) {
            run_command(hwnd, st, CMD_LANG_BASE + i as u32);
        }
    }
    let Some(st) = state_of(hwnd) else { return };
    // 글꼴 — 터미널은 백엔드 재생성(mono_format·폴백 체인·글리프 캐시 재구축)
    if v.term_font != st.term_font || v.term_font_size != st.term_font_size {
        st.term_font = v.term_font.clone();
        st.term_font_size = v.term_font_size;
        st.dw = None;
    }
    // 폰트 슬롯(X-12) — 기본/우클릭 메뉴/상태바/파일 목록: 변경 시 백엔드 재생성
    // (슬롯 포맷·레이아웃 캐시 재구축). 우클릭 메뉴는 저장만(α — HMENU = OS 폰트 규약,
    // 자체 그리기 메뉴 전환 시 적용).
    if v.base_font != st.base_font
        || v.base_font_size != st.base_font_size
        || v.status_font != st.status_font
        || v.status_font_size != st.status_font_size
        || v.list_font != st.list_font
        || v.list_font_size != st.list_font_size
    {
        st.base_font = v.base_font.clone();
        st.base_font_size = v.base_font_size.clamp(8, 32);
        st.status_font = v.status_font.clone();
        st.status_font_size = v.status_font_size.clamp(8, 32);
        st.list_font = v.list_font.clone();
        st.list_font_size = v.list_font_size.clamp(8, 32);
        st.dw = None;
    }
    if v.ctx_font != st.ctx_font || v.ctx_font_size != st.ctx_font_size {
        st.ctx_font = v.ctx_font.clone();
        st.ctx_font_size = v.ctx_font_size.clamp(8, 32);
    }
    // 파일 목록 장식(X-12) — 폴더 굵게·헤더 굵게/이탤릭: 전 탭 즉시
    if v.list_folder_bold != st.list_folder_bold
        || v.header_bold != st.header_bold
        || v.header_italic != st.header_italic
    {
        st.list_folder_bold = v.list_folder_bold;
        st.header_bold = v.header_bold;
        st.header_italic = v.header_italic;
        let mut inv = Invalidations::default();
        st.panels[0].set_font_decor(v.list_folder_bold, v.header_bold, v.header_italic, &mut inv);
        st.panels[1].set_font_decor(v.list_folder_bold, v.header_bold, v.header_italic, &mut inv);
        flush_invalidations(hwnd, &mut inv);
    }
    st.dlg_font = crate::dialog::DlgFont {
        family: v.dlg_font.clone(),
        size_pt: v.dlg_font_size,
    };
    // 파일 목록 필터(숨김·닷파일) — 변경 시 양쪽 재열기(X-7 설정 창 경유)
    if v.show_hidden != st.show_hidden || v.show_dotfiles != st.show_dotfiles {
        st.show_hidden = v.show_hidden;
        st.show_dotfiles = v.show_dotfiles;
        // 무효화 수집기 공유(X-16) — 버려지는 Invalidations 4건은 체크 표시의
        // 재도장 힌트를 유실시키고 말미 전창 무효화에 기대던 취약 지점
        let mut inv = Invalidations::default();
        st.menubar
            .set_checked(CMD_TOGGLE_HIDDEN, st.show_hidden, &mut inv);
        st.menubar
            .set_checked(CMD_TOGGLE_DOTFILES, st.show_dotfiles, &mut inv);
        st.toolbar
            .set_checked(CMD_TOGGLE_HIDDEN, st.show_hidden, &mut inv);
        st.toolbar
            .set_checked(CMD_TOGGLE_DOTFILES, st.show_dotfiles, &mut inv);
        let ctx = st.nav_ctx();
        st.panels[0].reopen_filtered(ctx, &mut inv);
        st.panels[1].reopen_filtered(ctx, &mut inv);
        flush_invalidations(hwnd, &mut inv);
    }
    // 하단 도크 표시 토글(설정 창 경유)
    if v.dock != st.panels[0].dock_visible() {
        let mut inv = Invalidations::default();
        st.panels[0].set_dock_visible(v.dock, &mut inv);
        st.panels[1].set_dock_visible(v.dock, &mut inv);
        st.menubar.set_checked(CMD_TOGGLE_DOCK, v.dock, &mut inv);
        layout(hwnd, st, &mut inv);
        update_dock_info(st, &mut inv);
        flush_invalidations(hwnd, &mut inv);
    }
    // 터미널 줄 바꿈·고정 열(X-3) — 다음 페인트에서 cols 재계산·PTY resize
    if v.term_wrap != st.term_wrap || v.term_cols != st.term_cols {
        st.term_wrap = v.term_wrap;
        st.term_cols = v.term_cols.clamp(80, 1000);
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
    // 폴더 우선 정렬 토글(G-13) — 전 탭 즉시 재정렬
    if v.sort_folders_first != st.sort_folders_first {
        st.sort_folders_first = v.sort_folders_first;
        let mut inv = Invalidations::default();
        st.panels[0].set_folders_first(v.sort_folders_first, &mut inv);
        st.panels[1].set_folders_first(v.sort_folders_first, &mut inv);
        flush_invalidations(hwnd, &mut inv);
    }
    // 대소문자 구분 정렬 토글(07-15) — 전 탭 즉시 재정렬
    if v.sort_case_sensitive != st.sort_case_sensitive {
        st.sort_case_sensitive = v.sort_case_sensitive;
        let mut inv = Invalidations::default();
        st.panels[0].set_sort_case(v.sort_case_sensitive, &mut inv);
        st.panels[1].set_sort_case(v.sort_case_sensitive, &mut inv);
        flush_invalidations(hwnd, &mut inv);
    }
    // 탭 더블클릭 동작(07-15)
    if v.tab_dblclick != st.tab_dblclick {
        st.tab_dblclick = v.tab_dblclick.clone();
    }
    // 타입어헤드 옵션(07-15) — 전 탭 즉시 적용
    if v.typeahead_scope != st.ta_scope
        || v.typeahead_reset_ms != st.ta_reset_ms
        || v.typeahead_pos != st.ta_pos
        || v.typeahead_special != st.ta_special
        || v.typeahead_space != st.ta_space
        || v.typeahead_backspace != st.ta_backspace
    {
        st.ta_scope = v.typeahead_scope.clone();
        st.ta_reset_ms = v.typeahead_reset_ms.clamp(200, 10_000);
        st.ta_pos = v.typeahead_pos.clamp(0, 8);
        st.ta_special = v.typeahead_special;
        st.ta_space = v.typeahead_space;
        st.ta_backspace = v.typeahead_backspace;
        let scope = scope_of(&st.ta_scope);
        let mut inv = Invalidations::default();
        for i in 0..2 {
            st.panels[i].set_typeahead_opts(
                scope,
                st.ta_reset_ms as u64,
                st.ta_special,
                st.ta_space,
                st.ta_backspace,
                st.ta_pos as u8,
                &mut inv,
            );
        }
        flush_invalidations(hwnd, &mut inv);
    }
    // Alt+↑ 자동 선택 배치(07-15)
    if v.nav_up_align != st.nav_up_align {
        st.nav_up_align = v.nav_up_align.clone();
        let align = align_of(&st.nav_up_align);
        st.panels[0].set_nav_up_align(align);
        st.panels[1].set_nav_up_align(align);
    }
    // 즉시 영속(원본 PREF 규율 — 종료 저장과 별개로 설정만 저장)
    let settings = current_settings(st);
    let _ = config::save(&config::data_dir(), SETTINGS_FILE, &settings.serialize());
    let _ = InvalidateRect(Some(hwnd), None, false);
    update_title(hwnd, st, "");
}

/// 설정 문자열 → 타입어헤드 검색 범위(미지 값은 가시 스트림 — 원본 docs/32 기본).
fn scope_of(s: &str) -> nexa_tree::FindScope {
    match s {
        "global" => nexa_tree::FindScope::GlobalFirst,
        "level" => nexa_tree::FindScope::CurrentLevel,
        _ => nexa_tree::FindScope::VisibleStream,
    }
}

/// 싱글 패널 모드인가(07-16 — 우 패널 숨김·상태 보존).
fn single_panel(st: &State) -> bool {
    st.panel_mode == "single"
}

/// 정보(도크) 모드의 **효과**: 싱글 패널이면 무조건 싱글(사용자 확정 규칙 —
/// 선호값 info_mode는 보존되어 듀얼 복귀 시 원복).
fn single_info(st: &State) -> bool {
    single_panel(st) || st.info_mode == "single"
}

/// 설정 문자열 → 보기 모드(미지 값은 트리 — 기존 동작).
fn mode_of(s: &str) -> nexa_gui::widgets::ViewMode {
    use nexa_gui::widgets::ViewMode;
    match s {
        "flat" => ViewMode::Flat,
        "tiles" => ViewMode::Tiles,
        _ => ViewMode::Tree,
    }
}

/// 설정 문자열 → 뷰 배치(Alt+↑ 자동 선택 — 미지 값은 중단).
fn align_of(s: &str) -> nexa_gui::widgets::ScrollAlign {
    use nexa_gui::widgets::ScrollAlign;
    match s {
        "top" => ScrollAlign::Top,
        "bottom" => ScrollAlign::Bottom,
        _ => ScrollAlign::Center,
    }
}

/// 현재 상태 → 세션 스냅샷(종료 저장·디바운스 자동 저장 공용 — 단일 원천, 07-15).
fn current_session(st: &mut State) -> Session {
    let (t0, a0) = st.panels[0].session();
    let (t1, a1) = st.panels[1].session();
    Session {
        active_panel: st.active,
        panels: [
            PanelSession {
                tabs: t0,
                active: a0,
                expanded: st.panels[0].session_expanded(),
                locked: st.panels[0].session_locked(),
                pinned: st.panels[0].session_pinned(),
                modes: st.panels[0].session_modes(),
                col_widths: st.panels[0].col_widths(),
            },
            PanelSession {
                tabs: t1,
                active: a1,
                expanded: st.panels[1].session_expanded(),
                locked: st.panels[1].session_locked(),
                pinned: st.panels[1].session_pinned(),
                modes: st.panels[1].session_modes(),
                col_widths: st.panels[1].col_widths(),
            },
        ],
    }
}

/// 현재 상태 → 설정 스냅샷(즉시 영속·종료 저장 공용 — 단일 원천).
/// 설정 즉시 영속(QA 07-17): 메뉴 라디오·토글도 변경 즉시 저장 — 종료 저장에만
/// 의존하면 비정상 종료 시 유실(언어 변경 유실 사고).
/// 컬럼 리사이즈 후 폭 동기(사용자 확정 07-18): 같은 패널 전 탭 = 상속,
/// `col_width_sync`면 반대 패널도 동일 폭. 마우스 디스패치 뒤 폴링.
fn sync_col_widths(st: &mut State, inv: &mut Invalidations) {
    for i in 0..2 {
        if st.panels[i].take_col_resized() {
            let w = st.panels[i].col_widths();
            st.panels[i].apply_col_widths(&w, inv);
            if st.col_width_sync {
                st.panels[1 - i].apply_col_widths(&w, inv);
            }
        }
    }
}

fn persist_settings(st: &State) {
    let settings = current_settings(st);
    let _ = config::save(&config::data_dir(), SETTINGS_FILE, &settings.serialize());
}

fn current_settings(st: &State) -> Settings {
    Settings {
        theme: st.theme_mode.as_str().into(),
        view_mode: st.view_mode.clone(),
        panel_mode: st.panel_mode.clone(),
        info_mode: st.info_mode.clone(),
        base_font: st.base_font.clone(),
        base_font_size: st.base_font_size,
        ctx_font: st.ctx_font.clone(),
        ctx_font_size: st.ctx_font_size,
        status_font: st.status_font.clone(),
        status_font_size: st.status_font_size,
        list_font: st.list_font.clone(),
        list_font_size: st.list_font_size,
        list_folder_bold: st.list_folder_bold,
        header_bold: st.header_bold,
        header_italic: st.header_italic,
        col_width_sync: st.col_width_sync,
        lang: st.lang_setting.clone(),
        show_hidden: st.show_hidden,
        show_dotfiles: st.show_dotfiles,
        split: st.split,
        dock: st.panels[0].dock_visible(),
        sort_folders_first: st.sort_folders_first,
        sort_case_sensitive: st.sort_case_sensitive,
        nav_up_align: st.nav_up_align.clone(),
        tab_dblclick: st.tab_dblclick.clone(),
        typeahead_scope: st.ta_scope.clone(),
        typeahead_reset_ms: st.ta_reset_ms,
        typeahead_pos: st.ta_pos,
        typeahead_special: st.ta_special,
        typeahead_space: st.ta_space,
        typeahead_backspace: st.ta_backspace,
        dock_ratio: st.panels[0].dock_ratio(),
        dock_split: st.dock_split,
        term_font: st.term_font.clone(),
        term_font_size: st.term_font_size,
        term_wrap: st.term_wrap,
        term_cols: st.term_cols,
        dlg_font: st.dlg_font.family.clone(),
        dlg_font_size: st.dlg_font.size_pt,
        launcher: st.launcher_visible,
        launcher_items: Some(st.launcher_items.clone()),
        launcher_seed: crate::launcher::SEED_VERSION,
    }
}

/// 파일 실행(더블클릭·Enter·Alt+↓ — QA 07-14) — 기본 연결 프로그램(탐색기 동일).
unsafe fn shell_open(hwnd: HWND, file: &std::path::Path) {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = HSTRING::from(file.as_os_str());
    let _ = ShellExecuteW(
        Some(hwnd),
        w!("open"),
        PCWSTR(wide.as_ptr()),
        None,
        None,
        SW_SHOWNORMAL,
    );
}

/// 탭 우클릭 메뉴(편의 UX ② — 원본 TAB-MENU): 잠금 토글·복제·닫기. 네이티브 팝업 —
/// TrackPopupMenuEx 모달 루프 동안 wndproc 재진입이 있으므로 **State 참조 없이** 표시 후
/// 결과만 재획득해 반영(shellmenu 동일 규약).
unsafe fn show_tab_menu(hwnd: HWND, panel: usize, tab: usize) {
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, TrackPopupMenuEx, MF_GRAYED,
        MF_STRING, TPM_LEFTALIGN, TPM_RETURNCMD, TPM_TOPALIGN,
    };
    const CMD_LOCK: usize = 1;
    const CMD_DUP: usize = 2;
    const CMD_CLOSE: usize = 3;
    const CMD_PIN: usize = 4;
    let (locked, pinned, count) = match state_of(hwnd) {
        Some(st) if tab < st.panels[panel].tab_count() => (
            st.panels[panel].tab_locked(tab),
            st.panels[panel].tab_pinned(tab),
            st.panels[panel].tab_count(),
        ),
        _ => return,
    };
    let Ok(menu) = CreatePopupMenu() else { return };
    let append = |id: usize, label: &str, enabled: bool| {
        let wide: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
        let flags = if enabled {
            MF_STRING
        } else {
            MF_STRING | MF_GRAYED
        };
        let _ = AppendMenuW(menu, flags, id, PCWSTR(wide.as_ptr()));
    };
    append(
        CMD_LOCK,
        &if locked {
            tr("tab.unlock")
        } else {
            tr("tab.lock")
        },
        true,
    );
    append(
        CMD_PIN,
        &if pinned {
            tr("tab.unpin")
        } else {
            tr("tab.pin")
        },
        true,
    );
    append(CMD_DUP, &tr("tab.duplicate"), true);
    append(CMD_CLOSE, &tr("tab.close"), !locked && count > 1);
    let mut pt = windows::Win32::Foundation::POINT::default();
    let _ = GetCursorPos(&mut pt);
    let cmd = TrackPopupMenuEx(
        menu,
        (TPM_LEFTALIGN | TPM_TOPALIGN | TPM_RETURNCMD).0,
        pt.x,
        pt.y,
        hwnd,
        None,
    );
    let _ = DestroyMenu(menu);
    let Some(st) = state_of(hwnd) else { return };
    let mut inv = Invalidations::default();
    match cmd.0 as usize {
        CMD_LOCK => st.panels[panel].toggle_tab_lock(tab, &mut inv),
        CMD_PIN => st.panels[panel].toggle_tab_pin(tab, &mut inv),
        CMD_DUP => {
            let ctx = st.nav_ctx();
            st.panels[panel].duplicate_tab(tab, ctx, &mut inv);
        }
        CMD_CLOSE => st.panels[panel].close_tab(tab, &mut inv),
        _ => {}
    }
    flush_invalidations(hwnd, &mut inv);
    update_title(hwnd, st, "");
    update_status(hwnd, st);
}

/// 경로바 편집 텍스트 **변경 후** 자동완성 갱신(PATH-SUG — 원본 TextChanged 대응).
/// 환경변수 확장 후 베이스 폴더의 하위 폴더를 제안(최대 20 — 원본 상한).
fn update_path_suggest(st: &mut State, inv: &mut Invalidations) {
    let Some(text) = st.active_panel().pathbar.edit_text() else {
        return;
    };
    let expanded = crate::pathinput::expand_env(&text);
    let items = crate::pathinput::suggest_folders(&expanded, crate::pathinput::fs_dirs, 20);
    st.active_panel().pathbar.set_suggestions(items, inv);
}

/// 붙여넣기용 클립보드 텍스트 정제(편집 필드는 한 줄) — 첫 줄만·제어 문자 제거.
unsafe fn paste_line() -> Option<String> {
    let raw = crate::clipboard::read_text()?;
    let line: String = raw
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    (!line.is_empty()).then_some(line)
}

/// 편집 모드 키 매핑(경로바 편집·인라인 리네임 공용 — edit.rs EditKey).
fn edit_key_of(vk: u16, ctrl: bool) -> Option<nexa_gui::EditKey> {
    use nexa_gui::EditKey;
    match vk {
        k if k == VK_LEFT.0 => Some(EditKey::Left),
        k if k == VK_RIGHT.0 => Some(EditKey::Right),
        k if k == VK_HOME.0 => Some(EditKey::Home),
        k if k == VK_END.0 => Some(EditKey::End),
        k if k == VK_DELETE.0 => Some(EditKey::DeleteForward),
        k if k == b'A' as u16 && ctrl => Some(EditKey::SelectAll),
        _ => None,
    }
}

fn vk_to_key(vk: u16) -> Option<Key> {
    match vk {
        k if k == VK_UP.0 => Some(Key::Up),
        k if k == VK_DOWN.0 => Some(Key::Down),
        k if k == VK_PRIOR.0 => Some(Key::PageUp),
        k if k == VK_NEXT.0 => Some(Key::PageDown),
        k if k == VK_HOME.0 => Some(Key::Home),
        k if k == VK_END.0 => Some(Key::End),
        k if k == VK_RIGHT.0 => Some(Key::Right),
        k if k == VK_LEFT.0 => Some(Key::Left),
        k if k == VK_SPACE.0 => Some(Key::Space),
        _ => None,
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // 상주 자니터 활동 기록(M2-8) — 키보드(0x100~0x109)·마우스(0x200~0x20E) 입력 전 범위
    if (0x0100..=0x0109).contains(&msg) || (0x0200..=0x020E).contains(&msg) {
        if let Some(st) = state_of(hwnd) {
            note_activity(hwnd, st);
        }
    }
    match msg {
        WM_NCCREATE => {
            let cs = &*(lparam.0 as *const CREATESTRUCTW);
            let ptr = cs.lpCreateParams as *mut State;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize);
            if let Some(st) = ptr.as_mut() {
                st.dpi = GetDpiForWindow(hwnd);
                let mut inv = Invalidations::default();
                let m = panel_metrics(st.dpi);
                let cols = columns(st.dpi);
                st.panels[0].set_metrics(m, cols.clone(), &mut inv);
                st.panels[1].set_metrics(m, cols, &mut inv);
                // 세션 컬럼 폭 복원(07-18) — DPI 기본 폭 리셋 **이후** 적용
                for i in 0..2 {
                    let w = std::mem::take(&mut st.pending_colw[i]);
                    if !w.is_empty() {
                        st.panels[i].apply_col_widths(&w, &mut inv);
                    }
                }
                sync_focus_visuals(st, &mut inv);
                st.menubar.set_metrics(m.row_h, m.pad_x, &mut inv);
                st.toolbar.set_metrics(m.row_h, m.pad_x, &mut inv);
                st.launcherbar.set_metrics(m.row_h, m.pad_x, &mut inv);
                st.statusbar.set_metrics(m.row_h, m.pad_x, &mut inv);
                apply_titlebar_theme(hwnd, st.theme == Theme::dark()); // 본문과 타이틀바 일치
                update_title(hwnd, st, "");
                update_status(hwnd, st);
                st.last_activity_ms = now_ms();
                SetTimer(Some(hwnd), TIMER_JANITOR, JANITOR_TICK_MS, None); // 상주 자니터(M2-8)
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_PAINT => {
            if let Some(st) = state_of(hwnd) {
                paint(hwnd, st);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_SIZE => {
            if let Some(st) = state_of(hwnd) {
                if wparam.0 == windows::Win32::UI::WindowsAndMessaging::SIZE_MINIMIZED as usize {
                    // 최소화 = 즉시 트림(원본 §5-1) — 복원 시 WM_SIZE+페인트가 재적재
                    trim_resident(st);
                    st.trimmed = true;
                    let _ = KillTimer(Some(hwnd), TIMER_JANITOR);
                } else {
                    note_activity(hwnd, st);
                    let mut inv = Invalidations::default();
                    layout(hwnd, st, &mut inv);
                    flush_invalidations(hwnd, &mut inv);
                }
            }
            LRESULT(0)
        }
        // 스플리터 hover 커서(QA 07-14): 좌/우 스플리터(파일·도크)=↔, 도크 상단 경계=↕
        m if m == windows::Win32::UI::WindowsAndMessaging::WM_SETCURSOR => {
            if let Some(st) = state_of(hwnd) {
                let mut pt = windows::Win32::Foundation::POINT::default();
                let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt);
                let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut pt);
                let s = |v: i32| (v * st.dpi as i32) / 96;
                let half = s(SPLIT_HALF).max(1);
                let band = st.panels[0].dock.bounds();
                let dock_on = st.panels[0].dock_visible() && band.h > 0;
                let gap = ((SPLIT_TH * st.dpi as i32) / 96).max(2);
                let strip_top = band.y - gap;
                let on_dock_split = dock_on
                    && pt.y >= band.y
                    && pt.x >= band.right() - 2
                    && pt.x < st.panels[1].dock.bounds().x + 2;
                let on_file_split = pt.y >= st.panels[0].bounds().y
                    && pt.y < st.panels[0].bounds().bottom()
                    && (pt.x - splitter_x(hwnd, st)).abs() <= half;
                let cursor = if dock_on
                    && pt.y >= strip_top - SPLIT_HALF
                    && pt.y <= strip_top + gap + SPLIT_HALF
                {
                    Some(windows::Win32::UI::WindowsAndMessaging::IDC_SIZENS)
                } else if on_dock_split || on_file_split {
                    Some(windows::Win32::UI::WindowsAndMessaging::IDC_SIZEWE)
                } else if st.panels.iter().any(|p| p.rows().resize_hot(pt.x, pt.y)) {
                    // 컬럼 경계 리사이즈 존(QA 07-15) — 드래그 중 포함
                    Some(windows::Win32::UI::WindowsAndMessaging::IDC_SIZEWE)
                } else {
                    None
                };
                if let Some(id) = cursor {
                    if let Ok(c) = LoadCursorW(None, id) {
                        windows::Win32::UI::WindowsAndMessaging::SetCursor(Some(c));
                        return LRESULT(1);
                    }
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_MOUSEWHEEL => {
            if let Some(st) = state_of(hwnd) {
                let delta = (wparam.0 >> 16) as i16 as i32;
                // 마우스 아래 패널로 라우팅(QA 07-14 — 활성 패널이 아닌 hover 기준)
                let (target, px, py) = wheel_target(hwnd, st, lparam);
                // 터미널 위 휠: TUI 마우스 모드(X-5)면 휠 버튼(64/65) 전달, 아니면
                // 스크롤백 보기(3줄/노치, QA 07-14). Shift=항상 로컬.
                // Shift+휠 = 터미널 가로 스크롤(X-3 비줄바꿈 고정 열 — 4열/노치)
                if wparam.0 & MK_SHIFT != 0 && !st.term_wrap && term_hit(st, target, px, py) {
                    if let Some(t) = &mut st.terms[target] {
                        if t.scroll_view_x(-4 * delta / 120) {
                            invalidate_dock(hwnd, st, target);
                        }
                    }
                    return LRESULT(0);
                }
                if wparam.0 & MK_SHIFT == 0 && term_hit(st, target, px, py) {
                    if let Some(t) = &mut st.terms[target] {
                        if t.screen.mouse_mode().is_some_and(|(_, sgr)| sgr) {
                            let btn = if delta > 0 { 64 } else { 65 };
                            for _ in 0..(delta.abs() / 120).max(1) {
                                term_send_mouse(t, px, py, btn, true);
                            }
                            return LRESULT(0);
                        }
                        if t.scroll_view(3 * delta / 120) {
                            invalidate_dock(hwnd, st, target);
                        }
                    }
                    return LRESULT(0);
                }
                let ev = if wparam.0 & MK_SHIFT != 0 {
                    InputEvent::HWheel { delta: -delta }
                } else {
                    InputEvent::Wheel { delta }
                };
                let mut inv = Invalidations::default();
                st.panels[target].on_event(&ev, &mut inv);
                flush_invalidations(hwnd, &mut inv);
            }
            LRESULT(0)
        }
        WM_MOUSEHWHEEL => {
            if let Some(st) = state_of(hwnd) {
                let delta = (wparam.0 >> 16) as i16 as i32;
                let (target, px, py) = wheel_target(hwnd, st, lparam);
                // 터미널 위 가로 휠 = 가로 스크롤(X-3 비줄바꿈 고정 열)
                if !st.term_wrap && term_hit(st, target, px, py) {
                    if let Some(t) = &mut st.terms[target] {
                        if t.scroll_view_x(4 * delta / 120) {
                            invalidate_dock(hwnd, st, target);
                        }
                    }
                    return LRESULT(0);
                }
                let mut inv = Invalidations::default();
                st.panels[target].on_event(&InputEvent::HWheel { delta }, &mut inv);
                flush_invalidations(hwnd, &mut inv);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            if let Some(st) = state_of(hwnd) {
                let (x, y) = mouse_xy(lparam);
                let shift = wparam.0 & MK_SHIFT != 0;
                let ctrl = wparam.0 & MK_CONTROL != 0;
                let mut inv = Invalidations::default();
                SetCapture(hwnd); // 스플리터·리사이즈·러버밴드 드래그 공용
                let sx = splitter_x(hwnd, st);
                let half = (SPLIT_HALF * st.dpi as i32) / 96;
                let ev = InputEvent::MouseDown { x, y, shift, ctrl };
                // 메뉴 우선(드롭다운 오버레이) — 열려 있으면 전용 라우팅
                if st.menubar.is_open() || y < st.toolbar.bounds().y {
                    st.menubar.on_event(&ev, &mut inv);
                    flush_invalidations(hwnd, &mut inv);
                    if let Some(cmd) = st.menubar.take_command() {
                        run_command(hwnd, st, cmd);
                    }
                    return LRESULT(0);
                }
                if y < st.panels[0].bounds().y {
                    // 도구 모음 / 퀵 런처 바(M5-1) 행 — y로 분기
                    let lb = st.launcherbar.bounds();
                    let bar = if lb.h > 0 && y >= lb.y {
                        &mut st.launcherbar
                    } else {
                        &mut st.toolbar
                    };
                    bar.on_event(&ev, &mut inv);
                    let cmd = bar.take_command();
                    flush_invalidations(hwnd, &mut inv);
                    if let Some(cmd) = cmd {
                        run_command(hwnd, st, cmd);
                    }
                    return LRESULT(0);
                }
                if y >= st.statusbar.bounds().y {
                    return LRESULT(0); // 상태바 — 표시 전용
                }
                if st.active_panel().pathbar.is_editing() {
                    if st.active_panel().pathbar.suggest_click(x, y, &mut inv) {
                        // 제안 클릭 = 그 폴더로 즉시 이동(PATH-SUG — 탐색기 동일)
                        let nav = st.nav_ctx();
                        st.active_panel().drain_actions(nav, &mut inv);
                        flush_invalidations(hwnd, &mut inv);
                        update_title(hwnd, st, "");
                        update_status(hwnd, st);
                        return LRESULT(0);
                    }
                    if st
                        .active_panel()
                        .pathbar
                        .bounds()
                        .contains(nexa_gui::Point { x, y })
                    {
                        // 필드 안 클릭 = 캐럿 배치(QA 07-13 — 위젯이 처리)
                        st.active_panel().pathbar.on_event(&ev, &mut inv);
                    } else {
                        // 포커스아웃 = 편집 취소(docs/27 §2)
                        st.active_panel().pathbar.cancel_edit(&mut inv);
                    }
                } else if st.panels[0].dock_visible() && st.panels[0].dock.bounds().h > 0 && {
                    let gap = ((SPLIT_TH * st.dpi as i32) / 96).max(2);
                    let strip_top = st.panels[0].dock.bounds().y - gap;
                    y >= strip_top - SPLIT_HALF && y <= strip_top + gap + SPLIT_HALF
                } {
                    // 도크 밴드 상단 가로 분리선 = 높이 드래그(전폭 — X-6)
                    st.dock_drag = Some(0);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                } else if st.panels[0].dock_visible()
                    && y >= st.panels[0].dock.bounds().y
                    && x >= st.panels[0].dock.bounds().right() - 2
                    && x < st.panels[1].dock.bounds().x + 2
                {
                    // 도크 밴드 좌/우 스플리터(X-6 — 파일 스플리터와 독립)
                    st.dock_split_drag = true;
                    let _ = InvalidateRect(Some(hwnd), None, false);
                } else if !single_panel(st)
                    && y < st.panels[0].bounds().bottom()
                    && (x - sx).abs() <= half.max(1)
                {
                    // 파일 영역 좌/우 스플리터(도크 밴드와 독립 — X-6·싱글 패널 무시)
                    st.split_drag = true;
                    let _ = InvalidateRect(Some(hwnd), None, false);
                } else if let Some(idx) = st.panel_at_pt(x, y) {
                    st.term_focus = None; // 기본 해제 — 아래에서 터미널 클릭이면 재설정(M4-3)
                    set_active(hwnd, st, idx);
                    // 프레스 시점(선택 반영 전) 행·기선택 판정 — **기선택 행만 OLE DnD 후보**.
                    // 미선택 행 드래그=러버밴드(원본 B-4)·리네임 중=텍스트 드래그(QA 07-13 3차)
                    let hit = {
                        let rows = st.panels[idx].rows();
                        (!rows.is_renaming())
                            .then(|| rows.row_at(x, y))
                            .flatten()
                            .filter(|_| !rows.marker_hit(x, y))
                            .and_then(|r| {
                                let tree = rows.source().tree();
                                let id = tree.visible_id(r)?;
                                let path = tree.node_path(id)?.to_string_lossy().into_owned();
                                Some((tree.is_selected(id), path))
                            })
                    };
                    let was_selected = hit.as_ref().is_some_and(|(sel, _)| *sel);
                    // 반대 패널 도크 텍스트 선택 해제(QA 07-15 — 복사 대상은 한 곳만)
                    st.panels[1 - idx].dock.clear_text_selection(&mut inv);
                    st.panels[idx].on_event(&ev, &mut inv);
                    let ctx = st.nav_ctx();
                    st.panels[idx].drain_actions(ctx, &mut inv);
                    // 터미널 [→] = 현재 폴더로 이동(QA 07-14 — 원본 '터미널에서 열기')
                    if st.panels[idx].dock.take_goto() {
                        let dir = st.panels[idx].root_path();
                        if let Some(t) = &mut st.terms[idx] {
                            if t.exited {
                                st.terms[idx] = None; // 재시작(cwd=현재 폴더 lazy start)
                            } else {
                                t.pty.write(&format!("cd \"{}\"\r", dir.display()));
                                t.view_off = 0;
                            }
                        }
                        // 시작 전이면 다음 페인트의 지연 시작이 현재 폴더(cwd)로 연다
                        st.term_caret_on = true;
                        SetTimer(Some(hwnd), TIMER_TERM_CARET, caret_blink_ms(), None);
                        st.term_focus = Some(idx);
                        update_dock_info(st, &mut inv);
                        invalidate_dock(hwnd, st, idx);
                    }
                    // 도크(종류 전환 반영 후) 터미널 영역 클릭 = 키 포커스(M4-3)
                    if st.panels[idx].dock_visible()
                        && y >= st.panels[idx].dock.bounds().y
                        && st.panels[idx].dock.active_kind() == 2
                    {
                        st.term_focus = Some(idx);
                        st.term_caret_on = true;
                        // 캐럿 깜빡임(QA 07-14) — 포커스 동안만, 시스템 깜빡임 주기
                        SetTimer(Some(hwnd), TIMER_TERM_CARET, caret_blink_ms(), None);
                        update_dock_info(st, &mut inv); // 종류 전환 직후 내용 동기
                                                        // 그리드 안 프레스: TUI 마우스 모드(X-5)면 시퀀스 전달(Shift=
                                                        // 로컬 우회 — 터미널 관례), 아니면 로컬 선택 시작(QA 07-14)
                        if let Some(t) = &mut st.terms[idx] {
                            if t.grid.0.contains(nexa_gui::Point { x, y }) {
                                if !shift && term_send_mouse(t, x, y, 0, true) {
                                    st.term_mouse_btn = Some((idx, 0));
                                } else {
                                    let cell = t.cell_at(x, y);
                                    t.sel = Some((cell, cell));
                                    st.term_drag = Some(idx);
                                    invalidate_dock(hwnd, st, idx);
                                }
                            }
                        }
                    } else if st.panels[idx].dock_visible() && y >= st.panels[idx].dock.bounds().y {
                        update_dock_info(st, &mut inv);
                    }
                    // 기선택 항목 재클릭(1s 이상 간격) = 이름 바꾸기 **예약**(진입은 MouseUp —
                    // 드래그가 시작되면 취소 = DnD 우선. 짧은 간격은 더블클릭 시도로 무시)
                    let now = now_ms();
                    st.rename_on_up = false;
                    let _ = KillTimer(Some(hwnd), TIMER_RENAME); // 새 클릭 = 예약 리네임 무효
                    if let Some((_, path)) = &hit {
                        if was_selected && !shift && !ctrl {
                            if let Some((p_idx, p_path, t)) = &st.slow_click {
                                st.rename_on_up = *p_idx == idx
                                    && p_path == path
                                    && now.saturating_sub(*t) >= SLOW_CLICK_RENAME_MS;
                            }
                        }
                        st.slow_click = Some((idx, path.clone(), now));
                    } else {
                        st.slow_click = None;
                    }
                    if was_selected {
                        st.drag_press = Some((x, y)); // 임계 이동 시 OLE 드래그 발신(M3-5 S4)
                    }
                }
                // 클릭으로 확정된 포커스 영역(리스트/터미널) 강조 동기(QA 07-15)
                sync_focus_visuals(st, &mut inv);
                flush_invalidations(hwnd, &mut inv);
                update_title(hwnd, st, "");
                update_status(hwnd, st);
            }
            LRESULT(0)
        }
        WM_RBUTTONDOWN => {
            let mut tab_menu: Option<(usize, usize)> = None;
            if let Some(st) = state_of(hwnd) {
                let (x, y) = mouse_xy(lparam);
                // TUI 마우스 모드(X-5) — 터미널 그리드 우클릭은 앱에 전달(Shift=로컬)
                if GetKeyState(VK_SHIFT.0 as i32) >= 0 {
                    if let Some(ti) = st.term_focus {
                        if term_hit(st, ti, x, y) {
                            if let Some(t) = &st.terms[ti] {
                                if term_send_mouse(t, x, y, 2, true) {
                                    st.term_mouse_btn = Some((ti, 2));
                                    return LRESULT(0);
                                }
                            }
                        }
                    }
                }
                if let Some(idx) = st.panel_at_pt(x, y) {
                    set_active(hwnd, st, idx);
                    let mut inv = Invalidations::default();
                    st.panels[idx].on_event(&InputEvent::RightDown { x, y }, &mut inv);
                    let ctx = st.nav_ctx();
                    st.panels[idx].drain_actions(ctx, &mut inv);
                    flush_invalidations(hwnd, &mut inv);
                    // 탭 우클릭 메뉴(편의 UX ②) — 모달 팝업은 State 참조를 끊고(재진입 규약)
                    tab_menu = st.panels[idx].take_tab_menu().map(|t| (idx, t));
                }
            }
            if let Some((panel, tab)) = tab_menu {
                show_tab_menu(hwnd, panel, tab);
            }
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            // 행 우클릭 = 셸 컨텍스트 메뉴 · 빈 본문 = 폴더 배경 메뉴(M3-4, ADR-0003).
            // 선택 규약(단독 선택/유지/해제)은 RBUTTONDOWN에서 반영됨.
            let (x, y) = mouse_xy(lparam);
            if let Some(st) = state_of(hwnd) {
                // TUI 마우스 릴리스(X-5 — 우클릭) — 컨텍스트 메뉴 억제
                if let Some((ti, 2)) = st.term_mouse_btn {
                    st.term_mouse_btn = None;
                    if let Some(t) = &st.terms[ti] {
                        term_send_mouse(t, x, y, 2, false);
                    }
                    return LRESULT(0);
                }
            }
            let hit = state_of(hwnd).map(|st| {
                let active = st.panel_at(x) == Some(st.active);
                let rows = st.active_panel().rows();
                let on_row = active && rows.row_at(x, y).is_some();
                (on_row, !on_row && active && rows.in_body(x, y))
            });
            match hit {
                Some((true, _)) => show_row_context_menu(hwnd, false),
                Some((_, true)) => show_background_context_menu(hwnd),
                _ => {}
            }
            LRESULT(0)
        }
        // 셸 메뉴 표시 구간 — IContextMenu2/3 메시지 포워딩(동적 서브메뉴·아이콘, ADR-0003:
        // 자기 wndproc 보유 → 원본의 comctl32 서브클래스 불요)
        m if matches!(
            m,
            WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR
        ) =>
        {
            match crate::shellmenu::forward_menu_msg(m, wparam, lparam) {
                Some(r) => r,
                None => DefWindowProcW(hwnd, msg, wparam, lparam),
            }
        }
        WM_MOUSEMOVE => {
            // OLE 드래그 발신(M3-5 S4): 행 누름 후 좌버튼 유지 + 임계 이동 → 모달 드래그.
            // 모달 루프 동안 wndproc 재진입 — State 참조를 끊고 시작(shellmenu 동일 규약).
            {
                let (x, y) = mouse_xy(lparam);
                // TUI 마우스 모션 전달(X-5 — 1002/1003·버튼 유지 중)
                if let Some(st) = state_of(hwnd) {
                    if let Some((ti, b)) = st.term_mouse_btn {
                        if wparam.0 & MK_LBUTTON != 0 || b == 2 {
                            if let Some(t) = &st.terms[ti] {
                                if matches!(t.screen.mouse_mode(), Some((m, _)) if m >= 1002) {
                                    term_send_mouse(t, x, y, b | 32, true);
                                }
                            }
                            return LRESULT(0);
                        }
                        st.term_mouse_btn = None;
                    }
                }
                // 터미널 마우스 선택 드래그(QA 07-14) — 끝점 확장 + 엣지 자동 스크롤
                if let Some(st) = state_of(hwnd) {
                    if let Some(ti) = st.term_drag {
                        if wparam.0 & MK_LBUTTON != 0 {
                            let mut edge = false;
                            if let Some(t) = &mut st.terms[ti] {
                                term_drag_extend(t, x, y);
                                let (rc, _, _) = t.grid;
                                edge = y < rc.y || y >= rc.bottom();
                            }
                            invalidate_dock(hwnd, st, ti);
                            if edge {
                                // 그리드 밖(상/하) 유지 = 자동 스크롤 반복 타이머
                                SetTimer(Some(hwnd), TIMER_TERM_SEL, 60, None);
                            } else {
                                let _ = KillTimer(Some(hwnd), TIMER_TERM_SEL);
                            }
                            return LRESULT(0);
                        }
                        st.term_drag = None;
                        let _ = KillTimer(Some(hwnd), TIMER_TERM_SEL);
                    }
                }
                let drag_paths = if wparam.0 & MK_LBUTTON != 0 {
                    state_of(hwnd).and_then(|st| {
                        let (px, py) = st.drag_press?;
                        use windows::Win32::UI::WindowsAndMessaging::{
                            GetSystemMetrics, SM_CXDRAG, SM_CYDRAG,
                        };
                        let tx = GetSystemMetrics(SM_CXDRAG).max(4);
                        let ty = GetSystemMetrics(SM_CYDRAG).max(4);
                        if (x - px).abs() < tx && (y - py).abs() < ty {
                            return None;
                        }
                        st.drag_press = None;
                        st.rename_on_up = false; // 드래그 시작 = 느린 재클릭 리네임 취소(DnD 우선)
                        let paths = keyboard_targets(st);
                        (!paths.is_empty()).then_some(paths)
                    })
                } else {
                    None
                };
                if let Some(paths) = drag_paths {
                    let _ = ReleaseCapture(); // LBUTTONDOWN의 캡처 해제 — OLE가 소유
                    crate::dnd::begin_drag(&paths);
                    if let Some(st) = state_of(hwnd) {
                        reload_both(hwnd, st, ""); // 이동/복사 결과 반영(수행은 드롭 대상 몫)
                    }
                    return LRESULT(0);
                }
            }
            if let Some(st) = state_of(hwnd) {
                let (x, y) = mouse_xy(lparam);
                let mut inv = Invalidations::default();
                if st.dock_drag.is_some() {
                    // 도크 밴드 높이 드래그(X-6 — 패널 상단~상태바 영역 대비 비율)
                    let top = st.panels[0].bounds().y;
                    let bottom = st.panels[0].dock.bounds().bottom();
                    if bottom > top {
                        let ratio = (bottom - y) as f32 / (bottom - top) as f32;
                        st.panels[0].set_dock_ratio(ratio, &mut inv);
                        st.panels[1].set_dock_ratio(ratio, &mut inv);
                        layout(hwnd, st, &mut inv);
                        update_dock_info(st, &mut inv);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                } else if st.dock_split_drag {
                    // 도크 밴드 좌/우 스플리터 드래그(X-6) — 50%·파일 스플리터에 자석 스냅
                    let rc = client_rect(hwnd);
                    if rc.w > 0 {
                        let x = snap_split_x(hwnd, st, x, splitter_x(hwnd, st));
                        st.dock_split = (x as f32 / rc.w as f32).clamp(0.15, 0.85);
                        layout(hwnd, st, &mut inv);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                } else if st.split_drag {
                    // 파일 좌/우 스플리터 — 50%·도크 스플리터에 자석 스냅(원본 F9)
                    let rc = client_rect(hwnd);
                    if rc.w > 0 {
                        let other = ((rc.w as f32 * st.dock_split) as i32).clamp(0, rc.w);
                        let x = snap_split_x(hwnd, st, x, other);
                        st.split = (x as f32 / rc.w as f32).clamp(0.1, 0.9);
                        layout(hwnd, st, &mut inv);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                } else {
                    let ev = InputEvent::MouseMove { x, y };
                    st.menubar.on_event(&ev, &mut inv);
                    st.toolbar.on_event(&ev, &mut inv);
                    if st.launcherbar.bounds().h > 0 {
                        st.launcherbar.on_event(&ev, &mut inv); // hover(M5-1)
                    }
                    if !st.menubar.is_open() {
                        // 드롭다운 아래 hover 잔상 방지
                        st.panels[0].on_event(&ev, &mut inv);
                        st.panels[1].on_event(&ev, &mut inv);
                        sync_col_widths(st, &mut inv); // 컬럼 폭 동기(07-18)
                                                       // 탭 드래그 재정렬(편의 UX ②) — Move 액션 즉시 반영
                        let ctx = st.nav_ctx();
                        st.panels[0].drain_actions(ctx, &mut inv);
                        st.panels[1].drain_actions(ctx, &mut inv);
                    }
                }
                flush_invalidations(hwnd, &mut inv);
            }
            LRESULT(0)
        }
        WM_CAPTURECHANGED => {
            // 캡처 강탈(팝업·전환 등)로 WM_LBUTTONUP이 안 오는 경로 — 스플리터류 드래그
            // 플래그가 남아 accent 강조가 잔존하는 것을 방어(QA 07-15).
            if let Some(st) = state_of(hwnd) {
                if st.split_drag || st.dock_split_drag || st.dock_drag.is_some() {
                    st.split_drag = false;
                    st.dock_split_drag = false;
                    st.dock_drag = None;
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_LBUTTONUP => {
            if let Some(st) = state_of(hwnd) {
                let (x, y) = mouse_xy(lparam);
                st.drag_press = None; // 드래그 후보 해제(임계 미달 클릭)
                if st.dock_drag.take().is_some() {
                    // 도크 높이 드래그 종료(M4-1 S2) — 강조색 해제 재도장 필수
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                if st.dock_split_drag {
                    // 도크 좌/우 스플리터 종료(X-6) — 강조색 해제 재도장 필수
                    st.dock_split_drag = false;
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                if let Some((ti, b)) = st.term_mouse_btn.take() {
                    // TUI 마우스 릴리스 전달(X-5)
                    if b == 0 {
                        if let Some(t) = &st.terms[ti] {
                            term_send_mouse(t, x, y, b, false);
                        }
                    } else {
                        st.term_mouse_btn = Some((ti, b)); // 우클릭은 RBUTTONUP에서
                    }
                }
                if st.term_drag.take().is_some() {
                    // 터미널 선택 확정(QA 07-14) — 선택은 유지(Ctrl+C 복사), 타이머 종료
                    let _ = KillTimer(Some(hwnd), TIMER_TERM_SEL);
                }
                if st.split_drag {
                    st.split_drag = false;
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                let ev = InputEvent::MouseUp { x, y };
                let mut inv = Invalidations::default();
                st.panels[0].on_event(&ev, &mut inv);
                st.panels[1].on_event(&ev, &mut inv);
                sync_col_widths(st, &mut inv); // 컬럼 폭 동기(07-18)
                flush_invalidations(hwnd, &mut inv);
                // 느린 재클릭 리네임 — 클릭 확정(무드래그) 시 **더블클릭 시간만큼 지연** 후
                // 진입(그 안에 두 번째 클릭 = 더블클릭 열기 → WM_LBUTTONDBLCLK가 취소, QA 07-14)
                if st.rename_on_up {
                    st.rename_on_up = false;
                    SetTimer(Some(hwnd), TIMER_RENAME, GetDoubleClickTime(), None);
                }
            }
            let _ = ReleaseCapture();
            LRESULT(0)
        }
        WM_XBUTTONDOWN => {
            if let Some(st) = state_of(hwnd) {
                let mut inv = Invalidations::default();
                let ctx = st.nav_ctx();
                match (wparam.0 >> 16) & 0xFFFF {
                    1 => st.active_panel().nav_back(ctx, &mut inv),
                    2 => st.active_panel().nav_forward(ctx, &mut inv),
                    _ => {}
                }
                flush_invalidations(hwnd, &mut inv);
                update_title(hwnd, st, "");
            }
            LRESULT(0)
        }
        WM_LBUTTONDBLCLK => {
            // 더블클릭 = 열기 — 지연 중인 느린 재클릭 리네임 취소(QA 07-14)
            let _ = KillTimer(Some(hwnd), TIMER_RENAME);
            let mut exec: Option<PathBuf> = None;
            if let Some(st) = state_of(hwnd) {
                let (x, y) = mouse_xy(lparam);
                if let Some(idx) = st.panel_at(x) {
                    set_active(hwnd, st, idx);
                    // 탭 본체 더블클릭 = 설정 동작(사용자 요청 07-15 — 기본 닫기)
                    if let Some(ti) = st.panels[idx].tabbar.tab_index_at(x, y) {
                        let mut inv = Invalidations::default();
                        match st.tab_dblclick.as_str() {
                            "pin" => st.panels[idx].toggle_tab_pin(ti, &mut inv),
                            "lock" => st.panels[idx].toggle_tab_lock(ti, &mut inv),
                            _ => st.panels[idx].close_tab(ti, &mut inv),
                        }
                        flush_invalidations(hwnd, &mut inv);
                        update_title(hwnd, st, "");
                        update_status(hwnd, st);
                        return LRESULT(0);
                    }
                    // 탭 바 빈 공간 더블클릭 = 새 탭(원본 F20 — QA 07-14)
                    if st.panels[idx].tabbar.empty_area_at(x, y) {
                        let mut inv = Invalidations::default();
                        let ctx = st.nav_ctx();
                        st.panels[idx].new_tab(ctx, &mut inv);
                        flush_invalidations(hwnd, &mut inv);
                        update_title(hwnd, st, "");
                        return LRESULT(0);
                    }
                    let rows = st.panels[idx].rows();
                    let hit = (!rows.marker_hit(x, y))
                        .then(|| rows.row_at(x, y))
                        .flatten();
                    if let Some(row) = hit {
                        let mut inv = Invalidations::default();
                        let ctx = st.nav_ctx();
                        exec = st.panels[idx].activate_row(row, ctx, &mut inv);
                        flush_invalidations(hwnd, &mut inv);
                        update_title(hwnd, st, "");
                    }
                }
            }
            if let Some(file) = exec {
                shell_open(hwnd, &file); // 파일 실행(QA 07-14 — 기본 연결 프로그램)
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if let Some(st) = state_of(hwnd) {
                let vk = wparam.0 as u16;
                let shift = GetKeyState(VK_SHIFT.0 as i32) < 0;
                let ctrl = GetKeyState(VK_CONTROL.0 as i32) < 0;
                let ctx = st.nav_ctx();
                let mut inv = Invalidations::default();
                if st.active_panel().pathbar.is_editing() {
                    if vk == VK_RETURN.0 {
                        st.active_panel().pathbar.submit_edit(&mut inv);
                        st.active_panel().drain_actions(ctx, &mut inv);
                    } else if vk == VK_ESCAPE.0 {
                        // 팝업 열림 = 팝업만 닫기(원본·탐색기 규약), 아니면 편집 취소
                        if st.active_panel().pathbar.suggest_open() {
                            st.active_panel().pathbar.close_suggest(&mut inv);
                        } else {
                            st.active_panel().pathbar.cancel_edit(&mut inv);
                        }
                    } else if vk == VK_UP.0 || vk == VK_DOWN.0 {
                        // 자동완성 ↑/↓(PATH-SUG) — 선택 미리 채움·↑ 복원
                        let d = if vk == VK_UP.0 { -1 } else { 1 };
                        st.active_panel().pathbar.suggest_move(d, &mut inv);
                    } else if ctrl && vk == b'C' as u16 {
                        // 표준 편집 클립보드(QA 07-14) — 선택 복사/잘라내기/붙여넣기
                        if let Some(t) = st.active_panel().pathbar.edit_selected_text() {
                            crate::clipboard::write_text(hwnd, &t);
                        }
                    } else if ctrl && vk == b'X' as u16 {
                        if let Some(t) = st.active_panel().pathbar.edit_cut(&mut inv) {
                            crate::clipboard::write_text(hwnd, &t);
                        }
                        update_path_suggest(st, &mut inv); // 텍스트 변경 — 제안 갱신
                    } else if ctrl && vk == b'V' as u16 {
                        if let Some(t) = paste_line() {
                            st.active_panel().pathbar.edit_paste(&t, &mut inv);
                            update_path_suggest(st, &mut inv);
                        }
                    } else if let Some(k) = edit_key_of(vk, ctrl) {
                        // 캐럿 이동·선택·삭제(QA 07-13 — edit.rs 공용 모델)
                        st.active_panel().pathbar.edit_key(k, shift, &mut inv);
                        if k == nexa_gui::EditKey::DeleteForward {
                            update_path_suggest(st, &mut inv); // 텍스트 변경 시에만
                        }
                    }
                    flush_invalidations(hwnd, &mut inv);
                    update_title(hwnd, st, "");
                    return LRESULT(0);
                }
                // 도크 터미널 포커스(M4-3) — 비문자 키를 VT 시퀀스로 전달, 그 외는 WM_CHAR로
                // (리스트 단축키 차단). 종료 상태에서 아무 키 = 재시작.
                if let Some(ti) = st.term_focus {
                    if st.panels[ti].dock_visible() && st.panels[ti].dock.active_kind() == 2 {
                        if let Some(t) = &mut st.terms[ti] {
                            if t.exited {
                                st.terms[ti] = None; // 다음 페인트에서 재시작(원본 exit 재시작)
                                let _ = InvalidateRect(Some(hwnd), None, false);
                                return LRESULT(0);
                            }
                            if let Some(seq) = term_key_seq(vk) {
                                t.pty.write(seq);
                            }
                            return LRESULT(0); // 문자 입력은 WM_CHAR 경로로 수신
                        }
                    } else {
                        // 낡은 터미널 포커스(도크 숨김/종류 전환) 해제 — 강조 동기(QA 07-15)
                        st.term_focus = None;
                        let mut inv = Invalidations::default();
                        sync_focus_visuals(st, &mut inv);
                        flush_invalidations(hwnd, &mut inv);
                    }
                }
                if st.active_panel().rows().is_renaming() {
                    // 인라인 이름변경 중 — Enter=확정·Esc=취소·편집 키 라우팅(M3-2·QA 07-13)
                    if vk == VK_RETURN.0 {
                        if let Some((row, new_name)) =
                            st.active_panel().rows_mut().submit_rename(&mut inv)
                        {
                            flush_invalidations(hwnd, &mut inv);
                            apply_rename(hwnd, st, row, &new_name);
                            return LRESULT(0);
                        }
                    } else if vk == VK_ESCAPE.0 {
                        st.active_panel().rows_mut().cancel_rename(&mut inv);
                    } else if ctrl && vk == b'C' as u16 {
                        // 표준 편집 클립보드(QA 07-14 — 경로바와 동일 규약)
                        if let Some(t) = st.active_panel().rows().rename_selected_text() {
                            crate::clipboard::write_text(hwnd, &t);
                        }
                    } else if ctrl && vk == b'X' as u16 {
                        if let Some(t) = st.active_panel().rows_mut().rename_cut(&mut inv) {
                            crate::clipboard::write_text(hwnd, &t);
                        }
                    } else if ctrl && vk == b'V' as u16 {
                        if let Some(t) = paste_line() {
                            st.active_panel().rows_mut().rename_paste(&t, &mut inv);
                        }
                    } else if let Some(k) = edit_key_of(vk, ctrl) {
                        st.active_panel().rows_mut().rename_key(k, shift, &mut inv);
                    }
                    flush_invalidations(hwnd, &mut inv);
                    return LRESULT(0);
                }
                if vk == VK_ESCAPE.0 && st.transfer.is_some() {
                    // Esc = 진행 중 전송 취소(원본 CancellationToken 대응)
                    if let Some(j) = &st.transfer {
                        j.shared.cancel.store(true, Ordering::Relaxed);
                    }
                    return LRESULT(0);
                } else if vk == VK_ESCAPE.0 && st.menubar.is_open() {
                    st.menubar.close(&mut inv);
                } else if vk == VK_F5.0 {
                    run_command(hwnd, st, CMD_REFRESH);
                    return LRESULT(0);
                } else if vk == VK_F6.0 {
                    // F6 = 테마 순환(다크→라이트→시스템)
                    let next = match st.theme_mode {
                        ThemeMode::Dark => CMD_THEME_LIGHT,
                        ThemeMode::Light => CMD_THEME_SYSTEM,
                        ThemeMode::System => CMD_THEME_DARK,
                    };
                    run_command(hwnd, st, next);
                    return LRESULT(0);
                } else if vk == VK_F3.0 {
                    bench(hwnd, st);
                } else if vk == VK_TAB.0 {
                    if ctrl {
                        st.active_panel().next_tab(&mut inv); // Ctrl+Tab = 다음 탭
                    } else {
                        let next = 1 - st.active;
                        set_active(hwnd, st, next); // Tab = 패널 전환(커맨더 규약)
                    }
                } else if vk == b'T' as u16 && ctrl {
                    st.active_panel().new_tab(ctx, &mut inv); // Ctrl+T = 새 탭
                } else if vk == b'W' as u16 && ctrl {
                    let i = st.active_panel().active_index();
                    st.active_panel().close_tab(i, &mut inv); // Ctrl+W = 탭 닫기
                } else if vk == b'A' as u16 && ctrl {
                    st.active_panel().on_event(&InputEvent::SelectAll, &mut inv);
                } else if vk == b'C' as u16 && ctrl {
                    // 도크 Info/Preview 텍스트 선택 우선(QA 07-15) — 없으면 파일 복사
                    // (M3-5 — CF_HDROP 단일 출처. 선택 없으면 클립보드 유지)
                    if let Some(t) = st
                        .panels
                        .iter()
                        .find_map(|p| p.dock_visible().then(|| p.dock.selected_text()).flatten())
                    {
                        crate::clipboard::write_text(hwnd, &t);
                    } else if let Some((paths, op)) = clip_from_selection(st, nexa_ops::Op::Copy) {
                        crate::clipboard::write_file_list(hwnd, &paths, op);
                    }
                } else if vk == b'X' as u16 && ctrl {
                    if let Some((paths, op)) = clip_from_selection(st, nexa_ops::Op::Move) {
                        crate::clipboard::write_file_list(hwnd, &paths, op);
                    }
                } else if vk == b'V' as u16 && ctrl {
                    if let Some((paths, op)) = crate::clipboard::read_file_list() {
                        if op == nexa_ops::Op::Move {
                            crate::clipboard::clear(hwnd); // 잘라내기는 1회성(탐색기 관례)
                        }
                        let dest = st.active_panel().root_path();
                        start_transfer(hwnd, st, paths, dest, op);
                        return LRESULT(0);
                    }
                } else if vk == b'Z' as u16 && ctrl {
                    do_undo_redo(hwnd, st, shift); // Ctrl+Z=실행 취소·Ctrl+Shift+Z=다시 실행(M3-3)
                    return LRESULT(0);
                } else if vk == b'Y' as u16 && ctrl {
                    do_undo_redo(hwnd, st, true); // Ctrl+Y=다시 실행(탐색기 관례)
                    return LRESULT(0);
                } else if vk == VK_RETURN.0 {
                    if let Some(c) = st.active_panel().rows().caret() {
                        if let Some(file) = st.active_panel().activate_row(c, ctx, &mut inv) {
                            shell_open(hwnd, &file); // Enter = 파일 실행(QA 07-14)
                        }
                    }
                } else if vk == VK_F2.0 {
                    begin_rename_caret(hwnd, st); // F2 = 인라인 이름변경(M3-2)
                    return LRESULT(0);
                } else if vk == VK_DELETE.0 {
                    do_delete(hwnd, st, shift); // Del=휴지통·Shift+Del=완전(M3-2)
                    return LRESULT(0);
                } else if vk == VK_APPS.0 {
                    show_row_context_menu(hwnd, true); // Apps 키 = 캐럿 행 셸 메뉴(M3-4)
                    return LRESULT(0);
                } else if vk == b'N' as u16 && ctrl && shift {
                    run_command(hwnd, st, CMD_NEW_FOLDER); // Ctrl+Shift+N = 새 폴더(탐색기 관례)
                    return LRESULT(0);
                } else if vk == b'R' as u16 && ctrl && shift {
                    run_command(hwnd, st, CMD_BULK_RENAME); // Ctrl+Shift+R = 일괄 이름변경(M5-1)
                    return LRESULT(0);
                } else if vk == b'H' as u16 && ctrl {
                    run_command(hwnd, st, CMD_TOGGLE_HIDDEN); // 메뉴 체크와 동기
                    return LRESULT(0);
                } else if vk == VK_OEM_PERIOD.0 && ctrl {
                    run_command(hwnd, st, CMD_TOGGLE_DOTFILES);
                    return LRESULT(0);
                } else if vk == VK_OEM_COMMA.0 && ctrl {
                    open_prefs(hwnd); // Ctrl+, = 설정 창(S6 — 원본 docs/40)
                    return LRESULT(0);
                } else if vk == VK_OEM_3.0 && ctrl {
                    run_command(hwnd, st, CMD_TOGGLE_DOCK); // Ctrl+` = 하단 도크(M4-1, 원본)
                    return LRESULT(0);
                } else if let Some(key) = vk_to_key(vk) {
                    st.active_panel()
                        .on_event(&InputEvent::Key { key, shift, ctrl }, &mut inv);
                }
                flush_invalidations(hwnd, &mut inv);
                update_title(hwnd, st, "");
                update_status(hwnd, st);
            }
            LRESULT(0)
        }
        WM_SYSKEYDOWN => {
            let vk = wparam.0 as u16;
            if let Some(st) = state_of(hwnd) {
                let ctx = st.nav_ctx();
                let mut inv = Invalidations::default();
                let handled = if vk == VK_F10.0 && GetKeyState(VK_SHIFT.0 as i32) < 0 {
                    show_row_context_menu(hwnd, true); // Shift+F10 = 캐럿 행 셸 메뉴(M3-4)
                    true
                } else if vk == VK_LEFT.0 {
                    st.active_panel().nav_back(ctx, &mut inv);
                    true
                } else if vk == VK_RIGHT.0 {
                    st.active_panel().nav_forward(ctx, &mut inv);
                    true
                } else if vk == VK_UP.0 {
                    st.active_panel().nav_up(ctx, &mut inv);
                    true
                } else if vk == VK_DOWN.0 {
                    // Alt+↓ = 캐럿 행 활성화(더블클릭 동등 — 원본 F19, QA 07-14)
                    if let Some(c) = st.active_panel().rows().caret() {
                        if let Some(file) = st.active_panel().activate_row(c, ctx, &mut inv) {
                            shell_open(hwnd, &file);
                        }
                    }
                    true
                } else {
                    false
                };
                if handled {
                    flush_invalidations(hwnd, &mut inv);
                    update_title(hwnd, st, "");
                    return LRESULT(0);
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        // 폴더 변경 통지(M3-6 watcher) — 세대 가드 후 디바운스 타이머 무장(300ms 코얼레싱)
        m if m == WM_APP_FSCHANGE => {
            if let Some(st) = state_of(hwnd) {
                let panel = wparam.0.min(1);
                let gen = lparam.0 as u64;
                if st.watchers[panel].as_ref().is_some_and(|w| w.gen == gen) {
                    SetTimer(
                        Some(hwnd),
                        TIMER_WATCH_BASE + panel,
                        WATCH_DEBOUNCE_MS,
                        None,
                    );
                }
            }
            LRESULT(0)
        }
        // 터미널 출력/종료 통지(M4-3) — 세대 가드 후 feed·도크 무효화
        m if m == WM_APP_TERM => {
            if let Some(st) = state_of(hwnd) {
                let exit = wparam.0 & crate::conpty::EXIT_FLAG != 0;
                let panel = (wparam.0 & !crate::conpty::EXIT_FLAG).min(1);
                let gen = lparam.0 as u64;
                if let Some(t) = &mut st.terms[panel] {
                    if t.pty.gen == gen {
                        if exit {
                            t.exited = true;
                        } else {
                            let sb0 = t.screen.scrollback_count();
                            let data = std::mem::take(&mut *t.pty.output.lock().unwrap());
                            t.screen.feed(&data);
                            if t.view_off > 0 {
                                // 스크롤백 보기 중 새 출력 = 보던 위치 고정(WT 규약, QA 07-14)
                                let grew = t.screen.scrollback_count().saturating_sub(sb0);
                                t.view_off = (t.view_off + grew).min(t.screen.scrollback_count());
                            }
                        }
                        let r = st.panels[panel].dock.bounds();
                        let rc = RECT {
                            left: r.x,
                            top: r.y,
                            right: r.right(),
                            bottom: r.bottom(),
                        };
                        let _ = InvalidateRect(Some(hwnd), Some(&rc), false);
                    }
                }
            }
            LRESULT(0)
        }
        m if m == WM_APP_PREFS => {
            open_prefs(hwnd); // 설정 창(S6) — State 차용 없는 시점에서 모달
            LRESULT(0)
        }
        m if m == WM_APP_BULK => {
            open_bulk_rename(hwnd); // 일괄 이름변경(M5-1) — 동일 재진입 규약
            LRESULT(0)
        }
        m if m == WM_APP_CTLDEMO => {
            // ctl 갤러리(개발 검증 전용 — 07-17 GroupCard)
            if let Some(st) = state_of(hwnd) {
                let _ = crate::ctldemo::show(hwnd, &st.dlg_font.clone());
            }
            LRESULT(0)
        }
        // 설정 창 즉시 적용 통지(X-8 — VS Code식): lparam 포인터는 SendMessage 동안만
        // 유효(같은 스레드 동기 호출)이므로 즉시 복사 후 적용.
        m if m == crate::prefs::WM_APP_PREFS_APPLY => {
            if let Some(v) = (lparam.0 as *const crate::prefs::PrefValues).as_ref() {
                let v = v.clone();
                apply_prefs(hwnd, &v);
            }
            LRESULT(0)
        }
        // 파일별 아이콘 워커 결과(QA 07-14) — WPARAM=Box<LoadResult> 소유권 인수(항상 해제)
        m if m == WM_APP_ICON => {
            let (key, raw) =
                *unsafe { Box::from_raw(wparam.0 as *mut crate::icons::shell::LoadResult) };
            if let Some(st) = state_of(hwnd) {
                if st.icons.borrow_mut().on_result(key, raw) {
                    // 파일 목록 본문만 무효화(X-16 — TIMER_ICONS와 동일 근거)
                    let mut inv = Invalidations::default();
                    inv.push(st.panels[0].rows().bounds());
                    inv.push(st.panels[1].rows().bounds());
                    inv.push(st.launcherbar.bounds()); // 런처 버튼도 셸 아이콘 사용(M5-1)
                    flush_invalidations(hwnd, &mut inv);
                }
            } else if raw != 0 {
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::DestroyIcon(
                        windows::Win32::UI::WindowsAndMessaging::HICON(
                            raw as *mut core::ffi::c_void,
                        ),
                    );
                }
            }
            LRESULT(0)
        }
        // 전송 워커 통지(M3-1) — 진행률·완료(양쪽 재로드)
        m if m == WM_APP_TRANSFER => {
            if let Some(st) = state_of(hwnd) {
                on_transfer_message(hwnd, st, wparam.0 as u64, lparam.0 == 1);
            }
            LRESULT(0)
        }
        // UIA SelectionItem 선택 요청(M5-3) — 프로바이더(임의 스레드)가 전역 행 인덱스로
        // 전달. 스냅샷 이후 목록이 바뀌었을 수 있으므로 범위 검사(select_program)로 방어.
        m if m == crate::uia::WM_APP_UIA_SELECT => {
            if let Some(st) = state_of(hwnd) {
                let row = wparam.0;
                let mut inv = Invalidations::default();
                let rows = st.panels[st.active].rows_mut();
                let selected = rows.source().is_selected(row);
                use nexa_gui::widgets::SelectOp;
                match lparam.0 {
                    crate::uia::SEL_SINGLE => rows.select_program(row, SelectOp::Single, &mut inv),
                    crate::uia::SEL_ADD if !selected => {
                        rows.select_program(row, SelectOp::Toggle, &mut inv)
                    }
                    crate::uia::SEL_REMOVE if selected => {
                        rows.select_program(row, SelectOp::Toggle, &mut inv)
                    }
                    _ => {}
                }
                flush_invalidations(hwnd, &mut inv);
                update_status(hwnd, st);
            }
            LRESULT(0)
        }
        // UIA 루트 요청(스크린리더 등) — 활성 패널 가시 행 스냅샷 프로바이더 반환(M2-7)
        m if m == WM_GETOBJECT => {
            if lparam.0 as i32 == crate::uia::UIA_ROOT_OBJECT_ID {
                if let Some(st) = state_of(hwnd) {
                    let snap = uia_snapshot(hwnd, st);
                    return crate::uia::return_provider(hwnd, wparam, lparam, snap);
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        // IME 조합 시작/갱신 — 조합 창을 편집 캐럿 옆으로(M2-7). 나머지는 기본 처리
        // (DefWindowProc가 결과 문자열을 WM_IME_CHAR→WM_CHAR로 변환해 기존 경로 수신).
        m if m == WM_IME_STARTCOMPOSITION || m == WM_IME_COMPOSITION => {
            if let Some(st) = state_of(hwnd) {
                position_ime(hwnd, st);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_CHAR => {
            if let Some(st) = state_of(hwnd) {
                if let Some(c) = char::from_u32(wparam.0 as u32) {
                    // 도크 터미널 포커스(M4-3) — 문자·제어문자(\r 등) 전부 셸 stdin으로
                    // (ConPTY가 해석). Enter/Backspace도 WM_CHAR로 도착.
                    // Backspace=DEL(0x7F, 1글자). 0x08은 ConPTY가 Ctrl+Backspace(단어
                    // 삭제)로 해석하므로 교차 매핑(원본 TerminalView.OnKeyDown 규약).
                    // Ctrl+C(0x03)=선택 있으면 복사·없으면 인터럽트, Ctrl+V(0x16)=붙여넣기
                    // (Windows Terminal 규약, QA 07-14). 입력 시 스크롤백 보기 해제.
                    if let Some(ti) = st.term_focus {
                        if st.panels[ti].dock_visible() && st.panels[ti].dock.active_kind() == 2 {
                            let mut handled = false;
                            if let Some(t) = &mut st.terms[ti] {
                                if !t.exited {
                                    handled = true;
                                    match c {
                                        '\u{3}' if t.sel_norm().is_some() => {
                                            let ((sl, sc), (el, ec)) = t.sel_norm().unwrap();
                                            let text = t.screen.get_text(sl, sc, el, ec);
                                            crate::clipboard::write_text(hwnd, &text);
                                            t.sel = None;
                                        }
                                        '\u{16}' => {
                                            if let Some(txt) = crate::clipboard::read_text() {
                                                t.pty.write(
                                                    &txt.replace("\r\n", "\r").replace('\n', "\r"),
                                                );
                                            }
                                            t.view_off = 0;
                                        }
                                        c => {
                                            let mut buf = [0u8; 4];
                                            t.pty.write(match c {
                                                '\u{8}' => "\x7f",  // Backspace → 1글자
                                                '\u{7f}' => "\x08", // Ctrl+BS → 단어 삭제
                                                c => c.encode_utf8(&mut buf),
                                            });
                                            t.view_off = 0;
                                            t.sel = None;
                                        }
                                    }
                                }
                            }
                            if handled {
                                st.term_caret_on = true; // 입력 중엔 캐럿 상시 표시(위상 리셋)
                                invalidate_dock(hwnd, st, ti);
                                return LRESULT(0);
                            }
                        }
                    }
                    let mut inv = Invalidations::default();
                    if st.active_panel().pathbar.is_editing() {
                        if c == '\u{8}' || !c.is_control() {
                            st.active_panel().pathbar.edit_char(c, &mut inv);
                            update_path_suggest(st, &mut inv); // 자동완성 갱신(PATH-SUG)
                        }
                    } else if GetKeyState(VK_CONTROL.0 as i32) >= 0
                        && (c == '\u{8}'
                            || (!c.is_control()
                                // 스페이스 = 선택 토글 키. 단 이름변경 중이거나
                                // **타입어헤드 접두사 입력 중**(공백 포함 옵션 — QA 07-15)엔 버퍼로
                                && (c != ' '
                                    || st.active_panel().rows().is_renaming()
                                    || !st.active_panel().rows().typeahead_text().is_empty())))
                    {
                        st.active_panel().on_event(
                            &InputEvent::Char {
                                c,
                                now_ms: now_ms(),
                            },
                            &mut inv,
                        );
                        if !st.active_panel().rows().typeahead_text().is_empty() {
                            SetTimer(Some(hwnd), TIMER_TYPEAHEAD, TIMER_TICK_MS, None);
                        }
                    }
                    flush_invalidations(hwnd, &mut inv);
                    update_title(hwnd, st, "");
                }
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == TIMER_SESSION_SAVE {
                // 디바운스 만료 — 마지막 세션 상태 1회 flush(원자적 쓰기·실패 무해)
                let _ = KillTimer(Some(hwnd), TIMER_SESSION_SAVE);
                if let Some(st) = state_of(hwnd) {
                    let session = current_session(st);
                    let _ = config::save(&config::data_dir(), SESSION_FILE, &session.serialize());
                }
                return LRESULT(0);
            }
            if wparam.0 == TIMER_PROG_CLOSE {
                // 전송 완료 2초 경과 — 진행 창 닫기(drop = DestroyWindow)
                let _ = KillTimer(Some(hwnd), TIMER_PROG_CLOSE);
                if let Some(st) = state_of(hwnd) {
                    st.transfer_close = None;
                }
                return LRESULT(0);
            }
            if wparam.0 >= TIMER_WATCH_BASE && wparam.0 < TIMER_WATCH_BASE + 2 {
                // watcher 디바운스 만료(M3-6) — 무간섭 재로드(펼침·선택·캐럿·스크롤 보존).
                // 편집/전송 중엔 미루고 재무장(재로드가 편집 행 인덱스를 흔들지 않게)
                let panel = wparam.0 - TIMER_WATCH_BASE;
                let _ = KillTimer(Some(hwnd), wparam.0);
                if let Some(st) = state_of(hwnd) {
                    if st.panels[panel].rows().is_renaming()
                        || st.panels[panel].pathbar.is_editing()
                        || st.transfer.is_some()
                    {
                        SetTimer(Some(hwnd), wparam.0, WATCH_DEBOUNCE_MS, None);
                    } else {
                        let ctx = st.nav_ctx();
                        let mut inv = Invalidations::default();
                        st.panels[panel].reopen_filtered(ctx, &mut inv);
                        flush_invalidations(hwnd, &mut inv);
                        update_title(hwnd, st, "");
                        update_status(hwnd, st);
                    }
                }
                return LRESULT(0);
            }
            if wparam.0 == TIMER_TYPEAHEAD {
                if let Some(st) = state_of(hwnd) {
                    let mut inv = Invalidations::default();
                    let now = now_ms();
                    st.panels[0].rows_mut().tick(now, &mut inv);
                    st.panels[1].rows_mut().tick(now, &mut inv);
                    flush_invalidations(hwnd, &mut inv);
                    if st.panels[0].rows().typeahead_text().is_empty()
                        && st.panels[1].rows().typeahead_text().is_empty()
                    {
                        let _ = KillTimer(Some(hwnd), TIMER_TYPEAHEAD);
                    }
                }
            } else if wparam.0 == TIMER_ICONS {
                if let Some(st) = state_of(hwnd) {
                    if st.icons.borrow_mut().tick(hwnd, WM_APP_ICON) {
                        // 아이콘은 파일 목록 본문에만 그려짐 — 전체 창 대신 두 패널의
                        // 목록 rect만 무효화(X-16: 로딩 중 80ms 틱마다 전창 재도장 방지)
                        let mut inv = Invalidations::default();
                        inv.push(st.panels[0].rows().bounds());
                        inv.push(st.panels[1].rows().bounds());
                        inv.push(st.launcherbar.bounds()); // 런처 버튼도 셸 아이콘 사용
                        flush_invalidations(hwnd, &mut inv);
                    }
                    if !st.icons.borrow().has_pending() {
                        let _ = KillTimer(Some(hwnd), TIMER_ICONS);
                    }
                }
            } else if wparam.0 == TIMER_TERM_CARET {
                // 터미널 캐럿 깜빡임(QA 07-14) — 포커스 해제 시 자연 종료(표시 위상 복원)
                if let Some(st) = state_of(hwnd) {
                    match st.term_focus {
                        Some(ti) => {
                            st.term_caret_on = !st.term_caret_on;
                            invalidate_dock(hwnd, st, ti);
                        }
                        None => {
                            st.term_caret_on = true;
                            let _ = KillTimer(Some(hwnd), TIMER_TERM_CARET);
                        }
                    }
                }
            } else if wparam.0 == TIMER_TERM_SEL {
                // 터미널 선택 엣지 자동 스크롤 반복(QA 07-14) — 커서 위치 기준
                if let Some(st) = state_of(hwnd) {
                    let ti = st.term_drag;
                    match ti {
                        Some(ti) => {
                            let mut pt = windows::Win32::Foundation::POINT::default();
                            let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt);
                            let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut pt);
                            if let Some(t) = &mut st.terms[ti] {
                                term_drag_extend(t, pt.x, pt.y);
                            }
                            invalidate_dock(hwnd, st, ti);
                        }
                        None => {
                            let _ = KillTimer(Some(hwnd), TIMER_TERM_SEL);
                        }
                    }
                }
            } else if wparam.0 == TIMER_RENAME {
                // 더블클릭 시간 경과 — 느린 재클릭 확정 = 리네임 진입(QA 07-14)
                let _ = KillTimer(Some(hwnd), TIMER_RENAME);
                if let Some(st) = state_of(hwnd) {
                    begin_rename_caret(hwnd, st);
                }
            } else if wparam.0 == TIMER_JANITOR {
                if let Some(st) = state_of(hwnd) {
                    if should_trim(now_ms(), st.last_activity_ms, st.trimmed) {
                        trim_resident(st);
                        st.trimmed = true;
                        // 유휴 동안 백그라운드 0% — 다음 입력(note_activity)이 재가동
                        let _ = KillTimer(Some(hwnd), TIMER_JANITOR);
                    }
                }
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            if let Some(st) = state_of(hwnd) {
                let dpi = (wparam.0 & 0xFFFF) as u32;
                st.dpi = dpi;
                if let Some(dw) = &mut st.dw {
                    let _ = dw.set_dpi(dpi);
                }
                let mut inv = Invalidations::default();
                let m = panel_metrics(dpi);
                let cols = columns(dpi);
                st.panels[0].set_metrics(m, cols.clone(), &mut inv);
                st.panels[1].set_metrics(m, cols, &mut inv);
                st.menubar.set_metrics(m.row_h, m.pad_x, &mut inv);
                st.toolbar.set_metrics(m.row_h, m.pad_x, &mut inv);
                st.launcherbar.set_metrics(m.row_h, m.pad_x, &mut inv);
                st.statusbar.set_metrics(m.row_h, m.pad_x, &mut inv);
                let rc = &*(lparam.0 as *const RECT);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    rc.left,
                    rc.top,
                    rc.right - rc.left,
                    rc.bottom - rc.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                layout(hwnd, st, &mut inv);
                flush_invalidations(hwnd, &mut inv);
            }
            LRESULT(0)
        }
        // OS 테마 변경(ImmersiveColorSet 등) — 시스템 모드일 때 재해석(docs/39 §3)
        WM_SETTINGCHANGE => {
            if let Some(st) = state_of(hwnd) {
                if st.theme_mode == ThemeMode::System {
                    let mut inv = Invalidations::default();
                    apply_theme(hwnd, st, &mut inv);
                    flush_invalidations(hwnd, &mut inv);
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_DESTROY => {
            // 종료 저장(M2-5 — data\ 원자적 쓰기 + 탭 변경 시 디바운스 자동 저장 07-15)
            if let Some(st) = state_of(hwnd) {
                let settings = current_settings(st);
                let session = current_session(st);
                let dir = config::data_dir();
                match config::save(&dir, SETTINGS_FILE, &settings.serialize())
                    .and_then(|_| config::save(&dir, SESSION_FILE, &session.serialize()))
                {
                    Ok(()) => config::purge_legacy(&dir), // 구 .txt 정리(마이그레이션 완료)
                    Err(e) => eprintln!("설정/세션 저장 실패: {e}"),
                }
            }
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_NCDESTROY => {
            let _ = windows::Win32::System::Ole::RevokeDragDrop(hwnd); // DnD 수신 해제(M3-5)
            let ptr = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) as *mut State;
            if !ptr.is_null() {
                drop(Box::from_raw(ptr));
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

#[cfg(test)]
mod tests {
    use super::should_trim;

    #[test]
    fn idle_trim_threshold_and_once() {
        assert!(!should_trim(59_999, 0, false), "임계 미달");
        assert!(should_trim(60_000, 0, false), "60s 유휴 = 트림");
        assert!(!should_trim(120_000, 0, true), "이미 트림 — 재실행 없음");
        assert!(!should_trim(70_000, 30_000, false), "활동 후 40s — 미달");
        assert!(
            !should_trim(10, 20, false),
            "시계 역전은 포화 감산으로 안전"
        );
    }
}
