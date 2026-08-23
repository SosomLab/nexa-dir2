# CLAUDE.md — Nexa Dir 프로젝트 컨텍스트 (이식용 메모리)

> 이 파일은 **다른 PC에서 clone 시 즉시 컨텍스트를 공유**하기 위한 휴대용 프로젝트 메모리다.
> **먼저 읽기:** [docs/STATUS.md](docs/STATUS.md)(현황) → [docs/10-decision-record.md](docs/10-decision-record.md)(결정).

## 1. 이 프로젝트는

**Nexa Dir** = 원본 [Nexa Dir](https://github.com/SosomLab/nexa-dir)(Rust 코어+WinUI 3/C#)의 기능을
**포터블 단일 exe · 초저메모리(RSS ≤30MB) · unmanaged 올 러스트**로 재구축하는 Windows 파일 탐색기.
원본은 기능 스펙·실측 교훈의 **원천(SSOT)** — 로컬 경로 `../nexa-dir`.
현 단계: **포스트 M5 — UX 고도화 + 배포 채널 정착**. M0(`0.1.0`)~M5(`0.6.0`) 완료, 최신 릴리스 **`0.16.0`**(08-11 — X-40 외부·클라우드 변경 자동 갱신·X-41 도구 모음 크기. **winget 2채널 모두 `0.16.0` 라이브**[08-11·08-14 병합 — 최신 릴리스와 완전 동기], Chocolatey 2종은 0.8.1 모더레이션에 잠긴 채라 **채널 제출 규칙으로 push 제외** — [21 §7·§8](docs/21-distribution.md)).

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
| DR-7 | **재개정(07-26 — ADR-0005)**: 미리보기 플러그인 런타임 = **WASM(wasmi)** — `.wasm` 단일 아티팩트·fuel 격리(Starlark는 실구축 후 교체·revert 가능). 내장은 폴백 |
| DR-8 | 외부 crate 기본 0 지향 — 추가는 docs/10 §1-2 원장에 건별 기록 |

## 3. 아키텍처 요약 ([docs/01](docs/01-architecture.md))

- 크레이트: `nexa-core`/`nexa-vfs`/`nexa-tree`(원본 이식) + `nexa-app`(bin·Win32 창) → M1에 `nexa-gui` 분리, M3+ `nexa-ops`/`nexa-shell`/`nexa-term`. **전부 rlib 정적 링크 = 단일 exe**.
- 렌더링: 창 1개 + WM_PAINT 더블버퍼 **가시 영역만 커스텀 드로잉**(nexa-tree 평면 스트림 부합). GPU 스왑체인 상시 보유 금지.
- 스레딩: UI 스레드 1 + 워커, 통지는 PostMessage(원본 A-1 세대 가드 계승).

## 4. 개발 환경 ([docs/11](docs/11-dev-environment.md))

- **맥 = 일상 개발**: `cargo test`(코어) + `cargo check --target x86_64-pc-windows-msvc`(**UI 코드까지 타입 검증** — WinUI 시절과 달리 가능).
- **Windows PC/CI = 실행·QA·예산 실측**. CI(windows-latest)가 실행 신뢰 원천.
- **비Windows 경로도 반드시 검사**(08-02 교훈 — core 잡이 07-27부터 일주일 넘게 붉었다): Windows `cargo test`가 green이어도 cfg 소거 경로는 미검증 → `cargo check --workspace --all-targets --target x86_64-unknown-linux-gnu`([docs/18 §2·§4](docs/18-build-and-test.md)).

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

## 7. 다음 단계 (2026-08-23 갱신)

> M0~M5는 전부 완료(`0.1.0`~`0.6.0`), 이후 포스트 M5 UX 고도화로 `0.16.0`까지 릴리스됨.
> 아래는 **지금 열려 있는 것**만. 최신 현황은 항상 [docs/STATUS.md](docs/STATUS.md).

1. **실기 QA 잔여분 소화** — 사용자 QA가 병목. 새 기능보다 우선. 최신 대기분 = **X-42 가상 파일 1·2차**(RDP 붙여넣기+워커·undo·DnD·클라우드) + **X-43 빈 폴더 글리프+β** + **X-44 간헐 무갱신 1·2차**(활성화=갱신·뷰포트 접힌 폴더 감시 — 전부 08-23 구현·미배포) + X-40 클라우드 실검증.
2. **릴리스 시 채널 제출 규칙**(08-23 사용자 지시 — [21 §8](docs/21-distribution.md)): 릴리스마다 winget·choco 배포 요청 상태를 실측해 **대기 중인 버전이 없으면 그 채널에 새 버전 제출, 대기 중이면 그 채널은 제외**(재개 시 중간 버전 생략·최신만).
3. **winget 완전 동기 달성**(08-23 확인): 0.16.0 PR 2건 모두 병합 — Portable [#415214](https://github.com/microsoft/winget-pkgs/pull/415214) 제출 당일(08-11)·설치형 [#415215](https://github.com/microsoft/winget-pkgs/pull/415215) 08-14(waiver 경유). 카탈로그 실측 = 두 채널 다 **0.16.0**. 대기 없음 → 다음 릴리스는 규칙대로 winget 제출.
4. **Chocolatey 2종 승인 대기**(우리 측 조치 불요·상태만 추적 — 08-11 기준): 0.8.1 두 패키지 모두 **큐 대기가 아니라 07-20 바이러스 스캔 플래그 후 무이벤트**(무서명 exe 오탐 — 모더레이터 면제 필요). **후속 버전 push는 위 규칙에 따라 계속 제외**(0.8.1이 잠겨 있어 이중 큐를 만들지 않는다) — 승인 시 저장소 변수 `CHOCO_PUSH=true` 등록으로 재개(현재 미등록). → [21 §7·§8](docs/21-distribution.md)
5. **클라우드 배포 선행 조건 3건**(X-37 잔여 = 코드 아님·본인 사용은 무관 — [ADR-0006 §2-4-1](docs/27-adr-0006-cloud-oauth.md)): Dropbox 프로덕션 승인(무료·수일 — 가장 쉽고 효과 큼) · Entra 게시자 확인(조직 계정 동의 차단 해소 — 커스텀 도메인 필요) · Google CASA(보류 — 프로덕션 게시로 7일 만료만 제거하는 절충 가능).
6. **백로그 진행** — [docs/TODO.md](docs/TODO.md) §7: X-11 원본 패리티 갭 건별([19](docs/19-parity-gap.md)) · X-2 플러그인(wasmi 전환 완료 — 잔여 콤보·핫 리로드) · X-16 최적화 잔여 · X-13 2/2.
7. **X-33 macOS·Linux 확장** — 검토 완료([23](docs/23-cross-platform-feasibility.md)), **착수 여부는 사용자 결정 대기**. 진행 시 다음 액션 = 맥 렌더 스파이크(결정 아님) + DR-1/2/8 개정 ADR(신규 번호 = ADR-0007).
