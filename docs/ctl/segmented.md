# NxSegmented — 세그먼트 라디오 (3호)

> [ctl 홈](README.md) · 클래스 `Nexa.NxSegmented` ·
> 소스 [`segmented.rs`](../../crates/nexa-app/src/ctl/segmented.rs)
> PF `AB CD | Ab Cd` / `→abc | ←abc` 세그먼트 대응.

## 모양(사용자 확정 07-17 — SegOpts)
| 필드 | 효과 |
|---|---|
| `corner` | 라운드 반경(0 = 각진·기본 6) |
| `gap` | 버튼 간 간격(**기본 0** = 연회색 컨테이너 + 선택 accent 필·흰 글자) |

- gap > 0 = 세그먼트별 독립 블록. 도형 = AA·모서리 = behind 블렌드.
- `h <= 0` = **컴팩트**(글꼴+2px — 버튼과 동일).
- **화살표 라벨**(07-18): `"→ "`/`"← "` 접두 = **Segoe MDL2 글리프**
  (Forward U+E72A·Back U+E72B — 원본 내비 버튼과 동일한 짧은 샤프트+큰 촉,
  컨트롤 소유 icon_font·Drop RAII). [글리프][5px][텍스트] 묶음 중앙 정렬.

## 동작
- 클릭·←/→ 키 = 선택 변경 → 통지. IsDialogMessage 아래에서도 ←→ 유지
  (`DLGC_WANTARROWS`).

## API·메시지 계약
| 항목 | 값 | 의미 |
|---|---|---|
| `create(parent, x, y, w, h, id, font, items, selected, opts, style)` | — | 라벨 복사 소유 |
| 통지 `SEG_CHANGED` | 1 | 선택 변경 |
| `SEG_GETSEL` / `SEG_SETSEL` | WM_USER+50/51 | 선택 조회/설정(SETSEL 통지 없음) |
