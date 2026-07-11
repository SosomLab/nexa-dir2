# STATUS — Nexa Dir 2 진행 현황

> **갱신: 2026-07-11 (KST)** — **M0 완료(`0.1.0`: B1 13.22MB·B2 0.20MB·B3 인박스만) → M1 착수.**
> `feat/m1-gui`: **M1-1 `nexa-gui` 분리 완료** — 플랫폼 중립 위젯 trait·무효화(rect 병합)·입력 이벤트·
> 테마 토큰(원본 docs/39 차용, 다크 기본)·`VirtualRows`(스파이크 로직 이식). 테스트 43 green·실기 확인.
> 다음: **M1-2 ADR-0002** — GDI vs DirectWrite GDI interop 스파이크 비교(품질·RSS·속도) 확정.

## 1. 확정된 결정 ([10](10-decision-record.md))

| # | 영역 | 결정 |
| --- | --- | --- |
| DR-1 | 스택 | **올 러스트 단일 바이너리** — Win32(windows-rs)+커스텀 드로잉 · ADR-0001 Accepted |
| DR-2 | 예산 | 유휴 RSS ≤30MB · exe ≤10MB · 임포트=OS 인박스만 — **병합 게이트** |
| DR-3 | 배포 | 포터블 **단일 exe 단독** 채널(`data\` 영속) |
| DR-4 | 코어 | 원본 nexa-core/vfs/tree **rlib 이식**(FFI 폐지) |
| DR-5 | UX | 원본 M1 기능 패리티·디자인 규약 계승 |
| DR-6 | 라이선스 | PolyForm NC + 의존성 퍼미시브 온리 |
| DR-7 | 플러그인 | .NET SDK 비이관 — 내장 미리보기 대체 |
| DR-8 | 외부 crate | 기본 0 지향, 건별 원장 기록(`windows` 승인) |

## 2. 예산 실측 현황 (DR-2)

| 항목 | 예산 | 최신 실측 | 시점 |
| --- | --- | --- | --- |
| B1 유휴 RSS | ≤30MB | **13.22MB** (100k 행 창·유휴 95s·3회 중앙값, Private 1.70MB) | 07-11 실기 |
| B2 exe 크기 | ≤10MB | **0.20MB** (214,528B) | 07-11 실기 |
| B3 임포트 DLL | OS 인박스만 | **통과** — user32·kernel32·gdi32·ntdll·oleaut32·api-ms-win-core-synch (CI 화이트리스트 게이트화) | 07-11 실기 |

## 3. 마일스톤 (상세 [MILESTONES](MILESTONES.md))

- **M0** 기반·게이트 ✅ (`0.1.0`) — 설계 문서·스캐폴딩·코어 3크레이트 이식·Win32 창·렌더 스파이크·CI·게이트 실측.
- **M1** 뷰어(★플래그십) 🚧 — M1-1 `nexa-gui` 분리 ✅ · M1-2 ADR-0002 ☐ 다음.
- M2 셸 골격 · M3 조작 · M4 패널 · M5 마감 — ☐.

## 4. 개발 모델 ([11](11-dev-environment.md))

- 맥 = 일상 개발(코어 test + **windows 타깃 cargo check로 UI 코드까지 타입 검증**) · Windows PC/CI = 실행·QA·예산 실측.

## 5. 다음 단계

1. ~~M0~~ ✅ (07-11) — 게이트 실측 통과 → `0.1.0`. ~~M1-1~~ ✅ (07-11, `feat/m1-gui`) — `nexa-gui` 분리.
2. **M1-2**: ADR-0002 렌더링 확정 — GDI vs DirectWrite GDI interop 스파이크 비교(품질·RSS·속도).
3. M1-3: 가상화 파일 리스트 초안(nexa-tree 배선·`RowSource` 자리). → [02](02-roadmap.md)
