# DEVLOG — 개발 진행 기록 (시간 역순)

> **전체 개발 진행을 시간순(최신이 위)** 으로 관찰하는 단일 기록. 세부 커밋은 [BRANCHES.md](BRANCHES.md)·git 로그.
> 짝 문서: 목적·기능·마일스톤 관점 = **[MILESTONES.md](MILESTONES.md)**.
> 기록 규약(원본 차용): 하루 상세는 `journal/YYYY-MM-DD.md`(내부 시간 역순), 이 문서에는 그날 요약을 맨 위에 추가.

---

## 2026-07-12

- **M1-2 ADR-0002 렌더링 확정(`feat/m1-adr0002-render`)**: DirectWrite GDI interop 백엔드(`dw.rs` — `IDWriteBitmapRenderTarget`+`IDWriteTextLayout`+커스텀 `IDWriteTextRenderer`) 구현, F2 전환·F3 200프레임 벤치 하네스로 **같은 창 실측 비교**. 결과: DW가 GDI 대비 **−28%**(4,373µs vs 6,072µs, 캐시 미적용 상한치)·RSS +4.1MB(17.4MB)·exe 0.23MB — 전 게이트 내. **DW interop 채택**([07 ADR-0002](07-adr-0002-rendering.md) Accepted), 기본 백엔드 전환. `DrawCtx` 어휘는 무변경(추상 검증). GDI 경로·비교 하네스는 M1-3에서 제거 예정. 상세 [journal/2026-07-12.md](journal/2026-07-12.md).

## 2026-07-11

- **M1-1 nexa-gui 크레이트 분리(`feat/m1-gui`)**: **플랫폼 중립(의존 0)** GUI 기반 — 위젯 trait+무효화 수집(교차 rect 병합)·입력 이벤트(분수 노치 휠 누적기)·시맨틱 테마 토큰(**원본 docs/39 §4 차용**, 다크 기본)·`VirtualRows` 가상화 리스트(M0-7 스크롤 로직 이식, `RowSource` 추상 — M1-3에서 nexa-tree 배선). nexa-app은 WM_* 번역·더블 버퍼·GDI `DrawCtx` 백엔드(`gdi.rs`, ADR-0002 확정까지)로 축소. 테스트 43 green(신규 17)·clippy 0·실기 기동 확인(exe 0.21MB·RSS 12.4MB). 상세 [journal/2026-07-11.md](journal/2026-07-11.md).
- **M0-8 게이트 실측 · M0 완료(`0.1.0`)**: Windows 실기(Win11 26200)에서 DR-2 예산 전 항목 통과 — **B1 유휴 RSS 13.22MB**(100k 행 창·3회 중앙값·Private 1.70MB) · **B2 exe 0.20MB**(214KB, 정적 CRT+lto+strip) · **B3 임포트 6종 전부 OS 인박스**(user32·kernel32·gdi32·ntdll·oleaut32·api-ms-win-core-synch). 실기 테스트 26 green. CI B3를 목록 가시화→**화이트리스트 fail 게이트**로 강화. 원본 대비: 유휴 RSS 60MB+ → 13MB(1/4 이하)·배포 64MB → 0.2MB. 다음 M1-1(`nexa-gui`)·M1-2(ADR-0002). 상세 [journal/2026-07-11.md](journal/2026-07-11.md).
- **M0-7 GDI 렌더 스파이크(`feat/m0-render-spike`)**: WM_PAINT **더블 버퍼**(메모리 DC 캐시·ERASEBKGND 생략) + 합성 **100k 행 중 가시 행만** ExtTextOutW(ETO_OPAQUE, 교대 음영) — 프레임 비용이 창 높이에만 비례함을 코드로 실증(M1 가상 리스트 전제). 휠(분수 노치 누적)·키보드(↑↓/PgUp/PgDn/Home/End) 스크롤·WM_DPICHANGED 대응. 맥에서 windows 타깃 check·clippy green. 잔여 M0-8(실기 게이트 실측→`0.1.0`). 상세 [journal/2026-07-11.md](journal/2026-07-11.md).
- **M0 스캐폴딩·코어 이식·Win32 스켈레톤(`feat/m0-scaffold`)**: 워크스페이스(정적 CRT·lto fat·panic abort) → **nexa-core/vfs/tree 원본 이식**(rlib 직접 링크, 테스트 21+5 green — 인터롭/ABI 계층 소멸) → **Win32 창 스켈레톤**(windows-rs 0.62: 클래스·메시지 루프·WM_PAINT·PerMonitorV2, 비-Windows 스텁) — **맥에서 windows 타깃 cargo check green**(WinUI 시절 불가능하던 UI 코드 사전 검증) → CI(core ubuntu/mac + windows·예산 B2 10MB 게이트·아티팩트). 잔여 M0-7(렌더 스파이크)·M0-8(실기 게이트 실측→`0.1.0`). 상세 [journal/2026-07-11.md](journal/2026-07-11.md).
- **권한 파일 사고 2건·복구**: IDE "Allow always"가 세션 스냅샷으로 `.claude/settings.json`을 덮어씀(2회) → git 커밋본에서 병합 복원, rust/python 전 명령 자동 허용 추가. 규율 확정: **이 파일은 병합만, 덮어쓰기 금지**. 상세 [journal/2026-07-11.md](journal/2026-07-11.md).
- **프로젝트 발족 — 설계 문서 세트(`docs/foundation`)**: 원본 nexa-dir 분석(스택 한계 실측: RSS 60MB+·단일 exe 고위험) → 올 러스트 재구축 결정. 비전/요구(00·05)·아키텍처/ADR-0001(01·06)·결정기록 DR-1~8(10)·환경/패키징/방법론/빌드(11·12·15·18)·로드맵 M0~M5(02)·MILESTONES·TODO(M0-1~M5-3)·운영 문서 작성. 상세 [journal/2026-07-11.md](journal/2026-07-11.md).
- **저장소 초기화**: .gitignore·PolyForm NC 라이선스(원본 계승)·`.claude/settings.json` dev 자동 허용(원본 규약 차용). 원격 `SosomLab/nexa-dir2` 최초 push.
