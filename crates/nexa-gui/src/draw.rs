//! 드로잉 추상 — 위젯이 그리는 최소 어휘. 구체 백엔드(GDI·DirectWrite interop)는
//! nexa-app이 구현하며 ADR-0002(M1-2)에서 확정한다. 어휘는 두 백엔드 공통 모델
//! (불투명 rect 채우기 + 불투명 배경 텍스트 1회 호출)로 유지한다.

use crate::geom::Rect;
use crate::theme::Color;

pub trait DrawCtx {
    /// rect를 단색으로 불투명하게 채운다.
    fn fill_rect(&mut self, rect: Rect, color: Color);

    /// `clip`을 `bg`로 불투명하게 채우면서 텍스트를 `(x, y)`에 그린다
    /// — GDI `ExtTextOutW(ETO_OPAQUE)` 모델(행 배경+텍스트 = 호출 1회, M0-7 실증).
    /// 텍스트가 `clip` 오른쪽을 넘으면 백엔드가 잘라낸다(DW = 말줄임표 트리밍).
    fn text_opaque(&mut self, x: i32, y: i32, clip: Rect, text: &str, fg: Color, bg: Color);

    /// 텍스트의 렌더 폭(px) — 우측 정렬(크기 컬럼 등)에 사용.
    fn text_width(&mut self, text: &str) -> i32;

    /// 아이콘 그리기 — `key`는 백엔드가 해석하는 불투명 식별자, `hint`는 로드 힌트(경로 등).
    /// 미로드 시 아무것도 그리지 않아도 된다(백엔드가 큐잉 후 재그리기). 기본 = no-op.
    fn draw_icon(&mut self, x: i32, y: i32, size: i32, key: &str, hint: &str) {
        let _ = (x, y, size, key, hint);
    }
}
