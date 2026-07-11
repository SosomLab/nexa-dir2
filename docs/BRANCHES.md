# BRANCHES — 브랜치 기록 (Branch History, 시간 역순)

> **목적**: 병합 후 삭제되는 작업 브랜치의 이력을 남긴다(원본 규약 차용). **정렬: 시간 역순(최신이 위)** — 새 브랜치는 표·상세 모두 맨 위에 추가. 시각=커밋 committer date(KST).
> **규약**: 브랜치는 main 병합·green 확인 후 삭제, 이력은 이 문서 + journal에 보존. push는 사용자 명시 요청 시에만.

## 요약 (시간 역순)

| 브랜치 | 생성 | 병합(커밋) | 삭제 | 커밋수 | 작업 요약 | 상세 |
| --- | --- | --- | --- | --- | --- | --- |
| `feat/m1-keyboard` | 2026-07-12 | 2026-07-12 (`14bfe86`) | — (CI green 후) | 3 | M1-6 — 캐럿 키보드 네비(이동+선택+스크롤 추적·Shift 범위·Ctrl·Space 토글·→자식/←부모)·타입어헤드(버퍼/cycle/HUD·코어 find_prefix C). 테스트 69 green·Shift+End 실기 "선택 61" | [2026-07-12](journal/2026-07-12.md) |
| `feat/m1-select` | 2026-07-12 | 2026-07-12 (`a37e01e`) | 2026-07-12 | 3 | M1-5 ★ — 인라인 펼침/선택 분리·교차폴더 다중 선택(Ctrl/Shift/Ctrl+A)·러버밴드·캐럿·sel_bg. 테스트 60 green·Ctrl+A 실기 확인·벤치 1,540µs | [2026-07-12](journal/2026-07-12.md) |
| `feat/m1-columns` | 2026-07-12 | 2026-07-12 (`6e4ba96`) | 2026-07-12 | 3 | M1-4 — 컬럼 시스템(원본 docs/23): 5컬럼·정렬 3상태+Shift 다중열·드래그 리사이즈·가로 스크롤·말줄임 트리밍·TZ 날짜. 테스트 55 green·벤치 1,813µs·B3 사전 통과 | [2026-07-12](journal/2026-07-12.md) |
| `feat/m1-virtual-list` | 2026-07-12 | 2026-07-12 (`baa3b3f`) | 2026-07-12 | 3 | M1-3 — 가상화 파일 리스트 초안: nexa-tree 평면 스트림 배선(TreeSource·들여쓰기·마커·클릭 토글)·GDI 경로 제거·DW 레이아웃 캐시(벤치 1,673µs). 테스트 48 green·실기 실측 | [2026-07-12](journal/2026-07-12.md) |
| `feat/m1-adr0002-render` | 2026-07-12 | 2026-07-12 (`e0daf56`) | 2026-07-12 | 2 | M1-2 — ADR-0002 확정: DirectWrite GDI interop 채택(벤치 −28%·RSS +4.1MB 예산 내), dw.rs 백엔드·F2/F3 비교 하네스·기본 백엔드 전환 | [2026-07-12](journal/2026-07-12.md) |
| `feat/m1-gui` | 2026-07-11 | 2026-07-11 (`c20ddde`) | 2026-07-11 | 3 | M1-1 — `nexa-gui` 크레이트 분리: 플랫폼 중립 위젯 trait·무효화(rect 병합)·입력 이벤트·테마 토큰(원본 docs/39 차용)·`VirtualRows` + nexa-app 재배선(`gdi.rs` DrawCtx 백엔드). 테스트 43 green·실기 확인 | [2026-07-11](journal/2026-07-11.md) |
| `feat/m0-render-spike` | 2026-07-11 | 2026-07-11 (`cc7e7ed`) | 2026-07-11 | 3 | M0-7 — GDI 렌더 스파이크: 더블 버퍼·합성 100k행 가시 영역만·휠/키 스크롤·DPI (windows 타깃 check·clippy green) + git -C 권한 병합 | [2026-07-11](journal/2026-07-11.md) |
| `feat/m0-scaffold` | 2026-07-11 | 2026-07-11 (`e1a2e7f`) | 2026-07-11 | 11 | M0-1~6 — 워크스페이스·코어 3크레이트 이식(테스트 green)·Win32 창 스켈레톤(windows 타깃 check green)·CI(예산 게이트) + 권한 복구 | [2026-07-11](journal/2026-07-11.md) |
| `docs/foundation` | 2026-07-11 | 2026-07-11 (`d2727b5`) | 2026-07-11 | 6 | 설계 문서 세트(비전·아키텍처·ADR-0001·DR·로드맵·TODO·운영 문서) + 권한 정리 | [2026-07-11](journal/2026-07-11.md) |

---

## feat/m1-keyboard

- **생성**: 2026-07-12 (분기: main `843da77`). **커밋**: `48ed980`(gui: 캐럿 네비·typeahead.rs·HUD) → `e5a2386`(app: WM_CHAR·수식키·타이머 배선) → `8a26d2d`(docs 현행화). 병합(`14bfe86`): 2026-07-12.
- **검증**: Windows 실기 — `cargo test --workspace` 69 green · clippy 0 · 릴리스 실행(캐럿 이동·타입어헤드 점프·keybd_event Shift+End "선택 61"·RSS 17.8MB) · B3 통과. QA 교훈: SendKeys 확장 키의 Shift 해제 특성.

## feat/m1-select

- **생성**: 2026-07-12 (분기: main `5de5385`). **커밋**: `df012bb`(gui: 선택 UX·러버밴드·키) → `67e784b`(app: TreeSource 선택·수식키·타이틀) → `378a233`(docs 현행화). 병합(`a37e01e`): 2026-07-12.
- **검증**: Windows 실기 — `cargo test --workspace` 60 green(교차폴더 AC2 포함) · clippy 0 · 릴리스 실행(SendKeys Ctrl+A → "선택 62" 타이틀 반영·벤치 1,540µs·RSS 18.20MB·정상 종료) · B3 통과.

## feat/m1-columns

- **생성**: 2026-07-12 (분기: main `6955308`). **커밋**: `57374d5`(gui: 컬럼 모델·헤더·정렬·리사이즈·가로 스크롤) → `47a9d28`(app: 5컬럼 셀 값·트리밍·이벤트 라우팅) → `8d9a6fc`(docs 현행화). 병합(`6e4ba96`): 2026-07-12.
- **검증**: Windows 실기 — `cargo test --workspace` 55 green(gui 23·source 6) · clippy 0 · 릴리스 실행(5컬럼 표시·벤치 1,813µs·RSS 18.14MB·정상 종료) · **B3 스크립트 사전 통과**(push 전 로컬).

## feat/m1-virtual-list

- **생성**: 2026-07-12 (분기: main `5a613b3` — CI 핫픽스 직후 rebase). **커밋**: `199d67a`(gui: RowItem·마커·클릭 토글) → `856ced5`(app: TreeSource 배선·GDI 삭제·레이아웃 캐시) → `5606dd7`(docs 현행화). 병합(`baa3b3f`): 2026-07-12.
- **검증**: Windows 실기 — `cargo test --workspace` 48 green(gui 20·source 2 포함) · clippy 0 · 릴리스 실행(실 트리 62행 표시·클릭 펼침/접힘·F3 벤치 1,673µs·RSS 18.17MB·정상 종료).

## feat/m1-adr0002-render

- **생성**: 2026-07-12 (분기: main `9e90d78`). **커밋**: `930eca9`(dw.rs 백엔드·비교 하네스·실측) → `6f3ba92`(ADR-0002 작성·docs 현행화). 병합(`e0daf56`): 2026-07-12.
- **검증**: Windows 실기 — 두 백엔드 벤치 자동화(SendKeys) 실측 · `cargo test --workspace` green · clippy 경고 0 · 릴리스 스모크(DW 기본, RSS 17.07MB).

## feat/m1-gui

- **생성**: 2026-07-11 (분기: main `37ca70f` — M0-8 직후). **커밋**: `63ff331`(nexa-gui 크레이트 신설) → `80cbbef`(nexa-app 재배선·gdi.rs) → `0055f7b`(docs 현행화). 병합(`c20ddde`): 2026-07-11.
- **검증**: Windows 실기 — `cargo test --workspace` 43 green(신규 nexa-gui 17) · clippy 경고 0 · fmt · 릴리스 빌드·기동·WM_CLOSE 정상(exe 0.21MB·RSS 12.4MB).

## feat/m0-render-spike

- **생성**: 2026-07-11 (분기: main `b894305`). **커밋**: `90a9243`(M0-7 렌더 스파이크) → `8c5f7f8`(git -C 권한 병합) → `8e6a986`(docs 현행화). 병합(`cc7e7ed`): 2026-07-11.
- **검증**: `cargo check/clippy --target x86_64-pc-windows-msvc` green(맥, 경고 0) · `cargo test` 26 green · fmt. 실행·화면 확인은 M0-8(Windows 실기)과 병행.

## feat/m0-scaffold

- **생성**: 2026-07-11 (분기: main `522a530`). **커밋**: `391e4bb`(M0-1 워크스페이스) → `38f92ff`(M0-2 core) → `585b2dc`(M0-3 vfs) → `370b56f`(M0-4 tree) → `34b5649`(M0-5 Win32 스켈레톤) → `3b2ddf8`(M0-6 CI) + 권한 복구 3건(`5c0d6bd`·`7282ad4`·`cb8e1db`) + docs 현행화. 병합: 2026-07-11.
- **검증**: `cargo test --workspace` green(tree 21+vfs 5) · `cargo check --target x86_64-pc-windows-msvc --workspace` green(맥). CI 러너 검증·게이트 실측(M0-8)은 push 후 Windows에서.

## docs/foundation

- **생성**: 2026-07-11 (분기: main `dffc8f9` 초기 커밋). **커밋 6개**(`43a0989` 권한 → `8f528de` 00·05 → `079fa34` 01·06 → `47597e4` 10·11·12·15·18 → `506fa4e` 02·MILESTONES·TODO → `db223e3` 운영). 병합(`d2727b5`)·삭제: 2026-07-11.
- **작업**: 원본 nexa-dir 문서 규약·형태를 차용한 설계 문서 세트 — 00/01/02/05/06/10/11/12/15/18 + README(홈)/STATUS/MILESTONES/DEVLOG/TODO/BRANCHES/journal/CLAUDE.md. 스택 결정(ADR-0001 올 러스트)·예산 게이트(DR-2)·로드맵 M0~M5.
