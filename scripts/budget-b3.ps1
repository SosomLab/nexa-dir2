# 예산 B3 게이트 — 임포트 DLL이 OS 인박스 화이트리스트 안인지 검사(docs/18 §6).
# CI(ci.yml)와 로컬(push 전) 공용 단일 출처. 사용: pwsh scripts/budget-b3.ps1 [exe경로]
param([string]$Exe = "target/release/nexa-app.exe")

$ErrorActionPreference = "Stop"

# OS 인박스 DLL 화이트리스트. 항목 추가는 근거(어느 기능이 요구)와 함께 커밋 메시지에 남긴다.
$whitelist = @(
  "kernel32.dll", "user32.dll", "gdi32.dll", "ntdll.dll", "oleaut32.dll",
  "dwrite.dll", "combase.dll",     # ADR-0002 DirectWrite interop
  "ole32.dll",                     # M3-3 휴지통 복원(CoInitializeEx·CoTaskMemFree — COM 초기화/PIDL 해제)
  "bcryptprimitives.dll",          # rust std HashMap RandomState(BCryptGenRandom)
  "shell32.dll",                   # M1-7 셸 아이콘(SHGetFileInfoW)
  "dwmapi.dll",                    # M2-4 다크 타이틀바(DwmSetWindowAttribute)
  "advapi32.dll",                  # M2-4 테마 감지(RegGetValueW)
  "imm32.dll",                     # M2-7 IME 조합 창 위치(ImmSetCompositionWindow)
  "uiautomationcore.dll"           # M2-7 UIA 프로바이더(UiaReturnRawElementProvider)
)

if (-not (Test-Path $Exe)) { throw "exe 없음: $Exe — cargo build --release 먼저" }
$dumpbin = Get-ChildItem "C:\Program Files*\Microsoft Visual Studio" -Recurse -Filter dumpbin.exe -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $dumpbin) { throw "dumpbin 미발견 — B3 게이트 실행 불가(VS Build Tools 필요)" }

$imports = & $dumpbin.FullName /imports $Exe |
  Select-String '^\s+(\S+\.dll)\s*$' | ForEach-Object { $_.Matches[0].Groups[1].Value.ToLower() } | Sort-Object -Unique
Write-Output "임포트: $($imports -join ', ')"

$violations = $imports | Where-Object { $_ -notin $whitelist -and $_ -notlike "api-ms-win-*" }
if ($violations) { throw "예산 B3 위반(OS 인박스 외): $($violations -join ', ')" }
Write-Output "B3 통과 — 전부 화이트리스트 내"
