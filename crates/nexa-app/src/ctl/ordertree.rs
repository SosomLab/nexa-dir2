//! ordertree — **NxOrderTree** 순서 편집 트리(ctl 15호, 07-19 사용자:
//! 도구모음/컬럼/컨텍스트 메뉴 순서 설정). 라이브러리 추상화 — 앱 비결합.
//!
//! ## 계약(판매용 명세 — 클래스 `Nexa.NxOrderTree`)
//! - 행 = `(라벨, 레벨 0/1, 체크 Option)` — [`set_rows`] 주입, 부모 = 순서
//!   유도(자식 = 직전 레벨 0 행 소속).
//! - 선택: 클릭 = 단일 · Shift = **같은 레벨·같은 부모 형제 범위만**
//!   (혼합 차단). 통지 [`NXOT_SELCHANGE`].
//! - 체크: 박스 클릭 = 토글 → [`NXOT_TOGGLE`](행 = [`take_toggled`]).
//!   **첫 컬럼 고정**(레벨 무관 x — 사용자 확정 07-19).
//! - **펼침/닫음(07-19)**: 그룹 행 셰브론(E76C 닫힘/E70D 펼침 — 파일뷰
//!   규약, **체크 뒤·하위 있는 그룹만**) 클릭 = 접기 토글, 기본 = 전부
//!   펼침. 접힘 상태는 라벨 기준 유지([`set_rows`] 재구성에도 보존)·접힌
//!   그룹 자식 선택은 해제.
//! - **드래그 이동(07-19)**: 선택 블록(그룹 = 자식 포함)을 끌어 **같은 부모
//!   안에서만** 이동 — 스냅 시 라이브 미리보기·커서 추종 **고스트 박스**
//!   (컬럼 이동 규약 차용)·가장자리 **자동 스크롤** · ESC = [`cancel_drag`].
//!   확정 = [`NXOT_DRAGMOVE`] 통지 후 [`take_drag_delta`](형제 칸 이동량) —
//!   내부 행은 원상 복원되므로 호스트가 모델 이동을 delta만큼 적용한다.
//! - **세로 스크롤**: 내용 초과 시 오버레이 썸(파일 목록 그리드 차용 —
//!   우측 얇은 썸·드래그 가능) + 휠.
//!
//! 이동/토글의 모델 반영은 호스트 몫([`crate::ordereditor`] 공통 창).

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, DrawTextW, EndPaint, InvalidateRect, SelectObject, SetBkMode, SetTextColor,
    DT_LEFT, DT_SINGLELINE, DT_VCENTER, HFONT, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetClientRect, KillTimer, SetTimer, HMENU, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_DESTROY, WM_ERASEBKGND, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    WM_MOUSEWHEEL, WM_PAINT, WM_TIMER, WS_CHILD, WS_VISIBLE,
};

use super::style::{fill, frame, Style};

/// 선택 변경 통지(WM_COMMAND HIWORD).
pub const NXOT_SELCHANGE: u32 = 1;
/// 체크 토글 통지(07-19 — 표시 여부 편집). 토글 행 = [`take_toggled`].
pub const NXOT_TOGGLE: u32 = 2;
/// 드래그 이동 확정 통지(07-19) — 이동량 = [`take_drag_delta`].
pub const NXOT_DRAGMOVE: u32 = 3;

/// 행 높이(px @96dpi).
const ROW_H: i32 = 22;
/// 그룹 셰브론 마커 존 폭(파일뷰 규약 — 상시 예약).
const MARK_W: i32 = 16;
/// 표시 체크 컬럼 폭(첫 컬럼 고정 — 레벨 무관, 사용자 확정 07-19).
const CHECK_COL: i32 = 18;
/// 레벨당 들여쓰기(px).
const INDENT: i32 = 18;
/// 자동 스크롤 가장자리 폭·타이머(드래그 중 — 07-19).
const EDGE: i32 = 14;
const TIMER_SCROLL: usize = 1;

type Row = (String, u8, Option<bool>);

/// 드래그 상태(07-19) — 미리보기는 rows를 직접 재배열, 확정/취소 시
/// orig로 복원 후 delta만 호스트에 전달(모델–뷰 정합).
struct OtDrag {
    press_y: i32,
    cur_y: i32,
    active: bool,
    /// 블록(선택 형제 + 그룹이면 자식 포함) 시작 row·행 수 — 미리보기 갱신.
    start: usize,
    count: usize,
    level: u8,
    /// 형제 칸 기준 현재 위치·시작 위치(delta = cur - orig).
    sibling_pos: i32,
    orig_sibling_pos: i32,
    orig_rows: Vec<Row>,
    orig_sel: Vec<usize>,
}

struct OtState {
    rows: Vec<Row>,
    /// 선택(index 오름차순 — 항상 같은 부모의 형제).
    sel: Vec<usize>,
    anchor: Option<usize>,
    toggled: Option<usize>,
    drag: Option<OtDrag>,
    drag_delta: i32,
    /// 세로 스크롤(px) — 내용 초과 시.
    scroll_y: i32,
    /// 스크롤 썸 드래그(시작 y·시작 scroll_y).
    sb_drag: Option<(i32, i32)>,
    /// 접힌 그룹(라벨 기준 — 이동/재구성에도 유지, 07-19).
    collapsed: std::collections::HashSet<String>,
    font: HFONT,
    /// 셰브론 마커 폰트(Segoe MDL2 Assets — 소유·Drop 해제).
    marker_font: HFONT,
    style: Style,
}

impl Drop for OtState {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Graphics::Gdi::DeleteObject(self.marker_font.into());
        }
    }
}

/// 셰브론 마커 폰트(Segoe MDL2 Assets 9px — 파일뷰/설정 트리 규약).
fn make_marker_font() -> HFONT {
    use windows::Win32::Graphics::Gdi::{
        CreateFontW, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_QUALITY, FF_DONTCARE, FW_NORMAL,
        OUT_DEFAULT_PRECIS,
    };
    unsafe {
        CreateFontW(
            -12,
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
            windows::core::w!("Segoe MDL2 Assets"),
        )
    }
}

static REGISTER: std::sync::Once = std::sync::Once::new();
const CLASS: PCWSTR = w!("Nexa.NxOrderTree");

/// 하위 체크 비활성 여부(07-19 사용자): 부모 그룹 체크가 해제면 자식
/// 체크는 **값 유지 + 비활성**(흐림·클릭/Space 무시).
fn check_disabled(rows: &[Row], i: usize) -> bool {
    let p = parent_of(rows, i);
    p >= 0 && rows[p as usize].2 == Some(false)
}

/// 부모 index(-1 = 최상위) — 순서 유도: 레벨 1 행은 직전 레벨 0 행 소속.
fn parent_of(rows: &[Row], i: usize) -> i32 {
    if rows[i].1 == 0 {
        return -1;
    }
    (0..i)
        .rev()
        .find(|&j| rows[j].1 == 0)
        .map(|j| j as i32)
        .unwrap_or(-1)
}

impl OtState {
    /// 표시 행 목록(절대 index) — 접힌 그룹의 자식 제외(07-19).
    fn visible(&self) -> Vec<usize> {
        let mut out = Vec::with_capacity(self.rows.len());
        let mut hide = false;
        for (i, (label, level, _)) in self.rows.iter().enumerate() {
            if *level == 0 {
                hide = self.collapsed.contains(label);
                out.push(i);
            } else if !hide {
                out.push(i);
            }
        }
        out
    }

    /// 절대 index → 표시 위치(접힘 시 None).
    fn vis_pos(&self, row: usize) -> Option<usize> {
        self.visible().iter().position(|&r| r == row)
    }

    fn content_h(&self) -> i32 {
        self.visible().len() as i32 * ROW_H + 2
    }

    fn clamp_scroll(&mut self, client_h: i32) {
        let max = (self.content_h() - client_h).max(0);
        self.scroll_y = self.scroll_y.clamp(0, max);
    }

    fn row_at(&self, y: i32) -> Option<usize> {
        let yy = y - 1 + self.scroll_y;
        if yy < 0 {
            return None;
        }
        let d = (yy / ROW_H) as usize;
        self.visible().get(d).copied()
    }

    /// 접기 토글(그룹 라벨 기준) — 접힌 자식 선택은 해제.
    fn toggle_collapse(&mut self, row: usize) {
        let label = self.rows[row].0.clone();
        if !self.collapsed.remove(&label) {
            self.collapsed.insert(label);
            // 접힌 그룹의 자식이 선택에 있으면 해제
            let vis = self.visible();
            self.sel.retain(|s| vis.contains(s));
            if self.anchor.is_some_and(|a| !vis.contains(&a)) {
                self.anchor = self.sel.first().copied();
            }
        }
    }

    /// 블록 범위: 선택 형제 + (레벨 0이면) 마지막 그룹의 자식들 포함.
    fn block_of_selection(&self) -> Option<(usize, usize)> {
        let (&s0, &s1) = (self.sel.first()?, self.sel.last()?);
        let mut end = s1 + 1;
        if self.rows[s0].1 == 0 {
            while end < self.rows.len() && self.rows[end].1 > 0 {
                end += 1; // 마지막 선택 그룹의 자식들
            }
        }
        Some((s0, end - s0))
    }

    /// 형제 칸 위치(같은 부모 안에서 몇 번째 형제인가 — 블록 시작 기준).
    fn sibling_pos(&self, row: usize) -> i32 {
        let (lv, pa) = (self.rows[row].1, parent_of(&self.rows, row));
        (0..row)
            .filter(|&j| self.rows[j].1 == lv && parent_of(&self.rows, j) == pa)
            .count() as i32
    }

    /// 드래그 한 칸 스왑(위/아래 이웃 형제 블록과) — 성공 시 true.
    fn swap_step(&mut self, up: bool) -> bool {
        let Some(d) = self.drag.as_ref() else {
            return false;
        };
        let (start, count, lv) = (d.start, d.count, d.level);
        let pa = parent_of(&self.rows, start);
        if up {
            // 이전 형제 블록: start 위쪽에서 같은 레벨·부모의 마지막 행
            let prev = (0..start)
                .rev()
                .find(|&j| self.rows[j].1 == lv && parent_of(&self.rows, j) == pa);
            let Some(pstart) = prev else { return false };
            let plen = start - pstart; // 그룹이면 자식 포함(연속 구간)
            self.rows[pstart..start + count].rotate_left(plen);
            for s in &mut self.sel {
                *s -= plen;
            }
            if let Some(d) = self.drag.as_mut() {
                d.start = pstart;
                d.sibling_pos -= 1;
            }
            true
        } else {
            // 다음 형제 블록: 블록 끝의 행이 같은 레벨·부모인가
            let nstart = start + count;
            if nstart >= self.rows.len()
                || self.rows[nstart].1 != lv
                || parent_of(&self.rows, nstart) != pa
            {
                return false;
            }
            let mut nend = nstart + 1;
            if self.rows[nstart].1 == 0 {
                while nend < self.rows.len() && self.rows[nend].1 > 0 {
                    nend += 1;
                }
            }
            let nlen = nend - nstart;
            self.rows[start..nend].rotate_left(count);
            for s in &mut self.sel {
                *s += nlen;
            }
            if let Some(d) = self.drag.as_mut() {
                d.start += nlen;
                d.sibling_pos += 1;
            }
            true
        }
    }

    /// 미리보기 갱신 — 커서 행이 블록 밖이면 그 방향으로 스왑 반복.
    fn preview_to(&mut self, y: i32) {
        loop {
            let Some(d) = self.drag.as_ref() else { return };
            let (start, count) = (d.start, d.count);
            let Some(r) = self.row_at(y) else { return };
            if r < start {
                if !self.swap_step(true) {
                    return;
                }
            } else if r >= start + count {
                if !self.swap_step(false) {
                    return;
                }
            } else {
                return;
            }
        }
    }
}

/// 생성 — 행은 [`set_rows`]로 주입.
#[allow(clippy::too_many_arguments)]
pub unsafe fn create(
    parent: HWND,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    id: u32,
    font: HFONT,
    style: Style,
) -> HWND {
    super::base::register_class(&REGISTER, CLASS, Some(proc));
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        CLASS,
        w!(""),
        WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0),
        x,
        y,
        w,
        h,
        Some(parent),
        Some(HMENU(id as isize as *mut core::ffi::c_void)),
        None,
        None,
    )
    .unwrap_or_default();
    if !hwnd.is_invalid() {
        super::base::attach_state(
            hwnd,
            Box::new(OtState {
                rows: Vec::new(),
                sel: Vec::new(),
                anchor: None,
                toggled: None,
                drag: None,
                drag_delta: 0,
                scroll_y: 0,
                sb_drag: None,
                collapsed: std::collections::HashSet::new(), // 기본 = 전부 펼침
                font,
                marker_font: make_marker_font(),
                style,
            }),
        );
    }
    hwnd
}

/// 행 재설정(선택/앵커/드래그 초기화 — 호스트가 이동 후 [`set_selection`]).
pub unsafe fn set_rows(hwnd: HWND, rows: Vec<Row>) {
    if let Some(st) = super::base::state::<OtState>(hwnd).as_mut() {
        st.rows = rows;
        st.sel.clear();
        st.anchor = None;
        st.drag = None;
        let mut rc = RECT::default();
        let _ = GetClientRect(hwnd, &mut rc);
        st.clamp_scroll(rc.bottom - rc.top);
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

/// 현재 선택(index 오름차순).
pub unsafe fn selection(hwnd: HWND) -> Vec<usize> {
    super::base::state::<OtState>(hwnd)
        .as_ref()
        .map(|st| st.sel.clone())
        .unwrap_or_default()
}

/// 행 가시 스크롤 보정(선택/키 이동 공용).
unsafe fn ensure_visible(hwnd: HWND, st: &mut OtState, row: usize) {
    let Some(d) = st.vis_pos(row) else { return };
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let ch = rc.bottom - rc.top;
    let top = 1 + d as i32 * ROW_H - st.scroll_y;
    if top < 0 {
        st.scroll_y += top;
    } else if top + ROW_H > ch {
        st.scroll_y += top + ROW_H - ch;
    }
    st.clamp_scroll(ch);
}

/// 선택 설정(범위 검증 없음 — 호스트가 형제 집합을 보장) + 가시 스크롤.
pub unsafe fn set_selection(hwnd: HWND, sel: &[usize]) {
    if let Some(st) = super::base::state::<OtState>(hwnd).as_mut() {
        st.sel = sel.to_vec();
        st.sel.sort_unstable();
        st.anchor = st.sel.first().copied();
        if let Some(&first) = st.sel.first() {
            ensure_visible(hwnd, st, first);
        }
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

/// 키보드 선택 이동(07-19 사용자): 이전/다음 행 단일 선택 + 가시 스크롤.
/// 선택 없음 = 첫/마지막 행 선택. 변경 시 [`NXOT_SELCHANGE`].
pub unsafe fn key_move(hwnd: HWND, up: bool) {
    let Some(st) = super::base::state::<OtState>(hwnd).as_mut() else {
        return;
    };
    if st.rows.is_empty() {
        return;
    }
    let vis = st.visible();
    if vis.is_empty() {
        return;
    }
    let cur = st.anchor.or_else(|| st.sel.first().copied());
    let next = match cur.and_then(|c| st.vis_pos(c)) {
        None => {
            if up {
                *vis.last().unwrap()
            } else {
                vis[0]
            }
        }
        Some(d) => {
            let nd = if up {
                d.saturating_sub(1)
            } else {
                (d + 1).min(vis.len() - 1)
            };
            vis[nd]
        }
    };
    if st.sel == vec![next] {
        return;
    }
    st.sel = vec![next];
    st.anchor = Some(next);
    ensure_visible(hwnd, st, next);
    let _ = InvalidateRect(Some(hwnd), None, false);
    super::base::notify(hwnd, NXOT_SELCHANGE);
}

/// Shift+방향(07-19): 같은 레벨·같은 부모 **형제로만** 범위 확장/축소
/// (클릭 Shift 규약과 동일 — 혼합 차단).
pub unsafe fn key_extend(hwnd: HWND, up: bool) {
    let Some(st) = super::base::state::<OtState>(hwnd).as_mut() else {
        return;
    };
    let Some(a) = st.anchor else {
        key_move(hwnd, up);
        return;
    };
    // 현재 범위 끝(앵커 반대편)에서 방향으로 다음 형제 탐색
    let cur_end = if up {
        st.sel.first().copied().unwrap_or(a)
    } else {
        st.sel.last().copied().unwrap_or(a)
    };
    let (lv, pa) = (st.rows[a].1, parent_of(&st.rows, a));
    let target = if up {
        (0..cur_end)
            .rev()
            .find(|&j| st.rows[j].1 == lv && parent_of(&st.rows, j) == pa)
    } else {
        (cur_end + 1..st.rows.len()).find(|&j| st.rows[j].1 == lv && parent_of(&st.rows, j) == pa)
    };
    // 축소 방향(앵커 쪽으로 되돌아옴)도 자연 처리: target 없으면 유지
    let t = match target {
        Some(t) => t,
        None => return,
    };
    let (lo, hi) = (a.min(t), a.max(t));
    st.sel = (lo..=hi)
        .filter(|&j| st.rows[j].1 == lv && parent_of(&st.rows, j) == pa)
        .collect();
    ensure_visible(hwnd, st, t);
    let _ = InvalidateRect(Some(hwnd), None, false);
    super::base::notify(hwnd, NXOT_SELCHANGE);
}

/// Space = 선택 행 체크 토글(단일 선택·체크 열 행 — 07-19).
/// 토글 시 [`NXOT_TOGGLE`](행 = [`take_toggled`]).
pub unsafe fn key_toggle(hwnd: HWND) {
    let Some(st) = super::base::state::<OtState>(hwnd).as_mut() else {
        return;
    };
    if st.sel.len() != 1 {
        return;
    }
    let i = st.sel[0];
    if check_disabled(&st.rows, i) {
        return; // 부모 그룹 해제 — 비활성(07-19)
    }
    if let Some(on) = st.rows[i].2 {
        st.rows[i].2 = Some(!on);
        st.toggled = Some(i);
        let _ = InvalidateRect(Some(hwnd), None, false);
        super::base::notify(hwnd, NXOT_TOGGLE);
    }
}

/// 필요 높이(행 수 기준 — 호스트 레이아웃 편의).
pub fn height_for(rows: usize) -> i32 {
    rows as i32 * ROW_H + 2
}

/// 마지막 체크 토글 행 수거(1회성 — NXOT_TOGGLE 통지 후 호출).
pub unsafe fn take_toggled(hwnd: HWND) -> Option<usize> {
    super::base::state::<OtState>(hwnd)
        .as_mut()
        .and_then(|st| st.toggled.take())
}

/// 현재 체크 상태 목록(체크 열 없는 행 = None).
pub unsafe fn checks(hwnd: HWND) -> Vec<Option<bool>> {
    super::base::state::<OtState>(hwnd)
        .as_ref()
        .map(|st| st.rows.iter().map(|(_, _, c)| *c).collect())
        .unwrap_or_default()
}

/// 드래그 확정 이동량 수거(1회성 — NXOT_DRAGMOVE 통지 후. 형제 칸 단위,
/// 음수 = 위로).
pub unsafe fn take_drag_delta(hwnd: HWND) -> i32 {
    super::base::state::<OtState>(hwnd)
        .as_mut()
        .map(|st| std::mem::take(&mut st.drag_delta))
        .unwrap_or(0)
}

/// ESC = 드래그 취소(07-19) — 시작 상태 복원. 활성 드래그가 있었으면 true.
pub unsafe fn cancel_drag(hwnd: HWND) -> bool {
    let Some(st) = super::base::state::<OtState>(hwnd).as_mut() else {
        return false;
    };
    let Some(d) = st.drag.take() else {
        return false;
    };
    let _ = KillTimer(Some(hwnd), TIMER_SCROLL);
    let _ = ReleaseCapture();
    if !d.active {
        return false;
    }
    st.rows = d.orig_rows;
    st.sel = d.orig_sel;
    let _ = InvalidateRect(Some(hwnd), None, false);
    true
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            if let Some(st) = super::base::state::<OtState>(hwnd).as_ref() {
                paint(hwnd, st);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_MOUSEWHEEL => {
            if let Some(st) = super::base::state::<OtState>(hwnd).as_mut() {
                let delta = (wp.0 >> 16) as i16 as i32;
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                st.scroll_y -= delta / 120 * ROW_H * 2;
                st.clamp_scroll(rc.bottom - rc.top);
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            on_lbutton_down(hwnd, lp);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            on_mouse_move(hwnd, lp);
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            on_lbutton_up(hwnd);
            LRESULT(0)
        }
        WM_TIMER if wp.0 == TIMER_SCROLL => {
            // 드래그 가장자리 자동 스크롤(07-19) — 커서 위치 기준 반복
            if let Some(st) = super::base::state::<OtState>(hwnd).as_mut() {
                if let Some(d) = st.drag.as_ref() {
                    if d.active {
                        let y = d.cur_y;
                        let mut rc = RECT::default();
                        let _ = GetClientRect(hwnd, &mut rc);
                        let ch = rc.bottom - rc.top;
                        if y < EDGE {
                            st.scroll_y -= ROW_H;
                        } else if y > ch - EDGE {
                            st.scroll_y += ROW_H;
                        }
                        st.clamp_scroll(ch);
                        st.preview_to(y);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            let _ = KillTimer(Some(hwnd), TIMER_SCROLL);
            super::base::drop_state::<OtState>(hwnd);
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe fn on_lbutton_down(hwnd: HWND, lp: LPARAM) {
    let Some(st) = super::base::state::<OtState>(hwnd).as_mut() else {
        return;
    };
    let x = (lp.0 as u32 & 0xFFFF) as i16 as i32;
    let y = (lp.0 as u32 >> 16) as i16 as i32;
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let (cw, ch) = (rc.right - rc.left, rc.bottom - rc.top);
    // 오버레이 스크롤바 썸 히트(우측 10px — 파일 목록 그리드 차용)
    if st.content_h() > ch && x >= cw - 10 {
        st.sb_drag = Some((y, st.scroll_y));
        let _ = SetCapture(hwnd);
        return;
    }
    let Some(i) = st.row_at(y) else { return };
    // 셰브론 존(체크 뒤) 클릭 = 접기 토글 — 하위가 있는 그룹만(사용자 확정 07-19)
    if st.rows[i].1 == 0
        && st.rows.get(i + 1).is_some_and(|r| r.1 == 1)
        && (2 + CHECK_COL..2 + CHECK_COL + MARK_W).contains(&x)
    {
        st.toggle_collapse(i);
        st.clamp_scroll(ch);
        let _ = InvalidateRect(Some(hwnd), None, false);
        super::base::notify(hwnd, NXOT_SELCHANGE);
        return;
    }
    // 체크 존(첫 컬럼 고정 — 레벨 무관) 클릭 = 표시 토글(부모 그룹 해제 시 비활성)
    if let Some(on) = st.rows[i].2 {
        let bx = 6;
        if x >= bx - 2 && x < bx + 16 {
            if check_disabled(&st.rows, i) {
                return; // 비활성 — 클릭 무시
            }
            st.rows[i].2 = Some(!on);
            st.toggled = Some(i);
            let _ = InvalidateRect(Some(hwnd), None, false);
            super::base::notify(hwnd, NXOT_TOGGLE);
            return;
        }
    }
    let shift = GetKeyState(VK_SHIFT.0 as i32) < 0;
    let mut changed = false;
    if shift {
        if let Some(a) = st.anchor {
            // 범위 = 같은 레벨·같은 부모 형제만(레벨 혼합 차단)
            if st.rows[a].1 == st.rows[i].1 && parent_of(&st.rows, a) == parent_of(&st.rows, i) {
                let (lo, hi) = (a.min(i), a.max(i));
                let (lv, pa) = (st.rows[a].1, parent_of(&st.rows, a));
                st.sel = (lo..=hi)
                    .filter(|&j| st.rows[j].1 == lv && parent_of(&st.rows, j) == pa)
                    .collect();
                changed = true;
            }
        }
    } else {
        // 기선택 행 프레스 = 선택 유지(블록 드래그 후보 — 컬럼/파일 규약),
        // 미선택 행 = 단일 선택
        if !st.sel.contains(&i) {
            st.sel = vec![i];
            st.anchor = Some(i);
            changed = true;
        }
        // 드래그 후보(임계 초과 시 활성 — 07-19)
        if let Some((start, count)) = st.block_of_selection() {
            if i >= start && i < start + count {
                let sp = st.sibling_pos(start);
                st.drag = Some(OtDrag {
                    press_y: y,
                    cur_y: y,
                    active: false,
                    start,
                    count,
                    level: st.rows[start].1,
                    sibling_pos: sp,
                    orig_sibling_pos: sp,
                    orig_rows: st.rows.clone(),
                    orig_sel: st.sel.clone(),
                });
                let _ = SetCapture(hwnd);
            }
        }
    }
    if changed {
        let _ = InvalidateRect(Some(hwnd), None, false);
        super::base::notify(hwnd, NXOT_SELCHANGE);
    }
}

unsafe fn on_mouse_move(hwnd: HWND, lp: LPARAM) {
    let Some(st) = super::base::state::<OtState>(hwnd).as_mut() else {
        return;
    };
    let y = (lp.0 as u32 >> 16) as i16 as i32;
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let ch = rc.bottom - rc.top;
    if let Some((sy, s0)) = st.sb_drag {
        // 썸 드래그 — 트랙 비율 → 콘텐츠 스크롤(그리드 규약)
        let content = st.content_h();
        if content > ch {
            let dy = y - sy;
            st.scroll_y = s0 + dy * content / ch.max(1);
            st.clamp_scroll(ch);
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
        return;
    }
    let mut needs_timer = false;
    if let Some(d) = st.drag.as_mut() {
        d.cur_y = y;
        if !d.active && (y - d.press_y).abs() > 5 {
            d.active = true;
        }
        if d.active {
            needs_timer = y < EDGE || y > ch - EDGE;
        }
    }
    if st.drag.as_ref().is_some_and(|d| d.active) {
        st.preview_to(y);
        if needs_timer {
            SetTimer(Some(hwnd), TIMER_SCROLL, 60, None);
        } else {
            let _ = KillTimer(Some(hwnd), TIMER_SCROLL);
        }
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

unsafe fn on_lbutton_up(hwnd: HWND) {
    let Some(st) = super::base::state::<OtState>(hwnd).as_mut() else {
        return;
    };
    let _ = KillTimer(Some(hwnd), TIMER_SCROLL);
    let _ = ReleaseCapture();
    if st.sb_drag.take().is_some() {
        let _ = InvalidateRect(Some(hwnd), None, false);
        return;
    }
    if let Some(d) = st.drag.take() {
        if d.active {
            let delta = d.sibling_pos - d.orig_sibling_pos;
            // 뷰는 원상 복원(모델–뷰 정합) — 호스트가 delta만큼 모델 이동 후
            // set_rows/set_selection으로 재구성한다
            st.rows = d.orig_rows;
            st.sel = d.orig_sel;
            let _ = InvalidateRect(Some(hwnd), None, false);
            if delta != 0 {
                st.drag_delta = delta;
                super::base::notify(hwnd, NXOT_DRAGMOVE);
            }
        }
    }
}

unsafe fn paint(hwnd: HWND, st: &OtState) {
    let mut ps = PAINTSTRUCT::default();
    let dc = BeginPaint(hwnd, &mut ps);
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let ch = rc.bottom - rc.top;
    fill(dc, &rc, st.style.bg);
    frame(dc, &rc, st.style.border);
    let old = SelectObject(dc, st.font.into());
    SetBkMode(dc, TRANSPARENT);
    for (d, &i) in st.visible().iter().enumerate() {
        let (label, level, check) = &st.rows[i];
        let top = 1 + d as i32 * ROW_H - st.scroll_y;
        if top + ROW_H < 0 {
            continue;
        }
        if top >= rc.bottom {
            break;
        }
        let row = RECT {
            left: rc.left + 1,
            top,
            right: rc.right - 1,
            bottom: top + ROW_H,
        };
        let selected = st.sel.contains(&i);
        if selected {
            fill(dc, &row, st.style.sel_bg);
        }
        SetTextColor(
            dc,
            if *level == 0 {
                st.style.text
            } else {
                st.style.text_dim
            },
        );
        let dis = check_disabled(&st.rows, i);
        // 표시 체크(첫 컬럼 고정 — 레벨 무관, 사용자 확정 07-19)
        if let Some(on) = check {
            let bs = 12;
            let bx = row.left + 5;
            let by = top + (ROW_H - bs) / 2;
            let brc = RECT {
                left: bx,
                top: by,
                right: bx + bs,
                bottom: by + bs,
            };
            // 부모 그룹 해제 = 자식 체크 비활성(값 유지·흐림 — 07-19)
            frame(
                dc,
                &brc,
                if dis {
                    st.style.text_dim
                } else {
                    st.style.border
                },
            );
            if *on {
                let irc = RECT {
                    left: bx + 3,
                    top: by + 3,
                    right: bx + bs - 3,
                    bottom: by + bs - 3,
                };
                fill(
                    dc,
                    &irc,
                    if dis {
                        st.style.text_dim
                    } else {
                        st.style.accent
                    },
                );
            }
        }
        // 셰브론 존(체크 뒤): 하위가 있는 그룹만 표시(사용자 확정 07-19)
        if *level == 0 && st.rows.get(i + 1).is_some_and(|r| r.1 == 1) {
            let closed = st.collapsed.contains(label);
            let mold = SelectObject(dc, st.marker_font.into());
            let mut m16: Vec<u16> = if closed { "\u{E76C}" } else { "\u{E70D}" }
                .encode_utf16()
                .collect();
            let mut mrc = RECT {
                left: row.left + 2 + CHECK_COL,
                top: row.top,
                right: row.left + 2 + CHECK_COL + MARK_W,
                bottom: row.bottom,
            };
            SetTextColor(dc, st.style.text_dim);
            DrawTextW(dc, &mut m16, &mut mrc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
            SelectObject(dc, mold);
            SetTextColor(dc, st.style.text); // 그룹 라벨 색 복원
        }
        let tx = row.left + 2 + CHECK_COL + MARK_W + 2 + *level as i32 * INDENT;
        if dis {
            SetTextColor(dc, st.style.text_dim); // 라벨도 흐림
        }
        let mut w16: Vec<u16> = label.encode_utf16().collect();
        if w16.is_empty() {
            continue; // 빈 Vec→DrawTextW = AV(원장)
        }
        let mut trc = RECT {
            left: tx,
            top: row.top,
            right: row.right - 4,
            bottom: row.bottom,
        };
        DrawTextW(dc, &mut w16, &mut trc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
    }
    // 오버레이 스크롤 썸(내용 초과 시 — 파일 목록 그리드 차용: 얇은 썸·
    // 드래그 중 진하게)
    let content = st.content_h();
    if content > ch {
        let th = (ch * ch / content).max(24);
        let ty = 1 + st.scroll_y * (ch - th) / (content - ch).max(1);
        let wthumb = if st.sb_drag.is_some() { 8 } else { 5 };
        let trc = RECT {
            left: rc.right - wthumb - 2,
            top: ty,
            right: rc.right - 2,
            bottom: ty + th,
        };
        fill(dc, &trc, st.style.border);
    }
    // 드래그 고스트 박스(07-19 — 컬럼 이동 규약 차용: 커서 y 추종·x 고정)
    if let Some(d) = &st.drag {
        if d.active {
            let n = st.sel.len();
            let label0 = st
                .sel
                .first()
                .and_then(|&i| st.rows.get(i))
                .map(|(l, _, _)| l.clone())
                .unwrap_or_default();
            let text = if n > 1 {
                format!("{label0} (+{})", n - 1)
            } else {
                label0
            };
            if !text.is_empty() {
                let gh = ROW_H;
                let gy = (d.cur_y - gh / 2).clamp(0, (ch - gh).max(0));
                let grc = RECT {
                    left: rc.left + 6,
                    top: gy,
                    right: rc.right - 14,
                    bottom: gy + gh,
                };
                fill(dc, &grc, st.style.sel_bg);
                frame(dc, &grc, st.style.accent);
                SetTextColor(dc, st.style.text);
                let mut w16: Vec<u16> = text.encode_utf16().collect();
                let mut trc = RECT {
                    left: grc.left + 8,
                    top: grc.top,
                    right: grc.right - 4,
                    bottom: grc.bottom,
                };
                DrawTextW(dc, &mut w16, &mut trc, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
            }
        }
    }
    SelectObject(dc, old);
    let _ = EndPaint(hwnd, &ps);
}
