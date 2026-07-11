# 10 · 통합 결정 기록 (Decision Record)

> 포터블 에디션의 **개발 방향·기술 요소를 확정**한 기록. 확정일: **2026-07-11**. 이후 변경은 새 ADR/journal로 추적.
> 원본 nexa-dir의 DR 중 본 저장소에 그대로 계승되는 것은 "계승"으로 표기.

## 1. 확정된 핵심 결정

| # | 영역 | 결정 | 근거 |
| --- | --- | --- | --- |
| **DR-1** | 기술 스택 | **올 러스트 단일 바이너리** — Win32(windows-rs) + 커스텀 드로잉(GDI→DirectWrite interop). 관리 런타임·UI 프레임워크·WebView 금지 | → [ADR-0001](06-adr-0001-stack.md) Accepted |
| **DR-2** | 예산(NFR) | **B1 유휴 RSS ≤30MB · B2 exe ≤10MB · B3 임포트=OS 인박스만** — 마일스톤 병합 게이트 | → [05 §2](05-requirements.md). 원본의 사후 최적화 실패 교훈 |
| **DR-3** | 배포 | **포터블 단일 exe 단독 채널**(설치 불요·영속물=exe 옆 `data\`). 서명은 후속(원본 PKG-4 결론 공유) | MSIX/setup.exe는 원본 담당 |
| **DR-4** | 코어 재사용 | 원본 `nexa-core`/`nexa-vfs`/`nexa-tree`를 **rlib 직접 링크로 이식**(cdylib/C ABI/ABI 버전 폐지). 의미 변경은 새 ADR | 원본 ADR-0004 계승 |
| **DR-5** | UX/디자인 | 원본 **M1 기능 패리티** 목표. 디자인 규약(프로툴 고밀도·다크 기본·키보드 우선·원본 docs/39 테마 토큰) 계승 | 원본 DR-2 계승 |
| **DR-6** | 라이선스 | 본체 **PolyForm Noncommercial 1.0.0**(개인무료/상업유료) · 의존성 **퍼미시브 온리**(GPL/AGPL 금지) | 원본 DR-5·의존성 정책 계승 |
| **DR-7** | 플러그인/미리보기 | .NET SDK(`Nexa.Plugins`) **비이관** — 내장 미리보기(텍스트/이미지)로 대체. WASM 호스트는 예산 충돌로 **보류**(M5+ 별도 ADR) | wasmtime 탑재 시 B1/B2 위반 |
| **DR-8** | 외부 crate | 기본 0 지향. 추가는 건별 기록(§2) + 예산 영향 평가. `windows`/`windows-sys`는 승인 | [05 §3 C3](05-requirements.md) |

### 1-1. ADR 색인

| ADR | 결정 | 상태 | 문서 |
| --- | --- | --- | --- |
| **ADR-0001** | 스택 — 올 러스트 + Win32 + 커스텀 드로잉 | **Accepted** | [06](06-adr-0001-stack.md) |
| ADR-0002 | 텍스트 렌더링 — GDI vs DirectWrite GDI interop (M0 스파이크 실측 후) | 예정(M1 초) | [07](07-adr-0002-rendering.md) 예약 |

### 1-2. 승인된 외부 crate (DR-8 원장, append)

| crate | 용도 | 라이선스 | 예산 영향 | 승인일 |
| --- | --- | --- | --- | --- |
| `windows` | Win32/COM 바인딩 | MIT/Apache-2.0 | exe +수백 KB(사용 API만 링크) | 2026-07-11 |

## 2. 원본과의 관계 (SSOT 분담)

- **기능 명세·UX 스펙·실측 교훈** = 원본 `nexa-dir/docs`가 원천 — 본 저장소 문서는 원본 문서를 직접 링크로 참조(중복 서술 금지).
- **본 저장소 결정**(스택·예산·패키징) = 이 문서 + ADR이 원천.
- 코어 크레이트 개선이 원본에도 유효하면 역이식(back-port)을 journal에 기록.

## 3. 다음 단계

1. M0 스캐폴딩 — 워크스페이스·코어 3크레이트 이식·Win32 창 스파이크·CI. → [02](02-roadmap.md)
2. M0 종료 게이트: 빈 창 RSS/exe 크기/임포트 실측 → ADR-0001 확증.
3. ADR-0002(렌더링) 확정 후 M1 뷰어 착수.
