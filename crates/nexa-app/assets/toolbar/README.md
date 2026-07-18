# toolbar — 도구 모음 임베드 아이콘

도구 모음 버튼용 16×16 이미지 세트(사용자 요청 07-18: "도구 모음을 16x16 이미지
형태로 변경 — 각 이미지는 큰 이미지를 기반으로 생성").
**64px 캔버스에 벡터로 드로잉 후 16/20/32px 다운스케일**(HighQualityBicubic) —
원 스크립트는 세션 스크래치(`gen_toolbar_icons2.ps1`, GDI+ PowerShell).

## 색 규약

- **활성 잉크** = `#6E747C` 단일색 — 라이트/다크 테마 겸용 중간톤
  (v1 `#9AA0A8`은 라이트 배경에서 흐려 교체).
- **비활성** = 같은 그림 **알파 38%**(`96/255`) — 색이 아닌 투명도라
  어떤 배경에서도 '흐림'으로 읽힘(라이트 테마에서 진해 보이던 v1 결함 수정).

## 파일(각 16/20/32px — 100%/125%/200% DPI 버킷)

| 이름 | 기능(명령) | 비활성 변형 |
| --- | --- | --- |
| `panel-dual` | 듀얼 파일 패널(CMD_PANEL_DUAL) | ○ (싱글 모드에서 선택 불가) |
| `panel-single` | 싱글 파일 패널(CMD_PANEL_SINGLE) | — |
| `colsync` | 컬럼 넓이 동기화(CMD_COLW_SYNC) | ○ (싱글 모드에서 선택 불가) |
| `view-tree` | 트리 보기(CMD_VIEW_TREE) | — |
| `view-flat` | 플랫 보기(CMD_VIEW_FLAT) | — |
| `view-tiles` | 타일 보기(CMD_VIEW_TILES) | — |
| `refresh` | 새로고침(CMD_REFRESH) — 300° 호 + 접선 화살촉 | — |
| `settings` | 설정(CMD_PREFS) — 톱니(도넛+치형 8) | — |
| `hidden` | 숨김 파일 토글(CMD_TOGGLE_HIDDEN) — 눈 | — |
| `dotfiles` | 닷파일 토글(CMD_TOGGLE_DOTFILES) — ⋯ | — |

## 렌더 경로

`include_bytes!` 임베드([icons.rs](../../src/icons.rs) `EMBEDDED`, 키
`emb:<이름>:<크기>`) → [gdipctx](../../src/ctl/gdipctx.rs) `png_to_hicon`
(GDI+ 디코드 → `GdipCreateHICONFromBitmap`) → HICON 캐시(LRU — 축출 시 재생성).
그리기 크기→버킷(16/20/32) 매핑과 비활성 `#dis`→`-disabled` 해석은
[dw.rs](../../src/dw.rs) `draw_icon`. 포터블 단일 exe 규약(DR-3)에 따라
파일은 참조용 보관본이며 실행 시 디스크 접근 없음.
