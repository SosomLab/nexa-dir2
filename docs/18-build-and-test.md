# 18 · 빌드 & 테스트 (SSOT)

> 빌드·테스트·측정 절차의 **단일 출처**. 명령·도구·전제·산출물이 바뀌는 변경은 같은 커밋에서 이 문서를 갱신한다(원본 규약 계승).

## 1. 전제 도구

| 도구 | macOS | Windows |
| --- | --- | --- |
| Rust stable(rustup) | ✅ | ✅ |
| Windows 타깃 std | `rustup target add x86_64-pc-windows-msvc` | (기본) |
| 링커 | 불요(check만) | VS Build Tools 2022(link.exe) |

## 2. 명령

```sh
# 전체 테스트 (macOS/Linux/Windows) — 순수 크레이트
cargo test

# Windows 코드 포함 전체 타입 검증 (macOS에서)
cargo check --target x86_64-pc-windows-msvc --workspace

# 비Windows 컴파일 검증 (Windows에서) — CI core 잡과 같은 cfg 경로를 로컬에서 재현
rustup target add x86_64-unknown-linux-gnu          # 최초 1회
cargo check --workspace --all-targets --target x86_64-unknown-linux-gnu

# 실행 (Windows)
cargo run -p nexa-app

# 릴리스 단일 exe (Windows)
cargo build --release -p nexa-app
# 산출: target/release/nexa-app.exe (CRT 정적 링크 — .cargo/config.toml)
```

## 3. 릴리스 프로파일 (예산 B2)

- 워크스페이스 `[profile.release]`: `opt-level=3` · `lto="fat"` · `codegen-units=1` · `panic="abort"` · `strip="symbols"`.
- `.cargo/config.toml`: `target.x86_64-pc-windows-msvc.rustflags = ["-C", "target-feature=+crt-static"]` — CRT 정적 링크(재배포 런타임 0).

## 3-1. 동봉 플러그인(.wasm) 빌드 — samples/*-wasm

미리보기 플러그인은 **워크스페이스 밖 독립 크레이트**(타깃이 `wasm32-unknown-unknown`)라
`cargo test --workspace`·`cargo build`가 건드리지 않는다. 배포에 함께 싣는 두 개
(`markdown.wasm`·`archive.wasm`)는 **단일 출처 스크립트**로 빌드한다.

```powershell
rustup target add wasm32-unknown-unknown        # 최초 1회

pwsh scripts/build-plugins.ps1                  # 빌드 + samples/*/dist 갱신
pwsh scripts/build-plugins.ps1 -OutDir plugins  # 배포 스테이징(plugins\)에도 복사
pwsh scripts/build-plugins.ps1 -SkipDist -OutDir plugins   # dist는 그대로(릴리스 CI 방식)
```

| 대상 | 산출물 | 쓰임 |
| --- | --- | --- |
| `samples/markdown-viewer-wasm` | `dist/markdown.wasm` | 저장소 동봉본 — **E2E 테스트가 로드** |
| `samples/archive-viewer-wasm` | `dist/archive.wasm` | 〃 |
| `-OutDir`(예: `plugins\`) | 두 파일 | 배포 스테이징 — 포터블 zip·설치본·플러그인 zip |

- **CI**: windows 잡이 같은 스크립트로 빌드 검증(릴리스에서 처음 깨지지 않게).
- **릴리스**: 태그의 소스로 다시 빌드해 배포한다(`-SkipDist` — 저장소 `dist/`는
  테스트용 고정본이라 릴리스가 덮어쓰지 않는다).
- **플러그인을 고쳤다면**: `pwsh scripts/build-plugins.ps1` → `cargo test -p nexa-app preview::sample`
  (E2E가 새 `dist/*.wasm`을 검증) → `dist/*.wasm`까지 **같은 커밋에 포함**.
- 새 플러그인을 동봉 대상에 추가하려면 [scripts/build-plugins.ps1](../scripts/build-plugins.ps1)의
  `$plugins` 배열에 한 줄 추가하면 된다(빌드·복사·CI·릴리스가 함께 따라온다).

앱에서의 적용 경로·교체 방법은 [24 §4](24-plugin-dev-guide.md) · 배포 형태는
[21 §5-2](21-distribution.md).

## 4. CI

`.github/workflows/ci.yml` — push/PR마다:
- **core**(ubuntu·macos): `cargo test`
  > **Windows에서 개발할 때 이 잡이 사각지대다**(08-02 CI 2연속 실패). Windows에서
  > `cargo test`가 아무리 green이어도, `#[cfg(windows)]` 블록이 통째로 사라지는
  > 비Windows 경로에서는 **타입 추론이 끊기거나**(`Vec::new()`의 원소 타입 —
  > E0282) **임포트가 미사용**이 되어 컴파일이 깨질 수 있다. Windows 전용 모듈을
  > 추가·게이팅했다면 push 전에 위 §2의 `--target x86_64-unknown-linux-gnu`
  > 검사를 돌린다(링크 불필요 — `cargo check`로 충분).
- **windows**(windows-latest): `cargo test` + `cargo build --release` + **예산 게이트** — B2(exe >10MB fail) · B3 = `scripts/budget-b3.ps1`(화이트리스트 **단일 출처**, CI·로컬 공용. 인박스 DLL이 늘어나는 변경은 push 전에 로컬로 `pwsh scripts/budget-b3.ps1` 실행해 확인하고 근거와 함께 갱신)

## 5. 릴리스 파이프라인 (M5-2 — GitHub Releases)

`.github/workflows/release.yml` — **버전 태그 push**(`0.5.0` 형식, `v` 접두사 허용) 시:

1. windows-latest에서 `cargo test` + `cargo build --release`
2. **예산 게이트**(B2 exe ≤10MB · B3 임포트 화이트리스트 — CI와 동일 스크립트) 통과 필수
3. 산출물을 `NexaDir-<버전>-win-x64.exe`로 개명(포터블 단일 exe — DR-3 기본 채널)
4. **설치형 빌드**(DR-3 개정 07-16 — 보조 채널): 러너 내장 Inno Setup 6 `ISCC`로
   `installer/nexa.iss` 컴파일(`/DAppVersion` 주입) → `NexaDir-Setup-<버전>.exe`
5. **GitHub Release 자동 생성**(자동 릴리스 노트) + **포터블·설치형 2종 첨부**
   (설계 상세 = [21-distribution.md](21-distribution.md))
6. **Chocolatey**(3채널, **2패키지** `nexa-dir`·`nexa-dir.portable`): 각 자산의
   SHA-256 주입 → `choco pack` → **`CHOCO_API_KEY` 시크릿 + 저장소 변수
   `CHOCO_PUSH=true`가 모두 있으면** `choco push`(스위치 07-21 — 승인 전까지 꺼 두었고
   **2026-09-02 승인으로 등록·재개**. [21 §7](21-distribution.md) — 최초 등록·수동 게시·재개 절차 포함)
   - 태그를 소진한 버전을 **빌드 없이** 다시 올릴 때는 [`resubmit-chocolatey`](../.github/workflows/resubmit-chocolatey.yml)
     dispatch(릴리스 자산 → 해시 → pack → push. main을 체크아웃하므로 **패키징 수정분이 반영**된다)

```sh
# 릴리스 절차(예: 0.6.0) — main green 확인 후
git tag 0.6.0 && git push origin 0.6.0
# → Actions "Release" 실행 → github.com/SosomLab/nexa-dir2/releases 에 exe 첨부
```

`workflow_dispatch` 수동 실행은 게이트+아티팩트 업로드까지만(Release 생성은 태그에서만).

## 6. 예산 측정 (DR-2 게이트)

| 항목 | 방법 |
| --- | --- |
| B1 유휴 RSS | Windows에서 앱 기동→10k 폴더 로드→유휴 5분→작업 관리자/`Get-Process`(WorkingSet64). 3회 중앙값 |
| B2 exe 크기 | `ls -l target/release/nexa-app.exe` — CI에서 10MB 초과 시 fail |
| B3 임포트 DLL | `dumpbin /imports` 또는 `llvm-objdump` — OS 인박스 외 발견 시 fail |
| B4 콜드 스타트 | 기동 로그 타임스탬프(창 표시까지). 후속: ETW |
| B5 100k 렌더 | 코어 벤치(원본 10만 노드 벤치 계승) + 실기 스크롤 |

측정 결과는 journal에 기록하고 [STATUS](STATUS.md)에 최신값을 유지한다.
