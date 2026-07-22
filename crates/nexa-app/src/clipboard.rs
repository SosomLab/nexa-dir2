//! OS 클립보드 **파일 목록 상호운용**(M3-5 S1/S2) — CF_HDROP + "Preferred DropEffect".
//! **원본 대응**: `MainWindow.PasteFromOsClipboardAsync`/`OsClipboardIsCutAsync`(읽기측) —
//! 원본은 WinUI DataPackage/StorageItems, dir2는 Win32 원시 포맷으로 재구현(관리 런타임 0).
//!
//! 원본과의 차이(개선): 원본은 내부 클립보드+OS 읽기측 병행이었으나 dir2는 **OS 클립보드를
//! 단일 출처**로 — 탐색기↔앱 양방향(복사/잘라내기/붙여넣기) 완전 상호운용, 이중 상태 제거.
//!
//! 포맷 규약(탐색기 동일): CF_HDROP = DROPFILES 헤더 + wide 경로 목록(이중 NUL 종단) ·
//! "Preferred DropEffect"(등록 포맷) = DWORD(DROPEFFECT_COPY=1 / DROPEFFECT_MOVE=2 — 잘라내기 판정).
//! 전부 user32/kernel32/shell32 — 신규 임포트 DLL 0(B3 무변).

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use windows::core::w;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND, POINT};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::{CF_HDROP, CF_UNICODETEXT};
use windows::Win32::UI::Shell::{DragQueryFileW, DROPFILES, HDROP};

const DROPEFFECT_COPY: u32 = 1;
const DROPEFFECT_MOVE: u32 = 2;

/// "Preferred DropEffect" 등록 포맷 ID(프로세스 수명 동안 불변 — 매 호출 등록해도 동일 값).
fn effect_format() -> u32 {
    unsafe { RegisterClipboardFormatW(w!("Preferred DropEffect")) }
}

/// 클립보드 열림 가드 — drop 시 CloseClipboard(전 경로 누수 방지).
struct Open;

impl Open {
    fn new(hwnd: Option<HWND>) -> Option<Self> {
        unsafe { OpenClipboard(hwnd).ok().map(|_| Self) }
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

/// 클립보드에 파일 목록이 있는가(열지 않고 판정) — 붙여넣기 메뉴 활성 판단용.
pub fn has_files() -> bool {
    unsafe { IsClipboardFormatAvailable(CF_HDROP.0 as u32).is_ok() }
}

thread_local! {
    /// 잘라내기 대기 표시 집합(X-32) — OS 클립보드의 '잘라내기' 파일 목록 미러(UI 스레드 전용).
    /// WM_CLIPBOARDUPDATE에서 [`sync_cut_marks`]로 갱신, 목록 페인트가 행 흐림 판정에 사용.
    static CUT_MARKS: RefCell<HashSet<PathBuf>> = RefCell::new(HashSet::new());
}

/// 잘라내기 표시 집합 갱신(X-32) — 클립보드가 '이동(잘라내기)' 파일 목록이면 그 목록,
/// 그 외(복사·비파일·비움)면 빈 집합. 표시가 실제로 바뀌었으면 `true`(호출자가 목록 재도장).
/// 외부 앱(탐색기)의 잘라내기도 동일하게 흐려진다 — OS 클립보드 단일 출처 규약 계승.
pub unsafe fn sync_cut_marks() -> bool {
    let next: HashSet<PathBuf> = if has_files() {
        match read_file_list() {
            Some((paths, nexa_ops::Op::Move)) => paths.into_iter().collect(),
            _ => HashSet::new(),
        }
    } else {
        HashSet::new()
    };
    CUT_MARKS.with(|m| {
        let mut m = m.borrow_mut();
        if *m == next {
            false
        } else {
            *m = next;
            true
        }
    })
}

/// 잘라내기 대기 표시가 하나라도 있는가 — 페인트 선판정(비면 경로 조회 생략).
pub fn has_cut_marks() -> bool {
    CUT_MARKS.with(|m| !m.borrow().is_empty())
}

/// 경로가 잘라내기 대기 중인가(X-32) — TreeSource 가시 행 페인트 판정.
pub fn is_cut_marked(path: &Path) -> bool {
    CUT_MARKS.with(|m| m.borrow().contains(path))
}

/// DWORD 1개를 담은 HGLOBAL(Preferred DropEffect 페이로드).
unsafe fn alloc_dword(value: u32) -> Option<HGLOBAL> {
    let hmem = GlobalAlloc(GMEM_MOVEABLE, 4).ok()?;
    let p = GlobalLock(hmem) as *mut u32;
    if p.is_null() {
        let _ = GlobalFree(Some(hmem));
        return None;
    }
    *p = value;
    let _ = GlobalUnlock(hmem);
    Some(hmem)
}

/// 파일 목록 → CF_HDROP HGLOBAL(DROPFILES 헤더+wide 이중 NUL) — 클립보드·DnD 발신 공용.
/// 성공 시 소유권은 호출자(SetClipboardData/STGMEDIUM으로 이전 또는 GlobalFree).
pub unsafe fn hglobal_file_list(paths: &[PathBuf]) -> Option<HGLOBAL> {
    use std::os::windows::ffi::OsStrExt;
    if paths.is_empty() {
        return None;
    }
    let mut list: Vec<u16> = Vec::new();
    for p in paths {
        list.extend(p.as_os_str().encode_wide());
        list.push(0);
    }
    list.push(0);
    let header = std::mem::size_of::<DROPFILES>();
    let total = header + list.len() * 2;
    let hmem = GlobalAlloc(GMEM_MOVEABLE, total).ok()?;
    let base = GlobalLock(hmem) as *mut u8;
    if base.is_null() {
        let _ = GlobalFree(Some(hmem));
        return None;
    }
    let df = DROPFILES {
        pFiles: header as u32,
        pt: POINT::default(),
        fNC: false.into(),
        fWide: true.into(), // 유니코드 경로(비ASCII 파일명)
    };
    std::ptr::write_unaligned(base as *mut DROPFILES, df);
    std::ptr::copy_nonoverlapping(list.as_ptr() as *const u8, base.add(header), list.len() * 2);
    let _ = GlobalUnlock(hmem);
    Some(hmem)
}

/// 파일 목록을 OS 클립보드에 게시(Ctrl+C/X — S2 쓰기측). `op`=Move면 잘라내기(탐색기 반투명 표시
/// 는 대상 앱 몫). 성공 시 HGLOBAL 소유권은 시스템으로 이전.
pub unsafe fn write_file_list(hwnd: HWND, paths: &[PathBuf], op: nexa_ops::Op) -> bool {
    let Some(_open) = Open::new(Some(hwnd)) else {
        return false;
    };
    if EmptyClipboard().is_err() {
        return false;
    }
    let Some(hmem) = hglobal_file_list(paths) else {
        return false;
    };
    if SetClipboardData(CF_HDROP.0 as u32, Some(HANDLE(hmem.0))).is_err() {
        let _ = GlobalFree(Some(hmem)); // 실패 시에만 소유권 잔존 — 해제
        return false;
    }
    // 잘라내기/복사 판정 포맷(탐색기 규약) — 실패해도 파일 목록은 유효(복사로 간주됨)
    let effect = if op == nexa_ops::Op::Move {
        DROPEFFECT_MOVE
    } else {
        DROPEFFECT_COPY
    };
    if let Some(hfx) = alloc_dword(effect) {
        if SetClipboardData(effect_format(), Some(HANDLE(hfx.0))).is_err() {
            let _ = GlobalFree(Some(hfx));
        }
    }
    true
}

/// HDROP에서 경로 목록 추출(클립보드·OLE DnD 공용 — 원본 DragQueryFile 루프).
pub unsafe fn paths_from_hdrop(hdrop: HDROP) -> Vec<PathBuf> {
    let count = DragQueryFileW(hdrop, u32::MAX, None);
    let mut paths = Vec::with_capacity(count as usize);
    for i in 0..count {
        let len = DragQueryFileW(hdrop, i, None); // NUL 제외 길이
        if len == 0 {
            continue; // 개별 항목 실패 격리
        }
        let mut buf = vec![0u16; len as usize + 1];
        let copied = DragQueryFileW(hdrop, i, Some(&mut buf));
        if copied == 0 {
            continue;
        }
        paths.push(PathBuf::from(String::from_utf16_lossy(
            &buf[..copied as usize],
        )));
    }
    paths
}

/// OS 클립보드에서 파일 목록을 읽는다(Ctrl+V — S1 읽기측).
/// 반환 `op`: Preferred DropEffect가 MOVE면 이동(잘라내기), 그 외/없음 = 복사(원본 규약 동일).
pub unsafe fn read_file_list() -> Option<(Vec<PathBuf>, nexa_ops::Op)> {
    let _open = Open::new(None)?;
    let h = GetClipboardData(CF_HDROP.0 as u32).ok()?;
    let paths = paths_from_hdrop(HDROP(h.0));
    if paths.is_empty() {
        return None;
    }
    // 잘라내기 판정 — 실패/없으면 복사로 간주(원본 OsClipboardIsCutAsync 동일)
    let mut op = nexa_ops::Op::Copy;
    if let Ok(hfx) = GetClipboardData(effect_format()) {
        let p = GlobalLock(HGLOBAL(hfx.0)) as *const u32;
        if !p.is_null() {
            if std::ptr::read_unaligned(p) & DROPEFFECT_MOVE != 0 {
                op = nexa_ops::Op::Move;
            }
            let _ = GlobalUnlock(HGLOBAL(hfx.0));
        }
    }
    Some((paths, op))
}

/// 텍스트를 OS 클립보드에 게시(CF_UNICODETEXT) — 편집 필드 Ctrl+C/X(QA 07-14).
pub unsafe fn write_text(hwnd: HWND, text: &str) -> bool {
    let Some(_open) = Open::new(Some(hwnd)) else {
        return false;
    };
    if EmptyClipboard().is_err() {
        return false;
    }
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = wide.len() * 2;
    let Ok(hmem) = GlobalAlloc(GMEM_MOVEABLE, bytes) else {
        return false;
    };
    let p = GlobalLock(hmem) as *mut u8;
    if p.is_null() {
        let _ = GlobalFree(Some(hmem));
        return false;
    }
    std::ptr::copy_nonoverlapping(wide.as_ptr() as *const u8, p, bytes);
    let _ = GlobalUnlock(hmem);
    if SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(hmem.0))).is_err() {
        let _ = GlobalFree(Some(hmem));
        return false;
    }
    true
}

/// OS 클립보드 텍스트 읽기(CF_UNICODETEXT — 시스템이 CF_TEXT를 자동 변환 제공) —
/// 편집 필드·터미널 Ctrl+V(QA 07-14).
pub unsafe fn read_text() -> Option<String> {
    let _open = Open::new(None)?;
    let h = GetClipboardData(CF_UNICODETEXT.0 as u32).ok()?;
    let hg = HGLOBAL(h.0);
    let p = GlobalLock(hg) as *const u16;
    if p.is_null() {
        return None;
    }
    let mut len = 0usize;
    while std::ptr::read_unaligned(p.add(len)) != 0 {
        len += 1;
    }
    let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, len));
    let _ = GlobalUnlock(hg);
    Some(s)
}

/// 클립보드 비우기 — 잘라내기 1회성(이동 붙여넣기 후, 탐색기 관례).
pub unsafe fn clear(hwnd: HWND) {
    if let Some(_open) = Open::new(Some(hwnd)) {
        let _ = EmptyClipboard();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 실 OS 클립보드 왕복(쓰기→판정→읽기·잘라내기 판정) — 사용자 클립보드를 덮으므로 수동 실행:
    /// `cargo test -p nexa-app clipboard -- --ignored`
    #[test]
    #[ignore]
    fn write_then_read_round_trip_with_cut_effect() {
        let paths = vec![
            PathBuf::from("C:\\Windows\\notepad.exe"),
            PathBuf::from("C:\\Windows\\한글 경로.txt"),
        ];
        unsafe {
            assert!(write_file_list(HWND::default(), &paths, nexa_ops::Op::Move));
            assert!(has_files());
            let (read, op) = read_file_list().expect("CF_HDROP 읽기");
            assert_eq!(read, paths, "경로 왕복(비ASCII 포함)");
            assert_eq!(op, nexa_ops::Op::Move, "Preferred DropEffect = 잘라내기");

            assert!(write_file_list(
                HWND::default(),
                &paths[..1],
                nexa_ops::Op::Copy
            ));
            let (_, op) = read_file_list().unwrap();
            assert_eq!(op, nexa_ops::Op::Copy);

            clear(HWND::default());
            assert!(!has_files(), "비운 뒤 파일 없음");
        }
    }
}
