# NxButton — 푸시 버튼 (11호)

> [ctl 홈](README.md) · 클래스 `Nexa.NxButton` ·
> 소스 [`button.rs`](../../crates/nexa-app/src/ctl/button.rs)
> macOS 시안 07-17: Cancel = 연회색 라운드·Rename = Default 파랑.

## 상태 3종(사용자 확정)
| 상태 | 필 | 글자 |
|---|---|---|
| 기본(`ButtonKind::Normal`) | sel_bg 라운드 | text |
| **Default**(`ButtonKind::Default`) | **accent** | bg(흰) — 대화상자 기본 동작 |
| Disabled | sel_bg | text_dim·클릭 무시 |

## 크기·동작
- `h <= 0` = **컴팩트**(글꼴 + 상/하 2px — 공통 auto보다 낮음, 사용자 확정).
- `w <= 0` = 라벨 실측 폭 + 좌우 16px(자동).
- 클릭/Space/Enter(enabled일 때만) = 통지. 라벨 = 윈도우 텍스트 위임
  (`WM_SETTEXT` 재도장).
- 우하단 정렬 배치는 호스트 헬퍼(bulkrename `place_buttons_br` — 실측 후 재배치) 참조.

## API·메시지 계약
| 항목 | 값 | 의미 |
|---|---|---|
| `create(parent, x, y, w, h, id, font, label, kind, enabled, style)` | — | — |
| 통지 `NXBTN_CLICK` | 1 | 클릭 |
| `NXBTN_GETENABLE` / `NXBTN_SETENABLE` | WM_USER+105/106 | 활성 조회/설정 |
| `NXBTN_SETDEFAULT` | WM_USER+107 | Default(accent) ↔ 기본 전환 |

## 주의
- 활성 제어는 `NXBTN_SETENABLE`(내부 상태 — 그리기 반영). Win32 `EnableWindow`는
  시각에 반영되지 않는다(bulkrename Rename 버튼 교훈 07-18).

## 개발자 레퍼런스

### 함수
| 함수 | 설명 |
|---|---|
| `create(parent, x, y, w, h, id, font, label, kind, enabled, style) -> HWND` | 버튼 생성. `w <= 0` = 라벨 실측 폭+32px·`h <= 0` = 컴팩트(글꼴+4px) |

| 인자 | 타입 | 설명 |
|---|---|---|
| `label` | `&str` | 버튼 라벨(윈도우 텍스트로 복사 소유 — `WM_SETTEXT`로 변경 가능) |
| `kind` | `ButtonKind` | 시각 종류(아래) |
| `enabled` | `bool` | 초기 활성(비활성 = 흐린 글자·클릭 무시) |

### 프로퍼티 — `ButtonKind`
| 값 | 설명 |
|---|---|
| `Normal` | 기본(sel_bg 라운드 필 + text 글자 — Cancel류) |
| `Default` | 대화상자 기본 동작(accent 필 + 흰 글자 — OK/Rename류) |

### 사용 예
```rust
let ok = button::create(dlg, 0, by, 0, 0, ID_OK, font, "확인",
                        button::ButtonKind::Default, true, style);
// 활성 제어는 반드시 메시지로(EnableWindow는 시각 미반영)
SendMessageW(ok, button::NXBTN_SETENABLE, Some(WPARAM(0)), None); // 비활성
// (ID_OK, button::NXBTN_CLICK) => 실행
```
