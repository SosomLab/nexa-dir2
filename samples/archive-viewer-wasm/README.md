# ArchiveViewerPlugin(wasm) — 압축 목록 플러그인 참조 구현

Nexa Dir **압축 미리보기 ABI v2**(X-46)의 참조 구현이자 시작 템플릿.
내장 리더가 다루지 않는 **ISO 9660(Joliet 포함) · ar(.a/.deb/.lib) · cpio(newc)**
목록을 읽어, "포맷 확장 = `.wasm` 파일 하나"임을 보여 준다.

- 설계 SSOT → [docs/28 압축 미리보기](../../docs/28-archive-preview.md)
- 플러그인 전 과정 → [docs/24 플러그인 개발 가이드](../../docs/24-plugin-dev-guide.md)

## 구조

```
archive-viewer-wasm/
├── Cargo.toml          # 독립 크레이트(cdylib · [workspace] 분리 · opt-level="z")
├── src/lib.rs          # ABI(nx_meta/nx_archive) + ISO/ar/cpio 리더
├── fixtures/           # 최소 샘플(sample.cpio · sample.a — 앱에서 바로 열어 확인)
└── dist/archive.wasm   # 빌드 산출물(동봉 — 저장소 E2E 테스트가 로드, 31KB)
```

## 빌드 → 배포

```
rustup target add wasm32-unknown-unknown        # 1회
cargo build --release --target wasm32-unknown-unknown
copy target\wasm32-unknown-unknown\release\archive_viewer.wasm  <NexaDir>\data\plugins\
```

저장소에서는 **두 동봉 플러그인을 한 번에** 빌드하는 스크립트를 쓴다(dist까지 갱신):

```powershell
pwsh scripts/build-plugins.ps1                 # markdown.wasm + archive.wasm → 각 dist/
pwsh scripts/build-plugins.ps1 -OutDir plugins # 배포 스테이징에도 복사
cargo test -p nexa-app preview::sample         # 동봉본 E2E 검증
```

앱을 다시 빌드할 필요가 없다 — **`.wasm` 하나를 복사하고 앱을 재시작**하면 끝이며,
같은 파일이 전 OS/아키텍처에서 동일하게 동작한다. 산출물을 갱신하면 `dist/archive.wasm`도
함께 교체한다(저장소 E2E: `cargo test -p nexa-app preview::sample`).

## ABI 요약 (v2 — 압축 목록)

```
nx_meta()    → "archive-sample\nArchive Sample (ISO/ar/cpio)\niso,a,deb,lib,cpio\narchive"
                id        표시명                              확장자          ← 4번째 줄 = 능력 선언
nx_archive() → "archive\n<표시명>\t<플래그>\n<항목>\n<항목>…"
                항목 = 경로 ⇥ 원본 ⇥ 압축 ⇥ 시각(Unix 초) ⇥ 속성 ⇥ 방식
                속성 = dir,enc,utc,unsafe (쉼표 목록 · 빈 값 = 해당 없음)
                플래그 = solid,multivolume,truncated
```

반환 버퍼는 기존 ABI와 같다(**선두 4바이트 LE 길이 + UTF-8 본문** 포인터).

호스트 import(`env`):

| 함수 | 용도 |
| --- | --- |
| `file_size() -> i64` | 대상 파일 크기(꼬리에서 읽는 포맷의 오프셋 계산) |
| `read_at(off, ptr, cap) -> n` | **대상 파일** 임의 위치 읽기(1회 4MB 상한) |
| `password(ptr, cap) -> n` | 사용자가 방금 입력한 암호(없으면 `-1`) |

### 암호가 필요한 포맷이라면

이 샘플의 3종은 암호 개념이 없어 쓰지 않지만, 규약은 다음 두 줄이 전부다.

```rust
let mut pw = [0u8; 256];
let n = unsafe { password(pw.as_mut_ptr(), 256) };
if n < 0 {
    return ret("password"); // 호스트가 입력창을 띄우고 같은 호출을 재시도한다
}
```

호스트는 **사용자가 방금 입력한 값만** 게스트 메모리에 1회 복사하고, 인스턴스는 호출이
끝나면 폐기된다(선형 메모리 소멸). 호스트 쪽 사본도 소거되며 디스크·로그에는 어디에도
남지 않는다 — 플러그인 역시 값을 파일로 쓰거나 반환 버퍼에 담아서는 안 된다(계약).

## 격리

fuel 2억 · 선형 메모리 64MB · 모듈 8MB · `read_at` 1회 4MB · 파일 접근은 **미리보기
대상 1개**로 제한. 초과·오류는 그 플러그인만 실패하고 앱과 다른 플러그인은 무영향이다.

## 새 포맷을 붙이려면

`src/lib.rs`의 모듈 하나(`iso`/`ar`/`cpio`)를 본떠 `sniff` + `list` 두 함수만 만들고
`nx_archive()`의 분기에 한 줄 추가하면 된다. 앱 쪽 내장 리더도 같은 모양이다
(`crates/nexa-vfs/src/archive/` — 파일 1개 + 레지스트리 한 줄).
