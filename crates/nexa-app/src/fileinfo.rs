//! 파일 상세 정보(10-02 사용자 요청) — 도크 Info의 **기본 정보 8줄**(이름·종류·경로·크기·
//! 디스크 할당 크기·만든/수정한/액세스한 날짜) + 탐색기 "자세히" 탭과 같은 **형식별 상세**
//! (PPT·Excel·Word·MP4·사진 …).
//!
//! - 기본 정보 = 파일 메타데이터(동기·즉시. `GetCompressedFileSizeW` + 클러스터 올림 = 탐색기
//!   "디스크 할당 크기").
//! - 상세 = **Windows 속성 시스템**: `IShellItem2` + 형식별 `System.PropList.FullDetails`
//!   (탐색기 자세히 탭의 항목·순서·그룹 그대로) + `IPropertyDescription::FormatForDisplay`
//!   (OS 로캘 서식 — 재생 시간 "00:01:23"·"1920 x 1080"). 형식별 파서 0 — Office·미디어
//!   속성 핸들러는 OS 인박스. 속성 핸들러가 파일을 열어 수십~수백 ms 걸리므로 호출자는
//!   **워커 스레드**에서 [`details`]를 부르고 세대 가드로 결과를 받는다(win.rs).
//! - 클라우드 플레이스홀더(`RECALL_ON_DATA_ACCESS`/`OFFLINE`)는 상세를 **생략**한다 —
//!   속성 핸들러가 파일을 열면 하이드레이션(다운로드)이 일어나기 때문.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// 기본 정보(파일 시스템 메타데이터). 시각은 unix ms(`source::fmt_datetime` 입력).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Basic {
    pub is_dir: bool,
    pub size: Option<u64>,
    pub size_on_disk: Option<u64>,
    pub created: Option<i64>,
    pub modified: Option<i64>,
    pub accessed: Option<i64>,
    /// 클라우드 플레이스홀더(내용 미보유) — 상세 조회 생략 대상.
    pub placeholder: bool,
}

/// 상세 한 줄 — 그룹 머리글(탐색기 "설명·원본·미디어·비디오…") 또는 속성(라벨, 값).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetailLine {
    Group(String),
    Prop(String, String),
}

const FILE_ATTRIBUTE_OFFLINE: u32 = 0x1000;
const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x40_0000;

fn unix_ms(t: std::io::Result<SystemTime>) -> Option<i64> {
    let t = t.ok()?;
    let d = t.duration_since(UNIX_EPOCH).ok()?;
    i64::try_from(d.as_millis()).ok()
}

/// 기본 정보 — 실패(접근 불가)면 None. 폴더는 크기 항목 없음(탐색기 동일 — 계산 비용).
pub fn basic(path: &Path) -> Option<Basic> {
    let md = std::fs::metadata(path).ok()?;
    let is_dir = md.is_dir();
    #[cfg(windows)]
    let (attrs, placeholder) = {
        use std::os::windows::fs::MetadataExt;
        let a = md.file_attributes();
        (
            a,
            a & (FILE_ATTRIBUTE_OFFLINE | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS) != 0,
        )
    };
    #[cfg(not(windows))]
    let (attrs, placeholder) = (0u32, false);
    let _ = attrs;
    let size = (!is_dir).then_some(md.len());
    let size_on_disk = if is_dir || placeholder {
        None
    } else {
        size_on_disk(path, md.len())
    };
    Some(Basic {
        is_dir,
        size,
        size_on_disk,
        created: unix_ms(md.created()),
        modified: unix_ms(md.modified()),
        accessed: unix_ms(md.accessed()),
        placeholder,
    })
}

/// 디스크 할당 크기 = 실제 점유 바이트(압축/스파스 반영)를 클러스터 단위로 올림.
/// UNC 경로는 생략(볼륨 조회가 네트워크 왕복 — UI 스레드 차단 방지).
#[cfg(windows)]
fn size_on_disk(path: &Path, logical: u64) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{GetCompressedFileSizeW, GetDiskFreeSpaceW};
    let s = path.as_os_str().to_string_lossy();
    if s.starts_with("\\\\") {
        return None;
    }
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut high = 0u32;
    let low = unsafe { GetCompressedFileSizeW(PCWSTR(wide.as_ptr()), Some(&mut high)) };
    if low == u32::MAX && unsafe { windows::Win32::Foundation::GetLastError() }.0 != 0 {
        return None;
    }
    let used = ((high as u64) << 32) | low as u64;
    // 클러스터 크기 — 루트("C:\")만 질의
    let root: Vec<u16> = {
        let p = std::path::Path::new(&*s);
        let r = p
            .components()
            .next()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .unwrap_or_default();
        format!("{r}\\")
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect()
    };
    let (mut spc, mut bps, mut free, mut total) = (0u32, 0u32, 0u32, 0u32);
    let ok = unsafe {
        GetDiskFreeSpaceW(
            PCWSTR(root.as_ptr()),
            Some(&mut spc),
            Some(&mut bps),
            Some(&mut free),
            Some(&mut total),
        )
    }
    .is_ok();
    let cluster = if ok { spc as u64 * bps as u64 } else { 0 };
    Some(round_up_cluster(used, cluster, logical))
}

#[cfg(not(windows))]
fn size_on_disk(_path: &Path, _logical: u64) -> Option<u64> {
    None
}

/// 클러스터 올림(순수 — 테스트): 클러스터 0/미상이면 점유 바이트 그대로. 압축 파일은
/// 점유가 논리 크기보다 작을 수 있고(그대로 둔다), 0바이트 파일은 0.
pub fn round_up_cluster(used: u64, cluster: u64, _logical: u64) -> u64 {
    if cluster == 0 || used == 0 {
        return used;
    }
    used.div_ceil(cluster) * cluster
}

/// 탐색기식 크기 표기 — "220 KB (225,871 B)" · 1 KB 미만 = "512 B".
pub fn fmt_size_long(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["KB", "MB", "GB", "TB", "PB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut v = bytes as f64 / 1024.0;
    let mut u = 0;
    while v >= 1024.0 && u + 1 < UNITS.len() {
        v /= 1024.0;
        u += 1;
    }
    // 탐색기 규약 = 유효 3자리 **버림**(220.57 KB → "220 KB", 반올림 아님)
    let short = if v < 10.0 {
        format!("{:.2}", (v * 100.0).floor() / 100.0)
    } else if v < 100.0 {
        format!("{:.1}", (v * 10.0).floor() / 10.0)
    } else {
        format!("{:.0}", v.floor())
    };
    format!("{} {} ({} B)", short, UNITS[u], group_thousands(bytes))
}

/// 천 단위 구분 "225,871".
pub fn group_thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `System.PropList.FullDetails` 문자열("prop:System.PropGroup.Description;System.Title;…")
/// → 정식 속성 이름 목록. 항목 앞 플래그 문자(`*`·`~`·`-` 등 비영문)는 제거.
pub fn parse_proplist(s: &str) -> Vec<String> {
    let body = s.strip_prefix("prop:").unwrap_or(s);
    body.split(';')
        .map(|e| {
            e.trim_start_matches(|c: char| !c.is_ascii_alphabetic())
                .trim()
        })
        .filter(|e| !e.is_empty())
        .map(str::to_string)
        .collect()
}

/// 기본 정보 8줄과 겹치는 속성(자세히 탭의 "파일" 그룹 중복) — 상세에서 제외.
const BASIC_DUPES: [&str; 10] = [
    "System.ItemNameDisplay",
    "System.ItemTypeText",
    "System.ItemFolderPathDisplay",
    "System.ItemPathDisplay",
    "System.ParsingPath",
    "System.Size",
    "System.DateCreated",
    "System.DateModified",
    "System.DateAccessed",
    "System.ItemType",
];

/// 상세 속성(탐색기 "자세히" 탭 순서·그룹). **워커 스레드에서 호출**(COM 초기화 포함 —
/// 속성 핸들러가 파일을 연다). 빈 값·기본 정보 중복은 제외, 속성이 하나도 없는 그룹 머리글도
/// 제외. 실패(핸들러 없음·접근 불가)면 빈 벡터.
#[cfg(windows)]
pub fn details(path: &Path) -> Vec<DetailLine> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::{Interface, PCWSTR};
    use windows::Win32::Foundation::PROPERTYKEY;
    use windows::Win32::System::Com::{
        CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_APARTMENTTHREADED,
        COINIT_DISABLE_OLE1DDE,
    };
    use windows::Win32::UI::Shell::PropertiesSystem::{
        IPropertyDescription, PSGetPropertyDescription, PSGetPropertyKeyFromName, PDFF_DEFAULT,
    };
    use windows::Win32::UI::Shell::{IShellItem2, SHCreateItemFromParsingName};

    unsafe fn pwstr_to_string(p: windows::core::PWSTR) -> String {
        if p.is_null() {
            return String::new();
        }
        let s = p.to_string().unwrap_or_default();
        CoTaskMemFree(Some(p.0 as *const core::ffi::c_void));
        s
    }
    unsafe fn key_of(name: &str) -> Option<PROPERTYKEY> {
        let w: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let mut key = PROPERTYKEY::default();
        PSGetPropertyKeyFromName(PCWSTR(w.as_ptr()), &mut key).ok()?;
        Some(key)
    }

    let mut out = Vec::new();
    unsafe {
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let run = || -> Option<Vec<DetailLine>> {
            let item: IShellItem2 =
                SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None).ok()?;
            let list_key = key_of("System.PropList.FullDetails")?;
            let list = pwstr_to_string(item.GetString(&list_key).ok()?);
            let mut lines = Vec::new();
            let mut pending_group: Option<String> = None;
            for name in parse_proplist(&list) {
                if BASIC_DUPES.contains(&name.as_str()) {
                    continue;
                }
                let Some(key) = key_of(&name) else { continue };
                let mut raw: *mut core::ffi::c_void = std::ptr::null_mut();
                if PSGetPropertyDescription(&key, &IPropertyDescription::IID, &mut raw).is_err()
                    || raw.is_null()
                {
                    continue;
                }
                let desc = IPropertyDescription::from_raw(raw);
                let label = pwstr_to_string(desc.GetDisplayName().unwrap_or_default());
                if label.is_empty() {
                    continue;
                }
                if name.starts_with("System.PropGroup.") {
                    pending_group = Some(label);
                    continue;
                }
                let Ok(pv) = item.GetProperty(&key) else {
                    continue;
                };
                let value =
                    pwstr_to_string(desc.FormatForDisplay(&pv, PDFF_DEFAULT).unwrap_or_default());
                let value = value.trim().to_string();
                if value.is_empty() {
                    continue;
                }
                if let Some(g) = pending_group.take() {
                    lines.push(DetailLine::Group(g));
                }
                lines.push(DetailLine::Prop(label, value));
            }
            Some(lines)
        };
        if let Some(l) = run() {
            out = l;
        }
        if hr.is_ok() {
            CoUninitialize();
        }
    }
    out
}

#[cfg(not(windows))]
pub fn details(_path: &Path) -> Vec<DetailLine> {
    Vec::new()
}

/// 상세 조회 워커(10-02) — **레인(패널)별 최신 요청 1칸**: 방향키로 선택을 빠르게 옮기면
/// 중간 요청은 덮여 사라지고 마지막 것만 처리된다(스레드·핸들러 호출 누적 없음). 두 패널이
/// 서로의 요청을 덮지 않도록 레인 2개. 스레드 1개가 COM STA를 평생 유지 — 속성 핸들러
/// DLL이 한 번 로드된 뒤 재사용된다.
pub struct DetailWorker {
    slot: std::sync::Arc<Slot>,
}

type Job = (u64, std::path::PathBuf);
type Slot = (std::sync::Mutex<[Option<Job>; 2]>, std::sync::Condvar);

impl DetailWorker {
    /// `notify(lane, gen, path, lines)` = 워커 스레드에서 호출(호출자는 PostMessage로 UI에).
    pub fn spawn<F>(notify: F) -> Self
    where
        F: Fn(usize, u64, std::path::PathBuf, Vec<DetailLine>) + Send + 'static,
    {
        let slot: std::sync::Arc<Slot> = std::sync::Arc::new((
            std::sync::Mutex::new([None, None]),
            std::sync::Condvar::new(),
        ));
        let s2 = slot.clone();
        let _ = std::thread::Builder::new()
            .name("nexa-fileinfo".into())
            .spawn(move || {
                #[cfg(windows)]
                unsafe {
                    use windows::Win32::System::Com::{
                        CoInitializeEx, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
                    };
                    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
                }
                let (m, cv) = &*s2;
                loop {
                    let (lane, job) = {
                        let mut g = m.lock().unwrap_or_else(|e| e.into_inner());
                        loop {
                            if let Some(i) = g.iter().position(Option::is_some) {
                                break (i, g[i].take());
                            }
                            g = cv.wait(g).unwrap_or_else(|e| e.into_inner());
                        }
                    };
                    if let Some((gen, path)) = job {
                        // 핸들러 panic 격리 — 워커가 죽으면 이후 상세가 영영 안 온다
                        let lines = std::panic::catch_unwind(|| details(path.as_path()))
                            .unwrap_or_default();
                        notify(lane, gen, path, lines);
                    }
                }
            });
        DetailWorker { slot }
    }

    /// 요청(같은 레인의 미처리 요청은 덮어쓴다). `lane` = 0/1(패널).
    pub fn request(&self, lane: usize, gen: u64, path: std::path::PathBuf) {
        let (m, cv) = &*self.slot;
        m.lock().unwrap_or_else(|e| e.into_inner())[lane.min(1)] = Some((gen, path));
        cv.notify_one();
    }
}

/// 상세 조회 대상인가 — 일반 로컬 파일만(폴더·플레이스홀더[하이드레이션 방지]·클라우드 가상 제외).
pub fn wants_details(b: &Basic) -> bool {
    !b.is_dir && !b.placeholder
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_long_matches_explorer_style() {
        assert_eq!(fmt_size_long(0), "0 B");
        assert_eq!(fmt_size_long(512), "512 B");
        assert_eq!(fmt_size_long(225_871), "220 KB (225,871 B)");
        assert_eq!(fmt_size_long(1_536), "1.50 KB (1,536 B)");
        assert_eq!(fmt_size_long(10_990_000), "10.4 MB (10,990,000 B)");
        assert_eq!(fmt_size_long(3 << 30), "3.00 GB (3,221,225,472 B)");
    }

    #[test]
    fn thousands_grouping() {
        assert_eq!(group_thousands(0), "0");
        assert_eq!(group_thousands(999), "999");
        assert_eq!(group_thousands(1000), "1,000");
        assert_eq!(group_thousands(1_234_567), "1,234,567");
    }

    #[test]
    fn worker_delivers_latest_request() {
        let dir = std::env::temp_dir().join(format!("nexa-fileinfo-w-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("c.txt");
        std::fs::write(&f, b"x").unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let w = DetailWorker::spawn(move |lane, gen, path, _lines| {
            let _ = tx.send((lane, gen, path));
        });
        w.request(1, 7, f.clone());
        let got = rx
            .recv_timeout(std::time::Duration::from_secs(20))
            .expect("응답");
        assert_eq!(got, (1, 7, f));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wants_details_excludes_dirs_and_placeholders() {
        let file = Basic {
            size: Some(1),
            ..Default::default()
        };
        assert!(wants_details(&file));
        assert!(!wants_details(&Basic {
            is_dir: true,
            ..Default::default()
        }));
        assert!(!wants_details(&Basic {
            placeholder: true,
            ..file
        }));
    }

    #[test]
    fn cluster_round_up() {
        assert_eq!(round_up_cluster(0, 4096, 0), 0, "빈 파일 = 0");
        assert_eq!(round_up_cluster(1, 4096, 1), 4096);
        assert_eq!(round_up_cluster(4096, 4096, 4096), 4096);
        assert_eq!(round_up_cluster(4097, 4096, 4097), 8192);
        assert_eq!(round_up_cluster(100, 0, 100), 100, "클러스터 미상 = 그대로");
    }

    #[test]
    fn proplist_parsing_strips_prefix_and_flags() {
        let v = parse_proplist("prop:System.PropGroup.Description;*System.Title;~System.Author; System.Media.Duration;;");
        assert_eq!(
            v,
            vec![
                "System.PropGroup.Description",
                "System.Title",
                "System.Author",
                "System.Media.Duration"
            ]
        );
        assert!(parse_proplist("").is_empty());
    }

    #[test]
    fn basic_of_real_file_and_dir() {
        let dir = std::env::temp_dir().join(format!("nexa-fileinfo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.txt");
        std::fs::write(&f, b"hello").unwrap();
        let b = basic(&f).expect("file basic");
        assert!(!b.is_dir && b.size == Some(5) && b.modified.is_some());
        #[cfg(windows)]
        assert!(
            b.size_on_disk.is_some_and(|d| d >= 5),
            "할당 크기 ≥ 논리 크기"
        );
        let d = basic(&dir).expect("dir basic");
        assert!(d.is_dir && d.size.is_none() && d.size_on_disk.is_none());
        assert!(basic(&dir.join("missing")).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn details_of_plain_text_file_has_no_panic_and_no_basic_dupes() {
        let dir = std::env::temp_dir().join(format!("nexa-fileinfo-d-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("b.txt");
        std::fs::write(&f, b"hello").unwrap();
        let d = details(&f);
        for l in &d {
            if let DetailLine::Prop(label, _) = l {
                assert!(!label.is_empty());
            }
        }
        // 그룹 머리글은 반드시 속성 앞에만(빈 그룹 없음)
        for w in d.windows(2) {
            assert!(!matches!(
                (&w[0], &w[1]),
                (DetailLine::Group(_), DetailLine::Group(_))
            ));
        }
        assert!(!matches!(d.last(), Some(DetailLine::Group(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
