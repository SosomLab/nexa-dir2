# 24 · 미리보기 플러그인 개발자 가이드 — WASM(wasmi) 판

> Nexa Dir **WASM 미리보기 플러그인**(ADR-0005 — 2026-07-26 Starlark에서 전환)을
> 독립 프로젝트로 만들고 배포하기까지의 전 과정. 참조 구현 =
> [`samples/markdown-viewer-wasm`](../samples/markdown-viewer-wasm/)(GitHub 렌더+Mermaid, 80KB).
>
> - 압축 목록 플러그인(ABI v2 — X-46): **§3-1** · 설계 = [28](28-archive-preview.md)
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

두 가지 출발점 중 하나를 고른다.

```
> xcopy /E samples\markdown-viewer-wasm my-viewer\   # 완성형 참조 구현에서 시작
> cargo new --lib my-viewer                          # 빈 크레이트에서 시작(§1-2)
```

### 1-1. 프로젝트 구성

```
my-viewer/
├── Cargo.toml          # cdylib · [workspace] 분리 · release 프로필
├── src/lib.rs          # ABI(nx_meta/nx_preview) + 렌더 로직
├── fixtures/           # 로컬 테스트용 예제 파일(선택)
└── dist/my-viewer.wasm # 배포 산출물(선택 — 저장소 동봉·E2E용)
```

```toml
# Cargo.toml — 필수 4항목
[lib]
crate-type = ["cdylib"]        # ① wasm 모듈 산출

[workspace]                    # ② 앱 워크스페이스와 분리(타깃이 다름)

[profile.release]              # ③ 크기 최적화(참조 구현 80KB·최소 예제 19KB)
opt-level = "z"
lto = true
panic = "abort"                # ④ 패닉 = 트랩(호스트가 오류 1줄로 격리)
strip = "symbols"
```

### 1-2. 최소 예제 (그대로 붙여넣어 동작 — 검증됨)

`.log`/`.ini`를 줄 번호와 함께 보여주는 19KB 플러그인 전체 코드:

```rust
#[link(wasm_import_module = "env")]
extern "C" {
    fn read_text(ptr: *mut u8, cap: i32) -> i32;
}

/// 반환 버퍼: 선두 4바이트 LE 길이 + UTF-8 본문(인스턴스는 호출당 1회 = leak 무해)
fn ret(s: &str) -> *mut u8 {
    let b = s.as_bytes();
    let mut v = Vec::with_capacity(4 + b.len());
    v.extend_from_slice(&(b.len() as u32).to_le_bytes());
    v.extend_from_slice(b);
    Box::leak(v.into_boxed_slice()).as_mut_ptr()
}

#[no_mangle]
pub extern "C" fn nx_meta() -> *mut u8 {
    ret("hello\nHello Viewer\nlog,ini") // id \n 표시명 \n 확장자들
}

#[no_mangle]
pub extern "C" fn nx_preview() -> *mut u8 {
    let mut buf = vec![0u8; 4096];
    let n = unsafe { read_text(buf.as_mut_ptr(), 4096) }.max(0) as usize;
    buf.truncate(n);
    let text = String::from_utf8_lossy(&buf);
    let mut out = String::from("lines\n\u{2}h1|Hello Viewer\n"); // \u{2}h1| = 제목 태그
    for (i, line) in text.lines().take(50).enumerate() {
        out.push_str(&format!("{:>3} | {}\n", i + 1, line));
    }
    ret(out.trim_end())
}
```

## 2. ABI 계약 (필수 export 2개)

| export | 반환 | 내용 |
| --- | --- | --- |
| `nx_meta()` | 버퍼 ptr | `id\n표시명\next1,ext2` — 3줄. `exts` = 적용 확장자 **기본값**(설정 `preview_map`이 재정의) |
| `nx_preview()` | 버퍼 ptr | 첫 줄 `lines` 또는 `image` + 이후 본문. `lines` 본문에는 §3 표시 태그 사용 가능 |

버퍼 규약 = **선두 4바이트 LE 길이 + UTF-8 본문** 포인터(위 `ret()` 참조).
`memory`는 자동 export되므로 별도 선언이 필요 없다.
`id`는 설정(`preview_map`·사용 여부)의 영구 키 — **개명 금지**.

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

## 3-1. ABI v2 — 압축 목록 플러그인 (X-46)

압축 파일은 **줄 텍스트가 아니라 항목 표**로 보여 준다(도크 = 요약 · 별도 창 =
그리드). 미리보기 플러그인과 같은 시임을 쓰되 export/import 몇 개가 더해진다 —
설계 SSOT는 [28 압축 미리보기](28-archive-preview.md), 참조 구현은
[`samples/archive-viewer-wasm`](../samples/archive-viewer-wasm/)(ISO 9660·ar·cpio, 31KB).

**선언** — `nx_meta()`의 **4번째 줄**에 능력을 적으면 호스트가 `nx_archive()`를 부른다
(4번째 줄이 없으면 종전과 동일한 미리보기 전용 플러그인 = 하위 호환).

```
archive-sample\nArchive Sample\niso,a,deb,lib,cpio\narchive
   id             표시명            확장자                 ← 능력 선언
```

**반환**(`nx_archive()`) — 첫 줄이 종류:

| 첫 줄 | 이후 |
| --- | --- |
| `archive` | 둘째 줄 `표시명	플래그`(플래그 = `solid,multivolume,truncated`), 셋째 줄부터 항목 1줄씩 |
| `password` | 없음 — 호스트가 암호 입력창을 띄우고 **같은 호출을 재시도**한다 |
| `error` | 둘째 줄 = 사용자에게 보일 사유 |

항목 1줄 = `경로	원본 크기	압축 크기	시각(Unix 초)	속성	방식`
(속성 = `dir`·`enc`[내용 암호화]·`utc`[시각이 UTC epoch]·`unsafe` 쉼표 목록.
모르는 값은 빈 칸. 시각에 `utc`가 없으면 **현지 벽시계 값**으로 해석한다 — DOS 시각 계열).

**추가 import**:

| import | 시그니처 | 설명 |
| --- | --- | --- |
| `file_size` | `() -> i64` | 대상 파일 크기(꼬리에서 읽는 포맷의 오프셋 계산) |
| `read_at` | `(off, ptr, cap) -> n` | **대상 파일** 임의 위치 읽기(1회 4MB 클램프) |
| `password` | `(ptr, cap) -> n` | 사용자가 방금 입력한 암호. **없으면 -1** |

```rust
// 암호가 필요한 포맷의 표준 흐름(2줄)
let mut pw = [0u8; 256];
if unsafe { password(pw.as_mut_ptr(), 256) } < 0 {
    return ret("password"); // 호스트가 입력받아 재호출
}
```

암호 규약: 호스트는 값을 게스트 메모리에 **1회 복사**할 뿐이고 인스턴스는 호출 종료와
함께 폐기된다. 플러그인도 값을 파일에 쓰거나 반환 버퍼에 담아서는 안 된다(계약 —
[28 §5](28-archive-preview.md)).

## 4. 빌드 → 적용 → 확인

### 4-1. 빌드

```
rustup target add wasm32-unknown-unknown                 # 최초 1회
cargo build --release --target wasm32-unknown-unknown
:: 산출: target\wasm32-unknown-unknown\release\<크레이트명>.wasm
```

크레이트명의 `-`는 산출물에서 `_`가 된다(`my-viewer` → `my_viewer.wasm`). 파일명은
자유롭게 바꿔도 되며, **파일명 사전순이 곧 로드 순서**(같은 확장자를 두 플러그인이
선언하면 앞선 파일이 이김 — 우선하려면 `00-` 접두).

### 4-2. 적용(설치)

앱은 **두 곳**을 이 순서로 읽는다(`preview::plugin_dirs`):

| 순위 | 위치 | 성격 |
| --- | --- | --- |
| ① | `data\plugins\` — 포터블 = `<exe 폴더>\data\plugins\` · 설치형(쓰기 불가 위치) = `%LOCALAPPDATA%\NexaDir\data\plugins\` | **사용자 설치분**. 직접 만든 플러그인은 여기 |
| ② | `<exe 폴더>\plugins\` | **동봉분**(배포에 함께 실리는 기본 플러그인 — 읽기 전용) |

**같은 `id`가 양쪽에 있으면 ①이 이긴다** → 동봉된 `markdown.wasm`·`archive.wasm`을
직접 빌드한 최신본으로 대체하려면 같은 id로 ①에 넣으면 된다(동봉분은 건드리지 않는다).

폴더가 없으면 만든다. **복사 후 앱 재시작 = 반영**(로드는 미리보기 최초 사용 시
1회, 이후 캐시). 제거 = 파일 삭제 후 재시작.

### 4-2-1. 동봉 플러그인을 고칠 때(저장소 개발자)

`samples/markdown-viewer-wasm`·`samples/archive-viewer-wasm`은 배포에 실리는 두
플러그인의 소스다. 고쳤다면:

```powershell
pwsh scripts/build-plugins.ps1                 # 두 샘플 빌드 + samples/*/dist 갱신
cargo test -p nexa-app preview::sample         # 동봉본 E2E(새 dist를 실제로 로드)
```

`dist/*.wasm`은 테스트가 로드하는 **고정 산출물**이므로 소스와 **같은 커밋**에
포함한다. 릴리스는 태그의 소스로 다시 빌드해 배포하므로(`-SkipDist`) `dist/`와
배포본이 어긋날 일은 없다 — 절차 SSOT = [18 §3-1](18-build-and-test.md) ·
배포 형태 = [21 §5-2](21-distribution.md).

### 4-3. 확인·제어

1. 대상 파일 선택 → 하단 도크 **미리보기** 또는 **F3**(독립 창 — 기준 캔버스).
2. **설정 → 플러그인**: 설치된 목록이 `표시명 (id) — 확장자`로 보이고, **체크 해제
   = 사용 안 함**(내장 미리보기로 대체·즉시 적용·영속). 목록이 비어 있으면 로드
   실패이거나 위치·재시작 문제다.
3. 확장자 강제 지정: `data\settings.cfg`에
   `preview_map=xyz:my-viewer|txt:my-viewer` (확장자:id를 `|`로 연결 — 플러그인
   선언보다 우선).
4. 오류는 미리보기에 `플러그인 오류(id): …` **1줄**로만 표시된다(앱·타 플러그인 무영향).

**배포물 = `.wasm` 1개.** 사용자에게는 "① `data\plugins\`에 복사 ② 재시작" 두 줄만
안내하면 된다.

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
