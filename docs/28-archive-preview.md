# 28 · 압축 파일 미리보기 (X-46) — 설계 SSOT

> 사용자 요청(2026-08-24): *"압축파일 미리보기(그리드 방식, 파일 그리드 재활용 가능),
> 별도창 보기·내장 지원 모두 지원. 암호가 필요하면 입력받되 **입력된 내용은 전달만 하고
> 기록되거나 Plain으로 노출되지 않도록**. 플러그인은 별도 개발 후 최종 파일만 배포하는
> 용도로. 이 미리보기는 프로젝트에 포함시켜 샘플로도 쓸 수 있게. 알려진 압축 포맷을
> 지원하고 **설계상 확장이 용이하도록**."*
>
> 관련: [09 ADR-0004](09-adr-0004-preview-plugins.md)(미리보기 시임) ·
> [25 ADR-0005](25-adr-0005-wasm-plugins.md)(WASM 런타임) ·
> [24 플러그인 가이드](24-plugin-dev-guide.md)(ABI v2 절)

## 1. 한 장 요약

```
파일 선택 ─▶ 확장자 → 공급자 결정(ADR-0004 시임 그대로)
                 │
                 ├─ 플러그인(.wasm, caps=archive) ─┐
                 └─ 내장 builtin.archive ──────────┤
                                                   ▼
                                      Listing(포맷 중립 항목 표)
                                        │                 │
                     하단 도크 = 요약 텍스트        F3/↗ = **그리드 창**
                                                    (NxGrid · 정렬·선택·복사)
                                                          │
                                        암호 필요 시 ── 암호 모달 → 재시도
```

**핵심 원칙 4가지**

1. **압축을 풀지 않는다.** 거의 모든 포맷은 항목 표(중앙 디렉터리·헤더 테이블)가
   평문이라 코덱 없이 목록을 읽을 수 있다. 미리보기의 관심사는 "무엇이 들어 있나"다.
2. **모델은 하나.** 내장·플러그인·도크·그리드가 전부
   [`Listing`/`ArchiveEntry`](../crates/nexa-vfs/src/archive/mod.rs) 하나를 공유한다.
3. **암호는 전달만.** 저장 경로가 코드에 아예 없다(§5).
4. **확장은 파일 1개.** 내장은 `archive/<포맷>.rs` + 레지스트리 한 줄, 외부는
   `.wasm` 하나(앱 재빌드 없음).

## 2. 계층

| 계층 | 위치 | 책임 |
| --- | --- | --- |
| 목록 리더 | [`nexa-vfs/src/archive/`](../crates/nexa-vfs/src/archive/) | 포맷 파싱 → `Listing`. OS 비의존·전 플랫폼 테스트 |
| 공급자 | [`nexa-app/src/preview/archive.rs`](../crates/nexa-app/src/preview/archive.rs) | 시임 장착(`builtin.archive`)·세션 암호·요약 텍스트·이름 디코더 |
| 플러그인 런타임 | [`preview/wasm.rs`](../crates/nexa-app/src/preview/wasm.rs) | ABI v2(`nx_archive`)·호스트 임포트·격리 |
| 그리드 창 | [`archivewnd.rs`](../crates/nexa-app/src/archivewnd.rs) | NxGrid 표시·정렬·복사·암호 재시도 |
| 암호 입력 | [`pwprompt.rs`](../crates/nexa-app/src/pwprompt.rs) | 마스킹 입력·즉시 소거 |
| 비밀 타입 | [`nexa-core/src/secret.rs`](../crates/nexa-core/src/secret.rs) | `Secret`(Debug 마스킹·Drop 소거) |

## 3. 지원 포맷 (내장)

| 포맷 | 확장자 | 읽는 것 | 비고 |
| --- | --- | --- | --- |
| ZIP | zip·zipx·jar·apk·docx·xlsx·epub 등 26종 | 중앙 디렉터리(Zip64·확장 필드) | AES/ZipCrypto = **이름은 보이고 내용만 잠김** · CD 암호화(비트 13) = 암호 필요 · SFX 델타 보정 |
| TAR | tar | 512B 헤더 | ustar/GNU 긴 이름/PAX/base-256 |
| CAB | cab | CFHEADER→CFFILE | 폴더 압축 방식(MSZIP/LZX) 표시 |
| RAR 5 | rar·r00·rev | 블록 헤더(vint) | 항목 암호화 표시 · 암호화 헤더 = 암호 필요 |
| RAR 4 | 〃 | 블록 헤더 | 유니코드 이름은 ASCII 부분 사용(β) |
| 7z | 7z | 시작 헤더 판정만 | 헤더가 LZMA 압축이라 **코덱 필요** → 플러그인 안내. AES 코더 감지 시 암호 요구 |
| 단일 스트림 | gz·tgz·bz2·xz·zst·lz4·lz·lzma·z 등 | 헤더/꼬리 | 항목 1건(gzip은 원 이름·mtime·ISIZE까지) |

`.tar.gz`처럼 **압축된 tar**은 안쪽 목록을 보려면 압축을 풀어야 하므로 tar 1건으로
보인다(전체 목록은 플러그인 몫 — §6).

### 3-1. 새 내장 포맷 추가 절차

```
1) crates/nexa-vfs/src/archive/myfmt.rs   — impl ArchiveFormat { id/label/exts/sniff/list }
2) crates/nexa-vfs/src/archive/mod.rs     — FORMATS 배열에 &myfmt::MyFmt 한 줄
```

확장자 선언·판정 순서·미리보기 라우팅·그리드 컬럼·설정 노출은 **레지스트리에서
파생**되므로 그 외에는 고칠 곳이 없다(`BuiltinArchive::new()`가 `all_exts()`를 읽는다).

## 4. 표시

- **하단 도크(축약)**: 포맷·파일/폴더 수 · 원본/압축 합계와 절감률 · 암호/솔리드/
  분할/절단 표시 · 주석 · 앞 60개 항목 · "F3 · ↗로 그리드" 안내.
- **그리드 창(별도 창)**: [`ctl::grid`](../crates/nexa-app/src/ctl/grid.rs) **NxGrid**
  재사용 — 파일 그리드와 같은 규약(헤더 리사이즈·정렬 3상태·다중 선택·오버레이
  스크롤바). 컬럼 = 이름·경로·크기·압축 크기·압축률·방식·수정한 날짜·표시.
  정렬은 **수치 기준**(크기·시각·압축률), `Esc` 닫기, `Ctrl+C` 선택 행 TSV 복사.
- **시각 이중 보정 방지**: DOS 시각 계열(zip·cab·rar4)은 시간대 정보가 없어 값이
  곧 현지 벽시계다 → 보정 없이 표시. Unix epoch 계열(tar·rar5·gzip·zip NTFS 확장)만
  시간대 보정(`ArchiveEntry.time_is_local`).
- **이름 코드페이지**: UTF-8 플래그가 없는 구형 zip 이름은 호스트 디코더
  (`MultiByteToWideChar(CP_ACP)`)로 해석 → CP949 한글 이름 정상 표시. 미주입
  환경(비Windows·테스트)은 UTF-8 → CP437 폴백.

## 5. 암호 취급 (사용자 지시의 핵심)

| 지시 | 구현 |
| --- | --- |
| 기록 금지 | 저장 경로가 **코드에 없다**. 설정·세션·로그·창 제목 어디에도 쓰지 않는다(토큰용 DPAPI 경로 [`secret.rs`](../crates/nexa-app/src/secret.rs)와 의도적으로 분리 — 암호는 **세션 한정**) |
| 평문 노출 금지 | 입력은 마스킹 EDIT(복사·잘라내기 불가) · `Secret`의 `Debug`는 `Secret(***)`(길이도 비노출) · `Display`/`AsRef<str>`/직렬화 없음 |
| 전달만 | 호출 직전 활성 슬롯에 주입 → 호출 종료 시 슬롯 비움. 내장·플러그인이 같은 경로로 받는다 |
| 잔상 제거 | 회수 즉시 `Secret`으로 **이동** + 경유 UTF-16 버퍼·EDIT 내용·되돌리기 버퍼 소거. `Secret`은 Drop에서 `write_volatile` 0 덮기 |
| 재입력 최소화 | 성공한 암호만 **메모리 캐시**(프로세스 종료 = 소멸). 틀리면 즉시 폐기하고 다시 묻는다 |

플러그인에 넘길 때도 원칙은 같다 — 호스트가 게스트 메모리에 1회 복사하고, 인스턴스는
호출 종료와 함께 폐기된다(선형 메모리 소멸). 호스트 임시 사본도 기록 직후 소거한다.

## 6. 플러그인 ABI v2 (하위 호환)

`nx_meta()`의 **4번째 줄**에 `archive`를 선언하면 압축 목록 공급자가 된다.

```
nx_meta()    → "id\n표시명\n확장자들\narchive"
nx_archive() → "archive\n<표시명>\t<플래그>\n<항목>…"   | "password" | "error\n<사유>"
                항목 = 경로 ⇥ 원본 ⇥ 압축 ⇥ 시각(Unix 초) ⇥ 속성 ⇥ 방식
                속성 = dir,enc,utc,unsafe   플래그 = solid,multivolume,truncated
```

호스트 임포트 추가: `file_size()` · `read_at(off, ptr, cap)`(1회 4MB) ·
`password(ptr, cap)`(없으면 `-1` → 게스트가 `password` 반환으로 요청).
격리는 그대로다(fuel 2억·메모리 64MB·모듈 8MB·**대상 파일 1개** 샌드박스).

참조 구현 = [`samples/archive-viewer-wasm`](../samples/archive-viewer-wasm/) —
**ISO 9660(Joliet)·ar·cpio**(내장이 다루지 않는 3종)를 31KB `.wasm` 하나로 붙인다.
배포는 `data\plugins\`에 파일 복사 + 앱 재시작이 전부다(앱 재빌드 없음).

## 7. 상한·안전

- 항목 50,000(초과 = `truncated` 표시) · 이름 4KB · 1회 읽기 64MB(플러그인 4MB).
- 오프셋은 전부 범위 검사 후 접근 — 손상 파일에 **패닉 없음**(`Option` 경로).
- **경로 탈출 차단**: `..`·절대 경로·드라이브 문자는 정규화로 흡수하고 `unsafe`(위험
  경로)로 표시한다. 미리보기는 읽기 전용이라 추출 위험은 없지만, 사용자가 그 아카이브의
  성격을 알아야 한다.
- **읽기 시점**: 목록은 선택 시(도크가 미리보기 종류일 때)와 창 열기 때 **UI 스레드에서
  동기 파싱**한다. 중앙 디렉터리 한 번 읽기라 로컬에서는 즉시지만, **UNC·클라우드의 매우
  큰 아카이브**는 체감이 생길 수 있다(실기 QA 관찰 대상 — 필요 시 워커 이관 또는 도크 전용
  항목 상한이 다음 카드).
- 실패는 사용자 행동으로 번역: 암호 필요 → 입력창 · 코덱 필요 → 플러그인 안내 ·
  손상/입출력 → 사유 1줄. 어떤 경우도 앱·다른 미리보기에 영향이 없다.

## 8. 범위 밖(후속 후보)

- **항목 내용 미리보기**(zip 안 텍스트 열기) — inflate 구현 또는 플러그인 필요.
- **추출·압축**(파일 관리 기능) — 미리보기와 분리된 별도 기능.
- 아카이브를 폴더처럼 **탐색**(VFS 공급자) — 목록 계층을 nexa-vfs에 둔 이유가 이
  확장을 열어 두기 위해서다.
- 7z 평문 헤더 파싱 · RAR4 유니코드 이름 완전 해석 · 그리드 트리 보기(폴더 접기).
