$ErrorActionPreference = 'Stop'

$packageName = $env:ChocolateyPackageName
$toolsDir = "$(Split-Path -parent $MyInvocation.MyCommand.Definition)"

# Remove from PATH
Uninstall-ChocolateyPath $toolsDir 'Machine'

Write-Host "Azure DevOps CLI (ado) has been uninstalled" -ForegroundColor Green