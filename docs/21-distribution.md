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
