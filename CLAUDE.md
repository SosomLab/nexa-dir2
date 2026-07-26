# CLAUDE.md — Nexa Dir 프로젝트 컨텍스트 (이식용 메모리)

> 이 파일은 **다른 PC에서 clone 시 즉시 컨텍스트를 공유**하기 위한 휴대용 프로젝트 메모리다.
> **먼저 읽기:** [docs/STATUS.md](docs/STATUS.md)(현황) → [docs/10-decision-record.md](docs/10-decision-record.md)(결정).

## 1. 이 프로젝트는

**Nexa Dir** = 원본 [Nexa Dir](https://github.com/SosomLab/nexa-dir)(Rust 코어+WinUI 3/C#)의 기능을
**포터블 단일 exe · 초저메모리(RSS ≤30MB) · unmanaged 올 러스트**로 재구축하는 Windows 파일 탐색기.
원본은 기능 스펙·실측 교훈의 **원천(SSOT)** — 로컬 경로 `../nexa-dir`.
현 단계: **포스트 M5 — UX 고도화 + 배포 채널 정착**. M0(`0.1.0`)~M5(`0.6.0`) 완료, 최신 릴리스 **`0.11.0`**(GitHub Release + winget Portable 배포 완료 · winget 설치형·Chocolatey 2종은 심사 대기 — [21 §7·§8](docs/21-distribution.md)).

- 조직: **SosomLab** · 개발자: Sangyong Bae · kiros33@gmail.com (원본과 동일)

## 2. 확정 결정 ([docs/10](docs/10-decision-record.md), 변경 시 새 ADR/journal)

| # | 결정 |
| --- | --- |
| DR-1 | **올 러스트 단일 바이너리** — Win32(windows-rs)+커스텀 드로잉(GDI→DirectWrite interop). 관리 런타임·UI 프레임워크 금지 (ADR-0001) |
| DR-2 | **예산 게이트**: 유휴 RSS ≤30MB · exe ≤10MB · 임포트=OS 인박스 DLL만 — 초과 시 main 병합 금지 |
| DR-3 | **개정(07-16)**: 배포 = 포터블 단일 exe **기본** + **설치형 exe(Inno Setup) 보조** 2채널 — 영속물은 exe 옆 `data\`(쓰기 불가 위치는 `%LOCALAPPDATA%\NexaDir\data` 폴백, [docs/21](docs/21-distribution.md)) |
| DR-4 | 원본 nexa-core/vfs/tree **rlib 이식**(cdylib/FFI/ABI 폐지) |
| DR-5 | 원본 M1 기능 패리티 + 디자인 규약(고밀도·다크·키보드 우선) 계승 |
| DR-6 | PolyForm NC + 의존성 **퍼미시브 온리**(GPL 금지 — Slint 배제 근거) |
| DR-7 | **개정(07-14)**: .NET SDK 비이관 유지 + **Starlark 미리보기 플러그인 도입**(ADR-0004 — 내장은 폴백) · WASM 보류 |
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
- 기록: 일자 상세 `docs/journal/YYYY-MM-DD.md`(시간 역순) + [DEVLOG](docs/DEVLOG.md) 요약 + [MILESTONES](docs/MILESTONES.md) + [BRANCHES](docs/BRANCHES.md). **한 작업 = 한 트랜잭션 갱신**(커밋→journal→DEVLOG→MILESTONES/TODO→BRANCHES).
- **문서·커밋/푸시 규약 SSOT = [docs/16](docs/16-doc-git-conventions.md)** — 4층 문서 체계·작성 규칙 8·커밋/브랜치/푸시 필수 규칙(타 프로젝트 이식용 지시문 §0 포함).
- **기능 설계 전 원본 문서·코드 먼저 확인**(재발명 금지). 이식 커밋에 원본 경로 명기.
- `.claude/settings.json`(권한)은 **덮어쓰기 금지, 병합만** — 세션 승인 항목 유실 사고 방지.
- 빌드/테스트 SSOT = [docs/18](docs/18-build-and-test.md) — 절차 변경 시 같은 커밋에서 갱신.

## 6. 새 세션 오리엔테이션

1. 이 CLAUDE.md + [docs/STATUS.md](docs/STATUS.md) → 2. [DEVLOG](docs/DEVLOG.md) 최상단 + 최신 journal → 3. 할 일 = [docs/TODO.md](docs/TODO.md)(M0-1부터 순차).

## 7. 다음 단계 (2026-07-24 갱신)

> M0~M5는 전부 완료(`0.1.0`~`0.6.0`), 이후 포스트 M5 UX 고도화로 `0.11.0`까지 릴리스됨.
> 아래는 **지금 열려 있는 것**만. 최신 현황은 항상 [docs/STATUS.md](docs/STATUS.md).

1. **실기 QA 잔여분 소화** — 사용자 QA가 병목. 새 기능보다 우선.
2. **배포 채널 심사 대기 3건**(우리 측 조치 불요·상태만 추적): winget 설치형(#404528 — `Policy-Test-1.2` waiver 대기) · Chocolatey `nexa-dir`·`nexa-dir.portable`(모더레이션 큐). 승인 시 `CHOCO_PUSH` 스위치를 켜 후속 버전 재개. → [21 §7·§8](docs/21-distribution.md)
3. **백로그 진행** — [docs/TODO.md](docs/TODO.md) §7: X-11 원본 패리티 갭 건별([19](docs/19-parity-gap.md)) · X-2 Starlark 플러그인 · X-16 최적화 잔여 · X-13 2/2.
4. **X-33 macOS·Linux 확장** — 검토 완료([23](docs/23-cross-platform-feasibility.md)), **착수 여부는 사용자 결정 대기**. 진행 시 다음 액션 = 맥 렌더 스파이크(결정 아님) + DR-1/2/8 개정 ADR-0005.
