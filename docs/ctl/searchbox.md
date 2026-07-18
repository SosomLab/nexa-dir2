# NxSearchBox — 검색 입력 (1호)

> [ctl 홈](README.md) · 클래스 `Nexa.NxSearchBox` ·
> 소스 [`searchbox.rs`](../../crates/nexa-app/src/ctl/searchbox.rs)
> ctl 1호(07-16) — 설정 창 검색 필드. **내장 ✕ 지우기** 버튼.

## 모양·동작
- 내부 무테두리 EDIT(세로 중앙) + 1px 테두리. **입력이 있을 때만 ✕ 표시**
  (사용자 확정) — 클릭 = 비우기.
- z-순서 진범 구조 해소가 탄생 배경(✕를 별도 컨트롤로 얹던 구조 폐기 →
  자기완결 컨트롤).

## 텍스트 API(드롭인 계약)
`WM_SETTEXT` / `WM_GETTEXT` / `WM_GETTEXTLENGTH` / `EM_SETCUEBANNER` → 내부 EDIT.
내용 변경 = `WM_COMMAND(id, EN_CHANGE)` 재발행.

## API
| 항목 | 의미 |
|---|---|
| `create(parent, x, y, w, h, id, font)` | 생성(현재 라이트 팔레트 고정) |

## 상태(구형 — 개정 대기)
- Style 인자화·AA 개정 미적용(라이트 고정 — 동일 팔레트라 시각 일치).
  개정은 **사용자 개별 요청 시에만**(07-18 프로토콜).
