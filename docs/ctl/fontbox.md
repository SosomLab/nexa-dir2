# NxFontBox — 글꼴 피커 (2호)

> [ctl 홈](README.md) · 클래스 `Nexa.NxFontBox` / 팝업 `...Pop` ·
> 소스 [`fontbox.rs`](../../crates/nexa-app/src/ctl/fontbox.rs)
> ctl 2호(07-16) — 설정 창 글꼴 입력(WT 글꼴 피커 참조).

## 구성·동작
- 자식 EDIT(입력) + **드롭다운 목록**(설치 글꼴 — **각 항목을 그 글꼴로 렌더**,
  항목별 미리보기 HFONT 지연 생성·파괴 시 해제).
- 스크롤·hover·키보드 ↑/↓/PgUp/PgDn/Enter/Esc. 입력 중 접두 매칭 위치로 자동 이동.
- 설치 글꼴 열거 = 프로세스 1회 캐시(EnumFontFamiliesExW — '@' 세로쓰기 제외).

## 선택 반영 규칙(사용자 확정 — 쉼표 = 폴백 체인)
- 입력에 `,`가 있으면 마지막 조각을 선택 글꼴로 **교체**(= 구분자 뒤 추가).
- 없으면 **전체 교체**(A 상태에서 B 선택 = B).

## 호스트 계약
- 텍스트 위임: `WM_SETTEXT`/`WM_GETTEXT`/`WM_GETTEXTLENGTH`.
- 내용 변경 = `EN_CHANGE`·**확정**(선택/포커스 이탈) = `EN_KILLFOCUS` 재발행 —
  설정 창의 즉시 적용 배선을 그대로 탄다.
- `FBM_HAS_DROP`(WM_USER+40): 드롭다운 열림 질의 — 호스트 모달 펌프가 Enter를
  컨트롤에 넘길지(열림 = 목록 확정) 판단.

## 상태(구형 — 개정 대기)
- Style 인자화·AA 개정 미적용(라이트 고정). 개정은 사용자 개별 요청 시에만.
- 팝업 owner = USERDATA 규약(승격 함정)·타이머 해제는 owner 기준(QA 07-17).
