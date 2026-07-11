# 06 · ADR-0001 — 기술 스택: 올 러스트 + Win32(windows-rs) + 커스텀 드로잉

- **상태**: **Accepted** (2026-07-11)
- **관계**: 원본 nexa-dir ADR-0001(Rust cdylib + WinUI 3/C#)을 **본 저장소에 한해 대체**. 원본 ADR-0004(코어 트리 모델)·ADR-0005(셸 컨텍스트 메뉴 방식)는 계승.

## 1. 배경

원본 스택은 기능 개발 속도(M1 후반 도달)는 입증했으나, 포터블 에디션의 3대 목표와 구조적으로 충돌한다.

1. **메모리**: 유휴 RSS 60MB+ — CLR·WinUI 컴포지션이 하한. Double Commander(<20MB) 체급 불가.
2. **단일 exe**: WinUI 3 XAML/PRI 게시 결함 실측 다수(원본 BUG-010·docs/12 §7) — 단일 파일화는 미검증 고위험.
3. **의존 최소화**: WinAppSDK 런타임 + .NET self-contained ≈64MB, dll 수십 개.

## 2. 후보 비교

| 후보 | 단일 exe | 유휴 RSS | 코어 재사용 | 라이선스(C2) | 판정 |
| --- | --- | --- | --- | --- | --- |
| **A. 올 러스트 + Win32(windows-rs) + 커스텀 드로잉** | ◎ 수 MB·DLL 0 | ◎ 15~30MB | ◎ rlib 직접 | ◎ MIT/Apache | **채택** |
| B. Rust + Slint | ○ 정적 링크 | ○ 30~50MB(SW 렌더) | ◎ | ✗ GPL/Royalty-free/상용 3중 — 퍼미시브 온리 위반 | 기각 |
| C. Rust + egui | ○ | △ 50MB+(GPU 상시 리페인트) | ◎ | ◎ | 기각 — B1 위반·IME/접근성 취약 |
| D. C++ + Win32 + Rust staticlib | ◎ | ◎ | ○ C ABI 유지 필요 | ◎ | 기각 — 언어 2개·FFI 잔존, A 대비 이점 없음 |
| E. Delphi/Free Pascal | ◎ | ◎ | ✗ 코어 폐기 | — | 기각 — 기존 자산 전량 상실 |
| (참고) 원본 유지 + 단일 파일 게시 | △ | ✗ 60MB+ | ◎ | ◎ | 기각 — B1~B3 전부 미달 |

## 3. 결정

**안 A.** 구성 요소:

| 레이어 | 선택 | 비고 |
| --- | --- | --- |
| 언어 | **Rust 단일**, edition 2021, stable | 관리 런타임 0 |
| 코어 | 원본 `nexa-core`/`nexa-vfs`/`nexa-tree` **rlib 이식** | cdylib/C ABI/csbindgen 폐지 |
| 창/입력 | **Win32 직접**(`windows` crate) — 창 1개, PerMonitorV2 | 자식 HWND 최소화 |
| 렌더링 | 커스텀 드로잉: GDI(스파이크) → **DirectWrite GDI interop**(M1, ADR-0002) | GPU 스왑체인 비보유 |
| 셸 통합 | windows-rs COM — IContextMenu·OLE DnD·IFileOperation·휴지통 | 원본 ADR-0005 방식 계승 |
| 터미널 | ConPTY + 자체 VT(원본 VtScreen 이식) | Win10 1809+ |
| 링크 | `+crt-static`·`lto`·`panic="abort"`·strip | 예산 B2 |
| 배포 | 포터블 단일 exe 단독 | DR-3 |

## 4. 결과 (Consequences)

- (+) 예산 B1~B3 달성이 구조적으로 가능해짐. 인터롭 계층·ABI 버전 관리 소멸. 언어 1개.
- (−) **UI 전면 자체 구현**(탭·스크롤바·인라인 편집기·메뉴…) — 최대 공수. 완화: 원본의 검증된 UX 스펙·코어 로직 재사용, 수직 슬라이스.
- (−) IME·UIA 접근성·DPI를 직접 부담 — TODO ACC 트랙으로 명시 관리.
- (−) WinUI가 제공하던 Fluent 비주얼 상실 — 커스텀 테마 토큰(원본 docs/39 계승)으로 재현.
- 검증 게이트: **M0 종료 시 빈 창 실측**(RSS·exe 크기·임포트 테이블). 미달 시 본 ADR 재검토.
