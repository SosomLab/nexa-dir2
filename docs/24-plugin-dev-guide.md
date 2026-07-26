# 24 · 미리보기 플러그인 개발자 가이드 — WASM(wasmi) 판

> Nexa Dir **WASM 미리보기 플러그인**(ADR-0005 — 2026-07-26 Starlark에서 전환)을
> 독립 프로젝트로 만들고 배포하기까지의 전 과정. 참조 구현 =
> [`samples/markdown-viewer-wasm`](../samples/markdown-viewer-wasm/)(GitHub 렌더+Mermaid, 80KB).
>
> - 결정 배경: [25 ADR-0005](25-adr-0005-wasm-plugins.md)(왜 wasmi인가 — 크로스플랫폼 단일
>   아티팩트·격리·러스트 개발) · [09 ADR-0004](09-adr-0004-preview-plugins.md)(시임·매핑 설계)
> - 이전 Starlark 판 가이드: git 이력(`git log -- docs/24-plugin-dev-guide.md`)

## 0. 개념

```
파일 선택 ─▶ 확장자 결정 ─▶ 공급자 결정 ─▶ nx_preview() 실행 ─▶ lines/image
                              │                                   │
             ① 설정 preview_map(외부 Override)         하단 도크(축약 뷰)
             ② 플러그인 meta 선언(파일명 순)            독립 미리보기 창(F3/↗ — 기준 캔버스)
             ③ 내장 폴백(builtin.image → builtin.text)
```

- **플러그인 = `.wasm` 모듈 1개**(`data\plugins\` 복사 — 재빌드·설치 불요). 같은
  파일이 **전 OS/아키텍처에서 동일 동작**(크로스플랫폼 단일 아티팩트).
- 개발 언어 = **러스트 권장**(`wasm32-unknown-unknown` 타깃) — WASM으로 컴파일되는
  언어면 무엇이든 가능(C/Zig/TinyGo…).
- **격리**: fuel 2억(연료 소진 = 트랩)·선형 메모리 64MB·모듈 8MB·read 256KB —
  초과/오류는 해당 플러그인만 미리보기 1줄, 앱·타 플러그인 무영향. 파일 접근은
  **미리보기 대상 1개**로 제한(샌드박스).
- 로드: 미리보기 최초 사용 시 지연 로드(파일명 순 — 우선순위 필요하면 `00-` 접두),
  모듈은 검증·컴파일 캐시(호출마다 인스턴스만 새로). 수정 반영 = 앱 재시작.

## 1. 프로젝트 만들기

```
> xcopy /E samples\markdown-viewer-wasm my-viewer\
```

핵심 Cargo 설정(독립 크레이트): `crate-type = ["cdylib"]` · `[workspace]`(앱과 분리) ·
`profile.release` = `opt-level="z"`, `lto`, `panic="abort"`, `strip`.

## 2. ABI 계약 (필수 export 2개)

버퍼 규약 = **선두 4바이트 LE 길이 + UTF-8 본문** 포인터 반환(인스턴스는 호출당
1회라 leak 무해 — 샘플 `ret()` 참조).

```rust
#[no_mangle]
pub extern "C" fn nx_meta() -> *mut u8 {
    // "id\n표시명\next1,ext2"  — exts = 적용 확장자 기본값(설정 preview_map이 재정의)
    ret("my-viewer\nMy Viewer\nxyz,abc")
}

#[no_mangle]
pub extern "C" fn nx_preview() -> *mut u8 {
    // 첫 줄 "lines" | "image" + 본문. lines에는 표시 계약(§3) 태그 사용 가능.
    ret(&format!("lines\n{}", body))
}
```

`id`는 설정(`preview_map`·사용 여부)의 영구 키 — 개명 금지.

## 3. 호스트 API(import "env")와 표시 계약

| import | 시그니처 | 설명 |
| --- | --- | --- |
| `read_text` | `(ptr, cap) -> len` | **대상 파일** 앞부분을 게스트 메모리에 기록(256KB 클램프) |
| `render_svg` | `(sptr, slen, optr, ocap) -> len` | SVG(svg.rs 서브셋: rect/line/polyline/path/text)를 **GDI+ AA 이미지**로 래스터 → BMP 경로 기록. 실패 = 0 |
| `is_dark` | `() -> i32` | 테마 신호(다이어그램 색 선택) |

**lines 표시 계약**(독립 창 = GitHub 근사 렌더·도크 = 평문 축약):

| 접두 | 렌더 |
| --- | --- |
| `\u{2}h1\|`·`h2\|`·`h3\|` | 제목 — 굵게(+h1/h2 밑줄 괘선) |
| `\u{2}code\|` | 코드 블록 — 배경 밴드 + 모노 |
| `\u{2}q\|` | 인용 — 좌측 바 + 흐림 |
| `\u{2}mono\|` | 표·텍스트 아트 — 모노(정렬 보존) |
| `\u{2}hr\|` | 수평선 괘선 |
| `\u{1}img\|<경로>` + `\u{1}pad`×n | 인라인 이미지(n+1행 예약 — render_svg 산출 BMP 등) |
| (무접두) | 본문 — 프로포셔널 |

## 4. 빌드 → 로컬 테스트 → 배포

```
rustup target add wasm32-unknown-unknown                 # 1회
cargo build --release --target wasm32-unknown-unknown
copy target\wasm32-unknown-unknown\release\my_viewer.wasm  <NexaDir>\data\plugins\
```

앱 재시작 → 대상 파일 선택 → 도크 미리보기 / **F3**(독립 창). 오류는 미리보기에
`플러그인 오류(id): …` 1줄. **배포 = `.wasm` 1개**(설치형은
`%LOCALAPPDATA%\NexaDir\data\plugins\`). 사용 여부는 **설정 → 플러그인** 체크,
확장자 재지정은 `preview_map=xyz:my-viewer|…`.

## 5. 자동 테스트(저장소 개발자)

빌드 산출물을 `dist/`에 동봉하면 [`preview/sample_tests.rs`](../crates/nexa-app/src/preview/sample_tests.rs)
패턴으로 **실제 wasmi 런타임 E2E**를 돌릴 수 있다: `cargo test -p nexa-app preview::`.

## 6. 트러블슈팅

| 증상 | 원인/대응 |
| --- | --- |
| 목록에 안 나옴 | 재시작 안 함 / `nx_meta` 형식 오류(로드 오류로 제외) / 모듈 8MB 초과 |
| `플러그인 오류(id): fuel` | 연료 소진 — 입력 상한·조기 요약으로 설계(대상: 300ms급 작업) |
| 다이어그램이 텍스트로 | `render_svg` 실패(비Windows·SVG 서브셋 밖·2000px 초과) — 폴백 정상 |
| 다른 플러그인이 가로챔 | 파일명 순 우선 — `00-` 접두 또는 `preview_map` 고정 |

## 7. 관련 파일

| 역할 | 경로 |
| --- | --- |
| **참조 구현(시작 템플릿)** | `samples/markdown-viewer-wasm/` |
| 런타임(로더·ABI·격리) | `crates/nexa-app/src/preview/wasm.rs` |
| 시임·preview_map·사용 여부 | `crates/nexa-app/src/preview/mod.rs` |
| 독립 미리보기 창 | `crates/nexa-app/src/previewwnd.rs` |
| 결정 기록 | `docs/25-adr-0005-wasm-plugins.md` · `docs/10-decision-record.md` |
