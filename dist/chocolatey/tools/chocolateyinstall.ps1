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

Install-ChocolateyZipPackage @packageArgs

# Create shim for the executable
$installDir = "$(Split-Path -parent $MyInvocation.MyCommand.Definition)"
Install-ChocolateyPath $installDir