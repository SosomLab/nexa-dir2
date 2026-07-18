# NxLabel — 폼 라벨 (12호)

> [ctl 홈](README.md) · 클래스 `Nexa.NxLabel` ·
> 소스 [`label.rs`](../../crates/nexa-app/src/ctl/label.rs)
> 카드 폼의 좌측 라벨 열(PF 구도) — 높이·기준선 일관성용 전용 라벨.

## 모양·동작
- `LabelAlign::Left` / `Right`(카드 라벨 열 = **우측 정렬** — 사용자 확정 07-17:
  콜론이 컨트롤에 붙는 구도).
- 배경 = `Style.behind` 필(부모 배경과 일치 — 카드 위/회색 상태줄 위 모두 자연).
- 텍스트 = 공통 +1px 하향(다른 Nx와 같은 row 기준선 일치).
- **클릭 투과**(`WM_NCHITTEST = HTTRANSPARENT`) — 라벨이 마우스를 가로채지 않음.
- `h <= 0` = 공통 자동 높이.

## 언어-내구 정렬(호스트 규약)
라벨 열 폭은 **현재 언어 라벨 실측 최대치**(`style::text_width`)로 계산 —
한/영 전환에도 라벨 열·컨트롤 열 정렬 유지(bulkrename `build_card_body` 참조).

## API
| 항목 | 의미 |
|---|---|
| `create(parent, x, y, w, h, id, font, text, align, style)` | 텍스트 복사 소유 |
| 통지 | 없음(표시 전용) |
