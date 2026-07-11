# ADR-0002 · 텍스트 렌더링 — DirectWrite GDI interop 채택

> 상태: **Accepted** (2026-07-12) · 결정자: Sangyong Bae + Claude
> 관련: [01 아키텍처 §3](01-architecture.md) · [06 ADR-0001](06-adr-0001-stack.md) · DR-2(예산)·DR-5(디자인 규약)
> 실측 스파이크: `feat/m1-adr0002-render` — 같은 창에서 F2로 두 백엔드 전환, F3 = 200프레임 스크롤 벤치.

## 1. 문제

커스텀 드로잉(창 1개·더블 버퍼·가시 행만)의 **텍스트 경로**를 확정해야 한다. M0-7 스파이크는 GDI
`ExtTextOutW`로 검증했고, docs/01 §3은 M1에서 DirectWrite GDI interop(`IDWriteBitmapRenderTarget`)
전환을 가정했다 — 실측 없이는 확정 불가(원본의 "사후 최적화" 실패 교훈).

## 2. 후보

| | A. GDI `ExtTextOutW` | B. DirectWrite GDI interop |
| --- | --- | --- |
| 구성 | 메모리 DC + `ETO_OPAQUE` 행 단위 출력 | `IDWriteBitmapRenderTarget`(자체 메모리 DC) + `IDWriteTextLayout` → 커스텀 `IDWriteTextRenderer` → `DrawGlyphRun` |
| 품질 | ClearType. 한글은 레지스트리 FontLink 폴백(제어 불가) | ClearType + **시스템 폰트 폴백**(한글·이모지 일관), 서브픽셀 배치, 컬러 글리프 |
| 확장성 | 말줄임·정밀 히트테스트·부분 스타일 = 수작업 | TextLayout이 트리밍·히트테스트·범위 스타일 제공 — 컬럼 시스템(M1-4)·타입어헤드 강조(M1-6)의 기반 |
| GPU | 없음 | 없음(CPU 래스터, 스왑체인 금지 준수) |

공통: 배경은 GDI(ETO_OPAQUE 빈 텍스트) 채우기, 화면 반영은 BitBlt 1회. D2D 스왑체인 경로는
docs/01 §3 금지 사항(B1 예산 위반 경로)으로 후보 제외.

## 3. 실측 (2026-07-12, Windows 11 Pro 26200 실기 · 릴리스 빌드 · 1200×800 창 · 100k 행)

| 항목 | A. GDI | B. DW interop | 비고 |
| --- | --- | --- | --- |
| 스크롤 벤치(200프레임 풀 페인트 평균) | 6,072µs | **4,373µs (−28%)** | 둘 다 60fps 예산(16.7ms) 내. B는 행당 TextLayout 생성 포함(캐시 미적용) — 상한치 |
| 유휴 RSS(중앙값) | 13.30MB | 17.40MB (+4.1MB) | DWrite 팩토리·폰트 캐시. B1 예산(≤30MB) 내 |
| exe 크기 | 0.21MB | 0.23MB (+0.02MB) | B2 예산(≤10MB) 내 |
| 임포트 | OS 인박스만 | + `dwrite.dll`(OS 인박스) | B3 준수 |

## 4. 결정

**B. DirectWrite GDI interop을 M1 텍스트 렌더링 경로로 채택한다.**

- 속도: 레이아웃 캐시 없이도 GDI보다 빠름(글리프 런 일괄 래스터). 캐시 도입 여지로 하한 더 낮음.
- 품질·확장성: 한글 폰트 폴백 일관성 + TextLayout 기능(트리밍·히트테스트)이 M1-4~6 요구와 직결.
- 비용: RSS +4.1MB는 B1 여유(12.6MB) 내. 모든 예산 게이트 통과.

## 5. 이행·결과

- `nexa-gui::DrawCtx` 어휘(fill_rect·text_opaque)는 두 백엔드에서 검증됨 — **변경 없음**.
- `nexa-app/src/dw.rs` = 채택 구현. GDI 백엔드(`gdi.rs`)와 F2 전환은 **M1-3(실제 리스트 배선)에서 제거** —
  그 전까지 품질 육안 비교용으로 유지. 기본 백엔드를 DW로 전환.
- 행당 `CreateTextLayout` 생성은 M1-3에서 가시 행 레이아웃 캐시로 최적화(무효화 단위와 정합).
- `windows-core` 직접 의존 추가(`#[implement]` 전개 경로) — DR-8 원장 기록([10 §1-2](10-decision-record.md)).
