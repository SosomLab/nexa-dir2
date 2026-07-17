//! ctldemo — **ctl 컨트롤 UI 검증용 갤러리 창**(개발 전용 — 사용자 확정 07-17:
//! "컨트롤 개발 과정을 별도 UI 검증용 윈도우로 검증"). 메뉴 비노출 —
//! `WM_APP_CTLDEMO`(0x8009) 주입으로만 연다. 기존 기능(일괄 이름변경 등)은
//! 이 창과 무관하게 유지 — 검증 완료 후 적용처(예: bulkrename 카드 재편)로 이식.
//!
//! 현재 수록: [`ctl::groupcard`] 2종(라운드/각진 — 타이틀·본문 높이 상이) +
//! 카드 본문에 ctl 자식(droplist/segmented/spin)과 user32 자식(STATIC/EDIT)을
//! 섞어 배치해 **통지 투과**(자식 → 카드 → 호스트 WM_COMMAND)를 상태줄로 증명.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{COLOR_BTNFACE, HBRUSH, HFONT};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, SendMessageW, SetWindowTextW,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_SETFONT, WNDCLASSW,
    WS_CAPTION, WS_CHILD, WS_POPUP, WS_SYSMENU, WS_VISIBLE,
};

use crate::ctl;
use crate::ctl::groupcard::GroupCardOpts;
use crate::ctl::style::Style;
use crate::dialog::DlgFont;

const CLASS: PCWSTR = w!("NexaCtlDemo");
static REGISTER: std::sync::Once = std::sync::Once::new();

const CARD_A: u32 = 100; // 라운드 카드(Replace Text 시안)
const CARD_B: u32 = 101; // 각진 카드(Add Number 시안)
const ID_SCOPE: u32 = 110;
const ID_MODE: u32 = 111;
const ID_FIND: u32 = 112;
const ID_DIR: u32 = 120;
const ID_OFF: u32 = 121;
/// 카드 A 타이틀의 동작 선택 NxComboBox(PF 시안 — 타이틀 영역 자식 배치 검증).
const ID_OPKIND: u32 = 130;
/// 대소문자 일치 NxCheckBox(macOS 시안 — 박스만).
const ID_CASE: u32 = 113;
/// 카드 A 타이틀 우측 +/−(PF 시안 — NxIconButton, − = 비활성 데모).
const ID_ADD: u32 = 140;
const ID_REMOVE: u32 = 141;
const ID_STATUS: u32 = 900;

/// 갤러리 창 열기(모달 아님 — 앱 메시지 루프가 디스패치). 반환 = 창 핸들.
/// 이미 열려 있으면 그 창을 앞으로(중복 생성 가드 — 도구 모음 버튼 연타).
pub unsafe fn show(owner: HWND, font_spec: &DlgFont) -> HWND {
    if let Ok(existing) = windows::Win32::UI::WindowsAndMessaging::FindWindowW(CLASS, None) {
        if !existing.is_invalid() {
            let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(existing);
            return existing;
        }
    }
    REGISTER.call_once(|| {
        let wc = WNDCLASSW {
            lpszClassName: CLASS,
            lpfnWndProc: Some(demo_proc),
            hbrBackground: HBRUSH((COLOR_BTNFACE.0 + 1) as isize as *mut core::ffi::c_void),
            hCursor: windows::Win32::UI::WindowsAndMessaging::LoadCursorW(
                None,
                windows::Win32::UI::WindowsAndMessaging::IDC_ARROW,
            )
            .unwrap_or_default(),
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);
    });
    let font = crate::dialog::make_font_pub(owner, font_spec);
    let style = WINDOW_STYLE(WS_POPUP.0 | WS_CAPTION.0 | WS_SYSMENU.0);
    let Ok(win) = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        CLASS,
        w!("ctl 검증 — GroupCard"),
        style | WS_VISIBLE,
        120,
        120,
        740,
        420,
        Some(owner),
        None,
        None,
        None,
    ) else {
        return HWND::default();
    };
    build(win, font);
    win
}

unsafe fn mk_static(parent: HWND, font: HFONT, text: &str, x: i32, y: i32, w: i32) -> HWND {
    let t = windows::core::HSTRING::from(text);
    let h = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("STATIC"),
        PCWSTR(t.as_ptr()),
        WS_CHILD | WS_VISIBLE,
        x,
        y,
        w,
        20,
        Some(parent),
        None,
        None,
        None,
    )
    .unwrap_or_default();
    SendMessageW(
        h,
        WM_SETFONT,
        Some(WPARAM(font.0 as usize)),
        Some(LPARAM(1)),
    );
    h
}

unsafe fn build(win: HWND, font: HFONT) {
    let st = Style::default();

    // ── 카드 A: 라운드(반경 10) · 타이틀 34 + 본문 240 ──
    // 타이틀 = 텍스트 대신 **동작 선택 NxComboBox**(PF 시안 — 타이틀 자식 배치)
    let a = ctl::groupcard::create(
        win,
        16,
        16,
        330,
        CARD_A,
        font,
        "",
        GroupCardOpts {
            corner: 10,
            title_h: 34,
            body_h: 240,
        },
        st,
    );
    let t = ctl::groupcard::title_rect(a);
    // 타이틀 밴드 위 컨트롤 = behind를 밴드 색(sel_bg)으로(AA 모서리 블렌드)
    let st_band = Style {
        behind: st.sel_bg,
        ..st
    };
    let cb_h = 24;
    ctl::combobox::create(
        a,
        t.left + 8,
        t.top + (t.bottom - t.top - cb_h) / 2,
        170,
        cb_h,
        ID_OPKIND,
        font,
        &[
            "Replace Text",
            "Replace RegEx",
            "Insert Text",
            "Change Case",
            "Add Number Sequence",
            "Add Date",
        ],
        0,
        st_band,
    );
    // 타이틀 우측 +/−(shape 투명 검증 — 회색 타이틀 밴드 위 원형만 보여야 함)
    let ib = 20;
    let iy = t.top + (t.bottom - t.top - ib) / 2;
    ctl::iconbutton::create(
        a,
        330 - 8 - ib * 2 - 6,
        iy,
        ib,
        ID_ADD,
        font,
        ctl::iconbutton::Icon::Plus,
        true,
        st_band,
    );
    ctl::iconbutton::create(
        a,
        330 - 8 - ib,
        iy,
        ib,
        ID_REMOVE,
        font,
        ctl::iconbutton::Icon::Minus,
        false, // 삭제 대상이 자신뿐 = 비활성(시안)
        st_band,
    );
    let b = ctl::groupcard::body_rect(a);
    let (bx, by) = (b.left + 12, b.top + 12);
    mk_static(a, font, "적용 대상:", bx, by + 4, 80);
    // 적용 대상 = NxComboBox(h=0 → 자동 높이: 글꼴+최소 여백 — 높이 규칙 검증)
    ctl::combobox::create(
        a,
        bx + 88,
        by,
        200,
        0,
        ID_SCOPE,
        font,
        &["이름", "이름+확장자", "확장자", "확장자(점 포함)"],
        0,
        st,
    );
    mk_static(a, font, "Mode:", bx, by + 40, 80);
    ctl::droplist::create(
        a,
        bx + 88,
        by + 36,
        200,
        26,
        ID_MODE,
        font,
        &["모든 일치", "첫 일치", "마지막 일치", "전체 교체"],
        0,
        st,
    );
    mk_static(a, font, "대소문자 일치:", bx, by + 76, 84);
    // 박스만(라벨 = 좌측 STATIC — 시안 배치)·h=0 자동
    ctl::checkbox::create(a, bx + 88, by + 72, 0, 0, ID_CASE, font, "", false, st);
    mk_static(a, font, "찾기:", bx, by + 112, 80);
    // NxTextBox — h=0 자동(공통 자동 높이 — 다른 Nx와 같은 row 기본 정렬 검증)
    ctl::textbox::create(a, bx + 88, by + 108, 200, 0, ID_FIND, font, st);

    // ── 카드 B: 각진 · 타이틀 26 + 본문 140(영역별 크기 상이 검증) ──
    let c = ctl::groupcard::create(
        win,
        366,
        16,
        330,
        CARD_B,
        font,
        "Add Number Sequence",
        GroupCardOpts {
            corner: 0,
            title_h: 26,
            body_h: 140,
        },
        st,
    );
    let b2 = ctl::groupcard::body_rect(c);
    let (cx, cy) = (b2.left + 12, b2.top + 12);
    mk_static(c, font, "위치:", cx, cy + 4, 60);
    ctl::spin::create(c, cx + 68, cy, 90, 26, ID_OFF, font, 0, 0, 999, st);
    ctl::segmented::create(
        c,
        cx + 68,
        cy + 36,
        160,
        26,
        ID_DIR,
        font,
        &["앞에서", "뒤에서"],
        0,
        st,
    );

    // ── 통지 투과 증명 상태줄 ──
    let s = mk_static(win, font, "(통지 대기)", 16, 350, 680);
    let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
        s,
        windows::Win32::UI::WindowsAndMessaging::GWLP_ID,
        ID_STATUS as isize,
    );
}

unsafe extern "system" fn demo_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_COMMAND => {
            // 카드가 투과한 자식 통지 — 상태줄에 표기(검증 계약)
            let id = (wparam.0 & 0xFFFF) as u32;
            let code = ((wparam.0 >> 16) & 0xFFFF) as u32;
            if id != ID_STATUS {
                let dlg = windows::Win32::UI::WindowsAndMessaging::GetDlgItem(
                    Some(hwnd),
                    ID_STATUS as i32,
                )
                .unwrap_or_default();
                let t = windows::core::HSTRING::from(format!("통지 수신: id={id} code={code}"));
                let _ = SetWindowTextW(dlg, PCWSTR(t.as_ptr()));
            }
            let _ = lparam;
            LRESULT(0)
        }
        m if m == windows::Win32::UI::WindowsAndMessaging::WM_CTLCOLORSTATIC => {
            // 카드가 투과한 CTLCOLOR — 카드 본문 라벨은 카드 bg(흰색)로 응답
            // (통지 투과 계약의 호스트 측 활용 예시).
            let child = HWND(lparam.0 as *mut core::ffi::c_void);
            let on_card = windows::Win32::UI::WindowsAndMessaging::GetParent(child)
                .ok()
                .map(|p| {
                    let mut cls = [0u16; 32];
                    let n = windows::Win32::UI::WindowsAndMessaging::GetClassNameW(p, &mut cls)
                        as usize;
                    String::from_utf16_lossy(&cls[..n]) == "Nexa.NxGroupCard"
                })
                .unwrap_or(false);
            if on_card {
                let dc = windows::Win32::Graphics::Gdi::HDC(wparam.0 as *mut core::ffi::c_void);
                windows::Win32::Graphics::Gdi::SetBkMode(
                    dc,
                    windows::Win32::Graphics::Gdi::TRANSPARENT,
                );
                return LRESULT(
                    windows::Win32::Graphics::Gdi::GetStockObject(
                        windows::Win32::Graphics::Gdi::WHITE_BRUSH,
                    )
                    .0 as isize,
                );
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => LRESULT(0),
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
