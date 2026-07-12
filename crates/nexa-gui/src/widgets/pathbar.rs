//! 계층 경로 바 — 원본 NexaPathBar(docs/27) α 이식.
//! 브레드크럼(세그먼트 클릭 이동·현재 비활성·드라이브 `C:`→`C:\`·hover) + 편집 모드
//! (우클릭 진입·타이핑·Enter 제출·Esc 취소). **네비게이션 비종속** — 위젯은 파일시스템을
//! 모르고 `take_navigation()`으로 통지만, 이동·검증은 호스트가 수행(§3 규약).
//! 후속(β/γ): 오버플로 `…`·UNC·자동완성(PATH-SUG)·형제 ▾ 드롭다운·드롭 타깃.

use std::cell::RefCell;

use crate::draw::DrawCtx;
use crate::event::InputEvent;
use crate::geom::{Point, Rect};
use crate::theme::Theme;
use crate::widget::{Invalidations, Widget};

/// 경로 세그먼트(라벨 + 클릭 시 이동할 전체 경로).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Segment {
    pub label: String,
    pub full: String,
}

/// 로컬 FS 세그먼터(원본 IPathSegmenter 기본 구현): 드라이브 `C:` → `C:\`, 이후 폴더 누적.
/// `\`·`/` 모두 허용, 조립은 `\`. UNC·VFS 스킴은 후속(§4).
pub fn split_path(path: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut acc = String::new();
    for part in path.split(['\\', '/']).filter(|p| !p.is_empty()) {
        if acc.is_empty() {
            // 첫 세그먼트: 드라이브(`C:`)면 루트는 `C:\`
            acc = if part.ends_with(':') {
                format!("{part}\\")
            } else {
                part.to_string()
            };
            out.push(Segment {
                label: part.to_string(),
                full: acc.clone(),
            });
        } else {
            if !acc.ends_with('\\') {
                acc.push('\\');
            }
            acc.push_str(part);
            out.push(Segment {
                label: part.to_string(),
                full: acc.clone(),
            });
        }
    }
    out
}

/// 계층 경로 바 위젯. 높이 1줄 고정(§5 — 경량 measure).
pub struct PathBar {
    path: String,
    segments: Vec<Segment>,
    bounds: Rect,
    row_h: i32,
    pad_x: i32,
    hover: Option<usize>,
    /// 편집 모드 버퍼(Some = 편집 중). 커서는 끝 고정(α — 전체 편집은 β).
    edit: Option<String>,
    /// 호스트가 수거할 이동 요청(세그먼트 클릭·편집 제출).
    pending_nav: Option<String>,
    /// 페인트 시 계산한 세그먼트 x 범위(히트 테스트용 — 텍스트 측정은 DrawCtx에서만 가능).
    ranges: RefCell<Vec<(i32, i32)>>,
}

impl PathBar {
    pub fn new(path: impl Into<String>, row_h: i32, pad_x: i32) -> Self {
        let path = path.into();
        PathBar {
            segments: split_path(&path),
            path,
            bounds: Rect::default(),
            row_h: row_h.max(1),
            pad_x,
            hover: None,
            edit: None,
            pending_nav: None,
            ranges: RefCell::new(Vec::new()),
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn is_editing(&self) -> bool {
        self.edit.is_some()
    }

    /// IME 조합 창 배치용(M2-7) — 편집 중이면 (버퍼, 필드 rect, pad_x).
    /// 캐럿 = `rect.x + pad_x + text_width(버퍼)` (커서는 끝 고정 — α 편집 모델과 일치).
    pub fn edit_info(&self) -> Option<(&str, Rect, i32)> {
        self.edit.as_deref().map(|b| (b, self.bounds, self.pad_x))
    }

    /// 호스트가 수행할 이동 요청 수거(있으면 1회성).
    pub fn take_navigation(&mut self) -> Option<String> {
        self.pending_nav.take()
    }

    /// 호스트가 이동 완료 후 호출 — 브레드크럼 갱신·편집 종료.
    pub fn set_path(&mut self, path: impl Into<String>, inv: &mut Invalidations) {
        self.path = path.into();
        self.segments = split_path(&self.path);
        self.edit = None;
        self.hover = None;
        inv.push(self.bounds);
    }

    pub fn set_metrics(&mut self, row_h: i32, pad_x: i32, inv: &mut Invalidations) {
        self.row_h = row_h.max(1);
        self.pad_x = pad_x;
        inv.push(self.bounds);
    }

    /// 편집 모드 진입(우클릭) — 버퍼 = 현재 경로(원본: 전체 선택 상태에 대응).
    pub fn begin_edit(&mut self, inv: &mut Invalidations) {
        self.edit = Some(self.path.clone());
        inv.push(self.bounds);
    }

    /// 편집 취소(Esc·포커스아웃) — 입력 무시·브레드크럼 복귀(§2).
    pub fn cancel_edit(&mut self, inv: &mut Invalidations) {
        if self.edit.take().is_some() {
            inv.push(self.bounds);
        }
    }

    /// 편집 제출(Enter) — 이동 요청으로 통지(검증·이동은 호스트).
    pub fn submit_edit(&mut self, inv: &mut Invalidations) {
        if let Some(buf) = self.edit.take() {
            let trimmed = buf.trim();
            if !trimmed.is_empty() {
                self.pending_nav = Some(trimmed.to_string());
            }
            inv.push(self.bounds);
        }
    }

    /// 편집 문자 입력(`'\u{8}'` = Backspace).
    pub fn edit_char(&mut self, c: char, inv: &mut Invalidations) {
        let Some(buf) = &mut self.edit else {
            return;
        };
        if c == '\u{8}' {
            buf.pop();
        } else if !c.is_control() {
            buf.push(c);
        }
        inv.push(self.bounds);
    }

    /// 좌표의 세그먼트 인덱스(페인트가 캐시한 범위 기준).
    fn segment_at(&self, x: i32, y: i32) -> Option<usize> {
        if !self.bounds.contains(Point { x, y }) {
            return None;
        }
        self.ranges
            .borrow()
            .iter()
            .position(|&(lo, hi)| x >= lo && x < hi)
    }
}

impl Widget for PathBar {
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
                if self.is_editing() {
                    return; // 편집 중 클릭은 호스트가 포커스아웃(cancel)으로 처리
                }
                if let Some(i) = self.segment_at(x, y) {
                    // 현재(마지막) 세그먼트는 비활성(§1-3)
                    if i + 1 < self.segments.len() {
                        self.pending_nav = Some(self.segments[i].full.clone());
                    }
                }
            }
            InputEvent::RightDown { x, y } => {
                if self.bounds.contains(Point { x, y }) && !self.is_editing() {
                    self.begin_edit(inv);
                }
            }
            InputEvent::MouseMove { x, y } => {
                let hover = if self.is_editing() {
                    None
                } else {
                    // 마지막(현재) 세그먼트는 hover 없음(§1-3)
                    self.segment_at(x, y)
                        .filter(|&i| i + 1 < self.segments.len())
                };
                if hover != self.hover {
                    self.hover = hover;
                    inv.push(self.bounds);
                }
            }
            _ => {}
        }
    }

    fn paint(&self, ctx: &mut dyn DrawCtx, theme: &Theme) {
        let b = self.bounds;
        let ty = b.y + (b.h - (b.h * 4) / 5) / 2;

        if let Some(buf) = &self.edit {
            // 편집 모드 — 필드 배경 + 버퍼 + 끝 커서(_) + accent 테두리(4변)
            let text = format!("{buf}_");
            ctx.text_opaque(b.x + self.pad_x, ty, b, &text, theme.text, theme.field_bg);
            ctx.fill_rect(Rect::new(b.x, b.y, b.w, 1), theme.accent);
            ctx.fill_rect(Rect::new(b.x, b.bottom() - 1, b.w, 1), theme.accent);
            ctx.fill_rect(Rect::new(b.x, b.y, 1, b.h), theme.accent);
            ctx.fill_rect(Rect::new(b.right() - 1, b.y, 1, b.h), theme.accent);
            self.ranges.borrow_mut().clear();
            return;
        }

        // 브레드크럼 — 세그먼트 + 구분자, hover 강조. x 범위 캐시(히트 테스트)
        let mut ranges = Vec::with_capacity(self.segments.len());
        let mut x = b.x + self.pad_x;
        let last = self.segments.len().saturating_sub(1);
        // 남은 영역 배경 먼저(세그먼트가 덮어씀)
        ctx.fill_rect(b, theme.chrome_bg);
        for (i, seg) in self.segments.iter().enumerate() {
            let w = ctx.text_width(&seg.label) + self.pad_x * 2;
            let cell = Rect::new(x, b.y, w.min(b.right() - x), b.h);
            if cell.w <= 0 {
                ranges.push((x, x)); // 화면 밖 — 빈 범위
                continue;
            }
            let bg = if self.hover == Some(i) {
                theme.sel_bg // hover 강조(클릭 가능 세그먼트만 — MouseMove에서 필터)
            } else {
                theme.chrome_bg
            };
            let fg = if i == last {
                theme.text
            } else {
                theme.text_dim
            };
            ctx.text_opaque(cell.x + self.pad_x, ty, cell, &seg.label, fg, bg);
            ranges.push((cell.x, cell.x + w));
            x += w;
            if i != last {
                let sep_w = ctx.text_width("›") + self.pad_x;
                let sep_cell = Rect::new(x, b.y, sep_w.min((b.right() - x).max(0)), b.h);
                if sep_cell.w > 0 {
                    ctx.text_opaque(
                        sep_cell.x,
                        ty,
                        sep_cell,
                        "›",
                        theme.text_dim,
                        theme.chrome_bg,
                    );
                }
                x += sep_w;
            }
        }
        // 하단 경계선(영역 구분 — 원본 docs/39 §2 "경계선 + 명도 차")
        ctx.fill_rect(Rect::new(b.x, b.bottom() - 1, b.w, 1), theme.border);
        *self.ranges.borrow_mut() = ranges;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Color;

    #[test]
    fn split_drive_and_folders() {
        let segs = split_path("C:\\Users\\kiros33");
        assert_eq!(segs.len(), 3);
        assert_eq!(
            (segs[0].label.as_str(), segs[0].full.as_str()),
            ("C:", "C:\\")
        );
        assert_eq!(segs[1].full, "C:\\Users");
        assert_eq!(segs[2].full, "C:\\Users\\kiros33");
        // 슬래시·끝 구분자 허용
        assert_eq!(split_path("C:/a/b/")[2].full, "C:\\a\\b");
        assert!(split_path("").is_empty());
    }

    struct Probe;
    impl DrawCtx for Probe {
        fn fill_rect(&mut self, _r: Rect, _c: Color) {}
        fn text_opaque(&mut self, _x: i32, _y: i32, _c: Rect, _t: &str, _f: Color, _b: Color) {}
        fn text_width(&mut self, text: &str) -> i32 {
            text.chars().count() as i32 * 8
        }
    }

    fn bar() -> (PathBar, Invalidations) {
        let mut inv = Invalidations::default();
        let mut p = PathBar::new("C:\\Users\\kiros33", 20, 6);
        p.set_bounds(Rect::new(0, 0, 600, 24), &mut inv);
        p.paint(&mut Probe, &Theme::dark()); // 히트 테스트 범위 캐시
        (p, inv)
    }

    #[test]
    fn segment_click_requests_navigation_but_last_is_inert() {
        let (mut p, mut inv) = bar();
        // 세그먼트 0("C:"): x = pad(6)..6+2*8+12=34
        p.on_event(
            &InputEvent::MouseDown {
                x: 10,
                y: 5,
                shift: false,
                ctrl: false,
            },
            &mut inv,
        );
        assert_eq!(p.take_navigation().as_deref(), Some("C:\\"));
        // 마지막 세그먼트(현재)는 무동작
        let last_x = p.ranges.borrow().last().unwrap().0 + 2;
        p.on_event(
            &InputEvent::MouseDown {
                x: last_x,
                y: 5,
                shift: false,
                ctrl: false,
            },
            &mut inv,
        );
        assert_eq!(p.take_navigation(), None);
    }

    #[test]
    fn right_click_edits_enter_submits_esc_cancels() {
        let (mut p, mut inv) = bar();
        p.on_event(&InputEvent::RightDown { x: 100, y: 5 }, &mut inv);
        assert!(p.is_editing());
        p.edit_char('\u{8}', &mut inv); // "…kiros3"
        p.edit_char('2', &mut inv);
        p.submit_edit(&mut inv);
        assert!(!p.is_editing());
        assert_eq!(p.take_navigation().as_deref(), Some("C:\\Users\\kiros32"));

        p.on_event(&InputEvent::RightDown { x: 100, y: 5 }, &mut inv);
        p.edit_char('x', &mut inv);
        p.cancel_edit(&mut inv); // Esc — 입력 무시·복귀(§2)
        assert!(!p.is_editing());
        assert_eq!(p.take_navigation(), None);
        assert_eq!(p.path(), "C:\\Users\\kiros33");
    }

    #[test]
    fn hover_tracks_clickable_segments_only() {
        let (mut p, mut inv) = bar();
        p.on_event(&InputEvent::MouseMove { x: 10, y: 5 }, &mut inv);
        assert_eq!(p.hover, Some(0));
        let last_x = p.ranges.borrow().last().unwrap().0 + 2;
        p.on_event(&InputEvent::MouseMove { x: last_x, y: 5 }, &mut inv);
        assert_eq!(p.hover, None, "현재 세그먼트는 hover 없음");
        p.on_event(&InputEvent::MouseMove { x: 10, y: 100 }, &mut inv);
        assert_eq!(p.hover, None, "영역 밖");
    }

    #[test]
    fn set_path_rebuilds_and_exits_edit() {
        let (mut p, mut inv) = bar();
        p.begin_edit(&mut inv);
        p.set_path("D:\\Work", &mut inv);
        assert!(!p.is_editing());
        assert_eq!(p.path(), "D:\\Work");
        assert_eq!(p.segments.len(), 2);
    }
}
