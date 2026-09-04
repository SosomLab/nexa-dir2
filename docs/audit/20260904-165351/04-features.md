# 04. 최근 보완 기능 회귀 조사(X-40~X-50) — 2026-09-04

방법: TODO §7 각 행의 주장을 코드로 대조(ⓐ 주장=구현 ⓑ 엣지 ⓒ 조용한 no-op ⓓ 테스트) + i18n 3종 키 집합.

## 발견(심각도순)
| # | 심각도 | 위치 | 문제 → 수정 |
| --- | --- | --- | --- |
| 1 | HIGH ✔ | `nexa-term lib.rs`(`get_runs`) | `Vec::with_capacity(el - sl + 1)`가 `start_line > end_line`에서 언더플로(디버그 패닉·릴리스 abort). → `if el < sl { return Vec::new() }` — **수정 `b279df7`** |
| 2 | HIGH ✔ | `nexa-vfs archive/mod.rs:319-320` | `out.truncate(MAX_NAME)`가 4096바이트가 UTF-8 경계가 아니면 패닉 — >4KiB CJK 이름 엔트리로 도크 미리보기 중 UI 스레드 abort. → `is_char_boundary`까지 후퇴 + 회귀 테스트 — **수정 `b279df7`** |
| 3 | MED-HIGH | `win.rs:6608` vs `:6639` | `WM_ACTIVATEAPP(FALSE)`가 최소화 뒤에 도착해 `TIMER_FSPOLL` 30s 재무장 → `reload_both`가 트림 직후 메모리 재팽창(X-40 "최소화 정지" 위배). → `if !st.trimmed` |
| 4 | MED | `win.rs:3217`·`:3249` | 뷰포트 스윕이 가시 폴더마다 전체 프로브, 느린 경로 게이트 없음(X-43은 `probe_skip_slow` 적용). → 게이트 재사용(루트별 GetDriveType 캐시) |
| 5 | MED | `fsprobe.rs:56, 90` | 서명이 (size, mtime)만 — **이름 미포함** → 크기·mtime 같은 이름 변경이 부모 mtime 불변 시 불가시(X-44 4차가 열거만 잡는다고 한 경로). → 이름 해시 혼합 |
| 6 | MED | `win.rs:8075, 8081` · `clipboard.rs:863-896` | 터미널 Ctrl+C가 `write_text_html_rtf` 반환값 무시 → 클립보드 열기 실패(타 앱 점유)에도 `sel=None`(복사 무·선택 소실) · `put_registered` 부분 실패 무신호. → 실패 시 선택 유지+상태 표시 |
| 7 | MED | `lib.rs:1161-1164` vs `win.rs:2347`·`:8056` | `TextRun`에 faint 없음(PSReadLine 예측이 진하게 복사) · 글꼴 = `term_font.split(',').next()` 원문(`fontchain::first_installed` 아님). → faint 추가·first_installed |
| 8 | MED | `prefs.rs:1479-1481`(+`:172`) | `Kind::Select`에 폴백 선택 없음 — 미지 `term_theme*` id·다크 슬롯의 라이트 id면 콤보 **공란**, 값은 그대로 영속. → 채움 후 `resolve_scheme` 결과로 CB_SETCURSEL + 값 기록 |
| 9 | MED | `rows.rs:23-25`(BAR_THIN 6·BAR_WIDE 10·THUMB_MIN 24) | 오버레이 바 치수가 논리 px 고정, bounds/row_h는 DPI 스케일 → 150/200%에서 ½~¼ 크기. → `set_metrics`에 dpi |
| 10 | MED | `rows.rs:993` | 키보드/타입어헤드 스크롤이 `scroll_row` 직접 변경 → `flash_bar` 없음(PageDown/End에 바 미표시). → `scroll_to` 경유 |
| 11 | MED-LOW | `rows.rs:745-754, 614` · `win.rs`(TrackMouseEvent 없음) | 포인터가 썸 위에서 창 밖으로 나가면 `bar_hover` 고착(페이드 안 됨). → WM_MOUSELEAVE/비활성에 해제 |
| 12 | MED-LOW | `win.rs:1980-1982, 1962-1966` | 가상 붙여넣기 실패(디스크립터 없음·전 항목 거부·추출 0)가 전부 조용한 no-op. → 0건 시 상태 표시 |

차순위: `archivewnd.rs:236` 라이트 고정 `Style::default()` · `:282` 모달 펌프 WM_QUIT 삼킴 · `:337-343` 빈 선택 Ctrl+C = 전체(헤더 없음) · 압축 목록이 선택 변경마다 UI 스레드 동기(`win.rs:2085`) · `sanitize_rel` 예약 장치명(CON·NUL·COM1)·후행 점/공백 미거부 · `fontchain.rs:37-49` GDI **현지화** 패밀리명 대조 + 프로세스 수명 캐시(`fontbox.rs:77`) · `fsprobe.rs:80/105` SCAN_CAP 경계 진동 시 영구 changed · `archivewnd.rs:112` 플래그 컬럼 정렬 키·0바이트 ratio None.

## 기능별 판정
| X | 판정 | 요지 |
| --- | --- | --- |
| X-40 | gap | fsprobe·디바운스·복귀 갱신 일치 · 이름 없는 서명(#5)·최소화 재무장(#3) |
| X-41 | n/a | 범위 밖 |
| X-42 | gap | 경로·충돌·워커·undo 일치 · 실패 무응답(#12)·예약 장치명 |
| X-43 | OK | `loaded_child_count`·캐시 프로브·`probe_skip_slow`·`is_dir` |
| X-44 | gap | 스윕 게이트 없음(#4) |
| X-45 | OK | 공유 명령·TOPMOST 밴드·영속·백필 테스트 |
| X-46 | gap | 절단 패닉(#2 ✔)·UI 스레드 목록·라이트 고정·WM_QUIT |
| X-47 | gap | DPI(#9)·키보드(#10)·호버(#11)·**테스트 0** |
| X-48 | gap | 현지화 이름·수명 캐시 |
| X-49 | OK | 콤보 공란(#8)만 |
| X-50 | gap | 역순(#1 ✔)·faint/글꼴(#7)·클립보드 실패(#6) |

## i18n
ko/en/ja **467키 동일 집합** · 코드 리터럴 182키 + 간접 테이블(archive.col.*·TERM_COPY_OPTS·pref.theme.*·pref.termTheme*·archive.*·bulkrename KINDS) 전부 해석 · 누락 0 · 스킴 이름은 고유명사(태그만 번역).

## 테스트
있음: fsprobe 7(SetFileTime 무통지 계약 포함)·panel(viewport/watch/조상 폴백)·config(ontop 백필·term_theme*·copy_format)·source·archive 30·archivewnd 3·nexa-term(스킴·대비·resolve·runs·export).
없음: X-47 전부(`thumb_rect`/`flash_bar`/`tick`/`bar_mouse_*` 순수 로직) · X-48 `first_installed`/`fallbacks`/`GdiChain::runs` · X-50 faint·4택 라우팅 · X-46 암호 재시도 루프 · X-42 sanitize_rel 예약명 표·중첩 plan_dests · X-43 `probe_skip_slow` · X-49 `pal_light` 반전 규칙(win.rs).
