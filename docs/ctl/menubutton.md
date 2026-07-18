# NxMenuButton — 오버플로 메뉴 버튼 (13호)

> [ctl 홈](README.md) · 클래스 `Nexa.NxMenuButton` / 팝업 `...Pop` ·
> 소스 [`menubutton.rs`](../../crates/nexa-app/src/ctl/menubutton.rs)
> `… ⌄` 좁은 자리 액션 드롭다운(관용 명칭 = 메뉴/드롭다운 버튼 — WinUI
> DropDownButton 대응). 프리셋 메뉴([프리셋들/─/Save/Edit])가 소비.

## 모양·동작
- 본체 = `… ⌄` 글리프 버튼. 클릭/↓ = 팝업 메뉴(폭 = 라벨 실측).
- 팝업 규약 = 콤보 동일(NOACTIVATE·바깥 클릭 60ms 타이머 닫기·owner USERDATA).
- ✓ 표기 없음(선택 상태가 아니라 **액션 실행** 메뉴).
- **구분선**: 항목 `"-"` = 수평선(pick 무시·hover/키보드 탐색 스킵 — 07-18).
- 항목 목록은 불변 — 갱신 = 재생성(프리셋 저장/삭제 후 rebuild).

## API·메시지 계약
| 항목 | 값 | 의미 |
|---|---|---|
| `create(parent, x, y, w, h, id, font, items, style)` | — | 라벨 복사 소유 |
| 통지 `NXMB_PICK` | 1 | 항목 실행(인덱스 = `NXMB_GETPICK`) |
| `NXMB_GETPICK` | WM_USER+110 | 마지막 pick 인덱스(-1 = 없음) |

## 주의(원장)
- owner 승격 함정 = 콤보와 동일(팝업 USERDATA에 owner).
- 모달 루프에서 팝업 항목을 프로그램으로 누를 땐 **PostMessage**(SendMessage는
  모달 진입에 블록 — QA 방법론).
