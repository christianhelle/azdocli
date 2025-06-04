# Chocolatey Package for Azure DevOps CLI

This directory contains the files needed to create the Chocolatey package for azdocli.

## Package Structure

- `azdocli.nuspec` - Chocolatey package specification file
- `tools/chocolateyinstall.ps1` - PowerShell script for installation
- `tools/chocolateyuninstall.ps1` - PowerShell script for uninstallation

## How it works

The Chocolatey package downloads the Windows binaries from the GitHub releases and installs them to the tools directory. The installation script:

1. Detects the system architecture (x64 or ARM64)
2. Downloads the appropriate binary from GitHub releases
3. Extracts the binary to the tools directory
4. Adds the binary to the system PATH

## Publishing

The package is automatically published to Chocolatey during the release workflow when:

1. A new release is created
2. The Windows binaries are built and uploaded to GitHub releases
3. The Chocolatey package is created with the correct version and checksums
4. The package is pushed to the Chocolatey repository using the CHOCOLATEY_API_KEY secret

## Local Testing

To test the package locally:

1. Install Chocolatey if not already installed
2. Navigate to this directory
3. Run `choco pack azdocli.nuspec`
4. Install locally with `choco install azdocli.{version}.nupkg --source .`