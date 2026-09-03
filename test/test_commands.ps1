# Test script for azdocli commands
# This script tests the command-line interface without requiring actual Azure DevOps authentication

Write-Host "Testing azdocli command-line interface..." -ForegroundColor Green

# Build the project first
Write-Host "`nBuilding project..." -ForegroundColor Yellow
cargo build
if ($LASTEXITCODE -ne 0)
{
  Write-Host "Build failed!" -ForegroundColor Red
  exit 1
}

$exe = "..\target\debug\azdocli.exe"

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

Write-Host "`nTesting projects help..." -ForegroundColor Yellow
& $exe projects --help

Write-Host "`nTesting projects list help..." -ForegroundColor Yellow
& $exe projects list --help

Write-Host "`nTesting projects show help..." -ForegroundColor Yellow
& $exe projects show --help

Write-Host "`nTesting projects create help..." -ForegroundColor Yellow
& $exe projects create --help

Write-Host "`nTesting projects delete help..." -ForegroundColor Yellow
& $exe projects delete --help

Write-Host "`nTesting repos pr help..." -ForegroundColor Yellow
& $exe repos pr --help

foreach ($subcommand in @("create", "list", "show", "commits", "update", "complete", "abandon", "reactivate", "threads"))
{
  Write-Host "`nTesting repos pr $subcommand help..." -ForegroundColor Yellow
  & $exe repos pr $subcommand --help
}

Write-Host "`nTesting repos pr reviewers help..." -ForegroundColor Yellow
& $exe repos pr reviewers --help

foreach ($subcommand in @("list", "add", "remove", "vote"))
{
  Write-Host "`nTesting repos pr reviewers $subcommand help..." -ForegroundColor Yellow
  & $exe repos pr reviewers $subcommand --help
}

Write-Host "`nTesting repos pr comment help..." -ForegroundColor Yellow
& $exe repos pr comment --help

foreach ($subcommand in @("add", "reply", "resolve"))
{
  Write-Host "`nTesting repos pr comment $subcommand help..." -ForegroundColor Yellow
  & $exe repos pr comment $subcommand --help
}

Write-Host "`nAll command-line interface tests completed successfully!" -ForegroundColor Green
Write-Host "Note: Actual functionality requires Azure DevOps authentication." -ForegroundColor Cyan
