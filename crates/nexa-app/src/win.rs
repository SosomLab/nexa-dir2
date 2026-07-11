//! Win32 창·메시지 루프 스켈레톤 (M0-5).
//! 창 1개 + `WM_PAINT` 커스텀 드로잉 골격 — 렌더 스파이크(M0-7)와 nexa-gui(M1)의 기반.
//! 프레임워크·컴포지션 계층 없음: 예산 B1(RSS)·B3(임포트=OS 인박스만)의 전제(ADR-0001).

use windows::core::{w, Result};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, DrawTextW, EndPaint, FillRect, GetSysColorBrush, SetBkMode, COLOR_WINDOW,
    DT_CENTER, DT_SINGLELINE, DT_VCENTER, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetMessageW, LoadCursorW,
    PostQuitMessage, RegisterClassW, TranslateMessage, CW_USEDEFAULT, IDC_ARROW, MSG,
    WM_DESTROY, WM_PAINT, WNDCLASSW, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

pub fn run() -> Result<()> {
    unsafe {
        // PerMonitorV2 DPI — 매니페스트 도입 전까지 코드로 선언(docs/01 §3)
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let hinstance = GetModuleHandleW(None)?;
        let class_name = w!("NexaDir2Main");
        let wc = WNDCLASSW {
            hInstance: hinstance.into(),
            lpszClassName: class_name,
            lpfnWndProc: Some(wndproc),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        let atom = RegisterClassW(&wc);
        debug_assert_ne!(atom, 0, "RegisterClassW 실패");

        CreateWindowExW(
            Default::default(),
            class_name,
            w!("Nexa Dir 2"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1200,
            800,
            None,
            None,
            Some(hinstance.into()),
            None,
        )?;

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            FillRect(hdc, &rc, GetSysColorBrush(COLOR_WINDOW));
            SetBkMode(hdc, TRANSPARENT);
            let mut text: Vec<u16> = "Nexa Dir 2 — M0 Win32 스켈레톤".encode_utf16().collect();
            DrawTextW(hdc, &mut text, &mut rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
