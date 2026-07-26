//! 미리보기 공급자 시임(ADR-0004 S1 — X-2). 내장 공급자(builtin.text/image) +
//! 확장자→공급자 레지스트리. 우선순위(사용자 확정 07-26): **설정 `preview_map`
//! 오버라이드 > 플러그인/내장 선언 매치(로드 순) > 내장 텍스트 폴백**.
//!
//! **런타임 교체(ADR-0005 — 사용자 결정 07-26)**: Starlark 런타임(star.rs·
//! `starlark` crate·markdown.star)은 제거됨(이력 = git — revert로 원복 가능).
//! 후속 wasmi 런타임(`wasm.rs` 예정)이 같은 [`PreviewProvider`] 자리에 장착된다 —
//! 시임·독립 창·설정(preview_map/plugins_disabled)·격리 설계·라인 태그 계약
//! (`\u{2}종류|`·`\u{1}img|`)은 런타임 중립 자산으로 유지.

pub mod wasm;

use crate::i18n::tr;
use std::path::Path;

/// 공급자 산출물 — 도크/독립 창이 해석.
#[derive(Debug)]
pub enum PreviewDoc {
    /// 텍스트 라인들(종류 태그 `\u{2}`·이미지 마커 `\u{1}` 포함 가능).
    Lines(Vec<String>),
    /// 이미지 경로 — 호스트 WIC 렌더 위임(draw_image).
    Image(String),
}

/// 미리보기 공급자 계약(ADR-0004 S1) — 확장자 선언(`EXTS` 대응) + 생성.
pub trait PreviewProvider {
    /// 안정 식별자(설정 `preview_map`·사용 여부 키. 내장 = `builtin.*`).
    #[allow(dead_code)] // 설정 오버라이드·플러그인 페이지에서 사용(테스트 포함)
    fn id(&self) -> &str;
    /// 선언 확장자(소문자·점 없음) — 내부 기본값. 외부 재정의는 `preview_map`.
    fn exts(&self) -> &[String];
    fn preview(&self, path: &Path) -> PreviewDoc;
}

/// WIC가 인박스로 디코드하는 이미지 확장자(원본 docs/35 이미지 공급자 대응).
const IMAGE_EXTS: [&str; 9] = [
    "png", "jpg", "jpeg", "bmp", "gif", "ico", "tif", "tiff", "webp",
];

const TEXT_READ_CAP: usize = 16 * 1024;
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

thread_local! {
    /// 현재 테마 다크 여부(호스트 주입 — 다이어그램 색 선택. 07-26).
    static DARK: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

/// 테마 신호 주입(win.rs — 미리보기 전 호출).
pub fn set_dark(dark: bool) {
    DARK.with(|d| d.set(dark));
}

/// 현재 다크 여부(런타임 호스트 API용).
#[allow(dead_code)] // wasmi 런타임(ADR-0005) 장착 시 사용
pub(crate) fn is_dark_now() -> bool {
    DARK.with(|d| d.get())
}

/// 문자 표시 폭 합(콘솔 셀 — CJK/이모지 2칸. 표·다이어그램 정렬용 호스트 API).
#[allow(dead_code)] // wasmi 런타임(ADR-0005) 장착 시 사용
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

/// SVG(svg.rs 서브셋) → 임시 32bpp BMP 캐시(내용 해시 — GDI+ AA 래스터).
/// 런타임 호스트 API `render_svg`의 구현(07-26 Mermaid 이미지 렌더).
#[cfg(windows)]
#[allow(dead_code)] // wasmi 런타임(ADR-0005) 장착 시 사용
pub(crate) fn render_svg_impl(svg: &str) -> Option<String> {
    if svg.len() > 256 * 1024 {
        return None;
    }
    let doc = crate::svg::parse(svg)?;
    let (w, h, mut px) = unsafe { crate::ctl::gdipctx::svg_to_pixels(&doc)? };
    for p in px.chunks_exact_mut(4) {
        p[3] = 0xFF;
    }
    use std::hash::{Hash, Hasher};
    let mut hsh = std::collections::hash_map::DefaultHasher::new();
    svg.hash(&mut hsh);
    let dir = std::env::temp_dir().join("nexa-preview");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("d{:016x}.bmp", hsh.finish()));
    if !path.exists() {
        let mut f = Vec::with_capacity(54 + px.len());
        f.extend_from_slice(b"BM");
        f.extend_from_slice(&(54 + px.len() as u32).to_le_bytes());
        f.extend_from_slice(&[0u8; 4]);
        f.extend_from_slice(&54u32.to_le_bytes());
        f.extend_from_slice(&40u32.to_le_bytes());
        f.extend_from_slice(&w.to_le_bytes());
        f.extend_from_slice(&(-h).to_le_bytes()); // top-down
        f.extend_from_slice(&1u16.to_le_bytes());
        f.extend_from_slice(&32u16.to_le_bytes());
        f.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
        f.extend_from_slice(&(px.len() as u32).to_le_bytes());
        f.extend_from_slice(&[0u8; 16]);
        f.extend_from_slice(&px);
        std::fs::write(&path, f).ok()?;
    }
    Some(path.to_string_lossy().into_owned())
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub(crate) fn render_svg_impl(_svg: &str) -> Option<String> {
    None
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

/// 플러그인 사용 안 함 판정(설정 `plugins_disabled` — 내장은 폴백 안전망이라 면역).
fn is_disabled(id: &str, disabled: &str) -> bool {
    !id.starts_with("builtin.") && disabled.split('|').any(|d| d.trim() == id)
}

/// 공급자 결정 — `preview_map` 오버라이드 > 선언 매치(로드 순) > 텍스트 폴백.
fn resolve<'a>(
    providers: &'a [Box<dyn PreviewProvider>],
    ext: &str,
    preview_map: &str,
    disabled: &str,
) -> &'a dyn PreviewProvider {
    if !ext.is_empty() {
        for pair in preview_map.split('|') {
            if let Some((e, id)) = pair.split_once(':') {
                if e.trim().eq_ignore_ascii_case(ext) && !is_disabled(id.trim(), disabled) {
                    if let Some(p) = providers.iter().find(|p| p.id() == id.trim()) {
                        return p.as_ref();
                    }
                }
            }
        }
    }
    if let Some(p) = providers
        .iter()
        .filter(|p| !is_disabled(p.id(), disabled))
        .find(|p| p.exts().iter().any(|e| e == ext))
    {
        return p.as_ref();
    }
    providers
        .iter()
        .find(|p| p.id() == "builtin.text")
        .expect("builtin.text 폴백은 항상 등재")
        .as_ref()
}

/// 미리보기 생성(시임 진입점 — win.rs 호출).
pub fn preview_for(path: &Path, preview_map: &str, disabled: &str) -> PreviewDoc {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    with_providers(|providers, _| resolve(providers, &ext, preview_map, disabled).preview(path))
}

/// 설정 UI용 플러그인 메타(설정 창 "플러그인" 페이지 목록).
#[derive(Clone)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub exts: Vec<String>,
}

/// 로드된 플러그인 목록 — wasmi 런타임(ADR-0005) 장착 전까지 빈 목록
/// (설정 페이지는 설치 안내 표시).
pub fn plugin_infos() -> Vec<PluginInfo> {
    with_providers(|_, infos| infos.to_vec())
}

/// 현재 공급자 전체로 콜백 실행 — 미리보기 최초 사용 시 지연 구성(B1 영향 0).
/// wasmi 런타임의 `.wasm` 로더가 내장 앞에 합류할 예정(ADR-0005).
fn with_providers<R>(f: impl FnOnce(&[Box<dyn PreviewProvider>], &[PluginInfo]) -> R) -> R {
    type Cache = (Vec<Box<dyn PreviewProvider>>, Vec<PluginInfo>);
    thread_local! {
        static PROVIDERS: std::cell::OnceCell<Cache> = const { std::cell::OnceCell::new() };
    }
    PROVIDERS.with(|c| {
        let (providers, infos) = c.get_or_init(|| {
            let (plugins, _errors) = wasm::load_dir(&crate::config::data_dir().join("plugins"));
            let infos: Vec<PluginInfo> = plugins
                .iter()
                .map(|p| PluginInfo {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    exts: p.exts.clone(),
                })
                .collect();
            let mut v: Vec<Box<dyn PreviewProvider>> = plugins
                .into_iter()
                .map(|p| Box::new(wasm::WasmProvider { plugin: p }) as Box<dyn PreviewProvider>)
                .collect();
            v.extend(builtins());
            (v, infos)
        });
        f(providers, infos)
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
        match preview_for(&img, "", "") {
            PreviewDoc::Image(p) => assert!(p.ends_with("a.png")),
            _ => panic!("이미지 확장자는 이미지 공급자"),
        }
        let txt = tmp("a.rs", "fn main() {}\tok".as_bytes());
        match preview_for(&txt, "", "") {
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
        let img = tmp("b.png", &[0x89u8, 0x50, 0x00, 0x47]);
        match preview_for(&img, "png:builtin.text", "") {
            PreviewDoc::Lines(lines) => assert_eq!(lines.len(), 1, "이진 안내 1줄"),
            _ => panic!("오버라이드가 선언 매치보다 우선해야 함"),
        }
        match preview_for(&img, "png:no.such.plugin", "") {
            PreviewDoc::Image(_) => {}
            _ => panic!("무효 id는 무시 — 선언 매치 유지"),
        }
        let _ = std::fs::remove_file(img);
    }

    /// 테스트용 더미 플러그인 공급자(사용 여부 체크 시나리오 — resolve 직접 검증).
    struct Fake {
        id: &'static str,
        exts: Vec<String>,
    }
    impl PreviewProvider for Fake {
        fn id(&self) -> &str {
            self.id
        }
        fn exts(&self) -> &[String] {
            &self.exts
        }
        fn preview(&self, _path: &Path) -> PreviewDoc {
            PreviewDoc::Lines(vec![self.id.to_string()])
        }
    }

    #[test]
    fn disabled_plugin_is_skipped_but_builtin_immune() {
        let providers: Vec<Box<dyn PreviewProvider>> = {
            let mut v: Vec<Box<dyn PreviewProvider>> = vec![Box::new(Fake {
                id: "markdown",
                exts: vec!["md".into()],
            })];
            v.extend(builtins());
            v
        };
        assert_eq!(resolve(&providers, "md", "", "").id(), "markdown");
        assert_eq!(resolve(&providers, "md", "", "markdown").id(), "builtin.text");
        assert_eq!(
            resolve(&providers, "md", "md:markdown", "markdown").id(),
            "builtin.text"
        );
        assert_eq!(
            resolve(&providers, "png", "", "builtin.image|markdown").id(),
            "builtin.image"
        );
    }
}
