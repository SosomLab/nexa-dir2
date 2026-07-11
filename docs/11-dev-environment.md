# 11 · 개발 환경 — 맥 일상 개발 / Windows 실행·신뢰 원천

> 원본 docs/11의 모델을 계승하되, C#/WinUI가 없어져 **맥에서 검증 가능한 범위가 넓어졌다**.

## 1. 부문별 개발 가능 범위

| 부문 | macOS | Windows | 비고 |
| --- | --- | --- | --- |
| 코어 크레이트(core/vfs/tree/ops 순수부) | ✅ build·test | ✅ | 원본과 동일 — 일상 개발은 맥 |
| Windows 전용 코드(`#[cfg(windows)]` — gui/shell/term) | ✅ **`cargo check --target x86_64-pc-windows-msvc`**(타입 검증) | ✅ build·실행 | 맥은 링크·실행 불가, 컴파일 검증까지 가능 |
| 실행·실기 QA·예산 실측(B1~B4) | ✗ | ✅ PC/VM/**CI** | CI(windows-latest)가 신뢰 원천 |

원본 대비 개선: WinUI XAML 컴파일러(Windows 전용) 의존이 사라져, **UI 코드까지 맥에서 타입 검증**된다.

## 2. 셋업

```sh
# macOS (일상 개발)
rustup toolchain install stable
rustup target add x86_64-pc-windows-msvc   # Windows 코드 cargo check용
cargo test                                  # 코어 크레이트 전체
cargo check --target x86_64-pc-windows-msvc # Windows 코드 포함 전체 타입 검증

# Windows (실행·QA)
rustup toolchain install stable             # + VS Build Tools(link.exe)
cargo run -p nexa-app
```

- 툴체인 고정: `rust-toolchain.toml`(stable). 빌드·테스트 절차 SSOT → [18](18-build-and-test.md).
- 다른 PC: clone → 위 셋업 → 빌드. bootstrap 스크립트는 필요해지면 추가(현재 rustup 두 줄로 충분).

## 3. CI (GitHub Actions)

| job | 러너 | 내용 |
| --- | --- | --- |
| `core` | ubuntu + macos | `cargo test`(순수부 크로스 검증) |
| `windows` | windows-latest | `cargo build --release` + `cargo test` + **예산 검사**(exe 크기·임포트 테이블) |

푸시마다 실행. WinUI 시절의 "맥은 앱 빌드 불가 → CI green 확인 필수" 규율은 **실행 검증**에만 남는다(컴파일은 맥에서 사전 확인).
