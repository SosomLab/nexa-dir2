//! 한 줄 텍스트 **편집 상태**(경로바 편집·인라인 리네임 공용 — 실기 QA 07-13).
//! 캐럿 이동(←→·Home/End·Shift 선택)·Ctrl+A·클릭 캐럿 배치·**세로바 캐럿**·
//! 오버플로 시 **캐럿 가시 끝 정렬**(긴 경로는 최근 폴더가 보이도록 앞이 잘림).
//!
//! 텍스트 측정은 paint에서만 가능(DrawCtx) — [`paint_field`](EditState::paint_field)가
//! 문자 경계 x 오프셋을 캐시하고, 클릭 히트([`click`](EditState::click))는 캐시를
//! 역참조한다(경로바 세그먼트 캐시와 동일 패턴).

use std::cell::RefCell;

use crate::draw::DrawCtx;
use crate::geom::Rect;
use crate::theme::Theme;

/// 편집 키(호스트 VK → 위젯 중립).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditKey {
    Left,
    Right,
    Home,
    End,
    /// Ctrl+A.
    SelectAll,
    /// Delete(전방 삭제).
    DeleteForward,
}

/// 편집 상태 — 버퍼(문자 단위)·캐럿(0..=len)·선택(anchor↔caret).
pub struct EditState {
    buf: Vec<char>,
    caret: usize,
    anchor: Option<usize>,
    /// 마우스 드래그 선택 진행 중(click~release).
    dragging: bool,
    /// paint 캐시: (필드 rect, 그리기 원점 x, 문자 경계 오프셋 0..=len) — 클릭 캐럿 배치용.
    layout: RefCell<(Rect, i32, Vec<i32>)>,
}

impl EditState {
    /// `select_all`=true면 전체 선택 상태로 시작(경로바 편집 진입 — 사용자 지시 07-13).
    pub fn new(text: &str, select_all: bool) -> Self {
        let buf: Vec<char> = text.chars().collect();
        let caret = buf.len();
        EditState {
            anchor: (select_all && !buf.is_empty()).then_some(0),
            caret,
            buf,
            dragging: false,
            layout: RefCell::new((Rect::default(), 0, Vec::new())),
        }
    }

    /// 앞에서 `sel_to` 문자까지 선택 상태로 시작(리네임 이름부 선택 — 탐색기 관례).
    pub fn with_selection_to(text: &str, sel_to: usize) -> Self {
        let mut s = Self::new(text, false);
        let to = sel_to.min(s.buf.len());
        s.anchor = (to > 0).then_some(0);
        s.caret = to;
        s
    }

    pub fn text(&self) -> String {
        self.buf.iter().collect()
    }

    /// 캐럿 앞 텍스트(IME 조합 창 배치 — 캐럿 x 계산용).
    pub fn text_before_caret(&self) -> String {
        self.buf[..self.caret].iter().collect()
    }

    /// 정렬된 선택 범위(문자 인덱스). 없으면 `None`.
    fn sel_range(&self) -> Option<(usize, usize)> {
        let a = self.anchor?;
        if a == self.caret {
            return None;
        }
        Some((a.min(self.caret), a.max(self.caret)))
    }

    /// 선택 구간 삭제. 지웠으면 `true`.
    fn delete_selection(&mut self) -> bool {
        let Some((a, b)) = self.sel_range() else {
            self.anchor = None;
            return false;
        };
        self.buf.drain(a..b);
        self.caret = a;
        self.anchor = None;
        true
    }

    /// 문자 입력 — 선택이 있으면 대체(표준 편집 모델).
    pub fn insert(&mut self, c: char) {
        self.delete_selection();
        self.buf.insert(self.caret, c);
        self.caret += 1;
    }

    /// Backspace — 선택이 있으면 선택 삭제.
    pub fn backspace(&mut self) {
        if !self.delete_selection() && self.caret > 0 {
            self.caret -= 1;
            self.buf.remove(self.caret);
        }
    }

    /// 키 처리. 비Shift 이동 중 선택이 있으면 선택 가장자리로 접기(표준 관례).
    pub fn key(&mut self, k: EditKey, shift: bool) {
        match k {
            EditKey::Left => {
                if let (false, Some((a, _))) = (shift, self.sel_range()) {
                    self.caret = a;
                    self.anchor = None;
                } else {
                    self.move_to(self.caret.saturating_sub(1), shift);
                }
            }
            EditKey::Right => {
                if let (false, Some((_, b))) = (shift, self.sel_range()) {
                    self.caret = b;
                    self.anchor = None;
                } else {
                    self.move_to((self.caret + 1).min(self.buf.len()), shift);
                }
            }
            EditKey::Home => self.move_to(0, shift),
            EditKey::End => self.move_to(self.buf.len(), shift),
            EditKey::SelectAll => {
                self.anchor = (!self.buf.is_empty()).then_some(0);
                self.caret = self.buf.len();
            }
            EditKey::DeleteForward => {
                if !self.delete_selection() && self.caret < self.buf.len() {
                    self.buf.remove(self.caret);
                }
            }
        }
    }

    fn move_to(&mut self, to: usize, shift: bool) {
        if shift {
            if self.anchor.is_none() {
                self.anchor = Some(self.caret);
            }
        } else {
            self.anchor = None;
        }
        self.caret = to;
    }

    /// 클릭 좌표가 필드 안인가(paint 캐시 기준 — paint 전이면 false).
    pub fn hit(&self, x: i32, y: i32) -> bool {
        let (rc, _, offs) = &*self.layout.borrow();
        !offs.is_empty() && rc.contains(crate::geom::Point { x, y })
    }

    /// 클릭 x → 최근접 문자 경계(paint 캐시 역참조). 없으면 `None`.
    fn index_at(&self, x: i32) -> Option<usize> {
        let (_, ox, offs) = &*self.layout.borrow();
        if offs.is_empty() {
            return None;
        }
        let rel = x - ox;
        let mut best = 0usize;
        let mut bd = i32::MAX;
        for (i, o) in offs.iter().enumerate() {
            let d = (o - rel).abs();
            if d < bd {
                bd = d;
                best = i;
            }
        }
        Some(best)
    }

    /// 마우스 누름 — 캐럿 배치 + 드래그 선택 시작점(anchor) 기록.
    pub fn click(&mut self, x: i32) {
        if let Some(i) = self.index_at(x) {
            self.caret = i;
            self.anchor = Some(i); // 드래그 선택 시작점(이동 없으면 release에서 해제)
            self.dragging = true;
        }
    }

    /// 마우스 드래그 — 시작점(anchor)부터 현재 x까지 선택 확장. 변화가 있었으면 `true`.
    pub fn drag(&mut self, x: i32) -> bool {
        if !self.dragging {
            return false;
        }
        match self.index_at(x) {
            Some(i) if i != self.caret => {
                self.caret = i;
                true
            }
            _ => false,
        }
    }

    /// 마우스 해제 — 드래그 종료(이동 없었으면 선택 해제 = 단순 클릭).
    pub fn release(&mut self) {
        self.dragging = false;
        if self.anchor == Some(self.caret) {
            self.anchor = None;
        }
    }

    /// 필드 페인트(공용) — 필드 배경·선택 하이라이트·텍스트·**세로바 캐럿**·accent 테두리.
    /// 텍스트가 넘치면 **캐럿이 보이도록 좌측을 잘라** 그린다(캐럿=끝이면 끝 정렬 —
    /// 긴 경로에서 최근 폴더 가시, 사용자 지시 07-13). 문자 경계 오프셋을 캐시한다.
    pub fn paint_field(&self, ctx: &mut dyn DrawCtx, rc: Rect, pad_x: i32, theme: &Theme) {
        let ty = rc.y + (rc.h - (rc.h * 4) / 5) / 2;
        // 문자 경계 오프셋 = **접두사 폭**(전체 문자열 레이아웃과 동일 커닝 — 문자별 합산은
        // 렌더 위치와 어긋나 캐럿 표시/클릭 매핑이 밀린다: 실기 QA 07-13)
        let mut offs = Vec::with_capacity(self.buf.len() + 1);
        offs.push(0);
        let mut prefix = String::new();
        for &c in &self.buf {
            prefix.push(c);
            offs.push(ctx.text_width(&prefix));
        }
        let total = *offs.last().unwrap();
        let avail = (rc.w - pad_x * 2 - 1).max(1);
        let caret_px = offs[self.caret.min(offs.len() - 1)];
        // 좌측 잘림량: 기본 끝 정렬, 캐럿이 항상 가시 범위에 들도록 보정
        let mut dx = if total > avail { total - avail } else { 0 };
        if caret_px < dx {
            dx = caret_px; // 캐럿이 왼쪽 밖 — 캐럿 기준으로 당김
        } else if caret_px - dx > avail {
            dx = caret_px - avail;
        }
        let x0 = rc.x + pad_x - dx;

        ctx.fill_rect(rc, theme.field_bg);
        // 잘림 시 첫 가시 문자 경계(렌더러가 좌측 클립을 보장하지 않으므로 문자 경계로 자름)
        let vis_from = offs.iter().position(|&o| o >= dx).unwrap_or(0);
        // 텍스트 런: [0,a)=일반 · [a,b)=선택 하이라이트 · [b,len)=일반.
        // text_opaque는 **clip 전체를 bg로 채우므로** 런 자신의 x 구간만 clip으로 넘긴다
        // (전체 rect를 넘기면 뒤 런이 앞 런을 지움 — 실기 QA 07-13 이름부 소실 원인).
        let (a, b) = self.sel_range().unwrap_or((0, 0));
        let runs = [
            (0usize, a, theme.field_bg),
            (a, b, theme.sel_bg),
            (b, self.buf.len(), theme.field_bg),
        ];
        for (s, e, bg) in runs {
            let s = s.max(vis_from);
            if s >= e {
                continue;
            }
            let rx = x0 + offs[s];
            let lo = rx.max(rc.x + 1);
            let hi = (x0 + offs[e]).min(rc.right() - 1);
            if hi <= lo {
                continue;
            }
            let run: String = self.buf[s..e].iter().collect();
            ctx.text_opaque(
                rx,
                ty,
                Rect::new(lo, rc.y + 1, hi - lo, rc.h - 2),
                &run,
                theme.text,
                bg,
            );
        }
        // 세로바 캐럿(사용자 지시 07-13 — `_` 대체)
        let cx = x0 + caret_px;
        if cx >= rc.x && cx < rc.right() {
            ctx.fill_rect(Rect::new(cx, rc.y + 2, 1, rc.h - 4), theme.text);
        }
        // accent 테두리(4변 — 기존 편집 필드 규약)
        ctx.fill_rect(Rect::new(rc.x, rc.y, rc.w, 1), theme.accent);
        ctx.fill_rect(Rect::new(rc.x, rc.bottom() - 1, rc.w, 1), theme.accent);
        ctx.fill_rect(Rect::new(rc.x, rc.y, 1, rc.h), theme.accent);
        ctx.fill_rect(Rect::new(rc.right() - 1, rc.y, 1, rc.h), theme.accent);
        *self.layout.borrow_mut() = (rc, x0, offs);
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
    fn select_all_start_replaces_on_insert() {
        let mut e = EditState::new("abc", true);
        e.insert('x'); // 전체 선택 → 대체
        assert_eq!(e.text(), "x");
        assert_eq!(e.caret, 1);
    }

    #[test]
    fn caret_moves_home_end_shift_selects() {
        let mut e = EditState::new("abcd", false); // 캐럿 = 끝
        e.key(EditKey::Left, false);
        e.key(EditKey::Left, true); // Shift+← → 선택 c..d?
        assert_eq!(e.sel_range(), Some((2, 3)));
        e.key(EditKey::Home, false);
        assert_eq!(e.caret, 0);
        assert_eq!(e.sel_range(), None, "비Shift 이동 = 선택 해제");
        e.key(EditKey::End, true);
        assert_eq!(e.sel_range(), Some((0, 4)));
        e.backspace(); // 선택 삭제
        assert_eq!(e.text(), "");
    }

    #[test]
    fn nonshift_left_collapses_to_selection_edge() {
        let mut e = EditState::new("abcd", true); // 전체 선택
        e.key(EditKey::Left, false);
        assert_eq!((e.caret, e.sel_range()), (0, None), "왼쪽 가장자리로 접힘");
        e.key(EditKey::SelectAll, false);
        e.key(EditKey::Right, false);
        assert_eq!(e.caret, 4, "오른쪽 가장자리");
    }

    #[test]
    fn delete_forward_and_stem_selection() {
        let mut e = EditState::with_selection_to("name.txt", 4); // 이름부 선택
        assert_eq!(e.sel_range(), Some((0, 4)));
        e.insert('x');
        assert_eq!(e.text(), "x.txt");
        e.key(EditKey::Home, false);
        e.key(EditKey::DeleteForward, false);
        assert_eq!(e.text(), ".txt");
    }

    #[test]
    fn click_places_caret_by_cached_layout() {
        let e = EditState::new("abcd", false);
        e.paint_field(&mut Probe, Rect::new(0, 0, 200, 24), 6, &Theme::dark());
        let mut e = e;
        e.click(6 + 17); // 폭 8/문자 — 경계 2(x=16)가 최근접
        assert_eq!(e.caret, 2);
        assert!(e.hit(10, 10) && !e.hit(10, 100));
    }

    #[test]
    fn drag_selects_range_click_alone_does_not() {
        let mut e = EditState::new("abcd", false);
        e.paint_field(&mut Probe, Rect::new(0, 0, 200, 24), 6, &Theme::dark());
        e.click(6 + 8); // 경계 1
        e.drag(6 + 8 * 3); // 경계 3까지 드래그
        e.release();
        assert_eq!(e.sel_range(), Some((1, 3)), "드래그 = 범위 선택");
        e.backspace(); // 선택 삭제
        assert_eq!(e.text(), "ad");

        // 단순 클릭(이동 없음)은 선택 없음
        e.paint_field(&mut Probe, Rect::new(0, 0, 200, 24), 6, &Theme::dark());
        e.click(6 + 8);
        e.release();
        assert_eq!(e.sel_range(), None, "클릭만 = 캐럿 배치");
        assert_eq!(e.caret, 1);
    }

    #[test]
    fn overflow_keeps_caret_visible_end_aligned() {
        // 40자 × 8px = 320px > 100px 필드 — 캐럿(끝)이 보이도록 끝 정렬
        let text = "a".repeat(40);
        let e = EditState::new(&text, false);
        e.paint_field(&mut Probe, Rect::new(0, 0, 100, 24), 6, &Theme::dark());
        let (_, x0, offs) = &*e.layout.borrow();
        let caret_x = x0 + offs[40];
        assert!(
            (0..100).contains(&caret_x),
            "캐럿 가시(끝 정렬): x={caret_x}"
        );
        assert!(*x0 < 0, "앞부분은 왼쪽으로 잘림");
    }
}
