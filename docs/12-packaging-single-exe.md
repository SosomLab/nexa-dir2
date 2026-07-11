# 12 · 패키징 — 포터블 단일 exe (DR-3)

> 원본 docs/12(포터블 zip·setup.exe·MSIX)와 달리 본 저장소는 **단일 exe 단독 채널**. 원본의 실측 교훈(BUG-010 등)이 이 결정의 배경([00 §2](00-vision.md)).

## 1. 산출물 정의

- `nexa-dir2-<ver>-win-x64.exe` **1개 파일** — 그 자체로 실행 가능, 설치·압축해제·재배포 런타임 불요.
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

## 5. arm64

`aarch64-pc-windows-msvc` 타깃 추가로 대응 가능(코드 변경 불요 전망). 수요 확인 후 CI 매트릭스에 추가.
