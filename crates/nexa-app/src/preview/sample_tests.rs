//! `samples/*-wasm` 참조 구현의 회귀 테스트 — 동봉된 빌드 산출물
//! (`dist/markdown.wasm`)을 실제 wasmi 런타임으로 로드·실행해 계약(ABI·태그·
//! Mermaid 이미지 마커)을 검증한다(가이드 24 "자동 테스트" 단계).
//! 압축 목록 샘플(`samples/archive-viewer-wasm` — ABI v2 X-46)도 같은 방식으로
//! `dist/archive.wasm`을 로드해 ISO/ar/cpio 목록 계약을 검증한다.
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

// ── 압축 목록 샘플 플러그인(ABI v2 — X-46) ──────────────────────────────

fn archive_sample_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/archive-viewer-wasm")
}

/// newc cpio 1건 조립(110B 16진 헤더 + 이름/데이터 4바이트 정렬).
fn cpio_member(name: &str, data: &[u8], mode: u32) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(b"070701");
    let f = |x: u64| format!("{x:08X}");
    for x in [
        1u64,             // ino
        mode as u64,      // mode
        0,
        0,
        1,                // nlink
        1_700_000_000,    // mtime
        data.len() as u64,
        0,
        0,
        0,
        0,
        name.len() as u64 + 1, // namesize(NUL 포함)
        0,
    ] {
        v.extend_from_slice(f(x).as_bytes());
    }
    v.extend_from_slice(name.as_bytes());
    v.push(0);
    while v.len() % 4 != 0 {
        v.push(0);
    }
    v.extend_from_slice(data);
    while v.len() % 4 != 0 {
        v.push(0);
    }
    v
}

/// ar 멤버 1건(60B ASCII 헤더 + 짝수 정렬 데이터).
fn ar_member(name: &str, data: &[u8]) -> Vec<u8> {
    let mut h = format!("{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}", name, 1_700_000_000u64, 0, 0, 100644, data.len());
    h.push('`');
    h.push('\n');
    let mut v = h.into_bytes();
    v.extend_from_slice(data);
    if v.len() % 2 != 0 {
        v.push(b'\n');
    }
    v
}

/// ISO 9660 디렉터리 레코드 1건.
fn iso_record(name: &[u8], extent: u32, size: u32, is_dir: bool) -> Vec<u8> {
    let mut r = vec![0u8; 33 + name.len() + usize::from(name.len().is_multiple_of(2))];
    r[0] = r.len() as u8;
    r[2..6].copy_from_slice(&extent.to_le_bytes());
    r[6..10].copy_from_slice(&extent.to_be_bytes());
    r[10..14].copy_from_slice(&size.to_le_bytes());
    r[14..18].copy_from_slice(&size.to_be_bytes());
    r[18..25].copy_from_slice(&[126, 8, 24, 9, 30, 0, 0]); // 2026-08-24 09:30 UTC
    r[25] = u8::from(is_dir) << 1;
    r[32] = name.len() as u8;
    r[33..33 + name.len()].copy_from_slice(name);
    r
}

/// 최소 ISO 9660 이미지(PVD → 루트 디렉터리 → 파일 1개).
fn iso_image() -> Vec<u8> {
    const S: usize = 2048;
    let mut img = vec![0u8; S * 21];
    // 16번 섹터 = 주 볼륨 기술자
    let pvd = &mut img[S * 16..S * 17];
    pvd[0] = 1;
    pvd[1..6].copy_from_slice(b"CD001");
    pvd[6] = 1;
    let root = iso_record(&[0], 18, S as u32, true);
    pvd[156..156 + root.len()].copy_from_slice(&root);
    // 17번 = 종료 기술자
    img[S * 17] = 255;
    img[S * 17 + 1..S * 17 + 6].copy_from_slice(b"CD001");
    // 18번 = 루트 디렉터리 내용(. · .. · 파일 1)
    let mut dir = Vec::new();
    dir.extend(iso_record(&[0], 18, S as u32, true));
    dir.extend(iso_record(&[1], 18, S as u32, true));
    dir.extend(iso_record(b"HELLO.TXT;1", 19, 5, false));
    img[S * 18..S * 18 + dir.len()].copy_from_slice(&dir);
    // 19번 = 파일 내용
    img[S * 19..S * 19 + 5].copy_from_slice(b"hello");
    img
}

#[test]
fn sample_archive_plugin_lists_iso_ar_and_cpio() {
    let d = std::env::temp_dir().join(format!("nexa_wasm_arc_sample_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::copy(
        archive_sample_root().join("dist/archive.wasm"),
        d.join("archive.wasm"),
    )
    .unwrap();
    let (plugins, errors) = super::wasm::load_dir(&d);
    assert!(errors.is_empty(), "{errors:?}");
    let p = &plugins[0];
    assert_eq!(p.id, "archive-sample");
    assert!(p.is_archive(), "nx_meta 4번째 줄 = archive 능력 선언");
    assert!(p.exts.contains(&"iso".to_string()) && p.exts.contains(&"cpio".to_string()));

    // cpio(newc) — 파일 1 + 폴더 1
    let mut cpio = cpio_member("dir", &[], 0o040755);
    cpio.extend(cpio_member("dir/a.txt", b"hello", 0o100644));
    cpio.extend(cpio_member("TRAILER!!!", &[], 0));
    let f = d.join("t.cpio");
    std::fs::write(&f, &cpio).unwrap();
    let doc = super::wasm::run_archive(p, &f).unwrap();
    assert!(doc.is_ok(), "{:?}", doc.status);
    assert_eq!(doc.listing.label, "cpio (newc)");
    let a = doc
        .listing
        .entries
        .iter()
        .find(|e| e.path == "dir/a.txt")
        .unwrap();
    assert_eq!((a.size, a.modified, a.time_is_local), (Some(5), Some(1_700_000_000), false));
    assert!(doc.listing.entries.iter().any(|e| e.path == "dir" && e.is_dir));

    // ar
    let mut ar = b"!<arch>\n".to_vec();
    ar.extend(ar_member("hello.o/", b"OBJ"));
    let f = d.join("t.a");
    std::fs::write(&f, &ar).unwrap();
    let doc = super::wasm::run_archive(p, &f).unwrap();
    assert_eq!(doc.listing.label, "ar");
    assert_eq!(doc.listing.entries[0].path, "hello.o");
    assert_eq!(doc.listing.entries[0].size, Some(3));

    // ISO 9660
    let f = d.join("t.iso");
    std::fs::write(&f, iso_image()).unwrap();
    let doc = super::wasm::run_archive(p, &f).unwrap();
    assert_eq!(doc.listing.label, "ISO 9660");
    let h = doc
        .listing
        .entries
        .iter()
        .find(|e| e.path == "HELLO.TXT")
        .expect("버전 접미(;1) 제거 후 파일 1건");
    assert_eq!(h.size, Some(5));
    assert_eq!(
        h.modified,
        Some(nexa_vfs::archive::ymd_hms_to_unix(2026, 8, 24, 9, 30, 0))
    );

    // 지원하지 않는 형식 = 오류 상태로 격리(앱은 안내 1줄)
    let f = d.join("t.iso.bad");
    std::fs::write(&f, b"garbage").unwrap();
    let doc = super::wasm::run_archive(p, &f).unwrap();
    assert!(!doc.is_ok(), "미지원 형식은 실패 상태");

    let _ = std::fs::remove_dir_all(&d);
}
