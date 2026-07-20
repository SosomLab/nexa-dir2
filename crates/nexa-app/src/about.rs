//! About 창(X-26 ③ — 사용자 요청 07-20): 제품 소개·버전·링크·라이선스·저작권.
//! [dialog.rs](dialog.rs)(NexaDlg) 규약 계승 — user32/gdi32만 사용(B3 임포트 게이트),
//! 지표는 설정 `dlg_font` 파생, 자체 모달 루프(소유자 입력 차단).
//! 홈페이지(X-26 ①② — 정적 사이트) 확정 전까지 링크 = GitHub(조직·저장소·Releases),
//! 홈페이지 완성 시 URL만 교체한다.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, DeleteObject, DrawTextW, EndPaint, GetDC, InvalidateRect,
    ReleaseDC, ScreenToClient, SelectObject, SetBkMode, SetTextColor, CLIP_DEFAULT_PRECIS,
    DEFAULT_CHARSET, DEFAULT_QUALITY, DT_CALCRECT, DT_LEFT, FF_DONTCARE, FW_NORMAL, FW_SEMIBOLD,
    HBRUSH, HFONT, OUT_DEFAULT_PRECIS, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetCursorPos, GetMessageW, GetWindowLongPtrW, GetWindowRect, IsWindow, LoadCursorW,
    RegisterClassW, SendMessageW, SetCursor, SetForegroundWindow, SetWindowLongPtrW,
    TranslateMessage, BS_DEFPUSHBUTTON, GWLP_USERDATA, IDC_ARROW, IDC_HAND, MSG, WINDOW_EX_STYLE,
    WM_CLOSE, WM_COMMAND, WM_LBUTTONDOWN, WM_PAINT, WM_SETCURSOR, WM_SETFONT, WNDCLASSW,
    WS_CAPTION, WS_CHILD, WS_POPUP, WS_SYSMENU, WS_VISIBLE,
};

use crate::dialog::DlgFont;
use crate::i18n::tr;

/// SosomLab 조직 페이지(X-26 ① 홈페이지 완성 시 교체).
const ORG_URL: &str = "https://github.com/SosomLab";
/// 제품 저장소(X-26 ② 제품 홈페이지 완성 시 교체 — 저장소명은 nexa-dir2 유지: 사용자 확정).
const REPO_URL: &str = "https://github.com/SosomLab/nexa-dir2";
/// 다운로드(릴리스 자동 첨부 — M5-2 release.yml).
const RELEASES_URL: &str = "https://github.com/SosomLab/nexa-dir2/releases";

const CLASS: PCWSTR = w!("NexaAbout");
const PAD: i32 = 16;
const ID_OK: u32 = 1;

/// 표시 행 종류 — 색·폰트 선택.
enum RowKind {
    /// 제품명(큰 semibold).
    Title,
    Body,
    /// 흐린 보조(라이선스·저작권).
    Dim,
    /// 클릭 링크(accent + 밑줄) — usize = links 인덱스.
    Link(usize),
}

struct Row {
    y: i32,
    text: Vec<u16>,
    kind: RowKind,
}

struct AboutState {
    font: HFONT,
    title_font: HFONT,
    rows: Vec<Row>,
    /// 링크 히트 존(클라이언트 좌표)·대상 URL.
    links: Vec<(RECT, &'static str)>,
}

static REGISTER: std::sync::Once = std::sync::Once::new();

unsafe fn make_font(hwnd: HWND, spec: &DlgFont, scale_pct: i32, weight: i32) -> HFONT {
    let dpi = GetDpiForWindow(hwnd).max(96);
    let pt = (spec.size_pt.clamp(7, 24) * scale_pct) / 100;
    let h = -((pt.max(7) * dpi as i32) / 72);
    let face = windows::core::HSTRING::from(&*spec.family);
    CreateFontW(
        h,
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        DEFAULT_QUALITY,
        FF_DONTCARE.0 as u32,
        PCWSTR(face.as_ptr()),
    )
}

/// 폰트 기준 (폭, 높이) 측정 — dialog.rs 규약(DT_CALCRECT).
unsafe fn measure(font: HFONT, text: &str) -> (i32, i32) {
    let hdc = GetDC(None);
    let old = SelectObject(hdc, font.into());
    let mut buf: Vec<u16> = text.encode_utf16().collect();
    let mut rc = RECT::default();
    DrawTextW(hdc, &mut buf, &mut rc, DT_CALCRECT | DT_LEFT);
    SelectObject(hdc, old);
    ReleaseDC(None, hdc);
    (rc.right - rc.left, (rc.bottom - rc.top).max(14))
}

unsafe fn ensure_class() {
    REGISTER.call_once(|| {
        let wc = WNDCLASSW {
            lpszClassName: CLASS,
            lpfnWndProc: Some(about_proc),
            hInstance: windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
                .unwrap_or_default()
                .into(),
            hbrBackground: HBRUSH(
                (windows::Win32::Graphics::Gdi::COLOR_BTNFACE.0 + 1) as isize
                    as *mut core::ffi::c_void,
            ),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hIcon: crate::icon::load(32).unwrap_or_default(), // 앱 아이콘 공통(QA 07-14)
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);
    });
}

/// 커서의 링크 히트(클라이언트 좌표 변환) — WM_SETCURSOR/클릭 공용.
unsafe fn link_at(hwnd: HWND, st: &AboutState) -> Option<&'static str> {
    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    let _ = ScreenToClient(hwnd, &mut pt);
    st.links
        .iter()
        .find(|(rc, _)| pt.x >= rc.left && pt.x < rc.right && pt.y >= rc.top && pt.y < rc.bottom)
        .map(|(_, url)| *url)
}

unsafe extern "system" fn about_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AboutState;
    match msg {
        WM_COMMAND => {
            if (wparam.0 & 0xFFFF) as u32 == ID_OK {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_SETCURSOR => {
            // 링크 위 = 손 커서(브라우저 관례)
            if !state.is_null() && link_at(hwnd, &*state).is_some() {
                if let Ok(c) = LoadCursorW(None, IDC_HAND) {
                    SetCursor(Some(c));
                    return LRESULT(1);
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_LBUTTONDOWN => {
            if !state.is_null() {
                if let Some(url) = link_at(hwnd, &*state) {
                    // 기본 브라우저로 열기(파일 실행 shell_open과 동일 동사)
                    let wide = windows::core::HSTRING::from(url);
                    let _ = windows::Win32::UI::Shell::ShellExecuteW(
                        Some(hwnd),
                        w!("open"),
                        PCWSTR(wide.as_ptr()),
                        None,
                        None,
                        windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
                    );
                    let _ = InvalidateRect(Some(hwnd), None, true);
                }
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            if !state.is_null() {
                let st = &*state;
                SetBkMode(hdc, TRANSPARENT);
                for row in &st.rows {
                    let font = match row.kind {
                        RowKind::Title => st.title_font,
                        _ => st.font,
                    };
                    let color = match row.kind {
                        RowKind::Dim => COLORREF(0x00666666),
                        RowKind::Link(_) => COLORREF(0x00D47B26), // accent(#267BD4, BGR)
                        _ => COLORREF(0x00000000),
                    };
                    let old = SelectObject(hdc, font.into());
                    SetTextColor(hdc, color);
                    let mut text = row.text.clone();
                    let mut rc = RECT {
                        left: PAD,
                        top: row.y,
                        right: ps.rcPaint.right.max(PAD),
                        bottom: row.y + 200,
                    };
                    DrawTextW(hdc, &mut text, &mut rc, DT_LEFT);
                    if let RowKind::Link(i) = row.kind {
                        // 밑줄(링크 관례) — 히트 존 하단 1px
                        if let Some((lrc, _)) = st.links.get(i) {
                            let brush = windows::Win32::Graphics::Gdi::CreateSolidBrush(color);
                            let line = RECT {
                                left: lrc.left,
                                top: lrc.bottom - 1,
                                right: lrc.right,
                                bottom: lrc.bottom,
                            };
                            windows::Win32::Graphics::Gdi::FillRect(hdc, &line, brush);
                            let _ = DeleteObject(brush.into());
                        }
                    }
                    SelectObject(hdc, old);
                }
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// About 모달 표시(도움말 메뉴 — CMD_ABOUT. WM_APP_ABOUT 지연 실행 규약).
pub unsafe fn show(owner: HWND, font_spec: &DlgFont) {
    ensure_class();
    let font = make_font(owner, font_spec, 100, FW_NORMAL.0 as i32);
    let title_font = make_font(owner, font_spec, 170, FW_SEMIBOLD.0 as i32);

    // 내용 구성(위→아래) — (텍스트, 종류, 링크 URL)
    let version = format!("{} {}", tr("about.version"), env!("CARGO_PKG_VERSION"));
    let entries: Vec<(String, RowKind, Option<&'static str>)> = vec![
        ("Nexa Dir".into(), RowKind::Title, None),
        (tr("about.desc"), RowKind::Body, None),
        (version, RowKind::Body, None),
        (String::new(), RowKind::Body, None),
        (tr("about.link.repo"), RowKind::Link(0), Some(REPO_URL)),
        (
            tr("about.link.releases"),
            RowKind::Link(0),
            Some(RELEASES_URL),
        ),
        ("SosomLab (GitHub)".into(), RowKind::Link(0), Some(ORG_URL)),
        (String::new(), RowKind::Body, None),
        (tr("about.license"), RowKind::Dim, None),
        (tr("about.copyright"), RowKind::Dim, None),
    ];

    // 레이아웃 산정(폰트 파생 — dialog.rs 규약)
    let (_, line_h) = measure(font, "Ag");
    let (_, title_h) = measure(title_font, "Ag");
    let gap = line_h / 3;
    let mut rows = Vec::with_capacity(entries.len());
    let mut links: Vec<(RECT, &'static str)> = Vec::new();
    let mut y = PAD;
    let mut max_w = 0;
    for (text, kind, url) in entries {
        if text.is_empty() {
            y += line_h / 2; // 빈 줄 = 절반 간격
            continue;
        }
        let is_title = matches!(kind, RowKind::Title);
        let f = if is_title { title_font } else { font };
        let (w, h) = measure(f, &text);
        max_w = max_w.max(w);
        let kind = if let Some(u) = url {
            links.push((
                RECT {
                    left: PAD,
                    top: y,
                    right: PAD + w,
                    bottom: y + h,
                },
                u,
            ));
            RowKind::Link(links.len() - 1)
        } else {
            kind
        };
        rows.push(Row {
            y,
            text: text.encode_utf16().collect(),
            kind,
        });
        y += if is_title { title_h + gap } else { h + gap };
    }

    let btn_h = line_h + 10;
    let btn_w = {
        let (w, _) = measure(font, &tr("about.ok"));
        (w + 24).max(72)
    };
    let client_w = (max_w + PAD * 2).max(360);
    let client_h = y + gap + btn_h + PAD;
    let mut win = RECT {
        right: client_w,
        bottom: client_h,
        ..Default::default()
    };
    let _ = AdjustWindowRectEx(
        &mut win,
        WS_POPUP | WS_CAPTION | WS_SYSMENU,
        false,
        WINDOW_EX_STYLE(0x00000001), // DLGMODALFRAME
    );
    let (w, h) = (win.right - win.left, win.bottom - win.top);
    // 소유자 중앙(dialog.rs center_over 동일 산식 — dy 0)
    let (cx, cy) = {
        let mut rc = RECT::default();
        if GetWindowRect(owner, &mut rc).is_ok() {
            (
                rc.left + ((rc.right - rc.left) - w) / 2,
                rc.top + ((rc.bottom - rc.top) - h) / 2,
            )
        } else {
            (200, 200)
        }
    };
    let mut state = Box::new(AboutState {
        font,
        title_font,
        rows,
        links,
    });
    let title_w = windows::core::HSTRING::from(tr("about.title"));
    let Ok(dlg) = CreateWindowExW(
        WINDOW_EX_STYLE(0x00000001), // DLGMODALFRAME
        CLASS,
        PCWSTR(title_w.as_ptr()),
        WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
        cx,
        cy,
        w,
        h,
        Some(owner),
        None,
        None,
        None,
    ) else {
        let _ = DeleteObject(font.into());
        let _ = DeleteObject(title_font.into());
        return;
    };
    SetWindowLongPtrW(dlg, GWLP_USERDATA, &mut *state as *mut AboutState as isize);
    // [확인] — 우하단(대화상자 규약)
    let ok_label = windows::core::HSTRING::from(tr("about.ok"));
    if let Ok(btn) = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("BUTTON"),
        PCWSTR(ok_label.as_ptr()),
        WS_CHILD
            | WS_VISIBLE
            | windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(BS_DEFPUSHBUTTON as u32),
        client_w - PAD - btn_w,
        client_h - PAD - btn_h,
        btn_w,
        btn_h,
        Some(dlg),
        Some(windows::Win32::UI::WindowsAndMessaging::HMENU(
            ID_OK as usize as *mut core::ffi::c_void,
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
    let _ = EnableWindow(owner, false); // 모달(소유자 입력 차단)
    let _ = SetForegroundWindow(dlg);
    let mut msg = MSG::default();
    while IsWindow(Some(dlg)).as_bool() && GetMessageW(&mut msg, None, 0, 0).as_bool() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    let _ = EnableWindow(owner, true);
    let _ = SetForegroundWindow(owner);
    let _ = DeleteObject(font.into());
    let _ = DeleteObject(title_font.into());
    drop(state);
}
