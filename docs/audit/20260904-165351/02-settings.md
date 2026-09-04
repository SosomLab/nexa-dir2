# 02. 설정·세션 관리 조사 — 2026-09-04

대상: `config.rs`(전체) · `prefs.rs`(PrefValues/harvest/apply_now) · `win.rs`(current_settings·persist_settings·apply_prefs·open_prefs·WM_DESTROY) · `panel.rs`(session_*·sync_expanded) · `launcher.rs` · `main.rs`.

## 발견(영향순)
| # | 심각도 | 위치 | 문제 → 수정 방향 |
| --- | --- | --- | --- |
| 1 | HIGH | `config.rs:700-706`(파싱) vs `:474-489`(직렬화) | `cloud_client_secret_<kind>`가 **파싱만 되고 직렬화되지 않음** — 어떤 설정 변경이든 다음 저장에서 유실. 왕복 테스트가 `client_id`만 단언(`:1268-1273`). 기본 시크릿(`oauth.rs:97`)이 가려 안 보임. → 쓰기 루프 추가 + 테스트 단언 |
| 2 | HIGH | `config.rs:899-908` | `fs::write(tmp)` → `if dst.exists() { remove_file(dst) }` → `rename` — ⓐ rename이 이미 교체하므로 **옛 파일 선삭제는 무보호 창**(rename 실패 시 파일 소실) ⓑ fsync 없음(전원 차단 = 0바이트) ⓒ 임시명 고정 = 2인스턴스 교차. → remove 제거·`File`+`sync_all`·`{name}.{pid}.tmp` |
| 3 | HIGH | `win.rs:6223-6226, 4657, 4675, 4721, 4765, 6039-6040, 8365` | 변경마다 UI 스레드에서 `current_settings`(~25 clone) + 전체 `serialize` + 4 syscall. `apply_prefs` 끝 저장은 **무조건**(`apply_now`가 EN_KILLFOCUS/CBN_KILLFOCUS에도 발화 → 설정 창 Tab 이동마다 저장). → `last_saved` 비교(PartialEq 이미 derive) 또는 `TIMER_SETTINGS_SAVE` 디바운스 |
| 4 | HIGH | `win.rs:1280-1300` → `panel.rs:209-216`·`:257-266` | 세션 복원이 **창 생성 전 동기**: 탭마다 `open_filtered` + `seed_expanded`가 펼침 경로(탭당 최대 200)마다 열거 → 8탭/패널 = 최대 3,200회 동기 read_dir, 끊긴 UNC면 SMB 타임아웃 동안 창 없음. → 창 먼저·활성 탭만·나머지 첫 전환 시 |
| 5 | MED-HIGH | `win.rs:8359-8371` | split·dock 비율·펼침·컬럼 폭은 **WM_DESTROY에서만** 저장. `WM_ENDSESSION`/`WM_QUERYENDSESSION` 핸들러 없음 → 종료/로그오프 시 미저장. 실패 보고는 `eprintln!`(GUI 서브시스템 = 안 보임) |
| 6 | MED-HIGH | `main.rs:103-108` | 단일 인스턴스/파일 잠금 없음 → 마지막 저장 승리 · 임시명 공유(#2) · 파서가 미지 키를 버려 다른 빌드가 키 제거 |
| 7 | MED | `win.rs:6041` | `apply_prefs` 끝 `InvalidateRect(None)` 전체 무효화 — 앞의 15개 영역 무효화 무력화 · `dlg_font`(`:5839`)·`term_copy_format`(`:5934`) 비교 없이 대입 |
| 8 | MED | `config.rs:894-896` → `win.rs:1244-1246` | `read_to_string().ok()` → 비UTF-8(손편집 ANSI 한글 글꼴명)이면 **전체 기본값** 후 첫 저장이 덮어씀. .bak·경고 없음. → `from_utf8_lossy` + `.bad` 사본 + 경고 |
| 9 | MED | `panel.rs`(session_dirty 14곳) · `win.rs:7162, 7141, 5886-5896` | dirty가 탭/경로만 — 펼침·컬럼 폭/순서/레이아웃·스플리터·도크 비율은 안 세움. `apply_prefs`가 `col_layout` 적용 후 settings.cfg만 저장(`col_layout`은 session.cfg) |
| 10 | MED | `win.rs:4569-4576`·`:8143-8151` → `panel.rs:279-289`·`:755-782` | 세션 디바운스 저장이 탭마다 `self.active` 임시 변경 + `sync_expanded` 전체 가시 행 스캔(행마다 PathBuf/String 할당) — 활동 중 매초. `.take(200)`이 사전순 절단 |

## 질문 답변
- **저장 시점/방식**: 즉시·동기·디바운스 없음(세션은 1s 디바운스 — 좋은 설계, 설정도 따라야 함). 15개 저장 호출처.
- **삼중 매핑**: 손으로 유지하는 필드 목록 **15곳**(불리언 하나 추가에 9~11곳 수정). 비대칭: 설정만(split·dock·view_mode·panel_mode·preview_map·launcher·cloud_*) / PrefValues만(`langs`·`col_layout`=세션 상태). 클램프 3곳 중복(`config.rs:553-575`, `prefs.rs:876-894`, `win.rs:5815-5819`) · `sanitize`가 `base/ctx/status/list_font` 빈 값을 안 막아 State는 `""`·파일은 거부 → 영구 불일치. `is_modified`가 호출마다 `Settings::default()`(toolbar 파싱 포함) 생성 → `OnceLock` 권장.
- **apply_now**: 비싼 분기(DW 재생성·양 패널 재열거·툴바 재구성)는 동등 비교로 게이트됨(정상). 미게이트 = 저장·전체 무효화·`update_title`.
- **세션 크기**: 펼침 200/탭·탭 인덱스 ≤64·런처 32·클라우드 32 상한. **무제한**: 탭 수·`cloud_client_id/secret_*`(중복 누적). MRU 없음.
- **파서**: 미지 키 무시·클램프·주석·CRLF·순서열 정규화+백필(테스트 있음) — 좋음. 약점: BOM 미제거(`kv_lines`)·`=` 주변 공백 불허·버전 검사 없음·비UTF-8 전체 초기화.
- **데이터 폴더**: `OnceLock` 1회 판정(정상) · `.w<pid>` 프로브 파일 잔존 가능 · 저장 실패 `let _`.
- **창 위치/크기 미저장**(`WINDOWPLACEMENT` 없음).

## 견고한 부분
`current_settings` 단일 원천 · 세션 디바운스(dirty+1s+KillTimer) · 관대한 클램프 파서 + 순서열 백필 · `launcher_items: Option` 첫 실행 구분 · `.txt→.cfg`/`NexaDir2→NexaDir` 마이그레이션 · 데이터 폴더 1회 판정 · apply_prefs 비싼 분기 게이트 · 왕복 테스트 폭.
