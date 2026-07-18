# NxIconButton — 원형 아이콘 버튼 (10호)

> [ctl 홈](README.md) · 클래스 `Nexa.NxIconButton` ·
> 소스 [`iconbutton.rs`](../../crates/nexa-app/src/ctl/iconbutton.rs)
> macOS 시안 07-17: 카드 타이틀의 ⊕⊖ 원형 버튼. 설계 = **단일 컨트롤 +
> 아이콘 소스 enum**(도형별 서브클래스 대신 도형 = 데이터).

## 두 가지 모드
1. **벡터 모드** — `create(..., icon, ...)`: AA 원판(활성 = text_dim·비활성 =
   border) + 글리프.
   - `Icon::Plus`(＋) / `Icon::Minus`(−) = bg색 AA 폴리라인 2px
   - `Icon::Help`(?) = GDI 텍스트(텍스트 = GDI 규약)
2. **이미지 모드**(07-18) — `create_image(..., active, disabled, fit, ...)`:
   PNG 바이트(알파)를 디코드해 그린다. enabled 전환 = 활성/비활성 raster 교체.
   리네임 카드 ±가 소비(`assets/rename/` — 초록 ＋·빨강 −·회색 비활성).

## ImageFit — 이미지 표시 모드(사용자 확정 07-18)
| 값 | 효과 |
|---|---|
| `Native` | **원본 픽셀 크기 그대로** 컨트롤 중앙 배치(크면 잘림) |
| `Stretch` | **컨트롤 크기에 맞춰 늘림**(바이큐빅 — 기본) |

## 크기·투명
- `d <= 0` = **글꼴 높이 지름**(체크박스 박스와 동일 크기 — 같은 row 시각 일치).
- shape 투명 = 모서리를 `behind`로 칠하고 AA 원판/이미지 알파 블렌드
  (1비트 리전 클립 폐기 — 계단 가장자리 진범).

## API·메시지 계약
| 항목 | 값 | 의미 |
|---|---|---|
| `create(parent, x, y, d, id, font, icon, enabled, style)` | — | 벡터 모드 |
| `create_image(parent, x, y, d, id, font, active, disabled, fit, enabled, style)` | — | 이미지 모드(PNG 바이트) |
| 통지 `NXIB_CLICK` | 1 | 클릭(enabled일 때만) |
| `NXIB_GETENABLE` / `NXIB_SETENABLE` | WM_USER+100/101 | 활성 조회/설정(재도장) |

## 내부 구현 메모
- GpImage 2장(활성/비활성)은 상태 보유·**Drop RAII 해제**. 디코드/드로우는
  gdipctx 경유(GDI+ 유일 접점 규약).
