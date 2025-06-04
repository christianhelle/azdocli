# Homebrew Formula for azdocli

This directory contains the Homebrew formula template for azdocli.

## How it works

1. The `azdocli.rb` file is a template with placeholders for version and SHA256 hashes
2. During the release workflow, the `scripts/update-homebrew-formula.sh` script:
   - Downloads the MacOS binary artifacts
   - Calculates SHA256 checksums
   - Updates the template with actual values
   - Generates the final Homebrew formula

## Formula Details

The formula:
- Detects the CPU architecture (ARM64 vs x64)
- Downloads the appropriate MacOS binary from GitHub releases
- Installs the `ado` binary to the Homebrew bin directory
- Includes a test that verifies the installation

## Distribution Options

To complete Homebrew distribution, the generated formula can be:

1. **Submitted to homebrew-core**: For inclusion in the main Homebrew repository
2. **Published to a custom tap**: Create a homebrew-tap repository (e.g., `homebrew-azdocli`)

### Option 1: Custom Tap (Recommended)

Create a separate repository named `homebrew-azdocli` and publish the formula there:

```bash
# Users would install with:
brew tap christianhelle/azdocli
brew install azdocli
```

### Option 2: Homebrew Core

Submit a PR to [homebrew-core](https://github.com/Homebrew/homebrew-core) with the formula.

```bash
# Users would install with:
brew install azdocli
```

## Current Implementation

The current release workflow creates the formula but doesn't automatically publish it. To complete the implementation, you would need to:

1. Create a homebrew-tap repository
2. Update the workflow to push the generated formula to that repository
3. Or submit the formula to homebrew-core manually