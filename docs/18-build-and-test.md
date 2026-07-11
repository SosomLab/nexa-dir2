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
- **windows**(windows-latest): `cargo test` + `cargo build --release` + **예산 게이트** — B2(exe >10MB fail) · B3(dumpbin 임포트가 화이트리스트[kernel32·user32·gdi32·ntdll·oleaut32·`api-ms-win-*`] 외면 fail)

## 5. 예산 측정 (DR-2 게이트)

| 항목 | 방법 |
| --- | --- |
| B1 유휴 RSS | Windows에서 앱 기동→10k 폴더 로드→유휴 5분→작업 관리자/`Get-Process`(WorkingSet64). 3회 중앙값 |
| B2 exe 크기 | `ls -l target/release/nexa-app.exe` — CI에서 10MB 초과 시 fail |
| B3 임포트 DLL | `dumpbin /imports` 또는 `llvm-objdump` — OS 인박스 외 발견 시 fail |
| B4 콜드 스타트 | 기동 로그 타임스탬프(창 표시까지). 후속: ETW |
| B5 100k 렌더 | 코어 벤치(원본 10만 노드 벤치 계승) + 실기 스크롤 |

측정 결과는 journal에 기록하고 [STATUS](STATUS.md)에 최신값을 유지한다.
