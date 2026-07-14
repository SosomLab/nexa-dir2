//! 설정 창(S6 — 원본 `PreferencesWindow`/docs/40 참고, 신규 앱 규모에 맞춘 간소판).
//! 네이티브 컨트롤(user32 STATIC/EDIT/BUTTON — comctl32 비의존)·모달·`Ctrl+,`.
//!
//! 항목(현 영속 설정 전부): 테마·언어(순환 버튼) · 터미널 글꼴/크기 · 대화상자
//! 글꼴/크기(EDIT). [저장]=적용 값 반환(적용·영속은 호스트 몫 — win.rs), [취소]=None.
//! 원본의 VS Code식 검색+카테고리 트리는 항목 수가 늘면 후속(현재 6항목 — 단일 페이지).

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{DeleteObject, HBRUSH, HFONT};
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetMessageW, GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, IsWindow, RegisterClassW,
    SendMessageW, SetForegroundWindow, SetWindowLongPtrW, SetWindowTextW, TranslateMessage,
    BS_PUSHBUTTON, ES_AUTOHSCROLL, GWLP_USERDATA, HMENU, MSG, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_CLOSE, WM_COMMAND, WM_SETFONT, WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CHILD, WS_POPUP,
    WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
};

use crate::dialog::DlgFont;
use crate::i18n::tr;

/// 설정 창 입력/결과 — 호스트(win.rs)가 현재 값을 넣고, 저장 시 수정본을 돌려받는다.
#[derive(Clone)]
pub struct PrefValues {
    pub theme: String, // "system"|"light"|"dark"
    pub lang: String,  // "system"|코드
    pub langs: Vec<String>,
    pub term_font: String,
    pub term_font_size: i32,
    pub dlg_font: String,
    pub dlg_font_size: i32,
}

struct PrefState {
    values: PrefValues,
    theme_btn: HWND,
    lang_btn: HWND,
    saved: bool,
}

const ID_SAVE: u32 = 1;
const ID_CANCEL: u32 = 2;
const ID_THEME: u32 = 10;
const ID_LANG: u32 = 11;
const ID_TERM_FONT: u32 = 20;
const ID_TERM_SIZE: u32 = 21;
const ID_DLG_FONT: u32 = 22;
const ID_DLG_SIZE: u32 = 23;

static REGISTER: std::sync::Once = std::sync::Once::new();
const CLASS: PCWSTR = w!("NexaPrefs");
const PAD: i32 = 14;
const ROW_H: i32 = 30;
const LABEL_W: i32 = 150;
const CTRL_W: i32 = 240;

unsafe extern "system" fn prefs_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut PrefState;
    match msg {
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as u32;
            if state.is_null() {
                return LRESULT(0);
            }
            match id {
                ID_THEME => {
                    // 순환: system → light → dark(라이브 적용은 저장 시 — 원본 규율)
                    let next = match (*state).values.theme.as_str() {
                        "system" => "light",
                        "light" => "dark",
                        _ => "system",
                    };
                    (*state).values.theme = next.to_string();
                    set_text(
                        (*state).theme_btn,
                        &tr(&format!("pref.theme.{}", (*state).values.theme)),
                    );
                }
                ID_LANG => {
                    // 순환: system → 발견 코드들
                    let mut opts = vec!["system".to_string()];
                    opts.extend((*state).values.langs.clone());
                    let cur = opts
                        .iter()
                        .position(|c| *c == (*state).values.lang)
                        .unwrap_or(0);
                    let next = opts[(cur + 1) % opts.len()].clone();
                    set_text((*state).lang_btn, &lang_label(&next));
                    (*state).values.lang = next;
                }
                ID_SAVE => {
                    (*state).saved = true;
                    let _ = DestroyWindow(hwnd);
                }
                ID_CANCEL => {
                    let _ = DestroyWindow(hwnd);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn lang_label(code: &str) -> String {
    if code == "system" {
        tr("pref.lang.system")
    } else {
        code.to_string()
    }
}

unsafe fn set_text(hwnd: HWND, text: &str) {
    let w = windows::core::HSTRING::from(text);
    let _ = SetWindowTextW(hwnd, PCWSTR(w.as_ptr()));
}

unsafe fn get_text(hwnd: HWND) -> String {
    let len = GetWindowTextLengthW(hwnd);
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; len as usize + 1];
    let got = GetWindowTextW(hwnd, &mut buf);
    String::from_utf16_lossy(&buf[..got.max(0) as usize])
}

/// 설정 창 표시(모달) — 저장 시 수정된 값, 취소/닫기 시 `None`.
///
/// # Safety
/// UI 스레드에서 호출(모달 루프 동안 wndproc 재진입 — 호출자는 State 참조를 끊을 것).
pub unsafe fn show(owner: HWND, values: PrefValues, font_spec: &DlgFont) -> Option<PrefValues> {
    REGISTER.call_once(|| {
        let wc = WNDCLASSW {
            lpszClassName: CLASS,
            lpfnWndProc: Some(prefs_proc),
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
            hIcon: crate::icon::load(32).unwrap_or_default(), // 원본 아이콘 공통(QA 07-14)
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);
    });
    let font = crate::dialog::make_font_pub(owner, font_spec);
    let rows = 6;
    let btn_h = 26;
    let client_w = PAD + LABEL_W + 8 + CTRL_W + PAD;
    let client_h = PAD + rows * ROW_H + 10 + btn_h + PAD;
    let mut win = RECT {
        right: client_w,
        bottom: client_h,
        ..Default::default()
    };
    let _ = AdjustWindowRectEx(
        &mut win,
        WS_POPUP | WS_CAPTION | WS_SYSMENU,
        false,
        WINDOW_EX_STYLE(0x00000001),
    );
    let (w_, h_) = (win.right - win.left, win.bottom - win.top);
    let title = windows::core::HSTRING::from(tr("pref.title"));
    let dlg = CreateWindowExW(
        WINDOW_EX_STYLE(0x00000001),
        CLASS,
        PCWSTR(title.as_ptr()),
        WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
        200,
        200,
        w_,
        h_,
        Some(owner),
        None,
        None,
        None,
    )
    .ok()?;
    let mut state = Box::new(PrefState {
        values,
        theme_btn: HWND::default(),
        lang_btn: HWND::default(),
        saved: false,
    });

    let mk = |class: PCWSTR, text: &str, style: u32, x: i32, y: i32, w: i32, h: i32, id: u32| {
        let t = windows::core::HSTRING::from(text);
        let hw = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            PCWSTR(t.as_ptr()),
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(style),
            x,
            y,
            w,
            h,
            Some(dlg),
            Some(HMENU(id as usize as *mut core::ffi::c_void)),
            None,
            None,
        )
        .unwrap_or_default();
        SendMessageW(
            hw,
            WM_SETFONT,
            Some(WPARAM(font.0 as usize)),
            Some(LPARAM(1)),
        );
        hw
    };
    let label = |text: &str, row: i32| {
        mk(
            w!("STATIC"),
            text,
            0,
            PAD,
            PAD + row * ROW_H + 4,
            LABEL_W,
            20,
            0,
        );
    };
    let edit = |text: &str, row: i32, id: u32| {
        mk(
            w!("EDIT"),
            text,
            (WS_BORDER | WS_TABSTOP).0 | ES_AUTOHSCROLL as u32,
            PAD + LABEL_W + 8,
            PAD + row * ROW_H + 2,
            CTRL_W,
            22,
            id,
        )
    };
    let button = |text: &str, row: i32, id: u32| {
        mk(
            w!("BUTTON"),
            text,
            (WS_TABSTOP).0 | BS_PUSHBUTTON as u32,
            PAD + LABEL_W + 8,
            PAD + row * ROW_H,
            CTRL_W,
            24,
            id,
        )
    };

    label(&tr("pref.theme"), 0);
    state.theme_btn = button(
        &tr(&format!("pref.theme.{}", state.values.theme)),
        0,
        ID_THEME,
    );
    label(&tr("pref.lang"), 1);
    state.lang_btn = button(&lang_label(&state.values.lang), 1, ID_LANG);
    label(&tr("pref.termFont"), 2);
    let e_tf = edit(&state.values.term_font, 2, ID_TERM_FONT);
    label(&tr("pref.termFontSize"), 3);
    let e_ts = edit(&state.values.term_font_size.to_string(), 3, ID_TERM_SIZE);
    label(&tr("pref.dlgFont"), 4);
    let e_df = edit(&state.values.dlg_font, 4, ID_DLG_FONT);
    label(&tr("pref.dlgFontSize"), 5);
    let e_ds = edit(&state.values.dlg_font_size.to_string(), 5, ID_DLG_SIZE);
    let by = PAD + rows * ROW_H + 10;
    mk(
        w!("BUTTON"),
        &tr("pref.save"),
        (WS_TABSTOP).0 | BS_PUSHBUTTON as u32,
        client_w - PAD - 90 - 8 - 90,
        by,
        90,
        btn_h,
        ID_SAVE,
    );
    mk(
        w!("BUTTON"),
        &tr("pref.cancel"),
        (WS_TABSTOP).0 | BS_PUSHBUTTON as u32,
        client_w - PAD - 90,
        by,
        90,
        btn_h,
        ID_CANCEL,
    );
    SetWindowLongPtrW(dlg, GWLP_USERDATA, &mut *state as *mut PrefState as isize);

    let _ = EnableWindow(owner, false);
    let _ = SetForegroundWindow(dlg);
    let mut msg = MSG::default();
    // 저장 직전 EDIT 값을 읽어야 하므로 파괴 전 후킹 대신 파괴 후 읽기 불가 — WM_COMMAND
    // ID_SAVE에서 파괴하기 전에 값을 흡수하도록 여기서 폴링: 루프 안에서 IsWindow 검사 전
    // 마지막 값을 계속 동기화한다(간이 — 항목 6개라 비용 无).
    while IsWindow(Some(dlg)).as_bool() && GetMessageW(&mut msg, None, 0, 0).as_bool() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
        if IsWindow(Some(dlg)).as_bool() {
            state.values.term_font = get_text(e_tf);
            state.values.term_font_size = get_text(e_ts).trim().parse().unwrap_or(12);
            state.values.dlg_font = get_text(e_df);
            state.values.dlg_font_size = get_text(e_ds).trim().parse().unwrap_or(9);
        }
    }
    let _ = EnableWindow(owner, true);
    let _ = SetForegroundWindow(owner);
    let _ = DeleteObject(HFONT(font.0).into());
    if state.saved {
        let mut v = state.values.clone();
        if v.term_font.trim().is_empty() {
            v.term_font = "Consolas".into();
        }
        if v.dlg_font.trim().is_empty() {
            v.dlg_font = "Segoe UI".into();
        }
        v.term_font_size = v.term_font_size.clamp(8, 32);
        v.dlg_font_size = v.dlg_font_size.clamp(7, 24);
        Some(v)
    } else {
        None
    }
}
