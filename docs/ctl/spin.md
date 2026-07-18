# NxSpin — 숫자 스피너 (4호)

> [ctl 홈](README.md) · 클래스 `Nexa.NxSpin` ·
> 소스 [`spin.rs`](../../crates/nexa-app/src/ctl/spin.rs)
> PF Position/Start/Step 스테퍼 대응(사용자 시안 재개정 07-17).

## 배치(사용자 확정)
- **독립 라운드 글상자**(NxTextBox 모양 — 숫자 **우측 정렬**) + 우측 **분리된**
  ⌃⌄ 버튼 블록(간격 4px).
- 블록 폭 = 높이의 2/3·높이 = 글상자 종속·개별 버튼 = 상/하 1/2.
- **min/max 도달 방향 = 비활성**(연한 셰브론 + 클릭 무시 — 타이핑 도달도 재도장).
- `h <= 0` = 공통 자동 높이. 도형 = AA·모서리 = behind.

## 동작
- 값 변경 경로: 타이핑·⌃⌄ 클릭·↑/↓ 키(내부 EDIT 서브클래스).
- 포커스 이탈 = 범위 클램프 확정 + `EN_KILLFOCUS` 재발행.
- **Tab 내비(07-18)**: 래퍼 = CONTROLPARENT·탭스톱 = 내부 EDIT.

## API·메시지 계약
| 항목 | 값 | 의미 |
|---|---|---|
| `create(parent, x, y, w, h, id, font, value, min, max, style)` | — | — |
| `SPIN_GETVAL` / `SPIN_SETVAL` | WM_USER+60/61 | 값 조회/설정(클램프·통지 없음) |
| 재발행 `EN_CHANGE` | 0x300 | 값 변경 → 부모 WM_COMMAND |
| 위임 `WM_GETTEXT`/`WM_SETTEXT` | — | 드롭인 텍스트 계약 |

## 내부 구현 메모
- 내부 EDIT 서브클래스(`GWLP_WNDPROC` 교체)는 WM_DESTROY에서 **원복 후**
  상태 박스 회수(base::drop_state).

## 개발자 레퍼런스

### 함수
| 함수 | 설명 |
|---|---|
| `create(parent, x, y, w, h, id, font, value, min, max, style) -> HWND` | 스피너 생성(`h <= 0` = 공통 자동) |

| 인자 | 타입 | 설명 |
|---|---|---|
| `value` | `i64` | 초기값(범위 클램프) |
| `min` / `max` | `i64` | 값 범위 — 도달 방향 버튼 자동 비활성 |

### 사용 예
```rust
let sp = spin::create(dlg, x, y, 70, 0, ID_POS, font, 0, 0, 999, style);
let v = SendMessageW(sp, spin::SPIN_GETVAL, None, None).0 as i64;
SendMessageW(sp, spin::SPIN_SETVAL, Some(WPARAM(5 as usize)), None); // 통지 없음
// (ID_POS, 0x300 /*EN_CHANGE*/) => 값 변경 실시간
```
