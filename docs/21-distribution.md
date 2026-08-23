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
자산은 exe 2종 + **zip 2종 + `SHA256SUMS.txt`**(§5-1) = 5개.

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

## 5-1. ZIP 자산 + 체크섬 (2026-07-31 — 사용자 다운로드 차단 보고 대응)

**문제**: 무서명(DR-3) exe는 브라우저의 **SmartScreen 다운로드 평판 필터**에 차단되는
사례가 있다. 서명이 없으면 게시자 평판이 축적되지 않고 파일 해시 평판은 버전마다
0에서 다시 시작하므로, 릴리스를 거듭해도 개선되지 않는다.

**조치**: exe를 zip으로 감싼 자산을 **추가**한다(기존 exe 자산은 유지).

| 단계 | zip 효과 |
| --- | --- |
| 브라우저 다운로드 차단 | ✅ 해소 — exe 확장자 평판 필터를 타지 않음 |
| 실행 시 SmartScreen 경고 | ❌ 남음 — Explorer·WinRAR·7-Zip 22.00+ 는 압축 해제 시 MOTW 전파 |
| Defender 검사 · Smart App Control | ❌ 무관 |

- **기존 exe 자산은 절대 교체하지 않는다** — winget(§8)·choco(§7) 매니페스트가
  URL+SHA256으로 직접 참조하므로 교체 시 3채널이 동시에 깨진다.
- 자산 5종: `NexaDir-<버전>-win-x64.exe` · `NexaDir-Setup-<버전>.exe` ·
  `NexaDir-<버전>-win-x64.zip` · `NexaDir-Setup-<버전>.zip` · `SHA256SUMS.txt`.
- zip에는 exe + **`README.txt`**(무서명 경고 예고·[추가 정보]>[실행] 안내·
  `Get-FileHash` 대조법 · 한/영 · UTF-8 BOM) 동봉.
- **안내는 포터블 zip 우선.** 설치형 zip은 압축을 풀어도 UAC "알 수 없는 게시자"
  프롬프트가 추가되고, 설치 프로그램 자체가 Defender 휴리스틱에 더 잘 걸린다.
- 구현 주의: `run: |` 안에서 PowerShell here-string(`@" ... "@`)은 종료자가 컬럼 0이어야
  해 **YAML 블록 스칼라를 깨뜨린다**. 문자열 배열 + `Set-Content`로 작성할 것.

**근본 해결이 아님.** 서명 경로 현황은 [12 §4](12-packaging-single-exe.md) 참조 —
EV의 SmartScreen 즉시 통과는 2024년 폐지, Azure Artifact Signing은 한국 이용 불가,
SignPath Foundation은 라이선스 결격. 경고를 완전히 없애는 유일한 무료 경로는
**Microsoft Store(MSIX 재서명)** 이며 별건 검토 대상이다.

## 6. 검증 체크리스트 (실기 QA)

- [ ] 포터블: 임의 폴더에서 실행 → `data\` 생성·설정 영속.
- [ ] 설치형(사용자별): 설치 → 시작 메뉴 실행 → exe 옆 `data\` 사용.
- [ ] 설치형(관리자): Program Files 설치 → `%LOCALAPPDATA%\NexaDir\data` 폴백 확인.
- [ ] 제거 → 데이터 보존·재설치 시 설정 복원.
- [ ] 태그 push → Release에 산출물 2종 첨부(다음 버전 태그에서 확인).
- [ ] 태그 push → Release 자산 **5종**(exe 2 + zip 2 + SHA256SUMS.txt) 첨부(§5-1).
- [ ] 브라우저에서 **zip 다운로드가 차단되지 않는지** 확인(원 증상 재현 대조).
- [ ] zip 해제 → `README.txt` 한글 정상 표시 → exe 실행(SmartScreen 경고는 예상 동작).
- [ ] `Get-FileHash`가 `SHA256SUMS.txt` 값과 일치.
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
  **07-31 상태 변화 — "조치 불요" 전제 깨짐**: 모더레이터(stephengillie, 자동화 봇 build
  1799) 코멘트로 **`Needs-Author-Feedback` 부착** — "`DisplayVersion`이 `PackageVersion`과
  동일값이므로 **제거하라**"(installer.yaml `AppsAndFeaturesEntries.DisplayVersion: 0.8.1`,
  로컬 사본 `packaging/winget/0.8.1/` 30행 동일). winget-pkgs는 Needs-Author-Feedback
  무응답 PR을 일정 기간 후 자동 클로즈하므로 **포크 브랜치에 수정 커밋 push가 필요**하다.
  **2026-08-06 00:46 UTC MERGED — 설치형 등록 완료**(07-19 제출 → 18일). 최종 라벨 =
  `Moderator-Approved`·`Validation-Completed`·`Publish-Pipeline-Succeeded`이며,
  포터블과 달리 **`Policy-Test-1.2`는 waiver 라벨 없이 그냥 제거**된 채 승인됐다
  (`Validation-Guide`도 동반 제거). `winget install SosomLab.NexaDir`로 설치 가능
  — 다만 등록 버전은 **0.8.1**이라 격차가 컸다(08-11 오전 점검).
  **08-11 저녁 — 첫 버전 업데이트 제출**: [#415215](https://github.com/microsoft/winget-pkgs/pull/415215)
  (`0.16.0`). 07-31 피드백대로 `AppsAndFeaturesEntries.DisplayVersion`은 **계속 뺀 채로**
  두었고, locale 설명이 `0.8.1` 시점(**"Dual-language UI"**·클라우드 미언급)에 멈춰 있어
  현행 기능(클라우드·3개 언어)으로 함께 갱신했다.
  → **2026-08-14 MERGED**(08-23 확인 — `Waived-Policy-Test-1.2` 경유: 버전 업데이트도
  최초 등록과 같은 waiver 경로를 탄다). 카탈로그 실측(08-23) = 0.8.1/**0.16.0**.

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
  후속 [#408280](https://github.com/microsoft/winget-pkgs/pull/408280)(0.12.0) MERGED ·
  [#410978](https://github.com/microsoft/winget-pkgs/pull/410978)(0.13.0, 08-01 제출) —
  **2026-08-06 00:16 UTC MERGED**(`Waived-Policy-Test-1.2` 부여 후 승인 = 0.11/0.12와
  같은 패턴, 이번엔 waiver까지 5일 걸렸다). 카탈로그 실측(08-11) = `winget show --versions`
  **0.8.1 / 0.11.0 / 0.12.0 / 0.13.0**.
  **08-11 저녁 — `0.16.0` 제출**([#415214](https://github.com/microsoft/winget-pkgs/pull/415214)).
  `0.14.0`·`0.15.0`은 **건너뛰었다**: 0.14.0은 0.13.0 PR이 OPEN이라 보류했고(08-02),
  0.15.0은 발행 직후 바 크기 재조정이 확정돼 **같은 패키지에 PR이 겹치는 것을 피하려고**
  0.16.0으로 한 번만 올렸다. winget은 중간 버전을 건너뛰어도 무방하다(사용자는 최신만 받는다).
  → **제출 당일(08-11 13:10 UTC) MERGED**(08-23 확인 — waiver 없이 `Moderator-Approved`,
  역대 최단). 카탈로그 실측(08-23) = 0.8.1/0.11.0/0.12.0/0.13.0/**0.16.0**.

### 채널 상태 요약 (2026-08-23 재점검 — 원천 실측: PR 라벨 JSON·`winget show --versions`·choco 패키지 페이지)

| 채널 | 패키지 | 카탈로그 버전 | 상태 | 우리 측 조치 |
| --- | --- | --- | --- | --- |
| winget | `SosomLab.NexaDir.Portable` | **0.16.0**(0.8.1/0.11.0/0.12.0/0.13.0/0.16.0 상주) | ✅ [#415214](https://github.com/microsoft/winget-pkgs/pull/415214) **MERGED 08-11**(제출 당일 — `Moderator-Approved`) | 없음 — **다음 릴리스부터 제출 규칙 적용**(대기 없음 = 제출) |
| winget | `SosomLab.NexaDir`(설치형) | **0.16.0**(0.8.1/0.16.0) | ✅ [#415215](https://github.com/microsoft/winget-pkgs/pull/415215) **MERGED 08-14**(`Policy-Test-1.2` waiver 경유 — 0.13.0 포터블과 같은 패턴) | 동일 |
| Chocolatey | `nexa-dir`(설치형) | — | ⏳ 0.8.1 **모더레이션 미승인 유지**(07-20 02:22 스캔 Flagged 이후 무이벤트 — 08-23 페이지 실측 무변동. 자동 3단계는 완료, 사람 검토만 미착수) | **push 제외 유지**(제출 규칙 — 대기 중) / 오탐 소명 코멘트는 선택 |
| Chocolatey | `nexa-dir.portable` | — | ⏳ 동일(07-20 02:21) | 동일 |
| GitHub Release | 포터블 + 설치형 | **0.16.0** (08-11 — X-40 자동 갱신·X-41 바 크기. 직전 `0.15.0`도 같은 날) | ✅ 상시(자산 5종 규약) | — |

> **08-23 재점검**: winget 0.16.0 PR 2건이 **둘 다 병합·카탈로그 라이브 확인**
> (`winget show --versions` 실측). 포터블은 제출 당일(08-11) 병합 — 역대 최단.
> 설치형은 08-14 `Waived-Policy-Test-1.2`로 병합(첫 버전 업데이트도 waiver 경로를
> 탄다는 확인 — 최초 등록 때와 동일 패턴). 이로써 **winget 두 채널이 처음으로 최신
> 릴리스와 완전 동기**됐고, 심사 대기는 choco 2건(0.8.1)만 남았다. 다음 릴리스는
> [채널 제출 규칙](#릴리스-시-채널-제출-규칙-2026-08-23--사용자-지시상시-규칙)대로
> winget 제출·choco 제외.

> **08-11 경과**: 오전 점검에서 winget 2건 병합을 확인해 **"심사 대기 4건 → choco 2건"** 으로
> 줄었고, 저녁에 `0.15.0`·`0.16.0`을 연이어 배포하며 **winget 0.16.0 PR 2건을 새로 제출**했다
> (0.15.0은 제출하지 않았다 — 같은 패키지에 PR이 겹치는 08-02 상황을 피하려고 **0.16.0으로
> 한 번만** 올렸다).
> **choco는 사용자 지시로 계속 제외** — 0.8.1이 모더레이션에 잠긴 채라 **이중 큐를 만들지
> 않는다**. `0.15.0`·`0.16.0` 모두 **팩 success · push 미실행**을 워크플로 로그로 확인했다
> (`CHOCO_PUSH` 미등록 — `gh variable list` exit 0·빈 목록).
> **체크섬 절차**: 매니페스트 값은 릴리스가 발행한 `SHA256SUMS.txt`에서 가져오고, **자산을
> 새로 내려받아 `Get-FileHash`로 재대조**한 뒤 제출한다(08-11 확립). 제출은 winget-pkgs
> 클론이 무거워 **포크 브랜치 + Contents API로 파일만 올리는 방식**을 쓴다.

**해석(08-02)**: 릴리스 0.13.0을 **Chocolatey 제외**로 배포했다(사용자 지시 — 워크플로
`choco push` 스텝이 `skipped`로 확인됨. `CHOCO_PUSH` 미설정 게이트가 의도대로 동작).
winget Portable은 0.13.0 PR 제출로 **다시 릴리스와 동기화 궤도**에 올랐다.

**choco 정체의 성격이 바뀐 기록**: 종전에는 "모더레이션 큐 대기"로 적었으나, 심사 로그를
읽어 보니 **07-20 자동 바이러스 스캔에서 플래그**된 뒤 멈춰 있다(스캔 결과 2/3 플래그).
무서명 Rust 단일 exe의 전형적 오탐이며([12 §4-1](12-packaging-single-exe.md)), 자동으로
풀리지 않고 **모더레이터가 결과를 보고 면제 처리**해야 넘어간다. 큐가 밀린 것과 다르다.

### 릴리스 시 채널 제출 규칙 (2026-08-23 — 사용자 지시·상시 규칙)

릴리스(태그·GitHub Release)마다 winget·Chocolatey의 **배포 요청 상태를 원천 실측**하고,
채널별로 다음을 적용한다. 08-02("0.13.0 PR OPEN이라 0.14.0 보류")·08-11("0.15.0을
건너뛰고 0.16.0으로 한 번만")에서 건별로 내린 판단을 상시 규칙으로 승격한 것.

| 채널 상태 | 조치 |
| --- | --- |
| **대기 중인 버전 없음**(심사·모더레이션에 걸린 제출 없음) | 릴리스 후 **그 버전을 그 채널에 제출** |
| **대기 중인 버전 있음**(PR OPEN / 모더레이션 미승인) | **그 채널은 이번 제출에서 제외**(같은 패키지 중복 큐·PR 겹침 방지) — 해소 후 **그 시점 최신 버전만** 제출(중간 버전 생략 무방 — 사용자는 최신만 받는다) |

- **판정 원천(채널별)** — winget: 우리가 제출한 PR이 OPEN인가(`gh pr view <번호>` 또는
  `gh search prs --repo microsoft/winget-pkgs --author <제출 계정>` — 패키지 2종 각각) ·
  Chocolatey: **모더레이션 미승인 버전이 있는가**(패키지 페이지 실측 — OData는 미승인분
  미반환. 현재 0.8.1 두 패키지가 07-20 스캔 플래그로 잠김 = **계속 제외**. 기존
  `CHOCO_PUSH` 미등록 게이트·사용자 지시와 합치 — 승인 시 `CHOCO_PUSH=true` 등록으로 재개).
- 점검·제출/제외 결과는 릴리스 기록(journal·§8 채널 표)에 남긴다.

### 다음 버전 절차

매니페스트는 아직 **수동**이다(Chocolatey처럼 CI 자동화하지 않음 — 외부 저장소 PR이라
포크·토큰 권한이 따로 필요). 버전 승격 시 `packaging/winget/<새 버전>/`을 복사해
`PackageVersion`·`InstallerUrl`·`InstallerSha256`·`ReleaseDate`·`ReleaseNotesUrl`를
갱신하고 winget-pkgs에 PR한다. **0.11.0 실제 절차**(gh CLI, 맥):
① Release 자산 `shasum -a 256`로 SHA-256 확보 ② `gh repo sync <포크>/winget-pkgs`로
업스트림 동기화 ③ `gh api …/git/refs`로 브랜치 생성 후 `gh api …/contents/<경로>`(base64)로
3파일 커밋 ④ `gh pr create --repo microsoft/winget-pkgs`. 경로 =
`manifests/s/SosomLab/NexaDir/Portable/<버전>/`.
