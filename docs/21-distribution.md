# 21 · 배포 설계 — 포터블(기본) + 설치형(보조)

> **작성: 2026-07-16** — 사용자 요청("최소파일 배포가 가능한 Portable 구성 설계 +
> exe 설치형 배포도 가능하도록 GitHub Actions 구성") 이행. [DR-3 개정](10-decision-record.md)
> 의 설계 상세. 릴리스 절차 SSOT = [18-build-and-test.md](18-build-and-test.md) §5.

## 1. 채널 개요

| 채널 | 산출물 | 대상 | 데이터 위치 |
| --- | --- | --- | --- |
| **포터블(기본)** | `NexaDir2-<버전>-win-x64.exe` **1파일** | USB·폴더 배치·무설치 | exe 옆 `data\` |
| **설치형(보조)** | `NexaDir2-Setup-<버전>.exe` | 시작 메뉴·바탕화면·제거 목록 원하는 사용자 | exe 옆 `data\`(사용자별 설치) 또는 `%LOCALAPPDATA%\NexaDir2\data`(폴백) |

두 채널 모두 **버전 태그 push 1회**로 GitHub Release에 자동 첨부된다(release.yml).

## 2. 포터블 — 최소파일 규율

- **배포 파일 = exe 1개가 전부.** DLL·설정·리소스 동봉 없음(임포트=OS 인박스만 — B3
  게이트, i18n en/ko 내장, 아이콘/폰트 리소스 임베드).
- 영속물은 **첫 저장 시** exe 옆 `data\` 자동 생성: `settings.cfg`·`session.cfg`·
  `renames\*.cfg`(프리셋)·`lang\*.lang`(선택 — 사용자 오버라이드용, 없어도 동작).
- 제거 = exe 삭제(+원하면 `data\` 삭제). 레지스트리·%APPDATA% 흔적 0
  (설치형 폴백 경로를 쓴 적이 없는 한).

## 3. 설치형 — Inno Setup (installer/nexa.iss)

- **사용자별 설치가 기본**(PrivilegesRequired=lowest + 다이얼로그) —
  `%LOCALAPPDATA%\Programs\NexaDir2`에 설치, **관리자 불요·UAC 없음**(VS Code 방식).
  이 경우 exe 옆이 쓰기 가능하므로 데이터도 포터블과 동일하게 exe 옆 `data\`.
- 관리자 선택 시 `Program Files\NexaDir2` — 이때 exe 옆은 쓰기 불가이므로
  앱의 **data_dir 폴백**이 동작(§4).
- 구성: 시작 메뉴 항목 + 바탕화면 아이콘(선택 Task) + 제거기(제거 목록 등재) +
  LICENSE 동의 페이지. **설치 파일도 exe 1개만 복사**(최소파일 공유).
  설치 UI는 영어(Korean.isl은 Inno 공식 미포함 — 필요 시 번역 파일 동봉 후 추가).
- 제거 시 **사용자 데이터는 보존**(재설치 복원 기대 — 명시 삭제 없음).
- 무서명(DR-3) — SmartScreen 경고 감수, 인증서 확보 시 서명 단계 추가.

## 4. data_dir 폴백 (config.rs)

```
data_dir() [프로세스당 1회 판정 — OnceLock]
  ├─ 후보 = exe 옆 data\ → 생성+쓰기 프로브 성공 → 그대로 (포터블·사용자별 설치)
  └─ 실패(Program Files·읽기 전용 매체 등)
       └─ %LOCALAPPDATA%\NexaDir2\data (설치형 폴백 — 부재 시 후보 유지)
```

- 프로브 = 디렉터리 생성 + 임시 파일 1개 쓰기/삭제(ACL·읽기 전용 감지).
- 회귀 테스트: `choose_data_dir_portable_first_installed_fallback`
  (쓰기 가능=그대로 · 불가능 경로=LOCALAPPDATA 폴백).
- 한계: 읽기 전용 매체(CD 등)에서 LOCALAPPDATA로 폴백되면 "포터블인데 흔적이
  LOCALAPPDATA에 남는" 케이스 — 의도된 우아한 저하(설정 저장 불가보다 낫다).

## 5. CI (release.yml — M5-2 확장)

태그 push → 게이트(test·B2 exe ≤10MB·B3 인박스 임포트) → 포터블 exe 명명 →
**ISCC로 설치형 빌드**(windows-latest 내장 Inno Setup 6, `/DAppVersion` 주입) →
Release에 **포터블 + 설치형 동시 첨부**. workflow_dispatch = 게이트+아티팩트만.

## 6. 검증 체크리스트 (실기 QA)

- [ ] 포터블: 임의 폴더에서 실행 → `data\` 생성·설정 영속.
- [ ] 설치형(사용자별): 설치 → 시작 메뉴 실행 → exe 옆 `data\` 사용.
- [ ] 설치형(관리자): Program Files 설치 → `%LOCALAPPDATA%\NexaDir2\data` 폴백 확인.
- [ ] 제거 → 데이터 보존·재설치 시 설정 복원.
- [ ] 태그 push → Release에 산출물 2종 첨부(다음 버전 태그에서 확인).
- [ ] `choco install nexa-dir` → Program Files 설치·시작 메뉴 등재 → `choco uninstall nexa-dir` → 데이터 보존.

## 7. Chocolatey — 3번째 채널 (packaging/chocolatey, 2026-07-19)

패키지 ID **`nexa-dir`**. 설치형 채널의 래퍼 — 별도 산출물을 만들지 않는다.

- **바이너리 미동봉.** nupkg는 스크립트만 담고, 설치 시 **GitHub Release의
  `NexaDir-Setup-<버전>.exe`를 SHA-256 검증 후 내려받아** 무인 설치한다.
  근거: PolyForm NC = 비-FOSS → 커뮤니티 저장소는 공식 URL 다운로드가 정석이며,
  nupkg 용량도 스크립트 수 KB로 유지된다.
- **머신 전역 설치.** choco는 관리자로 돌지만 `nexa.iss`의 `PrivilegesRequired=lowest`
  기본값은 *사용자별* 설치로 판정되므로, 설치 인자에 **`/ALLUSERS`를 명시**한다
  (이를 위해 `PrivilegesRequiredOverridesAllowed`에 `commandline` 추가).
  결과적으로 데이터는 `%LOCALAPPDATA%\NexaDir2\data` 폴백(§4)을 탄다.
- **제거**: `chocolateyuninstall.ps1`이 제거 레지스트리 키를 찾아 무인 실행 —
  §3과 동일하게 **사용자 데이터는 보존**.
- **CI**(release.yml): 설치형 빌드 직후 SHA-256을 계산해
  `chocolateyinstall.ps1`의 `{{VERSION}}`·`{{CHECKSUM64}}`를 치환 → `choco pack` →
  **`CHOCO_API_KEY` 시크릿이 있을 때만** `choco push`. 시크릿이 없으면 팩까지만
  수행하고 nupkg를 워크플로 아티팩트로 남긴다(수동 push 가능).

### 최초 등록 절차 (1회 — 사용자 수행)

1. <https://community.chocolatey.org> 계정 생성 → **Account → API Key** 복사.
2. 저장소 **Settings → Secrets and variables → Actions**에 `CHOCO_API_KEY` 등록.
3. 다음 버전 태그 push → 자동 pack·push. (2026-07-19 시크릿 등록 완료 → **`0.8.1`이
   최초 게시 버전**.)
4. **첫 패키지는 모더레이션 심사 대기**(수일~2주). 심사 통과 후에는 같은 ID의
   후속 버전이 자동 승인 경로를 탄다. 지적 사항은 패키지 페이지 코멘트로 온다.

> 심사 포인트: `tools/VERIFICATION.txt`(다운로드 검증 방법)·`requireLicenseAcceptance=true`
> (NC 라이선스)·vendor 본인이 메인테이너임을 명시 — 모두 반영 완료.

**게시 이력**: `0.8.1` 최초 push 성공(2026-07-19 — 모더레이션 큐 진입).

## 8. winget — 4번째 채널 (packaging/winget, 2026-07-19)

패키지 ID **`SosomLab.NexaDir`**. Chocolatey와 마찬가지로 **설치형 exe를 그대로 참조**하는
래퍼 — 새 산출물 없음. 다만 winget은 자체 저장소가 아니라 **microsoft/winget-pkgs에 PR**로
매니페스트를 등록하는 구조라, nupkg 같은 패키지 파일 자체가 없다.

- 매니페스트 3종(스키마 1.12.0) = `installer` · `locale.en-US` · `version`.
  저장소 사본은 `packaging/winget/<버전>/`, 실제 제출 경로는 winget-pkgs의
  `manifests/s/SosomLab/NexaDir/<버전>/`.
- **user·machine 두 스코프 모두 제공**: `InstallerSwitches.Custom`에 각각
  `/CURRENTUSER`·`/ALLUSERS`. §7의 `commandline` 허용이 여기서도 그대로 쓰인다
  (`winget install --scope machine` 지원).
- `AppsAndFeaturesEntries.ProductCode` = **`{AppId}_is1`**(Inno 규칙) —
  업그레이드·제거 상관에 필요.
- `InstallerSha256`은 Release 자산 실측(맥에서 다운로드 후 `shasum -a 256`으로 대조).
- 심사 = winget-pkgs PR의 자동 검증 봇(설치·제거 실기 테스트) + 리뷰어 승인.
- **제출 이력**: [winget-pkgs#404528](https://github.com/microsoft/winget-pkgs/pull/404528)
  (0.8.1 최초 등록, 2026-07-19). 맥 환경이라 `winget validate`/`winget install` 로컬
  검증은 불가 — 스키마 수기 대조 + YAML 파싱까지만 하고 **CI 검증에 의존**한다.
  (PR 체크리스트에도 그대로 명시했다. CLA 미서명이면 봇이 요청한다.)

### 다음 버전 절차

매니페스트는 아직 **수동**이다(Chocolatey처럼 CI 자동화하지 않음 — 외부 저장소 PR이라
포크·토큰 권한이 따로 필요). 버전 승격 시 `packaging/winget/<새 버전>/`을 복사해
`PackageVersion`·`InstallerUrl`·`InstallerSha256`·`DisplayVersion`·`ReleaseDate`·
`ReleaseNotesUrl`를 갱신하고 winget-pkgs에 PR한다.
