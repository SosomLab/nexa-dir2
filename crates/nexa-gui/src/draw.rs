//! 드로잉 추상 — 위젯이 그리는 최소 어휘. 구체 백엔드(GDI·DirectWrite interop)는
//! nexa-app이 구현하며 ADR-0002(M1-2)에서 확정한다. 어휘는 두 백엔드 공통 모델
//! (불투명 rect 채우기 + 불투명 배경 텍스트 1회 호출)로 유지한다.

use crate::geom::Rect;
use crate::theme::Color;

/// 폰트 슬롯(X-12 — 사용자 요청 07-16): 위젯이 페인트 시작에 자신의 슬롯을 선택한다.
/// 콘솔(term_text)·대화상자(네이티브 GDI)는 별도 경로.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FontSlot {
    /// 특정 슬롯이 없는 전부 — 메뉴·탭·경로바·도구/런처 바·도크.
    #[default]
    Base,
    /// 파일 목록 + 컬럼 헤더(굵게/이탤릭 장식은 select_font 인자).
    List,
    /// 하단 상태바.
    Status,
}

pub trait DrawCtx {
    /// 폰트 슬롯/장식 선택(X-12) — 이후의 text/text_opaque/text_width에 적용.
    /// 위젯은 **페인트 시작에 자신의 슬롯을 선택**한다(상태 공유 — 순서 무관 보장).
    /// 기본 = no-op(테스트 백엔드 등 단일 폰트).
    fn select_font(&mut self, slot: FontSlot, bold: bool, italic: bool) {
        let _ = (slot, bold, italic);
    }

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

    /// 터미널 셀 폭(px) — 모노스페이스 "0" 기준(M4-3). 기본 = text_width("0").
    fn term_cell_w(&mut self) -> i32 {
        self.text_width("0")
    }

    /// 터미널 텍스트(M4-3) — **모노스페이스** 폰트로 `clip`을 `bg` 채우고 그린다
    /// (셀 그리드 정렬 보장). 기본 = text_opaque 폴백(비모노 — 백엔드가 구현).
    fn term_text(&mut self, x: i32, y: i32, clip: Rect, text: &str, fg: Color, bg: Color) {
        self.text_opaque(x, y, clip, text, fg, bg);
    }

    /// 이미지 그리기(M4-2 미리보기) — `hint`(파일 경로)의 이미지를 `rect` 안에 **비율 유지**로
    /// 가운데 표시. 디코드·스케일·캐시는 백엔드(WIC) 소관. 실패 시 아무것도 그리지 않는다
    /// (호출자가 배경을 먼저 칠한다). 기본 = no-op.
    fn draw_image(&mut self, rect: Rect, hint: &str) {
        let _ = (rect, hint);
    }

    /// 아이콘 그리기 — `key`는 백엔드가 해석하는 불투명 식별자, `hint`는 로드 힌트(경로 등).
    /// 미로드 시 아무것도 그리지 않아도 된다(백엔드가 큐잉 후 재그리기). 기본 = no-op.
    /// 반환 = 실제로 그렸는가(M5-1 런처 — 미로드 동안 호출자가 폴백을 그릴 수 있게).
    fn draw_icon(&mut self, x: i32, y: i32, size: i32, key: &str, hint: &str) -> bool {
        let _ = (x, y, size, key, hint);
        false
    }

    /// 큰 글리프(버튼 화살표 등) — `clip` 안에 **가운데 정렬**로 본문보다 큰 크기로 그린다.
    /// 백엔드가 지원하지 않으면 text_opaque로 폴백(기본).
    fn glyph_opaque(&mut self, clip: Rect, text: &str, fg: Color, bg: Color) {
        let ty = clip.y + (clip.h - (clip.h * 4) / 5) / 2;
        let tx = clip.x + (clip.w - self.text_width(text)).max(0) / 2;
        self.text_opaque(tx, ty, clip, text, fg, bg);
    }

    // ── AA 도형 프리미티브(07-17 — ctl raster QA: 곡선·사선은 AA 백엔드가 그린다.
    //    규약: GDI+ 등 래스터라이저 호출은 **DrawCtx 구현체에만** 존재 — 위젯/컨트롤은
    //    이 인터페이스만 사용. 기본 = no-op(텍스트·테스트 백엔드 비구현 허용). ──

    /// 원/타원을 `rect`에 안티앨리어스로 채운다.
    fn fill_ellipse(&mut self, rect: Rect, color: Color) {
        let _ = (rect, color);
    }

    /// 라운드 사각형을 반경 `radius`(px)로 안티앨리어스 채움.
    fn fill_round_rect(&mut self, rect: Rect, radius: i32, color: Color) {
        let _ = (rect, radius, color);
    }

    /// 라운드 사각형 외곽선(폭 `width`px — 1.0/2.0 등) 안티앨리어스 스트로크.
    fn stroke_round_rect(&mut self, rect: Rect, radius: i32, color: Color, width: f32) {
        let _ = (rect, radius, color, width);
    }

    /// 꺾은선(✓·셰브론·+/− 글리프 등) — 둥근 캡/조인, 폭 `width`px AA 스트로크.
    fn polyline(&mut self, pts: &[(i32, i32)], color: Color, width: f32) {
        let _ = (pts, color, width);
    }
}
