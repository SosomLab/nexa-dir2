$ErrorActionPreference = 'Stop'

# {{VERSION}}/{{CHECKSUM64}}는 release.yml이 태그 빌드 시점에 치환한다(수동 편집 금지).
$packageName = 'nexa-dir'
$version     = '{{VERSION}}'
$url64       = "https://github.com/SosomLab/nexa-dir2/releases/download/$version/NexaDir-Setup-$version.exe"

$packageArgs = @{
  packageName    = $packageName
  fileType       = 'exe'
  url64bit       = $url64
  checksum64     = '{{CHECKSUM64}}'
  checksumType64 = 'sha256'
  softwareName   = 'Nexa Dir*'
  # Inno Setup. /ALLUSERS = 머신 전역 설치 강제 — choco는 관리자로 도는데
  # 설치기 기본은 PrivilegesRequired=lowest(사용자별)이므로 명시하지 않으면
  # 관리자 계정의 %LOCALAPPDATA%에 설치된다(installer/nexa.iss 참조).
  silentArgs     = '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP- /ALLUSERS'
  validExitCodes = @(0)
}

Install-ChocolateyPackage @packageArgs
