# NxGrid — 기본 그리드 (14호)

> [ctl 홈](README.md) · 클래스 `Nexa.NxGrid` · 소스 [`grid.rs`](../../crates/nexa-app/src/ctl/grid.rs)
> 사용자 확정 07-18: "가장 기본 기능의 Grid + 확장 가능한 설계". 미리보기(일괄
> 이름변경)·프리셋 관리 목록이 소비.

## 기본 기능(코어)
- 컬럼 헤더 + **경계 드래그 리사이즈**(±4px 히트·최소 40px·IDC_SIZEWE).
- **오버레이 스크롤바 세로/가로**(macOS 시안): 스크롤 순간 얇은 썸(6px) 표시 →
  900ms 페이드, 썸 드래그 = 트랙 있는 일반 바(10px). 컬럼 합 > 폭이면 가로
  (Shift+휠). 깜빡임 방지 = **WM_PAINT 더블버퍼**.
- **행 선택(파일 목록 규약)**: 클릭 = 단일 · Shift+클릭/방향키 = 앵커 연속 ·
  Ctrl+클릭 = 비연속 토글 · **Ctrl+방향키 = 포커스만 이동** 후 Space(Ctrl+Space =
  토글) · Ctrl+A · Home/End/PgUp/PgDn. 선택 = sel_bg 필·포커스 행 = accent 프레임.
- **다중열 정렬(원본 docs/23 §4 이식)**: 헤더 클릭 = 3상태 순환(없음→▲→▼→없음),
  Shift+클릭(기존 정렬 ≥1) = 키 추가/방향 순환/제거. ▲▼ = 컬럼명 앞·순번
  원문자(①②) = 뒤. **비교는 호스트**(`NXGR_SORT` 통지 → `sort_spec` 조회 →
  재정렬해 `set_rows`).

## 확장(GridOpts — 상속 대신 셀 데이터화)
| 필드 | 효과 |
|---|---|
| `no_header` | 헤더 숨김(목록 모드) |
| `zebra` | 지브라 줄무늬(bg↔sel_bg 50% 블렌드 — 빈 슬롯까지) |
| `outline` | 1px 외곽선(목록 모드) |
| `row_h` | 행 높이(≤0 = 자동 / 파일 목록 20px 정렬 시 지정) |
| `mark` | `None` / `Check`(컬럼 0 체크박스) / `Minus`(행 우측 빨간 ⊖) |

- `Mark::Check` + 헤더 = **헤더 체크박스**(전 행 집계: 전체 ✓/부분 흐릿한 ✓/해제,
  클릭 = 전체 토글 → `NXGR_GETROW == NXGR_ROW_ALL(-2)`).
- 행 데이터 = `GridRow { check: Option<bool>, cells: Vec<String> }`.

## API·메시지 계약
| 항목 | 값 | 의미 |
|---|---|---|
| `create(parent, x, y, w, h, id, font, cols, opts, style)` | — | `cols` = (제목, 초기 폭) |
| `set_rows(hwnd, Vec<GridRow>)` | — | 전체 교체(선택/포커스 클램프 유지) |
| `row_check(hwnd, idx)` | — | 행 체크 상태 |
| `sort_spec(hwnd)` | — | (컬럼, desc) 목록 |
| `selected_rows(hwnd)` | — | 선택 행 인덱스(오름차순) |
| 통지 `NXGR_TOGGLE` | 1 | 체크/⊖ 클릭(행 = `NXGR_GETROW`) |
| 통지 `NXGR_SELCHANGE` | 2 | 선택 변경 |
| 통지 `NXGR_SORT` | 3 | 정렬 변경(비교 = 호스트) |
| `NXGR_GETROW` | WM_USER+120 | 마지막 토글 행(-1 없음·-2 전체) |

## 주의(원장)
- **빈 문자열 셀은 그리지 않는다**(빈 Vec = 댕글링 → user32 AV — 07-18 진범).
- 도형(체크·⊖·썸) = AA(DrawCtx)·텍스트 = GDI(+1px 하향).

## 개발자 레퍼런스

### 함수
| 함수 | 설명 |
|---|---|
| `create(parent, x, y, w, h, id, font, cols, opts, style) -> HWND` | 그리드 생성. `cols: &[(&str, i32)]` = (헤더 제목, 초기 폭 px — 최소 40 클램프) |
| `set_rows(hwnd, rows: Vec<GridRow>)` | 행 전체 교체(대량 데이터 — 메시지 마샬링 회피 직접 API). 선택/포커스/스크롤은 새 행 수로 클램프 유지 |
| `row_check(hwnd, idx: usize) -> Option<bool>` | `idx` 행 체크 상태(`None` = 마크 없는 행) |
| `sort_spec(hwnd) -> Vec<(usize, bool)>` | 정렬 상태 — 우선순위 순 (컬럼 인덱스, 내림차순 여부). `NXGR_SORT` 수신 시 읽는다 |
| `selected_rows(hwnd) -> Vec<usize>` | 선택된 행 인덱스(오름차순) |

### 프로퍼티 — `GridRow`
| 필드 | 타입 | 설명 |
|---|---|---|
| `check` | `Option<bool>` | 마크 셀 상태. `None` = 마크 없음(무변경 행)·`Some(on)` = 체크/⊖ 표시 |
| `cells` | `Vec<String>` | 텍스트 셀(체크 열 사용 시 컬럼 1부터 — `cells[k]` ↔ `cols[k+1]`) |

### 프로퍼티 — `GridOpts` (전 필드 `Default`)
| 필드 | 타입 | 기본 | 설명 |
|---|---|---|---|
| `no_header` | `bool` | false | 헤더 숨김(목록 모드 — 정렬/리사이즈 비활성) |
| `zebra` | `bool` | false | 지브라 줄무늬(bg↔sel_bg 50% 블렌드·빈 슬롯 연속) |
| `outline` | `bool` | false | 1px 외곽선(`style.border`) |
| `row_h` | `i32` | 0 | 행 높이 px. `<= 0` = 자동(글꼴+8px) |
| `mark` | `Mark` | `None` | 마크 셀 종류(아래) |

### 프로퍼티 — `Mark`
| 값 | 설명 |
|---|---|
| `None` | 순수 텍스트 그리드 |
| `Check` | 컬럼 0 = 체크박스(클릭 토글 → `NXGR_TOGGLE`)·헤더 = 전체 토글(전체 ✓/부분 흐릿 ✓/해제) |
| `Minus` | 행 우측 끝 빨간 ⊖(`style.danger`) — 클릭 = `NXGR_TOGGLE`(삭제 실행은 호스트) |

### 사용 예 — 미리보기 그리드
```rust
let grid = grid::create(dlg, x, y, w, h, ID_PREV, font,
    &[("", 29), ("이전", 240), ("이후", 240)],
    grid::GridOpts { mark: grid::Mark::Check, row_h: 20, ..Default::default() },
    Style::default());
grid::set_rows(grid, rows);

// WndProc — 통지 3종
(ID_PREV, grid::NXGR_TOGGLE) => {
    let row = SendMessageW(grid, grid::NXGR_GETROW, None, None).0;
    if row == grid::NXGR_ROW_ALL { /* 헤더 전체 토글 — 전 행 재동기 */ }
    else if row >= 0 { /* 단일 행 — row_check로 반영 */ }
}
(ID_PREV, grid::NXGR_SORT) => {
    let keys = grid::sort_spec(grid);  // 비교 = 호스트 책임
    /* keys로 재정렬 → set_rows */
}
(ID_PREV, grid::NXGR_SELCHANGE) => { let sel = grid::selected_rows(grid); }
```
