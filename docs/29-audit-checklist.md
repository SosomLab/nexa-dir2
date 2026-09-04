# 29. 정규 점검 체크리스트 (Audit Checklist) — 순서·항목·기준

> **목적**: 릴리스 단위로 반복 가능한 **프로그램 점검**(기능 보완 회귀 · 성능 · 다수 파일 로직 · 설정 관리 ·
> 플러그인 · 보안 · 테스트 기법)을 **같은 순서·같은 항목·같은 기준**으로 수행하고 회차별 결과를 남긴다.
> 이 문서는 **살아 있는 규격**이다 — 회차마다 §1~§8의 항목·기준을 보완하고 §9에 결과를 누적한다.
> 자동화 가능한 항목은 [`scripts/audit.ps1`](../scripts/audit.ps1)이 실행한다(§0). 수동 항목은 방법을 표로 고정한다.
> 1차 점검: **2026-09-04**(§9-1). 상태 SSOT는 [STATUS](STATUS.md)·[TODO](TODO.md)(발견 사항은 X-51~ 로 항목화).

## 0. 실행 순서(런북)

| 단계 | 무엇을 | 도구 | 소요 |
| --- | --- | --- | --- |
| 1 | **정적 게이트** — 테스트·clippy·비Windows check·release 빌드·B-2·B-3·PE 완화 플래그·매니페스트 | `pwsh scripts/audit.ps1 -Quick` | ~5분 |
| 2 | **벤치·커버리지** — 폴더 열거(P-1)·VT 처리량/견고성(P-4)·라인 커버리지(T-4) | `pwsh scripts/audit.ps1 -Coverage -BigDir <100k 폴더>` | ~15분 |
| 3 | **실기 실측** — 기동 시간(P-0)·유휴 메모리(B-1 약식 60s)·유휴 CPU(P-5) | `pwsh scripts/audit.ps1 -Idle -BigDir <10k 폴더>` (정식 B-1은 docs/18: 300s·3회 중앙값) | ~3분 |
| 4 | **정적 코드 리뷰** — §3~§8의 질문 목록을 영역별로 답한다(1차는 조사 에이전트 6개 병렬) | 사람/에이전트 | ~1시간 |
| 5 | **수동 실기** — §4 보안·§8 기능 회귀의 수동 항목 | 사람 | 릴리스 QA와 병행 |
| 6 | **기록** — §9 회차 표(실측 + 발견 + 조치) · 발견 사항은 [TODO](TODO.md) §7 항목화 · journal | 문서 | ~30분 |

원칙: ① 실측 없는 "통과"는 쓰지 않는다(UNKNOWN으로 남긴다) ② 기준을 바꾸면 이 문서에서 바꾸고 회차 표에
"기준 개정"을 적는다 ③ 점검용 벤치·픽스처는 저장소에 둔다(`crates/*/examples/audit_*.rs`) — 결과 재현 가능
④ 점검 중 발견한 결함의 수정은 별도 커밋(점검 커밋에 섞지 않음 — 1차의 VT 2건은 테스트 도입이 강제한 예외).

## 1. 항목·기준 총표

ID 접두: **T** 테스트 · **B** 예산(DR-2) · **P** 성능 · **S** 보안 · **F** 파일 조작 · **C** 설정 · **G** 플러그인 ·
**X** 견고성/펜테스트 · **R** 최근 보완 기능 회귀. "자동" = audit.ps1이 판정.

| ID | 항목 | 방법 | 기준(PASS) | 자동 |
| --- | --- | --- | --- | --- |
| T-1 | 워크스페이스 테스트 | `cargo test --workspace` | 실패 0 | ○ |
| T-2 | clippy | `cargo clippy --workspace --all-targets` | 경고 0 | ○ |
| T-3 | 비Windows 경로 검사([18 §4](18-build-and-test.md)) | `cargo check --target x86_64-unknown-linux-gnu` | 오류 0 | ○ |
| T-4 | 라인 커버리지 | `cargo llvm-cov --workspace --summary-only` | **정보**(회차 추이 — 하락 시 사유) · 코어 크레이트(core/vfs/tree/term/ops) ≥ 70% 목표 | ○ |
| T-5 | 적대적 입력 테스트 존재 | §2-2 목록의 테스트가 전부 있고 green | 목록 전항 | 수동 |
| B-1 | 유휴 RSS | docs/18 프로토콜(10k 폴더 기동 → 300s 유휴 → WorkingSet64, 3회 중앙값) | ≤ 30MB | △(-Idle 약식 60s) |
| B-2 | exe 크기 | `target/release/nexa-app.exe` | ≤ 10MB | ○ |
| B-3 | 임포트 DLL | `scripts/budget-b3.ps1` | 인박스 화이트리스트 안 | ○ |
| P-0 | 기동 → 창 표시 | Start-Process → MainWindowHandle 획득 시각 | ≤ 1.5s(세션 복원 포함) · 제목줄 first render ≤ 1.2s | ○ |
| P-1 | 폴더 열거 | `nexa-vfs --example audit_enum <dir> 5` 중앙값 | ≤ 4µs/엔트리(100k ≤ 400ms) | ○ |
| P-2 | 스크롤 프레임 | 앱 F3 벤치(200프레임) | < 16.7ms/프레임(docs/18 P2) | 수동 |
| P-3 | 첫 렌더 100k | 제목줄 first render | < 150ms 열거 포함(docs/18 P1) | 수동 |
| P-4 | VT 파서 처리량·견고성 | `nexa-term --example audit_vt` | ≥ 10MB/s · 비정상 시퀀스 1만 건 무패닉 | ○ |
| P-5 | 유휴 CPU | 활성 창 10s TotalProcessorTime 증가 | ≤ 2%(활성) · 비활성/최소화 ~0% | △ |
| P-6 | 핫패스 정적 질문(§7) | 코드 리뷰 | HIGH 0 | 수동 |
| S-1 | PE 완화 기술 | PE 헤더 DllCharacteristics | DYNAMIC_BASE·HIGH_ENTROPY_VA·NX_COMPAT 필수 · GUARD_CF·CETCOMPAT 권장(WARN) | ○ |
| S-2 | 매니페스트 | 이미지 내 `requestedExecutionLevel` | `asInvoker` + `longPathAware` + DPI 선언 | ○(존재만) |
| S-3~ | §4 보안 표 A~G | 코드 리뷰 + 수동 실기 | 표의 기준 | 수동 |
| F-1~ | §5 파일 조작 질문 | 코드 리뷰 + 실기(다수 파일) | HIGH 0 · 데이터 손실 경로 0 | 수동 |
| C-1~ | §6 설정 관리 질문 | 코드 리뷰 | 원자적 저장 · UI 스레드 무정지 · 손상 시 기동 가능 | 수동 |
| G-1~ | §3 플러그인 표 A1~A30 | 코드 리뷰 + 픽스처 | 표의 기준 | 일부 테스트 |
| X-1~ | §2-2 적대적 입력 | 단위 테스트 | 무패닉·상한 유지·기본값 수렴 | ○(테스트) |
| R-1~ | §8 최근 보완 기능 | TODO 주장 vs 코드 + 실기 | 주장 = 구현 · 엣지 처리 · i18n 3종 키 존재 | 수동 |

## 2. 테스트 기법

### 2-1. 커버리지(T-4)
- 도구: `cargo-llvm-cov` + `llvm-tools-preview`(설치: `rustup component add llvm-tools-preview; cargo install cargo-llvm-cov`).
- 실행: `cargo llvm-cov --workspace --summary-only`(HTML은 `--html` → `target/llvm-cov/html`).
- 해석: nexa-app은 Win32 UI가 대부분이라 단위 테스트 커버리지가 구조적으로 낮다 — **크레이트별**로 본다.
  코어(core/vfs/tree/term/ops)는 순수 로직이라 높아야 하고, 하락은 회귀 신호다. 수치는 §9 회차 표에 누적.

### 2-2. 적대적 입력(펜테스팅형 견고성) 테스트 — 저장소에 상주
| ID | 대상 | 테스트 | 기준 |
| --- | --- | --- | --- |
| X-1 | VT 파서 | `nexa-term` `nasty_sequences_do_not_panic_and_stay_bounded` — 거대 파라미터·미완 CSI/OSC·NUL·비BMP·범위 밖 커서·거대 IL/DL/ICH/DCH·1×1 리사이즈 | 무패닉 · 커서/스크롤백 상한 · 500회 반복 < 1s |
| X-2 | 설정 파서 | `config::tests::parse_garbage_is_harmless_and_clamped` — 빈 파일·BOM·`=` 없음·이진·거대 수·NaN·1MB 한 줄·중복 키·CRLF | 무패닉 · 클램프 · 직렬화 왕복 안정 · Session도 동일 |
| X-3 | 압축 파싱 | `nexa-vfs` `zip_slip_paths_are_contained_and_flagged`·`truncated_tail_is_reported_not_panicking`·`garbage_is_rejected_without_panic` | 경로 탈출 차단·`suspicious` 플래그·잘림 보고 |
| X-4 | 가상 파일 디스크립터 | `clipboard.rs` sanitize_rel 테스트 | 절대 경로·`..`·드라이브 거부 |
| X-5 | 플러그인 | `preview::wasm` `nx_loop` 연료 소진·`broken.wasm` 스킵 | 오류 반환·타 플러그인 정상 |
| X-6 | 복사 서식 | `nexa-term` `export_html_rtf_and_cf_html_offsets` | HTML 이스케이프·CF_HTML 바이트 오프셋·RTF `\uN?` |
| **보강 대상** | VT 파서 fuzz(임의 바이트 1MB) · 플러그인 반환 버퍼 경계(`ptr≈u32::MAX`·`len>OUT_CAP`) · 메모리 상한 트랩 · 설정 파일 비UTF-8 | 2차 회차에 추가 | |

### 2-3. 회귀 고정 규칙
실측으로 잡은 결함은 **재현 테스트를 먼저 커밋**하고 수정한다(1차: VT 오버플로·IL/DL 소진이 X-1 도입으로 발견).

## 3. 플러그인 점검 기준(WASM/wasmi — [24](24-plugin-dev-guide.md)·[25](25-adr-0005-wasm-plugins.md)·[28](28-archive-preview.md))

| # | 항목 | 방법 | 기준 |
| --- | --- | --- | --- |
| A1 | 호출당 연료 상한 | `set_fuel` 확인 | 모든 게스트 호출 전 설정 |
| A2/A3 | 연료 소진 → 오류·호스트 생존·**1초 내 복귀** | `nx_loop` 픽스처 + 스톱워치 | 오류 반환·UI 응답 |
| A4 | **벽시계 타임아웃** | 호스트 임포트 내 `Instant` 검사 | 문서화된 상한(ADR-0005 약속) |
| A5 | 메모리 상한 트랩 | `memory.grow` 64MB 초과 픽스처 | 트랩·복구 |
| A6 | 호스트 임포트 비용이 예산에 잡히는가 | `read_at`/`render_svg` 루프 픽스처 | 호스트 작업 유계 |
| A7/A8 | 손상 모듈 스킵 + **사용자에게 사유 표시** | `\0asm junk` 투입 | 타 플러그인 정상·오류 로그/표시 |
| A9 | 8MB 초과 모듈 거부 | 9MB 파일 | 사전 거부 |
| A10/A11 | ABI v1(3줄 meta)·v2(caps=archive) 모두 로드 | 동봉 2종 | 라우팅 정확 |
| A12/A13 | 필수 export 누락·미지 import | 픽스처 | 1줄 오류·로드 격리 |
| A14/A15 | 게스트 포인터/길이 경계 · 반환 버퍼 > OUT_CAP | 픽스처 | 무패닉·**명확한 메시지** |
| A16/A17 | 파일 접근 = 미리보기 대상뿐 · 아카이브 경로 탈출 | 코드 리뷰 | `HostCtx.path` 외 접근 불가·정규화 |
| A18/A19 | 암호가 호스트에 잔존하지 않음 · **archive 능력 플러그인만 `password`** | 코드 리뷰 | zeroize · 링커 능력별 구성 |
| A20 | 쓰기/실행 임포트 없음 | 임포트 열거 | `render_svg` 임시 BMP만(개수 상한·정리) |
| A21 | **비활성 플러그인은 실행되지 않음** | 설정 비활성 후 추적 | `nx_meta` 미호출 |
| A22/A23 | 콜드 로드 < 50ms/플러그인 · 64KB md 미리보기 < 100ms | 스톱워치 테스트 | 수치 |
| A24 | 컴파일 모듈 캐시(호출마다 재컴파일 없음) · 링커 재구성 없음 | 코드 리뷰 | Module·Linker 재사용 |
| A25 | **dist/*.wasm = 소스**(드리프트 없음) | CI: 빌드 후 `git diff --exit-code samples/*/dist` | diff 0 |
| A26/A27 | 같은 id 양쪽 폴더 → 사용자 사본 우선 · 설정 왕복 | 테스트 | 통과 |
| A28 | 출처 검증(해시/서명/허용목록) | 로더 | 동봉본 SHA-256 매니페스트·미지 모듈 경고 |
| A29 | 잘림(50k 초과) 사용자 표시 | 픽스처 | `truncated` 플래그 |
| A30 | 반복 실패 격리(서킷 브레이커) | 10회 연속 트랩 | 세션 내 자동 비활성 + 안내 |
| 동봉 플러그인별 | markdown.wasm(64KB·400줄·Mermaid 3단 폴백) · archive.wasm(ISO/ar/cpio, 20k 엔트리) | `preview::sample_tests` | id/exts·렌더·garbage → 비Ok |

## 4. 보안 점검 — 권한 축소·샌드박스·완화 기술

| # | 항목 | 방법 | 기준 |
| --- | --- | --- | --- |
| A1 | 관리자 권한 요구 없음 | 매니페스트 `requestedExecutionLevel` | `asInvoker`·`uiAccess=false`(**명시**) |
| A2/A3 | Medium IL·토큰 API 없음 | Process Explorer · grep `OpenProcess/AdjustTokenPrivileges` | 0건 |
| A4/A5 | DPI·longPathAware 매니페스트 선언 | 매니페스트 | 존재 |
| A6 | 프로세스 완화 정책 | `SetProcessMitigationPolicy` | ImageLoad(원격/저라벨 차단)·ExtensionPointDisable |
| B1 | ASLR·HighEntropy·DEP | PE DllCharacteristics | 0x0020\|0x0040\|0x0100 |
| B2/B3 | CFG · CET | DllCharacteristics 0x4000 · 디버그 디렉터리 type 20 | 존재(`-C control-flow-guard=yes`·`/CETCOMPAT`) |
| B4/B5 | DLL 검색 경로 하드닝 · 하이재킹 가능 임포트 | LoadConfig DependentLoadFlags · imports ∖ KnownDLLs | 0x0800 · 공집합 |
| B6/B7 | 런타임 LoadLibrary 없음 · 인박스 화이트리스트 | grep · budget-b3 | 0건 · 통과 |
| B8 | release `overflow-checks` | Cargo.toml | true(권장) |
| B9/B10 | 코드 서명 · 출처 증명 | Authenticode · attestation | 정책 결정(현재 무서명 문서화) |
| C1 | 자식 프로세스 **전체 경로**로 실행 | `CreateProcessW` lpApplicationName | 전체 경로 |
| C2/C3/C4 | 핸들 비상속 · `cmd /c` 문자열 조립 없음 · 셸 메뉴 verb는 ordinal | 코드 | 통과 |
| C5 | 런처 `%path%` 치환 인용 | 코드 | .bat/.cmd 대상 시 인용 |
| C6 | `SetSearchPathMode(SAFE)` | 코드 | 호출 |
| D1~D3 | 토큰 DPAPI 보관 · 부가 엔트로피 · secrets 폴더 ACL | 코드·`icacls` | DPAPI·엔트로피·현재 사용자만 |
| D4/D5/D6 | settings.cfg·로그·소스에 시크릿 없음 | grep·`git log -p` | 0건 |
| D7~D9 | 루프백 127.0.0.1·state·PKCE S256·**CSPRNG 실패 = 오류** | 코드 | 통과(폴백 PRNG 금지) |
| D10/D11 | TLS 강제·검증 · 리디렉션 시 Bearer 미전달 | 코드·302 테스트 | `WINHTTP_FLAG_SECURE`·리디렉션 정책 |
| D12 | 자동 업데이트/기타 네트워크 없음 | URL 리터럴 열거 | 문서화된 3 제공자만 |
| E1~E6 | 플러그인 임포트 최소 · 연료/메모리 · 손상 격리 · 반환 버퍼 fuzz · 플러그인 폴더 신뢰 · 암호 비잔존 | §3 | §3 기준 |
| F1~F8 | zip-slip · 가상 파일 경로 · 임시 폴더 유일성 · 임시 정리 · **MOTW** · 링크 추적 없는 재귀 복사/삭제 · 긴 경로 | 코드·실기 | 통과 |
| G1 | **WM_APP 포인터 메시지 인증** | 타 프로세스에서 `PostMessage(hwnd,0x800F,0,0x41414141)` | 무시·무크래시 |
| G2/G3/G4 | WM_COPYDATA 없음 · `cargo audit`/툴체인 고정 · 위협 모델 문서 | CI·docs | 존재 |

## 5. 파일 조작(다수 파일) 점검 질문

| # | 질문 | 기준 |
| --- | --- | --- |
| F-1 | N개 전송 실행 모델(워커 1개? 배치? 바이트/항목 진행?) | 워커 · 바이트+항목 진행 · 취소는 항목 간·파일 중간·**계획 단계**에서도 |
| F-2 | 오류 격리(한 파일 실패 → 나머지 계속?) · 집계 · 사용자 표시 | 항목·**하위 트리 항목** 격리 · 실패 목록+재시도 UI |
| F-3 | 충돌·덮어쓰기(대상 삭제 시점) | **새 파일 커밋 후** 옛 대상 교체(임시 이름 → 교체) |
| F-4 | O(N²) 루프(진행 통지 클론·선택 복원·이름 충돌 검사) | N=50k에서 UI 스레드 정지 없음 |
| F-5 | 같은 볼륨 이동 vs 교차 볼륨 · rename 실패 폴백 · 정션/심링크 · 260자 초과 | 폴백 있음 · 재분석점 스킵/재생성 · longPathAware |
| F-6 | 완료 후 갱신(재열거)이 UI 스레드에서? 양 패널 전부? | 영향 패널만 · 워커 |
| F-7 | 완료 통지 유실 방어(세대 가드·재시도) — 전송·삭제·가상 붙여넣기·클라우드 모두 | 4경로 동일 |
| F-8 | 동시 작업(전송 중 Ctrl+V/드롭) | 큐잉 또는 명시 거부(**조용히 버리지 않음** — 드롭 스테이징 회수) |
| F-9 | 실행 취소 이력 상한(개수뿐 아니라 메모리) | 총 쌍 수 상한 |
| F-10 | 휴지통 삭제 배치(항목당 호출 아님) | 단일 배치 |

## 6. 설정 관리 점검 질문

| # | 질문 | 기준 |
| --- | --- | --- |
| C-1 | settings.cfg 쓰기 시점·방식 | 변경 코얼레싱(디바운스) · **원자적**(임시→rename, 옛 파일 선삭제 금지) · fsync · pid 유일 임시명 |
| C-2 | UI 스레드 저장 비용 · 무변경 저장 | dirty 비교 후 저장 · 핫패스(포커스 이탈·리사이즈) 저장 금지 |
| C-3 | Settings↔PrefValues↔State 매핑 중복 수 · 비대칭 필드 · 클램프 불일치 | 단일 원천(X-16 ②) · 클램프 한 곳 |
| C-4 | 즉시 적용 시 비싼 작업(DW 재생성·전체 무효화·재열거) 게이트 | 변경된 항목만 |
| C-5 | 세션 저장 범위·dirty 누락(펼침·컬럼·스플리터) · 종료 경로(WM_ENDSESSION) | 전 상태 dirty · 종료 시 저장 |
| C-6 | 파서 견고성(비UTF-8·BOM·`=` 주변 공백·버전 마이그레이션) | 손상 시 기본값이 아니라 **lossy 복구+백업** |
| C-7 | 다중 인스턴스(마지막 저장 승리) | 단일 인스턴스 또는 병합 |
| C-8 | 데이터 폴더 폴백 판정 비용·읽기 전용 대응 | 1회 판정 · 저장 실패 표시 |

## 7. 성능 핫패스 점검 질문

| # | 질문 | 기준 |
| --- | --- | --- |
| P-6a | 페인트가 `rcPaint`(무효 영역)를 존중하는가 | 부분 무효화 = 부분 재도장 |
| P-6b | 타이머 목록·주기·비활성/최소화 시 해제 | 비활성 시 반복 타이머 0(캐럿 포함) |
| P-6c | `update_status` 경유 비용(감시자 동기·프리뷰 재파싱·행 클론) | 호출당 O(가시 행) 이하·할당 0 |
| P-6d | 선택 복원·경로 검색 복잡도 | O(N+S) 해시 |
| P-6e | 열거·트리 작업 스레드 | 워커(UI 무정지) |
| P-6f | 캐시 상한·퇴출(레이아웃·글리프·이미지·아이콘) | LRU · 히트 시 무복사 |
| P-6g | 기동 시 즉시 열거 범위(세션 탭 전부?) | 활성 탭만 즉시 |
| P-6h | 터미널 셀 렌더(셀당 호출·셀 폭 재측정) | 런 병합·메모이즈 |
| P-6i | 노출 지표(평균 µs·first render) 외 p99·열거 시간·캐시 점유 | 추가 |

## 8. 최근 보완 기능 회귀 점검(R — TODO §7 X-40~X-50)

방법: 각 X 행의 **주장**을 코드로 대조(ⓐ 주장 = 구현 ⓑ 엣지: 빈 선택·0 크기·유니코드/전각·DPI 변경·작업 중 테마
전환·미지 설정 값 ⓒ 조용히 no-op 되는 오류 경로 ⓓ 테스트 유무) + i18n 3종 키 집합 일치. 1차 판정:

| X | 판정 | 요지 |
| --- | --- | --- |
| X-40 | gap | fsprobe·디바운스 상한·복귀 갱신 일치. **최소화 뒤 `WM_ACTIVATEAPP(FALSE)`가 FSPOLL 30s 재무장**(트림 직후 재팽창) · 서명에 **이름 미포함**(크기·mtime 같은 이름 변경 불가시) |
| X-41 | n/a | 도구 모음 치수 — 점검 범위 밖 |
| X-42 | gap | 가상 파일 경로·`(2)` 충돌·워커·undo 일치. **실패 경로 전부 조용한 no-op**(디스크립터 없음·전 항목 거부·추출 0건) · `sanitize_rel`이 예약 장치명(CON·NUL·COM1)·후행 점/공백 미거부 |
| X-43 | OK | 자식 수·캐시 프로브·`probe_skip_slow`·`is_dir` 유지 |
| X-44 | gap | 탭 stale·기준선·뷰포트 스윕·장치 변경·조상 폴백·셸 통지 있음. 스윕에 **느린 경로 게이트 없음**(UNC 폴더 3s마다 열거) |
| X-45 | OK | 메뉴+툴바 공유 명령·TOPMOST 밴드·영속·순서열 백필 테스트 |
| X-46 | gap | 레지스트리·리더·그리드·암호 모델 일치. **긴 CJK 이름 4096바이트 절단 패닉**(✔ 수정) · 목록이 UI 스레드 동기 · 창이 라이트 고정 스타일 · 모달 펌프 WM_QUIT 삼킴 · 빈 선택 Ctrl+C = 전체 복사 |
| X-47 | gap | 세로+가로 동작. **바 치수 DPI 미스케일**(150%에서 ½) · 키보드 스크롤엔 바 없음 · 창 밖 이탈 시 호버 고착 · **테스트 0** |
| X-48 | gap | 체인 파싱·DW 폴백·GDI 런 폴백·대화상자 일치. GDI **현지화 패밀리명**과 대조(한국어 OS "Malgun Gothic" 미설치 판정) · 프로세스 수명 캐시(기동 후 설치 글꼴 불가시) |
| X-49 | OK | 스킴 15·기호 색·즉시 재도장·폴백·대비 테스트. 미지 id면 콤보 **공란**(폴백 미표시) |
| X-50 | gap | 런·export·게시·4택 일치. **역순 범위 언더플로**(✔ 수정) · `TextRun`에 faint 없음(예측 텍스트가 진하게 복사) · 글꼴이 체인 1순위 원문(`first_installed` 아님) · 클립보드 실패 시 선택 해제·부분 서식 무신호 |
| i18n | OK | ko/en/ja **467키 동일 집합** · 코드 참조 182키 + 간접 테이블 전부 해석 · 누락 0 |

## 9. 점검 회차 기록

### 9-1. 2026-09-04 — 1차(기준선 수립)

**실측**(release `3,889,664B` = 0.19.0 + X-49/X-50 미배포분 · Windows 11 · 로컬 NVMe):

| ID | 결과 | 판정 |
| --- | --- | --- |
| T-1 | 13스위트 **330 green**(적대적 2건 + 회귀 1건 포함) | PASS |
| T-2 / T-3 | clippy 0 · linux check 오류 0(경고 14 = 기존 svg.rs 데드코드) | PASS |
| T-4 | 커버리지: §9-1 하단 표 | INFO |
| B-1 | 10k 폴더 기동 → 유휴 **WS 8.75MB @300s**(Private 19.3MB · 60s 시점 50.7MB = 재니터 트림 전) · 1회 | PASS |
| B-2 | 3,889,664B = **3.89MB** | PASS |
| B-3 | 22 임포트 전부 인박스 | PASS |
| P-0 | 창 표시 **430ms**(10k 인자) / **814ms**(세션 11탭 복원) · first render 1,120ms / 731ms | PASS(세션 복원은 1.2s 기준 근접) |
| P-1 | 10k **10.0ms**(1.0µs/엔트리) · 100k **99.3ms**(0.99µs) · System32 4,885 = 4.6ms | PASS |
| P-4 | **15.5MB/s** · nasty 1만 건 11ms 무패닉 | PASS |
| P-5 | 유휴 CPU: 10k 단일 탭 **0.13%**(290s 평균) · 11탭 세션+활성 창 **1.56%**(10s) | PASS(활성 세션은 상한 근접 — P-6b 참조) |
| S-1 | DllCharacteristics 0x8160 = DYNAMIC_BASE·HIGH_ENTROPY_VA·NX_COMPAT · **GUARD_CF 없음 · CET 없음** | PASS(필수) / WARN |
| S-2 | **매니페스트 없음**(requestedExecutionLevel·longPathAware·DPI 선언 부재 — DPI는 코드 호출) | FAIL |
| 스레드/핸들 | 10k 단일 탭 유휴 7스레드/418핸들 · 11탭 세션 54스레드/813핸들(감시자 탭당 스레드) | 정보 |

**커버리지(T-4, cargo-llvm-cov — 기준선)**: 라인 **38.5%**(함수 52.2%·리전 42.0%) — 크레이트별 라인: nexa-app 21.0%(28,811줄 Win32 UI) · gui 74.7% · core 97.0% · ops 95.3% · vfs 90.6% · tree 91.3% · term 89.3%
해석: nexa-app(41k줄 Win32 UI)이 전체를 끌어내린다 — 회차 비교는 **크레이트별** 수치로 한다. 최초 실행은 X-1
테스트가 잡은 VT 결함으로 중단됐고(테스트 도입 자체가 결함 2건 + R-#23 1건을 드러냄), 수정 후 재실행 값이다.

**발견 사항** — 심각도 순. 조치: ✔ 수정 커밋 / ☐ TODO 항목화(X-51~) / ○ 정책 결정 필요.

| # | 영역 | 발견 | 심각도 | 조치 |
| --- | --- | --- | --- | --- |
| 1 | F | **덮어쓰기가 새 파일을 쓰기 전에 대상을 삭제**(`nexa-ops lib.rs` copy/move `remove_dir_all(dest)` 선행) — 취소·실패 시 옛 대상 소실·부분 트리 잔존 | HIGH | ☐ |
| 2 | F | 전송 중 **DnD 드롭이 조용히 폐기**(`start_transfer` 초입 `st.transfer.is_some()` return) — `steal_volatile`로 원본을 이미 스테이징으로 옮긴 뒤라 파일이 사용자 시야에서 사라짐 | HIGH | ☐ |
| 3 | G | 플러그인 **벽시계 타임아웃 없음 + 호스트 임포트(read_at 4MB·render_svg) 연료 미과금 + UI 스레드 실행** = 정지 벡터(ADR-0005 약속 회귀) | HIGH | ☐ |
| 4 | S | ConPTY `CreateProcessW`가 **`pwsh.exe` 이름만**으로 실행(`default_shell`이 전체 경로를 찾고도 버림) — exe 폴더/CWD 바이너리 플랜팅 | HIGH | ☐ |
| 5 | S | **매니페스트 부재·DependentLoadFlags 0** — 인박스 22개 중 KnownDLLs 밖 7개(bcryptprimitives·dwrite·winhttp·crypt32·bcrypt·dwmapi·uiautomationcore) 사이드로딩 가능 | HIGH | ☐(`/DEPENDENTLOADFLAG:0x800`+매니페스트) |
| 6 | C | `save()` 비원자성(옛 파일 **선삭제** 후 rename·fsync 없음·임시명 고정 → 2인스턴스 교차) | HIGH | ☐ |
| 7 | C | 설정 변경마다 **UI 스레드 전체 직렬화+4 syscall**·무변경 저장(설정 창 포커스 이탈마다)·`apply_prefs` 끝 전체 무효화 | HIGH | ☐ |
| 8 | C | 세션 복원이 **창 생성 전 동기**(탭 전부 열거 + 펼침 경로 최대 200/탭 열거) — 끊긴 UNC면 "기동 안 됨" | HIGH | ☐ |
| 9 | P | `paint()`가 **`rcPaint` 무시** — 6px 바 무효화도 전체 장면 재도장(Invalidations 체계 무력화) | HIGH | ☐ |
| 10 | P | 터미널 캐럿 타이머가 **비활성/최소화에서도 계속**(포커스 해제 클릭 때만 kill) → #9와 결합해 백그라운드 2회/초 전체 재도장 | HIGH | ☐ |
| 11 | P | `sync_watchers`가 `update_status`(31 호출처)마다 **가시 행 전부 String 클론** · `index_of_path` O(선택×가시) 선형+할당 | HIGH | ☐ |
| 12 | F | 하위 트리 복사에서 **한 파일 실패 = 폴더 전체 중단**(`?` 전파) · 정션/디렉터리 심링크가 파일 경로로 떨어져 ACCESS_DENIED · 부분 대상 미정리 | HIGH | ☐ |
| 13 | F | 진행 통지가 **항목마다 PostMessage + 전체 Vec 클론**(O(N²)) · 계획 단계 트리 2회 순회·취소 불가 | HIGH(perf) | ☐ |
| 14 | F | `reload_both`가 매 작업·감시 틱마다 **양 패널 동기 재열거 + O(S×V) 선택 복원** | HIGH(perf) | ☐ |
| 15 | S | 무서명 배포 + 같은 페이지 SHA256SUMS(출처 증명 없음) · CFG/CET/overflow-checks 없음 | MED-HIGH | ○ 정책 / ☐ 플래그 4줄 |
| 16 | S | OAuth 클라이언트 시크릿 소스 하드코딩(`oauth.rs`) · 클라우드 임시 폴더 고정 경로·MOTW 미기록·정리 없음 | MED-HIGH | ○ / ☐ |
| 17 | S | WM_APP 5종이 `Box::from_raw(wparam/lparam)` 무검증 — 같은 세션 프로세스의 임의 포인터 해제 프리미티브 | MED | ☐(쿠키/큐) |
| 18 | G | 반환 버퍼 1MB 상한 vs 샘플 20k 엔트리(≈1.2MB) → "반환 버퍼 손상" 오표시 · 로드 오류 폐기(관측 불가) · `password` 임포트 전 플러그인 노출 · 비활성 플러그인도 실행 · 출처 검증 없음 · 서킷 브레이커 없음 · dist 드리프트 미검출 | MED | ☐ |
| 19 | C | `cloud_client_secret_*` **파싱만 되고 직렬화 안 됨**(다음 저장에 유실 — 기본 시크릿이 가림) · WM_ENDSESSION 미처리 · 다중 인스턴스 마지막 저장 승리 · 비UTF-8 = 전체 초기화 · `col_layout` 변경이 session dirty 안 됨 | MED | ☐ |
| 20 | P | 프리뷰가 `update_status`마다 재파싱 · 이미지 캐시 히트 시 전체 복사 · 레이아웃 캐시 LRU 없음(4096 전체 소거) · `term_cell_w` 매 프레임 레이아웃 생성 · FSPOLL이 뷰포트 폴더 전부 3s마다 열거(UNC 미제외) | MED | ☐ |
| 21 | X | VT 파라미터 누적 **곱셈 오버플로**(디버그 패닉·릴리스 래핑) · `ESC[999999999L` IL/DL **n회 회전 = CPU 소진** — X-1 테스트가 발견 | MED | ✔ `fix(term)` 포화 누적+65535 상한·행 수 클램프 |
| 22 | S | CSPRNG 실패 시 xorshift 폴백·sha256 실패 시 빈 challenge · WinHTTP 리디렉션 시 Bearer 전달 · DPAPI 부가 엔트로피 없음 · 완화 정책 미적용 | LOW-MED | ☐ |
| 23 | R | **`get_runs` 역순 범위 `el - sl + 1` 언더플로**(X-1 테스트가 호출·디버그 패닉/릴리스 abort) · **압축 엔트리 긴 CJK 이름 `truncate(4096)` 문자 경계 패닉**(도크 미리보기 중 UI 스레드) | HIGH | ✔ `fix(term)`·`fix(vfs)` + 회귀 테스트 |
| 24 | R | 최소화 뒤 `WM_ACTIVATEAPP(FALSE)`가 `TIMER_FSPOLL` 재무장(트림 무력화) · fsprobe 서명에 이름 미포함 · 뷰포트 스윕 느린 경로 미게이트 · `SCAN_CAP` 경계 진동 시 영구 changed | MED | ☐ |
| 25 | R | 터미널 Ctrl+C: 클립보드 열기 실패도 `sel=None`·HTML/RTF 부분 실패 무신호 · `TextRun` faint 없음 · 글꼴 이름 = 체인 원문 | MED | ☐ |
| 26 | R | `Kind::Select` 미지 값이면 콤보 공란(폴백 값 미기록) · 오버레이 바 DPI 미스케일·키보드 스크롤 무표시·호버 고착·테스트 0 | MED | ☐ |
| 27 | R | 가상 붙여넣기 실패 전부 무응답 · `sanitize_rel` 예약 장치명 미거부 · 압축 창 라이트 고정·WM_QUIT 삼킴·빈 선택 전체 복사·목록 UI 스레드 동기 · fontchain 현지화 이름·수명 캐시 | MED-LOW | ☐ |

**견고한 부분(회귀 금지)**: 단일 전송 퍼널·바이트/항목 진행·취소(항목 간+4MiB마다) · 휴지통 단일 배치+사후 존재 diff · `steal_volatile` · 완료 통지 재시도 · `sanitize_rel` · 세션 저장 디바운스 · 관대한 클램프 파서 · 데이터 폴더 1회 판정 · 행 가상화·`Invalidations` 모델 · 정렬은 열거 시 1회 · 감시자 수명(OVERLAPPED·overflow 복구·자가 치유) · 아이콘 4개/80ms 동기/비동기 분리 · 재니터 트림 · 플러그인 샌드박스(임포트 6개 전부 대상 파일 한정·연료·메모리·8MB) · `Secret` zeroize · OAuth PKCE/state/루프백/TLS 강제 · 토큰 비로그 · zip-slip 정규화(nexa-vfs unsafe 0).

**다음 회차 보완 과제**: ① 커버리지 크레이트별 기준선 확정 ② §2-2 보강 대상 테스트 추가 ③ audit.ps1에 PE
DependentLoadFlags·CET·임포트∖KnownDLLs 판정 추가 ④ 플러그인 A22/A23 스톱워치 테스트 ⑤ 발견 #1~#14 수정
후 F/C/P 재점검 ⑥ p99 페인트·열거 시간 지표 노출 후 P 기준 재설정.
