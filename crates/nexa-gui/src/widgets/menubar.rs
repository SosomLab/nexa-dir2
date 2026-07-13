//! 메뉴 바 — 원본 docs/20 §2 최상단 드롭다운 메뉴의 커스텀 드로잉 구현.
//! 도메인 비종속: 메뉴 정의(제목·항목·명령 id·체크 상태)만 알고, 실행은
//! `take_command()`로 호스트에 통지. 드롭다운은 **오버레이**(창 1개 원칙 —
//! 콘텐츠 위에 나중에 그림, 자식 HWND 없음).

use std::cell::RefCell;

use crate::draw::DrawCtx;
use crate::event::InputEvent;
use crate::geom::{Point, Rect};
use crate::theme::Theme;
use crate::widget::{Invalidations, Widget};

/// 메뉴 항목 — `id`는 호스트가 해석하는 명령 식별자.
#[derive(Clone, Debug)]
pub struct MenuItem {
    pub id: u32,
    pub label: String,
    /// 단축키 표시(오른쪽 정렬 힌트 텍스트).
    pub shortcut: String,
    /// 체크 표시(토글 항목 — `Some(true)` = ✓).
    pub checked: Option<bool>,
    pub separator: bool,
}

impl MenuItem {
    pub fn new(id: u32, label: impl Into<String>, shortcut: impl Into<String>) -> Self {
        MenuItem {
            id,
            label: label.into(),
            shortcut: shortcut.into(),
            checked: None,
            separator: false,
        }
    }
    pub fn checked(mut self, on: bool) -> Self {
        self.checked = Some(on);
        self
    }
    pub fn separator() -> Self {
        MenuItem {
            id: 0,
            label: String::new(),
            shortcut: String::new(),
            checked: None,
            separator: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Menu {
    pub title: String,
    pub items: Vec<MenuItem>,
}

pub struct MenuBar {
    menus: Vec<Menu>,
    bounds: Rect,
    row_h: i32,
    pad_x: i32,
    /// 열린 메뉴 인덱스(드롭다운 표시 중).
    open: Option<usize>,
    hover_title: Option<usize>,
    hover_item: Option<usize>,
    pending: Option<u32>,
    /// 페인트 캐시: 제목 x 범위·드롭다운 rect(항목 높이 = row_h).
    title_ranges: RefCell<Vec<(i32, i32)>>,
    drop_rect: RefCell<Rect>,
}

impl MenuBar {
    pub fn new(menus: Vec<Menu>, row_h: i32, pad_x: i32) -> Self {
        MenuBar {
            menus,
            bounds: Rect::default(),
            row_h: row_h.max(1),
            pad_x,
            open: None,
            hover_title: None,
            hover_item: None,
            pending: None,
            title_ranges: RefCell::new(Vec::new()),
            drop_rect: RefCell::new(Rect::default()),
        }
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    pub fn take_command(&mut self) -> Option<u32> {
        self.pending.take()
    }

    pub fn set_metrics(&mut self, row_h: i32, pad_x: i32, inv: &mut Invalidations) {
        self.row_h = row_h.max(1);
        self.pad_x = pad_x;
        inv.push(self.bounds);
    }

    /// 메뉴 정의 교체(i18n 언어 전환 등) — 열린 드롭다운은 닫고 전체 무효화. 배치·지표 유지.
    pub fn set_menus(&mut self, menus: Vec<Menu>, inv: &mut Invalidations) {
        self.close(inv);
        self.menus = menus;
        self.hover_title = None;
        self.title_ranges.borrow_mut().clear();
        inv.push(self.bounds);
    }

    /// 토글 항목 체크 상태 갱신(id 기준).
    pub fn set_checked(&mut self, id: u32, on: bool, inv: &mut Invalidations) {
        for m in &mut self.menus {
            for it in &mut m.items {
                if it.id == id && it.checked.is_some() {
                    it.checked = Some(on);
                }
            }
        }
        inv.push(self.bounds);
    }

    /// 드롭다운 닫기(외부 클릭·Esc·명령 실행 후).
    pub fn close(&mut self, inv: &mut Invalidations) {
        if self.open.take().is_some() {
            self.hover_item = None;
            inv.push(*self.drop_rect.borrow());
            inv.push(self.bounds);
        }
    }

    fn title_at(&self, x: i32, y: i32) -> Option<usize> {
        if !self.bounds.contains(Point { x, y }) {
            return None;
        }
        self.title_ranges
            .borrow()
            .iter()
            .position(|&(lo, hi)| x >= lo && x < hi)
    }

    /// 열리는 드롭다운의 **보수적 무효 영역**(스트립 아래·항목 수 기준 높이·전폭).
    /// 정확한 rect(폭=텍스트 측정)는 paint에서야 결정되므로, 열기/전환 시점의 무효화는
    /// 이 근사를 쓴다 — 이전 캐시(빈/다른 메뉴 rect)만 push하면 첫 프레임의 BitBlt가
    /// 무효 영역에 잘려 드롭다운이 깨져 보인다(QA 07-13: 첫 클릭 깨짐·마우스 이동 시 정상).
    fn drop_area_estimate(&self, menu: usize) -> Rect {
        let h = self.menus[menu].items.len() as i32 * self.row_h + 2;
        Rect::new(self.bounds.x, self.bounds.bottom(), self.bounds.w, h)
    }

    /// 드롭다운 항목 인덱스(열려 있을 때).
    fn item_at(&self, x: i32, y: i32) -> Option<usize> {
        let open = self.open?;
        let rect = *self.drop_rect.borrow();
        if !rect.contains(Point { x, y }) {
            return None;
        }
        let idx = ((y - rect.y) / self.row_h) as usize;
        (idx < self.menus[open].items.len()).then_some(idx)
    }
}

impl Widget for MenuBar {
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
                if let Some(i) = self.title_at(x, y) {
                    // 제목 클릭: 열기/닫기 토글
                    self.open = if self.open == Some(i) { None } else { Some(i) };
                    self.hover_item = None;
                    inv.push(self.bounds);
                    inv.push(*self.drop_rect.borrow()); // 이전 드롭다운 지우기
                    if let Some(open) = self.open {
                        inv.push(self.drop_area_estimate(open)); // 새 드롭다운 영역(근사)
                    }
                } else if let Some(item) = self.item_at(x, y) {
                    let open = self.open.unwrap();
                    let it = &self.menus[open].items[item];
                    if !it.separator {
                        self.pending = Some(it.id);
                    }
                    self.close(inv);
                } else if self.is_open() {
                    self.close(inv); // 외부 클릭 = 닫기
                }
            }
            InputEvent::MouseMove { x, y } => {
                let ht = self.title_at(x, y);
                let hi = self.item_at(x, y);
                if ht != self.hover_title || hi != self.hover_item {
                    // 열린 상태에서 다른 제목 hover = 그 메뉴로 전환(표준 메뉴 UX)
                    if let (Some(t), Some(_)) = (ht, self.open) {
                        if self.open != Some(t) {
                            self.open = Some(t);
                            inv.push(*self.drop_rect.borrow()); // 이전 드롭다운 지우기
                            inv.push(self.drop_area_estimate(t)); // 새 드롭다운 영역(근사)
                        }
                    }
                    self.hover_title = ht;
                    self.hover_item = hi;
                    inv.push(self.bounds);
                    if self.is_open() {
                        inv.push(*self.drop_rect.borrow());
                    }
                }
            }
            _ => {}
        }
    }

    fn paint(&self, ctx: &mut dyn DrawCtx, theme: &Theme) {
        let b = self.bounds;
        let ty = b.y + (b.h - (b.h * 4) / 5) / 2;
        ctx.fill_rect(b, theme.chrome_bg);

        // 제목 스트립
        let mut ranges = Vec::with_capacity(self.menus.len());
        let mut x = b.x + self.pad_x;
        for (i, m) in self.menus.iter().enumerate() {
            let w = ctx.text_width(&m.title) + self.pad_x * 2;
            let cell = Rect::new(x, b.y, w.min((b.right() - x).max(0)), b.h);
            let bg = if self.open == Some(i) {
                theme.sel_bg
            } else if self.hover_title == Some(i) {
                theme.header_bg
            } else {
                theme.chrome_bg
            };
            if cell.w > 0 {
                ctx.text_opaque(cell.x + self.pad_x, ty, cell, &m.title, theme.text, bg);
            }
            ranges.push((cell.x, cell.x + w));
            x += w;
        }
        ctx.fill_rect(Rect::new(b.x, b.bottom() - 1, b.w, 1), theme.border);
        *self.title_ranges.borrow_mut() = ranges;

        // 드롭다운(오버레이 — 콘텐츠 위)
        let Some(open) = self.open else {
            *self.drop_rect.borrow_mut() = Rect::default();
            return;
        };
        let items = &self.menus[open].items;
        let (tx, _) = self.title_ranges.borrow()[open];
        // 폭 = 라벨+단축키 최대치
        let mut w = 0;
        for it in items {
            let iw = ctx.text_width(&it.label)
                + ctx.text_width(&it.shortcut)
                + self.pad_x * 6
                + ctx.text_width("✓");
            w = w.max(iw);
        }
        let rect = Rect::new(tx, b.bottom(), w, self.row_h * items.len() as i32);
        for (i, it) in items.iter().enumerate() {
            let iy = rect.y + i as i32 * self.row_h;
            let cell = Rect::new(rect.x, iy, rect.w, self.row_h);
            if it.separator {
                ctx.fill_rect(cell, theme.field_bg);
                ctx.fill_rect(
                    Rect::new(
                        cell.x + self.pad_x,
                        iy + self.row_h / 2,
                        cell.w - self.pad_x * 2,
                        1,
                    ),
                    theme.border,
                );
                continue;
            }
            let bg = if self.hover_item == Some(i) {
                theme.sel_bg
            } else {
                theme.field_bg
            };
            let ity = iy + (self.row_h - (self.row_h * 4) / 5) / 2;
            // 체크 마크 열
            let check = match it.checked {
                Some(true) => "✓",
                _ => "",
            };
            ctx.text_opaque(cell.x + self.pad_x, ity, cell, check, theme.accent, bg);
            let label_x = cell.x + self.pad_x * 2 + ctx.text_width("✓");
            let label_rc = Rect::new(label_x, iy, cell.right() - label_x, self.row_h);
            ctx.text_opaque(label_x, ity, label_rc, &it.label, theme.text, bg);
            // 단축키(우측 정렬)
            if !it.shortcut.is_empty() {
                let sw = ctx.text_width(&it.shortcut);
                let sx = cell.right() - self.pad_x - sw;
                ctx.text_opaque(
                    sx,
                    ity,
                    Rect::new(sx, iy, sw, self.row_h),
                    &it.shortcut,
                    theme.text_dim,
                    bg,
                );
            }
        }
        // 테두리
        ctx.fill_rect(Rect::new(rect.x, rect.y, rect.w, 1), theme.border);
        ctx.fill_rect(
            Rect::new(rect.x, rect.bottom() - 1, rect.w, 1),
            theme.border,
        );
        ctx.fill_rect(Rect::new(rect.x, rect.y, 1, rect.h), theme.border);
        ctx.fill_rect(Rect::new(rect.right() - 1, rect.y, 1, rect.h), theme.border);
        *self.drop_rect.borrow_mut() = rect;
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

    fn menus() -> Vec<Menu> {
        vec![
            Menu {
                title: "파일".into(),
                items: vec![
                    MenuItem::new(1, "새 탭", "Ctrl+T"),
                    MenuItem::separator(),
                    MenuItem::new(2, "종료", ""),
                ],
            },
            Menu {
                title: "보기".into(),
                items: vec![MenuItem::new(10, "숨김 파일", "Ctrl+H").checked(true)],
            },
        ]
    }

    fn bar() -> (MenuBar, Invalidations) {
        let mut inv = Invalidations::default();
        let mut m = MenuBar::new(menus(), 20, 6);
        m.set_bounds(Rect::new(0, 0, 800, 22), &mut inv);
        m.paint(&mut Probe, &Theme::dark());
        (m, inv)
    }

    fn down(m: &mut MenuBar, inv: &mut Invalidations, x: i32, y: i32) {
        m.on_event(
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
    fn open_invalidates_dropdown_area_on_first_frame() {
        // 열기 직후 무효 영역이 스트립 아래 드롭다운 높이를 덮어야 함 — 이전엔 빈 drop_rect만
        // push돼 첫 프레임 BitBlt가 잘려 깨져 보임(QA 07-13)
        let (mut m, _) = bar();
        let mut inv = Invalidations::default();
        down(&mut m, &mut inv, 10, 5); // "파일"(항목 3개) 열기 — paint 전
        let need_bottom = 22 + 3 * 20 + 2;
        assert!(
            inv.drain().any(|r| r.y <= 22 && r.bottom() >= need_bottom),
            "드롭다운 예상 영역이 무효화돼야 첫 프레임에 표시"
        );
    }

    #[test]
    fn open_click_item_emits_command_and_closes() {
        let (mut m, mut inv) = bar();
        down(&mut m, &mut inv, 10, 5); // "파일" 열기
        assert!(m.is_open());
        m.paint(&mut Probe, &Theme::dark()); // 드롭다운 rect 캐시
        down(&mut m, &mut inv, 20, 22 + 10); // 항목 0("새 탭")
        assert_eq!(m.take_command(), Some(1));
        assert!(!m.is_open());
    }

    #[test]
    fn separator_click_closes_without_command() {
        let (mut m, mut inv) = bar();
        down(&mut m, &mut inv, 10, 5);
        m.paint(&mut Probe, &Theme::dark());
        down(&mut m, &mut inv, 20, 22 + 20 + 10); // 항목 1 = 구분선
        assert_eq!(m.take_command(), None);
        assert!(!m.is_open());
    }

    #[test]
    fn outside_click_closes_and_title_toggles() {
        let (mut m, mut inv) = bar();
        down(&mut m, &mut inv, 10, 5);
        assert!(m.is_open());
        m.paint(&mut Probe, &Theme::dark());
        down(&mut m, &mut inv, 400, 300); // 외부
        assert!(!m.is_open());
        down(&mut m, &mut inv, 10, 5);
        down(&mut m, &mut inv, 10, 5); // 같은 제목 재클릭 = 토글 닫기
        assert!(!m.is_open());
    }

    #[test]
    fn set_checked_updates_toggle_items() {
        let (mut m, mut inv) = bar();
        m.set_checked(10, false, &mut inv);
        assert_eq!(m.menus[1].items[0].checked, Some(false));
    }
}
