# 26 · 클라우드 연계 검토 — This PC 노출 + 기능 플러그인 클라이언트

> **작성: 2026-08-01** — 사용자 요청 2건 검토: ① This PC 영역에 Google Drive·OneDrive
> (개인/비즈니스)·Dropbox 등 연결된 클라우드를 보여주고 쉽게 접근 ② 직접 클라우드
> 클라이언트를 **미리보기와 다른 "기능 플러그인"** 방식으로 — 유형별 추가·사용 여부
> 토글·클라우드별 2개 이상 계정. 결정 문서가 아니라 **검토서**(착수는 §6 사용자 결정).
> 규약 유보: 원본(`../nexa-dir`) 클론이 이 PC에 없어 **원본 대조 미수행** — 착수 전
> 맥 세션에서 원본 클라우드 관련 스펙 유무 확인 필요(재발명 금지, CLAUDE.md §5).

## 0. 요약 (권고)

**2단계로 분리한다.** 두 요청은 가치·비용이 전혀 다르다.

| | Phase A — 동기화 폴더 노출 (X-36 제안) | Phase B — 직접 클라이언트 플러그인 (X-37 제안) |
| --- | --- | --- |
| 내용 | 설치된 동기화 클라이언트의 로컬 폴더를 탐지해 **내 PC(`::PC::`) 뷰에 "클라우드" 섹션**으로 노출 | 플러그인이 클라우드 API를 직접 호출하는 탐색/전송 — 유형별 플러그인·토글·다계정 |
| 네트워크 | **0**(전부 로컬 읽기) | 필요(호스트 중개 HTTPS) |
| 신규 의존 | **crate 0 · B3 무변** | crate 0 유지 가능하나 **B3 화이트리스트 +1(winhttp.dll)** · ADR 필요 |
| 규모 | 소(1~2 슬라이스) | 대(ABI v2 + VFS 공급자 + OAuth + 설정 UI) |
| 커버리지 | 동기화 클라이언트를 이미 쓰는 대다수 사용자 | 클라이언트 미설치 환경·온라인 전용 접근·진짜 다계정 병렬 |

**권고**: Phase A 즉시 착수(사용자 가치 대부분을 저비용으로 회수). Phase B는 본 검토
§3 설계로 ADR-0006 초안 → OneDrive 1종 읽기 전용 스파이크 → 평가 후 확대.

## 1. 현재 기반 (재사용 자산)

- **가상 최상위 `::PC::`**(X-17): 센티널 루트 + `drive_entries()` — 항목 이름이 절대
  경로라 `join` 시 부모 대체 = 진입이 실경로가 되는 설계([nexa-vfs lib.rs](../crates/nexa-vfs/src/lib.rs)
  `MY_PC` 문서). 열거 분기는 nexa-tree 1곳(`lib.rs:259`). **클라우드 섹션은 이 규약을
  그대로 탄다.**
- **wasmi 플러그인 런타임**(ADR-0005): `.wasm` 로더·`nx_meta`/`nx_preview` ABI·
  fuel/메모리 격리·설정 `plugins_disabled` 토글·오류 1줄 격리. **"종류(kind)" 필드만
  추가하면 기능 플러그인의 그릇이 된다.**
- **워커 잡 + PostMessage 통지 패턴**: 전송 엔진(Event 프로토콜·진행 창·취소)·삭제
  워커 — 클라우드 I/O의 비동기 실행 모델로 재사용.
- **DR 제약**: 단일 exe(DR-3)·B2 ≤10MB·B3 인박스 임포트만·crate 0 지향(DR-8)·
  퍼미시브 온리(DR-6).

## 2. Phase A — 동기화 클라이언트 탐지 + 내 PC 노출

### 2-1. 탐지 원천 (전부 로컬 읽기 — 네트워크 0 · crate 0)

| 서비스 | 1차 원천 | 폴백 | 다계정 |
| --- | --- | --- | --- |
| OneDrive 개인 | `HKCU\Software\Microsoft\OneDrive\Accounts\Personal` → `UserFolder` | env `%OneDriveConsumer%`·`%OneDrive%` | — |
| OneDrive 비즈니스 | `…\Accounts\Business1..N` → `UserFolder`·`DisplayName`(테넌트명) | env `%OneDriveCommercial%` | **키 열거로 자연 지원** |
| Google Drive (DriveFS) | `HKCU\Software\Google\DriveFS`(마운트 문자·버전별 상이) | 드라이브 열거 중 **볼륨 라벨 "Google Drive"** 프로브 | 계정별 마운트 문자 |
| Dropbox | `%APPDATA%\Dropbox\info.json`(또는 `%LOCALAPPDATA%`) — `personal`/`business` 키의 `path` | — | **JSON 구조가 이미 다계정** |
| iCloud Drive | `%USERPROFILE%\iCloudDrive` 실존 프로브 | — | — |

- 공통 방어: **폴더 실존+접근 프로브 통과분만 노출**(제거 잔재 레지스트리 대비 —
  `open_any_root` 프로브 전례).
- 레지스트리 읽기 = advapi32(인박스·이미 임포트) — **B2/B3/RSS 영향 0**.
- Google DriveFS처럼 **드라이브 문자로 마운트**되는 유형은 이미 드라이브 목록에
  나온다 — "클라우드" 섹션으로 **승격·라벨링**(중복 제거)이 개선의 실체.

### 2-2. UI 노출

- **내 PC 뷰에 "클라우드" 섹션**: `cloud_entries()`가 `drive_entries()` 뒤에 합류.
  항목 이름 = 동기화 폴더 **절대 경로**(join-대체 규약 그대로 — 진입 즉시 실경로 탐색).
- **표시명 분리 필요**: 현재 `Entry.name`이 표시명 겸 join 키 — 클라우드 행은
  "OneDrive – SosomLab"처럼 보여야 하므로 **표시명≠경로** 지원이 선행 단위
  (.lnk `display_name` 전례 있음 — 그 기구 확장 검토).
- 아이콘: 실폴더 셸 아이콘 요청이면 동기화 클라이언트가 등록한 배지가 자동 반영.
- β: 경로 바 `shell:` 스킴 별칭 · 정보 패널 용량 · 퀵 런처 시드.

### 2-3. 함정 (기존 실측 교훈 연결)

- **파일 온디맨드 플레이스홀더**: 열거는 하이드레이션을 유발하지 않지만 **내용
  읽기(미리보기 read_text·아이콘 추출)는 유발** — 07-14 실측(다운로드 exe 아이콘
  추출→Defender 수십 초, journal 07-14) 재발 지점. `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`
  (0x0040_0000)·`OFFLINE` 감지 시 **미리보기 지연/명시 동작**(자동 열지 않고 "온라인
  전용 — Enter로 내려받기" 1줄)으로 방어. `Entry.attrs`에 이미 속성이 실려 있어 무료.
- **watcher 폭주**: 동기화 폴더는 변경 이벤트 폭주원 — 07-31 안정성 S1(ENUM_DIR
  복구)·S2(자가 치유)가 전제를 이미 해소.
- 상태 컬럼(핀/온라인 전용/동기화 중)은 β — 속성 비트만으로 1차 구분 가능.

### 2-4. 슬라이스 (수직 규약)

1. `cloud_entries()` 탐지(cfg(windows) — nexa-vfs 중립성 유지 위해 Win 전용부는
   nexa-app 쪽 배치 검토) + 내 PC 섹션 표시 + 실존 프로브 테스트.
2. 표시명 분리(Entry 확장 또는 표시층 매핑) + i18n(`mypc.cloud` 등 lang 3종).
3. β: 플레이스홀더 미리보기 방어 · 상태 표시 · 용량.

### 2-5. 파일 온디맨드 기능 파리티 — "공간 확보"·"항상 이 장치에 유지" (08-01 사용자 질의)

**가능 — 두 경로가 보완 관계다** (이 기능들은 클라우드 API가 아니라 Windows Cloud
Files 계층 소관 = Phase A 영역).

| 경로 | 구현 | 커버리지 | 비용 |
| --- | --- | --- | --- |
| **① 셸 우클릭 위임** | 동기화 클라이언트가 등록한 컨텍스트 메뉴 확장 — **기존 shellmenu.rs 호스팅으로 이미 노출 가능성 높음**(실기 확인만) | **전 서비스**(DriveFS 포함 — 각자 자기 메뉴 제공) | 0 |
| **② 1급 기능화** | 상태 = `Entry.attrs` 비트 판독(`RECALL_ON_DATA_ACCESS` 0x40_0000=온라인 전용·`PINNED` 0x8_0000·`UNPINNED` 0x10_0000 — 무료) · pin/unpin = `SetFileAttributesW`(kernel32 — **B3 무변**, 엔진이 하이드/디하이드 수행) · 즉시 강제 = `CfHydratePlaceholder`/`CfDehydratePlaceholder`(cldapi.dll 인박스 Win10 1709+ — **B3 +1**) | **Cloud Files 기반만**(OneDrive·최신 Dropbox·iCloud). **Google DriveFS 불가**(자체 드라이버) → ①이 유일 | 속성=0 / cldapi=B3 +1 |

β 슬라이스 제안: **β1** 상태 컬럼/배지 + ① 실기 검증 → **β2** 자체 pin/unpin(속성만 —
**다중 선택 일괄**이 탐색기 대비 차별점) → **β3**(선택) cldapi 즉시 하이드레이션(B3
+1은 ADR-0006에 winhttp와 함께 묶어 결정). Phase B 직접 연결에는 이 개념이 없다 —
필요 시 "오프라인 캐시" 자체 설계(별도 규모).

## 3. Phase B — 직접 클라우드 클라이언트 = 기능 플러그인

### 3-1. 미리보기 플러그인과 무엇이 다른가

| | 미리보기(현행) | 기능/공급자 플러그인(제안) |
| --- | --- | --- |
| 호출 | 호스트→플러그인 단발 렌더 | **세션 상태**(인증·커서·캐시) + 다중 오퍼레이션 |
| I/O | `read_text` 1파일 | **네트워크**(목록·다운로드·업로드·스트리밍) |
| 시간 | 300ms/fuel 상한 | 장시간 비동기 잡(네트워크 대기) |
| 통합점 | 도크/독립 창 렌더 | **VFS 트리 열거·전송 엔진·내 PC 섹션** |

→ 같은 wasmi 런타임을 쓰되 **ABI·실행 모델·통합점이 별개**인 "제2종 플러그인"이다.

### 3-2. 아키텍처 스케치

```
설정(연결 관리)          내 PC 뷰
  cloud1.plugin=onedrive    └ 클라우드 섹션: 연결별 행 ::CLOUD:1::
  cloud1.label=회사          └ 진입 → nexa-tree 열거 분기(::PC:: 전례)
  cloud1.token→DPAPI blob        └ Provider(scheme="cloud") → 워커 잡
                                       └ wasmi: nx_list(JSON) ⇄ 호스트 nx_http
```

- **플러그인 종류**: `nx_meta` v2에 `kind: "preview" | "cloud"`(부재 = preview —
  기존 플러그인 무변·하위호환).
- **공급자 ABI**(전부 JSON 직렬화 — 태그 계약 전례): `nx_cloud_meta`(서비스명·OAuth
  엔드포인트·**도메인 허용목록**) · `nx_auth_url`/`nx_auth_exchange` · `nx_list` ·
  `nx_stat` · `nx_read`(range) · (2차) `nx_write`/`nx_delete`/`nx_move`.
- **호스트 임포트 v2**: `nx_http`(method·url·headers·body) · `nx_secret_get/set` ·
  `nx_now`. 네트워크 실체는 **호스트 WinHTTP**(winhttp.dll = OS 인박스 — TLS는
  schannel 위임, crate 0 유지) — **B3 화이트리스트 +1 필요**(budget-b3.ps1·DR-2 원장).
- **실행 모델**: wasmi는 동기 → 클라우드 호출은 **워커 잡 큐**(전송 Event 프로토콜
  재사용·진행/취소) + PostMessage. fuel은 연산만 제한(네트워크 대기는 호스트 잡
  타임아웃이 담당) — 미리보기 상한과 별도 프로파일.
- **VFS 통합**: `nexa_vfs::Provider` 트레이트를 실사용으로 승격(`list`/`stat`/`read`)
  — 센티널 `::CLOUD:<conn_id>::…`(`::PC::` 전례)·nexa-tree 열거 분기 1곳 추가.
  **1차 범위 = 읽기 전용**(탐색·다운로드·미리보기) — 쓰기 계열은 전송 엔진 통합과
  함께 2차.
- **읽기/쓰기 범위(08-01 사용자 질의)**: **쓰기도 기술적으로 전부 가능** — 3사 모두
  업로드(대용량 = Graph `uploadSession`·Dropbox `upload_session`·Google resumable)·
  삭제(클라우드 휴지통행)·이동/리네임/폴더 생성 + 충돌 전제조건(ETag/rev) 지원.
  청크 업로드는 호스트 워커가 임시 파일에서 스트리밍 — wasmi 64MB 상한 무관.
  "1차 읽기 전용"은 **능력이 아니라 순서 문제**: 쓰기를 켜면 전송 엔진·삭제/undo
  (X-35 의미론의 클라우드판)·충돌 UX(타 기기 선수정)·인라인 리네임·새로 만들기까지
  **앱 쓰기 경로 전반의 클라우드 분기**가 필요하고 Google은 쓰기 scope 심사 부담이
  가중된다. 절충안 = 스파이크 범위를 "OneDrive 읽기 + 업로드/삭제 1종"으로 잡아
  쓰기 난도를 조기 실측.

### 3-3. 인증·다계정 (요청 ③의 핵심)

**흐름(08-01 사용자 질의 — OAuth2 Authorization Code + PKCE, 3사 공통 표준)**:

```
1 code_verifier 랜덤 → challenge=SHA256(verifier)   [CNG bcrypt — 인박스]
2 루프백 리스너 http://127.0.0.1:{랜덤포트}/         [ws2_32 — 인박스]
3 인증 URL ShellExecute → 브라우저에서 로그인·동의   (앱은 비밀번호 비접촉)
4 302 리디렉션 → code 1회 수신 → "창을 닫으세요" 응답·리스너 종료
5 code+verifier POST → access+refresh 토큰           [WinHTTP — B3 +1]
6 DPAPI 암호화 → data\secrets\cloudN.tok             [crypt32 — 인박스]
이후: access 만료(~1h) → refresh로 무개입 자동 갱신. refresh 철회/만료 시만 재로그인.
```

| | OneDrive(Graph) | Dropbox | Google Drive |
| --- | --- | --- | --- |
| PKCE 공용 클라이언트 | ✅ | ✅ (`token_access_type=offline`) | ✅ (루프백 공식 지원) |
| scope 예 | `Files.Read.All`+`offline_access` | `files.content.read` | `drive.readonly`(restricted 심사)/`drive.file` |
| refresh 수명 | 90일 슬라이딩 | 무기한 | 무기한(앱 미검증 시 7일) |

- 사전 준비 = 3사 개발자 콘솔 앱 등록 → client_id(무료). PKCE라 **client_secret
  불요·exe 미동봉**(공개돼도 무해한 값만 포함).
- **포터블 특기**: DPAPI는 사용자+PC 바인딩 — `data\`를 타 PC로 옮기면 토큰 복호가
  **의도적으로 실패** = 재로그인(USB에 평문 토큰이 굴러다니지 않는 안전 특성으로
  명시·안내 1줄).


- **OAuth2 PKCE 공용 클라이언트**(시크릿 불요 — MS Graph·Google·Dropbox 모두 지원):
  브라우저 열기(ShellExecute 기존) + **루프백 리디렉션 1회 수신**(ws2_32 인박스 최소
  리스너). 토큰 저장 = **DPAPI 암호화**(crypt32 인박스) `data\secrets\` — X-15
  라이선스 설계와 같은 "crate 0 암호화" 노선.
- **연결(connection) 모델 — 플러그인은 무상태, 계정은 호스트 소유**:
  `cloudN.plugin/label/…` 직렬화(launcherN 전례). **같은 플러그인에 연결 N개 =
  클라우드별 2+계정 요구 충족**. 내 PC 클라우드 섹션에 연결별 행.
- **토큰 비노출 설계(보안 핵심)**: 플러그인은 토큰 원문을 받지 않는다 — `nx_http`에
  `auth: true`만 표시하면 **호스트가 Authorization 헤더를 주입**. 악성/오염 플러그인의
  토큰 탈취를 구조적으로 차단. 도메인 허용목록 밖 `nx_http`는 거부.
- **설정 UI**: 플러그인 페이지 확장(사용 여부 토글 = `plugins_disabled` 전례 그대로 —
  요청 ②의 "체크하면 기능 토글" 충족) + 연결 추가/삭제 편집기(X-13 런처 CRUD와 동일
  패턴 — 공동 설계 여지).

### 3-4. 비용·위험

- **B2**: wasmi 기내장 — 증분은 WinHTTP 글루+OAuth 리스너+연결 관리(추정 수십~수백 KB
  수준, 실측 게이트로 확인). **RSS**: 연결 메타·토큰 캐시 미미.
- **client_id 조달**: 서비스별 개발자 콘솔 등록 필요(무료). 단 **Google Drive 전체
  읽기 scope는 restricted 심사** 부담 — `drive.file`(앱이 연 파일만) 또는 **사용자
  자체 client_id 입력 옵션** 병행 검토. Dropbox·MS Graph는 심사 부담 낮음.
- **API 변동·레이트 리밋**: 플러그인 분리 덕에 앱 릴리스 없이 `.wasm` 교체로 대응
  가능 — 이 구조의 실질 이점.
- **위험 최대 항목**: VFS Provider 승격이 nexa-tree 실경로 전제(정렬·펼침·watcher·
  아이콘)와 맞닿는 범위 — 읽기 전용 1차로 한정해야 통제 가능.

## 4. 대안 비교 (기각 근거)

| 대안 | 판정 | 근거 |
| --- | --- | --- |
| 셸 네임스페이스 위임(IShellFolder로 탐색기 클라우드 노드 재사용) | ❌ | 가상 PIDL 모델이 nexa-tree 실경로 전제와 충돌(X-21 β와 같은 벽) — Phase A 실경로 방식이 단순·우월 |
| 외부 도구(rclone) 동봉 | ❌ | 별도 exe 동봉 = DR-3 단일 exe 위반. 사용자가 직접 마운트한 rclone 드라이브는 Phase A가 자연 커버 |
| 플러그인 직접 소켓(WASI sockets) | ❌ | wasmi 미지원 + 토큰 보호 불가 — 호스트 중개(`nx_http`) 확정 |

## 5. 권고 로드맵

1. **X-36 Phase A** 착수(슬라이스 1→2) — 원본 대조(맥) 선행.
2. **ADR-0006 초안**(플러그인 종류 확장 + B3 winhttp 추가 — DR-7·DR-2 개정) 승인 후
   **X-37 스파이크**: OneDrive(Graph — 문서 최상·심사 부담 최소) 읽기 전용 1종.
3. 스파이크 평가(B2 실측·UX) → Dropbox → Google Drive(scope 전략 결정 후) 확대.

## 6. 사용자 결정 대기

1. Phase A 착수 승인(+원본 대조를 맥 세션으로 미루고 착수할지).
2. Phase B 착수 여부·서비스 순서(제안: OneDrive→Dropbox→Google Drive).
3. 1차 범위 = 읽기 전용 확정 여부(업로드는 2차).
4. client_id 소유 방식(자사 등록 단독 vs 사용자 입력 병행).
5. ADR-0006(DR-7 플러그인 종류 확장·DR-2 B3 +winhttp) 개정 승인.
