# 08 · ADR-0003 — 셸 컨텍스트 메뉴 (클래식 IContextMenu 호스팅, 원본 ADR-0005 계승)

- 상태: **Accepted** (2026-07-13) · 관련: [10 결정](10-decision-record.md) DR-5 · 원본 [docs/38 ADR-0005](../../nexa-dir/docs/38-adr-0005-shell-context-menu.md) · [TODO M3-4](TODO.md)
- 대상: **M3-4** — "뷰어→탐색기" 제품 정체성(원본 B-2와 동일 위상).

## 맥락

우클릭에 셸 확장 생태계(7-Zip·Git·보내기·열기 방법·속성 등)가 떠야 탐색기 대체가 된다.
Windows 11 신형 메뉴는 호스팅 공개 API가 없고, 서드파티가 셸 메뉴를 호스팅하는 유일한
공개 경로는 **고전 `IContextMenu`(COM) + HMENU**다. 이 결정은 원본 ADR-0005에서 이미
검증됐다(대안 비교·위험 분석 포함) — 본 ADR은 **계승 + 올 러스트 재구축 특이점**만 기록한다.

## 결정

**원본 ADR-0005의 A안 계승**: 우클릭 = 클래식 네이티브 셸 메뉴(HMENU) 하나로 통합,
고유 항목은 ID 대역 분리(셸 1~0x7FFF · 고유 0x8000+)로 같은 HMENU에 병합.

### dir2 특이점 (원본과의 차이)

| # | 원본(C#/WinUI) | dir2(올 러스트) |
|---|---|---|
| 1 | `SetWindowSubclass`(comctl32)로 메뉴 메시지 포워딩 | **자기 wndproc 보유 → 서브클래스 불요.** wndproc에서 `WM_INITMENUPOPUP`/`WM_DRAWITEM`/`WM_MEASUREITEM`/`WM_MENUCHAR`를 활성 셸 메뉴의 `IContextMenu2/3`로 직접 포워딩 — **comctl32 임포트 회피(B3 무증가)** |
| 2 | 수동 COM vtable 선언(ComImport) | windows-rs 제공 `IContextMenu/2/3`·`IShellFolder` 그대로 — 선언 코드 0 |
| 3 | try/catch로 확장 예외 격리 | HRESULT `Result` 격리. 단 in-proc 확장의 SEH/크래시는 원본과 동일하게 **수용**(탐색기 동급 리스크) |
| 4 | XAML 다크 테마와 이질감 수용 | 커스텀 드로잉 다크 테마와 이질감 **동일 수용**(클래식 룩 — 원본 사용자 결정 계승) |

### 범위 (α 축소)

- **S1**: 행 우클릭 = 셸 메뉴 + `InvokeCommand` + **delete/rename 동사 가로채기**
  (앱 통합 — undo 기록·인라인 리네임 합류. 원본 verbInterceptor 계승).
- **S2**: 고유 항목 병합(완전 삭제·붙여넣기, 0x8000+) + Shift=확장 동사(CMF_EXTENDEDVERBS) + Shift+F10/Apps 키.
- **S3**: 빈 영역 = 폴더 배경 셸 메뉴(`CreateViewObject`) — 새로 만들기 서브메뉴 포함.
- **후속(M5)**: 커스텀 항목 레지스트리/설정 사용자화(원본 §7)·Checksum 서브메뉴·
  셸 동사 제자리 대체(VerbReplacement — copyaspath)·교차 부모 선택 확장.

### 다중 선택 규칙 (원본 S1 계승)

`GetUIObjectOf`는 단일 부모 폴더만 표현 — 교차폴더 선택은 **클릭 항목의 폴더 기준으로 축소**
해 전달(원본 sameDir 축소). 교차 부모 완전 지원은 후속.

## 위험 (원본 ADR-0005 §위험 전면 계승)

in-proc 확장 로드(크래시=앱 크래시, 수용) · 첫 호출 수백 ms(lazy, 수용) ·
`TrackPopupMenuEx` 모달 펌프(UI 스레드 표준 동작) · COM STA는 M3-3 휴지통 복원과 동일하게 국소 초기화.
