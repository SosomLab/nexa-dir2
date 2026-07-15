# BRANCHES — 브랜치 기록 (Branch History, 시간 역순)

> **목적**: 병합 후 삭제되는 작업 브랜치의 이력을 남긴다(원본 규약 차용). **정렬: 시간 역순(최신이 위)** — 새 브랜치는 표·상세 모두 맨 위에 추가. 시각=커밋 committer date(KST).
> **규약**: 브랜치는 main 병합·green 확인 후 삭제, 이력은 이 문서 + journal에 보존. push는 사용자 명시 요청 시에만.

## 요약 (시간 역순)

| 브랜치 | 생성 | 병합(커밋) | 삭제 | 커밋수 | 작업 요약 | 상세 |
| --- | --- | --- | --- | --- | --- | --- |
| `feat/session-autosave` | 2026-07-15 | 2026-07-15 (main 병합) | 2026-07-15 | 1 | 세션 **디바운스 자동 저장**(탭/경로 변경 플래그→1초 코얼레싱 flush·마지막 상태만 — 원본 SESS 이행·크래시 방어) + 진행 창 **[닫기 (N)] 카운트다운 버튼**(자동/클릭 즉시). 회귀 테스트·168 green | [2026-07-15](journal/2026-07-15.md) |
| `fix/prefs-batch2-qa` | 2026-07-15 | 2026-07-15 (`34a6564`) | 2026-07-15 | 1 | 배치 2 QA 5건 — 진행 창 중첩(erase)·공백 WM_CHAR 라우팅·파일 목록 그룹 분할+**본문 휠 스크롤**·**3×3 피커**(PosGrid)·리셋 ms 표시 누락(저장 리셋 진범) | [2026-07-15](journal/2026-07-15.md) |
| `feat/prefs-batch2` | 2026-07-15 | 2026-07-15 (`016577d`) | 2026-07-15 | 1 | 설정 배치 2(사용자 요청 ①~③) — 전송 완료 2초 자동 닫기(PROG-WIN)·탭 더블클릭 동작(+**탭 고정** 신규: 📌·그룹 정렬·세션)·타입어헤드 옵션 전면(범위·리셋·HUD 3×3·체크 3). ④~⑥=X-12~14 등재. 168 green | [2026-07-15](journal/2026-07-15.md) |
| `fix/sort-opts-propagate` | 2026-07-15 | 2026-07-15 (`0cbe1e5`) | 2026-07-15 | 1 | 정렬 옵션 전파 수정(사용자 QA "순서 불변") — replace_source 빈 정렬 덮어쓰기(열거 순서 퇴행) 차단·TreeSource 기본 키 Name asc·Panel 옵션 보관+apply_sort_opts(탐색/재로드/새 탭 재적용). 회귀 테스트 추가·168 green | [2026-07-15](journal/2026-07-15.md) |
| `fix/case-sort-group` | 2026-07-15 | 2026-07-15 (`c01d629`) | 2026-07-15 | 1 | 대소문자 정렬 **대문자 그룹 상단** 확정(사용자 스크린샷 QA — 코드포인트 순·Abc.txt<a c.txt. NTFS 순서 무관 확인). 테스트 167 green | [2026-07-15](journal/2026-07-15.md) |
| `fix/case-sort-upper` | 2026-07-15 | 2026-07-15 (`74b5f31`) | 2026-07-15 | 1 | 대소문자 정렬 규칙 보완(사용자 QA) — cmp_cs_upper_first: 알파벳 순 유지+같은 이름은 **대문자 우선**(코드포인트 그룹핑 배제). 테스트 167 green | [2026-07-15](journal/2026-07-15.md) |
| `fix/navup-align` | 2026-07-15 | 2026-07-15 (`5999443`) | 2026-07-15 | 1 | Alt+↑ 선택 위치 설정(상단/중단/하단 — select_program_aligned·nav_up_align 기본 중단)·**대소문자 구분 정렬**(SortSpec.case_sensitive·sort_case_sensitive 체크박스·전 탭 재정렬). 실기 노출·영속 검증 | [2026-07-15](journal/2026-07-15.md) |
| `feat/x3-term-opts` | 2026-07-15 | 2026-07-15 (`43fca56`) | 2026-07-15 | 1 | X-3 터미널 옵션 — term_wrap(기본 켜짐=현행 유지)·term_cols(80~1000 기본 240) 고정 열+**가로 스크롤**(view_x·Shift+휠/가로 휠·캐럿/히트 오프셋)·설정 창 노출·영속 | [2026-07-15](journal/2026-07-15.md) |
| `fix/easy-gaps` | 2026-07-15 | 2026-07-15 (`9fb91e9`) | 2026-07-15 | 1 | 쉬운 갭 — G-7(위로 시 떠난 폴더 선택)·G-12(webp)·G-13(폴더 우선 정렬 옵션·전 탭 재정렬)·**런처 시드 v2**(pwsh·cmd 아이콘+구분선·launcher_seed 1회 마이그레이션 — 실기 왕복 검증) | [2026-07-15](journal/2026-07-15.md) |
| `feat/x10-prefs-polish` | 2026-07-15 | 2026-07-15 (`abdbc89`) | 2026-07-15 | 1 | X-10 잔여 — 트리 매치 수 "(N)"·다중 토큰 AND+"N개 일치"·설명 문장(.desc 9종)·수정됨 accent 바·전역 검색 "카테고리: 항목" 접두. 자동화 검증(AND·접두·설명) | [2026-07-15](journal/2026-07-15.md) |
| `fix/prefs-tree-drill` | 2026-07-15 | 2026-07-15 (`30f881e`) | 2026-07-15 | 1 | 설정 드릴다운(사용자 QA 3건) — **그룹 = 하위 메뉴 링크 페이지**(세부는 leaf 선택 시·SS_NOTIFY 클릭 이동)·**검색 유지 탐색**(leaf=매치 상세만·그룹=매치 자식 링크·비움=일반 복귀)·검색창 **✕ 지우개**. 자동화 덤프 검증(검색 중 클릭=검색어 유지+매치 2항목) | [2026-07-15](journal/2026-07-15.md) |
| `fix/prefs-tree-search` | 2026-07-15 | 2026-07-15 (`dc9e006`) | 2026-07-15 | 1 | 설정 트리 검색 필터(사용자 QA — X-10 ①): 검색 시 **라벨 매치 또는 하위 상세 설정 매치 노드만** 표시(조상 경로 유지·그룹 매치=하위 전체·검색 중 ▾ 고정·클릭=검색 종료 이동). 자동화 검증 "글꼴"→4개(터미널=상세 매치)·해제→8개 | [2026-07-15](journal/2026-07-15.md) |
| `feat/prefs-tree` | 2026-07-15 | 2026-07-15 (`3b3e0ea`) | 2026-07-15 | 1 | 설정 창 전면 개편(사용자 지시) — 사이드바 = **계층 트리 단일 컴포넌트**(오너드로 LISTBOX·들여쓰기·▸/▾·pre-order TREE+펼침 평면 투영)·클릭=우측 세부(그룹=토글+하위 부제목 섹션)·계층 일반/파일 목록/하단 패널. 자동화 실기 검증(항목 8·접기 8→5→8) | [2026-07-15](journal/2026-07-15.md) |
| `feat/m5-ui-polish` | 2026-07-15 | 2026-07-15 (`f73a909`) | 2026-07-15 | 1 | M5-1 보완(사용자 요청 4건) — 런처 exe 아이콘 16×16 정사각(draw_icon 반환값 폴백)·그룹 구분선(`launcherN=-`)·도구 모음 토글 시각(배경 sel_bg+accent)·**일괄 이름변경 순서형 파이프라인**(RenameOp 목록 — 정규식 regex-lite[원장 등재·B2 1.01MB]·구간 이동·확장자 변경·블록 ▲▼✕·프리셋 data\renames\*.cfg). docs/19-parity-gap 신설. 테스트 166 green | [2026-07-15](journal/2026-07-15.md) |
| `feat/m5-bulk-rename` | 2026-07-15 | 2026-07-15 (`3a5834a`) | 2026-07-15 | 1 | M5-1 일괄 이름변경 α(원본 docs/25 스펙 최초 구현 — 원본도 설계만) — nexa-ops::batch_rename(치환→대소문자→삽입→연번 고정 파이프라인·이름부만·충돌 4종)+bulkrename.rs 다이얼로그(실시간 미리보기 LISTBOX·충돌 ⚠·[적용]=충돌 0)+MoveBatchOp 트랜잭션 undo·Ctrl+Shift+R. 테스트 163 green | [2026-07-15](journal/2026-07-15.md) |
| `feat/m5-launcher` | 2026-07-15 | 2026-07-15 (`28444ee`) | 2026-07-15 | 1 | M5-1 퀵 런처 바(원본 docs/44 이식) — 도구 모음 아래 외부 프로그램 버튼 바(%path% 치환 ShellExecuteW·실패 상태바 격리)·settings.cfg 영속(launcherN=라벨\|exe\|인자·count로 부재/비움 구분)·VS Code 시드·보기 메뉴 토글·current_settings 헬퍼 | [2026-07-15](journal/2026-07-15.md) |
| `feat/m5-a11y` | 2026-07-15 | 2026-07-15 (`eba53fc`) | 2026-07-15 | 1 | M5-3 접근성·IME 마감·서명 결정 — UIA SelectionItem 실동작(WM_APP_UIA_SELECT→select_program 범위 방어)·구조 변경 이벤트(ChildrenInvalidated — M2-7 1차 한계 2건 해소)·리네임 인라인 IME 조합 창 배치(rename_edit_info — M3-2 α 해소)·서명=무서명 유지 확정(DR-3 갱신). 테스트 160 green | [2026-07-15](journal/2026-07-15.md) |
| `feat/m5-release-pipeline` | 2026-07-15 | 2026-07-15 (`a491af0`) | 2026-07-15 | 1 | M5-2 릴리스 파이프라인 — release.yml(버전 태그 push→test+예산 게이트[B2·B3]→`NexaDir2-<버전>-win-x64.exe`를 GitHub Release 자동 생성·첨부, workflow_dispatch=게이트+아티팩트만)·docs/18 §5 신설(§6 재번호). 릴리스 절차 = `git tag X.Y.Z && git push origin X.Y.Z` | [2026-07-15](journal/2026-07-15.md) |
| `feat/prefs-full` | 2026-07-14 | 2026-07-14 (`809b7ca`) | — | 2 | X-7 설정 창 전면 확장(원본 docs/40 §8) — VS Code식 상단 검색(제목 부분 일치)+좌측 카테고리(모양·글꼴·파일 목록·도크·언어)+우측 동적 편집기·설정 레지스트리(Entry 목록 = 렌더·검색 공용)·항목 종류 5종(테마/언어 순환·글꼴 Text·크기 Number·불리언 CheckBox). 목록 필터·도크도 저장 시 라이브 반영 | [2026-07-14](journal/2026-07-14.md) |
| `fix/ux-qa4` | 2026-07-14 | 2026-07-14 (`089e1a0`) | — | 2 | UX QA 4차 — 스플리터 두께 통일(3px — 파일/도크 좌우·가로선)·자석 스냅(50%·반대편 구분선 ±20px·Alt=해제, 원본 F9)·이름 복사(Copy as name)·도구모음 그룹화([⟳]\|[⚙]\|[👁·… 토글 — separator/checked 신설])·터미널 [→] 폴더 이동(한 몸 버튼·활성=accent) | [2026-07-14](journal/2026-07-14.md) |
| `fix/ux-qa3` | 2026-07-14 | 2026-07-14 (`9df021a`) | — | 2 | UX QA 3차 — 경로 복사 라벨 앱 언어 추종(MIIM_STRING)·확인/진행 창 비겹침(±110)·**원본 앱 아이콘 임베드**(icon.rs — 창·대화상자 공통)·도크 가로선 text_dim+스플리터 절반 두께·설정 메뉴/도구모음 연결(CMD_PREFS→WM_APP_PREFS)·스플리터 hover 커서(↔/↕) | [2026-07-14](journal/2026-07-14.md) |
| `feat/ux-batch3` | 2026-07-14 | 2026-07-14 (`ef68feb`) | — | 7 | 편의 UX 배치 3(기능 단위 슬라이스) — S1 경로 복사 **제자리 대체**(셸 라벨/위치/다국어 유지)는 main 직커밋·S2 대화상자 폰트 설정(dlg_font — 잘림 수정·진행 창 공유)·S3 **도크 레이아웃**(전폭 밴드·독립 dock_split — X-6)·S4 **터미널 마우스 프로토콜**(DECSET→SGR 전달 — X-5)·S5 **폰트 폴백 체인+크기**(IDWriteFontFallback — X-3)·S6 **설정 창**(Ctrl+,·즉시 영속) | [2026-07-14](journal/2026-07-14.md) |
| `fix/ux-qa2` | 2026-07-14 | 2026-07-14 (`59fe10d`) | — | 2 | UX QA 2차 — 탭 드래그 스냅(중간점 통과 시에만)·경로 복사 표시 순서+셸 copyaspath 숨김·대체(VerbReplacement)·**dialog.rs 신설**(임의 버튼 모달 User Control·전송 진행 창[직접 페인트 바·취소])·충돌 확인 원본 4버튼(덮어쓰기/모두/건너뛰기/취소) | [2026-07-14](journal/2026-07-14.md) |
| `fix/ux-qa1` | 2026-07-14 | 2026-07-14 (`2fdd31d`) | — | 2 | 편의 UX 실기 QA 7건 — 방문≠확장(직전 루트 자동 등재 제거)·탭 빈 공간 더블클릭=새 탭·탭 드래그 잔존(MouseUp 미전달) 수정·파일 실행(더블클릭·Enter·Alt+↓ 신설 — ShellExecuteW)·교차 폴더 복사(copy/cut 가로채기·고유 "경로 복사")·충돌 확인 1회화(모두 덮어쓰기/건너뛰기/중단) | [2026-07-14](journal/2026-07-14.md) |
| `feat/ux-convenience` | 2026-07-14 | 2026-07-14 (`71243d6`) | — | 5 | 편의 UX 배치 1(사용자 지시) — ① 경로바 자동완성(PATH-SUG: 하위 폴더 제안·↑/↓ 순환+입력 복원·클릭 이동)+환경변수 해석(%VAR%·$env:) ② 탭 UX(드래그 재정렬·잠금 🔒·우클릭 메뉴[잠금/복제/닫기]·세션 영속) ③ 전송 충돌 일괄 적용(모두 예/건너뛰기 — MessageBox 2단·comctl32 비의존). 테스트 159 green | [2026-07-14](journal/2026-07-14.md) |
| `fix/cfg-filenames` | 2026-07-14 | 2026-07-14 (`b364368`) | — | 2 | 사용자 지시 — 영속 파일명 정비: settings.cfg(설정 전반)·session.cfg(패널·탭·펼침 세션) + 구 .txt 마이그레이션(load_migrated 폴백→저장 시 purge_legacy 정리·실기 왕복 검증) | [2026-07-14](journal/2026-07-14.md) |
| `feat/f18-expanded` | 2026-07-14 | 2026-07-14 (`1612e8c`) | — | 2 | X-4 — 펼침 상태 유지(원본 F18 이식): 탭별 영속 Expanded 집합(BTreeMap 부모 우선)·경계 동기(펼침=등재·접힘=말소·비가시 보존=재펼침 복원)·리네임 접두사 치환·세션 영속(panelN.expM·상한 200/탭). "B000 진입 후 복귀 시 A000 펼침 소실" 해소·테스트 153 green | [2026-07-14](journal/2026-07-14.md) |
| `fix/nav-freeze-watcher` | 2026-07-14 | 2026-07-14 (`5f6b441`) | — | 2 | **이동 프리즈 진범 해소** — DirWatcher drop의 CloseHandle이 동기 RDCW 완료(=이전 폴더 변경 발생)까지 UI 블로킹(비 OVERLAPPED 파일 객체 잠금 직렬화). OVERLAPPED+중지 이벤트로 재구현(drop=SetEvent 논블로킹·복제 핸들). 자동 재현 12,009ms→4~6ms 실측 + 터미널 캐럿 깜빡임(GetCaretBlinkTime·입력 위상 리셋) | [2026-07-14](journal/2026-07-14.md) |
| `feat/m4-term-select` | 2026-07-14 | 2026-07-14 (`8c8665c`) | — | 2 | 터미널 상호작용(QA/요청 5건) — 셀 단위 렌더(폴백 글꼴 전진폭 밀림 해소·단일 글리프 캐시)·마우스 드래그 선택(반전 하이라이트·엣지 자동 스크롤 60ms)·스크롤백 보기(휠 3줄/노치·보던 위치 고정·입력 시 스냅)·Ctrl+C(선택 복사/인터럽트)·Ctrl+V 붙여넣기·settings term_font(DWrite 해석·Consolas 폴백) | [2026-07-14](journal/2026-07-14.md) |
| `fix/m4-qa-batch2` | 2026-07-14 | 2026-07-14 (`55bdcc2`) | — | 2 | 실기 QA 5건 — **프리즈 근본 해소**(파일별 아이콘 SHGetFileInfoW를 워커 스레드로 — Downloads/Documents의 MotW·OneDrive 블로킹)·편집 필드 Ctrl+C/X/V(CF_UNICODETEXT·EditState 선택 복사/잘라내기/붙여넣기)·느린 재클릭 리네임 더블클릭 시간 지연(더블클릭=열기 우선)·휠 hover 라우팅(wheel_target)·바로가기 .lnk 숨김(NeverShowExt·리네임 시 복원) | [2026-07-14](journal/2026-07-14.md) |
| `fix/term-caret-color` | 2026-07-14 | 2026-07-14 (`d0e28d2`) | — | 1 | 사용자 지시 — 터미널 캐럿 밝은 회색(0xCCCCCC) 고정: 터미널 배경은 테마 무관 Campbell 다크라 theme.text가 라이트 테마에서 비가시 | [2026-07-14](journal/2026-07-14.md) |
| `fix/m4-term-qa` | 2026-07-14 | 2026-07-14 (`149394b`) | — | 2 | 터미널 실기 QA 2건 — 세로바 캐럿(1px·DPI — Windows Terminal bar 동일)·Backspace=DEL 교차 매핑(0x08=Ctrl+Backspace 단어 삭제 해석 → 입력 전체 삭제 결함 수정, 원본 TerminalView 규약) + X-3 터미널 설정 백로그 등록 | [2026-07-14](journal/2026-07-14.md) |
| `feat/m4-terminal` | 2026-07-14 | 2026-07-14 (`acbbdae`) | — | 4 | M4-3 — ConPTY 터미널(원본 VtScreen.cs·ConPtySession.cs 이식): nexa-term rlib(VT 파서·SGR 3계열·CSI·DECSTBM·스크롤백 800·전각·테스트 9)·ConPTY 세션(pwsh→cmd 폴백·UTF-8 경계 읽기 스레드·EXIT 통지·세대 가드)·도크 [터미널] 종류(Consolas 모노 그리드·런 병합·캐럿·클릭 포커스·VT 키·아무 키 재시작·리사이즈 동기). 테스트 148 green → **M4 전 항목 구현 완료** | [2026-07-14](journal/2026-07-14.md) |
| `feat/m4-preview` | 2026-07-13 | 2026-07-14 (`30bd7b6`) | — | 4 | M4-2 — 내장 미리보기(DR-7 — 플러그인 아님): 도크 [정보\|미리보기] 스트립·텍스트(16KB·이진 판정)·이미지(WIC Fant·비율 유지·캐시 8건·CoCreateInstance 지연=임포트 무변)·draw_image 프리미티브·독립 예제(examples/preview_image.rs — 실기 jpg 확인) | [2026-07-13](journal/2026-07-13.md) |
| `feat/m4-dock` | 2026-07-13 | 2026-07-13 (`b9942e7`) | — | 3 | M4-1 — 하단 도크(원본 BottomDockView·DockInfo·도크 대원칙): InfoDock 위젯·정보 뷰(다중=개수/단일=속성/없음=현재 폴더)·Ctrl+` 토글·높이 드래그(비율 0.15~0.5)·settings 영속(dock·dock_ratio). 테스트 140 green·실기 QA 대기 | [2026-07-13](journal/2026-07-13.md) |
| `feat/m3-watcher` | 2026-07-13 | 2026-07-13 (`8ba60f6`) | — | 5 | M3-6 — watcher(원본 FolderWatcher.cs): ReadDirectoryChangesW 비재귀·300ms 디바운스·무간섭 재로드(펼침·선택·캐럿·스크롤 보존)·편집/전송 중 지연·세대 가드. 실기 생성/삭제 자동 반영. + 폴더 이동 펼침 이월 + 드롭다운 첫 프레임 무효 영역 수정. 테스트 138 green → **M3 전 항목 구현 완료** | [2026-07-13](journal/2026-07-13.md) |
| `fix/ux-reload-editor` | 2026-07-13 | 2026-07-13 (`511619b`) | — | 3 | 실기 QA 6건 — 재로드 무간섭 갱신(펼침·선택·캐럿·스크롤 보존 = M3-6 선행·통합 테스트)·편집기 캐럿 모델(edit.rs EditState 공용 — 방향키/Ctrl+A/클릭 배치/세로바 캐럿/기본 선택[경로바 전체·리네임 이름부])·경로바 끝 정렬 오버플로(편집 캐럿 가시·브레드크럼 `…`). 테스트 135 green | [2026-07-13](journal/2026-07-13.md) |
| `fix/m3-5-dnd-conflicts` | 2026-07-13 | 2026-07-13 (`6a2dc70`) | — | 2 | M3-5 QA 결함 2건 — 발신 CF_HDROP 자체 IDataObject(SHCreateDataObject 미렌더링 → 앱→탐색기/패널 간 드래그 해소·교차폴더 지원)·충돌 순차 확인창(예/아니오/취소 — M3-1 α 해소) | [2026-07-13](journal/2026-07-13.md) |
| `feat/m3-clipboard-dnd` | 2026-07-13 | 2026-07-13 (`2e93db2`) | — | 6 | M3-5 — OS 클립보드 단일 출처(CF_HDROP·Preferred DropEffect — 내부 클립보드 제거·탐색기↔앱 Ctrl+C/X/V·왕복 테스트)·OLE DnD 수신(IDropTarget — 볼륨별 기본+Ctrl/Shift·최적화 이동 NONE)·발신(SHCreateDataObject+DoDragDrop — 원본 미삭제 안전 방향). B3 무변·DnD 실기 QA 대기 | [2026-07-13](journal/2026-07-13.md) |
| `fix/toolbar-refresh-only` | 2026-07-13 | 2026-07-13 (`4d4281b`) | — | 1 | 사용자 지시 — 전역 도구 모음 이전/다음 오동작 보고 → 네비 버튼 제거(패널별 네비 바 전담)·⟳만 유지 | [2026-07-13](journal/2026-07-13.md) |
| `feat/m3-shellmenu` | 2026-07-13 | 2026-07-13 (`e28fed3`) | — | 5 | M3-4 — 셸 컨텍스트 메뉴(ADR-0003, 원본 ADR-0005·ShellContextMenu.cs 계승): 행 우클릭=IContextMenu 셸 메뉴·빈 영역=배경 메뉴(CreateViewObject)·고유 병합 0x8000+(완전 삭제·붙여넣기·Undo/Redo)·delete/rename/paste 가로채기·Apps/Shift+F10·자기 wndproc 포워딩(comctl32 불요). 테스트 127 green·exe 0.60MB·B3 무변·셸 메뉴 상호작용 실기 QA 대기 | [2026-07-13](journal/2026-07-13.md) |
| `feat/m3-undo` | 2026-07-13 | 2026-07-13 (`8bf3da3`) | — | 4 | M3-3 — Undo/Redo(원본 OperationHistory.cs·RecycleBin.cs): nexa-ops `history` 모듈(ReversibleOp·스택 2개·실패 소실=무결성·연산 4종)·`OpError` 구조화·앱 배선(push 3곳·Ctrl+Z/Y·Ctrl+Shift+Z·편집 메뉴)·휴지통 복원(셸 undelete·STRRET 직접 파싱). 실 휴지통 왕복 통합 테스트 통과·exe 0.58MB·RSS 25.51MB·B3(ole32) | [2026-07-13](journal/2026-07-13.md) |
| `feat/m3-fileops` | 2026-07-13 | 2026-07-13 (`f86c021`) | — | 3 | M3-2 — 삭제(Del=휴지통 FOF_ALLOWUNDO·Shift+Del=완전+확인창)·F2 인라인 이름변경(rows 오버레이 편집기)·새 폴더/파일(생성→즉시 리네임). nexa-ops delete/rename/create_new. 실기 4종·테스트 116 green | [2026-07-13](journal/2026-07-13.md) |
| `feat/m3-ops-transfer` | 2026-07-13 | 2026-07-13 (`844b1e3`) | 2026-07-13 | 3 | M3-1 — `nexa-ops` rlib 신설(원본 docs/33·FileOps 이식): transfer 단일 경로(같은 폴더 규칙·충돌 순차·4MB 진행·취소·개별 격리·fast path) + Ctrl+C/X/V·워커·세대 가드·Esc 취소·양쪽 재로드. 실기 순번 복제/무동작·테스트 113 green | [2026-07-13](journal/2026-07-13.md) |
| `feat/m2-ime-uia` | 2026-07-12 | 2026-07-13 (`474515f`) | 2026-07-13 | 3 | M2-7 — IME 1차(조합 창을 경로바 편집 캐럿에 배치·WM_IME_*)·UIA 1차(스냅샷 프로바이더: List/ListItem·SelectionItem·FocusChanged). .NET UIA 클라이언트 실기 검증·테스트 105 green → **M2 완료 `0.3.0`** | [2026-07-13](journal/2026-07-13.md) |
| `feat/m2-resident` | 2026-07-12 | 2026-07-12 (`b848904`) | 2026-07-12 | 3 | M2-8 — 상주 규율: 유휴 60s(자니터)·최소화 시 트림(DW 백버퍼+레이아웃 캐시·HICON 해제·작업집합 반납)·지연 재적재·유휴 백그라운드 0%. 실측 활성 26.9→트림 0.21~2.9MB 게이트 통과·테스트 105 green | [2026-07-12](journal/2026-07-12.md) |
| `feat/m2-i18n` | 2026-07-12 | 2026-07-12 (`7a50692`) | 2026-07-12 | 3 | M2-6 — i18n: properties `.lang`(crate 0)·내장 en/ko+`data\lang\` 키 단위 오버라이드·폴백 체인·동적 전환(라디오·재시작 불요)·system=OS 추종·`lang` 영속. 테스트 104 green·실기 ko/en 타이틀 검증 | [2026-07-12](journal/2026-07-12.md) |
| `feat/m2-persistence` | 2026-07-12 | 2026-07-12 (`e04e9d4`) | 2026-07-12 | 2 | M2-5 — 설정/세션 영속: exe 옆 `data\` key=value 텍스트(crate 0)·원자 쓰기·기동 로드/종료 저장. 실기 재실행 복원 검증·테스트 102 green | [2026-07-12](journal/2026-07-12.md) |
| `feat/m2-ui-feedback` | 2026-07-12 | 2026-07-12 (`da5272d`) | 2026-07-12 | 1 | 사용자 지시 — 네비 화살표 15 DIP 글리프·편집 모드 4변 테두리·M2-6 동적 i18n 설계 확정 | [2026-07-12](journal/2026-07-12.md) |
| `feat/m2-navbar-flush` | 2026-07-12 | 2026-07-12 (`cf9df03`) | 2026-07-12 | 1 | 사용자 지시 — 경로바를 네비 버튼에 밀착(4px 미도색 틈 제거) | [2026-07-12](journal/2026-07-12.md) |
| `feat/m2-navbar-polish` | 2026-07-12 | 2026-07-12 (`2608f68`) | 2026-07-12 | 2 | 사용자 지시 — 네비 버튼 연속 배치(고정 폭 모드·중앙 정렬)·경로바 4px 여유 | [2026-07-12](journal/2026-07-12.md) |
| `feat/m2-panel-navbar` | 2026-07-12 | 2026-07-12 (`450f509`) | 2026-07-12 | 2 | 사용자 지시 — 패널별 [←][→][↑]를 경로바 옆에(원본 §2 네비 바), 해당 패널 활성 탭 대상. 테스트 97 green | [2026-07-12](journal/2026-07-12.md) |
| `feat/m2-theme` | 2026-07-12 | 2026-07-12 (`9891174`) | 2026-07-12 | 2 | M2-4 — 테마 시스템: 시스템(OS 추종·SETTINGCHANGE)/라이트/다크·메뉴 라디오·F6 순환·DWM 다크 타이틀바. 픽셀 검증·B3 dwmapi/advapi32 | [2026-07-12](journal/2026-07-12.md) |
| `feat/m2-chrome` | 2026-07-12 | 2026-07-12 (`3a1fcf5`) | 2026-07-12 | 3 | M2-3 — 크롬 3종: MenuBar(드롭다운 오버레이·체크 동기)·Toolbar(네비)·StatusBar + CMD_* 명령 단일화. 테스트 96 green·실기 숨김 토글/↑ 확인 | [2026-07-12](journal/2026-07-12.md) |
| `feat/m2-panels` | 2026-07-12 | 2026-07-12 (`bddd5ef`) | 2026-07-12 | 3 | M2-2 — 듀얼 패널+패널별 탭: Panel 추출(탭=독립 뷰+히스토리)·TabBar·스플리터·활성 패널(Tab)·Ctrl+T/W/Ctrl+Tab. 테스트 90 green·실기 전 시나리오 확인·듀얼 RSS 25.1MB | [2026-07-12](journal/2026-07-12.md) |
| `feat/m2-pathbar` | 2026-07-12 | 2026-07-12 (`3aa46fc`) | 2026-07-12 | 3 | M2-1 α — 계층 경로 바(원본 docs/27): 브레드크럼(클릭 이동·현재 비활성·hover)·편집 모드(우클릭·Enter/Esc)·네비 비종속·바+리스트 레이아웃. 테스트 83 green·실기 왕복 확인 | [2026-07-12](journal/2026-07-12.md) |
| `feat/m1-gate` | 2026-07-12 | 2026-07-12 (`0c716f5`) | 2026-07-12 | 2 | M1-9 — 게이트 실측: 첫 렌더 계측 추가, 100k 115ms(<150)·스크롤 2,098µs(60fps)·B1 10k 27.87MB 전부 통과 → **M1 완료 `0.2.0`** | [2026-07-12](journal/2026-07-12.md) |
| `feat/m1-navigation` | 2026-07-12 | 2026-07-12 (`415208f`) | 2026-07-12 | 3 | M1-8 — 네비게이션: History(push 절단·replace)·더블클릭/Enter 진입·Alt+화살표/X버튼·Ctrl+H/. 필터 토글·replace_source(정렬 유지). 테스트 80 green·실기 왕복 확인 | [2026-07-12](journal/2026-07-12.md) |
| `feat/m1-icons` | 2026-07-12 | 2026-07-12 (`b8833a1`) | 2026-07-12 | 3 | M1-7 — 셸 아이콘: icon_key·IconStore(LRU 256+dedupe 큐)·속도 제한 로딩(80ms×4, 원본 A-4)·SHGetFileInfoW. RSS 27.95MB(⚠ 여유 2MB — M2-8 감시). 테스트 74 green | [2026-07-12](journal/2026-07-12.md) |
| `feat/m1-keyboard` | 2026-07-12 | 2026-07-12 (`14bfe86`) | 2026-07-12 | 3 | M1-6 — 캐럿 키보드 네비(이동+선택+스크롤 추적·Shift 범위·Ctrl·Space 토글·→자식/←부모)·타입어헤드(버퍼/cycle/HUD·코어 find_prefix C). 테스트 69 green·Shift+End 실기 "선택 61" | [2026-07-12](journal/2026-07-12.md) |
| `feat/m1-select` | 2026-07-12 | 2026-07-12 (`a37e01e`) | 2026-07-12 | 3 | M1-5 ★ — 인라인 펼침/선택 분리·교차폴더 다중 선택(Ctrl/Shift/Ctrl+A)·러버밴드·캐럿·sel_bg. 테스트 60 green·Ctrl+A 실기 확인·벤치 1,540µs | [2026-07-12](journal/2026-07-12.md) |
| `feat/m1-columns` | 2026-07-12 | 2026-07-12 (`6e4ba96`) | 2026-07-12 | 3 | M1-4 — 컬럼 시스템(원본 docs/23): 5컬럼·정렬 3상태+Shift 다중열·드래그 리사이즈·가로 스크롤·말줄임 트리밍·TZ 날짜. 테스트 55 green·벤치 1,813µs·B3 사전 통과 | [2026-07-12](journal/2026-07-12.md) |
| `feat/m1-virtual-list` | 2026-07-12 | 2026-07-12 (`baa3b3f`) | 2026-07-12 | 3 | M1-3 — 가상화 파일 리스트 초안: nexa-tree 평면 스트림 배선(TreeSource·들여쓰기·마커·클릭 토글)·GDI 경로 제거·DW 레이아웃 캐시(벤치 1,673µs). 테스트 48 green·실기 실측 | [2026-07-12](journal/2026-07-12.md) |
| `feat/m1-adr0002-render` | 2026-07-12 | 2026-07-12 (`e0daf56`) | 2026-07-12 | 2 | M1-2 — ADR-0002 확정: DirectWrite GDI interop 채택(벤치 −28%·RSS +4.1MB 예산 내), dw.rs 백엔드·F2/F3 비교 하네스·기본 백엔드 전환 | [2026-07-12](journal/2026-07-12.md) |
| `feat/m1-gui` | 2026-07-11 | 2026-07-11 (`c20ddde`) | 2026-07-11 | 3 | M1-1 — `nexa-gui` 크레이트 분리: 플랫폼 중립 위젯 trait·무효화(rect 병합)·입력 이벤트·테마 토큰(원본 docs/39 차용)·`VirtualRows` + nexa-app 재배선(`gdi.rs` DrawCtx 백엔드). 테스트 43 green·실기 확인 | [2026-07-11](journal/2026-07-11.md) |
| `feat/m0-render-spike` | 2026-07-11 | 2026-07-11 (`cc7e7ed`) | 2026-07-11 | 3 | M0-7 — GDI 렌더 스파이크: 더블 버퍼·합성 100k행 가시 영역만·휠/키 스크롤·DPI (windows 타깃 check·clippy green) + git -C 권한 병합 | [2026-07-11](journal/2026-07-11.md) |
| `feat/m0-scaffold` | 2026-07-11 | 2026-07-11 (`e1a2e7f`) | 2026-07-11 | 11 | M0-1~6 — 워크스페이스·코어 3크레이트 이식(테스트 green)·Win32 창 스켈레톤(windows 타깃 check green)·CI(예산 게이트) + 권한 복구 | [2026-07-11](journal/2026-07-11.md) |
| `docs/foundation` | 2026-07-11 | 2026-07-11 (`d2727b5`) | 2026-07-11 | 6 | 설계 문서 세트(비전·아키텍처·ADR-0001·DR·로드맵·TODO·운영 문서) + 권한 정리 | [2026-07-11](journal/2026-07-11.md) |

---

## feat/prefs-full

- **생성**: 2026-07-14 (분기: main `750ef22`). **커밋**: `91eccc1`(설정 레지스트리·검색·카테고리·동적 렌더·목록/도크 라이브 반영) → `b8650d2`(docs). 병합(`809b7ca`): 2026-07-14. 삭제: CI green 확인 후.
- **검증**: 164 green · clippy 0 · fmt · B2 0.85MB · B3 무변(EDIT/BUTTON/STATIC=user32) · 릴리스 빌드. **실기 QA 대기**: Ctrl+, → 좌측 카테고리 전환·상단 검색어로 항목 필터·글꼴/크기 EDIT·숨김/닷파일/도크 체크박스·저장 시 즉시 반영 및 settings.cfg 갱신.
- **α 한계**: 재시작 필요 설정 확인창(원본 PREF-9)·항목 확대(터미널 wrap/cols[X-3]·글꼴 다중 슬롯)는 해당 기능 도입 시 레지스트리에 추가.

## fix/ux-qa4

- **생성**: 2026-07-14 (분기: main `2ccc70f`). **커밋**: `aa78c54`(QA 5건 일괄) → `231aa2f`(journal). 병합(`089e1a0`): 2026-07-14. 삭제: CI green 확인 후.
- **검증**: 164 green · clippy 0 · fmt · B2 0.86MB · B3 무변 · 릴리스 빌드. **실기 QA 대기**: 세 스플리터 동일 두께·50%/반대편 스냅(Alt 해제)·우클릭 "이름 복사"·도구모음 3그룹(토글 accent 줄·메뉴 동기)·도크 터미널 라벨 옆 [→](활성=accent) 클릭 시 현재 폴더 cd.
- **후속(사용자 요청 접수)**: **설정 창 전면 확장** — 원본 docs/40 §8(VS Code식 좌측 검색+카테고리 트리+영속 설정 전부 수록) 구조를 신규 앱 옵션 전체에 이식(X-7 승격).

## fix/ux-qa3

- **생성**: 2026-07-14 (분기: main `aa066e7`). **커밋**: `a11a3e7`(QA 6건 일괄) → `e059ee4`(journal). 병합(`9df021a`): 2026-07-14. 삭제: CI green 확인 후.
- **검증**: 164 green · clippy 0 · fmt · B2 0.84MB · B3 무변(아이콘=user32 CreateIconFromResourceEx) · 릴리스 빌드. **실기 QA 대기**: 언어 en 전환 후 우클릭 "Copy as path" 표기·확인창과 진행 창 동시 가시·타이틀바/Alt+Tab 원본 아이콘·다크에서 도크 가로선 식별·파일 메뉴/⚙ 버튼으로 설정 열기·스플리터 위 커서 변화.
- **α 한계**: exe 파일 아이콘(탐색기 표시 — 리소스 섹션·빌드 도구=DR-8 원장 검토 후속)·라이트 아이콘 판(nexa-dir-light.ico) 테마 연동 후속.

## feat/ux-batch3

- **생성**: 2026-07-14 (분기: main `bfe08fd` — S1 직커밋 직후). **커밋**: `a270be1`(S2 대화상자 폰트) → `2e1aeb1`(S3 도크 레이아웃) → `6bde8a1`(S4 터미널 마우스) → `8fa7c0c`(S5 폰트 체인+크기) → `81c2f4f`(S6 설정 창) → `777623e`(docs). 병합(`ef68feb`): 2026-07-14. 삭제: CI green 확인 후.
- **검증**: 164 green(nexa-term DECSET·config 왕복 확장 포함) · clippy 0 · fmt · B2 0.82MB · **B3 무변**(설정/대화상자=user32/gdi32·폴백 체인=dwrite) · 릴리스 빌드. **실기 QA 대기**: ① 우클릭 "경로 복사"가 윈도우 기본 위치·이름으로 1개만 ② 충돌 확인창 폰트·전체 표시(설정에서 dlg_font 변경 반영) ③ 도크 좌/우 스플리터 독립 드래그(파일과 별개)·재시작 복원 ④ Zellij에서 탭 클릭 이동(Shift=로컬 선택) ⑤ `term_font=D2Coding, JetBrainsMono Nerd Font` 후 Nerd 아이콘 표시 ⑥ Ctrl+, 설정 창 저장→적용·settings.cfg 즉시 갱신.
- **α 한계**: 설정 창 검색/카테고리(X-7)·터미널 wrap/cols(X-3 잔여)·대화상자 Esc/Tab 네비·라이트 고정 확인/진행 창·레거시(비SGR) 마우스 인코딩.

## fix/ux-qa2

- **생성**: 2026-07-14 (분기: main `f1fa849`). **커밋**: `095bf67`(탭 스냅·경로복사 순서/대체·dialog.rs·4버튼 확인창·진행 창) → `4f757ff`(journal). 병합(`59fe10d`): 2026-07-14. 삭제: CI green 확인 후.
- **검증**: 159 green · clippy 0 · fmt · B2 0.79MB · **B3 무변**(dialog.rs=user32/gdi32만 — comctl32 비의존 유지) · 릴리스 빌드. **실기 QA 대기**: 탭 드래그 스냅감·경로 복사 순서=화면 순서·셸 메뉴에 "경로 복사" 1개만 표시·충돌 2건+ 시 4버튼 확인창(모두 덮어쓰기=이후 무확인/덮어쓰기·건너뛰기=파일별)·대용량 전송 시 진행 창(바·취소).
- **α 한계**: 대화상자 키보드 네비(Esc/Tab — IsDialogMessage의 IDCANCEL=2 충돌로 미채용, 버튼 클릭 사용)·진행 창 라이트 고정(다크 테마 후속)·탭 패널 간 이동 드래그 후속.

## fix/ux-qa1

- **생성**: 2026-07-14 (분기: main `5c78aa9`). **커밋**: `4f0c943`(QA 7건 일괄) → `da12e63`(journal). 병합(`2fdd31d`): 2026-07-14. 삭제: CI green 확인 후.
- **검증**: `cargo test` 159 green · clippy 0 · fmt · B3 무변(ShellExecuteW=shell32 기등재) · 릴리스 빌드. **실기 QA 대기**: AppData 진입→복귀 시 비확장·탭 빈 공간 더블클릭·[+] 후 드래그 잔존 없음·파일 더블클릭/Enter/Alt+↓ 실행·교차 레벨 2파일 우클릭 복사/경로 복사→붙여넣기 모두 포함·충돌 시 1회 확인.
- **α 한계**: 충돌 파일별 개별 선택(예/모두 예 4버튼 — user32 커스텀 대화상자)·Alt=POSIX 경로 복사·VerbReplacement(셸 항목 제자리 대체) = 후속.

## feat/ux-convenience

- **생성**: 2026-07-14 (분기: main `aa0339b`). **커밋**: `4740275`(① 경로바 자동완성+환경변수 — pathinput.rs·PathBar 팝업) → clippy 정리 → `b7abae5`(② 탭 UX — TabBar Move/Context·Panel move/dup/lock·세션 locked) → `d1901dc`(③ 전송 충돌 일괄 적용) → `5ef0a31`(docs). 병합(`71243d6`): 2026-07-14. 삭제: CI green 확인 후.
- **검증**: `cargo test` **159 green**(신규 — pathinput expand/suggest·pathbar 제안 순환/복원/클릭·탭 move/lock/dup·세션 locked 왕복) · clippy 0 · fmt · B2 0.78MB · **B3 무변**(팝업 메뉴=user32·TaskDialog/comctl32 비의존 유지) · 릴리스 빌드. **실기 QA 대기**: 경로바 우클릭→타이핑 시 제안 팝업·↑/↓·클릭 이동·`%USERPROFILE%` 입력 이동 · 탭 드래그 재정렬·우클릭 메뉴(잠금 후 × 사라짐·Ctrl+W 거부·재시작 복원)·복제 · 파일 충돌 2건+ 전송 시 "남은 충돌 동일 적용" 확인창.
- **α 한계(후속 배치)**: ④ 설정 창(원본 PreferencesWindow — VS Code식) · 드라이브 제안("C:" 단계) · 탭 패널 간 이동/Ctrl 복제 드래그·고정(pin)·멀티라인 · 전송 진행 창(현재 타이틀 %).

## feat/f18-expanded

- **생성**: 2026-07-14 (분기: main `7bd2ed2` — 0.5.0 직후). **커밋**: `3c92ad4`(Tab.expanded 집합·경계 동기·리네임 치환·세션 영속·테스트 4종) → `784169f`(docs 현행화). 병합(`1612e8c`): 2026-07-14. 삭제: CI green 확인 후.
- **검증**: `cargo test` **153 green**(신규 — 형제 펼침 유지[진입·복귀]+접힘 유지·세션 시드 restore·config panelN.expM 왕복[빈 목록 생략 인덱스 정렬]) · clippy 0 · fmt · B2 0.73MB · B3 무변 · 릴리스 빌드. **실기 QA 통과(사용자 07-14)**: "폴더 상태가 유지되는 것은 확인했어".
- **α 한계**: 삭제된 폴더의 잔존 엔트리는 미정리(expand_path 무시로 무해 — 세션 저장 시 동기로 자연 수렴) · 탭 복제(new_tab)는 빈 집합에서 시작(원본 동일).

## fix/nav-freeze-watcher

- **생성**: 2026-07-14 (분기: main `59ed7aa`). **커밋**: `fcbc562`(watcher OVERLAPPED 재구현+캐럿 깜빡임) → `e00143a`(journal 기록). 병합(`5f6b441`): 2026-07-14. 삭제: CI green 확인 후.
- **검증**: **자동 재현 하네스**(PostMessage 키 주입+SendMessageTimeout 블로킹 실측) — 수정 전 홈→Documents **12,009ms** 무응답 → 수정 후 **6ms** · Documents→홈(조용한 OneDrive watcher 해제=최악) **4ms** · watcher 자동 반영 정상(생성/삭제 70→69) · 151 green · clippy 0 · B2 0.73MB · B3 무변(CancelIoEx/DuplicateHandle=kernel32).
- **교훈**: 비 OVERLAPPED 핸들의 CloseHandle은 파일 객체 잠금 직렬화로 **대기 중인 동기 I/O 완료까지 블로킹** — "핸들 닫기=중지 신호" 패턴은 디렉터리 감시에 부적합. 원본 .NET FileSystemWatcher는 내부가 overlapped라 이 문제가 없었음(이식 시 단순화가 결함 유입).

## feat/m4-term-select

- **생성**: 2026-07-14 (분기: main `b9f032c`). **커밋**: `3782477`(셀 정렬·선택·스크롤백·클립보드·글꼴 설정) → `2dcb73a`(docs — journal·X-3 현행화). 병합(`8c8665c`): 2026-07-14. 삭제: CI green 확인 후.
- **검증**: `cargo test` 151 green(config term_font 왕복 포함) · clippy 0 · fmt · B2 0.73MB · B3 무변(GetCursorPos/CoInitializeEx=기존 화이트리스트) · 릴리스 빌드. **실기 QA 대기**: ls 한글 파일명 열 정렬·드래그 선택+반전 표시·그리드 밖 자동 스크롤·휠 스크롤백(터미널 위)·Ctrl+C 복사/Ctrl+V 붙여넣기·data\settings.txt `term_font=D2Coding` 후 재시작.
- **α 한계**: 스크롤백 800 상한 도달 시 선택/보기 절대 인덱스 드리프트 · 더블클릭 단어 선택 없음 · 명시 폴백 체인(1→2순위 글꼴)·글꼴 크기 설정 = X-3 잔여.

## fix/m4-qa-batch2

- **생성**: 2026-07-14 (분기: main `6b8bd58`). **커밋**: `f390606`(QA 5건 일괄 — 아이콘 비동기·편집 클립보드·리네임 지연·휠 라우팅·lnk 숨김) → `6fa6982`(journal 기록). 병합(`55bdcc2`): 2026-07-14. 삭제: CI green 확인 후.
- **검증**: `cargo test` **151 green**(신규 3 — in-flight 중복 방지·insert 같은 키 대체 반환·display_name lnk/UTF-8 경계) · clippy 0 · fmt · 릴리스 빌드. **프리즈 재현 조건**: Downloads의 57.8MB 다운로드 exe(MotW) 아이콘 추출 → Defender 검사 수십 초 — 사용자 실기 재확인 대기(Downloads/Documents 진입·탭 추가 시 무응답 없어야 함).
- **α 한계**: 아이콘 실패 키는 재페인트마다 재시도(원 동작 유지) · 붙여넣기는 한 줄로 정제 · lnk 숨김은 이름 컬럼만(종류/확장자 컬럼은 유지).

## fix/m4-term-qa

- **생성**: 2026-07-14 (분기: main `1dea2c3`). **커밋**: `a0b4acf`(세로바 캐럿·Backspace=DEL 교차 매핑) → `cc54371`(docs — journal QA 기록·TODO X-3 터미널 설정 백로그). 병합(`149394b`): 2026-07-14. 삭제: CI green 확인 후.
- **검증**: `cargo test` 148 green · clippy 0 · fmt · 릴리스 빌드. **결함 원인**: WM_CHAR 0x08 직송 → ConPTY(PSReadLine)가 Ctrl+Backspace(단어 삭제)로 해석 — 원본 TerminalView.OnKeyDown(:636) 규약(Backspace→0x7F·Ctrl+Backspace→0x08)으로 정정. 실기 QA 대기(1글자 삭제·세로바 캐럿).
- **후속**: X-3 — 원본 터미널 설정 패리티(NoWrap 기본 true·MaxColumns 240 가로 스크롤·ConsoleFamily/Size) 미구현 확인·백로그 등재.

## feat/m4-terminal

- **생성**: 2026-07-14 (분기: main `17421cd` — ADR-0004 직후). **커밋**: `db266f0`(S1 — nexa-term 크레이트·VtScreen 이식·테스트 9) → `817eeb4`(S2 — conpty.rs ConPTY 세션·읽기/종료 스레드·폴백) → `4e96a0b`(S3 — 도크 통합·term_paint 셀 그리드·키 라우팅·재시작) → `d8c73fc`(docs 현행화). 병합(`acbbdae`): 2026-07-14. 삭제: CI green 확인 후.
- **검증**: Windows 실기 — `cargo test` 워크스페이스 **148 green**(nexa-term 9: CUP/SGR/줄바꿈 스크롤/ED/EL/ECH/ICH·DCH/DECSTBM/전각/리사이즈) · clippy 0 · fmt · B2 **0.69MB** · **B3 통과(임포트 무변** — Console/Pipes 피처는 kernel32 계열) · 릴리스 재기동 스모크(procs=1). **실기 QA 대기**: 도크 [터미널] 전환→셸 프롬프트·dir/ls 출력·한글 입출력·화살표(히스토리)·도크 리사이즈·exit 후 아무 키 재시작.
- **α 한계**: 스크롤백 휠 스크롤 표시·마우스 선택 복사·bold 별도 폰트 렌더 미구현(속성만 보존) · faint=배경 블렌드 근사 · 셸 종료 코드 미표시(안내 문구만).

- **생성**: 2026-07-13 (분기: main `d051cec`). **커밋**: `32175a4`(S1/S2 — clipboard.rs·CF_HDROP+DropEffect 읽기/쓰기·내부 클립보드 제거·왕복 테스트) → `8c06960`(S3 — dnd.rs IDropTarget 수신·B-14dnd 연산 결정·최적화 이동·훅 격리) → `549b2f9`(S4 — IDropSource+SHCreateDataObject 발신·drag_press 임계 감지) → `6627715`·`b963fde`(docs 현행화). 병합(`2e93db2`): 2026-07-13. 삭제: CI green 확인 후.
- **검증**: Windows 실기 — `cargo test` green · clippy 0 · fmt · **실 클립보드 왕복 통합 테스트**(비ASCII·cut effect, `#[ignore]` 수동) 통과 · B2 무변 · **B3 통과(임포트 무변** — DataExchange/Memory/StructuredStorage 피처는 컴파일 게이트) · 릴리스 기동 스모크. **DnD 실기 QA 대기**: 탐색기→앱 드롭(Ctrl/Shift 커서)·앱→탐색기 드래그·내부 패널 간·탐색기↔앱 Ctrl+C/V 왕복(모달 드래그 루프 — 자동화 불가).
- **α 한계**: spring-load 폴더 hover 진입(원본 B-15h)·드롭 대상 하이라이트·Performed DropEffect SetData 미구현 · 발신 이동은 원본 미삭제(비최적화 대상=복사로 남음 — 안전 방향).

## feat/m3-shellmenu

- **생성**: 2026-07-13 (분기: main `77c3265`). **커밋**: `895a660`(S1 — shellmenu.rs·IContextMenu 호스팅·wndproc 포워딩·delete/rename 가로채기·rows 우클릭 선택 규약+테스트·ADR-0003) → `11f7732`(S2 — 고유 병합 0x8000+[완전 삭제·붙여넣기]·Apps/Shift+F10·row_anchor) → `3bc4ebe`(S3 — 빈 영역 배경 메뉴 CreateViewObject·run_menu 통합·Undo/Redo 병합·paste 가로채기·in_body) → `a4241e4`(docs 현행화). 병합(`e28fed3`): 2026-07-13. 삭제: CI green 확인 후.
- **검증**: Windows 실기 — `cargo test` 워크스페이스 **127 green**(gui 52 — 우클릭 선택 신규) · clippy 0 · fmt · B2 0.60MB · **B3 통과(임포트 무변 — comctl32 서브클래스 회피 확인)** · 릴리스 기동 스모크(RSS 26MB·정상 종료) · **실기 QA(사용자) 5/6 통과**(파일/폴더 우클릭·보내기 서브메뉴·휴지통 복원·빈영역 Undo/Redo·새 폴더 — 잔여: Apps/Shift+F10 = TODO §7 X-1).
- **α 한계**: 커스텀 레지스트리/설정 사용자화(원본 §7)·Checksum·VerbReplacement·교차 부모 선택 = M5 후속 · 마커 존 우클릭 시 캐럿 불이동 · nexa-shell 크레이트 분리 대신 앱 모듈 채택(규모 도달 시 재검토).

## feat/m3-undo

- **생성**: 2026-07-13 (분기: main `1670bfb`). **커밋**: `faf4086`(nexa-ops history 모듈·테스트 9) → `71fcd29`(앱 배선 — State.history·push 3곳·Ctrl+Z/Y·편집 메뉴·i18n) → `4fcab3c`(휴지통 복원 recycle.rs·DeleteBatchOp) → `a45da5b`(휴지통 왕복 통합 테스트·B3 ole32). 병합(`8bf3da3`): 2026-07-13. 삭제: CI green 확인 후.
- **검증**: Windows 실기 — `cargo test` 워크스페이스 green(history 9: 스택 규약 5 + 연산 왕복 4 · ops 20) · clippy 0 · **실 휴지통 왕복 통합 테스트**(삭제→셸 undelete 복원→원위치·내용 무손상, `#[ignore]` 수동 `-- --ignored`) 통과 · 릴리스 기동 스모크(RSS 25.51MB·정상 종료) · B2 0.58MB·B3 통과(**ole32.dll 신규** — CoInitializeEx/CoTaskMemFree, OS 인박스라 DR-2 준수·화이트리스트 근거 등재).
- **α 한계**: 편집 메뉴 Undo/Redo 활성/비활성 표시 없음(위젯 enabled 미지원 — 타이틀 노트로 알림) · 다중 버전 휴지통 항목은 경로당 최초 일치 1건(삭제 시각 비교=후속) · 완전 삭제는 undo 불가(설계상 제외 — 확인창 방어).

## feat/m3-fileops

- **생성**: 2026-07-13 (분기: main `e8daec8`). **커밋**: `a291d1f`(ops 프리미티브) → `ccb8916`(gui 인라인 편집기+app 배선) → `7786eeb`(docs 현행화). 병합(`f86c021`): 2026-07-13. 삭제: CI green 확인 후.
- **검증**: Windows 실기 — `cargo test` 116 green(ops 10·rows 리네임 플로우) · clippy 0 · fmt · 릴리스 실기 4종(Ctrl+Shift+N→"MyDir" 리네임 · Del 휴지통 · Shift+Del Esc 취소 잔존 · Shift+Del Y 완전 삭제 — 타이틀 "완전 삭제: 1개 삭제") · B2 0.56MB·B3 통과(신규 임포트 없음).
- **α 한계**: 리네임 필드 IME 조합 창 위치 미배치 · 휴지통 undo 기록(M3-3) · 새 바로가기(M5).

## feat/m3-ops-transfer

- **생성**: 2026-07-13 (분기: main `6938df5`). **커밋**: `ce5e918`(nexa-ops 엔진·테스트 8) → `d06cdba`(앱 배선 — 클립보드·워커·통지·취소) → `ea1299e`(docs 현행화). 병합(`844b1e3`): 2026-07-13. 삭제: 2026-07-13(CI green 확인).
- **검증**: Windows 실기 — `cargo test` 113 green(ops 8: 순번 명명·같은 폴더 규칙·재귀·fast path·충돌 순차·순환 격리·진행 총량·취소 정리) · clippy 0 · fmt · 릴리스 실기(픽스처 3파일: Ctrl+A→C→V 같은 폴더 → **6항목·" (2)" 복제·"전송 3"** / Ctrl+A→X→V → **무동작·"건너뜀 6"**) · B2 0.54MB·B3 통과(신규 임포트 없음).
- **α 한계**: 충돌=건너뜀(확인 모달 후속)·진행 창 없음(타이틀 %)·내부 클립보드만(OS 상호운용 M3-5).

## feat/m2-ime-uia

- **생성**: 2026-07-12 (분기: main `5dc548f`). **커밋**: `2fab4bc`(IME 조합 창 캐럿 배치) → `1764257`(UIA 프로바이더·포커스 이벤트) → `2a155f7`(docs 현행화·M2 완료). 병합(`474515f`)·태그 `0.3.0`: 2026-07-13. 삭제: 2026-07-13(CI green 확인 — run 29197548967).
- **검증**: Windows 실기 — `cargo test` 105 green · clippy 0 · fmt · **UIA**: .NET System.Windows.Automation 클라이언트로 List Name(활성 경로)·가시 항목 14·항목 이름·SelectionItem IsSelected 조회 정상 · **IME**: 편집 모드에서 WM_IME_STARTCOMPOSITION/COMPOSITION 주입+한글 SendKeys 생존·Esc 복귀·정상 종료(실제 IME 조합 창 위치는 수동 확인 항목) · B2 0.50MB·B3 통과(imm32·uiautomationcore 등재 — 근거 커밋 메시지).

## feat/m2-resident

- **생성**: 2026-07-12 (분기: main `de2adbb`). **커밋**: `f2b5d15`(트림·자니터·활동 추적) → `437c214`(docs 현행화) → `61c83a1`(권한 병합). 병합(`b848904`): 2026-07-12. 삭제: 2026-07-12(CI green 확인 — run 29194988812).
- **검증**: Windows 실기 — `cargo test` 105 green(`should_trim` 1: 임계/1회성/시계 역전) · clippy 0 · fmt · 릴리스 실측(**최소화 사이클**: 활성 26.86MB → 트림 2.9MB → 복원 12.09MB·타이틀/동작 정상 / **유휴 78s**: 26.98MB → 0.21MB) — **상주 RSS ≤30MB 게이트 통과** · B2 0.49MB·B3 통과(Win32_System_Threading=kernel32 계열, 신규 DLL 없음).

## feat/m2-i18n

- **생성**: 2026-07-12 (분기: main `874559a`). **커밋**: `0bbc595`(i18n.rs·lang/·배선·언어 라디오) → `4be3f58`(docs 현행화) → `b2a5e38`(권한 병합). 병합(`7a50692`): 2026-07-12. 삭제: 2026-07-12(CI green 확인 — run 29193272097).
- **검증**: Windows 실기 — `cargo test` 104 green(i18n 4: 파싱 규칙·en/ko 키 파리티·병합/오버라이드/resolve·자리표) · clippy 0 · fmt · 릴리스 스모크(**타이틀 검증**: `lang=system`→ko "[좌] 62개 항목·탭 1/1" / `lang=en`→"[L] 62 items·Tab 1/1"·정상 종료 시 `lang` 영속) · B2 0.49MB·B3 통과(Win32_Globalization=kernel32 계열, 신규 DLL 없음).
- **비고**: 작업 도중 VSCode 비정상 종료 인시던트(작업 트리 무손실 — journal 참조). 메뉴 라디오 클릭 자동화는 mouse_event 플레이크로 생략, 단위 테스트로 커버.

## feat/m2-persistence

- **생성**: 2026-07-12 (분기: main `cc9e5ba`). **커밋**: `a156a21`(config.rs·Panel restore/session·기동 로드/종료 저장) → `7a99e02`(docs 현행화). 병합(`e04e9d4`): 2026-07-12.
- **검증**: Windows 실기 — `cargo test --workspace` 102 green(config 3: 라운드트립/관용 파싱/원자성) · clippy 0 · 릴리스 스모크(F6 라이트+Ctrl+T+Alt+↑ → 종료 → `data\` settings/session 생성 확인 → 재실행 **라이트 픽셀·[좌] C:\Users·탭 2/2 복원**·정상 종료) · B3 통과(신규 임포트 없음).

## feat/m2-panel-navbar

- **생성**: 2026-07-12 (분기: main `bc8e788`). **커밋**: `1f97534`(패널 네비 버튼) → `c395476`(docs). 병합(`450f509`): 2026-07-12.
- **검증**: `cargo test --workspace` 97 green(레이아웃 분할·[←]=해당 패널 뒤로) · clippy 0 · 릴리스 빌드.

## feat/m2-theme

- **생성**: 2026-07-12 (분기: main `9246bb2`). **커밋**: `3bc8283`(테마 모드·F6·DWM·B3) → `d89b21b`(docs 현행화). 병합(`9891174`): 2026-07-12.
- **검증**: Windows 실기 — `cargo test --workspace` 96 green · clippy 0 · 릴리스 실행 **픽셀 검증**(F6: #1F242B → #F5F7FA → 시스템 추종 → 복원·정상 종료) · B3 통과(dwmapi·advapi32 사전 등재).

## feat/m2-chrome

- **생성**: 2026-07-12 (분기: main `35bd424`). **커밋**: `f146a7c`(gui: MenuBar·Toolbar·StatusBar) → `cf7fb49`(app: 적층 레이아웃·CMD 라우팅) → `03d8804`(docs 현행화). 병합(`3a1fcf5`): 2026-07-12.
- **검증**: Windows 실기 — `cargo test --workspace` 96 green(menubar 4·chrome 2) · clippy 0 · 릴리스 실행(메뉴 숨김 토글 62→44행·도구 ↑ C:\Users 이동·정상 종료) · B3 통과. QA: mouse_event 클릭 간 대기 필요(플레이크) 확인.

## feat/m2-panels

- **생성**: 2026-07-12 (분기: main `2c8925a`). **커밋**: `fff90ed`(gui: TabBar) → `6d8e2c0`(app: panel.rs 추출·듀얼 배치·단축키) → `9c3e3b6`(docs 현행화). 병합(`bddd5ef`): 2026-07-12.
- **검증**: Windows 실기 — `cargo test --workspace` 90 green(tabbar 3·panel 4) · clippy 0 · 릴리스 실행(Tab 패널 전환·Ctrl+T 탭 2/2·탭별 독립 경로·Ctrl+Tab 순환·Ctrl+W 닫기·듀얼 RSS 25.1MB·정상 종료) · B3 통과.

## feat/m2-pathbar

- **생성**: 2026-07-12 (분기: main `9bc5bef`). **커밋**: `c2eddd6`(gui: PathBar·split_path·RightDown) → `356f9b9`(app: 레이아웃·라우팅·네비 연동) → `65833a4`(docs 현행화). 병합(`3aa46fc`): 2026-07-12.
- **검증**: Windows 실기 — `cargo test --workspace` 83 green(pathbar 5) · clippy 0 · 릴리스 실행(ClientToScreen 좌표 자동화: "C:" 세그먼트 클릭 → C:\ 이동·우클릭 편집+Enter → 복귀·정상 종료) · B3 통과. 1차 스모크의 좌표 오프셋 오류(창 rect 기준 추정치)를 ClientToScreen으로 교정해 재검증.

## feat/m1-gate

- **생성**: 2026-07-12 (분기: main `e570ef9`). **커밋**: `a7ff9df`(첫 렌더 계측·실측) → `6f5c754`(M1 완료 docs 현행화). 병합(`0c716f5`)·태그 `0.2.0`: 2026-07-12.
- **검증**: Windows 실기 — 100k 실제 파일 픽스처(%TEMP%): 첫 렌더 106/115/115ms·벤치 2,098µs·10k 유휴 RSS 27.87MB(60s 유휴·3회 중앙값) · 테스트 80 green · clippy 0 · B3 통과.

## feat/m1-navigation

- **생성**: 2026-07-12 (분기: main `f448a25`). **커밋**: `3bd09df`(gui: replace_source·marker_hit) → `8148510`(app: nav.rs·진입·Alt+화살표·토글) → `e29d3a1`(docs 현행화). 병합(`415208f`): 2026-07-12.
- **검증**: Windows 실기 — `cargo test --workspace` 80 green(nav 3) · clippy 0 · 릴리스 실행(Enter 진입→Alt+↑→Alt+← 타이틀 경로 왕복·Ctrl+H 재열기·RSS 24.5MB·정상 종료) · B3 통과.

## feat/m1-icons

- **생성**: 2026-07-12 (분기: main `984f3f4`). **커밋**: `033182d`(gui: draw_icon·icon 어휘) → `0bffc72`(app: icons.rs·DwCtx·타이머·B3 shell32) → `bdcffbb`(docs 현행화). 병합(`b8833a1`): 2026-07-12.
- **검증**: Windows 실기 — `cargo test --workspace` 74 green(icons 7) · clippy 0 · 릴리스 실행(아이콘 표시·벤치 2,547µs·RSS 27.95MB·정상 종료) · **B3 로컬 게이트가 shell32 신규 임포트를 push 전 검출**.

## feat/m1-keyboard

- **생성**: 2026-07-12 (분기: main `843da77`). **커밋**: `48ed980`(gui: 캐럿 네비·typeahead.rs·HUD) → `e5a2386`(app: WM_CHAR·수식키·타이머 배선) → `8a26d2d`(docs 현행화). 병합(`14bfe86`): 2026-07-12.
- **검증**: Windows 실기 — `cargo test --workspace` 69 green · clippy 0 · 릴리스 실행(캐럿 이동·타입어헤드 점프·keybd_event Shift+End "선택 61"·RSS 17.8MB) · B3 통과. QA 교훈: SendKeys 확장 키의 Shift 해제 특성.

## feat/m1-select

- **생성**: 2026-07-12 (분기: main `5de5385`). **커밋**: `df012bb`(gui: 선택 UX·러버밴드·키) → `67e784b`(app: TreeSource 선택·수식키·타이틀) → `378a233`(docs 현행화). 병합(`a37e01e`): 2026-07-12.
- **검증**: Windows 실기 — `cargo test --workspace` 60 green(교차폴더 AC2 포함) · clippy 0 · 릴리스 실행(SendKeys Ctrl+A → "선택 62" 타이틀 반영·벤치 1,540µs·RSS 18.20MB·정상 종료) · B3 통과.

## feat/m1-columns

- **생성**: 2026-07-12 (분기: main `6955308`). **커밋**: `57374d5`(gui: 컬럼 모델·헤더·정렬·리사이즈·가로 스크롤) → `47a9d28`(app: 5컬럼 셀 값·트리밍·이벤트 라우팅) → `8d9a6fc`(docs 현행화). 병합(`6e4ba96`): 2026-07-12.
- **검증**: Windows 실기 — `cargo test --workspace` 55 green(gui 23·source 6) · clippy 0 · 릴리스 실행(5컬럼 표시·벤치 1,813µs·RSS 18.14MB·정상 종료) · **B3 스크립트 사전 통과**(push 전 로컬).

## feat/m1-virtual-list

- **생성**: 2026-07-12 (분기: main `5a613b3` — CI 핫픽스 직후 rebase). **커밋**: `199d67a`(gui: RowItem·마커·클릭 토글) → `856ced5`(app: TreeSource 배선·GDI 삭제·레이아웃 캐시) → `5606dd7`(docs 현행화). 병합(`baa3b3f`): 2026-07-12.
- **검증**: Windows 실기 — `cargo test --workspace` 48 green(gui 20·source 2 포함) · clippy 0 · 릴리스 실행(실 트리 62행 표시·클릭 펼침/접힘·F3 벤치 1,673µs·RSS 18.17MB·정상 종료).

## feat/m1-adr0002-render

- **생성**: 2026-07-12 (분기: main `9e90d78`). **커밋**: `930eca9`(dw.rs 백엔드·비교 하네스·실측) → `6f3ba92`(ADR-0002 작성·docs 현행화). 병합(`e0daf56`): 2026-07-12.
- **검증**: Windows 실기 — 두 백엔드 벤치 자동화(SendKeys) 실측 · `cargo test --workspace` green · clippy 경고 0 · 릴리스 스모크(DW 기본, RSS 17.07MB).

## feat/m1-gui

- **생성**: 2026-07-11 (분기: main `37ca70f` — M0-8 직후). **커밋**: `63ff331`(nexa-gui 크레이트 신설) → `80cbbef`(nexa-app 재배선·gdi.rs) → `0055f7b`(docs 현행화). 병합(`c20ddde`): 2026-07-11.
- **검증**: Windows 실기 — `cargo test --workspace` 43 green(신규 nexa-gui 17) · clippy 경고 0 · fmt · 릴리스 빌드·기동·WM_CLOSE 정상(exe 0.21MB·RSS 12.4MB).

## feat/m0-render-spike

- **생성**: 2026-07-11 (분기: main `b894305`). **커밋**: `90a9243`(M0-7 렌더 스파이크) → `8c5f7f8`(git -C 권한 병합) → `8e6a986`(docs 현행화). 병합(`cc7e7ed`): 2026-07-11.
- **검증**: `cargo check/clippy --target x86_64-pc-windows-msvc` green(맥, 경고 0) · `cargo test` 26 green · fmt. 실행·화면 확인은 M0-8(Windows 실기)과 병행.

## feat/m0-scaffold

- **생성**: 2026-07-11 (분기: main `522a530`). **커밋**: `391e4bb`(M0-1 워크스페이스) → `38f92ff`(M0-2 core) → `585b2dc`(M0-3 vfs) → `370b56f`(M0-4 tree) → `34b5649`(M0-5 Win32 스켈레톤) → `3b2ddf8`(M0-6 CI) + 권한 복구 3건(`5c0d6bd`·`7282ad4`·`cb8e1db`) + docs 현행화. 병합: 2026-07-11.
- **검증**: `cargo test --workspace` green(tree 21+vfs 5) · `cargo check --target x86_64-pc-windows-msvc --workspace` green(맥). CI 러너 검증·게이트 실측(M0-8)은 push 후 Windows에서.

## docs/foundation

- **생성**: 2026-07-11 (분기: main `dffc8f9` 초기 커밋). **커밋 6개**(`43a0989` 권한 → `8f528de` 00·05 → `079fa34` 01·06 → `47597e4` 10·11·12·15·18 → `506fa4e` 02·MILESTONES·TODO → `db223e3` 운영). 병합(`d2727b5`)·삭제: 2026-07-11.
- **작업**: 원본 nexa-dir 문서 규약·형태를 차용한 설계 문서 세트 — 00/01/02/05/06/10/11/12/15/18 + README(홈)/STATUS/MILESTONES/DEVLOG/TODO/BRANCHES/journal/CLAUDE.md. 스택 결정(ADR-0001 올 러스트)·예산 게이트(DR-2)·로드맵 M0~M5.
