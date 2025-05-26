# Azure DevOps CLI

[![Build](https://github.com/christianhelle/azdocli/actions/workflows/build.yml/badge.svg)](https://github.com/christianhelle/azdocli/actions/workflows/build.yml)
[![Security Audit](https://github.com/christianhelle/azdocli/actions/workflows/security-audit.yml/badge.svg)](https://github.com/christianhelle/azdocli/actions/workflows/security-audit.yml)

CLI tool for interacting with Azure DevOps.

## Features

- **Repository Management**: List, create, update, delete, and clone repositories
- **Pipeline Management**: Manage Azure DevOps pipelines
- **Board Management**: Manage Azure DevOps boards  
- **Artifact Management**: Manage Azure DevOps artifacts
- **Authentication**: Secure login using Personal Access Tokens (PAT)

### Repository Clone Feature

The `repos clone` command allows you to clone all repositories from an Azure DevOps project:

```sh
# Clone all repositories from a project (with confirmation prompt)
azdocli repos clone --project MyProject

# Clone to a specific directory
azdocli repos clone --project MyProject --target-dir ./repos

# Skip confirmation prompt (useful for automation)
azdocli repos clone --project MyProject --yes
```

```sh
CLI tool for interacting with Azure DevOps

USAGE:
    azdocli [SUBCOMMAND]

OPTIONS:
    -h, --help       Print help information
    -V, --version    Print version information

SUBCOMMANDS:
    artifacts    Manage Azure DevOps artifacts
    boards       Manage Azure DevOps boards
    help         Print this message or the help of the given subcommand(s)
    login        Login to Azure DevOps with a Personal Access Token (PAT)
    logout       Logout from Azure DevOps
    pipelines    Manage Azure DevOps pipelines
    repos        Manage Azure DevOps repos
```

## Installation

*This section will contain information about installation methods once the tool is ready for distribution.*

## Usage

*This section will contain usage examples once the tool's functionality is implemented.*

## Building from Source

```bash
# Clone the repository
git clone https://github.com/christianhelle/azdocli.git
cd azdocli

# Build the project
cargo build

# Run tests
cargo test

# Run the CLI
cargo run -- <command>
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
