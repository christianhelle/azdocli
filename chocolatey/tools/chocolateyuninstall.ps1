$ErrorActionPreference = 'Stop'

$packageName = 'azdocli'

# Remove the install directory from PATH
$installDir = "$(Split-Path -parent $MyInvocation.MyCommand.Definition)"

Write-Host "Removing $installDir from PATH..."
Uninstall-ChocolateyPath $installDir

Write-Host "$packageName has been uninstalled successfully."
