# DEVLOG — 개발 진행 기록 (시간 역순)

> **전체 개발 진행을 시간순(최신이 위)** 으로 관찰하는 단일 기록. 세부 커밋은 [BRANCHES.md](BRANCHES.md)·git 로그.
> 짝 문서: 목적·기능·마일스톤 관점 = **[MILESTONES.md](MILESTONES.md)**.
> 기록 규약(원본 차용): 하루 상세는 `journal/YYYY-MM-DD.md`(내부 시간 역순), 이 문서에는 그날 요약을 맨 위에 추가.

---

## 2026-07-11

- **프로젝트 발족 — 설계 문서 세트(`docs/foundation`)**: 원본 nexa-dir 분석(스택 한계 실측: RSS 60MB+·단일 exe 고위험) → 올 러스트 재구축 결정. 비전/요구(00·05)·아키텍처/ADR-0001(01·06)·결정기록 DR-1~8(10)·환경/패키징/방법론/빌드(11·12·15·18)·로드맵 M0~M5(02)·MILESTONES·TODO(M0-1~M5-3)·운영 문서 작성. 상세 [journal/2026-07-11.md](journal/2026-07-11.md).
- **저장소 초기화**: .gitignore·PolyForm NC 라이선스(원본 계승)·`.claude/settings.json` dev 자동 허용(원본 규약 차용). 원격 `SosomLab/nexa-dir2` 최초 push.
