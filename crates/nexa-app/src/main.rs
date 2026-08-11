//! Nexa Dir — 앱 진입점.
//! Windows 전용 UI는 `#[cfg(windows)]`로 격리한다(설계: docs/01 §1·docs/11).

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(windows)]
mod about;
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
/// 클라우드 동기화 클라이언트 탐지(X-36 — 순수 파서는 전 플랫폼 테스트).
#[cfg_attr(not(windows), allow(dead_code))]
mod cloud;
/// 클라우드 OAuth2 PKCE 인증(X-37 — ADR-0006. 순수 로직은 전 플랫폼 테스트).
#[cfg_attr(not(windows), allow(dead_code))]
mod oauth;
/// 토큰 DPAPI 보관(X-37 — ADR-0006).
#[cfg_attr(not(windows), allow(dead_code))]
mod secret;
/// 클라우드 API 탐색 캐시·워커(X-37 2차 — ADR-0006 §3).
/// **Windows 전용** — 진행 창(`dialog`)·통지(`win`)와 직접 맞물려 있어 중립화 실익이
/// 없다(08-02 CI: 비Windows 잡이 `crate::win`/`dialog` 미해석으로 실패했다).
/// 소비자도 `win.rs` 하나뿐이라 `ctl`/`watcher`와 같은 게이팅으로 맞춘다.
#[cfg(windows)]
mod cloudfs;
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
/// 폴더 변경 저비용 프로브(08-11 QA — watcher 통지가 오지 않는 클라우드·네트워크
/// 경로용 폴백 서명). `std::fs`만 쓰므로 전 플랫폼 컴파일·테스트 대상이다.
#[cfg_attr(not(windows), allow(dead_code))]
mod fsprobe;
#[cfg(windows)]
mod icon;
#[cfg_attr(not(windows), allow(dead_code))]
mod icons;
#[cfg(windows)]
mod launcher;
#[cfg_attr(not(windows), allow(dead_code))]
mod nav;
#[cfg(windows)]
mod ordereditor;
#[cfg_attr(not(windows), allow(dead_code))]
mod panel;
#[cfg_attr(not(windows), allow(dead_code))]
mod pathinput;
#[cfg(windows)]
mod prefs;
/// 미리보기 공급자 시임(ADR-0004 S1~S3 — X-2)
#[cfg_attr(not(windows), allow(dead_code))]
mod preview;
/// 독립 미리보기 창(07-26 — 플러그인 기준 캔버스)
#[cfg(windows)]
mod previewwnd;
#[cfg(windows)]
mod recycle;
#[cfg(windows)]
mod shellmenu;
/// shell: 특수 폴더 스킴 해석(사용자 요청 07-17 — SHParseDisplayName 위임).
#[cfg(windows)]
mod shellpath;
#[cfg_attr(not(windows), allow(dead_code))]
mod source;
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
