# 🧩 ctl — Nexa Controls 문서 홈 (내비게이션)

> **Win32 커스텀 컨트롤 라이브러리**("Nexa Controls" — 차후 독립 판매 겨냥)의
> 컨트롤별 설명 문서 색인. 계약의 SSOT는 각 소스 모듈 doc — 이 문서들은
> 사용자/판매 관점 요약과 메시지 계약 표를 제공한다.
> 공통 규약(명명·판매 추상화·AA·높이·공통 베이스)은 **[개요](overview.md)** 먼저.

## 읽기 순서

1. [개요·공통 규약](overview.md) — 명명(Nexa.Nx*)·판매 추상화·AA 백엔드·자동 높이·base
2. 폼 입력 계열 → 3. 버튼 계열 → 4. 컨테이너/목록 계열

## 컨트롤 색인 (호수 = 제작 순서)

| # | 컨트롤 | 클래스 | 한 줄 | 문서 |
|---|---|---|---|---|
| 1 | NxSearchBox | `Nexa.NxSearchBox` | 내장 ✕ 검색 입력 | [searchbox](searchbox.md) |
| 2 | NxFontBox | `Nexa.NxFontBox` | 글꼴 피커(자기 글꼴 렌더 드롭다운) | [fontbox](fontbox.md) |
| 3 | NxSegmented | `Nexa.NxSegmented` | 세그먼트 라디오(accent 필·MDL2 화살표) | [segmented](segmented.md) |
| 4 | NxSpin | `Nexa.NxSpin` | 숫자 스피너(독립 글상자+분리 버튼) | [spin](spin.md) |
| 5 | ~~NxDropList~~ | — | **은퇴**(07-17 — NxComboBox로 통일) | — |
| 6 | NxGroupCard | `Nexa.NxGroupCard` | 타이틀 밴드+본문 카드 컨테이너 | [groupcard](groupcard.md) |
| 7 | NxComboBox | `Nexa.NxComboBox` | macOS 팝업 버튼(✓ 팝업 선택) | [combobox](combobox.md) |
| 8 | NxCheckBox | `Nexa.NxCheckBox` | 라운드 토글(2단/3단 체크) | [checkbox](checkbox.md) |
| 9 | NxTextBox | `Nexa.NxTextBox` | 라운드 입력(포커스 accent 링) | [textbox](textbox.md) |
| 10 | NxIconButton | `Nexa.NxIconButton` | 원형 아이콘 버튼(벡터/이미지 모드) | [iconbutton](iconbutton.md) |
| 11 | NxButton | `Nexa.NxButton` | 푸시 버튼(기본/Default/Disabled) | [button](button.md) |
| 12 | NxLabel | `Nexa.NxLabel` | 폼 라벨(좌/우 정렬·클릭 투과) | [label](label.md) |
| 13 | NxMenuButton | `Nexa.NxMenuButton` | `… ⌄` 오버플로 메뉴 버튼 | [menubutton](menubutton.md) |
| 14 | NxGrid | `Nexa.NxGrid` | 그리드(리사이즈·오버레이 바·선택·정렬·마크) | [grid](grid.md) |

## 공용 인프라

| 모듈 | 역할 | 문서 |
|---|---|---|
| `style` | 팔레트([`Style`])·공통 자동 높이·fill/frame/실측 | [개요 §Style](overview.md#style--팔레트공통-높이) |
| `gdipctx` | **GDI+ 유일 접점**(DrawCtx AA 백엔드·PNG 디코드) | [개요 §AA](overview.md#aa-렌더-규약--gdipctx) |
| `base` | 공통 생명주기(state/attach/drop_state/notify/register) | [개요 §base](overview.md#base--공통-생명주기) |

## 관련 문서

- 검증 갤러리 = `crates/nexa-app/src/ctldemo.rs`(임시 🃏 버튼 — WM_APP 0x8009)
- 진행 이력 = [DEVLOG](../DEVLOG.md) · [journal/2026-07-17](../journal/2026-07-17.md)(1~13호) · [journal/2026-07-18](../journal/2026-07-18.md)(14호·확장)
- 문서 홈 = [docs/README](../README.md)
