# MarkdownViewerPlugin(wasm) — 미리보기 플러그인 독립 프로젝트 샘플

Nexa Dir **WASM 미리보기 플러그인**(ADR-0005)의 참조 구현이자 시작 템플릿.
GitHub 근사 렌더(제목/코드/인용 태그·표 박스 드로잉·`<br/>`)와 **Mermaid**
(flowchart = 호스트 GDI+ SVG 이미지·sequence = 텍스트 아트)를 지원한다.
전 과정 가이드 → [docs/24 플러그인 개발 가이드](../../docs/24-plugin-dev-guide.md).

## 구조

```
markdown-viewer-wasm/
├── Cargo.toml        # 독립 크레이트(cdylib·[workspace] 분리·opt-level="z")
├── src/lib.rs        # ABI(nx_meta/nx_preview) + 마크다운 렌더
├── src/mm.rs         # Mermaid(flowchart SVG·sequence 아트)
├── fixtures/sample.md
└── dist/markdown.wasm  # 빌드 산출물(동봉 — E2E 테스트가 로드)
```

## 빌드 → 배포

```
rustup target add wasm32-unknown-unknown       # 1회
cargo build --release --target wasm32-unknown-unknown
copy target\wasm32-unknown-unknown\release\markdown_viewer.wasm  <NexaDir>\data\plugins\
```

저장소에서는 `pwsh scripts/build-plugins.ps1`로 **두 동봉 플러그인**(이 플러그인 +
`archive-viewer-wasm`)을 한 번에 빌드하고 각 `dist/`까지 갱신한다 — 절차 SSOT =
[18 §3-1](../../docs/18-build-and-test.md), 배포 형태 = [21 §5-2](../../docs/21-distribution.md).

앱 재시작 = 재로드. **`.wasm` 1개가 전 OS/아키텍처에서 동일 동작**(크로스플랫폼
단일 아티팩트 — 이 샘플 80KB). 산출물 갱신 시 `dist/markdown.wasm`도 함께 교체
(저장소 E2E: `cargo test -p nexa-app preview::`).

## 계약 요약 (ADR-0005 ABI)

- export: `memory` · `nx_meta()`/`nx_preview()` → 선두 4바이트 LE 길이 + UTF-8.
  meta = `id\nname\next1,ext2` · preview = 첫 줄 `lines`|`image` + 본문
  (라인 태그 `\u{2}h1|…hr|`·이미지 마커 `\u{1}img|경로`+`\u{1}pad`).
- import(env): `read_text(ptr,cap)`(**대상 파일만**·256KB) ·
  `render_svg(sptr,slen,optr,ocap)`(svg.rs 서브셋 → BMP 경로) · `is_dark()`.
- 격리: fuel 2억·메모리 64MB·모듈 8MB — 초과 = 해당 플러그인만 오류 1줄.
