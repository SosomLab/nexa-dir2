<#
.SYNOPSIS
  Chocolatey 패키지를 수동으로 pack(+push)한다 — Windows 전용.

.DESCRIPTION
  평시 게시는 release.yml(태그 push)이 자동으로 처리한다. 이 스크립트는
  태그가 이미 소진된 버전을 뒤늦게 올릴 때처럼 CI를 못 쓰는 경우에만 쓴다.

  동작은 CI와 동일하다: Release 자산을 내려받아 SHA-256을 계산하고
  chocolateyinstall.ps1의 {{VERSION}}/{{CHECKSUM64}}를 치환한 뒤 pack한다.
  치환은 사본에만 적용하고 원본 스크립트는 되돌려 놓는다.

.PARAMETER Version
  게시할 버전(예: 0.8.1). 해당 GitHub Release가 이미 존재해야 한다.

.PARAMETER Id
  대상 패키지. 생략하면 둘 다.

.PARAMETER ApiKey
  community.chocolatey.org API 키. 주면 push까지, 없으면 pack까지만.

.EXAMPLE
  # 팩만 (결과 확인용)
  pwsh packaging\chocolatey\pack-and-push.ps1 -Version 0.8.1

.EXAMPLE
  # 포터블 패키지만 게시
  pwsh packaging\chocolatey\pack-and-push.ps1 -Version 0.8.1 -Id nexa-dir.portable -ApiKey <키>
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory)][string]$Version,
  [ValidateSet('nexa-dir', 'nexa-dir.portable')][string[]]$Id = @('nexa-dir', 'nexa-dir.portable'),
  [string]$ApiKey
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $MyInvocation.MyCommand.Definition
$out  = Join-Path $root 'out'
$base = "https://github.com/SosomLab/nexa-dir2/releases/download/$Version"

# 패키지별 대상 Release 자산 — CI(release.yml)와 동일해야 한다.
$assets = @{
  'nexa-dir'          = "NexaDir-Setup-$Version.exe"
  'nexa-dir.portable' = "NexaDir-$Version-win-x64.exe"
}

New-Item -ItemType Directory -Force -Path $out | Out-Null

foreach ($pkg in $Id) {
  $asset = $assets[$pkg]
  $tmp   = Join-Path ([System.IO.Path]::GetTempPath()) $asset

  Write-Host "[$pkg] $asset 다운로드 중..." -ForegroundColor Cyan
  Invoke-WebRequest -Uri "$base/$asset" -OutFile $tmp -UseBasicParsing
  $sha = (Get-FileHash -Algorithm SHA256 $tmp).Hash.ToLower()
  Write-Host "[$pkg] SHA-256 = $sha"
  Remove-Item $tmp -Force

  # 원본은 {{...}} 자리표시자를 유지해야 하므로 치환 후 반드시 되돌린다.
  $ps1  = Join-Path $root "$pkg\tools\chocolateyinstall.ps1"
  $orig = Get-Content $ps1 -Raw
  try {
    $orig.Replace('{{VERSION}}', $Version).Replace('{{CHECKSUM64}}', $sha) |
      Set-Content $ps1 -NoNewline

    choco pack (Join-Path $root "$pkg\$pkg.nuspec") --version $Version --outputdirectory $out
    if ($LASTEXITCODE -ne 0) { throw "choco pack 실패($pkg): $LASTEXITCODE" }
  }
  finally {
    Set-Content $ps1 -Value $orig -NoNewline
  }

  $nupkg = Join-Path $out "$pkg.$Version.nupkg"
  Write-Host "[$pkg] 팩 완료: $nupkg" -ForegroundColor Green

  if ($ApiKey) {
    choco push $nupkg --source https://push.chocolatey.org/ --api-key $ApiKey
    if ($LASTEXITCODE -ne 0) { throw "choco push 실패($pkg): $LASTEXITCODE" }
    Write-Host "[$pkg] push 완료 — 모더레이션 대기" -ForegroundColor Green
  }
}

if (-not $ApiKey) {
  Write-Host "`nApiKey를 주지 않아 pack까지만 수행했습니다. 게시하려면 -ApiKey <키>를 추가하세요." -ForegroundColor Yellow
}
