# 우클릭 메뉴 지연 — Nexa Dir vs 파일 탐색기 (2026-10-02)

대상 파일: `C:\Users\kiros33\OneDrive - (주)비아이매트릭스\scm-project-sebang - 문서\2026 Sebang S&OP\03.  요구사항 관리\03. 모듈별 요구사항 - MP모듈\요구사항정의서_MP.pptx`

## 측정 방법

- **입력 → 메뉴 표시**: [`scripts/ctxmenu-timing.ps1`](../../../scripts/ctxmenu-timing.ps1) — 입력 직후부터 화면에 `#32768` 팝업이 뜰 때까지 1ms 폴링.
  Nexa = 앱 창에 우클릭 메시지 PostMessage · 탐색기 = `explorer /select` 창에서 Shift+F10(클래식 메뉴 — 같은 `IContextMenu` 경로)·메뉴 키(Win11 모던 메뉴).
- **앱 내부 단계**: `NEXA_CTX_TIMING=1`로 실행하면 [`shellmenu.rs`](../../../crates/nexa-app/src/shellmenu.rs) 단계별 시간을 임시 폴더 `nexa-ctxmenu-timing.log`에 기록.
- **핸들러별**: [`examples/ctxmenu_handlers.rs`](../../../crates/nexa-app/examples/ctxmenu_handlers.rs) — 레지스트리에 등록된 셸 확장 19개를 하나씩 생성·초기화·질의.

## 결과

| 앱 | 1회차(콜드) | 2~4회차(웜) |
| --- | --- | --- |
| **Nexa Dir** 우클릭 | 1.5~3.2 s | **0.7~2.1 s** |
| 탐색기 클래식(Shift+F10) | 0.3~4.5 s | **0.2~0.8 s** |
| 탐색기 모던(메뉴 키) | 0.8~1.8 s | 0.2~0.8 s |

앱 내부 단계(웜 4회):

| 단계 | ms |
| --- | --- |
| 경로 해석·바인드 | 15~25 |
| GetUIObjectOf | 0.3~17 |
| **QueryContextMenu**(셸 확장 전부 실행) | **635~1413** |
| 동사 조회·고유 항목·새로 만들기 병합 | 25~59 |
| 메뉴 창 표시(TrackPopupMenuEx 진입 후) | 수 ms |

핸들러별(웜, 상위):

| 핸들러 | ms | 비고 |
| --- | --- | --- |
| BIMATRIX iShare(.NET 2.0 CLR) | 160 | 생성만 92 ms — 매번 CLR 활성화 |
| Windows Defender 검사 메뉴 | 110 | |
| 연결 프로그램(Open With) | 70~80 | |
| 공유(Sharing) | 40~45 | |
| Google Drive(DriveFS) | 39~49 | |
| 작업 표시줄 고정 | 36~44 | |
| 보내기(SendTo) | 23~33 | |
| 그 외 12개 | 각 2~21 | |
| **합계** | **약 600** | |

## 결론

1. **지연의 95%는 `QueryContextMenu` 한 호출** — 등록된 셸 확장 19개가 순서대로 실행되는 시간이다. 앱 자체 처리(경로·병합·표시)는 50~100 ms.
2. 탐색기가 빠른 이유는 **같은 확장을 오래 떠 있는 프로세스에서 이미 로드·데워 둔 상태**로 쓰기 때문이다. 탐색기도 새 창의 첫 우클릭은 0.8~4.5 s로 느렸다.
3. Nexa의 웜 상태가 탐색기보다 여전히 0.5~1.3 s 느린 것은, 우리 프로세스가 확장 객체를 매번 새로 만들고(특히 .NET 확장 92 ms) OneDrive 경로에서 확장들이 동기화 상태를 질의하기 때문이다.

## 개선안(우선순위 — TODO X-61)

1. **기동 후 유휴 시 예열**(소): 백그라운드 STA 스레드에서 현재 폴더 파일 하나로 `QueryContextMenu`를 1회 미리 수행 → DLL·CLR 로드가 프로세스에 남아 첫 우클릭 콜드 비용(약 0.7~1.5 s)을 제거.
2. **선택 시 메뉴 선행 구축 + 전용 메뉴 스레드**(대): 메뉴를 UI 스레드가 아닌 전용 STA 스레드에서 만들고 표시까지 그 스레드가 맡는다. 선택이 300 ms 머물면 그 항목의 메뉴를 미리 질의해 두고, 우클릭 시 즉시 표시. UI는 메뉴 질의 중에도 멈추지 않는다.
3. **새로 만들기 병합 지연 로드**(소): 서브메뉴를 펼칠 때 `CLSID_NewMenu`를 초기화(25~59 ms 절감).
