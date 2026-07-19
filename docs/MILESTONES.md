# MILESTONES — 기능·마일스톤 기록 (프로젝트 목적 기준)

> **프로젝트 목적(포터블 단일 exe 네이티브 탐색기) 기준으로 기능과 마일스톤을 관찰**하는 단일 기록.
> 시간순 진행은 짝 문서 **[DEVLOG.md](DEVLOG.md)**. 로드맵 원안 [02](02-roadmap.md)·요구 [05](05-requirements.md).
> 상태: ✅ 완료 · 🚧 진행 · 📐 설계 · ☐ 미착수. ("완료" = 테스트 green + 해당 시 예산 게이트 통과.)

## 마일스톤 개요

| # | 목표 | 게이트 | 상태 |
| --- | --- | --- | --- |
| **M0** | 기반 — 코어 3크레이트 이식·Win32 창 스파이크·CI | 빈 창 RSS·exe 크기·임포트 실측 | ✅ `0.1.0` |
| **M1** | 뷰어 코어 — ★ 플래그십(인라인 트리+교차선택)·가상 리스트·정렬·타입어헤드 | 100k <150ms·60fps | ✅ `0.2.0` |
| **M2** | 셸 골격 — 경로바·탭/듀얼·메뉴·테마·설정/세션·IME/UIA 1차 | 상주 RSS ≤30MB | ✅ `0.3.0` |
| **M3** | 파일 조작 — nexa-ops(Undo/Redo)·셸 메뉴·클립보드·DnD·watcher | 유휴 RSS ≤30MB(10k 유휴 300s 6.29MB) | ✅ `0.4.0` |
| **M4** | 하단 패널 — 정보·미리보기·ConPTY 터미널 | 유휴 RSS ≤30MB(10k+터미널 상주 300s 5.07MB) | ✅ `0.5.0` |
| **M5** | 마감 — 잔여 패리티·릴리스 파이프라인·서명 결정 | 예산 최종(B1 16.86·B2 0.90·B3 통과) | ✅ `0.6.0`(릴리스 발행 — 실기 QA 잔여) |
| **포스트 M5** | UX 고도화 — 툴바 SVG·순서 편집 3종 창·컬럼 드래그/auto-fit·하단 도크 버튼·다크모드 아이콘·exe 리소스 아이콘·배포명 "Nexa Dir"·위키 | B1 0.27(트림)·B2 1.43·B3 17종 | ✅ `0.7.0`·`0.8.0`(릴리스 발행) |

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

## M2 — 셸 골격 ✅ (`0.3.0`)

- ✅ M2-1 경로 바 α(`feat/m2-pathbar`) — 원본 docs/27: 브레드크럼(클릭 이동·현재 비활성·hover)+편집 모드(우클릭·Enter/Esc)·네비게이션 비종속 규약. 자동완성(PATH-SUG)·오버플로·UNC·▾ 드롭다운은 β/γ 후속.
- ✅ M2-2 듀얼 패널+패널별 탭(`feat/m2-panels`) — Panel 추출(플랫폼 중립·탭=독립 뷰+독립 히스토리 docs/20 §3)·TabBar 위젯(활성 accent·× 닫기·[+])·스플리터·활성 패널(클릭/Tab)·Ctrl+T/W/Ctrl+Tab. 듀얼 RSS 25.1MB. 탭 드래그·잠금·세션=후속.
- ✅ M2-3 메뉴·도구·상태바(`feat/m2-chrome`) — 크롬 3종 커스텀 드로잉(자식 HWND 0): MenuBar 드롭다운 오버레이·체크 토글(단축키와 동기)·Toolbar 네비 버튼·StatusBar. 실기 숨김 토글 62→44행 확인.
- ✅ M2-4 테마 시스템(`feat/m2-theme`) — 시스템(레지스트리 추종·WM_SETTINGCHANGE 실시간)/라이트/다크, 메뉴 라디오·F6 순환·DWM 다크 타이틀바. 실기 픽셀 검증. 모드 영속=M2-5.
- ✅ M2-5 설정/세션 영속(`feat/m2-persistence`) — 원본 docs/34·40·43 + DR-3: exe 옆 `data\` settings.txt(테마·필터·스플리터)·session.txt(패널별 탭·활성). key=value 텍스트(crate 0·관용 파싱)·원자적 쓰기(tmp→rename)·기동 로드/종료 저장(argv 경로는 명시 의도 우선). 실기 재실행 복원 검증. 주기 저장·창 위치·탭 잠금=후속.
- ✅ M2-6 i18n(`feat/m2-i18n`) — 원본 docs/42: properties `.lang`(crate 0)·내장 en/ko 임베드+exe 옆 `data\lang\` 키 단위 오버라이드(DR-3)·폴백 체인(현재→en→키)·**동적 전환**(언어 라디오 → 테이블 스왑+메뉴/컬럼 재구성+재그리기 — 재시작 불요)·"system"=OS 언어 추종·settings `lang` 영속. en/ko 키 파리티 테스트. α 한계: 전환 시 컬럼 폭 리셋.
- ✅ M2-8 상주 규율(`feat/m2-resident`) — 원본 01 §5-1·docs/28: **유휴 60s(자니터)·최소화 시 트림** — DW 백버퍼+레이아웃 캐시·셸 아이콘 HICON 해제 + `SetProcessWorkingSetSize(-1,-1)` 반납, 다음 페인트 지연 재적재. 자니터 자기 소거(유휴 백그라운드 0%). **게이트 실측: 활성 26.9MB → 최소화 2.9MB / 유휴 0.21MB — 상주 RSS ≤30MB 통과**. 메모리 압박 구독(NFR-M4)·arena 회수=β 후속.
- ✅ M2-7 IME(한글)·UIA 1차(`feat/m2-ime-uia`) — 원본 NFR-A1: **IME** = WM_IME_*에서 조합 창을 경로바 편집 캐럿에 배치(결과 문자열은 기존 WM_CHAR 경로). **UIA** = WM_GETOBJECT 서버측 프로바이더(`uia.rs`, 불변 스냅샷 — 임의 스레드 콜백 안전): List(Name=경로)+ListItem(파일명·캐럿 포커스·SelectionItem)·FocusChanged 이벤트(리스닝 가드). .NET UIA 클라이언트로 실기 검증. 마감(조합 인라인·패턴 완성·구조 이벤트)=M5-3.
- **게이트 통과**: 상주 시나리오(듀얼·탭 4) RSS 26.9MB ≤30 + 트림 0.21~2.9MB → **M2 완료 `0.3.0`**.

## M3 — 파일 조작 ✅ (`0.4.0`, 2026-07-13)

> **마감 게이트 통과**(07-13): B1 = 10k 폴더 로드(활성 32.57MB) → 유휴 트림 직후 **0.21MB** ·
> 180s 0.25MB · **300s 6.29MB ≤ 30MB**(M3-5 OLE 상주분 회수 확인) · B2 0.60MB · B3 인박스만.

- ✅ M3-1 `nexa-ops` 전송 엔진(`feat/m3-ops-transfer`) — 원본 docs/33 TRANSFER-ENGINE·FileOps.cs 이식: **rlib 신설**(플랫폼 중립·crate 0), `transfer()` 단일 경로 — 같은 폴더 규칙(이동 무동작/복사 " (2)" 순번 복제)·충돌 항목만 순차 Overwrite/Skip·4MB 청크 진행률·취소(부분 파일 정리 — 안전 개선)·개별 격리·동일 볼륨 fast path·순환 금지. 배선: 내부 클립보드 Ctrl+C/X/V·워커+세대 가드(A-1)·Esc 취소·양쪽 재로드. 실기 순번 복제/무동작 검증·테스트 8.
- ✅ M3-2 삭제·이름변경·새로 만들기(`feat/m3-fileops`) — 원본 DeletePaths·B-6·BG-N1/N2 이식: Del=휴지통(SHFileOperationW FOF_ALLOWUNDO — α)·Shift+Del=완전(MessageBoxW 확인창·기본 취소·개별 격리)·F2=**인라인 이름변경**(VirtualRows 오버레이 편집기: 문자/Backspace·Enter/Esc·키 차단·클릭 취소)·Ctrl+Shift+N/파일 메뉴=새 폴더·새 파일(생성→즉시 리네임 = RevealAndRename). nexa-ops delete_permanent/rename/create_new. 실기 4종 통과.
- ✅ M3-3 Undo/Redo(`feat/m3-undo`) — 원본 OperationHistory.cs·RecycleBin.cs 이식(B-13u·docs/33): **nexa-ops `history` 모듈**(플랫폼 중립) = `ReversibleOp` trait + `OperationHistory`(스택 2개·새 push 시 redo 무효화·상한 100·실패 연산 소실=무결성 우선) + 연산 4종(Move 역이동·Copy 사본 삭제 주입·Rename·Create 재생성 주입), 오류 `OpError` 구조화(i18n=앱). 배선: `State.history`+push 3곳(전송 완료=수행 쌍만·이름변경·새로 만들기)·**Ctrl+Z / Ctrl+Y·Ctrl+Shift+Z**·편집 메뉴·양쪽 재로드·타이틀 노트. **휴지통 복원**(`recycle.rs`): 셸 폴더 열거→원위치 매칭→**undelete 동사**(STRRET 직접 파싱=shlwapi 회피)·`DeleteBatchOp`(undo=복원/redo=재삭제). **완전 삭제는 undo 불가**(설계). 실 휴지통 왕복 통합 테스트 통과·exe 0.58MB·RSS 25.51MB·**B3 통과**(ole32 신규=OS 인박스). α: 메뉴 활성 표시 없음·다중 버전 최초 일치 1건.
- ✅ M3-4 셸 컨텍스트 메뉴(`feat/m3-shellmenu`, [ADR-0003](08-adr-0003-shell-context-menu.md) — 원본 ADR-0005 계승) — 행 우클릭=클래식 IContextMenu 셸 메뉴(HMENU 호스팅·동적 서브메뉴는 자기 wndproc 포워딩 = comctl32 불요·B3 무변)·빈 영역=배경 메뉴(CreateViewObject·새로 만들기 서브메뉴)·고유 병합 0x8000+(완전 삭제·붙여넣기·Undo/Redo)·delete/rename/paste 동사 가로채기(undo·인라인·전송 합류)·Apps/Shift+F10·우클릭 선택 규약(rows). 앱 모듈(shellmenu.rs — 크레이트 분리 회피). 테스트 127 green·셸 메뉴 상호작용 실기 QA 대기. 후속(M5): 레지스트리 §7·Checksum·VerbReplacement·교차 부모.
- ✅ M3-5 클립보드 상호운용·OLE DnD(`feat/m3-clipboard-dnd`) — 원본 OS 클립보드 읽기측·OleDropTarget.cs·B-14dnd 이식(Win32 원시 포맷+OLE COM 재구현): **OS 클립보드 단일 출처**(CF_HDROP·Preferred DropEffect — 내부 클립보드 제거·탐색기↔앱 Ctrl+C/X/V·잘라내기 1회성·실 왕복 테스트)·**DnD 수신**(IDropTarget — Ctrl 복사/Shift 이동/기본 볼륨별·자기/하위 금지·최적화 이동 NONE)·**발신**(SHCreateDataObject+IDropSource+DoDragDrop — 임계 이동 시작·원본 미삭제 안전 방향·내부 패널 간 동일 왕복). B3 무변·실기 QA 대기. α: spring-load·드롭 하이라이트.
- ✅ M3-6 watcher(`feat/m3-watcher`) — 원본 FolderWatcher.cs(B-12w) 이식: ReadDirectoryChangesW **비재귀**(패널 현재 폴더만)·300ms 디바운스 코얼레싱·**무간섭 재로드**(펼침·선택·캐럿·스크롤 보존)·편집/전송 중 지연 재무장·세대 가드(A-1)·중지=핸들 닫기(Drop)·실패=F5 폴백. 실기: 외부 생성(0→2)·삭제(2→1) 자동 반영. α: 펼친 하위 폴더 비감시. **→ M3 전 항목 구현 완료 — 마감 게이트(B1 유휴 재실측·`0.4.0` 태그) 잔여.**

## M4 — 하단 패널 ✅ 완료 (`0.5.0` · 2026-07-14)

> **마감 게이트**: B1 유휴 RSS(10k 픽스처 + **도크 터미널 상주·캐럿 깜빡임** = 최악 케이스) — 활성 32.75MB → 트림 직후(65s) 0.22MB → 180s 2.00MB → **300s 5.07MB ≤ 30MB 통과**(ConPTY 자식 프로세스는 앱 RSS 무부담) · B2 0.73MB · B3 인박스만 · 테스트 151 green.

- ✅ M4-1 하단 도크(`feat/m4-dock`) — 원본 BottomDockView(Info)·DockInfo·도크 대원칙(듀얼=좌↔좌·우↔우) 이식: **InfoDock 위젯**(라벨 스트립+텍스트 라인·변경 시에만 무효화)·**정보 뷰**(다중=개수/단일=이름·종류·크기[원시 B]·수정·경로/없음=현재 폴더 — 선택 변경 자동 갱신)·Ctrl+`/보기 메뉴 토글(체크 동기)·**높이 드래그**(경계 ±3px·비율 0.15~0.5·양 패널 동기)·settings 영속(dock·dock_ratio·왕복 테스트). 테스트 140 green. Preview(M4-2)·Terminal(M4-3)은 종류 스왑 확장.
- ✅ M4-2 내장 미리보기(`feat/m4-preview`) — **내장 방식**(DR-7: 원본 .NET Nexa.Plugins SDK 비이관 — 플러그인 아님): 도크 종류 스트립(정보|미리보기 — 클릭 전환·활성 강조)·**텍스트**(첫 16KB·UTF-8 lossy·탭→4칸·200줄 상한·1KB NUL=이진 판정)·**이미지**(WIC: 디코더→Fant 스케일러[확대 없음]→32bppBGRA→StretchDIBits — png/jpg/jpeg/bmp/gif/ico/tif·(경로,크기) 캐시 8건·상주 트림 시 소멸). `DrawCtx::draw_image` 프리미티브 신설. WIC은 CoCreateInstance 지연 활성화 — **임포트 테이블 무변(B3 통과)**. 독립 검증 예제 `examples/preview_image.rs`(동일 파이프라인 — `cargo run --example preview_image -- <경로>`).
- ✅ M4-3 ConPTY 터미널(`feat/m4-terminal`) — 원본 VtScreen.cs·ConPtySession.cs(docs/37) 이식: **nexa-term rlib**(VT 파서+셀 그리드 — SGR 16/256/트루컬러·CSI 커서/지우기/삽입삭제·DECSTBM 마진·스크롤백 800·전각 연속 셀·테스트 9)·**ConPTY 세션**(pwsh→powershell→cmd 폴백·UTF-8 경계 보존 읽기 스레드·EXIT_FLAG 통지·세대 가드·Drop 정리)·**도크 [터미널] 종류**(Consolas 12 모노 셀 그리드[DrawCtx::term_text/term_cell_w 신설]·동일 색 런 병합·reverse/faint·캐럿·클릭 키 포커스·화살표 등 VT 시퀀스·셸 종료 시 아무 키 재시작·리사이즈 동기). 테스트 148 green·B2 0.69MB·B3 무변. **→ M4 전 항목 구현 완료 — 마감 게이트(B1) 잔여.**
- ✅ M4-3 후속 — 실기 QA 시리즈(07-14, `fix/m4-term-qa`·`fix/term-caret-color`·`fix/m4-qa-batch2`·`feat/m4-term-select`·`fix/nav-freeze-watcher`): **터미널 상호작용 완성** — 셀 단위 렌더(폴백 글꼴 열 밀림 해소)·마우스 드래그 선택(엣지 자동 스크롤)·스크롤백 휠·Ctrl+C/V·세로바 캐럿(깜빡임·밝은 회색)·Backspace=DEL 교차 매핑·settings term_font. **프리즈 2건 근본 해소** — 파일별 아이콘 워커화(Defender/OneDrive)·watcher drop CloseHandle 직렬화(OVERLAPPED 재구현 — 자동 재현 12,009ms→4ms 실측). + 편집 필드 클립보드·리네임 더블클릭 지연·휠 hover 라우팅·.lnk 숨김. 테스트 151 green·B2 0.73MB·B3 무변.

## M5 — 마감·릴리스 ✅

- ✅ M5-1 퀵 런처(`feat/m5-launcher`, 07-15 — 원본 docs/44 이식) — 도구 모음 아래 **상시 표시 런처 바**(사용자 정의 외부 프로그램 버튼·Toolbar 위젯 재사용): `%path%`→활성 패널 폴더 치환 ShellExecuteW(작업 디렉터리 동반)·실패 상태바 격리(원본 오류 격리 규약)·보기 메뉴 토글·settings.cfg 영속(`launcher`·`launcher_count`·`launcherN=라벨|exe|인자` — count로 첫 실행/비움 구분)·VS Code 시드(ResolveVsCode 3경로). α: 항목 CRUD=settings.cfg 직접 편집·exe 아이콘/항목 단축키 후속.
- ✅ M5-1 일괄 이름변경 α(`feat/m5-bulk-rename`, 07-15 — 원본 docs/25 스펙 **최초 구현**·원본도 설계만) — **nexa-ops::batch_rename**(순수·맥 테스트): 치환(대소문자 무시 문자 단위)→대소문자(UPPER/lower/Title/Sentence)→삽입(접두/접미)→연번(시작·증가·0패딩·위치) 고정 파이프라인·이름부만 적용·충돌 4종(빈·금지 문자·배치 내 중복·기존 존재). **다이얼로그**(user32·comctl32 비의존): 동작 폼+실시간 미리보기(`원본 → 새 이름`·충돌 ⚠)·[적용]=충돌 0·변경 ≥1일 때만. 적용 = **MoveBatchOp 트랜잭션 1건**(B-13u — Ctrl+Z 배치 되돌림)+F18 접두사 치환. 진입 = 편집 메뉴·Ctrl+Shift+R. β 이후: 정규식·날짜·토큰 언어·프리셋·블록 재배열.
- ✅ M5-2 릴리스 파이프라인(`feat/m5-release-pipeline`, 07-15) — `.github/workflows/release.yml` 신설: 버전 태그 push(`0.5.0` 형식·`v` 접두사 허용) → windows-latest `cargo test`+release 빌드 → **예산 게이트(B2 exe ≤10MB·B3 임포트 화이트리스트 — CI와 동일 스크립트 `scripts/budget-b3.ps1`)** 통과 필수 → `NexaDir2-<버전>-win-x64.exe`(포터블 단일 exe — DR-3) 개명 → **GitHub Release 자동 생성·첨부**(자동 노트). `workflow_dispatch` 수동 실행=게이트+아티팩트까지(Release는 태그에서만). 릴리스 절차 SSOT = [18](18-build-and-test.md) §5(`git tag X.Y.Z && git push origin X.Y.Z`).
- ✅ M5-3 접근성·IME 마감·서명 결정(`feat/m5-a11y`, 07-15) — **UIA SelectionItem 실동작**(Select/Add/Remove → WM_APP_UIA_SELECT UI 스레드 전달·select_program 범위 방어)·**구조 변경 이벤트**(uia_notify (패널·경로·행 수) 서명 → ChildrenInvalidated — M2-7 1차 한계 2건 해소)·**리네임 인라인 IME 조합 창 배치**(rename_edit_info — M3-2 α 해소)·**서명 = 무서명 유지 확정**(DR-3 갱신 — 원본 PKG-4 공동 보류[Store $19 vs OV 연 $100~400 비용 결정 대기]·SmartScreen 감수·인증서 확보 시 release.yml 서명 단계 추가).

## M1+ (요약)

[02 로드맵](02-roadmap.md) 참조. 기능 상세 스펙은 원본 nexa-dir docs(07 플래그십·23 컬럼·32 타입어헤드·33 파일조작·37 터미널·38 셸메뉴·39 테마·40 설정·42 i18n)를 원천으로 사용한다.
