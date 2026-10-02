# 성능 베이스라인 — 2026-10-02 (Ultracode 착수 전, `0ca1e48` 릴리스 빌드)

측정 = [`scripts/ui-qa.ps1`](../../../scripts/ui-qa.ps1)(PostMessage 조작) + 제목줄 계측(`first render`·`avg` paint) + `Get-NexaStats`(WorkingSet/Private/CPU). 창 1384×761 · 듀얼 패널 · 도크 2개(Info) · 라이트 테마. 수치는 실행마다 ±20% 변동(디스크 캐시).

| 항목 | 실측 | 비고 |
| --- | --- | --- |
| 프로세스 시작 → 창 핸들 | 959~1290 ms | `Start-Process` 후 MainWindowHandle 획득까지 |
| first render(제목줄) | 393 ms(홈 48항목) · 700 ms(.cargo 8항목, 콜드) | 기동 시각 기준 — 세션/설정/i18n/DW/글꼴/아이콘 초기화 포함 |
| 유휴 CPU 10 s | 0~1.1 % | 활성 창 기준(docs/29 P-5 ≤2% 충족) |
| 기동 직후 메모리 | WS 41 MB · Private 13.7 MB | |
| 클릭 10회(행 선택 이동, 48항목) | CPU 0.15~0.17 s = **15~17 ms/클릭** | 도크 Info 갱신·상태바·제목 포함 |
| 선택 이동 1회 + 도크 바 플래시 1.2 s | CPU 0.0~0.1 s | 플래시 틱 비용 미미(소 영역 무효화) |
| paint avg(제목줄) | 6.7~9.0 ms | 전체 창 기준 |
| 폴더 진입(.cargo → registry) | first render 유지, avg 7.3 ms | |
| Downloads 파일 10회 클릭 | CPU 0.17 s · WS 46.8 · Private 24.8 MB | 아이콘/도크 비용 포함 |
| 1차 실행 이상치 | 클릭 10회 CPU 0.85 s · 1.2 s 창 CPU 2.42 s · 종료 시 WS 91.6/Private 52 MB | 세션 복원 직후 Downloads(대형 JPG 10.5 MB 포함) — 아이콘/썸네일 초기 로드가 겹친 것으로 추정. **재현·원인 규명 대상**(X1/X2 워크스트림) |
| OneDrive 탭 복원(설치본, 10-02) | first render 1030 ms · avg 242 ms | 플레이스홀더/네트워크 열거 — 체감 지연 1순위 후보 |

목표(제안): first render < 150 ms(로컬) · 클릭 ≤ 5 ms CPU · paint avg ≤ 4 ms · 유휴 WS 안정(증가 0) · OneDrive 진입 시 UI 비블로킹.
