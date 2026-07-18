//! ordertree — **NxOrderTree** 순서 편집 트리(ctl 15호, 07-19 사용자:
//! 도구모음 순서 설정). 라이브러리 추상화 — 앱 비결합·comctl32 비의존.
//!
//! ## 계약(판매용 명세 — 클래스 `Nexa.NxOrderTree`)
//! - 생성: [`create`] — 행 목록은 [`set_rows`]로 주입(라벨 복사 소유).
//!   행 = `(라벨, 레벨)` — 레벨 0 = 최상위(그룹/단일)·1 = 그룹 자식.
//!   부모 관계는 **순서에서 유도**(자식 = 직전 레벨 0 행에 소속).
//! - 선택: 클릭 = 단일 · **Shift+클릭 = 연속 범위**(사용자 확정 07-19 —
//!   Ctrl 비연속 미지원). 범위는 **같은 레벨·같은 부모(형제)**일 때만
//!   적용되고 사이의 다른 레벨 행은 선택에 포함되지 않는다 — 레벨 혼합
//!   선택 차단 규칙.
//! - 통지: 선택 변경 = `WM_COMMAND(MAKEWPARAM(id, NXOT_SELCHANGE))`.
//! - 조회/설정: [`selection`](오름차순 index)·[`set_selection`]·[`set_rows`]
//!   (재설정 시 선택/앵커 초기화 — 호스트가 이동 후 재선택).
//! - 이동 자체는 호스트 몫(호스트가 모델 재배열 → set_rows → set_selection).

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, DrawTextW, EndPaint, InvalidateRect, SelectObject, SetBkMode, SetTextColor,
    DT_LEFT, DT_SINGLELINE, DT_VCENTER, HFONT, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_SHIFT};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetClientRect, HMENU, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_DESTROY, WM_ERASEBKGND, WM_LBUTTONDOWN, WM_PAINT, WS_CHILD, WS_VISIBLE,
};

use super::style::{fill, frame, Style};

/// 선택 변경 통지(WM_COMMAND HIWORD).
pub const NXOT_SELCHANGE: u32 = 1;
/// 체크 토글 통지(07-19 — 표시 여부 편집). 토글 행 = [`take_toggled`].
pub const NXOT_TOGGLE: u32 = 2;

/// 행 높이(px @96dpi).
const ROW_H: i32 = 22;
/// 레벨당 들여쓰기(px).
const INDENT: i32 = 18;

struct OtState {
    /// (라벨, 레벨 0/1, 체크 — None = 체크 열 없음).
    rows: Vec<(String, u8, Option<bool>)>,
    /// 선택(index 오름차순 — 항상 같은 부모의 형제).
    sel: Vec<usize>,
    anchor: Option<usize>,
    /// 마지막 토글 행(NXOT_TOGGLE 통지 후 호스트 수거).
    toggled: Option<usize>,
    font: HFONT,
    style: Style,
}

static REGISTER: std::sync::Once = std::sync::Once::new();
const CLASS: PCWSTR = w!("Nexa.NxOrderTree");

/// 부모 index(-1 = 최상위) — 순서 유도: 레벨 1 행은 직전 레벨 0 행 소속.
fn parent_of(rows: &[(String, u8, Option<bool>)], i: usize) -> i32 {
    if rows[i].1 == 0 {
        return -1;
    }
    (0..i)
        .rev()
        .find(|&j| rows[j].1 == 0)
        .map(|j| j as i32)
        .unwrap_or(-1)
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
                font,
                style,
            }),
        );
    }
    hwnd
}

/// 행 재설정(선택/앵커 초기화 — 호스트가 이동 후 [`set_selection`]).
pub unsafe fn set_rows(hwnd: HWND, rows: Vec<(String, u8, Option<bool>)>) {
    if let Some(st) = super::base::state::<OtState>(hwnd).as_mut() {
        st.rows = rows;
        st.sel.clear();
        st.anchor = None;
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

/// 선택 설정(범위 검증 없음 — 호스트가 형제 집합을 보장).
pub unsafe fn set_selection(hwnd: HWND, sel: &[usize]) {
    if let Some(st) = super::base::state::<OtState>(hwnd).as_mut() {
        st.sel = sel.to_vec();
        st.sel.sort_unstable();
        st.anchor = st.sel.first().copied();
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

/// 필요 높이(행 수 기준 — 호스트 레이아웃 편의).
pub fn height_for(rows: usize) -> i32 {
    rows as i32 * ROW_H + 2
}

/// 마지막 체크 토글 행 수거(1회성 — NXOT_TOGGLE 통지 후 호출).
/// 토글은 내부 상태에 이미 반영됨(호스트 거부 시 [`set_rows`]로 재설정).
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

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            if let Some(st) = super::base::state::<OtState>(hwnd).as_ref() {
                let mut ps = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut ps);
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                fill(dc, &rc, st.style.bg);
                frame(dc, &rc, st.style.border);
                let old = SelectObject(dc, st.font.into());
                SetBkMode(dc, TRANSPARENT);
                for (i, (label, level, check)) in st.rows.iter().enumerate() {
                    let top = 1 + i as i32 * ROW_H;
                    let row = RECT {
                        left: rc.left + 1,
                        top,
                        right: rc.right - 1,
                        bottom: top + ROW_H,
                    };
                    if row.top >= rc.bottom {
                        break;
                    }
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
                    let mut tx = row.left + 8 + *level as i32 * INDENT;
                    if let Some(on) = check {
                        // 체크 박스(12px — 표시 여부 열, 07-19)
                        let bs = 12;
                        let bx = tx;
                        let by = row.top + (ROW_H - bs) / 2;
                        let brc = RECT {
                            left: bx,
                            top: by,
                            right: bx + bs,
                            bottom: by + bs,
                        };
                        frame(dc, &brc, st.style.border);
                        if *on {
                            let irc = RECT {
                                left: bx + 3,
                                top: by + 3,
                                right: bx + bs - 3,
                                bottom: by + bs - 3,
                            };
                            fill(dc, &irc, st.style.accent);
                        }
                        tx += bs + 6;
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
                    DrawTextW(
                        dc,
                        &mut w16,
                        &mut trc,
                        DT_LEFT | DT_SINGLELINE | DT_VCENTER,
                    );
                }
                SelectObject(dc, old);
                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_LBUTTONDOWN => {
            if let Some(st) = super::base::state::<OtState>(hwnd).as_mut() {
                let y = (lp.0 as u32 >> 16) as i16 as i32;
                let i = ((y - 1) / ROW_H) as usize;
                if (y - 1) >= 0 && i < st.rows.len() {
                    let x = (lp.0 as u32 & 0xFFFF) as i16 as i32;
                    // 체크 존(박스 12px + 여백) 클릭 = 표시 토글(07-19)
                    if let Some(on) = st.rows[i].2 {
                        let bx = 2 + 8 + st.rows[i].1 as i32 * INDENT;
                        if x >= bx - 2 && x < bx + 16 {
                            st.rows[i].2 = Some(!on);
                            st.toggled = Some(i);
                            let _ = InvalidateRect(Some(hwnd), None, false);
                            super::base::notify(hwnd, NXOT_TOGGLE);
                            return LRESULT(0);
                        }
                    }
                    let shift = GetKeyState(VK_SHIFT.0 as i32) < 0;
                    let mut changed = false;
                    if shift {
                        if let Some(a) = st.anchor {
                            // 범위 = 같은 레벨·같은 부모 형제만(레벨 혼합 차단)
                            if st.rows[a].1 == st.rows[i].1
                                && parent_of(&st.rows, a) == parent_of(&st.rows, i)
                            {
                                let (lo, hi) = (a.min(i), a.max(i));
                                let (lv, pa) = (st.rows[a].1, parent_of(&st.rows, a));
                                st.sel = (lo..=hi)
                                    .filter(|&j| {
                                        st.rows[j].1 == lv && parent_of(&st.rows, j) == pa
                                    })
                                    .collect();
                                changed = true;
                            }
                        }
                    }
                    if !changed && !shift {
                        st.sel = vec![i];
                        st.anchor = Some(i);
                        changed = true;
                    }
                    if changed {
                        let _ = InvalidateRect(Some(hwnd), None, false);
                        super::base::notify(hwnd, NXOT_SELCHANGE);
                    }
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            super::base::drop_state::<OtState>(hwnd);
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
