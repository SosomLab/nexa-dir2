# 21 · 배포 설계 — 포터블(기본) + 설치형(보조)

> **작성: 2026-07-16** — 사용자 요청("최소파일 배포가 가능한 Portable 구성 설계 +
> exe 설치형 배포도 가능하도록 GitHub Actions 구성") 이행. [DR-3 개정](10-decision-record.md)
> 의 설계 상세. 릴리스 절차 SSOT = [18-build-and-test.md](18-build-and-test.md) §5.

## 1. 채널 개요

| 채널 | 산출물 | 대상 | 데이터 위치 |
| --- | --- | --- | --- |
| **포터블(기본)** | `NexaDir-<버전>-win-x64.exe` **1파일** | USB·폴더 배치·무설치 | exe 옆 `data\` |
| **설치형(보조)** | `NexaDir-Setup-<버전>.exe` | 시작 메뉴·바탕화면·제거 목록 원하는 사용자 | exe 옆 `data\`(사용자별 설치) 또는 `%LOCALAPPDATA%\NexaDir\data`(폴백) |

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
  `%LOCALAPPDATA%\Programs\NexaDir`에 설치, **관리자 불요·UAC 없음**(VS Code 방식).
  이 경우 exe 옆이 쓰기 가능하므로 데이터도 포터블과 동일하게 exe 옆 `data\`.
- 관리자 선택 시 `Program Files\NexaDir` — 이때 exe 옆은 쓰기 불가이므로
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
       └─ %LOCALAPPDATA%\NexaDir\data (설치형 폴백 — 부재 시 후보 유지)
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
- [ ] 설치형(관리자): Program Files 설치 → `%LOCALAPPDATA%\NexaDir\data` 폴백 확인.
- [ ] 제거 → 데이터 보존·재설치 시 설정 복원.
- [ ] 태그 push → Release에 산출물 2종 첨부(다음 버전 태그에서 확인).
- [ ] `choco install nexa-dir` → Program Files 설치·시작 메뉴 등재 → `choco uninstall nexa-dir` → 데이터 보존.

## 7. Chocolatey — 3번째 채널 (packaging/chocolatey, 2026-07-19)

**패키지 2종** — 둘 다 기존 Release 자산의 래퍼일 뿐 별도 산출물을 만들지 않는다.
디렉터리는 패키지별로 나뉜다(`packaging/chocolatey/<id>/`).

| ID | 대상 자산 | 설치 방식 |
| --- | --- | --- |
| **`nexa-dir`** | `NexaDir-Setup-<버전>.exe` | Inno Setup 무인 설치(머신 전역) |
| **`nexa-dir.portable`** | `NexaDir-<버전>-win-x64.exe` | 패키지 `tools\`에 배치 + shim |

`.portable` 접미사는 Chocolatey 관례(`git.install`/`git.portable`). 엄밀히는 설치형도
`nexa-dir.install`이어야 하지만, `nexa-dir`이 이미 모더레이션 중이라 개명하지 않았다.

### 7-1. `nexa-dir` (설치형 래퍼)

- **바이너리 미동봉.** nupkg는 스크립트만 담고, 설치 시 **GitHub Release의
  `NexaDir-Setup-<버전>.exe`를 SHA-256 검증 후 내려받아** 무인 설치한다.
  근거: PolyForm NC = 비-FOSS → 커뮤니티 저장소는 공식 URL 다운로드가 정석이며,
  nupkg 용량도 스크립트 수 KB로 유지된다.
- **머신 전역 설치.** choco는 관리자로 돌지만 `nexa.iss`의 `PrivilegesRequired=lowest`
  기본값은 *사용자별* 설치로 판정되므로, 설치 인자에 **`/ALLUSERS`를 명시**한다
  (이를 위해 `PrivilegesRequiredOverridesAllowed`에 `commandline` 추가).
  결과적으로 데이터는 `%LOCALAPPDATA%\NexaDir\data` 폴백(§4)을 탄다.
- **제거**: `chocolateyuninstall.ps1`이 제거 레지스트리 키를 찾아 무인 실행 —
  §3과 동일하게 **사용자 데이터는 보존**.
### 7-2. `nexa-dir.portable` (포터블 래퍼)

- 포터블 단일 exe를 패키지 `tools\NexaDir.exe`로 내려받고(`Get-ChocolateyWebFile`),
  Chocolatey가 이를 **자동 shim 처리**해 PATH에 노출한다 → `NexaDir` 명령으로 실행.
- `tools\NexaDir.exe.gui`(빈 파일) = shimgen에 **GUI 앱임을 알리는 마커** —
  없으면 shim이 프로세스 종료를 기다린다.
- **주의 — 제거 시 데이터 소멸.** `data\`가 패키지 폴더 안에 생기므로
  `choco uninstall`이 폴더째 지운다. §3의 보존 규칙은 이 패키지에 적용되지 않으며,
  보존을 원하면 `nexa-dir`(설치형)을 쓰라고 설명에 명시했다. winget portable과 동일 한계(§8).

### 7-3. 게시 (CI 자동 / 수동)

- **CI**(release.yml, 평시 경로): 각 자산의 SHA-256을 계산해
  `chocolateyinstall.ps1`의 `{{VERSION}}`·`{{CHECKSUM64}}`를 치환 → **2패키지 pack** →
  **`CHOCO_API_KEY` 시크릿 + 저장소 변수 `CHOCO_PUSH=true`가 모두 있을 때만** 각각
  `choco push`(스위치 도입 07-21 — 모더레이션 대기 중 이중 큐 회피, GitHub Release만
  배포 가능). 조건 미충족 시 팩까지만 수행하고 nupkg를 워크플로 아티팩트로 남긴다.
  재개 = **Settings → Secrets and variables → Actions → Variables**에
  `CHOCO_PUSH=true` 등록(코드 무변).
- **수동**(`packaging/chocolatey/pack-and-push.ps1`, **Windows 전용**): 태그가 이미
  소진된 버전을 뒤늦게 올릴 때 쓴다. Release 자산을 내려받아 체크섬을 계산하는 동작이
  CI와 동일하고, 자리표시자 치환은 사본에만 적용 후 원본을 되돌린다.

  ```powershell
  # 팩만(확인용) — 둘 다
  pwsh packaging\chocolatey\pack-and-push.ps1 -Version 0.8.1
  # 포터블만 게시
  pwsh packaging\chocolatey\pack-and-push.ps1 -Version 0.8.1 -Id nexa-dir.portable -ApiKey <키>
  ```

  `choco`는 Windows 전용이라 맥에서는 pack/push를 실행할 수 없다(작성은 무관).
  **이미 게시된 ID+버전 조합은 재push 불가** — 새 ID의 첫 버전은 기존 릴리스를
  건드리지 않고 그대로 올릴 수 있다(0.8.1 포터블이 이 경우).

### 최초 등록 절차 (1회 — 사용자 수행)

1. <https://community.chocolatey.org> 계정 생성 → **Account → API Key** 복사.
2. 저장소 **Settings → Secrets and variables → Actions**에 `CHOCO_API_KEY` 등록.
3. 다음 버전 태그 push → 자동 pack·push. (2026-07-19 시크릿 등록 완료 → **`0.8.1`이
   최초 게시 버전**.)
4. **첫 패키지는 모더레이션 심사 대기**(수일~2주). 심사 통과 후에는 같은 ID의
   후속 버전이 자동 승인 경로를 탄다. 지적 사항은 패키지 페이지 코멘트로 온다.

> 심사 포인트: `tools/VERIFICATION.txt`(다운로드 검증 방법)·`requireLicenseAcceptance=true`
> (NC 라이선스)·vendor 본인이 메인테이너임을 명시 — 모두 반영 완료.

**게시 이력**: `nexa-dir` `0.8.1` 최초 push 성공(2026-07-19 — 모더레이션 큐 진입).
`nexa-dir.portable` `0.8.1`은 수동 게시 완료(pack-and-push.ps1). `0.9.0`은
**GitHub Release 전용**(2026-07-21 — `CHOCO_PUSH` 미설정으로 choco push 스킵,
승인 후 후속 버전부터 재개 예정).

**모더레이션 진행 상태 (2026-07-24 점검 — 패키지 페이지 실측)**

| 패키지 | 버전 | 자동 검증 | verification | 바이러스 스캔 | 현재 상태 |
| --- | --- | --- | --- | --- | --- |
| `nexa-dir` | 0.8.1 | 통과(07-19 14:07 — 최초 13:21 Requirements 실패분은 이메일 제거 재제출로 해소) | 통과(07-19 14:09) | **Flagged Note**(07-20 02:22 — VirusTotal 1~5 검출 = 승인 차단 아님) | **Ready = awaiting moderation**(다운로드 6) |
| `nexa-dir.portable` | 0.8.1 | 통과(07-19 14:07) | 통과(07-19 14:10) | **Flagged Note**(07-20 02:21 — 동일) | **Ready = awaiting moderation**(다운로드 5) |

- **자동 단계는 양쪽 모두 끝났고, 07-20 02:2x 이후 모더레이터 코멘트가 없다**
  (점검 시점까지 4일 무변동). 남은 것은 사람 검토 하나뿐 — **우리 측 조치 불요**.
- 미승인 버전은 공개 피드 검색에 노출되지 않는다(§7 서술) — OData
  `Packages()?$filter=Id eq '…'` 조회가 두 ID 모두 빈 결과인 것이 방증.
- **버전 격차**: 최신 릴리스는 `0.11.0`이지만 choco 양 패키지는 `0.8.1`에 머물러 있다
  (07-21 보류 방침 — `CHOCO_PUSH` 꺼짐). 첫 승인 후 후속 버전부터 재개하는 설계 그대로다.

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
- **제출 이력(설치형)**: [winget-pkgs#404528](https://github.com/microsoft/winget-pkgs/pull/404528)
  (0.8.1 최초 등록, 2026-07-19) — **2026-07-24 기준 여전히 OPEN**. 라벨 =
  `Azure-Pipeline-Passed`(검증 통과) + **`Policy-Test-1.2`**(미해제) + `Validation-Guide`
  + `New-Package`이며, **07-19 13:46 이후 라벨·코멘트 무변동**(코멘트는 봇 검증 링크와
  `@wingetbot run` 시도 → "Commenter does not have sufficient privileges" 3건이 전부).
  **포터블과의 결정적 차이 = 정책 플래그 해제 여부**: 포터블 PR은 07-20 22:57 모더레이터가
  **`Waived-Policy-Test-1.2`**를 부여하며 `Policy-Test-1.2`를 떼어냈고 다음 날 승인·병합됐다.
  설치형은 같은 플래그가 그대로 남아 있어 **모더레이터의 waiver가 병목**이며, 신규 기여자는
  파이프라인 재실행 권한이 없어 **우리 측에서 진행시킬 수단이 없다**(필요 시 PR 코멘트로
  waiver 상황을 문의하는 정도). 맥 환경이라
  `winget validate`/`winget install` 로컬 검증은 불가 — 스키마 수기 대조 + YAML 파싱까지만
  하고 **CI 검증에 의존**한다. (PR 체크리스트에도 그대로 명시했다. CLA 미서명이면 봇이 요청한다.)

### 포터블 변형 — `SosomLab.NexaDir.Portable`

포터블 단일 exe(기본 채널)도 winget에 **별도 패키지**로 등록한다. 설치형과 식별자를
나누는 이유 = winget은 한 패키지 안에서 사용자가 설치 방식을 고를 수단이 없기 때문.

- **명명 = 점 구분 변형 세그먼트.** Authoring.md("특정 경우 마침표 세그먼트 추가 가능")
  + 저장소 선례(`calibre.calibre.portable`·`Neovim.Neovim.Nightly`·
  `VSCodium.VSCodium.Insiders`). `NexaDirPortable`처럼 붙여 쓰면 검색 시 형제로
  묶이지 않아 채택하지 않았다.
- 경로 = `manifests/s/SosomLab/NexaDir/Portable/<버전>/` — 기존 `NexaDir/<버전>/`과
  한 폴더에 공존한다(VS Code가 같은 형태로 운영 중이라 구조상 문제 없음).
- `InstallerType: portable` + `PortableCommandAlias: nexadir`(참고 = `jqlang.jq`) →
  PATH에 등록되어 `nexadir` 명령으로 실행된다.
- **주의 — 설치형과 제거 동작이 다르다.** winget portable은 exe를
  `%LOCALAPPDATA%\Microsoft\WinGet\Packages\` 고정 경로에 두므로 `data\`도 거기 쌓이고,
  `winget uninstall`이 **폴더째 삭제 = 사용자 데이터도 함께 소멸**한다.
  §3의 "제거 시 데이터 보존" 규칙은 이 채널에 적용되지 않는다(locale 설명에 명시).
- **제출 이력(포터블)**: [winget-pkgs#404533](https://github.com/microsoft/winget-pkgs/pull/404533)
  (0.8.1 최초 등록) — **2026-07-21 22:14 MERGED = 배포 완료**(첫 패키지 매니저 등재 채널).
  `winget install SosomLab.NexaDir.Portable`로 설치 가능. 후속
  [winget-pkgs#405973](https://github.com/microsoft/winget-pkgs/pull/405973)(0.11.0 버전
  업데이트, 2026-07-22 제출) — **2026-07-22 18:44 UTC MERGED**(`Moderator-Approved` ·
  `Validation-Completed` · `Publish-Pipeline-Succeeded` — 07-22 기록의 "OPEN·검증 대기"는
  **정정**). winget-pkgs master에 `Portable/0.8.1`·`Portable/0.11.0` 매니페스트 상주 확인
  (raw 조회 200, 2026-07-24). **∴ winget Portable = 첫 승인 채널이자 유일하게 최신
  버전(`0.11.0`)까지 배포된 채널.**

### 채널 상태 요약 (2026-07-24 점검)

| 채널 | 패키지 | 배포된 버전 | 상태 | 우리 측 조치 |
| --- | --- | --- | --- | --- |
| winget | `SosomLab.NexaDir.Portable` | **0.8.1 → 0.11.0** | ✅ **배포 완료**(#404533·#405973 MERGED) | 없음 — 다음 릴리스 시 버전 업데이트 PR |
| winget | `SosomLab.NexaDir`(설치형) | — | ⏳ OPEN(#404528) — **`Policy-Test-1.2` waiver 대기** | 없음(권한 밖) |
| Chocolatey | `nexa-dir`(설치형) | — | ⏳ 0.8.1 awaiting moderation(자동 3단계 완료) | 없음 |
| Chocolatey | `nexa-dir.portable` | — | ⏳ 0.8.1 awaiting moderation(자동 3단계 완료) | 없음 |
| GitHub Release | 포터블 + 설치형 | **0.11.0** | ✅ 상시 | — |

**해석**: 4개 심사 항목 중 **포터블 winget만 통과**했고, 나머지 3건은 전부 사람 검토
단계에서 멈춰 있다(설치형 winget = 정책 waiver, choco 2종 = 모더레이션 큐).
공통점은 **자동 검증은 모두 통과**했다는 것 — 매니페스트·패키지 자체의 결함은 없다.

### 다음 버전 절차

매니페스트는 아직 **수동**이다(Chocolatey처럼 CI 자동화하지 않음 — 외부 저장소 PR이라
포크·토큰 권한이 따로 필요). 버전 승격 시 `packaging/winget/<새 버전>/`을 복사해
`PackageVersion`·`InstallerUrl`·`InstallerSha256`·`ReleaseDate`·`ReleaseNotesUrl`를
갱신하고 winget-pkgs에 PR한다. **0.11.0 실제 절차**(gh CLI, 맥):
① Release 자산 `shasum -a 256`로 SHA-256 확보 ② `gh repo sync <포크>/winget-pkgs`로
업스트림 동기화 ③ `gh api …/git/refs`로 브랜치 생성 후 `gh api …/contents/<경로>`(base64)로
3파일 커밋 ④ `gh pr create --repo microsoft/winget-pkgs`. 경로 =
`manifests/s/SosomLab/NexaDir/Portable/<버전>/`.
