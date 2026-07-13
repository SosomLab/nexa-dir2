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

    /// **배경을 칠하지 않고** 텍스트만 그린다 — 선택 하이라이트 위 겹쳐 그리기(편집 필드).
    /// 런 분할 없이 1회 호출로 그려야 경계 잘림/이음새가 없다(QA 07-13). 오른쪽 초과분은
    /// 백엔드가 잘라낸다. 기본 = no-op(텍스트 백엔드가 반드시 구현).
    fn text(&mut self, x: i32, y: i32, clip: Rect, text: &str, fg: Color) {
        let _ = (x, y, clip, text, fg);
    }

    /// 이미지 그리기(M4-2 미리보기) — `hint`(파일 경로)의 이미지를 `rect` 안에 **비율 유지**로
    /// 가운데 표시. 디코드·스케일·캐시는 백엔드(WIC) 소관. 실패 시 아무것도 그리지 않는다
    /// (호출자가 배경을 먼저 칠한다). 기본 = no-op.
    fn draw_image(&mut self, rect: Rect, hint: &str) {
        let _ = (rect, hint);
    }

    /// 아이콘 그리기 — `key`는 백엔드가 해석하는 불투명 식별자, `hint`는 로드 힌트(경로 등).
    /// 미로드 시 아무것도 그리지 않아도 된다(백엔드가 큐잉 후 재그리기). 기본 = no-op.
    fn draw_icon(&mut self, x: i32, y: i32, size: i32, key: &str, hint: &str) {
        let _ = (x, y, size, key, hint);
    }

    /// 큰 글리프(버튼 화살표 등) — `clip` 안에 **가운데 정렬**로 본문보다 큰 크기로 그린다.
    /// 백엔드가 지원하지 않으면 text_opaque로 폴백(기본).
    fn glyph_opaque(&mut self, clip: Rect, text: &str, fg: Color, bg: Color) {
        let ty = clip.y + (clip.h - (clip.h * 4) / 5) / 2;
        let tx = clip.x + (clip.w - self.text_width(text)).max(0) / 2;
        self.text_opaque(tx, ty, clip, text, fg, bg);
    }
}
