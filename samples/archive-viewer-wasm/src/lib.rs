//! archive-viewer.wasm — **압축 목록 플러그인 참조 구현**(ABI v2 · X-46).
//!
//! 내장이 다루지 않는 아카이브 3종을 붙여 "포맷 확장은 플러그인 파일 하나"임을
//! 보여 준다 — 앱을 다시 빌드하지 않고 `data\plugins\`에 `.wasm`을 넣기만 하면 된다.
//!
//! | 포맷 | 확장자 | 읽는 방법 |
//! | --- | --- | --- |
//! | ISO 9660 (+Joliet) | `.iso` | PVD/SVD → 루트 디렉터리 레코드 재귀 |
//! | ar | `.a`·`.deb`·`.lib` | 60바이트 ASCII 헤더 순회(GNU 긴 이름 표 지원) |
//! | cpio (newc) | `.cpio` | 110바이트 16진 헤더 순회 |
//!
//! 셋 다 **목록이 평문**이라 압축 해제가 필요 없다(내장 리더와 같은 원리).
//!
//! ## ABI(요약 — 전체는 docs/24 · docs/28)
//!
//! - export `nx_meta()` → `id\n표시명\n확장자들\narchive`(**4번째 줄 = 능력 선언**)
//! - export `nx_archive()` → 첫 줄 `archive`|`password`|`error`
//!   - `archive`: 둘째 줄 `표시명<TAB>플래그`, 이후 한 줄 = 한 항목
//!     `경로<TAB>원본<TAB>압축<TAB>시각<TAB>속성<TAB>방식`
//!     (속성 = `dir`,`enc`,`utc`,`unsafe` 쉼표 목록)
//! - import `file_size()` · `read_at(off, ptr, cap)` · `password(ptr, cap)`
//!   (암호가 필요한 포맷은 `password`를 반환해 호스트에 입력을 요청한다 — 이 샘플의
//!   3종은 암호 개념이 없어 사용하지 않는다. 사용 예는 README 참조)

#![allow(clippy::missing_safety_doc)]

// ── 호스트 import(ABI v2) ───────────────────────────────────────────────

#[link(wasm_import_module = "env")]
extern "C" {
    fn file_size() -> i64;
    fn read_at(off: i64, ptr: *mut u8, cap: i32) -> i32;
}

/// 임의 위치 읽기 — 요청한 만큼 못 읽으면 짧게 돌아온다.
fn read(off: u64, len: usize) -> Vec<u8> {
    let mut buf = vec![0u8; len];
    let n = unsafe { read_at(off as i64, buf.as_mut_ptr(), len as i32) }.max(0) as usize;
    buf.truncate(n.min(len));
    buf
}

fn size() -> u64 {
    unsafe { file_size() }.max(0) as u64
}

/// 반환 버퍼(선두 4바이트 LE 길이) — 인스턴스는 호출당 1회라 leak = 무해.
fn ret(s: &str) -> *mut u8 {
    let b = s.as_bytes();
    let mut v = Vec::with_capacity(4 + b.len());
    v.extend_from_slice(&(b.len() as u32).to_le_bytes());
    v.extend_from_slice(b);
    Box::leak(v.into_boxed_slice()).as_mut_ptr()
}

/// 목록 상한(호스트도 자체 상한이 있지만 게스트에서 먼저 끊는다 — 연료 절약).
const MAX_ENTRIES: usize = 20_000;

/// 항목 한 줄 조립.
fn row(path: &str, size: Option<u64>, packed: Option<u64>, mtime: i64, attr: &str, method: &str) -> String {
    let n = |v: Option<u64>| v.map(|v| v.to_string()).unwrap_or_default();
    format!(
        "{path}\t{}\t{}\t{mtime}\t{attr}\t{method}",
        n(size),
        n(packed)
    )
}

#[no_mangle]
pub extern "C" fn nx_meta() -> *mut u8 {
    // id · 표시명 · 확장자 · **능력 선언**(archive = 압축 목록 공급자)
    ret("archive-sample\nArchive Sample (ISO/ar/cpio)\niso,a,deb,lib,cpio\narchive")
}

#[no_mangle]
pub extern "C" fn nx_archive() -> *mut u8 {
    let head = read(0, 512);
    let body = if iso::sniff(&head) {
        iso::list()
    } else if ar::sniff(&head) {
        ar::list()
    } else if cpio::sniff(&head) {
        cpio::list()
    } else {
        return ret("error\n알 수 없는 형식입니다(ISO 9660 · ar · cpio 지원)");
    };
    ret(&body)
}

// ── 공통 유틸 ───────────────────────────────────────────────────────────

fn u32le(b: &[u8], o: usize) -> u32 {
    match b.get(o..o + 4) {
        Some(s) => u32::from_le_bytes([s[0], s[1], s[2], s[3]]),
        None => 0,
    }
}

/// (y, m, d, h, mi, s) → Unix 초 — days_from_civil(Howard Hinnant).
fn to_unix(y: i64, m: i64, d: i64, h: i64, mi: i64, s: i64) -> i64 {
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = if y2 >= 0 { y2 } else { y2 - 399 } / 400;
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468) * 86_400 + h * 3600 + mi * 60 + s
}

/// ASCII 10진/8진/16진 필드 해석(공백·NUL 종료).
fn radix(field: &[u8], radix: u32) -> u64 {
    let s: String = field
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| b as char)
        .collect();
    u64::from_str_radix(s.trim(), radix).unwrap_or(0)
}

// ── ISO 9660 ────────────────────────────────────────────────────────────

mod iso {
    use super::*;

    const SECTOR: u64 = 2048;
    /// 디렉터리 재귀 깊이 상한(순환·손상 방어).
    const MAX_DEPTH: usize = 12;

    pub fn sniff(_head: &[u8]) -> bool {
        // 시그니처는 16번째 섹터에 있다(파일 앞 512B로는 알 수 없다)
        let d = read(16 * SECTOR, 8);
        d.len() >= 6 && &d[1..6] == b"CD001"
    }

    /// 볼륨 기술자 훑기 — (루트 레코드, Joliet 여부).
    fn root_record() -> Option<(Vec<u8>, bool)> {
        let total = size();
        let mut primary: Option<Vec<u8>> = None;
        let mut joliet: Option<Vec<u8>> = None;
        for i in 16..40u64 {
            let at = i * SECTOR;
            if at + SECTOR > total {
                break;
            }
            let d = read(at, 190);
            if d.len() < 190 || &d[1..6] != b"CD001" {
                break;
            }
            match d[0] {
                1 => primary = Some(d[156..190].to_vec()),
                // 보조 기술자 + UCS-2 이스케이프(%/@ %/C %/E) = Joliet
                2 if matches!(d.get(88..91), Some(b"%/@") | Some(b"%/C") | Some(b"%/E")) => {
                    joliet = Some(d[156..190].to_vec())
                }
                255 => break, // 종료 기술자
                _ => {}
            }
        }
        match (joliet, primary) {
            (Some(j), _) => Some((j, true)),
            (None, Some(p)) => Some((p, false)),
            _ => None,
        }
    }

    /// 디렉터리 레코드의 7바이트 시각 → Unix 초(UTC 보정 포함).
    fn rec_time(b: &[u8]) -> i64 {
        let g = |i: usize| b.get(i).copied().unwrap_or(0) as i64;
        if g(0) == 0 {
            return 0;
        }
        let tz = b.get(6).copied().unwrap_or(0) as i8 as i64; // 15분 단위
        to_unix(1900 + g(0), g(1), g(2), g(3), g(4), g(5)) - tz * 15 * 60
    }

    /// 이름 디코드 — Joliet = UTF-16BE, 아니면 ASCII(`;1` 버전 접미 제거).
    fn rec_name(b: &[u8], joliet: bool) -> String {
        let s = if joliet {
            let units: Vec<u16> = b
                .chunks_exact(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&units)
        } else {
            b.iter().map(|&c| c as char).collect()
        };
        match s.split_once(';') {
            Some((n, _)) => n.to_string(),
            None => s,
        }
    }

    /// 한 디렉터리(extent, 길이)를 훑어 항목을 모은다.
    fn walk(out: &mut Vec<String>, extent: u64, len: u64, prefix: &str, joliet: bool, depth: usize) {
        if depth > MAX_DEPTH || len == 0 || out.len() >= MAX_ENTRIES {
            return;
        }
        let data = read(extent * SECTOR, (len as usize).min(4 * 1024 * 1024));
        let mut p = 0usize;
        // (자식 디렉터리는 현재 디렉터리를 다 읽은 뒤 재귀 — 읽기 재진입 최소화)
        let mut subdirs: Vec<(u64, u64, String)> = Vec::new();
        while p < data.len() && out.len() < MAX_ENTRIES {
            let rlen = data[p] as usize;
            if rlen == 0 {
                // 섹터 경계 패딩 — 다음 섹터 시작으로
                p = (p / SECTOR as usize + 1) * SECTOR as usize;
                continue;
            }
            if p + rlen > data.len() {
                break;
            }
            let rec = &data[p..p + rlen];
            let ext = u32le(rec, 2) as u64;
            let dlen = u32le(rec, 10) as u64;
            let flags = rec.get(25).copied().unwrap_or(0);
            let nlen = rec.get(32).copied().unwrap_or(0) as usize;
            let name_b = rec.get(33..33 + nlen).unwrap_or(&[]);
            p += rlen;
            // '.'(0x00)·'..'(0x01) 자기/상위 항목은 건너뛴다
            if nlen == 1 && (name_b[0] == 0 || name_b[0] == 1) {
                continue;
            }
            let name = rec_name(name_b, joliet);
            if name.is_empty() {
                continue;
            }
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let is_dir = flags & 0x02 != 0;
            let t = rec_time(rec.get(18..25).unwrap_or(&[]));
            out.push(row(
                &path,
                (!is_dir).then_some(dlen),
                (!is_dir).then_some(dlen), // 비압축 컨테이너 = 원본 = 저장 크기
                t,
                if is_dir { "dir,utc" } else { "utc" },
                if is_dir { "" } else { "Store" },
            ));
            if is_dir {
                subdirs.push((ext, dlen, path));
            }
        }
        for (ext, dlen, path) in subdirs {
            walk(out, ext, dlen, &path, joliet, depth + 1);
        }
    }

    pub fn list() -> String {
        let Some((root, joliet)) = root_record() else {
            return "error\nISO 볼륨 기술자를 찾지 못했습니다".into();
        };
        let mut rows = Vec::new();
        walk(
            &mut rows,
            u32le(&root, 2) as u64,
            u32le(&root, 10) as u64,
            "",
            joliet,
            0,
        );
        let label = if joliet { "ISO 9660 (Joliet)" } else { "ISO 9660" };
        let flags = if rows.len() >= MAX_ENTRIES { "truncated" } else { "" };
        format!("archive\n{label}\t{flags}\n{}", rows.join("\n"))
    }
}

// ── ar (.a / .deb / .lib) ───────────────────────────────────────────────

mod ar {
    use super::*;

    const MAGIC: &[u8] = b"!<arch>\n";
    const HDR: usize = 60;

    pub fn sniff(head: &[u8]) -> bool {
        head.starts_with(MAGIC)
    }

    pub fn list() -> String {
        let total = size();
        let mut rows = Vec::new();
        // GNU 긴 이름 표("//" 멤버) — `/오프셋` 이름이 이 표를 가리킨다
        let mut longnames: Vec<u8> = Vec::new();
        let mut off = MAGIC.len() as u64;
        while off + HDR as u64 <= total && rows.len() < MAX_ENTRIES {
            let h = read(off, HDR);
            if h.len() < HDR || &h[58..60] != b"`\n" {
                break;
            }
            let name_raw = String::from_utf8_lossy(&h[0..16]).trim_end().to_string();
            let mtime = radix(&h[16..28], 10) as i64;
            let fsize = radix(&h[48..58], 10);
            let data_at = off + HDR as u64;
            if name_raw == "//" {
                // 긴 이름 표 자체는 목록에 넣지 않는다
                longnames = read(data_at, (fsize as usize).min(1 << 20));
                off = data_at + fsize + (fsize & 1);
                continue;
            }
            if name_raw == "/" || name_raw == "/SYM64/" {
                off = data_at + fsize + (fsize & 1); // 심볼 인덱스 — 표시 생략
                continue;
            }
            let name = if let Some(idx) =
                name_raw.strip_prefix('/').and_then(|d| d.parse::<usize>().ok())
            {
                // GNU: 긴 이름 표에서 꺼낸다(`/` 또는 개행으로 끝난다)
                let tail = longnames.get(idx..).unwrap_or(&[]);
                let end = tail
                    .iter()
                    .position(|&b| b == b'/' || b == 10)
                    .unwrap_or(tail.len());
                String::from_utf8_lossy(&tail[..end]).to_string()
            } else if let Some(n) =
                name_raw.strip_prefix("#1/").and_then(|d| d.parse::<usize>().ok())
            {
                // BSD: 이름이 데이터 앞부분에 온다
                let nb = read(data_at, n.min(1024));
                String::from_utf8_lossy(&nb).trim_end_matches(char::from(0)).to_string()
            } else {
                name_raw.trim_end_matches('/').to_string()
            };
            if !name.is_empty() {
                rows.push(row(&name, Some(fsize), Some(fsize), mtime, "utc", "Store"));
            }
            off = data_at + fsize + (fsize & 1); // 멤버는 짝수 경계 정렬
        }
        format!("archive\nar\t\n{}", rows.join("\n"))
    }
}

// ── cpio (newc) ─────────────────────────────────────────────────────────

mod cpio {
    use super::*;

    const HDR: usize = 110;

    pub fn sniff(head: &[u8]) -> bool {
        head.starts_with(b"070701") || head.starts_with(b"070702")
    }

    pub fn list() -> String {
        let total = size();
        let mut rows = Vec::new();
        let mut off = 0u64;
        while off + HDR as u64 <= total && rows.len() < MAX_ENTRIES {
            let h = read(off, HDR);
            if h.len() < HDR || !(h.starts_with(b"070701") || h.starts_with(b"070702")) {
                break;
            }
            let mode = radix(&h[14..22], 16);
            let mtime = radix(&h[46..54], 16) as i64;
            let fsize = radix(&h[54..62], 16);
            let nsize = radix(&h[94..102], 16) as usize;
            let name_b = read(off + HDR as u64, nsize.min(4096));
            let name = String::from_utf8_lossy(&name_b)
                .trim_end_matches('\0')
                .to_string();
            if name == "TRAILER!!!" {
                break;
            }
            // newc: 이름·데이터 모두 4바이트 경계 정렬
            let name_end = off + HDR as u64 + nsize as u64;
            let data_at = (name_end + 3) & !3;
            let is_dir = mode & 0o170000 == 0o040000;
            if !name.is_empty() && name != "." {
                rows.push(row(
                    &name,
                    (!is_dir).then_some(fsize),
                    (!is_dir).then_some(fsize),
                    mtime,
                    if is_dir { "dir,utc" } else { "utc" },
                    if is_dir { "" } else { "Store" },
                ));
            }
            off = (data_at + fsize + 3) & !3;
        }
        format!("archive\ncpio (newc)\t\n{}", rows.join("\n"))
    }
}
