# Distribution

Templates and generators for every package manager channel. Generated files go
to `output/` (git-ignored).

## Channels

| Channel | Template | Generator | Published to | Job in `packaging.yml` | Secret |
|---|---|---|---|---|---|
| crates.io | `Cargo.toml` | - | crates.io (`publish-crates` in `release.yml`) | - | `CRATES_TOKEN` |
| WinGet | `dist/winget/*.yaml` | `scripts/generate-winget.sh` | microsoft/winget-pkgs PR (winget-releaser) | `winget` | `WINGET_TOKEN` |
| Chocolatey | `dist/chocolatey/` | `scripts/generate-chocolatey.sh` | push.chocolatey.org | `chocolatey` | `CHOCOLATEY_API_KEY` |
| Homebrew | `dist/homebrew/azdocli.rb` | `scripts/generate-homebrew.sh` | christianhelle/homebrew-tap `Formula/` | `homebrew` | `PACKAGES_TOKEN` |
| Scoop | `dist/scoop/azdocli.json` | `scripts/generate-scoop.sh` | christianhelle/scoop-bucket `bucket/` | `scoop` | `PACKAGES_TOKEN` |
| Snap | `snapcraft.yaml` (repo root) | snapcraft build | Snap Store, `stable` | `snap` | `SNAPCRAFT_STORE_CREDENTIALS` |
| cargo-binstall | `[package.metadata.binstall]` in `Cargo.toml` | - | reads GitHub Releases | - | - |

## Automation

`.github/workflows/packaging.yml` runs when a GitHub release is **published**, or
manually via `workflow_dispatch` with a `version` input (the release must already
exist).

1. **Download**: the release zips (`windows-x64.zip`, `windows-arm64.zip`,
   `macos-x64.zip`, `macos-arm64.zip`, `linux-x64.zip`, `linux-arm64.zip`) are
   fetched with `gh release download`. Asset names are fixed and used by
   `azdocli update`, `docs/install.*` and the channels above.
2. **Generate & validate**: each channel job computes SHA-256 checksums,
   generates its files and validates them natively (`brew style`/`brew test`,
   `choco pack`, `winget validate`, `scoop install`, `snapcore/action-build`).
3. **Publish**: each publish step runs only when its secret is set. Without the
   secret the job still validates and uploads its workflow artifact. The tap
   and bucket only move forward: if they already hold a newer version, the
   publish step skips. Roll back by editing those repos by hand.

Run a generator locally with dummy values to check its output:

```bash
./scripts/generate-homebrew.sh <version> <macos-x64-sha> <macos-arm64-sha> <linux-x64-sha> <linux-arm64-sha>
./scripts/generate-chocolatey.sh <version> <windows-x64-sha>
./scripts/generate-winget.sh <version> <windows-x64-sha> <windows-arm64-sha>
./scripts/generate-scoop.sh <version> <windows-x64-sha> <windows-arm64-sha>
```

## One-time setup (repository owner)

1. Create public repos `christianhelle/homebrew-tap` (with an initial commit on
   `main`) and `christianhelle/scoop-bucket` (with an initial commit on `main`).
2. Create one classic PAT with the `public_repo` scope. Store it as the
   `WINGET_TOKEN` and `PACKAGES_TOKEN` repository secrets.
3. Export Snap Store credentials and store them as `SNAPCRAFT_STORE_CREDENTIALS`:
   ```bash
   snapcraft export-login --snaps=azdocli --acls package_access,package_push,package_update,package_release -
   ```
4. Confirm `CHOCOLATEY_API_KEY` is still valid (chocolatey.org account, API keys).
5. Optional: in the Snapcraft dashboard, disable the build-service auto-build from
   GitHub, since CI now publishes snaps.
6. After the WinGet PR for a release merges, run `packaging.yml` via
   `workflow_dispatch` to publish the other channels for that version.

## Notes

- WinGet is submitted automatically by winget-releaser; the first version of a
  package must be submitted manually.
- Chocolatey versions go through community moderation before they are listed.
- Snap builds from a git tag (`git describe --tags`), so the snap version matches
  the release. The Cargo version in `Cargo.toml` is patched at build time only.
