//! 셸 아이콘 캐시 — 원본 A-4 이식(IconKey + ShellIconCache: LRU + 속도 제한 로딩 큐).
//! 키 계산·LRU·큐는 플랫폼 중립(전 플랫폼 테스트), 셸 호출(SHGetFileInfoW)만 Windows.
//! 원본 교훈: 빠른 스크롤 시 셸 호출 폭주 → 크래시. 요청은 큐에 넣고 틱마다 상한 개수만 로드.

use std::collections::{HashMap, HashSet, VecDeque};

/// 파일별 고유 아이콘 확장자(확장자 공유 금지 — 원본 IconKey.PerFile).
const PER_FILE_EXTS: [&str; 7] = [".exe", ".lnk", ".ico", ".cur", ".msi", ".scr", ".appref-ms"];

/// 캐시 키: 폴더=`"dir"` · 확장자 없음=`"file"` · 일반=소문자 확장자(`".txt"`) ·
/// 파일별 고유 아이콘(exe 등)=소문자 전체 경로. 구분자 `\`·`/` 직접 처리(원본 규약).
pub fn icon_key(is_dir: bool, path: &str) -> String {
    if is_dir {
        return "dir".to_string();
    }
    let name = file_name(path);
    let ext = extension(name);
    if ext.is_empty() {
        return "file".to_string();
    }
    if PER_FILE_EXTS.contains(&ext.as_str()) {
        path.to_lowercase()
    } else {
        ext
    }
}

fn file_name(path: &str) -> &str {
    let trimmed = path.trim_end_matches(['\\', '/']);
    match trimmed.rfind(['\\', '/']) {
        Some(i) => &trimmed[i + 1..],
        None => trimmed,
    }
}

/// 소문자 확장자(`".txt"`). 없음/선행 점만(dotfile)/끝 점 = 빈 문자열.
fn extension(name: &str) -> String {
    match name.rfind('.') {
        Some(i) if i > 0 && i < name.len() - 1 => name[i..].to_lowercase(),
        _ => String::new(),
    }
}

/// LRU 캐시 + 중복 제거 로딩 큐(플랫폼 중립 — 핸들 타입 제네릭).
/// 원본 ShellIconCache의 자료구조부: Capacity 초과 시 최소 사용 축출, 큐는 키 단위 dedupe.
pub struct IconStore<T> {
    cap: usize,
    entries: HashMap<String, (T, u64)>, // (핸들, 최근 사용 tick)
    clock: u64,
    pending: VecDeque<(String, String)>, // (키, 로드 힌트=경로)
    queued: HashSet<String>,
}

impl<T> IconStore<T> {
    pub fn new(cap: usize) -> Self {
        IconStore {
            cap: cap.max(1),
            entries: HashMap::new(),
            clock: 0,
            pending: VecDeque::new(),
            queued: HashSet::new(),
        }
    }

    /// 캐시 조회(히트 시 최근 사용 갱신).
    pub fn get(&mut self, key: &str) -> Option<&T> {
        self.clock += 1;
        let clock = self.clock;
        match self.entries.get_mut(key) {
            Some((v, used)) => {
                *used = clock;
                Some(&*v)
            }
            None => None,
        }
    }

    /// 미스 시 로딩 큐 등록(키 단위 중복 제거). 이미 큐에 있으면 무시.
    pub fn request(&mut self, key: &str, hint: &str) {
        if self.entries.contains_key(key) || self.queued.contains(key) {
            return;
        }
        self.queued.insert(key.to_string());
        self.pending.push_back((key.to_string(), hint.to_string()));
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// 틱 처리분 꺼내기(최대 `n`개 — 속도 제한). 꺼낸 키는 큐에서 제거.
    pub fn take_batch(&mut self, n: usize) -> Vec<(String, String)> {
        let mut out = Vec::new();
        while out.len() < n {
            let Some((key, hint)) = self.pending.pop_front() else {
                break;
            };
            self.queued.remove(&key);
            out.push((key, hint));
        }
        out
    }

    /// 캐시에 넣고, 상한 초과 시 최소 사용 엔트리를 축출해 반환(호출자가 핸들 해제).
    pub fn insert(&mut self, key: String, value: T) -> Option<T> {
        self.clock += 1;
        self.entries.insert(key, (value, self.clock));
        if self.entries.len() > self.cap {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, (_, used))| *used)
                .map(|(k, _)| k.clone())?;
            return self.entries.remove(&oldest).map(|(v, _)| v);
        }
        None
    }

    /// 전체 핸들 배출(drop 시 해제용).
    pub fn drain(&mut self) -> Vec<T> {
        self.pending.clear();
        self.queued.clear();
        self.entries.drain().map(|(_, (v, _))| v).collect()
    }
}

/// Windows 셸 로더 — `SHGetFileInfoW` 16px 타입 아이콘.
/// 확장자/폴더 키는 `SHGFI_USEFILEATTRIBUTES`(파일 접근 없이 레지스트리 타입 아이콘 — 빠름),
/// 파일별 키(exe 등)만 실제 경로 조회.
#[cfg(windows)]
pub mod shell {
    use super::IconStore;
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_FLAGS_AND_ATTRIBUTES,
    };
    use windows::Win32::UI::Shell::{
        SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_SMALLICON, SHGFI_USEFILEATTRIBUTES,
    };
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, HICON};

    /// LRU 상한(원본 Capacity=256) · 틱당 로드 상한(원본 MaxConcurrent=4) · 틱 주기(원본 80ms).
    pub const CAPACITY: usize = 256;
    pub const BATCH: usize = 4;
    pub const TICK_MS: u32 = 80;

    pub struct ShellIcons {
        store: IconStore<HICON>,
    }

    impl ShellIcons {
        pub fn new() -> Self {
            ShellIcons {
                store: IconStore::new(CAPACITY),
            }
        }

        /// 캐시 조회 — 미스면 큐 등록 후 `None`(틱이 로드).
        pub fn get_or_request(&mut self, key: &str, hint: &str) -> Option<HICON> {
            if let Some(&h) = self.store.get(key) {
                return Some(h);
            }
            self.store.request(key, hint);
            None
        }

        pub fn has_pending(&self) -> bool {
            self.store.has_pending()
        }

        /// 틱 — 큐에서 최대 [`BATCH`]개 로드. 하나라도 로드했으면 `true`(다시 그리기 필요).
        pub fn tick(&mut self) -> bool {
            let batch = self.store.take_batch(BATCH);
            let mut loaded = false;
            for (key, hint) in batch {
                if let Some(icon) = unsafe { load_icon(&key, &hint) } {
                    if let Some(evicted) = self.store.insert(key, icon) {
                        unsafe {
                            let _ = DestroyIcon(evicted);
                        }
                    }
                    loaded = true;
                }
            }
            loaded
        }
    }

    impl ShellIcons {
        /// 상주 트림(M2-8 — 원본 01 §5-1) — 전 핸들 해제·큐 비움.
        /// 가시 행이 다음 페인트에서 재요청하므로 지연 재적재로 복원된다.
        pub fn trim(&mut self) {
            for h in self.store.drain() {
                unsafe {
                    let _ = DestroyIcon(h);
                }
            }
        }
    }

    impl Drop for ShellIcons {
        fn drop(&mut self) {
            for h in self.store.drain() {
                unsafe {
                    let _ = DestroyIcon(h);
                }
            }
        }
    }

    /// 키 종류별 셸 아이콘 로드. 실패(접근 불가 등) 시 `None` — 글리프 없이 공백 유지(오류 격리).
    unsafe fn load_icon(key: &str, hint: &str) -> Option<HICON> {
        let mut info = SHFILEINFOW::default();
        let flags = SHGFI_ICON | SHGFI_SMALLICON;
        let ok = if key == "dir" {
            SHGetFileInfoW(
                w!("folder"),
                FILE_ATTRIBUTE_DIRECTORY,
                Some(&mut info),
                std::mem::size_of::<SHFILEINFOW>() as u32,
                flags | SHGFI_USEFILEATTRIBUTES,
            )
        } else if key == "file" || key.starts_with('.') {
            // 타입 아이콘: 더미 파일명 + USEFILEATTRIBUTES — 파일 접근 없음
            let dummy = HSTRING::from(format!("x{}", if key == "file" { "" } else { key }));
            SHGetFileInfoW(
                PCWSTR(dummy.as_ptr()),
                FILE_ATTRIBUTE_NORMAL,
                Some(&mut info),
                std::mem::size_of::<SHFILEINFOW>() as u32,
                flags | SHGFI_USEFILEATTRIBUTES,
            )
        } else {
            // 파일별 고유 아이콘(exe·lnk…) — 실제 경로 조회
            let path = HSTRING::from(hint);
            SHGetFileInfoW(
                PCWSTR(path.as_ptr()),
                FILE_FLAGS_AND_ATTRIBUTES(0),
                Some(&mut info),
                std::mem::size_of::<SHFILEINFOW>() as u32,
                flags,
            )
        };
        (ok != 0 && !info.hIcon.is_invalid()).then_some(info.hIcon)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── IconKey (원본 IconKeyTests 이식) ──

    #[test]
    fn key_dir_file_and_ext() {
        assert_eq!(icon_key(true, "C:\\Users\\x"), "dir");
        assert_eq!(icon_key(false, "C:\\a\\README"), "file");
        assert_eq!(icon_key(false, "C:\\a\\b.TXT"), ".txt");
        assert_eq!(icon_key(false, "/unix/style/c.Md"), ".md");
    }

    #[test]
    fn key_per_file_exts_use_full_path() {
        assert_eq!(icon_key(false, "C:\\Tools\\App.EXE"), "c:\\tools\\app.exe");
        assert_eq!(icon_key(false, "C:\\l\\Short.Lnk"), "c:\\l\\short.lnk");
    }

    #[test]
    fn key_dotfile_and_trailing_dot_are_generic_file() {
        assert_eq!(icon_key(false, "C:\\a\\.gitignore"), "file");
        assert_eq!(icon_key(false, "C:\\a\\name."), "file");
    }

    // ── IconStore (LRU + 큐) ──

    #[test]
    fn lru_evicts_least_recently_used() {
        let mut s: IconStore<u32> = IconStore::new(2);
        assert_eq!(s.insert("a".into(), 1), None);
        assert_eq!(s.insert("b".into(), 2), None);
        assert_eq!(s.get("a"), Some(&1)); // a를 최근 사용으로
        let evicted = s.insert("c".into(), 3); // 상한 초과 → b 축출
        assert_eq!(evicted, Some(2));
        assert_eq!(s.get("a"), Some(&1));
        assert_eq!(s.get("b"), None);
        assert_eq!(s.get("c"), Some(&3));
    }

    #[test]
    fn queue_dedupes_and_batches() {
        let mut s: IconStore<u32> = IconStore::new(8);
        s.request(".txt", "a.txt");
        s.request(".txt", "b.txt"); // 같은 키 — 무시
        s.request("dir", "C:\\d");
        assert!(s.has_pending());
        let batch = s.take_batch(4);
        assert_eq!(batch.len(), 2);
        assert_eq!(batch[0].0, ".txt");
        assert!(!s.has_pending());
        // 캐시에 들어간 키는 재요청 안 됨
        s.insert(".txt".into(), 9);
        s.request(".txt", "c.txt");
        assert!(!s.has_pending());
    }
}
