# 22 · 일괄 이름 변경 v2 설계 — Path Finder 패리티

> **작성: 2026-07-17** — 사용자 주도 Path Finder Batch Rename 전 동작 분석(6종 —
> 스크린샷 단계별 대조)을 기반으로 한 dir2 일괄 이름 변경 확장 설계.
> 현재 구현 = [nexa-ops::batch_rename](../crates/nexa-ops/src/batch_rename.rs)
> (파이프라인·프리셋·정규식 — M5-1·07-15) + [bulkrename.rs](../crates/nexa-app/src/bulkrename.rs).

## 1. 분석 결과 요약 (PF 6동작 대조)

| PF 동작 | dir2 현재 | 갭 |
| --- | --- | --- |
| Replace Text | `Replace{find,with,match_case}` | **Apply to 스코프** · **Mode**(First/Last/Entire) |
| Replace RegEx | `Replace{regex:true}` — $1 캡처·사전 검증 | **Apply to 스코프**(Mode는 PF도 없음 — 앵커로 대체) |
| Insert Text | `Insert{text,suffix}` (앞/뒤만) | **Apply to** · **임의 위치**(N+방향·초과 클램프) |
| Change Case | `Case` 4모드 — **정확히 일치** | **Apply to**만 |
| Add Number Sequence | `Number{start,step,pad,suffix}` | **Apply to** · **임의 위치** · **Prefix/Suffix 감싸기** |
| Add Date | **없음** | **동작 신설**(원천·포맷·위치·감싸기) |
| — dir2 고유 | `Move`(구간 이동)·`ChangeExt` | PF에 없는 우리 강점 — 유지 |

공통 관찰: ① **Apply to(적용 스코프)가 전 동작 공통 필드** ② Insert·Number·Date가
**같은 위치 모델**(오프셋 N + 앞/뒤 방향 + 범위 초과 시 반대편 클램프 — 오류 아님)
③ 미리보기 = **변경 항목 ✓ 표시 + "N items will be renamed" 카운트**.

## 2. 코어 설계 (nexa-ops::batch_rename v2 — 순수 로직 유지)

### 2-1. 공통 타입

```rust
/// 적용 스코프(PF Apply to) — 동작별 필드. 기본 Name(현행과 동일).
enum Scope { Name, NameExt, Ext, ExtDot }

/// 삽입 위치(PF Position — Insert·Number·Date 공용):
/// 선택한 끝에서 `offset` 문자 지점, 범위 초과는 반대편 클램프(관대 규약 — Move와 동일).
struct InsertAt { offset: usize, from_end: bool }

/// 치환 범위(PF Mode — Replace 일반 텍스트 전용. 정규식은 항상 All — PF 동일).
enum ReplaceMode { All, First, Last, Entire }
```

- 파이프라인 적용부는 (stem, ext) 쌍으로 동작: 스코프가 작업 문자열을 선택
  (`Name`=stem · `NameExt`=stem+"."+ext · `Ext`=ext · `ExtDot`="."+ext) → 동작 적용 →
  재조립. NameExt 결과는 **마지막 `.` 기준 재분해**(탐색기 규약), ExtDot에서 점이
  사라지면 확장자 없음.

### 2-2. RenameOp v2

```rust
enum RenameOp {
    Replace { scope, find, with, match_case, regex, mode: ReplaceMode },
    Case { scope, mode: CaseMode },                    // 4모드 = PF와 1:1(현행 유지)
    Insert { scope, text, at: InsertAt },
    Number { scope, start, step, pad, at: InsertAt, prefix: String, suffix: String },
    Date { scope, kind: DateKind, format: String, at: InsertAt, prefix: String, suffix: String },
    Move { .. }, ChangeExt { .. },                     // dir2 고유 — 변경 없음
}

enum DateKind { Modified, Created }                     // PF Type 대응
```

### 2-3. Add Date 포맷 — 토큰 문자열(드래그 빌더 대체)

PF는 드래그형 토큰 빌더지만 네이티브 다이얼로그 규모에 과함 — **포맷 문자열 +
실시간 미리보기 + 사전 검증**으로 대체(정규식 검증 UI와 동일 패턴):

| 토큰 | 의미 | 예(2026-07-01) |
| --- | --- | --- |
| `yyyy`/`yy` | 연 4/2자리 | 2026 / 26 |
| `MM`/`M`/`MMM` | 월 2/1자리/영문 축약 | 07 / 7 / Jul |
| `dd`/`d` | 일 2/1자리 | 01 / 1 |
| `HH`·`mm`·`ss` | 시·분·초 2자리 | 01·43·49 |
| `ddd` | 요일 영문 축약 | Wed |
| 그 외 문자 | 리터럴 | - _ 등 |

기본값 `yyyy-MM-dd`. 시간 값은 기존 `fmt_datetime`처럼 TZ 오프셋 반영.

### 2-4. 미리보기 입력 확장 (시그니처 변경)

Date가 파일별 메타데이터를 요구 — `preview(names)` →
`preview(items: &[RenameInput], ops)`:

```rust
struct RenameInput { name: String, modified_unix_ms: i64, created_unix_ms: i64 }
```

호출부(bulkrename.rs)는 수정일 = VisibleRow 보유값, 생성일 = `std::fs::metadata`
`created()` 1회 조회(실패 시 0 → Date 결과 빈 문자열 — 오류 격리).

### 2-5. 직렬화·프리셋 하위호환

- 신규 필드는 **생략 = 기본**: `scope` 생략=Name · `at` 생략=구 `suffix:bool` 매핑
  (`false→{0,false}` · `true→{0,true}`) · `mode` 생략=All · Number `prefix/suffix` 생략=빈.
- 구 프리셋 파일은 그대로 로딩(파서는 미지 필드 무시 — 현행 관대 파싱 유지).

## 3. UI 설계 (bulkrename.rs)

1. **스코프 콤보** — 각 파라미터 패널 최상단 공통(이름/전체/확장자/점 포함 확장자).
2. **위치 입력** — Insert·Number·Date 공용: [숫자 EDIT][앞에서/뒤에서 라디오]
   (PF 토글 대응 — prefs 3×3 등 기존 라디오 패턴 재사용).
3. **Replace Mode 콤보** — 정규식 체크 시 숨김(항상 All — PF 규약).
4. **Number/Date의 Prefix·Suffix EDIT** 2필드.
5. **Date 패널** — Type 콤보(수정일/생성일)·포맷 EDIT(기본 yyyy-MM-dd)·위치·감싸기.
6. **결과 표기 라벨**(PF 디테일 채택): Padding 콤보 = "1, 2, 3…"/"01, 02…" ·
   Case = "AB CD"/"Ab Cd"/"Ab cd"/"ab cd".
7. **미리보기 개선**: 변경 항목 ✓ 마커 열 + 하단 "N개 항목이 변경됩니다" 카운트 ·
   무변경 항목은 적용에서 제외(현행 동작 명시화).

## 4. 구현 슬라이스 (수직 — 커밋 단위)

| # | 내용 | 규모 |
| --- | --- | --- |
| S1 | 코어 v2: Scope·InsertAt·ReplaceMode·Number 감싸기·Date 토큰 엔진 + 단위 테스트(하위호환 왕복 포함) | 중 |
| S2 | preview 입력 확장(RenameInput) + UI 재구성(스코프·위치·모드·Date 패널·결과 라벨) | 중 |
| S3 | 미리보기 ✓ 마커·건수 카운트·무변경 제외 명시 | 소 |

등재: TODO **X-22**(P2). PF에 없는 Move·ChangeExt는 그대로 유지 — 우리 강점.
