# 12 · 패키징 — 포터블 단일 exe (DR-3)

> 원본 docs/12(포터블 zip·setup.exe·MSIX)와 달리 본 저장소는 **단일 exe 단독 채널**. 원본의 실측 교훈(BUG-010 등)이 이 결정의 배경([00 §2](00-vision.md)).

## 1. 산출물 정의

- `NexaDir-<ver>-win-x64.exe` **1개 파일** — 그 자체로 실행 가능, 설치·압축해제·재배포 런타임 불요.
- 임베드 리소스: 앱 아이콘(.ico), 기본 언어팩(en/ko), 기본 테마 토큰 — `include_bytes!`/PE 리소스 섹션.
- 임포트 = OS 인박스 DLL만(B3). 검증: `dumpbin /imports`.

## 2. 영속 규율 (원본 docs/43 차용)

| 항목 | 위치 |
| --- | --- |
| 설정 `settings.json` · 세션 `session.json` · 로그 | `<exe 폴더>\data\` |
| 사용자 언어팩(내장 오버라이드) | `<exe 폴더>\data\lang\*.lang` |
| 레지스트리·%APPDATA% | **사용 안 함**(셸 연동 등록 없음 — 앱 내부 IContextMenu 호출만) |

- `data\`는 첫 쓰기 시 생성. 읽기 전용 매체(CD/보호 USB)면 메모리 상주 폴백 + 상태바 경고.
- 원본과 달리 `portable.ini` 마커 불요 — **항상 포터블 모드**가 기본이자 유일.

## 3. 빌드 파이프라인

1. `cargo build --release -p nexa-app` (프로파일·정적 CRT → [18 §3](18-build-and-test.md))
2. CI 예산 검사: exe ≤10MB · 임포트 화이트리스트 · (후속) 스모크 실행.
3. 릴리스 태그 시 exe를 GitHub Release에 자동 첨부(원본 package job 방식 차용).

## 4. 서명 (후속)

원본 PKG-4 조사 결론 공유 — Azure Artifact Signing은 한국 개인 불가, OV 클라우드 서명 또는 Store 위임이 현실 경로.
서명 전까지 SmartScreen 경고는 수용(README에 안내). 결정은 원본과 함께 진행.

### 4-1. 재조사 (2026-07-31 — 사용자 다운로드 차단 보고 계기)

Microsoft 공식 문서 [code-signing-options](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options)(2026-04)·
[smartscreen-reputation](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation)(2026-05) 기준.
**위 §4의 전제 2건이 무효**가 되었으므로 아래를 우선한다.

| 경로 | 비용 | SmartScreen | 한국 가용 | 비고 |
| --- | --- | --- | --- | --- |
| **Microsoft Store (MSIX)** | **무료**(등록비 2026 폐지) | ✅ **경고 없음** | ✅ | MS가 심사 후 **재서명**. 인증서 불요. MSI/EXE 제출은 재서명 없음 — **MSIX여야 함** |
| Azure Artifact Signing(구 Trusted Signing) | ~$9.99/월 | ⚠️ 평판 축적 | ❌ | 조직=미국·캐나다·EU·영국 / 개인=미국·캐나다 **지역 제한** |
| SignPath Foundation(무료 OSS) | 무료 | ⚠️ 평판 축적 | — | **결격** — "OSI-approved license without commercial dual-licensing" 요구. PolyForm NC(DR-6)+상업 라이선스([13](13-licensing.md)) 이중 위배. 인증서 명의도 SignPath Foundation |
| OV 인증서 | 연 $150~300 + HSM | ⚠️ 평판 축적 | ✅ | 한국에서 **유일한 유료 선택지** |
| EV 인증서 | 연 $400+ | ⚠️ OV와 동일 | ✅ | **2024년 즉시 통과 폐지** — 프리미엄 지불 근거 소멸 |

**핵심 정정 3건**

1. **EV = 즉시 통과가 아니다.** 2024년 폐지되어 OV와 동일한 평판 축적 대상.
2. **Azure Artifact Signing은 한국에서 쓸 수 없다**(법인·개인 모두 대상 지역 밖).
3. **서명해도 초기 경고는 뜬다.** 서명이 주는 것은 *버전 간 평판 승계*(무서명은
   버전마다 해시 평판 0에서 재시작)이지 경고 면제가 아니다. 경고를 실제로 없애는
   경로는 **Microsoft Store(MSIX)뿐**.

**현 방침**: DR-3 무서명 유지 + [21 §5-1](21-distribution.md) **zip 자산으로 다운로드
단계만 완화**. Microsoft Store(MSIX) 채널은 [00-vision](00-vision.md)의 배제 전제
(비용·서명 필요)가 무너졌으므로 재검토 대상 — 착수 시 MSIX 컨테이너에서
셸 통합·터미널 통합·WASM 플러그인 로딩이 동작하는지 실측이 선행되어야 하고,
`broadFileSystemAccess` 제한 기능 심사가 관문이다. DR-3/00-vision 개정 ADR 사안.

### 4-2. Defender ML 오탐 — 설치형만 격리 (2026-08-24 실측)

**증상**: 사용자 PC에서 `NexaDir-Setup-0.18.0.exe`가 다운로드 직후
`Trojan:Win32/Wacatac.C!ml`로 격리. **0.18.0의 신규 문제가 아니었다** — 격리 목록에
`NexaDir-Setup-0.17.0.exe`가 같은 날 오전 `Wacatac.B!ml`로 이미 들어가 있었다.

**실측으로 좁힌 범위**(전부 같은 PC·같은 시각대):

| 검사 대상 | 결과 |
| --- | --- |
| 포터블 exe · `markdown.wasm` · `archive.wasm` | 탐지 없음 |
| 0.16.0·0.17.0·0.18.0 **설치형** 온디맨드 스캔(임시 폴더) | 셋 다 탐지 없음 |
| 브라우저로 내려받은 설치형(`webfile:` 컨텍스트) | **격리** |
| 릴리스 자산 SHA-256 대조 | 일치(변조 없음) |

∴ **파일 내용이 아니라 "무서명 + 새 해시 + 프리밸런스 0"인 다운로드 시점의 클라우드
ML 판정**이 원인이고, Inno Setup 설치형이 임시 폴더에 PE를 풀어 실행하는 구조라
드로퍼 패턴과 형태가 겹쳐 **포터블이 아니라 설치형만** 걸린다(`!ml` = ML 휴리스틱).

**조치**

1. **VERSIONINFO 보강**(즉시 — `installer/nexa.iss`): Inno는 지정하지 않으면 설치형 exe의
   **파일 버전을 비워 둔다**(실측: 포터블 `0.18.0.0` / 설치형 공백). *무서명 + 버전 정보
   없음*은 ML 가중 요소라, `VersionInfoVersion`·`ProductName`·`Company`·`Copyright`·
   `OriginalFileName`을 채웠다. 다음 릴리스부터 적용.
2. **오탐 신고**(발행자 — 재발 시 상시 절차): [WDSI 파일 제출](https://www.microsoft.com/en-us/wdsi/filesubmission)
   에 *Software developer* 자격으로 해시·URL·빌드 근거 제출 → 클라우드 판정이 정정되면
   **모든 사용자에게 반영**된다. 제출 문안 템플릿 = [packaging/av-false-positive.md](../packaging/av-false-positive.md).
3. **사용자 안내**: 즉시 해법은 **포터블 채널**(탐지 없음) — [21 §5-1](21-distribution.md)의
   "안내는 포터블 우선" 규약이 이 사례로 재확인됐다. 이미 격리됐다면
   `MpCmdRun.exe -Restore -FilePath <경로>`(관리자) 또는 보호 기록에서 **장치에서 허용**.
4. **근본 해결은 여전히 §4-1** — 서명(평판 승계) 또는 MS Store(MSIX 재서명).

## 5. arm64

`aarch64-pc-windows-msvc` 타깃 추가로 대응 가능(코드 변경 불요 전망). 수요 확인 후 CI 매트릭스에 추가.
