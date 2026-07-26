//! markdown-viewer.wasm — Markdown(Mermaid) 뷰어 플러그인(ADR-0005 참조 구현).
//! `.wasm` 1개 = 배포 단위(전 OS/아키텍처 동일 동작). 사양 = Starlark판
//! markdown.star(07-26 QA 확정분)의 러스트 이식: 라인 태그 계약(`\u{2}종류|`)·
//! `<br/>` 줄바꿈·표 60칸·인라인 코드 백틱 유지·Mermaid flowchart SVG 이미지
//! (`\u{1}img|` 마커) + sequence/폴백 텍스트 아트.
//!
//! ABI(ADR-0005): export `nx_meta`/`nx_preview`(선두 4바이트 LE 길이 버퍼) ·
//! import env::read_text/render_svg/is_dark(호스트 — 대상 파일 한정·GDI+ 래스터).

// ── 호스트 import(ABI) ───────────────────────────────────────────────────

#[link(wasm_import_module = "env")]
extern "C" {
    fn read_text(ptr: *mut u8, cap: i32) -> i32;
    fn render_svg(sptr: *const u8, slen: i32, optr: *mut u8, ocap: i32) -> i32;
    fn is_dark() -> i32;
}

fn host_read_text(cap: usize) -> String {
    let mut buf = vec![0u8; cap];
    let n = unsafe { read_text(buf.as_mut_ptr(), cap as i32) }.max(0) as usize;
    buf.truncate(n.min(cap));
    String::from_utf8_lossy(&buf).into_owned()
}

fn host_render_svg(svg: &str) -> Option<String> {
    let mut out = vec![0u8; 1024];
    let n = unsafe {
        render_svg(svg.as_ptr(), svg.len() as i32, out.as_mut_ptr(), 1024)
    }
    .max(0) as usize;
    if n == 0 {
        return None;
    }
    out.truncate(n.min(1024));
    Some(String::from_utf8_lossy(&out).into_owned())
}

fn host_is_dark() -> bool {
    (unsafe { is_dark() }) != 0
}

/// 반환 버퍼(선두 4바이트 LE 길이) — 인스턴스는 호출당 1회라 leak = 무해.
fn ret(s: &str) -> *mut u8 {
    let b = s.as_bytes();
    let mut v = Vec::with_capacity(4 + b.len());
    v.extend_from_slice(&(b.len() as u32).to_le_bytes());
    v.extend_from_slice(b);
    Box::leak(v.into_boxed_slice()).as_mut_ptr()
}

#[no_mangle]
pub extern "C" fn nx_meta() -> *mut u8 {
    ret("markdown\nMarkdown Viewer\nmd,markdown,mdown,mkd")
}

#[no_mangle]
pub extern "C" fn nx_preview() -> *mut u8 {
    let src = host_read_text(READ_CAP);
    if src.is_empty() {
        return ret("lines\n(empty file)");
    }
    let lines = render(&src);
    ret(&format!("lines\n{}", lines.join("\n")))
}

// ── 공통 상수·유틸 (markdown.star 사양) ──────────────────────────────────

const READ_CAP: usize = 65536;
const LINE_CAP: usize = 400;
const CELL_CAP: usize = 60;

/// 표시 폭(콘솔 셀 — CJK/이모지 2칸).
fn dw(s: &str) -> usize {
    s.chars()
        .map(|c| {
            let u = c as u32;
            if matches!(
                u,
                0x1100..=0x115F | 0x2E80..=0x303E | 0x3041..=0x33FF | 0x3400..=0x4DBF
                    | 0x4E00..=0x9FFF | 0xA000..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF
                    | 0xFE30..=0xFE4F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6
                    | 0x1F300..=0x1FAFF | 0x20000..=0x3FFFD
            ) {
                2
            } else {
                1
            }
        })
        .sum()
}

fn trunc(s: &str, w: usize) -> String {
    if dw(s) <= w {
        return s.to_string();
    }
    let mut out = String::new();
    let mut acc = 0;
    for c in s.chars() {
        let cw = dw(&c.to_string());
        if acc + cw > w.saturating_sub(1) {
            break;
        }
        out.push(c);
        acc += cw;
    }
    out.push('…');
    out
}

/// mermaid/본문 라벨의 `<br/>` 계열 = 줄바꿈.
fn br_lines(label: &str) -> Vec<String> {
    let t = label
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<br>", "\n");
    let out: Vec<String> = t
        .split('\n')
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(str::to_string)
        .collect();
    if out.is_empty() {
        vec![label.to_string()]
    } else {
        out
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

// ── 인라인 정리(마커 제거·코드 백틱 유지·링크 라벨·이미지 🖼) ────────────

fn inline(s: &str) -> String {
    let cs: Vec<char> = s.chars().collect();
    let n = cs.len();
    let mut out = String::new();
    let mut i = 0;
    while i < n {
        let c = cs[i];
        if c == '\\' && i + 1 < n {
            out.push(cs[i + 1]);
            i += 2;
        } else if c == '`' {
            let r = cs[i..].iter().take_while(|&&x| x == '`').count();
            if let Some(close) = find_run(&cs, i + r, '`', r) {
                out.push('`');
                out.extend(&cs[i + r..close]);
                out.push('`');
                i = close + r;
            } else {
                out.push(c);
                i += 1;
            }
        } else if (c == '[') || (c == '!' && cs.get(i + 1) == Some(&'[')) {
            let lb = if c == '!' { i + 1 } else { i };
            if let Some((rb, ce)) = link_parts(&cs, lb) {
                if c == '!' {
                    out.push_str("🖼 ");
                }
                out.extend(&cs[lb + 1..rb]);
                i = ce + 1;
            } else {
                out.push(c);
                i += 1;
            }
        } else if c == '*' || c == '_' {
            let r = cs[i..].iter().take_while(|&&x| x == c).count().min(3);
            let word_ok = c == '*' || i == 0 || !cs[i - 1].is_alphanumeric();
            let inner_ok = i + r < n && cs[i + r] != ' ';
            let close = if word_ok && inner_ok {
                find_run(&cs, i + r + 1, c, r)
            } else {
                None
            };
            match close {
                Some(cl) if cs[cl - 1] != ' ' => {
                    out.extend(&cs[i + r..cl]);
                    i = cl + r;
                }
                _ => {
                    out.push(c);
                    i += 1;
                }
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

fn find_run(cs: &[char], from: usize, ch: char, r: usize) -> Option<usize> {
    (from..cs.len().saturating_sub(r - 1)).find(|&j| cs[j..j + r].iter().all(|&x| x == ch))
}

fn link_parts(cs: &[char], lb: usize) -> Option<(usize, usize)> {
    let rb = (lb + 1..cs.len()).find(|&j| cs[j] == ']')?;
    if cs.get(rb + 1) != Some(&'(') {
        return None;
    }
    let ce = (rb + 2..cs.len()).find(|&j| cs[j] == ')')?;
    Some((rb, ce))
}

// ── 표(박스 드로잉 — \u{2}mono|) ─────────────────────────────────────────

fn split_row(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').map(|c| c.trim().to_string()).collect()
}

fn is_sep(line: &str) -> bool {
    let t = line.trim();
    t.contains('|') && {
        let cells = split_row(t);
        !cells.is_empty()
            && cells.iter().all(|c| {
                !c.is_empty() && c.contains('-') && c.chars().all(|ch| ch == '-' || ch == ':')
            })
    }
}

fn render_table(header: &str, sep: &str, body: &[&str]) -> Vec<String> {
    let aligns: Vec<u8> = split_row(sep)
        .iter()
        .map(|c| match (c.starts_with(':'), c.ends_with(':')) {
            (true, true) => b'c',
            (false, true) => b'r',
            _ => b'l',
        })
        .collect();
    let mut rows: Vec<Vec<String>> = vec![split_row(header).iter().map(|c| inline(c)).collect()];
    for b in body.iter().take(50) {
        rows.push(split_row(b).iter().map(|c| inline(c)).collect());
    }
    let ncol = rows.iter().map(Vec::len).max().unwrap_or(1);
    let mut widths = vec![1usize; ncol];
    for r in &rows {
        for (k, c) in r.iter().enumerate() {
            widths[k] = widths[k].max(dw(c).min(CELL_CAP));
        }
    }
    let bar = |l: char, m: char, r: char| {
        let mut s = String::new();
        s.push(l);
        for (k, w) in widths.iter().enumerate() {
            for _ in 0..w + 2 {
                s.push('─');
            }
            s.push(if k + 1 == widths.len() { r } else { m });
        }
        s
    };
    let mut out = vec![bar('┌', '┬', '┐')];
    for (ri, r) in rows.iter().enumerate() {
        let mut s = String::from("│");
        for (k, w) in widths.iter().enumerate() {
            let cell = r.get(k).map(String::as_str).unwrap_or("");
            let t = trunc(cell, *w);
            let gap = w.saturating_sub(dw(&t));
            let (lp, rp) = match aligns.get(k).copied().unwrap_or(b'l') {
                b'r' => (gap, 0),
                b'c' => (gap / 2, gap - gap / 2),
                _ => (0, gap),
            };
            s.push(' ');
            for _ in 0..lp {
                s.push(' ');
            }
            s.push_str(&t);
            for _ in 0..rp {
                s.push(' ');
            }
            s.push_str(" │");
        }
        out.push(s);
        if ri == 0 {
            out.push(bar('├', '┼', '┤'));
        }
    }
    out.push(bar('└', '┴', '┘'));
    out
}

// ── Mermaid(flowchart SVG 이미지 + sequence/폴백 텍스트) ─────────────────

mod mm;

// ── 블록 파서 ────────────────────────────────────────────────────────────

fn list_marker(t: &str) -> Option<(String, &str)> {
    for m in ["- ", "* ", "+ "] {
        if let Some(rest) = t.strip_prefix(m) {
            for (tag, mark) in [("[ ] ", "☐ "), ("[x] ", "☑ "), ("[X] ", "☑ ")] {
                if let Some(r2) = rest.strip_prefix(tag) {
                    return Some((mark.into(), r2));
                }
            }
            return Some(("• ".into(), rest));
        }
    }
    let d = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if (1..=9).contains(&d) {
        for sep in [". ", ") "] {
            if let Some(r) = t[d..].strip_prefix(sep) {
                return Some((format!("{}. ", &t[..d]), r));
            }
        }
    }
    None
}

fn is_hr(t: &str) -> bool {
    let s: String = t.chars().filter(|c| !c.is_whitespace()).collect();
    s.len() >= 3 && ['-', '*', '_'].iter().any(|&c| s.chars().all(|x| x == c))
}

fn render(src: &str) -> Vec<String> {
    let mut lines: Vec<String> = src.lines().take(4000).map(str::to_string).collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0usize;
    let mut fence = String::new();
    let mut mermaid: Option<Vec<String>> = None;
    let mut last_blank = false;
    while i < lines.len() && out.len() < LINE_CAP {
        let mut line = lines[i].replace('\t', "    ");
        let mut t = line.trim().to_string();
        // 본문/리스트 <br/> = 줄바꿈(펜스 밖 — 후속 줄 2칸 들여쓰기)
        if fence.is_empty() && line.contains("<br") {
            let parts = br_lines(&line);
            if parts.len() > 1 {
                let mut repl = vec![parts[0].clone()];
                repl.extend(parts[1..].iter().map(|x| format!("  {x}")));
                lines.splice(i..i + 1, repl);
                line = lines[i].replace('\t', "    ");
                t = line.trim().to_string();
            }
        }
        if !fence.is_empty() {
            let fc = fence.chars().next().unwrap();
            let run = t.chars().take_while(|&c| c == fc).count();
            if run >= fence.len() && t[run..].trim().is_empty() {
                if let Some(buf) = mermaid.take() {
                    for x in mm::mermaid(&buf) {
                        if x.starts_with('\u{1}') {
                            out.push(x);
                        } else {
                            out.push(format!("\u{2}mono|{x}"));
                        }
                    }
                }
                fence.clear();
            } else if let Some(buf) = mermaid.as_mut() {
                buf.push(line.clone());
            } else {
                out.push(format!("\u{2}code|{line}"));
            }
            i += 1;
            continue;
        }
        if t.starts_with("```") || t.starts_with("~~~") {
            let fc = t.chars().next().unwrap();
            let run = t.chars().take_while(|&c| c == fc).count();
            fence = t[..run].to_string();
            let lang = t[run..].trim().to_ascii_lowercase();
            mermaid = (lang == "mermaid").then(Vec::new);
            i += 1;
            continue;
        }
        if t.is_empty() {
            if !last_blank && !out.is_empty() {
                out.push(String::new());
            }
            last_blank = true;
            i += 1;
            continue;
        }
        last_blank = false;
        // 표(구분행 필수)
        if t.contains('|') && i + 1 < lines.len() && is_sep(&lines[i + 1]) {
            let mut j = i + 2;
            let mut body: Vec<&str> = Vec::new();
            while j < lines.len() && lines[j].contains('|') && !lines[j].trim().is_empty() {
                body.push(&lines[j]);
                j += 1;
            }
            for x in render_table(&lines[i], &lines[i + 1], &body) {
                if out.len() >= LINE_CAP {
                    break;
                }
                out.push(format!("\u{2}mono|{x}"));
            }
            i = j;
            continue;
        }
        // 제목(종류 태그 — 뷰어가 굵게+괘선)
        if t.starts_with('#') {
            let h = t.chars().take_while(|&c| c == '#').count();
            if h <= 6 && t.len() > h && t.as_bytes()[h] == b' ' {
                let title = inline(t[h + 1..].trim());
                let tag = match h {
                    1 => "h1",
                    2 => "h2",
                    _ => "h3",
                };
                out.push(format!("\u{2}{tag}|{title}"));
                i += 1;
                continue;
            }
        }
        if is_hr(&t) {
            out.push("\u{2}hr|".into());
            i += 1;
            continue;
        }
        if t.starts_with('>') {
            let mut depth = 0usize;
            let mut rest = t.as_str();
            loop {
                let r = rest.trim_start();
                if let Some(r2) = r.strip_prefix('>') {
                    depth += 1;
                    rest = r2;
                } else {
                    rest = r;
                    break;
                }
            }
            out.push(format!(
                "\u{2}q|{}{}",
                "» ".repeat(depth.saturating_sub(1)),
                inline(rest)
            ));
            i += 1;
            continue;
        }
        if let Some((mark, rest)) = list_marker(&t) {
            let indent = line.len() - line.trim_start().len();
            out.push(format!(
                "{}{}{}",
                " ".repeat(indent.min(16)),
                mark,
                inline(rest)
            ));
            i += 1;
            continue;
        }
        out.push(inline(&line));
        i += 1;
    }
    if i < lines.len() {
        out.push("\u{2}q|… (표시 상한 — 이후 생략)".into());
    }
    out
}
