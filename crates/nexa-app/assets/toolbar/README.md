# toolbar — 도구 모음 임베드 아이콘(SVG)

도구 모음 버튼 아이콘 원본. **전부 SVG 단일 파이프라인**(07-19 완결 —
PNG 버킷 기구 폐지): `include_str!` 임베드 → [svg.rs](../../src/svg.rs)
서브셋 파서 → [gdipctx](../../src/ctl/gdipctx.rs) `svg_to_hicon`(GDI+
오프스크린 ARGB → HICON, **요청 크기 즉석 래스터** — DPI 무관 선명).

## 규격(사용자 확정 07-19)

- **32 viewBox · 콘텐츠 1..31(꽉 참) · stroke 2 · currentColor** —
  단, 16px 정합이 중요한 스트로크는 정수 픽셀 정렬(panel-toggle 2..30).
- 잉크: 활성 `SVG_INK #2B3036`(어두운 회색) · 비활성 = 알파 38% ·
  켜짐 강조 = accent `#3D8BFF`(`-on` 접미 — 전체 재렌더).
- 지원 서브셋(초과 시 전체 무효 → 글리프 폴백): rect(rx)/circle/line/
  polyline/path(M·L·H·V·C·**A[원형 한정]**·Z)/text — 요소별 `stroke`/`fill`
  색·채움 오버라이드.

## 파일(명령 매핑)

| 파일 | 기능(명령) |
| --- | --- |
| `panel-toggle.svg` | 패널 듀얼↔싱글 토글(CMD_PANEL_TOGGLE) — 켜짐 = 전체 accent |
| `colsync.svg` | 컬럼 넓이 동기화(CMD_COLW_SYNC) — 싱글 모드 비활성(알파 렌더) |
| `view-tree.svg` | 트리 보기(CMD_VIEW_TREE) |
| `view-flat.svg` | 플랫 보기(CMD_VIEW_FLAT) |
| `view-tiles.svg` | 타일 보기(CMD_VIEW_TILES) |
| `refresh.svg` | 새로고침(CMD_REFRESH) — A 원호 |
| `settings.svg` | 설정(CMD_PREFS) — 톱니(원호 다수) |
| `hidden.svg` | 숨김 파일 토글(CMD_TOGGLE_HIDDEN) — 파일+H |
| `dotfiles.svg` | 닷파일 토글(CMD_TOGGLE_DOTFILES) — 파일+점 |

등록 = [icons.rs](../../src/icons.rs) `EMBEDDED_SVG`(이름·경로 한 줄).
전 시안 = 사용자 제공(07-18~19).
