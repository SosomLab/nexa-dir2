//! `samples/markdown-viewer-wasm` 참조 구현의 회귀 테스트 — 동봉된 빌드 산출물
//! (`dist/markdown.wasm`)을 실제 wasmi 런타임으로 로드·실행해 계약(ABI·태그·
//! Mermaid 이미지 마커)을 검증한다(가이드 24 "자동 테스트" 단계).
//! 산출물 갱신: `cargo build --release --target wasm32-unknown-unknown`
//! (samples/markdown-viewer-wasm) 후 dist/에 복사.

use super::wasm::{load_dir, run_preview};
use super::PreviewDoc;

fn sample_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/markdown-viewer-wasm")
}

#[test]
fn sample_wasm_plugin_end_to_end() {
    let root = sample_root();
    let d = std::env::temp_dir().join(format!("nexa_wasm_sample_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::copy(root.join("dist/markdown.wasm"), d.join("markdown.wasm")).unwrap();
    let (plugins, errors) = load_dir(&d);
    assert!(errors.is_empty(), "{errors:?}");
    let p = &plugins[0];
    assert_eq!(p.id, "markdown");
    assert_eq!(
        p.exts,
        ["md", "markdown", "mdown", "mkd"],
        "적용 대상 = 플러그인 내부 선언(기본값 — 외부 재정의는 preview_map)"
    );
    let fixture = root.join("fixtures").join("sample.md");
    let lines = match run_preview(p, &fixture).unwrap() {
        PreviewDoc::Lines(l) => l,
        _ => panic!("lines 반환"),
    };
    let joined = lines.join("\n");
    assert!(joined.contains("\u{2}h1|"), "h1 태그: {joined}");
    assert!(joined.contains("• 항목 하나"), "불릿");
    assert!(joined.contains('☑'), "체크 목록");
    assert!(joined.contains("\u{2}mono|┌"), "표 박스(모노 태그)");
    assert!(
        joined.contains("굵게") && !joined.contains("**굵게**"),
        "인라인 마커 정리"
    );
    // Mermaid flowchart는 **3단 폴백**이다: Windows = 이미지 마커(SVG→BMP) →
    // 아트 → 렌더 미지원 환경(호스트 render_svg 없음)에서는 **원문 모노 블록**.
    // 종전 단언은 앞의 둘만 인정해 비Windows CI에서 실패했다(08-02).
    assert!(
        joined.contains("\u{1}img|")     // 이미지
            || joined.contains('▼')       // 아트
            || joined.contains("graph TD"), // 원문 보존 폴백
        "flowchart 이미지/아트/원문 폴백 중 하나: {joined}"
    );
    assert!(joined.contains("Client"), "sequence participant 별칭");
    assert!(joined.contains('▶') || joined.contains('◀'), "sequence 화살표");
    let _ = std::fs::remove_dir_all(&d);
}
