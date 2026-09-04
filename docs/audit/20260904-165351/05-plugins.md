# 05. 플러그인 런타임·동봉 플러그인 조사 — 2026-09-04

대상: `preview/{mod.rs, wasm.rs, archive.rs, sample_tests.rs}` · `scripts/build-plugins.ps1` · `samples/markdown-viewer-wasm` · `samples/archive-viewer-wasm` · docs 24/25/28.

## 런타임 격리(현황)
| 통제 | 값 | 근거 |
| --- | --- | --- |
| 연료/호출 | 200,000,000 | `wasm.rs:35`, `:226` |
| 연료 계량 | 플러그인별 Engine `consume_fuel(true)` | `wasm.rs:247-249` |
| 선형 메모리 | 64MB(`StoreLimits`) | `wasm.rs:37, 222, 225` |
| 모듈 크기 | 8MB(컴파일 전) | `wasm.rs:244-246` |
| **벽시계 타임아웃** | **없음**(ADR-0005 `docs/25:39` 약속 · Starlark 시절 300/500ms — wasmi 이식에서 회귀) | grep 0건 |
| 스택 | wasmi 기본(미고정) | |
| 호스트 임포트 연료 과금 | **없음**(`caller.consume_fuel` 0건) | `wasm.rs:82-216` |

복구: 트랩/연료/OOM/export 누락/버퍼 손상 → `call_buf`→`Err(String)` → 1줄 오류 표시(`wasm.rs:436-439`), Store 폐기. **반복 실패 격리 없음**, 로그 없음. `nx_loop` 테스트(`:557-582`)는 오류 존재만 단언(시간 상한 없음).

## ABI
버전 정수 없음 — meta 4번째 줄 caps로 판별(`wasm.rs:260-280`). v1(3줄)은 그대로 로드. 필수 export(`memory`·`nx_meta`) 로드 시 검사, `nx_preview`/`nx_archive` 누락은 **호출 시** 오류. 임포트 7종(`read_text·render_svg·is_dark·file_size·read_at·password·disp_width`) 전부 호스트 경계 검사·길이 클램프(256KB/4MB/4096)·반환 버퍼 1MB(`OUT_CAP`) · 출력 형태 검증(lines/image, archive/password/error, 50k·경로 정규화).

## 로딩
발견 = `data_dir()/plugins` → `exe/plugins`(dedupe, 이름 정렬, 폴더 간 id 선승) · **지연 로드 + 스레드별 OnceCell**(다른 스레드는 재컴파일) · 로드 시 **모든 .wasm의 `nx_meta` 실행(비활성 포함)** · 호출마다 Store·**Linker(7 func_wrap) 재구성**·인스턴스화 · 크기 markdown 81,635B·archive 31,218B · **로드/호출 시간 미측정** · 핫 리로드 없음.

## 보안
파일 접근 = `HostCtx.path` 한 경로뿐(게스트가 경로 지정 불가) · `read_at`은 호출마다 재오픈(TOCTOU·syscall N) · 쓰기 임포트 = `render_svg`→`%TEMP%\nexa-preview\d<hash>.bmp`(2000×2000·256KB 상한, **개수 무제한·미정리**) · 암호 = `Secret`+thread-local 스코프+zeroize(우수). 갭: `password`가 **전 플러그인**에 노출(`linker()` 능력 무관) · 잘림 무신호(재프롬프트 루프) · **출처 검증 없음**(포터블 `data\plugins`는 exe 옆 쓰기 가능).

## 동봉 플러그인
- **markdown.wasm**(79.7KB, v1): exts md/markdown/mdown/mkd · 64KB·400줄·셀 60 · Mermaid 3단 폴백(image→art→raw, 비Windows는 raw) · 테스트 `sample_tests.rs:16-58`.
- **archive.wasm**(30.5KB, v2 caps=archive): ISO 9660+Joliet(깊이 12)·ar(GNU/BSD 긴 이름)·cpio newc · MAX_ENTRIES 20,000(ISO만 truncated 표시·ar/cpio 무신호) · 비ISO 확장자마다 섹터 16 `read_at` 1회 · 테스트 `sample_tests.rs:153-222`.
- 합성 WAT 픽스처: `up.wasm`(meta+preview+nx_loop) · `arc.wasm`(암호 왕복) · `broken.wasm` 스킵.

## 설정
`plugins_disabled`(≤512, `builtin.*` 면제) · `preview_map`(≤512, 최우선, 무효 id 무시) · 순서는 파일명 정렬만 · **로드 오류 벡터 폐기**(`mod.rs:298` `_errors`) · 비활성도 로드·`nx_meta` 실행.

## 체크리스트 판정(A1~A30 — 규격은 29 §3)
PASS: A1·A7·A10·A11·A16·A17·A18·A24(스레드별)·A27 · 코드상 PASS/미검증: A9·A12·A13·A14·A26 · **FAIL**: A3(시간 상한 미단언)·A4·A6·A8·A19·A21·A28·A29·A30 · PARTIAL: A15(1MB 상한 "손상" 오표시)·A20(임시 BMP 무제한) · UNKNOWN: A5·A22·A23·A25.

## 발견(심각도순)
| # | 문제 → 수정 |
| --- | --- |
| 1 | 타임아웃 없음 + 임포트 무과금 + UI 스레드 → `set_fuel` + 임포트 내 `Instant` 검사·`consume_fuel` 과금 + 워커 이동(TODO X-2 잔여) |
| 2 | OUT_CAP 1MB vs 20k 엔트리(≈1.2MB) → "반환 버퍼 손상" 오표시 → `nx_archive`용 상한 상향/페이징·게스트 상한 정합·메시지 구분 |
| 3 | 로드 오류 폐기 → 설정 창/상태에 사유 표시 + 로그 |
| 4 | `password` 전 플러그인 노출 → `plugin.caps`로 링커 구성 |
| 5 | 비활성 플러그인도 실행 → `is_disabled` 선판정 |
| 6 | 출처 검증 없음 → 동봉본 SHA-256 매니페스트·미지 모듈 경고 |
| 7 | 서킷 브레이커 없음 → 연속 실패 카운터로 세션 내 자동 비활성 |
| 8 | dist 드리프트 미검출(CI `-SkipDist`) → 빌드 후 `git diff --exit-code samples/*/dist` |
| 9 | 호스트 잘림 무신호(`.take(ARCHIVE_CAP)`에 `truncated` 미설정) · 샘플 ar/cpio 동일 |
| 10 | 임시 BMP 무제한·링커 재구성·`read_at` 재오픈 → 상한/정리·캐시·핸들 재사용 |
