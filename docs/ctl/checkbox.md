# NxCheckBox — 라운드 토글 (8호)

> [ctl 홈](README.md) · 클래스 `Nexa.NxCheckBox` ·
> 소스 [`checkbox.rs`](../../crates/nexa-app/src/ctl/checkbox.rs)
> macOS 시안 07-17: 미체크 = 연회색 라운드 박스·체크 = accent + 흰 ✓.

## 모양·동작
- 박스 = **글꼴 높이 정사각**을 세로 중앙에(컨트롤 높이와 분리 — 클릭 영역은
  전체, 시각만 시안 비율). 미체크 = sel_bg + 1px 외곽선·체크 = accent + 흰 ✓(AA).
- **체크 단수(사용자 확정 07-18)** — `CheckMode`:
  - `Two` = 체크/해제(0↔1)
  - `Three` = 체크/부분/해제(0→1→2→0 순환). **부분(2) = accent 박스 +
    흐릿한 ✓**(bg·accent 50% 블렌드)
- 클릭/Space = 토글. 라벨(빈 문자열 = 박스만)은 우측 세로 중앙.
- `h <= 0` = 공통 자동 높이. 배경 = `style.bg`(호스트 배경과 일치시킬 것).

## API·메시지 계약
| 항목 | 값 | 의미 |
|---|---|---|
| `create(parent, x, y, w, h, id, font, label, check, mode, style)` | — | `check` = 0/1/2 |
| 통지 `NXCHK_CHANGED` | 1 | 토글 시 부모 WM_COMMAND |
| `NXCHK_GETCHECK` | WM_USER+95 | 상태 조회(0 해제·1 체크·2 부분) |
| `NXCHK_SETCHECK` | WM_USER+96 | 상태 설정(통지 없음·모드별 클램프) |

## 연관
- NxGrid `Mark::Check` 헤더 체크박스가 같은 3단 시각 규약을 공유(전체/부분/해제).

## 개발자 레퍼런스

### 함수
| 함수 | 설명 |
|---|---|
| `create(parent, x, y, w, h, id, font, label, check, mode, style) -> HWND` | 체크박스 생성. `w <= 0` = 박스만(정사각) |

| 인자 | 타입 | 설명 |
|---|---|---|
| `label` | `&str` | 우측 라벨(빈 문자열 = 박스만) |
| `check` | `u32` | 초기 상태 0/1/2(2는 `Three` 모드만 — 모드별 클램프) |
| `mode` | `CheckMode` | 체크 단수(아래) |

### 프로퍼티 — `CheckMode`
| 값 | 클릭 순환 | 설명 |
|---|---|---|
| `Two` | 0↔1 | 체크/해제 |
| `Three` | 0→1→2→0 | 체크/부분(흐릿한 ✓)/해제 — 전체 선택 헤더류 |

### 사용 예
```rust
let chk = checkbox::create(dlg, x, y, 0, 0, ID_MC, font, "", 0,
                           checkbox::CheckMode::Two, style);
// (ID_MC, checkbox::NXCHK_CHANGED) =>
let v = SendMessageW(chk, checkbox::NXCHK_GETCHECK, None, None).0; // 0/1/2
```
