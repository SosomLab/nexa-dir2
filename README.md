# Nexa Dir — 포터블 단일 exe 네이티브 탐색기

> Portable, single-exe, ultra-lightweight native file explorer for Windows.

**Nexa Dir**는 [Nexa Dir](https://github.com/SosomLab/nexa-dir)(Rust 코어 + WinUI 3)의 기능 경험을
**unmanaged 올 러스트**(Win32 직접 호출 + 커스텀 드로잉)로 재구축하는 프로젝트입니다.

## 설계 원칙

1. **예산이 정체성 (Budget-first)** — 유휴 RSS ≤30MB · 단일 exe ≤10MB · 외부 DLL 0(OS 제공 제외). 마일스톤마다 실측 게이트.
2. **진짜 포터블** — 설치·런타임·레지스트리 없이 exe 1개. 영속물은 exe 옆 `data\`.
3. **성능 최우선 (Native-first)** — 100k 항목 첫 렌더 <150ms·60fps (원본 계승).
4. **원본 패리티** — 인라인 트리+교차 다중 선택(플래그십)·듀얼 패널/탭·파일 조작·터미널을 순차 재현.

## 현재 상태

- 단계: **M0 기반·게이트 진행** — 설계 문서 확정, 스캐폴딩 착수. 상세 → [docs/STATUS.md](docs/STATUS.md).

## 문서 — 📖 [문서 홈 (Wiki)](docs/README.md)에서 시작

바로가기: [현황 STATUS](docs/STATUS.md) · [기능·마일스톤](docs/MILESTONES.md) · [진행 DEVLOG](docs/DEVLOG.md) · [비전 00](docs/00-vision.md) · [결정 기록 10](docs/10-decision-record.md) · [이식 메모리](CLAUDE.md)

## 프로젝트 정보 / 라이선스

| 항목 | 내용 |
| --- | --- |
| 조직 | **SosomLab** — <https://sosomlab.com> |
| 원본 저장소 | <https://github.com/SosomLab/nexa-dir> (기능 스펙 원천) |
| 개발자 | Sangyong Bae — kiros33@gmail.com |

**PolyForm Noncommercial 1.0.0** ([LICENSE.md](LICENSE.md) · 한글 [LICENSE.ko.md](LICENSE.ko.md)) — 개인·비상업 무료, 상업 사용은 유료 라이선스(문의 kiros33@sosomlab.com).
