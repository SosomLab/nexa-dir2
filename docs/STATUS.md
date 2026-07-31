# STATUS — Nexa Dir 진행 현황

> **갱신: 2026-08-01 3차 (KST)** — **클라우드 연계 검토(사용자 요청 —
> `docs/cloud-integration-study`, 병합 대기)**: **[26 검토서]
> (26-cloud-integration-study.md) 신설 — 2단계 분리 권고**. **Phase A
> (X-36)** = 동기화 폴더 탐지(OneDrive 레지스트리[BusinessN 다계정]·
> DriveFS·Dropbox info.json·iCloud) → `::PC::` "클라우드" 섹션(X-17
> join-대체 재사용·crate 0·예산 무변) · **Phase B(X-37)** = 기능 플러그인
> (`kind: cloud`·공급자 ABI·호스트 `nx_http`[WinHTTP — B3 +1]·OAuth2
> PKCE·**연결 모델 = 클라우드별 2+계정·토큰 플러그인 비노출**·읽기 전용
> 1차 — **ADR-0006 전제**). TODO X-36/X-37 등록. **착수 = 사용자 결정
> 5건 대기**(검토서 §6 — Phase A 승인·B 순서·범위·client_id·ADR).
> [journal/2026-08-01.md](journal/2026-08-01.md).
>
> **직전(08-01 2차)** — **CLAUDE.md 노후화 정정 + 기록
> 마감·push(사용자 지시)**: §1 최신 릴리스 `0.12.0`·§7-2 채널 전제
> 08-01 기준 정정 후 **08-01 4커밋 main push**(배포 점검·winget
> 매니페스트 수정·CLAUDE.md·기록 마감 — 전부 main 직커밋). **잔여**:
> 실기 QA 4건(pptx 활성화·OneDrive 갱신·터미널 스레드·다음 태그 자산
> 5종) · 외부 심사 3건 추적(winget 설치형 waiver·choco 2종 큐 — 승인 시
> `CHOCO_PUSH` 재개) · 백로그([TODO](TODO.md) §7).
> [journal/2026-08-01.md](journal/2026-08-01.md).
>
> **직전(08-01 1차)** — **winget 설치형 #404528 피드백
> 처리(사용자 승인)**: `DisplayVersion` 제거 요청(07-31 부착
> `Needs-Author-Feedback`) 대응 — 포크 PR 브랜치 installer.yaml 1줄
> 제거(**커밋 `7e49694`** — gh contents API·kiros33 전환 후 원복) +
> 회신 코멘트 + 로컬 사본 동기. **author 쪽 블로커 해소** — 잔여
> (`Policy-Test-1.2` waiver·라벨 해제)는 모더레이터 몫, 상태 추적만.
> [21 §8](21-distribution.md) 표 갱신.
> [journal/2026-08-01.md](journal/2026-08-01.md).
>
> **직전(07-31 17차)** — **winget·choco 배포 상태 점검(사용자
> 요청 — 원천 실측)**: ① **winget Portable 0.12.0(#408280) MERGED**
> (07-27 제출 당일 승인 — 릴리스와 완전 동기화된 유일 패키지 매니저 채널)
> ② **winget 설치형(#404528)** — 07-31 `Needs-Author-Feedback` 부착:
> `DisplayVersion` 제거 요청(모더레이터 자동화). **"조치 불요" 전제
> 깨짐** — 포크 수정 push 필요(무응답 시 자동 클로즈·사용자 승인 대기.
> `Policy-Test-1.2` waiver는 그 후에도 모더레이터 몫) ③ choco 2종 0.8.1
> "Ready for review" 무변동(12일째). [21 §8](21-distribution.md) 채널 표
> 갱신. [journal/2026-07-31.md](journal/2026-07-31.md).
>
> **직전(07-31 16차)** — **3건 마감 — main 병합·push(사용자
> 지시)**: 오늘 작업 3브랜치(`feat/dist-zip-assets` ZIP 자산 ·
> `fix/foreground-activation` 포그라운드 양도 · `fix/stability-hardening`
> 안정성 감사 7건) 선형 스택 **main ff 병합·push·브랜치 삭제**(BRANCHES
> 표 이동). **219 green·clippy 0·exe 3.20MB(B2)**. 잔여 실기 QA(사용 중
> 확인): 다음 태그 Release 자산 5종·zip 다운로드 무차단 / PowerPoint 띄운
> 채 .pptx 더블클릭 → 창 앞으로 / OneDrive 엑셀 저장 반복 → 자동 갱신
> 유지 / 터미널 여닫기 → 스레드 무누적.
> [journal/2026-07-31.md](journal/2026-07-31.md).
>
> **직전(07-31 15차)** — **전 소스 안정성 감사(사용자 요청 —
> `fix/stability-hardening`)**: 워커 6곳·unwrap
> 131건·핸들 수명 전수(전제 = panic=abort). 결함 6건 수정 — **S1/S2**
> watcher 죽음(버퍼 넘침 오판·재구독 불가 — **OneDrive 간헐 무갱신의
> 유력 원인**) → ENUM_DIR 복구+`alive` 자가 치유 · **S3** conpty 종료
> 대기 원시 핸들 경쟁(영구 대기 누수) → `DuplicateHandle` · **S4/S5**
> 삭제·전송 완료 통지 유실 = 상태 영구 고착 → `post_final_notify` 재시도 ·
> **S6** 기동 `C:\ expect` abort → `open_any_root()` 순회 · **S7** 공유
> Mutex poison 내성 `plock()` + panic 후크 `data\crash.txt`. **219
> green(신규 2)·clippy 0·3.20MB(B2)·스모크 6초 생존**. **QA 대기**:
> OneDrive 엑셀 저장 반복 → 자동 갱신 유지.
> [journal/2026-07-31.md](journal/2026-07-31.md).
>
> **직전(07-31 14차)** — **연 프로그램이 뒤에 숨는 결함
> 픽스(사용자 QA — `fix/foreground-activation`, 실기 QA 후 병합 대기)**:
> 파워포인트 등을 더블클릭하면 문서는 열리나 **창이 활성화되지 않고 뒤에
> 숨던** 결함. 원인 = **Windows 포그라운드 잠금** — `ShellExecute`·
> `InvokeCommand`는 연결 프로그램이 **이미 떠 있으면** DDE/COM으로 기존
> 프로세스에 위임하는데, 그 프로세스는 우리 자식이 아니라 권한을 물려받지
> 못해 `SetForegroundWindow`가 거부된다(Office는 단일 인스턴스라 새 문서도
> 같은 경로). 수정 = `win::allow_foreground_handoff()`
> (`AllowSetForegroundWindow(ASFW_ANY)`)를 실행 위임 **4개소**(더블클릭·
> 셸 우클릭 열기·퀵 런처·About 링크)에 적용 — 탐색기 동일 규약.
> **217 green·clippy 0**. **QA 대기**(자동 검증 불가 — 핵심 = PowerPoint를
> 띄워 둔 채 다른 .pptx 더블클릭). [journal/2026-07-31.md](journal/2026-07-31.md).
>
> **직전(07-31 13차)** — **ZIP 자산 + SHA256SUMS 추가
> (사용자 요청 — `feat/dist-zip-assets`, 병합 대기)**: 브라우저 Release exe
> 다운로드 **보안 차단** 보고 대응. 원인 = 무서명(DR-3) exe에 대한
> SmartScreen **다운로드 평판 필터**. release.yml에 zip 2종(exe+안내
> `README.txt`) + `SHA256SUMS.txt` 스텝 신설 — **기존 exe 자산은 유지**
> (winget·choco가 URL+SHA256 직참조). 결과 자산 5종. 로컬 dry-run 검증
> 완료, 실 검증은 다음 태그 push. 함께 **서명 경로 재조사**([12 §4-1](12-packaging-single-exe.md)
> 신설) — EV 즉시 통과 2024년 폐지·Azure Artifact Signing 한국 불가·
> SignPath Foundation 라이선스 결격 → 경고 완전 해소는 **Microsoft
> Store(MSIX)** 뿐(재검토 대상·착수는 사용자 결정 대기).
> [journal/2026-07-31.md](journal/2026-07-31.md).
>
> **직전(07-27 12차)** — **릴리스 `0.12.0` + winget Portable
> 업데이트 PR(사용자 요청 — choco 제외)**: 0.12.0 승격(X-34 새로 만들기·
> X-35 삭제 잠금 처리) → 태그 push → **Actions 게이트 전부 통과** →
> [Release 0.12.0](https://github.com/SosomLab/nexa-dir2/releases/tag/0.12.0)
> 포터블 3.35MB+설치형 첨부(choco = `CHOCO_PUSH` 꺼짐 자동 제외).
> winget `SosomLab.NexaDir.Portable` 0.12.0 매니페스트(SHA 실측) →
> **[winget-pkgs#408280](https://github.com/microsoft/winget-pkgs/pull/408280)
> 제출(심사 대기)**. [21 §8](21-distribution.md) 채널 표 갱신.
> [journal/2026-07-27.md](journal/2026-07-27.md).
>
> **직전(07-27 11차)** — **X-35 마감 — main 병합·push(사용자
> 지시)**: 휴지통 삭제 잠금 사전 프로브 + 실패 통지·재시도(레이어드 P+B) +
> 펼친 하위 폴더 감시(M3-6 α 해소)를 `feat/x35-delete-locked` 5커밋으로
> **main ff 병합·push·브랜치 삭제**. **217 green·clippy 0·exe 3.36MB**.
> 잔여 실기 QA(엑셀 `~$` 자동 소멸·사전 모달·부분 Ctrl+Z·폴더 하위 잠김
> 백스톱)는 사용 중 확인. [journal/2026-07-27.md](journal/2026-07-27.md).
>
> **직전(07-27 10차)** — **X-35 QA 1차 — 펼친 하위 폴더
> 감시(사용자 QA)**: 잠금 삭제 차단 정상 확인 · 잔결함 = 엑셀 `~$`
> 임시파일이 **펼친 하위 폴더**에서 정리돼도 목록 잔존 → M3-6 α(비재귀·
> 루트만 감시 — 원본도 동일 한계) 해소: `watch_dirs`(루트+가시 펼침
> 폴더·상한 64) + 패널별 watcher Vec + **diff 재구독**(폴더별 비재귀
> 핸들 — 재귀 감시의 대형 트리 폭주 회피). **217 green·clippy 0·exe
> 3.36MB**. 재QA: 엑셀 종료 시 `~$` 자동 소멸.
> [journal/2026-07-27.md](journal/2026-07-27.md).
>
> **직전(07-27 9차)** — **X-35 구현(사용자 지시 —
> `feat/x35-delete-locked` 1커밋, 실기 QA 후 병합 대기)**: 확정 설계
> 그대로 — 사전 잠금 프로브(µs 판정·잠긴 항목 **배치 모달 1회**
> [건너뛰고 삭제(N)]/[다시 시도]/[취소]·숨김 제외) + 사후 `exists()`
> diff 백스톱(성공분만 **부분 undo**·실패분 **선택 강조**·실패 모달
> [다시 시도]=실패분 재진입) + watcher `pending_delete` 가드(조기 재출현
> 차단). **217 green·clippy 0·exe 3.36MB**. **QA 대기**: 잠긴 파일 DEL
> 사전 모달·폴더 하위 잠김 백스톱·부분 Ctrl+Z·삭제 중 FS 변경.
> [journal/2026-07-27.md](journal/2026-07-27.md).
>
> **직전(07-27 8차)** — **X-35 휴지통 삭제 실패 무통지 —
> 분석·설계 확정(사용자 보고 — 코드 무수정)**: 열린 파일 삭제 실패 시
> 행만 재출현·무통지 → 결함 4건(타이틀 한 줄 통지·배치 1비트 결과·부분
> undo 미기록·watcher 가드 부재)·원본 회귀 3건 진단. 설계 = **레이어드
> P+B**: 사전 잠금 프로브(사용자 제안 — `CreateFileW(DELETE)` µs 판정) +
> `exists()` diff 백스톱 + 모달 [건너뛰고 삭제]/[다시 시도](사용자 확정) +
> 성공분만 부분 undo + watcher `pending_delete` 가드. IFileOperation은
> 후속 재검토. **TODO X-35 등록 — 구현 대기**.
> [journal/2026-07-27.md](journal/2026-07-27.md).
>
> **직전(07-27 7차)** — **X-34 마감 — main 병합·push(사용자
> 지시)**: 항목 우클릭 "새로 만들기"(CLSID_NewMenu 호스팅) + 생성 후
> 인라인 리네임 + QA 1차 픽스(선별 라우팅)를 `feat/ctx-new-menu` 5커밋으로
> **main ff 병합·push·브랜치 삭제**. **217 green·clippy 0·exe 3.35MB**
> (픽스 반영 재빌드). 잔여 실기 QA(템플릿 생성·리네임 진입·다중 숨김·설정
> 숨김)는 사용 중 확인. [journal/2026-07-27.md](journal/2026-07-27.md).
>
> **직전(07-27 6차)** — **X-34 QA 1차 픽스(사용자 QA —
> 스크린샷)**: New 템플릿이 서브메뉴가 아닌 **주 메뉴에 평탄 삽입**되던
> 결함 — 진범 = 전 핸들러 브로드캐스트(주 메뉴 `WM_INITMENUPOPUP`이
> CNewMenu로 전달) → `MenuHost` 소유 서브메뉴 핸들·명령 대역 **선별
> 라우팅** 전환. 66 green·clippy 0. **release 재빌드 필요**(실행 중 exe
> 잠금으로 보류 — QA 재개 전 앱 종료 후 빌드).
> [journal/2026-07-27.md](journal/2026-07-27.md).
>
> **직전(07-27 5차)** — **항목 우클릭 "새로 만들기" +
> 생성 후 인라인 리네임(사용자 요청 — `feat/ctx-new-menu` 1커밋,
> 실기 QA 후 병합 대기)**: 셸은 New를 배경 메뉴에만 제공 →
> **CLSID_NewMenu 직접 호스팅**(전용 대역 0x7000·포워딩 ACTIVE Vec)으로
> 항목 메뉴 하단에 배경 메뉴와 동일한 전체 ShellNew 템플릿 서브메뉴 병합.
> 단일 선택만(파일=부모·폴더=자신·다중 숨김 — 사용자 확정)·라벨 앱 언어
> `ctx.new`·설정 row `new` 키. **생성 후 리네임**: invoke 전후 폴더
> diff(정확히 1개)로 `Outcome::Created` → 캐럿·begin_rename
> (`focus_created_and_rename` 공용화·접힌 폴더는 hover_expand·배경 메뉴
> 휴리스틱 포함). **217 green·clippy 0·exe 3.34MB**. **QA 대기**: 항목
> New 템플릿 생성·리네임 진입·다중 숨김·설정 숨김.
> [journal/2026-07-27.md](journal/2026-07-27.md).
>
> **직전(07-27 4차)** — **로컬 브랜치 정리(사용자 지시 —
> 확인 후 선별)**: `feat/md-preview` **삭제**(07-26 폐기 지시분 ·
> md/mermaid 자산은 wasm 샘플로 이식 완료 = 중복 · tip `6f47512` 기록) /
> `feat/x13-launcher-crud` **보존**(`launcherN.icon` 직렬화가 **main
> 미반영 고유 작업**이고 TODO X-13이 참조 — 삭제 시 유실. X-13 2/2 착수
> 시 rebase 후 병합). BRANCHES 상단에 보존 사유·삭제 SHA 명기.
> [journal/2026-07-27.md](journal/2026-07-27.md).
>
> **직전(07-27 3차)** — **플러그인 개발 문서 정리·가이드
> 보강(사용자 요청 — main 직커밋)**: 체계는 갖춰져 있었으나 가이드가
> 압축돼 **최소 예제·적용 절차가 부재**한 것을 확인하고 보강 —
> [24](24-plugin-dev-guide.md) 116→193줄: **§1 프로젝트 구성**(골격·
> Cargo 필수 4항목) · **§1-2 최소 예제 전체 코드**(19KB `.log` 뷰어 —
> **컴파일 + wasmi 런타임 동작 검증 후 수록**) · §2 ABI export 표 ·
> **§4 빌드→적용→확인 3단**(산출물 명명·파일명 = 로드 순서·포터블/
> 설치형 복사 위치·설정 창 체크·`preview_map`·오류 1줄 격리).
> 코드 무변경·트리 clean. [journal/2026-07-27.md](journal/2026-07-27.md).
>
> **직전(07-27 2차)** — **`.star` → `.wasm` 표기 정정(사용자
> QA — main 직커밋)**: 런타임 전환 후에도 **설정 창 문구**가 `.star`로
> 남아 있던 것을 포함해 현행 표면 전체 정정 — lang 3종
> (`pref.plugins.desc/empty` = `data\plugins\*.wasm`) · **wiki
> 개발-플러그인 wasmi 판 재작성**(개념·왜 WASM·계약 요약·설정·빌드/배포·
> 로드맵) · 기능-설정 · 설계-결정 · ADR-0004 상단 "구판" 명시.
> 과거 기록(journal/DEVLOG/BRANCHES)은 규약대로 보존. **217 green·
> clippy 0·exe 3.18MB**. [journal/2026-07-27.md](journal/2026-07-27.md).
>
> **직전(07-27 1차)** — **플러그인 런타임 Starlark→wasmi 전환
> 완료(사용자 결정 — `feat/wasmi-plugin` 6커밋, 실기 QA 후 병합 대기)**:
> ① 부분 교체(1안) — Starlark 코어만 제거(**git revert로 원복 가능**),
> 시임·독립 창·설정·격리·라인 태그 계약 유지 ② **[ADR-0005]
> (25-adr-0005-wasm-plugins.md)** — wasmi 채택(wasmtime JIT = B2 초과
> 기각·`.wasm` 단일 아티팩트 = 크로스플랫폼 정합) ③ **wasmi 1.1 런타임**
> (nx_meta/nx_preview ABI·read_text/render_svg/is_dark import·fuel 2억
> [무한 루프 트랩 실증]·메모리 64MB) — **B2 실측 +1.68MB(1.50→3.18MB,
> starlark 대비 -2.58)** ④ **markdown-viewer-wasm 참조 플러그인**(러스트
> 크레이트 → 80KB .wasm — GitHub 태그 렌더·<br/>·표·Mermaid flowchart
> SVG 이미지/sequence 아트) + dist 동봉 실런타임 E2E ⑤ 가이드 24 wasmi 판
> 개정. **217 green·clippy 0·exe 3.18MB(B2)**.
> [journal/2026-07-26.md](journal/2026-07-26.md).
>
> **직전(07-26 4차)** — **X-2 마감 배치: Mermaid 이미지 수준
> 렌더·실행 격리·QA 픽스 → main 병합·push(사용자 지시)**:
> ① **Mermaid 이미지 렌더(A안)** — 호스트 `render_svg`(svg.rs 서브셋 →
> GDI+ AA 래스터 → BMP 내용 해시 캐시)·`is_dark()` + 인라인 이미지 마커
> (`\u{1}img|`+pad — 도크/독립 창 렌더·복사 제외). markdown.star flowchart
> = 테마 연동 SVG(라운드 노드·화살촉·간선 라벨), 실패 = 텍스트 폴백.
> ② **실행 격리 상한**(사용자 요구 — 지연 미리보기의 프로세스 영향 차단):
> 실행 300ms·로드 500ms·연료 5천만 틱·힙 64MB — 초과 = 플러그인만 오류
> 1줄. ③ ↗ 오버레이 = **이미지 버튼**(emb:popout SVG·hover sel_bg·pressed
> accent 38%+1px·안 릴리스 발화) ④ **리네임 재클릭 1회 지연 결함 픽스**
> (사용자 QA — 편집 필드 안 클릭만 가드·취소 클릭은 시드만) ⑤ 검토 기록
> (Python md 라이브러리 = Starlark에서 import 불가·sosomlab-nexa-viewer =
> 웹뷰 스택이라 연동/GFM 이식 경로 권장). `feat/starlark-plugin` **17커밋
> main 병합(ff)·push**. **218 green·clippy 0·exe 5.76MB(B2)·B3 통과**.
> [journal/2026-07-26.md](journal/2026-07-26.md).
>
> **직전(07-26 3차)** — **미리보기 UX 6종(사용자 요청 —
> `feat/starlark-plugin` 계속)**: ① 도크 미리보기 **우상단 ↗ "크게"
> 오버레이**(클릭 = 독립 창 — 1회성 통지·히트 존 paint 캐시) ② 독립
> 미리보기 창 **모달 전환**(소유자 차단·자체 펌프) ③ **스크롤** — 도크
> 휠 세로(3줄/노치·내용 hover 시 파일 목록 대신 라우팅·선택/히트 절대
> 라인 재정렬) + 독립 창 세로/가로(최장 라인 실측 상한·Shift+휠·←→)
> ④ 독립 창 **드래그 문자 선택**(도크 규약·3분할 렌더·클릭만 = 해제)
> ⑤ **rich 복사** — CF_UNICODETEXT + 모노 RTF(Consolas) 동시 게시(표·
> 박스 드로잉 정렬 유지 — 도크 Ctrl+C도 승격) ⑥ **드래그 자동 스크롤**
> (도크 상/하 1행·창 상하좌우 + 50ms 타이머 연속). **218 green(+3)·
> clippy 0·exe 5.76MB**. [journal/2026-07-26.md](journal/2026-07-26.md).
>
> **직전(07-26 2차)** — **플러그인 목록·사용 여부 설정
> 페이지(사용자 요청 — `feat/starlark-plugin` 8커밋째)**: 설정 창 사이드바
> **플러그인** 카테고리 — 로드된 `.star` 목록을 `NAME (id) — 확장자`
> 체크박스로 나열(동적 목록 — `ID_PLUGIN_BASE` 2100으로 기존 "클릭 즉시
> harvest+apply" 경로 재사용). **체크 해제 = `plugins_disabled=id|…`**
> (settings 왕복·영속) → resolve에서 **preview_map 오버라이드·EXTS 선언
> 매치 모두 제외**(내장 `builtin.*` = 폴백 안전망 면역) → 도크 미리보기
> 즉시 갱신. 빈 목록 = `data\plugins` 안내 1줄. harvest는 플러그인
> 페이지에서만 값 재구성(타 페이지의 값 소거 예방).
> `preview::plugin_infos()` 신설(설정 UI 메타). 가이드 [24 §7-4]
> (24-plugin-dev-guide.md) 추가. **215 green(+1)·clippy 0·5.74MB 불변**.
> [journal/2026-07-26.md](journal/2026-07-26.md).
>
> **직전(07-26 1차)** — **X-2 Starlark 플러그인 시스템 +
> MarkdownViewerPlugin 샘플(사용자 지시 — `feat/starlark-plugin` 6커밋,
> 실기 QA 후 병합 대기)**: 1차 내장 러스트 md 뷰어(`feat/md-preview`)는
> 사용자 지시로 **폐기(미병합 보존)** → Starlark 기반 재설계.
> ① ADR-0004 **S1 시임** + **`preview_map` 설정 오버라이드**(우선순위 =
> 설정 > 스크립트 `EXTS` 내부 선언[파일명 순] > 내장 폴백 — 사용자 확정)
> ② **S2 `starlark` 0.14.2 런타임**(DR-8 원장 확정 — **B2 실측 +4.24MB**:
> 1.50→**5.74MB** ≤10 통과) — `data\plugins\*.star` 지연 로드·동결 캐시·
> **오류 격리**(로드 실패 = 파일만 제외·실행 오류 = 미리보기 1줄)·호스트
> API(`file.path/ext/size`·`read_text(n)` **대상 파일만** 256KB·
> `disp_width` CJK 2칸) ③ **독립 미리보기 창 F3**(모덜리스·콘솔 폰트
> 문자 그리드+스크롤 — **플러그인 개발 기준 캔버스**, 도크 = 축약 뷰·
> 벤치 = Shift+F3 이동) ④ **samples/markdown-viewer 독립 프로젝트**
> (markdown.star 참조 구현 — md 서브셋 + **Mermaid flowchart/sequence
> 텍스트 다이어그램** 순수 Starlark·와이드 2칸 격자) + 실제 런타임 E2E
> ⑤ **개발자 가이드 [24](24-plugin-dev-guide.md)** — 프로젝트 생성→계약→
> 구현→렌더 규칙→로컬/자동 테스트→배포(.star 1개)→운용 8단계 순차.
> 실측 **214 green(+5)·clippy 0·B3 통과**. 잔여: S3 공급자 콤보·핫
> 리로드·연료 상한·S4 exif.star.
> [journal/2026-07-26.md](journal/2026-07-26.md).
>
> **직전(07-24 5차)** — **문서 내용 정리(사용자 요청)**: 커밋
> 대기분이 없어 문서 자체의 규약 위반 3건을 정리. ① **이 문서(STATUS)가
> "한 장" 규약 이탈** — 차수 22건·본문 전 340줄(전체 382줄의 89%) →
> 07-22 이후 7건은 원문 유지하고 **07-21~07-15 15건은 하루 한 줄 색인으로
> 압축**(각 줄 journal 링크·원문은 journal에 그대로 = 정보 손실 없음),
> **382 → 175줄** ② **docs/README 색인 결손 7건 보충**(07·08·09 ADR·19·20·
> 21·22 — "ADR · 기능 설계" 하위 표 신설) ③ **CLAUDE.md 13일 노후화 정정**
> — §1 "M5 진행" → **포스트 M5(`0.11.0`)**, §7 "다음 단계(2026-07-11)"의
> M0/M1 → **07-24 기준**(실기 QA·심사 대기 3건·백로그·X-33)으로 교체 +
> "최신 현황은 항상 STATUS" 문구로 재노후화 예방. 코드 무변경.
> [journal/2026-07-24.md](journal/2026-07-24.md).
>
> **직전(07-24 4차)** — **macOS·Linux 확장 타당성 검토(사용자
> 요청)**: 실측 기반 검토서 [23](23-cross-platform-feasibility.md) 신설 —
> 전체 **40,041 LOC 중 중립 16,102(40.2%)** / Windows 결합 23,939(59.8%),
> **맥 네이티브 테스트 147 green** + `nexa-app` 스텁 빌드 성공(M0 cfg 격리
> 골격 생존). ① **화면 로직은 이미 중립** — `nexa-gui/widgets` 5,519 LOC가
> `DrawCtx` ~10메서드에만 의존 ⇒ 플랫폼 백엔드 1개로 주 화면 렌더 가능 가설
> ② **최대 비용 = `ctl` 18종 7,139 LOC**(HWND 골격 종속) + OS 통합 3K
> (셸 컨텍스트 메뉴는 **등가물 없음 = 기능 상실**, ConPTY→`forkpty`는 단순화,
> MDL2는 중립 SVG 파서로 대체 가능) ③ **진짜 관문 = DR-1·DR-2·DR-8**
> ("Windows 인박스 API 존재" 전제 — **Linux는 문안 그대로면 성립 불가**,
> ADR-0005 개정 필요). 선택지 4종 중 **C(winit+softbuffer+cosmic-text 위
> 커스텀 드로잉 유지 — 전부 퍼미시브)** 가 현실적 후보. **권고 = 지금 결정
> 말고 맥 렌더 스파이크 먼저**(M0-7 방식). 사용자 결정 5건 = 검토서 §7.
> [TODO](TODO.md) **X-33 등록**(📐 착수 대기) + X-32 누락분 소급 등록.
> 코드 무변경. [journal/2026-07-24.md](journal/2026-07-24.md).
>
> **직전(07-24 3차)** — **배포 채널 상태 점검 — winget·Chocolatey
> 4항목 실측 최신화(사용자 요청)**: 원천 조회(PR 라벨 타임라인·master 매니페스트
> raw·choco 모더레이션 로그)로 재확인 → **정정 2건**. ① **winget Portable
> `0.11.0`(#405973)은 이미 MERGED**(07-22 18:44 — 문서엔 "OPEN·검증 대기"로
> 남아 있었음) ⇒ **포터블 = 0.8.1·0.11.0 매니페스트 상주, 최신 버전까지 배포된
> 유일한 채널** ② **설치형(#404528) 병목 = `Policy-Test-1.2` waiver 미부여**
> (포터블은 07-20 22:57 `Waived-Policy-Test-1.2` 부여 후 승인 — 설치형은 플래그
> 잔존·07-19 13:46 이후 무변동·재실행 권한 없어 **우리 측 조치 수단 없음**).
> **choco 2종** = 자동 3단계 완료(scan `Flagged Note` = 차단 아님) 후 **07-20
> 02:2x 이후 4일 무변동**, 0.8.1 고정(보류 방침 정상 작동 — 변경 없음).
> **채널 현황**: winget Portable ✅ **0.11.0 배포** · winget 설치형 ⏳ waiver 대기 ·
> choco 2종 ⏳ 모더레이션 · GitHub Release ✅ 0.11.0.
> [21 §7·§8](21-distribution.md)에 상태 표 2종 신설. 코드 무변경.
> [journal/2026-07-24.md](journal/2026-07-24.md).
>
> **직전(07-24 2차)** — **저장소 최신화 — 로컬 브랜치 정리 +
> BRANCHES 이력 보정(사용자 요청)**: 동기화 실측 = `main` ↔ `origin/main`
> **0 ahead / 0 behind**(HEAD `5eba31f`) · 태그 `0.1.0`~`0.11.0` **12종
> 로컬·원격 일치** · 트리 clean → **이미 최신**. 잔여물은 머지 완료 후
> 남은 **로컬 전용 브랜치 3개**(`feat/chocolatey-packaging`·
> `feat/m0-render-spike`·`feat/m0-scaffold`) → `git branch -d` 안전 삭제.
> 이 과정에서 [BRANCHES](BRANCHES.md) 어긋남 2건 보정: **chocolatey 행
> 누락 소급 기록**(07-19 merge `0743472`) + M0 2건 **삭제 열 정정**
> (`07-11`→`07-24(정정)` — ref가 오늘까지 잔존) + **"삭제 열 = 로컬 ref를
> 실제로 지운 날" 정의 명문화**(재발 방지). 코드 무변경.
> [journal/2026-07-24.md](journal/2026-07-24.md).
>
> **직전(07-24 1차)** — **문서·커밋/푸시 규약 문서화(사용자 요청 —
> 타 프로젝트 이식용)**: 지금까지 암묵 운영하던 기록·git 기준을
> [16 문서·커밋/푸시 규약](16-doc-git-conventions.md)으로 고정 —
> **§0 붙여넣기용 지시문 블록**(새 프로젝트 세션에 그대로 투입) · 문서
> **4층 체계**(진입/현황/경과/지식 — 시간·목적·상태 3축 분리, 상세는
> journal 한 곳·나머지는 요약+링크) · 작성 필수 규칙 8(**한 작업 = 한
> 트랜잭션 갱신**·SSOT 동시 갱신·"왜+실측값"·문서 번호 불변) · 커밋
> (Conventional + 맥락 태그) · **🔴 push는 명시 요청 시에만·태그 push
> 별도 승인·파괴적 작업 사전 확인** · 릴리스 4단계 · 새 프로젝트 적용
> 체크리스트. 배선: docs/README ④ 색인 · CLAUDE.md §5(규약 SSOT 명시) ·
> [15](15-dev-methodology.md)(개발 규율 ↔ 기록/git 규약 분담).
> 코드 무변경. [journal/2026-07-24.md](journal/2026-07-24.md).
>
> **직전(07-22 3차)** — **winget Portable `0.11.0` 배포(사용자
> 요청)**: 채널 재점검에서 winget **Portable(`SosomLab.NexaDir.Portable`)은
> 0.8.1이 이미 병합·배포 완료**(PR #404533 MERGED 07-21 — 기존 문서 OPEN
> 표기 정정)임을 확인 → 최신 릴리스 **0.11.0**으로 버전 업데이트 매니페스트
> 3종(`packaging/winget/portable/0.11.0/` — SHA-256 `523D864F…02772`·locale
> 언어 en/ko/**ja** 정확화) 제출: microsoft/winget-pkgs **PR #405973** OPEN
> (검증 대기). 이미 승인된 패키지의 버전 업데이트라 choco 보류 방침 무관.
> **채널 현황**: winget Portable = **0.8.1 배포 완료 · 0.11.0 심사 중** ·
> winget Setup(#404528) OPEN · choco `nexa-dir`·`nexa-dir.portable` = 둘 다
> **미승인(awaiting moderation)**. **Wiki 설치 페이지 신설**([설치와-다운로드]
> — 포터블 winget 안내·GitHub Release 다운로드, `ce22af5` 발행)·배포 문서 §8
> 이력 갱신. [journal/2026-07-22.md](journal/2026-07-22.md).
>
> **직전(07-22 2차)** — **릴리스 `0.11.0`(GitHub 전용) —
> 컬럼 리사이즈 단독 조절 + Ctrl+V 대상 보완(사용자 QA)**:
> ① 컬럼 경계 드래그 = **해당 컬럼만** 변경(이웃 불변·총폭 가변 —
> 한 쌍 동시 조절[QA 07-15] 폐지, 초과분 = 가로 스크롤) ② Ctrl+V —
> 파일 1개 선택 시 **그 파일이 있는 폴더**로 붙여넣기(폴더=그 안·
> 없음/다중=현재 폴더). 07-22 "미동작" 재현 = 구버전 exe 원인(재빌드
> 해소). 위키(목록/파일 조작/단축키) 동기·발행. choco push 스위치
> 꺼짐 유지. [journal/2026-07-22.md](journal/2026-07-22.md).
>
> **직전(07-22 1차)** — **X-32 클립보드·DnD UX 4종 병합 +
> 릴리스 `0.10.0`(GitHub 전용 — 사용자 요청)**:
> `feat/clipboard-dnd-ux` main 병합(ff `ec8d727`) — ① Ctrl+X 잘라내기
> **흐림 표시**(OS 클립보드 미러·WM_CLIPBOARDUPDATE 단일 길목 — 외부
> 탐색기 잘라내기 포함) ② Ctrl+V **선택 폴더 안 붙여넣기**(폴더 1개
> 선택 시) ③ DnD **엣지 자동 스크롤** ④ DnD **호버 3초 = 탭
> 전환/접힌 폴더 펼침**(설정 `dnd_hover_ms` 기본 3000·200~10000 —
> prefs "파일 전송"·lang 3종). 209 green·clippy 0·release exe 1.50MB.
> **위키 반영**(파일 조작·설정·단축키 + 내장 3언어 표기)·choco push는
> 스위치 꺼짐 유지(GitHub Release만). 태그 `0.10.0` push → Actions.
> [journal/2026-07-22.md](journal/2026-07-22.md).
>
> ---
>
> **이전 이력 요약 (07-21~07-15)** — 상세는 [DEVLOG](DEVLOG.md)와 각 일자
> [journal/](journal/). 아래는 하루 한 줄 색인이며, **원문은 삭제되지 않고
> journal에 그대로 있다**(STATUS = "지금 상태 한 장" 규약 복원 — [16 §1](16-doc-git-conventions.md)).
>
> - **07-21** — 릴리스 **`0.9.0` 배포 완료**(보류 방침을 choco push만으로 축소·
>   `vars.CHOCO_PUSH` 스위치 도입) · X-31 **일본어 내장 언어팩**(en·ko·ja) ·
>   **X-30 전송 진행 UX 개편** main 병합(세그먼트 바·`transfer_close_ms`·전송 중 잠금)
>   + QA 3건(DEL 휴지통 워커 이관·최소 3px·5색 팔레트) · 배포 채널 실측 점검 ·
>   배포 보류 방침 확정. [journal/2026-07-21](journal/2026-07-21.md)
> - **07-20** — **X-28 탭 바 UX**(우클릭 New Tab·멀티라인 탭·상하 드래그) ·
>   **X-29 도크 텍스트 문자 단위 선택 복사** · X-27 ✅(툴바 hover 통일) ·
>   X-25 1차(`Conflict::Nested`) · **X-26 ③ About 창** · 백로그 3건 등재.
>   exe 1.45MB. [journal/2026-07-20](journal/2026-07-20.md)
> - **07-19** — **툴바 전면 개편**(아이콘 9종 **SVG 단일 파이프라인**·툴팁 i18n) ·
>   **순서/표시 편집 공통 창**(ordereditor + NxOrderTree) · 컬럼 드래그 재배열·
>   auto-fit · 하단 도크 툴바 버튼 · **UI 용어 체계 확정** · 릴리스 **`0.7.0`·`0.8.0`·
>   `0.8.1`** · **GitHub Wiki 신설**(19페이지) · 다크모드 아이콘·exe 리소스 아이콘 ·
>   **배포 채널 등록**(Chocolatey 2종·winget 2종) · 제품명 "2" 제거.
>   [journal/2026-07-19](journal/2026-07-19.md)
> - **07-18** — **NxGrid(ctl 14호)** 신설(오버레이 스크롤바·행 선택 규약·GridOpts) ·
>   **다중열 정렬**(원본 docs/24 §4 이식) · **Segoe MDL2 글리프 통일** ·
>   **ctl base.rs 재사용 리팩터 + 전수 검토**(잠재 크래시 1건 수정) · 카드 스크롤 뷰포트 ·
>   **컬럼 넓이 동기화** 설정 · ctl 판매 매뉴얼(13문서). 188 green.
>   [journal/2026-07-18](journal/2026-07-18.md)
> - **07-17** — **X-22 일괄 이름 변경 v2**(PF 6동작 패리티 — 코어 재작성·Date 엔진) ·
>   X-21 `shell:` 특수 폴더 · **Nx 컨트롤 세트 완성**(GroupCard·ComboBox·CheckBox·
>   TextBox·IconButton·Button·Segmented·Spin) · **렌더 규약 개정**(AA = DrawCtx
>   백엔드만 · GdipCtx = 유일 GDI+ 접점). [journal/2026-07-17](journal/2026-07-17.md)
> - **07-16** — 대규모 하루(병합 브랜치 14개·등재 4건): **X-16 최적화 1차** ·
>   **X-18 배포 2채널**(DR-3 개정 — 포터블+설치형) · **X-17 가상 최상위 "내 PC"** ·
>   **X-19 보기 모드 3종**(타일/일반/트리) · **X-12 폰트 슬롯** ·
>   **ctl 커스텀 컨트롤 라이브러리 신설**(searchbox·fontbox) ·
>   **X-20 싱글/듀얼 패널·정보 모드**. [journal/2026-07-16](journal/2026-07-16.md)
> - **07-15** — **M5 마감 → `0.6.0` 릴리스 발행**(첫 태그 실행 검증) ·
>   **설정 창 전면 개편 3연타**(계층 트리·검색 필터·드릴다운) · M5-1 보완(런처 바·
>   일괄 이름변경 파이프라인) · **원본 패리티 갭 문서화**([19](19-parity-gap.md)) ·
>   세션 디바운스 자동 저장([20](20-session-coalescing.md)) · 쉬운 갭 4건.
>   [journal/2026-07-15](journal/2026-07-15.md)
>
## 1. 확정된 결정 ([10](10-decision-record.md))

| # | 영역 | 결정 |
| --- | --- | --- |
| DR-1 | 스택 | **올 러스트 단일 바이너리** — Win32(windows-rs)+커스텀 드로잉 · ADR-0001 Accepted |
| DR-2 | 예산 | 유휴 RSS ≤30MB · exe ≤10MB · 임포트=OS 인박스만 — **병합 게이트** |
| DR-3 | 배포 | **개정(07-16)**: 포터블 단일 exe **기본** + 설치형 exe **보조** 2채널(`data\` 영속·쓰기 불가 시 LOCALAPPDATA 폴백 — [21](21-distribution.md)) |
| DR-4 | 코어 | 원본 nexa-core/vfs/tree **rlib 이식**(FFI 폐지) |
| DR-5 | UX | 원본 M1 기능 패리티·디자인 규약 계승 |
| DR-6 | 라이선스 | PolyForm NC + 의존성 퍼미시브 온리 |
| DR-7 | 플러그인 | .NET SDK 비이관 — 내장 미리보기 대체 |
| DR-8 | 외부 crate | 기본 0 지향, 건별 원장 기록(`windows` 승인) |

## 2. 예산 실측 현황 (DR-2)

| 항목 | 예산 | 최신 실측 | 시점 |
| --- | --- | --- | --- |
| B1 유휴 RSS | ≤30MB | **16.86MB**(중앙값, 3회 18/16.86/4.12 — **M5 마감 실측**: 10k·도크 정보 뷰·런처 바·유휴 300s. 활성 ~36→트림 직후 2.6MB 후 재상승 편차 큼 — 최저 4.12는 M4 수준, 재상승 원인 관찰은 β arena 회수와 공동 과제). M4 터미널 상주 실측 5.07MB(07-14) | 07-15 실기 |
| B2 exe 크기 | ≤10MB | **1.45MB** (X-28 멀티라인 탭·X-29 도크 선택·About 창 포함 — 0.8.1 1.43MB 대비 +0.02) | 07-20 실기 |
| P1 100k 첫 렌더 | <150ms | **115ms**(중앙값, 열거 포함 — 10k는 42ms) | 07-12 실기 |
| P2 스크롤 | 60fps(<16.7ms) | **2.1ms/프레임**(100k·200프레임 벤치) | 07-12 실기 |
| B3 임포트 DLL | OS 인박스만 | **통과** — 기존 + dwrite·combase·ole32·bcryptprimitives·shell32 (`scripts/budget-b3.ps1` 단일 출처) | 07-13 실기 |

## 3. 마일스톤 (상세 [MILESTONES](MILESTONES.md))

- **M0** 기반·게이트 ✅ (`0.1.0`) — 설계 문서·스캐폴딩·코어 3크레이트 이식·Win32 창·렌더 스파이크·CI·게이트 실측.
- **M1** 뷰어(★플래그십) ✅ (`0.2.0`) — 전 항목 완료 + 게이트 통과(100k 115ms·60fps·RSS).
- **M2** 셸 골격 ✅ (`0.3.0`) — 경로 바·듀얼/탭·크롬·테마·설정/세션·i18n·IME/UIA 1차·상주 규율(게이트: 듀얼·탭4 26.9MB ≤30).
- **M3** 파일 조작 ✅ (`0.4.0`) — 전송·삭제/이름변경/새로 만들기·Undo/Redo(휴지통 복원)·셸 컨텍스트 메뉴·OS 클립보드/OLE DnD·watcher + 탐색기 클릭/편집 시맨틱(게이트: 10k 유휴 300s **6.29MB** ≤30).
- **M4** 하단 패널 ✅ (`0.5.0`) — 도크·정보 뷰·미리보기·ConPTY 터미널(+상호작용 QA 시리즈)·프리즈 2건 근본 해소(게이트: 10k+터미널 상주 유휴 300s **5.07MB** ≤30).
- **M5** 마감 ✅ (`0.6.0` 발행) — M5-1(퀵 런처 바·일괄 이름변경 α+파이프라인 확장) · M5-2(릴리스 파이프라인 — 첫 태그 실행 검증 완료) · M5-3(UIA/IME 마감·서명=무서명 확정). 실기 QA 잔여.

## 4. 개발 모델 ([11](11-dev-environment.md))

- 맥 = 일상 개발(코어 test + **windows 타깃 cargo check로 UI 코드까지 타입 검증**) · Windows PC/CI = 실행·QA·예산 실측.

## 5. 다음 단계

1. ~~M0~~ ✅ `0.1.0` · ~~M1~~ ✅ `0.2.0` · ~~M2~~ ✅ `0.3.0` · ~~M3~~ ✅ `0.4.0` · ~~M4~~ ✅ `0.5.0`.
2. ~~M5 마감~~ ✅(07-15) — ~~`0.6.0` 태그·M5-2 첫 실행 검증~~ ✅(Release 발행·exe 첨부 확인). 실기 QA 잔여.
3. 백로그: **X-11 원본 패리티 갭 건별**([19](19-parity-gap.md) — G-7·12·13 해소) · X-2 Starlark 플러그인(**S1~S3 1차 + markdown.star 샘플 ✅ 07-26** — `feat/starlark-plugin` QA 후 병합. 잔여: 공급자 콤보·핫 리로드·연료 상한·S4 exif) · ~~X-3~~ ✅(07-15) · ~~X-10~~ ✅(07-15 — 기본값 복원만 잔여) · X-1 Apps 키 QA.
