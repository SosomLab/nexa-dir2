//! 독립 미리보기 창(사용자 요청 07-26 — 미리보기의 **기본 제공 뷰**).
//! 하단 도크와 같은 `lines`를 **콘솔 폰트 문자 그리드**로 크게 표시 —
//! 플러그인 개발의 **기준 캔버스**(박스 드로잉·표 정렬은 이 창 기준. 도크는 축약 뷰).
//! **모달**(소유자 입력 차단 — 07-26 개편)·F3/도크 ↗에서 연다.
//! 스크롤 = 세로+가로(스크롤바·휠·Shift+휠·방향키·PgUp/Dn·Home/End),
//! **드래그 문자 선택**(경계 밖 = 상하좌우 자동 스크롤 — 타이머 연속) 후
//! Ctrl+C = **rich 복사**(CF_UNICODETEXT + 모노 RTF — 표/박스 정렬 유지).
//! user32/gdi32만 사용(B3 임포트 게이트) — 이미지 미리보기는 도크(WIC) 담당.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, DeleteObject, EndPaint, ExtTextOutW, GetDC, GetTextExtentPoint32W,
    InvalidateRect, ReleaseDC, ScreenToClient, SelectObject, SetBkColor, SetTextColor,
    CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_QUALITY, ETO_CLIPPED, ETO_OPAQUE, FF_DONTCARE,
    FIXED_PITCH, FW_NORMAL, HBRUSH, HDC, HFONT, OUT_DEFAULT_PRECIS, PAINTSTRUCT,
};
use windows::Win32::UI::Controls::SetScrollInfo;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, GetKeyState, ReleaseCapture, SetCapture, VK_CONTROL,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetCursorPos,
    GetMessageW, GetWindowLongPtrW, GetWindowRect, IsWindow, KillTimer, LoadCursorW,
    RegisterClassW, SetForegroundWindow, SetTimer, SetWindowLongPtrW, TranslateMessage,
    GWLP_USERDATA, IDC_IBEAM, MSG, SB_HORZ, SB_VERT, SCROLLINFO, SIF_PAGE, SIF_POS, SIF_RANGE,
    WINDOW_EX_STYLE, WM_CLOSE, WM_DESTROY, WM_ERASEBKGND, WM_HSCROLL, WM_KEYDOWN, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_SIZE, WM_TIMER, WM_VSCROLL,
    WNDCLASSW, WS_HSCROLL, WS_OVERLAPPEDWINDOW, WS_VISIBLE, WS_VSCROLL,
};

const CLASS: PCWSTR = w!("NexaPreviewWnd");
/// 좌측 여백(px) — 문자 원점.
const PAD_X: i32 = 8;
/// 드래그 자동 스크롤 연속 타이머(커서 정지 상태에서도 계속 — 07-26).
const TIMER_DRAG: usize = 1;

/// 라인 종류(스타일드 렌더 — `\u{2}tag|` 계약, 07-26 GitHub 근사):
/// 0=본문(프로포셔널) 1..=3=h1~h3(굵게·h1/h2 밑줄 괘선) 4=코드(밴드+모노)
/// 5=인용(바+흐림) 6=모노(표·다이어그램 아트) 7=수평선.
fn parse_kind(l: &str) -> (u8, &str) {
    for (tag, k) in [
        ("\u{2}h1|", 1u8),
        ("\u{2}h2|", 2),
        ("\u{2}h3|", 3),
        ("\u{2}code|", 4),
        ("\u{2}q|", 5),
        ("\u{2}mono|", 6),
        ("\u{2}hr|", 7),
    ] {
        if let Some(r) = l.strip_prefix(tag) {
            return (k, r);
        }
    }
    (0, l)
}

struct PvState {
    /// 콘솔(모노) 폰트 — 코드·표·다이어그램 아트.
    font: HFONT,
    /// 본문 프로포셔널(Segoe UI)·굵게(제목) — GitHub 근사(07-26).
    body: HFONT,
    body_bold: HFONT,
    /// 라인 종류(parse_kind — text와 병렬).
    kinds: Vec<u8>,
    /// 라인 원문(선택/복사) — 탭 4칸 치환 후.
    text: Vec<String>,
    /// UTF-16(NUL 종료 — 그리기).
    lines: Vec<Vec<u16>>,
    line_h: i32,
    /// 최장 라인 픽셀 폭(가로 스크롤 상한).
    max_w: i32,
    /// 첫 가시 라인(세로)·가로 픽셀 오프셋.
    top: i32,
    left: i32,
    dark: bool,
    /// 문자 선택 (앵커, 현재) = (라인, 문자 경계) — 도크 규약 동일(절대 인덱스).
    sel: Option<((usize, usize), (usize, usize))>,
    drag: bool,
    /// 인라인 이미지 캐시(07-26 다이어그램) — 경로 → (w, h, BGRA top-down).
    images: std::collections::HashMap<String, Option<(i32, i32, Vec<u8>)>>,
}

/// 자체 기록 32bpp BMP 로드(star.rs render_svg 산출물 — BGRA top-down 반환).
fn load_bmp(path: &str) -> Option<(i32, i32, Vec<u8>)> {
    let b = std::fs::read(path).ok()?;
    if b.len() < 54 || &b[0..2] != b"BM" {
        return None;
    }
    let off = u32::from_le_bytes(b[10..14].try_into().ok()?) as usize;
    let w = i32::from_le_bytes(b[18..22].try_into().ok()?);
    let h_raw = i32::from_le_bytes(b[22..26].try_into().ok()?);
    let bpp = u16::from_le_bytes(b[28..30].try_into().ok()?);
    if bpp != 32 || w <= 0 || w > 4096 || h_raw == 0 || h_raw.abs() > 4096 {
        return None;
    }
    let h = h_raw.abs();
    let need = off + (w * h * 4) as usize;
    if b.len() < need {
        return None;
    }
    let mut px = b[off..need].to_vec();
    if h_raw > 0 {
        // bottom-up → top-down 뒤집기(자체 산출물은 top-down이라 보통 불필요)
        let stride = (w * 4) as usize;
        let mut flipped = vec![0u8; px.len()];
        for r in 0..h as usize {
            flipped[r * stride..(r + 1) * stride]
                .copy_from_slice(&px[(h as usize - 1 - r) * stride..][..stride]);
        }
        px = flipped;
    }
    Some((w, h, px))
}

/// 테마색(BGR COLORREF) — nexa-gui Theme 다크/라이트 토큰과 동일 값.
/// (배경, 본문, 선택 배경).
fn colors(dark: bool) -> (COLORREF, COLORREF, COLORREF) {
    if dark {
        (
            COLORREF(0x00211C19), // panel_bg 0x191C21
            COLORREF(0x00E0DAD6), // text 0xD6DAE0
            COLORREF(0x005F4024), // sel_bg 0x24405F
        )
    } else {
        (
            COLORREF(0x00FFFFFF),
            COLORREF(0x00261F1B), // text 0x1B1F26
            COLORREF(0x00FFE8D8), // sel_bg 0xD8E8FF
        )
    }
}

static REGISTER: std::sync::Once = std::sync::Once::new();

unsafe fn ensure_class() {
    REGISTER.call_once(|| {
        let wc = WNDCLASSW {
            lpszClassName: CLASS,
            lpfnWndProc: Some(pv_proc),
            hInstance: windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
                .unwrap_or_default()
                .into(),
            hbrBackground: HBRUSH(std::ptr::null_mut()), // WM_PAINT가 불투명 도장
            hCursor: LoadCursorW(None, IDC_IBEAM).unwrap_or_default(), // 텍스트 선택 캔버스
            hIcon: crate::icon::load(32).unwrap_or_default(),
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);
    });
}

/// 콘솔 폰트 생성 — 설정 `term_font` 1순위 패밀리 + 크기(DPI 반영·FIXED_PITCH 힌트).
unsafe fn make_mono(hwnd: HWND, family: &str, size_pt: i32) -> HFONT {
    let dpi = GetDpiForWindow(hwnd).max(96);
    let h = -((size_pt.clamp(8, 32) * dpi as i32) / 72);
    let first = family.split(',').next().unwrap_or("Consolas").trim();
    let face = windows::core::HSTRING::from(first);
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
        (FIXED_PITCH.0 | FF_DONTCARE.0) as u32,
        PCWSTR(face.as_ptr()),
    )
}

/// 텍스트 픽셀 폭(현재 폰트).
/// 본문 프로포셔널 폰트(Segoe UI — GitHub 근사·07-26). `bold` = 제목.
unsafe fn make_ui(hwnd: HWND, size_pt: i32, bold: bool) -> HFONT {
    let dpi = GetDpiForWindow(hwnd).max(96);
    let h = -((size_pt.clamp(8, 32) * dpi as i32) / 72);
    CreateFontW(
        h,
        0,
        0,
        0,
        if bold { 700 } else { FW_NORMAL.0 as i32 },
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        DEFAULT_QUALITY,
        FF_DONTCARE.0 as u32,
        w!("Segoe UI"),
    )
}

unsafe fn text_w(hdc: HDC, text: &str) -> i32 {
    if text.is_empty() {
        return 0;
    }
    let wide: Vec<u16> = text.encode_utf16().collect();
    let mut sz = SIZE::default();
    let _ = GetTextExtentPoint32W(hdc, &wide, &mut sz);
    sz.cx
}

/// 라인 종류 → 폰트(측정·그리기 공용 — 지표 일치).
fn line_font(st: &PvState, line: usize) -> HFONT {
    match st.kinds.get(line).copied().unwrap_or(0) {
        1..=3 => st.body_bold,
        4 | 6 => st.font,
        _ => st.body,
    }
}

/// 라인의 문자 경계 x 오프셋(px — 도크 offsets 규약: [0, w1, w1+w2, …]).
/// 이미지 마커/패드 라인 = 경계 0 하나(선택 대상 아님 — 07-26).
unsafe fn char_offsets(hwnd: HWND, st: &PvState, line: usize) -> Vec<i32> {
    if st.text[line].starts_with('\u{1}') {
        return vec![0];
    }
    let hdc = GetDC(Some(hwnd));
    let old = SelectObject(hdc, line_font(st, line).into());
    let mut offs = vec![0i32];
    let mut prefix = String::new();
    for c in st.text[line].chars() {
        prefix.push(c);
        offs.push(text_w(hdc, &prefix));
    }
    SelectObject(hdc, old);
    ReleaseDC(Some(hwnd), hdc);
    offs
}

unsafe fn client(hwnd: HWND) -> RECT {
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    rc
}

unsafe fn visible_rows(hwnd: HWND, st: &PvState) -> i32 {
    ((client(hwnd).bottom) / st.line_h.max(1)).max(1)
}

/// 스크롤 클램프 + 스크롤바 동기(세로 = 라인·가로 = px).
unsafe fn sync_scroll(hwnd: HWND, st: &mut PvState) {
    let vis = visible_rows(hwnd, st);
    st.top = st.top.clamp(0, (st.lines.len() as i32 - vis).max(0));
    let cw = client(hwnd).right;
    st.left = st.left.clamp(0, (st.max_w + PAD_X * 2 - cw).max(0));
    let vsi = SCROLLINFO {
        cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
        fMask: SIF_PAGE | SIF_POS | SIF_RANGE,
        nMin: 0,
        nMax: (st.lines.len() as i32 - 1).max(0),
        nPage: vis as u32,
        nPos: st.top,
        ..Default::default()
    };
    let _ = SetScrollInfo(hwnd, SB_VERT, &vsi, true);
    let hsi = SCROLLINFO {
        cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
        fMask: SIF_PAGE | SIF_POS | SIF_RANGE,
        nMin: 0,
        nMax: (st.max_w + PAD_X * 2 - 1).max(0),
        nPage: cw.max(0) as u32,
        nPos: st.left,
        ..Default::default()
    };
    let _ = SetScrollInfo(hwnd, SB_HORZ, &hsi, true);
}

unsafe fn scroll_to(hwnd: HWND, st: &mut PvState, top: i32, left: i32) {
    let (bt, bl) = (st.top, st.left);
    st.top = top;
    st.left = left;
    sync_scroll(hwnd, st);
    if (st.top, st.left) != (bt, bl) {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

/// 클라이언트 좌표 → (라인, 최근접 문자 경계). 영역 밖은 첫/끝으로 클램프.
unsafe fn hit(hwnd: HWND, st: &PvState, x: i32, y: i32) -> (usize, usize) {
    if st.text.is_empty() {
        return (0, 0);
    }
    let row = st.top + (y - PAD_X / 2).div_euclid(st.line_h.max(1));
    let line = row.clamp(0, st.text.len() as i32 - 1) as usize;
    let offs = char_offsets(hwnd, st, line);
    let rel = x - PAD_X + st.left;
    let mut best = 0usize;
    let mut bd = i32::MAX;
    for (i, o) in offs.iter().enumerate() {
        let d = (o - rel).abs();
        if d < bd {
            bd = d;
            best = i;
        }
    }
    (line, best)
}

/// 선택 텍스트(정규화 — Ctrl+C 복사. 도크 selected_text 규약 동일).
fn selected_text(st: &PvState) -> Option<String> {
    let (a, c) = st.sel?;
    let (lo, hi) = if a <= c { (a, c) } else { (c, a) };
    if lo == hi || lo.0 >= st.text.len() {
        return None;
    }
    // 이미지 마커/패드 라인은 복사 제외(07-26 — 제어 문자 유출 방지)
    let chars_of = |l: usize| {
        let s = &st.text[l];
        if s.starts_with('\u{1}') {
            Vec::new()
        } else {
            s.chars().collect::<Vec<char>>()
        }
    };
    let (ll, lc) = lo;
    let (hl, hc) = (hi.0.min(st.text.len() - 1), hi.1);
    if ll == hl {
        let cs = chars_of(ll);
        let (s, e) = (lc.min(cs.len()), hc.min(cs.len()));
        return (e > s).then(|| cs[s..e].iter().collect());
    }
    let mut parts = Vec::with_capacity(hl - ll + 1);
    let f = chars_of(ll);
    parts.push(f[lc.min(f.len())..].iter().collect::<String>());
    for l in ll + 1..hl {
        let s = &st.text[l];
        parts.push(if s.starts_with('\u{1}') {
            String::new()
        } else {
            s.clone()
        });
    }
    let t = chars_of(hl);
    parts.push(t[..hc.min(t.len())].iter().collect::<String>());
    Some(parts.join("\r\n"))
}

/// 드래그 중 자동 스크롤(07-26 — 상하좌우): 커서가 영역 밖이면 한 스텝 이동 후
/// 선택 확장. WM_MOUSEMOVE·WM_TIMER 공용.
unsafe fn drag_track(hwnd: HWND, st: &mut PvState, x: i32, y: i32) {
    let rc = client(hwnd);
    let (mut top, mut left) = (st.top, st.left);
    if y < 0 {
        top -= 1;
    } else if y >= rc.bottom {
        top += 1;
    }
    if x < 0 {
        left -= 24;
    } else if x >= rc.right {
        left += 24;
    }
    scroll_to(hwnd, st, top, left);
    let pos = hit(hwnd, st, x, y);
    if let Some((a, cur)) = st.sel {
        if cur != pos {
            st.sel = Some((a, pos));
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }
}

unsafe extern "system" fn pv_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut PvState;
    match msg {
        WM_ERASEBKGND => LRESULT(1), // 전면 도장(깜빡임 방지)
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            if !state.is_null() {
                let st = &mut *state;
                let (bg, fg, selbg) = colors(st.dark);
                let old = SelectObject(hdc, st.font.into());
                let rc = client(hwnd);
                let vis = visible_rows(hwnd, st);
                let sel = st.sel.map(|(a, c)| if a <= c { (a, c) } else { (c, a) });
                let x0 = PAD_X - st.left;
                let pad_top = PAD_X / 2;
                let mut y = pad_top;
                // 인라인 이미지(07-26 다이어그램) — 행 bg 도장 후 마지막에 그림
                // (pad 행의 불투명 도장이 이미지를 지우지 않도록 지연)
                let mut deferred: Vec<(i32, i32, String)> = Vec::new();
                for i in st.top..(st.top + vis + 1).min(st.lines.len() as i32) {
                    let li = i as usize;
                    let is_marker = st.text[li].starts_with('\u{1}');
                    if is_marker {
                        let row = RECT {
                            left: rc.left,
                            top: y,
                            right: rc.right,
                            bottom: y + st.line_h,
                        };
                        SetBkColor(hdc, bg);
                        let _ = ExtTextOutW(hdc, 0, 0, ETO_OPAQUE, Some(&row), w!(""), 0, None);
                        if let Some(path) = st.text[li]
                            .strip_prefix(nexa_gui::widgets::dock::IMG_MARKER)
                            .map(str::to_string)
                        {
                            let k = 1 + st.text[li + 1..]
                                .iter()
                                .take_while(|l| {
                                    l.as_str() == nexa_gui::widgets::dock::IMG_PAD
                                })
                                .count() as i32;
                            deferred.push(((st.line_h * k).min(rc.bottom - y), y, path));
                        }
                        y += st.line_h;
                        continue;
                    }
                    let row = RECT {
                        left: rc.left,
                        top: y,
                        right: rc.right,
                        bottom: y + st.line_h,
                    };
                    // 종류 스타일(07-26 GitHub 근사) — 폰트·색·밴드·괘선
                    let kind = st.kinds.get(li).copied().unwrap_or(0);
                    let (dim, band, border) = if st.dark {
                        (COLORREF(0x009C918A), COLORREF(0x00332B26), COLORREF(0x00463C36))
                    } else {
                        (COLORREF(0x006A6057), COLORREF(0x00FAF8F6), COLORREF(0x00E4DED8))
                    };
                    if kind == 7 {
                        // 수평선 — 행 배경 + 중앙 1px 괘선
                        let row = RECT {
                            left: rc.left,
                            top: y,
                            right: rc.right,
                            bottom: y + st.line_h,
                        };
                        SetBkColor(hdc, bg);
                        let _ = ExtTextOutW(hdc, 0, 0, ETO_OPAQUE, Some(&row), w!(""), 0, None);
                        SetBkColor(hdc, border);
                        let lr = RECT {
                            left: rc.left + PAD_X,
                            top: y + st.line_h / 2,
                            right: rc.right - PAD_X,
                            bottom: y + st.line_h / 2 + 1,
                        };
                        let _ = ExtTextOutW(hdc, 0, 0, ETO_OPAQUE, Some(&lr), w!(""), 0, None);
                        y += st.line_h;
                        continue;
                    }
                    SelectObject(hdc, line_font(st, li).into());
                    let bg = if kind == 4 { band } else { bg }; // 코드 = 배경 밴드
                    let fg = if kind == 5 { dim } else { fg }; // 인용 = 흐림
                    // 선택 구간 = 3분할(앞/선택/뒤 — 모노 그리드라 경계 px는 오프셋 합)
                    let seg = sel
                        .filter(|&((ll, _), (hl, _))| ll <= li && li <= hl)
                        .map(|((ll, lc), (hl, hc))| {
                            let offs = char_offsets(hwnd, st, li);
                            let last = offs.len() - 1;
                            let s = if li == ll { lc.min(last) } else { 0 };
                            let e = if li == hl { hc.min(last) } else { last };
                            (offs[s], offs[e])
                        })
                        .filter(|(s, e)| e > s);
                    let text = &st.lines[li];
                    let n = (text.len() - 1) as u32; // NUL 제외
                    match seg {
                        None => {
                            SetBkColor(hdc, bg);
                            SetTextColor(hdc, fg);
                            let _ = ExtTextOutW(
                                hdc,
                                x0,
                                y,
                                ETO_OPAQUE | ETO_CLIPPED,
                                Some(&row),
                                PCWSTR(text.as_ptr()),
                                n,
                                None,
                            );
                        }
                        Some((s, e)) => {
                            // 배경(행 전체) → 선택 배경 → 텍스트 1회(투명 아님 —
                            // 텍스트는 불투명 배경 없이 다시 그리면 이음새가 없다)
                            SetBkColor(hdc, bg);
                            let _ = ExtTextOutW(hdc, 0, 0, ETO_OPAQUE, Some(&row), w!(""), 0, None);
                            let selrc = RECT {
                                left: x0 + s,
                                top: y,
                                right: x0 + e,
                                bottom: y + st.line_h,
                            };
                            SetBkColor(hdc, selbg);
                            let _ =
                                ExtTextOutW(hdc, 0, 0, ETO_OPAQUE, Some(&selrc), w!(""), 0, None);
                            SetBkColor(hdc, bg);
                            SetTextColor(hdc, fg);
                            let _ = windows::Win32::Graphics::Gdi::SetBkMode(
                                hdc,
                                windows::Win32::Graphics::Gdi::TRANSPARENT,
                            );
                            let _ = ExtTextOutW(
                                hdc,
                                x0,
                                y,
                                ETO_CLIPPED,
                                Some(&row),
                                PCWSTR(text.as_ptr()),
                                n,
                                None,
                            );
                            let _ = windows::Win32::Graphics::Gdi::SetBkMode(
                                hdc,
                                windows::Win32::Graphics::Gdi::OPAQUE,
                            );
                        }
                    }
                    if kind == 5 {
                        // 인용 좌측 바(GitHub 관례)
                        SetBkColor(hdc, dim);
                        let bar = RECT {
                            left: 2,
                            top: y + 2,
                            right: 5,
                            bottom: y + st.line_h - 2,
                        };
                        let _ = ExtTextOutW(hdc, 0, 0, ETO_OPAQUE, Some(&bar), w!(""), 0, None);
                    }
                    if kind == 1 || kind == 2 {
                        // h1/h2 밑줄 괘선(GitHub 관례)
                        SetBkColor(hdc, border);
                        let ul = RECT {
                            left: rc.left,
                            top: y + st.line_h - 1,
                            right: rc.right,
                            bottom: y + st.line_h,
                        };
                        let _ = ExtTextOutW(hdc, 0, 0, ETO_OPAQUE, Some(&ul), w!(""), 0, None);
                    }
                    y += st.line_h;
                }
                // 지연 이미지 드로우(07-26 — pad 행 불투명 도장 이후 최종)
                for (area_h, iy, path) in deferred {
                    let entry = st.images.entry(path.clone()).or_insert_with(|| load_bmp(&path));
                    if let Some((iw, ih, bits)) = entry {
                        use windows::Win32::Graphics::Gdi::{
                            StretchDIBits, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
                            SRCCOPY,
                        };
                        let avail_w = (rc.right - PAD_X * 2).max(1);
                        let scale = (avail_w as f32 / *iw as f32)
                            .min(area_h as f32 / *ih as f32)
                            .min(1.0);
                        let (dw, dh) =
                            (((*iw as f32) * scale) as i32, ((*ih as f32) * scale) as i32);
                        if dw > 0 && dh > 0 {
                            let bmi = BITMAPINFO {
                                bmiHeader: BITMAPINFOHEADER {
                                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                                    biWidth: *iw,
                                    biHeight: -*ih, // top-down
                                    biPlanes: 1,
                                    biBitCount: 32,
                                    biCompression: BI_RGB.0,
                                    ..Default::default()
                                },
                                ..Default::default()
                            };
                            StretchDIBits(
                                hdc,
                                PAD_X, // 좌측 정렬(GitHub 관례 — QA 07-26)
                                iy + (area_h - dh) / 2,
                                dw,
                                dh,
                                0,
                                0,
                                *iw,
                                *ih,
                                Some(bits.as_ptr() as *const core::ffi::c_void),
                                &bmi,
                                DIB_RGB_COLORS,
                                SRCCOPY,
                            );
                        }
                    }
                }
                // 잔여 배경(상단 패드 + 마지막 라인 아래)
                for r in [
                    RECT {
                        left: rc.left,
                        top: 0,
                        right: rc.right,
                        bottom: pad_top,
                    },
                    RECT {
                        left: rc.left,
                        top: y,
                        right: rc.right,
                        bottom: rc.bottom,
                    },
                ] {
                    if r.bottom > r.top {
                        SetBkColor(hdc, bg);
                        let _ = ExtTextOutW(hdc, 0, 0, ETO_OPAQUE, Some(&r), w!(""), 0, None);
                    }
                }
                SelectObject(hdc, old);
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_SIZE => {
            if !state.is_null() {
                sync_scroll(hwnd, &mut *state);
            }
            LRESULT(0)
        }
        WM_VSCROLL => {
            if !state.is_null() {
                let st = &mut *state;
                let vis = visible_rows(hwnd, st);
                let code = (wparam.0 & 0xFFFF) as u32;
                let pos = (wparam.0 >> 16) as i32;
                let top = match code {
                    0 => st.top - 1,
                    1 => st.top + 1,
                    2 => st.top - vis,
                    3 => st.top + vis,
                    4 | 5 => pos,
                    6 => 0,
                    7 => i32::MAX,
                    _ => st.top,
                };
                let left = st.left;
                scroll_to(hwnd, st, top, left);
            }
            LRESULT(0)
        }
        WM_HSCROLL => {
            if !state.is_null() {
                let st = &mut *state;
                let cw = client(hwnd).right;
                let code = (wparam.0 & 0xFFFF) as u32;
                let pos = (wparam.0 >> 16) as i32;
                let left = match code {
                    0 => st.left - 24,
                    1 => st.left + 24,
                    2 => st.left - cw,
                    3 => st.left + cw,
                    4 | 5 => pos,
                    6 => 0,
                    7 => i32::MAX,
                    _ => st.left,
                };
                let top = st.top;
                scroll_to(hwnd, st, top, left);
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            if !state.is_null() {
                let st = &mut *state;
                let delta = ((wparam.0 >> 16) as i16) as i32;
                if wparam.0 & 0x0004 != 0 {
                    // MK_SHIFT = 가로(터미널 규약 동일)
                    let left = st.left - delta;
                    let top = st.top;
                    scroll_to(hwnd, st, top, left);
                } else {
                    let top = st.top - delta / 40; // 120 = 3행
                    let left = st.left;
                    scroll_to(hwnd, st, top, left);
                }
            }
            LRESULT(0)
        }
        0x020E /* WM_MOUSEHWHEEL */ => {
            if !state.is_null() {
                let st = &mut *state;
                let delta = ((wparam.0 >> 16) as i16) as i32;
                let left = st.left + delta;
                let top = st.top;
                scroll_to(hwnd, st, top, left);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            if !state.is_null() {
                let st = &mut *state;
                let (x, y) = ((lparam.0 & 0xFFFF) as i16 as i32, (lparam.0 >> 16) as i16 as i32);
                let pos = hit(hwnd, st, x, y);
                st.sel = Some((pos, pos));
                st.drag = true;
                SetCapture(hwnd);
                SetTimer(Some(hwnd), TIMER_DRAG, 50, None); // 경계 밖 연속 스크롤(07-26)
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if !state.is_null() && (*state).drag {
                let (x, y) = ((lparam.0 & 0xFFFF) as i16 as i32, (lparam.0 >> 16) as i16 as i32);
                drag_track(hwnd, &mut *state, x, y);
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == TIMER_DRAG && !state.is_null() && (*state).drag {
                // 커서 정지 상태에서도 경계 밖이면 계속 스크롤(07-26)
                let mut pt = POINT::default();
                let _ = GetCursorPos(&mut pt);
                let _ = ScreenToClient(hwnd, &mut pt);
                let rc = client(hwnd);
                if pt.x < 0 || pt.y < 0 || pt.x >= rc.right || pt.y >= rc.bottom {
                    drag_track(hwnd, &mut *state, pt.x, pt.y);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            if !state.is_null() {
                let st = &mut *state;
                if st.drag {
                    st.drag = false;
                    let _ = ReleaseCapture();
                    let _ = KillTimer(Some(hwnd), TIMER_DRAG);
                    if let Some((a, c)) = st.sel {
                        if a == c {
                            st.sel = None; // 이동 없는 단순 클릭 = 선택 없음(도크 규약)
                            let _ = InvalidateRect(Some(hwnd), None, false);
                        }
                    }
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if !state.is_null() {
                let st = &mut *state;
                let vis = visible_rows(hwnd, st);
                let ctrl = GetKeyState(VK_CONTROL.0 as i32) < 0;
                let (top, left) = (st.top, st.left);
                match wparam.0 as u32 {
                    0x1B => {
                        let _ = DestroyWindow(hwnd); // ESC
                    }
                    0x43 if ctrl => {
                        // Ctrl+C = rich 복사(07-26 — 평문 + 모노 RTF 동시 게시)
                        if let Some(t) = selected_text(st) {
                            let _ = crate::clipboard::write_text_rich(hwnd, &t);
                        }
                    }
                    0x26 => scroll_to(hwnd, st, top - 1, left),
                    0x28 => scroll_to(hwnd, st, top + 1, left),
                    0x25 => scroll_to(hwnd, st, top, left - 24), // ← 가로
                    0x27 => scroll_to(hwnd, st, top, left + 24), // →
                    0x21 => scroll_to(hwnd, st, top - vis, left),
                    0x22 => scroll_to(hwnd, st, top + vis, left),
                    0x24 => scroll_to(hwnd, st, 0, 0),
                    0x23 => scroll_to(hwnd, st, i32::MAX, left),
                    _ => {}
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            if !state.is_null() {
                let st = Box::from_raw(state);
                let _ = DeleteObject(st.font.into());
                let _ = DeleteObject(st.body.into());
                let _ = DeleteObject(st.body_bold.into());
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// 독립 미리보기 창(**모달** — 소유자 입력 차단·닫힐 때까지 블로킹, 07-26).
/// `mono` = (설정 term_font, 크기 pt) — 플러그인 기준 캔버스는 콘솔 폰트 그리드.
pub unsafe fn show(owner: HWND, title: &str, lines: Vec<String>, mono: (&str, i32), dark: bool) {
    ensure_class();
    let title_w = windows::core::HSTRING::from(format!("{title} — Preview"));
    // 소유자 중앙·3/4 크기(이후 리사이즈 자유)
    let (mut w, mut h) = (900, 640);
    let (mut x, mut y) = (120, 120);
    let mut rc = RECT::default();
    if GetWindowRect(owner, &mut rc).is_ok() {
        w = ((rc.right - rc.left) * 3 / 4).clamp(480, 1400);
        h = ((rc.bottom - rc.top) * 3 / 4).clamp(360, 1000);
        x = rc.left + ((rc.right - rc.left) - w) / 2;
        y = rc.top + ((rc.bottom - rc.top) - h) / 2;
    }
    let Ok(hwnd) = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        CLASS,
        PCWSTR(title_w.as_ptr()),
        WS_OVERLAPPEDWINDOW | WS_VISIBLE | WS_VSCROLL | WS_HSCROLL,
        x,
        y,
        w,
        h,
        Some(owner),
        None,
        None,
        None,
    ) else {
        return;
    };
    let font = make_mono(hwnd, mono.0, mono.1);
    let body = make_ui(hwnd, mono.1, false);
    let body_bold = make_ui(hwnd, mono.1, true);
    // 종류 태그 분리(\u{2}tag| — 표시/복사 텍스트는 태그 없이 저장)
    let parsed: Vec<(u8, String)> = lines
        .into_iter()
        .map(|l| {
            let r = l.replace('\t', "    ");
            let (k, t) = parse_kind(&r);
            (k, t.to_string())
        })
        .collect();
    let kinds: Vec<u8> = parsed.iter().map(|p| p.0).collect();
    let text: Vec<String> = parsed.into_iter().map(|p| p.1).collect();
    // 지표·최장 폭 실측(라인별 폰트 — 가로 스크롤 상한·행 높이 = 최대 폰트)
    let (line_h, max_w) = {
        let hdc = GetDC(Some(hwnd));
        let mut lh = 12;
        for f in [font, body, body_bold] {
            let old = SelectObject(hdc, f.into());
            let mut sz = SIZE::default();
            let probe: Vec<u16> = "Ag한".encode_utf16().collect();
            let _ = GetTextExtentPoint32W(hdc, &probe, &mut sz);
            lh = lh.max(sz.cy);
            SelectObject(hdc, old);
        }
        let mut mw = 0;
        for (i, l) in text.iter().enumerate() {
            if l.starts_with('\u{1}') {
                continue;
            }
            let f = match kinds[i] {
                1..=3 => body_bold,
                4 | 6 => font,
                _ => body,
            };
            let old = SelectObject(hdc, f.into());
            mw = mw.max(text_w(hdc, l));
            SelectObject(hdc, old);
        }
        ReleaseDC(Some(hwnd), hdc);
        (lh + 3, mw)
    };
    let wide: Vec<Vec<u16>> = text
        .iter()
        .map(|l| l.encode_utf16().chain(std::iter::once(0)).collect())
        .collect();
    let mut state = Box::new(PvState {
        font,
        body,
        body_bold,
        kinds,
        text,
        lines: wide,
        line_h,
        max_w,
        top: 0,
        left: 0,
        dark,
        sel: None,
        drag: false,
        images: std::collections::HashMap::new(),
    });
    sync_scroll(hwnd, &mut state);
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
    // 모달(07-26 — about.rs 규약): 소유자 입력 차단 + 자체 펌프
    let _ = EnableWindow(owner, false);
    let _ = SetForegroundWindow(hwnd);
    let mut msg = MSG::default();
    while IsWindow(Some(hwnd)).as_bool() && GetMessageW(&mut msg, None, 0, 0).as_bool() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    let _ = EnableWindow(owner, true);
    let _ = SetForegroundWindow(owner);
}
