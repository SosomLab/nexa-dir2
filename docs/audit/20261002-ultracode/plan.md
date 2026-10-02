# Ultracode 품질 개선 계획 — 2026-10-02

> 입력: 16개 분석 그룹(G1~G13 파일 단위 · X1~X4 횡단)의 발견 중 **반박 검증을 통과한 242건**과 **기각 40건**, 그룹 요약·누락 지적.
> 기준 커밋: `424e3ab`(도크 휠 분수 누적 반영) · 작업 사본에 `crates/nexa-app/examples/ctxmenu_probe.rs` 미커밋 수정 있음.
> 성능 기준선: [baseline.md](baseline.md) · 우클릭 지연: [ctxmenu-latency.md](ctxmenu-latency.md).
> 이 문서는 **계획**이다. 코드 변경은 아래 배치 순서대로 작업 1개 = 커밋 1개(Conventional Commits)로 진행한다.

## 0. 읽는 법

- **계획 ID**: `A`=견고성·오류, `B`=성능·UX 체감, `C`=중복·모듈성, `D`=불용 코드, `E`=테스트, `T0`=게이트·계측 정상화. 각 행에 원 발견 ID(G·X)를 병기한다.
- **M 표기**: 그룹 요약의 "누락 지적"에서 승격한 항목. 반박 검증을 거치지 않았으므로 **착수 전 재현 확인**이 선행 조건이다.
- **줄 번호**: 발견 다수가 HEAD 대비 약 90~120행 어긋나 있다(검증 단계 보정 기록). 착수 시 함수명으로 `grep` 재대조한다.
- **이미 반영됨**: G6-04의 도크 부분(정밀 터치패드 휠 누적)은 `424e3ab`에서 해결됐다. 터미널 경로만 A34로 남긴다.
- **TODO 연계**: X-52(동기 재열거)·X-53(세션 지연 복원)·X-54(rcPaint·캐럿·재파싱·index_of_path)·X-56(워커 이동·링커 캐시)·X-61(우클릭 예열)과 겹치는 항목은 신규 TODO를 만들지 않고 해당 항목의 근거로 기록한다.

---

## 1. 총괄

### 1-1. 그룹별 확정·기각

| 그룹 | 범위 | 확정 | HIGH | MED | LOW | 기각 |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| G1 | win.rs 1/3 (기동·상태·레이아웃·명령) | 18 | 1 | 6 | 11 | 0 |
| G2 | win.rs 2/3 (페인트·도크·터미널·클립보드) | 15 | 3 | 5 | 7 | 2 |
| G3 | win.rs 3/3 (wndproc 입력·타이머·통지) | 18 | 1 | 9 | 8 | 3 |
| G4 | 패널·네비·감시·폴링 | 10 | 2 | 6 | 2 | 3 |
| G5 | 렌더링 백엔드·아이콘·SVG·글꼴 | 11 | 1 | 5 | 5 | 2 |
| G6 | nexa-gui 위젯 | 10 | 2 | 4 | 4 | 3 |
| G7 | 코어 데이터(nexa-core·vfs·tree) | 15 | 2 | 8 | 5 | 1 |
| G8 | 파일 조작·클립보드·DnD·휴지통 | 18 | 1 | 8 | 9 | 5 |
| G9 | 터미널 | 12 | 0 | 4 | 8 | 4 |
| G10 | 설정·세션·i18n·런처 | 11 | 0 | 4 | 7 | 6 |
| G11 | 미리보기·압축·플러그인 | 15 | 2 | 5 | 8 | 0 |
| G12 | 클라우드·OAuth | 21 | 3 | 14 | 4 | 2 |
| G13 | 다이얼로그·컨트롤·일괄 이름변경·기타 | 12 | 1 | 5 | 6 | 5 |
| X1 | 횡단: 기동·첫 렌더·폴더 진입 | 15 | 3 | 8 | 4 | 0 |
| X2 | 횡단: 클릭·선택·우클릭·키 반응 | 11 | 3 | 5 | 3 | 3 |
| X3 | 횡단: MC/DC 조건식 전수 | 14 | 2 | 4 | 8 | 0 |
| X4 | 횡단: 커버리지·불용 코드 | 16 | 0 | 3 | 13 | 1 |
| **합계** | | **242** | **27** | **103** | **112** | **40** |

242건에는 같은 결함을 여러 그룹이 찾은 중복 군집이 많다(예: 미리보기 재생성 5건, 캐럿 타이머 3건, 프로브 스윕 6건). 이를 통합해 **실행 작업 약 160개**(배치 0~21 + 백로그)로 정리했다.

### 1-2. 가장 중요한 10건 (크래시·데이터 오류·체감 지연 우선)

| 순위 | 계획 ID | 원 발견 | 내용 | 왜 먼저인가 |
| ---: | --- | --- | --- | --- |
| 1 | A01 | G13-01 | `pathinput::expand_env`가 `to_lowercase()` 바이트 오프셋으로 원문을 잘라 'İ%한%'·'K%TEMP%' 입력에서 panic. 경로 바 **키 입력마다** 호출 | release `panic=abort` → 타이핑 중 즉사. 09-22 shellpath 크래시와 같은 유형. 재현 확정 |
| 2 | A02·A03·A05 | G7-01·G7-02·G7-11·G7 누락1 | 압축 항목명 'a한.txt' 슬라이스 panic, tar PAX 길이 오류 panic, 조작된 tar의 base-256 크기 wrap으로 **UI 스레드 무한 루프** | 미리보기는 UI 스레드 동기 실행 → 파일 하나로 즉사·행 |
| 3 | A12 | X2-03·G3-08·X3-09 | 터미널 포커스 블록에서 `terms[ti]==None`이면 키가 파일 목록으로 샘. 종료 터미널을 문자 키로 재시작하면 WM_CHAR가 **결정적으로** 누수, ConPty 실패 상태면 Delete가 보이지 않는 캐럿 항목을 휴지통으로 | 확인 없는 삭제로 이어지는 입력 라우팅 결함 |
| 4 | A14 (M) | G8 누락1 | 교차 볼륨 폴더 이동이 `read_dir().flatten()`으로 열거 오류를 버린 뒤 `remove_dir_all(src)` → 건너뛴 항목이 사본에도 원본에도 없음 | 데이터 손실 후보. 재현 확인 후 최우선 |
| 5 | A18·A19 | X3-02·G3-02·X3-01 | 싱글 정보 모드에서 0-rect 우 도크가 '터미널'이면 우 파일 목록 클릭마다 숨은 터미널이 키보드 포커스를 가져감. 공유 도크 클릭은 활성 패널을 좌로 뒤집어 미리보기 내용을 바꿈 | `h > 0` 피연산자 하나 누락(X-20에서 panel.rs만 고침) |
| 6 | A17·A20 | G6-01·X3-03 | OLE 드래그 뒤 MouseUp이 오지 않아 `press_pending`이 남고, 다음 클릭에서 선택이 낡은 인덱스(재로드 후 다른 파일)로 붕괴 | 다음 동작(삭제·잘라내기)의 대상이 바뀜 |
| 7 | A-rowsclip | G6-02·X4 누락1 | 파일 목록이 가로 스크롤 시 bounds 왼쪽 밖 셀을 클립 없이 칠해 우 패널이 좌 패널 오른쪽 가장자리를 덮어씀 | 10-02 (c) 도크 번짐과 같은 결함이 목록에 남음 |
| 8 | A25 | X2-01·X1-02·X2-13 | 더블클릭·Alt+방향키·X버튼 폴더 진입이 `update_status`를 건너뜀 → 상태바·도크·watcher·**프로브 기준선**이 0~3초 낡음 | X-44 S2 수정이 이 경로에서 재현 = '간헐 무갱신' 잔여 경로 |
| 9 | B02·B01 | G2-02·G1-06·G3-16·G2-01 | 터미널 포커스를 둔 채 비활성·최소화돼도 캐럿 타이머가 530ms마다 **창 전체** 재도장(rcPaint 무시) + 유휴 트림 직후 DW 백엔드 재생성 | DR-2 유휴 RSS·CPU 0% 규율이 조건 하나로 무력화 |
| 10 | B04 | G1-01·G2-10·G11-01·X1-10·X2-02·X2 누락3 | `update_status`(31곳) 마다 도크 미리보기를 캐시 없이 재생성(파일 읽기·wasm 실행·압축 재파싱), Info도 매번 `fileinfo::basic` 동기 I/O. 수식키 단독 입력에도 실행 | 클릭 15~17ms(목표 ≤5ms)의 주범 후보 |

차순위: A15(G11 누락1 — `read_at` 고정 연료 과금 때문에 번들 archive.wasm이 수천 멤버 .lib/cpio에서 **항상** 실패하고 브레이커까지 트립), A44(G12-01 — 클라우드 목록 500건 초과 조용히 절단), A13(G13 누락1 — File>Exit가 `PostQuitMessage`만 호출해 세션 저장 경로 우회), 백로그 1번(G2-06 — undo/redo가 교차 볼륨 이동을 UI 스레드에서 동기 재실행), B05(X1-01 — 기동 시 창이 뜨기 전 활성 탭 이중 열거).

---

## 2. 워크스트림

열 설명: **배치** = 3장의 실행 배치 번호, `BL` = 백로그(7장), `U` = 사용자 결정(6장).

### 2-A. 견고성·오류 수정 (mcdc / robustness)

| 계획 ID | 원 발견 | 심각도 | 파일 | 요지 | 배치 |
| --- | --- | --- | --- | --- | --- |
| A01 | G13-01 | HIGH | pathinput.rs | ASCII 대소문자 무시 검색으로 `s` 하나만 인덱싱 | 1 |
| A02 | G7-01 | HIGH | nexa-vfs archive/mod.rs | `normalize_path` 바이트 비교 | 1 |
| A03 | G7-02 | HIGH | archive/tar.rs | `parse_pax` 바이트 기반·`len <= sp+1` 가드 | 1 |
| A04 | G7-03 | MED | archive/rar.rs | rar5 extra `checked_add` 전진 보장 | 1 |
| A05 | G7-11(tar)·G7 누락1 | MED→HIGH | archive/tar.rs | 오프셋 `checked_next_multiple_of`·`checked_add` 실패 시 break | 2 |
| A06a | G7-11(zip·rar5)·G7-12 | MED/LOW | zip.rs·rar.rs·cab.rs | 오프셋 checked, CAB 상한 +1 규약 통일(truncated) | 2 |
| A06b | G7 누락5 | LOW | archive/mod.rs | `Listing::totals` saturating_add | 20 |
| A07 | G9-01·G9-11 | MED | conpty.rs | `Utf8Chunker`(불량 바이트 U+FFFD 치환·꼬리 보존) 순수 분리 | 1 |
| A08 | G9-02·X4-06 | MED | nexa-term lib.rs | SU/SD `n.min(영역 높이)` | 1 |
| A09 | G9-05·X4-01 | LOW | nexa-term lib.rs | `S::Charset`·`S::Str`(DCS/PM/APC/SOS) 상태 추가 | 2 |
| A10 | G9-08·G9 누락3 | LOW | nexa-term lib.rs | CSI 중 ESC 재진입·C0 즉시 실행·pars 상한·'>' '=' '<' 마커 | 5 |
| A11 | G10-04 | LOW | secret.rs | `decode_hex`(ASCII·짝수 길이) | 1 |
| A12 | X2-03·G3-08·X3-09 | HIGH | win.rs | `term_focus_live` 판정 후 KEYDOWN·CHAR 항상 소비, ConPty 실패 시 포커스 해제 | 1 |
| A13 (M) | G13 누락1 | HIGH 후보 | win.rs | `CMD_EXIT` → `PostMessage(WM_CLOSE)` | 2 |
| A14 (M) | G8 누락1 | HIGH 후보 | nexa-ops lib.rs | 열거 오류 전파, 교차 볼륨 이동은 복사한 항목만 삭제 후 빈 폴더 `remove_dir` | 2 |
| A15 (M) | G11 누락1 | HIGH 후보 | preview/wasm.rs | `read_at` 고정 과금 축소·바이트 비례, 누적 바이트 상한으로 폭주 방지 | 2 |
| A16 | G8-03 | LOW | dnd.rs | `steal_volatile` temp_dir 접두 가드·`is_virtual` 재판정 | 2 |
| A16b | G8-04·G8 누락5 | MED | win.rs | src가 `%TEMP%\NexaDir\dnd-` 접두면 VPasteOp로 undo 기록, 빈 슬롯 정리 | 2 |
| A17 | G6-01·X3-03 | HIGH | rows.rs | MouseDown·RightDown 진입 시 `press_pending=None`, `abort_press()` 공개, replace_source 보조 리셋 | 3 |
| A18 | X3-02 | HIGH | panel.rs·win.rs | `Panel::dock_shown()`(visible && h>0)로 win.rs 판정 통일 | 3 |
| A19 | G3-02·X3-01·G3-17 | HIGH | win.rs | 공유 도크(Info/Preview) 클릭은 활성 유지, DBLCLK도 `panel_at_pt` | 3 |
| A20 | X3-03·X2-12 | HIGH/LOW | win.rs | `begin_drag` 반환 직후 `abort_press`, 반환 bool이 거짓이면 재로드 생략 | 3 |
| A21 | G3-01·G3-09·G3-15·G3 누락4 | MED | win.rs | TUI 우클릭 SetCapture·버튼별 held 판정, `reset_mouse_transients`, RBUTTONUP 쌍 확인, LBUTTONDOWN 선두 `drag_press=None` | 3 |
| A22 | G6-10 | MED | dock.rs | 팝아웃 판정을 오버레이 바보다 먼저 | 3 |
| A23 | G8-10·G8 누락4 | MED | clipboard.rs | `Open::new` 짧은 재시도(5×10ms) — 흐림 표시·Ctrl+V·쓰기 공통 | 3 |
| A24 | G8-20 | LOW | shellmenu.rs | TrackPopupMenuEx 직후 `ACTIVE` 비움 | 3 |
| A25 | X2-01·X1-02·X2-13 | HIGH | win.rs | `finish_input`/`after_navigation` 꼬리 통일(DBLCLK·SYSKEYDOWN·XBUTTON·LBUTTONUP·CHAR·탭 바 새 탭), 도크 이중 갱신 제거 | 4 |
| A26 | G3-05·X2-06 | MED | win.rs | `pending_rename(panel, path)` 대조 후에만 리네임 진입 | 4 |
| A27 | G3-03 | MED | win.rs | 리네임 필드 안 더블클릭은 activate_row 금지 | 4 |
| A28 | G3-06·G3 누락3 | MED | win.rs | Term 대상 확정·터미널 클릭 시 경로바 편집·리네임 취소 | 4 |
| A29 | X3-07 | LOW | rows.rs | `tree_col()`로 리네임 필드·마커 존 공용 | 4 |
| A30 | G5-04 | MED | icons.rs | `is_per_file`(`L|` 접두 제거 후 판정) 공용 | 4 |
| A31 | G7-04·G7 누락4 | MED | nexa-vfs lib.rs | 폴더 링크·정션은 `Dir`, 링크 표식은 별도 비트(0x400 단독 금지) | 4 |
| A32 | G10-17·G10-14 | LOW | secret.rs | `save_token`을 `config::save`로, `clear_from`이 `clear_token` 사용, 낡은 allow 제거 | 4 |
| A33 | G3-04·X2-07·X3-04 | MED | win.rs·route.rs(신규) | `wheel_route` 순수 함수 — 터미널 히트면 Shift·wrap 무관 소비 | 5 |
| A34 | X3-06·X3 누락1·G6-04(터미널) | LOW | win.rs | 터미널 스크롤백·TUI 휠을 `WheelAccum` 하나로 | 5 |
| A35 | G2 누락1·G2-15 일부 | MED | win.rs | `term_cell_of` 공용(view_x 반영) — 마우스 모드 좌표 오류 수정 | 5 |
| A36 | G9-12 | LOW | conpty.rs | `HandleGuard`·`AttrList` RAII | 5 |
| A37 | X3-10 | LOW | rows.rs | 마커 존 우클릭도 선택 규약 적용 | 5 |
| A38 | X3-14 | LOW | dock.rs | `scroll_to` 끝에서 `scroll_x` 재클램프 | 5 |
| A39 | G4-01 | HIGH | panel.rs | `nearest_existing_ancestor`(UNC share 루트에서 정지·네트워크 오류면 순회 생략) | 7 |
| A40 (M) | G4 누락1·G4-07 단기 | HIGH 후보 | win.rs | sync_watchers 실패 메모·지수 백오프(3s→30s→5min), 탐색·F5 시 리셋 | 7 |
| A41 | G4-07 | MED | watcher.rs | `thread::Builder`(이름·64KB 스택)·`WatchFail` 반환 | 8 |
| A42 | G7-05 | MED | main.rs | `SetErrorMode(SEM_FAILCRITICALERRORS|SEM_NOOPENFILEERRORBOX)` | 7 |
| A43 | X3-11·G12 누락4 | MED | win.rs | `cloud_slot_busy` 게이트(쓰기·다운로드·계정 간 복사·외부 드롭) | 10 |
| A44 | G12-01 | HIGH | cloudfs.rs | 3사 커서 루프(Google `fields`에 nextPageToken 추가), 페이지 상한·취소 | 10 |
| A45 | G12-06·G12 누락1 | HIGH | cloudfs.rs | `resolve_google_id`→`Result<Option>`, Dropbox ensure_folder `autorename:false`, 409/conflict만 Ok | 10 |
| A46 | G12-07 | MED | cloudfs.rs | `part_path` 덧붙임 + 실행별 stage 하위 폴더 | 10 |
| A47 | G12-13 | MED | oauth.rs | CrackUrl·Location 버퍼 길이 조회 후 재호출 | 10 |
| A48 | G12-04 | LOW | oauth.rs | `classify_redirect` — 무관 연결은 404 후 계속 | 10 |
| A49 | G8-02 | MED | nexa-ops lib.rs | 하위 파일 오류 수집 후 계속, 신규 폴더 취소 시 정리 | 10 |
| A52 | G8-06 | LOW | nexa-ops lib.rs | rename 오류 17(EXDEV 18)이면 복사+삭제 폴백 | 11 |
| A53 | G11-02 | MED | preview/wasm.rs | 시간·연료 초과는 카운트 제외, 격리 후 half-open 1회 재시도, 압축 능력 실패는 `Archive(Failed)` | 11 |
| A54 | G10-03 | MED | prefs.rs | `(ID_OPT_BASE..ID_NAV_BASE)` 범위 가드 + 플러그인 아암 | 12 |
| A55 | G2-03 | MED | prefs.rs·config.rs | sanitize에 글꼴 크기 5종 클램프, `FONT_PT_RANGE` 상수 공유 | 12 |
| A56 | G10-05 | LOW | config.rs | `kv_lines` BOM 제거·`=` 주변 trim(launcherN·tabs 왕복 검증) | 12 |
| A57 | G13-06 | MED | dialog.rs | 첫 버튼 포커스·Esc 선처리(결과 0)·`outer_size` 헬퍼 — IDCANCEL=2가 '모두 덮어쓰기'로 확정되는 함정 회피 | 12 |
| A58 | G13-16 | MED | tip.rs | `MonitorFromPoint` rcWork 클램프(음수 좌표 모니터) | 12 |
| A59 | G13-05 | MED | pwprompt.rs | `outer_size`(AdjustWindowRectExForDpi) 사용 | 13 |
| A60 | G13-17 | LOW | ctl/spin.rs·segmented.rs | 키 경로 saturating, 빈 items 조기 반환 | 14 |
| A61 | G11-11 | LOW | previewwnd.rs | ESC → `PostMessage(WM_CLOSE)` | 6 |
| A62 | G11-07 | LOW | preview/mod.rs | render_svg 임시 파일 tmp→rename·크기 검증 | 13 |
| A63 | G11-14 | LOW | preview/mod.rs·wasm.rs | read_text `take().read_to_end`, password 길이 초과 시 음수 통보 | 13 |
| A64 (M) | G11 누락3 | LOW | previewwnd.rs·wasm.rs | 플러그인 지정 이미지 경로를 `%TEMP%\nexa-preview\d*.bmp`·대상 파일로 제한 | 21 |
| A65a·b | G9-07(?25)·G9-06 | MED/LOW | nexa-term lib.rs → win.rs | DECTCEM·2004 추적 → 캐럿 조건·`term_paste` 괄호 | 13 |
| A66 | X3-05 | MED | win.rs·panel.rs | `refresh_on_return`·`reload_both`에도 편집 중 보류 가드 | 14 |
| A67 (M) | G10 누락·X1 누락2·G4 누락2 | MED | panel.rs·win.rs | `restore`가 살아남은 원래 인덱스 반환 → active·locked·pinned·modes·views 재매핑 | 6 |
| A69 | G1-02 | MED | win.rs·panel.rs | `open_start_tree(path, ctx)`·`open_any_root_filtered` | 9 |
| A70 (M) | G9 누락1 | MED | win.rs | 터미널 기동 실패 메모(같은 cwd 재시도 금지, 클릭·cwd 변경 시만) | 14 |
| A-rowsclip | G6-02·X4 누락1 | HIGH | rows.rs·draw.rs | paint `push_clip(bounds)`(Tiles 조기 return 포함 pop 보장) + `clip_text_left` 첫 가시 문자부터 | 9 |
| A-G6-06 | G6-06 | MED | rows.rs | `bar_hit()`(바가 소비하는 술어와 동일) → `row_at`·호스트 체인 | 14 |
| A-IsIconic | G3-10·X1-08 | LOW/MED | win.rs | 비활성 분기 `!IsIconic`일 때만 FSPOLL 재무장 | 7 |
| A-G8-15 | G8-15 | LOW | clipboard.rs | `read_text` GlobalSize 상한, tymed 확인 | 15 |
| A-G8-18 | G8-18 | LOW | batch_rename.rs·lib.rs | 예약 장치명(stem) Invalid — 확장자 형태는 U9 | 20 |
| A-G1-14 | G1-14 | LOW | win.rs | WM_TIMECHANGE → tz 갱신(TreeSource 전파 포함) | 20 |
| A-G1-15 | G1-15 | LOW | win.rs | DW 생성 실패 진단 파일 + GDI 폴백 1줄 | 20 |
| A-G9m2 (M) | G9 누락2 | LOW | nexa-term lib.rs | IL/DL이 DECSTBM 마진 밖이면 무시 | 21 |
| A-G2-16 | G2-16 | LOW | win.rs | `bench(hwnd)` 시그니처로 별칭 제거 | 20 |
| A-G6-08 | G6-08 | LOW | rows.rs | 타일 PageUp/Down·드래그 엣지 존을 `grid_h()` 기준 | 20 |

### 2-B. 성능 / UX 체감 (performance / ux-latency)

| 계획 ID | 원 발견 | 심각도 | 파일 | 요지 | 배치 |
| --- | --- | --- | --- | --- | --- |
| B01 | G2-01 | HIGH | win.rs | paint `rcPaint` 기반 위젯 intersects 가드 + DW 클립(오버레이·드롭다운 예외, 백엔드 재생성 직후 전체) | 9 |
| B02 | G2-02·G3-16·G1-06③ | HIGH | win.rs | 캐럿 타이머: ACTIVATEAPP(0)·최소화·trim에서 Kill, 활성·note_activity에서 재무장(`set_term_caret_timer` 단일화) | 5 |
| B03 | G1-06①②·G9 누락4 | HIGH | win.rs·dw.rs | term_paint 런 단위(`term_glyph` 배경 없음), 캐럿 셀 rect만 무효화, 전체 rect `pal.bg` 선채움 | 9 |
| B04 | G1-01·G2-10·X1-10·X2-02·G11-01·X2 누락3·G7-06(캐시) | HIGH | win.rs | 패널별 도크 키(경로·mtime·len·kind·dark·preview_map·plugins_disabled + `dock_subject_key`) 같으면 재생성 생략, Info도 적용, **활성 암호 슬롯이 있으면 우회**·pw::remember/forget 시 무효화, `set_kinds`는 생성·apply_lang에서만 | 6 |
| B05 | X1-01·X1-05①·X1 누락1·G3 누락2 | HIGH | win.rs | `refresh_on_return`을 코얼레싱 타이머(30ms)로, 최초 표시 전 활성화는 재로드 안 함, `State.minimized` 별도 플래그 | 7 |
| B06 | G1-05·G2-08·G3-11·G4-02·X1-07(1단계)·G4 누락3 | MED/HIGH | win.rs·fsprobe.rs·source.rs | `probe_skip_slow`를 fsprobe로 공용화, 스윕·update_status 기준선에서 UNC·DRIVE_REMOTE·CDROM은 루트만(또는 생략) | 8 |
| B07 | G4-03 | MED | shellnotify.rs·win.rs | `take_payload`·`hits_watched` — 감시 대상 아닐 때 디바운스 무장 안 함(해석 실패는 발화) | 8 |
| B08a | G7-10 | MED | nexa-tree | `VisibleRowRef`에 id·depth·attrs·expanded·has_children, `row()`는 그 파생 | 7 |
| B08b | G4-05·X1-09·G7 누락3 | MED/LOW | panel.rs | watch_dirs·sync_expanded·viewport_dirs 클론 제거, Flat/Tiles 조기 반환 | 8 |
| B09 | G4-06·G7-08·G7-09·G2-11 | MED | nexa-tree·panel.rs | `path_key` 단일 규칙 + `indices_of_paths` 배치 API(O(V+S)) → 선택 복원 교체 | 8 |
| B11 | X1-04·X1 누락3 | MED | rows.rs·source.rs | Flat/Tiles에서 마커 계산(read_dir 프로브) 생략 플래그 | 7 |
| B12a | G5-05·X1-12 | LOW | icons.rs | 타입 키(dir·file·.ext·L| 포함) 4ms 예산 내 동기 로드 | 6 |
| B12b (M) | G5 누락1 | MED | win.rs | TIMER_ICONS를 무장 1회로(페인트마다 리셋 금지) | 7 |
| B13 | G5-02·G6-07·X3-13 | HIGH/MED | draw.rs·dw.rs·dock.rs·edit.rs | `DrawCtx::char_offsets`(DW는 레이아웃 1개 클러스터 메트릭, 기본=접두 루프), 줄 폭 캐시 | 19 |
| B14a | G5-03·G11-03(근본) | MED/HIGH | fontchain.rs | `offsets`를 런별 `GetTextExtentExPointW` 1회로 O(L) | 6 |
| B14b | G11-03 | HIGH | previewwnd.rs | 라인 오프셋 캐시(LRU)·`max_w` 측정 캡·드래그 무효화 띠 한정 | 13 |
| B15 | G5-07(S) | MED | dw.rs | 이미지 캐시 `Rc<DecodedImage>`·키에 (len, mtime) | 19 |
| B16 | G5-10 | LOW | dw.rs | `term_cell_w` 캐시, set_dpi에서 `layout_count` 리셋 | 19 |
| B18a | G8-01 | HIGH | nexa-ops batch_rename.rs | `conflicts` 정규화 1회 + HashMap 카운트 + 조상 접두 HashSet → O(N·depth) | 9 |
| B18b | G8-12 | MED | batch_rename.rs | `compile_ops` 1회 컴파일 — preview·validate 공유 | 10 |
| B18c | G13-14 | MED | bulkrename.rs | 120ms 디바운스, exists 캐시, 정렬 키 1회 소문자화, 카드 컨트롤 HWND 보관 | 14 |
| B19 | G2-04·G2-13 일부 | MED | win.rs | 진행 통지 `posted` 코얼레싱, `seg_snapshot`·`pct` 통일, `TransferShared::new_arc` 사용 | 11 |
| B20 | G2-05 | LOW | win.rs | 전송 완료 → `reload_both` 경유, 중복 update_status 제거 | 8 |
| B24 | G13-02 | MED | pathinput.rs·win.rs | `split_base_prefix`·`filter_dirs` 분리 + base 캐시 + 80ms 디바운스 | 14 |
| B25a | G10-01 | MED | prefs.rs | ENTERSIZEMOVE~EXITSIZEMOVE 동안 rebuild 보류 + `rebuilding` 가드 | 12 |
| B25b | G10-02 | LOW | win.rs | apply_prefs 말미 직렬화 문자열 비교 후 저장·전창 무효화 | 12 |
| B26a | G12-03·G12-11·X4-02 | HIGH | cloudfs.rs·secret.rs | 연결별 access 토큰 캐시(만료-60s, 401 시 1회 재시도), 회전 refresh 원자 저장(연결 세대 확인) | 11 |
| B26b | G12-02 | MED | cloudfs.rs | `on_prog`는 카운터만 갱신(폴링 200ms가 반영) | 11 |
| B26c | G12-10 | MED | cloudfs.rs | `affected_parents`·`invalidate_paths`(Rename/Move는 하위 접두) | 11 |
| B26d | G12-14 | MED | cloudfs.rs | `expand_tree` 취소·진행·빈 폴더 선생성 제거 | 11 |
| B26e | G12-09 | MED | oauth.rs | WinHTTP 세션·호스트 연결 재사용 + `WinHttpSetTimeouts` | 11 |
| B27 | G12-15 | MED | cloud.rs | `GetLogicalDrives` + REMOTE·CDROM 건너뜀, 우클릭은 스냅숏 재사용 | 8 |
| B28 | G11-06·X2-02④ | MED | preview/wasm.rs | `HostCtx` 파일 1회 open(SHARE_DELETE)·scratch 재사용, 플러그인당 Linker 캐시 | 6 |
| B30 | G11-10 | LOW | archivewnd.rs | 셀 1회 생성 + 정렬 permutation(copy_selection 역매핑) | 13 |
| B32 | G9-04 | LOW | nexa-term·win.rs | 스크롤백 후행 공백 trim + 상한 시 Vec 재사용(배경 선채움 B03 전제) | 19 |
| B-G3-07 | G3-07 | LOW | win.rs | WM_APP_TERM은 `kind==2 && h>0`일 때만 `invalidate_dock` | 6 |
| B-G5-06 | G5-06(최소) | MED | dw.rs | 레이아웃 캐시 히트 시 `SetMaxHeight` | 6 |
| B-G6-09 | G6-09 | LOW | dock.rs·overlaybar.rs | flash/tick이 thumb None 축은 표시 안 함 | 9 |
| B-G8-07 | G8-07 | LOW | nexa-ops lib.rs | 같은 볼륨 이동은 Plan의 `sizes[i]` 보고 | 14 |
| B-G8-09 | G8-09 | MED | shellmenu.rs | `dir_snapshot(cap 4096)` 초과 시 생성 감지 생략 | 19 |
| B-G8-11 | G8-11 | MED | nexa-ops lib.rs | `File::set_modified`로 mtime 보존(속성은 별도 검토) | 19 |
| B-G8-22 | G8-22 | LOW | nexa-ops lib.rs | 복사 버퍼 1회 할당·재사용 | 9 |
| B-X1-11 | X1-11·G7-05·G4-12(드라이브) | HIGH | nexa-vfs lib.rs·main.rs·source.rs | `set_drive_lister` 주입(GetLogicalDrives+GetDriveTypeW), 원격 드라이브 용량 조회 생략 | 8 |
| B-X2-11 | X2-11② | MED | win.rs | 셸 메뉴 `Outcome::Shell`은 `arm_watch_debounce`로 | 14 |
| B-G1-13 | G1-13 | LOW | win.rs | `ensure_dw` FontSpec 구성을 None 분기로 | 20 |
| B-G5-12 | G5-12(예열만) | LOW | main.rs·fontbox | 기동 시 `fontbox::families()` 백그라운드 예열 | 19 |
| B-G11-05q | G11-05(버그 수정분) | MED | previewwnd.rs | 휠을 `WheelAccum`으로 | 13 |

#### 2-B-1. 측정 계획

계측 수단 세 가지를 조합한다.

1. **ui-qa.ps1 시나리오**: PostMessage 조작 + PrintWindow 캡처. 배치 0(T0-5)에서 `Measure-CpuDelta { ... }`, `Get-StartupTrace`, `Invoke-QaScenario -Name`, `Compare-Capture -Rect`, `Send-Text`를 추가한다.
2. **제목줄 계측**: `first render`·`avg` paint(기존) + 배치 0(T0-4)의 `NEXA_TRACE=1` 기동 체크포인트(`data\trace.txt`: settings·i18n·panels·window·nccreate·dw-init·first-paint 시작/끝, 기준 = `GetProcessTimes` 생성 시각).
3. **Get-NexaStats**: WorkingSet·Private·CPU 누적 차이.

| 시나리오 | 조작 | 지표 | 기준선 | 목표 | 관련 작업 |
| --- | --- | --- | --- | --- | --- |
| P01 기동(로컬) | Start-NexaDev, 홈 48항목 | first render·trace 단계표 | 393 ms | < 150 ms | B05·B11·B12a·B12b |
| P02 기동(OneDrive 탭) | 세션에 OneDrive 탭 2개 | first render·avg | 1030 ms·242 ms | 창 출현 비블로킹 | B05·B-X1-11·BL-2 |
| P03 클릭 10회 | 행 선택 이동 | CPU 차/10 | 15~17 ms | ≤ 5 ms | B04·B01·B08b |
| P04 ↓키 30회(도크 Preview) | .md 50개·큰 zip | CPU 차 | 측정 | 전/후 50% 이상 감소 | B04·B28·A15 |
| P05 Shift 단독 30회 | 수식키만 | CPU 차 | 측정 | ≈ 0 | B04 |
| P06 터미널 포커스 유휴 | 터미널 클릭→다른 창 활성 70 s | WS·CPU | 측정 | 트림 유지·CPU 0 | B02·B03 |
| P07 최소화 65 s | SC_MINIMIZE | CPU 차 | 2틱 증가 | 0.00 | A-IsIconic |
| P08 활성화 복귀 10회 | notepad↔AppActivate | CPU 차·복귀 첫 프레임 | 측정 | 재로드 1회/복귀 | B05 |
| P09 FSPOLL 60 s | 트리 보기·폴더 40개 펼침·UNC 1개 | CPU 차·클릭 응답 | 측정 | 원격 스윕 0 | B06·A40 |
| P10 열 드래그·가로 스크롤 | 헤더 경계 Drag 50px·Shift+휠 | avg paint | 6.7~9.0 ms | ≤ 4 ms | B01·B-G5-06·B13 |
| P11 F3 긴 줄 | 16KB 한 줄 파일 F3 | 창 첫 캡처까지 | 측정 | < 100 ms | B14a·B14b |
| P12 일괄 이름변경 | 3000파일 Find 10자 | 키 간 캡처 간격 | 측정 | 키당 < 30 ms | B18a·B18b·B18c |
| P13 클라우드 진입 | 하위 폴더 2곳 연속 | 진입~목록 시간 | 2 RTT | 1 RTT | B26a·B26e |
| P14 소파일 전송 | 2000개 직접 선택 복사 | 메인 창 WM_NULL 왕복 p95 | 측정 | < 50 ms | B19 |
| P15 우클릭 | 셸 메뉴 ESC 후 | 재열거 여부·CPU | 측정 | 재열거 0 | B-X2-11·A20 |

측정 결과는 배치 종료 시 `docs/audit/20261002-ultracode/perf-<배치>.md`에 기준선 표 형식으로 남긴다(작성 규칙: 수치는 ±20% 변동이므로 5회 평균).

### 2-C. 중복 제거 / 모듈성

| 계획 ID | 원 발견 | 파일 | 요지 | 배치 |
| --- | --- | --- | --- | --- |
| C01 | G1-07·G2-12 | win.rs | `ViewFlags`·`rebuild_toolbar`·`rebuild_menus`, 인라인 `config::save` 5곳 → `persist_settings` | 16 |
| C02 | G1-08 | win.rs | `paste_into`(Move→clear 순서 유지) 4곳 치환 | 15 |
| C03 | G1-09·G1 누락2 | panel.rs·win.rs | `Panel::caret_path`, 선택 0이면 표시 순서 순회 생략 | 15 |
| C05 | G1-12·G1-16 | win.rs → win/layout.rs | `compute_layout(&LayoutInput)` + `panel_at_x` 순수 함수 | 18 |
| C06 | G3-13·G3-20·X2-05·X2-14·X3-08 | win.rs·route.rs | `hit_zone`·`splitter_hit` 순수 함수(싱글 패널 ↔ 커서 해소) | 18 |
| C08 | G6-11·G6 누락2 | nexa-gui draw.rs·widgets/* | `text_y`·`stroke_rect_1px`·`paint_hud`·`paint_band`, 공용 `testing::Probe`, HUD 라벨 i18n 주입 | 15 |
| C10 | G8-13·X4-07 | clipboard.rs | `put_unicode_text` 추출, `write_text_rich`는 위임(+to_rtf_mono 테스트, X4-08 대체) | 15 |
| C-G8-14 | G8-14 | clipboard.rs | `get_contents`·`copy_stream`만 공용화(동기 경로 마샬 금지) | 16 |
| C-G13-11 | G13-11 | ctl/*·dialog.rs·about.rs | font_height·text_width·notify 공용 사용, `make_font_ex`·`measure` 통합 | 16 |
| C13 | G4-09·X1-14 | panel.rs | `Tab::new`(apply_sort_opts·보기 모드·열 출처 포함) — duplicate_tab 결함 수정 동반 | 15 |
| C14 | G2-13 | win.rs | `RecycleRoundTrip`으로 DeleteBatchOp·VPasteOp 통합 | 16 |
| C16 | G7-15 | nexa-core·nexa-tree·source.rs·rar.rs | `nexa_core::ext_of`, `rar_level_name` | 15 |
| D01 | G1-10·G3-14·X1-13·G4-12 | win.rs·panel.rs | `panel_shows_cloud` 삭제, `reopen_cloud_tabs(ctx, inv, Some(idx))`(내 PC는 추가 루트 변경 시만), ShellExecute 블록 → `shell_open` | 15 |
| C-X1-06 | X1-06 | win.rs | `set_probe_baseline(st, i)` 통합(트리 서명 산출은 하지 않음) | 21 |
| C-G4-10 | G4-10·G2-15 일부 | fsprobe.rs·win.rs | `ReloadDebounce`(Start/Extend/Hold, Reload/Defer, reset) | 21 |

모듈성 대형 작업(BL-6·BL-7·BL-8·BL-9·BL-10·BL-11·BL-12)은 7장 백로그로 둔다.

### 2-D. 불용 코드 정리

| 계획 ID | 원 발견 | 파일 | 요지 | 배치 |
| --- | --- | --- | --- | --- |
| T0-1 | X4-12 | svg.rs·preview/mod.rs | clippy `chunks_exact_to_as_chunks` 6건 → `as_chunks` | 0 |
| T0-2 | X4-11·X4 누락2 | main.rs | `mod svg`·`mod fileinfo`에 `cfg_attr(not(windows), allow(dead_code))` | 0 |
| D02 | G13-09 | main.rs·win.rs·ctl/mod.rs | ctldemo `#[cfg(debug_assertions)]` 게이트(삭제는 U8) | 16 |
| D03 | X4-09·G12-21·G10-14 일부 | oauth.rs·preview/*·grid.rs | 사용 중 항목의 낡은 allow 제거, 진짜 미사용(http_get_bytes·pw::len 등) 정리. ImageFit·Icon::Up/Down·selected_rows는 계약 API로 유지 | 16 |
| D06 | X4-10 | nexa-tree·chrome.rs | `Tree::root_count` 삭제, 테스트 전용 pub은 `#[doc(hidden)]`(cfg(test) 금지 — 크레이트 경계) | 18 |
| D07 | G7-14 | nexa-vfs·nexa-core | Provider·CORE_VERSION·`header_encrypted` 삭제, 테스트 전용 함수 정리 | 16 |
| D08 | G11-12 | previewwnd.rs·preview/archive.rs | `PvState.lines` 삭제, archive.rs 거짓 allow 정정 | 16 |
| D10 (M) | X1 누락5 | panel.rs | `viewport_dirs` 낡은 주석 정정 | 21 |
| D11 (M) | G2 누락2(주석) | fileinfo.rs | `catch_unwind` 주석을 panic=abort 사실에 맞게 | 21 |
| D12 | G5-01(최소) | dw.rs·draw.rs | push_clip 계약을 '채움·아이콘만 클립'으로 정정 | 21 |

### 2-E. 테스트 보강 (단위 + 자동 UI)

| 계획 ID | 원 발견 | 대상 | 내용 | 배치 |
| --- | --- | --- | --- | --- |
| T0-3 | X4-11 | scripts/audit.ps1 | T-3 판정에 warnings=0 추가 | 0 |
| T0-5 | — | scripts/ui-qa.ps1 | 측정·회귀 함수(2-B-1) | 0 |
| E02 | X4-15 | nexa-gui testutil.rs(신규) | `ClipProbe`(클립 스택·텍스트 x 기록·균형 검사) | 9 |
| E03 | X4-04 | rows.rs·pathbar.rs·edit.rs | X-58 rename/pathbar 클립보드 API 왕복 | 17 |
| E04 | X4-05 | zip.rs | Zip64(3조합)·AES·NTFS·UT·EOCD64 로케이터 | 17 |
| E05 | G7-13 | nexa-tree | Filter 4조합·collapse_all·loaded_child_count·anchor 해제·select_range 폴백 | 17 |
| E06 | G8-19 | dnd.rs·shellmenu.rs | `decide(...)`·`diff_single` 순수 분리 + 결정표 | 17 |
| E07 | G10-09 | prefs.rs | tree_visible·q_tokens·sanitize 테스트 + `route_command`·`wheel_step` 분리 | 12 |
| E08a | G12-22·G12-12·G12 누락5 | oauth.rs·cloudfs.rs | `finish_response`(상태·error 코드 보존)·`parse_refresh`·Reauth 분류, 401 문자열 매칭 제거 | 18 |
| E08b | G12-23 | cloudfs.rs | `list_request`·`plan_cross_copy`·`rel_dest`·fetch 주입 | 18 |
| E09 | G13-15 | ordereditor.rs | `OrderModel` 분리 + 결정 테스트, fmt_bytes·segment_widths | 17 |
| E10 | G11-15·G11 누락4 | preview/archive.rs·archivewnd.rs | `pick_password`·`next_step`(플러그인+세션 캐시 조합 포함) | 18 |
| E11 | X4-17 | nexa-gui theme.rs | 라이트·다크 대비 ≥ 3.0 | 17 |
| E12 | X4-16 | cutmarks.rs(신규)·source.rs·clipboard.rs | 잘라내기 마크 플랫폼 중립 분리 + is_ghosted MC/DC | 17 |
| E13 | X4-03·X4-14·G2-15·X4 누락4 | win/keymap.rs·win/settings_map.rs | 순수 함수 이동 + VK 표·열 레이아웃 왕복(재배열 후 전부 숨김 케이스) | 18 |
| E15 | X4-06 | nexa-term | SD·RI·RIS·OSC ST·`ESC[65535T` 시간 상한 | 17 |
| E-panel | X4-13 | panel.rs | `rename_expanded` 키 기준 매칭 수정 + 중첩·후행 구분자 테스트 | 18 |
| E-G5-11 | G5-11 | dw.rs·icons.rs | `layout_key`·`glyph_ns`·`emb_cache_key`·`parse_emb_key` 왕복 | 21 |

각 결함 수정 작업은 자체 회귀 테스트를 같은 커밋에 포함한다(3장 '검증' 열). 위 E 항목은 결함 수정과 분리된 순수 보강이다.

#### 2-E-1. ui-qa.ps1 회귀 시나리오 (캡처 비교 항목)

| ID | 시나리오 | 비교 영역 | 관련 |
| --- | --- | --- | --- |
| R01 | 기동(듀얼·도크 Info·라이트/다크) | 전체 프레임 | 전 배치 |
| R02 | 행 클릭 → Ctrl 다중 → 기선택 재클릭 | 상태바·도크 Info 텍스트 | A17·A25·B04 |
| R03 | 더블클릭 폴더 진입 / Alt+← / XBUTTON / 탭 바 빈 곳 더블클릭 | 상태바·경로바·도크·탭 바 | A25 |
| R04 | F2 → 우클릭 2회 메뉴 6항목 → Ctrl+X | 리네임 필드·메뉴 | A26~A28·E03 |
| R05 | 우 패널 가로 스크롤 → 좌 패널 클릭 | 좌 패널 오른쪽 20px 스트립 | A-rowsclip |
| R06 | 도크 Info/Preview/터미널 전환 + 휠·Shift+휠(wrap on/off) | 도크·목록 가로 오프셋 | A33·A34·A38 |
| R07 | 싱글 패널 / 싱글 정보 토글 후 우 패널 클릭·↓ | 툴바·도크·캐럿 | A18·A19·C06 |
| R08 | 터미널 `exit` → 'a'·Delete | 목록 캐럿·파일 존재 | A12 |
| R09 | 드래그 후 ESC → 다른 행 클릭 | 선택 하이라이트 | A17·A20 |
| R10 | 설정 창 열기 → 리사이즈 → Tab 10회 → 플러그인 체크 | 컨트롤 포커스·settings.cfg mtime | A54·B25a·B25b |
| R11 | F3 미리보기·압축 그리드·암호 zip | 창 첫 프레임·그리드 | B14b·A53·A61 |
| R12 | 타일 보기 전환 직후 첫 프레임 | 아이콘 열 | A30·B12a |
| R13 | 경로바 편집 'İ'+'%' 입력 | 창 생존(IsWindow) | A01 |
| R14 | 테마 전환·DPI 150% 대화상자(암호·확인) | 하단 버튼 영역 | A57·A59 |
| R15 | File>Exit 직후 session 파일 | mtime·내용 | A13 |
| R16 | 최소화 65 s·복원 | CPU 차·재로드 횟수(trace) | B05·A-IsIconic |

---

## 3. 구현 배치

### 3-0. 배치 규칙

- **병렬 레인**: 같은 배치 안에서 파일 교집합이 없는 작업. 서로 다른 작업자(서브에이전트)가 동시에 진행할 수 있다.
- **직렬 레인**: 같은 파일(특히 `win.rs`·`rows.rs`·`panel.rs`·`cloudfs.rs`·`prefs.rs`·`nexa-term lib.rs`)을 건드리는 작업. 표기 순서대로 하나씩 커밋한다. 레인 사이 의존은 '선행' 열에 적는다.
- 작업 1개 = 커밋 1개. 커밋 제목은 표의 '커밋' 열(Conventional Commits, 한국어 본문). 커밋 꼬리에 원 발견 ID를 적는다.
- 배치 순서 = 위험 낮고 가치 큰 것부터. 배치 0은 게이트를 정상화해 이후 '신규 경고 0'을 판정 가능하게 만든다.
- 각 배치 끝에서 4장 품질 게이트를 통과해야 다음 배치로 넘어간다. push는 사용자 요청 시에만.
- 편집 후 `rustfmt <편집 파일>`만 실행한다(`cargo fmt -p` 금지).

### 배치 0 — 게이트·계측 정상화 (5)

| ID | 커밋 | 파일 | 변경 요지 | 동작 불변 | 검증 | 규모 |
| --- | --- | --- | --- | --- | --- | --- |
| T0-1 | `chore(lint): clippy 1.98 chunks_exact → as_chunks` | svg.rs·preview/mod.rs | 6곳 `as_chunks::<N>().0` 패턴 바인딩 | 나머지 버림 동일 | `cargo clippy` 0·svg 테스트 11개 | S |
| T0-2 | `chore(build): svg·fileinfo 모듈 비Windows dead_code 허용` | main.rs | 두 `mod`에 cfg_attr | cfg 속성만 | linux check 경고 0 | S |
| T0-3 | `chore(audit): T-3 비Windows 경고도 판정` | scripts/audit.ps1 | `$lwarn -eq 0` 추가·docs/18 §4 문구 | — | `pwsh scripts/audit.ps1 -Quick` PASS | S |
| T0-4 | `perf(diag): NEXA_TRACE 기동 체크포인트` | win.rs (직렬) | 단계별 `now_ms` 기록, first render 기준 = 프로세스 생성 시각 | 기본 비활성 | trace.txt 생성·제목줄 수치 표기 | S |
| T0-5 | `test(ui-qa): 측정·회귀 함수 추가` | scripts/ui-qa.ps1 | 2-B-1 함수 + R01~R16 골격 | — | P01·P03 기준선 재측정 기록 | S |

### 배치 1 — panic·즉사 제거 (8)

| ID | 커밋 | 파일 | 변경 요지 | 동작 불변 | 검증 | 규모 |
| --- | --- | --- | --- | --- | --- | --- |
| A01 | `fix(pathinput): 환경변수 확장 문자 경계 panic` | pathinput.rs | `find_ascii_ci`, `lower` 제거 | ASCII 토큰 매칭 의미 동일 | 'İ%한%'·'K%TEMP%'·'KK%TEMP%' 단위, R13 | S |
| A02 | `fix(vfs): 압축 항목명 드라이브 판정 문자 경계 panic` | nexa-vfs archive/mod.rs | `as_bytes().get(..3)` 비교 | suspicious 플래그 동일 | 'a한.txt'·'C:/한' 단위 + zip 포맷 회귀 1건 | S |
| A03 | `fix(vfs): tar PAX 레코드 길이 검증` | archive/tar.rs | `&[u8]` 파싱·`len <= sp+1` break | 정상 레코드 결과 동일 | `b"1 x=y\n"`·'6 p=한' 패닉 없음 | S |
| A04 | `fix(vfs): rar5 확장영역 순회 전진 보장` | archive/rar.rs | `checked_add` 실패 시 false | 정상 판정 동일 | size=u64::MAX-n1 즉시 반환 | S |
| A07 | `fix(term): ConPTY UTF-8 불량 바이트 시 출력 정지` | conpty.rs | `Utf8Chunker` 순수 타입, 읽기 스레드는 호출만 | 유효 입력 출력 동일 | `[0xFF,'a']`·1바이트 분할 '한'·랜덤 100KB 후 pending<4 | S |
| A08 | `fix(term): SU/SD 반복 수를 영역 높이로 제한` | nexa-term lib.rs | `scroll_up/down` 선두 클램프 | n ≤ 높이인 호출 결과 동일 | `\x1b[65535S` 피크 상한·마진 밖 불변 | S |
| A11 | `fix(secret): 손상 토큰 hex 파일 panic` | secret.rs | `decode_hex`(ASCII·짝수) | 정상 왕복 동일 | 'a\u{e9}a'·BOM·'00ff' | S |
| A12 | `fix(input): 터미널 포커스 중 키가 파일 목록으로 누수` | win.rs (직렬) | `term_focus_live`, KEYDOWN·CHAR 항상 소비, term_paint 실패 시 term_focus 해제 | 살아 있는 터미널 입력·재시작 키 동일 | `route_key_with_term` MC/DC 4쌍, R08 | S |

### 배치 2 — 무한 루프·데이터 손실 (8)

| ID | 커밋 | 파일 | 변경 요지 | 동작 불변 | 검증 | 규모 |
| --- | --- | --- | --- | --- | --- | --- |
| A05 | `fix(vfs): 조작된 tar 크기로 인한 무한 루프` | archive/tar.rs (A03 후) | checked 오프셋·break | 정상 tar 목록 동일 | base-256 'L' 헤더 픽스처 즉시 반환 | S |
| A06a | `fix(vfs): zip·rar·cab 오프셋 checked와 CAB 상한 표시` | zip.rs·rar.rs·cab.rs | checked_add, CAB `len() > limit` 선두 break | 정상 결과 동일 | CAB limit 2/3파일 → truncated | S |
| A14 | `fix(ops): 교차 볼륨 폴더 이동이 열거 오류 항목을 삭제` | nexa-ops lib.rs | 엔트리 Err 전파·수집, 복사 완료 목록만 삭제 후 `remove_dir` | 정상 이동 결과 동일 | 권한 거부 파일 포함 폴더 이동 → 원본 잔존. **선행: 재현 확인** | M |
| A15 | `fix(plugin): read_at 고정 연료 과금으로 대형 ar·cpio 실패` | preview/wasm.rs | 고정분 축소·바이트 비례, 호출당 누적 바이트 상한 | 폭주 방지 유지(벽시계 1.5 s) | ar 2500멤버 합성 → Ok·entries 2500 | S |
| A16 | `fix(dnd): 지연 렌더링 확보를 임시 폴더로 한정` | dnd.rs | temp_dir 접두 가드·`is_virtual` 재판정 | 7-Zip·압축 폴더 확보 유지 | 실 폴더 경로 → stolen 비어 있음 | S |
| A09 | `fix(term): 문자셋 지정자·DCS 페이로드가 화면에 찍힘` | nexa-term lib.rs (A08 후) | `S::Charset`·`S::Str` | 기존 17 테스트 | `"a\x1b(Bb"`→"ab"·DCS 폐기 | S |
| A13 | `fix(app): 메뉴 종료가 세션 저장을 건너뜀` | win.rs (직렬 1) | `CMD_EXIT` → WM_CLOSE | 창 닫기 경로와 동일 | R15 | S |
| A16b | `fix(dnd): 확보 드롭의 Ctrl+Z가 파일을 임시 폴더로 이동` | win.rs (직렬 2) | 스테이징 src 쌍은 VPasteOp, 빈 슬롯 remove_dir | 일반 이동 undo 동일 | 7-Zip 드롭 → Ctrl+Z → 휴지통(수동 QA) | S |

### 배치 3 — 포인터·포커스 상태 결함 (8)

| ID | 커밋 | 파일 | 변경 요지 | 동작 불변 | 검증 | 규모 |
| --- | --- | --- | --- | --- | --- | --- |
| A17 | `fix(rows): OLE 드래그 뒤 낡은 press_pending 선택 붕괴` | rows.rs | MouseDown·RightDown 선두 리셋, `abort_press` 공개, replace_source 보조 리셋 | 다중 선택 드래그 후 선택 유지 | `press_pending_does_not_survive_replace_source` | S |
| A22 | `fix(dock): 팝아웃 버튼 오른쪽 절반이 스크롤바에 삼켜짐` | dock.rs | popout 판정 선행 | 바 드래그 동작 동일 | `popout_click_wins_over_flashed_bar` | S |
| A23 | `fix(clipboard): 클립보드 열기 경합 재시도` | clipboard.rs | `Open::new` 5×10ms | 성공 경로 동일 | 점유 스레드 #[ignore] 테스트 | S |
| A24 | `fix(shellmenu): 메뉴 닫힌 뒤 메시지 포워딩 해제` | shellmenu.rs | TrackPopupMenuEx 직후 `ACTIVE` 비움 | 메뉴 표시 중 포워딩 동일 | 연결 프로그램 대화상자 우클릭 수동 | S |
| A18 | `fix(dock): 숨은 0-rect 도크 판정 통일` | panel.rs·win.rs (직렬 1) | `dock_shown()` 공개, win.rs 판정 치환(이미 h>0 있는 곳 제외) | 듀얼 정보·싱글 패널 결과 동일 | dock_shown 표·R07 | S |
| A19 | `fix(dock): 공유 도크 클릭이 활성 패널을 뒤집음` | win.rs (직렬 2) | Info/Preview 공유 도크는 활성 유지, 터미널은 좌 세션 규약 유지, DBLCLK `panel_at_pt` | 듀얼 도크 결과 동일 | `panel_at_pt_impl` 표·R07 | M |
| A20 | `fix(dnd): 드래그 반환 후 위젯 프레스 취소·취소 시 재로드 생략` | win.rs (직렬 3, 선행 A17) | `abort_press`, `begin_drag`=false면 재로드 안 함 | 드롭 성공 반영 동일 | R09·P15 | S |
| A21 | `fix(input): 캡처 상실·TUI 우클릭 잔여 상태 정리` | win.rs (직렬 4) | SetCapture/ReleaseCapture, 버튼별 held, `reset_mouse_transients`, RBUTTONUP 쌍, `drag_press` 초기화 | 정상 드래그·우클릭 동일 | `tui_btn_held` 4조합, WM_CANCELMODE 시나리오 | M |

### 배치 4 — 탐색 길목·편집 상태 (8)

| ID | 커밋 | 파일 | 변경 요지 | 동작 불변 | 검증 | 규모 |
| --- | --- | --- | --- | --- | --- | --- |
| A29 | `fix(rows): 이름변경 필드를 트리 열 위치에` | rows.rs | `tree_col()` 공용 | 기본 열 순서 좌표 동일 | 열 재배열 후 rename rect 단언 | S |
| A30 | `fix(icons): 타일 키 per_file 판정` | icons.rs | `is_per_file` | 접두 없는 키 동일 | 6케이스·R12 | S |
| A31 | `fix(vfs): 폴더 링크·정션을 폴더로 열거` | nexa-vfs lib.rs | Dir 우선, 링크 비트 별도 | 비링크 항목 동일 | cfg(windows) 정션 테스트·숨김 시스템 정션 QA | S |
| A32 | `fix(secret): 토큰 저장 원자화·clear_from 정리` | secret.rs | `config::save`, allow 제거 | 왕복 동일 | token_roundtrip_and_clear | S |
| A25 | `fix(nav): 모든 진입 경로가 상태 동기 길목을 지나도록` | win.rs (직렬 1) | `finish_input`·`after_navigation`, 도크 이중 호출 제거 | Enter 경로 결과 기준 | R02·R03, 진입 직후 외부 파일 생성 반영 | S |
| A26 | `fix(rename): 지연 리네임이 다른 행·패널에서 시작` | win.rs (직렬 2) | `pending_rename` 대조 | 정상 느린 재클릭 동일 | `rename_timer_should_fire` 표 | S |
| A27 | `fix(rename): 필드 안 더블클릭이 폴더 진입` | win.rs (직렬 3) | is_renaming && field_hit면 return | 다른 행 더블클릭 동일 | 필드 중앙 DBLCLK 캡처 | S |
| A28 | `fix(edit): 터미널 대상 편집 메뉴가 경로바·리네임에 작용` | win.rs (직렬 4) | Term 확정·터미널 클릭 시 cancel_edit·cancel_rename | 키보드 Ctrl+C/V 순서 동일 | R04·경로바 편집 중 터미널 Paste | S |

### 배치 5 — 터미널·휠 (8)

| ID | 커밋 | 파일 | 변경 요지 | 동작 불변 | 검증 | 규모 |
| --- | --- | --- | --- | --- | --- | --- |
| A36 | `refactor(conpty): 기동 실패 경로 RAII` | conpty.rs | HandleGuard·AttrList | 성공 경로 핸들 집합 동일 | 실패 100회 후 핸들 수 | S |
| A10 | `fix(term): CSI 중 ESC·C0·부파라미터·private 마커` | nexa-term lib.rs | 선두 분기·pars 상한·'>' '=' '<' dispatch 생략(':'는 별도 목록) | 기존 테스트 | ESC 재진입·`ESC[>4;1m` 무영향 | S |
| A37 | `fix(rows): 펼침 삼각형 우클릭도 그 항목 선택` | rows.rs | 마커 존 선택 규약 | 일반 행 동일 | RightDown 마커 존 테스트 | S |
| A38 | `fix(dock): 세로 스크롤 후 가로 오프셋 재클램프` | dock.rs | `scroll_to` 끝 클램프 | 상한 내 동일 | 긴 줄→짧은 줄 테스트 | S |
| B02 | `perf(term): 비활성·최소화·트림 중 캐럿 타이머 정지` | win.rs (직렬 1) | ACTIVATEAPP·SIZE_MINIMIZED·trim에서 Kill, 재무장 단일 헬퍼 | 포커스 복귀 시 깜빡임 재개 | `caret_timer_should_run` 표·P06 | S |
| A33 | `fix(wheel): 터미널 위 휠이 목록으로 새는 조합` | win.rs·route.rs(신규) (직렬 2) | `wheel_route` 순수 함수 | 비줄바꿈 Shift+휠·TUI 동일 | 32조합 표·R06 | S |
| A34 | `fix(term): 정밀 터치패드 휠 누적` | win.rs (직렬 3) | TermState `WheelAccum`, TUI 경로도 누적 | ±120 결과 동일 | delta 30×4 → 3줄 | S |
| A35 | `fix(term): 가로 스크롤 시 마우스 모드 좌표 오류` | win.rs (직렬 4) | `term_cell_of`(view_x 반영) 공용 | 스크롤 0 결과 동일 | 경계 셀 테스트 | S |

### 배치 6 — 미리보기·도크 지연 (8)

| ID | 커밋 | 파일 | 변경 요지 | 동작 불변 | 검증 | 규모 |
| --- | --- | --- | --- | --- | --- | --- |
| B28 | `perf(plugin): 호스트 파일 핸들·링커 재사용` | preview/wasm.rs (A15 후) | HostCtx 지연 open·scratch, Linker 캐시 | 같은 바이트·크기 | open 카운터 1 | S |
| A61 | `fix(preview): ESC 닫기를 차용 밖으로` | previewwnd.rs | PostMessage(WM_CLOSE) | 닫힘 동작 동일 | 드래그 중 ESC 수동 | S |
| B14a | `perf(font): 폴백 체인 오프셋 O(L)` | fontchain.rs | 런별 GetTextExtentExPointW | 누적 폭 동일 | 구현 대조 cfg(windows) 테스트 | S |
| B-G5-06 | `fix(dw): 부분 높이 행 레이아웃 캐시 세로 위치` | dw.rs | 히트 시 SetMaxHeight | 같은 높이 픽셀 동일 | 도크 휠 1칸 캡처 기준선 | S |
| B12a | `perf(icons): 타입 아이콘 예산 내 동기 로드` | icons.rs | 4ms 예산 | per_file 경로 동일 | `take_batch_typed` 테스트·R12 | S |
| B04 | `perf(dock): 같은 대상이면 미리보기·Info 재생성 생략` | win.rs (직렬 1) | 2-B 요지 | 표시 내용 동일, 암호 재시도 경로 우회 | 키 동치 테스트·P04·P05·암호 zip 재시도 | M |
| A67 | `fix(session): 실패 탭 제거 후 탭별 플래그 재매핑` | panel.rs·win.rs (직렬 2) | `restore` → `(Panel, kept)` | 모든 탭 성공 시 동일 | [없음,A,B] locked·active 테스트 | S |
| B-G3-07 | `perf(term): 다른 종류 도크일 때 출력 무효화 생략` | win.rs (직렬 3) | kind==2 && h>0 조건 | 터미널 표시 시 동일 | `term_notify_needs_paint` 표 | S |

### 배치 7 — 기동·폴링·최소화 (8)

| ID | 커밋 | 파일 | 변경 요지 | 동작 불변 | 검증 | 규모 |
| --- | --- | --- | --- | --- | --- | --- |
| A39 | `fix(panel): 끊긴 UNC 조상 순회 상한` | panel.rs | `nearest_existing_ancestor`(주입 exists) | 로컬 최근접 조상 이동 동일 | 호출 경로 기록 테스트 | S |
| B11 | `perf(rows): 평면·타일 보기에서 빈 폴더 프로브 생략` | rows.rs·source.rs | 마커 필요 플래그 | 트리 보기 글리프 동일 | 프로브 카운터 0 | S |
| B08a | `perf(tree): VisibleRowRef 필드 확장` | nexa-tree | id·depth·attrs·expanded·has_children | row 값 동일 | row_ref_matches_row 확장 | S |
| A42 | `fix(app): 매체 없는 드라이브 시스템 대화상자 억제` | main.rs | SetErrorMode | — | 빈 카드리더 내 PC 진입 수동 | S |
| B05 | `perf(startup): 활성화·복원 재로드 코얼레싱` | win.rs (직렬 1) | 30ms 타이머, 최초 표시 전 생략, `minimized` 플래그 | 복귀 시 변경 반영 | `should_refresh_on_activate` 표·P01·P08·R16 | M |
| A-IsIconic | `fix(poll): 최소화 중 폴링 재무장` | win.rs (직렬 2) | `!IsIconic` 가드 | 비활성 30 s 폴링 동일 | `fspoll_interval` 표·P07 | S |
| B12b | `perf(icons): 아이콘 틱 타이머 리셋 기아` | win.rs (직렬 3) | armed 플래그 | 로딩 결과 동일 | 30ms 휠 20회 중 캡처 | S |
| A40 | `fix(watch): 감시 등록 실패 백오프` | win.rs (직렬 4) | 실패 메모·지수 백오프·경로 변경 리셋 | 정상 경로 감시 동일 | 끊긴 매핑 드라이브 클릭 응답 | S |

### 배치 8 — 감시·열거 비용 (8)

| ID | 커밋 | 파일 | 변경 요지 | 동작 불변 | 검증 | 규모 |
| --- | --- | --- | --- | --- | --- | --- |
| B08b | `perf(panel): 감시 목록 계산의 행 이름 클론 제거` | panel.rs (B08a 후) | visible_id·is_expanded·node_path, Flat 조기 반환 | 목록·순서 동일 | 기존 viewport_dirs 테스트 | S |
| B09 | `perf(tree): 경로 정규화 단일화와 일괄 인덱스 조회` | nexa-tree | `path_key`·`indices_of_paths` | ASCII·동일 구분자 결과 동일 | path_index vs index_of_path 일치 | M |
| A41 | `fix(watch): 감시 스레드 생성 실패 처리` | watcher.rs | Builder·WatchFail | 통지 규약 동일 | 파일 경로 start → Err | S |
| B-X1-11 | `perf(vfs): 내 PC 드라이브 목록 주입` | nexa-vfs lib.rs·main.rs·source.rs | `set_drive_lister`, 원격 용량 생략 | 로컬 드라이브 표시 동일 | 주입·미주입 테스트 | M |
| B27 | `perf(cloud): 구글 드라이브 탐지 드라이브 순회 축소` | cloud.rs | GetLogicalDrives·REMOTE·CDROM 생략 | DriveFS 탐지 유지 | `is_probe_candidate` 표 | S |
| B06 | `perf(poll): 원격 경로 프로브 스윕 제외` | win.rs·fsprobe.rs·source.rs (직렬 1) | probe_skip_slow 공용화, 스윕·기준선에 적용 | 로컬 감지 규칙 동일 | P09 | S |
| B07 | `perf(watch): 셸 통지 경로 필터` | shellnotify.rs·win.rs (직렬 2) | `hits_watched` | 해석 실패 시 발화 | 4케이스 단위·Temp 루프 60 s 재로드 0 | M |
| B20 | `perf(ops): 전송 완료 후 중복 재로드 제거` | win.rs (직렬 3) | reload_both 경유 | 완료 반영 동일 | 디버그 reopen 카운터 1 | S |

### 배치 9 — 렌더링 핫패스 (8)

| ID | 커밋 | 파일 | 변경 요지 | 동작 불변 | 검증 | 규모 |
| --- | --- | --- | --- | --- | --- | --- |
| E02 | `test(gui): ClipProbe 테스트 도구` | nexa-gui testutil.rs·lib.rs (직렬 1) | 클립 스택·텍스트 기록 | 테스트 전용 | 자체 테스트 | S |
| A-rowsclip | `fix(rows): 가로 스크롤 시 패널 경계 밖 그리기` | rows.rs·draw.rs (직렬 2) | push/pop_clip, `clip_text_left` | bounds 안 픽셀 동일 | `hscrolled_cells_never_paint_left_of_bounds`·R05 | M |
| B-G6-09 | `perf(dock): 스크롤 불가 축은 바 표시 안 함` | dock.rs·overlaybar.rs | thumb None 축 flash 생략 | 넘칠 때 표시 동일 | 2줄 → tick 없음 | S |
| B-G8-22 | `perf(ops): 파일 복사 버퍼 재사용` | nexa-ops lib.rs | 버퍼 전달(pub 래퍼 유지) | 결과 동일 | 기존 테스트·5만 파일 측정 | S |
| B18a | `perf(rename): 일괄 이름변경 충돌 검출 O(N)` | batch_rename.rs | 2-B 요지 | 결과 동일 | 기존 2종 + N=5000 상한 | M |
| B01 | `perf(paint): rcPaint 기반 부분 재도장` | win.rs (직렬 1) | 2-B 요지 | 클립 밖 직전 프레임 유지 | `paint_into` Probe 테스트·R01~R07 전체 캡처 | M |
| B03 | `perf(term): 런 단위 글리프와 캐럿 셀 무효화` | win.rs·dw.rs (직렬 2, B01 후) | `term_glyph`, 캐럿 rect, 배경 선채움 | 셀 x 배치 유지(07-14 회귀 방지) | Probe 호출 수·P06 | M |
| A69 | `fix(startup): 명령줄 경로 기동 시 숨김 설정 무시` | win.rs·panel.rs (직렬 3) | `open_start_tree(path, ctx)` | 세션 복원 경로 동일 | `new_with_hidden_off_has_consistent_enum_filters` | S |

### 배치 10 — 클라우드 정확성 (8)

| ID | 커밋 | 파일 | 변경 요지 | 동작 불변 | 검증 | 규모 |
| --- | --- | --- | --- | --- | --- | --- |
| A44 | `fix(cloud): 500건 초과 목록 페이지 넘김` | cloudfs.rs (직렬 1) | `fetch_objects` 커서 루프 | 1페이지 폴더 동일 | 2페이지 픽스처·600파일 실기 | M |
| A45 | `fix(cloud): 폴더 보장 시 중복 생성과 오류 삼킴` | cloudfs.rs (직렬 2) | Option 반환·autorename false·conflict만 Ok | 정상 생성 동일 | `is_already_exists` 표 | S |
| A46 | `fix(cloud): 임시 파일·스테이징 경로 충돌` | cloudfs.rs (직렬 3) | 덧붙임 + 실행별 하위 폴더 | 결과 파일 동일 | `part_path` 테스트 | S |
| A47 | `fix(oauth): 긴 사전 인증 URL 버퍼` | oauth.rs (직렬 1) | 길이 조회 후 재호출 | 짧은 URL 동일 | 비즈니스 OneDrive 수동 | S |
| A48 | `fix(oauth): 무관 연결이 인증 리스너를 닫음` | oauth.rs (직렬 2) | `classify_redirect` | 정상 code 경로 동일 | 4분기 단위 | S |
| A43 | `fix(cloud): 클라우드 작업 동시 실행 게이트` | win.rs | `cloud_slot_busy` | 단일 작업 동일 | 표 테스트·두 번째 드롭 거부 | S |
| B18b | `perf(rename): 정규식 1회 컴파일` | batch_rename.rs (B18a 후) | `compile_ops` | API 동일 | N=5000 정규식 2블록 상한 | S |
| A49 | `fix(ops): 폴더 복사 하위 실패 격리와 취소 정리` | nexa-ops lib.rs | errors 수집·신규 폴더 정리 | 성공 경로 동일 | 잠금 파일·취소 3케이스 | M |

### 배치 11 — 클라우드 성능·전송 (8)

| ID | 커밋 | 파일 | 변경 요지 | 동작 불변 | 검증 | 규모 |
| --- | --- | --- | --- | --- | --- | --- |
| B26a | `perf(cloud): 액세스 토큰 캐시와 회전 토큰 저장` | cloudfs.rs·secret.rs (직렬 1) | 2-B 요지, invalidate_all에서 비움 | 오류 문구 경로 동일 | `cache_decision`·`merge_refresh` 표·P13 | M |
| B26b | `perf(cloud): 다운로드 진행 통지 폭주 제거` | cloudfs.rs (직렬 2) | 카운터만 갱신 | 진행 표시 ≤200ms | `should_post` 경계 | S |
| B26c | `perf(cloud): 쓰기 후 영향 폴더만 무효화` | cloudfs.rs (직렬 3) | affected_parents | stale 시 F5 복구 | 7 variant 테스트 | S |
| B26d | `fix(cloud): 폴더 다운로드 전개 중 취소` | cloudfs.rs (직렬 4) | cancel·진행·폴더 선생성 제거 | 결과 트리 동일 | lister 주입 테스트 | S |
| B26e | `perf(oauth): WinHTTP 세션 재사용과 타임아웃` | oauth.rs | 세션·연결 캐시 | 요청 핸들 수명 동일 | 100MB 업로드 시간 | M |
| B19 | `perf(transfer): 진행 통지 코얼레싱과 스냅숏 통일` | win.rs | posted 플래그·seg_snapshot·pct | 종결 통지 동일 | seg_snapshot MC/DC·P14 | M |
| A52 | `fix(ops): 마운트 폴더 이동 시 장치 불일치 폴백` | nexa-ops lib.rs | rename_or_cross | 같은 볼륨 경로 동일 | mountvol #[ignore] 왕복 | S |
| A53 | `fix(plugin): 서킷 브레이커 실패 분류` | preview/wasm.rs | 2-A 요지 | 진짜 ABI 결함은 즉시 격리 | 시간 초과 5회 → 미격리 등 4케이스 | M |

### 배치 12 — 설정·대화상자 (8)

| ID | 커밋 | 파일 | 변경 요지 | 동작 불변 | 검증 | 규모 |
| --- | --- | --- | --- | --- | --- | --- |
| A54 | `fix(prefs): 플러그인 체크박스 즉시 적용` | prefs.rs (직렬 1) | 범위 가드 + 전용 아암 | 라디오 동작 동일 | R10 | S |
| A55 | `fix(prefs): 범위 밖 글꼴 크기로 백엔드 반복 재생성` | prefs.rs·config.rs (직렬 2) | sanitize 클램프·상수 공유 | 범위 내 동일 | `font_slots_changed` MC/DC 4쌍 | S |
| B25a | `perf(prefs): 크기 조절 중 컨트롤 재생성 보류` | prefs.rs (직렬 3) | ENTER/EXITSIZEMOVE·rebuilding 가드 | 실제 변경 즉시 적용 유지 | 리사이즈 중 포커스 유지 | M |
| E07 | `test(prefs): 트리·검색·정규화·명령 라우팅` | prefs.rs (직렬 4) | `route_command`·`wheel_step` 분리 + 테스트 | 동작 불변 | id×notify 전수 | M |
| B25b | `perf(prefs): 변경 없는 적용에서 저장 생략` | win.rs | 직렬화 문자열 비교 | 변경 시 저장 동일 | Tab 10회 mtime 불변 | S |
| A56 | `fix(config): 설정 파일 BOM과 공백` | config.rs | 공통 split_kv | 왕복 동일 | BOM·`show_hidden= 0` 테스트 | S |
| A57 | `fix(dialog): 확인 창 키보드 조작` | dialog.rs | 포커스·Esc 선처리·outer_size | 마우스 결과 동일 | Esc→0·Enter→첫 버튼 | S |
| A58 | `fix(tip): 툴팁 모니터 경계` | tip.rs | rcWork 클램프 | 주 모니터 안 동일 | 음수 좌표 모니터 수동 | S |

### 배치 13 — 미리보기 창·압축·터미널 기능 (8)

| ID | 커밋 | 파일 | 변경 요지 | 동작 불변 | 검증 | 규모 |
| --- | --- | --- | --- | --- | --- | --- |
| B-G11-05q | `fix(preview): 미리보기 창 트랙패드 휠` | previewwnd.rs (직렬 1) | WheelAccum | ±120 동일 | delta 30×4 캡처 | S |
| B14b | `perf(preview): 미리보기 창 오프셋 캐시·폭 측정 상한` | previewwnd.rs (직렬 2, B14a 후) | LRU 캐시·max_w 캡·무효화 띠 | 히트 결과 동일 | P11 | M |
| A59 | `fix(dialog): 암호 창 고DPI 하단 잘림` | pwprompt.rs (A57 후) | outer_size | 96dpi 크기 동일 | R14 150% | S |
| B30 | `perf(archive): 압축 그리드 정렬 시 셀 재생성 제거` | archivewnd.rs | permutation | 표시·정렬 동일 | 기존 3테스트·5만 벤치 | S |
| A62 | `fix(preview): 다이어그램 임시 파일 원자 쓰기` | preview/mod.rs (직렬 1) | tmp→rename·크기 검증 | 성공 경로 동일 | 잘린 파일 재생성 | S |
| A63 | `fix(preview): 짧은 읽기·긴 암호 처리` | preview/mod.rs·wasm.rs (직렬 2) | read_to_end·음수 통보 | 정상 결과 동일 | 1KB 가짜 Read·cap 4 암호 | S |
| A65a | `feat(term): 커서 숨김·괄호 붙여넣기 모드 추적` | nexa-term lib.rs | cursor_visible·bracketed_paste | 모드 미사용 스트림 동일 | DECSET 표 | S |
| A65b | `feat(term): 숨긴 커서 미표시·괄호 붙여넣기 전송` | win.rs (A65a 후) | 캐럿 조건 AND, term_paste 감싸기 | cmd 동작 동일 | WSL vim 수동 | S |

### 배치 14 — 입력 잔여·지연 2차 (8)

| ID | 커밋 | 파일 | 변경 요지 | 동작 불변 | 검증 | 규모 |
| --- | --- | --- | --- | --- | --- | --- |
| A-G6-06 | `fix(rows): 스크롤바 프레스를 행 프레스로 오판` | rows.rs | `bar_hit` | 바 없을 때 동일 | `row_at_is_none_over_visible_thumb` | S |
| A60 | `fix(ctl): 스핀·세그먼트 경계 산술` | spin.rs·segmented.rs | saturating·빈 목록 | 범위 내 동일 | 최댓값 주입 수동 | S |
| B18c | `perf(rename): 일괄 이름변경 입력 디바운스·존재 캐시` | bulkrename.rs | 2-B 요지 | 적용 결과 동일 | P12 | M |
| B-G8-07 | `perf(ops): 같은 볼륨 이동의 크기 재열거 제거` | nexa-ops lib.rs | sizes[i] 보고 | done_bytes 합 동일 | Bytes 이벤트 1회 | S |
| B24 | `perf(pathbar): 자동완성 기준 폴더 캐시·디바운스` | pathinput.rs·win.rs (직렬 1) | 2-B 요지 | 제안 목록 동일 | 분해 함수 단위·UNC 타이핑 | M |
| A70 | `fix(term): 셸 기동 실패 시 페인트마다 재시도` | win.rs (직렬 2, A36 후) | 실패 메모 | 성공 경로 동일 | 없는 폴더 루트 conhost 수 | S |
| A66 | `fix(rename): 재열람이 이름변경 중 행을 바꿈` | win.rs·panel.rs (직렬 3) | 편집 중 보류 가드 | 비편집 재열람 동일 | F2→외부 생성→복귀→Enter | S |
| B-X2-11 | `perf(shellmenu): 셸 동사 후 즉시 재열거 대신 디바운스` | win.rs (직렬 4) | arm_watch_debounce | 변경 반영 ≤300ms | P15 | S |

### 배치 15 — 중복 제거 1 (8)

| ID | 커밋 | 파일 | 변경 요지 | 동작 불변 | 검증 | 규모 |
| --- | --- | --- | --- | --- | --- | --- |
| D01 | `refactor(cloud): 결과 버리던 판정 제거와 재열기 범위 지정` | win.rs·panel.rs (직렬 1) | 2-C 요지 | 08-01 QA(연결 루트 표시) 유지 | 클라우드 2패널 캡처·내 PC 소스 포인터 불변 | S |
| C13 | `refactor(panel): Tab 생성 공용화와 탭 복제 정렬 계승` | panel.rs (직렬 2) | Tab::new | 4경로 초기값 동일(복제 결함만 수정) | `duplicate_tab_inherits_sort_and_view` | S |
| C03 | `refactor(panel): 캐럿 경로 공용·우클릭 순회 단락` | panel.rs·win.rs (직렬 3) | caret_path, selection 0 단락 | 대상 동일 | 캐럿 없음/있음 테스트 | S |
| C02 | `refactor(clipboard): 붙여넣기 분기 통합` | win.rs (직렬 4) | paste_into | Move→clear 순서 | `paste_plan` MC/DC 3쌍 | S |
| C10 | `refactor(clipboard): 유니코드 텍스트 게시 공용화` | clipboard.rs (직렬 1) | put_unicode_text | 반환값·순서 동일 | rich 게시 #[ignore] | S |
| A-G8-15 | `fix(clipboard): 외부 텍스트 블록 크기 상한` | clipboard.rs (직렬 2) | GlobalSize·tymed | 정상 소스 동일 | with_hglobal 테스트 | S |
| C16 | `refactor(core): 확장자·RAR 압축 수준 표 단일화` | nexa-core·nexa-tree·source.rs·rar.rs | ext_of·rar_level_name | 동일 | 정렬·COL_KIND·rar 테스트 | S |
| C08 | `refactor(gui): 외곽선·텍스트 세로 위치·HUD 공용화` | nexa-gui draw.rs·widgets/* | 2-C 요지 | 픽셀 동일 | 기존 60여 테스트 | M |

### 배치 16 — 중복 제거 2·불용 코드 (8)

| ID | 커밋 | 파일 | 변경 요지 | 동작 불변 | 검증 | 규모 |
| --- | --- | --- | --- | --- | --- | --- |
| C01 | `refactor(app): 보기 플래그 묶음과 툴바·메뉴 재구성 헬퍼` | win.rs (직렬 1) | ViewFlags·rebuild_* | 버튼·메뉴 목록 동일 | 스냅숏 테스트·R07 | M |
| D02 | `chore(app): 컨트롤 갤러리를 디버그 빌드로 한정` | main.rs·win.rs·ctl/mod.rs (직렬 2) | cfg 게이트 | 릴리스 사용자 경로 없음 | release strings 검사·exe 크기 기록 | S |
| C14 | `refactor(history): 휴지통 왕복 연산 통합` | win.rs (직렬 3) | RecycleRoundTrip | undo/redo 순서 동일 | fake 주입 테스트 | S |
| D03 | `chore(app): 낡은 dead_code 허용 정리` | oauth.rs·preview/*·grid.rs | 2-D 요지 | — | clippy·linux check 0 | S |
| D07 | `chore(vfs): 미사용 공개 API 정리` | nexa-vfs·nexa-core | 2-D 요지 | — | 두 타깃 check | S |
| D08 | `chore(preview): 미사용 UTF-16 사본 제거` | previewwnd.rs·preview/archive.rs | 2-D 요지 | 표시 동일 | R11 | S |
| C-G8-14 | `refactor(clipboard): 가상 파일 추출 공용부` | clipboard.rs | get_contents·copy_stream | 동기·워커 결과 동일 | 기존 2종 + copy_stream | M |
| C-G13-11 | `refactor(ctl): 글꼴 높이·텍스트 폭·통지 공용` | ctl/*·dialog.rs·about.rs | 2-C 요지 | 픽셀 동일 | About·설정 캡처 | S |

### 배치 17 — 테스트 보강 (8, 테스트 위주)

| ID | 커밋 | 파일 | 변경 요지 | 동작 불변 | 검증 | 규모 |
| --- | --- | --- | --- | --- | --- | --- |
| E03 | `test(gui): X-58 편집 클립보드 API` | rows.rs·pathbar.rs·edit.rs | 왕복 3건 | 테스트만 | — | S |
| E04 | `test(vfs): zip 확장 필드` | zip.rs | 5케이스 | 테스트만 | — | S |
| E05 | `test(tree): 필터·접기·앵커` | nexa-tree | 5건 + synthetic attrs | 테스트만 | — | S |
| E06 | `test(dnd): 드롭 결정표` | dnd.rs·shellmenu.rs | decide·diff_single 분리 | 판정 동일 | 6케이스 MC/DC | M |
| E11 | `test(gui): 테마 대비` | theme.rs | 대비 ≥ 3.0 | 테스트만 | — | S |
| E15 | `test(term): SD·RI·RIS·OSC` | nexa-term | 5건 | 테스트만 | — | S |
| E12 | `refactor(clipboard): 잘라내기 표시 상태 중립 모듈` | cutmarks.rs·source.rs·clipboard.rs | 2-E 요지 | Windows 경로 동일 | MC/DC 3쌍 | S |
| E09 | `test(order): 순서 편집 모델` | ordereditor.rs | OrderModel 분리 | 동작 동일 | 그룹 경계·잠금·flat | S |

### 배치 18 — 테스트 가능 구조 분리 (8)

| ID | 커밋 | 파일 | 변경 요지 | 동작 불변 | 검증 | 규모 |
| --- | --- | --- | --- | --- | --- | --- |
| C05 | `refactor(layout): 레이아웃 계산 순수 함수 분리` | win.rs → win/layout.rs (직렬 1) | compute_layout·panel_at_x(싱글 패널 범위 검사) | 동일 식 | (a)~(e) + clamp 불변식 | M |
| E13 | `refactor(app): 키 매핑·설정 문자열 매핑 모듈 분리` | win.rs → win/keymap.rs·settings_map.rs (직렬 2) | 이동 + 테스트 | 로직 변경 0 | VK 표·열 레이아웃 왕복 | M |
| C06 | `refactor(input): 히트 판정 순수 함수` | win.rs·route.rs (직렬 3) | hit_zone·splitter_hit | 듀얼 히트 범위 동일 | 격자 표 40+케이스 | M |
| E08a | `refactor(oauth): 응답 판정 분리와 재인증 분류` | oauth.rs·cloudfs.rs (직렬 1) | finish_response·parse_refresh·Reauth | 성공 경로 동일 | 각 3~5케이스 | M |
| E08b | `test(cloud): 목록 요청·계정 간 복사 계획` | cloudfs.rs (직렬 2) | 순수 분리 | 동일 문자열 | 3사×3 기대값 | M |
| E10 | `test(archive): 암호 우선순위·재시도 단계` | preview/archive.rs·archivewnd.rs | pick_password·next_step | 동작 동일 | MC/DC 3쌍·4분기 | S |
| D06 | `chore(tree): 미사용 root_count 삭제` | nexa-tree·chrome.rs | 2-D 요지 | — | test green | S |
| E-panel | `fix(panel): 펼침 집합 이름변경 키 기준 매칭` | panel.rs | 키 기준 매칭·문자 단위 꼬리 | 정상 입력 결과 동일 | 중첩·후행 구분자 | S |

### 배치 19 — 성능 2차 (8)

| ID | 커밋 | 파일 | 변경 요지 | 동작 불변 | 검증 | 규모 |
| --- | --- | --- | --- | --- | --- | --- |
| B13 | `perf(text): 문자 경계 일괄 측정` | draw.rs·dw.rs·dock.rs·edit.rs (직렬 1) | char_offsets | 길이·마지막 값 동일 | Probe 기본 구현·'abc한글' 대조·P10 | M |
| B15 | `perf(dw): 이미지 캐시 복사 제거와 변경 감지` | dw.rs (직렬 2) | Rc·(len, mtime) | 배치 동일 | 예산 LRU 테스트 | S |
| B16 | `perf(dw): 터미널 셀 폭 캐시` | dw.rs (직렬 3) | mono_cell_w·카운터 리셋 | 값 동일 | 생성 횟수 1 | S |
| E-G5-08 | `refactor(dw): MDL2 범위 판정 단일화` | dw.rs·chrome.rs (직렬 4) | is_mdl2(단일 문자일 때만 측정 분기) | 셰브론·E700..E8FF 동일 | 경계 표 | S |
| B32 | `perf(term): 스크롤백 후행 공백 절단` | nexa-term·win.rs | trim·Vec 재사용 | get_text·get_runs 동일 | trim 테스트·audit B-3 | M |
| B-G8-11 | `fix(ops): 복사본 수정 시각 보존` | nexa-ops lib.rs (직렬 1) | set_modified | 내용 동일 | 과거 mtime 왕복 | S |
| B-G8-09 | `perf(shellmenu): 배경 명령 전후 열거 상한` | shellmenu.rs | dir_snapshot cap | 소규모 폴더 동일 | cap 초과 None | S |
| B-G5-12 | `perf(font): 글꼴 목록 예열` | main.rs·fontbox | 백그라운드 families() | 결과 동일 | P01 trace | S |

### 배치 20 — 잔여 S 1 (8)

| ID | 커밋 | 파일 | 변경 요지 | 동작 불변 | 검증 | 규모 |
| --- | --- | --- | --- | --- | --- | --- |
| A06b | `fix(vfs): 압축 합계 포화 덧셈` | archive/mod.rs | saturating_add | 정상 합 동일 | 극단값 | S |
| A-G8-18 | `fix(rename): 예약 장치명 거부` | batch_rename.rs·nexa-ops lib.rs | stem 판정 | 일반 이름 동일 | con·COM10·NUL | S |
| A-G6-08 | `fix(rows): 타일 페이지 이동 단위` | rows.rs | grid_h 기준 | 리스트 동일 | `tiles_page_down_moves_one_screen` | S |
| A-G11-09 | `feat(plugin): 로드 실패 사유 표시` | preview/mod.rs·prefs.rs | PluginInfo.error | 성공분 동일 | 깨진 모듈 1건 | S |
| A-G1-14 | `fix(app): 시간대 변경 반영` | win.rs (직렬 1) | WM_TIMECHANGE | — | 시간대 변경 수동 | S |
| A-G1-15 | `fix(dw): 백엔드 생성 실패 진단` | win.rs (직렬 2) | 진단 파일·GDI 폴백 | 정상 경로 동일 | 주입 example | S |
| A-G2-16 | `refactor(diag): 벤치 상태 별칭 제거` | win.rs (직렬 3) | bench(hwnd) | 결과 동일 | Shift+F3 | S |
| B-G1-13 | `perf(dw): 백엔드 존재 시 글꼴 사양 생성 생략` | win.rs (직렬 4) | None 분기 이동 | 동일 | F3 avg | S |

### 배치 21 — 잔여 S 2 (8)

| ID | 커밋 | 파일 | 변경 요지 | 동작 불변 | 검증 | 규모 |
| --- | --- | --- | --- | --- | --- | --- |
| E-G5-11 | `test(dw): 캐시 키·내장 아이콘 키 순수 함수` | dw.rs·icons.rs (직렬 1) | 추출 + 왕복 | 문자열 동일 | 접미 조합 표 | S |
| D12 | `docs(dw): 클립 계약 정정` | dw.rs·draw.rs (직렬 2) | 주석 | — | — | S |
| A-G9m2 | `fix(term): 삽입·삭제 줄이 스크롤 마진 무시` | nexa-term lib.rs | 마진 밖이면 무시 | 마진 안 동일 | 상태줄 보존 | S |
| A64 | `fix(plugin): 플러그인 이미지 경로 제한` | previewwnd.rs·wasm.rs | 허용 경로 검증 | 정상 다이어그램 동일 | 임의 경로 거부 | S |
| C-X1-06 | `refactor(poll): 기준선 수립 함수 통합` | win.rs (직렬 1) | set_probe_baseline | 동일 | P09 | S |
| C-G4-10 | `refactor(poll): 재로드 디바운스 상태 기계` | fsprobe.rs·win.rs (직렬 2) | ReloadDebounce(+reset) | 전이표 동일 | (a)~(c)·타이머 소실 | S |
| D10 | `docs(panel): 뷰포트 프로브 주석 정정` | panel.rs | 주석 | — | — | S |
| D11 | `docs(info): catch_unwind 주석 정정` | fileinfo.rs | 주석 | — | — | S |

---

## 4. 품질 게이트 (각 배치 종료 시)

| # | 게이트 | 명령·절차 | 통과 조건 |
| --- | --- | --- | --- |
| Q1 | 단위 테스트 | `cargo test --workspace` (Windows) | 실패 0 |
| Q2 | clippy | `cargo clippy --workspace --all-targets --target x86_64-pc-windows-msvc` | 배치 0 이후 경고 0(신규 0) |
| Q3 | 비Windows 검사 | `cargo check --workspace --all-targets --target x86_64-unknown-linux-gnu` | 오류 0·경고 0(배치 0 이후) — docs/18 §2·§4 |
| Q4 | 커버리지 | `cargo llvm-cov --workspace --lcov --output-path target/llvm-cov.lcov` | 기준선 전체 39.3%·nexa-app 21.2%·nexa-gui 76.1% 대비 **하락 금지**. 배치 18 종료 목표: 전체 ≥ 43%, nexa-app ≥ 26%, win.rs ≥ 5%(분리 모듈 포함) |
| Q5 | 정규 점검 | `pwsh scripts/audit.ps1 -Quick` | PASS |
| Q6 | 예산(DR-2) | release 빌드 후 exe 크기·`Get-NexaStats` 유휴(트림 후) | exe ≤ 10 MB, 유휴 RSS ≤ 30 MB, 임포트 인박스 DLL만 |
| Q7 | 자동 UI 회귀 | ui-qa.ps1 R01~R16 중 해당 배치 영향 항목 + R01 | 기준 캡처와 비교 영역 일치(의도된 변경은 캡처 갱신 사유 기록) |
| Q8 | 성능 | 2-B-1 시나리오 중 해당 배치 항목 5회 평균 | 기준선 대비 악화 없음, 목표 항목은 진척 기록 |
| Q9 | 기록 | journal·DEVLOG·TODO(X-52/53/54/56/61 근거 갱신)·STATUS 한 트랜잭션 | docs/16 규약 |

배치별 필수 Q7 항목: 1→R08·R13 / 2→R15 / 3→R07·R09 / 4→R02·R03·R04 / 5→R06·R08 / 6→R02·R11 / 7→R12·R16 / 8→R01·R03 / 9→R01~R07 전체 / 10·11→클라우드 수동 / 12→R10·R14 / 13→R11 / 14→R04 / 15·16→R01·R07·R10 / 17·18→R01·R07 / 19→R06·R11 / 20·21→R01.

---

## 5. 기각 목록 (40)

| ID | 제목(요약) | 기각 이유 |
| --- | --- | --- |
| G2-07 | 활성화·복원마다 무조건 재열거 | X1-05와 중복 — B05로 흡수 |
| G2-14 | 워커 panic 시 pending 고착 | release `panic=abort`라 배포 빌드에서 시나리오 불성립 |
| G3-12 | 활성화 복귀 reload_both 멈춤 | X1-05 중복 — B05로 흡수 |
| G3-19 | 두 타이머의 tick 2중 호출 | tick 멱등·동시 구간 짧아 비용 미미 |
| G3-21 | wndproc 1850행 단일 함수 | 개별 결함(G3-05/09/10/16)은 수정, 구조 분할은 BL-7에 흡수 |
| G4-04 | 재로드 1회당 열거 2회 | G1-05·X1-06과 중복 — B06·C-X1-06 |
| G4-08 | 페인트 경로 빈 폴더 프로브가 OneDrive 하이드레이션 | 설계 동작·원격 제외 이미 적용, 하이드레이션 근거 부족 — 비용 문제는 X1-04(B11)로 |
| G4-13 | rename_expanded 바이트 슬라이스 | X4-13과 중복 — E-panel |
| G5-09 | 아이콘 실패 키 재요청 | 재도장 단위 재시도는 의도된 동작, 비용 미미 |
| G5-13 | DwBackend 책임 혼재 | 순수 이동, 기능 이득 없음 |
| G6-05 | 페이드 중 트랙 클릭 활성 | 설계 선택·빈도 낮음 |
| G6-12 | VirtualRows 2,260행 | L 이동 비용 > 가치, G6-03·G6-01·G6-06 선행 후 재검토 |
| G6-13 | 페인트마다 소유 String 생성 | DW·GDI 비용 대비 미미 |
| G7-16 | nexa-tree·vfs 파일 분할 | 순수 이동, 충돌 위험 |
| G8-08 | 셸 메뉴 전 항목 GetCommandString | 실측상 지연의 95%는 QueryContextMenu, 이 루프 비중 미미 |
| G8-16 | DragOver마다 경로 검사 할당 | 측정 근거 없음·비용 미미 |
| G8-17 | 프리셋 값 무상한 | UI 컨트롤 클램프를 거쳐 도달 경로 없음 |
| G8-21 | 마샬 패킷 Drop 부재 | 현재 누수 경로 0 |
| G8-23 | clipboard.rs 4책임 | 순수 분할 이득 낮음 — 중복은 C10·C-G8-14로 |
| G9-03 | 4KB 청크마다 PostMessage | 피해 근거 오류(conhost 프레임 스로틀, 큐 포화 근거 없음) |
| G9-09 | resize 행 축소 시 이력 손실 | ConPTY 재송신과 겹쳐 중복 줄 위험, 회귀 위험 > 이득 |
| G9-13 | par() 도달 불가 분기 | 동작상 완전 중복·무해 |
| G9-15 | nexa-term lib.rs 1709줄 | 순수 이동 — 파서 상태 추가 시 함께 분리 |
| G10-07 | 설정 창 오버레이 스크롤바 중복 | 실재하나 G10-09 모델 분리 뒤 재검토(우선순위 미달) |
| G10-10 | 클램프 규칙 3곳 분산 | A55(sanitize 클램프·상수 공유)로 흡수 |
| G10-11 | rebuild마다 registry·default 재생성 | 설정 창 한정·비용 미미 |
| G10-12 | tr() 호출마다 String 할당 | 측정 근거 없음·비용 미미 |
| G10-15 | 런처 시드 휴리스틱 오탐 | 비현실적 사례·영향 미미 |
| G10-16 | 런처 ShellExecuteW 동기 | 사용자 명시 실행 동작, 워커화 시 포그라운드 권한 위험 |
| G12-19 | 서비스 분기 문자열 match 9곳 | 미지 kind 도달 불가, trait 도입 L 비용 대비 이득 없음 |
| G12-20 | oauth.rs 4책임 | 순수 이동 — BL-11 빌더 도입 때 net 모듈만 분리 |
| G13-03 | 순서 편집 창 휠 미도달 | 기본 휠 라우팅(커서 아래 창)에서 정상 수신 |
| G13-04 | 일괄 이름변경 휠 포커스 기준 | 동일 — 이미 커서 아래 대상이 스크롤 |
| G13-08 | About rcPaint 줄바꿈 | DT_WORDBREAK 없어 접힘 불성립 |
| G13-12 | NxGrid font_height 반복 | 1 ms 미만 |
| G13-13 | 진행 창 세그먼트 O(n²) | 해당 경로 미진입(n 작을 때만 실행) |
| X2-04 | 행 선택마다 목록 전체 무효화 | B01(rcPaint) 선행 전 효과 0 — B01 후 재검토 |
| X2-08 | update_title 캐시 없음 | 비용 미미 |
| X2-09 | update_status 고정 비용 | 개별 항목(B04·B08b·B09·X-54)으로 흡수 |
| X4-08 | RTF 이스케이프 중복 | 공개 API 추가 비용 > 이득 — C10에서 to_rtf_mono 테스트만 추가 |

---

## 6. 사용자 결정 필요 (U)

| ID | 원 발견 | 결정할 내용 | 기본 제안 |
| --- | --- | --- | --- |
| U1 | X1-05② | 활성화 복귀를 '서명 변화 패널만 재열람'으로 바꿀지(08-23 X-44 결정 뒤집기) | 보류 — B05(지연만)로 충분한지 측정 후 |
| U2 | X3-12·G3-18 | 경로바 편집 중 다른 영역 클릭이 취소와 동시에 그 클릭을 전달할지 | 탐색기 관례대로 전달(단, 리네임 예약·drag_press 연쇄 차단 동반) |
| U3 | G9-14 | 복사 평문과 HTML/RTF의 줄 끝 공백 규칙(전각 공백) 통일 방향 | get_runs 규칙(' '만)으로 통일 + 테스트 |
| U4 | G6-09 | '내용 교체 시 두 바 잠깐 표시'(10-02 발견성)를 넘칠 때만으로 좁힐지 | 좁힘(배치 9 B-G6-09는 결정 후 진행) |
| U5 | G11-08 | 평문 암호를 워커와 공유하는 구조 허용 여부(X-56 선행) | X-56 착수 시 재결정 |
| U6 | G2 누락2 | 상세 정보(속성 시스템) 서드파티 핸들러 인프로세스 로드 범위(GPS 플래그) | GPS_FASTPROPERTIESONLY 검토 실측 |
| U7 | G8 누락3 | 배경 메뉴 셸 명령 후 새 항목 1개면 자동 리네임하는 휴리스틱 범위 | '새로 만들기' 서브메뉴로 한정 |
| U8 | G13-09 | ctldemo 삭제 vs 디버그 게이트 | 게이트(D02), X-23 β 종료 후 삭제 |
| U9 | G8-18 | 'nul.txt'처럼 확장자 붙은 예약명 차단(Windows 11 동작 차이) | stem만 차단, 확장자형은 실측 후 |
| U10 | X2-11① | 우클릭 셸 확장 예열 | X-61 ②(전용 STA 스레드)로 이관, 메인 UI 스레드 예열 금지 |
| U11 | G4-07 | 감시 스레드 IOCP 허브 착수 | A40·A41 효과 측정 후 |
| U12 | G1-11·G2-17·X1-15 | win.rs 대분해(st.cfg·Jobs/Watch/TermHost·run 분해) 착수 시점 | 배치 18 이후, 브랜치 분리 |
| U13 | G9-07 | 대체 화면(1049) 처리 | 인박스 ConPTY 실기 캡처 후 |

---

## 7. 백로그 (배치 외 · L/M 구조 작업, 권장 순서)

| ID | 원 발견 | 내용 | 규모 | 선행 |
| --- | --- | --- | --- | --- |
| BL-1 | G2-06·G8-05 | undo/redo 워커 실행(`ReversibleOp + Send`, `OpCx` 진행·취소, take/commit, 교차 볼륨 이동 undo 진행 창) | L | 배치 11 B19 |
| BL-2 | G1-04·G4-11·X1-03·G10-13·G1 누락4 | 세션 지연 복원(`Tree::deferred`, 비활성 탭 stale, 탭별 플래그 선적용, 실패 탭 보존) | L | A67 |
| BL-3 | G1-05·G2-08·G3-11·X1-07(2단계)·G4-02 | 프로브 스윕 워커(probe_gen 가드·단일 비행) | M | B06 측정 |
| BL-4 | X2-10·G7-07 | 폴더 열거 비동기(collect_entries/from_entries/expand_with, 세대 가드·nav 히스토리 도착 시점) | L | BL-2 |
| BL-5 | G1-18 | 가상 파일 붙여넣기·드롭 워커(+클라우드 대상 드롭 무시 안내) | M | — |
| BL-6 | G1-11 | st.cfg 미러 통합 → 터미널 상태 분리 → jobs/watch 분리 | L | U12 |
| BL-7 | X1-15②③·G2-17·G3-21 | update_status 분해·run_command 모듈 분할·`plan_transfer` 순수 추출 | L | C05·C06·E13 |
| BL-8 | G6-03 | rows 내장 오버레이 바 → OverlayBars 치환(+tick 분리) | M | 회귀 고정 테스트 3건 선행 |
| BL-9 | G11-04·G11-05·G11-13·G1-17 | textdoc 공용(태그 해석 정확 일치)·ScrollModel2D·PvModel·parse_bmp | M | — |
| BL-10 | G13-07·G13-10 | `run_modal`(10곳·WM_QUIT 처리)·`ctl::popup`(모니터 맞춤) | M | A57 |
| BL-11 | G12-16·G12-17·G12-18·G12 누락3 | HTTP Req 빌더·graph/gdrive 헬퍼·upload_tree_generic(취소·진행 전달) | M | E08a |
| BL-12 | G10-08 | prefs Slot 레지스트리(필드군별 단계 커밋) | L | E07 |
| BL-13 | G4-07 | WatchHub(IOCP 단일 스레드) | L | U11 |
| BL-14 | G7-06·G12-08·G12 누락2 | BufReadAt·positional read, 다운로드 스트리밍(+Dropbox 진행·취소) | M | — |
| BL-15 | G5-01·G5-07·G5-06(폭 키) | 스크래치 BRT 클립, WIC 워커 디코드, 레이아웃 키 축소·세대 축출 | M | 측정 |
| BL-16 | G1-03·G2-09·G12 누락6 | 클라우드 lister 연결 레지스트리, 모달 `ask()` + state_of 재진입 debug_assert | M | — |
| BL-17 | G9-10·G9-16·G9-07(1049) | 이모지 폭(대상 OS 표 명시)·지연 줄바꿈 플래그(기대값 'abcxe')·대체 화면 | M | U13 |
| BL-18 | G12-05 | Google 재개 업로드 308·Range 재동기화(헤더 추출 일반화) | S~M | E08a |
| BL-19 | G10-06 | 설정 창 DPI 스케일(fontbox·searchbox·ordereditor 포함) | M | E07 |
| BL-20 | X4 누락3 | pathbar·rows 편집 래퍼 7쌍 `EditState::menu_state` 공용 | S | E03 |
| BL-21 | G11 누락4 | 플러그인 압축 세션 암호 기억 | S | U5 |

---

## 8. 진행 원칙 요약

1. 배치 0 → 1 → 2는 쉬지 않고 진행한다(크래시·데이터 손실). A13·A14·A15·A40·A67·A70 등 M 표기는 착수 전 재현 캡처를 journal에 남긴다.
2. 배치 3~9는 입력·렌더 경로라 매 배치 R01~R07 캡처를 남긴다.
3. 배치 10·11(클라우드)은 Dropbox 테스트 연결로 실기 확인하고, 배치 순서를 바꿔도 된다(다른 배치와 파일 교집합 없음 — win.rs 1건씩 제외).
4. 사용자 QA가 병목(CLAUDE.md §7-2)이므로, 각 배치 종료 시 'QA 대기분' 목록을 STATUS에 갱신한다.
5. 릴리스는 배치 2·9·14·19 종료 시점이 자연스러운 경계다(태그 push는 사용자 승인 후).
