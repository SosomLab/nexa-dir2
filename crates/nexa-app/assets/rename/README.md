# rename — 일괄 이름변경 ＋/− 아이콘

일괄 이름변경 카드의 **추가(＋)/삭제(−)** 버튼용 작은 이미지(사용자 요청 07-18).
채운 원 + 흰 기호, PNG·투명 배경·안티앨리어스.

## 색 규약(앱 팔레트)
- **추가 ＋** = 초록 `#33B24E`(앱 아이콘 계열 — 긍정/추가)
- **삭제 −** = 빨강 `#FF3B30`(= `Style.danger` — 삭제 위계 일치)
- **비활성** = 회색 `#A4A8AC`(= `Style.border` — NxIconButton disabled 규약,
  마지막 카드 1장 삭제 불가 등)

## 파일(각 16/20/32px — 100%/125%/200% DPI)
| 상태 | 추가 | 삭제 |
| --- | --- | --- |
| 활성 | `rename-add-{16,20,32}.png` | `rename-remove-{16,20,32}.png` |
| 비활성 | `rename-add-disabled-{16,20,32}.png` | `rename-remove-disabled-{16,20,32}.png` |

- 기하: 원 = 캔버스 − 1px 인셋(가장자리 잘림 방지)·기호 획 = size/8·둥근 캡.
- 현재 카드 ± 버튼은 [NxIconButton](../../src/ctl/iconbutton.rs) 벡터 렌더 —
  이 이미지는 raster 대체/고밀도 옵션용 보관(draw_icon 임베드 시 사용).
