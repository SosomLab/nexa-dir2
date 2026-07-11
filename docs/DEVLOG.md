# DEVLOG — 개발 진행 기록 (시간 역순)

> **전체 개발 진행을 시간순(최신이 위)** 으로 관찰하는 단일 기록. 세부 커밋은 [BRANCHES.md](BRANCHES.md)·git 로그.
> 짝 문서: 목적·기능·마일스톤 관점 = **[MILESTONES.md](MILESTONES.md)**.
> 기록 규약(원본 차용): 하루 상세는 `journal/YYYY-MM-DD.md`(내부 시간 역순), 이 문서에는 그날 요약을 맨 위에 추가.

---

## 2026-07-11

- **M0 스캐폴딩·코어 이식·Win32 스켈레톤(`feat/m0-scaffold`)**: 워크스페이스(정적 CRT·lto fat·panic abort) → **nexa-core/vfs/tree 원본 이식**(rlib 직접 링크, 테스트 21+5 green — 인터롭/ABI 계층 소멸) → **Win32 창 스켈레톤**(windows-rs 0.62: 클래스·메시지 루프·WM_PAINT·PerMonitorV2, 비-Windows 스텁) — **맥에서 windows 타깃 cargo check green**(WinUI 시절 불가능하던 UI 코드 사전 검증) → CI(core ubuntu/mac + windows·예산 B2 10MB 게이트·아티팩트). 잔여 M0-7(렌더 스파이크)·M0-8(실기 게이트 실측→`0.1.0`). 상세 [journal/2026-07-11.md](journal/2026-07-11.md).
- **권한 파일 사고 2건·복구**: IDE "Allow always"가 세션 스냅샷으로 `.claude/settings.json`을 덮어씀(2회) → git 커밋본에서 병합 복원, rust/python 전 명령 자동 허용 추가. 규율 확정: **이 파일은 병합만, 덮어쓰기 금지**. 상세 [journal/2026-07-11.md](journal/2026-07-11.md).
- **프로젝트 발족 — 설계 문서 세트(`docs/foundation`)**: 원본 nexa-dir 분석(스택 한계 실측: RSS 60MB+·단일 exe 고위험) → 올 러스트 재구축 결정. 비전/요구(00·05)·아키텍처/ADR-0001(01·06)·결정기록 DR-1~8(10)·환경/패키징/방법론/빌드(11·12·15·18)·로드맵 M0~M5(02)·MILESTONES·TODO(M0-1~M5-3)·운영 문서 작성. 상세 [journal/2026-07-11.md](journal/2026-07-11.md).
- **저장소 초기화**: .gitignore·PolyForm NC 라이선스(원본 계승)·`.claude/settings.json` dev 자동 허용(원본 규약 차용). 원격 `SosomLab/nexa-dir2` 최초 push.
