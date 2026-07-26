# MarkdownViewerPlugin — 미리보기 플러그인 독립 프로젝트 샘플

Nexa Dir **Starlark 미리보기 플러그인**의 참조 구현이자, 개발자가 **자기 플러그인
프로젝트를 만들 때 그대로 복사해 시작하는 템플릿**이다.
전 과정 가이드(개념·계약·API·테스트·배포)는 → [docs/24 플러그인 개발 가이드](../../docs/24-plugin-dev-guide.md).

## 프로젝트 구조

```
markdown-viewer/
├── markdown.star        # 플러그인 본체 — 단일 파일 = 배포 단위
├── fixtures/
│   └── sample.md        # 로컬 테스트용 예제 문서(전 문법 커버)
└── README.md            # 이 문서
```

## 계약 요약 (스크립트가 정의해야 하는 것)

```python
ID   = "markdown"                          # 안정 식별자(설정 preview_map 키)
NAME = "Markdown Viewer"                   # 표시명
EXTS = ["md", "markdown", "mdown", "mkd"]  # 적용 확장자 기본값(스크립트 내부 지정)

def preview(file):                         # file.path / file.ext / file.size
    src = read_text(65536)                 # 호스트 API — 대상 파일만 읽기
    return {"lines": [...]}                # 또는 {"image": 경로}
```

- **적용 대상 결정**: 설정 `preview_map`(`md:markdown|…`) 오버라이드 → 스크립트
  `EXTS` 선언(플러그인 파일명 순) → 내장 폴백. 스크립트는 기본값만 선언하고,
  최종 결정은 사용자 설정이 갖는다.
- **렌더 기준 캔버스 = 독립 미리보기 창**(F3 · 콘솔 폰트 문자 그리드) — 표·상자는
  `disp_width()`(CJK 2칸)로 정렬한다. 하단 도크는 같은 lines의 축약 뷰.

## 개발 → 테스트 → 배포 (요약)

1. **개발**: `markdown.star` 수정(Starlark = Python 부분집합 — `while`/재귀 금지,
   `for`+`break` 사용).
2. **로컬 테스트**: `data\plugins\` 에 복사 후 앱 재시작 → `fixtures/sample.md`
   선택 → 도크 미리보기 / **F3** 독립 창 확인. 오류는 미리보기에 1줄로 표시된다
   (다른 플러그인·앱은 무영향).
3. **자동 테스트**: 저장소 개발자는 `cargo test -p nexa-app star` —
   이 샘플을 실제 런타임으로 로드·실행하는 통합 테스트가 포함돼 있다.
4. **배포**: `.star` 파일 1개를 배포하면 끝 — 사용자는 `data\plugins\` 에 복사
   (포터블: exe 옆 · 설치형: `%LOCALAPPDATA%\NexaDir\data\plugins\`).
