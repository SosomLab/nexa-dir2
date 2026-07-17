//! ctl — 앱 내 **Win32 커스텀 컨트롤 라이브러리**(사용자 요청 07-16).
//! 네이티브 대화상자(설정·일괄 이름변경 등)에서 재사용하는 자기완결 컨트롤 묶음.
//! comctl32 비의존 규율(B3)은 그대로 — user32 창 클래스 + 자체 그리기.
//!
//! **명명 규약**(사용자 확정 07-17): 컨트롤 이름 = **Nx 접두어**(Nexa) ·
//! 네임스페이스 = **Nexa** — Win32 클래스명 `Nexa.Nx<이름>`(팝업 = `...Pop`).
//!
//! 수록: NxSearchBox([`searchbox`] — 내장 ✕ 검색 입력) · NxFontBox([`fontbox`] —
//! 글꼴 피커) · NxSegmented([`segmented`]) · NxSpin([`spin`]) ·
//! NxGroupCard([`groupcard`] — 타이틀+본문 카드) ·
//! NxComboBox([`combobox`] — macOS 팝업 버튼 스타일 ✓ 선택) ·
//! NxCheckBox([`checkbox`] — 라운드 박스 토글) · NxTextBox([`textbox`] —
//! 라운드 입력·포커스 accent 링) · NxIconButton([`iconbutton`] — shape 투명
//! 원형 버튼) · NxButton([`button`] — 푸시 버튼: 기본/Default[accent]/
//! Disabled 3상태) · NxLabel([`label`] — 폼 라벨: 공통 높이·좌/우 정렬·클릭
//! 투과) · NxMenuButton([`menubutton`] — `… ⌄` 오버플로 메뉴 버튼: 좁은 자리
//! 액션 드롭다운·NXMB_PICK/GETPICK) ·
//! 공용 [`style`](팔레트·**공통 자동 높이** auto_height — 모든 컨트롤
//! `h<=0` 동일 기본 높이 = 반듯한 기본 배치).
//! 후속 후보: 런처 편집기 목록(X-13)·모달 공통 골격(X-16 백로그 ①).
//! UI 검증 = 갤러리 창(crate::ctldemo — WM_APP_CTLDEMO 0x8009 주입 전용).

//! **판매용 추상화 규약**(사용자 확정 07-17): 앱 비결합(색 = [`style::Style`] 인자·
//! 라벨 = 복사 소유·i18n/테마 미참조) · 텍스트 API 위임(WM_SETTEXT/GETTEXT) ·
//! 통지 = 컨트롤 id로 WM_COMMAND 재발행 · 상태 = GWLP_USERDATA Box(파괴 시 회수).
//! searchbox/fontbox의 Style 인자화는 후속(현재 라이트 고정 — 동일 팔레트).
//!
//! **AA 렌더 규약(개정 07-17 — 사용자 지시)**: 곡선·사선 도형은
//! `nexa_gui::DrawCtx`의 AA 프리미티브로만 그린다 — **GDI+ 직접 호출 금지**,
//! 유일한 접점 = [`gdipctx`](DrawCtx 백엔드 — 래스터라이저 교체 시 이 모듈만).
//! 텍스트는 GDI/DirectWrite 유지. 판매 단위 = ctl + DrawCtx 트레이트 + 백엔드
//! (기존 "nexa-gui 미참조" 문구는 **트레이트 참조 허용**으로 개정 — 사용자 승인).

pub mod button;
pub mod checkbox;
pub mod combobox;
pub mod fontbox;
pub mod gdipctx;
pub mod groupcard;
pub mod iconbutton;
pub mod label;
pub mod menubutton;
pub mod searchbox;
pub mod segmented;
pub mod spin;
pub mod style;
pub mod textbox;
