//! 도구 모음·상태바 — 원본 docs/20 §2의 고정 높이 크롬 위젯.
//! Toolbar: 버튼 id 통지(네비 ←→↑⟳ 등 — 실행은 호스트). StatusBar: 좌/우 텍스트 표시 전용.

use std::cell::RefCell;

use crate::draw::DrawCtx;
use crate::event::InputEvent;
use crate::geom::{Point, Rect};
use crate::theme::Theme;
use crate::widget::{Invalidations, Widget};

/// 도구 버튼(글리프 텍스트 + 명령 id).
#[derive(Clone, Debug)]
pub struct ToolButton {
    pub id: u32,
    pub glyph: String,
    /// 그룹 구분선(QA 07-14 — 원본 도구 모음 그룹화 PR#10). 클릭 불가.
    pub separator: bool,
    /// 토글 상태 표시(연한 강조 배경 — 숨김/닷파일 보기 등. 하단 줄 제거 QA 07-16).
    pub checked: bool,
    /// 아이콘 버튼(M5-1 런처 — 원본 exe 16px 썸네일 대응): `(키, 로드 힌트)` —
    /// DrawCtx::draw_icon이 해석(셸 아이콘 큐잉). Some이면 **정사각 버튼**(글리프 대신).
    pub icon: Option<(String, String)>,
    /// 활성 여부(사용자 확정 07-18 — 내비 이전/다음/상위): 비활성 = 흐린
    /// 글리프(text_dim)·hover/클릭 무시.
    pub enabled: bool,
    /// 툴팁 텍스트(07-18 — i18n 문자열은 호출자가 주입). 빈 문자열 = 없음.
    pub tip: String,
}

impl ToolButton {
    pub fn new(id: u32, glyph: impl Into<String>) -> Self {
        ToolButton {
            id,
            glyph: glyph.into(),
            separator: false,
            checked: false,
            icon: None,
            enabled: true,
            tip: String::new(),
        }
    }

    /// 그룹 구분선.
    pub fn sep() -> Self {
        ToolButton {
            id: 0,
            glyph: String::new(),
            separator: true,
            checked: false,
            icon: None,
            enabled: true,
            tip: String::new(),
        }
    }

    pub fn toggled(mut self, on: bool) -> Self {
        self.checked = on;
        self
    }

    /// 아이콘 버튼(정사각) — 아이콘 미로드 동안은 글리프 텍스트가 폴백.
    pub fn with_icon(mut self, key: impl Into<String>, hint: impl Into<String>) -> Self {
        self.icon = Some((key.into(), hint.into()));
        self
    }

    /// 초기 활성 상태(빌더) — 런타임 변경은 [`Toolbar::set_enabled`].
    pub fn enable(mut self, on: bool) -> Self {
        self.enabled = on;
        self
    }

    /// 툴팁 텍스트(빌더 — 07-18). 표시는 호스트 몫([`Toolbar::hover_tip`] 폴링).
    pub fn with_tip(mut self, tip: impl Into<String>) -> Self {
        self.tip = tip.into();
        self
    }

}

pub struct Toolbar {
    buttons: Vec<ToolButton>,
    bounds: Rect,
    row_h: i32,
    pad_x: i32,
    /// 고정 버튼 폭(px). `None` = 글리프 폭 기반. 패널 네비 버튼처럼
    /// 레이아웃이 폭을 미리 알아야 할 때 사용(버튼은 간격 없이 연속 배치).
    button_w: Option<i32>,
    hover: Option<usize>,
    pending: Option<u32>,
    ranges: RefCell<Vec<(i32, i32)>>,
}

impl Toolbar {
    pub fn new(buttons: Vec<ToolButton>, row_h: i32, pad_x: i32) -> Self {
        Toolbar {
            buttons,
            bounds: Rect::default(),
            row_h: row_h.max(1),
            pad_x,
            button_w: None,
            hover: None,
            pending: None,
            ranges: RefCell::new(Vec::new()),
        }
    }

    /// 고정 버튼 폭 모드(선두 여백 없이 bounds 시작부터 연속 배치).
    pub fn with_button_width(mut self, w: i32) -> Self {
        self.button_w = Some(w.max(1));
        self
    }

    pub fn set_button_width(&mut self, w: Option<i32>, inv: &mut Invalidations) {
        self.button_w = w.map(|v| v.max(1));
        inv.push(self.bounds);
    }

    pub fn take_command(&mut self) -> Option<u32> {
        self.pending.take()
    }

    /// 토글 버튼 상태 동기(QA 07-14 — 메뉴 체크와 동일 흐름).
    pub fn set_checked(&mut self, id: u32, on: bool, inv: &mut Invalidations) {
        for b in &mut self.buttons {
            if !b.separator && b.id == id && b.checked != on {
                b.checked = on;
                inv.push(self.bounds);
            }
        }
    }

    /// 활성 상태 동기(사용자 확정 07-18 — 내비 이전/다음/상위 사용 가능 시만).
    pub fn set_enabled(&mut self, id: u32, on: bool, inv: &mut Invalidations) {
        for b in &mut self.buttons {
            if !b.separator && b.id == id && b.enabled != on {
                b.enabled = on;
                inv.push(self.bounds);
            }
        }
    }

    pub fn set_metrics(&mut self, row_h: i32, pad_x: i32, inv: &mut Invalidations) {
        self.row_h = row_h.max(1);
        self.pad_x = pad_x;
        inv.push(self.bounds);
    }

    /// 버튼 목록 교체(07-18 — 언어 전환 시 툴팁 i18n 재주입). hover/pending 리셋.
    pub fn set_buttons(&mut self, buttons: Vec<ToolButton>, inv: &mut Invalidations) {
        self.buttons = buttons;
        self.hover = None;
        self.pending = None;
        self.ranges.borrow_mut().clear();
        inv.push(self.bounds);
    }

    fn button_at(&self, x: i32, y: i32) -> Option<usize> {
        if !self.bounds.contains(Point { x, y }) {
            return None;
        }
        self.ranges
            .borrow()
            .iter()
            .position(|&(lo, hi)| x >= lo && x < hi)
    }

    /// 좌표가 버튼(구분선 제외) 위인가(07-19 — 빈 영역 우클릭 팝업 판정).
    pub fn is_button_at(&self, x: i32, y: i32) -> bool {
        self.button_at(x, y)
            .is_some_and(|i| !self.buttons[i].separator)
    }

    /// hover 중인 버튼의 툴팁(07-18) — `(id, 텍스트, 버튼 rect[클라이언트])`.
    /// 툴팁 없는 버튼/hover 없음 = `None`. 표시·타이밍은 호스트가 관리.
    pub fn hover_tip(&self) -> Option<(u32, String, Rect)> {
        let i = self.hover?;
        let b = self.buttons.get(i)?;
        if b.separator || b.tip.is_empty() {
            return None;
        }
        let (lo, hi) = *self.ranges.borrow().get(i)?;
        Some((
            b.id,
            b.tip.clone(),
            Rect::new(lo, self.bounds.y, (hi - lo).max(0), self.bounds.h),
        ))
    }
}

impl Widget for Toolbar {
    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect, inv: &mut Invalidations) {
        if self.bounds != bounds {
            let old = self.bounds;
            self.bounds = bounds;
            inv.push(old.union(&bounds));
        }
    }

    fn on_event(&mut self, ev: &InputEvent, inv: &mut Invalidations) {
        match *ev {
            InputEvent::MouseDown { x, y, .. } => {
                if let Some(i) = self.button_at(x, y) {
                    // 비활성 = 클릭 무시(사용자 확정 07-18)
                    if !self.buttons[i].separator && self.buttons[i].enabled {
                        self.pending = Some(self.buttons[i].id);
                        inv.push(self.bounds);
                    }
                }
            }
            InputEvent::MouseMove { x, y } => {
                // 비활성 버튼은 hover 강조도 없음(식별 — 07-18)
                let hover = self
                    .button_at(x, y)
                    .filter(|&i| self.buttons[i].enabled && !self.buttons[i].separator);
                if hover != self.hover {
                    self.hover = hover;
                    inv.push(self.bounds);
                }
            }
            _ => {}
        }
    }

    fn paint(&self, ctx: &mut dyn DrawCtx, theme: &Theme) {
        ctx.select_font(crate::FontSlot::Base, false, false); // 폰트 슬롯(X-12)
        let b = self.bounds;
        let ty = b.y + (b.h - (b.h * 4) / 5) / 2;
        ctx.fill_rect(b, theme.chrome_bg);
        let mut ranges = Vec::with_capacity(self.buttons.len());
        // 고정 폭 모드 = bounds 시작부터, 글리프 폭 모드 = 선두 여백. 버튼은 간격 없이 연속.
        let mut x = if self.button_w.is_some() {
            b.x
        } else {
            b.x + self.pad_x
        };
        for (i, btn) in self.buttons.iter().enumerate() {
            if btn.separator {
                // 그룹 구분선(QA 07-14) — 세로 1px + 양측 여백
                let w = self.pad_x.max(4);
                let lx = x + w / 2;
                ctx.fill_rect(Rect::new(lx, b.y + 3, 1, (b.h - 6).max(0)), theme.border);
                ranges.push((x, x)); // 히트 없음(빈 범위)
                x += w;
                continue;
            }
            // 아이콘 버튼(런처 — M5-1) = 정사각(폭 = 바 높이), 그 외 = 고정/글리프 폭
            let w = if btn.icon.is_some() {
                b.h
            } else {
                self.button_w
                    .unwrap_or_else(|| ctx.text_width(&btn.glyph) + self.pad_x * 2)
            };
            let cell = Rect::new(x, b.y, w.min((b.right() - x).max(0)), b.h);
            // 토글 켜짐 배경 = **accent 38% 블렌드**(07-19 재확정 — 파랑 필
            // + 흰 선 시안은 원복, 아이콘은 검정 유지. 라이트 ≈ #ABCAF9·
            // 다크 ≈ #2A4A7A)
            let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * 0.38) as u8;
            let bg = if btn.checked {
                crate::theme::Color {
                    r: mix(theme.chrome_bg.r, theme.accent.r),
                    g: mix(theme.chrome_bg.g, theme.accent.g),
                    b: mix(theme.chrome_bg.b, theme.accent.b),
                }
            } else if self.hover == Some(i) && btn.enabled {
                theme.header_bg
            } else {
                theme.chrome_bg
            };
            // 비활성 = 흐린 글리프(text_dim — 사용자 확정 07-18 식별 규약)
            let fg = if btn.enabled {
                theme.text
            } else {
                theme.text_dim
            };
            if cell.w > 0 {
                if let Some((key, hint)) = &btn.icon {
                    // 16×16 상당(바 높이 − 상하 5px 여백 — 07-19 바 26px에서
                    // 아이콘 16px 유지) 아이콘을 정사각 셀 중앙에.
                    ctx.fill_rect(cell, bg);
                    let isz = (b.h - 10).max(8);
                    // 상태 변형 키: 비활성 = `#dis`(흐림 — dw.rs가 임베드
                    // 변형으로 해석). 켜짐도 아이콘은 기본 검정 유지
                    // (07-19 재확정 — 배경 블렌드만으로 식별).
                    let var_key;
                    let key = if !btn.enabled {
                        var_key = format!("{key}#dis");
                        var_key.as_str()
                    } else {
                        key.as_str()
                    };
                    let drew = ctx.draw_icon(
                        cell.x + (cell.w - isz) / 2,
                        b.y + (b.h - isz) / 2,
                        isz,
                        key,
                        hint,
                    );
                    if !drew {
                        // 미로드/추출 실패 폴백 = 라벨 앞 2자(원본 텍스트 폴백 대응)
                        let short = btn.glyph.chars().take(2).collect::<String>();
                        let tw = ctx.text_width(&short);
                        ctx.text_opaque(
                            cell.x + (cell.w - tw).max(0) / 2,
                            ty,
                            cell,
                            &short,
                            fg,
                            bg,
                        );
                    }
                } else if self.button_w.is_some()
                    || btn
                        .glyph
                        .chars()
                        .next()
                        .is_some_and(|c| ('\u{E700}'..='\u{E8FF}').contains(&c))
                {
                    // 고정 폭(네비) 또는 **MDL2 PUA 글리프**(설정 ⚙ 등 — 07-18:
                    // 본문 폰트 경로는 tofu) — 글리프 렌더로 셀 중앙에
                    ctx.glyph_opaque(cell, &btn.glyph, fg, bg);
                } else {
                    ctx.text_opaque(cell.x + self.pad_x, ty, cell, &btn.glyph, fg, bg);
                }
            }
            ranges.push((cell.x, cell.x + w));
            x += w;
        }
        ctx.fill_rect(Rect::new(b.x, b.bottom() - 1, b.w, 1), theme.border);
        *self.ranges.borrow_mut() = ranges;
    }
}

/// 상태바 — 좌(선택/항목 정보)·우(보조 정보) 텍스트 표시 전용(원본 docs/20 §2).
pub struct StatusBar {
    left: String,
    right: String,
    bounds: Rect,
    row_h: i32,
    pad_x: i32,
}

impl StatusBar {
    pub fn new(row_h: i32, pad_x: i32) -> Self {
        StatusBar {
            left: String::new(),
            right: String::new(),
            bounds: Rect::default(),
            row_h: row_h.max(1),
            pad_x,
        }
    }

    pub fn set_text(
        &mut self,
        left: impl Into<String>,
        right: impl Into<String>,
        inv: &mut Invalidations,
    ) {
        let (l, r) = (left.into(), right.into());
        if l != self.left || r != self.right {
            self.left = l;
            self.right = r;
            inv.push(self.bounds);
        }
    }

    pub fn set_metrics(&mut self, row_h: i32, pad_x: i32, inv: &mut Invalidations) {
        self.row_h = row_h.max(1);
        self.pad_x = pad_x;
        inv.push(self.bounds);
    }
}

impl Widget for StatusBar {
    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect, inv: &mut Invalidations) {
        if self.bounds != bounds {
            let old = self.bounds;
            self.bounds = bounds;
            inv.push(old.union(&bounds));
        }
    }

    fn on_event(&mut self, _ev: &InputEvent, _inv: &mut Invalidations) {}

    fn paint(&self, ctx: &mut dyn DrawCtx, theme: &Theme) {
        ctx.select_font(crate::FontSlot::Status, false, false); // 폰트 슬롯(X-12)
        let b = self.bounds;
        let ty = b.y + (b.h - (b.h * 4) / 5) / 2;
        ctx.fill_rect(b, theme.status_bar_bg);
        ctx.text_opaque(
            b.x + self.pad_x,
            ty,
            b,
            &self.left,
            theme.text,
            theme.status_bar_bg,
        );
        if !self.right.is_empty() {
            let rw = ctx.text_width(&self.right);
            let rx = (b.right() - self.pad_x - rw).max(b.x + self.pad_x);
            ctx.text_opaque(
                rx,
                ty,
                Rect::new(rx, b.y, b.right() - rx, b.h),
                &self.right,
                theme.text_dim,
                theme.status_bar_bg,
            );
        }
        ctx.fill_rect(Rect::new(b.x, b.y, b.w, 1), theme.border);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Color;

    struct Probe;
    impl DrawCtx for Probe {
        fn fill_rect(&mut self, _r: Rect, _c: Color) {}
        fn text_opaque(&mut self, _x: i32, _y: i32, _c: Rect, _t: &str, _f: Color, _b: Color) {}
        fn text_width(&mut self, text: &str) -> i32 {
            text.chars().count() as i32 * 8
        }
    }

    #[test]
    fn toolbar_click_emits_button_id() {
        let mut inv = Invalidations::default();
        let mut t = Toolbar::new(
            vec![ToolButton::new(100, "←"), ToolButton::new(101, "→")],
            20,
            6,
        );
        t.set_bounds(Rect::new(0, 0, 400, 24), &mut inv);
        t.paint(&mut Probe, &Theme::dark());
        // 버튼0: [6, 6+8+12=26) · 버튼1: [29, 49)
        t.on_event(
            &InputEvent::MouseDown {
                x: 10,
                y: 5,
                shift: false,
                ctrl: false,
            },
            &mut inv,
        );
        assert_eq!(t.take_command(), Some(100));
        t.on_event(
            &InputEvent::MouseDown {
                x: 35,
                y: 5,
                shift: false,
                ctrl: false,
            },
            &mut inv,
        );
        assert_eq!(t.take_command(), Some(101));
        t.on_event(
            &InputEvent::MouseDown {
                x: 300,
                y: 5,
                shift: false,
                ctrl: false,
            },
            &mut inv,
        );
        assert_eq!(t.take_command(), None);
    }

    #[test]
    fn statusbar_set_text_invalidates_only_on_change() {
        let mut inv = Invalidations::default();
        let mut s = StatusBar::new(20, 6);
        s.set_bounds(Rect::new(0, 400, 600, 22), &mut inv);
        inv.drain().for_each(drop);
        s.set_text("3개 항목", "RSS", &mut inv);
        assert!(!inv.is_empty());
        inv.drain().for_each(drop);
        s.set_text("3개 항목", "RSS", &mut inv);
        assert!(inv.is_empty(), "동일 텍스트는 무효화 없음");
    }
}
