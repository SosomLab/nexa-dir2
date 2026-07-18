//! iconbutton — **NxIconButton** 도형 투명 이미지 버튼(ctl 10호 — macOS 시안
//! 07-17: 원형 +/− 버튼. 라이브러리 추상화 — 앱 비결합).
//!
//! ## 설계(사용자 질의 07-17 — "추상 클래스+도형별 vs PNG 1개")
//! **단일 컨트롤 + 아이콘 소스 enum**을 채택한다:
//! - Rust/Win32에는 클래스 상속이 없어 "추상 기반 + 도형별 서브클래스"는 창
//!   클래스·플러밍(상태·통지·히트테스트) 중복만 남는다 — 도형은 **데이터**로.
//! - [`Icon`] enum이 소스 추상화: 현재 = 벡터 글리프(펜 — DPI/색 자유·자산 0).
//!   **PNG 등 알파 이미지는 같은 enum의 확장 변형**(`Icon::Image(HBITMAP)` —
//!   32bpp premultiplied + msimg32 AlphaBlend[OS 인박스])으로 추가한다. 호스트
//!   API는 불변.
//! - **shape 투명** = 모서리를 `style.behind`(부모 배경색)로 칠하고 AA 원판을
//!   블렌드(07-17 개정 — 1비트 리전 클립은 계단 가장자리라 폐기).
//!
//! ## 계약(판매용 명세 — 클래스 `Nexa.NxIconButton`)
//! - 생성: [`create`] — `d <= 0` = **글꼴 높이 지름**(NxCheckBox 박스와 동일
//!   크기 — 사용자 확정 07-17. 배치는 호스트가 세로 중앙 정렬).
//! - 클릭(enabled일 때만) → `WM_COMMAND(MAKEWPARAM(id, NXIB_CLICK))`.
//! - [`NXIB_GETENABLE`]/[`NXIB_SETENABLE`](WM_USER+100/101): 비활성 =
//!   sel_bg 원 + 글리프(시안 — 삭제 대상이 자신뿐인 − 버튼), 클릭 무시.

use nexa_gui::{DrawCtx, Rect};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, InvalidateRect, PAINTSTRUCT};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetDlgCtrlID, GetParent, SendMessageW, HMENU, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_LBUTTONDOWN, WM_PAINT, WS_CHILD,
    WS_TABSTOP, WS_VISIBLE,
};

use super::gdipctx::{color, GdipCtx};
use super::style::{fill, Style};

/// 클릭 통지(WM_COMMAND HIWORD — enabled일 때만).
pub const NXIB_CLICK: u32 = 1;
/// 활성 상태 조회(반환 = 0/1).
pub const NXIB_GETENABLE: u32 = 0x0400 + 100;
/// 활성 상태 설정(wparam = 0/1 — 재도장).
pub const NXIB_SETENABLE: u32 = 0x0400 + 101;

/// 아이콘 소스 — 현재 벡터 글리프(펜). PNG/HBITMAP 알파 이미지는 확장 변형으로
/// 추가(모듈 설계 주석 — AlphaBlend 경로), 호스트 API 불변.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    /// ＋ (십자)
    Plus,
    /// − (수평선)
    Minus,
    /// ? (도움말 — 글자는 GDI 텍스트: 텍스트 = GDI 규약, 07-17)
    Help,
}

struct IbState {
    icon: Icon,
    enabled: bool,
    style: Style,
    /// Help(?) 글자 렌더용(텍스트 = GDI 규약).
    font: windows::Win32::Graphics::Gdi::HFONT,
    /// 이미지 모드(07-18 — [`create_image`]): 활성/비활성 raster(디코드된 GDI+
    /// 이미지, 널 = 벡터 글리프 모드). enum 확장 변형 규약의 실체.
    img_on: *mut windows::Win32::Graphics::GdiPlus::GpImage,
    img_off: *mut windows::Win32::Graphics::GdiPlus::GpImage,
}

impl Drop for IbState {
    fn drop(&mut self) {
        // raster 이미지 RAII 해제(base::drop_state가 박스 회수 시)
        unsafe {
            super::gdipctx::dispose_image(self.img_on);
            super::gdipctx::dispose_image(self.img_off);
        }
    }
}

static REGISTER: std::sync::Once = std::sync::Once::new();
const CLASS: PCWSTR = w!("Nexa.NxIconButton");

/// NxIconButton 생성 — 원형(지름 `d`·`d <= 0` = 공통 자동 높이). shape 밖 투명.
#[allow(clippy::too_many_arguments)] // Win32 create 계열 관례
pub unsafe fn create(
    parent: HWND,
    x: i32,
    y: i32,
    d: i32,
    id: u32,
    font: windows::Win32::Graphics::Gdi::HFONT,
    icon: Icon,
    enabled: bool,
    style: Style,
) -> HWND {
    super::base::register_class(&REGISTER, CLASS, Some(proc));
    // d<=0 = 글꼴 높이 지름(**체크박스 박스와 동일 크기** — 사용자 확정 07-17:
    // 같은 row에서 체크박스·이미지 버튼의 시각 크기가 일치)
    let d = if d <= 0 {
        super::style::font_height(parent, font).max(10)
    } else {
        d
    };
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        CLASS,
        w!(""),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(WS_TABSTOP.0),
        x,
        y,
        d,
        d,
        Some(parent),
        Some(HMENU(id as usize as *mut core::ffi::c_void)),
        None,
        None,
    )
    .unwrap_or_default();
    let st = Box::new(IbState {
        icon,
        enabled,
        style,
        font,
        img_on: std::ptr::null_mut(),
        img_off: std::ptr::null_mut(),
    });
    super::base::attach_state(hwnd, st);
    // shape 투명(07-17 AA 개정): 1비트 리전 클립은 계단 가장자리의 진범 —
    // 대신 모서리를 style.behind(부모 배경색)로 칠하고 AA 원판을 얹는다.
    hwnd
}

/// **이미지 아이콘 버튼**(07-18 — 리네임 ± raster 교체): `active`/`disabled`
/// PNG 바이트로 원형 벡터 대신 알파 이미지를 그린다(상태 전환은 enabled).
/// enum 확장 변형(모듈 설계 §)의 실체 — 벡터 `create`와 동일 통지·히트테스트.
#[allow(clippy::too_many_arguments)] // Win32 create 계열 관례
pub unsafe fn create_image(
    parent: HWND,
    x: i32,
    y: i32,
    d: i32,
    id: u32,
    font: windows::Win32::Graphics::Gdi::HFONT,
    active: &[u8],
    disabled: &[u8],
    enabled: bool,
    style: Style,
) -> HWND {
    let hwnd = create(parent, x, y, d, id, font, Icon::Plus, enabled, style);
    if let Some(st) = state(hwnd).as_mut() {
        st.img_on = super::gdipctx::decode_png(active);
        st.img_off = super::gdipctx::decode_png(disabled);
        let _ = InvalidateRect(Some(hwnd), None, true);
    }
    hwnd
}

unsafe fn state(hwnd: HWND) -> *mut IbState {
    super::base::state(hwnd)
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => LRESULT(0),
        WM_DESTROY => {
            super::base::drop_state::<IbState>(hwnd);
            LRESULT(0)
        }
        m if m == NXIB_GETENABLE => LRESULT(state(hwnd).as_ref().map_or(0, |s| s.enabled as isize)),
        m if m == NXIB_SETENABLE => {
            if let Some(st) = state(hwnd).as_mut() {
                st.enabled = wparam.0 != 0;
                let _ = InvalidateRect(Some(hwnd), None, true);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            if let Some(st) = state(hwnd).as_ref() {
                if st.enabled {
                    if let Ok(parent) = GetParent(hwnd) {
                        let id = GetDlgCtrlID(hwnd) as u32;
                        SendMessageW(
                            parent,
                            WM_COMMAND,
                            Some(WPARAM(((NXIB_CLICK as usize) << 16) | id as usize)),
                            Some(LPARAM(hwnd.0 as isize)),
                        );
                    }
                }
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            if let Some(st) = state(hwnd).as_ref() {
                let mut rc = RECT::default();
                let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rc);
                // 모서리 = behind(부모 배경색) → AA 원판/이미지 알파가 자연 블렌드
                fill(dc, &rc, st.style.behind);
                // 이미지 모드(07-18 raster): 활성/비활성 PNG를 셀에 스케일 드로우
                let img = if st.enabled { st.img_on } else { st.img_off };
                if !img.is_null() {
                    let mut g = GdipCtx::new(dc);
                    g.draw_image(
                        img,
                        Rect::new(rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top),
                    );
                    let _ = EndPaint(hwnd, &ps);
                    return LRESULT(0);
                }
                // 원판: 활성 = text_dim(진회색)·비활성 = border(중간 회색 — QA 07-17:
                // sel_bg는 타이틀 밴드와 동색이라 묻힘 → 한 단계 진하게)
                let disc = if st.enabled {
                    st.style.text_dim
                } else {
                    st.style.border
                };
                // AA 도형 = DrawCtx 백엔드만(07-17 규약 — GDI+ 직접 호출 금지)
                let mut g = GdipCtx::new(dc);
                g.fill_ellipse(
                    Rect::new(rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top),
                    color(disc),
                );
                // 글리프 = bg 색 AA 폴리라인 2px(흰 +/−) · Help = GDI 텍스트 "?"
                let d = rc.right - rc.left;
                let (cx, cy) = ((rc.left + rc.right) / 2, (rc.top + rc.bottom) / 2);
                let arm = (d / 4).max(3);
                match st.icon {
                    Icon::Plus => {
                        g.polyline(&[(cx - arm, cy), (cx + arm, cy)], color(st.style.bg), 2.0);
                        g.polyline(&[(cx, cy - arm), (cx, cy + arm)], color(st.style.bg), 2.0);
                    }
                    Icon::Minus => {
                        g.polyline(&[(cx - arm, cy), (cx + arm, cy)], color(st.style.bg), 2.0);
                    }
                    Icon::Help => {
                        drop(g); // GDI 텍스트 전에 Graphics 해제(HDC 혼용 규약)
                        use windows::Win32::Graphics::Gdi::{
                            DrawTextW, SelectObject, SetBkMode, SetTextColor, DT_CENTER,
                            DT_SINGLELINE, DT_VCENTER, TRANSPARENT,
                        };
                        let old = SelectObject(dc, st.font.into());
                        SetBkMode(dc, TRANSPARENT);
                        SetTextColor(dc, st.style.bg);
                        let mut w16: Vec<u16> = "?".encode_utf16().collect();
                        let mut trc = rc;
                        DrawTextW(
                            dc,
                            &mut w16,
                            &mut trc,
                            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                        );
                        SelectObject(dc, old);
                        let _ = EndPaint(hwnd, &ps);
                        return LRESULT(0);
                    }
                }
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
