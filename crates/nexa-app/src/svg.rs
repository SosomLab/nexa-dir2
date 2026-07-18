//! svg — **임베드 아이콘용 최소 SVG 서브셋 파서**(07-18 사용자 요청: "svg
//! 방식도 적용되도록"). 외부 crate 0 규약(DR-8)에 따라 자체 구현 — 신뢰된
//! 임베드 자산 전용이며 일반 SVG 호환을 목표로 하지 않는다.
//!
//! ## 지원 서브셋
//! - 루트 `<svg>`: `viewBox`(필수) · `stroke-width`(기본 1) — 그 외 표현
//!   속성(stroke 색·linecap 등)은 렌더러 고정 규약(라운드 캡/조인·단색 잉크).
//! - 요소: `rect`(x/y/width/height/rx) · `circle`(cx/cy/r) ·
//!   `line`(x1/y1/x2/y2) · `polyline`(points) ·
//!   `path`(`d` = M/m·L/l·H/h·V/v·C/c·Z/z) ·
//!   `text`(x/y[베이스라인]/font-size/font-weight/text-anchor=middle —
//!   글꼴 지정은 무시, 렌더러 고정 산세리프).
//! - 도형 채색 = 루트 `fill` 지시(`none`/부재 = **스트로크**, 색 지정 =
//!   **채움** — 07-19 SYNC 아이콘). 텍스트는 항상 채움. 미지 요소는 건너뜀.
//!
//! 파싱 결과는 플랫폼 중립 [`Doc`](드로 op 목록) — 래스터는
//! [gdipctx](crate::ctl::gdipctx) `svg_to_hicon`(Windows 전용)이 수행.

/// 경로 세그먼트(절대 좌표로 정규화).
#[derive(Debug, Clone, PartialEq)]
pub enum Seg {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    /// 3차 베지어(제어 1·제어 2·끝).
    CurveTo([(f32, f32); 3]),
    Close,
}

/// 드로 op — 좌표는 viewBox 기준(렌더러가 스케일).
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// 라운드 사각 스트로크(`rx` 0 = 직각).
    Rect { x: f32, y: f32, w: f32, h: f32, rx: f32 },
    Circle { cx: f32, cy: f32, r: f32 },
    Line { x1: f32, y1: f32, x2: f32, y2: f32 },
    Polyline(Vec<(f32, f32)>),
    Path(Vec<Seg>),
    /// 텍스트(`y` = 베이스라인 — SVG 규약). `middle` = 수평 중앙 앵커.
    Text {
        x: f32,
        y: f32,
        size: f32,
        bold: bool,
        middle: bool,
        content: String,
    },
}

/// 문서 내 한 요소 — op + 색 오버라이드(요소 `stroke`/`fill`의 `#RRGGBB`.
/// `currentColor`/부재 = `None` → 렌더러 잉크. 알파는 잉크 것을 따름 —
/// 비활성 흐림이 오버라이드 색에도 적용).
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub op: Op,
    pub color: Option<u32>,
}

/// 파싱된 문서 — viewBox `(x, y, w, h)` + 루트 스트로크 폭 + 요소 목록.
#[derive(Debug, Clone, PartialEq)]
pub struct Doc {
    pub viewbox: (f32, f32, f32, f32),
    pub stroke_width: f32,
    /// 루트 `fill` 채움 모드(true = 도형 채움·false = 스트로크).
    pub fill: bool,
    pub ops: Vec<Element>,
}

/// SVG 텍스트 파싱. viewBox 없음/형식 오류 = `None`(오류 격리 — 아이콘 공백).
pub fn parse(svg: &str) -> Option<Doc> {
    let mut doc = Doc {
        viewbox: (0.0, 0.0, 0.0, 0.0),
        stroke_width: 1.0,
        fill: false,
        ops: Vec::new(),
    };
    let mut seen_root = false;
    for chunk in svg.split('<').skip(1) {
        let Some(gt) = chunk.find('>') else {
            continue; // 닫히지 않은 조각(말미 공백 등) — 건너뜀
        };
        let tag = chunk[..gt].trim_end_matches('/').trim();
        let inner = &chunk[gt + 1..]; // 태그 뒤 내용(text 요소의 본문)
        let (name, attrs) = match tag.split_once(char::is_whitespace) {
            Some((n, a)) => (n, a),
            None => (tag, ""),
        };
        match name {
            "svg" => {
                let vb = attr(attrs, "viewBox")?;
                let v: Vec<f32> = vb
                    .split([' ', ','])
                    .filter(|s| !s.is_empty())
                    .filter_map(|s| s.parse().ok())
                    .collect();
                if v.len() != 4 || v[2] <= 0.0 || v[3] <= 0.0 {
                    return None;
                }
                doc.viewbox = (v[0], v[1], v[2], v[3]);
                if let Some(sw) = attr(attrs, "stroke-width").and_then(|s| s.parse().ok()) {
                    doc.stroke_width = sw;
                }
                doc.fill = attr(attrs, "fill").is_some_and(|f| f != "none");
                seen_root = true;
            }
            "rect" => doc.ops.push(Element {
                op: Op::Rect {
                    x: num(attrs, "x"),
                    y: num(attrs, "y"),
                    w: num(attrs, "width"),
                    h: num(attrs, "height"),
                    rx: num(attrs, "rx"),
                },
                color: elem_color(attrs),
            }),
            "circle" => doc.ops.push(Element {
                op: Op::Circle {
                    cx: num(attrs, "cx"),
                    cy: num(attrs, "cy"),
                    r: num(attrs, "r"),
                },
                color: elem_color(attrs),
            }),
            "line" => doc.ops.push(Element {
                op: Op::Line {
                    x1: num(attrs, "x1"),
                    y1: num(attrs, "y1"),
                    x2: num(attrs, "x2"),
                    y2: num(attrs, "y2"),
                },
                color: elem_color(attrs),
            }),
            "polyline" => {
                let pts: Vec<f32> = attr(attrs, "points")
                    .unwrap_or_default()
                    .split([' ', ','])
                    .filter(|s| !s.is_empty())
                    .filter_map(|s| s.parse().ok())
                    .collect();
                let pairs: Vec<(f32, f32)> =
                    pts.chunks_exact(2).map(|c| (c[0], c[1])).collect();
                if pairs.len() >= 2 {
                    doc.ops.push(Element {
                        op: Op::Polyline(pairs),
                        color: elem_color(attrs),
                    });
                }
            }
            "path" => {
                if let Some(d) = attr(attrs, "d") {
                    let segs = parse_path(&d)?;
                    if !segs.is_empty() {
                        doc.ops.push(Element {
                            op: Op::Path(segs),
                            color: elem_color(attrs),
                        });
                    }
                }
            }
            "text" => {
                let content = inner.trim().to_string();
                if !content.is_empty() {
                    doc.ops.push(Element {
                        op: Op::Text {
                            x: num(attrs, "x"),
                            y: num(attrs, "y"),
                            size: attr(attrs, "font-size")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(10.0),
                            bold: attr(attrs, "font-weight").is_some_and(|w| {
                                w == "bold" || w.parse::<i32>().is_ok_and(|n| n >= 600)
                            }),
                            middle: attr(attrs, "text-anchor").is_some_and(|a| a == "middle"),
                            content,
                        },
                        color: elem_color(attrs),
                    });
                }
            }
            _ => {} // 미지 요소(닫는 태그·주석 포함) — 건너뜀
        }
    }
    (seen_root && !doc.ops.is_empty()).then_some(doc)
}

/// 속성값 추출 — `k="v"` 형태(단순 스캔 — 임베드 자산 전용).
fn attr(attrs: &str, key: &str) -> Option<String> {
    let mut rest = attrs;
    while let Some(i) = rest.find(key) {
        // 키 경계 확인(예: "x"가 "rx"에 매칭되지 않게)
        let before_ok = i == 0
            || !rest[..i]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '-');
        let after = &rest[i + key.len()..];
        if before_ok {
            if let Some(v) = after.strip_prefix('=') {
                let v = v.trim_start();
                let quote = v.chars().next()?;
                if quote == '"' || quote == '\'' {
                    return v[1..].split(quote).next().map(str::to_string);
                }
            }
        }
        rest = &rest[i + key.len()..];
    }
    None
}

/// 요소 색 오버라이드 — `stroke`/`fill`의 `#RRGGBB`(6자리)만 인식.
/// `currentColor`·`none`·부재 = `None`(렌더러 잉크).
fn elem_color(attrs: &str) -> Option<u32> {
    for key in ["stroke", "fill"] {
        if let Some(v) = attr(attrs, key) {
            if let Some(hex) = v.strip_prefix('#') {
                if hex.len() == 6 {
                    if let Ok(rgb) = u32::from_str_radix(hex, 16) {
                        return Some(rgb);
                    }
                }
            }
        }
    }
    None
}

fn num(attrs: &str, key: &str) -> f32 {
    attr(attrs, key)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0)
}

/// path `d` 파싱 — 상대 명령은 절대 좌표로 정규화. 미지 명령 = `None`(전체 무효).
fn parse_path(d: &str) -> Option<Vec<Seg>> {
    let mut segs = Vec::new();
    let (mut cx, mut cy) = (0.0f32, 0.0f32); // 현재점
    let (mut sx, mut sy) = (0.0f32, 0.0f32); // 서브패스 시작(Z 복귀)
    let mut nums: Vec<f32> = Vec::new();
    let mut cmd = ' ';
    // 토큰화: 명령 문자 기준 분할, 숫자는 공백/쉼표/부호 경계
    let mut it = d.chars().peekable();
    loop {
        // 다음 명령 문자까지의 숫자 나열 소비
        nums.clear();
        while let Some(&c) = it.peek() {
            if c.is_ascii_alphabetic() {
                break;
            }
            it.next();
            if c == ',' || c.is_whitespace() {
                continue;
            }
            // 숫자 시작(부호 포함) — 끝까지 읽기
            let mut s = String::new();
            s.push(c);
            while let Some(&n) = it.peek() {
                if n.is_ascii_digit() || n == '.' {
                    s.push(n);
                    it.next();
                } else {
                    break;
                }
            }
            nums.push(s.parse().ok()?);
        }
        if cmd != ' ' {
            apply_cmd(cmd, &nums, &mut segs, &mut cx, &mut cy, &mut sx, &mut sy)?;
        } else if !nums.is_empty() {
            return None; // 명령 없이 숫자 시작
        }
        match it.next() {
            Some(c) => cmd = c,
            None => break,
        }
    }
    Some(segs)
}

/// 한 명령 적용(반복 인자 허용 — 예: `L x1 y1 x2 y2`).
#[allow(clippy::too_many_arguments)]
fn apply_cmd(
    cmd: char,
    n: &[f32],
    segs: &mut Vec<Seg>,
    cx: &mut f32,
    cy: &mut f32,
    sx: &mut f32,
    sy: &mut f32,
) -> Option<()> {
    let rel = cmd.is_ascii_lowercase();
    match cmd.to_ascii_uppercase() {
        'M' => {
            for (i, p) in n.chunks_exact(2).enumerate() {
                let (x, y) = if rel {
                    (*cx + p[0], *cy + p[1])
                } else {
                    (p[0], p[1])
                };
                *cx = x;
                *cy = y;
                if i == 0 {
                    *sx = x;
                    *sy = y;
                    segs.push(Seg::MoveTo(x, y));
                } else {
                    segs.push(Seg::LineTo(x, y)); // 후속 쌍 = 암묵 LineTo(SVG 규약)
                }
            }
            (n.len() >= 2 && n.len().is_multiple_of(2)).then_some(())
        }
        'L' => {
            for p in n.chunks_exact(2) {
                let (x, y) = if rel {
                    (*cx + p[0], *cy + p[1])
                } else {
                    (p[0], p[1])
                };
                *cx = x;
                *cy = y;
                segs.push(Seg::LineTo(x, y));
            }
            (n.len() >= 2 && n.len().is_multiple_of(2)).then_some(())
        }
        'H' => {
            for &v in n {
                *cx = if rel { *cx + v } else { v };
                segs.push(Seg::LineTo(*cx, *cy));
            }
            (!n.is_empty()).then_some(())
        }
        'V' => {
            for &v in n {
                *cy = if rel { *cy + v } else { v };
                segs.push(Seg::LineTo(*cx, *cy));
            }
            (!n.is_empty()).then_some(())
        }
        'C' => {
            for p in n.chunks_exact(6) {
                let f = |i: usize| {
                    if rel {
                        (*cx + p[i], *cy + p[i + 1])
                    } else {
                        (p[i], p[i + 1])
                    }
                };
                let pts = [f(0), f(2), f(4)];
                *cx = pts[2].0;
                *cy = pts[2].1;
                segs.push(Seg::CurveTo(pts));
            }
            (n.len() >= 6 && n.len().is_multiple_of(6)).then_some(())
        }
        'Z' => {
            *cx = *sx;
            *cy = *sy;
            segs.push(Seg::Close);
            Some(())
        }
        _ => None, // 미지원 명령(A/Q/S/T…) — 문서 §서브셋
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 사용자 제공 플랫 보기 SVG(07-18) — 실자산 왕복.
    const FLAT: &str = include_str!("../assets/toolbar/view-flat.svg");

    #[test]
    fn parses_user_flat_icon() {
        let doc = parse(FLAT).expect("parse");
        assert_eq!(doc.viewbox, (0.0, 0.0, 32.0, 32.0));
        assert_eq!(doc.stroke_width, 2.0);
        assert_eq!(doc.ops.len(), 6, "rect 3 + path 3");
        assert_eq!(
            doc.ops[0].op,
            Op::Rect { x: 4.0, y: 4.0, w: 6.0, h: 6.0, rx: 1.0 }
        );
        assert_eq!(doc.ops[0].color, None, "currentColor = 잉크");
        assert_eq!(
            doc.ops[1].op,
            Op::Path(vec![Seg::MoveTo(15.0, 7.0), Seg::LineTo(28.0, 7.0)])
        );
    }

    #[test]
    fn path_relative_and_close() {
        let svg = r#"<svg viewBox="0 0 10 10"><path d="M1 1 l2 0 v2 h-2 z"/></svg>"#;
        let doc = parse(svg).unwrap();
        assert_eq!(
            doc.ops[0].op,
            Op::Path(vec![
                Seg::MoveTo(1.0, 1.0),
                Seg::LineTo(3.0, 1.0),
                Seg::LineTo(3.0, 3.0),
                Seg::LineTo(1.0, 3.0),
                Seg::Close,
            ])
        );
    }

    #[test]
    fn curve_and_negative_numbers() {
        let svg = r#"<svg viewBox="0 0 10 10"><path d="M0 5C1 -1,4 -1,5 5"/></svg>"#;
        let doc = parse(svg).unwrap();
        assert_eq!(
            doc.ops[0].op,
            Op::Path(vec![
                Seg::MoveTo(0.0, 5.0),
                Seg::CurveTo([(1.0, -1.0), (4.0, -1.0), (5.0, 5.0)]),
            ])
        );
    }

    #[test]
    fn rect_rx_boundary_not_confused_with_x() {
        let svg = r#"<svg viewBox="0 0 8 8"><rect x="1" y="2" width="3" height="4" rx="0.5"/></svg>"#;
        let doc = parse(svg).unwrap();
        assert_eq!(
            doc.ops[0].op,
            Op::Rect { x: 1.0, y: 2.0, w: 3.0, h: 4.0, rx: 0.5 }
        );
    }

    #[test]
    fn rejects_missing_viewbox_or_unknown_path_cmd() {
        assert!(parse(r#"<svg width="8"><rect x="1" width="2" height="2"/></svg>"#).is_none());
        assert!(parse(r#"<svg viewBox="0 0 8 8"><path d="M0 0 A 1 1 0 0 1 2 2"/></svg>"#).is_none());
    }

    #[test]
    fn fill_root_and_text_element() {
        let svg = concat!(
            r#"<svg viewBox="0 0 32 32" fill="currentColor">"#,
            r#"<path d="M2 11 L10 6 Z"/>"#,
            r#"<text x="16" y="26" text-anchor="middle" font-size="9" font-weight="700">SYNC</text>"#,
            "</svg>",
        );
        let doc = parse(svg).unwrap();
        assert!(doc.fill);
        assert_eq!(doc.ops.len(), 2);
        assert_eq!(
            doc.ops[1].op,
            Op::Text {
                x: 16.0,
                y: 26.0,
                size: 9.0,
                bold: true,
                middle: true,
                content: "SYNC".into(),
            }
        );
    }

    #[test]
    fn element_color_override() {
        // 07-19 패널 토글 켜짐 시안: rect = currentColor(잉크)·선 = #3D8BFF
        let svg = concat!(
            r##"<svg viewBox="0 0 32 32" fill="none" stroke="currentColor">"##,
            r##"<rect x="5" y="5" width="22" height="22" rx="3"/>"##,
            r##"<path d="M16 5 V27" stroke="#3D8BFF"/>"##,
            "</svg>",
        );
        let doc = parse(svg).unwrap();
        assert_eq!(doc.ops[0].color, None);
        assert_eq!(doc.ops[1].color, Some(0x3D8BFF));
    }

    #[test]
    fn stroke_root_keeps_fill_false() {
        let svg = r#"<svg viewBox="0 0 8 8" fill="none" stroke="currentColor"><line x1="0" y1="0" x2="4" y2="4"/></svg>"#;
        assert!(!parse(svg).unwrap().fill);
    }

    #[test]
    fn skips_unknown_elements() {
        let svg = r#"<svg viewBox="0 0 8 8"><title>x</title><g><line x1="0" y1="0" x2="4" y2="4"/></g></svg>"#;
        let doc = parse(svg).unwrap();
        assert_eq!(doc.ops.len(), 1);
    }
}
