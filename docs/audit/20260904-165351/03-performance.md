# 03. 성능 핫패스 조사 — 2026-09-04

대상: `win.rs`(WM_PAINT·ensure_dw·term_paint·update_status·타이머) · `watcher.rs` · `fsprobe.rs` · `shellnotify.rs` · `source.rs` · `nexa-gui rows.rs` · `nexa-tree lib.rs` · `dw.rs` · `icon.rs/icons.rs`.

## 발견(영향순)
| # | 심각도 | 위치 | 문제 → 수정 방향 |
| --- | --- | --- | --- |
| 1 | CRIT | `win.rs:4326-4430` | `paint()`가 `ps.rcPaint`를 읽지 않고 클립도 안 걸어 **모든 무효화가 전체 장면 재도장**(패널 2·터미널 2·툴바·상태바) + 전폭 BitBlt. `Invalidations` 체계(`nexa-gui widget.rs:35-45`)가 만든 좁은 rect가 버려짐. → `IntersectClipRect(rcPaint)` + 위젯에 rcPaint 전달 + 부분 BitBlt |
| 2 | CRIT | `win.rs:6908, 6921, 8250-8262, 5584` | `TIMER_TERM_CARET`(~530ms)이 터미널 클릭에 무장되고 **`term_focus=None`일 때만** 해제 — `WM_ACTIVATEAPP`·최소화가 안 끔 → 백그라운드 창이 #1과 결합해 초당 2회 전체 재도장. → 비활성/최소화에 kill·재활성에 re-arm·캐럿 2-rect 무효화 |
| 3 | CRIT | `panel.rs:829-858` → `nexa-tree lib.rs:444-458` | `sync_watchers`→`watch_dirs(64)`가 가시 행 전부 `tree.row(i)`(이름 String 클론). `update_status`(**31 호출처** — 클릭·키·탭·명령 전부) + FSPOLL 틱마다. 100k 폴더 = 키 입력마다 100k 할당. → `row_ref`/`(NodeId, expanded, is_dir)` 무할당 접근자 |
| 4 | CRIT | `panel.rs:1436-1444` → `lib.rs:540-546` | `index_of_path` = `visible` 선형 스캔 + 후보마다 `to_string_lossy()` 할당. 선택 경로마다 호출 → Ctrl+A 100k 후 감시 리로드 = 10¹⁰ 비교. `TIMER_WATCH_BASE`(`win.rs:8196`)가 자동 유발. → 소문자 경로 HashMap O(N+S) |
| 5 | HIGH | `panel.rs:1387-1445`·`:200-254` · `nexa-tree lib.rs:229-286` · `nexa-vfs lib.rs:50-77` | 열거·트리 작업 전부 **UI 스레드 동기**(워커는 watcher·conpty·icons·cloud·recycle·OAuth만). `dirent.metadata()`가 재분석점(OneDrive 자리표시자)마다 실제 CreateFile. 정렬은 열거 시 1회(정상). → 워커 + `WM_APP_*` 완료 통지 |
| 6 | HIGH | `win.rs:3192-3236`·`:119-128` · `fsprobe.rs:64-97` · `panel.rs:864-882` | FSPOLL 틱(3s 활성)마다 패널당 루트 read_dir(≤4096+metadata) + **가시 폴더 행마다 read_dir** — 트리 모드 40폴더 = 3초마다 82회. `probe_skip_slow`(`source.rs:67-83`) 미적용 → UNC/네트워크 전부 스윕. → 게이트 재사용·틱당 몇 폴더 라운드로빈 |
| 7 | HIGH | `win.rs:2174-2205`·`:2067-2115` · `preview/mod.rs:241-247` | `update_status`마다 `update_dock_info` → 프리뷰 종류면 `preview_for` **디스크 재읽기+재파싱**(마크다운·압축 목록·이미지 디코드) — 캐시 없음, 화살표 키마다. → `(path, mtime, len)→PreviewDoc` 캐시 |
| 8 | HIGH | `dw.rs:507-521`·`:573-579` | `image_scaled` 캐시 **히트에서 `v.clone()`**(BGRA Vec 전체 복사, 1200×800 = 3.8MB/프레임). → `Rc`/borrow 내 StretchDIBits |
| 9 | HIGH(mem) | `dw.rs:33, 427-462, 880-889` · `:690` | 레이아웃 캐시 키 = 픽셀 폭+스타일 → 컬럼 드래그/리사이즈마다 새 네임스페이스, LRU 없음, 4096에서 **전체 소거**(정지+재구축 반복). `mono_glyphs` 2048 동일 |
| 10 | HIGH(startup) | `win.rs:1272-1300` → `panel.rs:200-266` · `icons.rs:271-290` | 세션 탭 전부 즉시 열거 + 펼침 경로 전부 expand, 창 생성 전. 첫 WM_PAINT에 DW 팩토리·포맷 6·폴백 4·`EnumFontFamiliesExW`·SVG 28개 동기 래스터. → 활성 탭만·비가시 아이콘 지연 |
| 11 | MED-HIGH | `win.rs:2356-2366` · `dw.rs:666-717`·`:646-664` | 셀마다 `fill_rect`+`layout.Draw`(80×30 = 4,800 호출/프레임, 캐럿 주기) · `term_cell_w()`가 **매 term_paint마다** `"0"` 레이아웃 생성+GetMetrics. → 메모이즈·ASCII 런 병합 |
| 12 | MED | `dw.rs:792-810`·`:602-608` → `gdipctx.rs:445-454, 533-539` | MDL2 글리프 `text_width` 캐시 우회(버튼마다 레이아웃 생성) · `fill_round_rect_alpha`가 호출마다 GdipCtx 생성/파괴(오버레이 바 프레임당 여러 번). → 캐시·장수명 GdipCtx |

## 타이머 표
| id | 상수 | 주기 | 무장 | 비활성 | 최소화 |
| --- | --- | --- | --- | --- | --- |
| 1 | TYPEAHEAD | 250ms | 버퍼 있을 때·자가 해제 | ○ | 일시 |
| 2 | ICONS | 80ms | 대기 아이콘 있을 때·자가 해제 | ○ | ×(페인트 없음) |
| 3 | JANITOR | 10s | 활동 시·트림 후 자가 해제 | ○ | kill |
| 6 | **TERM_CARET** | ~530ms | 터미널 포커스 · **포커스 해제 클릭만 kill** | **○(결함)** | **○(결함)** |
| 8 | SESSION_SAVE | 1s 디바운스 | dirty 시 1회 | ○ | ○ |
| 10/11 | WATCH_BASE | 300ms(최대 1s) | 통지 시 1회 | ○ | ○ |
| 13 | CLOUD_POLL | 200ms | 전송 중 | ○ | ○ |
| 14 | **FSPOLL** | 3s/30s | 가시 중 상시 | ○(설계) | kill(단, `WM_ACTIVATEAPP(FALSE)`가 재무장 — 04 #3) |
| 15 | WIDGET_TICK | 40ms | request_tick 시·~1.1s 후 해제 | ○ | ○ |

## 메모리
레이아웃 4096/글리프 2048 nuke-on-full · 이미지 8개(각 BGRA 전체·히트 복사) · 아이콘 256 LRU(O(n) 퇴출) · `fontbox::FAMILIES` 영구 · 터미널 스크롤백 800×cols×16B ≈ 2.5MB/패널(트림 미대상, `Vec<Vec>` 800 할당) · `Screen::resize`가 페인트 경로에서 전체 재구축.

## 감시자
RDCW 8KB 비재귀 OVERLAPPED · 디렉터리당 스레드 1(패널당 64 상한 = **최대 128스레드**, 각 2MB 스택 예약) · 디바운스 300ms+1s 상한(테스트 있음) · overflow 재열거 복구 · `is_alive` 자가 치유 · UNC에도 무차별 구독(신뢰성 낮음 → fsprobe가 보완이나 게이트 없음).

## 아이콘
확장자/폴더 키는 UI 스레드 동기(`SHGFI_USEFILEATTRIBUTES`, 4개/80ms 제한) · 파일별 키(.exe/.lnk…)는 STA 워커 · 256 LRU · `emb:` SVG는 페인트 중 동기 래스터(트림 후 첫 프레임 툴바 전체 재래스터).

## 노출 지표
제목줄 평균 µs·first render·프레임 수·F3 벤치(200프레임)·항목/선택 수·필터 상태·watcher 생존·아이콘 큐. **없음**: p99 페인트·열거 시간·fsprobe 틱 비용·스레드 수·캐시 점유. 평균은 세션 수명 누적이라 둔감(bench 외 리셋 없음).

## 견고한 부분
행 가상화·컬럼 컬링 · `Invalidations` 모델 · 정렬 위치(열거 시 1회·무할당 비교) · 타입어헤드 무할당 · `row_ref` · 감시자 수명 관리 · 디바운스 상한 · 아이콘 분리·제한 · 재니터 트림·최소화 즉시 트림 · `fsprobe` 2단 서명(테스트) · `probe_skip_slow` · 크래시 훅.
