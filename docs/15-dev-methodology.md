# 15 · 개발 방법론 — 작은 단위 · 점진 프로토타이핑 · 단위별 커밋

> **원본 docs/15를 그대로 계승**한다(수직 슬라이스·초안→확장·단위=커밋). 이 문서는 차이점과 본 저장소 백로그만 기술.

## 1. 핵심 원칙 (원본 계승)

1. **수직 슬라이스** — 관찰·테스트 가능한 얇은 끝단(코어→gui→창) 단위.
2. **작게·순차로** · **초안 먼저, 확장은 별도 커밋** · **main 항상 green** · **단위 = 커밋 1개** · **테스트 동반**.
3. 큰 미지수는 **스파이크**(버릴 수 있는 실험)로 먼저 검증 — 본 저장소 M0의 렌더링 스파이크가 대표.

## 2. 본 저장소 추가 규율

- **예산 게이트**(DR-2): 마일스톤 종료 시 B1~B3 실측·기록. 초과 상태로 main 병합 금지.
- **원본 참조 우선**: 기능 설계 전 원본 문서·코드를 먼저 확인(재발명 금지). 이식 커밋 본문에 원본 경로 명기.
- **브랜치**: 설계/개발 큰 단위마다 브랜치(`docs/…`, `feat/m0-…`, `refactor/…`) → 세부 기능 단위 커밋 → green 확인 후 main 병합. **push는 사용자 명시 요청 시에만.**
- 커밋 규약: Conventional Commits `type(scope)` — scope 예: `core/tree` `gui/list` `app/win` `ops` `shell` `term` `pkg`.

## 3. 진행 추적 (원본 규약 차용)

- 일자 상세 = `docs/journal/YYYY-MM-DD.md`(시간 역순) · 요약 = [DEVLOG](DEVLOG.md)(시간 역순) · 기능/마일스톤 = [MILESTONES](MILESTONES.md) · 브랜치 이력 = [BRANCHES](BRANCHES.md).
- 단위 시각 표기 `> ⏱ …(커밋)` — git 커밋 시각(KST) 원천.

## 4. M0 → M1 초입 단위 백로그 (순차)

**M0 (기반·게이트)**
1. `chore(repo)`: 워크스페이스 스캐폴딩(Cargo.toml·rust-toolchain·프로파일)
2. `feat(core)`: 원본 nexa-core 이식 + 테스트
3. `feat(core/vfs)`: 원본 nexa-vfs 이식 + 테스트
4. `feat(core/tree)`: 원본 nexa-tree 이식 + 테스트(플래그십 코어)
5. `feat(app/win)`: Win32 창 스켈레톤(클래스 등록·메시지 루프·WM_PAINT 배경) — 맥 스텁 포함
6. `ci`: core(ubuntu/mac test) + windows(build·test·예산 검사) 워크플로
7. `feat(app/win)`: 렌더 스파이크(GDI 텍스트 N행 그리기·DPI) → **M0 게이트 실측**(빈 창 RSS·exe 크기·임포트)

**M1 (뷰어 코어, 초안→확장)**
8. `feat(gui)`: nexa-gui 분리(위젯 trait·무효화·입력 라우팅)
9. ADR-0002 렌더링 확정(GDI vs DirectWrite interop 스파이크 비교)
10. `feat(gui/list)`: 가상화 리스트 초안(nexa-tree 배선·스크롤) → 확장(컬럼·정렬 헤더·아이콘)
11. `feat(gui/list)`: 인라인 펼침·교차 선택(코어 그대로) → 키보드 네비 → 타입어헤드
12. `feat(gui)`: 스크롤바·포커스·IME 1차
