//! Starlark 미리보기 플러그인 런타임(ADR-0004 S2+S3 — X-2, 사용자 지시 07-26).
//! `data\plugins\*.star`를 파일명 순으로 로드(결정적) — 각 파일 = 플러그인 1개.
//! 스크립트는 메타(`ID`/`NAME`/`EXTS` — **적용 대상 기본값**)와 `preview(file)`를
//! 정의하고, 반환 `{"lines": [str]}` 또는 `{"image": path}`를 호스트가 해석한다.
//! 샌드박스: Starlark는 기본적으로 I/O가 없고, 호스트가 준 API만 접근 가능 —
//! `read_text(n)`은 **현재 미리보기 대상 파일만** 읽는다(임의 경로 불가).
//! 오류 격리: 로드/실행 실패는 해당 플러그인만 비활성·오류 1줄(앱 무영향).

use starlark::any::ProvidesStaticType;
use starlark::environment::{FrozenModule, Globals, GlobalsBuilder, Module};
use starlark::eval::Evaluator;
use starlark::starlark_module;
use starlark::syntax::{AstModule, Dialect};
use starlark::values::dict::DictRef;
use starlark::values::list::ListRef;
use starlark::values::structs::AllocStruct;
use starlark::values::Value;
use std::path::{Path, PathBuf};

use super::PreviewDoc;

/// 플러그인 read_text 상한(호스트 강제 — 스크립트 요청과 무관한 안전 상한).
const READ_CAP: usize = 256 * 1024;
/// 반환 lines 상한(도크/독립 창 보호).
const LINES_CAP: usize = 1000;
/// 라인당 문자 상한.
const LINE_LEN_CAP: usize = 4096;

/// 로드된 플러그인 — 평가 1회 후 동결(FrozenModule = Send+Sync, 호출마다 재파싱 없음).
pub struct StarPlugin {
    pub id: String,
    pub name: String,
    /// 스크립트 내부 선언(EXTS) = 적용 대상 **기본값**. 외부 재정의는 settings
    /// `preview_map`(mod.rs resolve 1순위).
    pub exts: Vec<String>,
    pub source: PathBuf,
    frozen: FrozenModule,
}

/// 미리보기 실행 문맥 — 호스트 API(read_text)가 접근(eval.extra 주입).
#[derive(ProvidesStaticType)]
struct PreviewCtx {
    path: PathBuf,
}

#[starlark_module]
fn host_globals(builder: &mut GlobalsBuilder) {
    /// 미리보기 **대상 파일**의 앞 `n`바이트를 텍스트로 읽는다(UTF-8 lossy·
    /// 호스트 상한 클램프). 대상 외 경로 접근 불가(샌드박스 표면).
    fn read_text(n: i32, eval: &mut Evaluator) -> anyhow::Result<String> {
        let ctx = eval
            .extra
            .and_then(|e| e.downcast_ref::<PreviewCtx>())
            .ok_or_else(|| anyhow::anyhow!("read_text: 미리보기 문맥 없음"))?;
        let cap = (n.max(0) as usize).min(READ_CAP);
        let (text, _) = super::read_text(&ctx.path, cap.max(1))
            .map_err(|_| anyhow::anyhow!("read_text: 읽기 실패"))?;
        Ok(text)
    }

    /// 문자열 표시 폭(콘솔 셀 기준 — CJK/이모지 = 2칸). 표·다이어그램 정렬용
    /// 순수 헬퍼(Starlark엔 ord()가 없어 호스트가 제공).
    fn disp_width(s: &str) -> anyhow::Result<i32> {
        Ok(disp_width_impl(s) as i32)
    }
}

/// 표시 폭 구현 — 동아시아 Wide/전각·이모지 = 2(터미널 관례).
pub(crate) fn disp_width_impl(s: &str) -> usize {
    s.chars()
        .map(|c| {
            let u = c as u32;
            if matches!(
                u,
                0x1100..=0x115F | 0x2E80..=0x303E | 0x3041..=0x33FF | 0x3400..=0x4DBF
                    | 0x4E00..=0x9FFF | 0xA000..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF
                    | 0xFE30..=0xFE4F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6
                    | 0x1F300..=0x1FAFF | 0x20000..=0x3FFFD
            ) {
                2
            } else {
                1
            }
        })
        .sum()
}

fn globals() -> &'static Globals {
    static G: std::sync::OnceLock<Globals> = std::sync::OnceLock::new();
    G.get_or_init(|| GlobalsBuilder::standard().with(host_globals).build())
}

/// `.star` 1개 로드 — 파스 → 평가(모듈 톱레벨) → 동결 → 메타 추출.
fn load_one(path: &Path) -> Result<StarPlugin, String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("읽기 실패: {e}"))?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let ast = AstModule::parse(&name, src, &Dialect::Extended).map_err(|e| e.to_string())?;
    // 0.14 heap 스코프 API — 모듈은 임시 힙에서 평가 후 동결(FrozenModule = 'static).
    let frozen = Module::with_temp_heap(|module| -> Result<FrozenModule, String> {
        {
            let mut eval = Evaluator::new(&module);
            eval.eval_module(ast, globals()).map_err(|e| e.to_string())?;
        }
        module.freeze().map_err(|e| format!("freeze: {e:?}"))
    })?;
    let get_str = |key: &str| -> Result<String, String> {
        let v = frozen.get(key).map_err(|_| format!("{key} 선언 없음"))?;
        v.value()
            .unpack_str()
            .map(str::to_string)
            .ok_or_else(|| format!("{key}는 str이어야 함"))
    };
    let id = get_str("ID")?;
    let plugin_name = get_str("NAME").unwrap_or_else(|_| id.clone());
    let exts_v = frozen.get("EXTS").map_err(|_| "EXTS 선언 없음".to_string())?;
    let mut exts = Vec::new();
    {
        let exts_val = exts_v.value();
        let exts_list = ListRef::from_value(exts_val)
            .ok_or_else(|| "EXTS는 list[str]이어야 함".to_string())?;
        for e in exts_list.iter() {
            if let Some(s) = e.unpack_str() {
                exts.push(s.trim_start_matches('.').to_ascii_lowercase());
            }
        }
    }
    frozen
        .get("preview")
        .map_err(|_| "preview(file) 함수 없음".to_string())?;
    Ok(StarPlugin {
        id,
        name: plugin_name,
        exts,
        source: path.to_path_buf(),
        frozen,
    })
}

/// 디렉터리의 `*.star` 전부 로드(파일명 순) — (플러그인들, 오류 목록).
/// 오류는 해당 파일만 건너뜀(격리).
pub fn load_dir(dir: &Path) -> (Vec<StarPlugin>, Vec<String>) {
    let mut plugins = Vec::new();
    let mut errors = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return (plugins, errors); // 디렉터리 없음 = 플러그인 0(정상)
    };
    let mut files: Vec<PathBuf> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("star"))
        })
        .collect();
    files.sort(); // 로드 순 = 파일명 순(결정적 — 선언 매치 우선순위)
    for f in files {
        match load_one(&f) {
            Ok(p) => plugins.push(p),
            Err(e) => errors.push(format!("{}: {e}", f.file_name().unwrap_or_default().to_string_lossy())),
        }
    }
    (plugins, errors)
}

/// `preview(file)` 실행 — `file` = struct(path/ext/size 속성, ADR-0004 호스트 API).
/// 반환 dict 해석: `{"lines": [str]}` | `{"image": str}`. 호출마다 임시 힙(잔류 0 — B1).
pub fn run_preview(plugin: &StarPlugin, path: &Path) -> Result<PreviewDoc, String> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let owned = plugin.frozen.get("preview").map_err(|e| e.to_string())?;
    let ctx = PreviewCtx {
        path: path.to_path_buf(),
    };
    Module::with_temp_heap(|module| -> Result<PreviewDoc, String> {
        // 플러그인 함수 값을 이 모듈 동결 힙에 참조 등록 — 값 수명 = 모듈 힙 수명.
        // SAFETY: 반환 FrozenValue는 이 클로저(모듈 스코프) 안에서만 사용한다.
        let preview_fn = unsafe { owned.owned_frozen_value(module.frozen_heap()).to_value() };
        let heap = module.heap();
        let file = heap.alloc(AllocStruct([
            ("path", heap.alloc(path.to_string_lossy().into_owned())),
            ("ext", heap.alloc(ext.clone())),
            ("size", heap.alloc(size.min(i32::MAX as u64) as i32)),
        ]));
        let mut eval = Evaluator::new(&module);
        eval.extra = Some(&ctx);
        let res = eval
            .eval_function(preview_fn, &[file], &[])
            .map_err(|e| e.to_string())?;
        parse_result(res)
    })
}

/// 플러그인 반환값 → PreviewDoc(상한 강제 — 도크/독립 창 보호).
fn parse_result(res: Value) -> Result<PreviewDoc, String> {
    let dict = DictRef::from_value(res).ok_or("preview()는 dict를 반환해야 함")?;
    for (k, v) in dict.iter() {
        match k.unpack_str() {
            Some("lines") => {
                let list = ListRef::from_value(v).ok_or("lines는 list[str]이어야 함")?;
                let mut lines = Vec::new();
                for item in list.iter().take(LINES_CAP) {
                    let s = item
                        .unpack_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| item.to_string());
                    lines.push(if s.chars().count() > LINE_LEN_CAP {
                        s.chars().take(LINE_LEN_CAP).collect()
                    } else {
                        s
                    });
                }
                return Ok(PreviewDoc::Lines(lines));
            }
            Some("image") => {
                let s = v.unpack_str().ok_or("image는 str 경로여야 함")?;
                return Ok(PreviewDoc::Image(s.to_string()));
            }
            _ => {}
        }
    }
    Err("반환 dict에 lines/image 키 없음".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nexa_star_{}_{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn loads_meta_runs_preview_and_isolates_errors() {
        let d = tmp_dir("basic");
        // 정상 플러그인 — 메타 + read_text/disp_width 호스트 API 사용
        std::fs::write(
            d.join("upper.star"),
            r#"
ID = "upper"
NAME = "Upper Viewer"
EXTS = [".AbC"]

def preview(file):
    t = read_text(64)
    return {"lines": [file.ext, str(file.size), str(disp_width("한a")), t.upper()]}
"#,
        )
        .unwrap();
        // 깨진 플러그인 — 문법 오류(격리 확인)
        std::fs::write(d.join("broken.star"), "def preview(:\n").unwrap();
        let (plugins, errors) = load_dir(&d);
        assert_eq!(plugins.len(), 1, "정상 1개만 로드");
        assert_eq!(errors.len(), 1, "깨진 파일은 오류 격리");
        let p = &plugins[0];
        assert_eq!((p.id.as_str(), p.name.as_str()), ("upper", "Upper Viewer"));
        assert_eq!(p.exts, ["abc"], "점 제거·소문자 정규화");
        // 실행 — 대상 파일만 read_text 가능(문맥 주입)
        let target = d.join("t.abc");
        std::fs::write(&target, "hi 한글").unwrap();
        match run_preview(p, &target).unwrap() {
            PreviewDoc::Lines(lines) => {
                assert_eq!(lines[0], "abc");
                assert_eq!(lines[2], "3", "disp_width(한a) = 2+1");
                assert_eq!(lines[3], "HI 한글");
            }
            _ => panic!("lines 반환"),
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn runtime_error_reports_not_panics() {
        let d = tmp_dir("err");
        std::fs::write(
            d.join("boom.star"),
            "ID = \"boom\"\nNAME = \"Boom\"\nEXTS = [\"x\"]\ndef preview(file):\n    fail(\"boom!\")\n",
        )
        .unwrap();
        let (plugins, errors) = load_dir(&d);
        assert!(errors.is_empty());
        let target = d.join("t.x");
        std::fs::write(&target, "z").unwrap();
        let err = run_preview(&plugins[0], &target).unwrap_err();
        assert!(err.contains("boom"), "실행 오류 문자열 전달: {err}");
        let _ = std::fs::remove_dir_all(&d);
    }
}
