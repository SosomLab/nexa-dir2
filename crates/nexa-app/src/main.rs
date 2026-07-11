//! Nexa Dir 2 — 앱 진입점.
//! Windows 전용 UI는 `#[cfg(windows)]`로 격리한다(설계: docs/01 §1·docs/11).

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

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
