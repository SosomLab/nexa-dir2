//! 압축 미리보기 **그리드 창**(X-46 — 사용자 요청 08-24: "그리드 방식, 별도창 보기").
//!
//! 하단 도크가 요약 텍스트를 보여 주는 것과 같은 자료([`ArchiveDoc`])를 **파일 목록과
//! 같은 규약의 그리드**로 크게 보여 준다 — 컨트롤은 앱의 그리드 라이브러리
//! [`ctl::grid`](NxGrid: 헤더 리사이즈·정렬·선택·오버레이 스크롤바 = 파일 그리드
//! 규약 계승)을 재사용한다.
//!
//! 흐름: 목록 읽기 → (암호가 필요하면 [`crate::pwprompt`]로 입력받아 재시도) →
//! 그리드 표시. 암호는 성공했을 때만 **세션 메모리**에 기억한다
//! ([`crate::preview::archive::pw`] — 디스크 기록 없음).
//!
//! 키: `Esc` 닫기 · `Ctrl+C` 선택 행 복사(탭 구분) · 방향키/`Ctrl+A` = 그리드 규약.

use std::path::Path;

use nexa_vfs::archive::ArchiveEntry;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::HFONT;
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, GetKeyState, SetFocus, VK_CONTROL};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetMessageW,
    GetWindowLongPtrW, GetWindowRect, IsWindow, LoadCursorW, MoveWindow, RegisterClassW,
    SetForegroundWindow, SetWindowLongPtrW, TranslateMessage, GWLP_USERDATA, IDC_ARROW, MSG,
    WM_COMMAND, WM_DESTROY, WM_KEYDOWN, WM_SIZE, WNDCLASSW, WINDOW_EX_STYLE, WS_OVERLAPPEDWINDOW,
    WS_VISIBLE,
};

use crate::ctl::{self, style::Style};
use crate::i18n::{tr, trf};
use crate::preview::archive::{self as arc, ArchiveDoc, ArchiveStatus};
use crate::source::human_size;

const CLASS: PCWSTR = w!("NexaArchiveWnd");
static REGISTER: std::sync::Once = std::sync::Once::new();

const ID_GRID: u32 = 1;
const PAD: i32 = 10;

/// 컬럼(제목 키, 기본 폭) — 순서 = [`row_cells`]의 셀 순서.
const COLS: [(&str, i32); 8] = [
    ("archive.col.name", 240),
    ("archive.col.path", 200),
    ("archive.col.size", 90),
    ("archive.col.packed", 90),
    ("archive.col.ratio", 60),
    ("archive.col.method", 90),
    ("archive.col.modified", 130),
    ("archive.col.flags", 90),
];

struct ArcCtx {
    grid: HWND,
    status: HWND,
    entries: Vec<ArchiveEntry>,
    tz: i32,
    /// 창 글꼴(레이아웃 지표 — WM_SIZE에서 행 높이 계산에 쓴다).
    font: HFONT,
}

/// 항목 1개 → 그리드 셀들(컬럼 순서 = [`COLS`]).
fn row_cells(e: &ArchiveEntry, tz: i32) -> Vec<String> {
    let mut flags: Vec<String> = Vec::new();
    if e.is_dir {
        flags.push(tr("archive.flag.dir"));
    }
    if e.encrypted {
        flags.push(tr("archive.flag.locked"));
    }
    if e.suspicious {
        flags.push(tr("archive.flag.unsafe"));
    }
    vec![
        e.name().to_string(),
        e.parent().to_string(),
        match (e.is_dir, e.size) {
            (true, _) => String::new(),
            (_, Some(s)) => human_size(s),
            _ => "-".into(),
        },
        match (e.is_dir, e.packed) {
            (true, _) => String::new(),
            (_, Some(s)) => human_size(s),
            _ => "-".into(),
        },
        e.ratio().map(|r| format!("{r}%")).unwrap_or_default(),
        e.method.clone(),
        e.modified
            .map(|t| arc::fmt_entry_time(t, e.time_is_local, tz))
            .unwrap_or_default(),
        flags.join(" · "),
    ]
}

/// 정렬 — 컬럼별 자연스러운 기준(크기·시각은 수치, 나머지는 문자열).
/// 폴더 우선은 두지 않는다(압축 목록은 "무엇이 들어 있나"가 관심사 — 정렬은 사용자 몫).
fn sort_entries(entries: &mut [ArchiveEntry], spec: &[(usize, bool)]) {
    if spec.is_empty() {
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        return;
    }
    entries.sort_by(|a, b| {
        for &(col, desc) in spec {
            let ord = match col {
                0 => a.name().cmp(b.name()),
                1 => a.parent().cmp(b.parent()),
                2 => a.size.unwrap_or(0).cmp(&b.size.unwrap_or(0)),
                3 => a.packed.unwrap_or(0).cmp(&b.packed.unwrap_or(0)),
                4 => a.ratio().unwrap_or(0).cmp(&b.ratio().unwrap_or(0)),
                5 => a.method.cmp(&b.method),
                6 => a.modified.unwrap_or(0).cmp(&b.modified.unwrap_or(0)),
                7 => (a.is_dir, a.encrypted, a.suspicious).cmp(&(b.is_dir, b.encrypted, b.suspicious)),
                _ => std::cmp::Ordering::Equal,
            };
            let ord = if desc { ord.reverse() } else { ord };
            if ord != std::cmp::Ordering::Equal {
                return ord;
            }
        }
        a.path.cmp(&b.path) // 동률 = 경로순(안정적 표시)
    });
}

/// 상태 줄 문구 — 포맷·항목 수·총 크기(실패 상태면 사유를 그대로 보여 준다).
fn status_text(doc: &ArchiveDoc) -> String {
    match &doc.status {
        ArchiveStatus::Ok => {
            let l = &doc.listing;
            let (size, packed) = l.totals();
            let mut s = trf(
                "archive.status",
                &[
                    &l.label,
                    &l.entries.len().to_string(),
                    &human_size(size),
                    &human_size(packed),
                ],
            );
            for (flag, key) in [
                (l.has_encrypted, "archive.encrypted"),
                (l.solid, "archive.solid"),
                (l.multivolume, "archive.multivolume"),
                (l.truncated, "archive.truncated"),
            ] {
                if flag {
                    s.push_str(" · ");
                    s.push_str(&tr(key));
                }
            }
            s
        }
        ArchiveStatus::NeedPassword => tr("archive.needPassword"),
        ArchiveStatus::NeedPlugin(fmt, codec) => trf("archive.needPlugin", &[fmt, codec]),
        ArchiveStatus::Failed(why) => trf("archive.failed", &[why]),
    }
}

/// 이미 읽어 둔 목록으로 그리드 창을 연다(필요하면 암호를 받아 재시도).
/// 사용자가 암호 입력을 취소하면 창을 열지 않는다.
/// `doc` = 미리보기 시임이 만든 결과(내장 또는 플러그인 — 재조회 없이 그대로 쓴다).
pub unsafe fn open(
    owner: HWND,
    path: &Path,
    mut doc: ArchiveDoc,
    route: (&str, &str),
    tz: i32,
    font: HFONT,
) {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut retry = false;
    while doc.status == ArchiveStatus::NeedPassword {
        let Some(secret) = crate::pwprompt::ask(owner, &name, retry, font) else {
            return; // 취소 = 아무것도 열지 않는다
        };
        // 재시도는 **같은 공급자 경로**로(플러그인이 읽던 포맷이면 플러그인이 다시 읽는다).
        // 암호는 활성 슬롯으로만 전달되고 호출이 끝나면 지워진다.
        doc = arc::read_via(path, route.0, route.1, Some(secret.clone()));
        match doc.status {
            // 성공한 암호만 세션 동안 기억(디스크 기록 없음)
            ArchiveStatus::Ok => arc::pw::remember(path, secret),
            ArchiveStatus::NeedPassword => {
                arc::pw::forget(path);
                retry = true;
            }
            _ => break,
        }
    }
    show(owner, &name, doc, tz, font);
}

/// 창 생성 + 모달 펌프(previewwnd·ordereditor와 같은 규약).
unsafe fn show(owner: HWND, name: &str, doc: ArchiveDoc, tz: i32, font: HFONT) {
    REGISTER.call_once(|| {
        let wc = WNDCLASSW {
            lpszClassName: CLASS,
            lpfnWndProc: Some(proc),
            hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH(
                (windows::Win32::Graphics::Gdi::COLOR_WINDOW.0 + 1) as isize
                    as *mut core::ffi::c_void,
            ),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);
    });
    // 소유자 중앙·3/4 크기(previewwnd 규약)
    let (mut w, mut h) = (960, 640);
    let (mut x, mut y) = (120, 120);
    let mut orc = RECT::default();
    if GetWindowRect(owner, &mut orc).is_ok() {
        w = ((orc.right - orc.left) * 3 / 4).clamp(560, 1500);
        h = ((orc.bottom - orc.top) * 3 / 4).clamp(360, 1000);
        x = orc.left + ((orc.right - orc.left) - w) / 2;
        y = orc.top + ((orc.bottom - orc.top) - h) / 2;
    }
    let title = windows::core::HSTRING::from(trf("archive.window.title", &[name]));
    let Ok(hwnd) = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        CLASS,
        PCWSTR(title.as_ptr()),
        WS_OVERLAPPEDWINDOW | WS_VISIBLE,
        x,
        y,
        w,
        h,
        Some(owner),
        None,
        None,
        None,
    ) else {
        return;
    };
    let style = Style::default();
    let cols: Vec<(String, i32)> = COLS.iter().map(|(k, w)| (tr(k), *w)).collect();
    let col_refs: Vec<(&str, i32)> = cols.iter().map(|(t, w)| (t.as_str(), *w)).collect();
    let grid = ctl::grid::create(
        hwnd,
        0,
        0,
        10,
        10,
        ID_GRID,
        font,
        &col_refs,
        ctl::grid::GridOpts {
            zebra: true,
            ..Default::default()
        },
        style,
    );
    let status = ctl::label::create(
        hwnd,
        PAD,
        0,
        10,
        0,
        0,
        font,
        &status_text(&doc),
        ctl::label::LabelAlign::Left,
        style,
    );
    let mut entries = doc.listing.entries; // 창이 소유(원본 doc은 여기서 소멸)
    sort_entries(&mut entries, &[]);
    let mut ctx = Box::new(ArcCtx {
        grid,
        status,
        entries,
        tz,
        font,
    });
    refresh_rows(&ctx);
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, &mut *ctx as *mut ArcCtx as isize);
    layout(hwnd, &ctx);
    let _ = EnableWindow(owner, false);
    let _ = SetForegroundWindow(hwnd);
    let _ = SetFocus(Some(grid));
    let mut msg = MSG::default();
    while IsWindow(Some(hwnd)).as_bool() && GetMessageW(&mut msg, None, 0, 0).as_bool() {
        // Esc = 닫기 · Ctrl+C = 선택 행 복사(그리드가 문자 입력을 쓰지 않는다)
        if msg.message == WM_KEYDOWN {
            match msg.wParam.0 as u16 {
                0x1B => {
                    let _ = DestroyWindow(hwnd);
                    continue;
                }
                0x43 if GetKeyState(VK_CONTROL.0 as i32) < 0 => {
                    copy_selection(hwnd, &ctx);
                    continue;
                }
                _ => {}
            }
        }
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    let _ = EnableWindow(owner, true);
    let _ = SetForegroundWindow(owner);
    drop(ctx);
}

/// 현재 항목들을 그리드 행으로 밀어 넣는다.
unsafe fn refresh_rows(ctx: &ArcCtx) {
    let rows: Vec<ctl::grid::GridRow> = ctx
        .entries
        .iter()
        .map(|e| ctl::grid::GridRow {
            check: None,
            cells: row_cells(e, ctx.tz),
        })
        .collect();
    ctl::grid::set_rows(ctx.grid, rows);
}

/// 그리드 = 상단 전체 · 상태 줄 = 하단 1행.
unsafe fn layout(hwnd: HWND, ctx: &ArcCtx) {
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let lh = ctl::style::font_height(hwnd, ctx.font).max(12);
    let status_h = lh + 8;
    let gh = (rc.bottom - rc.top - status_h - PAD).max(10);
    let _ = MoveWindow(ctx.grid, 0, 0, rc.right - rc.left, gh, true);
    let _ = MoveWindow(
        ctx.status,
        PAD,
        gh + 4,
        (rc.right - rc.left - PAD * 2).max(10),
        status_h,
        true,
    );
}

/// 선택 행(없으면 전체)을 탭 구분 텍스트로 클립보드에 복사.
unsafe fn copy_selection(hwnd: HWND, ctx: &ArcCtx) {
    let sel = ctl::grid::selected_rows(ctx.grid);
    let idx: Vec<usize> = if sel.is_empty() {
        (0..ctx.entries.len()).collect()
    } else {
        sel
    };
    let mut text = String::new();
    for i in idx {
        if let Some(e) = ctx.entries.get(i) {
            let cells = row_cells(e, ctx.tz);
            text.push_str(&cells.join("\t"));
            text.push_str("\r\n");
        }
    }
    if !text.is_empty() {
        crate::clipboard::write_text(hwnd, &text);
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let ctx = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ArcCtx;
    match msg {
        WM_SIZE if !ctx.is_null() => {
            layout(hwnd, &*ctx);
            LRESULT(0)
        }
        WM_COMMAND if !ctx.is_null() => {
            let id = (wp.0 & 0xFFFF) as u32;
            let code = ((wp.0 >> 16) & 0xFFFF) as u32;
            if id == ID_GRID && code == ctl::grid::NXGR_SORT {
                let spec = ctl::grid::sort_spec((*ctx).grid);
                sort_entries(&mut (*ctx).entries, &spec);
                refresh_rows(&*ctx);
            }
            LRESULT(0)
        }
        WM_DESTROY => LRESULT(0),
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, size: u64, packed: u64, dir: bool) -> ArchiveEntry {
        ArchiveEntry {
            path: path.into(),
            is_dir: dir,
            size: (!dir).then_some(size),
            packed: (!dir).then_some(packed),
            modified: Some(1_700_000_000),
            method: "Deflate".into(),
            ..Default::default()
        }
    }

    #[test]
    fn cells_follow_column_order_and_blank_dirs() {
        let cells = row_cells(&entry("docs/readme.md", 1000, 250, false), 0);
        assert_eq!(cells.len(), COLS.len());
        assert_eq!(cells[0], "readme.md");
        assert_eq!(cells[1], "docs");
        assert_eq!(cells[4], "75%");
        assert_eq!(cells[5], "Deflate");
        let dir = row_cells(&entry("docs", 0, 0, true), 0);
        assert!(dir[2].is_empty() && dir[3].is_empty(), "폴더 행은 크기 비움");
    }

    #[test]
    fn sort_uses_numeric_order_for_size_and_reverses() {
        let mut v = vec![
            entry("a.bin", 100, 50, false),
            entry("b.bin", 3000, 100, false),
            entry("c.bin", 20, 10, false),
        ];
        sort_entries(&mut v, &[(2, false)]);
        assert_eq!(
            v.iter().map(|e| e.path.as_str()).collect::<Vec<_>>(),
            ["c.bin", "a.bin", "b.bin"],
            "크기는 문자열이 아니라 수치 순"
        );
        sort_entries(&mut v, &[(2, true)]);
        assert_eq!(v[0].path, "b.bin");
    }

    #[test]
    fn empty_sort_spec_falls_back_to_path_order() {
        let mut v = vec![entry("z.txt", 1, 1, false), entry("a/b.txt", 1, 1, false)];
        sort_entries(&mut v, &[]);
        assert_eq!(v[0].path, "a/b.txt");
    }
}
