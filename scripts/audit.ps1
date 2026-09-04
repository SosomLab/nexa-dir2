# 정규 점검 하네스 — docs/29-audit-checklist.md의 자동화 가능한 항목을 한 번에 실행한다.
# 사용: pwsh scripts/audit.ps1 [-Quick] [-Coverage] [-Idle] [-BigDir <경로>]
#   -Quick     : 테스트·clippy·비Windows check·PE·B2·B3만(벤치·커버리지·유휴 측정 생략)
#   -Coverage  : cargo-llvm-cov 라인 커버리지(설치돼 있을 때만 — 없으면 skip 표기)
#   -Idle      : release exe를 10k 폴더로 기동해 60초 유휴 후 WorkingSet/Private/CPU 실측(B1 약식)
#   -BigDir    : 열거 벤치 대상 폴더(기본 = System32). 10k/100k 폴더가 있으면 그 경로를 준다.
# 출력: 항목별 PASS/FAIL/SKIP 한 줄 + 마지막 요약. 종료 코드 = FAIL 개수.
#   -OutDir    : 결과 폴더(기본 docs/audit/<yyyyMMdd-HHmmss>/ — 회차별 원본 보관 규약, docs/29 §0).
#                summary.md(판정 표)·audit.log(원문)를 쓴다. -NoSave면 저장 안 함.
param(
  [switch]$Quick,
  [switch]$Coverage,
  [switch]$Idle,
  [string]$BigDir = "$env:SystemRoot\System32",
  [string]$Exe = "target/release/nexa-app.exe",
  [string]$OutDir = "",
  [switch]$NoSave
)
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
if (-not $OutDir) { $OutDir = "docs/audit/$stamp" }
$ErrorActionPreference = "Continue"
$results = New-Object System.Collections.Generic.List[object]
function Rec($id, $name, $status, $detail) {
  $results.Add([pscustomobject]@{ id = $id; name = $name; status = $status; detail = $detail })
  $c = switch ($status) { 'PASS' { 'Green' } 'FAIL' { 'Red' } default { 'Yellow' } }
  Write-Host ("[{0,-4}] {1,-6} {2,-34} {3}" -f $status, $id, $name, $detail) -ForegroundColor $c
}
function RunCapture($cmd) { $out = & pwsh -NoProfile -Command $cmd 2>&1 | Out-String; return $out }

Push-Location (Split-Path $PSScriptRoot -Parent)
try {
  # ── T-1 테스트 ────────────────────────────────────────────────────────────
  $t = cargo test --workspace 2>&1 | Out-String
  $pass = ([regex]::Matches($t, 'test result: ok\. (\d+) passed') | ForEach-Object { [int]$_.Groups[1].Value } | Measure-Object -Sum).Sum
  $fail = ([regex]::Matches($t, '(\d+) failed') | ForEach-Object { [int]$_.Groups[1].Value } | Measure-Object -Sum).Sum
  Rec 'T-1' 'cargo test --workspace' ($(if ($fail -eq 0 -and $pass -gt 0) { 'PASS' } else { 'FAIL' })) "passed=$pass failed=$fail"

  # ── T-2 clippy 0 ───────────────────────────────────────────────────────────
  $c = cargo clippy --workspace --all-targets 2>&1 | Out-String
  $warn = ([regex]::Matches($c, '^warning: (?!`)', 'Multiline')).Count
  $err = ([regex]::Matches($c, '^error', 'Multiline')).Count
  Rec 'T-2' 'clippy warnings/errors' ($(if ($warn -eq 0 -and $err -eq 0) { 'PASS' } else { 'FAIL' })) "warnings=$warn errors=$err"

  # ── T-3 비Windows 경로 검사(docs/18 §4) ─────────────────────────────────────
  $l = cargo check --workspace --all-targets --target x86_64-unknown-linux-gnu 2>&1 | Out-String
  $lerr = ([regex]::Matches($l, '^error', 'Multiline')).Count
  Rec 'T-3' 'linux target check' ($(if ($lerr -eq 0) { 'PASS' } else { 'FAIL' })) "errors=$lerr (경고는 기존 svg.rs 데드코드)"

  # ── T-4 커버리지(선택) ─────────────────────────────────────────────────────
  if ($Coverage -and -not $Quick) {
    $has = (Get-Command cargo-llvm-cov -ErrorAction SilentlyContinue) -ne $null
    if ($has) {
      $cov = cargo llvm-cov --workspace --summary-only 2>&1 | Out-String
      $m = [regex]::Match($cov, 'TOTAL\s+\d+\s+\d+\s+([\d.]+)%\s+\d+\s+\d+\s+([\d.]+)%\s+\d+\s+\d+\s+([\d.]+)%')
      if ($m.Success) { Rec 'T-4' 'line coverage (llvm-cov)' 'INFO' ("regions={0}% functions={1}% lines={2}%" -f $m.Groups[1].Value, $m.Groups[2].Value, $m.Groups[3].Value) }
      else { Rec 'T-4' 'line coverage (llvm-cov)' 'SKIP' 'summary parse 실패 — cargo llvm-cov --workspace 직접 실행' }
    } else { Rec 'T-4' 'line coverage (llvm-cov)' 'SKIP' 'cargo-llvm-cov 미설치(cargo install cargo-llvm-cov; rustup component add llvm-tools-preview)' }
  } else { Rec 'T-4' 'line coverage (llvm-cov)' 'SKIP' '-Coverage 미지정' }

  # ── 빌드(release) ──────────────────────────────────────────────────────────
  $b = cargo build --release -p nexa-app 2>&1 | Out-String
  if (-not (Test-Path $Exe)) { Rec 'B-0' 'release build' 'FAIL' 'exe 없음'; throw 'no exe' }
  if ($b -match 'error\[|os error 5') { Rec 'B-0' 'release build' 'FAIL' '빌드 실패(실행 중 exe 잠금이면 앱 종료 후 재실행)' } else { Rec 'B-0' 'release build' 'PASS' '' }

  # ── B-2 exe 크기 ──────────────────────────────────────────────────────────
  $len = (Get-Item $Exe).Length
  Rec 'B-2' 'exe size <= 10MB' ($(if ($len -le 10000000) { 'PASS' } else { 'FAIL' })) ("{0:N0} B = {1:N2} MB (십진 — docs 표기 규약)" -f $len, ($len / 1e6))

  # ── B-3 인박스 DLL(단일 출처 스크립트) ──────────────────────────────────────
  $b3 = & pwsh -NoProfile -File scripts/budget-b3.ps1 $Exe 2>&1 | Out-String
  Rec 'B-3' 'imports = OS inbox only' ($(if ($LASTEXITCODE -eq 0) { 'PASS' } else { 'FAIL' })) (($b3 -split "`n" | Select-Object -Last 2) -join ' ').Trim()

  # ── S-1 PE 완화 기술(DYNAMIC_BASE·HIGH_ENTROPY_VA·NX_COMPAT·GUARD_CF) ────────
  $bytes = [IO.File]::ReadAllBytes($Exe)
  $pe = [BitConverter]::ToInt32($bytes, 0x3c)
  $opt = $pe + 24
  $magic = [BitConverter]::ToUInt16($bytes, $opt)
  $dllch = [BitConverter]::ToUInt16($bytes, $opt + 70)
  $flags = @{}
  $flags['DYNAMIC_BASE'] = ($dllch -band 0x0040) -ne 0
  $flags['HIGH_ENTROPY_VA'] = ($dllch -band 0x0020) -ne 0
  $flags['NX_COMPAT'] = ($dllch -band 0x0100) -ne 0
  $flags['GUARD_CF'] = ($dllch -band 0x4000) -ne 0
  $missing = @($flags.GetEnumerator() | Where-Object { -not $_.Value } | ForEach-Object { $_.Key })
  $req = @('DYNAMIC_BASE', 'HIGH_ENTROPY_VA', 'NX_COMPAT')
  $reqMissing = @($req | Where-Object { -not $flags[$_] })
  Rec 'S-1' 'PE mitigations' ($(if ($reqMissing.Count -eq 0) { 'PASS' } else { 'FAIL' })) ("DllCharacteristics=0x{0:X4} missing=[{1}]" -f $dllch, ($missing -join ','))
  if (-not $flags['GUARD_CF']) { Rec 'S-1b' 'Control Flow Guard' 'WARN' 'GUARD_CF 없음 — rustflags -C control-flow-guard 검토(docs/29 §S)' }

  # ── S-2 매니페스트(관리자 요구 없음) ────────────────────────────────────────
  $txt = [Text.Encoding]::ASCII.GetString($bytes)
  $lvl = [regex]::Match($txt, 'requestedExecutionLevel\s+level="([^"]+)"')
  if ($lvl.Success) { Rec 'S-2' 'manifest execution level' ($(if ($lvl.Groups[1].Value -eq 'asInvoker') { 'PASS' } else { 'FAIL' })) $lvl.Groups[1].Value }
  else { Rec 'S-2' 'manifest execution level' 'WARN' 'requestedExecutionLevel 미발견(매니페스트 미포함 — asInvoker로 동작하나 명시 선언·longPathAware·DPI 선언 권장, docs/29 §4 A1/A4/A5)' }

  if (-not $Quick) {
    # ── P-1 폴더 열거 벤치 ────────────────────────────────────────────────
    $e = cargo run --release -q -p nexa-vfs --example audit_enum -- $BigDir 5 2>&1 | Out-String
    $m = [regex]::Match($e, '(\d+) entries .*median ([\d.]+) ms')
    if ($m.Success) {
      $n = [int]$m.Groups[1].Value; $ms = [double]$m.Groups[2].Value
      $limit = [math]::Max(10, $n / 250.0)   # 기준: 4 µs/entry(100k → 400ms) — 최소 10ms
      Rec 'P-1' 'dir enumerate (median)' ($(if ($ms -le $limit) { 'PASS' } else { 'FAIL' })) ("{0} entries {1} ms (limit {2:N0} ms) {3}" -f $n, $ms, $limit, $BigDir)
    } else { Rec 'P-1' 'dir enumerate' 'SKIP' $e.Trim() }

    # ── P-4 VT 파서 처리량 + 견고성 ─────────────────────────────────────────
    $v = cargo run --release -q -p nexa-term --example audit_vt 2>&1 | Out-String
    $m = [regex]::Match($v, '= ([\d.]+) MB/s')
    $ok = $v -match 'robustness: 10k nasty sequences ok'
    if ($m.Success) { Rec 'P-4' 'VT throughput >= 10 MB/s + nasty ok' ($(if ([double]$m.Groups[1].Value -ge 10 -and $ok) { 'PASS' } else { 'FAIL' })) ($v.Trim() -replace "`r?`n", ' | ') }
    else { Rec 'P-4' 'VT throughput' 'SKIP' $v.Trim() }
  }

  # ── B-1 유휴 실측(선택 — 실행 중 exe가 있으면 먼저 정상 종료) ────────────────
  if ($Idle -and -not $Quick) {
    $running = Get-Process nexa-app -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "*$((Get-Location).Path)*" }
    foreach ($p in $running) { $null = $p.CloseMainWindow(); $p.WaitForExit(8000) | Out-Null }
    $env:NO_COLOR = $null   # 도구 셸 상속 방지(09-04 교훈 — pwsh 색 꺼짐)
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $p = Start-Process -FilePath (Resolve-Path $Exe) -ArgumentList "`"$BigDir`"" -PassThru
    while (-not $p.MainWindowHandle) { Start-Sleep -Milliseconds 20; $p.Refresh(); if ($sw.ElapsedMilliseconds -gt 15000) { break } }
    $startMs = $sw.ElapsedMilliseconds
    Start-Sleep -Seconds 60; $p.Refresh()
    $c0 = $p.TotalProcessorTime; Start-Sleep -Seconds 10; $p.Refresh(); $cpu = ($p.TotalProcessorTime - $c0).TotalMilliseconds / 100
    $ws = $p.WorkingSet64 / 1MB; $pv = $p.PrivateMemorySize64 / 1MB
    Rec 'P-0' 'startup to window' ($(if ($startMs -le 1500) { 'PASS' } else { 'WARN' })) ("{0} ms (title: {1})" -f $startMs, $p.MainWindowTitle)
    Rec 'B-1' 'idle WorkingSet <= 30MB (60s)' ($(if ($ws -le 30) { 'PASS' } elseif ($pv -le 30) { 'WARN' } else { 'FAIL' })) ("WS={0:N1}MB Private={1:N1}MB threads={2} handles={3}" -f $ws, $pv, $p.Threads.Count, $p.HandleCount)
    Rec 'P-5' 'idle CPU ~0% (10s, active window)' ($(if ($cpu -le 2) { 'PASS' } else { 'FAIL' })) ("{0:N2}%" -f $cpu)
    $null = $p.CloseMainWindow(); $p.WaitForExit(8000) | Out-Null
  } else { Rec 'B-1' 'idle memory' 'SKIP' '-Idle 미지정(정식 B1은 docs/18: 10k 폴더·300s·3회 중앙값)' }
}
finally { Pop-Location }

$failCount = @($results | Where-Object status -eq 'FAIL').Count
$summaryLine = ("== audit summary: PASS={0} FAIL={1} WARN={2} SKIP={3} INFO={4}" -f @($results | Where-Object status -eq 'PASS').Count, $failCount, @($results | Where-Object status -eq 'WARN').Count, @($results | Where-Object status -eq 'SKIP').Count, @($results | Where-Object status -eq 'INFO').Count)
Write-Host "`n$summaryLine"

# ── 회차 폴더 저장(docs/29 §0 — 결과 원본은 docs/audit/<yyyyMMdd-HHmmss>/) ──
if (-not $NoSave) {
  $root = Split-Path $PSScriptRoot -Parent
  $dir = Join-Path $root $OutDir
  New-Item -ItemType Directory -Force $dir | Out-Null
  $mode = @(); if ($Quick) { $mode += '-Quick' }; if ($Coverage) { $mode += '-Coverage' }; if ($Idle) { $mode += '-Idle' }
  $sha = (git -C $root rev-parse --short HEAD 2>$null)
  $md = New-Object System.Collections.Generic.List[string]
  $md.Add("# 점검 자동 판정 — $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss') ($($mode -join ' '))")
  $md.Add("")
  $md.Add("> 규격 [docs/29-audit-checklist.md](../../29-audit-checklist.md) · 커밋 ``$sha`` · exe ``$Exe`` · BigDir ``$BigDir``")
  $md.Add("> 이 파일은 ``scripts/audit.ps1``이 생성한다. 정적 리뷰·실기 결과는 같은 폴더에 ``README.md``·``0N-*.md``로 사람이 추가한다.")
  $md.Add("")
  $md.Add("| ID | 항목 | 판정 | 상세 |")
  $md.Add("| --- | --- | --- | --- |")
  foreach ($r in $results) { $md.Add("| $($r.id) | $($r.name) | $($r.status) | $(($r.detail -replace '\|', '\|')) |") }
  $md.Add("")
  $md.Add($summaryLine)
  [IO.File]::WriteAllLines((Join-Path $dir 'summary.md'), $md, (New-Object Text.UTF8Encoding $false))
  $log = $results | ForEach-Object { "[{0,-4}] {1,-6} {2,-34} {3}" -f $_.status, $_.id, $_.name, $_.detail }
  [IO.File]::WriteAllLines((Join-Path $dir 'audit.log'), @($log + $summaryLine), (New-Object Text.UTF8Encoding $false))
  Write-Host "saved: $OutDir/summary.md, audit.log"
}
exit $failCount
