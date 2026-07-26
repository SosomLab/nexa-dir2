# 23 · 미리보기 플러그인 개발자 가이드 — MarkdownViewerPlugin으로 배우는 전 과정

> Nexa Dir **Starlark 미리보기 플러그인**을 독립 프로젝트로 만들고 배포하기까지의
> **전 과정을 순서대로** 안내한다. 각 단계는 동봉 샘플
> [`samples/markdown-viewer`](../samples/markdown-viewer/)(Markdown 뷰어 — 참조 구현)로 실증한다.
>
> - 설계 결정(왜 Starlark인가): [09 ADR-0004](09-adr-0004-preview-plugins.md) · DR-7/DR-8([10](10-decision-record.md))
> - 도입 경위·실측: [journal/2026-07-26](journal/2026-07-26.md) · 브랜치 `feat/starlark-plugin`

---

## 0. 개념 — 플러그인 시스템이 동작하는 방식

### 0-1. 한 장 요약

```
파일 선택 ─▶ 확장자 결정 ─▶ 공급자 결정 ─▶ preview(file) 실행 ─▶ lines/image
                              │                                    │
             ① 설정 preview_map(외부 Override)          하단 도크(축약 뷰)
             ② 스크립트 EXTS 선언(기본값·파일명 순)      독립 미리보기 창(F3 — 기준 캔버스)
             ③ 내장 폴백(builtin.image → builtin.text)
```

- **플러그인 = `.star` 파일 1개**(Python 문법의 샌드박스 부분집합 Starlark).
  `data\plugins\` 에 복사만 하면 로드된다 — 재빌드·설치·레지스트리 등록 없음.
- **적용 대상은 스크립트가 기본값을 선언하고(`EXTS`), 최종 결정은 사용자 설정이
  가진다**(`preview_map` — §7-3). 같은 확장자를 여러 플러그인이 선언하면
  파일명(로드) 순으로 먼저 온 쪽이 이긴다.
- **표시 캔버스는 2개**: 하단 도크(항상)와 **독립 미리보기 창**(F3). 플러그인은
  **독립 창 기준으로 개발**한다 — 콘솔 폰트 **문자 그리드**(모노스페이스)라서
  박스 드로잉·표 정렬이 안정적이고, 도크는 같은 lines의 축약 뷰다.
- **격리**: 플러그인 오류는 그 플러그인만 비활성/오류 1줄 — 앱과 다른 플러그인에
  영향 없다. 파일 접근은 **미리보기 대상 파일 1개**로 제한된다(샌드박스).

### 0-2. 로드·실행 수명

| 시점 | 동작 |
| --- | --- |
| 앱 시작 | 아무 일도 없음(상주 RSS 영향 0 — B1 예산) |
| 미리보기 최초 사용 | `data\plugins\*.star` **파일명 순** 로드 → 파스·평가·동결(캐시) |
| 파일 선택마다 | 캐시된 플러그인의 `preview(file)` 실행(임시 힙 — 실행 후 회수) |
| 스크립트 수정 후 | **앱 재시작** = 재로드(핫 리로드는 후속) |

---

## 1. 1단계 — 프로젝트 만들기

플러그인은 단일 `.star` 파일이지만, **독립 프로젝트**로 관리하면 픽스처·문서와
함께 버전 관리를 할 수 있다. 샘플을 그대로 복사해 시작한다:

```
my-viewer/                       # 프로젝트 루트(임의 위치 — 앱과 무관)
├── my-viewer.star               # 플러그인 본체(배포 단위 — 파일명 = 로드 순서 결정)
├── fixtures/                    # 테스트용 예제 파일들
│   └── sample.xyz
└── README.md                    # 계약·설치 안내(배포 시 동봉 권장)
```

시작 명령(샘플 복사):

```
> xcopy /E samples\markdown-viewer my-viewer\
> ren my-viewer\markdown.star my-viewer.star
```

> **파일명 규약**: 소문자-하이픈 권장. 같은 확장자를 두 플러그인이 선언하면
> **파일명 사전순**이 우선순위이므로, 의도적으로 앞서려면 `00-` 접두를 쓸 수 있다.

## 2. 2단계 — 계약 선언 (메타 3종)

스크립트 상단에 **전역 상수 3개**를 선언한다. 이것이 플러그인의 매니페스트다
(별도 JSON/TOML 없음):

```python
ID   = "markdown"                          # 안정 식별자 — 설정 preview_map의 키. 개명 금지
NAME = "Markdown Viewer"                   # 표시명(공급자 콤보 — S3 예정)
EXTS = ["md", "markdown", "mdown", "mkd"]  # 적용 확장자 기본값(점 없이·대소문자 무관)
```

- `EXTS` = **스크립트 내부에 지정하는 적용 대상 기본 설정**. 사용자가 설정
  `preview_map`으로 외부에서 재정의(Override)할 수 있으므로(§7-3), 스크립트는
  "이 플러그인이 잘 다루는 확장자"만 선언하면 된다.
- `ID`는 설정 파일에 기록되는 영구 키다 — 배포 후 바꾸면 사용자 설정이 끊어진다.

## 3. 3단계 — `preview(file)` 구현

### 3-1. 시그니처와 반환

```python
def preview(file):
    # file.path : str  — 대상 파일 전체 경로
    # file.ext  : str  — 확장자(소문자·점 없음)
    # file.size : int  — 바이트 크기
    src = read_text(65536)
    return {"lines": ["첫 줄", "둘째 줄"]}   # 또는 {"image": file.path}
```

| 반환 | 의미 |
| --- | --- |
| `{"lines": [str]}` | 텍스트 라인들 — 도크·독립 창이 표시(**상한 1000줄·줄당 4096자** — 초과분 잘림) |
| `{"image": 경로}` | 호스트 WIC 이미지 렌더 위임(도크에서 표시) |

### 3-2. 호스트 API (샌드박스 표면 — 이것 외 I/O 불가)

| API | 반환 | 설명 |
| --- | --- | --- |
| `read_text(n)` | str | **미리보기 대상 파일**의 앞 n바이트(UTF-8 lossy). 호스트 상한 256KB 클램프. 임의 경로 읽기 불가 |
| `disp_width(s)` | int | 표시 폭(CJK/이모지 = 2칸) — 표·상자 정렬용(콘솔 그리드 기준) |
| `file.path/ext/size` | — | `preview(file)` 인자의 속성 |

### 3-3. Starlark 언어 주의(Python과 다른 점)

- **`while` 없음 · 재귀 금지** — `for _ in range(상한)` + `break`로 대체
  (샘플의 `_inline`/`_render` 참조).
- 문자열 순회는 `s.elems()`(1문자 str), `ord`/`chr` 없음 — 폭 계산은 `disp_width()`.
- 표준 내장: `len/range/enumerate/zip/sorted/min/max/str/int/format/join/split/…`.
  f-string 없음 — `"{}".format(x)`.
- 모듈 톱레벨 문장은 로드시 1회 실행(메타 선언 + 헬퍼 def 권장 — 무거운 계산 금지).

## 4. 4단계 — 렌더 규칙 (독립 창 = 기준 캔버스)

- **독립 미리보기 창(F3)**: 설정 콘솔 폰트(`term_font`)의 **문자 그리드** + 세로
  스크롤. 박스 드로잉(`┌─┐│…`)·표는 `disp_width()` 폭 계산과 함께 쓰면 정렬이
  보장된다. **여기 보이는 그대로가 플러그인의 정답 화면**이다.
- **하단 도크**: 같은 lines를 본문 폰트로 축약 표시(스크롤 없음·가시 영역만).
  긴 문서·넓은 표는 독립 창에서 확인하도록 안내 라인을 넣는 것도 방법.
- 권장: 한 줄 폭 ≤ 120칸 · 라인 수는 목적에 맞게(호스트 상한 1000줄).

## 5. 5단계 — 로컬 테스트 (수동)

1. 산출물 복사 → **플러그인 디렉터리**:
   - 포터블: `<exe 폴더>\data\plugins\my-viewer.star`
   - 설치형(쓰기 불가 폴더): `%LOCALAPPDATA%\NexaDir\data\plugins\my-viewer.star`
2. 앱 재시작(재로드) → `fixtures\sample.xyz` 파일 선택.
3. 하단 도크 **미리보기** 종류 + **F3**(독립 창)으로 확인.
4. 오류 확인법:
   - **로드 실패**(문법 오류·메타 누락) = 그 파일만 건너뜀(다른 확장자는 정상).
   - **실행 오류**(`fail()`·예외) = 미리보기에 `플러그인 오류(id): 메시지` 1줄.
5. 반복: 스크립트 수정 → 복사 → 재시작.

## 6. 6단계 — 자동 테스트 (저장소 개발자)

본 저장소에는 샘플을 **실제 런타임으로 로드·실행**하는 통합 테스트가 있다 —
자기 플러그인도 같은 패턴으로 추가할 수 있다
([`preview/sample_tests.rs`](../crates/nexa-app/src/preview/sample_tests.rs)):

```
cargo test -p nexa-app preview::        # 시임·런타임·샘플 E2E
```

패턴: 임시 디렉터리에 `.star` 복사 → `star::load_dir()` → 메타 단정 →
`star::run_preview(플러그인, 픽스처)` → 출력 라인 단정.

## 7. 7단계 — 배포

### 7-1. 배포물 = `.star` 파일 1개

빌드·패키징 과정이 없다. 파일 상단 주석에 **버전·요구 사항을 명기**하는 것을
권장한다(호스트가 파싱하지 않는 문서용 관례):

```python
# my-viewer.star v1.0.0 — Nexa Dir 0.12+ (preview plugin)
```

### 7-2. 사용자 설치 안내(README에 포함할 문구)

```
1) Nexa Dir 폴더의 data\plugins\ 에 my-viewer.star를 복사
   (없으면 폴더를 만드세요. 설치형은 %LOCALAPPDATA%\NexaDir\data\plugins\)
2) Nexa Dir 재시작
3) 대상 파일을 선택하고 하단 도크 미리보기 또는 F3
제거 = 파일 삭제 후 재시작
```

### 7-3. 적용 대상 커스터마이즈(사용자 설정 Override)

플러그인의 `EXTS` 선언은 기본값이고, 사용자는 `data\settings.cfg`(포터블 기준)의
`preview_map` 키로 **확장자→플러그인을 직접 연결**할 수 있다:

```
preview_map=md:markdown|txt:markdown|jpg:builtin.image
```

- 형식: `확장자:플러그인ID` 를 `|` 로 연결. 잘못된 ID는 무시(선언 매치로 폴백).
- 내장 공급자 ID: `builtin.text`(평문) · `builtin.image`(이미지).
- 우선순위: **preview_map > 스크립트 EXTS(파일명 순) > 내장 폴백**.

### 7-4. 사용 여부 켜고 끄기(설정 창 — 07-26)

**설정 → 플러그인** 페이지에 로드된 플러그인이 `NAME (id) — 확장자` 체크박스로
나열된다. **체크 해제 = 사용 안 함**(즉시 적용·영속 — 내장 미리보기로 대체,
파일 삭제 불필요). 저장 키 = `plugins_disabled=id|…`. 내장(`builtin.*`)은 폴백
안전망이라 끌 수 없다. 사용자 안내문에 "임시로 끄려면 설정 → 플러그인" 문구를
포함하면 좋다.

## 8. 8단계 — 운용·트러블슈팅

| 증상 | 원인 | 대응 |
| --- | --- | --- |
| 복사했는데 반응 없음 | 재시작 안 함 / 확장자 미선언 / **설정에서 체크 해제됨** | 재시작 · `EXTS`/`preview_map` · 설정 → 플러그인 체크 확인 |
| `플러그인 오류(id): …` 1줄 | 스크립트 실행 오류 | 메시지의 위치 정보로 수정(다른 파일 미리보기는 정상) |
| 특정 파일만 이상 | 인코딩/이진 파일 | `read_text`는 UTF-8 lossy — 이진 판정이 필요하면 NUL 검사 후 안내 반환 |
| 다른 플러그인이 가로챔 | 같은 EXTS 선언(파일명 순) | 파일명 접두 조정 또는 `preview_map`으로 고정 |
| 표 세로선 어긋남 | CJK 폭 미반영·도크(비모노) | `disp_width()` 사용 · 판정은 **F3 독립 창** 기준 |
| 아주 긴 출력 잘림 | 호스트 상한(1000줄·4096자/줄) | 요약 출력 + "F3로 보세요" 안내 |

**보안 모델**: Starlark엔 파일/네트워크/프로세스 API가 없고, 호스트가 준
`read_text`(대상 파일 한정)만 있다. 신뢰할 수 없는 `.star`라도 임의 파일 접근은
불가하다(실행 시간 상한은 후속 — [TODO X-2](TODO.md)).

---

## 9. 부록

### 9-1. 계약 레퍼런스(한 장)

```python
ID   = "my-viewer"          # 필수 · str · 안정 키(개명 금지)
NAME = "My Viewer"          # 권장 · str · 표시명(없으면 ID)
EXTS = ["xyz"]              # 필수 · list[str] · 적용 확장자 기본값

def preview(file):          # 필수 · file.path/.ext/.size
    ...
    return {"lines": [...]} # 또는 {"image": 경로}
```

### 9-2. 관련 파일

| 역할 | 경로 |
| --- | --- |
| **샘플 프로젝트(참조 구현)** | `samples/markdown-viewer/` — `markdown.star`·`fixtures/sample.md`·README |
| 런타임(로더·호스트 API·격리) | `crates/nexa-app/src/preview/star.rs` |
| 공급자 시임·preview_map 결정 | `crates/nexa-app/src/preview/mod.rs` |
| 독립 미리보기 창(F3) | `crates/nexa-app/src/previewwnd.rs` |
| 샘플 자동 테스트 | `crates/nexa-app/src/preview/sample_tests.rs` |
| 설정 키(preview_map) | `crates/nexa-app/src/config.rs` |
| 설계 SSOT | `docs/09-adr-0004-preview-plugins.md` · `docs/10-decision-record.md`(DR-7·8) |
