# 01 · 아키텍처 — 올 러스트 단일 바이너리

> 결정 근거: [ADR-0001](06-adr-0001-stack.md)(Accepted) · 예산 [05 §2](05-requirements.md).
> 원본(Rust cdylib + WinUI 3/C#) 대비 핵심 변화 = **언어 경계·FFI·관리 런타임 제거**.

## 1. 층 구조

```
┌─────────────────────────────────────────────────┐
│ nexa-app (bin)  창·메시지 루프·레이아웃·명령 배선     │
├─────────────────────────────────────────────────┤
│ nexa-gui        위젯(리스트·탭·경로바·메뉴)·렌더러     │  ← M1에서 분리
│                 (Win32 창 1개 + 전면 커스텀 드로잉)   │
├──────────────┬──────────────┬───────────────────┤
│ nexa-tree ★  │ nexa-ops     │ nexa-shell        │
│ 트리·선택·정렬 │ 전송·Undo    │ COM(셸메뉴·DnD·    │
│ (원본 이식)   │ (M3 신규)    │ 휴지통·아이콘, M3)  │
├──────────────┴──────┬───────┴───────────────────┤
│ nexa-vfs (원본 이식) │ nexa-term (ConPTY·VT, M4) │
├─────────────────────┴───────────────────────────┤
│ nexa-core (공용 타입, 원본 이식)                    │
└─────────────────────────────────────────────────┘
        전부 rlib 정적 링크 → 단일 exe (FFI/ABI 없음)
```

- **핫패스 전부 러스트 단일 프로세스** — 원본 ABI v7·P/Invoke 마샬링·행당 3회 왕복(원본 E-13) 같은 경계 비용이 소멸.
- 원본에서 C#에 있던 무게중심(FileOps·셸 COM·터미널 VT·설정/i18n)을 러스트 크레이트로 이식(원본 로드맵 B-1 "nexa-ops 이관"의 실현).

## 2. 크레이트 계획

| 크레이트 | 출처 | 내용 | 도입 |
| --- | --- | --- | --- |
| `nexa-core` | **원본 이식** | 공용 타입(FileKind 등), 의존성 0 | M0 |
| `nexa-vfs` | **원본 이식** | 로컬 스트리밍 열거·Provider trait | M0 |
| `nexa-tree` ★ | **원본 이식** | 가시 노드 평면 스트림·교차 선택·정렬·타입어헤드 (원본 ADR-0004) | M0 |
| `nexa-app` | 신규 | bin. Win32 창·메시지 루프·조립. `#[cfg(windows)]` 격리, 비-Windows는 스텁 | M0 |
| `nexa-gui` | 신규 | 위젯 트리·커스텀 드로잉 렌더러·입력 라우팅·테마 토큰 (nexa-app에서 분리) | M1 |
| `nexa-ops` | 신규 | 전송 엔진(진행률·충돌·취소)·Undo/Redo·휴지통 — 원본 `FileOps`(C#) 이식 | M3 |
| `nexa-shell` | 신규 | `IContextMenu` 호스팅·OLE DnD·셸 아이콘·클립보드 — 원본 C# P/Invoke 코드의 windows-rs 번역 | M3 |
| `nexa-term` | 신규 | ConPTY 세션·VT 파서/스크린 버퍼 — 원본 `ConPtySession`/`VtScreen`(C#) 이식 | M4 |

## 3. 렌더링 전략 (창 1개 + 전면 커스텀 드로잉)

Double Commander/Total Commander/Everything 체급의 메모리는 **GPU 컴포지팅 프레임워크 없이** Win32 창에 직접 그릴 때 달성된다.

- **창/입력**: `RegisterClassW`+`CreateWindowExW` 창 1개(자식 HWND 최소화 — 위젯은 논리 객체). PerMonitorV2 DPI.
- **페인트**: `WM_PAINT` 더블 버퍼(메모리 DC). **가시 영역만** 그린다 — `nexa-tree`의 가시 행 평면 스트림이 이 모델에 정확히 부합(행 = `rect × draw(dc)`).
- **텍스트**: **DirectWrite GDI interop**(`IDWriteBitmapRenderTarget` + `IDWriteTextLayout`) — [ADR-0002](07-adr-0002-rendering.md) **Accepted**(실측: GDI 대비 −28%·RSS +4.1MB 예산 내). ClearType + 시스템 폰트 폴백 + GPU 스왑체인 없음.
- **금지**: D3D 스왑체인 상시 보유·상시 리페인트 루프(즉시모드) — B1 예산 위반 경로.

## 4. 이벤트·스레딩 모델

- **UI 스레드 1개**(메시지 루프) + **워커**(디렉터리 열거·전송·watcher). 워커→UI 통지는 `PostMessage`(커스텀 메시지)로 단일화 — 원본 A-1(백그라운드 열거+세대 가드) 교훈 계승.
- 취소는 세대 카운터/`AtomicBool`. 패널·탭 상태의 진실원천은 코어(`nexa-tree` 핸들) — 원본 ADR-0004 유지.

## 5. 영속·리소스 (포터블 규율)

- 영속물(설정·세션·언어팩·로그) = **exe 옆 `data\`** 단일 원천(원본 `AppPaths`/docs/43 차용, 레지스트리·%APPDATA% 비의존).
- 아이콘·기본 언어팩(en/ko)은 exe **리소스 섹션/`include_bytes!` 임베드** — 외부 파일 0 충족. 사용자 언어팩은 `data\lang\` 오버라이드.

## 6. 원본과의 대응 관계

| 원본 | 본 저장소 |
| --- | --- |
| `core/`(Rust cdylib, ABI v7) + `NativeInterop.cs`(P/Invoke) | rlib 직접 링크 — 인터롭 계층 삭제 |
| WinUI 3 XAML·ItemsRepeater 가상화 | `nexa-gui` 커스텀 드로잉 가상화 |
| `Nexa.ViewModels`(순수 C# 로직: 경로·정렬·타입어헤드·i18n) | 각 러스트 크레이트로 흡수(맥 테스트 가능 성질 유지) |
| `Nexa.Plugins`(.NET 미리보기 SDK) | 비이관 — 내장 미리보기(DR-7) |
| MSIX·setup.exe·포터블 zip | **단일 exe 단일 채널**(DR-3) |
