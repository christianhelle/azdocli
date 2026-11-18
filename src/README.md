# Azure DevOps CLI

[![Build](https://github.com/christianhelle/azdocli/actions/workflows/build.yml/badge.svg)](https://github.com/christianhelle/azdocli/actions/workflows/build.yml)
![Crates.io Version](https://img.shields.io/crates/v/azdocli)

CLI tool for interacting with Azure DevOps: repos, pull requests, pipelines, releases, boards, artifacts, projects and users.

Full documentation: <https://github.com/christianhelle/azdocli#readme>

## Installation

```bash
cargo install azdocli
```

To update, run the same command again. `azdocli update` recognises a binary in Cargo's bin directory (`$CARGO_HOME/bin`, by default `~/.cargo/bin`) and tells you to do this. It doesn't recognise a custom `--root` or `CARGO_INSTALL_ROOT`.

## Authentication

Create a Personal Access Token (PAT) in Azure DevOps (profile picture → **Personal Access Tokens** → **New Token**) with the scopes you need:

- **Code**: Read & write (repositories, pull requests)
- **Build**: Read & execute (pipelines)
- **Work Items**: Read & write (boards)
- **Project and Team**: Read (projects)

Then log in. The PAT is never stored in plaintext. On Windows it is encrypted with DPAPI. On macOS and Linux it is encrypted with a key derived from the machine id and home directory, which are not secret, so the real protection is the credential file's `0600` permissions.

```sh
azdocli login
```

Enterprise / on-premises Azure DevOps Server is supported: enter the server URL (e.g. `https://tfs.mycompany.com/tfs/DefaultCollection`) when prompted for the organization. Use `azdocli login --profile <name>` to keep several logins side by side.

## Quick Start

```sh
# Set a default project (optional but recommended)
azdocli project MyProject

# List repositories
azdocli repos list

# List pipelines
azdocli pipelines list
```

## Commands

```text
azdocli [OPTIONS] [SUBCOMMAND]

OPTIONS:
    -o, --output <OUTPUT>    Output format [default: text] [possible values: text, json, yaml, markdown]

SUBCOMMANDS:
    artifacts       Manage Azure Artifacts feeds and Universal Packages
    boards          Manage Azure DevOps boards
    check-update    Check whether a newer azdocli has been released
    login           Login to Azure DevOps with a Personal Access Token (PAT)
    logout          Logout from Azure DevOps
    migrate         Migrate one or more team projects between organizations (Experimental)
    pipelines       Manage Azure DevOps pipelines
    project         Set or view the default project
    projects        Manage Azure DevOps team projects, teams and process templates
    releases        Manage Azure DevOps classic releases
    repos           Manage Azure DevOps repos
    update          Install the latest azdocli release in place of this one
    user            Manage user entitlements
    wiki            Manage Azure DevOps wikis
```

Run `azdocli <SUBCOMMAND> --help` for details.

## Output Formats

`-o/--output` prints results as `text` (default), `json`, `yaml` or `markdown`, for scripting and pasting into wikis or issues:

```shell
azdocli repos list -o json | jq -r '.[].name'
azdocli repos pr list --output markdown
azdocli boards work-item show --id 42 -o yaml
```

Formats other than `text` are supported by `repos list`, `repos show`, `repos pr list`, `repos pr show`, `pipelines list`, `pipelines show`, `pipelines runs list`, `boards work-item list`, `boards work-item show` and `boards query`. Other commands accept only `text` and fail with a clear error for any other format.

## Features

- **Repository Management**: List, create, delete, clone, view, browse and manage pull requests
- **Pipeline Management**: List, show and run pipelines, and inspect runs
- **Release Management**: List release definitions and releases, and create a release from a build
- **Artifacts**: Browse feeds and packages, and download or publish Universal Packages
- **Project Management**: Create, delete, list and show team projects, teams, members and process templates
- **Migration (experimental)**: Cross-tenant team-project migration with `azdocli migrate`
- **Board Management**: Work items, queries, area and iteration paths
- **User Management**: Add, list, show, remove and update organization users
- **Default Project**: Set a default project to avoid passing `--project` every time

See the [full documentation](https://github.com/christianhelle/azdocli#readme) for every command and option.

## License

MIT
