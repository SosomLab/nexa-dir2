//! 앱 아이콘(QA 07-14 — 원본 `Assets/AppIcon/nexa-dir.ico` 이식): 단일 exe에
//! `include_bytes!`로 임베드하고 런타임에 ICONDIR을 파싱해 HICON 생성 —
//! 메인 창·대화상자·설정/진행 창 클래스에 공통 적용.
//! exe 파일 자체의 탐색기 아이콘(리소스 섹션)은 빌드 도구 의존이라 후속(DR-8 원장 검토).

use windows::Win32::UI::WindowsAndMessaging::{CreateIconFromResourceEx, HICON, LR_DEFAULTCOLOR};

/// 원본 앱 아이콘(다크판 기본 — 풀블리드 폴더 + `>_`).
static ICO: &[u8] = include_bytes!("../assets/nexa-dir.ico");

/// `.ico`(ICONDIR)에서 요청 크기에 가장 근접한 이미지를 골라 HICON 생성.
/// 실패 시 `None` — 호출측은 아이콘 없이 진행(치명 아님).
pub unsafe fn load(size: i32) -> Option<HICON> {
    // ICONDIR: u16 reserved·u16 type(1)·u16 count / entry(16B): bW bH bColors bRes
    // wPlanes wBitCount dwBytesInRes dwImageOffset
    if ICO.len() < 6 || u16::from_le_bytes([ICO[2], ICO[3]]) != 1 {
        return None;
    }
    let count = u16::from_le_bytes([ICO[4], ICO[5]]) as usize;
    let mut best: Option<(i32, usize, usize)> = None; // (크기 차, offset, len)
    for i in 0..count {
        let e = 6 + i * 16;
        if e + 16 > ICO.len() {
            break;
        }
        let w = if ICO[e] == 0 { 256 } else { ICO[e] as i32 };
        let len = u32::from_le_bytes([ICO[e + 8], ICO[e + 9], ICO[e + 10], ICO[e + 11]]) as usize;
        let off = u32::from_le_bytes([ICO[e + 12], ICO[e + 13], ICO[e + 14], ICO[e + 15]]) as usize;
        if off + len > ICO.len() {
            continue;
        }
        let diff = (w - size).abs();
        if best.is_none_or(|(d, _, _)| diff < d) {
            best = Some((diff, off, len));
        }
    }
    let (_, off, len) = best?;
    CreateIconFromResourceEx(
        &ICO[off..off + len],
        true,
        0x0003_0000,
        size,
        size,
        LR_DEFAULTCOLOR,
    )
    .ok()
}
