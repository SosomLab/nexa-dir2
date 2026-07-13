# 09 · ADR-0004 — 미리보기 플러그인 시스템 (Starlark 임베드, DR-7 개정)

- 상태: **Accepted** (2026-07-14, 사용자 결정) · 관련: [10 결정](10-decision-record.md) DR-7·DR-8 · 원본 [docs/35 미리보기](../../nexa-dir/docs/35-preview-system.md)·[docs/36 플러그인 개발](../../nexa-dir/docs/36-plugin-development.md)
- 대상: 미리보기 확장(예: **EXIF 정보 미리보기**) — 향후 다른 확장점(컨텍스트 메뉴 항목 등)으로 일반화 여지.

## 맥락 (결정 변경)

DR-7은 원본의 .NET 플러그인 SDK를 **비이관**하고 내장 미리보기로 대체했다(관리 런타임 0·단일
exe 원칙과 네이티브/관리 DLL 로딩이 상충). M4-2에서 내장 텍스트·WIC 이미지 미리보기가 구현됐다.

**사용자 요구(2026-07-14)**: "EXIF 포맷 정보를 보여주는 미리보기를 SDK 기반으로 Python처럼
개발해 미리보기에 표시"되는 플러그인 도입 — **향후 내장이 아닌 Starlark 기반 플러그인**으로 전환.

## 결정

**Starlark**(Python 문법의 결정적·샌드박스 부분집합 — Bazel/Buck2 계열) 인터프리터를
**임베드**해 미리보기 공급자를 플러그인화한다.

### 채택 근거 (대안 비교)

| 안 | 판정 | 이유 |
|---|---|---|
| **A. Starlark 임베드**(`starlark` crate — Meta, Apache-2.0) | ★ **채택** | Python 유사 문법(사용자 개발 경험 그대로)·**샌드박스**(파일/네트워크 기본 차단 — 호스트가 준 API만)·결정적 실행·정적 링크로 **단일 exe 유지**(DR-3)·퍼미시브(DR-6) |
| B. .NET SDK 이관(원본 방식) | 기각 | 관리 런타임 필요 — DR-1(unmanaged 올 러스트)·단일 exe와 상충(기존 DR-7 근거 유지) |
| C. 실제 CPython 임베드 | 기각 | 런타임 동봉(수십 MB)·B2 예산(exe ≤10MB) 초과·샌드박스 부재 |
| D. WASM(기존 보류안) | 보류 유지 | 샌드박스는 우수하나 "Python으로 개발" 요구와 거리(툴체인 필요)·런타임 크기. Starlark로 부족해지면 재검토 |

### 플러그인 계약 (미리보기 공급자)

- 위치: `data\plugins\*.star`(포터블 — exe 옆, DR-3. 파일 추가/수정만으로 배포·재빌드 불요).
- 각 플러그인은 **메타(선언) + 함수**를 정의:

```python
# data\plugins\exif.star — 예시: EXIF 정보 미리보기
ID = "exif"                      # 안정 식별자(설정 매핑·콤보 표기 키)
NAME = "EXIF 정보"               # 콤보/설정 표시명
EXTS = ["jpg", "jpeg", "tiff"]   # 기본 내장 지원 확장자(선언적 — 사용자 요구 ①)

def can_preview(file):
    return True                  # 선택: EXTS 매치 후 동적 정밀 판정(생략 가능)

def preview(file):
    # file: 호스트가 주입하는 핸들 객체(아래 호스트 API)
    tags = file.exif()           # 호스트 내장 EXIF 파서(순수 러스트) 호출
    if not tags:
        return {"lines": ["EXIF 없음"]}
    lines = ["{}: {}".format(k, v) for k, v in tags.items()]
    return {"lines": lines}      # 또는 {"image": path} / {"kv": {...}}
```

- **반환 규약**: `{"lines": [str]}`(텍스트 라인) · `{"image": path}`(호스트 WIC 렌더 위임) ·
  `{"kv": {k: v}}`(정보 표) 중 하나. 도크가 해석해 표시(M4-2 InfoDock 재사용).
- **호스트 API**(샌드박스 표면 — 이것 외 I/O 불가): `file.path`·`file.ext`·`file.size`·
  `file.read(n)`(상한 강제, bytes)·`file.exif()`(호스트 제공 파서 — 플러그인이 바이너리
  파싱을 직접 하지 않도록 자주 쓰는 포맷은 호스트가 헬퍼로 제공, 필요 시 확장).

### 확장자 매핑과 공급자 선택 (사용자 요구 2026-07-14)

1. **기본 매핑 = 플러그인 선언(`EXTS`)**: 로드 시 확장자→후보 목록 레지스트리 구성.
2. **설정 오버라이드**: settings에 `preview_map`(확장자→플러그인 `ID`) — 특정 확장자를
   특정 플러그인(또는 내장 `builtin.text`/`builtin.image`)에 **직접 연결**. 선언보다 우선.
   1차=설정 파일 키(`preview_map=jpg:exif|txt:builtin.text` 형태), 설정 UI는 후속(M5).
3. **다중 매치 콤보**: 한 확장자에 후보가 2개 이상이면 **도크 미리보기 우상단에 소형
   콤보박스**(현재 공급자명 ▾ — 클릭 시 후보 드롭다운)로 즉석 전환. 선택은 확장자별로
   기억(`preview_map`에 반영 = 설정 오버라이드와 같은 저장소·재실행 유지).
4. **우선순위**: 설정 오버라이드 > 콤보 선택(=동일 저장소) > 선언 매치(로드 순) > 내장 폴백
   (텍스트·WIC 이미지 — 플러그인 0개여도 기본 동작 보장. "향후 내장 대신"은 기본 공급자까지
   `.star`로 옮길 수 있는 구조를 뜻하며, 폴백 안전망은 존치).

- **격리·예산**: 실행 시간 상한(예: 50ms)·연료(스텝) 제한·오류 시 해당 플러그인만 비활성
  (도크에 오류 1줄). 로딩은 미리보기 최초 사용 시 지연(상주 RSS 영향 0 — B1).

### 단계 (백로그 [TODO §7 X-2](TODO.md))

1. **S1 공급자 시임**: M4-2 `preview_content`를 `PreviewProvider` 계약 뒤로 리팩터(내장
   텍스트/이미지 = 기본 공급자 `builtin.text`/`builtin.image`) + 확장자→후보 레지스트리 —
   플러그인 도입 전 구조 선행(러스트만, crate 0).
2. **S2 Starlark 런타임**: `starlark` crate 도입(DR-8 원장 등재 — 의존 트리·버전·B2 증가분
   실측 후 확정)·`data\plugins\*.star` 로딩(ID/NAME/EXTS 메타)·`preview` 호출 배선.
3. **S3 호스트 API + 매핑**: file 핸들(read 상한·exif 헬퍼[순수 러스트 — TIFF/JPEG APP1])·
   settings `preview_map` 오버라이드·**도크 우상단 공급자 콤보**(다중 매치 시 — 선택 영속).
4. **S4 예시 플러그인**: `exif.star` 동봉 + 원본 docs/36 대응 개발 가이드 문서.

## 결과

- DR-7 개정: "플러그인 비이관" → "**Starlark 플러그인 도입**(.NET SDK만 비이관 유지)".
- DR-8 예외 승인 대상: `starlark` crate(+의존) — 도입 시점(S2)에 원장 확정.
- exe 예산(B2 ≤10MB): starlark-rust 정적 링크 증가분(수 MB 예상)은 S2에서 실측 후 판단 —
  초과 시 경량 대안(자체 미니 인터프리터·WASM 재검토) 결정.
