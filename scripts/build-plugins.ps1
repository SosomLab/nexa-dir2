# build-plugins.ps1 — 동봉 미리보기 플러그인(.wasm) 빌드 (docs/18 §3-1 · docs/24 §4)
#
# samples/*-wasm 각각을 wasm32-unknown-unknown으로 빌드해
#   ① 샘플의 dist\<이름>.wasm  (저장소 동봉본 — E2E 테스트가 로드)
#   ② -OutDir 로 지정한 폴더    (배포 스테이징 — 릴리스 워크플로가 zip·설치본에 싣는다)
# 두 곳에 복사한다. CI·로컬 공용(단일 출처) — 절차가 바뀌면 이 파일만 고친다.
#
# 사용:
#   pwsh scripts/build-plugins.ps1                 # dist만 갱신
#   pwsh scripts/build-plugins.ps1 -OutDir plugins # 배포용 폴더에도 복사
#   pwsh scripts/build-plugins.ps1 -SkipDist       # dist는 그대로 두고 OutDir만
#
# 사전 준비(1회): rustup target add wasm32-unknown-unknown

[CmdletBinding()]
param(
    [string]$OutDir,
    [switch]$SkipDist
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot

# (샘플 폴더, cargo 산출물 이름, 배포 파일명) — 새 플러그인은 여기 한 줄 추가
$plugins = @(
    @{ Dir = "markdown-viewer-wasm"; Artifact = "markdown_viewer.wasm"; Name = "markdown.wasm" }
    @{ Dir = "archive-viewer-wasm";  Artifact = "archive_viewer.wasm";  Name = "archive.wasm" }
)

if (-not (rustup target list --installed | Select-String -SimpleMatch "wasm32-unknown-unknown")) {
    throw "wasm32-unknown-unknown 타깃이 없습니다 — rustup target add wasm32-unknown-unknown"
}

if ($OutDir) {
    $OutDir = [System.IO.Path]::GetFullPath((Join-Path $repo $OutDir))
    New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
}

foreach ($p in $plugins) {
    $src = Join-Path $repo "samples\$($p.Dir)"
    if (-not (Test-Path $src)) { throw "샘플 폴더 없음: $src" }
    Push-Location $src
    try {
        cargo build --release --target wasm32-unknown-unknown
        if ($LASTEXITCODE -ne 0) { throw "cargo build 실패: $($p.Dir)" }
    } finally {
        Pop-Location
    }
    $built = Join-Path $src "target\wasm32-unknown-unknown\release\$($p.Artifact)"
    if (-not (Test-Path $built)) { throw "산출물 없음: $built" }
    $kb = [math]::Round((Get-Item $built).Length / 1KB, 1)

    if (-not $SkipDist) {
        $dist = Join-Path $src "dist"
        New-Item -ItemType Directory -Force -Path $dist | Out-Null
        Copy-Item $built (Join-Path $dist $p.Name) -Force
    }
    if ($OutDir) {
        Copy-Item $built (Join-Path $OutDir $p.Name) -Force
    }
    Write-Output "$($p.Name) = $kb KB  ($($p.Dir))"
}

Write-Output "완료 — dist 갱신: $(-not $SkipDist) · 배포 폴더: $(if ($OutDir) { $OutDir } else { '(없음)' })"
