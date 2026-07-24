# 📖 Nexa Dir — 문서 홈 (Wiki)

> **처음 보는 사람을 위한 길잡이.** 아래 **추천 읽기 순서**를 따라가면 _시발점 → 주요 목표 → 진행 경과 → 상세_ 순으로 이해할 수 있다.
> 프로젝트 한 줄 소개는 루트 [README](../README.md).
>
> **한 장 현황이 급하면 →** [STATUS](STATUS.md) · **최근 무슨 일 →** [DEVLOG](DEVLOG.md) · **기능 현황 →** [MILESTONES](MILESTONES.md).
> **기능 스펙의 원천은 원본 저장소** [`SosomLab/nexa-dir`](https://github.com/SosomLab/nexa-dir)`/docs` — 본 저장소 문서는 중복 서술 대신 원본을 참조한다.

---

## 🧭 추천 읽기 순서

1. **왜 만드나** — [00 비전](00-vision.md) : 원본 실측 한계·예산 목표·차별화
2. **무엇을 만드나** — [05 요구사항](05-requirements.md) : FR(원본 패리티)/NFR(예산 게이트)/제약
3. **핵심 결정** — [10 결정 기록](10-decision-record.md) : DR-1~8 + ADR 색인
4. **어떻게 짓나** — [01 아키텍처](01-architecture.md) → [02 로드맵](02-roadmap.md)
5. **지금 상태** — [STATUS](STATUS.md) → [MILESTONES](MILESTONES.md)
6. **진행 경과** — [DEVLOG](DEVLOG.md)(시간 역순) → 관심 날짜 [journal/](journal/)

## ① 시발점 · 정체성

| 문서 | 내용 |
|---|---|
| [00 비전](00-vision.md) | 비전·예산 목표·경쟁 분석(포터블 관점) |
| [05 요구사항](05-requirements.md) | FR/NFR(예산)/제약/리스크 |
| [10 결정 기록](10-decision-record.md) | ★ DR-1~8·ADR 색인·crate 원장 |
| [06 ADR-0001 스택](06-adr-0001-stack.md) | 올 러스트 + Win32 선정 근거(후보 5안 비교) |
| [CLAUDE.md](../CLAUDE.md) | 이식용 프로젝트 메모리 |

## ② 주요 목표 · 설계

| 문서 | 내용 |
|---|---|
| [02 로드맵](02-roadmap.md) | 단계별 계획 M0~M5 (원안) |
| [MILESTONES](MILESTONES.md) | ★ 기능·마일스톤 현황(✅/🚧/📐/☐) |
| [01 아키텍처](01-architecture.md) | 크레이트 구조·렌더링·스레딩·원본 대응표 |
| [12 패키징](12-packaging-single-exe.md) | 단일 exe·영속 규율·서명 |
| [ctl 컨트롤 라이브러리](ctl/README.md) | ★ Nexa Controls — 컨트롤 14종 문서 색인·공통 규약(판매용) |

## ③ 진행 경과 · 할 일

| 문서 | 내용 |
|---|---|
| [DEVLOG](DEVLOG.md) | ★ 진행 시간순(최신 위) · [journal/](journal/) 일자 상세 |
| [BRANCHES](BRANCHES.md) | 브랜치 생성/작업/병합/삭제 이력 |
| [STATUS](STATUS.md) | ★ 현재 상태 한 장 |
| [TODO](TODO.md) | 목표 순차 백로그(M0-1~) |

## ④ 개발 · 기여

| 문서 | 내용 |
|---|---|
| [11 개발 환경](11-dev-environment.md) | 맥(일상)/Windows(실행·QA)/CI |
| [18 빌드 & 테스트](18-build-and-test.md) | ★ 빌드·테스트·예산 측정 절차(SSOT) |
| [15 개발 방법론](15-dev-methodology.md) | 수직 슬라이스·커밋 규약·단위 백로그 |
| [16 문서·커밋/푸시 규약](16-doc-git-conventions.md) | ★ 문서 4층 체계·커밋/브랜치/푸시 규칙 — **타 프로젝트 이식용 지시문 포함** |

---

> 문서 규약(원본 차용): 진행 기록은 **일자 단위** — 상세 `journal/YYYY-MM-DD.md`(시간 역순), 요약 [DEVLOG](DEVLOG.md)·기능 [MILESTONES](MILESTONES.md). 결정은 [10](10-decision-record.md), 빌드/테스트 SSOT는 [18](18-build-and-test.md).
