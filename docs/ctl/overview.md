# ctl 개요 — 공통 규약·통합 가이드

> [ctl 홈](README.md) · 소스 [`src/ctl/mod.rs`](../../crates/nexa-app/src/ctl/mod.rs)

comctl32 비의존(B3 게이트) — user32 창 클래스 + 자체 그리기 커스텀 컨트롤 묶음.
네이티브 대화상자(설정·일괄 이름변경)에서 재사용하며, **차후 독립 라이브러리
판매**를 겨냥해 앱 비결합으로 설계한다(사용자 확정 07-17).

## 시작하기 (Quick Start)

모든 컨트롤은 같은 사용 절차를 따른다 — ① 생성 ② 부모 WndProc에서 통지 수신
③ 메시지로 상태 조회/설정.

```rust
use ctl::{button, checkbox, style::Style};

// ① 생성 — 클래스 등록은 create가 내부에서 1회 처리(Once)
let style = Style::default(); // 라이트 팔레트. 다크는 필드 값만 교체
let ok = button::create(dlg, 0, y, 0, 0, ID_OK, font, "확인",
                        button::ButtonKind::Default, true, style);
let chk = checkbox::create(dlg, x, y, 0, 0, ID_OPT, font, "옵션", 0,
                           checkbox::CheckMode::Two, style);

// ② 부모 WndProc — 통지는 WM_COMMAND(MAKEWPARAM(id, code), lparam=컨트롤)
WM_COMMAND => {
    let id   = (wparam.0 & 0xFFFF) as u32;
    let code = ((wparam.0 >> 16) & 0xFFFF) as u32;
    match (id, code) {
        (ID_OK, button::NXBTN_CLICK) => { /* 확인 실행 */ }
        (ID_OPT, checkbox::NXCHK_CHANGED) => {
            // ③ 상태 조회 — WM_USER 오프셋 메시지
            let on = SendMessageW(chk, checkbox::NXCHK_GETCHECK, None, None).0;
        }
        _ => {}
    }
}
```

### 공통 생성 인자
전 컨트롤의 `create` 선두 인자는 동일하다(각 문서에는 **컨트롤 고유 인자만** 서술):

| 인자 | 타입 | 설명 |
|---|---|---|
| `parent` | `HWND` | 부모 창(통지 수신처 — WM_COMMAND 재발행 대상) |
| `x`, `y` | `i32` | 부모 클라이언트 기준 위치(px) |
| `w`, `h` | `i32` | 크기. **`h <= 0` = 공통 자동 높이**(글꼴+4px — 버튼/세그는 컴팩트 +2px). `w <= 0` 자동은 컨트롤별 명시 |
| `id` | `u32` | 컨트롤 id(통지의 `LOWORD(wparam)` — 다이얼로그 내 유일) |
| `font` | `HFONT` | 본문 글꼴(컨트롤은 참조만 — 소유·해제는 호스트) |
| `style` | `Style` | 팔레트(마지막 인자 — 값 복사 소유) |

### Tab 내비게이션
호스트 모달 루프에 `IsDialogMessageW(dlg, &msg)`를 넣으면 Tab이 배치(생성)
순서로 이동한다. 컨테이너(NxGroupCard·NxTextBox·NxSpin)는 `WS_EX_CONTROLPARENT`
로 내부 입력에 착지하고, 방향키 소비 컨트롤은 `DLGC_WANTARROWS`를 선언해
화살표 동작이 유지된다.

## 명명 규약
- 컨트롤 = **Nx 접두어**, 네임스페이스 = **Nexa** → Win32 클래스 `Nexa.Nx<이름>`.
- 팝업 창 클래스 = `...Pop`(예: `Nexa.NxComboBoxPop`).

## 판매용 추상화 규약
- **앱 비결합**: 색 = [`Style`] 인자·라벨 = 복사 소유·i18n/테마 미참조.
- 텍스트 API 위임: `WM_SETTEXT`/`WM_GETTEXT`(+`EM_SETCUEBANNER`·`EM_SETSEL`) → 내부 EDIT.
- **통지 = 컨트롤 id로 `WM_COMMAND` 재발행**: `MAKEWPARAM(id, code)`·lparam = 컨트롤 HWND.
- 상태 = `GWLP_USERDATA` Box(파괴 시 회수 — base 공통).
- 판매 단위 = ctl + `nexa_gui::DrawCtx` 트레이트 + 백엔드(gdipctx).

## AA 렌더 규약 — gdipctx
- 곡선·사선 도형은 **DrawCtx AA 프리미티브로만**(fill_ellipse/fill_round_rect/
  stroke_round_rect/polyline) — **GDI+ 직접 호출 금지**(사용자 지시 07-17).
- [`gdipctx`](../../crates/nexa-app/src/ctl/gdipctx.rs) = **코드베이스 유일 GDI+
  접점**(gdiplus.dll 인박스). D2D 승격 시 이 모듈만 교체.
- 텍스트 = GDI `DrawTextW`/DirectWrite 유지(GDI+ 텍스트 부적합 — 사용자 확정).
- **shape 투명** = 1비트 리전 클립 폐기 → 모서리를 `Style.behind`(부모 배경색)로
  칠하고 AA 도형을 블렌드.
- **PNG 이미지**(07-18): `decode_png`(SHCreateMemStream)·`image_size`·
  `GdipCtx::draw_image`(바이큐빅) — NxIconButton 이미지 모드가 소비.
- **주의**: GDI 텍스트를 그리기 전에 `GdipCtx`를 drop(HDC 혼용 규약).

## Style — 팔레트·공통 높이
[`style`](../../crates/nexa-app/src/ctl/style.rs) — 색 하드코딩 금지, 전 컨트롤이
[`Style`] 인자를 받는다(기본 = 라이트 팔레트, 다크/커스텀은 호스트가 값만 교체).

| 필드 | 용도 |
|---|---|
| `bg` / `border` / `text` / `text_dim` | 배경·1px 테두리·본문·보조 글자 |
| `accent` | 강조(선택 필·포커스 링·Default 버튼) |
| `sel_bg` | 선택/hover/컨테이너 배경 |
| `behind` | **도형 밖 배후색**(부모 배경 — AA 가장자리 블렌드 기준) |
| `danger` | 파괴적 액션(그리드 ⊖·삭제 빨강 `#FF3B30`) |

- **공통 자동 높이**: `h <= 0` = `auto_height`(글꼴 + 상/하 4px) — 전 컨트롤
  동일해 "수정 없이 배치만 해도 반듯"(사용자 확정). 예외 = 버튼/세그먼트
  **컴팩트**(글꼴 + 상/하 2px).
- 텍스트 세로 = 중앙 **+1px 하향** 보정(콤보/글상자/세그 공통 — QA 07-17).
- 라벨 실측 = `text_width`(언어 전환에도 정렬 유지 — 카드 라벨 열).

## base — 공통 생명주기
[`base`](../../crates/nexa-app/src/ctl/base.rs)(07-18 리팩터) — 전 컨트롤이
반복하던 4대 패턴의 공통 헬퍼. 새 컨트롤은 이것만 쓰면 된다:

| 헬퍼 | 역할 |
|---|---|
| `state<T>(hwnd)` | GWLP_USERDATA → `*mut T`(미설치 = 널) |
| `attach_state<T>(hwnd, Box<T>)` | 생성 시 상태 설치(소유권 이전) |
| `drop_state<T>(hwnd)` | WM_DESTROY 표준 처리(박스 회수 — **Drop 실행**) |
| `notify(hwnd, code)` | 부모 `WM_COMMAND(MAKEWPARAM(id, code))` 재발행 |
| `register_class(once, class, proc)` | 창 클래스 1회 등록(화살표 커서) |

- GDI 자원(HFONT·HBRUSH·GpImage)을 쥔 상태는 **Drop 구현**으로 해제(RAII —
  segmented icon_font·textbox bg_brush·iconbutton GpImage가 선례).
- 팝업 소유 컨트롤은 `close_drop`(팝업·타이머 정리) 후 `drop_state` 위임.

## Win32 함정 원장(계승 필수)
- **WS_POPUP owner를 자식으로 주면 최상위로 승격** → GetParent 오독. 팝업의
  USERDATA에 owner 저장(콤보/메뉴/폰트박스 공통).
- **빈 문자열을 DrawTextW에 전달 금지**(빈 Vec = 댕글링 포인터 → user32 AV).
- NOACTIVATE 팝업 + 60ms 바깥클릭 타이머 = 표준 닫기 패턴.
- 깜빡임 = 전체 무효화(erase=true) — 잦은 갱신 컨트롤(그리드)은 WM_PAINT
  더블버퍼 + `WM_ERASEBKGND=1` + erase=false.
