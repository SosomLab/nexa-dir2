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

## 5. arm64

`aarch64-pc-windows-msvc` 타깃 추가로 대응 가능(코드 변경 불요 전망). 수요 확인 후 CI 매트릭스에 추가.
