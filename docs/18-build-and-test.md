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

# 실행 (Windows)
cargo run -p nexa-app

# 릴리스 단일 exe (Windows)
cargo build --release -p nexa-app
# 산출: target/release/nexa-app.exe (CRT 정적 링크 — .cargo/config.toml)
```

## 3. 릴리스 프로파일 (예산 B2)

- 워크스페이스 `[profile.release]`: `opt-level=3` · `lto="fat"` · `codegen-units=1` · `panic="abort"` · `strip="symbols"`.
- `.cargo/config.toml`: `target.x86_64-pc-windows-msvc.rustflags = ["-C", "target-feature=+crt-static"]` — CRT 정적 링크(재배포 런타임 0).

## 4. CI

`.github/workflows/ci.yml` — push/PR마다:
- **core**(ubuntu·macos): `cargo test`
- **windows**(windows-latest): `cargo test` + `cargo build --release` + **예산 게이트** — B2(exe >10MB fail) · B3 = `scripts/budget-b3.ps1`(화이트리스트 **단일 출처**, CI·로컬 공용. 인박스 DLL이 늘어나는 변경은 push 전에 로컬로 `pwsh scripts/budget-b3.ps1` 실행해 확인하고 근거와 함께 갱신)

## 5. 릴리스 파이프라인 (M5-2 — GitHub Releases)

`.github/workflows/release.yml` — **버전 태그 push**(`0.5.0` 형식, `v` 접두사 허용) 시:

1. windows-latest에서 `cargo test` + `cargo build --release`
2. **예산 게이트**(B2 exe ≤10MB · B3 임포트 화이트리스트 — CI와 동일 스크립트) 통과 필수
3. 산출물을 `NexaDir2-<버전>-win-x64.exe`로 개명(포터블 단일 exe — DR-3 기본 채널)
4. **설치형 빌드**(DR-3 개정 07-16 — 보조 채널): 러너 내장 Inno Setup 6 `ISCC`로
   `installer/nexa.iss` 컴파일(`/DAppVersion` 주입) → `NexaDir2-Setup-<버전>.exe`
5. **GitHub Release 자동 생성**(자동 릴리스 노트) + **포터블·설치형 2종 첨부**
   (설계 상세 = [21-distribution.md](21-distribution.md))

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
