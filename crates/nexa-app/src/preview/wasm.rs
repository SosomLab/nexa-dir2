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

/// 호스트 상태 — 미리보기 대상 파일(샌드박스: 이 파일 외 접근 불가) + 메모리 리미터.
struct HostCtx {
    path: PathBuf,
    limits: StoreLimits,
}

/// 로드된 플러그인 — 모듈은 검증·컴파일 완료 캐시(호출마다 인스턴스만 생성).
pub struct WasmPlugin {
    pub id: String,
    pub name: String,
    pub exts: Vec<String>,
    module: Module,
    engine: Engine,
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
        |mut caller: Caller<'_, HostCtx>, ptr: i32, cap: i32| -> i32 {
            let path = caller.data().path.clone();
            let cap = (cap.max(0) as usize).min(READ_CAP);
            let Ok((text, _)) = super::read_text(&path, cap.max(1)) else {
                return 0;
            };
            let bytes = text.as_bytes();
            let n = bytes.len().min(cap);
            let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return 0;
            };
            if mem.write(&mut caller, ptr as usize, &bytes[..n]).is_err() {
                return 0;
            }
            n as i32
        },
    )?;
    // render_svg(sptr, slen, optr, ocap) -> len : SVG → BMP 경로(실패 = 0)
    l.func_wrap(
        "env",
        "render_svg",
        |mut caller: Caller<'_, HostCtx>, sptr: i32, slen: i32, optr: i32, ocap: i32| -> i32 {
            let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return 0;
            };
            let mut svg = vec![0u8; (slen.max(0) as usize).min(READ_CAP)];
            if mem.read(&caller, sptr as usize, &mut svg).is_err() {
                return 0;
            }
            let Ok(svg) = String::from_utf8(svg) else {
                return 0;
            };
            let Some(out) = super::render_svg_impl(&svg) else {
                return 0;
            };
            let b = out.as_bytes();
            let n = b.len().min(ocap.max(0) as usize);
            if mem.write(&mut caller, optr as usize, &b[..n]).is_err() {
                return 0;
            }
            n as i32
        },
    )?;
    // is_dark() -> i32 : 테마 신호
    l.func_wrap("env", "is_dark", |_: Caller<'_, HostCtx>| -> i32 {
        i32::from(super::is_dark_now())
    })?;
    // disp_width(ptr, len) -> i32 : 표시 폭(CJK 2칸)
    l.func_wrap(
        "env",
        "disp_width",
        |caller: Caller<'_, HostCtx>, ptr: i32, len: i32| -> i32 {
            let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return 0;
            };
            let mut b = vec![0u8; (len.max(0) as usize).min(4096)];
            if mem.read(&caller, ptr as usize, &mut b).is_err() {
                return 0;
            }
            super::disp_width_impl(&String::from_utf8_lossy(&b)) as i32
        },
    )?;
    Ok(l)
}

/// 인스턴스 생성 + `fn_name() -> ptr` 호출 후 버퍼 회수(연료·메모리 상한 적용).
fn call_buf(plugin: &WasmPlugin, path: &Path, fn_name: &str) -> Result<String, String> {
    let ctx = HostCtx {
        path: path.to_path_buf(),
        limits: StoreLimitsBuilder::new().memory_size(MEM_CAP).build(),
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
    Ok(WasmPlugin {
        id,
        name,
        exts,
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

/// WASM 플러그인 → 공급자 어댑터 — 실행 오류는 해당 플러그인만 1줄 격리.
pub(super) struct WasmProvider {
    pub plugin: WasmPlugin,
}

impl PreviewProvider for WasmProvider {
    fn id(&self) -> &str {
        &self.plugin.id
    }
    fn exts(&self) -> &[String] {
        &self.plugin.exts
    }
    fn preview(&self, path: &Path) -> PreviewDoc {
        match run_preview(&self.plugin, path) {
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
  (func (export "nx_loop") (result i32) (loop br 0) (i32.const 0)))
"#;

    fn build(dir: &std::path::Path) {
        let bytes = wat::parse_str(WAT).unwrap();
        std::fs::write(dir.join("up.wasm"), bytes).unwrap();
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
        let _ = std::fs::remove_dir_all(&d);
    }
}
