# 점검 자동 판정 — 2026-09-04 17:45:40 (-Quick)

> 규격 [docs/29-audit-checklist.md](../../29-audit-checklist.md) · 커밋 `3e829f7` · exe `target/release/nexa-app.exe` · BigDir `C:\WINDOWS\System32`
> 이 파일은 `scripts/audit.ps1`이 생성한다. 정적 리뷰·실기 결과는 같은 폴더에 `README.md`·`0N-*.md`로 사람이 추가한다.

| ID | 항목 | 판정 | 상세 |
| --- | --- | --- | --- |
| T-1 | cargo test --workspace | PASS | passed=332 failed=0 |
| T-2 | clippy warnings/errors | PASS | warnings=0 errors=0 |
| T-3 | linux target check | PASS | errors=0 (경고는 기존 svg.rs 데드코드) |
| T-4 | line coverage (llvm-cov) | SKIP | -Coverage 미지정 |
| B-0 | release build | PASS |  |
| B-2 | exe size <= 10MB | PASS | 3,926,528 B = 3.93 MB (십진 — docs 표기 규약) |
| B-3 | imports = OS inbox only | PASS | B3 통과 — 전부 화이트리스트 내 |
| S-1 | PE mitigations | PASS | DllCharacteristics=0xC160 missing=[] |
| S-2 | manifest execution level | PASS | asInvoker |
| B-1 | idle memory | SKIP | -Idle 미지정(정식 B1은 docs/18: 10k 폴더·300s·3회 중앙값) |

== audit summary: PASS=8 FAIL=0 WARN=0 SKIP=2 INFO=0
