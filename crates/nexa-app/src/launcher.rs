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

/// 첫 실행 시드(원본 DefaultLauncherTools) — VS Code 발견 시 1종, 없으면 빈 목록.
pub fn seed() -> Vec<LauncherItem> {
    resolve_vscode()
        .map(|exe| {
            vec![LauncherItem {
                label: "VS Code".into(),
                exe,
                args: "\"%path%\"".into(),
            }]
        })
        .unwrap_or_default()
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
