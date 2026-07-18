# NxComboBox — 팝업 선택 버튼 (7호)

> [ctl 홈](README.md) · 클래스 `Nexa.NxComboBox` / 팝업 `...Pop` ·
> 소스 [`combobox.rs`](../../crates/nexa-app/src/ctl/combobox.rs)
> macOS 팝업 버튼 스타일(사용자 시안 07-17). NxDropList 은퇴 후 콤보 통일.

## 모양·동작
- 본체 = **라운드 필(sel_bg)** + 현재 항목 라벨 + 우측 이중 셰브론(⌃⌄ AA 펜) +
  1px 외곽선(동색 배경 구별 — QA 07-17). 텍스트 +1px 하향.
- 클릭/↓ = **✓ 팝업**: 현재 선택 = ✓ 표기·hover = accent·클릭/Enter 확정·
  Esc/바깥 클릭 닫기(NOACTIVATE + 60ms 타이머).
- 높이: `h <= 0` = 공통 자동(글꼴+4px), 크게 주면 상/하 균등 여백 중앙.
- IsDialogMessage 아래에서도 ↑↓ 유지(`WM_GETDLGCODE = DLGC_WANTARROWS`).

## API·메시지 계약
| 항목 | 값 | 의미 |
|---|---|---|
| `create(parent, x, y, w, h, id, font, items, selected, style)` | — | 항목 라벨 복사 소유 |
| 통지 `NXCB_CHANGED` | 1 | 선택 확정 시 부모 WM_COMMAND |
| `NXCB_GETSEL` | WM_USER+90 | 현재 선택 조회 |
| `NXCB_SETSEL` | WM_USER+91 | 선택 설정(통지 없음·범위 밖 무시) |

## 주의(원장)
- **owner 승격 함정**: WS_POPUP 팝업의 owner는 시스템이 최상위로 교체 —
  GetParent 금지, **팝업 USERDATA에 owner 저장**(droplist 크래시 교훈).
- 항목 목록은 불변 — 갱신은 재생성(프리셋 메뉴 규약과 동일).
