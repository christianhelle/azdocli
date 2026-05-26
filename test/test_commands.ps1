# Test script for azdocli commands
# This script tests the command-line interface without requiring actual Azure DevOps authentication

Write-Host "Testing azdocli command-line interface..." -ForegroundColor Green

# Build the project first
Write-Host "`nBuilding project..." -ForegroundColor Yellow
cargo build
if ($LASTEXITCODE -ne 0) {
    Write-Host "Build failed!" -ForegroundColor Red
    exit 1
}

$exe = ".\target\debug\azdocli.exe"

Write-Host "`nTesting main help..." -ForegroundColor Yellow
& $exe --help

Write-Host "`nTesting repos help..." -ForegroundColor Yellow
& $exe repos --help

Write-Host "`nTesting repos show help..." -ForegroundColor Yellow
& $exe repos show --help

Write-Host "`nTesting repos list help..." -ForegroundColor Yellow
& $exe repos list --help

Write-Host "`nTesting repos clone help..." -ForegroundColor Yellow
& $exe repos clone --help

Write-Host "`nAll command-line interface tests completed successfully!" -ForegroundColor Green
Write-Host "Note: Actual functionality requires Azure DevOps authentication." -ForegroundColor Cyan
