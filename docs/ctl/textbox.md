# NxTextBox — 라운드 입력 (9호)

> [ctl 홈](README.md) · 클래스 `Nexa.NxTextBox` ·
> 소스 [`textbox.rs`](../../crates/nexa-app/src/ctl/textbox.rs)
> macOS 시안 07-17: 라운드 테두리·포커스 시 accent 두꺼운 링.

## 모양·동작
- 본체 = AA 라운드 사각(모서리 = behind 블렌드) + 평시 1px border·
  **포커스 = accent 2px 링**. 입력은 내부 무테두리 EDIT(글꼴 높이 세로 중앙 +1px).
- `h <= 0` = 공통 자동 높이(글꼴+4px).
- **Tab 내비(07-18)**: 래퍼 = `WS_EX_CONTROLPARENT`·탭스톱 = 내부 EDIT 단독 —
  IsDialogMessage에서 Tab 착지가 입력칸으로 직행.

## 텍스트 API(드롭인 계약)
반환 HWND에 표준 텍스트 API를 그대로 쓴다 — 내부 EDIT로 위임:
`WM_SETTEXT` / `WM_GETTEXT` / `WM_GETTEXTLENGTH` / `EM_SETCUEBANNER`(플레이스홀더) /
`EM_SETSEL`(전체 선택 — Save 팝업 기본 이름 시안 07-18).

## API·메시지 계약
| 항목 | 값 | 의미 |
|---|---|---|
| `create(parent, x, y, w, h, id, font, style)` | — | — |
| 재발행 `EN_CHANGE` | 0x300 | 내용 변경 → 부모 WM_COMMAND(컨트롤 id) |
| 재발행 `EN_SETFOCUS`/`EN_KILLFOCUS` | 0x100/0x200 | 포커스 in/out(링 재도장 포함) |

## 내부 구현 메모
- CTLCOLOR 브러시는 상태에 1회 보관(메시지마다 생성 시 GDI 누수) —
  **Drop RAII 해제**(07-18 리팩터).
