<#
.SYNOPSIS
  STRATA Rust CI 로컬 재현 — `.github/workflows/strata-rust-ci.yml` 과 같은 게이트를 이 PC 에서 돌린다.

.DESCRIPTION
  GitHub Actions 가 계정 결제 잠금 등으로 실행되지 않을 때 같은 검사를 로컬에서 수행한다.
    1) cargo fmt --all -- --check
    2) cargo check --workspace --all-targets --locked
    3) cargo clippy --workspace --all-targets --locked -- -D warnings
    4) cargo test --workspace --locked
    5) --verify-all --screenshot-tour (V1~V8 하네스 + 캡처 산출물 크기 검사)

.PARAMETER SkipHarness
 5단계(V1~V8 하네스·캡처)를 건너뛴다. 빠른 확인용.

.PARAMETER ShotDir
 5단계 캡처 산출물 폴더(기본 `target/ci-shots`).

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File strata-arch3d/scripts/ci.ps1
#>
[CmdletBinding()]
param(
    [switch]$SkipHarness,
    [string]$ShotDir = "target/ci-shots"
)

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root

# 이 PC 는 cargo 가 PATH 에 없고, clippy/rustfmt 는 임시 폴더에 풀려 있을 수 있다.
$cargoBin = Join-Path $env:USERPROFILE '.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin'
if (Test-Path $cargoBin) { $env:PATH = "$cargoBin;$env:PATH" }
$toolBin = Join-Path $env:TEMP 'strata-tools\bin'
if (Test-Path $toolBin) { $env:PATH = "$toolBin;$env:PATH" }

function Invoke-Gate {
    param([string]$Name, [string]$Exe, [string[]]$CmdArgs)
    Write-Host ""
    Write-Host "=== $Name ===" -ForegroundColor Cyan
    $sw = [Diagnostics.Stopwatch]::StartNew()
    & $Exe @CmdArgs
    $code = $LASTEXITCODE
    $sw.Stop()
    if ($code -ne 0) {
        Write-Host "FAIL: $Name (exit $code, $([math]::Round($sw.Elapsed.TotalSeconds,1))s)" -ForegroundColor Red
        exit $code
    }
    Write-Host ("OK: {0} ({1:N1}s)" -f $Name, $sw.Elapsed.TotalSeconds) -ForegroundColor Green
}

$swAll = [Diagnostics.Stopwatch]::StartNew()

Invoke-Gate -Name "1/5 cargo fmt --check" -Exe cargo -CmdArgs @('fmt', '--all', '--', '--check')
Invoke-Gate -Name "2/5 cargo check" -Exe cargo -CmdArgs @('check', '--workspace', '--all-targets', '--locked')
Invoke-Gate -Name "3/5 cargo clippy (-D warnings)" -Exe cargo -CmdArgs @('clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings')
Invoke-Gate -Name "4/5 cargo test" -Exe cargo -CmdArgs @('test', '--workspace', '--locked')

if (-not $SkipHarness) {
    if (Test-Path $ShotDir) { Remove-Item $ShotDir -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $ShotDir | Out-Null
    Invoke-Gate -Name "5/5 V1~V8 harness + capture" -Exe cargo -CmdArgs @(
        'run', '--release', '--locked', '-p', 'strata-studio', '--', '--verify-all', '--screenshot-tour', $ShotDir
    )
    $files = Get-ChildItem "$ShotDir/*" -File
    if ($files.Count -eq 0) { Write-Host "FAIL: 캡처 산출물이 없습니다" -ForegroundColor Red; exit 1 }
    foreach ($f in $files) {
        if ($f.Length -eq 0) { Write-Host "FAIL: $($f.Name) 이 비어 있습니다" -ForegroundColor Red; exit 1 }
        if ($f.Extension -match '^\.(png|jpg|jpeg)$' -and $f.Length -lt 1000) {
            Write-Host "FAIL: 이미지 $($f.Name) 이 너무 작습니다 ($($f.Length) B)" -ForegroundColor Red; exit 1
        }
    }
    Write-Host "OK: 캡처 산출물 $($files.Count)개 (이미지 1,000 B 이상 · 빈 파일 없음)" -ForegroundColor Green
}

$swAll.Stop()
Write-Host ""
Write-Host ("CI 재현 통과 — 총 {0:N1}초" -f $swAll.Elapsed.TotalSeconds) -ForegroundColor Green
