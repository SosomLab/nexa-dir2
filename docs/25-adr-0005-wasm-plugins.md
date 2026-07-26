# 25 · ADR-0005 — 플러그인 런타임 전환: Starlark → WASM(wasmi)

- 상태: **Accepted** (2026-07-26, 사용자 결정 — "부분 교체·revert 가능 방식") · 대체: [09 ADR-0004](09-adr-0004-preview-plugins.md)의 런타임 선택(A안 Starlark)을 **정정**(시임·격리·매핑 설계는 유지)
- 관련: [10 결정](10-decision-record.md) DR-7·DR-8 · [23 크로스플랫폼 검토](23-cross-platform-feasibility.md) · [journal/2026-07-26](journal/2026-07-26.md)

## 맥락 (왜 정정하는가)

Starlark로 MarkdownViewerPlugin을 실제 구축한 결과(07-26 실측):

1. **표현력 병목은 언어가 아니라 호스트 계약** — 품질 요구가 오를 때마다 해결책이 전부
   호스트 확장(render_svg·이미지 마커·라인 태그…)이었고 Starlark는 데이터 생성만 담당.
2. **개발 체험 한계** — 재귀·while 금지, 디버거 부재, 대형 로직(mermaid 레이아웃) 유지보수
   비용이 러스트 대비 높음.
3. **크로스플랫폼 확장(docs/23) 관점** — 러스트 개발자 대상 플러그인의 정답은
   cdylib(ABI 불안정·배포 매트릭스·격리 상실)이 아니라 **WASM**: `.wasm` 1개가 전
   OS/아키텍처에서 동일 동작 + 격리 유지 + 다언어(러스트 포함).

## 결정

1. **런타임 = `wasmi`**(순수 러스트 인터프리터) 임베드 — JIT(wasmtime +12~20MB 추정)은
   B2(≤10MB) 초과라 기각, wasmer headless는 아키텍처별 사전 컴파일이 `.wasm` 단일 배포
   이점을 상실해 기각. wasmi 예상 +1~2MB(**도입 커밋에서 실측 후 DR-8 원장 확정**).
   성능(인터프리터)은 미리보기 용도·연료 상한에 충분, fuel 계측 내장 = 격리 모델 정합.
2. **플러그인 = `wasm32-unknown-unknown` 모듈 1개**(`data\plugins\*.wasm`) — 참조 구현은
   러스트 크레이트(폐기 브랜치 `feat/md-preview`의 md/mermaid 러스트 자산 이식).
3. **유지(런타임 중립)**: PreviewProvider 시임·`preview_map`/`plugins_disabled` 결정
   규칙·독립 미리보기 창(F3/↗ — 기준 캔버스)·라인 태그 계약(`\u{2}종류|`·`\u{1}img|`)·
   실행 격리(시간/연료/메모리)·설정 플러그인 페이지·개발자 가이드 골격([24](24-plugin-dev-guide.md) 개정).
4. **원복 절차**: Starlark 제거는 정방향 커밋(force push 없음) — `git revert`로 언제든 복귀.

## 계약 (WASM ABI 초안 — 구현 커밋에서 확정)

- 플러그인 export: `nx_alloc(len)->ptr` · `nx_meta()->ptr`(UTF-8 `id\nname\next1,ext2`) ·
  `nx_preview()->ptr`(UTF-8, 첫 줄 `lines`/`image`, 이후 본문 — 라인 태그 포함).
  반환 버퍼 규약 = 선두 4바이트 LE 길이 + 본문.
- 호스트 import(`env`): `read_text(ptr,cap)->len`(대상 파일만·256KB 클램프) ·
  `render_svg(sptr,slen,optr,ocap)->len`(BMP 경로) · `is_dark()->i32` ·
  `disp_width(ptr,len)->i32`.
- 격리: wasmi **fuel**(연료 상한)·메모리 상한(StoreLimits)·시간 상한(호출 전후 실측) —
  초과 = 해당 플러그인만 오류 1줄(기존 경로).

## 결과

- DR-7 재조정: 플러그인 = **3계층**(내장 러스트 rlib / 경량 확장은 차후 재검토 /
  서드파티 = WASM). Starlark 원장 행은 "제거(07-26)"로 정정.
- md 뷰어 품질 상한 상승(러스트 구현) + 크로스플랫폼 단일 아티팩트 확보.
- 트레이드오프: 플러그인 개발에 러스트+wasm32 툴체인 필요(텍스트 파일 편집 편의 상실) —
  가이드 24에 빌드 절차 문서화.
