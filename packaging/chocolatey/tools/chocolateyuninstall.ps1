$ErrorActionPreference = 'Stop'

# 제거해도 사용자 데이터(설정·세션)는 보존한다 — docs/21-distribution.md §3.
$packageName  = 'nexa-dir'
$softwareName = 'Nexa Dir*'

$keys = @(Get-UninstallRegistryKey -SoftwareName $softwareName)

if ($keys.Count -eq 0) {
  Write-Warning "$packageName: 제거 항목을 찾지 못했습니다(이미 제거됨)."
  return
}
if ($keys.Count -gt 1) {
  Write-Warning "$packageName: 제거 항목이 여러 개 발견되어 건너뜁니다:"
  $keys | ForEach-Object { Write-Warning "  - $($_.DisplayName)" }
  return
}

$key = $keys[0]
Uninstall-ChocolateyPackage `
  -PackageName $packageName `
  -FileType 'exe' `
  -SilentArgs '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART' `
  -ValidExitCodes @(0) `
  -File $key.UninstallString.Trim('"')
