//! Win32 창·메시지 루프 — 렌더·입력은 nexa-gui 위젯, 데이터는 nexa-tree(M1-3 배선).
//! 이 모듈의 책임: 창 수명·백버퍼(DW 비트맵 렌더 타깃)·WM_* → [`nexa_gui::InputEvent`] 번역·
//! [`nexa_gui::Invalidations`] → `InvalidateRect` 번역. 텍스트 경로 = DirectWrite interop(ADR-0002).
//! F3 = 200프레임 스크롤 벤치(M1-9 게이트 관측용) — 타이틀바에 평균 페인트 시간 표시.

use std::time::Instant;

use nexa_gui::widgets::VirtualRows;
use nexa_gui::{Column, InputEvent, Invalidations, Key, Rect as GRect, Theme, Widget};
use nexa_tree::Tree;
use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, EndPaint, InvalidateRect, UpdateWindow, PAINTSTRUCT, SRCCOPY,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
use windows::Win32::UI::HiDpi::{
    GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, VK_DOWN, VK_END, VK_F3, VK_HOME, VK_NEXT, VK_PRIOR, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetMessageW,
    GetWindowLongPtrW, LoadCursorW, PostQuitMessage, RegisterClassW, SetWindowLongPtrW,
    SetWindowPos, SetWindowTextW, TranslateMessage, CREATESTRUCTW, CW_USEDEFAULT, GWLP_USERDATA,
    IDC_ARROW, MSG, SWP_NOACTIVATE, SWP_NOZORDER, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND,
    WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL,
    WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_SIZE, WNDCLASSW, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

use crate::dw::{DwBackend, DwCtx};
use crate::source::{TreeSource, COL_EXT, COL_KIND, COL_MODIFIED, COL_NAME, COL_SIZE};

/// wParam 마우스 수식키 비트(winuser.h MK_SHIFT).
const MK_SHIFT: usize = 0x0004;

/// F3 스크롤 벤치 프레임 수(M1-9 게이트 관측).
const BENCH_FRAMES: usize = 200;

/// 평균 페인트 시간(µs) 누적 — 벤치 시 리셋.
#[derive(Default)]
struct PaintStats {
    total_us: u64,
    frames: u32,
}

impl PaintStats {
    fn add(&mut self, us: u64) {
        self.total_us += us;
        self.frames += 1;
    }
    fn avg_us(&self) -> u64 {
        if self.frames == 0 {
            0
        } else {
            self.total_us / self.frames as u64
        }
    }
    fn reset(&mut self) {
        *self = PaintStats::default();
    }
}

/// 창 단위 상태 — `GWLP_USERDATA`에 Box raw 포인터로 보관(WM_NCCREATE~WM_NCDESTROY).
struct State {
    rows: VirtualRows<TreeSource>,
    theme: Theme,
    dpi: u32,
    dw: Option<DwBackend>,
    stats: PaintStats,
}

/// DPI 의존 지표: (행 높이, 좌 패딩, 트리 들여쓰기 폭). 고밀도 규약 = 20px 행 @96dpi.
fn metrics(dpi: u32) -> (i32, i32, i32) {
    let s = |v: i32| (v * dpi as i32) / 96;
    (s(20).max(14), s(6), s(16))
}

/// 기본 5컬럼(원본 docs/23 §2-1 기본 정의열 중 M1-4 범위). 폭은 dpi 스케일.
fn columns(dpi: u32) -> Vec<Column> {
    let s = |v: i32| (v * dpi as i32) / 96;
    vec![
        Column::new(COL_NAME, "이름", s(340)),
        Column::new(COL_EXT, "확장자", s(64)),
        Column::new(COL_SIZE, "크기", s(96)).right_aligned(),
        Column::new(COL_MODIFIED, "수정한 날짜", s(140)),
        Column::new(COL_KIND, "종류", s(110)),
    ]
}

/// 로컬 타임존 오프셋(분, UTC 동쪽 양수) — 수정한 날짜 표시용(원본은 셸 표시 규약).
unsafe fn tz_offset_min() -> i32 {
    let mut tzi = TIME_ZONE_INFORMATION::default();
    let code = GetTimeZoneInformation(&mut tzi);
    // Bias는 UTC = local + bias(분, 서쪽 양수) → 표시 오프셋은 부호 반전. DST 활성 시 보정.
    let bias = tzi.Bias
        + match code {
            1 => tzi.StandardBias,
            2 => tzi.DaylightBias,
            _ => 0,
        };
    -bias
}

/// 표시할 루트 경로: argv[1] → %USERPROFILE% → C:\.
fn root_path() -> std::path::PathBuf {
    std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(std::path::PathBuf::from))
        .unwrap_or_else(|| std::path::PathBuf::from("C:\\"))
}

pub fn run() -> Result<()> {
    // 창 생성 전 트리 로드(초안: 동기 — 백그라운드 열거는 M2 상주 규율에서)
    let path = root_path();
    let tree = Tree::open(&path).unwrap_or_else(|e| {
        eprintln!("{} 열기 실패({e}) — C:\\ 로 대체", path.display());
        Tree::open("C:\\").expect("C:\\ 열기 실패")
    });
    let (row_h, pad_x, indent_w) = metrics(96); // 실제 DPI는 WM_NCCREATE에서 반영
    let tz = unsafe { tz_offset_min() };
    let state = Box::new(State {
        rows: VirtualRows::new(TreeSource::new(tree, tz), row_h, pad_x, indent_w),
        theme: Theme::default(), // 다크(DR-5) — 모드 선택은 M2 테마 시스템
        dpi: 96,
        dw: None,
        stats: PaintStats::default(),
    });

    unsafe {
        // PerMonitorV2 DPI — 매니페스트 도입 전까지 코드로 선언(docs/01 §3)
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let hinstance = GetModuleHandleW(None)?;
        let class_name = w!("NexaDir2Main");
        let wc = WNDCLASSW {
            hInstance: hinstance.into(),
            lpszClassName: class_name,
            lpfnWndProc: Some(wndproc),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        let atom = RegisterClassW(&wc);
        debug_assert_ne!(atom, 0, "RegisterClassW 실패");

        CreateWindowExW(
            Default::default(),
            class_name,
            w!("Nexa Dir 2"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1200,
            800,
            None,
            None,
            Some(hinstance.into()),
            Some(Box::into_raw(state) as *const core::ffi::c_void),
        )?;

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

unsafe fn state_of<'a>(hwnd: HWND) -> Option<&'a mut State> {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
    ptr.as_mut()
}

unsafe fn client_rect(hwnd: HWND) -> GRect {
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    GRect::new(0, 0, rc.right - rc.left, rc.bottom - rc.top)
}

/// 위젯이 수집한 무효화 rect를 OS 무효화로 번역.
unsafe fn flush_invalidations(hwnd: HWND, inv: &mut Invalidations) {
    for r in inv.drain() {
        let rc = RECT {
            left: r.x,
            top: r.y,
            right: r.right(),
            bottom: r.bottom(),
        };
        let _ = InvalidateRect(Some(hwnd), Some(&rc), false);
    }
}

unsafe fn route_event(hwnd: HWND, st: &mut State, ev: InputEvent) {
    let mut inv = Invalidations::default();
    st.rows.on_event(&ev, &mut inv);
    flush_invalidations(hwnd, &mut inv);
}

/// 타이틀바 — 루트 경로·행 수·평균 페인트 시간(M1-9 관측).
unsafe fn update_title(hwnd: HWND, st: &State, note: &str) {
    use nexa_gui::widgets::RowSource;
    let text = format!(
        "Nexa Dir 2 — {} [{}행 · 평균 {}µs]{}\0",
        st.rows.source().tree().root_path().display(),
        st.rows.source().len(),
        st.stats.avg_us(),
        note,
    );
    let wtext: Vec<u16> = text.encode_utf16().collect();
    let _ = SetWindowTextW(hwnd, PCWSTR(wtext.as_ptr()));
}

/// DW 렌더 타깃을 클라이언트 크기와 일치시킨다(최초 생성 포함).
unsafe fn ensure_dw(st: &mut State, hdc: windows::Win32::Graphics::Gdi::HDC, w: i32, h: i32) {
    match &mut st.dw {
        None => match DwBackend::new(hdc, w, h, st.dpi) {
            Ok(b) => st.dw = Some(b),
            Err(e) => eprintln!("DirectWrite 초기화 실패: {e}"), // OS 인박스 — 실질 도달 불가
        },
        Some(b) => {
            if b.size() != (w, h) {
                let _ = b.resize(w, h);
            }
        }
    }
}

/// 위젯을 DW 백버퍼에 그린 뒤 화면으로 BitBlt — 가시 영역만(docs/01 §3).
unsafe fn paint(hwnd: HWND, st: &mut State) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let rc = client_rect(hwnd);
    let t0 = Instant::now();

    ensure_dw(st, hdc, rc.w, rc.h);
    if let Some(back) = &st.dw {
        let mut ctx = DwCtx { back };
        st.rows.paint(&mut ctx, &st.theme);
        let _ = BitBlt(hdc, 0, 0, rc.w, rc.h, Some(back.memory_dc()), 0, 0, SRCCOPY);
    }

    st.stats.add(t0.elapsed().as_micros() as u64);
    let _ = EndPaint(hwnd, &ps);

    // 첫 프레임(WM_NCCREATE의 SetWindowText는 창 생성 타이틀로 덮임) + 20프레임마다 갱신
    if st.stats.frames == 1 || st.stats.frames.is_multiple_of(20) {
        update_title(hwnd, st, "");
    }
}

/// F3 — 200프레임 연속 스크롤 벤치(동기 UpdateWindow). 결과는 타이틀바.
unsafe fn bench(hwnd: HWND, st: &mut State) {
    route_event(hwnd, st, InputEvent::Key(Key::Home));
    let _ = UpdateWindow(hwnd);
    st.stats.reset();
    for _ in 0..BENCH_FRAMES {
        // 휠 1노치(3행)씩 아래로 — 실사용 스크롤 패턴
        if let Some(s) = state_of(hwnd) {
            route_event(hwnd, s, InputEvent::Wheel { delta: -120 });
        }
        let _ = UpdateWindow(hwnd);
    }
    if let Some(s) = state_of(hwnd) {
        update_title(hwnd, s, " · 벤치 완료");
    }
}

fn vk_to_key(vk: u16) -> Option<Key> {
    match vk {
        k if k == VK_UP.0 => Some(Key::Up),
        k if k == VK_DOWN.0 => Some(Key::Down),
        k if k == VK_PRIOR.0 => Some(Key::PageUp),
        k if k == VK_NEXT.0 => Some(Key::PageDown),
        k if k == VK_HOME.0 => Some(Key::Home),
        k if k == VK_END.0 => Some(Key::End),
        _ => None,
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            // run()이 만든 State를 lpCreateParams로 받아 연결 + 실제 DPI 반영
            let cs = &*(lparam.0 as *const CREATESTRUCTW);
            let ptr = cs.lpCreateParams as *mut State;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize);
            if let Some(st) = ptr.as_mut() {
                st.dpi = GetDpiForWindow(hwnd);
                let (row_h, pad_x, indent_w) = metrics(st.dpi);
                let mut inv = Invalidations::default();
                st.rows.set_metrics(row_h, pad_x, indent_w, &mut inv);
                st.rows.set_columns(columns(st.dpi), &mut inv);
                update_title(hwnd, st, "");
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_PAINT => {
            if let Some(st) = state_of(hwnd) {
                paint(hwnd, st);
            }
            LRESULT(0)
        }
        // 더블 버퍼가 전체를 덮으므로 배경 지우기 생략(깜빡임 제거)
        WM_ERASEBKGND => LRESULT(1),
        WM_SIZE => {
            if let Some(st) = state_of(hwnd) {
                let mut inv = Invalidations::default();
                st.rows.set_bounds(client_rect(hwnd), &mut inv);
                flush_invalidations(hwnd, &mut inv);
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            if let Some(st) = state_of(hwnd) {
                let delta = (wparam.0 >> 16) as i16 as i32; // WHEEL_DELTA 단위(트랙패드는 분수 노치)
                if wparam.0 & MK_SHIFT != 0 {
                    // Shift+휠 = 가로(휠 아래 = 오른쪽 — 관례상 부호 반전)
                    route_event(hwnd, st, InputEvent::HWheel { delta: -delta });
                } else {
                    route_event(hwnd, st, InputEvent::Wheel { delta });
                }
            }
            LRESULT(0)
        }
        WM_MOUSEHWHEEL => {
            if let Some(st) = state_of(hwnd) {
                let delta = (wparam.0 >> 16) as i16 as i32; // 양수 = 오른쪽
                route_event(hwnd, st, InputEvent::HWheel { delta });
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            if let Some(st) = state_of(hwnd) {
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                // 리사이즈 드래그가 창 밖으로 나가도 추적되도록 캡처(MouseUp에서 해제)
                SetCapture(hwnd);
                let shift = wparam.0 & MK_SHIFT != 0;
                route_event(hwnd, st, InputEvent::MouseDown { x, y, shift });
                update_title(hwnd, st, ""); // 펼침/접힘·정렬로 행 수/헤더 변동 반영
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if let Some(st) = state_of(hwnd) {
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                route_event(hwnd, st, InputEvent::MouseMove { x, y });
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            if let Some(st) = state_of(hwnd) {
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                route_event(hwnd, st, InputEvent::MouseUp { x, y });
            }
            let _ = ReleaseCapture();
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if let Some(st) = state_of(hwnd) {
                let vk = wparam.0 as u16;
                if vk == VK_F3.0 {
                    bench(hwnd, st);
                } else if let Some(key) = vk_to_key(vk) {
                    route_event(hwnd, st, InputEvent::Key(key));
                }
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            if let Some(st) = state_of(hwnd) {
                let dpi = (wparam.0 & 0xFFFF) as u32;
                st.dpi = dpi;
                if let Some(dw) = &mut st.dw {
                    let _ = dw.set_dpi(dpi);
                }
                let (row_h, pad_x, indent_w) = metrics(dpi);
                let mut inv = Invalidations::default();
                st.rows.set_metrics(row_h, pad_x, indent_w, &mut inv);
                let rc = &*(lparam.0 as *const RECT);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    rc.left,
                    rc.top,
                    rc.right - rc.left,
                    rc.bottom - rc.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                flush_invalidations(hwnd, &mut inv);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_NCDESTROY => {
            let ptr = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) as *mut State;
            if !ptr.is_null() {
                drop(Box::from_raw(ptr)); // dw(COM 참조) 포함 전부 drop
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
