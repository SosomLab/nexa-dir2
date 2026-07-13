//! 이미지 미리보기 **독립 실행 예제**(M4-2 검증·실험용) — 본편과 동일한 WIC 파이프라인을
//! 최소 창 하나로 재현한다: `CoInitialize → WIC 팩토리 → 디코더 → 스케일러(Fant·확대 없음)
//! → 32bppBGRA 변환 → StretchDIBits(top-down)`.
//!
//! 실행: `cargo run -p nexa-app --example preview_image -- <이미지 경로>`
//! (예: `cargo run -p nexa-app --example preview_image -- C:\Windows\Web\Wallpaper\Windows\img0.jpg`)
//!
//! 참고: 본편 미리보기는 **플러그인이 아니라 내장**(DR-7 — 원본의 .NET Nexa.Plugins SDK 비이관).
//! 본편 구현 위치: `src/dw.rs::image_scaled`(디코드·캐시) + `DwCtx::draw_image`(표시).

#[cfg(windows)]
fn main() {
    if let Err(e) = win::run() {
        eprintln!("실패: {e}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    println!("Windows 전용 예제입니다.");
}

#[cfg(windows)]
mod win {
    use windows::core::{w, Result, PCWSTR};
    use windows::Win32::Foundation::{GENERIC_READ, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        BeginPaint, EndPaint, FillRect, GetSysColorBrush, StretchDIBits, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, COLOR_WINDOW, DIB_RGB_COLORS, PAINTSTRUCT, SRCCOPY,
    };
    use windows::Win32::Graphics::Imaging::{
        CLSID_WICImagingFactory, GUID_WICPixelFormat32bppBGRA, IWICImagingFactory,
        WICBitmapDitherTypeNone, WICBitmapInterpolationModeFant, WICBitmapPaletteTypeCustom,
        WICDecodeMetadataCacheOnDemand,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetMessageW,
        LoadCursorW, PostQuitMessage, RegisterClassW, TranslateMessage, CW_USEDEFAULT, IDC_ARROW,
        MSG, WM_DESTROY, WM_PAINT, WM_SIZE, WNDCLASSW, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
    };

    /// 디코드 결과(전역 1장 — 예제 단순화). (w, h, BGRA top-down)
    static mut IMAGE: Option<(i32, i32, Vec<u8>)> = None;
    static mut PATH: String = String::new();

    /// 본편 `DwBackend::image_scaled`와 동일한 파이프라인(캐시 없이 매 리사이즈 재디코드).
    unsafe fn decode(path: &str, max_w: i32, max_h: i32) -> Option<(i32, i32, Vec<u8>)> {
        let wic: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
        let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        let dec = wic
            .CreateDecoderFromFilename(
                PCWSTR(wide.as_ptr()),
                None,
                GENERIC_READ,
                WICDecodeMetadataCacheOnDemand,
            )
            .ok()?;
        let frame = dec.GetFrame(0).ok()?;
        let (mut iw, mut ih) = (0u32, 0u32);
        frame.GetSize(&mut iw, &mut ih).ok()?;
        if iw == 0 || ih == 0 || max_w <= 0 || max_h <= 0 {
            return None;
        }
        let scale = (max_w as f32 / iw as f32)
            .min(max_h as f32 / ih as f32)
            .min(1.0); // 확대 없음(본편 동일)
        let tw = ((iw as f32 * scale) as u32).max(1);
        let th = ((ih as f32 * scale) as u32).max(1);
        let scaler = wic.CreateBitmapScaler().ok()?;
        scaler
            .Initialize(&frame, tw, th, WICBitmapInterpolationModeFant)
            .ok()?;
        let conv = wic.CreateFormatConverter().ok()?;
        conv.Initialize(
            &scaler,
            &GUID_WICPixelFormat32bppBGRA,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeCustom,
        )
        .ok()?;
        let stride = tw * 4;
        let mut buf = vec![0u8; (stride * th) as usize];
        conv.CopyPixels(std::ptr::null(), stride, &mut buf).ok()?;
        Some((tw as i32, th as i32, buf))
    }

    unsafe extern "system" fn wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_SIZE => {
                // 창 크기에 맞춰 재디코드(본편은 도크 크기 기준 캐시)
                let mut rc = windows::Win32::Foundation::RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                #[allow(static_mut_refs)]
                {
                    IMAGE = decode(&PATH, rc.right - rc.left, rc.bottom - rc.top);
                }
                let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(hwnd), None, true);
                LRESULT(0)
            }
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                let mut rc = windows::Win32::Foundation::RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                FillRect(hdc, &rc, GetSysColorBrush(COLOR_WINDOW));
                #[allow(static_mut_refs)]
                if let Some((w, h, bits)) = IMAGE.as_ref() {
                    let bmi = BITMAPINFO {
                        bmiHeader: BITMAPINFOHEADER {
                            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                            biWidth: *w,
                            biHeight: -h, // top-down
                            biPlanes: 1,
                            biBitCount: 32,
                            biCompression: BI_RGB.0,
                            ..Default::default()
                        },
                        ..Default::default()
                    };
                    let dx = (rc.right - rc.left - w) / 2;
                    let dy = (rc.bottom - rc.top - h) / 2;
                    StretchDIBits(
                        hdc,
                        dx,
                        dy,
                        *w,
                        *h,
                        0,
                        0,
                        *w,
                        *h,
                        Some(bits.as_ptr() as *const core::ffi::c_void),
                        &bmi,
                        DIB_RGB_COLORS,
                        SRCCOPY,
                    );
                }
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    pub fn run() -> Result<()> {
        let path = std::env::args().nth(1).unwrap_or_default();
        if path.is_empty() {
            eprintln!("사용법: cargo run -p nexa-app --example preview_image -- <이미지 경로>");
            std::process::exit(2);
        }
        unsafe {
            #[allow(static_mut_refs)]
            {
                PATH = path;
            }
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let class = w!("NexaPreviewExample");
            let wc = WNDCLASSW {
                lpszClassName: class,
                lpfnWndProc: Some(wndproc),
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                ..Default::default()
            };
            RegisterClassW(&wc);
            CreateWindowExW(
                Default::default(),
                class,
                w!("이미지 미리보기 예제 — WIC 파이프라인(본편 M4-2와 동일)"),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                900,
                650,
                None,
                None,
                None,
                None,
            )?;
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        Ok(())
    }
}
