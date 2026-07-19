$ErrorActionPreference = 'Stop'

# {{VERSION}}/{{CHECKSUM64}}는 release.yml이 태그 빌드 시점에 치환한다(수동 편집 금지).
$version  = '{{VERSION}}'
$toolsDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$url64    = "https://github.com/SosomLab/nexa-dir2/releases/download/$version/NexaDir-$version-win-x64.exe"

# 포터블 단일 exe를 패키지 tools\에 내려받는다. 설치기를 거치지 않으므로
# Chocolatey가 tools\의 exe를 자동으로 shim 처리해 PATH에 노출한다
# (NexaDir.exe.gui 마커 = GUI 앱이라 shim이 종료를 기다리지 않게 한다).
Get-ChocolateyWebFile `
  -PackageName 'nexa-dir.portable' `
  -FileFullPath "$toolsDir\NexaDir.exe" `
  -Url64bit $url64 `
  -Checksum64 '{{CHECKSUM64}}' `
  -ChecksumType64 'sha256'
