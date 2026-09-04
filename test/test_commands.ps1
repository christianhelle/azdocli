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

# Runs the CLI and aborts the script if it reports failure, so a broken
# subcommand cannot be reported as a passing run.
function Invoke-Cli
{
  param([Parameter(ValueFromRemainingArguments = $true)][string[]]$CliArgs)

  & $exe @CliArgs
  if ($LASTEXITCODE -ne 0)
  {
    Write-Host "Command failed: azdocli $($CliArgs -join ' ') (exit code $LASTEXITCODE)" -ForegroundColor Red
    exit 1
  }
}

Write-Host "`nTesting main help..." -ForegroundColor Yellow
Invoke-Cli --help

Write-Host "`nTesting repos help..." -ForegroundColor Yellow
Invoke-Cli repos --help

Write-Host "`nTesting repos show help..." -ForegroundColor Yellow
Invoke-Cli repos show --help

Write-Host "`nTesting repos list help..." -ForegroundColor Yellow
Invoke-Cli repos list --help

Write-Host "`nTesting repos clone help..." -ForegroundColor Yellow
Invoke-Cli repos clone --help

foreach ($subcommand in @("branches", "commits", "files", "file"))
{
  Write-Host "`nTesting repos $subcommand help..." -ForegroundColor Yellow
  Invoke-Cli repos $subcommand --help
}

Write-Host "`nTesting projects help..." -ForegroundColor Yellow
Invoke-Cli projects --help

Write-Host "`nTesting projects list help..." -ForegroundColor Yellow
Invoke-Cli projects list --help

Write-Host "`nTesting projects show help..." -ForegroundColor Yellow
Invoke-Cli projects show --help

Write-Host "`nTesting projects create help..." -ForegroundColor Yellow
Invoke-Cli projects create --help

Write-Host "`nTesting projects delete help..." -ForegroundColor Yellow
Invoke-Cli projects delete --help

Write-Host "`nTesting repos pr help..." -ForegroundColor Yellow
Invoke-Cli repos pr --help

foreach ($subcommand in @("create", "list", "show", "commits", "update", "complete", "abandon", "reactivate", "threads"))
{
  Write-Host "`nTesting repos pr $subcommand help..." -ForegroundColor Yellow
  Invoke-Cli repos pr $subcommand --help
}

Write-Host "`nTesting repos pr reviewers help..." -ForegroundColor Yellow
Invoke-Cli repos pr reviewers --help

foreach ($subcommand in @("list", "add", "remove", "vote"))
{
  Write-Host "`nTesting repos pr reviewers $subcommand help..." -ForegroundColor Yellow
  Invoke-Cli repos pr reviewers $subcommand --help
}

Write-Host "`nTesting repos pr comment help..." -ForegroundColor Yellow
Invoke-Cli repos pr comment --help

foreach ($subcommand in @("add", "reply", "resolve"))
{
  Write-Host "`nTesting repos pr comment $subcommand help..." -ForegroundColor Yellow
  Invoke-Cli repos pr comment $subcommand --help
}

Write-Host "`nTesting boards help..." -ForegroundColor Yellow
Invoke-Cli boards --help

Write-Host "`nTesting boards query help..." -ForegroundColor Yellow
Invoke-Cli boards query --help

Write-Host "`nTesting boards work-item help..." -ForegroundColor Yellow
Invoke-Cli boards work-item --help

foreach ($subcommand in @("create", "delete", "list", "show", "types", "update"))
{
  Write-Host "`nTesting boards work-item $subcommand help..." -ForegroundColor Yellow
  Invoke-Cli boards work-item $subcommand --help
}

Write-Host "`nTesting boards work-item comment help..." -ForegroundColor Yellow
Invoke-Cli boards work-item comment --help

foreach ($subcommand in @("list", "add"))
{
  Write-Host "`nTesting boards work-item comment $subcommand help..." -ForegroundColor Yellow
  Invoke-Cli boards work-item comment $subcommand --help
}

Write-Host "`nAll command-line interface tests completed successfully!" -ForegroundColor Green
Write-Host "Note: Actual functionality requires Azure DevOps authentication." -ForegroundColor Cyan
