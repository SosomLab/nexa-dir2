//! 암호 입력 모달(X-46 — 압축 파일의 목록/내용이 암호로 잠겨 있을 때).
//!
//! **취급 규약**(사용자 지시 08-24: "전달만 하고 기록되거나 Plain으로 노출되지
//! 않도록") — 이 창은 다음을 보장한다:
//!
//! 1. 입력은 내부 EDIT의 **마스킹 모드**(`ES_PASSWORD` 규약 — 복사/잘라내기 불가).
//!    "암호 표시"를 켠 동안만 평문이 화면에 보인다(사용자 확인용, 선택).
//! 2. 확인 시 값은 [`Secret`]으로 **한 번만** 옮기고, 경유 UTF-16 버퍼와 컨트롤
//!    내용·되돌리기 버퍼를 즉시 지운다([`ctl::textbox::clear_secret`]).
//! 3. 창은 값을 상태에 남기지 않는다(반환 즉시 이동) — 로그·설정·창 제목 어디에도
//!    쓰지 않는다. 보관은 호출자의 세션 캐시([`crate::preview::archive::pw`])뿐이며
//!    그마저 메모리 한정이다.

use nexa_core::secret::Secret;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::HFONT;
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, GetWindowRect,
    IsWindow, LoadCursorW, RegisterClassW, SetForegroundWindow, SetWindowLongPtrW,
    TranslateMessage, GWLP_USERDATA, IDC_ARROW, MSG, WM_CLOSE, WM_COMMAND, WM_GETTEXTLENGTH,
    WM_KEYDOWN, WNDCLASSW, WINDOW_EX_STYLE, WINDOW_STYLE, WS_CAPTION, WS_POPUP, WS_SYSMENU,
    WS_VISIBLE,
};

use crate::ctl::{self, style::Style};
use crate::i18n::{tr, trf};

const CLASS: PCWSTR = w!("NexaPwPrompt");
static REGISTER: std::sync::Once = std::sync::Once::new();

const ID_EDIT: u32 = 1;
const ID_SHOW: u32 = 2;
const ID_OK: u32 = 3;
const ID_CANCEL: u32 = 4;

const PAD: i32 = 14;
const FORM_W: i32 = 420;
/// 마스킹 문자(●).
const MASK: char = '\u{25CF}';
/// 암호 길이 상한(입력 버퍼 — 실질 무제한이면서 폭주 방지).
const MAX_LEN: usize = 1024;

struct PwCtx {
    edit: HWND,
    /// 확인으로 닫혔을 때만 채워진다(취소·닫기 = `None`).
    result: Option<Secret>,
}

/// 암호를 묻는다 — 확인 = `Some`, 취소/닫기 = `None`.
/// `retry`가 `true`면 "암호가 맞지 않습니다" 안내를 함께 보여 준다.
pub unsafe fn ask(owner: HWND, file_name: &str, retry: bool, font: HFONT) -> Option<Secret> {
    REGISTER.call_once(|| {
        let wc = WNDCLASSW {
            lpszClassName: CLASS,
            lpfnWndProc: Some(proc),
            hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH(
                (windows::Win32::Graphics::Gdi::COLOR_WINDOW.0 + 1) as isize
                    as *mut core::ffi::c_void,
            ),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);
    });
    let style = Style::default();
    let lh = ctl::style::font_height(owner, font).max(12);
    let row = lh + 12;
    // 제목 아래: 안내 1~2줄 · 라벨+입력 · 표시 체크 · 주의 문구 · 버튼
    let notes = 1 + i32::from(retry);
    let client_h = PAD + lh * notes + 10 + row + 8 + row + 6 + lh + PAD + row + PAD;
    let client_w = FORM_W + PAD * 2;
    let mut orc = RECT::default();
    let _ = GetWindowRect(owner, &mut orc);
    let (cx, cy) = (
        orc.left + ((orc.right - orc.left) - client_w) / 2,
        orc.top + ((orc.bottom - orc.top) - client_h) / 3,
    );
    let title = windows::core::HSTRING::from(tr("archive.pw.title"));
    let Ok(dlg) = CreateWindowExW(
        WINDOW_EX_STYLE(0x0000_0001), // DLGMODALFRAME
        CLASS,
        PCWSTR(title.as_ptr()),
        WINDOW_STYLE(WS_POPUP.0 | WS_CAPTION.0 | WS_SYSMENU.0) | WS_VISIBLE,
        cx,
        cy,
        client_w,
        client_h + 30, // 캡션 보정(ordereditor 규약)
        Some(owner),
        None,
        None,
        None,
    ) else {
        return None;
    };

    let mut y = PAD;
    // 안내(파일명은 표시하지만 암호는 어디에도 표시하지 않는다)
    ctl::label::create(
        dlg,
        PAD,
        y,
        FORM_W,
        lh,
        0,
        font,
        &trf("archive.pw.prompt", &[file_name]),
        ctl::label::LabelAlign::Left,
        style,
    );
    y += lh;
    if retry {
        ctl::label::create(
            dlg,
            PAD,
            y,
            FORM_W,
            lh,
            0,
            font,
            &tr("archive.pw.wrong"),
            ctl::label::LabelAlign::Left,
            Style {
                text: style.danger,
                ..style
            },
        );
        y += lh;
    }
    y += 10;
    let label_w = 72;
    ctl::label::create(
        dlg,
        PAD,
        y,
        label_w,
        row,
        0,
        font,
        &tr("archive.pw.label"),
        ctl::label::LabelAlign::Left,
        style,
    );
    let edit = ctl::textbox::create(
        dlg,
        PAD + label_w,
        y,
        FORM_W - label_w,
        row,
        ID_EDIT,
        font,
        style,
    );
    ctl::textbox::set_password_char(edit, Some(MASK));
    y += row + 8;
    ctl::checkbox::create(
        dlg,
        PAD + label_w,
        y,
        FORM_W - label_w,
        row,
        ID_SHOW,
        font,
        &tr("archive.pw.show"),
        0,
        ctl::checkbox::CheckMode::Two,
        style,
    );
    y += row + 6;
    ctl::label::create(
        dlg,
        PAD,
        y,
        FORM_W,
        lh,
        0,
        font,
        &tr("archive.pw.note"),
        ctl::label::LabelAlign::Left,
        Style {
            text: style.text_dim,
            ..style
        },
    );
    y += lh + PAD;
    let bw = 88;
    ctl::button::create(
        dlg,
        PAD + FORM_W - bw * 2 - 8,
        y,
        bw,
        row,
        ID_OK,
        font,
        &tr("archive.pw.ok"),
        ctl::button::ButtonKind::Default,
        true,
        style,
    );
    ctl::button::create(
        dlg,
        PAD + FORM_W - bw,
        y,
        bw,
        row,
        ID_CANCEL,
        font,
        &tr("archive.pw.cancel"),
        ctl::button::ButtonKind::Normal,
        true,
        style,
    );

    let mut ctx = Box::new(PwCtx { edit, result: None });
    SetWindowLongPtrW(dlg, GWLP_USERDATA, &mut *ctx as *mut PwCtx as isize);
    let _ = EnableWindow(owner, false);
    let _ = SetForegroundWindow(dlg);
    let _ = SetFocus(Some(edit));
    let mut msg = MSG::default();
    while IsWindow(Some(dlg)).as_bool() && GetMessageW(&mut msg, None, 0, 0).as_bool() {
        // Enter/Esc = 확인/취소(자체 펌프 — 대화상자 관리자 없음)
        if msg.message == WM_KEYDOWN {
            match msg.wParam.0 as u16 {
                0x0D => {
                    take_and_close(dlg, &mut ctx);
                    continue;
                }
                0x1B => {
                    let _ = DestroyWindow(dlg);
                    continue;
                }
                _ => {}
            }
        }
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    let _ = EnableWindow(owner, true);
    let _ = SetForegroundWindow(owner);
    ctx.result.take() // 창 상태에는 남기지 않는다(이동)
}

/// 입력값을 [`Secret`]으로 회수하고 컨트롤 잔상을 지운 뒤 창을 닫는다.
unsafe fn take_and_close(dlg: HWND, ctx: &mut PwCtx) {
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_GETTEXT};
    let len = SendMessageW(ctx.edit, WM_GETTEXTLENGTH, None, None).0 as usize;
    if len > 0 {
        let cap = len.min(MAX_LEN) + 1;
        let mut buf = vec![0u16; cap];
        SendMessageW(
            ctx.edit,
            WM_GETTEXT,
            Some(WPARAM(cap)),
            Some(LPARAM(buf.as_mut_ptr() as isize)),
        );
        // 경유 버퍼는 Secret 생성과 동시에 0으로 덮인다(nexa-core 규약)
        let secret = Secret::take_from_u16(&mut buf);
        if !secret.is_empty() {
            ctx.result = Some(secret);
        }
    }
    ctl::textbox::clear_secret(ctx.edit); // 컨트롤 내용·되돌리기 버퍼 제거
    let _ = DestroyWindow(dlg);
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let ctx = windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA)
        as *mut PwCtx;
    match msg {
        WM_COMMAND if !ctx.is_null() => {
            let id = (wp.0 & 0xFFFF) as u32;
            let code = ((wp.0 >> 16) & 0xFFFF) as u32;
            match id {
                ID_OK if code == ctl::button::NXBTN_CLICK => take_and_close(hwnd, &mut *ctx),
                ID_CANCEL if code == ctl::button::NXBTN_CLICK => {
                    ctl::textbox::clear_secret((*ctx).edit);
                    let _ = DestroyWindow(hwnd);
                }
                ID_SHOW if code == ctl::checkbox::NXCHK_CHANGED => {
                    let on = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                        HWND(lp.0 as *mut core::ffi::c_void),
                        ctl::checkbox::NXCHK_GETCHECK,
                        None,
                        None,
                    )
                    .0 != 0;
                    // 표시 토글은 화면에만 영향 — 값은 그대로 컨트롤 안에 있다
                    ctl::textbox::set_password_char((*ctx).edit, (!on).then_some(MASK));
                    let _ = SetFocus(Some((*ctx).edit));
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            if !ctx.is_null() {
                ctl::textbox::clear_secret((*ctx).edit);
            }
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
