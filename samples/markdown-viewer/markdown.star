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

# ── Mermaid(커밋 5에서 flowchart·sequence 텍스트 렌더로 확장) ────────────

def _mermaid(src_lines):
    out = ["┌── mermaid"]
    for l in src_lines:
        out.append("│ " + l)
    out.append("└──")
    return out

# ── 진입점 ──────────────────────────────────────────────────────────────

def preview(file):
    src = read_text(_READ_CAP)
    if src == "":
        return {"lines": ["(empty file)"]}
    return {"lines": _render(src.splitlines())}
