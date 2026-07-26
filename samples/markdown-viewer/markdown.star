# markdown.star — Markdown 뷰어 플러그인 (MarkdownViewerPlugin 샘플)
#
# Nexa Dir 미리보기 플러그인의 **참조 구현**이다. 단일 .star 파일 = 배포 단위 —
# data\plugins\ 에 복사하면 즉시 동작(재빌드·설치 불요).
# 렌더 기준 캔버스 = 독립 미리보기 창(F3 — 콘솔 폰트 문자 그리드).
#
# 플러그인 계약(ADR-0004):
#   ID/NAME/EXTS  — 메타 선언. EXTS = 적용 확장자 **기본값**(스크립트 내부 지정).
#                   외부 재정의는 설정 preview_map(예: md:markdown) — 설정이 우선.
#   preview(file) — file.path / file.ext / file.size 속성 사용 가능.
#                   반환 {"lines": [str]} 또는 {"image": 경로}.
# 호스트 API:
#   read_text(n)  — 미리보기 대상 파일 앞 n바이트(UTF-8 lossy·호스트 상한).
#   disp_width(s) — 표시 폭(CJK/이모지 2칸) — 표·상자 정렬용.

ID = "markdown"
NAME = "Markdown Viewer"
EXTS = ["md", "markdown", "mdown", "mkd"]

_READ_CAP = 65536   # 읽기 상한(바이트)
_LINE_CAP = 400     # 출력 라인 상한
_CELL_CAP = 24      # 표 셀 표시 폭 상한

# ── 인라인 마커 정리(평문화) ────────────────────────────────────────────
# **b**/*i*/__b__/_i_ = 마커 제거 · `c` = ⟨c⟩ · [t](u) = t · ![a](u) = 🖼 a
# 재귀 금지(Starlark) — 단일 패스 인덱스 루프.

def _run_len(cs, i, ch):
    n = 0
    for j in range(i, len(cs)):
        if cs[j] != ch:
            break
        n += 1
    return n

def _find_run(cs, start, ch, r):
    # start부터 ch가 r개 연속인 첫 위치(-1 = 없음)
    for j in range(start, len(cs) - r + 1):
        ok = True
        for k in range(r):
            if cs[j + k] != ch:
                ok = False
                break
        if ok:
            return j
    return -1

def _inline(s):
    cs = list(s.elems())
    n = len(cs)
    out = []
    i = 0
    for _ in range(n + n + 4):  # while 대체 — 충분 상한
        if i >= n:
            break
        c = cs[i]
        if c == "\\" and i + 1 < n:
            out.append(cs[i + 1])
            i += 2
        elif c == "`":
            r = _run_len(cs, i, "`")
            close = _find_run(cs, i + r, "`", r)
            if close < 0:
                out.append(c)
                i += 1
            else:
                out.append("⟨" + "".join(cs[i + r:close]) + "⟩")
                i = close + r
        elif c == "!" and i + 1 < n and cs[i + 1] == "[":
            lb = i + 1
            rb = -1
            for j in range(lb + 1, n):
                if cs[j] == "]":
                    rb = j
                    break
            if rb > 0 and rb + 1 < n and cs[rb + 1] == "(":
                ce = -1
                for j in range(rb + 2, n):
                    if cs[j] == ")":
                        ce = j
                        break
                if ce > 0:
                    out.append("🖼 " + "".join(cs[lb + 1:rb]))
                    i = ce + 1
                else:
                    out.append(c)
                    i += 1
            else:
                out.append(c)
                i += 1
        elif c == "[":
            rb = -1
            for j in range(i + 1, n):
                if cs[j] == "]":
                    rb = j
                    break
            if rb > 0 and rb + 1 < n and cs[rb + 1] == "(":
                ce = -1
                for j in range(rb + 2, n):
                    if cs[j] == ")":
                        ce = j
                        break
                if ce > 0:
                    out.append("".join(cs[i + 1:rb]))
                    i = ce + 1
                else:
                    out.append(c)
                    i += 1
            else:
                out.append(c)
                i += 1
        elif c == "*" or c == "_":
            r = min(_run_len(cs, i, c), 3)
            word_ok = c == "*" or i == 0 or (not cs[i - 1].isalnum())
            inner_ok = i + r < n and cs[i + r] != " "
            close = _find_run(cs, i + r + 1, c, r) if (word_ok and inner_ok) else -1
            if close > 0 and cs[close - 1] != " ":
                out.append("".join(cs[i + r:close]))
                i = close + r
            else:
                out.append(c)
                i += 1
        else:
            out.append(c)
            i += 1
    return "".join(out)

# ── 표 렌더(박스 드로잉 — disp_width 정렬·CJK 2칸) ──────────────────────

def _split_row(line):
    t = line.strip()
    if t.startswith("|"):
        t = t[1:]
    if t.endswith("|"):
        t = t[:-1]
    return [c.strip() for c in t.split("|")]

def _is_sep(line):
    t = line.strip()
    if "|" not in t:
        return False
    cells = _split_row(t)
    if len(cells) == 0:
        return False
    for c in cells:
        if c == "" or "-" not in c:
            return False
        for ch in c.elems():
            if ch != "-" and ch != ":":
                return False
    return True

def _trunc(s, w):
    if disp_width(s) <= w:
        return s
    out = []
    acc = 0
    for ch in s.elems():
        cw = disp_width(ch)
        if acc + cw > w - 1:
            break
        out.append(ch)
        acc += cw
    return "".join(out) + "…"

def _pad(s, w, align):
    t = _trunc(s, w)
    gap = w - disp_width(t)
    if align == "r":
        return " " * gap + t
    if align == "c":
        l = gap // 2
        return " " * l + t + " " * (gap - l)
    return t + " " * gap

def _bar(widths, l, m, r):
    parts = [l]
    for k in range(len(widths)):
        parts.append("─" * (widths[k] + 2))
        parts.append(r if k + 1 == len(widths) else m)
    return "".join(parts)

def _render_table(header, sep, body):
    aligns = []
    for c in _split_row(sep):
        a = "l"
        if c.startswith(":") and c.endswith(":"):
            a = "c"
        elif c.endswith(":"):
            a = "r"
        aligns.append(a)
    rows = [[_inline(c) for c in _split_row(header)]]
    for b in body[:50]:
        rows.append([_inline(c) for c in _split_row(b)])
    ncol = max([len(r) for r in rows])
    widths = [1] * ncol
    for r in rows:
        for k in range(len(r)):
            widths[k] = max(widths[k], min(disp_width(r[k]), _CELL_CAP))
    out = [_bar(widths, "┌", "┬", "┐")]
    for ri in range(len(rows)):
        r = rows[ri]
        parts = ["│"]
        for k in range(ncol):
            cell = r[k] if k < len(r) else ""
            a = aligns[k] if k < len(aligns) else "l"
            parts.append(" " + _pad(cell, widths[k], a) + " │")
        out.append("".join(parts))
        if ri == 0:
            out.append(_bar(widths, "├", "┼", "┤"))
    out.append(_bar(widths, "└", "┴", "┘"))
    return out

# ── 블록 파서 ────────────────────────────────────────────────────────────

def _list_marker(t):
    # (접두, 내용) 또는 None — 불릿 •·체크 ☐☑·번호 N.
    for m in ["- ", "* ", "+ "]:
        if t.startswith(m):
            rest = t[len(m):]
            for tag, mark in [("[ ] ", "☐ "), ("[x] ", "☑ "), ("[X] ", "☑ ")]:
                if rest.startswith(tag):
                    return [mark, rest[len(tag):]]
            return ["• ", rest]
    d = 0
    for ch in t.elems():
        if ch.isdigit():
            d += 1
        else:
            break
    if d > 0 and d <= 9:
        rest = t[d:]
        for sepc in [". ", ") "]:
            if rest.startswith(sepc):
                return [t[:d] + ". ", rest[len(sepc):]]
    return None

def _is_hr(t):
    s = t.replace(" ", "")
    if len(s) < 3:
        return False
    for ch in ["-", "*", "_"]:
        if s == ch * len(s):
            return True
    return False

def _render(lines):
    out = []
    i = 0
    fence = ""      # ``` 내부면 펜스 문자열
    mermaid = None  # mermaid 수집 버퍼(list) 또는 None
    last_blank = False
    for _ in range(len(lines) + len(lines) + 8):  # while 대체
        if i >= len(lines) or len(out) >= _LINE_CAP:
            break
        line = lines[i].replace("\t", "    ")
        t = line.strip()
        if fence != "":
            if t.startswith(fence) and t.strip(fence[:1]).strip() == "":
                if mermaid != None:
                    out.extend(_mermaid(mermaid))
                    mermaid = None
                else:
                    out.append("└──")
                fence = ""
            elif mermaid != None:
                mermaid.append(line)
            else:
                out.append("│ " + line)
            i += 1
            continue
        if t.startswith("```") or t.startswith("~~~"):
            fc = t[:1]
            r = _run_len(list(t.elems()), 0, fc)
            fence = fc * r
            lang = t[r:].strip()
            if lang.lower() == "mermaid":
                mermaid = []
            else:
                out.append("┌── " + (lang if lang != "" else "code"))
            i += 1
            continue
        if t == "":
            if not last_blank and len(out) > 0:
                out.append("")
            last_blank = True
            i += 1
            continue
        last_blank = False
        # 표(구분행 필수)
        if "|" in t and i + 1 < len(lines) and _is_sep(lines[i + 1]):
            body = []
            j = i + 2
            for _2 in range(len(lines)):
                if j >= len(lines) or "|" not in lines[j] or lines[j].strip() == "":
                    break
                body.append(lines[j])
                j += 1
            out.extend(_render_table(lines[i], lines[i + 1], body))
            i = j
            continue
        # 제목 — h1 ═ 밑줄·h2 ─ 밑줄·h3+ › 접두
        if t.startswith("#"):
            h = _run_len(list(t.elems()), 0, "#")
            if h <= 6 and len(t) > h and t[h] == " ":
                title = _inline(t[h + 1:].strip())
                if h == 1:
                    out.append(title)
                    out.append("═" * max(disp_width(title), 4))
                elif h == 2:
                    out.append(title)
                    out.append("─" * max(disp_width(title), 4))
                else:
                    out.append("› " + title)
                i += 1
                continue
        if _is_hr(t):
            out.append("─" * 56)
            i += 1
            continue
        if t.startswith(">"):
            depth = 0
            rest = t
            for _2 in range(8):
                r2 = rest.strip()
                if r2.startswith(">"):
                    depth += 1
                    rest = r2[1:]
                else:
                    rest = r2
                    break
            out.append("│ " * depth + _inline(rest))
            i += 1
            continue
        lm = _list_marker(t)
        if lm != None:
            indent = len(line) - len(line.lstrip())
            out.append(" " * min(indent, 16) + lm[0] + _inline(lm[1]))
            i += 1
            continue
        out.append(_inline(line))
        i += 1
    if i < len(lines):
        out.append("… (표시 상한 — 이후 생략)")
    return out

# ── Mermaid 텍스트 다이어그램(flowchart TD/LR·sequenceDiagram 서브셋) ────
# 박스 드로잉 문자 격자로 직접 렌더 — 미지원 형식/상한 초과는 원문 상자 폴백.
# 격자 셀 = 표시 열(와이드 문자는 2칸 점유 — 뒤 칸 "" 센티널, join 시 소거).

_MM_NODES = 16   # 노드 상한
_MM_EDGES = 40   # 간선 상한
_MM_W = 200      # 격자 폭 상한
_MM_H = 80       # 격자 높이 상한

def _mm_grid(w, h):
    if w < 1 or h < 1 or w > _MM_W or h > _MM_H:
        return None
    return [[" " for _ in range(w)] for _ in range(h)]

def _mm_put(g, x, y, ch):
    if y >= 0 and y < len(g) and x >= 0 and x < len(g[0]):
        g[y][x] = ch

def _mm_put_str(g, x, y, s):
    cx = x
    for ch in s.elems():
        cw = disp_width(ch)
        if cx + cw > len(g[0]):
            break
        _mm_put(g, cx, y, ch)
        if cw == 2:
            _mm_put(g, cx + 1, y, "")
        cx += cw

def _mm_box(g, x, y, w, label, rnd):
    tl = "╭" if rnd else "┌"
    tr = "╮" if rnd else "┐"
    bl = "╰" if rnd else "└"
    br = "╯" if rnd else "┘"
    for i in range(1, w - 1):
        _mm_put(g, x + i, y, "─")
        _mm_put(g, x + i, y + 2, "─")
    _mm_put(g, x, y, tl)
    _mm_put(g, x + w - 1, y, tr)
    _mm_put(g, x, y + 2, bl)
    _mm_put(g, x + w - 1, y + 2, br)
    _mm_put(g, x, y + 1, "│")
    _mm_put(g, x + w - 1, y + 1, "│")
    _mm_put_str(g, x + 2, y + 1, label)

def _mm_emit(g):
    out = []
    for row in g:
        s = "".join(row)
        # rstrip — 뒤 공백 제거
        e = len(s)
        for _ in range(len(s)):
            if e > 0 and s[e - 1] == " ":
                e -= 1
            else:
                break
        out.append(s[:e])
    for _ in range(len(out)):
        if len(out) > 0 and out[len(out) - 1] == "":
            out.pop()
        else:
            break
    return out

def _mm_source(lines0):
    out = ["┌── mermaid"]
    for l in lines0:
        out.append("│ " + l)
    out.append("└──")
    return out

def _mm_parse_node(tok, reg):
    t = tok.strip()
    idlen = 0
    for ch in t.elems():
        if ch.isalnum() or ch == "_":
            idlen += 1
        else:
            break
    if idlen == 0:
        return -1
    nid = t[:idlen]
    rest = t[idlen:]
    label = None
    rnd = False
    for spec in [
        ["((", "))", True, False],
        ["([", "])", True, False],
        ["[[", "]]", False, False],
        ["{{", "}}", False, True],
        ["[", "]", False, False],
        ["(", ")", True, False],
        ["{", "}", False, True],
    ]:
        if rest.startswith(spec[0]):
            e = rest.find(spec[1], len(spec[0]))
            if e > 0:
                raw = rest[len(spec[0]):e].strip().strip("\"")
                label = ("◇ " + raw) if spec[3] else raw
                rnd = spec[2]
                break
    if nid in reg["idx"]:
        i = reg["idx"][nid]
        if label != None:
            reg["labels"][i] = label
            reg["round"][i] = rnd
        return i
    i = len(reg["ids"])
    reg["ids"].append(nid)
    reg["labels"].append(label if label != None else nid)
    reg["round"].append(rnd)
    reg["idx"][nid] = i
    return i

def _mm_node_span(t):
    # t(앞 공백 제거) 기준 노드 토큰 길이 = id + **직결** 브래킷 라벨(있을 때만).
    idl = 0
    for ch in t.elems():
        if ch.isalnum() or ch == "_":
            idl += 1
        else:
            break
    rest = t[idl:]
    for spec in [["((", "))"], ["([", "])"], ["[[", "]]"], ["{{", "}}"], ["[", "]"], ["(", ")"], ["{", "}"]]:
        if rest.startswith(spec[0]):
            e = rest.find(spec[1], len(spec[0]))
            if e > 0:
                return idl + e + len(spec[1])
            break
    return idl

def _esc(s):
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")

def _mm_colors():
    # 호스트 is_dark() — 테마 연동 팔레트(GitHub 근사)
    if is_dark():
        return {"bg": "#191C21", "node": "#262B33", "border": "#3D8BFF", "fg": "#D6DAE0", "line": "#8A919C"}
    return {"bg": "#FFFFFF", "node": "#F6F8FA", "border": "#0969DA", "fg": "#1F2328", "line": "#57606A"}

def _mm_flow_svg(reg, edges, rowidx, rows, horizontal):
    # flowchart를 **이미지 수준**으로 — SVG 생성 → 호스트 render_svg(GDI+ AA 래스터,
    # 07-26). 실패(비지원 플랫폼·상한 초과) = None → 텍스트 아트 폴백.
    n = len(reg["ids"])
    C = _mm_colors()
    BH = 34
    bw = [disp_width(l) * 7 + 26 for l in reg["labels"]]
    pos = [[0, 0] for _ in range(n)]
    if horizontal:
        gap = 46
        col_w = [max([bw[i] for i in row]) for row in rows]
        col_h = [len(row) * (BH + 14) - 14 for row in rows]
        H = max(col_h) + 20
        x = 10
        for ci in range(len(rows)):
            yy = 10 + (H - 20 - col_h[ci]) // 2
            for node in rows[ci]:
                pos[node] = [x, yy]
                yy += BH + 14
            x += col_w[ci] + gap
        W = x - gap + 10
    else:
        gaph = 28
        gapv = 50
        row_w = []
        for row in rows:
            w2 = 0
            for i in row:
                w2 += bw[i]
            row_w.append(w2 + gaph * (len(row) - 1))
        W = max(row_w) + 20
        for li in range(len(rows)):
            yy = 10 + li * (BH + gapv)
            xx = 10 + (W - 20 - row_w[li]) // 2
            for node in rows[li]:
                pos[node] = [xx, yy]
                xx += bw[node] + gaph
        H = 10 + len(rows) * (BH + gapv) - gapv + 10
    if W > 1600 or H > 1200:
        return None
    p = ['<svg viewBox="0 0 {} {}" stroke-width="1.5">'.format(W, H)]
    p.append('<rect x="0" y="0" width="{}" height="{}" fill="{}"/>'.format(W, H, C["bg"]))
    for e in edges:
        if rowidx[e[1]] != rowidx[e[0]] + 1:
            continue
        if horizontal:
            x1 = pos[e[0]][0] + bw[e[0]]
            y1 = pos[e[0]][1] + BH // 2
            x2 = pos[e[1]][0]
            y2 = pos[e[1]][1] + BH // 2
            mx = (x1 + x2) // 2
            p.append('<polyline points="{},{} {},{} {},{} {},{}" fill="none" stroke="{}"/>'.format(
                x1, y1, mx, y1, mx, y2, x2 - 2, y2, C["line"]))
            p.append('<path d="M {} {} L {} {} L {} {} Z" fill="{}"/>'.format(
                x2, y2, x2 - 8, y2 - 4, x2 - 8, y2 + 4, C["line"]))
            lx = mx
            ly = min(y1, y2) - 6
        else:
            x1 = pos[e[0]][0] + bw[e[0]] // 2
            y1 = pos[e[0]][1] + BH
            x2 = pos[e[1]][0] + bw[e[1]] // 2
            y2 = pos[e[1]][1]
            my = (y1 + y2) // 2
            p.append('<polyline points="{},{} {},{} {},{} {},{}" fill="none" stroke="{}"/>'.format(
                x1, y1, x1, my, x2, my, x2, y2 - 2, C["line"]))
            p.append('<path d="M {} {} L {} {} L {} {} Z" fill="{}"/>'.format(
                x2, y2, x2 - 4, y2 - 8, x2 + 4, y2 - 8, C["line"]))
            lx = (x1 + x2) // 2 + 6
            ly = my - 4
        if e[2] != "":
            p.append('<text x="{}" y="{}" font-size="12" fill="{}">{}</text>'.format(
                lx, ly, C["fg"], _esc(e[2])))
    for i in range(n):
        rx = 16 if reg["round"][i] else 6
        p.append('<rect x="{}" y="{}" width="{}" height="{}" rx="{}" fill="{}"/>'.format(
            pos[i][0], pos[i][1], bw[i], BH, rx, C["node"]))
        p.append('<rect x="{}" y="{}" width="{}" height="{}" rx="{}" fill="none" stroke="{}"/>'.format(
            pos[i][0], pos[i][1], bw[i], BH, rx, C["border"]))
        p.append('<text x="{}" y="{}" font-size="13" text-anchor="middle" fill="{}">{}</text>'.format(
            pos[i][0] + bw[i] // 2, pos[i][1] + BH // 2 + 5, C["fg"], _esc(reg["labels"][i])))
    p.append("</svg>")
    img = render_svg("".join(p))
    if img == "":
        return None
    out = ["\x01img|" + img]
    nrows = min(max(H // 22, 3), 18)
    for _ in range(nrows - 1):
        out.append("\x01pad")
    for e in edges:
        if rowidx[e[1]] != rowidx[e[0]] + 1:
            lbl = "" if e[2] == "" else " ({})".format(e[2])
            out.append("· {} ─▶ {}{}".format(reg["labels"][e[0]], reg["labels"][e[1]], lbl))
    return out

_MM_SKIP = ["subgraph", "end", "style", "classDef", "class", "click", "linkStyle", "direction"]

def _mm_flow(body, horizontal):
    reg = {"ids": [], "labels": [], "round": [], "idx": {}}
    edges = []  # [from, to, label]
    for line in body:
        first = [t for t in line.split(" ") if t != ""]
        if len(first) > 0 and first[0] in _MM_SKIP:
            continue
        rest = line
        prev = -1
        for _ in range(12):  # 체인 상한
            if prev < 0:
                prev = _mm_parse_node(rest, reg)
                if prev < 0:
                    break
                t = rest.strip()
                rest = t[_mm_node_span(t):]
            # 화살표
            r = rest.strip()
            arrow = ""
            for a in ["-.->", "==>", "-->", "---", "-.-", "==="]:
                if r.startswith(a):
                    arrow = a
                    break
            if arrow == "":
                break
            after = r[len(arrow):].strip()
            label = ""
            if after.startswith("|"):
                e = after.find("|", 1)
                if e > 0:
                    label = after[1:e].strip()
                    after = after[e + 1:]
            nxt = _mm_parse_node(after, reg)
            if nxt < 0:
                break
            edges.append([prev, nxt, label])
            t = after.strip()
            rest = t[_mm_node_span(t):]
            prev = nxt
    n = len(reg["ids"])
    if n == 0 or n > _MM_NODES or len(edges) > _MM_EDGES:
        return None
    # 레벨 = 최장 경로(반복 이완 — 사이클은 n 상한 수렴)
    level = [0 for _ in range(n)]
    for _ in range(n):
        changed = False
        for e in edges:
            if e[0] != e[1] and level[e[0]] + 1 > level[e[1]] and level[e[0]] < n:
                level[e[1]] = level[e[0]] + 1
                changed = True
        if not changed:
            break
    used = sorted({l: True for l in level}.keys())
    rowidx = [used.index(l) for l in level]
    rows = [[] for _ in used]
    for i in range(n):
        rows[rowidx[i]].append(i)
    # 1순위 = 이미지 수준 SVG 렌더(호스트 GDI+ — 실패 시 텍스트 아트 폴백)
    svg_art = _mm_flow_svg(reg, edges, rowidx, rows, horizontal)
    if svg_art != None:
        return svg_art
    bw = [disp_width(l) + 4 for l in reg["labels"]]
    pos = [[0, 0] for _ in range(n)]
    if horizontal:
        gap = 5
        col_w = [max([bw[i] for i in row]) for row in rows]
        col_h = [len(row) * 4 - 1 for row in rows]
        gh = max(col_h)
        gw = 0
        for k in range(len(rows)):
            gw += col_w[k]
        gw += gap * (len(rows) - 1)
        g = _mm_grid(gw, gh)
        if g == None:
            return None
        x = 0
        for ci in range(len(rows)):
            y = (gh - col_h[ci]) // 2
            for node in rows[ci]:
                pos[node] = [x, y]
                y += 4
            x += col_w[ci] + gap
        for i in range(n):
            _mm_box(g, pos[i][0], pos[i][1], bw[i], reg["labels"][i], reg["round"][i])
        for e in edges:
            if rowidx[e[1]] != rowidx[e[0]] + 1:
                continue
            sy = pos[e[0]][1] + 1
            ty = pos[e[1]][1] + 1
            sx = pos[e[0]][0] + bw[e[0]]
            tx = pos[e[1]][0]
            if sy == ty:
                for cx in range(sx, tx - 1):
                    _mm_put(g, cx, sy, "─")
                _mm_put(g, tx - 1, sy, "▶")
                if e[2] != "" and tx > sx + disp_width(e[2]) + 3:
                    _mm_put_str(g, sx + 1, sy, " " + e[2] + " ")
            else:
                cc = sx + 1
                _mm_put(g, sx, sy, "─")
                _mm_put(g, cc, sy, "╮" if ty > sy else "╯")
                lo = min(sy, ty)
                hi = max(sy, ty)
                for cy in range(lo + 1, hi):
                    _mm_put(g, cc, cy, "│")
                _mm_put(g, cc, ty, "╰" if ty > sy else "╭")
                for cx in range(cc + 1, tx - 1):
                    _mm_put(g, cx, ty, "─")
                _mm_put(g, tx - 1, ty, "▶")
    else:
        gaph = 4
        gapv = 3
        row_w = []
        for row in rows:
            w = 0
            for i in row:
                w += bw[i]
            w += gaph * (len(row) - 1)
            row_w.append(w)
        gw = max(row_w)
        gh = len(rows) * 3 + gapv * (len(rows) - 1)
        g = _mm_grid(gw, gh)
        if g == None:
            return None
        for li in range(len(rows)):
            y = li * (3 + gapv)
            x = (gw - row_w[li]) // 2
            for node in rows[li]:
                pos[node] = [x, y]
                x += bw[node] + gaph
        for i in range(n):
            _mm_box(g, pos[i][0], pos[i][1], bw[i], reg["labels"][i], reg["round"][i])
        for e in edges:
            if rowidx[e[1]] != rowidx[e[0]] + 1:
                continue
            px = pos[e[0]][0] + bw[e[0]] // 2
            cx = pos[e[1]][0] + bw[e[1]] // 2
            gy = pos[e[0]][1] + 3
            _mm_put(g, px, gy, "│")
            if px == cx:
                _mm_put(g, px, gy + 1, "│")
                if e[2] != "":
                    _mm_put_str(g, px + 2, gy + 1, e[2])
            else:
                lo = min(px, cx)
                hi = max(px, cx)
                for x2 in range(lo + 1, hi):
                    _mm_put(g, x2, gy + 1, "─")
                _mm_put(g, px, gy + 1, "└" if cx > px else "┘")
                _mm_put(g, cx, gy + 1, "┐" if cx > px else "┌")
                if e[2] != "" and hi > lo + 2:
                    _mm_put_str(g, lo + 2, gy + 1, e[2])
            _mm_put(g, cx, gy + 2, "▼")
    out = _mm_emit(g)
    extras = []
    for e in edges:
        if rowidx[e[1]] != rowidx[e[0]] + 1:
            lbl = "" if e[2] == "" else " ({})".format(e[2])
            extras.append("· {} ─▶ {}{}".format(reg["labels"][e[0]], reg["labels"][e[1]], lbl))
    if len(extras) > 0:
        out.append("")
        out.extend(extras)
    return out

_MM_ARROWS = ["-->>", "->>", "--x", "--)", "-->", "-x", "-)", "->"]

def _mm_seq(body):
    names = []
    idx = {}
    rows = []  # ["m", from, to, text, dashed] | ["k", marker]
    for line in body:
        first = [t for t in line.split(" ") if t != ""]
        f0 = first[0] if len(first) > 0 else ""
        if f0 in ["activate", "deactivate", "autonumber"]:
            continue
        if f0 in ["participant", "actor"]:
            rest = line[len(f0):].strip()
            parts = rest.split(" as ")
            pid = parts[0].strip()
            disp = parts[1].strip() if len(parts) > 1 else pid
            if pid not in idx:
                idx[pid] = len(names)
                names.append(disp)
            else:
                names[idx[pid]] = disp
            continue
        low = f0.lower()
        if low == "note":
            p = line.find(":")
            rows.append(["k", "· " + (line[p + 1:].strip() if p > 0 else "")])
            continue
        if low in ["loop", "alt", "opt", "par", "critical", "break", "rect"]:
            rows.append(["k", "┌─ " + line])
            continue
        if low == "else":
            rows.append(["k", "├─ " + line])
            continue
        if low == "end":
            rows.append(["k", "└─"])
            continue
        # 메시지 — 가장 앞 매치(동순위 = 긴 것)
        bp = -1
        ba = ""
        for a in _MM_ARROWS:
            p = line.find(a)
            if p >= 0 and (bp < 0 or p < bp or (p == bp and len(a) > len(ba))):
                bp = p
                ba = a
        if bp < 0:
            continue
        lid = line[:bp].strip()
        rest = line[bp + len(ba):]
        cp = rest.find(":")
        rid = (rest[:cp] if cp >= 0 else rest).strip()
        text = rest[cp + 1:].strip() if cp >= 0 else ""
        for pid in [lid, rid]:
            if pid not in idx:
                idx[pid] = len(names)
                names.append(pid)
        rows.append(["m", idx[lid], idx[rid], text, ba.startswith("--")])
    if len(names) == 0 or len(rows) == 0 or len(names) > 6 or len(rows) > 40:
        return None
    bw = [disp_width(nm) + 4 for nm in names]
    cx = [bw[0] // 2]
    for i in range(1, len(names)):
        d = max((bw[i - 1] + bw[i]) // 2 + 4, 14)
        cx.append(cx[i - 1] + d)
    gw = cx[len(cx) - 1] + (bw[len(bw) - 1] + 1) // 2 + 1
    gh = 3
    for r in rows:
        gh += 2 if r[0] == "m" else 1
    g = _mm_grid(gw, gh)
    if g == None:
        return None
    for i in range(len(names)):
        _mm_box(g, cx[i] - bw[i] // 2, 0, bw[i], names[i], False)
    y = 3
    for r in rows:
        h = 2 if r[0] == "m" else 1
        for dy in range(h):
            for c in cx:
                _mm_put(g, c, y + dy, "│")
        if r[0] == "k":
            _mm_put_str(g, 0, y, r[1])
            y += 1
        else:
            frm = r[1]
            to = r[2]
            text = r[3]
            dashed = r[4]
            if frm == to:
                _mm_put(g, cx[frm] + 1, y + 1, "⟲")
                if text != "":
                    _mm_put_str(g, cx[frm] + 2, y, text)
            else:
                lo = min(cx[frm], cx[to])
                hi = max(cx[frm], cx[to])
                if text != "":
                    start = max((lo + hi) // 2 - disp_width(text) // 2, lo + 1)
                    _mm_put_str(g, start, y, text)
                for x2 in range(lo + 1, hi):
                    _mm_put(g, x2, y + 1, "╌" if dashed else "─")
                if cx[to] > cx[frm]:
                    _mm_put(g, hi - 1, y + 1, "▶")
                else:
                    _mm_put(g, lo + 1, y + 1, "◀")
            y += 2
    return _mm_emit(g)

def _mermaid(src_lines):
    lines = []
    for l in src_lines:
        for part in l.split(";"):
            t = part.strip()
            if t != "" and not t.startswith("%%"):
                lines.append(t)
    if len(lines) == 0:
        return _mm_source(src_lines)
    fields = [t for t in lines[0].split(" ") if t != ""]
    kind = fields[0] if len(fields) > 0 else ""
    art = None
    if kind == "graph" or kind == "flowchart":
        dirn = fields[1].upper() if len(fields) > 1 else "TD"
        art = _mm_flow(lines[1:], dirn == "LR" or dirn == "RL")
    elif kind == "sequenceDiagram":
        art = _mm_seq(lines[1:])
    if art == None:
        return _mm_source(src_lines)  # 미지원 형식/상한 초과 = 원문 상자 폴백
    return art

# ── 진입점 ──────────────────────────────────────────────────────────────

def preview(file):
    src = read_text(_READ_CAP)
    if src == "":
        return {"lines": ["(empty file)"]}
    return {"lines": _render(src.splitlines())}
