//! Nexa Dir — 앱 진입점.
//! Windows 전용 UI는 `#[cfg(windows)]`로 격리한다(설계: docs/01 §1·docs/11).

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg_attr(not(windows), allow(dead_code))]
mod config;
#[cfg(windows)]
mod dw;
#[cfg_attr(not(windows), allow(dead_code))]
mod i18n;
// 비-Windows에선 창이 없어 미사용이지만 순수 로직이라 테스트는 전 플랫폼 실행
#[cfg(windows)]
mod bulkrename;
#[cfg(windows)]
mod clipboard;
#[cfg(windows)]
mod conpty;
/// Win32 커스텀 컨트롤 라이브러리(사용자 요청 07-16 — searchbox 등).
#[cfg(windows)]
mod ctl;
/// ctl UI 검증 갤러리(개발 전용 — WM_APP_CTLDEMO 주입으로만 연다).
#[cfg(windows)]
mod ctldemo;
#[cfg(windows)]
mod dialog;
#[cfg(windows)]
mod dnd;
#[cfg(windows)]
mod icon;
#[cfg_attr(not(windows), allow(dead_code))]
mod icons;
#[cfg(windows)]
mod launcher;
#[cfg_attr(not(windows), allow(dead_code))]
mod nav;
#[cfg_attr(not(windows), allow(dead_code))]
mod panel;
#[cfg_attr(not(windows), allow(dead_code))]
mod pathinput;
#[cfg(windows)]
mod prefs;
#[cfg(windows)]
mod recycle;
#[cfg(windows)]
mod shellmenu;
/// shell: 특수 폴더 스킴 해석(사용자 요청 07-17 — SHParseDisplayName 위임).
#[cfg(windows)]
mod shellpath;
#[cfg_attr(not(windows), allow(dead_code))]
mod source;
#[cfg(windows)]
mod ordereditor;
mod svg;
#[cfg(windows)]
mod tip;
#[cfg(windows)]
mod uia;
#[cfg(windows)]
mod watcher;
#[cfg(windows)]
mod win;

#[cfg(windows)]
fn main() {
    if let Err(e) = win::run() {
        eprintln!("nexa-app 실행 실패: {e}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    println!(
        "nexa-app {}은 Windows 전용 UI입니다. 이 OS에서의 검증: cargo check --target x86_64-pc-windows-msvc (docs/11)",
        env!("CARGO_PKG_VERSION")
    );
}
