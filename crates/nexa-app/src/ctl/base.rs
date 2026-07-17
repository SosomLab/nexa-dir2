//! base — Nx 컨트롤 **공통 생명주기 헬퍼**(재사용 계층 — 07-18 리팩터).
//!
//! 모든 Nx 컨트롤이 문자 그대로 반복하던 4대 패턴을 한곳에 모은다
//! (판매용 추상화 계약 [mod.rs §판매용 추상화 규약]의 실행 기반):
//! - **상태 박스**: `Box<State>` ↔ `GWLP_USERDATA`(생성 설치·파괴 회수).
//! - **통지**: 부모에 `WM_COMMAND(MAKEWPARAM(id, code))` 재발행(lparam = 컨트롤).
//! - **클래스 등록**: `Once` 가드 + 화살표 커서 창 클래스 1회 등록.
//!
//! GDI 자원(HFONT·HBRUSH 등)을 보유하는 상태는 `Drop`을 구현해 [`drop_state`]가
//! 박스 회수 시 함께 해제되게 한다(RAII — WM_DESTROY 수동 해제 누락 방지).

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    GetDlgCtrlID, GetParent, GetWindowLongPtrW, LoadCursorW, RegisterClassW, SendMessageW,
    SetWindowLongPtrW, GWLP_USERDATA, IDC_ARROW, WM_COMMAND, WNDCLASSW, WNDPROC,
};

/// `GWLP_USERDATA`에서 상태 박스 포인터를 읽는다(미설치 = 널).
///
/// # Safety
/// `hwnd`가 [`attach_state`]로 `T`를 설치한 Nx 컨트롤이어야 한다(타입 일치).
pub(crate) unsafe fn state<T>(hwnd: HWND) -> *mut T {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut T
}

/// 상태 박스를 `GWLP_USERDATA`에 설치(생성 시 1회 — 소유권 이전).
///
/// # Safety
/// 같은 `hwnd`에 두 번 설치하면 이전 박스가 누수된다(생성 경로에서만 호출).
pub(crate) unsafe fn attach_state<T>(hwnd: HWND, st: Box<T>) {
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(st) as isize);
}

/// WM_DESTROY 표준 처리 — USERDATA를 비우고 상태 박스를 회수(Drop 실행).
///
/// # Safety
/// `T`는 [`attach_state`]로 설치한 실제 상태 타입과 일치해야 한다.
pub(crate) unsafe fn drop_state<T>(hwnd: HWND) {
    let p = state::<T>(hwnd);
    if !p.is_null() {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        drop(Box::from_raw(p));
    }
}

/// 부모에 `WM_COMMAND(MAKEWPARAM(id, code))` 재발행(통지 규약 — lparam = 컨트롤).
/// 부모가 없으면 무시.
///
/// # Safety
/// 유효한 자식 컨트롤 `hwnd`에서 호출(GetParent/GetDlgCtrlID 전제).
pub(crate) unsafe fn notify(hwnd: HWND, code: u32) {
    if let Ok(parent) = GetParent(hwnd) {
        let id = GetDlgCtrlID(hwnd) as u32;
        SendMessageW(
            parent,
            WM_COMMAND,
            Some(WPARAM(((code as usize) << 16) | id as usize)),
            Some(LPARAM(hwnd.0 as isize)),
        );
    }
}

/// 창 클래스 1회 등록(공통 커서 = 화살표). `once`는 컨트롤별 정적 [`std::sync::Once`].
///
/// # Safety
/// `proc`는 유효한 윈도우 프로시저, `class`는 정적 수명 문자열이어야 한다.
pub(crate) unsafe fn register_class(once: &std::sync::Once, class: PCWSTR, proc: WNDPROC) {
    once.call_once(|| {
        let wc = WNDCLASSW {
            lpfnWndProc: proc,
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassW(&wc);
    });
}
