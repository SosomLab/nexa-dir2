# CLAUDE.md — Nexa Dir 2 프로젝트 컨텍스트 (이식용 메모리)

> 이 파일은 **다른 PC에서 clone 시 즉시 컨텍스트를 공유**하기 위한 휴대용 프로젝트 메모리다.
> **먼저 읽기:** [docs/STATUS.md](docs/STATUS.md)(현황) → [docs/10-decision-record.md](docs/10-decision-record.md)(결정).

## 1. 이 프로젝트는

**Nexa Dir 2** = 원본 [Nexa Dir](https://github.com/SosomLab/nexa-dir)(Rust 코어+WinUI 3/C#)의 기능을
**포터블 단일 exe · 초저메모리(RSS ≤30MB) · unmanaged 올 러스트**로 재구축하는 Windows 파일 탐색기.
원본은 기능 스펙·실측 교훈의 **원천(SSOT)** — 로컬 경로 `../nexa-dir`. 현 단계: **M2(셸 골격) 진행** — M0(`0.1.0`)·M1(`0.2.0`) 완료.

- 조직: **SosomLab** · 개발자: Sangyong Bae · kiros33@gmail.com (원본과 동일)

## 2. 확정 결정 ([docs/10](docs/10-decision-record.md), 변경 시 새 ADR/journal)

| # | 결정 |
| --- | --- |
| DR-1 | **올 러스트 단일 바이너리** — Win32(windows-rs)+커스텀 드로잉(GDI→DirectWrite interop). 관리 런타임·UI 프레임워크 금지 (ADR-0001) |
| DR-2 | **예산 게이트**: 유휴 RSS ≤30MB · exe ≤10MB · 임포트=OS 인박스 DLL만 — 초과 시 main 병합 금지 |
| DR-3 | 배포 = **포터블 단일 exe 단독**, 영속물은 exe 옆 `data\` |
| DR-4 | 원본 nexa-core/vfs/tree **rlib 이식**(cdylib/FFI/ABI 폐지) |
| DR-5 | 원본 M1 기능 패리티 + 디자인 규약(고밀도·다크·키보드 우선) 계승 |
| DR-6 | PolyForm NC + 의존성 **퍼미시브 온리**(GPL 금지 — Slint 배제 근거) |
| DR-7 | .NET 플러그인 SDK 비이관(내장 미리보기 대체) · WASM 보류 |
| DR-8 | 외부 crate 기본 0 지향 — 추가는 docs/10 §1-2 원장에 건별 기록 |

## 3. 아키텍처 요약 ([docs/01](docs/01-architecture.md))

- 크레이트: `nexa-core`/`nexa-vfs`/`nexa-tree`(원본 이식) + `nexa-app`(bin·Win32 창) → M1에 `nexa-gui` 분리, M3+ `nexa-ops`/`nexa-shell`/`nexa-term`. **전부 rlib 정적 링크 = 단일 exe**.
- 렌더링: 창 1개 + WM_PAINT 더블버퍼 **가시 영역만 커스텀 드로잉**(nexa-tree 평면 스트림 부합). GPU 스왑체인 상시 보유 금지.
- 스레딩: UI 스레드 1 + 워커, 통지는 PostMessage(원본 A-1 세대 가드 계승).

## 4. 개발 환경 ([docs/11](docs/11-dev-environment.md))

- **맥 = 일상 개발**: `cargo test`(코어) + `cargo check --target x86_64-pc-windows-msvc`(**UI 코드까지 타입 검증** — WinUI 시절과 달리 가능).
- **Windows PC/CI = 실행·QA·예산 실측**. CI(windows-latest)가 실행 신뢰 원천.

## 5. 작업 규약

- 원본 규약 전면 계승([docs/15](docs/15-dev-methodology.md)): **수직 슬라이스·단위=커밋 1개·초안→확장·main 항상 green·Conventional Commits**.
- **큰 단위=브랜치, 세부 기능=커밋. push는 사용자 명시 요청 시에만.** 사용자 개입 최소화 — 특별한 상황 외 자동 진행(파괴적 작업 제외).
- 기록: 일자 상세 `docs/journal/YYYY-MM-DD.md`(시간 역순) + [DEVLOG](docs/DEVLOG.md) 요약 + [MILESTONES](docs/MILESTONES.md) + [BRANCHES](docs/BRANCHES.md).
- **기능 설계 전 원본 문서·코드 먼저 확인**(재발명 금지). 이식 커밋에 원본 경로 명기.
- `.claude/settings.json`(권한)은 **덮어쓰기 금지, 병합만** — 세션 승인 항목 유실 사고 방지.
- 빌드/테스트 SSOT = [docs/18](docs/18-build-and-test.md) — 절차 변경 시 같은 커밋에서 갱신.

## 6. 새 세션 오리엔테이션

1. 이 CLAUDE.md + [docs/STATUS.md](docs/STATUS.md) → 2. [DEVLOG](docs/DEVLOG.md) 최상단 + 최신 journal → 3. 할 일 = [docs/TODO.md](docs/TODO.md)(M0-1부터 순차).

## 7. 다음 단계 (2026-07-11)

1. **M0**: `feat/m0-scaffold` — 워크스페이스·코어 3크레이트 이식·Win32 창 스켈레톤·CI·**게이트 실측**(Windows 실기) → `0.1.0`.
2. M1: ADR-0002(렌더링 확정) → 가상 리스트·플래그십 재현. → [docs/02](docs/02-roadmap.md)
