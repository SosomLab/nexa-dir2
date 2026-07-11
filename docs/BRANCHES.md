# BRANCHES — 브랜치 기록 (Branch History, 시간 역순)

> **목적**: 병합 후 삭제되는 작업 브랜치의 이력을 남긴다(원본 규약 차용). **정렬: 시간 역순(최신이 위)** — 새 브랜치는 표·상세 모두 맨 위에 추가. 시각=커밋 committer date(KST).
> **규약**: 브랜치는 main 병합·green 확인 후 삭제, 이력은 이 문서 + journal에 보존. push는 사용자 명시 요청 시에만.

## 요약 (시간 역순)

| 브랜치 | 생성 | 병합(커밋) | 삭제 | 커밋수 | 작업 요약 | 상세 |
| --- | --- | --- | --- | --- | --- | --- |
| `feat/m0-render-spike` | 2026-07-11 | 2026-07-11 (`cc7e7ed`) | — (CI green 후) | 3 | M0-7 — GDI 렌더 스파이크: 더블 버퍼·합성 100k행 가시 영역만·휠/키 스크롤·DPI (windows 타깃 check·clippy green) + git -C 권한 병합 | [2026-07-11](journal/2026-07-11.md) |
| `feat/m0-scaffold` | 2026-07-11 | 2026-07-11 (`e1a2e7f`) | — (CI green 후) | 11 | M0-1~6 — 워크스페이스·코어 3크레이트 이식(테스트 green)·Win32 창 스켈레톤(windows 타깃 check green)·CI(예산 게이트) + 권한 복구 | [2026-07-11](journal/2026-07-11.md) |
| `docs/foundation` | 2026-07-11 | 2026-07-11 (`d2727b5`) | 2026-07-11 | 6 | 설계 문서 세트(비전·아키텍처·ADR-0001·DR·로드맵·TODO·운영 문서) + 권한 정리 | [2026-07-11](journal/2026-07-11.md) |

---

## feat/m0-render-spike

- **생성**: 2026-07-11 (분기: main `b894305`). **커밋**: `90a9243`(M0-7 렌더 스파이크) → `8c5f7f8`(git -C 권한 병합) → `8e6a986`(docs 현행화). 병합(`cc7e7ed`): 2026-07-11.
- **검증**: `cargo check/clippy --target x86_64-pc-windows-msvc` green(맥, 경고 0) · `cargo test` 26 green · fmt. 실행·화면 확인은 M0-8(Windows 실기)과 병행.

## feat/m0-scaffold

- **생성**: 2026-07-11 (분기: main `522a530`). **커밋**: `391e4bb`(M0-1 워크스페이스) → `38f92ff`(M0-2 core) → `585b2dc`(M0-3 vfs) → `370b56f`(M0-4 tree) → `34b5649`(M0-5 Win32 스켈레톤) → `3b2ddf8`(M0-6 CI) + 권한 복구 3건(`5c0d6bd`·`7282ad4`·`cb8e1db`) + docs 현행화. 병합: 2026-07-11.
- **검증**: `cargo test --workspace` green(tree 21+vfs 5) · `cargo check --target x86_64-pc-windows-msvc --workspace` green(맥). CI 러너 검증·게이트 실측(M0-8)은 push 후 Windows에서.

## docs/foundation

- **생성**: 2026-07-11 (분기: main `dffc8f9` 초기 커밋). **커밋 6개**(`43a0989` 권한 → `8f528de` 00·05 → `079fa34` 01·06 → `47597e4` 10·11·12·15·18 → `506fa4e` 02·MILESTONES·TODO → `db223e3` 운영). 병합(`d2727b5`)·삭제: 2026-07-11.
- **작업**: 원본 nexa-dir 문서 규약·형태를 차용한 설계 문서 세트 — 00/01/02/05/06/10/11/12/15/18 + README(홈)/STATUS/MILESTONES/DEVLOG/TODO/BRANCHES/journal/CLAUDE.md. 스택 결정(ADR-0001 올 러스트)·예산 게이트(DR-2)·로드맵 M0~M5.
