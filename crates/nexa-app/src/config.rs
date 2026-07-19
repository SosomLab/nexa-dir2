//! 설정/세션 영속(M2-5) — 원본 docs/34·40·43 차용, 포터블 규율(DR-3):
//! 영속물 = **exe 옆 `data\`**(레지스트리·%APPDATA% 비의존). 외부 crate 0 유지를 위해
//! 단순 `key=value` 텍스트(UTF-8·한 줄 1키·`#` 주석). 쓰기는 원자적(temp → rename).
//! 주기 저장·코얼레싱(원본 SESS 규율)은 후속 — 초안은 기동 로드/종료 저장.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// 퀵 런처 항목(M5-1 — 원본 docs/44 `Launcher.Items` 설계: Label/Path/Args).
/// `args`의 `%path%` = 활성 패널의 현재 폴더로 치환(원본 ToolLauncher 규약).
/// 그룹 구분선(도구 모음 그룹화 대응)은 `launcherN=-` — [`LauncherItem::separator`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LauncherItem {
    pub label: String,
    pub exe: String,
    pub args: String,
}

impl LauncherItem {
    /// 그룹 구분선 항목(`launcherN=-`).
    pub fn separator() -> Self {
        LauncherItem {
            label: "-".into(),
            exe: String::new(),
            args: String::new(),
        }
    }

    pub fn is_separator(&self) -> bool {
        self.exe.is_empty() && self.label == "-"
    }
}

/// 설정(원본 ViewOptions·ThemeOptions 대응) — `data\settings.cfg`.
#[derive(Clone, PartialEq, Debug)]
pub struct Settings {
    /// "system" | "light" | "dark"
    pub theme: String,
    /// "system" | 언어 코드(en·ko·발견된 `data\lang\*.lang`) — M2-6.
    pub lang: String,
    pub show_hidden: bool,
    pub show_dotfiles: bool,
    /// 좌 패널 폭 비율.
    pub split: f32,
    /// 하단 도크 표시(M4-1 — 원본 세션 저장 계승).
    pub dock: bool,
    /// 도크 높이 비율(S2 — 원본 분할 위치 저장 계승).
    pub dock_ratio: f32,
    /// 도크 밴드 좌/우 분할 비율(X-6 — 파일 좌/우와 독립, 원본 BottomSplitter 대응).
    pub dock_split: f32,
    /// 터미널 글꼴(QA 07-14 — 원본 Fonts.ConsoleFamily 대응). **쉼표 목록 = 폴백 체인**
    /// (WT식 `D2Coding, JetBrainsMono Nerd Font` — 1순위에 없는 글리프는 다음 폰트).
    pub term_font: String,
    /// 터미널 글꼴 크기(DIP, 8~32 — 원본 ConsoleSize 대응).
    pub term_font_size: i32,
    /// 터미널 줄 바꿈(X-3 ① — false=고정 열+가로 스크롤. 기본 true=현행 유지 —
    /// 원본 NoWrap 기본 true와 다름: dir2는 뷰 폭 래핑이 기존 동작이라 보존).
    pub term_wrap: bool,
    /// 터미널 고정 열 수(X-3 ② — 80~1000, 원본 MaxColumns 240. `term_wrap=false`일 때만).
    pub term_cols: i32,
    /// 대화상자(확인창·진행 창) 글꼴/크기(pt — QA 07-14 "대화창용 폰트 설정").
    pub dlg_font: String,
    pub dlg_font_size: i32,
    /// 폰트 슬롯(X-12 — 사용자 요청 07-16, 원본 Fonts 스크린샷):
    /// 기본(메뉴·탭·경로바·도크 등 특정 슬롯 없는 전부) / 우클릭 메뉴 / 상태바 /
    /// 파일 목록(+컬럼 헤더). 콘솔(터미널)=term_font·대화상자=dlg_font 기존 유지.
    pub base_font: String,
    pub base_font_size: i32,
    pub ctx_font: String,
    pub ctx_font_size: i32,
    pub status_font: String,
    pub status_font_size: i32,
    pub list_font: String,
    pub list_font_size: i32,
    /// 파일 목록 장식(X-12): 폴더 이름 굵게 / 헤더 굵게·이탤릭.
    pub list_folder_bold: bool,
    pub header_bold: bool,
    pub header_italic: bool,
    /// 폴더 우선 정렬(G-13 — 기본 true=탐색기 규약. false=파일·폴더 혼합 정렬).
    pub sort_folders_first: bool,
    /// 대소문자 구분 정렬(사용자 확정 07-15 — 기본 false. 코드포인트 순 = **대문자 그룹 상단**).
    pub sort_case_sensitive: bool,
    /// Alt+↑ 떠난 폴더 자동 선택의 뷰 배치(사용자 QA 07-15): "top"|"center"|"bottom".
    pub nav_up_align: String,
    /// 탭 더블클릭 동작(사용자 요청 07-15): "close"(기본)|"pin"|"lock" — 옵션 추가 예정.
    pub tab_dblclick: String,
    /// 타입어헤드(원본 docs/32 §7 — 설정 07-15): 범위 "global"|"level"|"visible"(기본).
    pub typeahead_scope: String,
    /// 입력 리셋(ms, 200~10000 — 기본 1000).
    pub typeahead_reset_ms: i32,
    /// HUD 배지 위치(0..8 = 3×3 — 기본 6=좌하).
    pub typeahead_pos: i32,
    /// 특수문자 포함(파일명 안전) · 공백 포함(접두사 입력 중) · Backspace 지우기.
    pub typeahead_special: bool,
    pub typeahead_space: bool,
    pub typeahead_backspace: bool,
    /// 보기 모드(사용자 요청 07-16): "tree"(계층 — 기본)|"flat"(일반 폴더)|"tiles"(타일).
    pub view_mode: String,
    /// 컬럼 너비 동기화(사용자 확정 07-18) — on = 좌/우 패널 폭 실시간 동기,
    /// off = 패널별 독립(탭은 패널 폭 상속).
    pub col_width_sync: bool,
    /// 컬럼 auto-fit(경계 더블클릭) 최대 폭(px @96dpi — 07-19 사용자,
    /// 50~2000 클램프).
    pub col_autofit_max: i32,
    /// 도구모음 블록/자식 순서(07-19 사용자 — prefs 트리 편집).
    /// 형식 = [`serialize_toolbar_order`], 읽기 = [`parse_toolbar_order`]
    /// (검증·누락 보충 — 전방 호환).
    pub toolbar_order: String,
    /// 앱 고유 컨텍스트 메뉴 항목 순서/표시(07-19 — [`CTXMENU_BLOCKS`] 문법).
    pub ctx_menu_order: String,
    /// 패널 모드(사용자 요청 07-16 — 원본 FR-C1 단일↔듀얼): "dual"(기본)|"single"
    /// (우 패널 숨김 — 상태는 보존, 복귀 시 원복).
    pub panel_mode: String,
    /// 정보(도크) 모드: "dual"(기본 — 좌/우 독립)|"single"(전폭 공유 — 활성 패널 추종).
    /// **사용자 선호값** — 싱글 패널에서는 효과가 싱글로 강제되지만 이 값은 보존되어
    /// 듀얼 패널 복귀 시 원복된다(사용자 확정 설계).
    pub info_mode: String,
    /// 퀵 런처 바 표시(M5-1 — 원본 LayoutState.ShowLauncher 대응, 기본 표시).
    pub launcher: bool,
    /// 퀵 런처 항목(M5-1). `None` = 키 부재(첫 실행 — 호스트가 시드 주입) ·
    /// `Some(빈 목록)` = 사용자가 비움(시드 재주입 금지). settings.cfg에서 직접 편집(α —
    /// `launcherN=라벨|exe|인자`). UI CRUD는 후속.
    pub launcher_items: Option<Vec<LauncherItem>>,
    /// 적용된 시드 버전(launcher.rs `SEED_VERSION`) — 낮으면 기동 시 신규 시드 1회 추가.
    pub launcher_seed: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "dark".into(), // DR-5 다크 기본
            lang: "system".into(),
            show_hidden: true,
            show_dotfiles: true,
            split: 0.5,
            dock: true, // 기본 표시(사용자 확정 07-19 — 현 배치 승격)
            dock_ratio: 0.3,
            dock_split: 0.5,
            term_font: "Consolas".into(),
            term_font_size: 12,
            term_wrap: true,
            term_cols: 240,
            dlg_font: "Segoe UI".into(),
            dlg_font_size: 9,
            base_font: "Segoe UI".into(),
            base_font_size: 12, // DIP — 기존 단일 UI 포맷과 동일(시각 무변)
            ctx_font: "Segoe UI".into(),
            ctx_font_size: 12,
            status_font: "Segoe UI".into(),
            status_font_size: 12,
            list_font: "Segoe UI".into(),
            list_font_size: 12,
            list_folder_bold: false,
            header_bold: false,
            header_italic: false,
            sort_folders_first: true,
            sort_case_sensitive: false,
            nav_up_align: "center".into(),
            tab_dblclick: "close".into(),
            typeahead_scope: "visible".into(),
            typeahead_reset_ms: 1000,
            typeahead_pos: 6,
            typeahead_special: true,
            typeahead_space: true,
            typeahead_backspace: true,
            view_mode: "tree".into(),
            col_width_sync: true,
            col_autofit_max: 400,
            toolbar_order: default_toolbar_order(),
            ctx_menu_order: default_order(CTXMENU_BLOCKS),
            panel_mode: "dual".into(),
            info_mode: "dual".into(),
            launcher: true,
            launcher_items: None,
            launcher_seed: 0,
        }
    }
}

/// 패널 1개의 세션(탭 경로들·활성 탭·탭별 펼침 집합[F18 — X-4, 원본 TabSession.Expanded]).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct PanelSession {
    pub tabs: Vec<PathBuf>,
    pub active: usize,
    /// 탭 인덱스 정렬(부족분 허용) — 각 탭의 펼침 경로 목록.
    pub expanded: Vec<Vec<PathBuf>>,
    /// 탭별 잠금(닫기 제외 — 원본 TabSession.Locked, 편의 UX ②).
    pub locked: Vec<bool>,
    /// 탭별 고정(📌 — 사용자 요청 07-15).
    pub pinned: Vec<bool>,
    /// 탭별 보기 모드("tree"|"flat"|"tiles" — 사용자 요청 07-16: 탭별 설정).
    pub modes: Vec<String>,
    /// 패널 컬럼 폭(px — 탭은 패널 폭 상속, 사용자 확정 07-18).
    /// 확장성: 향후 탭별 폭은 `panel{i}.colw{j}` 키로 같은 레벨에 추가한다
    /// (탭별 modes와 동일 직렬화 레벨 — 사용자 지침).
    pub col_widths: Vec<i32>,
    /// 컬럼 레이아웃(07-19) — `cols[name:1,ext:0,…]` 문법([`COLUMN_BLOCKS`]).
    /// 빈 문자열 = 기본. 폭([`Self::col_widths`])은 **표시 컬럼의 표시 순** 대응.
    pub col_layout: String,
}

/// 세션(원본 session.json 대응) — `data\session.cfg`(패널·탭·활성·펼침).
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Session {
    pub active_panel: usize,
    pub panels: [PanelSession; 2],
}

/// 영속 디렉터리(DR-3 개정 07-16 — 포터블 우선 + 설치형 폴백):
/// **exe 옆 `data\`가 기본**(포터블 — 기존 동작 그대로). 설치형(Program Files 등
/// **쓰기 불가 위치**)이면 `%LOCALAPPDATA%\NexaDir\data`로 폴백.
/// 판정은 프로세스당 1회(OnceLock — 매 저장마다 쓰기 프로브 방지).
pub fn data_dir() -> PathBuf {
    static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let exe_side = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("data");
        choose_data_dir(exe_side)
    })
    .clone()
}

/// 포터블/설치형 선택 규칙(테스트 분리용): 후보에 **디렉터리 생성+쓰기 프로브**가
/// 성공하면 그대로(포터블), 실패하면 `%LOCALAPPDATA%\NexaDir\data`(설치형).
/// LOCALAPPDATA조차 없으면 후보 유지(기존 폴백 동작 보존).
/// **마이그레이션(07-19 제품명 정리 nexa-dir2→nexa-dir)**: 구 폴백
/// `%LOCALAPPDATA%\NexaDir2\data`가 있으면 신 경로로 rename(실패 시 구 경로 유지 —
/// 데이터 무손실). 신 경로가 이미 있으면 마이그레이션 생략.
fn choose_data_dir(exe_side: PathBuf) -> PathBuf {
    if dir_writable(&exe_side) {
        return exe_side;
    }
    match std::env::var_os("LOCALAPPDATA") {
        Some(la) => {
            let base = PathBuf::from(&la);
            let new = base.join("NexaDir").join("data");
            let old = base.join("NexaDir2").join("data");
            if !new.exists() && old.exists() {
                if let Some(parent) = new.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if fs::rename(&old, &new).is_err() {
                    return old; // rename 실패 시 구 경로 그대로 사용(데이터 보존)
                }
            }
            new
        }
        None => exe_side,
    }
}

/// 쓰기 가능 프로브 — 생성 시도 후 임시 파일 1개 쓰고 지운다(ACL·읽기 전용 감지).
fn dir_writable(dir: &Path) -> bool {
    if fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(format!(".w{}", std::process::id()));
    match fs::write(&probe, b"") {
        Ok(()) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

// ── 직렬화(key=value) ────────────────────────────────────────────

fn kv_lines(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.lines().filter_map(|l| {
        let l = l.trim();
        if l.is_empty() || l.starts_with('#') {
            return None;
        }
        l.split_once('=')
    })
}

impl Settings {
    pub fn serialize(&self) -> String {
        let mut out = format!(
            "# nexa-dir settings v1\ntheme={}\nlang={}\nshow_hidden={}\nshow_dotfiles={}\nsplit={:.3}\ndock={}\ndock_ratio={:.3}\ndock_split={:.3}\nterm_font={}\nterm_font_size={}\ndlg_font={}\ndlg_font_size={}\nlauncher={}\n",
            self.theme,
            self.lang,
            u8::from(self.show_hidden),
            u8::from(self.show_dotfiles),
            self.split,
            u8::from(self.dock),
            self.dock_ratio,
            self.dock_split,
            self.term_font,
            self.term_font_size,
            self.dlg_font,
            self.dlg_font_size,
            u8::from(self.launcher),
        );
        // 항목은 시드 주입 후 항상 Some — count를 명시해 "부재(첫 실행)"와 "비움"을 구분
        out.push_str(&format!(
            "term_wrap={}\nterm_cols={}\n",
            u8::from(self.term_wrap),
            self.term_cols
        ));
        out.push_str(&format!(
            "sort_folders_first={}\nsort_case_sensitive={}\nnav_up_align={}\ntab_dblclick={}\nview_mode={}\npanel_mode={}\ninfo_mode={}\n",
            u8::from(self.sort_folders_first),
            u8::from(self.sort_case_sensitive),
            self.nav_up_align,
            self.tab_dblclick,
            self.view_mode,
            self.panel_mode,
            self.info_mode
        ));
        out.push_str(&format!(
            "col_width_sync={}\n",
            u8::from(self.col_width_sync)
        ));
        out.push_str(&format!("col_autofit_max={}\n", self.col_autofit_max));
        out.push_str(&format!("toolbar_order={}\n", self.toolbar_order));
        out.push_str(&format!("ctx_menu_order={}\n", self.ctx_menu_order));
        out.push_str(&format!(
            "typeahead_scope={}\ntypeahead_reset_ms={}\ntypeahead_pos={}\ntypeahead_special={}\ntypeahead_space={}\ntypeahead_backspace={}\n",
            self.typeahead_scope,
            self.typeahead_reset_ms,
            self.typeahead_pos,
            u8::from(self.typeahead_special),
            u8::from(self.typeahead_space),
            u8::from(self.typeahead_backspace)
        ));
        out.push_str(&format!(
            "base_font={}\nbase_font_size={}\nctx_font={}\nctx_font_size={}\nstatus_font={}\nstatus_font_size={}\nlist_font={}\nlist_font_size={}\nlist_folder_bold={}\nheader_bold={}\nheader_italic={}\n",
            self.base_font,
            self.base_font_size,
            self.ctx_font,
            self.ctx_font_size,
            self.status_font,
            self.status_font_size,
            self.list_font,
            self.list_font_size,
            u8::from(self.list_folder_bold),
            u8::from(self.header_bold),
            u8::from(self.header_italic)
        ));
        out.push_str(&format!("launcher_seed={}\n", self.launcher_seed));
        if let Some(items) = &self.launcher_items {
            out.push_str(&format!("launcher_count={}\n", items.len()));
            for (i, it) in items.iter().enumerate() {
                if it.is_separator() {
                    out.push_str(&format!("launcher{i}=-\n")); // 그룹 구분선
                } else {
                    out.push_str(&format!(
                        "launcher{i}={}|{}|{}\n",
                        it.label, it.exe, it.args
                    ));
                }
            }
        }
        out
    }

    /// 손상·미지 키는 무시하고 기본값 위에 덮어쓴다(관용 파싱).
    pub fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        for (k, v) in kv_lines(text) {
            match k {
                "theme" if matches!(v, "system" | "light" | "dark") => s.theme = v.into(),
                // 코드 검증은 i18n resolve(발견 목록 대조)가 담당 — 여기선 형태만
                "lang" if !v.is_empty() && v.len() <= 16 => s.lang = v.into(),
                "show_hidden" => s.show_hidden = v != "0",
                "show_dotfiles" => s.show_dotfiles = v != "0",
                "dock" => s.dock = v != "0",
                "term_font" if !v.trim().is_empty() && v.len() <= 128 => {
                    s.term_font = v.trim().into()
                }
                "term_font_size" => {
                    if let Ok(n) = v.parse::<i32>() {
                        s.term_font_size = n.clamp(8, 32);
                    }
                }
                "dlg_font" if !v.trim().is_empty() && v.len() <= 64 => s.dlg_font = v.trim().into(),
                "dlg_font_size" => {
                    if let Ok(n) = v.parse::<i32>() {
                        s.dlg_font_size = n.clamp(7, 24);
                    }
                }
                "dock_ratio" => {
                    if let Ok(f) = v.parse::<f32>() {
                        if f.is_finite() {
                            s.dock_ratio = f.clamp(0.15, 0.5);
                        }
                    }
                }
                "dock_split" => {
                    if let Ok(f) = v.parse::<f32>() {
                        if f.is_finite() {
                            s.dock_split = f.clamp(0.15, 0.85);
                        }
                    }
                }
                "split" => {
                    if let Ok(f) = v.parse::<f32>() {
                        if f.is_finite() {
                            s.split = f.clamp(0.1, 0.9);
                        }
                    }
                }
                "base_font" if !v.trim().is_empty() => s.base_font = v.trim().into(),
                "base_font_size" => {
                    if let Ok(n) = v.parse::<i32>() {
                        s.base_font_size = n.clamp(8, 32);
                    }
                }
                "ctx_font" if !v.trim().is_empty() => s.ctx_font = v.trim().into(),
                "ctx_font_size" => {
                    if let Ok(n) = v.parse::<i32>() {
                        s.ctx_font_size = n.clamp(8, 32);
                    }
                }
                "status_font" if !v.trim().is_empty() => s.status_font = v.trim().into(),
                "status_font_size" => {
                    if let Ok(n) = v.parse::<i32>() {
                        s.status_font_size = n.clamp(8, 32);
                    }
                }
                "list_font" if !v.trim().is_empty() => s.list_font = v.trim().into(),
                "list_font_size" => {
                    if let Ok(n) = v.parse::<i32>() {
                        s.list_font_size = n.clamp(8, 32);
                    }
                }
                "list_folder_bold" => s.list_folder_bold = v != "0",
                "header_bold" => s.header_bold = v != "0",
                "header_italic" => s.header_italic = v != "0",
                "term_wrap" => s.term_wrap = v != "0",
                "term_cols" => {
                    if let Ok(n) = v.parse::<i32>() {
                        s.term_cols = n.clamp(80, 1000);
                    }
                }
                "sort_folders_first" => s.sort_folders_first = v != "0",
                "sort_case_sensitive" => s.sort_case_sensitive = v != "0",
                "nav_up_align" if matches!(v, "top" | "center" | "bottom") => {
                    s.nav_up_align = v.into()
                }
                "col_width_sync" => s.col_width_sync = v != "0",
                "col_autofit_max" => {
                    if let Ok(n) = v.parse::<i32>() {
                        s.col_autofit_max = n.clamp(50, 2000);
                    }
                }
                // 재직렬화로 정규화(미지 토큰 제거·누락 보충 — 전방 호환)
                "toolbar_order" => {
                    s.toolbar_order = serialize_toolbar_order(&parse_toolbar_order(v))
                }
                "ctx_menu_order" => {
                    s.ctx_menu_order =
                        serialize_order_with(&parse_order_with(CTXMENU_BLOCKS, v), true)
                }
                "view_mode" if matches!(v, "tree" | "flat" | "tiles") => {
                    s.view_mode = v.into() // 미지 값 = 기본(tree) 유지
                }
                "panel_mode" if matches!(v, "single" | "dual") => s.panel_mode = v.into(),
                "info_mode" if matches!(v, "single" | "dual") => s.info_mode = v.into(),
                "tab_dblclick" if matches!(v, "close" | "pin" | "lock") => {
                    s.tab_dblclick = v.into()
                }
                "typeahead_scope" if matches!(v, "global" | "level" | "visible") => {
                    s.typeahead_scope = v.into()
                }
                "typeahead_reset_ms" => {
                    if let Ok(n) = v.parse::<i32>() {
                        s.typeahead_reset_ms = n.clamp(200, 10_000);
                    }
                }
                "typeahead_pos" => {
                    if let Ok(n) = v.parse::<i32>() {
                        s.typeahead_pos = n.clamp(0, 8);
                    }
                }
                "typeahead_special" => s.typeahead_special = v != "0",
                "typeahead_space" => s.typeahead_space = v != "0",
                "typeahead_backspace" => s.typeahead_backspace = v != "0",
                "launcher" => s.launcher = v != "0",
                "launcher_seed" => s.launcher_seed = v.parse().unwrap_or(0),
                // count 키 존재 = 항목 목록 확정(비움 포함) — launcherN은 아래에서 채움
                "launcher_count" => {
                    if s.launcher_items.is_none() {
                        s.launcher_items = Some(Vec::new());
                    }
                }
                k if k.starts_with("launcher")
                    && k["launcher".len()..].parse::<usize>().is_ok() =>
                {
                    let items = s.launcher_items.get_or_insert_with(Vec::new);
                    if items.len() >= 32 {
                        continue;
                    }
                    if v.trim() == "-" {
                        items.push(LauncherItem::separator()); // 그룹 구분선
                        continue;
                    }
                    let mut parts = v.splitn(3, '|');
                    let (label, exe) = (
                        parts.next().unwrap_or("").trim(),
                        parts.next().unwrap_or("").trim(),
                    );
                    let args = parts.next().unwrap_or("").to_string();
                    if !label.is_empty() && !exe.is_empty() {
                        items.push(LauncherItem {
                            label: label.into(),
                            exe: exe.into(),
                            args,
                        });
                    }
                }
                _ => {}
            }
        }
        s
    }
}

impl Session {
    /// 탭 경로 구분자 `|` — Windows 경로에 등장 불가 문자.
    pub fn serialize(&self) -> String {
        let mut out = String::from("# nexa-dir session v1\n");
        out.push_str(&format!("active_panel={}\n", self.active_panel));
        for (i, p) in self.panels.iter().enumerate() {
            let tabs: Vec<String> = p
                .tabs
                .iter()
                .map(|t| t.to_string_lossy().into_owned())
                .collect();
            out.push_str(&format!("panel{i}.tabs={}\n", tabs.join("|")));
            out.push_str(&format!("panel{i}.active={}\n", p.active));
            // 탭별 펼침 집합(F18) — 빈 목록은 생략(하위 호환·파일 간결)
            for (j, exp) in p.expanded.iter().enumerate() {
                if !exp.is_empty() {
                    let list: Vec<String> = exp
                        .iter()
                        .map(|t| t.to_string_lossy().into_owned())
                        .collect();
                    out.push_str(&format!("panel{i}.exp{j}={}\n", list.join("|")));
                }
            }
            // 탭별 잠금(편의 UX ②) — 하나라도 잠겨 있을 때만 기록
            if p.locked.iter().any(|l| *l) {
                let flags: Vec<&str> = p
                    .locked
                    .iter()
                    .map(|l| if *l { "1" } else { "0" })
                    .collect();
                out.push_str(&format!("panel{i}.locked={}\n", flags.join("|")));
            }
            // 탭별 고정(07-15) — 동일 규약
            if p.pinned.iter().any(|l| *l) {
                let flags: Vec<&str> = p
                    .pinned
                    .iter()
                    .map(|l| if *l { "1" } else { "0" })
                    .collect();
                out.push_str(&format!("panel{i}.pinned={}\n", flags.join("|")));
            }
            // 탭별 보기 모드(07-16) — 전부 tree(기본)면 생략
            if p.modes.iter().any(|m| m != "tree") {
                out.push_str(&format!("panel{i}.modes={}\n", p.modes.join("|")));
            }
            // 패널 컬럼 폭(07-18 — 탭 상속. 탭별 확장 = panel{i}.colw{j} 예약)
            if !p.col_layout.is_empty() {
                out.push_str(&format!("panel{i}.cols={}\n", p.col_layout));
            }
            if !p.col_widths.is_empty() {
                let ws: Vec<String> = p.col_widths.iter().map(|w| w.to_string()).collect();
                out.push_str(&format!("panel{i}.colw={}\n", ws.join(",")));
            }
        }
        out
    }

    pub fn parse(text: &str) -> Session {
        let mut s = Session::default();
        for (k, v) in kv_lines(text) {
            match k {
                "active_panel" => s.active_panel = v.parse().unwrap_or(0).min(1),
                "panel0.tabs" | "panel1.tabs" => {
                    let idx = usize::from(k.starts_with("panel1"));
                    s.panels[idx].tabs = v
                        .split('|')
                        .filter(|p| !p.is_empty())
                        .map(PathBuf::from)
                        .collect();
                }
                "panel0.active" | "panel1.active" => {
                    let idx = usize::from(k.starts_with("panel1"));
                    s.panels[idx].active = v.parse().unwrap_or(0);
                }
                "panel0.locked" | "panel1.locked" => {
                    let idx = usize::from(k.starts_with("panel1"));
                    s.panels[idx].locked = v.split('|').map(|f| f == "1").collect();
                }
                "panel0.pinned" | "panel1.pinned" => {
                    let idx = usize::from(k.starts_with("panel1"));
                    s.panels[idx].pinned = v.split('|').map(|f| f == "1").collect();
                }
                "panel0.cols" | "panel1.cols" => {
                    // 컬럼 레이아웃(07-19) — 재직렬화 정규화(미지 토큰 제거·보충)
                    let idx = usize::from(k.starts_with("panel1"));
                    s.panels[idx].col_layout =
                        serialize_order_with(&parse_order_with(COLUMN_BLOCKS, v), true);
                }
                "panel0.colw" | "panel1.colw" => {
                    // 패널 컬럼 폭(07-18 — 탭 상속. 탭별 확장 = panel{i}.colw{j})
                    let idx = usize::from(k.starts_with("panel1"));
                    s.panels[idx].col_widths = v
                        .split(',')
                        .filter_map(|w| w.trim().parse::<i32>().ok())
                        .collect();
                }
                "panel0.modes" | "panel1.modes" => {
                    let idx = usize::from(k.starts_with("panel1"));
                    s.panels[idx].modes = v
                        .split('|')
                        .map(|m| {
                            if matches!(m, "flat" | "tiles") {
                                m.to_string()
                            } else {
                                "tree".to_string() // 미지 값 = 기본
                            }
                        })
                        .collect();
                }
                k if k.starts_with("panel0.exp") || k.starts_with("panel1.exp") => {
                    let idx = usize::from(k.starts_with("panel1"));
                    let Ok(j) = k["panelN.exp".len()..].parse::<usize>() else {
                        continue;
                    };
                    if j > 64 {
                        continue; // 손상 방어
                    }
                    let exp = &mut s.panels[idx].expanded;
                    if exp.len() <= j {
                        exp.resize(j + 1, Vec::new());
                    }
                    exp[j] = v
                        .split('|')
                        .filter(|p| !p.is_empty())
                        .map(PathBuf::from)
                        .collect();
                }
                _ => {}
            }
        }
        s
    }
}

// ── 파일 I/O(원자적 쓰기) ────────────────────────────────────────

pub fn load(dir: &Path, name: &str) -> Option<String> {
    fs::read_to_string(dir.join(name)).ok()
}

/// temp에 쓰고 rename — 저장 중 크래시에도 기존 파일 보존(원본 SESS 원자성 계승).
pub fn save(dir: &Path, name: &str, content: &str) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{name}.tmp"));
    fs::write(&tmp, content)?;
    let dst = dir.join(name);
    if dst.exists() {
        fs::remove_file(&dst)?;
    }
    fs::rename(&tmp, &dst)
}

/// 영속 파일명(사용자 지시 07-14): 저장 항목을 포괄하는 이름 + `.cfg` 확장자.
/// settings = 앱 설정 전반(테마·언어·보기·도크·터미널 글꼴) · session = 화면 세션
/// (패널·탭·활성·펼침 집합).
pub const SETTINGS_FILE: &str = "settings.cfg";
pub const SESSION_FILE: &str = "session.cfg";
/// 구 파일명(~0.5.0 — `.txt`) 마이그레이션 폴백.
pub const SETTINGS_FILE_OLD: &str = "settings.txt";
pub const SESSION_FILE_OLD: &str = "session.txt";

/// 새 이름 우선 로드, 없으면 구 이름(1회성 마이그레이션 — 다음 저장은 새 이름·구 파일은
/// [`purge_legacy`]가 정리).
pub fn load_migrated(dir: &Path, name: &str, old: &str) -> Option<String> {
    load(dir, name).or_else(|| load(dir, old))
}

/// 새 이름 저장 성공 후 구 `.txt` 파일 정리(포터블 data\ 청결 — 실패 무시).
pub fn purge_legacy(dir: &Path) {
    let _ = fs::remove_file(dir.join(SETTINGS_FILE_OLD));
    let _ = fs::remove_file(dir.join(SESSION_FILE_OLD));
}

/// 도구모음 블록 정의(순서 편집 SSOT — 07-19): `(블록 key, 자식 key 목록)`.
/// 빈 자식 = 단일 버튼 블록. key는 [`Settings::toolbar_order`] 직렬화 토큰.
pub const TOOLBAR_BLOCKS: &[(&str, &[&str])] = &[
    // 기본 순서 = 사용자 확정 07-19("현재 순서를 기본값으로")
    ("refresh", &[]),
    ("panel", &["toggle", "dock", "info", "colsync"]),
    ("view", &["tree", "flat", "tiles"]),
    ("show", &["hidden", "dot"]),
    ("settings", &[]),
];

/// 기본 순서 문자열(정의 순·전부 표시 — vis 포함 문법).
pub fn default_order(defs: OrderDefs) -> String {
    serialize_order_with(
        &defs
            .iter()
            .map(|(b, items)| {
                (
                    b.to_string(),
                    true,
                    items.iter().map(|i| (i.to_string(), true)).collect(),
                )
            })
            .collect::<Vec<_>>(),
        true,
    )
}

/// 기본 순서 문자열(= [`TOOLBAR_BLOCKS`] 정의 순·전부 표시).
pub fn default_toolbar_order() -> String {
    default_order(TOOLBAR_BLOCKS)
}

/// 순서 정의 타입(SSOT) — `(블록 key, 자식 key 목록)`, 빈 자식 = 단일 블록.
pub type OrderDefs = &'static [(&'static str, &'static [&'static str])];

/// 파싱된 순서 블록 — `(블록 key, 블록 표시, 자식[(key, 표시)])`(07-19).
pub type OrderBlock = (String, bool, Vec<(String, bool)>);

/// 파일 목록 컬럼 정의(07-19 — key 순서 = 기본 표시 순서. name = 상시 표시).
pub const COLUMN_BLOCKS: OrderDefs = &[("cols", &["name", "ext", "size", "modified", "kind"])];

/// 앱 고유 컨텍스트 메뉴 항목 정의(07-19 — 셸 제공 동사는 대상 아님).
pub const CTXMENU_BLOCKS: OrderDefs = &[
    ("row", &["deletePermanent", "copyName", "pasteInto"]),
    ("bg", &["paste", "undo", "redo"]),
];

/// 제네릭 순서 직렬화 — `블록:vis[자식:vis,…]|블록:vis|…`(단일 블록 =
/// 대괄호 생략). `with_vis` = 표시 여부 포함. **블록 vis(07-19)**: 그룹
/// 숨김 = 통째 비표시(자식 상태는 보존).
pub fn serialize_order_with(
    order: &[OrderBlock],
    with_vis: bool,
) -> String {
    order
        .iter()
        .map(|(b, bv, items)| {
            let head = if with_vis {
                format!("{b}:{}", u8::from(*bv))
            } else {
                b.clone()
            };
            if items.is_empty() {
                head
            } else {
                let inner: Vec<String> = items
                    .iter()
                    .map(|(k, v)| {
                        if with_vis {
                            format!("{k}:{}", u8::from(*v))
                        } else {
                            k.clone()
                        }
                    })
                    .collect();
                format!("{head}[{}]", inner.join(","))
            }
        })
        .collect::<Vec<_>>()
        .join("|")
}

/// 제네릭 순서 파싱 + 검증 — 미지 블록/자식·중복은 버리고, **누락분은 기본
/// 정의 순으로 보충**(전방 호환). `자식:0/1` = 표시 여부(생략 = 표시).
pub fn parse_order_with(defs: OrderDefs, s: &str) -> Vec<OrderBlock> {
    let mut out: Vec<OrderBlock> = Vec::new();
    for tok in s.split('|') {
        let tok = tok.trim();
        let (head, inner) = match tok.split_once('[') {
            Some((n, rest)) => (n.trim(), Some(rest.trim_end_matches(']'))),
            None => (tok, None),
        };
        // 블록 vis(07-19): `이름:0` — 생략 = 표시(구형 호환)
        let (name, bvis) = match head.split_once(':') {
            Some((n, v)) => (n.trim(), v.trim() != "0"),
            None => (head, true),
        };
        let Some((_, def_items)) = defs.iter().find(|(b, _)| *b == name) else {
            continue; // 미지 블록
        };
        if out.iter().any(|(b, _, _)| b == name) {
            continue; // 중복 블록
        }
        let mut items: Vec<(String, bool)> = Vec::new();
        if let Some(inner) = inner {
            for it in inner.split(',') {
                let it = it.trim();
                let (k, vis) = match it.split_once(':') {
                    Some((k, v)) => (k.trim(), v.trim() != "0"),
                    None => (it, true),
                };
                if def_items.contains(&k) && !items.iter().any(|(x, _)| x == k) {
                    items.push((k.to_string(), vis));
                }
            }
        }
        for (di, d) in def_items.iter().enumerate() {
            if !items.iter().any(|(x, _)| x == d) {
                // 누락 자식 보충(표시) — 정의상 앞 형제 뒤에 삽입(07-19:
                // 신규 버튼이 저장된 구 순서에서도 의도 위치에 들어가도록)
                let pos = def_items[..di]
                    .iter()
                    .rev()
                    .find_map(|prev| items.iter().position(|(x, _)| x == prev).map(|p| p + 1))
                    .unwrap_or(0);
                items.insert(pos, (d.to_string(), true));
            }
        }
        out.push((name.to_string(), bvis, items));
    }
    for (b, def_items) in defs {
        if !out.iter().any(|(n, _, _)| n == b) {
            out.push((
                b.to_string(),
                true,
                def_items.iter().map(|i| (i.to_string(), true)).collect(),
            ));
        }
    }
    out
}

/// 순서 직렬화(툴바 — 07-19 표시 여부 공통화로 vis 포함).
pub fn serialize_toolbar_order(order: &[OrderBlock]) -> String {
    serialize_order_with(order, true)
}

/// 순서 파싱(툴바 래퍼 — [`parse_order_with`] + [`TOOLBAR_BLOCKS`].
/// 구형 vis 없는 문자열도 호환 — 생략 = 표시).
pub fn parse_toolbar_order(s: &str) -> Vec<OrderBlock> {
    parse_order_with(TOOLBAR_BLOCKS, s)
}

#[cfg(test)]
mod toolbar_order_tests {
    use super::*;

    #[test]
    fn roundtrip_and_merge() {
        let d = default_toolbar_order();
        assert_eq!(serialize_toolbar_order(&parse_toolbar_order(&d)), d, "기본 왕복");
        // 재배열 + 표시 여부 왕복(07-19 — 블록/자식 vis 공통)
        let s = "view:0[tiles:1,tree:0,flat:1]|refresh:1|panel:1[colsync:1,toggle:1,dock:1,info:1]|show:1[dot:1,hidden:1]|settings:1";
        assert_eq!(serialize_toolbar_order(&parse_toolbar_order(s)), s, "재배열/표시 보존");
        // 구형(vis 생략) 호환 + 미지 토큰 제거 + 누락 보충(정의 위치 삽입 —
        // 07-19: tree/flat이 정의상 tiles 앞이므로 앞에 들어간다)
        let s = "view[tiles,junk]|bogus|panel[toggle:0]";
        let norm = serialize_toolbar_order(&parse_toolbar_order(s));
        assert_eq!(
            norm,
            "view:1[tree:1,flat:1,tiles:1]|panel:1[toggle:0,dock:1,info:1,colsync:1]|refresh:1|show:1[hidden:1,dot:1]|settings:1"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_roundtrip_and_lenient_parse() {
        let s = Settings {
            theme: "light".into(),
            lang: "ko".into(),
            show_hidden: false,
            show_dotfiles: true,
            split: 0.62,
            dock: true,
            col_width_sync: false, // 기본 true — 왕복 검증 위해 반전
            col_autofit_max: 640,
            toolbar_order: "view:1[tiles:1,tree:1,flat:1]|panel:0[toggle:1,dock:1,info:0,colsync:1]|refresh:1|settings:1|show:1[hidden:1,dot:1]".into(),
            ctx_menu_order: "row:1[copyName:1,deletePermanent:0,pasteInto:1]|bg:0[paste:1,undo:1,redo:1]".into(),
            dock_ratio: 0.42,
            dock_split: 0.61,
            term_font: "D2Coding, JetBrainsMono Nerd Font".into(),
            term_font_size: 14,
            term_wrap: false,
            term_cols: 132,
            dlg_font: "맑은 고딕".into(),
            dlg_font_size: 10,
            base_font: "본고딕".into(),
            base_font_size: 13,
            ctx_font: "Segoe UI".into(),
            ctx_font_size: 12,
            status_font: "D2Coding".into(),
            status_font_size: 11,
            list_font: "맑은 고딕".into(),
            list_font_size: 14,
            list_folder_bold: true,
            header_bold: true,
            header_italic: false,
            sort_folders_first: false,
            sort_case_sensitive: true,
            nav_up_align: "top".into(),
            tab_dblclick: "lock".into(),
            typeahead_scope: "level".into(),
            typeahead_reset_ms: 700,
            typeahead_pos: 2,
            typeahead_special: false,
            typeahead_space: false,
            typeahead_backspace: false,
            view_mode: "tiles".into(),
            panel_mode: "single".into(),
            info_mode: "single".into(),
            launcher: false,
            launcher_items: Some(vec![
                LauncherItem {
                    label: "VS Code".into(),
                    exe: "C:\\Apps\\Code.exe".into(),
                    args: "\"%path%\"".into(),
                },
                LauncherItem {
                    label: "pwsh".into(),
                    exe: "pwsh.exe".into(),
                    // 인자 안 `|`는 마지막 필드라 보존(splitn 3)
                    args: "-NoExit -Command \"echo a|b\"".into(),
                },
            ]),
            launcher_seed: 2,
        };
        let parsed = Settings::parse(&s.serialize());
        assert_eq!(parsed.theme, "light");
        assert_eq!(parsed.lang, "ko");
        assert!(!parsed.show_hidden && parsed.show_dotfiles);
        assert!(parsed.dock, "도크 표시 왕복(M4-1)");
        assert!((parsed.dock_ratio - 0.42).abs() < 0.001, "도크 비율 왕복");
        assert!(
            (parsed.dock_split - 0.61).abs() < 0.001,
            "도크 분할 왕복(X-6)"
        );
        assert_eq!(
            parsed.term_font, "D2Coding, JetBrainsMono Nerd Font",
            "터미널 글꼴 체인 왕복(QA 07-14)"
        );
        assert_eq!(parsed.term_font_size, 14);
        assert!(!parsed.term_wrap, "터미널 줄 바꿈 왕복(X-3)");
        assert_eq!(parsed.term_cols, 132, "터미널 고정 열 왕복(X-3)");
        assert_eq!(parsed.col_autofit_max, 640, "auto-fit 최대 폭 왕복(07-19)");
        assert_eq!(
            parsed.toolbar_order,
            "view:1[tiles:1,tree:1,flat:1]|panel:0[toggle:1,dock:1,info:0,colsync:1]|refresh:1|settings:1|show:1[hidden:1,dot:1]",
            "도구모음 순서/표시 왕복(07-19)"
        );
        assert_eq!(
            parsed.ctx_menu_order,
            "row:1[copyName:1,deletePermanent:0,pasteInto:1]|bg:0[paste:1,undo:1,redo:1]",
            "컨텍스트 메뉴 순서/표시 왕복(07-19)"
        );
        assert_eq!(
            Settings::parse("term_cols=20").term_cols,
            80,
            "열 하한 클램프"
        );
        assert!(parsed.sort_case_sensitive, "대소문자 정렬 왕복");
        assert_eq!(parsed.nav_up_align, "top", "Alt+↑ 배치 왕복");
        assert_eq!(parsed.tab_dblclick, "lock", "탭 더블클릭 동작 왕복(07-15)");
        assert_eq!(parsed.view_mode, "tiles", "보기 모드 왕복(07-16)");
        assert!(!parsed.col_width_sync, "컬럼 동기화 왕복(07-18)");
        assert_eq!(parsed.panel_mode, "single", "패널 모드 왕복(07-16)");
        assert_eq!(parsed.info_mode, "single", "정보 모드 왕복(07-16)");
        assert_eq!(
            Settings::parse("panel_mode=triple").panel_mode,
            "dual",
            "미지 패널 모드 = 기본"
        );
        assert_eq!(
            Settings::parse("view_mode=grid").view_mode,
            "tree",
            "미지 보기 모드 = 기본"
        );
        assert_eq!(parsed.typeahead_scope, "level", "타입어헤드 범위 왕복");
        assert_eq!(parsed.typeahead_reset_ms, 700);
        assert_eq!(parsed.typeahead_pos, 2);
        assert!(!parsed.typeahead_special && !parsed.typeahead_space);
        assert!(!parsed.typeahead_backspace);
        assert_eq!(
            Settings::parse("nav_up_align=middle").nav_up_align,
            "center",
            "미지 값 = 기본"
        );
        assert_eq!(parsed.dlg_font, "맑은 고딕", "대화상자 글꼴 왕복");
        assert_eq!(parsed.dlg_font_size, 10);
        // 폰트 슬롯 왕복(X-12)
        assert_eq!(parsed.base_font, "본고딕");
        assert_eq!(parsed.base_font_size, 13);
        assert_eq!(parsed.status_font, "D2Coding");
        assert_eq!(parsed.list_font, "맑은 고딕");
        assert_eq!(parsed.list_font_size, 14);
        assert!(parsed.list_folder_bold && parsed.header_bold);
        assert!(!parsed.header_italic);
        assert_eq!(
            Settings::parse("base_font_size=99").base_font_size,
            32,
            "크기 클램프"
        );
        assert!((parsed.split - 0.62).abs() < 0.001);
        // 퀵 런처 왕복(M5-1) — 표시 플래그·항목(인자 안 | 보존)
        assert!(!parsed.launcher, "런처 바 표시 왕복");
        assert_eq!(parsed.launcher_items, s.launcher_items, "런처 항목 왕복");
        // 키 부재 = None(첫 실행 시드 대상) · count만 있고 항목 0 = 비움 확정
        assert_eq!(Settings::parse("").launcher_items, None);
        assert_eq!(
            Settings::parse("launcher_count=0").launcher_items,
            Some(vec![]),
            "비움은 시드 재주입 금지"
        );
        // 손상·미지 키·잘못된 값 → 기본값 유지
        let junk = Settings::parse("theme=neon\nsplit=abc\nnope=1\n# c\n\nshow_hidden=0");
        assert_eq!(junk.theme, "dark");
        assert_eq!(junk.split, 0.5);
        assert!(!junk.show_hidden);
        // split 클램프
        assert_eq!(Settings::parse("split=99").split, 0.9);
    }

    #[test]
    fn session_roundtrip_with_pipe_separator() {
        let s = Session {
            active_panel: 1,
            panels: [
                PanelSession {
                    tabs: vec![PathBuf::from("C:\\a"), PathBuf::from("D:\\b c\\d")],
                    active: 1,
                    // 탭0=펼침 없음(생략 직렬화)·탭1=2개 — F18 왕복(X-4)
                    expanded: vec![
                        vec![],
                        vec![
                            PathBuf::from("D:\\b c\\d\\sub"),
                            PathBuf::from("D:\\b c\\d\\한글"),
                        ],
                    ],
                    locked: vec![false, true], // 탭1 잠금 — 편의 UX ② 왕복
                    pinned: vec![true, false], // 탭0 고정 — 07-15 왕복
                    modes: vec!["tiles".into(), "tree".into()], // 탭별 보기 모드 — 07-16 왕복
                    col_widths: vec![320, 64, 96], // 패널 컬럼 폭 — 07-18 왕복
                    col_layout: "cols:1[ext:1,name:1,size:0,modified:1,kind:1]".into(),
                },
                PanelSession {
                    tabs: vec![PathBuf::from("C:\\")],
                    active: 0,
                    expanded: vec![],
                    locked: vec![],
                    pinned: vec![],
                    modes: vec![],
                    col_widths: vec![], // 빈 목록 = 생략 직렬화
                    col_layout: String::new(),
                },
            ],
        };
        let parsed = Session::parse(&s.serialize());
        assert_eq!(parsed.active_panel, s.active_panel);
        assert_eq!(parsed.panels[0].tabs, s.panels[0].tabs);
        assert_eq!(parsed.panels[1], s.panels[1]);
        // 빈 목록 생략 직렬화 → 파싱은 인덱스 정렬 유지(탭0 빈 자리)
        assert_eq!(parsed.panels[0].expanded.len(), 2);
        assert!(parsed.panels[0].expanded[0].is_empty());
        assert_eq!(parsed.panels[0].expanded[1], s.panels[0].expanded[1]);
        assert_eq!(parsed.panels[0].locked, vec![false, true], "잠금 왕복");
        assert_eq!(
            parsed.panels[0].pinned,
            vec![true, false],
            "고정 왕복(07-15)"
        );
        assert_eq!(
            parsed.panels[0].modes,
            vec!["tiles".to_string(), "tree".to_string()],
            "탭별 보기 모드 왕복(07-16)"
        );
        // 빈/손상 → 기본
        let empty = Session::parse("");
        assert_eq!(empty.active_panel, 0);
        assert!(empty.panels[0].tabs.is_empty());
    }

    #[test]
    fn choose_data_dir_portable_first_installed_fallback() {
        // 쓰기 가능 후보 = 그대로(포터블 — DR-3 기본)
        let ok = std::env::temp_dir().join(format!("nexa_dd_{}", std::process::id()));
        let _ = fs::remove_dir_all(&ok);
        assert_eq!(choose_data_dir(ok.clone()), ok);
        let _ = fs::remove_dir_all(&ok);
        // 쓰기 불가(파일을 부모로 둔 불가능 경로) = LOCALAPPDATA 폴백(설치형)
        let blocker = std::env::temp_dir().join(format!("nexa_ddf_{}", std::process::id()));
        fs::write(&blocker, b"x").unwrap();
        let bad = blocker.join("data");
        let picked = choose_data_dir(bad.clone());
        match std::env::var_os("LOCALAPPDATA") {
            Some(_) => assert!(
                picked.ends_with(Path::new("NexaDir").join("data")),
                "설치형 폴백: {picked:?}"
            ),
            None => assert_eq!(picked, bad, "LOCALAPPDATA 부재 = 후보 유지"),
        }
        fs::remove_file(&blocker).unwrap();
    }

    #[test]
    fn save_is_atomic_and_load_roundtrips() {
        let dir = std::env::temp_dir().join(format!("nexa_cfg_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        save(&dir, "t.txt", "hello=1\n").unwrap();
        assert_eq!(load(&dir, "t.txt").unwrap(), "hello=1\n");
        save(&dir, "t.txt", "hello=2\n").unwrap(); // 덮어쓰기(기존 존재)
        assert_eq!(load(&dir, "t.txt").unwrap(), "hello=2\n");
        assert!(!dir.join("t.txt.tmp").exists(), "임시 파일 잔존 없음");
        assert_eq!(load(&dir, "missing.txt"), None);
        fs::remove_dir_all(&dir).unwrap();
    }
}
