# STATUS — Nexa Dir 진행 현황

> **갱신: 2026-09-02 (KST)** — **채널 마감: Chocolatey 2종 승인(0.8.1 — 07-20 플래그 후 44일)
> → `0.18.1` 두 패키지 제출·검수 중**(빌드 없음 = 태그 이후 코드 무변 · `resubmit-chocolatey`
> dispatch로 main의 모더레이터 반영분 사용 · winget 2채널은 `0.18.1` 병합 완료로 제출 대상 아님 —
> [journal/2026-09-02.md](journal/2026-09-02.md)). 직전 = **X-46 압축 파일 미리보기 + 플러그인 동봉 배포 →
> 릴리스 `0.18.0`·`0.18.1`**(0.18.1 = Defender ML 오탐 대응 설치형 VERSIONINFO 보강 재배포.
> 상세 [journal/2026-08-24.md](journal/2026-08-24.md) — **실기 QA 대기**):
>
> - **압축을 풀지 않고 목록만 읽는 계층 신설**([nexa-vfs/archive](../crates/nexa-vfs/src/archive/)) —
>   거의 모든 포맷이 항목 표를 평문으로 갖고 있다는 사실이 설계를 결정했다. 포맷
>   레지스트리 = **새 포맷은 파일 1개 + 한 줄**(확장자·판정·라우팅·컬럼은 파생).
>   내장 = ZIP(Zip64·AES/ZipCrypto 표시·SFX 델타 보정·중앙 디렉터리 암호화 = 암호 요구·
>   26개 확장자) · TAR(ustar/GNU/PAX) · CAB · RAR 5/4 · 7z(헤더가 LZMA → **플러그인 안내**) ·
>   단일 스트림(gz/bz2/xz/zst/lz4/lz/z). 경로 탈출(zip slip) 정규화 + 위험 표시
> - **표시 2종** — 하단 도크 = 요약(포맷·개수·크기·절감률·암호/솔리드/분할 + 앞 60개) ·
>   **별도 그리드 창**([archivewnd](../crates/nexa-app/src/archivewnd.rs) — **NxGrid 재사용**으로
>   파일 그리드 규약 계승: 8열·헤더 정렬(크기·시각·압축률은 수치 기준)·다중 선택·
>   `Ctrl+C` TSV 복사·`Esc`). F3·도크 ↗가 압축이면 이 창으로 라우팅(재조회 없음).
>   **시각 이중 보정 차단**(DOS 시각 = 현지 벽시계 그대로·Unix epoch만 보정) ·
>   구형 zip CP949 이름 디코더 주입
> - **암호는 구조로 강제**([Secret](../crates/nexa-core/src/secret.rs)) — `Debug`는
>   `Secret(***)`(길이도 비노출)·`Display`/직렬화 없음·Drop `write_volatile` 소거 ·
>   마스킹 입력 모달([pwprompt](../crates/nexa-app/src/pwprompt.rs))이 회수 즉시 이동 +
>   경유 UTF-16·EDIT 내용·되돌리기 버퍼 소거 · **저장 경로가 코드에 아예 없다**
>   (세션 메모리 한정 — 토큰용 DPAPI 경로와 의도적 분리) · 틀린 암호는 폐기 후 재시도
> - **플러그인 ABI v2**(하위 호환) — `nx_meta` 4번째 줄 = 능력 선언(`archive`) ·
>   `nx_archive()` · 임포트 `file_size`/`read_at`/`password`(활성 암호만·없으면 -1).
>   참조 구현 [samples/archive-viewer-wasm](../samples/archive-viewer-wasm/) =
>   **ISO 9660(Joliet)·ar·cpio를 31KB `.wasm` 하나**로(앱 재빌드 없이 포맷 확장 =
>   "별도 개발 후 최종 파일만 배포") · 설계 SSOT [28](28-archive-preview.md) ·
>   가이드 [24 §3-1](24-plugin-dev-guide.md)
> - **동봉 플러그인 배포 신설**(사용자 지시 "빌드해서 markdown.wasm과 함께 배포") —
>   착수 실측에서 **릴리스 파이프라인에 `wasm`이 한 번도 없었다**는 사실 확인(플러그인은
>   저장소에만 있고 배포된 적 없음) → ① 빌드 단일 출처 [scripts/build-plugins.ps1](../scripts/build-plugins.ps1)
>   + **CI 편입**(워크스페이스 밖 크레이트라 안 건드리면 릴리스 당일 첫 실패) ②
>   **탐색 경로 2원화**(`data\plugins` 사용자 설치분 → `<exe>\plugins` 동봉분·같은 id는
>   사용자분 우선) ③ 포터블 zip 동봉 · 설치형 `{app}\plugins` · **`NexaDir-Plugins-<ver>.zip`
>   신규 자산**(단일 exe 자산은 최소파일 규율 유지) ④ 설명 문서 6곳(18 §3-1 빌드 SSOT ·
>   21 §5-2 배포 형태 · 24 §4-2 두 경로/수정 절차 · README · 위키 2쪽 · 샘플 README)
> - 게이트: 워크스페이스 **320 green**(X-46 신규 44 + 동봉 배포 1 — 앱 127) · clippy 0 ·
>   비Windows check · **B2 3.83MB** ≤10 · **B3 21종 무변**(신규 DLL 0) · 외부 crate 0(DR-8 유지)
> - **릴리스 `0.18.0` → `0.18.1`**: 0.18.0 = 압축 미리보기·플러그인 동봉 첫 배포(플러그인 zip 평면
>   구조를 실측 발견해 폴더째로 재포장·워크플로 수정). **`0.18.1`**(오탐 대응 재배포) =
>   [Release](https://github.com/SosomLab/nexa-dir2/releases/tag/0.18.1) 자산 6종 · **설치형
>   FileVersion=0.18.1 채워짐 실측**(0.18.0 공백 해소) · 플러그인 zip **폴더 구조 정상**(재포장 불요) ·
>   해시 6종 일치 · 설치형 온디맨드 clean · choco skipped
> - **채널**(제출 규칙 2·3회째): `0.18.0` PR 2건 **제출 ~40분 만에 병합**
>   ([#423258](https://github.com/microsoft/winget-pkgs/pull/423258)·[#423259](https://github.com/microsoft/winget-pkgs/pull/423259)) →
>   `0.18.1` 대기 0건 재확인 후 PR 2건 제출([#423330](https://github.com/microsoft/winget-pkgs/pull/423330)
>   Portable · [#423331](https://github.com/microsoft/winget-pkgs/pull/423331) 설치형) · choco는 0.8.1 잠김 유지로 제외
> - **사용자 문서 동일 트랜잭션**: 위키 [기능-압축-미리보기](wiki/기능-압축-미리보기.md) 신설 +
>   6쪽 갱신(사이드바·개요·하단 도크·빌드와 테스트·설치와 다운로드·Home/개발 여정 버전) + 루트 README
> - **Defender ML 오탐(설치형) — 해소 확인**(사용자 보고 → 재배포 → 실기 확증): 0.17.0/0.18.0
>   설치형이 다운로드 시 `Wacatac.*!ml`로 격리되던 것(포터블·wasm은 무관·온디맨드는 clean =
>   **무서명 + 새 해시 + 프리밸런스 0**의 클라우드 ML 판정)을 규명 → **Inno 설치형의 빈 파일 버전을
>   발견해 VERSIONINFO 보강**(`0.18.1`) → **다운로드 시 더 이상 격리되지 않음**(사용자 실기 + 재다운로드
>   실측). 압축 완화는 실측으로 기각([12 §4-2] 표). 오탐 신고 절차도 상설화
>   ([packaging/av-false-positive.md](../packaging/av-false-positive.md)·[12 §4-2](12-packaging-single-exe.md)·
>   [21 §6](21-distribution.md) 체크리스트). **이번 회차 종결**(근본 평판은 서명/Store 과제로 잔존)
>
> ---
>
> **이전 이력 요약 (08-23 ~ 07-15)** — 상세는 [DEVLOG](DEVLOG.md)와 각 일자
> [journal/](journal/). 아래는 하루 한 줄 색인이며, **원문은 삭제되지 않고
> journal에 그대로 있다**(STATUS = "지금 상태 한 장" 규약 — [16 §1](16-doc-git-conventions.md)).
>
> - **08-23** — **X-42 가상 파일 붙여넣기 1·2차 + X-43 빈 폴더 글리프 + X-44 간헐
>   무갱신 1~5차 + X-45 항상 맨 위 → 릴리스 `0.17.0`**: 원격(RDP)·Outlook·zip 내부 복사분이
>   안 붙던 결함을 가상 파일 2종 폴백으로 해소(워커화·undo·DnD·클라우드) · 빈 폴더 ▸ 억제 ·
>   "간혹 F5" 전수 조사 → **OneDrive 플레이스홀더 생성은 mtime도 통지도 안 남긴다** 실측 확정 →
>   **셸 변경 통지 구독**(이벤트 1선 + 감속 스윕 2선) · 항상 맨 위 토글 · winget 2건 제출
>   (채널 제출 규칙 첫 적용). [journal/2026-08-23](journal/2026-08-23.md)
> - **08-11** — **X-40 자동 갱신 + X-41 바 크기 → 릴리스 `0.15.0`·`0.16.0` + winget 2건
>   제출 + 위키 발행**: 갱신 계기가 RDCW 통지 하나뿐이던 것을 [fsprobe.rs](../crates/nexa-app/src/fsprobe.rs)
>   (mtime 프로브)+활성화 갱신+3s 폴링(창 활성 시에만)으로 해소 · 바 크기 실물 QA 3라운드
>   확정(도구 모음 28/퀵 런처 24 — 두 바 높이 분리) · winget `0.16.0` PR 2건 제출
>   (0.15.0은 건너뜀 — PR 겹침 회피) · clippy 게이트 복구·MB/MiB 정정·위키 5파일 발행.
>   [journal/2026-08-11](journal/2026-08-11.md)
> - **08-02(5~6차)** — **기능의 날 + 릴리스 `0.14.0`**: 7-Zip **지연 렌더링 드롭** 픽스 2건
>   (X-38 — Drop 재조회 + 같은 볼륨 rename 스테이징 확보) · 폴더 우선 정렬 토글(시안 D) ·
>   **보기 옵션 적용 범위 3택**(X-39 — 값의 SSOT를 탭으로) · **패널 간 탭 DnD**("미리 보기 =
>   실이동"·ESC 무손실 복귀) · `0.14.0` 배포 + 채널 재점검. [journal/2026-08-02](journal/2026-08-02.md)
> - **08-02(1~4차)** — **클라우드 매뉴얼·브랜딩 + 3사 배포 한도 점검**(1차 — [기능-클라우드](wiki/기능-클라우드.md)
>   신설·잘못된 리디렉션 안내 정정·[ADR-0006 §2-4-1](27-adr-0006-cloud-oauth.md)) · **릴리스 `0.13.0`
>   + winget Portable #410978 + 비Windows CI 복구 2건**(2차 — core 잡이 07-27부터 붉던 것 발견·
>   [18](18-build-and-test.md) 절차 보완) · **문서 내용 정리 + 진행사항 최신화**(3차 — STATUS 565→161줄·
>   DR-7 정정·예산 표 갱신). · **사용자 대상 문서 정리 2차 — 루트 README·위키 + 11일치 발행**(4차 — B3 21종 실측·설치 절 신설·기능-클라우드 222줄 발행). [journal/2026-08-02](journal/2026-08-02.md)
> - **08-01** — **클라우드 하루**(8차까지 — `feat/x36-cloud-connections` 22커밋):
>   [26 검토서](26-cloud-integration-study.md) 신설·2단계 분리 권고 → **X-36 Link**(동기화 폴더 탐지·
>   `::PC::` 클라우드 섹션·Cloud 메뉴) → **X-37 Connect**([ADR-0006](27-adr-0006-cloud-oauth.md) 신설·
>   **DR-2 개정 B3 +4**·PKCE 루프백·DPAPI) → 탐색·다운로드·쓰기·3사 완성 →
>   **실사용 QA 17건 전부 진짜 결함**(Google `client_secret`·로딩 무한 5건·계정 간 복사·
>   Dropbox 5건) · client_id **하이브리드 모델 정정**(사용자 지적) · winget 설치형 #404528
>   피드백 처리. **254 green·3.44MB(B2)**. [journal/2026-08-01](journal/2026-08-01.md)
> - **07-31** — **안정성·배포 하루**: 전 소스 **안정성 감사 7건**(watcher 죽음 = OneDrive 간헐
>   무갱신의 유력 원인·conpty 핸들 경쟁·통지 유실 고착·panic 후크) · 포그라운드 양도
>   (연 프로그램이 뒤에 숨던 결함) · **Release ZIP 자산 + SHA256SUMS**(SmartScreen 다운로드
>   차단 완화) · [12 §4-1](12-packaging-single-exe.md) **서명 경로 재조사**(EV 즉시 통과 2024 폐지·
>   Azure 한국 불가 → 완전 해소는 MS Store MSIX뿐) · 3건 main 병합·push · winget Portable
>   0.12.0 **MERGED** 확인. [journal/2026-07-31](journal/2026-07-31.md)
> - **07-27** — **릴리스 `0.12.0`** + winget Portable PR #408280 · **X-35 휴지통 삭제 실패
>   무통지 해소**(사전 잠금 프로브 + `exists()` diff 백스톱·부분 undo·펼친 하위 폴더 감시로
>   M3-6 α 해소) · **X-34 항목 우클릭 "새로 만들기"**(CLSID_NewMenu 호스팅·생성 후 인라인
>   리네임) · `.star`→`.wasm` 표기 정정 · 플러그인 가이드 [24](24-plugin-dev-guide.md) 보강 ·
>   로컬 브랜치 정리. [journal/2026-07-27](journal/2026-07-27.md)
> - **07-26** — **X-2 플러그인 하루**: Starlark 시스템 구축(시임·preview_map·독립 미리보기 창
>   F3·markdown.star 샘플) → 미리보기 UX 6종 → Mermaid **이미지 수준 렌더**·실행 격리 상한 →
>   **런타임 wasmi 전환 결정·구현**([ADR-0005](25-adr-0005-wasm-plugins.md) — `.wasm` 단일 아티팩트·
>   fuel 격리·**B2 5.74→3.18MB**). [journal/2026-07-26](journal/2026-07-26.md)
> - **07-24** — **문서 정리 하루**: STATUS "한 장" 복원(382→175줄) · docs/README 색인 결손 7건 ·
>   CLAUDE.md 13일 노후화 정정 · **[23 macOS·Linux 확장 검토서](23-cross-platform-feasibility.md)**
>   (중립 40.2%·`ctl` 18종이 최대 비용·**DR-1/2/8이 Linux에선 성립 불가** — 권고 = 맥 렌더 스파이크) ·
>   배포 채널 실측 정정 2건 · 저장소·브랜치 정리. [journal/2026-07-24](journal/2026-07-24.md)
> - **07-22** — 릴리스 **`0.10.0`·`0.11.0`**(GitHub 전용) — **X-32 클립보드·DnD UX 4종**
>   (Ctrl+X 흐림·선택 대상 안 붙여넣기·엣지 자동 스크롤·호버 3초 전환) · 컬럼 리사이즈 단독 조절 ·
>   winget Portable `0.11.0` 제출 · **Wiki 설치 페이지 신설**. [journal/2026-07-22](journal/2026-07-22.md)
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

## 1. 확정된 결정 ([10](10-decision-record.md))

| # | 영역 | 결정 |
| --- | --- | --- |
| DR-1 | 스택 | **올 러스트 단일 바이너리** — Win32(windows-rs)+커스텀 드로잉 · ADR-0001 Accepted |
| DR-2 | 예산 | 유휴 RSS ≤30MB · exe ≤10MB · 임포트=OS 인박스만 — **병합 게이트**. **개정(08-01 [ADR-0006](27-adr-0006-cloud-oauth.md))**: B3 화이트리스트 +4(winhttp·crypt32·ws2_32·bcrypt — 전부 인박스라 원칙 불변) |
| DR-3 | 배포 | **개정(07-16)**: 포터블 단일 exe **기본** + 설치형 exe **보조** 2채널(`data\` 영속·쓰기 불가 시 LOCALAPPDATA 폴백 — [21](21-distribution.md)) |
| DR-4 | 코어 | 원본 nexa-core/vfs/tree **rlib 이식**(FFI 폐지) |
| DR-5 | UX | 원본 M1 기능 패리티·디자인 규약 계승 |
| DR-6 | 라이선스 | PolyForm NC + 의존성 퍼미시브 온리 |
| DR-7 | 플러그인 | **재개정(07-26 [ADR-0005](25-adr-0005-wasm-plugins.md))**: 미리보기 플러그인 런타임 = **WASM(wasmi)** — `data\plugins\*.wasm`·fuel/메모리 격리. .NET SDK 비이관·내장(텍스트/WIC) 폴백 존치는 유지 |
| DR-8 | 외부 crate | 기본 0 지향, 건별 원장 기록(`windows`·`regex-lite`·`wasmi` 승인 — starlark은 07-26 제거) |

## 2. 예산 실측 현황 (DR-2)

| 항목 | 예산 | 최신 실측 | 시점 |
| --- | --- | --- | --- |
| B1 유휴 RSS | ≤30MB | **16.86MB**(중앙값, 3회 18/16.86/4.12 — **M5 마감 실측**: 10k·도크 정보 뷰·런처 바·유휴 300s. 활성 ~36→트림 직후 2.6MB 후 재상승 편차 큼 — 최저 4.12는 M4 수준, 재상승 원인 관찰은 β arena 회수와 공동 과제). **클라우드(X-36/37) 이후 미재측정** | 07-15 실기 |
| B2 exe 크기 | ≤10MB | **3.66MB**(`0.16.0` 릴리스 포터블 실측). 08-23 작업분(X-42 2차·X-43·X-44 5차·X-45) 포함 로컬 release 빌드 **3.71MB** | 08-11 실기(로컬 08-23) |
| P1 100k 첫 렌더 | <150ms | **115ms**(중앙값, 열거 포함 — 10k는 42ms) | 07-12 실기 |
| P2 스크롤 | 60fps(<16.7ms) | **2.1ms/프레임**(100k·200프레임 벤치) | 07-12 실기 |
| B3 임포트 DLL | OS 인박스만 | **통과** — 기존 + dwrite·combase·ole32·bcryptprimitives·shell32 + **클라우드 4종**(winhttp·crypt32·ws2_32·bcrypt — ADR-0006 개정. `scripts/budget-b3.ps1` 단일 출처) | 08-01 실기 |

## 3. 마일스톤 (상세 [MILESTONES](MILESTONES.md))

- **M0** 기반·게이트 ✅ (`0.1.0`) — 설계 문서·스캐폴딩·코어 3크레이트 이식·Win32 창·렌더 스파이크·CI·게이트 실측.
- **M1** 뷰어(★플래그십) ✅ (`0.2.0`) — 전 항목 완료 + 게이트 통과(100k 115ms·60fps·RSS).
- **M2** 셸 골격 ✅ (`0.3.0`) — 경로 바·듀얼/탭·크롬·테마·설정/세션·i18n·IME/UIA 1차·상주 규율(게이트: 듀얼·탭4 26.9MB ≤30).
- **M3** 파일 조작 ✅ (`0.4.0`) — 전송·삭제/이름변경/새로 만들기·Undo/Redo(휴지통 복원)·셸 컨텍스트 메뉴·OS 클립보드/OLE DnD·watcher + 탐색기 클릭/편집 시맨틱(게이트: 10k 유휴 300s **6.29MB** ≤30).
- **M4** 하단 패널 ✅ (`0.5.0`) — 도크·정보 뷰·미리보기·ConPTY 터미널(+상호작용 QA 시리즈)·프리즈 2건 근본 해소(게이트: 10k+터미널 상주 유휴 300s **5.07MB** ≤30).
- **M5** 마감 ✅ (`0.6.0` 발행) — M5-1(퀵 런처 바·일괄 이름변경 α+파이프라인 확장) · M5-2(릴리스 파이프라인 — 첫 태그 실행 검증 완료) · M5-3(UIA/IME 마감·서명=무서명 확정). 실기 QA 잔여.
- **포스트 M5** UX 고도화·배포 채널 정착 ✅ 진행 (`0.7.0`~**`0.13.0`**) — 툴바 SVG·ctl 컨트롤 15종·일괄 이름변경 v2·탭/전송 UX·플러그인 wasmi·안정성 감사·**클라우드 Link/Connect**(X-36/X-37).

## 4. 개발 모델 ([11](11-dev-environment.md))

- 맥 = 일상 개발(코어 test + **windows 타깃 cargo check로 UI 코드까지 타입 검증**) · Windows PC/CI = 실행·QA·예산 실측.
- **비Windows 경로 검증 필수**(08-02 교훈 — [18 §2·§4](18-build-and-test.md)): Windows `cargo test`가 green이어도 cfg 소거 경로는 미검증 → `cargo check --workspace --all-targets --target x86_64-unknown-linux-gnu`.

## 5. 다음 단계 (09-02 기준 — 상세는 [TODO](TODO.md) §7)

1. **실기 QA 잔여분 소화** — 사용자 QA가 병목. 새 기능보다 우선. **최우선 = X-40 클라우드 실검증**(`0.16.0` 배포됨 — OneDrive/Dropbox/Google Drive 동기화 폴더에서 밖의 저장·동기화가 갱신 버튼 없이 반영되는지. 로컬 NTFS는 기존 watcher로도 잡히므로 **클라우드/네트워크 경로가 진짜 검증 대상**) + **X-42 가상 파일 붙여넣기**(08-23 구현·미배포 — 맥 RDP에서 복사→앱 붙여넣기: 단일/다중/폴더/대용량/한글 이름. Outlook 첨부·zip 내부도 같은 경로) + **X-43 빈 폴더 글리프**(08-23 구현·미배포 — 빈/숨김만 폴더 ▸ 사라짐·보기 토글 복귀·설정 해제 = 현행·네트워크 경로 스크롤 체감. **둘 다 앱 종료 후 재빌드 필요** — 실행 중 exe는 X-41까지). X-41 외관은 3라운드 QA로 확정됨.
2. **X-44 간헐 무갱신 1~5차 — 구현 완료·실기 QA 대기**: 탭 전환 순간 목록 최신화 · 알트탭 복귀 갱신 · **클라우드 생성 → 로컬 반영 순간 비활성에서 즉시 표시**(5차 셸 통지 — 핵심 확인 항목) · USB 삽입/제거 · 소실 폴더 조상 이동 · **동기화 중 갱신 누락 재발 장기 관찰**. X-42 2차 QA = RDP 대용량(창 반응·진행·취소)·Ctrl+Z·Outlook/zip 드래그·클라우드 붙여넣기. [TODO](TODO.md) §7.
3. **배포 채널 = 조치 불요·상태 추적만**(09-02 실측): **winget 2채널 `0.18.1` 라이브**([#423330](https://github.com/microsoft/winget-pkgs/pull/423330)·[#423331](https://github.com/microsoft/winget-pkgs/pull/423331) — 08-24 제출 ~1시간 만 병합) · **Chocolatey 2종 `0.8.1` 승인**(07-20 스캔 플래그 후 44일 — 모더레이터 면제) → 규칙대로 **`0.18.1` 두 패키지 제출**([run](https://github.com/SosomLab/nexa-dir2/actions/runs/33578690160) — `Submitted`). `CHOCO_PUSH=true` 등록됨 = 다음 릴리스부터 태그 push가 choco도 자동 게시(단 0.18.1 계류 중이면 제출 규칙상 제외). → [21 §7·§8](21-distribution.md)
4. **다음 릴리스 판정**(제출 규칙): winget = 대기 0건 → 제출 대상 · choco = `0.18.1` 검수 중이면 **제외**. 릴리스 시점에 재실측한다.
5. **클라우드 배포 선행 조건 3건**(코드 작업 없음 — 본인 사용은 무관·[ADR-0006 §2-4-1](27-adr-0006-cloud-oauth.md)) — Dropbox 프로덕션 승인(가장 쉽고 효과 큼) · Entra 게시자 확인(조직 계정 동의 차단 해소 — 도메인 필요) · Google CASA(보류 — 프로덕션 게시로 7일 만료만 제거하는 절충 가능).
6. **백로그** — X-11 원본 패리티 갭 건별([19](19-parity-gap.md)) · X-2 잔여(공급자 콤보·핫 리로드·비동기 미리보기) · X-16 최적화 잔여 · X-13 2/2(브랜치 `feat/x13-launcher-crud` 보존 중) · X-14·X-15·X-24.
7. **X-33 macOS·Linux 확장** — 검토 완료([23](23-cross-platform-feasibility.md)), **착수 여부 = 사용자 결정 대기**. 진행 시 다음 액션 = 맥 렌더 스파이크(결정 아님) + DR-1/2/8 개정 ADR.
