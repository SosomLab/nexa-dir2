//! nexa-gui — 위젯 트리·커스텀 드로잉 추상·입력 라우팅·테마 토큰 (M1-1, docs/01 §2).
//!
//! **플랫폼 중립**: OS 의존 0 — 위젯·스크롤·무효화 로직을 맥에서도 `cargo test`로 검증(docs/11).
//! 구체 렌더 백엔드(GDI ↔ DirectWrite interop)는 [`draw::DrawCtx`] 구현체로 nexa-app이 제공하며,
//! ADR-0002(M1-2) 확정 후 이 크레이트로의 이동 여부를 재결정한다.

pub mod columns;
pub mod draw;
pub mod event;
pub mod geom;
pub mod theme;
pub mod widget;
pub mod widgets;

pub use columns::{Align, Column};
pub use draw::DrawCtx;
pub use event::{InputEvent, Key, WheelAccum, WHEEL_DELTA};
pub use geom::{Point, Rect, Size};
pub use theme::{Color, Theme};
pub use widget::{Invalidations, Widget};
