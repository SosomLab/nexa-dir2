# 01. 파일 조작(다수 파일) 조사 — 2026-09-04

대상: `crates/nexa-ops/src/*.rs`, `crates/nexa-app/src/{win.rs(start_transfer·on_transfer·paste·drop), recycle.rs, dnd.rs, bulkrename.rs, clipboard.rs}`.

## 구조 답변
- **실행 모델**: 모든 진입점(Ctrl+V `win.rs:7580`, 컨텍스트 메뉴 `win.rs:2552/2563/2749`, DnD `win.rs:2852`)이 `start_transfer`(`win.rs:3890`) 한 곳으로 모여 **워커 1개**(`win.rs:4048`)에서 `nexa_ops::transfer`(`lib.rs:350`) 실행. CopyFileEx/SHFileOperation 아님 — 4MiB 버퍼 read/write 루프(`lib.rs:42,154`). 진행 = 바이트(4MiB마다) + 항목. 취소 = 항목 간(`lib.rs:368`)·파일 중간(`lib.rs:173`) — **계획 단계는 불가**.
- **오류 격리**: 최상위 항목만(`lib.rs:445-453`). 하위 트리는 첫 오류가 전체 중단. 집계는 `Outcome::errors`지만 **개수만** 표시 후 폐기.
- **충돌**: 폴더 간 = 워커 모달(Yes/Yes-all/Skip/Cancel, `win.rs:4061-4098`) · 같은 폴더 = `unique_dest`. O(N²) 지점 아래.
- **볼륨**: `same_volume`은 접두만 비교(`lib.rs:79`), rename 실패 폴백 없음. 재귀 깊이 무제한.
- **UI 스레드**: 작업 후 양 패널 동기 재열거 · DragOver마다 `is_dir()` · 일괄 이름변경 키 입력마다 O(N²)+N syscall.
- **undo**: 100개 작업 상한(메모리 상한 아님). **휴지통**: 단일 배치 `SHFileOperationW`(`win.rs:2761-2778`) — 정상.

## 발견(영향순)
| # | 심각도 | 위치 | 문제 → 수정 방향 |
| --- | --- | --- | --- |
| 1 | HIGH | `nexa-ops/src/lib.rs:219-221`(copy) · `:241-247`(move) | `overwrite && dest.is_dir()` → `remove_dir_all(dest)` **먼저** → 10GB 덮어쓰기 10%에서 취소/실패 시 옛 대상 소실·부분 트리 잔존·undo 없음. → 대상 폴더에 임시 이름으로 쓰고 성공 후 교체(`ReplaceFileW`/rename 스왑) |
| 2 | HIGH | `dnd.rs:427-435` → `win.rs:3897` | `steal_volatile`이 원본을 `%TEMP%\NexaDir\dnd-…`로 옮긴 **뒤** `start_transfer` 첫 줄 `st.transfer.is_some() { return }` → 전송 중 드롭은 조용히 폐기, 파일은 스테이징에 고립. `dest_cloud` 조기 return도 동일. → 큐잉 또는 스틸 전에 "사용 중" 거부 |
| 3 | HIGH(perf) | `win.rs:4136-4149` · `:4197-4212` | ItemEnd마다 PostMessage + `plock(items).clone()` 전체 Vec + O(N) 스캔 2회 + Invalidate + SetWindowText → 50k 파일 = 50k×50k 복사. → 30Hz 스로틀·스칼라 전달·`len()≤512`에서만 스냅샷 |
| 4 | HIGH(perf) | `lib.rs:359` · `:250` | 계획 단계 `size_of` 전 트리 순회 + 같은 볼륨 이동은 rename 후 `size_of(dest)` **재순회** · `size_of`에 cancel 없음. → Move&&same_volume이면 계획 생략·cancel 전파 |
| 5 | HIGH | `lib.rs:196-207` | 하위 엔트리 `?` 전파 = 한 파일 실패가 폴더 전체 중단. `FileType::is_dir()`가 정션/디렉터리 심링크에 false → `copy_file_with_progress` → ACCESS_DENIED → 폴더 중단. 부분 대상 미정리·깊이 무제한. → 엔트리별 오류 수집 후 계속·재분석점 분기·반복 순회+깊이 상한 |
| 6 | HIGH(perf, UI) | `win.rs:3269-3278` → `panel.rs:1397`·`:1437-1443` → `nexa-tree lib.rs:540` | `reload_both` = 양 패널 동기 전체 열거 + `index_of_path` 선형 스캔으로 선택 복원 O(S×V)(10k 선택×50k 행 = 5·10⁸). 전송·삭제·이름변경·undo·**감시 디바운스 틱**마다. → 소문자 경로 HashMap 1회 구축·영향 패널만·워커 |
| 7 | MED-HIGH | `win.rs:4277-4279` | `Outcome::errors`(경로+사유)를 개수만 제목줄에 표시하고 폐기 — 삭제 경로(`win.rs:3516-3534`)는 목록+재시도 있음. → `name_list`+`dialog::show_buttons` [실패 재시도] 재사용 |
| 8 | MED-HIGH(UI) | `batch_rename.rs:710-716`·`:727-729` ← `bulkrename.rs:447`(EN_CHANGE) | 항목마다 중첩 O(N) 스캔 2회(할당 동반) + N회 `exists()` — 키 입력마다. → 소문자 사전 계산·HashMap·`exists()`는 적용 시/디바운스 |
| 9 | MED | `win.rs:5752-5757` · `lib.rs:288-293` | 일괄 이름변경이 스왑/시프트(`1→2, 2→3`) 불가 — 검증은 최종 이름만 → 적용 중 부분 실패. UI 스레드·진행/취소 없음. → 위상 정렬 + 순환은 임시 이름 2단계 |
| 10 | MED | `win.rs:3816`·`:3763-3770`·`:3877-3884`·`:7881` · `clipboard.rs:620` | 클라우드/가상 붙여넣기 진행 슬롯에 busy·세대 가드 없음(동시 시작 시 취소 핸들 고립·창 파괴) · 완료 통지가 `let _ = PostMessageW`(재시도 없음 → 유실 시 이후 가상 붙여넣기 영구 동기 폴백). → `st.transfer`와 같은 gen 가드·`post_final_notify` 경유 |

## 그 외
- `clipboard.rs:381-393` `plan_dests`: 계획 시점 `unique_dest` + 이름 키 → 같은 이름 첨부 2개가 같은 대상(마지막 승리)·`a.txt`+`a (2).txt` 충돌. → 예약 이름 HashSet.
- `lib.rs:248-251`: 마운트 포인트(`ERROR_NOT_SAME_DEVICE`)에서 rename 실패 시 copy+delete 폴백 없음.
- `build.rs`: 매니페스트 없음 → `longPathAware` 없음. `SHFileOperationW`는 260자 초과 불가.
- `history.rs:48,78-84`: 100작업 × 전체 쌍 Vec — 메모리 상한 없음.
- `dnd.rs:179`·`win.rs:2842`: DragOver마다 stat + O(N) 할당 스캔.
- 충돌 대화상자에 "모두 건너뛰기" 없음 · 동시 전송 무응답 · `transfer`가 `dest_dir` 미생성.

## 견고한 부분
단일 퍼널 · 실제 취소(항목 간 + 4MiB) + 부분 파일 정리 · 휴지통 단일 배치 + 사전 잠금 프로브 + 사후 존재 diff + 부분 undo + 재시도 모달 · 드롭 `DROPEFFECT_NONE`(원본 경쟁 삭제 방지) · `steal_volatile` · `post_final_notify` 50회 재시도 + gen 가드 · `in_flight` 가드 · `plock` · `sanitize_rel` · 순환 이동 3중 방어 · nexa-ops 테스트(취소·충돌·이벤트 프로토콜).
