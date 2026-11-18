#!/usr/bin/env pwsh
param(
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][string]$WinX64Sha,
    [Parameter(Mandatory)][string]$WinArm64Sha
)

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$distDir = Join-Path $repoRoot 'dist/winget'
$outputRoot = if ($env:OUTPUT_DIR) { $env:OUTPUT_DIR } else { Join-Path $repoRoot 'output' }
$outputDir = Join-Path $outputRoot "winget-$Version"

if (Test-Path $outputDir) {
    Remove-Item -Recurse -Force $outputDir
}
New-Item -ItemType Directory -Force $outputDir | Out-Null

# Create WinGet manifests, updating version and SHA256 in each
foreach ($template in Get-ChildItem -Path $distDir -Filter '*.yaml') {
    $content = (Get-Content -Path $template.FullName -Raw) `
        -replace 'VERSION_PLACEHOLDER', $Version `
        -replace 'SHA256_X64_PLACEHOLDER', $WinX64Sha `
        -replace 'SHA256_ARM64_PLACEHOLDER', $WinArm64Sha
    Set-Content -Path (Join-Path $outputDir $template.Name) -Value $content -NoNewline
}

Write-Host "WinGet manifests created in $outputDir/"
Get-ChildItem $outputDir
