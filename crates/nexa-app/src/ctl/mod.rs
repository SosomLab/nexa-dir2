//! ctl — 앱 내 **Win32 커스텀 컨트롤 라이브러리**(사용자 요청 07-16).
//! 네이티브 대화상자(설정·일괄 이름변경 등)에서 재사용하는 자기완결 컨트롤 묶음.
//! comctl32 비의존 규율(B3)은 그대로 — user32 창 클래스 + 자체 그리기.
//!
//! 수록: [`searchbox`](내장 ✕ 검색 입력) · [`fontbox`](글꼴 피커) ·
//! [`segmented`](세그먼트 라디오) · [`spin`](숫자 스피너) · [`droplist`](드롭다운
//! 선택) · [`groupcard`](타이틀+본문 그룹 카드 — 07-17) · 공용 [`style`].
//! 후속 후보: 런처 편집기 목록(X-13)·모달 공통 골격(X-16 백로그 ①).
//! UI 검증 = 갤러리 창(crate::ctldemo — WM_APP_CTLDEMO 0x8009 주입 전용).

//! **판매용 추상화 규약**(사용자 확정 07-17): 앱 비결합(색 = [`style::Style`] 인자·
//! 라벨 = 복사 소유·i18n/테마 미참조) · 텍스트 API 위임(WM_SETTEXT/GETTEXT) ·
//! 통지 = 컨트롤 id로 WM_COMMAND 재발행 · 상태 = GWLP_USERDATA Box(파괴 시 회수).
//! searchbox/fontbox의 Style 인자화는 후속(현재 라이트 고정 — 동일 팔레트).

pub mod droplist;
pub mod fontbox;
pub mod groupcard;
pub mod searchbox;
pub mod segmented;
pub mod spin;
pub mod style;
