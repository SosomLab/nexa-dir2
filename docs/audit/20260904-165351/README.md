# 점검 회차 보고 — 2026-09-04 16:53:51 (1차 · 기준선 수립)

> 규격: [docs/29-audit-checklist.md](../../29-audit-checklist.md). 이 폴더는 **그 회차의 결과 원본**이다(규격 문서 §9에는 요약만).
> 대상: `main` `9719e71` 기준 소스 · release `nexa-app.exe` 3,889,664B(0.19.0 + X-49/X-50 미배포분) · Windows 11 Pro 26200 · 로컬 NVMe.
> 폴더 구성 — `README.md`(이 보고) · `01~06-*.md`(영역별 조사 원본, file:line 증거) · `audit-quick.log`(하네스 출력) ·
> `coverage-llvm-cov.txt`(커버리지 원본) · `idle-memory.log`(B1 프로토콜 원본).

## 1. 자동 판정(`scripts/audit.ps1 -Quick`)

| ID | 항목 | 판정 | 상세 |
| --- | --- | --- | --- |
| T-1 | cargo test --workspace | PASS | 13스위트 330 passed / 0 failed |
| T-2 | clippy | PASS | warnings 0 · errors 0 |
| T-3 | linux target check | PASS | errors 0(경고 14 = 기존 svg.rs 데드코드) |
| T-4 | 라인 커버리지 | INFO | 라인 38.5% · 함수 52.2% · 리전 42.0% — 크레이트별 §3 |
| B-0 | release build | PASS | |
| B-2 | exe ≤ 10MB | PASS | 3,889,664B = 3.89MB |
| B-3 | 임포트 = 인박스 | PASS | 22개 전부 화이트리스트 |
| S-1 | PE 완화 | PASS / WARN | 0x8160 = DYNAMIC_BASE·HIGH_ENTROPY_VA·NX_COMPAT · **GUARD_CF 없음**(WARN) · CET 없음 |
| S-2 | 매니페스트 | WARN | requestedExecutionLevel 미발견(매니페스트 미포함) |
| B-1 | 유휴 메모리 | 별도 §2 | -Quick에서는 SKIP |

## 2. 실측

| 항목 | 값 | 기준 | 판정 |
| --- | --- | --- | --- |
| B-1 유휴 WorkingSet(10k 폴더 인자 기동, 300s) | **8.75MB**(Private 19.31MB · 스레드 7 · 핸들 418) | ≤ 30MB | PASS |
| B-1 경과 | 10s 51.05 / 60s 50.69 / 180s 6.02 / 300s 8.75MB — 60s 재니터 트림 전후 | | |
| P-0 창 표시 | 430ms(10k 인자) · 814ms(11탭 세션 복원) · first render 1,120 / 731ms | ≤ 1.5s | PASS |
| P-1 열거(median 5회) | 10k **10.0ms**(1.00µs/엔트리) · 100k **99.3ms**(0.99µs) · System32 4,885 = 4.6ms | ≤ 4µs/엔트리 | PASS |
| P-4 VT 파서 | **15.5MB/s**(32MB) · nasty 1만 건 11ms 무패닉 | ≥ 10MB/s | PASS |
| P-5 유휴 CPU | 0.13%(단일 탭 290s 평균) · 1.56%(11탭 세션·활성 창 10s) | ≤ 2% | PASS(활성 세션 상한 근접) |
| 세션 복원 리소스 | 11탭: WS 66MB(5s) · 스레드 54 · 핸들 813 | 정보 | 감시자 탭당 스레드(→ 03 성능 #5) |

## 3. 커버리지(cargo-llvm-cov, 라인)

| 크레이트 | 줄 | 커버 |
| --- | --- | --- |
| nexa-app | 28,811 | 21.0%(Win32 UI — 구조적 하한) |
| nexa-gui | 5,253 | 74.7% |
| nexa-core | 99 | 97.0% |
| nexa-ops | 1,869 | 95.3% |
| nexa-vfs | 1,958 | 90.6% |
| nexa-tree | 965 | 91.3% |
| nexa-term | 974 | 89.3% |
| **전체** | 39,929 | **38.5%** |

## 4. 조사(정적 리뷰 6영역) — 발견 27건, HIGH 14

영역별 원본은 01~06 파일. 심각도순 요약과 조치 상태는 [29 §9-1](../../29-audit-checklist.md) 표와 동일.

| 영역 | 파일 | HIGH | 핵심 |
| --- | --- | --- | --- |
| 파일 조작(다수 파일) | [01-file-ops.md](01-file-ops.md) | 5 | 덮어쓰기 대상 선삭제 · 전송 중 드롭 폐기 · 하위 트리 한 파일 실패 = 중단 · O(N²) 진행 · reload_both |
| 설정·세션 관리 | [02-settings.md](02-settings.md) | 3 | save() 비원자 · 변경마다 UI 스레드 전체 저장 · 세션 복원 창 생성 전 동기 |
| 성능 핫패스 | [03-performance.md](03-performance.md) | 3 | paint rcPaint 무시 · 캐럿 타이머 · sync_watchers 클론 |
| 최근 보완 기능 회귀 | [04-features.md](04-features.md) | 1(✔) | get_runs 언더플로·CJK 이름 절단 패닉(수정) · X-40/42/44/46/47/48/50 gap |
| 플러그인 | [05-plugins.md](05-plugins.md) | 1 | 벽시계 타임아웃 없음 + 임포트 미과금 + UI 스레드 |
| 보안 | [06-security.md](06-security.md) | 2 | pwsh.exe 이름 실행 · 매니페스트/DependentLoadFlags 없음 |

## 5. 즉시 수정(이 회차 안에서 — 테스트 도입이 강제)

| 결함 | 커밋 |
| --- | --- |
| VT 파라미터 곱셈 오버플로 → 포화 누적 + 65535 상한 | `b279df7` |
| `ESC[999999999L` IL/DL n회 회전(CPU 소진) → 행 수 클램프 | `b279df7` |
| `get_runs` 역순 범위 `el-sl+1` 언더플로 | `b279df7` |
| 압축 엔트리 이름 4096바이트 절단 UTF-8 경계 패닉 → 경계 후퇴 + 회귀 테스트 | `b279df7` |

## 6. 후속

- 발견 → [TODO](../../TODO.md) X-52(파일 조작) · X-53(설정) · X-54(렌더/타이머) · X-55(보안) · X-56(플러그인) · X-57(잔여).
- 정책 결정 필요: 코드 서명·출처 증명 · 소스 하드코딩 OAuth 시크릿.
- 다음 회차: 커버리지 크레이트별 기준선 확정 · 적대적 테스트 보강(VT fuzz·플러그인 반환 버퍼·메모리 트랩·비UTF-8 설정) ·
  audit.ps1에 DependentLoadFlags/CET/임포트∖KnownDLLs 판정 · 플러그인 A22/A23 스톱워치.

## 7. 조치 계획 — 위험도별(바로 조치 대상 · 방법 · 검증)

### 7-1. 즉시(이번 주 — 데이터 손실·코드 실행 경로. HIGH)
| 순위 | 대상 | 조치 방법 | 검증 | 규모 |
| --- | --- | --- | --- | --- |
| 1 ✔ `c23ba62` | 덮어쓰기 대상 선삭제(01 #1) | `copy/move_onto_with_progress`: 대상 폴더에 `.<name>.nexa-tmp-<pid>`로 쓰고 성공 시 `ReplaceFileW`/rename 스왑, 실패·취소 시 임시만 삭제 | nexa-ops 테스트: 중간 취소 후 옛 대상 무결·임시 잔존 0 | 중 |
| 2 ✔ `07b28db` | 전송 중 드롭 조용히 폐기(01 #2) | `start_transfer`가 busy면 스테이징 파일을 **원위치 복귀**(rename back) 후 상태바 "전송 중 — 잠시 후 다시" 표시. 이후 큐잉(X-52) | 실기: 대용량 전송 중 7-Zip 드롭 → 파일 생존 확인 | 소 |
| 3 ✔ `b16e96b` | ConPTY `pwsh.exe` 이름 실행(06 #1) | `default_shell()`이 찾은 전체 경로를 반환하고 `CreateProcessW(lpApplicationName=경로)` · `SetSearchPathMode(SAFE)` 1회 | exe 옆에 가짜 pwsh.exe 두고 터미널 열기 → 실행 안 됨 | 소(5줄) |
| 4 ✔ `3e829f7`(overflow-checks 보류) | DLL 사이드로딩·매니페스트(06 #2) | `.cargo/config.toml`에 `-C link-arg=/DEPENDENTLOADFLAG:0x800`·`-C control-flow-guard=yes`·`-C link-arg=/CETCOMPAT` · `[profile.release] overflow-checks=true` · `build.rs`에 매니페스트(asInvoker·uiAccess=false·longPathAware·PerMonitorV2·supportedOS) | audit.ps1 S-1 GUARD_CF 통과·S-2 asInvoker 검출 · B-3 무변 · B-2 크기 · 릴리스 스모크 | 소 |
| 5 | 플러그인 무타임아웃(05 #1) | 호스트 임포트마다 `Instant` 경과 검사(예: 500ms) + `caller.consume_fuel` 과금 · 반복 실패 3회면 세션 비활성 | `nx_loop` 테스트에 시간 상한 단언 · 1초 내 오류 | 중 |
| 6 | 설정 저장 비원자(02 #2) | `save()`: 옛 파일 선삭제 제거 · `File`+`sync_all` · `{name}.{pid}.tmp` · `cloud_client_secret_*` 직렬화 추가 | config 왕복 테스트 단언 추가 · 저장 중 프로세스 kill 실험 | 소 |

**7-1 조치 결과(같은 날)**: 1~4 수정 커밋 · 5·6 대기. 재판정 회차 [`docs/audit/20260904-174325/`](../20260904-174325/summary.md) = PASS=8 FAIL=0 WARN=0 SKIP=2
(S-1 GUARD_CF 통과·S-2 asInvoker 검출 — WARN 2건 소멸). 실측: DllCharacteristics 0xC160 · DependentLoadFlags 0x0800 ·
CET compat · exe 3,926,528B(+37KB) · B3 통과 · 창 447ms. 테스트: ops 36(덮어쓰기 취소 보존 신규)·app 130(steal→restore·default_shell 절대 경로).

### 7-2. 단기(다음 릴리스 전 — 성능 정지·UX 결함. HIGH/MED)
| 대상 | 조치 방법 | 검증 |
| --- | --- | --- |
| `paint()` rcPaint 무시 + 캐럿 타이머(03 #1·#2) | `IntersectClipRect(rcPaint)`·부분 BitBlt · `WM_ACTIVATEAPP(FALSE)`/최소화에 `TIMER_TERM_CARET` kill·재활성에 re-arm | 백그라운드 유휴 CPU 0% · 제목줄 평균 µs 하락 |
| `sync_watchers` 클론·`index_of_path` O(S×V)(03 #3·#4, 01 #6) | 무할당 접근자 · 소문자 경로 HashMap 1회 · `reload_both`는 영향 패널만 | 100k 폴더 Ctrl+A 후 리로드 < 100ms |
| 세션 복원 동기(02 #4) | 창 생성 → 활성 탭만 열거 → 나머지 첫 전환 시 | 끊긴 UNC 탭 포함 세션에서 창 1.5s 내 표시 |
| 설정 변경마다 전체 저장·전체 무효화(02 #3·#7) | `last_saved` 비교 + 디바운스 · `changed` 플래그별 무효화 | 설정 창 Tab 이동 시 파일 mtime 불변 |
| 하위 트리 한 파일 실패 = 중단·정션(01 #5) · 진행 O(N²)(01 #3) · 계획 2회 순회(01 #4) · 오류 목록 표시(01 #7) | 엔트리별 오류 수집·재분석점 분기 · 30Hz 스로틀·스칼라 · Move&&same_volume 계획 생략 · 삭제 경로의 목록+재시도 모달 재사용 | nexa-ops 테스트(잠긴 파일 1개 포함 폴더 복사 = 나머지 성공) · 50k 소파일 복사 CPU |
| 플러그인 OUT_CAP·오류 표시·능력별 링커·비활성 미실행(05 #2~#5) | 상한 정합·설정 창에 로드 오류·`caps`로 링커·`is_disabled` 선판정 | 픽스처 테스트 |
| 최근 기능 잔여(04 #3~#12) | FSPOLL 재무장 가드 · fsprobe 이름 해시 · Ctrl+C 실패 시 선택 유지 · faint/first_installed · 콤보 폴백 · 오버레이 DPI/키보드/호버 · 가상 붙여넣기 신호 | 각 단위 테스트(X-47 테스트 0 → 추가) |
| WM_APP 포인터(06 #7) · 클라우드 임시 폴더/MOTW(06 #6) · CSPRNG 폴백(06 #9) | 프로세스 랜덤 쿠키 · pid+seq 폴더·`Zone.Identifier` 기록·종료 시 정리 · 실패 = `AuthError` | 타 프로세스 PostMessage 무해 · 다운로드 파일 스트림 확인 |

### 7-3. 정책 결정(사용자) 후 착수
| 대상 | 선택지 |
| --- | --- |
| 코드 서명·출처 증명(06 #3) | ⓐ 무서명 유지(현행 문서화) ⓑ GitHub attestation(무료)만 ⓒ Authenticode(비용·Defender 오탐 해소 효과 큼) |
| OAuth 클라이언트 시크릿 하드코딩(06 #5) | ⓐ 유지(설치형 앱 관행) ⓑ 회전 + 빌드 시 주입 + 히스토리 정리 |
| 플러그인 출처(05 #6) | ⓐ 동봉본 SHA-256 매니페스트 + 미지 모듈 경고 ⓑ 서명 |
| 단일 인스턴스(02 #6) | 명명 뮤텍스로 두 번째 실행은 기존 창 활성화 vs 다중 허용+병합 저장 |

### 7-4. 위험도 판정 기준(이 회차에서 쓴 것 — 29 §9에 규격화)
- **HIGH**: 사용자 데이터 손실 가능 · 코드 실행/권한 경계 침해 가능 · 앱 정지(UI 스레드 무한/장시간) · 예산(DR-2) 위반.
- **MED**: 기능 오동작·상태 불일치·성능 저하(정지 아님)·관측 불가.
- **LOW**: 방어 심층·표기·정리.
- 조치 순서 = HIGH 중 데이터 손실 → 코드 실행 → 정지 → MED(UX 영향 큰 순) → 정책 결정 항목.
