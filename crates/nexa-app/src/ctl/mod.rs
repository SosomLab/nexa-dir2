//! ctl — 앱 내 **Win32 커스텀 컨트롤 라이브러리**(사용자 요청 07-16).
//! 네이티브 대화상자(설정·일괄 이름변경 등)에서 재사용하는 자기완결 컨트롤 묶음.
//! comctl32 비의존 규율(B3)은 그대로 — user32 창 클래스 + 자체 그리기.
//!
//! **명명 규약**(사용자 확정 07-17): 컨트롤 이름 = **Nx 접두어**(Nexa) ·
//! 네임스페이스 = **Nexa** — Win32 클래스명 `Nexa.Nx<이름>`(팝업 = `...Pop`).
//!
//! 수록: NxSearchBox([`searchbox`] — 내장 ✕ 검색 입력) · NxFontBox([`fontbox`] —
//! 글꼴 피커) · NxSegmented([`segmented`]) · NxSpin([`spin`]) ·
//! NxDropList([`droplist`]) · NxGroupCard([`groupcard`] — 타이틀+본문 카드) ·
//! NxComboBox([`combobox`] — macOS 팝업 버튼 스타일 ✓ 선택) ·
//! NxCheckBox([`checkbox`] — 라운드 박스 토글) · NxTextBox([`textbox`] —
//! 라운드 입력·포커스 accent 링) · NxIconButton([`iconbutton`] — **shape 투명**
//! 원형 버튼: 리전 클립이라 어떤 부모 배경 위에서도 도형만 보임) ·
//! 공용 [`style`](팔레트·**공통 자동 높이** auto_height — 모든 컨트롤
//! `h<=0` 동일 기본 높이 = 반듯한 기본 배치).
//! 후속 후보: 런처 편집기 목록(X-13)·모달 공통 골격(X-16 백로그 ①).
//! UI 검증 = 갤러리 창(crate::ctldemo — WM_APP_CTLDEMO 0x8009 주입 전용).

//! **판매용 추상화 규약**(사용자 확정 07-17): 앱 비결합(색 = [`style::Style`] 인자·
//! 라벨 = 복사 소유·i18n/테마 미참조) · 텍스트 API 위임(WM_SETTEXT/GETTEXT) ·
//! 통지 = 컨트롤 id로 WM_COMMAND 재발행 · 상태 = GWLP_USERDATA Box(파괴 시 회수).
//! searchbox/fontbox의 Style 인자화는 후속(현재 라이트 고정 — 동일 팔레트).

pub mod checkbox;
pub mod combobox;
pub mod droplist;
pub mod fontbox;
pub mod groupcard;
pub mod iconbutton;
pub mod searchbox;
pub mod segmented;
pub mod spin;
pub mod style;
pub mod textbox;
