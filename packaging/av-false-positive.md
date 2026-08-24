# 안티바이러스 오탐 신고 — 제출 절차·문안 (2026-08-24 신설)

무서명(DR-3) 배포의 구조적 부작용인 **머신러닝 휴리스틱 오탐**(`Trojan:Win32/Wacatac.*!ml`
등)이 났을 때 쓰는 상시 절차. 배경·실측은 [12 §4-2](../docs/12-packaging-single-exe.md).

> **원칙**: 오탐은 릴리스마다 재발할 수 있다(해시가 바뀌면 프리밸런스가 0에서 다시 시작).
> 사용자에게는 **포터블 채널**을 먼저 안내하고(설치형만 걸린다), 발행자는 아래 신고로
> 클라우드 판정을 정정한다.

## 1. Microsoft (Defender) — 우선 채널

**제출처**: <https://www.microsoft.com/en-us/wdsi/filesubmission>
(Microsoft 계정 로그인 필요 — 브라우저에서 수행)

| 항목 | 값 |
| --- | --- |
| Submission type | **Software developer** |
| Product | Microsoft Defender Antivirus |
| Detection name | 격리 알림에 표시된 이름(예: `Trojan:Win32/Wacatac.C!ml`) |
| File / URL | 릴리스 자산 파일 업로드 + 다운로드 URL |
| Definition version | `Get-MpComputerStatus` 의 `AntivirusSignatureVersion` |
| Do you believe this is | **Incorrectly detected (false positive)** |

**Detection details 붙여넣기 문안**(영문 — 버전·해시만 바꿔 재사용):

```text
This file is the official installer of Nexa Dir, an open-source-published Windows file
explorer released by SosomLab. It is a false positive from the ML heuristic ("!ml").

Product   : Nexa Dir <VERSION> (installer, Inno Setup 6)
File      : NexaDir-Setup-<VERSION>.exe
SHA-256   : <SHA256>
Download  : https://github.com/SosomLab/nexa-dir2/releases/download/<VERSION>/NexaDir-Setup-<VERSION>.exe
Publisher : SosomLab (Sangyong Bae, kiros33@gmail.com)
Homepage  : https://sosomlab.com  ·  Source: https://github.com/SosomLab/nexa-dir2

Why this is a false positive:
- The binary is built from public source by a public GitHub Actions workflow on the
  release tag (build log: https://github.com/SosomLab/nexa-dir2/actions). Nothing is
  fetched or executed at build time beyond crates.io dependencies pinned in Cargo.lock.
- The application is a plain Win32 desktop app written in Rust. It imports only OS inbox
  DLLs (21 of them; enforced by a CI gate) and contains no network beaconing, no
  persistence mechanism, no process injection and no self-modifying code.
- The same payload shipped as a portable single exe is NOT detected; only the Inno Setup
  installer wrapper is flagged, which is the classic low-prevalence unsigned-installer
  heuristic pattern.
- The product is distributed through winget (SosomLab.NexaDir, SosomLab.NexaDir.Portable),
  where the same binaries passed Microsoft's winget-pkgs validation pipeline.
- The installer is unsigned because no code-signing path is currently available to us
  (Azure Artifact Signing is not offered in our region); we are not evading signing.

Please re-evaluate the cloud verdict for this hash.
```

## 2. 그 밖의 엔진

- **VirusTotal**로 범위 확인(업로드 불요 — 해시 검색):
  `https://www.virustotal.com/gui/file/<SHA256>`
- 다른 엔진이 걸리면 각 벤더의 false positive 폼에 같은 문안으로 제출한다
  (Chocolatey 모더레이션이 막힌 것도 같은 계열의 스캔 플래그 — [21 §7](../docs/21-distribution.md)).

## 3. 신고 후

- 판정 정정은 보통 수 시간~수 일. **재검사로 확인**:
  `& "$env:ProgramFiles\Windows Defender\MpCmdRun.exe" -Scan -ScanType 3 -File <경로>`
- 결과(제출일·회신·정정 여부)는 그 릴리스의 journal에 한 줄 남긴다.
- 이미 격리된 사용자 파일 복구(관리자):

  ```powershell
  & "$env:ProgramFiles\Windows Defender\MpCmdRun.exe" -Restore -FilePath "<경로>"
  ```

## 4. 릴리스 체크리스트 편입

릴리스 직후 **설치형 자산을 한 번 내려받아 스캔**하고(브라우저 다운로드 맥락 재현),
격리되면 이 문서의 §1을 그대로 수행한다 — [21 §6 검증 체크리스트](../docs/21-distribution.md).
