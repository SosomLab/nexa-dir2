//! WASM 플러그인 런타임(ADR-0005 — wasmi 임베드, 사용자 결정 07-26).
//! `data\plugins\*.wasm`(wasm32-unknown-unknown 모듈)을 파일명 순으로 로드 —
//! `.wasm` 1개가 전 OS/아키텍처 동일 동작(크로스플랫폼 단일 아티팩트).
//!
//! **ABI(ADR-0005 §계약)** — 버퍼 = 선두 4바이트 LE 길이 + UTF-8 본문:
//! - export: `memory` · `nx_meta() -> ptr`(`id\nname\next1,ext2`) ·
//!   `nx_preview() -> ptr`(첫 줄 `lines`/`image:<경로 없음 — 본문 1줄>`,
//!   이후 라인들 — `\u{2}종류|`·`\u{1}img|` 태그 계약 그대로)
//! - import(`env`): `read_text(ptr, cap) -> len`(**대상 파일만**·256KB 클램프) ·
//!   `render_svg(sptr, slen, optr, ocap) -> len` · `is_dark() -> i32` ·
//!   `disp_width(ptr, len) -> i32`
//!
//! **ABI v2 — 압축 목록(X-46, 하위 호환)**: `nx_meta`의 **4번째 줄**에 능력 선언
//! (`archive`)을 두면 호스트가 [`run_archive`]로 `nx_archive()`를 부른다.
//! - export: `nx_archive() -> ptr` — 첫 줄 `archive`|`password`|`error`
//!   (`archive` = 둘째 줄 `표시명<TAB>플래그`, 이후 항목 = `경로<TAB>원본<TAB>압축<TAB>시각<TAB>속성<TAB>방식`)
//! - import 추가: `file_size() -> i64` · `read_at(off, ptr, cap) -> n`(임의 위치 —
//!   중앙 디렉터리처럼 꼬리부터 읽어야 하는 포맷용) · `password(ptr, cap) -> n`
//!   (**활성 암호만** 전달 — 없으면 `-1`. 게스트는 `password`를 반환해 요청한다)
//!
//! 암호 취급: 호스트는 사용자가 방금 입력한 암호를 게스트 메모리에 **1회 복사**할
//! 뿐이고, 인스턴스는 호출 종료와 함께 폐기된다(스토어 소멸 = 선형 메모리 해제).
//! 호스트 쪽 사본은 [`crate::preview::archive`]의 Secret(Drop 소거)로만 존재한다.
//!
//! 격리(ADR-0004 §격리 계승): **fuel 상한**(wasmi 내장)·메모리 상한(limiter)·
//! 오류 = 해당 플러그인만 미리보기 1줄(`preview.plugin.error`).

use std::path::{Path, PathBuf};
use wasmi::{Caller, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

use super::{PreviewDoc, PreviewProvider};
use crate::i18n::trf;

/// 실행 연료 상한(호출당 — 인터프리터 명령 수. 초과 = 트랩 → 오류 1줄).
const FUEL: u64 = 200_000_000;
/// 플러그인 선형 메모리 상한(64MB — Starlark 격리와 동일 수치).
const MEM_CAP: usize = 64 * 1024 * 1024;
/// read_text 호스트 클램프.
const READ_CAP: usize = 256 * 1024;
/// 반환 본문 상한(1000줄 상당 여유 — 도크/독립 창 보호).
const OUT_CAP: usize = 1 << 20;
/// `read_at` 1회 클램프(임의 위치 읽기 — 목록 파싱에 충분하고 폭주는 막는다).
const READ_AT_CAP: usize = 4 * 1024 * 1024;
/// 압축 목록 항목 상한(그리드 보호 — nexa-vfs 상한과 동일 취지).
const ARCHIVE_CAP: usize = 50_000;
/// 호출당 **벽시계 상한**(점검 1차 #5 — ADR-0005가 약속한 시간 상한. 호스트 임포트 진입 시 검사 →
/// 초과면 트랩. 순수 게스트 루프는 연료가 막는다). UI 스레드 실행이라 짧게.
const CALL_TIMEOUT_MS: u64 = 1_500;
/// 연속 실패 격리(서킷 브레이커 — 점검 1차 #5): 이 횟수 연속 실패한 플러그인은 세션 동안 실행하지 않는다.
const BREAKER_LIMIT: u32 = 3;

/// 호스트 상태 — 미리보기 대상 파일(샌드박스: 이 파일 외 접근 불가) + 메모리 리미터 + 호출 마감 시각.
struct HostCtx {
    path: PathBuf,
    limits: StoreLimits,
    deadline: std::time::Instant,
}

/// 호스트 임포트 공통 게이트(점검 1차 #5): ① 벽시계 상한 초과 → 트랩 ② 호스트 작업 비용을 **연료에 과금**
/// — 종전은 임포트가 연료 0이라 게스트가 임포트 루프(4MB read_at·2000² SVG 래스터)로 호스트를 무한정
/// 태울 수 있었다.
fn host_guard(caller: &mut Caller<'_, HostCtx>, cost: u64) -> Result<(), wasmi::Error> {
    if std::time::Instant::now() >= caller.data().deadline {
        return Err(wasmi::Error::new(format!(
            "호출 시간 상한 {CALL_TIMEOUT_MS}ms 초과"
        )));
    }
    let fuel = caller.get_fuel()?;
    if fuel < cost {
        return Err(wasmi::Error::new("연료 소진(호스트 작업 과금)"));
    }
    caller.set_fuel(fuel - cost)
}

/// 로드된 플러그인 — 모듈은 검증·컴파일 완료 캐시(호출마다 인스턴스만 생성).
pub struct WasmPlugin {
    pub id: String,
    pub name: String,
    pub exts: Vec<String>,
    /// 능력 선언(nx_meta 4번째 줄) — `archive` = 압축 목록 공급자.
    pub caps: Vec<String>,
    module: Module,
    engine: Engine,
}

impl WasmPlugin {
    /// 압축 목록 능력을 선언했는가(ABI v2).
    pub fn is_archive(&self) -> bool {
        self.caps.iter().any(|c| c == "archive")
    }
}

/// 게스트 메모리에서 (4바이트 LE 길이 + 본문) 버퍼를 읽는다.
fn read_buf(mem: &[u8], ptr: u32) -> Option<String> {
    let p = ptr as usize;
    let len = u32::from_le_bytes(mem.get(p..p + 4)?.try_into().ok()?) as usize;
    if len > OUT_CAP {
        return None;
    }
    Some(String::from_utf8_lossy(mem.get(p + 4..p + 4 + len)?).into_owned())
}

/// 링커 구성 — 호스트 import 4종(ADR-0005 §계약).
fn linker(engine: &Engine) -> Result<Linker<HostCtx>, wasmi::Error> {
    let mut l = Linker::new(engine);
    // read_text(ptr, cap) -> len : 대상 파일 앞부분을 게스트 메모리에 기록
    l.func_wrap(
        "env",
        "read_text",
        |mut caller: Caller<'_, HostCtx>, ptr: i32, cap: i32| -> Result<i32, wasmi::Error> {
            host_guard(&mut caller, 200_000)?;
            let path = caller.data().path.clone();
            let cap = (cap.max(0) as usize).min(READ_CAP);
            let Ok((text, _)) = super::read_text(&path, cap.max(1)) else {
                return Ok(0);
            };
            let bytes = text.as_bytes();
            let n = bytes.len().min(cap);
            let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return Ok(0);
            };
            if mem.write(&mut caller, ptr as usize, &bytes[..n]).is_err() {
                return Ok(0);
            }
            Ok(n as i32)
        },
    )?;
    // render_svg(sptr, slen, optr, ocap) -> len : SVG → BMP 경로(실패 = 0)
    l.func_wrap(
        "env",
        "render_svg",
        |mut caller: Caller<'_, HostCtx>, sptr: i32, slen: i32, optr: i32, ocap: i32| -> Result<i32, wasmi::Error> {
            host_guard(&mut caller, 5_000_000)?; // GDI+ 래스터 + 임시 BMP — 가장 비싼 임포트
            let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return Ok(0);
            };
            let mut svg = vec![0u8; (slen.max(0) as usize).min(READ_CAP)];
            if mem.read(&caller, sptr as usize, &mut svg).is_err() {
                return Ok(0);
            }
            let Ok(svg) = String::from_utf8(svg) else {
                return Ok(0);
            };
            let Some(out) = super::render_svg_impl(&svg) else {
                return Ok(0);
            };
            let b = out.as_bytes();
            let n = b.len().min(ocap.max(0) as usize);
            if mem.write(&mut caller, optr as usize, &b[..n]).is_err() {
                return Ok(0);
            }
            Ok(n as i32)
        },
    )?;
    // is_dark() -> i32 : 테마 신호
    l.func_wrap(
        "env",
        "is_dark",
        |mut caller: Caller<'_, HostCtx>| -> Result<i32, wasmi::Error> {
            host_guard(&mut caller, 1_000)?;
            Ok(i32::from(super::is_dark_now()))
        },
    )?;
    // ── ABI v2(X-46 압축 목록) ──
    // file_size() -> i64 : 대상 파일 크기(꼬리 오프셋 계산용)
    l.func_wrap(
        "env",
        "file_size",
        |mut caller: Caller<'_, HostCtx>| -> Result<i64, wasmi::Error> {
            host_guard(&mut caller, 10_000)?;
            Ok(std::fs::metadata(&caller.data().path)
                .map(|m| m.len() as i64)
                .unwrap_or(-1))
        },
    )?;
    // read_at(off, ptr, cap) -> n : **대상 파일 임의 위치** 읽기(중앙 디렉터리 등)
    l.func_wrap(
        "env",
        "read_at",
        |mut caller: Caller<'_, HostCtx>, off: i64, ptr: i32, cap: i32| -> Result<i32, wasmi::Error> {
            use std::io::{Read, Seek, SeekFrom};
            host_guard(&mut caller, 100_000 + (cap.max(0) as u64) / 4)?; // 고정 + 바이트 비례
            if off < 0 {
                return Ok(0);
            }
            let path = caller.data().path.clone();
            let cap = (cap.max(0) as usize).min(READ_AT_CAP);
            let Ok(mut f) = std::fs::File::open(&path) else {
                return Ok(0);
            };
            if f.seek(SeekFrom::Start(off as u64)).is_err() {
                return Ok(0);
            }
            let mut buf = vec![0u8; cap];
            let mut got = 0;
            while got < cap {
                match f.read(&mut buf[got..]) {
                    Ok(0) => break,
                    Ok(n) => got += n,
                    Err(_) => break,
                }
            }
            let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return Ok(0);
            };
            if mem.write(&mut caller, ptr as usize, &buf[..got]).is_err() {
                return Ok(0);
            }
            Ok(got as i32)
        },
    )?;
    // password(ptr, cap) -> n : **활성 암호**(사용자가 방금 입력한 값)만 전달.
    // 없으면 -1 — 게스트는 `password` 반환으로 요청한다. 호스트는 사본을 남기지 않는다.
    l.func_wrap(
        "env",
        "password",
        |mut caller: Caller<'_, HostCtx>, ptr: i32, cap: i32| -> Result<i32, wasmi::Error> {
            host_guard(&mut caller, 10_000)?;
            let bytes = super::archive::with_active_password(|pw| pw.map(|s| s.expose().to_vec()));
            let Some(bytes) = bytes else { return Ok(-1) };
            let n = bytes.len().min(cap.max(0) as usize);
            let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return Ok(-1);
            };
            let ok = mem.write(&mut caller, ptr as usize, &bytes[..n]).is_ok();
            // 호스트 임시 사본 즉시 소거(게스트 메모리는 인스턴스 폐기와 함께 사라진다)
            let mut bytes = bytes;
            nexa_core::secret::zeroize_bytes(&mut bytes);
            Ok(if ok { n as i32 } else { -1 })
        },
    )?;
    // disp_width(ptr, len) -> i32 : 표시 폭(CJK 2칸)
    l.func_wrap(
        "env",
        "disp_width",
        |mut caller: Caller<'_, HostCtx>, ptr: i32, len: i32| -> Result<i32, wasmi::Error> {
            host_guard(&mut caller, 10_000)?;
            let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return Ok(0);
            };
            let mut b = vec![0u8; (len.max(0) as usize).min(4096)];
            if mem.read(&caller, ptr as usize, &mut b).is_err() {
                return Ok(0);
            }
            Ok(super::disp_width_impl(&String::from_utf8_lossy(&b)) as i32)
        },
    )?;
    Ok(l)
}

/// 인스턴스 생성 + `fn_name() -> ptr` 호출 후 버퍼 회수(연료·메모리 상한 적용).
fn call_buf(plugin: &WasmPlugin, path: &Path, fn_name: &str) -> Result<String, String> {
    let ctx = HostCtx {
        path: path.to_path_buf(),
        limits: StoreLimitsBuilder::new().memory_size(MEM_CAP).build(),
        deadline: std::time::Instant::now() + std::time::Duration::from_millis(CALL_TIMEOUT_MS),
    };
    let mut store = Store::new(&plugin.engine, ctx);
    store.limiter(|c| &mut c.limits);
    store.set_fuel(FUEL).map_err(|e| e.to_string())?;
    let l = linker(&plugin.engine).map_err(|e| e.to_string())?;
    let instance = l
        .instantiate_and_start(&mut store, &plugin.module)
        .map_err(|e| e.to_string())?;
    let f = instance
        .get_typed_func::<(), i32>(&store, fn_name)
        .map_err(|_| format!("{fn_name}() export 없음"))?;
    let ptr = f.call(&mut store, ()).map_err(|e| e.to_string())?;
    let mem = instance
        .get_memory(&store, "memory")
        .ok_or("memory export 없음")?;
    read_buf(mem.data(&store), ptr as u32).ok_or_else(|| "반환 버퍼 손상".into())
}

/// `.wasm` 1개 로드 — 검증·컴파일 + 메타(nx_meta) 추출.
fn load_one(path: &Path) -> Result<WasmPlugin, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("읽기 실패: {e}"))?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("모듈 8MB 상한 초과".into());
    }
    let mut cfg = wasmi::Config::default();
    cfg.consume_fuel(true); // 연료 계측 활성(격리 — ADR-0005)
    let engine = Engine::new(&cfg);
    let module = Module::new(&engine, &bytes).map_err(|e| e.to_string())?;
    let plugin = WasmPlugin {
        id: String::new(),
        name: String::new(),
        exts: Vec::new(),
        caps: Vec::new(),
        module,
        engine,
    };
    let meta = call_buf(&plugin, Path::new(""), "nx_meta")?;
    let mut it = meta.lines();
    let id = it.next().unwrap_or_default().trim().to_string();
    if id.is_empty() {
        return Err("nx_meta: id 없음".into());
    }
    let name = it.next().unwrap_or(&id).trim().to_string();
    let exts = it
        .next()
        .unwrap_or_default()
        .split(',')
        .map(|e| e.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|e| !e.is_empty())
        .collect();
    // 4번째 줄 = 능력 선언(ABI v2 — 없으면 기존 미리보기 전용 플러그인)
    let caps = it
        .next()
        .unwrap_or_default()
        .split(',')
        .map(|c| c.trim().to_ascii_lowercase())
        .filter(|c| !c.is_empty())
        .collect();
    Ok(WasmPlugin {
        id,
        name,
        exts,
        caps,
        ..plugin
    })
}

/// 디렉터리의 `*.wasm` 전부 로드(파일명 순 — 결정적). 오류는 해당 파일만 격리.
pub fn load_dir(dir: &Path) -> (Vec<WasmPlugin>, Vec<String>) {
    let mut plugins = Vec::new();
    let mut errors = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return (plugins, errors);
    };
    let mut files: Vec<PathBuf> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("wasm"))
        })
        .collect();
    files.sort();
    for f in files {
        match load_one(&f) {
            Ok(p) => plugins.push(p),
            Err(e) => errors.push(format!(
                "{}: {e}",
                f.file_name().unwrap_or_default().to_string_lossy()
            )),
        }
    }
    (plugins, errors)
}

/// preview 실행 — 반환 첫 줄 = `lines` | `image`, 이후 본문(태그 계약 그대로).
pub fn run_preview(plugin: &WasmPlugin, path: &Path) -> Result<PreviewDoc, String> {
    let out = call_buf(plugin, path, "nx_preview")?;
    let mut it = out.splitn(2, '\n');
    match it.next().unwrap_or("") {
        "lines" => Ok(PreviewDoc::Lines(
            it.next()
                .unwrap_or("")
                .lines()
                .take(1000)
                .map(|l| l.chars().take(4096).collect())
                .collect(),
        )),
        "image" => Ok(PreviewDoc::Image(
            it.next().unwrap_or("").trim().to_string(),
        )),
        k => Err(format!("알 수 없는 반환 종류: {k}")),
    }
}

/// `nx_archive()` 실행 — 압축 목록 공급자(ABI v2). 반환 계약은 모듈 헤더 참조.
///
/// 항목 줄 = `경로<TAB>원본<TAB>압축<TAB>시각(Unix 초)<TAB>속성<TAB>방식`
/// (속성 = `dir`·`enc`·`utc`·`unsafe` 쉼표 목록. 빈 칸/`-` = 모름)
pub fn run_archive(
    plugin: &WasmPlugin,
    path: &Path,
) -> Result<crate::preview::archive::ArchiveDoc, String> {
    use crate::preview::archive::{ArchiveDoc, ArchiveStatus};
    use nexa_vfs::archive::{ArchiveEntry, Listing};

    let out = call_buf(plugin, path, "nx_archive")?;
    let mut it = out.split('\n');
    let kind = it.next().unwrap_or("").trim();
    let doc = |status, listing| ArchiveDoc {
        path: path.to_path_buf(),
        listing,
        status,
        provider: plugin.id.clone(),
    };
    match kind {
        // 암호 필요 — 호스트가 프롬프트 후 같은 경로로 재호출한다
        "password" => return Ok(doc(ArchiveStatus::NeedPassword, Listing::default())),
        "error" => {
            let why = it.next().unwrap_or("").trim().to_string();
            return Ok(doc(ArchiveStatus::Failed(why), Listing::default()));
        }
        "archive" => {}
        k => return Err(format!("알 수 없는 반환 종류: {k}")),
    }
    let head = it.next().unwrap_or("");
    let mut hp = head.split('\t');
    let label = hp.next().unwrap_or(&plugin.name).trim().to_string();
    let flags: Vec<&str> = hp.next().unwrap_or("").split(',').map(str::trim).collect();
    let mut listing = Listing {
        format: plugin.id.clone(),
        label,
        solid: flags.contains(&"solid"),
        multivolume: flags.contains(&"multivolume"),
        truncated: flags.contains(&"truncated"),
        ..Default::default()
    };
    for line in it.take(ARCHIVE_CAP) {
        if line.trim().is_empty() {
            continue;
        }
        let mut f = line.split('\t');
        let raw = f.next().unwrap_or("");
        let num = |v: Option<&str>| v.and_then(|v| v.trim().parse::<u64>().ok());
        let size = num(f.next());
        let packed = num(f.next());
        let mtime = f.next().and_then(|v| v.trim().parse::<i64>().ok());
        let attr: Vec<&str> = f.next().unwrap_or("").split(',').map(str::trim).collect();
        let method = f.next().unwrap_or("").trim().to_string();
        let (p, suspicious) = nexa_vfs::archive::normalize_path(raw);
        if p.is_empty() {
            continue;
        }
        let is_dir = attr.contains(&"dir");
        listing.entries.push(ArchiveEntry {
            path: p,
            is_dir,
            size: (!is_dir).then_some(()).and(size),
            packed: (!is_dir).then_some(()).and(packed),
            modified: mtime.filter(|&t| t > 0),
            // 기본은 현지 벽시계(DOS 계열이 다수) — `utc` 속성이 있으면 epoch로 본다
            time_is_local: !attr.contains(&"utc"),
            encrypted: attr.contains(&"enc"),
            method,
            crc32: None,
            suspicious: suspicious || attr.contains(&"unsafe"),
        });
    }
    listing.has_encrypted = listing.entries.iter().any(|e| e.encrypted);
    Ok(doc(ArchiveStatus::Ok, listing))
}

/// WASM 플러그인 → 공급자 어댑터 — 실행 오류는 해당 플러그인만 1줄 격리.
/// 연속 실패 [`BREAKER_LIMIT`]회면 세션 동안 실행하지 않는다(점검 1차 #5 — 종전은 실패마다 재인스턴스 =
/// 화살표 키마다 연료 200M 소진).
pub(super) struct WasmProvider {
    pub plugin: WasmPlugin,
    /// 연속 실패 수(성공 시 0). 프로바이더는 스레드 로컬 캐시라 Cell로 충분.
    failures: std::cell::Cell<u32>,
}

impl WasmProvider {
    pub(super) fn new(plugin: WasmPlugin) -> Self {
        WasmProvider {
            plugin,
            failures: std::cell::Cell::new(0),
        }
    }

    fn tripped(&self) -> bool {
        self.failures.get() >= BREAKER_LIMIT
    }

    fn record(&self, ok: bool) {
        self.failures.set(if ok { 0 } else { self.failures.get() + 1 });
    }
}

impl PreviewProvider for WasmProvider {
    fn id(&self) -> &str {
        &self.plugin.id
    }
    fn exts(&self) -> &[String] {
        &self.plugin.exts
    }
    fn preview(&self, path: &Path) -> PreviewDoc {
        if self.tripped() {
            return PreviewDoc::Lines(vec![trf(
                "preview.plugin.disabled",
                &[&self.plugin.id, &BREAKER_LIMIT.to_string()],
            )]);
        }
        // 압축 능력 선언 플러그인은 목록 ABI로 라우팅(X-46) — 실패는 미리보기로 저하
        let result = if self.plugin.is_archive() {
            run_archive(&self.plugin, path).map(|doc| PreviewDoc::Archive(Box::new(doc)))
        } else {
            run_preview(&self.plugin, path)
        };
        self.record(result.is_ok());
        match result {
            Ok(doc) => doc,
            Err(e) => PreviewDoc::Lines(vec![trf("preview.plugin.error", &[&self.plugin.id, &e])]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 테스트 모듈(WAT) — 메타 + read_text/is_dark 사용 + 무한루프 연료 검증용.
    const WAT: &str = r#"
(module
  (import "env" "read_text" (func $read (param i32 i32) (result i32)))
  (import "env" "is_dark" (func $dark (result i32)))
  (memory (export "memory") 1)
  ;; data 세그먼트: 오프셋 1024 = 메타 본문
  (data (i32.const 1024) "up\nUpper\nabc,md")
  (func (export "nx_meta") (result i32)
    (i32.store (i32.const 1020) (i32.const 15)) ;; len("up\nUpper\nabc,md")=15
    (i32.const 1020))
  (func (export "nx_preview") (result i32)
    ;; "lines\nok:" + is_dark + read_text 1바이트 여부는 생략 — 고정 응답
    (i32.store (i32.const 2044) (i32.const 8))
    (i32.store (i32.const 2048) (i32.const 0x656e696c)) ;; "line"
    (i32.store (i32.const 2052) (i32.const 0x6b6f0a73)) ;; "s\nok"
    (i32.const 2044))
  (func (export "nx_loop") (result i32) (loop br 0) (i32.const 0))
  ;; 호스트 임포트 루프(점검 1차 #5) — 종전은 임포트가 연료 0이라 무한
  (func (export "nx_hostloop") (result i32) (loop (drop (call $dark)) (br 0)) (i32.const 0)))
"#;

    /// nx_preview export가 없는 모듈 — 매 호출 실패(브레이커 검증용).
    const WAT_BROKEN_PREVIEW: &str = r#"
(module
  (memory (export "memory") 1)
  (data (i32.const 1024) "bad\nBad\nabc")
  (func (export "nx_meta") (result i32)
    (i32.store (i32.const 1020) (i32.const 11))
    (i32.const 1020)))
"#;

    fn build(dir: &std::path::Path) {
        let bytes = wat::parse_str(WAT).unwrap();
        std::fs::write(dir.join("up.wasm"), bytes).unwrap();
    }

    /// 압축 목록 ABI v2 테스트 모듈(WAT) — `password` import 결과에 따라
    /// "암호 필요" 또는 항목 2건을 돌려준다(호스트 라우팅·파싱 동시 검증).
    fn archive_wat() -> String {
        // 데이터 = WAT 이스케이프(\n·\t)로 넣고, 길이는 버퍼 앞 4바이트에 기록
        let meta = "arc\nArchive Sample\nfoo\narchive";
        let body = concat!(
            "archive\n",
            "FOO\tsolid\n",
            "a/b.txt\t100\t40\t1700000000\tutc\tStore\n",
            "d\t0\t0\t0\tdir\t"
        );
        let esc = |s: &str| s.replace('\\', "\\\\").replace('\n', "\\n").replace('\t', "\\t");
        format!(
            r#"
(module
  (import "env" "password" (func $pw (param i32 i32) (result i32)))
  (import "env" "read_at" (func $readat (param i64 i32 i32) (result i32)))
  (import "env" "file_size" (func $fsize (result i64)))
  (memory (export "memory") 1)
  (data (i32.const 1024) "{meta}")
  (data (i32.const 2048) "{body}")
  (data (i32.const 3072) "password")
  (func (export "nx_meta") (result i32)
    (i32.store (i32.const 1020) (i32.const {meta_len}))
    (i32.const 1020))
  (func (export "nx_archive") (result i32)
    ;; 활성 암호가 없으면(-1) 호스트에 요청
    (if (i32.lt_s (call $pw (i32.const 4096) (i32.const 64)) (i32.const 0))
      (then
        (i32.store (i32.const 3068) (i32.const 8))
        (return (i32.const 3068))))
    (drop (call $fsize))
    (drop (call $readat (i64.const 0) (i32.const 5120) (i32.const 16)))
    (i32.store (i32.const 2044) (i32.const {body_len}))
    (i32.const 2044)))
"#,
            meta = esc(meta),
            body = esc(body),
            meta_len = meta.len(),
            body_len = body.len(),
        )
    }

    #[test]
    fn archive_capability_routes_to_nx_archive_and_password_flow() {
        use crate::preview::archive::ArchiveStatus;
        let d = std::env::temp_dir().join(format!("nexa_wasm_arc_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("arc.wasm"), wat::parse_str(archive_wat()).unwrap()).unwrap();
        let (plugins, errors) = load_dir(&d);
        assert_eq!(plugins.len(), 1, "{errors:?}");
        let p = &plugins[0];
        assert!(p.is_archive(), "nx_meta 4번째 줄 = 능력 선언");
        assert_eq!(p.exts, ["foo".to_string()]);

        let target = d.join("t.foo");
        std::fs::write(&target, b"payload").unwrap();
        // 암호 없음 = 플러그인이 요청 → 호스트는 NeedPassword로 번역
        let doc = run_archive(p, &target).unwrap();
        assert_eq!(doc.status, ArchiveStatus::NeedPassword);
        assert_eq!(doc.provider, "arc");

        // 암호 주입 후 = 목록 파싱(속성·시각·방식·플래그)
        let doc = crate::preview::archive::with_password_scope(
            Some(nexa_core::secret::Secret::new(b"pw".to_vec())),
            || run_archive(p, &target).unwrap(),
        );
        assert_eq!(doc.status, ArchiveStatus::Ok);
        assert_eq!(doc.listing.label, "FOO");
        assert!(doc.listing.solid);
        assert_eq!(doc.listing.entries.len(), 2);
        let f = doc
            .listing
            .entries
            .iter()
            .find(|e| e.path == "a/b.txt")
            .unwrap();
        assert_eq!((f.size, f.packed, f.method.as_str()), (Some(100), Some(40), "Store"));
        assert_eq!((f.modified, f.time_is_local), (Some(1_700_000_000), false));
        let dir = doc.listing.entries.iter().find(|e| e.path == "d").unwrap();
        assert!(dir.is_dir && dir.size.is_none(), "폴더 행은 크기 없음");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn loads_meta_runs_preview_and_fuel_traps_infinite_loop() {
        let d = std::env::temp_dir().join(format!("nexa_wasm_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        build(&d);
        std::fs::write(d.join("broken.wasm"), b"\x00asm junk").unwrap();
        let (plugins, errors) = load_dir(&d);
        assert_eq!(plugins.len(), 1, "{errors:?}");
        assert_eq!(errors.len(), 1, "깨진 모듈 격리");
        let p = &plugins[0];
        assert_eq!(
            (p.id.as_str(), p.name.as_str(), p.exts.as_slice()),
            ("up", "Upper", ["abc".to_string(), "md".into()].as_slice())
        );
        let t = d.join("t.abc");
        std::fs::write(&t, "hi").unwrap();
        match run_preview(p, &t).unwrap() {
            PreviewDoc::Lines(l) => assert_eq!(l, ["ok"]),
            _ => panic!("lines"),
        }
        // 연료 상한 — 무한 루프가 트랩으로 격리되는지(전체 프로세스 무영향)
        let err = call_buf(p, &t, "nx_loop").unwrap_err();
        assert!(!err.is_empty(), "연료 소진 트랩: {err}");
        // 점검 1차 #5: 호스트 임포트 루프도 유계(연료 과금 또는 벽시계) — 시간 상한 안에 오류
        let t0 = std::time::Instant::now();
        let err = call_buf(p, &t, "nx_hostloop").unwrap_err();
        let dt = t0.elapsed();
        assert!(!err.is_empty(), "호스트 루프 트랩: {err}");
        assert!(
            dt.as_millis() < (CALL_TIMEOUT_MS as u128) * 4,
            "호스트 루프가 상한 안에 끝나야 한다: {dt:?}"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn breaker_disables_plugin_after_consecutive_failures() {
        let d = std::env::temp_dir().join(format!("nexa_wasm_brk_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("bad.wasm"), wat::parse_str(WAT_BROKEN_PREVIEW).unwrap()).unwrap();
        let (mut plugins, _) = load_dir(&d);
        let prov = WasmProvider::new(plugins.remove(0));
        let t = d.join("x.abc");
        std::fs::write(&t, "hi").unwrap();
        for i in 0..BREAKER_LIMIT {
            match prov.preview(&t) {
                PreviewDoc::Lines(l) => assert!(l[0].contains("nx_preview"), "{i}: 실행 오류 1줄: {l:?}"),
                _ => panic!("lines"),
            }
        }
        assert!(prov.tripped(), "연속 {BREAKER_LIMIT}회 실패 → 격리");
        match prov.preview(&t) {
            PreviewDoc::Lines(l) => assert!(!l[0].contains("nx_preview"), "격리 안내로 대체: {l:?}"),
            _ => panic!("lines"),
        }
        let _ = std::fs::remove_dir_all(&d);
    }
}
