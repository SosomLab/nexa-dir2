//! 커스텀 대화상자 컨트롤(QA 07-14 — "User Control"): **임의 버튼/메시지 모달**(원본
//! 전송 확인창 4버튼 대응) + **전송 진행 창**(프로그레스 바·취소). user32/gdi32만 사용
//! (comctl32 비의존 — B3 임포트 게이트 유지, M3-4 회피 규약 계승).
//!
//! **폰트**: 설정 `dlg_font`/`dlg_font_size`([`DlgFont`])를 따르고 모든 지표(버튼·여백·
//! 창 크기)가 폰트 높이에서 파생 — 메시지는 워드랩 측정으로 **전체가 보이도록** 창 높이
//! 산정(AdjustWindowRectEx로 비클라이언트 보정 — QA 07-14 잘림 수정).
//!
//! - [`show_buttons`]: 메시지 + 호출자 정의 버튼 목록 → 클릭한 버튼 id(닫힘=0).
//!   자체 모달 루프 — 워커 스레드에서도 사용 가능(MessageBox 대체).
//! - [`Progress`]: 비모달 진행 창 — 바(직접 페인트)·백분율 텍스트·[취소] 버튼.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect, GetDC,
    ReleaseDC, SelectObject, SetBkMode, SetTextColor, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET,
    DEFAULT_QUALITY, DT_CALCRECT, DT_LEFT, DT_WORDBREAK, FF_DONTCARE, FW_NORMAL, HBRUSH, HFONT,
    OUT_DEFAULT_PRECIS, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetClientRect, GetMessageW, GetWindowLongPtrW, GetWindowRect, IsWindow, KillTimer,
    RegisterClassW, SendMessageW, SetForegroundWindow, SetTimer, SetWindowLongPtrW,
    TranslateMessage, BS_DEFPUSHBUTTON, BS_PUSHBUTTON, GWLP_USERDATA, MSG, SW_SHOWNORMAL,
    WINDOW_EX_STYLE, WM_CLOSE, WM_COMMAND, WM_PAINT, WM_SETFONT, WM_TIMER, WNDCLASSW, WS_CAPTION,
    WS_CHILD, WS_POPUP, WS_SYSMENU, WS_VISIBLE,
};

/// 대화상자 버튼(호출자 정의) — `id`는 1 이상(0=닫힘 예약).
pub struct DlgButton {
    pub id: u32,
    pub label: String,
}

/// 대화상자 폰트 스펙(설정 `dlg_font`/`dlg_font_size` — pt 단위).
#[derive(Clone)]
pub struct DlgFont {
    pub family: String,
    pub size_pt: i32,
}

/// 공용 상태(GWLP_USERDATA) — wndproc가 기록, 소유자가 읽는다.
struct DlgState {
    result: u32,
    text: Vec<u16>,
    font: HFONT,
    /// 본문 영역(클라이언트 좌표) — WM_PAINT가 사용.
    text_rc: RECT,
}

static REGISTER: std::sync::Once = std::sync::Once::new();
const CLASS: PCWSTR = w!("NexaDlg");
const PAD: i32 = 12;
const GAP: i32 = 6;

unsafe fn make_font(hwnd: HWND, spec: &DlgFont) -> HFONT {
    let dpi = GetDpiForWindow(hwnd).max(96);
    let h = -((spec.size_pt.clamp(7, 24) * dpi as i32) / 72);
    let face = windows::core::HSTRING::from(&*spec.family);
    CreateFontW(
        h,
        0,
        0,
        0,
        FW_NORMAL.0 as i32,
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

/// 폰트 기준 텍스트 측정 — (워드랩 높이, 한 줄 높이).
unsafe fn measure(font: HFONT, text: &str, width: i32) -> (i32, i32) {
    let hdc = GetDC(None);
    let old = SelectObject(hdc, font.into());
    let mut buf: Vec<u16> = text.encode_utf16().collect();
    let mut rc = RECT {
        right: width,
        ..Default::default()
    };
    DrawTextW(hdc, &mut buf, &mut rc, DT_CALCRECT | DT_LEFT | DT_WORDBREAK);
    let mut line: Vec<u16> = "Ag".encode_utf16().collect();
    let mut lrc = RECT::default();
    DrawTextW(hdc, &mut line, &mut lrc, DT_CALCRECT | DT_LEFT);
    SelectObject(hdc, old);
    ReleaseDC(None, hdc);
    ((rc.bottom - rc.top).max(16), (lrc.bottom - lrc.top).max(14))
}

/// 폰트 기준 문자열 폭.
unsafe fn text_width(font: HFONT, text: &str) -> i32 {
    let hdc = GetDC(None);
    let old = SelectObject(hdc, font.into());
    let mut buf: Vec<u16> = text.encode_utf16().collect();
    let mut rc = RECT::default();
    DrawTextW(hdc, &mut buf, &mut rc, DT_CALCRECT | DT_LEFT);
    SelectObject(hdc, old);
    ReleaseDC(None, hdc);
    rc.right - rc.left
}

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
            hIcon: crate::icon::load(32).unwrap_or_default(), // 원본 아이콘 공통(QA 07-14)
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
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            if !state.is_null() {
                let old = SelectObject(hdc, (*state).font.into());
                SetBkMode(hdc, TRANSPARENT);
                SetTextColor(hdc, COLORREF(0x00000000));
                let mut rc = (*state).text_rc;
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

/// 임의 버튼 모달 대화상자 — 클릭한 버튼 id(닫힘=0). 어느 스레드든 호출 가능
/// (자체 메시지 루프 — MessageBox 동등). 첫 버튼 = 기본 강조.
pub unsafe fn show_buttons(
    owner: HWND,
    title: &str,
    message: &str,
    buttons: &[DlgButton],
    font_spec: &DlgFont,
) -> u32 {
    ensure_class();
    let font = make_font(owner, font_spec);
    // 지표 전부 폰트 파생(QA 07-14 — "폰트에 맞춰 전반적으로 작게")
    let btn_widths: Vec<i32> = buttons
        .iter()
        .map(|b| (text_width(font, &b.label) + 20).max(56))
        .collect();
    let btn_total: i32 = btn_widths.iter().sum::<i32>() + GAP * (buttons.len() as i32 - 1).max(0);
    let client_w = (btn_total + PAD * 2).max(380);
    let (text_h, line_h) = measure(font, message, client_w - PAD * 2);
    let btn_h = line_h + 10;
    let client_h = PAD + text_h + PAD + btn_h + PAD;
    // 비클라이언트(캡션·프레임) 보정 — 메시지 잘림 방지(QA 07-14)
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
    let (cx, cy) = center_over(owner, w, h, 110); // 진행 창(위)과 비겹침(QA 07-14)
    let mut state = Box::new(DlgState {
        result: 0,
        text: message.encode_utf16().collect(),
        font,
        text_rc: RECT {
            left: PAD,
            top: PAD,
            right: client_w - PAD,
            bottom: PAD + text_h,
        },
    });
    let title_w = windows::core::HSTRING::from(title);
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
        return 0;
    };
    SetWindowLongPtrW(dlg, GWLP_USERDATA, &mut *state as *mut DlgState as isize);
    // 버튼 — 우측 정렬(왼→오 순서 유지)
    let mut x = client_w - PAD - btn_total;
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
            btn_widths[i],
            btn_h,
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
        x += btn_widths[i] + GAP;
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
    state.result
}

/// 폰트 생성 공개 래퍼(설정 창 등 다른 커스텀 창과 공용).
pub unsafe fn make_font_pub(hwnd: HWND, spec: &DlgFont) -> HFONT {
    make_font(hwnd, spec)
}

/// 소유자 중앙 좌표.
unsafe fn center_over(owner: HWND, w: i32, h: i32, dy: i32) -> (i32, i32) {
    // dy = 세로 오프셋(QA 07-14 — 확인창은 아래·진행 창은 위: 서로 겹치지 않게)
    let mut rc = RECT::default();
    if GetWindowRect(owner, &mut rc).is_ok() {
        (
            rc.left + ((rc.right - rc.left) - w) / 2,
            rc.top + ((rc.bottom - rc.top) - h) / 2 + dy,
        )
    } else {
        (200, 200 + dy)
    }
}

// ── 전송 진행 창(비모달 — UI 스레드 소유) ────────────────────────

/// 항목(파일/폴더) 세그먼트 상태(07-21 — 전체 바를 항목 크기 비례로 분할 표시).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SegStatus {
    Pending,
    Active,
    Done,
    Skipped,
    Failed,
}

/// 항목 1개의 진행(크기·완료 바이트·상태) — 워커가 채우고 UI가 스냅샷으로 받는다.
#[derive(Clone, Copy)]
pub struct SegItem {
    pub size: u64,
    pub done: u64,
    pub status: SegStatus,
}

/// 진행 창 상태(GWLP_USERDATA — [`Progress`]가 Box 소유).
struct ProgState {
    /// 완료 카운트다운(초 — Some이면 닫기 모드: 버튼 [닫기 (N)]·0=자동 닫힘, 07-15).
    closing: Option<i32>,
    /// 하단 버튼(취소→닫기 전환·카운트다운 재라벨).
    btn: HWND,
    done: u64,
    total: u64,
    /// 항목별 세그먼트(07-21) — 빈 목록 = 계획 미도착(단색 바 폴백).
    items: Vec<SegItem>,
    /// 표시용 현재 항목 번호(1-기반)·총 항목 수(07-21 — "파일 {0}/{1}").
    cur_item: usize,
    item_count: usize,
    label: Vec<u16>,
    font: HFONT,
    line_h: i32,
    cancelled: bool,
}

static REGISTER_PROG: std::sync::Once = std::sync::Once::new();
const CLASS_PROG: PCWSTR = w!("NexaProgress");
const ID_CANCEL: u32 = 1;

/// [닫기 (N)] 버튼 라벨(카운트다운 — 07-15).
unsafe fn set_btn_countdown(btn: HWND, n: i32) {
    if btn.is_invalid() {
        return;
    }
    let label = format!("{} ({n})", crate::i18n::tr("ops.close"));
    let w = windows::core::HSTRING::from(label);
    let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowTextW(btn, PCWSTR(w.as_ptr()));
}

/// 파일별 진행색 5색 순환 팔레트(사용자 요청 07-21 — 항목 인덱스 % 5).
/// 1색 = 앱 accent 근사(기존 단색 바와 동일), 이후 초록/주황/보라/분홍(BGR).
const SEG_PALETTE: [COLORREF; 5] = [
    COLORREF(0x00D47B26), // 파랑(앱 accent 근사)
    COLORREF(0x0059C734), // 초록
    COLORREF(0x000A9FFF), // 주황
    COLORREF(0x00DE52AF), // 보라
    COLORREF(0x005F37FF), // 분홍
];

/// 세그먼트 진행 바(07-21): 항목별 크기 비례 구간 + 파일별 순환 팔레트색(완료=전체·
/// 진행=부분 채움) + 건너뜀=회색·실패=적색 + 구간 경계선. **모든 항목 최소 3px 보장**
/// (QA 07-21 — 작은 파일 구간이 반올림으로 소멸해 4파일이 3구간으로 보이던 문제:
/// 부족분은 가장 넓은 구간에서 차감). 계획 미도착/항목 과다(>512)는 단색 바 폴백.
unsafe fn paint_segments(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    inner: &RECT,
    items: &[SegItem],
    done: u64,
    total: u64,
) {
    const SKIP: COLORREF = COLORREF(0x00A0A0A0); // 건너뜀 회색
    const FAIL: COLORREF = COLORREF(0x003C3CDC); // 실패 적색(BGR)
    const MIN_W: i32 = 3; // 구간 최소 가시 폭(경계선 1px + 본체)
    let w = (inner.right - inner.left) as i64;
    let fill = |rc: &RECT, c: COLORREF| {
        if rc.right > rc.left {
            let b = CreateSolidBrush(c);
            FillRect(hdc, rc, b);
            let _ = DeleteObject(b.into());
        }
    };
    if total == 0 || items.is_empty() || items.len() > 512 {
        // 폴백 — 단색 바(전체 백분율)
        let pct = if total > 0 {
            (done as f64 / total as f64).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let rc = RECT {
            right: inner.left + (w as f64 * pct) as i32,
            ..*inner
        };
        fill(&rc, SEG_PALETTE[0]);
        return;
    }
    // 1) 크기 비례 폭(누적 경계 — 오차 비누적) → 2) 최소 폭 보정(공간이 허락할 때):
    // 작은/0바이트 항목도 자기 구간이 보이도록 부족분을 가장 넓은 구간에서 가져온다.
    let n = items.len();
    let mut widths: Vec<i32> = Vec::with_capacity(n);
    let mut cum = 0u64;
    let mut prev_x = 0i32;
    for it in items {
        cum += it.size;
        let x = (w * cum as i64 / total as i64) as i32;
        widths.push(x - prev_x);
        prev_x = x;
    }
    if (n as i64) * ((MIN_W + 1) as i64) <= w {
        for i in 0..n {
            while widths[i] < MIN_W {
                // 가장 넓은 구간에서 1px씩 차감(단순·항목 수 소규모 전제)
                let Some(j) = (0..n)
                    .filter(|&j| j != i && widths[j] > MIN_W)
                    .max_by_key(|&j| widths[j])
                else {
                    break;
                };
                widths[j] -= 1;
                widths[i] += 1;
            }
        }
    }
    let mut x0 = inner.left;
    for (k, it) in items.iter().enumerate() {
        let x1 = x0 + widths[k];
        if x1 <= x0 {
            continue; // 최소 폭 보정 불능(항목 과다) — 표시 생략
        }
        let seg = RECT {
            left: x0,
            right: x1,
            ..*inner
        };
        let color = SEG_PALETTE[k % SEG_PALETTE.len()];
        match it.status {
            SegStatus::Done => fill(&seg, color),
            SegStatus::Skipped => fill(&seg, SKIP),
            SegStatus::Failed => fill(&seg, FAIL),
            SegStatus::Active => {
                let frac = if it.size > 0 {
                    (it.done as f64 / it.size as f64).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let rc = RECT {
                    right: x0 + ((x1 - x0) as f64 * frac) as i32,
                    ..seg
                };
                fill(&rc, color);
            }
            SegStatus::Pending => {}
        }
        // 구간 경계선(왼쪽 변)
        if k > 0 {
            let div = RECT {
                left: x0,
                right: x0 + 1,
                ..*inner
            };
            fill(&div, COLORREF(0x00808080));
        }
        x0 = x1;
    }
}

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
                if (*state).closing.is_some() {
                    // 닫기 모드(완료) — 버튼 = 즉시 닫기 트리거(07-15)
                    let _ = KillTimer(Some(hwnd), 1);
                    let _ = DestroyWindow(hwnd);
                } else {
                    (*state).cancelled = true;
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            if !state.is_null() {
                if (*state).closing.is_some() {
                    let _ = KillTimer(Some(hwnd), 1);
                    let _ = DestroyWindow(hwnd); // 완료 후 X = 즉시 닫기
                } else {
                    (*state).cancelled = true; // 진행 중 X = 취소 요청
                }
            }
            LRESULT(0)
        }
        WM_TIMER => {
            // 완료 카운트다운(07-15) — [닫기 (N)] 재라벨, 0 = 자동 닫힘.
            // 첫 틱은 ms 나머지 간격(set_done — 07-21 ms 정밀), 이후 1초 주기로 재무장.
            if !state.is_null() {
                if let Some(n) = (*state).closing {
                    let left = n - 1;
                    if left <= 0 {
                        let _ = KillTimer(Some(hwnd), 1);
                        let _ = DestroyWindow(hwnd);
                    } else {
                        (*state).closing = Some(left);
                        set_btn_countdown((*state).btn, left);
                        SetTimer(Some(hwnd), 1, 1_000, None); // 같은 id = 간격 재설정
                    }
                }
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            if !state.is_null() {
                let old = SelectObject(hdc, (*state).font.into());
                SetBkMode(hdc, TRANSPARENT);
                SetTextColor(hdc, COLORREF(0x00000000));
                let mut crc = RECT::default();
                let _ = GetClientRect(hwnd, &mut crc);
                let lh = (*state).line_h;
                let (done, total) = ((*state).done, (*state).total);
                let pct = if total > 0 {
                    ((done as f64 / total as f64) * 100.0) as i32
                } else if (*state).closing.is_some() {
                    100 // 0바이트 전송 완료(빈 파일 등) — 0%로 닫히지 않게(07-21)
                } else {
                    0
                };
                let mut rc = RECT {
                    left: PAD,
                    top: PAD,
                    right: crc.right - PAD,
                    bottom: PAD + lh,
                };
                let mut label = (*state).label.clone();
                DrawTextW(hdc, &mut label, &mut rc, DT_LEFT);
                // 정보 줄(바 바로 위 — 07-21): 진행/전체 용량(B~TB 적응)·백분율·파일 수
                let mut info = format!("{} / {}  ({pct}%)", fmt_bytes(done), fmt_bytes(total));
                if (*state).item_count > 0 {
                    info.push_str(&format!(
                        "  ·  {}",
                        crate::i18n::trf(
                            "ops.fileCount",
                            &[
                                &(*state).cur_item.to_string(),
                                &(*state).item_count.to_string()
                            ]
                        )
                    ));
                }
                let mut info_w: Vec<u16> = info.encode_utf16().collect();
                let mut rc2 = RECT {
                    left: PAD,
                    top: PAD + lh + 4,
                    right: crc.right - PAD,
                    bottom: PAD + lh * 2 + 4,
                };
                DrawTextW(hdc, &mut info_w, &mut rc2, DT_LEFT);
                // 진행 바(직접 페인트 — comctl32 비의존)
                let bar = RECT {
                    left: PAD,
                    top: rc2.bottom + 6,
                    right: crc.right - PAD,
                    bottom: rc2.bottom + 6 + lh,
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
                paint_segments(hdc, &inner, &(*state).items, done, total);
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
    /// 진행 창 생성(소유자 중앙) — `label`=작업 설명. **대화상자 폰트 공유**(QA 07-14).
    pub unsafe fn open(
        owner: HWND,
        title: &str,
        label: &str,
        font_spec: &DlgFont,
    ) -> Option<Progress> {
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
                hIcon: crate::icon::load(32).unwrap_or_default(), // 원본 아이콘 공통(QA 07-14)
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);
        });
        let font = make_font(owner, font_spec);
        let (_, line_h) = measure(font, "Ag", 400);
        let btn_h = line_h + 10;
        let btn_w = (text_width(font, &crate::i18n::tr("ops.cancel")) + 20).max(64);
        let client_w = 400;
        let client_h = PAD + line_h * 2 + 4 + 6 + line_h + 8 + btn_h + PAD;
        let mut win = RECT {
            right: client_w,
            bottom: client_h,
            ..Default::default()
        };
        let _ = AdjustWindowRectEx(
            &mut win,
            WS_POPUP | WS_CAPTION | WS_SYSMENU,
            false,
            WINDOW_EX_STYLE(0),
        );
        let (w, h) = (win.right - win.left, win.bottom - win.top);
        let mut state = Box::new(ProgState {
            closing: None,
            btn: HWND::default(),
            done: 0,
            total: 0,
            items: Vec::new(),
            cur_item: 0,
            item_count: 0,
            label: label.encode_utf16().collect(),
            font,
            line_h,
            cancelled: false,
        });
        let (cx, cy) = center_over(owner, w, h, -110); // 확인창(아래)과 비겹침(QA 07-14)
        let title_w = windows::core::HSTRING::from(title);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS_PROG,
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
        )
        .ok()?;
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, &mut *state as *mut ProgState as isize);
        let cancel = windows::core::HSTRING::from(crate::i18n::tr("ops.cancel"));
        if let Ok(btn) = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("BUTTON"),
            PCWSTR(cancel.as_ptr()),
            WS_CHILD
                | WS_VISIBLE
                | windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(BS_PUSHBUTTON as u32),
            client_w - PAD - btn_w,
            client_h - PAD - btn_h,
            btn_w,
            btn_h,
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
                Some(WPARAM(state.font.0 as usize)),
                Some(LPARAM(1)),
            );
            state.btn = btn;
        }
        let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(hwnd, SW_SHOWNORMAL);
        Some(Progress { hwnd, state })
    }

    /// 진행 값 갱신 + 다시 그리기(WM_APP_TRANSFER 통지에서 호출).
    /// `items` = 항목별 세그먼트 스냅샷·`cur`/`count` = 파일 {cur}/{count} 표기(07-21).
    pub unsafe fn update(
        &mut self,
        done: u64,
        total: u64,
        items: Vec<SegItem>,
        cur: usize,
        count: usize,
    ) {
        self.state.done = done;
        self.state.total = total;
        self.state.items = items;
        self.state.cur_item = cur;
        self.state.item_count = count;
        // erase=true — WM_PAINT가 TRANSPARENT로 그려 이전 텍스트가 중첩(QA 07-15)
        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(self.hwnd), None, true);
    }

    /// [취소]·X가 눌렸는가(호스트가 워커 cancel 플래그에 반영).
    pub fn cancelled(&self) -> bool {
        self.state.cancelled
    }

    /// 완료 표시(원본 PROG-WIN — **커스텀 카운트다운 닫기 버튼**, 사용자 요청 07-15):
    /// 라벨 교체 + [취소]→[닫기 (N)] 전환. 창 자체 타이머가 1초마다 (N)을
    /// 줄이고 0이 되거나 버튼 클릭 시 창을 닫는다(닫기 트리거 = 버튼). 호스트는
    /// 백스톱 타이머로 구조체만 지연 해제. `ms` = 닫기 대기(설정 `transfer_close_ms`).
    ///
    /// 진행값은 실제 바이트를 유지한다(07-21 — 1/1로 강제하던 것이 "1 B / 1 B" 오표기
    /// 원인이었음). 취소·실패로 미완료면 바도 부분 진행 그대로 남는다(정직 표기).
    pub unsafe fn set_done(&mut self, label: &str, ms: i32) {
        self.state.label = label.encode_utf16().collect();
        // ms 정밀(설정 단위 ms — 07-21 2차): 버튼 표시는 올림 초 [닫기 (N)],
        // 첫 틱 = ms 나머지 간격(이후 wndproc가 1초 주기 재무장) → 총 대기 = 정확히 ms.
        let ms = ms.max(1);
        let n = (ms + 999) / 1_000; // 올림 초(div_ceil — 고정 툴체인 미안정이라 수동)
        self.state.closing = Some(n);
        set_btn_countdown(self.state.btn, n);
        let first = (ms - (n - 1) * 1_000).max(50); // 1..=1000ms(과소 방지 하한)
        SetTimer(Some(self.hwnd), 1, first as u32, None);
        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(self.hwnd), None, true);
    }
}

impl Drop for Progress {
    fn drop(&mut self) {
        unsafe {
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0); // wndproc 상태 참조 해제
            let _ = DestroyWindow(self.hwnd);
            let _ = DeleteObject(self.state.font.into());
        }
    }
}

/// 사람이 읽는 바이트 표기(진행 창 — B/KB/MB/GB/TB 적응, 07-21 TB 추가).
fn fmt_bytes(b: u64) -> String {
    const K: f64 = 1024.0;
    if b < 1024 {
        return format!("{b} B");
    }
    let mut v = b as f64 / K;
    for unit in ["KB", "MB", "GB"] {
        if v < K {
            return format!("{v:.1} {unit}");
        }
        v /= K;
    }
    format!("{v:.1} TB")
}
