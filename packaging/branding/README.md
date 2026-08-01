# 클라우드 콘솔 브랜딩 자산

3사 개발자 콘솔(Dropbox·Microsoft Entra·Google Cloud)에 입력하는 **앱 정보와 아이콘**을
한곳에 모아 둔다. 콘솔마다 문구를 다시 짜면 표기가 갈리므로 **여기가 SSOT**다.

## 아이콘

| 파일 | 크기 | 용도 |
| --- | --- | --- |
| `nexa-dir-64.png` | 64×64 | Dropbox App icon (작은 쪽) |
| `nexa-dir-256.png` | 256×256 | Dropbox App icon (큰 쪽) · Google 로고(120×120로 축소 업로드 가능) |

출처는 `crates/nexa-app/assets/nexa-dir.ico`이며, 해당 크기의 프레임을 **재인코딩 없이
그대로 추출**했다(투명도·품질 무손실). 아이콘을 바꿀 때는 `.ico`를 먼저 고치고 다시 추출한다.

```powershell
# .ico에서 64/256 프레임을 PNG로 추출(무손실 — ICO 안에 PNG로 저장돼 있다)
$bytes = [System.IO.File]::ReadAllBytes("crates\nexa-app\assets\nexa-dir.ico")
$count = [BitConverter]::ToUInt16($bytes, 4)
for ($i = 0; $i -lt $count; $i++) {
  $o = 6 + $i * 16; $w = $bytes[$o]; if ($w -eq 0) { $w = 256 }
  if ($w -ne 64 -and $w -ne 256) { continue }
  $sz = [BitConverter]::ToUInt32($bytes, $o+8); $off = [BitConverter]::ToUInt32($bytes, $o+12)
  [System.IO.File]::WriteAllBytes("packaging\branding\nexa-dir-$w.png", $bytes[$off..($off+$sz-1)])
}
```

---

## 공통 문구

| 항목 | 값 |
| --- | --- |
| App name | `Nexa Dir` |
| Publisher / 개발자 | `SosomLab` |
| App website | `https://sosomlab.com/apps/nexa-dir/` |
| Support email | `kiros33@gmail.com` |
| Repository | `https://github.com/SosomLab/nexa-dir2` |

### Description (영문 — 동의 화면 노출용)

동의 화면에 뜨는 문구다. **무엇을 하는 앱인지 + 왜 이 권한이 필요한지**를 한 번에
읽히게 쓴다(심사자가 보는 것도 같은 문구다).

> Nexa Dir is an ultra-lightweight portable file explorer for Windows — a single
> executable that runs without installation. Connecting your Dropbox account lets you
> browse, open, upload, download and organize your Dropbox files directly inside a
> dual-pane explorer, side by side with your local drives, without syncing anything to
> your disk. Files are transferred only when you ask for them, and your credentials are
> never seen or stored by the app: sign-in happens in your browser, and the resulting
> token is encrypted on your own machine.

**짧은 판**(길이 제한이 있는 칸용):

> A portable, ultra-lightweight Windows file explorer. Browse, upload and download your
> Dropbox files in a dual-pane view without syncing them to disk.

다른 서비스에 쓸 때는 `Dropbox`만 해당 서비스명으로 바꾼다.

### 요청 권한 사유 (심사 답변용)

| 권한 | 왜 필요한가 |
| --- | --- |
| `files.metadata.read` | 폴더 목록 표시(이름·크기·수정일) — 탐색기의 기본 화면 |
| `files.content.read` | 파일 열기·로컬로 내려받기 |
| `files.metadata.write` | 이름 변경·폴더 만들기·이동 |
| `files.content.write` | 로컬 파일 업로드·덮어쓰기 |
| `account_info.read` | 연결 목록에 **어느 계정인지** 표시(이메일) — 다계정 구분에 필요 |

---

## 콘솔별 입력 위치

### Dropbox — App Console → Branding

| 칸 | 값 |
| --- | --- |
| App name | `Nexa Dir` |
| Publisher | `SosomLab` |
| Description | 위 영문 Description |
| App website | `https://sosomlab.com/apps/nexa-dir/` |
| Privacy policy URL | **프로덕션 승인 전 준비 필요**(아래 참고) |
| App icons | `nexa-dir-64.png` · `nexa-dir-256.png` |

> **아이콘 업로드 방식이 특이하다**: `Choose from Dropbox` 버튼이라 **먼저 Dropbox
> 계정에 PNG를 올려 둔 뒤** 거기서 고른다. 로컬 파일 선택 대화상자가 아니다.

> **Privacy policy URL은 프로덕션 승인 신청 전에 채워야 한다.** 앱이 사용자 데이터를
> 서버로 보내지 않으므로 내용은 짧아도 되지만(로컬 전용·토큰은 사용자 PC에 DPAPI 암호화),
> URL 자체는 있어야 한다. → [TODO](../../docs/TODO.md) X-37 잔여 ①

### Microsoft Entra — 앱 등록 → 브랜딩 및 속성

| 칸 | 값 |
| --- | --- |
| 이름 | `Nexa Dir` |
| 홈페이지 URL | `https://sosomlab.com/apps/nexa-dir/` |
| 로고 | `nexa-dir-256.png` |
| 게시자 도메인 | `sosomlab.com`(DNS 검증 필요 — 게시자 확인의 선행 조건) |

### Google Cloud — Google 인증 플랫폼 → 브랜딩

| 칸 | 값 |
| --- | --- |
| 앱 이름 | `Nexa Dir` |
| 사용자 지원 이메일 | `kiros33@gmail.com` |
| 앱 로고 | `nexa-dir-256.png`(120×120으로 자동 축소) |
| 애플리케이션 홈페이지 | `https://sosomlab.com/apps/nexa-dir/` |
| 승인된 도메인 | `sosomlab.com` |

> 로고를 올리면 **인증(verification)이 트리거**된다. 테스트 모드로만 쓸 계획이라면
> 로고를 비워 두는 편이 심사 대기를 피할 수 있다.

---

## 관련 문서

- [ADR-0006 §2-4](../../docs/27-adr-0006-cloud-oauth.md) — client_id 소유 모델·배포 인원 한도
- [위키 · 클라우드](../../docs/wiki/기능-클라우드.md) — 사용자용 발급 가이드
