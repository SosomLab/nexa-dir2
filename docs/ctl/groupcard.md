# NxGroupCard — 카드 컨테이너 (6호)

> [ctl 홈](README.md) · 클래스 `Nexa.NxGroupCard` ·
> 소스 [`groupcard.rs`](../../crates/nexa-app/src/ctl/groupcard.rs)
> PF 카드 UI 대응 — 일괄 이름변경 **카드 스택 = 파이프라인**(X-23)의 토대.

## 모양·구조
- **타이틀 밴드(sel_bg) + 본문** 2영역. `GroupCardOpts`:
  | 필드 | 효과 |
  |---|---|
  | `corner` | 라운드 반경(0 = 각진 — 창 리전 클립) |
  | `title_h` / `body_h` | 영역별 높이 각각 지정 |
- 타이틀 = 윈도우 텍스트 위임. **타이틀 밴드에 자식 배치 가능**(`title_rect` —
  동작 선택 콤보 + ⊕⊖ 버튼이 앉는 PF 구도).
- 외곽선 = corner > 0 이면 RoundRect 펜·각진은 frame.

## 통지 투과(중첩 투명성)
자식(콤보·체크·텍스트박스…)의 `WM_COMMAND`/`WM_CTLCOLOR*` 등을 **카드 부모로
그대로 투과** — 호스트 다이얼로그는 카드 존재를 모른 채 지역 id로 수신한다.
(호스트의 컨트롤 조회는 카드 내부까지 탐색 필요 — bulkrename `ctl()` 참조.)

## Tab 내비(07-18)
`WS_EX_CONTROLPARENT` — IsDialogMessage가 카드 안 자식으로 Tab 진입.

## API·메시지 계약
| 항목 | 값 | 의미 |
|---|---|---|
| `create(parent, x, y, w, id, font, title, opts, style)` | — | 높이 = title_h+body_h |
| `title_rect(hwnd)` / `body_rect(hwnd)` | — | 자식 배치용 영역 rect |
| `GC_GETTITLEH` | WM_USER+80 | 타이틀 높이 조회 |

## 개발자 레퍼런스

### 함수
| 함수 | 설명 |
|---|---|
| `create(parent, x, y, w, id, font, title, opts, style) -> HWND` | 카드 생성 — **높이는 `opts.title_h + opts.body_h`로 결정**(인자에 h 없음) |
| `title_rect(hwnd) -> RECT` | 타이틀 밴드 내부 rect(자식 배치용 — 콤보·± 버튼) |
| `body_rect(hwnd) -> RECT` | 본문 내부 rect(폼 배치용) |

| 인자 | 타입 | 설명 |
|---|---|---|
| `title` | `&str` | 타이틀 텍스트(윈도우 텍스트 위임 — 타이틀 자식이 가리면 무시 가능) |
| `opts` | `GroupCardOpts` | 모양(아래) |

### 프로퍼티 — `GroupCardOpts`
| 필드 | 타입 | 설명 |
|---|---|---|
| `corner` | `i32` | 0 = 각진·>0 = 라운드 반경(창 리전 클립) |
| `title_h` | `i32` | 타이틀 밴드 높이(px) |
| `body_h` | `i32` | 본문 높이(px) |

### 사용 예 — 타이틀에 콤보를 얹는 PF 구도
```rust
let card = groupcard::create(dlg, x, y, w, ID_CARD, font, "",
    groupcard::GroupCardOpts { corner: 8, title_h: 34, body_h: 168 }, style);
let trc = groupcard::title_rect(card);
let combo = combobox::create(card, trc.left + 8, trc.top + 5, 170, 24,
                             ID_KIND, font, &items, 0, band_style);
// 카드 자식 통지는 카드 부모(dlg)로 투과 — dlg WndProc에서 (ID_KIND, ...) 수신
```
