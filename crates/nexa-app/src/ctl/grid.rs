//! grid — **NxGrid** 기본 그리드(ctl 14호 — 사용자 확정 07-18: "가장 기본
//! 기능의 Grid + 나중에 확장 그리드로 확장 가능한 설계".
//! 라이브러리 추상화 — 앱 비결합).
//!
//! ## 계약(판매용 명세 — 클래스 `Nexa.NxGrid`)
//! - **기본 기능(코어)**: 컬럼 헤더 + **경계 드래그 리사이즈**(최소 폭 클램프·
//!   IDC_SIZEWE 커서) · **오버레이 스크롤바 세로/가로**(macOS 시안 07-18 —
//!   스크롤 중에만 얇은 썸 표시 후 페이드아웃, 썸 드래그 시 트랙 있는 일반
//!   바 모드. 컬럼 합이 폭을 넘으면 가로 스크롤) · 행 = 텍스트 셀.
//! - **확장 설계**(Rust — 상속 대신 **코어 재사용 + 셀 종류 데이터화**,
//!   NxIconButton Icon enum과 동일 규약): 셀 표현은 [`GridRow`]의 마크/텍스트
//!   데이터로 확장한다. 확장 스위치 = [`GridOpts`](헤더 숨김·지브라·외곽선·
//!   [`Mark`]): `Mark::Check` = 컬럼 0 체크박스(행별 `Option<bool>` — `None` =
//!   표시 없음, 클릭 토글)·`Mark::Minus` = 행 우측 빨간 ⊖ 삭제 버튼(클릭 =
//!   NXGR_TOGGLE 통지 — 삭제 판단은 호스트). 추가 셀 종류(아이콘·게이지 등)는
//!   같은 방식의 변형으로 코어를 재사용해 확장한다(레이아웃·히트테스트·스크롤 불변).
//! - 데이터: [`set_rows`](전체 교체 — 크레이트 내 직접 API)·[`row_check`].
//! - 체크 토글 시 부모에 `WM_COMMAND(MAKEWPARAM(id, NXGR_TOGGLE))` →
//!   [`NXGR_GETROW`]로 마지막 토글 행 조회.
//! - **행 선택**(07-18 — 파일 목록 그리드와 동일 규약): 클릭 = 단일 선택,
//!   Shift+클릭/방향키 = 앵커 기준 연속 선택, Ctrl+클릭 = 비연속 토글,
//!   Ctrl+방향키 = 포커스만 이동 후 Space(Ctrl+Space) = 토글, Ctrl+A = 전체,
//!   Home/End/PgUp/PgDn 내비. 선택 변경 시 `NXGR_SELCHANGE` 통지,
//!   조회 = [`selected_rows`]. 체크 열 클릭은 선택을 바꾸지 않는다(적용 토글 전용).
//! - 도형(체크·썸) = AA(DrawCtx 백엔드 규약) · 텍스트 = GDI(1px 하향 보정).
//!   **빈 문자열 셀은 그리지 않는다**(빈 Vec = 댕글링 포인터 → user32 AV —
//!   07-18 크래시 진범 채록).

use nexa_gui::{DrawCtx, Rect};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
    DrawTextW, EndPaint, InvalidateRect, SelectObject, SetBkMode, SetTextColor, DT_END_ELLIPSIS,
    DT_LEFT, DT_SINGLELINE, DT_VCENTER, HFONT, PAINTSTRUCT, SRCCOPY, TRANSPARENT,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, SetFocus, VK_CONTROL, VK_SHIFT};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetClientRect, KillTimer, SendMessageW, SetTimer,
    DLGC_WANTARROWS, DLGC_WANTCHARS, HMENU, IDC_SIZEWE, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CREATE,
    WM_DESTROY, WM_GETDLGCODE, WM_KEYDOWN, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_SETCURSOR, WM_SETFOCUS, WM_SETFONT, WM_SIZE,
    WM_TIMER, WS_CHILD, WS_TABSTOP, WS_VISIBLE,
};

use super::gdipctx::{color, GdipCtx};
use super::style::{fill, font_height, Style};

/// 체크 토글 통지(WM_COMMAND HIWORD) — 행은 [`NXGR_GETROW`]로.
/// `NXGR_GETROW == -2` = **헤더 전체 토글**(07-18 — 호스트는 전 행 재동기).
pub const NXGR_TOGGLE: u32 = 1;
/// 헤더 전체 토글 표식([`NXGR_GETROW`] 반환값).
pub const NXGR_ROW_ALL: isize = -2;
/// 정렬 변경 통지(WM_COMMAND HIWORD — 07-18 원본 docs/23 §4 이식).
/// 비교는 **호스트**가 수행(컨트롤 = 상태·표시·통지만 — 도메인 비종속):
/// [`sort_spec`]으로 (컬럼, desc) 목록을 읽어 행을 재정렬해 [`set_rows`]로 반영.
pub const NXGR_SORT: u32 = 3;
/// 행 선택 변경 통지(WM_COMMAND HIWORD) — 조회는 [`selected_rows`].
pub const NXGR_SELCHANGE: u32 = 2;
/// 마지막 토글 행 인덱스 조회(없음 = -1).
pub const NXGR_GETROW: u32 = 0x0400 + 120;

/// 컬럼(제목·폭 — 리사이즈 최소 40px).
struct GridCol {
    title: String,
    width: i32,
}
const COL_MIN: i32 = 40;

/// 오버레이 스크롤바(07-18 시안): 평소 숨김 → 스크롤 순간 표시 → 페이드.
const TIMER_FADE: usize = 2;
const FADE_MS: u32 = 900;
/// 썸 두께 — 오버레이(얇게)/드래그(일반 바).
const BAR_THIN: i32 = 6;
const BAR_WIDE: i32 = 10;
const THUMB_MIN: i32 = 24;

/// 드래그 중인 바(트랙 표시 = 일반 스크롤바 모드).
#[derive(Clone, Copy, PartialEq, Eq)]
enum BarDrag {
    /// (드래그 시작 y, 시작 top행)
    V(i32, usize),
    /// (드래그 시작 x, 시작 h_off)
    H(i32, i32),
}

/// 행 데이터 — `check`: `None` = 마크 없음·`Some(on)` = 마크(확장 열 —
/// [`Mark`] 종류에 따라 체크박스 또는 ⊖ 삭제 버튼).
#[derive(Clone, Default)]
pub struct GridRow {
    pub check: Option<bool>,
    pub cells: Vec<String>,
}

/// 마크 셀 종류(확장 — 셀 데이터화 규약).
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Mark {
    /// 마크 없음(순수 텍스트 그리드).
    #[default]
    None,
    /// 컬럼 0 = 체크박스 이미지(클릭 토글 — Apply 열).
    Check,
    /// 행 우측 끝 = 빨간 ⊖ 삭제 버튼(클릭 = NXGR_TOGGLE 통지, Edit 시안 07-18).
    Minus,
}

/// 그리드 옵션(확장 스위치 — 기본값 = 헤더 있는 순수 그리드).
#[derive(Clone, Copy, Default)]
pub struct GridOpts {
    /// 헤더 숨김(목록 모드 — Edit 시안).
    pub no_header: bool,
    /// 지브라 줄무늬(짝수행 흰색·홀수행 sel_bg 절반 톤 — 빈 슬롯까지 이어 그림).
    pub zebra: bool,
    /// 1px 외곽선(목록 모드 — Edit 시안).
    pub outline: bool,
    /// 행 높이(≤0 = 자동: 글꼴+상/하 4px — 호스트가 파일 목록 등과 정렬 시 지정).
    pub row_h: i32,
    pub mark: Mark,
}

struct GridState {
    cols: Vec<GridCol>,
    rows: Vec<GridRow>,
    /// 스크롤 상단 행.
    top: usize,
    /// 가로 스크롤 픽셀 오프셋(컬럼 합 > 폭일 때 — 07-18).
    h_off: i32,
    opts: GridOpts,
    /// 헤더 경계 드래그(컬럼 인덱스·시작 x·시작 폭).
    drag: Option<(usize, i32, i32)>,
    /// 스크롤바 썸 드래그(일반 바 모드).
    bar_drag: Option<BarDrag>,
    /// 오버레이 바 표시 중(스크롤 직후 — 페이드 타이머로 소등).
    bars_visible: bool,
    last_toggle: isize,
    /// 행 선택(rows와 평행 — 07-18 파일 목록 규약).
    sel: Vec<bool>,
    /// 키보드 포커스 행(Ctrl+방향키 = 선택 없이 이동).
    focus: Option<usize>,
    /// Shift 연속 선택 앵커.
    anchor: Option<usize>,
    /// 컨트롤이 키보드 포커스 보유(포커스 행 테두리 표시 조건).
    has_focus: bool,
    /// 정렬 상태(우선순위 순 (컬럼 idx, desc) — 07-18 원본 docs/23 §4 이식).
    sort: Vec<(usize, bool)>,
    font: HFONT,
    style: Style,
}

static REGISTER: std::sync::Once = std::sync::Once::new();
const CLASS: PCWSTR = w!("Nexa.NxGrid");

/// NxGrid 생성 — `cols` = (제목, 초기 폭). `opts` = 확장 스위치(헤더·지브라·마크).
#[allow(clippy::too_many_arguments)] // Win32 create 계열 관례
pub unsafe fn create(
    parent: HWND,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    id: u32,
    font: HFONT,
    cols: &[(&str, i32)],
    opts: GridOpts,
    style: Style,
) -> HWND {
    super::base::register_class(&REGISTER, CLASS, Some(proc));
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        CLASS,
        w!(""),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(WS_TABSTOP.0),
        x,
        y,
        w,
        h,
        Some(parent),
        Some(HMENU(id as usize as *mut core::ffi::c_void)),
        None,
        None,
    )
    .unwrap_or_default();
    let st = Box::new(GridState {
        cols: cols
            .iter()
            .map(|(t, w)| GridCol {
                title: t.to_string(),
                width: (*w).max(COL_MIN),
            })
            .collect(),
        rows: Vec::new(),
        top: 0,
        h_off: 0,
        opts,
        drag: None,
        bar_drag: None,
        bars_visible: false,
        last_toggle: -1,
        sel: Vec::new(),
        focus: None,
        anchor: None,
        has_focus: false,
        sort: Vec::new(),
        font,
        style,
    });
    super::base::attach_state(hwnd, st);
    SendMessageW(
        hwnd,
        WM_SETFONT,
        Some(WPARAM(font.0 as usize)),
        Some(LPARAM(1)),
    );
    hwnd
}

/// 행 전체 교체(크레이트 내 직접 API — 대량 데이터 메시지 마샬링 회피).
/// 선택/포커스는 새 행 수로 클램프해 유지(실시간 미리보기 갱신 대응).
pub unsafe fn set_rows(hwnd: HWND, rows: Vec<GridRow>) {
    if let Some(st) = state(hwnd).as_mut() {
        st.rows = rows;
        st.sel.resize(st.rows.len(), false);
        let last = st.rows.len().checked_sub(1);
        st.focus = last.and_then(|l| st.focus.map(|f| f.min(l)));
        st.anchor = last.and_then(|l| st.anchor.map(|a| a.min(l)));
        let vis = visible_rows(hwnd, st);
        st.top = st.top.min(st.rows.len().saturating_sub(vis));
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

/// 행 체크 상태 조회.
pub unsafe fn row_check(hwnd: HWND, idx: usize) -> Option<bool> {
    state(hwnd)
        .as_ref()
        .and_then(|st| st.rows.get(idx))
        .and_then(|r| r.check)
}

/// 정렬 상태 조회(우선순위 순 (컬럼 idx, desc) — [`NXGR_SORT`] 수신 시 호스트가 읽는다).
pub unsafe fn sort_spec(hwnd: HWND) -> Vec<(usize, bool)> {
    state(hwnd)
        .as_ref()
        .map_or(Vec::new(), |st| st.sort.clone())
}

/// 선택된 행 인덱스(오름차순 — 크레이트 내 직접 API).
#[allow(dead_code)] // 판매용 계약 API — 호스트 소비는 선택(현재 앱은 통지만 사용)
pub unsafe fn selected_rows(hwnd: HWND) -> Vec<usize> {
    state(hwnd).as_ref().map_or(Vec::new(), |st| {
        st.sel
            .iter()
            .enumerate()
            .filter_map(|(i, on)| on.then_some(i))
            .collect()
    })
}

unsafe fn state(hwnd: HWND) -> *mut GridState {
    super::base::state(hwnd)
}

unsafe fn row_h(hwnd: HWND, st: &GridState) -> i32 {
    if st.opts.row_h > 0 {
        st.opts.row_h
    } else {
        font_height(hwnd, st.font) + super::style::PAD_Y * 2
    }
}

unsafe fn header_h(hwnd: HWND, st: &GridState) -> i32 {
    if st.opts.no_header {
        0
    } else {
        row_h(hwnd, st) + 2
    }
}

/// 부모에 WM_COMMAND(MAKEWPARAM(id, code)) 재발행(ctl 통지 규약 — base 위임).
unsafe fn notify(hwnd: HWND, code: u32) {
    super::base::notify(hwnd, code);
}

/// 단일 선택(전체 해제 후 i만) + 포커스·앵커 동기.
fn sel_single(st: &mut GridState, i: usize) {
    st.sel.iter_mut().for_each(|s| *s = false);
    if let Some(s) = st.sel.get_mut(i) {
        *s = true;
    }
    st.focus = Some(i);
    st.anchor = Some(i);
}

/// 앵커~i 연속 선택(기존 선택 대체 — Shift 규약).
fn sel_range(st: &mut GridState, i: usize) {
    let a = st.anchor.unwrap_or(i);
    let (lo, hi) = (a.min(i), a.max(i));
    for (k, s) in st.sel.iter_mut().enumerate() {
        *s = k >= lo && k <= hi;
    }
    st.focus = Some(i);
}

/// 포커스 행이 보이도록 스크롤(키보드 내비).
unsafe fn ensure_visible(hwnd: HWND, st: &mut GridState, i: usize) {
    let vis = visible_rows(hwnd, st).max(1);
    if i < st.top {
        scroll_to(hwnd, st, i as isize);
    } else if i >= st.top + vis {
        scroll_to(hwnd, st, i as isize - vis as isize + 1);
    }
}

/// 헤더 체크 상태(Mark::Check — 07-18 시안): `None` = 마크 행 없음 ·
/// `Some(0/1/2)` = 전체 해제/전체 체크/부분(흐릿한 ✓).
fn header_check(st: &GridState) -> Option<u32> {
    let (mut on, mut total) = (0usize, 0usize);
    for r in &st.rows {
        if let Some(c) = r.check {
            total += 1;
            if c {
                on += 1;
            }
        }
    }
    if total == 0 {
        None
    } else if on == 0 {
        Some(0)
    } else if on == total {
        Some(1)
    } else {
        Some(2)
    }
}

/// 헤더 클릭 정렬(07-18 — 원본 docs/23 §4 확정 규약 이식): 단순 클릭 =
/// 단일 정렬 3상태 순환(없음→▲오름→▼내림→없음) · Shift+클릭 & 기존 정렬 ≥1 =
/// 키 추가/방향 순환/제거(순번 당김). 비교는 호스트([`NXGR_SORT`] 통지).
unsafe fn apply_sort(hwnd: HWND, st: &mut GridState, ci: usize, shift: bool) {
    let cur = st.sort.iter().find(|(k, _)| *k == ci).map(|(_, d)| *d);
    if shift && !st.sort.is_empty() {
        match cur {
            None => st.sort.push((ci, false)), // 추가 = 오름
            Some(false) => {
                if let Some(e) = st.sort.iter_mut().find(|(k, _)| *k == ci) {
                    e.1 = true;
                }
            }
            Some(true) => st.sort.retain(|(k, _)| *k != ci), // 없음 = 제거
        }
    } else {
        st.sort = match cur {
            None => vec![(ci, false)],
            Some(false) => vec![(ci, true)],
            Some(true) => Vec::new(), // 없음 = 원래 순서
        };
    }
    let _ = InvalidateRect(Some(hwnd), None, false);
    notify(hwnd, NXGR_SORT);
}

/// 헤더 셀 제목: ▲/▼는 이름 앞·다중 정렬 순번(①②…)은 이름 뒤(원본 docs/23 §4).
fn header_label(st: &GridState, i: usize, c: &GridCol) -> String {
    let mut s = String::new();
    if let Some(desc) = st.sort.iter().find(|(k, _)| *k == i).map(|(_, d)| *d) {
        s.push_str(if desc { "▼ " } else { "▲ " });
    }
    s.push_str(&c.title);
    // 순번 = 정렬 시작부터 상시 표시(사용자 확정 07-18 — 단일 = ①)
    if let Some(order) = st.sort.iter().position(|(k, _)| *k == i) {
        s.push(' ');
        s.push_str(nexa_gui::order_badge(order));
    }
    s
}

/// ⊖ 삭제 마크 rect(Mark::Minus — 행 우측 끝 고정, 가로 스크롤 무관).
unsafe fn minus_rect(hwnd: HWND, st: &GridState, row_on_screen: i32) -> RECT {
    let rc = client(hwnd);
    let rh = row_h(hwnd, st);
    let side = font_height(hwnd, st.font).max(10);
    let top = rc.top + header_h(hwnd, st) + row_on_screen * rh + (rh - side) / 2;
    RECT {
        left: rc.right - side - 10,
        top,
        right: rc.right - 10,
        bottom: top + side,
    }
}

unsafe fn client(hwnd: HWND) -> RECT {
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    rc
}

unsafe fn visible_rows(hwnd: HWND, st: &GridState) -> usize {
    let rc = client(hwnd);
    let rh = row_h(hwnd, st);
    (((rc.bottom - rc.top) - header_h(hwnd, st)) / rh.max(1)).max(0) as usize
}

/// 컬럼 폭 합(가로 스크롤 범위).
fn total_w(st: &GridState) -> i32 {
    st.cols.iter().map(|c| c.width).sum()
}

/// 가로 오프셋 최대치.
unsafe fn max_h_off(hwnd: HWND, st: &GridState) -> i32 {
    let rc = client(hwnd);
    (total_w(st) - (rc.right - rc.left)).max(0)
}

/// 세로 썸 rect(드래그 = 일반 바 폭) — 세로 스크롤 불요 시 None.
unsafe fn v_thumb(hwnd: HWND, st: &GridState) -> Option<RECT> {
    let vis = visible_rows(hwnd, st);
    if st.rows.len() <= vis {
        return None;
    }
    let rc = client(hwnd);
    let hh = header_h(hwnd, st);
    let track_h = (rc.bottom - hh).max(1);
    let th = (track_h * vis as i32 / st.rows.len() as i32).max(THUMB_MIN);
    let max_top = (st.rows.len() - vis) as i32;
    let ty = hh + ((track_h - th) * st.top as i32) / max_top.max(1);
    let wbar = if matches!(st.bar_drag, Some(BarDrag::V(..))) {
        BAR_WIDE
    } else {
        BAR_THIN
    };
    Some(RECT {
        left: rc.right - wbar - 2,
        top: ty,
        right: rc.right - 2,
        bottom: ty + th,
    })
}

/// 가로 썸 rect — 가로 스크롤 불요 시 None.
unsafe fn h_thumb(hwnd: HWND, st: &GridState) -> Option<RECT> {
    let rc = client(hwnd);
    let vw = rc.right - rc.left;
    let tw = total_w(st);
    if tw <= vw {
        return None;
    }
    let track_w = vw.max(1);
    let th = (track_w * vw / tw).max(THUMB_MIN);
    let tx = ((track_w - th) * st.h_off) / max_h_off(hwnd, st).max(1);
    let hbar = if matches!(st.bar_drag, Some(BarDrag::H(..))) {
        BAR_WIDE
    } else {
        BAR_THIN
    };
    Some(RECT {
        left: tx,
        top: rc.bottom - hbar - 2,
        right: tx + th,
        bottom: rc.bottom - 2,
    })
}

/// 스크롤 직후 오버레이 표시 + 페이드 타이머 재무장(macOS 규약 — 07-18).
unsafe fn flash_bars(hwnd: HWND, st: &mut GridState) {
    st.bars_visible = true;
    let _ = SetTimer(Some(hwnd), TIMER_FADE, FADE_MS, None);
    let _ = InvalidateRect(Some(hwnd), None, false);
}

unsafe fn scroll_to(hwnd: HWND, st: &mut GridState, top: isize) {
    let vis = visible_rows(hwnd, st);
    let max_top = st.rows.len().saturating_sub(vis);
    st.top = top.clamp(0, max_top as isize) as usize;
    flash_bars(hwnd, st);
}

unsafe fn hscroll_to(hwnd: HWND, st: &mut GridState, off: i32) {
    st.h_off = off.clamp(0, max_h_off(hwnd, st));
    flash_bars(hwnd, st);
}

/// 헤더 경계 히트(±4px — 가로 오프셋 반영) — 리사이즈 대상 컬럼 인덱스.
unsafe fn border_hit(st: &GridState, x: i32) -> Option<usize> {
    let x = x + st.h_off;
    let mut edge = 0;
    for (i, c) in st.cols.iter().enumerate() {
        edge += c.width;
        if (x - edge).abs() <= 4 {
            return Some(i);
        }
    }
    None
}

unsafe fn pt_in(rc: &RECT, x: i32, y: i32) -> bool {
    x >= rc.left && x < rc.right && y >= rc.top && y < rc.bottom
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => LRESULT(0),
        WM_DESTROY => {
            super::base::drop_state::<GridState>(hwnd);
            LRESULT(0)
        }
        WM_SETFONT => {
            if let Some(st) = state(hwnd).as_mut() {
                st.font = HFONT(wparam.0 as *mut core::ffi::c_void);
            }
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        m if m == NXGR_GETROW => LRESULT(state(hwnd).as_ref().map_or(-1, |s| s.last_toggle)),
        WM_SIZE => {
            if let Some(st) = state(hwnd).as_mut() {
                let vis = visible_rows(hwnd, st);
                st.top = st.top.min(st.rows.len().saturating_sub(vis));
                st.h_off = st.h_off.clamp(0, max_h_off(hwnd, st));
            }
            let _ = InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == TIMER_FADE => {
            if let Some(st) = state(hwnd).as_mut() {
                let _ = KillTimer(Some(hwnd), TIMER_FADE);
                if st.bar_drag.is_none() {
                    st.bars_visible = false; // 페이드아웃(오버레이 소등)
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            if let Some(st) = state(hwnd).as_mut() {
                let delta = ((wparam.0 >> 16) & 0xFFFF) as i16 as isize;
                let shift = (wparam.0 & 0x0004/* MK_SHIFT */) != 0;
                if shift {
                    let off = st.h_off - delta.signum() as i32 * 48;
                    hscroll_to(hwnd, st, off); // Shift+휠 = 가로
                } else {
                    let top = st.top as isize - delta.signum() * 3;
                    scroll_to(hwnd, st, top);
                }
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            // 헤더 경계 위 = 좌우 리사이즈 커서
            if let Some(st) = state(hwnd).as_ref() {
                let mut pt = windows::Win32::Foundation::POINT::default();
                let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt);
                let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut pt);
                let in_header = pt.y >= 0 && pt.y < header_h(hwnd, st);
                if (in_header && border_hit(st, pt.x).is_some()) || st.drag.is_some() {
                    let cur =
                        windows::Win32::UI::WindowsAndMessaging::LoadCursorW(None, IDC_SIZEWE)
                            .unwrap_or_default();
                    windows::Win32::UI::WindowsAndMessaging::SetCursor(Some(cur));
                    return LRESULT(1);
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_LBUTTONDOWN => {
            if let Some(st) = state(hwnd).as_mut() {
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                // ① 오버레이 썸(표시 중일 때) = 드래그 → 일반 바 모드
                if st.bars_visible {
                    if let Some(t) = v_thumb(hwnd, st) {
                        if pt_in(&t, x, y) {
                            st.bar_drag = Some(BarDrag::V(y, st.top));
                            let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetCapture(hwnd);
                            flash_bars(hwnd, st);
                            return LRESULT(0);
                        }
                    }
                    if let Some(t) = h_thumb(hwnd, st) {
                        if pt_in(&t, x, y) {
                            st.bar_drag = Some(BarDrag::H(x, st.h_off));
                            let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetCapture(hwnd);
                            flash_bars(hwnd, st);
                            return LRESULT(0);
                        }
                    }
                }
                let hh = header_h(hwnd, st);
                if y < hh {
                    // ② 헤더: 경계 드래그 = 컬럼 리사이즈 · 체크 열 = 전체 토글(07-18)
                    if let Some(ci) = border_hit(st, x) {
                        st.drag = Some((ci, x, st.cols[ci].width));
                        let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetCapture(hwnd);
                    } else if st.opts.mark == Mark::Check
                        && x + st.h_off < st.cols.first().map_or(0, |c| c.width)
                    {
                        if let Some(state0) = header_check(st) {
                            // 전체 체크 상태면 전체 해제, 그 외(해제/부분) = 전체 체크
                            let target = state0 != 1;
                            for r in st.rows.iter_mut() {
                                if r.check.is_some() {
                                    r.check = Some(target);
                                }
                            }
                            st.last_toggle = NXGR_ROW_ALL;
                            let _ = InvalidateRect(Some(hwnd), None, false);
                            notify(hwnd, NXGR_TOGGLE);
                        }
                    } else {
                        // 헤더 클릭 = 정렬(07-18 — 컬럼 판정 후 3상태/Shift 다중)
                        let xx = x + st.h_off;
                        let mut edge = 0;
                        let mut hit = None;
                        for (i, c) in st.cols.iter().enumerate() {
                            if xx < edge + c.width {
                                hit = Some(i);
                                break;
                            }
                            edge += c.width;
                        }
                        if let Some(ci) = hit {
                            // 다중열 트리거 = Shift 전용(사용자 재확정 07-18)
                            let shift = (wparam.0 & 0x0004/* MK_SHIFT */) != 0;
                            apply_sort(hwnd, st, ci, shift);
                        }
                    }
                } else {
                    // ③ 본문: 마크 히트 → 토글/삭제 통지, 그 외 = 행 선택(07-18)
                    let _ = SetFocus(Some(hwnd));
                    let rh = row_h(hwnd, st);
                    let on_screen = (y - hh) / rh.max(1);
                    let idx = st.top + on_screen as usize;
                    if idx >= st.rows.len() {
                        // 빈 영역 클릭 = 선택 해제(탐색기 규약)
                        if st.sel.iter().any(|s| *s) {
                            st.sel.iter_mut().for_each(|s| *s = false);
                            let _ = InvalidateRect(Some(hwnd), None, false);
                            notify(hwnd, NXGR_SELCHANGE);
                        }
                        return LRESULT(0);
                    }
                    let mark_hit = match st.opts.mark {
                        Mark::Check => {
                            let cw = st.cols.first().map_or(0, |c| c.width);
                            x + st.h_off < cw && st.rows[idx].check.is_some()
                        }
                        Mark::Minus => {
                            let mr = minus_rect(hwnd, st, on_screen);
                            pt_in(&mr, x, y) && st.rows[idx].check.is_some()
                        }
                        Mark::None => false,
                    };
                    if mark_hit {
                        // 마크 클릭은 선택을 바꾸지 않는다(적용 토글/삭제 전용)
                        if let Some(on) = st.rows[idx].check {
                            st.rows[idx].check = Some(!on);
                            st.last_toggle = idx as isize;
                            let _ = InvalidateRect(Some(hwnd), None, false);
                            notify(hwnd, NXGR_TOGGLE);
                        }
                    } else {
                        let shift = (wparam.0 & 0x0004/* MK_SHIFT */) != 0;
                        let ctrl = (wparam.0 & 0x0008/* MK_CONTROL */) != 0;
                        if shift {
                            sel_range(st, idx); // 앵커 기준 연속
                        } else if ctrl {
                            // 비연속 토글 + 앵커 이동
                            if let Some(s) = st.sel.get_mut(idx) {
                                *s = !*s;
                            }
                            st.focus = Some(idx);
                            st.anchor = Some(idx);
                        } else {
                            sel_single(st, idx);
                        }
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        notify(hwnd, NXGR_SELCHANGE);
                    }
                }
            }
            LRESULT(0)
        }
        WM_GETDLGCODE => LRESULT((DLGC_WANTARROWS | DLGC_WANTCHARS) as isize),
        WM_SETFOCUS | WM_KILLFOCUS => {
            if let Some(st) = state(hwnd).as_mut() {
                st.has_focus = msg == WM_SETFOCUS;
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if let Some(st) = state(hwnd).as_mut() {
                if st.rows.is_empty() {
                    return LRESULT(0);
                }
                let last = st.rows.len() - 1;
                let vis = visible_rows(hwnd, st).max(1);
                let shift = GetKeyState(VK_SHIFT.0 as i32) < 0;
                let ctrl = GetKeyState(VK_CONTROL.0 as i32) < 0;
                let cur = st.focus.unwrap_or(st.top.min(last));
                // 방향키 계열 = 이동 목적지 계산(VK: ↑0x26 ↓0x28 PgUp0x21 PgDn0x22 Home0x24 End0x23)
                let dest = match wparam.0 as u32 {
                    0x26 => Some(cur.saturating_sub(1)),
                    0x28 => Some((cur + 1).min(last)),
                    0x21 => Some(cur.saturating_sub(vis)),
                    0x22 => Some((cur + vis).min(last)),
                    0x24 => Some(0),
                    0x23 => Some(last),
                    _ => None,
                };
                if let Some(i) = dest {
                    if ctrl {
                        st.focus = Some(i); // Ctrl+이동 = 포커스만(선택 유지)
                    } else if shift {
                        sel_range(st, i);
                        notify(hwnd, NXGR_SELCHANGE);
                    } else {
                        sel_single(st, i);
                        notify(hwnd, NXGR_SELCHANGE);
                    }
                    ensure_visible(hwnd, st, i);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                } else if wparam.0 == 0x20 {
                    // Space: Ctrl = 포커스 행 토글(비연속 누적), 그 외 = 단일 선택
                    if ctrl {
                        if let Some(s) = st.sel.get_mut(cur) {
                            *s = !*s;
                        }
                        st.anchor = Some(cur);
                    } else {
                        sel_single(st, cur);
                    }
                    st.focus = Some(cur);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    notify(hwnd, NXGR_SELCHANGE);
                } else if wparam.0 == 0x41 && ctrl {
                    // Ctrl+A = 전체 선택
                    st.sel.iter_mut().for_each(|s| *s = true);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                    notify(hwnd, NXGR_SELCHANGE);
                }
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if let Some(st) = state(hwnd).as_mut() {
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                if let Some((ci, sx, sw)) = st.drag {
                    st.cols[ci].width = (sw + (x - sx)).max(COL_MIN);
                    st.h_off = st.h_off.clamp(0, max_h_off(hwnd, st));
                    let _ = InvalidateRect(Some(hwnd), None, false);
                } else {
                    match st.bar_drag {
                        Some(BarDrag::V(sy, stop)) => {
                            // 픽셀 → 행 비례 이동(일반 바 모드)
                            let vis = visible_rows(hwnd, st);
                            let rc = client(hwnd);
                            let hh = header_h(hwnd, st);
                            let track_h = (rc.bottom - hh).max(1);
                            let th =
                                (track_h * vis as i32 / st.rows.len().max(1) as i32).max(THUMB_MIN);
                            let denom = (track_h - th).max(1);
                            let max_top = st.rows.len().saturating_sub(vis) as i32;
                            let dtop = ((y - sy) * max_top) / denom;
                            scroll_to(hwnd, st, stop as isize + dtop as isize);
                        }
                        Some(BarDrag::H(sx, soff)) => {
                            let rc = client(hwnd);
                            let vw = (rc.right - rc.left).max(1);
                            let tw = total_w(st);
                            let th = (vw * vw / tw.max(1)).max(THUMB_MIN);
                            let denom = (vw - th).max(1);
                            let doff = ((x - sx) * max_h_off(hwnd, st)) / denom;
                            hscroll_to(hwnd, st, soff + doff);
                        }
                        None => {}
                    }
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            if let Some(st) = state(hwnd).as_mut() {
                let had = st.drag.take().is_some() | st.bar_drag.take().is_some();
                if had {
                    let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
                    flash_bars(hwnd, st); // 드래그 종료 → 오버레이로 복귀 후 페이드
                }
            }
            LRESULT(0)
        }
        // 배경 지우기 생략(WM_PAINT 더블버퍼가 전면 도장 — blink 방지 07-18)
        0x0014 /* WM_ERASEBKGND */ => LRESULT(1),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc0 = BeginPaint(hwnd, &mut ps);
            // 더블버퍼(blink QA 07-18): 메모리 DC에 전부 그린 뒤 1회 BitBlt
            let rcw = client(hwnd);
            let (bw, bh) = (
                (rcw.right - rcw.left).max(1),
                (rcw.bottom - rcw.top).max(1),
            );
            let mem = CreateCompatibleDC(Some(dc0));
            let bmp = CreateCompatibleBitmap(dc0, bw, bh);
            let old_bmp = SelectObject(mem, bmp.into());
            let dc = mem;
            if let Some(st) = state(hwnd).as_ref() {
                let rc = client(hwnd);
                fill(dc, &rc, st.style.bg);
                let hh = header_h(hwnd, st);
                let rh = row_h(hwnd, st);
                let fh = font_height(hwnd, st.font).max(10);
                let ox = st.h_off; // 가로 오프셋(헤더·셀 공통)
                let band = RECT {
                    bottom: rc.top + hh,
                    ..rc
                };
                if hh > 0 {
                    // 헤더 밴드 + 하단 구분선
                    fill(dc, &band, st.style.sel_bg);
                    let sep = RECT {
                        top: band.bottom - 1,
                        ..band
                    };
                    fill(dc, &sep, st.style.border);
                    // 컬럼 경계선(헤더 전용)
                    let mut edge = rc.left - ox;
                    for c in &st.cols {
                        edge += c.width;
                        if edge >= rc.left && edge <= rc.right {
                            let line = RECT {
                                left: edge - 1,
                                top: band.top + 3,
                                right: edge,
                                bottom: band.bottom - 3,
                            };
                            fill(dc, &line, st.style.border);
                        }
                    }
                }
                // 행 배경(지브라 → 선택 → 포커스 테두리 순 — 07-18 선택 규약)
                let vis0 = visible_rows(hwnd, st);
                if st.opts.zebra {
                    // sel_bg 절반 톤(bg와 50% 블렌드) — 빈 슬롯까지 줄무늬 유지
                    let (b, s) = (st.style.bg.0, st.style.sel_bg.0);
                    let half = |sh: u32| (((b >> sh & 0xFF) + (s >> sh & 0xFF)) / 2) << sh;
                    let zc = windows::Win32::Foundation::COLORREF(half(0) | half(8) | half(16));
                    let slots = ((rc.bottom - rc.top - hh) / rh.max(1)) + 1;
                    for k in 0..slots {
                        if (st.top as i32 + k) % 2 == 1 {
                            let top = rc.top + hh + k * rh;
                            let zr = RECT {
                                top,
                                bottom: (top + rh).min(rc.bottom),
                                ..rc
                            };
                            fill(dc, &zr, zc);
                        }
                    }
                }
                for (row, _) in st.rows.iter().skip(st.top).take(vis0).enumerate() {
                    let idx = st.top + row;
                    let top = rc.top + hh + row as i32 * rh;
                    let rr = RECT {
                        top,
                        bottom: top + rh,
                        ..rc
                    };
                    if st.sel.get(idx).copied().unwrap_or(false) {
                        fill(dc, &rr, st.style.sel_bg);
                    }
                    if st.has_focus && st.focus == Some(idx) {
                        super::style::frame(dc, &rr, st.style.accent);
                    }
                }
                // 체크 마크 + 스크롤 썸(AA — 도형 패스)
                {
                    let mut g = GdipCtx::new(dc);
                    let vis = visible_rows(hwnd, st);
                    match st.opts.mark {
                        Mark::Check => {
                            let cw = st.cols.first().map_or(0, |c| c.width);
                            let side = fh;
                            // 헤더 체크(07-18 시안): 전체 토글 — 0 해제(백지+외곽선)·
                            // 1 전체(accent+✓)·2 부분(accent+**흐릿한 ✓**)
                            if hh > 0 {
                                if let Some(hc) = header_check(st) {
                                    let bx = rc.left - ox + (cw - side) / 2;
                                    let top = rc.top + (hh - 1 - side) / 2;
                                    if bx + side >= rc.left {
                                        let rect = Rect::new(bx, top, side, side);
                                        let radius = (side / 3).max(4);
                                        if hc == 0 {
                                            // 헤더 밴드(sel_bg) 위 구별 = bg 필 + 외곽선
                                            g.fill_round_rect(rect, radius, color(st.style.bg));
                                            g.stroke_round_rect(
                                                rect,
                                                radius,
                                                color(st.style.border),
                                                1.0,
                                            );
                                        } else {
                                            g.fill_round_rect(rect, radius, color(st.style.accent));
                                            let vc = if hc == 2 {
                                                let (b, a) =
                                                    (st.style.bg.0, st.style.accent.0);
                                                let half = |sh: u32| {
                                                    (((b >> sh & 0xFF) + (a >> sh & 0xFF)) / 2)
                                                        << sh
                                                };
                                                windows::Win32::Foundation::COLORREF(
                                                    half(0) | half(8) | half(16),
                                                )
                                            } else {
                                                st.style.bg
                                            };
                                            let (cx, cy) = (bx + side / 2, top + side / 2);
                                            g.polyline(
                                                &[
                                                    (cx - side / 4, cy),
                                                    (cx - side / 12, cy + side / 4 - 1),
                                                    (cx + side / 4, cy - side / 5),
                                                ],
                                                color(vc),
                                                2.0,
                                            );
                                        }
                                    }
                                }
                            }
                            for (row, r) in st.rows.iter().skip(st.top).take(vis).enumerate() {
                                let Some(on) = r.check else { continue };
                                let top = rc.top + hh + row as i32 * rh + (rh - side) / 2;
                                let bx = rc.left - ox + (cw - side) / 2;
                                if bx + side < rc.left {
                                    continue; // 가로 스크롤로 화면 밖
                                }
                                let rect = Rect::new(bx, top, side, side);
                                let radius = (side / 3).max(4);
                                if on {
                                    g.fill_round_rect(rect, radius, color(st.style.accent));
                                    let (cx, cy) = (bx + side / 2, top + side / 2);
                                    g.polyline(
                                        &[
                                            (cx - side / 4, cy),
                                            (cx - side / 12, cy + side / 4 - 1),
                                            (cx + side / 4, cy - side / 5),
                                        ],
                                        color(st.style.bg),
                                        2.0,
                                    );
                                } else {
                                    g.fill_round_rect(rect, radius, color(st.style.sel_bg));
                                    g.stroke_round_rect(rect, radius, color(st.style.border), 1.0);
                                }
                            }
                        }
                        Mark::Minus => {
                            // 행 우측 끝 빨간 ⊖(Edit 시안 07-18) — check=Some 행만
                            for (row, r) in st.rows.iter().skip(st.top).take(vis).enumerate() {
                                if r.check.is_none() {
                                    continue;
                                }
                                let mr = minus_rect(hwnd, st, row as i32);
                                let side = mr.bottom - mr.top;
                                g.fill_ellipse(
                                    Rect::new(mr.left, mr.top, side, side),
                                    color(st.style.danger),
                                );
                                let (cx, cy) = (mr.left + side / 2, mr.top + side / 2);
                                g.polyline(
                                    &[(cx - side / 4, cy), (cx + side / 4, cy)],
                                    color(st.style.bg),
                                    2.0,
                                );
                            }
                        }
                        Mark::None => {}
                    }
                    // 오버레이 스크롤바(스크롤 직후·드래그 중에만 — macOS 시안)
                    if st.bars_visible || st.bar_drag.is_some() {
                        if let Some(t) = v_thumb(hwnd, st) {
                            if matches!(st.bar_drag, Some(BarDrag::V(..))) {
                                // 일반 바 모드 = 트랙 표시
                                let track = RECT {
                                    left: t.left,
                                    top: rc.top + hh,
                                    right: rc.right,
                                    bottom: rc.bottom,
                                };
                                fill(dc, &track, st.style.sel_bg);
                            }
                            g.fill_round_rect(
                                Rect::new(t.left, t.top, t.right - t.left, t.bottom - t.top),
                                (t.right - t.left) / 2,
                                color(st.style.border),
                            );
                        }
                        if let Some(t) = h_thumb(hwnd, st) {
                            if matches!(st.bar_drag, Some(BarDrag::H(..))) {
                                let track = RECT {
                                    left: rc.left,
                                    top: t.top,
                                    right: rc.right,
                                    bottom: rc.bottom,
                                };
                                fill(dc, &track, st.style.sel_bg);
                            }
                            g.fill_round_rect(
                                Rect::new(t.left, t.top, t.right - t.left, t.bottom - t.top),
                                (t.bottom - t.top) / 2,
                                color(st.style.border),
                            );
                        }
                    }
                } // GDI 텍스트 전에 Graphics 해제(HDC 혼용 규약)
                  // 텍스트 패스(헤더 라벨 + 셀 — 1px 하향·말줄임)
                let old = SelectObject(dc, st.font.into());
                SetBkMode(dc, TRANSPARENT);
                SetTextColor(dc, st.style.text);
                let mut x0 = rc.left - ox;
                for (ci, c) in st.cols.iter().enumerate() {
                    // 정렬 글리프(▲/▼ 앞·순번 뒤 — 07-18) 포함 헤더 라벨
                    let label = header_label(st, ci, c);
                    if hh > 0 && !label.is_empty() && x0 + c.width > rc.left && x0 < rc.right {
                        let mut w16: Vec<u16> = label.encode_utf16().collect();
                        let mut trc = RECT {
                            left: x0 + 6,
                            top: band.top + 1,
                            right: x0 + c.width - 4,
                            bottom: band.bottom + 1,
                        };
                        DrawTextW(dc, &mut w16, &mut trc, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
                    }
                    x0 += c.width;
                }
                let vis = visible_rows(hwnd, st);
                for (row, r) in st.rows.iter().skip(st.top).take(vis).enumerate() {
                    let top = rc.top + hh + row as i32 * rh;
                    // 셀은 컬럼 1부터(체크 열이면) — cells[k] ↔ cols[k + skip]
                    let skip = usize::from(st.opts.mark == Mark::Check);
                    let mut x0 =
                        rc.left - ox + st.cols.iter().take(skip).map(|c| c.width).sum::<i32>();
                    for (k, c) in st.cols.iter().skip(skip).enumerate() {
                        // 빈 문자열 = 빈 Vec(댕글링 포인터) → user32 AV(07-18 진범)
                        if let Some(text) = r.cells.get(k).filter(|t| !t.is_empty()) {
                            if x0 + c.width > rc.left && x0 < rc.right {
                                let mut w16: Vec<u16> = text.encode_utf16().collect();
                                let mut trc = RECT {
                                    left: x0 + 6,
                                    top: top + 1,
                                    right: x0 + c.width - 4,
                                    bottom: top + rh + 1,
                                };
                                DrawTextW(
                                    dc,
                                    &mut w16,
                                    &mut trc,
                                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                                );
                            }
                        }
                        x0 += c.width;
                    }
                }
                SelectObject(dc, old);
                if st.opts.outline {
                    super::style::frame(dc, &rc, st.style.border); // 목록 모드 외곽선
                }
            }
            let _ = BitBlt(dc0, 0, 0, bw, bh, Some(mem), 0, 0, SRCCOPY);
            SelectObject(mem, old_bmp);
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(mem);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
