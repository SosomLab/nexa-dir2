# STATUS — Nexa Dir 2 진행 현황

> **갱신: 2026-07-11 (KST)** — **M0 마지막 항목(M0-8)만 잔여**. 설계 문서 세트·`feat/m0-scaffold`(M0-1~6) 병합.
> `feat/m0-render-spike`: **M0-7 GDI 렌더 스파이크 완료** — 더블 버퍼(메모리 DC)·합성 100k 행 중 **가시 행만** 그리기·
> 휠/키보드 스크롤·DPI 대응 (맥에서 windows 타깃 check·clippy green, 테스트 26 green).
> 잔여: **M0-8 게이트 실측(Windows 실기)** — 빈 창 RSS·exe 크기·임포트 → `0.1.0`.

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
| B1 유휴 RSS | ≤30MB | — (M0 게이트 대기) | — |
| B2 exe 크기 | ≤10MB | — | — |
| B3 임포트 DLL | OS 인박스만 | — | — |

## 3. 마일스톤 (상세 [MILESTONES](MILESTONES.md))

- **M0** 기반·게이트 🚧 — 설계 문서 📐 완료 · 스캐폴딩/코어 이식/창 스파이크/CI/게이트 ☐.
- M1 뷰어(★플래그십) · M2 셸 골격 · M3 조작 · M4 패널 · M5 마감 — ☐.

## 4. 개발 모델 ([11](11-dev-environment.md))

- 맥 = 일상 개발(코어 test + **windows 타깃 cargo check로 UI 코드까지 타입 검증**) · Windows PC/CI = 실행·QA·예산 실측.

## 5. 다음 단계

1. ~~M0-1~6~~ ✅ (07-11, `feat/m0-scaffold`) — 스캐폴딩·코어 이식·Win32 스켈레톤·CI.
2. ~~M0-7~~ ✅ (07-11, `feat/m0-render-spike`) — GDI 렌더 스파이크(더블 버퍼·가시 100k행·스크롤·DPI).
3. **M0-8**: Windows 실기/CI에서 게이트 실측(빈 창 RSS·exe 크기·임포트) → journal 기록 → `0.1.0` 태그.
4. M1 착수: `nexa-gui` 분리 + ADR-0002(렌더링) 스파이크 비교.
