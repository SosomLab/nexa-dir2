//! 설정/세션 영속(M2-5) — 원본 docs/34·40·43 차용, 포터블 규율(DR-3):
//! 영속물 = **exe 옆 `data\`**(레지스트리·%APPDATA% 비의존). 외부 crate 0 유지를 위해
//! 단순 `key=value` 텍스트(UTF-8·한 줄 1키·`#` 주석). 쓰기는 원자적(temp → rename).
//! 주기 저장·코얼레싱(원본 SESS 규율)은 후속 — 초안은 기동 로드/종료 저장.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

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
    /// 대화상자(확인창·진행 창) 글꼴/크기(pt — QA 07-14 "대화창용 폰트 설정").
    pub dlg_font: String,
    pub dlg_font_size: i32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "dark".into(), // DR-5 다크 기본
            lang: "system".into(),
            show_hidden: true,
            show_dotfiles: true,
            split: 0.5,
            dock: false,
            dock_ratio: 0.3,
            dock_split: 0.5,
            term_font: "Consolas".into(),
            term_font_size: 12,
            dlg_font: "Segoe UI".into(),
            dlg_font_size: 9,
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
}

/// 세션(원본 session.json 대응) — `data\session.cfg`(패널·탭·활성·펼침).
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Session {
    pub active_panel: usize,
    pub panels: [PanelSession; 2],
}

/// exe 옆 `data\` (실패 시 커런트 디렉터리 기준 — 테스트/특수 환경 폴백).
pub fn data_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("data")
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
        format!(
            "# nexa-dir2 settings v1\ntheme={}\nlang={}\nshow_hidden={}\nshow_dotfiles={}\nsplit={:.3}\ndock={}\ndock_ratio={:.3}\ndock_split={:.3}\nterm_font={}\nterm_font_size={}\ndlg_font={}\ndlg_font_size={}\n",
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
        )
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
                "dlg_font" if !v.trim().is_empty() && v.len() <= 64 => {
                    s.dlg_font = v.trim().into()
                }
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
                _ => {}
            }
        }
        s
    }
}

impl Session {
    /// 탭 경로 구분자 `|` — Windows 경로에 등장 불가 문자.
    pub fn serialize(&self) -> String {
        let mut out = String::from("# nexa-dir2 session v1\n");
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
            dock_ratio: 0.42,
            dock_split: 0.61,
            term_font: "D2Coding, JetBrainsMono Nerd Font".into(),
            term_font_size: 14,
            dlg_font: "맑은 고딕".into(),
            dlg_font_size: 10,
        };
        let parsed = Settings::parse(&s.serialize());
        assert_eq!(parsed.theme, "light");
        assert_eq!(parsed.lang, "ko");
        assert!(!parsed.show_hidden && parsed.show_dotfiles);
        assert!(parsed.dock, "도크 표시 왕복(M4-1)");
        assert!((parsed.dock_ratio - 0.42).abs() < 0.001, "도크 비율 왕복");
        assert!((parsed.dock_split - 0.61).abs() < 0.001, "도크 분할 왕복(X-6)");
        assert_eq!(
            parsed.term_font, "D2Coding, JetBrainsMono Nerd Font",
            "터미널 글꼴 체인 왕복(QA 07-14)"
        );
        assert_eq!(parsed.term_font_size, 14);
        assert_eq!(parsed.dlg_font, "맑은 고딕", "대화상자 글꼴 왕복");
        assert_eq!(parsed.dlg_font_size, 10);
        assert!((parsed.split - 0.62).abs() < 0.001);
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
                },
                PanelSession {
                    tabs: vec![PathBuf::from("C:\\")],
                    active: 0,
                    expanded: vec![],
                    locked: vec![],
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
        // 빈/손상 → 기본
        let empty = Session::parse("");
        assert_eq!(empty.active_panel, 0);
        assert!(empty.panels[0].tabs.is_empty());
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
