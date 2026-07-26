//! `samples/markdown-viewer` 독립 프로젝트 샘플의 회귀 테스트 —
//! 실제 Starlark 런타임으로 로드·실행해 플러그인 계약(메타·호스트 API·렌더)을
//! 검증한다(개발자 가이드 docs/24 §샘플의 "자동 테스트" 단계).

use super::star::{load_dir, run_preview};
use super::PreviewDoc;

fn sample_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/markdown-viewer")
}

#[test]
fn sample_markdown_viewer_plugin_end_to_end() {
    let root = sample_root();
    let d = std::env::temp_dir().join(format!("nexa_star_sample_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::copy(root.join("markdown.star"), d.join("markdown.star")).unwrap();
    let (plugins, errors) = load_dir(&d);
    assert!(errors.is_empty(), "{errors:?}");
    let p = &plugins[0];
    assert_eq!(p.id, "markdown");
    assert_eq!(
        p.exts,
        ["md", "markdown", "mdown", "mkd"],
        "적용 대상 = 스크립트 내부 선언(기본값 — 외부 재정의는 preview_map)"
    );
    let fixture = root.join("fixtures").join("sample.md");
    let lines = match run_preview(p, &fixture).unwrap() {
        PreviewDoc::Lines(l) => l,
        _ => panic!("lines 반환"),
    };
    let joined = lines.join("\n");
    assert!(joined.contains("\u{2}h1|"), "h1 종류 태그: {joined}");
    assert!(joined.contains("• 항목 하나"), "불릿");
    assert!(joined.contains("☑"), "체크 목록");
    assert!(joined.contains("┌─"), "표/코드 상자");
    assert!(
        joined.contains("굵게") && !joined.contains("**굵게**"),
        "인라인 마커 정리"
    );
    // 표 CJK 정렬 — 경계 '│' 표시 폭 일치(disp_width 호스트 API 사용 검증)
    let table: Vec<&String> = lines.iter().filter(|l| l.contains('│')).collect();
    assert!(table.len() >= 3, "표 행: {joined}");
    // Mermaid — flowchart: Windows = 이미지 마커(SVG→BMP 래스터·07-26),
    // 비지원 플랫폼 = 텍스트 아트 폴백. sequence = 텍스트 아트(별칭·화살표).
    assert!(
        joined.contains('\u{1}') || joined.contains('▼'),
        "flowchart 이미지 마커 또는 텍스트 아트: {joined}"
    );
    assert!(joined.contains("Client"), "participant as 별칭");
    assert!(joined.contains('▶') || joined.contains('◀'), "sequence 화살표");
    let _ = std::fs::remove_dir_all(&d);
}
