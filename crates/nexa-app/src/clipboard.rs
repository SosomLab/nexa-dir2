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
//!
//! **가상 파일 읽기측**(X-42 — RDP rdpclip·Outlook 첨부·압축 폴더): 원격/컨테이너 소스는
//! 대상 디스크에 실경로가 없어 CF_HDROP 대신 "FileGroupDescriptorW"(항목 목록) +
//! "FileContents"(항목별 IStream 지연 스트리밍)를 게시한다(탐색기가 지원하는 MS 규약
//! CFSTR_FILEDESCRIPTORW/CFSTR_FILECONTENTS). lindex 지정이 필요해 원시 GetClipboardData가
//! 아니라 OLE `OleGetClipboard`/`IDataObject` 경로 — ole32는 M3-5 DnD부터 사용 중(B3 무변).

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use windows::core::w;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND, POINT};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::core::Interface;
use windows::Win32::System::Com::StructuredStorage::CoGetInterfaceAndReleaseStream;
use windows::Win32::System::Com::{
    CoInitializeEx, CoUninitialize, IDataObject, IStream, COINIT_MULTITHREADED, DVASPECT_CONTENT,
    FORMATETC, TYMED_HGLOBAL, TYMED_ISTREAM,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::{OleGetClipboard, ReleaseStgMedium, CF_HDROP, CF_UNICODETEXT};
use windows::Win32::UI::Shell::{DragQueryFileW, DROPFILES, FILEDESCRIPTORW, HDROP};

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
/// 실경로(CF_HDROP)와 가상 파일(FileGroupDescriptorW — X-42) 모두 참.
pub fn has_files() -> bool {
    unsafe {
        IsClipboardFormatAvailable(CF_HDROP.0 as u32).is_ok() || has_virtual_files()
    }
}

/// 클립보드에 **가상 파일**(FileGroupDescriptorW)이 있는가(X-42) — CF_HDROP 부재 시의
/// 붙여넣기 폴백 판정(RDP·Outlook·압축 폴더). 실경로가 있으면 전송 엔진 경로가 우선.
pub fn has_virtual_files() -> bool {
    unsafe { IsClipboardFormatAvailable(descriptor_format()).is_ok() }
}

/// "FileGroupDescriptorW"(CFSTR_FILEDESCRIPTORW) 등록 포맷 ID.
fn descriptor_format() -> u32 {
    unsafe { RegisterClipboardFormatW(w!("FileGroupDescriptorW")) }
}

/// "FileContents"(CFSTR_FILECONTENTS) 등록 포맷 ID.
fn contents_format() -> u32 {
    unsafe { RegisterClipboardFormatW(w!("FileContents")) }
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

// ---- 가상 파일 붙여넣기(X-42) ----------------------------------------------------------

/// FILEDESCRIPTORW.dwFlags — 어느 필드가 유효한가(shlobj_core.h FD_*).
const FD_ATTRIBUTES: u32 = 0x0000_0004;
const FD_FILESIZE: u32 = 0x0000_0040;
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;

/// 가상 파일 항목 1개 — 디스크립터의 상대 경로(하위 폴더 포함 가능)·종류·크기 힌트.
#[derive(Debug, PartialEq)]
struct VirtualItem {
    rel: PathBuf,
    is_dir: bool,
    /// FD_FILESIZE 유효 시의 크기 — 미상(None)이면 EOF까지 읽는다.
    size: Option<u64>,
}

/// 디스크립터 이름 → 안전한 상대 경로. 구분자는 `\`(규약)·`/` 모두 수용, 말미 구분자는 무시.
/// 소스가 외부 앱이라 신뢰 불가 — 절대 경로·드라이브(`:`)·`.`/`..` 조각은 항목째 기각(탈출 방지).
fn sanitize_rel(name: &str) -> Option<PathBuf> {
    if name.is_empty() || name.contains(':') {
        return None;
    }
    let parts: Vec<&str> = name.split(['\\', '/']).collect();
    let mut rel = PathBuf::new();
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() && i + 1 == parts.len() {
            break; // 말미 구분자("폴더\") 허용
        }
        if part.is_empty() || *part == "." || *part == ".." {
            return None;
        }
        rel.push(part);
    }
    (!rel.as_os_str().is_empty()).then_some(rel)
}

/// FILEGROUPDESCRIPTORW 바이트(cItems + FILEDESCRIPTORW 배열) → 항목 목록.
/// cItems가 실제 크기보다 커도 담긴 만큼만(손상 방어) · 부적합 이름은 항목별 격리.
fn parse_group_descriptor(bytes: &[u8]) -> Vec<VirtualItem> {
    const FD: usize = std::mem::size_of::<FILEDESCRIPTORW>();
    if bytes.len() < 4 {
        return Vec::new();
    }
    let count = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    let avail = (bytes.len() - 4) / FD;
    let mut out = Vec::new();
    for i in 0..count.min(avail) {
        // 정렬 무보장 버퍼 — read_unaligned로 항목째 복사
        let fd: FILEDESCRIPTORW = unsafe {
            std::ptr::read_unaligned(bytes.as_ptr().add(4 + i * FD) as *const FILEDESCRIPTORW)
        };
        let name_buf = fd.cFileName; // packed 구조체 — 필드 참조 불가, 값 복사로 정렬 확보
        let len = name_buf
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(name_buf.len());
        let Some(rel) = sanitize_rel(&String::from_utf16_lossy(&name_buf[..len])) else {
            continue;
        };
        let is_dir = fd.dwFlags & FD_ATTRIBUTES != 0
            && fd.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0;
        let size = (fd.dwFlags & FD_FILESIZE != 0 && !is_dir)
            .then(|| (u64::from(fd.nFileSizeHigh) << 32) | u64::from(fd.nFileSizeLow));
        out.push(VirtualItem { rel, is_dir, size });
    }
    out
}

/// 데이터 객체가 가상 파일(FileGroupDescriptorW)을 광고하는가 — DnD 수신 판정
/// (X-42 β-ⓐ: Outlook 첨부·탐색기 zip 내부·MTP 드래그는 CF_HDROP 없이 이것만 온다).
pub unsafe fn data_has_virtual_files(data: &IDataObject) -> bool {
    let fmt = FORMATETC {
        cfFormat: descriptor_format() as u16,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    };
    data.QueryGetData(&fmt).is_ok()
}

/// OS 클립보드의 가상 파일을 `dest_dir`에 추출(Ctrl+V 폴백 — X-42).
/// 반환: 생성된 **최상위** 경로 목록(빈 Vec = 가상 파일 없음/전량 실패).
///
/// # Safety
/// UI 스레드 전용(OleGetClipboard = STA 구속). 동기 추출(1차 슬라이스) — RDP 등 원격
/// 소스는 여기서 실제 전송이 일어나므로 대용량은 그동안 창이 멈춘다(워커화는 후속).
pub unsafe fn extract_virtual_files(dest_dir: &Path) -> Vec<PathBuf> {
    let Ok(data) = OleGetClipboard() else {
        return Vec::new();
    };
    extract_virtual_from(&data, dest_dir)
}

/// 데이터 객체에서 FILEGROUPDESCRIPTORW를 읽어 항목 목록으로(동기 추출·계획 공용).
unsafe fn read_descriptor(data: &IDataObject) -> Vec<VirtualItem> {
    let fmt = FORMATETC {
        cfFormat: descriptor_format() as u16,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    };
    let Ok(mut medium) = data.GetData(&fmt) else {
        return Vec::new();
    };
    let items = {
        let h = medium.u.hGlobal;
        let p = GlobalLock(h) as *const u8;
        if p.is_null() {
            Vec::new()
        } else {
            let v = parse_group_descriptor(std::slice::from_raw_parts(p, GlobalSize(h)));
            let _ = GlobalUnlock(h);
            v
        }
    };
    ReleaseStgMedium(&mut medium);
    items
}

/// 항목별 대상 경로 계획 — 최상위 이름 충돌은 " (2)"(nexa_ops::unique_dest — 전송 엔진
/// 규약 동일)·하위 항목은 같은 매핑 추종(자식이 부모보다 먼저 와도 성립).
/// 반환: (항목별 대상, 생성될 최상위 목록).
fn plan_dests(items: &[VirtualItem], dest_dir: &Path) -> (Vec<Option<PathBuf>>, Vec<PathBuf>) {
    let mut roots: Vec<(std::ffi::OsString, PathBuf)> = Vec::new();
    let mut dests = Vec::with_capacity(items.len());
    for item in items {
        let mut comps = item.rel.iter();
        let Some(first) = comps.next() else {
            dests.push(None);
            continue;
        };
        let rest: PathBuf = comps.collect();
        let nested = !rest.as_os_str().is_empty();
        let root = match roots.iter().find(|(k, _)| k == first) {
            Some((_, p)) => p.clone(),
            None => {
                let uniq = nexa_ops::unique_dest(
                    dest_dir,
                    &first.to_string_lossy(),
                    item.is_dir || nested,
                );
                roots.push((first.to_os_string(), uniq.clone()));
                uniq
            }
        };
        dests.push(Some(if nested { root.join(&rest) } else { root }));
    }
    (dests, roots.into_iter().map(|(_, p)| p).collect())
}

/// 데이터 객체의 가상 파일을 `dest_dir`에 **동기** 추출 — DnD Drop(반환 전 확보 규약)·
/// 진행 슬롯이 바쁠 때의 폴백·클라우드 스테이징(2차-ⓓ) 공용 본체.
/// 항목별 실패 격리(부분 성공 허용).
pub unsafe fn extract_virtual_from(data: &IDataObject, dest_dir: &Path) -> Vec<PathBuf> {
    let items = read_descriptor(data);
    let (dests, mut created) = plan_dests(&items, dest_dir);
    for (i, item) in items.iter().enumerate() {
        let Some(dest) = &dests[i] else { continue };
        if item.is_dir {
            let _ = std::fs::create_dir_all(dest);
            continue;
        }
        if let Some(parent) = dest.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if !save_contents(data, i, dest, item.size) {
            let _ = std::fs::remove_file(dest); // 실패 항목의 파편 소거 후 다음 항목
        }
    }
    created.retain(|p| p.exists()); // 전량 실패한 루트는 결과에서 제외
    created
}

/// FileContents(lindex = 디스크립터 인덱스)를 `dest` 파일로 기록.
/// rdpclip·Outlook은 IStream, 일부 소스는 HGLOBAL — 둘 다 수용.
unsafe fn save_contents(data: &IDataObject, index: usize, dest: &Path, size: Option<u64>) -> bool {
    let fmt = FORMATETC {
        cfFormat: contents_format() as u16,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: index as i32,
        tymed: (TYMED_ISTREAM.0 | TYMED_HGLOBAL.0) as u32,
    };
    let Ok(mut medium) = data.GetData(&fmt) else {
        return false;
    };
    let ok = if medium.tymed == TYMED_ISTREAM.0 as u32 {
        match (*medium.u.pstm).as_ref() {
            Some(stream) => write_stream(stream, dest, size),
            None => false,
        }
    } else if medium.tymed == TYMED_HGLOBAL.0 as u32 {
        let h = medium.u.hGlobal;
        let p = GlobalLock(h) as *const u8;
        if p.is_null() {
            false
        } else {
            // HGLOBAL 관례상 여분 패딩이 붙을 수 있어 크기 힌트가 있으면 그만큼만
            let cap = GlobalSize(h);
            let len = size.map_or(cap, |s| (s as usize).min(cap));
            let ok = std::fs::write(dest, std::slice::from_raw_parts(p, len)).is_ok();
            let _ = GlobalUnlock(h);
            ok
        }
    } else {
        false
    };
    ReleaseStgMedium(&mut medium);
    ok
}

// ---- 워커화(X-42 2차-ⓑ) — 계획(UI 스레드) + 실행(워커) 분리 ------------------------
//
// OleGetClipboard의 IDataObject는 STA 구속이라 워커에서 직접 못 쓴다. 대신 UI 스레드가
// **디스크립터 파싱·대상 계획·항목별 GetData(스트림 확보)까지** 마치고, IStream만
// `CoMarshalInterThreadInterfaceInStream`으로 마샬링해 넘긴다 — out-of-proc 소스
// (rdpclip 등)는 워커의 언마샬이 **원 프로세스로 직접 이어지는 프록시**가 되어
// Read(실제 바이트 전송)가 UI 스레드 펌프에 의존하지 않는다. HGLOBAL 소스는 이미
// 메모리에 있어 UI 스레드에서 즉시 복사한다. 진행/취소는 전송 진행 창 공용 상태
// ([`crate::win::TransferShared`])로 — 클라우드 전송의 "같은 진행 창 재사용" 전례.

/// 워커 소스 1개 — 계획 시점에 확보 완료(Send 가능).
pub enum VSource {
    /// 마샬 패킷(IStream)의 raw 포인터 — 워커가 `CoGetInterfaceAndReleaseStream`으로
    /// 원 스트림 프록시를 되살린다(취소여도 정확히 1회 소비 — 패킷 누수 방지).
    Marshaled(isize),
    /// TYMED_HGLOBAL 소스 — 계획 시점 복사본.
    Bytes(Vec<u8>),
}

/// 워커 잡 항목(계획 순서 = 부모 우선 보장).
pub enum VOp {
    Dir(PathBuf),
    File {
        dest: PathBuf,
        src: VSource,
        /// FD_FILESIZE 힌트 — 진행 총량·세그먼트 크기(미상 = 0 세그먼트).
        size: Option<u64>,
    },
}

/// 붙여넣기 계획 — UI 스레드가 만들고 워커가 소비.
pub struct VPlan {
    pub ops: Vec<VOp>,
    /// 생성될 최상위 경로(완료 후 undo 기록·재로드 참조).
    pub roots: Vec<PathBuf>,
    /// 크기 힌트 합(미상 항목 제외) — 진행 총량.
    pub total: u64,
}

/// 클립보드의 가상 파일 붙여넣기 계획(UI 스레드 전용 — STA).
pub unsafe fn plan_clipboard_paste(dest_dir: &Path) -> Option<VPlan> {
    let data = OleGetClipboard().ok()?;
    plan_virtual_paste(&data, dest_dir)
}

/// 데이터 객체 기반 계획 수립(UI 스레드 전용) — 소스 확보(GetData)는 항목당 1회로
/// 빠르고(스트림 핸들만 받는다), 실제 바이트 전송은 워커의 Read에서 일어난다.
/// 확보 실패 항목만 격리.
pub unsafe fn plan_virtual_paste(data: &IDataObject, dest_dir: &Path) -> Option<VPlan> {
    let items = read_descriptor(data);
    if items.is_empty() {
        return None;
    }
    let (dests, roots) = plan_dests(&items, dest_dir);
    let mut ops = Vec::new();
    let mut total = 0u64;
    for (i, item) in items.iter().enumerate() {
        let Some(dest) = &dests[i] else { continue };
        if item.is_dir {
            ops.push(VOp::Dir(dest.clone()));
            continue;
        }
        let Some(src) = acquire_contents(data, i, item.size) else {
            continue; // 확보 실패 항목 격리
        };
        total = total.saturating_add(item.size.unwrap_or(0));
        ops.push(VOp::File {
            dest: dest.clone(),
            src,
            size: item.size,
        });
    }
    (!ops.is_empty()).then_some(VPlan { ops, roots, total })
}

/// FileContents 소스 확보(UI 스레드) — IStream = 마샬 패킷화, HGLOBAL = 즉시 복사.
unsafe fn acquire_contents(data: &IDataObject, index: usize, size: Option<u64>) -> Option<VSource> {
    let fmt = FORMATETC {
        cfFormat: contents_format() as u16,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: index as i32,
        tymed: (TYMED_ISTREAM.0 | TYMED_HGLOBAL.0) as u32,
    };
    let Ok(mut medium) = data.GetData(&fmt) else {
        return None;
    };
    let out = if medium.tymed == TYMED_ISTREAM.0 as u32 {
        (*medium.u.pstm).as_ref().and_then(|stream| {
            windows::Win32::System::Com::Marshal::CoMarshalInterThreadInterfaceInStream(
                &IStream::IID,
                stream,
            )
            .ok()
            .map(|pkt| VSource::Marshaled(pkt.into_raw() as isize))
        })
    } else if medium.tymed == TYMED_HGLOBAL.0 as u32 {
        let h = medium.u.hGlobal;
        let p = GlobalLock(h) as *const u8;
        if p.is_null() {
            None
        } else {
            let cap = GlobalSize(h);
            let len = size.map_or(cap, |s| (s as usize).min(cap));
            let v = std::slice::from_raw_parts(p, len).to_vec();
            let _ = GlobalUnlock(h);
            Some(VSource::Bytes(v))
        }
    } else {
        None
    };
    ReleaseStgMedium(&mut medium);
    out
}

/// 계획 실행(워커 스레드 — MTA): 스트림을 읽어 파일로. 진행/취소는 `shared`, 완료는
/// `PostMessageW(hwnd, done_msg, 전건 성공 여부, 0)`. 취소 시 쓰다 만 파일은 소거하고
/// 잔여 세그먼트는 Skipped. 반환한 JoinHandle은 호출자가 무시해도 된다(테스트는 join).
pub fn run_virtual_paste(
    plan: VPlan,
    shared: std::sync::Arc<crate::win::TransferShared>,
    hwnd: isize,
    done_msg: u32,
) -> std::thread::JoinHandle<()> {
    use std::sync::atomic::Ordering;
    std::thread::spawn(move || unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        shared.total_bytes.store(plan.total, Ordering::Relaxed);
        {
            let segs: Vec<crate::dialog::SegItem> = plan
                .ops
                .iter()
                .filter_map(|o| match o {
                    VOp::File { size, .. } => Some(crate::dialog::SegItem {
                        size: size.unwrap_or(0),
                        done: 0,
                        status: crate::dialog::SegStatus::Pending,
                    }),
                    VOp::Dir(_) => None,
                })
                .collect();
            *crate::win::plock(&shared.items) = segs;
        }
        let mut all_ok = true;
        let mut seg = 0usize;
        for op in plan.ops {
            let cancelled = shared.cancel.load(Ordering::Relaxed);
            match op {
                VOp::Dir(d) => {
                    if !cancelled {
                        let _ = std::fs::create_dir_all(&d);
                    }
                }
                VOp::File { dest, src, size } => {
                    let ok = run_file_op(&shared, &dest, src, size, seg, cancelled);
                    all_ok &= ok;
                    seg += 1;
                }
            }
        }
        CoUninitialize();
        let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
            Some(windows::Win32::Foundation::HWND(hwnd as *mut _)),
            done_msg,
            windows::Win32::Foundation::WPARAM(usize::from(all_ok)),
            windows::Win32::Foundation::LPARAM(0),
        );
    })
}

/// 파일 1건 처리(워커) — 취소여도 마샬 패킷은 소비한다(누수 방지). 성공이면 true.
unsafe fn run_file_op(
    shared: &crate::win::TransferShared,
    dest: &Path,
    src: VSource,
    size: Option<u64>,
    seg: usize,
    cancelled: bool,
) -> bool {
    use crate::dialog::SegStatus;
    use std::sync::atomic::Ordering;
    let stream = match src {
        VSource::Marshaled(ptr) => {
            let pkt = IStream::from_raw(ptr as *mut _);
            CoGetInterfaceAndReleaseStream::<_, IStream>(&pkt).ok()
        }
        VSource::Bytes(b) => {
            if cancelled {
                mark_seg(shared, seg, SegStatus::Skipped, 0);
                return false;
            }
            mark_seg(shared, seg, SegStatus::Active, 0);
            if let Some(parent) = dest.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let n = b.len() as u64;
            let ok = std::fs::write(dest, &b).is_ok();
            shared.done_bytes.fetch_add(n, Ordering::Relaxed);
            mark_seg(
                shared,
                seg,
                if ok { SegStatus::Done } else { SegStatus::Failed },
                n,
            );
            return ok;
        }
    };
    if cancelled {
        mark_seg(shared, seg, SegStatus::Skipped, 0);
        return false;
    }
    mark_seg(shared, seg, SegStatus::Active, 0);
    if let Some(parent) = dest.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    *crate::win::plock(&shared.in_flight) = Some(dest.to_path_buf());
    let ok = match &stream {
        Some(s) => write_stream_job(s, dest, size, shared, seg),
        None => false,
    };
    *crate::win::plock(&shared.in_flight) = None;
    if ok {
        mark_seg(shared, seg, SegStatus::Done, u64::MAX);
    } else {
        let _ = std::fs::remove_file(dest); // 실패/취소 파편 소거
        let was_cancel = shared.cancel.load(Ordering::Relaxed);
        mark_seg(
            shared,
            seg,
            if was_cancel {
                SegStatus::Skipped
            } else {
                SegStatus::Failed
            },
            0,
        );
    }
    ok
}

/// 세그먼트 상태 갱신(done = u64::MAX면 size 그대로 완료 표기).
fn mark_seg(shared: &crate::win::TransferShared, idx: usize, status: crate::dialog::SegStatus, done: u64) {
    let mut items = crate::win::plock(&shared.items);
    if let Some(it) = items.get_mut(idx) {
        it.status = status;
        it.done = if done == u64::MAX { it.size } else { done };
    }
}

/// 스트림 → 파일(워커 — 256KiB 버퍼·청크마다 취소 확인·진행 가산). 크기 미상은 EOF까지.
fn write_stream_job(
    stream: &IStream,
    dest: &Path,
    size: Option<u64>,
    shared: &crate::win::TransferShared,
    seg: usize,
) -> bool {
    use std::io::Write;
    use std::sync::atomic::Ordering;
    let Ok(mut file) = std::fs::File::create(dest) else {
        return false;
    };
    let mut buf = vec![0u8; 256 * 1024];
    let mut total: u64 = 0;
    loop {
        if shared.cancel.load(Ordering::Relaxed) {
            return false;
        }
        let mut read = 0u32;
        let hr =
            unsafe { stream.Read(buf.as_mut_ptr().cast(), buf.len() as u32, Some(&mut read)) };
        if read > 0 {
            if file.write_all(&buf[..read as usize]).is_err() {
                return false;
            }
            total += u64::from(read);
            shared.done_bytes.fetch_add(u64::from(read), Ordering::Relaxed);
            {
                let mut items = crate::win::plock(&shared.items);
                if let Some(it) = items.get_mut(seg) {
                    it.done = total;
                }
            }
        }
        if hr.is_err() {
            return false;
        }
        if read == 0 {
            break; // S_FALSE 포함 = EOF
        }
        if size.is_some_and(|s| total >= s) {
            break; // 크기 힌트 도달 — EOF를 안 주는 소스 방어
        }
    }
    true
}

/// IStream → 파일(64KiB 버퍼) — 크기 미상은 EOF(read 0)까지. RDP는 여기서 실제 전송.
unsafe fn write_stream(stream: &IStream, dest: &Path, size: Option<u64>) -> bool {
    use std::io::Write;
    let Ok(mut file) = std::fs::File::create(dest) else {
        return false;
    };
    let mut buf = vec![0u8; 64 * 1024];
    let mut total: u64 = 0;
    loop {
        let mut read = 0u32;
        let hr = stream.Read(buf.as_mut_ptr().cast(), buf.len() as u32, Some(&mut read));
        if read > 0 {
            if file.write_all(&buf[..read as usize]).is_err() {
                return false;
            }
            total += u64::from(read);
        }
        if hr.is_err() {
            return false;
        }
        if read == 0 {
            break; // S_FALSE 포함 = EOF
        }
        if size.is_some_and(|s| total >= s) {
            break; // 크기 힌트 도달 — EOF를 안 주는 소스 방어
        }
    }
    true
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

/// 미리보기 복사 전용 **rich 동시 게시**(07-26 — 사용자 요청): CF_UNICODETEXT +
/// "Rich Text Format". RTF는 **모노스페이스(Consolas) 지정**이라 박스 드로잉·표를
/// Word/Outlook 등에 붙여도 정렬이 유지된다. 평문 대상 앱은 CF_UNICODETEXT를 취한다.
pub unsafe fn write_text_rich(hwnd: HWND, text: &str) -> bool {
    let Some(_open) = Open::new(Some(hwnd)) else {
        return false;
    };
    if EmptyClipboard().is_err() {
        return false;
    }
    // ① 평문(CF_UNICODETEXT)
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
    // ② RTF(등록 포맷 — 실패해도 평문은 이미 게시됨)
    let rtf = to_rtf_mono(text);
    let fmt = RegisterClipboardFormatW(w!("Rich Text Format"));
    if fmt != 0 {
        let bytes = rtf.as_bytes();
        if let Ok(hr) = GlobalAlloc(GMEM_MOVEABLE, bytes.len() + 1) {
            let p = GlobalLock(hr) as *mut u8;
            if !p.is_null() {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
                *p.add(bytes.len()) = 0; // RTF는 NUL 종료 관례
                let _ = GlobalUnlock(hr);
                if SetClipboardData(fmt, Some(HANDLE(hr.0))).is_err() {
                    let _ = GlobalFree(Some(hr));
                }
            } else {
                let _ = GlobalFree(Some(hr));
            }
        }
    }
    true
}

/// 등록 포맷 하나를 바이트로 게시(NUL 종료 부가) — 실패는 무시(평문은 이미 게시됨).
unsafe fn put_registered(name: windows::core::PCWSTR, bytes: &[u8]) {
    let fmt = RegisterClipboardFormatW(name);
    if fmt == 0 {
        return;
    }
    let Ok(hr) = GlobalAlloc(GMEM_MOVEABLE, bytes.len() + 1) else {
        return;
    };
    let p = GlobalLock(hr) as *mut u8;
    if p.is_null() {
        let _ = GlobalFree(Some(hr));
        return;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
    *p.add(bytes.len()) = 0;
    let _ = GlobalUnlock(hr);
    if SetClipboardData(fmt, Some(HANDLE(hr.0))).is_err() {
        let _ = GlobalFree(Some(hr));
    }
}

/// 터미널 선택 복사(09-04 — 사용자 요청 "WT처럼 HTML 및 RTF 모두"): CF_UNICODETEXT +
/// "HTML Format"(CF_HTML — `cf_html`로 이미 래핑된 문자열) + "Rich Text Format". 색·굵게·
/// 글꼴이 Word/Outlook/브라우저 편집기에 그대로 붙고, 평문 대상 앱은 CF_UNICODETEXT를 취한다.
pub unsafe fn write_text_html_rtf(
    hwnd: HWND,
    text: &str,
    cf_html: Option<&str>,
    rtf: Option<&str>,
) -> bool {
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
    if let Some(h) = cf_html {
        put_registered(w!("HTML Format"), h.as_bytes());
    }
    if let Some(r) = rtf {
        put_registered(w!("Rich Text Format"), r.as_bytes());
    }
    true
}

/// 평문 → 모노스페이스 RTF(7비트 ASCII 본문 + 비ASCII = `\uN?` 유니코드 이스케이프).
fn to_rtf_mono(text: &str) -> String {
    let mut body = String::with_capacity(text.len() * 2);
    for line in text.split("\r\n") {
        for c in line.chars() {
            match c {
                '\\' => body.push_str("\\\\"),
                '{' => body.push_str("\\{"),
                '}' => body.push_str("\\}"),
                c if (c as u32) < 0x80 => body.push(c),
                c => {
                    // RTF \uN = **부호 있는 16비트**. BMP 밖은 서로게이트 쌍으로.
                    let mut buf = [0u16; 2];
                    for u in c.encode_utf16(&mut buf) {
                        body.push_str(&format!("\\u{}?", *u as i16));
                    }
                }
            }
        }
        body.push_str("\\par ");
    }
    format!(
        "{{\\rtf1\\ansi\\deff0{{\\fonttbl{{\\f0\\fmodern Consolas;}}}}\\f0\\fs18 {body}}}"
    )
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
    use windows::Win32::System::Com::{STGMEDIUM, STGMEDIUM_0};

    /// FILEDESCRIPTORW 1개 구성(테스트 전용) — 이름·플래그·속성·크기 하위 32비트.
    fn fd(name: &str, flags: u32, attrs: u32, size_lo: u32) -> FILEDESCRIPTORW {
        let mut buf = [0u16; 260]; // packed 구조체 — 필드 인덱싱 불가, 통째 대입
        for (i, u) in name.encode_utf16().enumerate() {
            buf[i] = u;
        }
        FILEDESCRIPTORW {
            dwFlags: flags,
            dwFileAttributes: attrs,
            nFileSizeLow: size_lo,
            cFileName: buf,
            ..Default::default()
        }
    }

    /// FILEGROUPDESCRIPTORW 바이트 조립(cItems + 배열).
    fn group_bytes(fds: &[FILEDESCRIPTORW]) -> Vec<u8> {
        let mut bytes = (fds.len() as u32).to_le_bytes().to_vec();
        for f in fds {
            bytes.extend_from_slice(unsafe {
                std::slice::from_raw_parts(
                    (f as *const FILEDESCRIPTORW) as *const u8,
                    std::mem::size_of::<FILEDESCRIPTORW>(),
                )
            });
        }
        bytes
    }

    #[test]
    fn parse_group_descriptor_items_and_traversal_guard() {
        let fds = [
            fd("한글 문서.txt", FD_FILESIZE, 0, 7),
            fd("폴더", FD_ATTRIBUTES, FILE_ATTRIBUTE_DIRECTORY, 0),
            fd("폴더\\안쪽.bin", 0, 0, 0),
            fd("..\\탈출.txt", 0, 0, 0),       // 상위 탈출 — 기각
            fd("C:\\abs\\경로.txt", 0, 0, 0), // 드라이브 절대 — 기각
        ];
        let items = parse_group_descriptor(&group_bytes(&fds));
        assert_eq!(items.len(), 3, "부적합 이름 2건은 항목별 격리");
        assert_eq!(
            items[0],
            VirtualItem {
                rel: "한글 문서.txt".into(),
                is_dir: false,
                size: Some(7),
            }
        );
        assert!(items[1].is_dir);
        assert_eq!(items[2].rel, Path::new("폴더").join("안쪽.bin"));
        assert_eq!(items[2].size, None, "FD_FILESIZE 없음 = 크기 미상");
        // 손상 방어 — cItems가 실제보다 커도 담긴 만큼만
        let mut lying = group_bytes(&fds[..1]);
        lying[0] = 9;
        assert_eq!(parse_group_descriptor(&lying).len(), 1);
        assert!(parse_group_descriptor(&[1, 0]).is_empty(), "헤더 미달");
    }

    /// 가상 파일 소스 흉내(rdpclip 형상) — 디스크립터 HGLOBAL + FileContents IStream.
    #[windows::core::implement(IDataObject)]
    struct VirtualSource {
        descriptor: Vec<u8>,
        /// lindex → 내용(디렉터리 항목은 None).
        contents: Vec<Option<Vec<u8>>>,
    }

    impl windows::Win32::System::Com::IDataObject_Impl for VirtualSource_Impl {
        fn GetData(&self, fmt: *const FORMATETC) -> windows::core::Result<STGMEDIUM> {
            use windows::Win32::Foundation::{DV_E_FORMATETC, E_OUTOFMEMORY};
            let fmt = unsafe { &*fmt };
            if u32::from(fmt.cfFormat) == descriptor_format() {
                let bytes = &self.descriptor;
                let hmem = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) }
                    .map_err(|_| windows::core::Error::from(E_OUTOFMEMORY))?;
                unsafe {
                    let p = GlobalLock(hmem) as *mut u8;
                    std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
                    let _ = GlobalUnlock(hmem);
                }
                return Ok(STGMEDIUM {
                    tymed: TYMED_HGLOBAL.0 as u32,
                    u: STGMEDIUM_0 { hGlobal: hmem },
                    pUnkForRelease: std::mem::ManuallyDrop::new(None),
                });
            }
            if u32::from(fmt.cfFormat) == contents_format() {
                let body = self
                    .contents
                    .get(fmt.lindex as usize)
                    .and_then(|c| c.as_ref())
                    .ok_or_else(|| windows::core::Error::from(DV_E_FORMATETC))?;
                let stream = unsafe { windows::Win32::UI::Shell::SHCreateMemStream(Some(body)) }
                    .ok_or_else(|| windows::core::Error::from(E_OUTOFMEMORY))?;
                return Ok(STGMEDIUM {
                    tymed: TYMED_ISTREAM.0 as u32,
                    u: windows::Win32::System::Com::STGMEDIUM_0 {
                        pstm: std::mem::ManuallyDrop::new(Some(stream)),
                    },
                    pUnkForRelease: std::mem::ManuallyDrop::new(None),
                });
            }
            Err(windows::Win32::Foundation::DV_E_FORMATETC.into())
        }

        fn GetDataHere(
            &self,
            _: *const FORMATETC,
            _: *mut STGMEDIUM,
        ) -> windows::core::Result<()> {
            Err(windows::Win32::Foundation::E_NOTIMPL.into())
        }

        fn QueryGetData(&self, _: *const FORMATETC) -> windows_core::HRESULT {
            windows::Win32::Foundation::S_OK
        }

        fn GetCanonicalFormatEtc(
            &self,
            _: *const FORMATETC,
            _: *mut FORMATETC,
        ) -> windows_core::HRESULT {
            windows::Win32::Foundation::DATA_S_SAMEFORMATETC
        }

        fn SetData(
            &self,
            _: *const FORMATETC,
            _: *const STGMEDIUM,
            _: windows_core::BOOL,
        ) -> windows::core::Result<()> {
            Err(windows::Win32::Foundation::E_NOTIMPL.into())
        }

        fn EnumFormatEtc(
            &self,
            _: u32,
        ) -> windows::core::Result<windows::Win32::System::Com::IEnumFORMATETC> {
            Err(windows::Win32::Foundation::E_NOTIMPL.into())
        }

        fn DAdvise(
            &self,
            _: *const FORMATETC,
            _: u32,
            _: windows_core::Ref<windows::Win32::System::Com::IAdviseSink>,
        ) -> windows::core::Result<u32> {
            Err(windows::Win32::Foundation::OLE_E_ADVISENOTSUPPORTED.into())
        }

        fn DUnadvise(&self, _: u32) -> windows::core::Result<()> {
            Err(windows::Win32::Foundation::OLE_E_ADVISENOTSUPPORTED.into())
        }

        fn EnumDAdvise(
            &self,
        ) -> windows::core::Result<windows::Win32::System::Com::IEnumSTATDATA> {
            Err(windows::Win32::Foundation::OLE_E_ADVISENOTSUPPORTED.into())
        }
    }

    /// 가짜 소스 → 실제 추출 종단(실 클립보드 비접촉 — 자동 실행 가능): 파일 내용·하위 폴더·
    /// 크기 미상 스트림·기존 이름 충돌 " (2)"까지.
    #[test]
    fn extract_virtual_from_writes_files_dirs_and_resolves_collision() {
        let dir = std::env::temp_dir().join(format!("nexa_clip_virt_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("문서.txt"), "이미 있음").unwrap(); // 충돌 유도

        let fds = [
            fd("문서.txt", FD_FILESIZE, 0, 4),
            fd("폴더", FD_ATTRIBUTES, FILE_ATTRIBUTE_DIRECTORY, 0),
            fd("폴더\\안쪽.bin", 0, 0, 0), // 크기 미상 — EOF까지
        ];
        let data: IDataObject = VirtualSource {
            descriptor: group_bytes(&fds),
            contents: vec![Some(b"abcd".to_vec()), None, Some(vec![7u8; 100_000])],
        }
        .into();

        assert!(
            unsafe { data_has_virtual_files(&data) },
            "가상 파일 광고 판정(X-42 β-ⓐ DnD 수신 게이트)"
        );
        let created = unsafe { extract_virtual_from(&data, &dir) };
        assert_eq!(
            created,
            vec![dir.join("문서 (2).txt"), dir.join("폴더")],
            "최상위 2건 — 충돌은 \" (2)\""
        );
        assert_eq!(std::fs::read(dir.join("문서 (2).txt")).unwrap(), b"abcd");
        assert_eq!(
            std::fs::read_to_string(dir.join("문서.txt")).unwrap(),
            "이미 있음",
            "기존 파일 무손상"
        );
        assert_eq!(
            std::fs::read(dir.join("폴더").join("안쪽.bin")).unwrap(),
            vec![7u8; 100_000],
            "64KiB 버퍼 초과 스트림 왕복"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 워커 경로 종단(X-42 2차-ⓑ — 실 클립보드 비접촉): 계획(스트림 마샬링) → 워커
    /// 실행 → 파일 내용·진행 바이트·세그먼트 상태까지. 256KiB 버퍼 초과(다중 청크)와
    /// 하위 폴더 항목 포함. 테스트 스레드를 MTA로 초기화(앱은 UI=STA·워커=MTA —
    /// out-of-proc 소스는 표준 마샬링이 원 프로세스로 직접 잇는다).
    #[test]
    fn plan_and_run_virtual_paste_worker_round_trip() {
        use std::sync::atomic::Ordering;
        let dir = std::env::temp_dir().join(format!("nexa_clip_vworker_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let big = vec![9u8; 300_000];
        let fds = [
            fd("워커.bin", FD_FILESIZE, 0, big.len() as u32),
            fd("폴더", FD_ATTRIBUTES, FILE_ATTRIBUTE_DIRECTORY, 0),
            fd("폴더\\작은.txt", 0, 0, 0), // 크기 미상 — EOF까지
        ];
        let data: IDataObject = VirtualSource {
            descriptor: group_bytes(&fds),
            contents: vec![Some(big.clone()), None, Some(b"ab".to_vec())],
        }
        .into();
        let plan = unsafe { plan_virtual_paste(&data, &dir) }.expect("계획 수립");
        assert_eq!(plan.roots, vec![dir.join("워커.bin"), dir.join("폴더")]);
        assert_eq!(plan.total, big.len() as u64, "크기 미상 항목은 총량 제외");
        let shared = crate::win::TransferShared::new_arc();
        let h = run_virtual_paste(plan, shared.clone(), 0, 0);
        h.join().unwrap();
        assert_eq!(std::fs::read(dir.join("워커.bin")).unwrap(), big, "다중 청크 왕복");
        assert_eq!(
            std::fs::read(dir.join("폴더").join("작은.txt")).unwrap(),
            b"ab"
        );
        assert_eq!(
            shared.done_bytes.load(Ordering::Relaxed),
            big.len() as u64 + 2
        );
        let items = crate::win::plock(&shared.items).clone();
        assert_eq!(items.len(), 2, "파일 세그먼트만(폴더 제외)");
        assert!(
            items
                .iter()
                .all(|i| i.status == crate::dialog::SegStatus::Done),
            "전 세그먼트 완료"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

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
