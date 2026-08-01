# STATUS — Nexa Dir 진행 현황

> **갱신: 2026-08-02 3차 (KST)** — **문서 내용 정리 + 진행사항 최신화(사용자 요청)**:
> 커밋 대기분이 없어(트리 clean·main↔origin 0/0) **문서 자체의 어긋남 5건**을 정리했다.
> ① **이 문서가 다시 "한 장" 규약 이탈** — 07-24에 175줄로 압축했던 것이 차수 25건·
> **565줄**로 재팽창 → 08-02 2건만 원문 유지하고 **08-01 이전은 하루 한 줄 색인**으로 압축
> (원문은 journal 그대로 = 정보 손실 없음), **565 → 161줄**
> ② **DR-7이 두 세대 낡아 있었다** — 이 문서 §1은 `.NET 비이관·내장 대체`(07-14 이전),
> [10](10-decision-record.md) §1은 **Starlark**(07-14) 판. 실제는 **wasmi**([ADR-0005](25-adr-0005-wasm-plugins.md) 07-26)
> ③ **ADR 색인 결손 2건** — [10](10-decision-record.md) §1-1에 **ADR-0005·ADR-0006 누락**(README에는 있었다) +
> ADR-0004는 "런타임은 ADR-0005로 대체" 명기 ④ **예산 표 13일 정체** — B2 `1.45MB`(07-20) →
> **3.44MB**(08-02) · B3 시점 07-13 → **08-01 개정**(ADR-0006 +4 DLL) ⑤ **완료분이 진행 중으로 남아 있었다**
> — [TODO](TODO.md) X-36 🚧 → ✅(08-02 병합·`0.13.0` 배포) · X-37 잔여를 **"구현 잔여"가 아니라
> 배포 선행 조건**으로 재분류 · [MILESTONES](MILESTONES.md) 포스트 M5 행이 `0.11.0`에서 멈춰 있던 것을
> `0.13.0`·클라우드까지 확장. 코드 무변경.
> [journal/2026-08-02.md](journal/2026-08-02.md).
>
> **직전(08-02 2차)** — **릴리스 `0.13.0`(Chocolatey 제외) +
> winget Portable 업그레이드 + 비Windows CI 복구(사용자 지시)**:
> ① **0.13.0 배포** — X-36/X-37 클라우드 반영. [Release](https://github.com/SosomLab/nexa-dir2/releases/tag/0.13.0)
> **자산 5종 첫 실배포 확인**(07-31 추가한 zip 2 + `SHA256SUMS.txt`) ·
> **Chocolatey push 스텝 `skipped` 확인**(`CHOCO_PUSH` 게이트 정상 동작)
> ② **winget Portable 0.13.0 PR** [#410978](https://github.com/microsoft/winget-pkgs/pull/410978)
> (SHA-256은 릴리스 `SHA256SUMS.txt`와 대조·설명에 클라우드 추가)
> ③ **main CI가 비Windows에서 깨져 있던 것을 발견·복구 2건** — `cloudfs`가
> `crate::win`/`dialog`를 43곳 참조하는 UI 결합 워커라 `#[cfg(windows)]` 정렬 ·
> `cloud::detect()`의 `Vec::new()`가 cfg 블록 소거로 타입 추론 근거를 잃던 `E0282`.
> **Windows 잡·릴리스 산출물은 무사**했다. 컴파일을 뚫자 드러난 테스트 2건은
> **07-27부터 계속 붉던 것**(클라우드 작업 소산이 아님 — Windows 잡만 보느라
> 일주일 넘게 미발견). `open_any_root`는 Windows 전용 폴백이라 cfg 게이팅,
> Mermaid flowchart는 3단 폴백 중 둘만 단언하던 것을 확장. **3잡 green 복귀**
> ④ **절차 보완**([18 §2·§4](18-build-and-test.md))
> — Windows에서 `cargo test`가 green이어도 cfg 소거 경로는 미검증이라
> `--target x86_64-unknown-linux-gnu` 검사를 명령 목록에 추가.
> [21 §8](21-distribution.md) 채널 표 갱신 — **choco 정체는 큐 대기가 아니라
> 07-20 바이러스 스캔 플래그**(무서명 exe 오탐 · 모더레이터 면제 필요)로 정정.
> [journal/2026-08-02.md](journal/2026-08-02.md).
>
> **직전(08-02 1차)** — **클라우드 매뉴얼·브랜딩 + 배포 한도 점검
> (사용자 요청 — `feat/x36-cloud-connections` 병합 완료)**:
> ① **위키에 클라우드 문서가 아예 없던 것**을 확인하고 기능 전체를 매뉴얼화
> ([기능-클라우드](wiki/기능-클라우드.md) 신설 — **Link vs Connect 구분표**·
> 토큰 DPAPI 보관·**서비스별 발급 절차 3종**[걸려 넘어졌던 함정 그대로]·
> 한도표·문제 해결표) + [기능-설정](wiki/기능-설정.md)에 **"파일 직접 편집"**
> 절 신설(`cloud_client_id/secret` 키 표) ② **앱 안내 문구의 잘못된 리디렉션
> URI 정정**(lang 3종 — `127.0.0.1(포트 임의)`로 안내했으나 실제는 localhost
> 고정 포트. **그대로 따르면 등록 실패**) + 사문 키 2개 제거
> ③ `packaging/branding/` — 콘솔 3사 문구 SSOT·영문 Description·권한 사유표
> + **아이콘 64/256 무손실 추출** ④ **배포 인원 한도 점검**([ADR-0006 §2-4-1]
> (27-adr-0006-cloud-oauth.md)) — **셋 다 무제한이 아니고 벽이 다르다**:
> Dropbox 500명(**50명에서 2주 카운트다운**) · OneDrive **개인 무제한·조직 차단**
> · Google 100명(**게시해도 유지·리셋 불가**). 기존 ADR 서술 2건 정정
> (OneDrive "게시자 인증 선택"은 틀렸다 — 조직 계정엔 필수).
> **254 green·clippy 0·3.44MB(B2)·B3 통과**.
> **잔여 = 배포 선행 조건 3건**(Dropbox 프로덕션·Entra 게시자 확인·Google CASA)
> — 전부 심사·검증이라 코드 작업 없음. [journal/2026-08-02.md](journal/2026-08-02.md).
>
> ---
>
> **이전 이력 요약 (08-01~07-15)** — 상세는 [DEVLOG](DEVLOG.md)와 각 일자
> [journal/](journal/). 아래는 하루 한 줄 색인이며, **원문은 삭제되지 않고
> journal에 그대로 있다**(STATUS = "지금 상태 한 장" 규약 — [16 §1](16-doc-git-conventions.md)).
>
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
| B2 exe 크기 | ≤10MB | **3.44MB**(클라우드 포함 — `0.13.0` 릴리스 포터블 3.43MB. 07-26 wasmi 도입 3.18 대비 +0.26) | 08-02 실기 |
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

## 5. 다음 단계 (08-02 기준 — 상세는 [TODO](TODO.md) §7)

1. **실기 QA 잔여분 소화** — 사용자 QA가 병목. 새 기능보다 우선.
2. **배포 채널 심사 대기 4건**(우리 측 조치 불요·상태 추적만) — winget Portable `0.13.0`([#410978](https://github.com/microsoft/winget-pkgs/pull/410978)) · winget 설치형([#404528](https://github.com/microsoft/winget-pkgs/pull/404528) — `Policy-Test-1.2` waiver = 모더레이터 몫) · Chocolatey 2종(**07-20 바이러스 스캔 플래그** — 무서명 exe 오탐·면제 필요. 승인 시 `CHOCO_PUSH` 재개). → [21 §7·§8](21-distribution.md)
3. **클라우드 배포 선행 조건 3건**(코드 작업 없음 — 본인 사용은 무관·[ADR-0006 §2-4-1](27-adr-0006-cloud-oauth.md)) — Dropbox 프로덕션 승인(가장 쉽고 효과 큼) · Entra 게시자 확인(조직 계정 동의 차단 해소 — 도메인 필요) · Google CASA(보류 — 프로덕션 게시로 7일 만료만 제거하는 절충 가능).
4. **백로그** — X-11 원본 패리티 갭 건별([19](19-parity-gap.md)) · X-2 잔여(공급자 콤보·핫 리로드·비동기 미리보기) · X-16 최적화 잔여 · X-13 2/2(브랜치 `feat/x13-launcher-crud` 보존 중) · X-14·X-15·X-24.
5. **X-33 macOS·Linux 확장** — 검토 완료([23](23-cross-platform-feasibility.md)), **착수 여부 = 사용자 결정 대기**. 진행 시 다음 액션 = 맥 렌더 스파이크(결정 아님) + DR-1/2/8 개정 ADR.
