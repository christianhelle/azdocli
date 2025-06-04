# WinGet Distribution Setup

This document describes the setup required for distributing Windows binaries to WinGet.

## Prerequisites

1. **WINGET_TOKEN Secret**: A GitHub personal access token with the following permissions:
   - `public_repo` scope for creating PRs to the winget-pkgs repository
   - Must be added as a repository secret named `WINGET_TOKEN`

2. **Fork Configuration**: The workflow is configured to create PRs from the `christianhelle` user account.

## How It Works

1. When a release is created, the `winget-release` job automatically runs
2. The job uses the `vedantmgoyal2009/winget-releaser@v2` action
3. It detects Windows binary assets matching the pattern `windows-(x64|arm64)\.zip$`
4. Creates a PR to the [Microsoft WinGet Community Repository](https://github.com/microsoft/winget-pkgs)
5. Once the PR is merged by Microsoft maintainers, the package becomes available via `winget install ChristianHelle.azdocli`

## Package Information

- **Package ID**: `ChristianHelle.azdocli`
- **Supported Architectures**: x64, ARM64
- **Installer Type**: Zip archive (portable)
- **Update Method**: Automatic via GitHub releases

## Manual Verification

After a release, you can verify the WinGet submission by:
1. Checking the Actions tab for the `winget-release` job status
2. Looking for a new PR in the [winget-pkgs repository](https://github.com/microsoft/winget-pkgs/pulls)
3. Monitoring for Microsoft maintainer approval and merge

## Troubleshooting

- Ensure the `WINGET_TOKEN` secret is valid and has proper permissions
- Verify that Windows binary assets are properly named in the release
- Check that the release is published (not draft) before the WinGet job runs