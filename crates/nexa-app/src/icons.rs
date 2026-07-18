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

    /// 틱 처리분 꺼내기(최대 `n`개 — 속도 제한). 꺼낸 키는 **로드 완료([`finish`])까지
    /// 큐 표시(in-flight) 유지** — 비동기 로드 중 재요청(재페인트)이 중복 로드를 만들지 않게.
    pub fn take_batch(&mut self, n: usize) -> Vec<(String, String)> {
        let mut out = Vec::new();
        while out.len() < n {
            let Some((key, hint)) = self.pending.pop_front() else {
                break;
            };
            out.push((key, hint));
        }
        out
    }

    /// 로드 완료 표시(성공·실패 공통) — 이후 미스는 다시 요청 가능.
    pub fn finish(&mut self, key: &str) {
        self.queued.remove(key);
    }

    /// 캐시에 넣고, 대체된 기존 값 또는 상한 초과 축출 값을 반환(호출자가 핸들 해제).
    pub fn insert(&mut self, key: String, value: T) -> Option<T> {
        self.clock += 1;
        if let Some((old, _)) = self.entries.insert(key, (value, self.clock)) {
            return Some(old); // 같은 키 재로드(트림 경합 등) — 이전 핸들 반환
        }
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
/// 확장자/폴더 키는 `SHGFI_USEFILEATTRIBUTES`(파일 접근 없이 레지스트리 타입 아이콘 — 빠름)로
/// **동기** 로드, 파일별 키(exe·lnk 등)는 실제 파일을 여는 조회라 **워커 스레드**에서 로드 후
/// `PostMessage`로 회수(QA 07-14 — Downloads의 대형 다운로드 exe가 Defender 실시간 검사로
/// 수십 초 블로킹 → UI "응답 없음". 원본은 WinRT GetThumbnailAsync라 비동기였음).
#[cfg(windows)]
pub mod shell {
    use std::sync::mpsc;

    use super::IconStore;
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_FLAGS_AND_ATTRIBUTES,
    };
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{
        SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_SMALLICON, SHGFI_USEFILEATTRIBUTES,
    };
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, PostMessageW, HICON};

    /// LRU 상한(원본 Capacity=256) · 틱당 로드 상한(원본 MaxConcurrent=4) · 틱 주기(원본 80ms).
    pub const CAPACITY: usize = 256;
    pub const BATCH: usize = 4;
    pub const TICK_MS: u32 = 80;

    /// 임베드 툴바 아이콘(07-18 — 사용자: "도구 모음을 16x16 이미지 형태로").
    /// 키 = `emb:<이름>:<크기>` — dw.rs [`draw_icon`](crate::dw)이 그리기 크기에 맞는
    /// 버킷(16/20/32 = 100/125/200% DPI)을 골라 조회. 원본 64px 벡터에서 다운스케일
    /// 생성([assets/toolbar](../assets/toolbar/README.md)) 후 exe에 임베드(포터블 규약).
    macro_rules! emb {
        ($($name:literal),+ $(,)?) => {
            &[$(
                (concat!("emb:", $name, ":16"),
                 include_bytes!(concat!("../assets/toolbar/", $name, "-16.png")) as &[u8]),
                (concat!("emb:", $name, ":20"),
                 include_bytes!(concat!("../assets/toolbar/", $name, "-20.png")) as &[u8]),
                (concat!("emb:", $name, ":32"),
                 include_bytes!(concat!("../assets/toolbar/", $name, "-32.png")) as &[u8]),
            )+]
        };
    }
    const EMBEDDED: &[(&str, &[u8])] = emb!(
        "panel-dual",
        "panel-single",
        "colsync",
        "colsync-disabled",
        "view-tree",
        "view-flat",
        "view-tiles",
        "refresh",
        "settings",
        "hidden",
        "dotfiles",
    );

    /// 워커 요청: (키, 경로, 대상 창 raw, 통지 메시지).
    type Req = (String, String, isize, u32);
    /// 워커 결과(PostMessage WPARAM으로 전달되는 Box): (키, HICON raw — 0=실패).
    pub type LoadResult = (String, isize);

    pub struct ShellIcons {
        store: IconStore<HICON>,
        /// 파일별 아이콘 로더 스레드 채널(지연 생성 — 아이콘 없는 세션은 스레드 0).
        tx: Option<mpsc::Sender<Req>>,
    }

    impl ShellIcons {
        pub fn new() -> Self {
            ShellIcons {
                store: IconStore::new(CAPACITY),
                tx: None,
            }
        }

        /// 캐시 조회 — 미스면 큐 등록 후 `None`(틱이 로드).
        /// `emb:` 키는 임베드 PNG에서 **동기** 생성(디코드 1회 — 이후 캐시 히트,
        /// LRU 축출돼도 다음 조회에서 재생성).
        pub fn get_or_request(&mut self, key: &str, hint: &str) -> Option<HICON> {
            if let Some(&h) = self.store.get(key) {
                return Some(h);
            }
            if key.starts_with("emb:") {
                let bytes = EMBEDDED.iter().find(|(k, _)| *k == key).map(|(_, b)| *b)?;
                let icon = unsafe { crate::ctl::gdipctx::png_to_hicon(bytes) }?;
                if let Some(old) = self.store.insert(key.to_string(), icon) {
                    unsafe {
                        let _ = DestroyIcon(old);
                    }
                }
                return Some(icon);
            }
            self.store.request(key, hint);
            None
        }

        pub fn has_pending(&self) -> bool {
            self.store.has_pending()
        }

        /// 틱 — 큐에서 최대 [`BATCH`]개 처리. 타입 아이콘(레지스트리)은 즉시 로드,
        /// 파일별 아이콘은 워커로 넘기고 결과는 `msg`(WPARAM=Box<[`Result`]>)로 돌아온다
        /// → [`on_result`]. 동기 로드가 있었으면 `true`(다시 그리기 필요).
        pub fn tick(&mut self, hwnd: HWND, msg: u32) -> bool {
            let batch = self.store.take_batch(BATCH);
            let mut loaded = false;
            for (key, hint) in batch {
                let per_file = key != "dir" && key != "file" && !key.starts_with('.');
                if per_file {
                    let _ = self.worker().send((key, hint, hwnd.0 as isize, msg));
                } else {
                    if let Some(icon) = unsafe { load_icon(&key, &hint) } {
                        if let Some(old) = self.store.insert(key.clone(), icon) {
                            unsafe {
                                let _ = DestroyIcon(old);
                            }
                        }
                        loaded = true;
                    }
                    self.store.finish(&key);
                }
            }
            loaded
        }

        /// 워커 결과 반영(UI 스레드 — `msg` 핸들러에서). 캐시에 들어갔으면 `true`.
        pub fn on_result(&mut self, key: String, raw: isize) -> bool {
            self.store.finish(&key);
            if raw == 0 {
                return false; // 실패 — 다음 미스에서 재시도
            }
            let icon = HICON(raw as *mut core::ffi::c_void);
            if let Some(old) = self.store.insert(key, icon) {
                unsafe {
                    let _ = DestroyIcon(old);
                }
            }
            true
        }

        /// 로더 스레드(1개) 지연 생성 — SHGetFileInfoW 규약대로 COM(STA) 초기화 후 순차 처리.
        /// 채널이 닫히면(ShellIcons drop) 자연 종료.
        fn worker(&mut self) -> &mpsc::Sender<Req> {
            if self.tx.is_none() {
                let (tx, rx) = mpsc::channel::<Req>();
                std::thread::spawn(move || {
                    unsafe {
                        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                    }
                    while let Ok((key, hint, hwnd_raw, msg)) = rx.recv() {
                        let raw = unsafe { load_icon(&key, &hint) }
                            .map(|h| h.0 as isize)
                            .unwrap_or(0);
                        let boxed = Box::into_raw(Box::new((key, raw)));
                        unsafe {
                            let hwnd = HWND(hwnd_raw as *mut core::ffi::c_void);
                            if PostMessageW(Some(hwnd), msg, WPARAM(boxed as usize), LPARAM(0))
                                .is_err()
                            {
                                // 창 소멸 등 — 결과 회수 불가, 여기서 정리
                                let (_, raw) = *Box::from_raw(boxed);
                                if raw != 0 {
                                    let _ = DestroyIcon(HICON(raw as *mut core::ffi::c_void));
                                }
                            }
                        }
                    }
                });
                self.tx = Some(tx);
            }
            self.tx.as_ref().unwrap()
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
    /// `L|` 접두사 = **라지 아이콘**(32px — 타일 보기 07-16). 캐시 키가 달라 소/라지 공존.
    unsafe fn load_icon(key: &str, hint: &str) -> Option<HICON> {
        let mut info = SHFILEINFOW::default();
        let (key, size_flag) = match key.strip_prefix("L|") {
            Some(k) => (k, windows::Win32::UI::Shell::SHGFI_LARGEICON),
            None => (key, SHGFI_SMALLICON),
        };
        let flags = SHGFI_ICON | size_flag;
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
    fn take_batch_keeps_inflight_until_finish() {
        let mut s: IconStore<u32> = IconStore::new(8);
        s.request("c:\\a.exe", "C:\\a.exe");
        assert_eq!(s.take_batch(4).len(), 1);
        s.request("c:\\a.exe", "C:\\a.exe"); // 비동기 로드 중 재요청 — 무시
        assert!(!s.has_pending());
        s.finish("c:\\a.exe"); // 실패 통지 후에는 재시도 가능
        s.request("c:\\a.exe", "C:\\a.exe");
        assert!(s.has_pending());
    }

    #[test]
    fn insert_same_key_returns_replaced() {
        let mut s: IconStore<u32> = IconStore::new(8);
        assert_eq!(s.insert("k".into(), 1), None);
        assert_eq!(
            s.insert("k".into(), 2),
            Some(1),
            "대체된 이전 값 반환(핸들 해제용)"
        );
        assert_eq!(s.get("k"), Some(&2));
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
