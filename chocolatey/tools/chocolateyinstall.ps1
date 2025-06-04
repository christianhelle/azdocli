$ErrorActionPreference = 'Stop'

$packageName = $env:ChocolateyPackageName
$version = $env:ChocolateyPackageVersion
$toolsDir = "$(Split-Path -parent $MyInvocation.MyCommand.Definition)"

# Determine architecture
$pp = Get-PackageParameters
if ($pp.x86 -or ((Get-OSArchitectureWidth -compare 32) -or ($env:PROCESSOR_ARCHITEW6432 -eq $null -and $env:PROCESSOR_ARCHITECTURE -eq 'x86'))) {
    Write-Warning "32-bit Windows is not supported by azdocli"
    throw "This package does not support 32-bit Windows"
}

$architecture = 'x64'
# Check for ARM64 architecture using environment variables (more compatible)
if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64' -or $env:PROCESSOR_ARCHITEW6432 -eq 'ARM64') {
    $architecture = 'arm64'
}

# Define download URLs
$url64 = "https://github.com/christianhelle/azdocli/releases/download/$version/windows-x64.zip"
$urlArm64 = "https://github.com/christianhelle/azdocli/releases/download/$version/windows-arm64.zip"

$packageArgs = @{
    packageName   = $packageName
    unzipLocation = $toolsDir
    fileType      = 'zip'
    url64bit      = $url64
    checksumType64 = 'sha256'
    checksum64    = '' # Will be filled by the release workflow
}

# Use ARM64 URL if on ARM64 architecture
if ($architecture -eq 'arm64') {
    $packageArgs.url64bit = $urlArm64
    $packageArgs.checksum64 = '__ARM64_CHECKSUM__' # Will be filled by the release workflow for ARM64
}

Install-ChocolateyZipPackage @packageArgs

# Add to PATH
$binPath = Join-Path $toolsDir "ado.exe"
if (Test-Path $binPath) {
    Install-ChocolateyPath $toolsDir 'Machine'
    Write-Host "Azure DevOps CLI (ado) has been installed and added to PATH" -ForegroundColor Green
    Write-Host "You can now use 'ado' command from any command prompt" -ForegroundColor Green
    Write-Host ""
    Write-Host "To get started:" -ForegroundColor Yellow
    Write-Host "  1. Run 'ado login' to authenticate with Azure DevOps" -ForegroundColor Yellow
    Write-Host "  2. Run 'ado --help' to see available commands" -ForegroundColor Yellow
} else {
    throw "Installation failed: ado.exe not found in the extracted files"
}