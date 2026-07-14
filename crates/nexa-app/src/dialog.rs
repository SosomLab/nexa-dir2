//! 커스텀 대화상자 컨트롤(QA 07-14 — "User Control"): **임의 버튼/메시지 모달**(원본
//! 전송 확인창 4버튼 대응) + **전송 진행 창**(프로그레스 바·취소). user32/gdi32만 사용
//! (comctl32 비의존 — B3 임포트 게이트 유지, M3-4 회피 규약 계승).
//!
//! - [`show_buttons`]: 메시지 + 호출자 정의 버튼 목록 → 클릭한 버튼 id(닫힘/Esc=0).
//!   자체 모달 루프 — 워커 스레드에서도 사용 가능(MessageBox 대체).
//! - [`Progress`]: 비모달 진행 창 — 바(직접 페인트)·백분율 텍스트·[취소] 버튼.
//!   UI 스레드 소유, 진행 값은 호출자가 [`Progress::update`]로 밀어 넣는다.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect, GetDC,
    GetStockObject, ReleaseDC, SelectObject, SetBkMode, SetTextColor, DEFAULT_GUI_FONT,
    DT_CALCRECT, DT_LEFT, DT_WORDBREAK, HBRUSH, HDC, HFONT, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetMessageW,
    GetWindowLongPtrW, GetWindowRect, IsWindow, RegisterClassW, SendMessageW, SetForegroundWindow,
    SetWindowLongPtrW, TranslateMessage, BS_DEFPUSHBUTTON, BS_PUSHBUTTON, GWLP_USERDATA, MSG,
    SW_SHOWNORMAL, WINDOW_EX_STYLE, WM_CLOSE, WM_COMMAND, WM_KEYDOWN, WM_PAINT, WM_SETFONT,
    WNDCLASSW, WS_CAPTION, WS_CHILD, WS_POPUP, WS_SYSMENU, WS_VISIBLE,
};

/// 대화상자 버튼(호출자 정의) — `id`는 1 이상(0=닫힘/Esc 예약).
pub struct DlgButton {
    pub id: u32,
    pub label: String,
}

const BTN_W: i32 = 104;
const BTN_H: i32 = 28;
const PAD: i32 = 14;
const DLG_W: i32 = 460;

/// 모달 상태(GWLP_USERDATA) — wndproc가 기록, 모달 루프가 읽는다.
struct DlgState {
    result: u32,
    /// 진행 창 전용: 취소 플래그(Arc 원시 — [`Progress`]가 소유·해제).
    text: Vec<u16>,
}

static REGISTER: std::sync::Once = std::sync::Once::new();
const CLASS: PCWSTR = w!("NexaDlg");

unsafe fn ensure_class() {
    REGISTER.call_once(|| {
        let wc = WNDCLASSW {
            lpszClassName: CLASS,
            lpfnWndProc: Some(dlg_proc),
            hInstance: windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
                .unwrap_or_default()
                .into(),
            hbrBackground: HBRUSH(
                (windows::Win32::Graphics::Gdi::COLOR_BTNFACE.0 + 1) as isize
                    as *mut core::ffi::c_void,
            ),
            hCursor: windows::Win32::UI::WindowsAndMessaging::LoadCursorW(
                None,
                windows::Win32::UI::WindowsAndMessaging::IDC_ARROW,
            )
            .unwrap_or_default(),
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);
    });
}

unsafe extern "system" fn dlg_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut DlgState;
    match msg {
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as u32;
            if !state.is_null() && id > 0 {
                (*state).result = id;
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wparam.0 as u16 == windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE.0 {
                let _ = DestroyWindow(hwnd); // Esc = 0(취소 예약)
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_PAINT => {
            // 메시지 본문(멀티라인 워드랩) — STATIC 대신 직접 그려 폰트/여백 일관
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            if !state.is_null() {
                let font = HFONT(GetStockObject(DEFAULT_GUI_FONT).0);
                let old = SelectObject(hdc, font.into());
                SetBkMode(hdc, TRANSPARENT);
                SetTextColor(hdc, COLORREF(0x00000000));
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                rc.left += PAD;
                rc.top += PAD;
                rc.right -= PAD;
                rc.bottom -= PAD + BTN_H + PAD;
                let mut text = (*state).text.clone();
                DrawTextW(hdc, &mut text, &mut rc, DT_LEFT | DT_WORDBREAK);
                SelectObject(hdc, old);
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// 메시지 높이 측정(DEFAULT_GUI_FONT·워드랩) — 창 크기 산정.
unsafe fn measure_text(text: &str, width: i32) -> i32 {
    let hdc: HDC = GetDC(None);
    let font = HFONT(GetStockObject(DEFAULT_GUI_FONT).0);
    let old = SelectObject(hdc, font.into());
    let mut buf: Vec<u16> = text.encode_utf16().collect();
    let mut rc = RECT {
        right: width,
        ..Default::default()
    };
    DrawTextW(hdc, &mut buf, &mut rc, DT_CALCRECT | DT_LEFT | DT_WORDBREAK);
    SelectObject(hdc, old);
    ReleaseDC(None, hdc);
    (rc.bottom - rc.top).max(20)
}

/// 임의 버튼 모달 대화상자 — 클릭한 버튼 id(닫힘/Esc=0). 어느 스레드든 호출 가능
/// (자체 메시지 루프 — MessageBox 동등). 첫 버튼 = 기본(Enter).
pub unsafe fn show_buttons(owner: HWND, title: &str, message: &str, buttons: &[DlgButton]) -> u32 {
    ensure_class();
    let text_h = measure_text(message, DLG_W - PAD * 2);
    let h = PAD + text_h + PAD + BTN_H + PAD + 32; // +캡션 근사
    let (cx, cy) = center_over(owner, DLG_W, h);
    let mut state = Box::new(DlgState {
        result: 0,
        text: message.encode_utf16().collect(),
    });
    let title_w = windows::core::HSTRING::from(title);
    let Ok(dlg) = CreateWindowExW(
        WINDOW_EX_STYLE(0x00000008 | 0x00000001), // TOPMOST | DLGMODALFRAME
        CLASS,
        PCWSTR(title_w.as_ptr()),
        WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
        cx,
        cy,
        DLG_W,
        h,
        Some(owner),
        None,
        None,
        None,
    ) else {
        return 0;
    };
    SetWindowLongPtrW(dlg, GWLP_USERDATA, &mut *state as *mut DlgState as isize);
    // 버튼(우측 정렬·역순 배치 — 첫 버튼이 가장 왼쪽이 아니라 관례대로 왼→오 순서 유지)
    let font = HFONT(GetStockObject(DEFAULT_GUI_FONT).0);
    let total = buttons.len() as i32 * (BTN_W + 8) - 8;
    let mut x = DLG_W - PAD - total - 8; // -8: 프레임 근사
    let by = PAD + text_h + PAD;
    for (i, b) in buttons.iter().enumerate() {
        let style = if i == 0 {
            BS_DEFPUSHBUTTON
        } else {
            BS_PUSHBUTTON
        };
        let label = windows::core::HSTRING::from(&*b.label);
        if let Ok(btn) = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("BUTTON"),
            PCWSTR(label.as_ptr()),
            WS_CHILD
                | WS_VISIBLE
                | windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(style as u32),
            x,
            by,
            BTN_W,
            BTN_H,
            Some(dlg),
            Some(windows::Win32::UI::WindowsAndMessaging::HMENU(
                b.id as usize as *mut core::ffi::c_void,
            )),
            None,
            None,
        ) {
            SendMessageW(
                btn,
                WM_SETFONT,
                Some(WPARAM(font.0 as usize)),
                Some(LPARAM(1)),
            );
        }
        x += BTN_W + 8;
    }
    let _ = EnableWindow(owner, false); // 모달(소유자 입력 차단)
    let _ = SetForegroundWindow(dlg);
    let mut msg = MSG::default();
    while IsWindow(Some(dlg)).as_bool() && GetMessageW(&mut msg, None, 0, 0).as_bool() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    let _ = EnableWindow(owner, true);
    let _ = SetForegroundWindow(owner);
    state.result
}

/// 소유자 중앙 좌표.
unsafe fn center_over(owner: HWND, w: i32, h: i32) -> (i32, i32) {
    let mut rc = RECT::default();
    if GetWindowRect(owner, &mut rc).is_ok() {
        (
            rc.left + ((rc.right - rc.left) - w) / 2,
            rc.top + ((rc.bottom - rc.top) - h) / 2,
        )
    } else {
        (200, 200)
    }
}

// ── 전송 진행 창(비모달 — UI 스레드 소유) ────────────────────────

/// 진행 창 wndproc용 공유 상태(GWLP_USERDATA — [`Progress`]가 Box 소유).
struct ProgState {
    done: u64,
    total: u64,
    label: Vec<u16>,
    /// 취소 요청 — [취소] 버튼이 세팅, 호출자가 폴링([`Progress::cancelled`]).
    cancelled: bool,
}

static REGISTER_PROG: std::sync::Once = std::sync::Once::new();
const CLASS_PROG: PCWSTR = w!("NexaProgress");
const PROG_W: i32 = 420;
const PROG_H: i32 = 150;
const ID_CANCEL: u32 = 1;

unsafe extern "system" fn prog_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ProgState;
    match msg {
        WM_COMMAND => {
            if (wparam.0 & 0xFFFF) as u32 == ID_CANCEL && !state.is_null() {
                (*state).cancelled = true;
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            if !state.is_null() {
                (*state).cancelled = true; // X = 취소 요청(창은 완료 시 호스트가 닫음)
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            if !state.is_null() {
                let font = HFONT(GetStockObject(DEFAULT_GUI_FONT).0);
                let old = SelectObject(hdc, font.into());
                SetBkMode(hdc, TRANSPARENT);
                SetTextColor(hdc, COLORREF(0x00000000));
                let (done, total) = ((*state).done, (*state).total);
                let pct = if total > 0 {
                    ((done as f64 / total as f64) * 100.0) as i32
                } else {
                    0
                };
                // 라벨 + 수치
                let mut rc = RECT {
                    left: PAD,
                    top: PAD,
                    right: PROG_W - PAD,
                    bottom: PAD + 20,
                };
                let mut label = (*state).label.clone();
                DrawTextW(hdc, &mut label, &mut rc, DT_LEFT);
                let info = format!("{} / {}  ({pct}%)", fmt_bytes(done), fmt_bytes(total));
                let mut info_w: Vec<u16> = info.encode_utf16().collect();
                let mut rc2 = RECT {
                    left: PAD,
                    top: PAD + 22,
                    right: PROG_W - PAD,
                    bottom: PAD + 42,
                };
                DrawTextW(hdc, &mut info_w, &mut rc2, DT_LEFT);
                // 진행 바(직접 페인트 — comctl32 비의존)
                let bar = RECT {
                    left: PAD,
                    top: PAD + 48,
                    right: PROG_W - PAD - 16,
                    bottom: PAD + 66,
                };
                let frame = CreateSolidBrush(COLORREF(0x00808080));
                FillRect(hdc, &bar, frame);
                let _ = DeleteObject(frame.into());
                let inner = RECT {
                    left: bar.left + 1,
                    top: bar.top + 1,
                    right: bar.right - 1,
                    bottom: bar.bottom - 1,
                };
                let bg = CreateSolidBrush(COLORREF(0x00F0F0F0));
                FillRect(hdc, &inner, bg);
                let _ = DeleteObject(bg.into());
                let fill_w = ((inner.right - inner.left) as i64 * pct as i64 / 100) as i32;
                if fill_w > 0 {
                    let fill = RECT {
                        right: inner.left + fill_w,
                        ..inner
                    };
                    let fb = CreateSolidBrush(COLORREF(0x00D47B26)); // 앱 accent 근사(BGR)
                    FillRect(hdc, &fill, fb);
                    let _ = DeleteObject(fb.into());
                }
                SelectObject(hdc, old);
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// 전송 진행 창 — 생성/갱신/취소 폴링/닫기. UI 스레드 전용(비모달 — 앱 조작 가능).
pub struct Progress {
    hwnd: HWND,
    state: Box<ProgState>,
}

impl Progress {
    /// 진행 창 생성(소유자 중앙) — `label`=작업 설명(예: "복사 중…").
    pub unsafe fn open(owner: HWND, title: &str, label: &str) -> Option<Progress> {
        REGISTER_PROG.call_once(|| {
            let wc = WNDCLASSW {
                lpszClassName: CLASS_PROG,
                lpfnWndProc: Some(prog_proc),
                hInstance: windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
                    .unwrap_or_default()
                    .into(),
                hbrBackground: HBRUSH(
                    (windows::Win32::Graphics::Gdi::COLOR_BTNFACE.0 + 1) as isize
                        as *mut core::ffi::c_void,
                ),
                hCursor: windows::Win32::UI::WindowsAndMessaging::LoadCursorW(
                    None,
                    windows::Win32::UI::WindowsAndMessaging::IDC_ARROW,
                )
                .unwrap_or_default(),
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);
        });
        let mut state = Box::new(ProgState {
            done: 0,
            total: 0,
            label: label.encode_utf16().collect(),
            cancelled: false,
        });
        let (cx, cy) = center_over(owner, PROG_W, PROG_H);
        let title_w = windows::core::HSTRING::from(title);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS_PROG,
            PCWSTR(title_w.as_ptr()),
            WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
            cx,
            cy,
            PROG_W,
            PROG_H,
            Some(owner),
            None,
            None,
            None,
        )
        .ok()?;
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, &mut *state as *mut ProgState as isize);
        // [취소] 버튼
        let font = HFONT(GetStockObject(DEFAULT_GUI_FONT).0);
        let cancel = windows::core::HSTRING::from(crate::i18n::tr("ops.cancel"));
        if let Ok(btn) = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("BUTTON"),
            PCWSTR(cancel.as_ptr()),
            WS_CHILD
                | WS_VISIBLE
                | windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(BS_PUSHBUTTON as u32),
            PROG_W - PAD - BTN_W - 16,
            PROG_H - PAD - BTN_H - 32,
            BTN_W,
            BTN_H,
            Some(hwnd),
            Some(windows::Win32::UI::WindowsAndMessaging::HMENU(
                ID_CANCEL as usize as *mut core::ffi::c_void,
            )),
            None,
            None,
        ) {
            SendMessageW(
                btn,
                WM_SETFONT,
                Some(WPARAM(font.0 as usize)),
                Some(LPARAM(1)),
            );
        }
        let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(hwnd, SW_SHOWNORMAL);
        Some(Progress { hwnd, state })
    }

    /// 진행 값 갱신 + 다시 그리기(WM_APP_TRANSFER 통지에서 호출).
    pub unsafe fn update(&mut self, done: u64, total: u64) {
        self.state.done = done;
        self.state.total = total;
        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(self.hwnd), None, false);
    }

    /// [취소]·X가 눌렸는가(1회성 아님 — 호스트가 cancel 플래그에 반영).
    pub fn cancelled(&self) -> bool {
        self.state.cancelled
    }
}

impl Drop for Progress {
    fn drop(&mut self) {
        unsafe {
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0); // wndproc의 상태 참조 해제
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

/// 사람이 읽는 바이트 표기(진행 창 — KB/MB/GB).
fn fmt_bytes(b: u64) -> String {
    const K: f64 = 1024.0;
    let bf = b as f64;
    if bf >= K * K * K {
        format!("{:.1} GB", bf / (K * K * K))
    } else if bf >= K * K {
        format!("{:.1} MB", bf / (K * K))
    } else if bf >= K {
        format!("{:.1} KB", bf / K)
    } else {
        format!("{b} B")
    }
}
