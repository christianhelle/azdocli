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

# Clone repositories in parallel for faster execution
azdocli repos clone --project MyProject --parallel

# Control the number of concurrent clone operations (default: 4, max: 8)
azdocli repos clone --project MyProject --parallel --concurrency 6

# Combine all options for maximum efficiency
azdocli repos clone --project MyProject --target-dir ./repos --yes --parallel --concurrency 8
```

**Features:**
- **Bulk cloning**: Clone all repositories from a project with a single command
- **Target directory**: Specify where to clone repositories (defaults to current directory)
- **Confirmation prompts**: Interactive confirmation with repository listing before cloning
- **Automation support**: Skip prompts with `--yes` flag for CI/CD scenarios
- **Parallel execution**: Use `--parallel` flag to clone multiple repositories simultaneously
- **Concurrency control**: Adjust the number of concurrent operations with `--concurrency` (1-8)
- **Error handling**: Comprehensive error reporting for failed clones
- **Progress tracking**: Real-time feedback on clone operations

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
