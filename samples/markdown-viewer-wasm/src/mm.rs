//! Mermaid — flowchart = SVG 이미지(호스트 render_svg → `\u{1}img|` 마커) ·
//! sequence = 텍스트 아트 · 미지원/상한 초과 = 원문 상자 폴백.
//! markdown.star(07-26 QA 확정: `<br/>` 멀티라인·따옴표 라벨·실린더 노드·
//! 캔버스 2000px·노드 24/간선 60)의 러스트 이식.

use crate::{br_lines, dw, esc, host_is_dark, host_render_svg, trunc};

const MAX_NODES: usize = 24;
const MAX_EDGES: usize = 60;

struct Flow {
    ids: Vec<String>,
    labels: Vec<String>,
    round: Vec<bool>,
    edges: Vec<(usize, usize, String)>,
}

impl Flow {
    fn intern(&mut self, id: &str, label: Option<String>, round: bool) -> usize {
        if let Some(i) = self.ids.iter().position(|x| x == id) {
            if let Some(l) = label {
                self.labels[i] = l;
                self.round[i] = round;
            }
            return i;
        }
        self.ids.push(id.to_string());
        self.labels.push(label.unwrap_or_else(|| id.to_string()));
        self.round.push(round);
        self.ids.len() - 1
    }
}

const SHAPES: [(&str, &str, bool, bool); 8] = [
    ("((", "))", true, false),
    ("([", "])", true, false),
    ("[(", ")]", true, false),
    ("[[", "]]", false, false),
    ("{{", "}}", false, true),
    ("[", "]", false, false),
    ("(", ")", true, false),
    ("{", "}", false, true),
];

fn node_span(t: &str) -> usize {
    let idl = t
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .count();
    let rest = &t[idl..];
    for (o, c, _, _) in SHAPES {
        if rest.starts_with(o) {
            if let Some(e) = rest[o.len()..].find(c) {
                return idl + o.len() + e + c.len();
            }
            break;
        }
    }
    idl
}

fn parse_node<'a>(r: &'a str, f: &mut Flow) -> Option<(usize, &'a str)> {
    let r = r.trim_start();
    let idl = r
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .count();
    if idl == 0 {
        return None;
    }
    let id = &r[..idl];
    let rest = &r[idl..];
    let mut label = None;
    let mut rnd = false;
    for (o, c, is_round, diamond) in SHAPES {
        if rest.starts_with(o) {
            if let Some(e) = rest[o.len()..].find(c) {
                let raw = rest[o.len()..o.len() + e].trim().trim_matches('"').to_string();
                label = Some(if diamond { format!("◇ {raw}") } else { raw });
                rnd = is_round;
            }
            break;
        }
    }
    let idx = f.intern(id, label, rnd);
    let span = node_span(r);
    Some((idx, &r[span..]))
}

fn match_arrow(r: &str) -> Option<(String, &str)> {
    for a in ["-.->", "==>", "-->", "---", "-.-", "==="] {
        if let Some(rr) = r.strip_prefix(a) {
            let t = rr.trim_start();
            if let Some(x) = t.strip_prefix('|') {
                if let Some(e) = x.find('|') {
                    return Some((x[..e].trim().trim_matches('"').to_string(), &x[e + 1..]));
                }
            }
            return Some((String::new(), rr));
        }
    }
    if let Some(x) = r.strip_prefix("--") {
        if let Some(e) = x.find("-->") {
            return Some((x[..e].trim().to_string(), &x[e + 3..]));
        }
    }
    None
}

fn source_box(src: &[String]) -> Vec<String> {
    let mut out = vec!["┌── mermaid".to_string()];
    out.extend(src.iter().map(|l| format!("│ {l}")));
    out.push("└──".into());
    out
}

/// mermaid 블록 진입점 — `\u{1}` 마커 라인(이미지) 또는 평문 아트 반환.
pub fn mermaid(src: &[String]) -> Vec<String> {
    let lines: Vec<String> = src
        .iter()
        .flat_map(|l| l.split(';'))
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("%%"))
        .map(str::to_string)
        .collect();
    let Some(head) = lines.first() else {
        return source_box(src);
    };
    let mut toks = head.split_whitespace();
    let art = match toks.next().unwrap_or("") {
        "graph" | "flowchart" => {
            let dir = toks.next().unwrap_or("TD").to_ascii_uppercase();
            flowchart(&lines[1..], dir == "LR" || dir == "RL")
        }
        "sequenceDiagram" => sequence(&lines[1..]),
        _ => None,
    };
    art.unwrap_or_else(|| source_box(src))
}

const SKIP: [&str; 8] = [
    "subgraph",
    "end",
    "style",
    "classDef",
    "class",
    "click",
    "linkStyle",
    "direction",
];

fn flowchart(body: &[String], horizontal: bool) -> Option<Vec<String>> {
    let mut f = Flow {
        ids: Vec::new(),
        labels: Vec::new(),
        round: Vec::new(),
        edges: Vec::new(),
    };
    for line in body {
        let first = line.split_whitespace().next().unwrap_or("");
        if SKIP.contains(&first) {
            continue;
        }
        let Some((mut prev, mut rest)) = parse_node(line, &mut f) else {
            continue;
        };
        while let Some((label, after)) = match_arrow(rest.trim_start()) {
            let Some((nxt, after2)) = parse_node(after, &mut f) else {
                break;
            };
            f.edges.push((prev, nxt, label));
            prev = nxt;
            rest = after2;
        }
    }
    let n = f.ids.len();
    if n == 0 || n > MAX_NODES || f.edges.len() > MAX_EDGES {
        return None;
    }
    // 레벨 = 최장 경로(반복 이완 — 사이클은 n 상한 수렴)
    let mut level = vec![0usize; n];
    for _ in 0..n {
        let mut changed = false;
        for &(a, b, _) in &f.edges {
            if a != b && level[a] + 1 > level[b] && level[a] < n {
                level[b] = level[a] + 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut used: Vec<usize> = level.clone();
    used.sort_unstable();
    used.dedup();
    let rowidx: Vec<usize> = level.iter().map(|l| used.binary_search(l).unwrap()).collect();
    let mut rows: Vec<Vec<usize>> = vec![Vec::new(); used.len()];
    for i in 0..n {
        rows[rowidx[i]].push(i);
    }
    // 멀티라인 라벨(<br/>) — 폭 = 최장 줄·높이 = 줄 수 비례(60칸 말줄임)
    let lls: Vec<Vec<String>> = f
        .labels
        .iter()
        .map(|l| br_lines(l).iter().map(|x| trunc(x, 60)).collect())
        .collect();
    let bw: Vec<i32> = lls
        .iter()
        .map(|ls| ls.iter().map(|x| dw(x)).max().unwrap_or(1) as i32 * 7 + 26)
        .collect();
    let bh: Vec<i32> = lls.iter().map(|ls| 14 + 20 * ls.len() as i32).collect();
    let (dark_bg, node_bg, border, fg, line_c) = if host_is_dark() {
        ("#191C21", "#262B33", "#3D8BFF", "#D6DAE0", "#8A919C")
    } else {
        ("#FFFFFF", "#F6F8FA", "#0969DA", "#1F2328", "#57606A")
    };
    let mut pos = vec![(0i32, 0i32); n];
    let (w_all, h_all);
    if horizontal {
        let gap = 46;
        let col_w: Vec<i32> = rows
            .iter()
            .map(|r| r.iter().map(|&i| bw[i]).max().unwrap_or(1))
            .collect();
        let col_h: Vec<i32> = rows
            .iter()
            .map(|r| r.iter().map(|&i| bh[i] + 14).sum::<i32>() - 14)
            .collect();
        h_all = col_h.iter().copied().max().unwrap_or(1) + 20;
        let mut x = 10;
        for (ci, row) in rows.iter().enumerate() {
            let mut yy = 10 + (h_all - 20 - col_h[ci]) / 2;
            for &node in row {
                pos[node] = (x, yy);
                yy += bh[node] + 14;
            }
            x += col_w[ci] + gap;
        }
        w_all = x - gap + 10;
    } else {
        let (gaph, gapv) = (28, 50);
        let row_w: Vec<i32> = rows
            .iter()
            .map(|r| r.iter().map(|&i| bw[i]).sum::<i32>() + gaph * (r.len() as i32 - 1))
            .collect();
        w_all = row_w.iter().copied().max().unwrap_or(1) + 20;
        let row_h: Vec<i32> = rows
            .iter()
            .map(|r| r.iter().map(|&i| bh[i]).max().unwrap_or(1))
            .collect();
        let mut yy = 10;
        for (li, row) in rows.iter().enumerate() {
            let mut xx = 10 + (w_all - 20 - row_w[li]) / 2;
            for &node in row {
                pos[node] = (xx, yy);
                xx += bw[node] + gaph;
            }
            yy += row_h[li] + gapv;
        }
        h_all = yy - gapv + 10;
    }
    if w_all > 2000 || h_all > 2000 {
        return None;
    }
    let mut p = vec![format!(
        r#"<svg viewBox="0 0 {w_all} {h_all}" stroke-width="1.5">"#
    )];
    p.push(format!(
        r#"<rect x="0" y="0" width="{w_all}" height="{h_all}" fill="{dark_bg}"/>"#
    ));
    for (a, b, lbl) in &f.edges {
        let (a, b) = (*a, *b);
        if rowidx[b] != rowidx[a] + 1 {
            continue;
        }
        let (lx, ly);
        if horizontal {
            let (x1, y1) = (pos[a].0 + bw[a], pos[a].1 + bh[a] / 2);
            let (x2, y2) = (pos[b].0, pos[b].1 + bh[b] / 2);
            let mx = (x1 + x2) / 2;
            p.push(format!(
                r#"<polyline points="{x1},{y1} {mx},{y1} {mx},{y2} {},{y2}" fill="none" stroke="{line_c}"/>"#,
                x2 - 2
            ));
            p.push(format!(
                r#"<path d="M {x2} {y2} L {} {} L {} {} Z" fill="{line_c}"/>"#,
                x2 - 8,
                y2 - 4,
                x2 - 8,
                y2 + 4
            ));
            lx = mx;
            ly = y1.min(y2) - 6;
        } else {
            let (x1, y1) = (pos[a].0 + bw[a] / 2, pos[a].1 + bh[a]);
            let (x2, y2) = (pos[b].0 + bw[b] / 2, pos[b].1);
            let my = (y1 + y2) / 2;
            p.push(format!(
                r#"<polyline points="{x1},{y1} {x1},{my} {x2},{my} {x2},{}" fill="none" stroke="{line_c}"/>"#,
                y2 - 2
            ));
            p.push(format!(
                r#"<path d="M {x2} {y2} L {} {} L {} {} Z" fill="{line_c}"/>"#,
                x2 - 4,
                y2 - 8,
                x2 + 4,
                y2 - 8
            ));
            lx = (x1 + x2) / 2 + 6;
            ly = my - 4;
        }
        let l = trunc(&br_lines(lbl).join(" "), 40);
        if !l.is_empty() {
            p.push(format!(
                r#"<text x="{lx}" y="{ly}" font-size="12" fill="{fg}">{}</text>"#,
                esc(&l)
            ));
        }
    }
    for i in 0..n {
        let rx = if f.round[i] { 16 } else { 6 };
        let (x, y) = pos[i];
        p.push(format!(
            r#"<rect x="{x}" y="{y}" width="{}" height="{}" rx="{rx}" fill="{node_bg}"/>"#,
            bw[i], bh[i]
        ));
        p.push(format!(
            r#"<rect x="{x}" y="{y}" width="{}" height="{}" rx="{rx}" fill="none" stroke="{border}"/>"#,
            bw[i], bh[i]
        ));
        for (li, ls) in lls[i].iter().enumerate() {
            p.push(format!(
                r#"<text x="{}" y="{}" font-size="13" text-anchor="middle" fill="{fg}">{}</text>"#,
                x + bw[i] / 2,
                y + 22 + 20 * li as i32,
                esc(ls)
            ));
        }
    }
    p.push("</svg>".into());
    let img = host_render_svg(&p.join(""))?;
    let rows_n = ((h_all / 22).clamp(3, 18)) as usize;
    let mut out = vec![format!("\u{1}img|{img}")];
    out.extend(std::iter::repeat_n("\u{1}pad".to_string(), rows_n - 1));
    for (a, b, l) in &f.edges {
        if rowidx[*b] != rowidx[*a] + 1 {
            let lbl = if l.is_empty() {
                String::new()
            } else {
                format!(" ({l})")
            };
            out.push(format!("· {} ─▶ {}{lbl}", f.labels[*a], f.labels[*b]));
        }
    }
    Some(out)
}

// ── sequence(텍스트 아트 — star판 이식) ─────────────────────────────────

fn sequence(body: &[String]) -> Option<Vec<String>> {
    const ARROWS: [&str; 8] = ["-->>", "->>", "--x", "--)", "-->", "-x", "-)", "->"];
    enum Row {
        Msg(usize, usize, String, bool),
        Mark(String),
    }
    let mut names: Vec<String> = Vec::new();
    let mut intern = |names: &mut Vec<String>, id: &str| -> usize {
        let id = id.trim();
        if let Some(i) = names.iter().position(|n| n == id) {
            return i;
        }
        names.push(id.to_string());
        names.len() - 1
    };
    let mut rows: Vec<Row> = Vec::new();
    for line in body {
        let first = line.split_whitespace().next().unwrap_or("");
        if matches!(first, "activate" | "deactivate" | "autonumber") {
            continue;
        }
        if let Some(rest) = line
            .strip_prefix("participant ")
            .or_else(|| line.strip_prefix("actor "))
        {
            let (id, disp) = rest
                .split_once(" as ")
                .map(|(a, b)| (a.trim(), b.trim()))
                .unwrap_or((rest.trim(), rest.trim()));
            let i = intern(&mut names, id);
            names[i] = disp.to_string();
            continue;
        }
        let low = first.to_ascii_lowercase();
        if low == "note" {
            let text = line.split_once(':').map(|(_, t)| t.trim()).unwrap_or("");
            rows.push(Row::Mark(format!("· {text}")));
            continue;
        }
        if matches!(
            low.as_str(),
            "loop" | "alt" | "opt" | "par" | "critical" | "break" | "rect"
        ) {
            rows.push(Row::Mark(format!("┌─ {line}")));
            continue;
        }
        if low == "else" {
            rows.push(Row::Mark(format!("├─ {line}")));
            continue;
        }
        if low == "end" {
            rows.push(Row::Mark("└─".into()));
            continue;
        }
        let mut best: Option<(usize, &str)> = None;
        for a in ARROWS {
            if let Some(pp) = line.find(a) {
                if best.is_none_or(|(bp, ba)| pp < bp || (pp == bp && a.len() > ba.len())) {
                    best = Some((pp, a));
                }
            }
        }
        let Some((pp, a)) = best else { continue };
        let from = intern(&mut names, &line[..pp]);
        let rest = &line[pp + a.len()..];
        let (target, text) = rest
            .split_once(':')
            .map(|(t, x)| (t, x.trim()))
            .unwrap_or((rest, ""));
        let to = intern(&mut names, target);
        rows.push(Row::Msg(from, to, text.to_string(), a.starts_with("--")));
    }
    if names.is_empty() || rows.is_empty() || names.len() > 6 || rows.len() > 40 {
        return None;
    }
    let bw: Vec<usize> = names.iter().map(|n| dw(n) + 4).collect();
    let mut cx = vec![(bw[0] / 2) as i32];
    for i in 1..names.len() {
        cx.push(cx[i - 1] + (((bw[i - 1] + bw[i]) / 2 + 4).max(14)) as i32);
    }
    let gw = (*cx.last().unwrap() + (bw[names.len() - 1] as i32 + 1) / 2 + 1) as usize;
    let gh = 3 + rows
        .iter()
        .map(|r| if matches!(r, Row::Msg(..)) { 2 } else { 1 })
        .sum::<usize>();
    if gw > 240 || gh > 160 {
        return None;
    }
    let mut g = vec![vec![' '; gw]; gh];
    let mut put = |g: &mut Vec<Vec<char>>, x: i32, y: usize, ch: char| {
        if y < g.len() && x >= 0 && (x as usize) < gw {
            g[y][x as usize] = ch;
        }
    };
    let put_str = |g: &mut Vec<Vec<char>>, x: i32, y: usize, s: &str| {
        let mut cxp = x;
        for ch in s.chars() {
            let cw = dw(&ch.to_string()) as i32;
            if cxp + cw > gw as i32 {
                break;
            }
            if y < g.len() && cxp >= 0 {
                g[y][cxp as usize] = ch;
                if cw == 2 && (cxp + 1) < gw as i32 {
                    g[y][(cxp + 1) as usize] = '\0';
                }
            }
            cxp += cw;
        }
    };
    for i in 0..names.len() {
        let (x, w) = (cx[i] - (bw[i] / 2) as i32, bw[i] as i32);
        for dx in 1..w - 1 {
            put(&mut g, x + dx, 0, '─');
            put(&mut g, x + dx, 2, '─');
        }
        put(&mut g, x, 0, '┌');
        put(&mut g, x + w - 1, 0, '┐');
        put(&mut g, x, 2, '└');
        put(&mut g, x + w - 1, 2, '┘');
        put(&mut g, x, 1, '│');
        put(&mut g, x + w - 1, 1, '│');
        put_str(&mut g, x + 2, 1, &names[i]);
    }
    let mut y = 3usize;
    for r in &rows {
        let h = if matches!(r, Row::Msg(..)) { 2 } else { 1 };
        for dy in 0..h {
            for &c in &cx {
                put(&mut g, c, y + dy, '│');
            }
        }
        match r {
            Row::Mark(m) => {
                put_str(&mut g, 0, y, m);
                y += 1;
            }
            Row::Msg(from, to, text, dashed) => {
                if from == to {
                    put(&mut g, cx[*from] + 1, y + 1, '⟲');
                    if !text.is_empty() {
                        put_str(&mut g, cx[*from] + 2, y, text);
                    }
                } else {
                    let (lo, hi) = (cx[*from].min(cx[*to]), cx[*from].max(cx[*to]));
                    if !text.is_empty() {
                        let start = ((lo + hi) / 2 - dw(text) as i32 / 2).max(lo + 1);
                        put_str(&mut g, start, y, text);
                    }
                    for x in lo + 1..hi {
                        put(&mut g, x, y + 1, if *dashed { '╌' } else { '─' });
                    }
                    if cx[*to] > cx[*from] {
                        put(&mut g, hi - 1, y + 1, '▶');
                    } else {
                        put(&mut g, lo + 1, y + 1, '◀');
                    }
                }
                y += 2;
            }
        }
    }
    let mut out: Vec<String> = g
        .into_iter()
        .map(|row| {
            row.into_iter()
                .filter(|&c| c != '\0')
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect();
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    Some(out)
}
