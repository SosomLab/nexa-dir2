//! Nexa Dir 2 — 앱 진입점.
//! Windows 전용 UI는 `#[cfg(windows)]`로 격리한다(설계: docs/01 §1·docs/11).

fn main() {
    println!("nexa-app {} — M0 스캐폴딩 (Win32 창은 M0-5)", env!("CARGO_PKG_VERSION"));
}
