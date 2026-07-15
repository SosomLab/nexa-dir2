//! 퀵 런처(M5-1) — 원본 docs/44 §런처·ToolLauncher.cs 이식: 사용자 정의 외부 프로그램
//! 버튼 바. 실행 = 인자 템플릿의 `%path%`를 **활성 패널 현재 폴더**로 치환 후
//! ShellExecuteW(작업 디렉터리 = 그 폴더). 실패는 예외 없이 `false` → 상태바 안내
//! (앱 무중단 — 원본 오류 격리 규약).
//!
//! 항목 영속 = settings.cfg `launcherN=라벨|exe|인자`(config.rs — 원본 `Launcher.Items`
//! 설계 대응). 첫 실행(키 부재) 시드 = VS Code(발견 시 1종 — 원본 슬라이스 1 동일).
//! UI CRUD·항목별 단축키·exe 아이콘 추출은 후속(α — 원본도 시드+실행만 구현).

use std::path::Path;

use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::config::LauncherItem;

/// VS Code 실행 파일 탐색(원본 ToolLauncher.ResolveVsCode — 후보 3경로 순회).
fn resolve_vscode() -> Option<String> {
    let candidates = [
        std::env::var("LOCALAPPDATA")
            .map(|p| format!("{p}\\Programs\\Microsoft VS Code\\Code.exe")),
        std::env::var("ProgramFiles").map(|p| format!("{p}\\Microsoft VS Code\\Code.exe")),
        std::env::var("ProgramFiles(x86)").map(|p| format!("{p}\\Microsoft VS Code\\Code.exe")),
    ];
    candidates
        .into_iter()
        .flatten()
        .find(|p| Path::new(p).is_file())
}

/// 시드 버전(사용자 요청 07-15: cmd·pwsh 추가 = v2) — settings `launcher_seed`가 이보다
/// 낮으면 기동 시 누락분만 1회 추가([`seed_missing`] — 사용자 편집 항목은 보존).
pub const SEED_VERSION: u32 = 2;

/// PATH에서 실행 파일 전체 경로 탐색(아이콘 추출에 실경로 필요 — 셸 아이콘 키).
fn resolve_on_path(name: &str) -> Option<String> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|d| d.join(name))
            .find(|p| p.is_file())
            .map(|p| p.to_string_lossy().into_owned())
    })
}

/// pwsh(PowerShell 7+) 우선, 없으면 Windows PowerShell 폴백(원본 OpenTerminal 순서).
fn resolve_pwsh() -> Option<(String, String)> {
    if let Some(p) = resolve_on_path("pwsh.exe") {
        return Some(("pwsh".into(), p));
    }
    std::env::var("SystemRoot")
        .map(|r| format!("{r}\\System32\\WindowsPowerShell\\v1.0\\powershell.exe"))
        .ok()
        .filter(|p| Path::new(p).is_file())
        .or_else(|| resolve_on_path("powershell.exe"))
        .map(|p| ("PowerShell".into(), p))
}

/// cmd — %ComSpec% 우선(항상 존재하는 인박스 셸).
fn resolve_cmd() -> Option<String> {
    std::env::var("ComSpec")
        .ok()
        .filter(|p| Path::new(p).is_file())
        .or_else(|| resolve_on_path("cmd.exe"))
}

/// 셸 시드 2종(pwsh/PowerShell·cmd — 사용자 요청 07-15). 작업 디렉터리 = 활성 폴더
/// ([`launch`]가 지정)이므로 인자 불요 — 그 폴더에서 셸이 열린다.
fn shell_items() -> Vec<LauncherItem> {
    let mut out = Vec::new();
    if let Some((label, exe)) = resolve_pwsh() {
        out.push(LauncherItem {
            label,
            exe,
            args: String::new(),
        });
    }
    if let Some(exe) = resolve_cmd() {
        out.push(LauncherItem {
            label: "cmd".into(),
            exe,
            args: String::new(),
        });
    }
    out
}

/// 첫 실행 시드(v2) — [VS Code] │ [pwsh]·[cmd](발견분만·구분선으로 그룹).
pub fn seed() -> Vec<LauncherItem> {
    let mut out: Vec<LauncherItem> = resolve_vscode()
        .map(|exe| {
            vec![LauncherItem {
                label: "VS Code".into(),
                exe,
                args: "\"%path%\"".into(),
            }]
        })
        .unwrap_or_default();
    let shells = shell_items();
    if !shells.is_empty() {
        if !out.is_empty() {
            out.push(LauncherItem::separator());
        }
        out.extend(shells);
    }
    out
}

/// 구버전 시드 마이그레이션(1회 — `launcher_seed` < [`SEED_VERSION`]): pwsh/powershell·
/// cmd 항목이 없으면 뒤에 추가(그룹 구분선 동반). 사용자 항목·비움 편집은 보존.
pub fn seed_missing(items: &mut Vec<LauncherItem>) {
    let has = |needle: &str| {
        items
            .iter()
            .any(|i| !i.is_separator() && i.exe.to_lowercase().contains(needle))
    };
    let mut add: Vec<LauncherItem> = Vec::new();
    if !has("pwsh") && !has("powershell") {
        if let Some((label, exe)) = resolve_pwsh() {
            add.push(LauncherItem {
                label,
                exe,
                args: String::new(),
            });
        }
    }
    if !has("cmd.exe") {
        if let Some(exe) = resolve_cmd() {
            add.push(LauncherItem {
                label: "cmd".into(),
                exe,
                args: String::new(),
            });
        }
    }
    if !add.is_empty() {
        if !items.is_empty() && !items.last().is_some_and(LauncherItem::is_separator) {
            items.push(LauncherItem::separator());
        }
        items.extend(add);
    }
}

/// 항목 실행 — `%path%` 치환 + ShellExecuteW(UseShellExecute 대응). 성공 여부 반환.
pub unsafe fn launch(hwnd: HWND, item: &LauncherItem, folder: &Path) -> bool {
    let dir = folder.to_string_lossy();
    let args = item.args.replace("%path%", &dir);
    let exe = HSTRING::from(item.exe.as_str());
    let params = HSTRING::from(args.as_str());
    let workdir = HSTRING::from(dir.as_ref());
    let params_ptr = if args.is_empty() {
        PCWSTR::null()
    } else {
        PCWSTR(params.as_ptr())
    };
    let h = ShellExecuteW(
        Some(hwnd),
        w!("open"),
        PCWSTR(exe.as_ptr()),
        params_ptr,
        PCWSTR(workdir.as_ptr()),
        SW_SHOWNORMAL,
    );
    // ShellExecuteW 규약: 반환값 > 32 = 성공
    h.0 as usize > 32
}
