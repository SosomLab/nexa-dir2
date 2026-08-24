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

- 단계: **포스트 M5 — UX 고도화·배포 채널 정착**. M0~M5 완료(`0.1.0`~`0.6.0`), 최신 릴리스 **`0.18.0`**(2026-08-24 — **압축 파일 미리보기**[그리드 창·암호 입력] + **미리보기 플러그인 2종 동봉 배포**). 상세 → [docs/STATUS.md](docs/STATUS.md).
- 실측(`0.18.0`): exe **3.83MB**(3.65MiB) ≤10 · 임포트 **OS 인박스 21종만** · 유휴 RSS 16.86MB ≤30(07-15 실측) · 100k 첫 렌더 115ms.

## 설치

```powershell
winget install SosomLab.NexaDir.Portable
```

설치형은 `winget install SosomLab.NexaDir`(2026-08-06 등록). 설치 없이 쓰려면
[Releases](https://github.com/SosomLab/nexa-dir2/releases/latest)에서 포터블 exe(또는 zip)를 받아 실행합니다.
자세한 안내는 위키 [설치와 다운로드](https://github.com/SosomLab/nexa-dir2/wiki/설치와-다운로드).

### 미리보기 플러그인 (동봉)

`markdown.wasm`(Markdown·Mermaid) · `archive.wasm`(ISO·ar·cpio 목록) 두 플러그인이
**포터블 zip과 설치본에 함께 들어 있습니다**(exe 옆 `plugins\`). 단일 exe만 받았다면
Releases의 `NexaDir-Plugins-<버전>.zip`을 exe 옆에 풀면 됩니다. 넣거나 지운 뒤
앱을 다시 시작하면 반영되고, 설정 > 플러그인에서 개별로 끌 수 있습니다.

직접 빌드·수정하려면(소스 = [samples/](samples/)):

```powershell
rustup target add wasm32-unknown-unknown   # 최초 1회
pwsh scripts/build-plugins.ps1             # 두 플러그인 빌드 + samples/*/dist 갱신
```

만드는 방법은 [플러그인 개발 가이드](docs/24-plugin-dev-guide.md),
빌드 절차는 [18 §3-1](docs/18-build-and-test.md), 배포 형태는 [21 §5-2](docs/21-distribution.md).

## 문서 — 📖 [문서 홈 (Wiki)](docs/README.md)에서 시작

바로가기: [현황 STATUS](docs/STATUS.md) · [기능·마일스톤](docs/MILESTONES.md) · [진행 DEVLOG](docs/DEVLOG.md) · [비전 00](docs/00-vision.md) · [결정 기록 10](docs/10-decision-record.md) · [이식 메모리](CLAUDE.md)

## 프로젝트 정보 / 라이선스

| 항목 | 내용 |
| --- | --- |
| 조직 | **SosomLab** — <https://sosomlab.com> |
| 원본 저장소 | <https://github.com/SosomLab/nexa-dir> (기능 스펙 원천) |
| 개발자 | Sangyong Bae — kiros33@gmail.com |

**PolyForm Noncommercial 1.0.0** ([LICENSE.md](LICENSE.md) · 한글 [LICENSE.ko.md](LICENSE.ko.md)) — 개인·비상업 무료, 상업 사용은 유료 라이선스(문의 kiros33@sosomlab.com).
