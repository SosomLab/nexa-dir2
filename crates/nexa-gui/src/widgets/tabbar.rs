//! 패널 탭 바 — 원본 PanelTab(docs/20 §2) 대응 위젯. 탭 표시·전환·닫기·새 탭 +
//! **드래그 재정렬·잠금 표시·우클릭 메뉴 통지**(편의 UX ② — 원본 탭 UX 계승) +
//! **멀티라인 배치**(사용자 요청 07-20 — 폭 초과 시 줄바꿈·상하 드래그 이동).
//! 도메인 비종속: 제목·잠금 목록만 알고, 동작은 [`TabAction`]으로 호스트에 통지(take_action).
//! 줄 수는 paint가 측정·캐시([`lines`](TabBar::lines)) — 호스트가 변경 통지
//! ([`take_lines_changed`](TabBar::take_lines_changed))를 보고 재레이아웃한다
//! (텍스트 측정은 paint에서만 가능 — edit.rs 캐시 규약 동일).
//! 패널 간 이동/Ctrl 복제 드래그는 후속.

use std::cell::{Cell, RefCell};

use crate::draw::DrawCtx;
use crate::event::InputEvent;
use crate::geom::{Point, Rect};
use crate::theme::Theme;
use crate::widget::{Invalidations, Widget};

/// 호스트가 수행할 탭 동작.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TabAction {
    Switch(usize),
    Close(usize),
    New,
    /// 드래그 재정렬(편의 UX ②) — `from` 탭을 `to` 위치로.
    Move {
        from: usize,
        to: usize,
    },
    /// 탭 우클릭(컨텍스트 메뉴는 호스트가 표시 — 잠금/복제/닫기).
    Context(usize),
}

/// 탭 바 위젯(멀티라인 — 폭 초과 시 줄바꿈). 활성 탭 = 패널 배경 + 상단 accent 줄(원본 규약).
pub struct TabBar {
    titles: Vec<String>,
    /// 탭별 잠금(닫기 제외 — 원본 TAB-MENU) 표시. titles와 인덱스 정렬(부족분=false).
    locked: Vec<bool>,
    /// 탭 고정(📌 — 핀 그룹 앞 정렬은 호스트 몫, 사용자 요청 07-15).
    pinned: Vec<bool>,
    active: usize,
    /// 활성 패널 여부 — 비활성 패널은 accent 줄을 흐리게(테두리 색).
    focused: bool,
    bounds: Rect,
    row_h: i32,
    pad_x: i32,
    /// 한 줄 높이(호스트 = PanelMetrics.tab_h). 0 = bounds 높이 전체(단일 줄 폴백).
    line_h: i32,
    hover: Option<usize>,
    /// 드래그 재정렬 상태: (현재 잡은 탭 인덱스, 프레스 좌표, 이동 시작 여부).
    drag: Option<(usize, Point, bool)>,
    pending: Option<TabAction>,
    /// 페인트 시 캐시한 탭별 rect + [+] 버튼 rect(멀티라인 — 줄바꿈 반영).
    ranges: RefCell<HitRanges>,
    /// 페인트가 계산한 필요 줄 수(멀티라인) — 호스트 레이아웃이 높이 산정에 사용.
    lines: Cell<usize>,
    /// 줄 수 변경 표지(1회성) — 호스트가 수거해 재레이아웃.
    lines_changed: Cell<bool>,
}

/// (탭별 rect, [+] 버튼 rect) — 페인트가 채우고 히트 테스트가 읽는다.
type HitRanges = (Vec<Rect>, Rect);

/// 닫기(×) 히트 존 폭 배수(pad_x 기준).
const CLOSE_ZONE_PADS: i32 = 3;

/// 드래그 시작 임계(px — 가로/세로 공통).
const DRAG_THRESHOLD: i32 = 8;

impl TabBar {
    pub fn new(row_h: i32, pad_x: i32) -> Self {
        TabBar {
            titles: Vec::new(),
            locked: Vec::new(),
            pinned: Vec::new(),
            active: 0,
            focused: true,
            bounds: Rect::default(),
            row_h: row_h.max(1),
            pad_x,
            line_h: 0,
            hover: None,
            drag: None,
            pending: None,
            ranges: RefCell::new((Vec::new(), Rect::default())),
            lines: Cell::new(1),
            lines_changed: Cell::new(false),
        }
    }

    pub fn set_tabs(&mut self, titles: Vec<String>, active: usize, inv: &mut Invalidations) {
        self.titles = titles;
        self.active = active.min(self.titles.len().saturating_sub(1));
        self.hover = None;
        inv.push(self.bounds);
    }

    /// 탭별 잠금 표시 갱신(편의 UX ② — sync_chrome에서 titles와 함께).
    pub fn set_locked(&mut self, locked: Vec<bool>, inv: &mut Invalidations) {
        if self.locked != locked {
            self.locked = locked;
            inv.push(self.bounds);
        }
    }

    pub fn set_pinned(&mut self, pinned: Vec<bool>, inv: &mut Invalidations) {
        if self.pinned != pinned {
            self.pinned = pinned;
            inv.push(self.bounds);
        }
    }

    /// 탭 본체 히트(× 버튼 여부 무시) — 호스트의 더블클릭 동작 라우팅용(07-15).
    pub fn tab_index_at(&self, x: i32, y: i32) -> Option<usize> {
        self.tab_at(x, y).map(|(i, _)| i)
    }

    pub fn set_focused(&mut self, focused: bool, inv: &mut Invalidations) {
        if self.focused != focused {
            self.focused = focused;
            inv.push(self.bounds);
        }
    }

    pub fn set_metrics(&mut self, row_h: i32, pad_x: i32, inv: &mut Invalidations) {
        self.row_h = row_h.max(1);
        self.pad_x = pad_x;
        inv.push(self.bounds);
    }

    /// 한 줄 높이 지정(멀티라인 — 호스트의 tab_h. 레이아웃 높이 = line_h × lines).
    pub fn set_line_height(&mut self, h: i32, inv: &mut Invalidations) {
        if self.line_h != h.max(0) {
            self.line_h = h.max(0);
            inv.push(self.bounds);
        }
    }

    /// 마지막 페인트가 계산한 필요 줄 수(≥1) — 호스트 레이아웃의 탭 바 높이 산정용.
    pub fn lines(&self) -> usize {
        self.lines.get().max(1)
    }

    /// 줄 수 변경 표지 수거(1회성) — `true`면 호스트가 재레이아웃해야 한다.
    pub fn take_lines_changed(&self) -> bool {
        self.lines_changed.replace(false)
    }

    /// 호스트가 수거할 탭 동작(1회성).
    pub fn take_action(&mut self) -> Option<TabAction> {
        self.pending.take()
    }

    fn tab_at(&self, x: i32, y: i32) -> Option<(usize, bool)> {
        if !self.bounds.contains(Point { x, y }) {
            return None;
        }
        let (tabs, _) = &*self.ranges.borrow();
        for (i, r) in tabs.iter().enumerate() {
            if r.w > 0 && r.contains(Point { x, y }) {
                // 오른쪽 닫기(×) 존
                let close = x >= r.right() - self.pad_x * CLOSE_ZONE_PADS;
                return Some((i, close));
            }
        }
        None
    }

    fn plus_at(&self, x: i32, y: i32) -> bool {
        if !self.bounds.contains(Point { x, y }) {
            return false;
        }
        let (_, plus) = *self.ranges.borrow();
        plus.w > 0 && plus.contains(Point { x, y })
    }

    /// 탭 바 안 빈 공간(탭·[+] 밖) 판정 — 더블클릭=새 탭(원본 F20, QA 07-14).
    pub fn empty_area_at(&self, x: i32, y: i32) -> bool {
        self.bounds.contains(Point { x, y }) && self.tab_at(x, y).is_none() && !self.plus_at(x, y)
    }
}

impl Widget for TabBar {
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
                if let Some((i, close)) = self.tab_at(x, y) {
                    let locked = self.locked.get(i).copied().unwrap_or(false);
                    self.pending = Some(if close && self.titles.len() > 1 && !locked {
                        // 마지막 탭·잠긴 탭은 닫기 불가(원본 TAB-MENU)
                        TabAction::Close(i)
                    } else {
                        TabAction::Switch(i)
                    });
                    if !close {
                        // 본문 프레스 = 드래그 재정렬 후보(멀티라인 — 상하 이동 포함)
                        self.drag = Some((i, Point { x, y }, false));
                    }
                    inv.push(self.bounds);
                } else if self.plus_at(x, y) {
                    self.pending = Some(TabAction::New);
                    inv.push(self.bounds);
                }
            }
            InputEvent::RightDown { x, y } => {
                if let Some((i, _)) = self.tab_at(x, y) {
                    // 우클릭 메뉴는 호스트 몫(새 탭/잠금/복제/닫기 — 원본 TAB-MENU)
                    self.pending = Some(TabAction::Context(i));
                }
            }
            InputEvent::MouseMove { x, y } => {
                // 드래그 재정렬(편의 UX ②): 임계(8px) 이동 후, **대상 탭의 x 중간점을
                // 통과한 순간에만 스냅**(QA 07-14 — 탭 사이로 넘어갈 때). 멀티라인
                // (07-20)에서는 다른 줄의 탭 위도 같은 규칙 — 줄바꿈 흐름상 아래 줄 =
                // 항상 뒤 인덱스라 중간점 규칙이 그대로 성립(위/아래 드래그 이동).
                // 호스트가 이동을 반영(set_tabs)하면 잡은 인덱스를 목적지로 갱신해 연속 드래그.
                if let Some((from, press, started)) = self.drag {
                    let begun = started
                        || (x - press.x).abs() > DRAG_THRESHOLD
                        || (y - press.y).abs() > DRAG_THRESHOLD;
                    if begun {
                        if let Some((to, _)) = self.tab_at(x, y) {
                            let crossed = to != from && {
                                let r = self.ranges.borrow().0[to];
                                let mid = r.x + r.w / 2;
                                // 뒤(오른쪽·아래 줄)로: 대상 탭 중간을 지나야 · 앞도 대칭
                                (to > from && x >= mid) || (to < from && x <= mid)
                            };
                            if crossed {
                                self.pending = Some(TabAction::Move { from, to });
                                self.drag = Some((to, Point { x, y }, true));
                                inv.push(self.bounds);
                                return;
                            }
                        }
                        self.drag = Some((from, press, true));
                    }
                    return;
                }
                let hover = self.tab_at(x, y).map(|(i, _)| i);
                if hover != self.hover {
                    self.hover = hover;
                    inv.push(self.bounds);
                }
            }
            InputEvent::MouseUp { .. } => {
                self.drag = None;
            }
            _ => {}
        }
    }

    fn paint(&self, ctx: &mut dyn DrawCtx, theme: &Theme) {
        ctx.select_font(crate::FontSlot::Base, false, false); // 폰트 슬롯(X-12)
        let b = self.bounds;
        ctx.fill_rect(b, theme.tab_bar_bg);
        // 한 줄 높이 — 미지정(0)은 bounds 전체(구 단일 줄 동작 유지)
        let lh = if self.line_h > 0 {
            self.line_h.min(b.h.max(1))
        } else {
            b.h.max(1)
        };
        let ty_of = |cell: Rect| cell.y + (cell.h - (cell.h * 4) / 5) / 2;

        let mut tabs = Vec::with_capacity(self.titles.len());
        let mut x = b.x;
        let mut row = 0i32;
        let close_w = self.pad_x * CLOSE_ZONE_PADS;
        for (i, title) in self.titles.iter().enumerate() {
            let locked = self.locked.get(i).copied().unwrap_or(false);
            let pinned = self.pinned.get(i).copied().unwrap_or(false);
            let pin_title; // 고정 표지(원본 TAB-MENU 핀 — 사용자 요청 07-15)
            let title = if pinned {
                pin_title = format!("📌{title}");
                &pin_title
            } else {
                title
            };
            let shown = if locked {
                format!("🔒{title}") // 잠금 표지(원본 TAB-MENU)
            } else {
                title.clone()
            };
            let text_w = ctx.text_width(&shown);
            let w = (text_w + self.pad_x * 2 + close_w).min(b.w.max(1)).max(1);
            // 멀티라인(07-20): 남은 폭에 안 들어가면 다음 줄로(줄 첫 탭은 그대로)
            if x + w > b.right() && x > b.x {
                row += 1;
                x = b.x;
            }
            let cell = Rect::new(x, b.y + row * lh, w, lh);
            // bounds 아래로 넘친 줄은 그리지 않음(호스트 재레이아웃 전 1프레임) —
            // rect는 캐시해 줄 수 계산·재레이아웃 후 히트에 대비
            let visible = cell.y < b.bottom();
            let active = i == self.active;
            let bg = if active {
                theme.panel_bg
            } else if self.hover == Some(i) {
                theme.header_bg
            } else {
                theme.tab_bar_bg
            };
            if visible {
                let ty = ty_of(cell);
                ctx.text_opaque(cell.x + self.pad_x, ty, cell, &shown, theme.text, bg);
                // 닫기 × (탭 오른쪽 — 잠긴 탭은 흐린 표시 유지·클릭은 전환으로)
                let close_x = cell.right() - close_w + self.pad_x;
                ctx.text_opaque(
                    close_x,
                    ty,
                    Rect::new(cell.right() - close_w, cell.y, close_w, cell.h),
                    if locked { " " } else { "×" },
                    theme.text_dim,
                    bg,
                );
                if active {
                    // 활성 탭 상단 accent 줄(비활성 패널은 흐리게 — 활성 패널 시각화)
                    let line = if self.focused {
                        theme.accent
                    } else {
                        theme.border
                    };
                    ctx.fill_rect(Rect::new(cell.x, cell.y, cell.w, 2), line);
                }
            }
            tabs.push(cell);
            x += w;
        }
        // [+] 새 탭 버튼 — 마지막 탭 뒤(폭 부족 시 다음 줄)
        let plus_w = ctx.text_width("+") + self.pad_x * 2;
        if x + plus_w > b.right() && x > b.x {
            row += 1;
            x = b.x;
        }
        let plus = Rect::new(x, b.y + row * lh, plus_w.min(b.w.max(1)), lh);
        if plus.w > 0 && plus.y < b.bottom() {
            ctx.text_opaque(
                plus.x + self.pad_x,
                ty_of(plus),
                plus,
                "+",
                theme.text_dim,
                theme.tab_bar_bg,
            );
        }
        // 하단 경계선
        ctx.fill_rect(Rect::new(b.x, b.bottom() - 1, b.w, 1), theme.border);
        *self.ranges.borrow_mut() = (tabs, plus);
        // 필요 줄 수 캐시 + 변경 표지(호스트 재레이아웃 트리거 — take_lines_changed)
        let lines = (row + 1).max(1) as usize;
        if self.lines.replace(lines) != lines {
            self.lines_changed.set(true);
        }
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

    fn bar_sized(titles: &[&str], active: usize, w: i32, h: i32) -> (TabBar, Invalidations) {
        let mut inv = Invalidations::default();
        let mut t = TabBar::new(20, 6);
        t.set_line_height(22, &mut inv);
        t.set_bounds(Rect::new(0, 0, w, h), &mut inv);
        t.set_tabs(
            titles.iter().map(|s| s.to_string()).collect(),
            active,
            &mut inv,
        );
        t.paint(&mut Probe, &Theme::dark());
        (t, inv)
    }

    fn bar(titles: &[&str], active: usize) -> (TabBar, Invalidations) {
        bar_sized(titles, active, 600, 22)
    }

    fn down(t: &mut TabBar, inv: &mut Invalidations, x: i32) {
        down_at(t, inv, x, 5);
    }

    fn down_at(t: &mut TabBar, inv: &mut Invalidations, x: i32, y: i32) {
        t.on_event(
            &InputEvent::MouseDown {
                x,
                y,
                shift: false,
                ctrl: false,
            },
            inv,
        );
    }

    #[test]
    fn click_switches_close_zone_closes_and_plus_creates() {
        let (mut t, mut inv) = bar(&["alpha", "beta"], 0);
        // 탭0: 폭 = 5*8+12+18 = 70 → [0,70). 본문 클릭 = 전환
        down(&mut t, &mut inv, 10);
        assert_eq!(t.take_action(), Some(TabAction::Switch(0)));
        // 탭1 [70,132): 닫기 존 = [132-18, 132)
        down(&mut t, &mut inv, 120);
        assert_eq!(t.take_action(), Some(TabAction::Close(1)));
        // [+] = [132, 132+20)
        down(&mut t, &mut inv, 140);
        assert_eq!(t.take_action(), Some(TabAction::New));
        // 빈 영역 무시
        down(&mut t, &mut inv, 500);
        assert_eq!(t.take_action(), None);
    }

    #[test]
    fn last_tab_close_zone_switches_instead() {
        let (mut t, mut inv) = bar(&["only"], 0);
        // 탭0 [0, 4*8+12+18=62), 닫기 존 클릭
        down(&mut t, &mut inv, 55);
        assert_eq!(
            t.take_action(),
            Some(TabAction::Switch(0)),
            "마지막 탭은 닫기 불가"
        );
    }

    #[test]
    fn hover_tracks_tabs() {
        let (mut t, mut inv) = bar(&["alpha", "beta"], 0);
        t.on_event(&InputEvent::MouseMove { x: 80, y: 5 }, &mut inv);
        assert_eq!(t.hover, Some(1));
        t.on_event(&InputEvent::MouseMove { x: 80, y: 50 }, &mut inv);
        assert_eq!(t.hover, None);
    }

    #[test]
    fn tabs_wrap_to_multiple_lines_and_report_count() {
        // 탭 폭 = 5*8+12+18 = 70 · [+] = 20. 폭 150 = 줄당 2탭 →
        // 4탭 = 2줄 + [+] 3번째 줄 아님(2줄째 [70..140) 뒤 [140..150) 부족 → 3줄)
        let (t, _) = bar_sized(&["alpha", "betaa", "gamma", "delta"], 0, 150, 66);
        assert_eq!(t.lines(), 3, "2탭×2줄 + [+] 줄");
        assert!(t.take_lines_changed(), "1→3줄 변경 표지");
        assert!(!t.take_lines_changed(), "표지는 1회성");
        // 줄별 히트: 탭2는 2번째 줄 [0,70)×[22,44)
        assert_eq!(t.tab_index_at(10, 30), Some(2));
        assert_eq!(t.tab_index_at(10, 5), Some(0));
    }

    #[test]
    fn drag_moves_across_lines() {
        // 폭 150 = 줄당 2탭: 0줄=[탭0,탭1] · 1줄=[탭2,탭3]
        let (mut t, mut inv) = bar_sized(&["alpha", "betaa", "gamma", "delta"], 0, 150, 66);
        // 탭0 본문 프레스 → 아래 줄 탭2 중간점(x=35) 통과 드래그 = Move{0→2}
        down_at(&mut t, &mut inv, 10, 5);
        assert_eq!(t.take_action(), Some(TabAction::Switch(0)));
        t.on_event(&InputEvent::MouseMove { x: 40, y: 30 }, &mut inv);
        assert_eq!(
            t.take_action(),
            Some(TabAction::Move { from: 0, to: 2 }),
            "아래 줄로 드래그 이동"
        );
        // 계속 위 줄 탭0 위치로 되돌리면 Move{2→0}
        t.on_event(&InputEvent::MouseMove { x: 20, y: 5 }, &mut inv);
        assert_eq!(t.take_action(), Some(TabAction::Move { from: 2, to: 0 }));
        t.on_event(&InputEvent::MouseUp { x: 20, y: 5 }, &mut inv);
    }
}
