# 06. 보안 조사 — 권한 축소·샌드박스·완화 기술 — 2026-09-04

대상: `build.rs`·`.cargo/config.toml`·`Cargo.toml` · `conpty.rs`·`launcher.rs`·`shellmenu.rs` · `secret.rs`·`oauth.rs`·`cloudfs.rs` · `preview/*` · `nexa-vfs/archive` · `clipboard.rs`·`dnd.rs`·`shellnotify.rs` · `release.yml`·`installer/nexa.iss` + release exe PE 실측.

**총평**: 권한은 잘 지킨다(승격 없음·`PrivilegesRequired=lowest`·토큰 API 0건). 설계된 샌드박스 2개(wasmi·압축 경로 정규화)는 좋다. 갭은 **로드 시점 신뢰**에 몰려 있다 — 매니페스트·DLL 검색 하드닝·자식 프로세스 이름·서명·CFG/CET.

## 발견(위험순)
| # | 위험 | 위치 | 문제 → 수정 |
| --- | --- | --- | --- |
| 1 | HIGH | `conpty.rs:116-137`, `:280-294` | `CreateProcessW(lpApplicationName=NULL, "pwsh.exe")` — `default_shell()`이 전체 경로를 찾고도 파일명만 반환. NULL 앱 이름 + 이름 검색 = **앱 폴더·CWD 우선** → 바이너리 플랜팅. `SetSearchPathMode` 없음. (`launcher.rs:38-45`는 전체 경로 — 대조). → 전체 경로를 lpApplicationName으로 |
| 2 | HIGH | `build.rs:41-79` · PE 실측 | `.rc`에 ICON+VERSIONINFO만 — **매니페스트 없음**(requestedExecutionLevel·longPathAware·DPI 미선언) · LoadConfig `DependentLoadFlags=0`(0x0800 아님) · `SetDefaultDllDirectories` 없음 · delay-import 없음 → 임포트 22 중 KnownDLLs 밖 **7개**(bcryptprimitives·uiautomationcore·dwmapi·crypt32·bcrypt·dwrite·winhttp) exe 폴더에서 하이재킹 가능(bcryptprimitives는 std가 main 전 로드 = 링크 시 해결만 가능). → `-C link-arg=/DEPENDENTLOADFLAG:0x800` + 매니페스트 |
| 3 | HIGH(공급망) | `release.yml:80-96` · PE cert 0 | 무서명 + 같은 릴리스 페이지의 SHA256SUMS(변조 시 함께 변조) · attestation 없음 · Defender ML 오탐(`packaging/av-false-positive.md`)이 같은 원인의 증상 |
| 4 | MED-HIGH | PE 0x8160 · `.cargo/config.toml` · `Cargo.toml` | GUARD_CF 없음(GuardFlags 0x100 = 테이블만) · CETCOMPAT 없음 · `overflow-checks` 미설정 — unsafe 573곳·압축/이미지 인프로세스 파싱. → `-C control-flow-guard=yes`·`/CETCOMPAT`·`overflow-checks=true` |
| 5 | MED-HIGH | `oauth.rs:97, :117` | Google/Dropbox 클라이언트 시크릿 소스 하드코딩(공개 저장소·히스토리 영구). settings.cfg에는 안 씀(정상). → 정책 결정(회전·별도 주입) |
| 6 | MED | `win.rs:3711-3712`, `:7901-7916` | 클라우드 다운로드 임시 폴더 **고정 경로**(`%TEMP%\NexaDir\cloud`, pid/시퀀스 없음 — vpaste·dnd는 있음) + `ShellExecuteW(open)` 자동 실행 + **MOTW(Zone.Identifier) 미기록** → 브라우저보다 경고 적음 · 정리 없음 |
| 7 | MED | `win.rs:7802, 7841, 7903, 7939, 7969` · `:7872` | WM_APP 0x8004/0x800E/0x800F/0x8010/0x8011 핸들러가 `Box::from_raw(wparam/lparam)` 무검증 · `shellnotify::release_payload`에 임의 HANDLE → 같은 세션 프로세스가 `PostMessage`로 임의 포인터 해제(UIPI가 저IL만 차단). → 프로세스 랜덤 쿠키/정수 토큰 큐 |
| 8 | MED | `preview/mod.rs:265-275` · `config.rs:321-376` | 플러그인 폴더 2곳 모두 사용자 쓰기 가능(`dir_writable`은 ACL 검사 아님) + 서명/허용목록 없음. 완화: 샌드박스가 좁아(임포트 6·대상 파일만·네트워크/쓰기 없음) 폭발 반경 작음 — 단 #2 DLL과 exe도 같은 폴더 |
| 9 | MED | `oauth.rs:212-234`, `:241-271` | `BCryptGenRandom` 실패 시 시간+주소 xorshift 폴백(PKCE verifier·state 둘 다) · `sha256` 실패 시 빈 challenge. → 하드 오류 |
| 10 | LOW-MED | `oauth.rs:963-1001` | `WINHTTP_OPTION_REDIRECT_POLICY` 미설정 → 3xx에 Bearer 재전송 · `SECURE_PROTOCOLS` OS 기본 |
| 11 | LOW-MED | `secret.rs:74`, `:21-32` | `CryptProtectData` 부가 엔트로피 None(같은 사용자 프로세스가 복호 가능) · `data\secrets` 기본 ACL |
| 12 | LOW-MED | `shellmenu.rs:378` · `gdipctx.rs:60-64` | 셸 확장 인프로세스 로드·GDI+ 디코드에 `SetProcessMitigationPolicy` 없음(Signature/ImageLoad/ExtensionPointDisable). verb는 ordinal(정상) |

그 외: `panic=abort`(파서 패닉 = 프로세스 종료, 가용성) · `launcher.rs:146-168` `%path%` 미인용(.bat/.cmd 대상만 위험) · `win.rs:3737-3742` 클라우드 파일명 `rsplit('/')`에 `sanitize_rel` 미적용.

## 이미 옳은 것(회귀 금지)
승격/가장/고접근 핸들 0건 · ConPTY `bInheritHandles=false`(`conpty.rs:131`) · OAuth PKCE S256·state 비교·루프백 127.0.0.1/::1 한정·5s 타임아웃 · `WINHTTP_FLAG_SECURE` 무조건·보안 플래그 무시 없음 · 토큰 비로그(20 출력처 검토) · `Secret` 규율 · zip-slip 정규화·상한(nexa-vfs unsafe 0) · 디스크립터 `sanitize_rel`·`cItems` 클램프·`read_unaligned` · B3 임포트 게이트(CI).

## 체크리스트 판정(A~G — 규격은 29 §4)
A1 FAIL(매니페스트 없음) · A2/A3 PASS · A4/A5 FAIL · A6 FAIL · B1 PASS · B2/B3/B4/B5 FAIL · B6/B7 PASS · B8 FAIL · B9/B10 FAIL(정책) · C1 FAIL(conpty)/PASS(launcher) · C2/C3/C4 PASS · C5 PARTIAL · C6 FAIL · D1 PASS · D2 FAIL · D3 UNKNOWN · D4/D5 PASS · D6 FAIL · D7/D8 PASS · D9 FAIL · D10 PASS · D11 FAIL · D12 PASS · E1/E2/E3/E6 PASS · E4 미검증 · E5 FAIL · F1/F2 PASS · F3 PARTIAL · F4/F5 FAIL · F6/F7 PASS(std 위임) · F8 UNKNOWN · G1 FAIL · G2 PASS · G3 FAIL(cargo audit·툴체인 고정 없음) · G4 FAIL(위협 모델 문서 없음).

## 효과 대비 비용 최상위
1. `.cargo/config.toml` 4줄: `-C control-flow-guard=yes` · `-C link-arg=/CETCOMPAT` · `-C link-arg=/DEPENDENTLOADFLAG:0x800` · `[profile.release] overflow-checks=true` → #2·#4. budget-b3에 PE 단언 추가.
2. `default_shell()`이 찾은 전체 경로를 `lpApplicationName`으로 → #1(5줄).
3. `build.rs`에 매니페스트(asInvoker·uiAccess=false·longPathAware·PerMonitorV2·supportedOS) → A1/A4/A5.
4. WM_APP 포인터 핸들러 쿠키 → #7.
