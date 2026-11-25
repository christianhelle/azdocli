$ErrorActionPreference = 'Stop'

$packageName = 'azdocli'
$url64 = 'https://github.com/christianhelle/azdocli/releases/download/VERSION_PLACEHOLDER/windows-x64.zip'
$checksum64 = 'SHA256_X64_PLACEHOLDER'
$checksumType64 = 'sha256'

$packageArgs = @{
  packageName    = $packageName
  unzipLocation  = "$(Split-Path -parent $MyInvocation.MyCommand.Definition)"
  url64bit       = $url64
  checksum64     = $checksum64
  checksumType64 = $checksumType64
}

# Chocolatey shims the azdocli.exe in the tools directory automatically, so
# the install directory does not need to be added to PATH.
Install-ChocolateyZipPackage @packageArgs
