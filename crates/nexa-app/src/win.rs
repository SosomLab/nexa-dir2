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
    GetKeyState, ReleaseCapture, SetCapture, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE,
    VK_F2, VK_F3, VK_F5, VK_F6, VK_HOME, VK_LEFT, VK_NEXT, VK_OEM_PERIOD, VK_PRIOR, VK_RETURN,
    VK_RIGHT, VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetMessageW,
    GetWindowLongPtrW, KillTimer, LoadCursorW, PostMessageW, PostQuitMessage, RegisterClassW,
    SetTimer, SetWindowLongPtrW, SetWindowPos, SetWindowTextW, TranslateMessage, CREATESTRUCTW,
    CS_DBLCLKS, CW_USEDEFAULT, GWLP_USERDATA, IDC_ARROW, MSG, SWP_NOACTIVATE, SWP_NOZORDER,
    WM_CHAR, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_GETOBJECT, WM_IME_COMPOSITION,
    WM_IME_STARTCOMPOSITION, WM_KEYDOWN, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCREATE, WM_NCDESTROY, WM_PAINT,
    WM_RBUTTONDOWN, WM_SETTINGCHANGE, WM_SIZE, WM_SYSKEYDOWN, WM_TIMER, WM_XBUTTONDOWN, WNDCLASSW,
    WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

use crate::config::{self, PanelSession, Session, Settings, SESSION_FILE, SETTINGS_FILE};
use crate::dw::{DwBackend, DwCtx};
use crate::i18n::{self, tr, trf};
use crate::icons::shell::ShellIcons;
use crate::panel::{NavCtx, Panel, PanelMetrics};
use crate::source::{COL_EXT, COL_KIND, COL_MODIFIED, COL_NAME, COL_SIZE};

/// wParam 마우스 수식키 비트(winuser.h MK_SHIFT/MK_CONTROL).
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
const JANITOR_TICK_MS: u32 = 10_000;
const IDLE_TRIM_MS: u64 = 60_000;
/// 패널 최소 폭(논리 px)·스플리터 히트 존 반폭.
const MIN_PANEL: i32 = 200;
const SPLIT_HALF: i32 = 3;

/// 명령 id(메뉴·도구 모음 공용 — docs/20 §2).
const CMD_NEW_TAB: u32 = 1;
const CMD_CLOSE_TAB: u32 = 2;
const CMD_EXIT: u32 = 3;
const CMD_NEW_FOLDER: u32 = 4;
const CMD_NEW_FILE: u32 = 5;
const CMD_TOGGLE_HIDDEN: u32 = 10;
const CMD_TOGGLE_DOTFILES: u32 = 11;
const CMD_REFRESH: u32 = 12;
const CMD_NAV_BACK: u32 = 20;
const CMD_NAV_FORWARD: u32 = 21;
const CMD_NAV_UP: u32 = 22;
const CMD_THEME_SYSTEM: u32 = 30;
const CMD_THEME_LIGHT: u32 = 31;
const CMD_THEME_DARK: u32 = 32;
/// 언어 라디오 — 40 = 시스템, 41+idx = State.langs[idx](발견 목록 순).
const CMD_LANG_SYSTEM: u32 = 40;
const CMD_LANG_BASE: u32 = 41;

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
fn build_menus(
    show_hidden: bool,
    show_dotfiles: bool,
    mode: ThemeMode,
    lang_setting: &str,
    langs: &[(String, String)],
) -> Vec<Menu> {
    let mut view_items = vec![
        MenuItem::new(CMD_TOGGLE_HIDDEN, tr("menu.view.hidden"), "Ctrl+H").checked(show_hidden),
        MenuItem::new(CMD_TOGGLE_DOTFILES, tr("menu.view.dot"), "Ctrl+.").checked(show_dotfiles),
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
                MenuItem::new(CMD_EXIT, tr("menu.file.exit"), ""),
            ],
        },
        Menu {
            title: tr("menu.view"),
            items: view_items,
        },
    ]
}

/// 도구 모음 버튼(네비 ←→↑⟳ — docs/20 §2).
fn build_toolbar() -> Vec<ToolButton> {
    [
        (CMD_NAV_BACK, "←"),
        (CMD_NAV_FORWARD, "→"),
        (CMD_NAV_UP, "↑"),
        (CMD_REFRESH, "⟳"),
    ]
    .into_iter()
    .map(|(id, g)| ToolButton {
        id,
        glyph: g.into(),
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
    /// 언어 설정값("system"|코드)·발견 목록(메뉴 라디오 CMD_LANG_BASE+idx 매핑) — M2-6.
    lang_setting: String,
    langs: Vec<(String, String)>,
    /// 상주 자니터(M2-8): 마지막 입력 활동 시각(now_ms 기준)·트림 완료 플래그.
    last_activity_ms: u64,
    trimmed: bool,
    /// UIA 포커스 이벤트 중복 억제(M2-7): 마지막 통지한 (활성 패널, 캐럿).
    uia_caret: Option<(usize, Option<usize>)>,
    /// 내부 클립보드(원본 FileClipboard — 경로+모드). OS 클립보드 상호운용은 M3-5.
    clipboard: Option<(Vec<PathBuf>, nexa_ops::Op)>,
    /// 진행 중 전송 잡(M3-1) — 동시 1개(α). 세대 번호로 낡은 워커 통지 무시.
    transfer: Option<TransferJob>,
    transfer_gen: u64,
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
    // 설정/세션 로드(data\ — 없으면 기본값. M2-5)
    let data = config::data_dir();
    let settings = config::load(&data, SETTINGS_FILE)
        .map(|t| Settings::parse(&t))
        .unwrap_or_default();
    let session = config::load(&data, SESSION_FILE)
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
                &fallback,
                ctx,
                m,
                columns(96),
            ),
            Panel::restore(
                &session.panels[1].tabs,
                session.panels[1].active,
                &fallback,
                ctx,
                m,
                columns(96),
            ),
            session.active_panel,
        )
    };
    let state = Box::new(State {
        menubar: MenuBar::new(
            build_menus(
                settings.show_hidden,
                settings.show_dotfiles,
                theme_mode,
                &settings.lang,
                &langs,
            ),
            m.row_h,
            m.pad_x,
        ),
        toolbar: Toolbar::new(build_toolbar(), m.row_h, m.pad_x),
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
        lang_setting: settings.lang,
        langs,
        last_activity_ms: 0,
        trimmed: false,
        uia_caret: None,
        clipboard: None,
        transfer: None,
        transfer_gen: 0,
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
            ..Default::default()
        };
        let atom = RegisterClassW(&wc);
        debug_assert_ne!(atom, 0, "RegisterClassW 실패");

        CreateWindowExW(
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

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
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

/// 전체 레이아웃(docs/20 §1): 메뉴 / 도구 모음 / [좌 ║ 우 패널] / 상태바.
unsafe fn layout(hwnd: HWND, st: &mut State, inv: &mut Invalidations) {
    let rc = client_rect(hwnd);
    let s = |v: i32| (v * st.dpi as i32) / 96;
    let (menu_h, tool_h, status_h) = (s(22), s(24), s(22));
    st.menubar
        .set_bounds(GRect::new(0, 0, rc.w, menu_h.min(rc.h)), inv);
    st.toolbar
        .set_bounds(GRect::new(0, menu_h, rc.w, tool_h), inv);
    let top = menu_h + tool_h;
    let bottom = (rc.h - status_h).max(top);
    st.statusbar
        .set_bounds(GRect::new(0, bottom, rc.w, rc.h - bottom), inv);

    let sx = splitter_x(hwnd, st);
    let half = s(SPLIT_HALF).max(1);
    let ph = (bottom - top).max(0);
    st.panels[0].set_bounds(GRect::new(0, top, (sx - half).max(0), ph), inv);
    st.panels[1].set_bounds(
        GRect::new(sx + half, top, (rc.w - sx - half).max(0), ph),
        inv,
    );
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
    let text = format!(
        "Nexa Dir 2 — [{side}] {} [{}{} · {} · {}{}]{}\0",
        p.root_path().display(),
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
    match &mut st.dw {
        None => match DwBackend::new(hdc, w, h, st.dpi) {
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

/// 캐럿 변경 시 UIA 포커스 이벤트 발행(M2-7) — 클라이언트가 붙어 있을 때만(가드 비용 0).
unsafe fn uia_notify(hwnd: HWND, st: &mut State) {
    let cur = (st.active, st.panels[st.active].rows().caret());
    if st.uia_caret == Some(cur) {
        return;
    }
    st.uia_caret = Some(cur);
    if cur.1.is_none() || !crate::uia::listening() {
        return;
    }
    crate::uia::raise_focus(hwnd, uia_snapshot(hwnd, st));
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
    let note = match nexa_ops::rename(&path, new_name) {
        Ok(_) => String::new(),
        Err(e) => format!(" · {}", trf("rename.fail", &[&e.to_string()])),
    };
    reload_both(hwnd, st, &note);
}

/// 새로 만들기(M3-2, 원본 BG-N1/N2) — 생성 → 재로드 → 그 행 즉시 인라인 이름변경(RevealAndRename).
unsafe fn create_new(hwnd: HWND, st: &mut State, folder: bool) {
    let dir = st.active_panel().root_path().to_path_buf();
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
    std::thread::spawn(move || {
        let hwnd = HWND(hwnd_raw as *mut core::ffi::c_void);
        let out = nexa_ops::transfer(
            &sources,
            &dest,
            op,
            &mut |_| nexa_ops::Conflict::Skip, // α: 충돌 = 건너뜀
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
    st.transfer = Some(TransferJob { shared, gen });
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
        update_title(
            hwnd,
            st,
            &format!(" · {}", trf("ops.progress", &[&pct.to_string()])),
        );
        return;
    }
    let job = st.transfer.take().unwrap();
    let out = job
        .shared
        .outcome
        .lock()
        .unwrap()
        .take()
        .unwrap_or_default();
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
/// 대상 = 편집 중인 경로바(활성 패널 우선). 없으면 IME 기본 위치에 둔다.
/// 결과 문자열은 기존 WM_CHAR 경로로 수신(DefWindowProc의 WM_IME_CHAR 변환).
unsafe fn position_ime(hwnd: HWND, st: &mut State) {
    use windows::Win32::UI::Input::Ime::{
        ImmGetContext, ImmReleaseContext, ImmSetCompositionWindow, CFS_POINT, COMPOSITIONFORM,
    };
    let Some((buf, field, pad)) = [st.active, 1 - st.active].into_iter().find_map(|i| {
        st.panels[i]
            .pathbar
            .edit_info()
            .map(|(b, r, p)| (b.to_string(), r, p))
    }) else {
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
        st.panels[1].paint(&mut ctx, &st.theme);
        // 스플리터(패널 영역 한정·드래그 중 accent)
        use nexa_gui::DrawCtx;
        let lx = st.panels[0].bounds().right();
        let pb = st.panels[0].bounds();
        let w = st.panels[1].bounds().x - lx;
        let color = if st.split_drag {
            st.theme.accent
        } else {
            st.theme.border
        };
        ctx.fill_rect(GRect::new(lx, pb.y, w, pb.h), color);
        st.toolbar.paint(&mut ctx, &st.theme);
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
    uia_notify(hwnd, st); // 캐럿 변경 시 스크린리더 통지(M2-7)
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
            let ctx = st.nav_ctx();
            st.active_panel().reopen_filtered(ctx, &mut inv);
        }
        CMD_TOGGLE_DOTFILES => {
            st.show_dotfiles = !st.show_dotfiles;
            let on = st.show_dotfiles;
            st.menubar.set_checked(CMD_TOGGLE_DOTFILES, on, &mut inv);
            let ctx = st.nav_ctx();
            st.active_panel().reopen_filtered(ctx, &mut inv);
        }
        CMD_NEW_FOLDER | CMD_NEW_FILE => {
            create_new(hwnd, st, id == CMD_NEW_FOLDER);
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
        }
        CMD_LANG_SYSTEM => {
            st.lang_setting = "system".into();
            apply_lang(hwnd, st, &mut inv);
        }
        i if i >= CMD_LANG_BASE && ((i - CMD_LANG_BASE) as usize) < st.langs.len() => {
            st.lang_setting = st.langs[(i - CMD_LANG_BASE) as usize].0.clone();
            apply_lang(hwnd, st, &mut inv);
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
            st.theme_mode,
            &st.lang_setting,
            &st.langs,
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
    if st.active != idx {
        st.active = idx;
        let mut inv = Invalidations::default();
        st.panels[0].set_focused(idx == 0, &mut inv);
        st.panels[1].set_focused(idx == 1, &mut inv);
        flush_invalidations(hwnd, &mut inv);
        update_title(hwnd, st, "");
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
                st.panels[1].set_focused(false, &mut inv);
                st.menubar.set_metrics(m.row_h, m.pad_x, &mut inv);
                st.toolbar.set_metrics(m.row_h, m.pad_x, &mut inv);
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
        WM_MOUSEWHEEL => {
            if let Some(st) = state_of(hwnd) {
                let delta = (wparam.0 >> 16) as i16 as i32;
                let ev = if wparam.0 & MK_SHIFT != 0 {
                    InputEvent::HWheel { delta: -delta }
                } else {
                    InputEvent::Wheel { delta }
                };
                let mut inv = Invalidations::default();
                st.active_panel().on_event(&ev, &mut inv);
                flush_invalidations(hwnd, &mut inv);
            }
            LRESULT(0)
        }
        WM_MOUSEHWHEEL => {
            if let Some(st) = state_of(hwnd) {
                let delta = (wparam.0 >> 16) as i16 as i32;
                let mut inv = Invalidations::default();
                st.active_panel()
                    .on_event(&InputEvent::HWheel { delta }, &mut inv);
                flush_invalidations(hwnd, &mut inv);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            if let Some(st) = state_of(hwnd) {
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
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
                    // 도구 모음 행
                    st.toolbar.on_event(&ev, &mut inv);
                    flush_invalidations(hwnd, &mut inv);
                    if let Some(cmd) = st.toolbar.take_command() {
                        run_command(hwnd, st, cmd);
                    }
                    return LRESULT(0);
                }
                if y >= st.statusbar.bounds().y {
                    return LRESULT(0); // 상태바 — 표시 전용
                }
                if st.active_panel().pathbar.is_editing() {
                    // 포커스아웃 = 편집 취소(docs/27 §2)
                    st.active_panel().pathbar.cancel_edit(&mut inv);
                } else if (x - sx).abs() <= half.max(1) {
                    st.split_drag = true;
                    let _ = InvalidateRect(Some(hwnd), None, false);
                } else if let Some(idx) = st.panel_at(x) {
                    set_active(hwnd, st, idx);
                    st.panels[idx].on_event(&ev, &mut inv);
                    let ctx = st.nav_ctx();
                    st.panels[idx].drain_actions(ctx, &mut inv);
                }
                flush_invalidations(hwnd, &mut inv);
                update_title(hwnd, st, "");
                update_status(hwnd, st);
            }
            LRESULT(0)
        }
        WM_RBUTTONDOWN => {
            if let Some(st) = state_of(hwnd) {
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                if let Some(idx) = st.panel_at(x) {
                    set_active(hwnd, st, idx);
                    let mut inv = Invalidations::default();
                    st.panels[idx].on_event(&InputEvent::RightDown { x, y }, &mut inv);
                    flush_invalidations(hwnd, &mut inv);
                }
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if let Some(st) = state_of(hwnd) {
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                let mut inv = Invalidations::default();
                if st.split_drag {
                    let rc = client_rect(hwnd);
                    if rc.w > 0 {
                        st.split = (x as f32 / rc.w as f32).clamp(0.1, 0.9);
                        layout(hwnd, st, &mut inv);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                } else {
                    let ev = InputEvent::MouseMove { x, y };
                    st.menubar.on_event(&ev, &mut inv);
                    st.toolbar.on_event(&ev, &mut inv);
                    if !st.menubar.is_open() {
                        // 드롭다운 아래 hover 잔상 방지
                        st.panels[0].on_event(&ev, &mut inv);
                        st.panels[1].on_event(&ev, &mut inv);
                    }
                }
                flush_invalidations(hwnd, &mut inv);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            if let Some(st) = state_of(hwnd) {
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                if st.split_drag {
                    st.split_drag = false;
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                let ev = InputEvent::MouseUp { x, y };
                let mut inv = Invalidations::default();
                st.panels[0].on_event(&ev, &mut inv);
                st.panels[1].on_event(&ev, &mut inv);
                flush_invalidations(hwnd, &mut inv);
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
            if let Some(st) = state_of(hwnd) {
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                if let Some(idx) = st.panel_at(x) {
                    set_active(hwnd, st, idx);
                    let rows = st.panels[idx].rows();
                    let hit = (!rows.marker_hit(x, y))
                        .then(|| rows.row_at(x, y))
                        .flatten();
                    if let Some(row) = hit {
                        let mut inv = Invalidations::default();
                        let ctx = st.nav_ctx();
                        st.panels[idx].activate_row(row, ctx, &mut inv);
                        flush_invalidations(hwnd, &mut inv);
                        update_title(hwnd, st, "");
                    }
                }
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
                        st.active_panel().pathbar.cancel_edit(&mut inv);
                    }
                    flush_invalidations(hwnd, &mut inv);
                    update_title(hwnd, st, "");
                    return LRESULT(0);
                }
                if st.active_panel().rows().is_renaming() {
                    // 인라인 이름변경 중 — Enter=확정·Esc=취소, 그 외 키는 편집기가 차단(M3-2)
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
                    // 내부 클립보드 복사/잘라내기(M3-1) — 선택 없으면 클립보드 유지
                    if let Some(c) = clip_from_selection(st, nexa_ops::Op::Copy) {
                        st.clipboard = Some(c);
                    }
                } else if vk == b'X' as u16 && ctrl {
                    if let Some(c) = clip_from_selection(st, nexa_ops::Op::Move) {
                        st.clipboard = Some(c);
                    }
                } else if vk == b'V' as u16 && ctrl {
                    if let Some((paths, op)) = st.clipboard.clone() {
                        if op == nexa_ops::Op::Move {
                            st.clipboard = None; // 잘라내기는 1회성(원본 관례)
                        }
                        let dest = st.active_panel().root_path().to_path_buf();
                        start_transfer(hwnd, st, paths, dest, op);
                        return LRESULT(0);
                    }
                } else if vk == VK_RETURN.0 {
                    if let Some(c) = st.active_panel().rows().caret() {
                        st.active_panel().activate_row(c, ctx, &mut inv);
                    }
                } else if vk == VK_F2.0 {
                    begin_rename_caret(hwnd, st); // F2 = 인라인 이름변경(M3-2)
                    return LRESULT(0);
                } else if vk == VK_DELETE.0 {
                    do_delete(hwnd, st, shift); // Del=휴지통·Shift+Del=완전(M3-2)
                    return LRESULT(0);
                } else if vk == b'N' as u16 && ctrl && shift {
                    run_command(hwnd, st, CMD_NEW_FOLDER); // Ctrl+Shift+N = 새 폴더(탐색기 관례)
                    return LRESULT(0);
                } else if vk == b'H' as u16 && ctrl {
                    run_command(hwnd, st, CMD_TOGGLE_HIDDEN); // 메뉴 체크와 동기
                    return LRESULT(0);
                } else if vk == VK_OEM_PERIOD.0 && ctrl {
                    run_command(hwnd, st, CMD_TOGGLE_DOTFILES);
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
                let handled = if vk == VK_LEFT.0 {
                    st.active_panel().nav_back(ctx, &mut inv);
                    true
                } else if vk == VK_RIGHT.0 {
                    st.active_panel().nav_forward(ctx, &mut inv);
                    true
                } else if vk == VK_UP.0 {
                    st.active_panel().nav_up(ctx, &mut inv);
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
        // 전송 워커 통지(M3-1) — 진행률·완료(양쪽 재로드)
        m if m == WM_APP_TRANSFER => {
            if let Some(st) = state_of(hwnd) {
                on_transfer_message(hwnd, st, wparam.0 as u64, lparam.0 == 1);
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
                    let mut inv = Invalidations::default();
                    if st.active_panel().pathbar.is_editing() {
                        if c == '\u{8}' || !c.is_control() {
                            st.active_panel().pathbar.edit_char(c, &mut inv);
                        }
                    } else if GetKeyState(VK_CONTROL.0 as i32) >= 0
                        && (c == '\u{8}'
                            || (!c.is_control()
                                // 스페이스는 선택 토글 키 — 단 이름변경 중엔 버퍼로(M3-2)
                                && (c != ' ' || st.active_panel().rows().is_renaming())))
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
                    if st.icons.borrow_mut().tick() {
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                    if !st.icons.borrow().has_pending() {
                        let _ = KillTimer(Some(hwnd), TIMER_ICONS);
                    }
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
            // 종료 저장(M2-5 — data\ 원자적 쓰기). 주기 저장·코얼레싱은 후속(원본 SESS)
            if let Some(st) = state_of(hwnd) {
                let settings = Settings {
                    theme: st.theme_mode.as_str().into(),
                    lang: st.lang_setting.clone(),
                    show_hidden: st.show_hidden,
                    show_dotfiles: st.show_dotfiles,
                    split: st.split,
                };
                let (t0, a0) = st.panels[0].session();
                let (t1, a1) = st.panels[1].session();
                let session = Session {
                    active_panel: st.active,
                    panels: [
                        PanelSession {
                            tabs: t0,
                            active: a0,
                        },
                        PanelSession {
                            tabs: t1,
                            active: a1,
                        },
                    ],
                };
                let dir = config::data_dir();
                if let Err(e) = config::save(&dir, SETTINGS_FILE, &settings.serialize())
                    .and_then(|_| config::save(&dir, SESSION_FILE, &session.serialize()))
                {
                    eprintln!("설정/세션 저장 실패: {e}");
                }
            }
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_NCDESTROY => {
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
