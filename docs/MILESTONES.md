# MILESTONES — 기능·마일스톤 기록 (프로젝트 목적 기준)

> **프로젝트 목적(포터블 단일 exe 네이티브 탐색기) 기준으로 기능과 마일스톤을 관찰**하는 단일 기록.
> 시간순 진행은 짝 문서 **[DEVLOG.md](DEVLOG.md)**. 로드맵 원안 [02](02-roadmap.md)·요구 [05](05-requirements.md).
> 상태: ✅ 완료 · 🚧 진행 · 📐 설계 · ☐ 미착수. ("완료" = 테스트 green + 해당 시 예산 게이트 통과.)

## 마일스톤 개요

| # | 목표 | 게이트 | 상태 |
| --- | --- | --- | --- |
| **M0** | 기반 — 코어 3크레이트 이식·Win32 창 스파이크·CI | 빈 창 RSS·exe 크기·임포트 실측 | ✅ `0.1.0` |
| **M1** | 뷰어 코어 — ★ 플래그십(인라인 트리+교차선택)·가상 리스트·정렬·타입어헤드 | 100k <150ms·60fps | ✅ `0.2.0` |
| **M2** | 셸 골격 — 경로바·탭/듀얼·메뉴·테마·설정/세션·IME/UIA 1차 | 상주 RSS ≤30MB | ☐ |
| **M3** | 파일 조작 — nexa-ops(Undo/Redo)·셸 메뉴·클립보드·DnD·watcher | — | ☐ |
| **M4** | 하단 패널 — 정보·미리보기·ConPTY 터미널 | — | ☐ |
| **M5** | 마감 — 잔여 패리티·릴리스 파이프라인·서명 결정 | 예산 최종 | ☐ |

---

## M0 — 기반·게이트 ✅ (`0.1.0`)

- ✅ 설계 문서 세트(00·01·02·05·06·10·11·12·15·18 + 운영 문서) — `docs/foundation` 병합(`d2727b5`).
- ✅ 워크스페이스 스캐폴딩(정적 CRT·릴리스 프로파일) · ✅ **nexa-core/vfs/tree 이식**(rlib 직접 링크, 테스트 21+5 green — FFI/ABI 폐지 실현) · ✅ Win32 창 스켈레톤(windows-rs, 맥 check 검증) · ✅ CI(core 2종+windows·예산 B2 게이트) — `feat/m0-scaffold`.
- ✅ 렌더 스파이크(M0-7 — 더블 버퍼·가시 100k행·휠/키 스크롤·DPI, `feat/m0-render-spike`) · ✅ **게이트 실측**(M0-8, Windows 실기): **B1 유휴 RSS 13.22MB ≤30 · B2 exe 0.20MB ≤10 · B3 임포트 OS 인박스만** + CI B3 화이트리스트 게이트화 → `0.1.0` 태그.

## M1 — 뷰어 코어 ✅ (`0.2.0`)

- ✅ M1-1 `nexa-gui` 크레이트 분리(`feat/m1-gui`) — 플랫폼 중립 위젯 trait·무효화(rect 병합)·입력 이벤트·테마 토큰(원본 docs/39 차용, 다크 기본 DR-5)·`VirtualRows`(스파이크 로직 이식·`RowSource` 추상). GDI 백엔드는 nexa-app `gdi.rs`(ADR-0002 확정까지).
- ✅ M1-2 [ADR-0002](07-adr-0002-rendering.md) **Accepted**(`feat/m1-adr0002-render`) — 텍스트 렌더링 = **DirectWrite GDI interop**. 실측: 스크롤 벤치 GDI 대비 −28%·RSS +4.1MB(17.4MB)·exe 0.23MB 전 게이트 내. 기본 백엔드 전환.
- ✅ M1-3 가상화 파일 리스트 초안(`feat/m1-virtual-list`) — **실제 파일시스템 첫 표시**: nexa-tree 평면 스트림 → `RowItem`(들여쓰기·▸/▾ 마커) 투영, 클릭=펼침/접힘. ADR-0002 §5 이행(GDI 삭제·레이아웃 캐시 — 벤치 1,673µs/프레임, RSS 18.2MB·exe 0.27MB).
- ✅ M1-4 컬럼 시스템(`feat/m1-columns`) — 원본 docs/23 이식: 5컬럼·헤더 정렬 **3상태 순환+Shift 다중열**(화살표 앞·순번 뒤 규약)·드래그 리사이즈·가로 스크롤·말줄임 트리밍·TZ 반영 날짜(civil_from_days 순수 구현, crate 0 유지). 벤치 1,813µs·RSS 18.1MB·exe 0.31MB.
- ✅ M1-5 ★ **플래그십**(`feat/m1-select`) — 원본 docs/07: 삼각형=펼침 vs 본문=선택 분리·**교차폴더 다중 선택**(Ctrl 토글·Shift 범위·Ctrl+A, 코어 OrderedSet·anchor 배선, AC2 테스트)·**러버밴드**(빈 영역 드래그)·캐럿·선택 하이라이트(sel_bg 토큰)·→/← 인라인 펼침. 벤치 1,540µs·RSS 18.2MB.
- ✅ M1-6 키보드 네비+타입어헤드(`feat/m1-keyboard`) — 원본 docs/32 확정 규약: 캐럿 이동+단일 선택+스크롤 추적·Shift 범위·Ctrl 캐럿만·Space/Ctrl+Space 토글·→자식/←부모·타입어헤드(가시 스트림 C·1s·반복 cycle·Backspace·HUD 배지). Shift+End 실기 "선택 61". Alt+화살표는 M1-8.
- ✅ M1-7 셸 아이콘(`feat/m1-icons`) — 원본 A-4 이식: icon_key(확장자 공유·exe류 파일별 키)·LRU 256·속도 제한 로딩 큐(80ms×4 — 스크롤 폭주 방지)·SHGetFileInfoW(USEFILEATTRIBUTES). 벤치 2,547µs·**RSS 27.95MB(여유 2MB ⚠ M2-8 트림 감시)**.
- ✅ M1-8 네비게이션(`feat/m1-navigation`) — 더블클릭/Enter 진입·히스토리(Alt+화살표·X버튼, push 시 앞으로 절단)·위로·숨김/점 토글(Ctrl+H·Ctrl+.). 소스 교체 시 정렬 유지. 실기 경로 왕복 확인.
- ✅ M1-9 게이트(`feat/m1-gate`) — **100k 첫 렌더 115ms(<150·열거 포함) · 스크롤 2,098µs/프레임(60fps 예산 13%) · B1 10k 유휴 RSS 27.87MB(≤30)**. 100k 스트레스 53MB는 예산 외 관측(M2-8·arena 회수 과제). → **M1 완료 `0.2.0`**.

## M1+ (요약)

[02 로드맵](02-roadmap.md) 참조. 기능 상세 스펙은 원본 nexa-dir docs(07 플래그십·23 컬럼·32 타입어헤드·33 파일조작·37 터미널·38 셸메뉴·39 테마·40 설정·42 i18n)를 원천으로 사용한다.
