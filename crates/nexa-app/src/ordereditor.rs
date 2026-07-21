//! ordereditor — **순서/표시 편집 공통 모달 창**(07-19 사용자: "컬럼·툴바·
//! Context menu 설정은 각각 별도 창, 공통점은 어댑터 방식으로").
//!
//! 하나의 창 구현이 [`EditorSpec`](어댑터: 정의·라벨·표시 열·평면 여부·잠금)을
//! 받아 세 편집기(도구모음/컬럼/컨텍스트 메뉴)를 모두 구성한다. 값은
//! [`crate::config::parse_order_with`] 문법 문자열 — 변경 시마다 소유자에
//! [`WM_APP_ORDER_EDIT`]를 동기 통지(즉시 적용 X-8 규약)한다.
//!
//! 조작 규약(도구모음 편집과 동일 — 사용자 확정 07-19):
//! - 그룹(레벨 0) 선택 = 블록 통째 이동 · 자식(레벨 1) = 그룹 안에서만.
//! - Shift = 같은 레벨·같은 부모 연속 다중 선택(혼합 차단) — 일괄 이동.
//! - 표시 열(with_vis)은 체크 클릭 = 토글(잠금 key는 무시 — 예: 컬럼 name).

use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::HFONT;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetWindowLongPtrW, IsWindow, PostMessageW, RegisterClassW, SendMessageW, SetWindowLongPtrW,
    TranslateMessage, GWLP_USERDATA, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND,
    WM_DESTROY, WM_NCDESTROY, WNDCLASSW, WS_BORDER, WS_POPUP, WS_VISIBLE,
};

use crate::config::{self, OrderDefs};
use crate::ctl::{self, style::Style};

/// 편집 값 변경 통지(소유자 wndproc) — wparam = 호출 시 넘긴 `field`,
/// lparam = `*const String`(통지 동안만 유효 — 같은 스레드 동기, 즉시 복사).
pub const WM_APP_ORDER_EDIT: u32 = 0x8007; // WM_APP + 7

const ID_TREE: u32 = 1;
const ID_UP: u32 = 2;
const ID_DOWN: u32 = 3;

/// 편집기 어댑터 — 세 설정 창의 차이를 데이터로 기술.
pub struct EditorSpec {
    pub title: String,
    pub defs: OrderDefs,
    /// 표시 여부 열(체크) 사용 — 값 직렬화에 `:0/:1` 포함.
    pub with_vis: bool,
    /// 단일 블록 평면 표시(그룹 헤더 생략 — 컬럼 편집).
    pub flat: bool,
    /// 체크 잠금 key(해제 불가 — 예: 컬럼 `name`).
    pub locked: &'static [&'static str],
    /// (블록, 자식) → 표시 라벨(i18n).
    pub label: fn(&str, Option<&str>) -> String,
}

struct EdCtx {
    model: Vec<config::OrderBlock>,
    with_vis: bool,
    flat: bool,
    locked: &'static [&'static str],
    label: fn(&str, Option<&str>) -> String,
    field: u32,
    owner: HWND,
    tree: HWND,
}

impl EdCtx {
    /// 트리 행 구성 — [`row_map`]과 index 정렬 일치.
    fn rows(&self) -> Vec<(String, u8, Option<bool>)> {
        let mut rows = Vec::new();
        for (block, bvis, items) in &self.model {
            if self.flat {
                for (k, vis) in items {
                    rows.push((
                        (self.label)(block, Some(k)),
                        0,
                        self.with_vis.then_some(*vis),
                    ));
                }
            } else {
                // 그룹 체크(07-19 사용자): 해제 = 통째 숨김·자식 상태 보존
                rows.push(((self.label)(block, None), 0, self.with_vis.then_some(*bvis)));
                for (k, vis) in items {
                    rows.push((
                        (self.label)(block, Some(k)),
                        1,
                        self.with_vis.then_some(*vis),
                    ));
                }
            }
        }
        rows
    }

    /// 행 index → (블록 index, 자식 index — 그룹 헤더 행 = None).
    fn row_map(&self) -> Vec<(usize, Option<usize>)> {
        let mut map = Vec::new();
        for (bi, (_, _, items)) in self.model.iter().enumerate() {
            if !self.flat {
                map.push((bi, None));
            }
            for ii in 0..items.len() {
                map.push((bi, Some(ii)));
            }
        }
        map
    }

    /// 변경 통지(직렬화 → 소유자 동기 전달 — 즉시 적용).
    unsafe fn emit(&self) {
        let s = config::serialize_order_with(&self.model, self.with_vis);
        SendMessageW(
            self.owner,
            WM_APP_ORDER_EDIT,
            Some(WPARAM(self.field as usize)),
            Some(LPARAM(&s as *const String as isize)),
        );
    }

    /// 선택 이동(공용 규칙) — 그룹 = 블록 통째·자식 = 그룹 안. 이동 후 재선택.
    unsafe fn move_sel(&mut self, up: bool) {
        let sel = ctl::ordertree::selection(self.tree);
        if sel.is_empty() {
            return;
        }
        let map = self.row_map();
        let moved: Vec<(usize, Option<usize>)> = sel.iter().map(|&r| map[r]).collect();
        let ok = match moved[0].1 {
            None => {
                let bis: Vec<usize> = moved.iter().map(|(b, _)| *b).collect();
                shift_range(&mut self.model, &bis, up)
            }
            Some(_) => {
                let bi = moved[0].0;
                let iis: Vec<usize> = moved.iter().filter_map(|(_, i)| *i).collect();
                shift_range(&mut self.model[bi].2, &iis, up)
            }
        };
        if !ok {
            return;
        }
        ctl::ordertree::set_rows(self.tree, self.rows());
        let new_map = self.row_map();
        let delta = |v: usize| if up { v - 1 } else { v + 1 };
        let new_sel: Vec<usize> = match moved[0].1 {
            None => {
                let bis: Vec<usize> = moved.iter().map(|(b, _)| delta(*b)).collect();
                new_map
                    .iter()
                    .enumerate()
                    .filter(|(_, (b, i))| i.is_none() && bis.contains(b))
                    .map(|(r, _)| r)
                    .collect()
            }
            Some(_) => {
                let bi = moved[0].0;
                let iis: Vec<usize> = moved.iter().filter_map(|(_, i)| *i).map(delta).collect();
                new_map
                    .iter()
                    .enumerate()
                    .filter(|(_, (b, i))| *b == bi && i.is_some_and(|x| iis.contains(&x)))
                    .map(|(r, _)| r)
                    .collect()
            }
        };
        ctl::ordertree::set_selection(self.tree, &new_sel);
        self.emit();
    }

    /// 드래그 확정(07-19) — delta(형제 칸)를 단일 스텝 이동으로 반복 적용
    /// (모델·행·선택·통지는 move_sel 재사용 — 뷰는 이미 원상 복원됨).
    unsafe fn apply_drag(&mut self) {
        let delta = ctl::ordertree::take_drag_delta(self.tree);
        for _ in 0..delta.abs() {
            self.move_sel(delta < 0);
        }
    }

    /// ▲▼ 활성 동기(07-19 사용자: 선택 없으면 비활성).
    unsafe fn sync_buttons(&self, dlg: HWND) {
        use windows::Win32::UI::WindowsAndMessaging::GetDlgItem;
        let on = !ctl::ordertree::selection(self.tree).is_empty();
        for id in [ID_UP, ID_DOWN] {
            if let Ok(b) = GetDlgItem(Some(dlg), id as i32) {
                SendMessageW(
                    b,
                    ctl::iconbutton::NXIB_SETENABLE,
                    Some(WPARAM(usize::from(on))),
                    Some(LPARAM(0)),
                );
            }
        }
    }

    /// 체크 토글 반영(잠금 key = 거부 — 트리 재설정으로 원복).
    unsafe fn on_toggle(&mut self) {
        let Some(row) = ctl::ordertree::take_toggled(self.tree) else {
            return;
        };
        let map = self.row_map();
        let checks = ctl::ordertree::checks(self.tree);
        match map.get(row) {
            Some(&(bi, Some(ii))) => {
                let key = self.model[bi].2[ii].0.clone();
                if self.locked.contains(&key.as_str()) {
                    ctl::ordertree::set_rows(self.tree, self.rows()); // 잠금 — 원복
                    return;
                }
                if let Some(Some(on)) = checks.get(row) {
                    self.model[bi].2[ii].1 = *on;
                    self.emit();
                }
            }
            Some(&(bi, None)) => {
                // 그룹 체크(07-19): 통째 표시/숨김 — 자식 상태는 유지
                if let Some(Some(on)) = checks.get(row) {
                    self.model[bi].1 = *on;
                    self.emit();
                }
            }
            None => {
                ctl::ordertree::set_rows(self.tree, self.rows()); // 방어
            }
        }
    }
}

/// 연속 선택 집합 한 칸 이동(도구모음 편집과 동일 알고리즘).
fn shift_range<T>(v: &mut [T], sel: &[usize], up: bool) -> bool {
    let (lo, hi) = (sel[0], sel[sel.len() - 1]);
    if up {
        if lo == 0 {
            return false;
        }
        v[lo - 1..=hi].rotate_left(1);
    } else {
        if hi + 1 >= v.len() {
            return false;
        }
        v[lo..=hi + 1].rotate_right(1);
    }
    true
}

static REGISTER: std::sync::Once = std::sync::Once::new();

/// 편집 창 표시(모달 — 소유자 비활성화). 변경은 [`WM_APP_ORDER_EDIT`]로
/// 실시간 통지되므로 반환값 없음(닫기 = 완료).
pub unsafe fn show(owner: HWND, spec: &EditorSpec, value: &str, field: u32, font: HFONT) {
    use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
    use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
    REGISTER.call_once(|| {
        let wc = WNDCLASSW {
            lpszClassName: w!("NexaOrderEditor"),
            lpfnWndProc: Some(proc),
            hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH(
                (windows::Win32::Graphics::Gdi::COLOR_WINDOW.0 + 1) as isize
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
    let model = config::parse_order_with(spec.defs, value);
    let row_count = model
        .iter()
        .map(|(_, _, items)| items.len() + usize::from(!spec.flat))
        .sum::<usize>();
    const PAD: i32 = 14;
    let tree_w = 260;
    // 12행 초과 = 내부 스크롤(오버레이 썸 — 07-19)
    let tree_h = ctl::ordertree::height_for(row_count.min(12));
    let (w0, h0) = (PAD * 2 + tree_w + 40, PAD * 2 + 24 + tree_h + 8);
    let mut orc = windows::Win32::Foundation::RECT::default();
    let _ = windows::Win32::UI::WindowsAndMessaging::GetWindowRect(owner, &mut orc);
    let (cx, cy) = (
        orc.left + ((orc.right - orc.left) - w0) / 2,
        orc.top + ((orc.bottom - orc.top) - h0) / 2,
    );
    let title = windows::core::HSTRING::from(&*spec.title);
    let Ok(dlg) = CreateWindowExW(
        WINDOW_EX_STYLE(0x00000001), // DLGMODALFRAME
        w!("NexaOrderEditor"),
        windows::core::PCWSTR(title.as_ptr()),
        WINDOW_STYLE(
            WS_POPUP.0
                | WS_BORDER.0
                | windows::Win32::UI::WindowsAndMessaging::WS_CAPTION.0
                | windows::Win32::UI::WindowsAndMessaging::WS_SYSMENU.0,
        ) | WS_VISIBLE,
        cx,
        cy,
        w0,
        h0 + 30, // 캡션 보정
        Some(owner),
        None,
        None,
        None,
    ) else {
        return;
    };
    let style = Style::default();
    let tree = ctl::ordertree::create(dlg, PAD, PAD, tree_w, tree_h, ID_TREE, font, style);
    // ▲▼ = 사용자 제공 SVG 이미지 16px(07-19 — assets/ui, 원형 벡터 폐지·
    // 잉크 = 기본 텍스트색)
    let _ = ctl::iconbutton::create_svg(
        dlg,
        PAD + tree_w + 10,
        PAD + 2,
        16,
        ID_UP,
        font,
        include_str!("../assets/ui/arrow-up.svg"),
        false, // 선택 전 비활성(사용자 확정 07-19)
        style,
    );
    let _ = ctl::iconbutton::create_svg(
        dlg,
        PAD + tree_w + 10,
        PAD + 26,
        16,
        ID_DOWN,
        font,
        include_str!("../assets/ui/arrow-down.svg"),
        false, // 선택 전 비활성(사용자 확정 07-19)
        style,
    );
    let mut ctx = Box::new(EdCtx {
        model,
        with_vis: spec.with_vis,
        flat: spec.flat,
        locked: spec.locked,
        label: spec.label,
        field,
        owner,
        tree,
    });
    ctl::ordertree::set_rows(tree, ctx.rows());
    SetWindowLongPtrW(dlg, GWLP_USERDATA, &mut *ctx as *mut EdCtx as isize);
    let _ = EnableWindow(owner, false);
    let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(dlg)); // 키보드(07-19)
    let mut msg = MSG::default();
    while IsWindow(Some(dlg)).as_bool() && GetMessageW(&mut msg, None, 0, 0).as_bool() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    let _ = EnableWindow(owner, true);
    let _ = SetForegroundWindow(owner);
    drop(ctx);
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let ctx = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut EdCtx;
    match msg {
        WM_COMMAND => {
            let id = (wp.0 & 0xFFFF) as u32;
            let code = (wp.0 >> 16) as u32;
            if let Some(ctx) = ctx.as_mut() {
                match id {
                    ID_UP if code == ctl::iconbutton::NXIB_CLICK => ctx.move_sel(true),
                    ID_DOWN if code == ctl::iconbutton::NXIB_CLICK => ctx.move_sel(false),
                    ID_TREE if code == ctl::ordertree::NXOT_TOGGLE => ctx.on_toggle(),
                    ID_TREE if code == ctl::ordertree::NXOT_DRAGMOVE => ctx.apply_drag(),
                    _ => {}
                }
                ctx.sync_buttons(hwnd); // 선택 유무 = ▲▼ 활성(07-19)
            }
            LRESULT(0)
        }
        m if m == windows::Win32::UI::WindowsAndMessaging::WM_KEYDOWN => {
            use windows::Win32::UI::Input::KeyboardAndMouse::{
                GetKeyState, VK_CONTROL, VK_DOWN, VK_ESCAPE, VK_SHIFT, VK_SPACE, VK_UP,
            };
            let vk = wp.0 as u16;
            if vk == VK_ESCAPE.0 {
                // ESC = 드래그 취소(있으면) → 없으면 창 닫기(대화상자 관례)
                if let Some(ctx) = ctx.as_ref() {
                    if ctl::ordertree::cancel_drag(ctx.tree) {
                        return LRESULT(0);
                    }
                }
                let _ = DestroyWindow(hwnd);
                return LRESULT(0);
            }
            // 키보드 이동(07-19 사용자): ↑/↓ = 선택 · Shift = 형제 범위 확장 ·
            // Ctrl = 순서 이동(▲▼ 동일) · Space = 체크 토글. 트리 통지는
            // WM_COMMAND arm이 처리(중첩 차용 회피 — tree 핸들만 복사).
            if vk == VK_UP.0 || vk == VK_DOWN.0 {
                let up = vk == VK_UP.0;
                let ctrl = GetKeyState(VK_CONTROL.0 as i32) < 0;
                let shift = GetKeyState(VK_SHIFT.0 as i32) < 0;
                if ctrl {
                    if let Some(ctx) = ctx.as_mut() {
                        ctx.move_sel(up);
                        ctx.sync_buttons(hwnd);
                    }
                } else if let Some(tree) = ctx.as_ref().map(|c| c.tree) {
                    if shift {
                        ctl::ordertree::key_extend(tree, up);
                    } else {
                        ctl::ordertree::key_move(tree, up);
                    }
                }
                return LRESULT(0);
            }
            if vk == VK_SPACE.0 {
                if let Some(tree) = ctx.as_ref().map(|c| c.tree) {
                    ctl::ordertree::key_toggle(tree);
                }
                return LRESULT(0);
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            // 모달 루프 종료 트리거(IsWindow=false) — 상태는 show()가 소유
            let _ = PostMessageW(None, 0, WPARAM(0), LPARAM(0));
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_NCDESTROY => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
