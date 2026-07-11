//! GDI 백엔드 — `nexa_gui::DrawCtx`의 1차 구현(M1-1).
//! DirectWrite GDI interop과의 비교·확정은 ADR-0002(M1-2). 확정 후 nexa-gui 이동 여부 재결정.

use nexa_gui::{Color, DrawCtx, Rect};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::Gdi::{ExtTextOutW, SetBkColor, SetTextColor, ETO_OPAQUE, HDC};

pub fn colorref(c: Color) -> COLORREF {
    // COLORREF = 0x00BBGGRR
    COLORREF(((c.b as u32) << 16) | ((c.g as u32) << 8) | c.r as u32)
}

fn to_rect(r: Rect) -> RECT {
    RECT {
        left: r.x,
        top: r.y,
        right: r.right(),
        bottom: r.bottom(),
    }
}

/// 프레임 단위 드로잉 컨텍스트 — 메모리 DC(더블 버퍼) 위에 그린다.
/// 폰트 선택 등 DC 준비는 호출자(win::paint) 책임.
pub struct GdiCtx {
    pub dc: HDC,
}

impl DrawCtx for GdiCtx {
    fn fill_rect(&mut self, rect: Rect, color: Color) {
        unsafe {
            // 브러시 생성 없는 불투명 채우기 — ETO_OPAQUE + 빈 텍스트(M0-7 모델의 일반화)
            SetBkColor(self.dc, colorref(color));
            let rc = to_rect(rect);
            let _ = ExtTextOutW(
                self.dc,
                rect.x,
                rect.y,
                ETO_OPAQUE,
                Some(&rc),
                w!(""),
                0,
                None,
            );
        }
    }

    fn text_opaque(&mut self, x: i32, y: i32, clip: Rect, text: &str, fg: Color, bg: Color) {
        unsafe {
            SetBkColor(self.dc, colorref(bg));
            SetTextColor(self.dc, colorref(fg));
            let wtext: Vec<u16> = text.encode_utf16().collect();
            let rc = to_rect(clip);
            let _ = ExtTextOutW(
                self.dc,
                x,
                y,
                ETO_OPAQUE,
                Some(&rc),
                PCWSTR(wtext.as_ptr()),
                wtext.len() as u32,
                None,
            );
        }
    }
}
