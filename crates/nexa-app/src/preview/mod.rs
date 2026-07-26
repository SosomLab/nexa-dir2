//! 미리보기 공급자 시임(ADR-0004 S1 — X-2). M4-2 내장 미리보기를
//! [`PreviewProvider`] 계약 뒤로 배치(내장 = `builtin.text`/`builtin.image`) +
//! 확장자→공급자 레지스트리. 우선순위(ADR-0004 §확장자 매핑, 사용자 확정 07-26):
//! **설정 `preview_map` 오버라이드 > 플러그인/내장 선언 매치(로드 순) > 내장 텍스트 폴백**.
//! Starlark 플러그인(S2)은 [`star`] 모듈이 이 레지스트리 앞단에 이어진다.
//! 원본 대응: `../nexa-dir/docs/35-preview-system.md`(공급자 모델).

pub mod star;

use crate::i18n::{tr, trf};
use std::path::Path;

/// 공급자 산출물 — 도크/독립 창이 해석(ADR-0004 반환 규약 `lines`/`image` 대응.
/// `kv`는 후속 — 표 렌더로 수렴 예정).
#[derive(Debug)]
pub enum PreviewDoc {
    /// 텍스트 라인들 — 도크·독립 창(모노 그리드) 공통.
    Lines(Vec<String>),
    /// 이미지 경로 — 호스트 WIC 렌더 위임(draw_image).
    Image(String),
}

/// 미리보기 공급자 계약(ADR-0004 S1) — 확장자 선언(`EXTS` 대응) + 생성.
pub trait PreviewProvider {
    /// 안정 식별자(설정 `preview_map`·공급자 표기 키. 내장 = `builtin.*`).
    fn id(&self) -> &str;
    /// 선언 확장자(소문자·점 없음) — **스크립트/공급자 내부 기본값**.
    /// 빈 슬라이스 = 선언 매치 없음(폴백 전용). 외부 재정의는 `preview_map`.
    fn exts(&self) -> &[String];
    fn preview(&self, path: &Path) -> PreviewDoc;
}

/// WIC가 인박스로 디코드하는 이미지 확장자(원본 docs/35 이미지 공급자 대응).
// webp(G-12) = OS WIC 확장 코덱 의존 — 미설치면 디코드 실패로 텍스트/이진 판정 폴백(무해)
const IMAGE_EXTS: [&str; 9] = [
    "png", "jpg", "jpeg", "bmp", "gif", "ico", "tif", "tiff", "webp",
];

/// 텍스트 읽기 상한(M4-2 — 대용량 안전). 첫 1KB NUL = 이진 판정.
const TEXT_READ_CAP: usize = 16 * 1024;
/// 도크 표시 라인 상한(높이 초과분은 그리지 않음 — 여유 상한).
const TEXT_LINE_CAP: usize = 200;

/// 상한까지 읽어 (내용, 이진 여부) — 공급자·호스트 API 공용. `Err` = 열기/읽기 실패.
pub(crate) fn read_text(path: &Path, cap: usize) -> Result<(String, bool), ()> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).map_err(|_| ())?;
    let mut buf = vec![0u8; cap];
    let n = f.read(&mut buf).unwrap_or(0);
    buf.truncate(n);
    let binary = buf[..n.min(1024)].contains(&0);
    Ok((String::from_utf8_lossy(&buf).into_owned(), binary))
}

/// 내장 텍스트 공급자(M4-2 이관) — 첫 16KB·이진 판정·200줄·탭 4칸.
struct BuiltinText {
    exts: Vec<String>,
}

impl PreviewProvider for BuiltinText {
    fn id(&self) -> &str {
        "builtin.text"
    }
    fn exts(&self) -> &[String] {
        &self.exts // 빈 목록 = 폴백 전용
    }
    fn preview(&self, path: &Path) -> PreviewDoc {
        let Ok((text, binary)) = read_text(path, TEXT_READ_CAP) else {
            return PreviewDoc::Lines(vec![tr("preview.fail")]);
        };
        if text.is_empty() {
            return PreviewDoc::Lines(vec![tr("preview.empty")]);
        }
        if binary {
            return PreviewDoc::Lines(vec![tr("preview.binary")]);
        }
        PreviewDoc::Lines(
            text.lines()
                .take(TEXT_LINE_CAP)
                .map(|l| l.replace('\t', "    "))
                .collect(),
        )
    }
}

/// 내장 이미지 공급자(M4-2 이관) — WIC 디코드는 백엔드 소관, 경로만 위임.
struct BuiltinImage {
    exts: Vec<String>,
}

impl PreviewProvider for BuiltinImage {
    fn id(&self) -> &str {
        "builtin.image"
    }
    fn exts(&self) -> &[String] {
        &self.exts
    }
    fn preview(&self, path: &Path) -> PreviewDoc {
        PreviewDoc::Image(path.to_string_lossy().into_owned())
    }
}

/// 내장 공급자 목록(로드 순 = 선언 매치 우선순위. 텍스트는 마지막 폴백 전용).
fn builtins() -> Vec<Box<dyn PreviewProvider>> {
    vec![
        Box::new(BuiltinImage {
            exts: IMAGE_EXTS.iter().map(|s| s.to_string()).collect(),
        }),
        Box::new(BuiltinText { exts: Vec::new() }),
    ]
}

/// Starlark 플러그인 → 공급자 어댑터(S2) — 실행 오류는 해당 플러그인만
/// 오류 1줄로 격리(ADR-0004 §격리).
struct StarProvider {
    plugin: star::StarPlugin,
}

impl PreviewProvider for StarProvider {
    fn id(&self) -> &str {
        &self.plugin.id
    }
    fn exts(&self) -> &[String] {
        &self.plugin.exts
    }
    fn preview(&self, path: &Path) -> PreviewDoc {
        match star::run_preview(&self.plugin, path) {
            Ok(doc) => doc,
            Err(e) => PreviewDoc::Lines(vec![trf("preview.plugin.error", &[&self.plugin.id, &e])]),
        }
    }
}

/// 공급자 결정 — `preview_map`(설정 오버라이드 `ext:id|…`) > 선언 매치(로드 순) >
/// 텍스트 폴백. `providers` = 전체 후보(플러그인이 내장보다 앞 — S2에서 합류).
fn resolve<'a>(
    providers: &'a [Box<dyn PreviewProvider>],
    ext: &str,
    preview_map: &str,
) -> &'a dyn PreviewProvider {
    // 1) 설정 오버라이드: "md:markdown|jpg:builtin.image" — id 매치 실패는 무시(안전)
    if !ext.is_empty() {
        for pair in preview_map.split('|') {
            if let Some((e, id)) = pair.split_once(':') {
                if e.trim().eq_ignore_ascii_case(ext) {
                    if let Some(p) = providers.iter().find(|p| p.id() == id.trim()) {
                        return p.as_ref();
                    }
                }
            }
        }
    }
    // 2) 선언 매치(로드 순 — 스크립트/공급자 내부 EXTS 기본값)
    if let Some(p) = providers
        .iter()
        .find(|p| p.exts().iter().any(|e| e == ext))
    {
        return p.as_ref();
    }
    // 3) 내장 텍스트 폴백(항상 존재)
    providers
        .iter()
        .find(|p| p.id() == "builtin.text")
        .expect("builtin.text 폴백은 항상 등재")
        .as_ref()
}

/// 미리보기 생성(시임 진입점 — win.rs 호출). `preview_map` = 설정 원문(파싱은 여기서).
pub fn preview_for(path: &Path, preview_map: &str) -> PreviewDoc {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    with_providers(|providers| resolve(providers, &ext, preview_map).preview(path))
}

/// 현재 공급자 전체로 콜백 실행 — **플러그인(`data\plugins\*.star`, 파일명 순)이
/// 내장보다 앞**(같은 확장자 선언 시 플러그인 우선). 미리보기 최초 사용 시 지연
/// 로드(B1 상주 영향 0)·이후 캐시(재로드 = 앱 재시작).
fn with_providers<R>(f: impl FnOnce(&[Box<dyn PreviewProvider>]) -> R) -> R {
    thread_local! {
        /// UI 스레드 전용 캐시(공급자 Value는 Send 아님 — 도크/독립 창 모두 UI 스레드).
        static PROVIDERS: std::cell::OnceCell<Vec<Box<dyn PreviewProvider>>> =
            const { std::cell::OnceCell::new() };
    }
    PROVIDERS.with(|c| {
        f(c.get_or_init(|| {
            let (plugins, _errors) = star::load_dir(&crate::config::data_dir().join("plugins"));
            let mut v: Vec<Box<dyn PreviewProvider>> = plugins
                .into_iter()
                .map(|p| Box::new(StarProvider { plugin: p }) as Box<dyn PreviewProvider>)
                .collect();
            v.extend(builtins());
            v
        }))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("nexa_prev_{}_{}", std::process::id(), name));
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn declared_ext_routes_and_text_falls_back() {
        let img = tmp("a.png", b"\x89PNG");
        match preview_for(&img, "") {
            PreviewDoc::Image(p) => assert!(p.ends_with("a.png")),
            _ => panic!("이미지 확장자는 이미지 공급자"),
        }
        let txt = tmp("a.rs", "fn main() {}\tok".as_bytes());
        match preview_for(&txt, "") {
            PreviewDoc::Lines(lines) => {
                assert_eq!(lines[0], "fn main() {}    ok", "탭 4칸 치환 유지")
            }
            _ => panic!("비매치 확장자는 텍스트 폴백"),
        }
        for p in [img, txt] {
            let _ = std::fs::remove_file(p);
        }
    }

    #[test]
    fn preview_map_overrides_declared_match() {
        // 설정 오버라이드 = 스크립트 내부 EXTS 선언보다 우선(사용자 확정 07-26).
        // png를 builtin.text로 강제 → 이미지 공급자 대신 텍스트(이진 판정) 경로.
        let img = tmp("b.png", &[0x89u8, 0x50, 0x00, 0x47]);
        match preview_for(&img, "png:builtin.text") {
            PreviewDoc::Lines(lines) => assert_eq!(lines.len(), 1, "이진 안내 1줄"),
            _ => panic!("오버라이드가 선언 매치보다 우선해야 함"),
        }
        // 존재하지 않는 id 오버라이드 = 무시하고 선언 매치로(안전)
        match preview_for(&img, "png:no.such.plugin") {
            PreviewDoc::Image(_) => {}
            _ => panic!("무효 id는 무시 — 선언 매치 유지"),
        }
        let _ = std::fs::remove_file(img);
    }
}
