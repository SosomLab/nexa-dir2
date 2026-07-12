//! 설정/세션 영속(M2-5) — 원본 docs/34·40·43 차용, 포터블 규율(DR-3):
//! 영속물 = **exe 옆 `data\`**(레지스트리·%APPDATA% 비의존). 외부 crate 0 유지를 위해
//! 단순 `key=value` 텍스트(UTF-8·한 줄 1키·`#` 주석). 쓰기는 원자적(temp → rename).
//! 주기 저장·코얼레싱(원본 SESS 규율)은 후속 — 초안은 기동 로드/종료 저장.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// 설정(원본 ViewOptions·ThemeOptions 대응) — `data\settings.txt`.
#[derive(Clone, PartialEq, Debug)]
pub struct Settings {
    /// "system" | "light" | "dark"
    pub theme: String,
    pub show_hidden: bool,
    pub show_dotfiles: bool,
    /// 좌 패널 폭 비율.
    pub split: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "dark".into(), // DR-5 다크 기본
            show_hidden: true,
            show_dotfiles: true,
            split: 0.5,
        }
    }
}

/// 패널 1개의 세션(탭 경로들·활성 탭).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct PanelSession {
    pub tabs: Vec<PathBuf>,
    pub active: usize,
}

/// 세션(원본 session.json 대응) — `data\session.txt`.
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
            "# nexa-dir2 settings v1\ntheme={}\nshow_hidden={}\nshow_dotfiles={}\nsplit={:.3}\n",
            self.theme,
            u8::from(self.show_hidden),
            u8::from(self.show_dotfiles),
            self.split,
        )
    }

    /// 손상·미지 키는 무시하고 기본값 위에 덮어쓴다(관용 파싱).
    pub fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        for (k, v) in kv_lines(text) {
            match k {
                "theme" if matches!(v, "system" | "light" | "dark") => s.theme = v.into(),
                "show_hidden" => s.show_hidden = v != "0",
                "show_dotfiles" => s.show_dotfiles = v != "0",
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

pub const SETTINGS_FILE: &str = "settings.txt";
pub const SESSION_FILE: &str = "session.txt";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_roundtrip_and_lenient_parse() {
        let s = Settings {
            theme: "light".into(),
            show_hidden: false,
            show_dotfiles: true,
            split: 0.62,
        };
        let parsed = Settings::parse(&s.serialize());
        assert_eq!(parsed.theme, "light");
        assert!(!parsed.show_hidden && parsed.show_dotfiles);
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
                },
                PanelSession {
                    tabs: vec![PathBuf::from("C:\\")],
                    active: 0,
                },
            ],
        };
        let parsed = Session::parse(&s.serialize());
        assert_eq!(parsed, s);
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
