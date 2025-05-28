# Azure DevOps CLI

[![Build](https://github.com/christianhelle/azdocli/actions/workflows/build.yml/badge.svg)](https://github.com/christianhelle/azdocli/actions/workflows/build.yml)
[![Security Audit](https://github.com/christianhelle/azdocli/actions/workflows/security-audit.yml/badge.svg)](https://github.com/christianhelle/azdocli/actions/workflows/security-audit.yml)

CLI tool for interacting with Azure DevOps.

## Features

- **Repository Management**: List, create, update, delete, clone, and view repositories
- **Pipeline Management**: Manage Azure DevOps pipelines
- **Board Management**: Manage Azure DevOps boards  
- **Artifact Management**: Manage Azure DevOps artifacts
- **Authentication**: Secure login using Personal Access Tokens (PAT)

### Repository Management Features

#### Repository Clone Feature

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

**Clone Features:**

- **Bulk cloning**: Clone all repositories from a project with a single command
- **Target directory**: Specify where to clone repositories (defaults to current directory)
- **Confirmation prompts**: Interactive confirmation with repository listing before cloning
- **Automation support**: Skip prompts with `--yes` flag for CI/CD scenarios
- **Parallel execution**: Use `--parallel` flag to clone multiple repositories simultaneously
- **Concurrency control**: Adjust the number of concurrent operations with `--concurrency` (1-8)
- **Error handling**: Comprehensive error reporting for failed clones
- **Progress tracking**: Real-time feedback on clone operations

#### Repository Show Feature

The `repos show` command displays detailed information about a specific repository:

```sh
# Show details of a repository by name
azdocli repos show --id MyRepository --project MyProject
```

**Show Features:**

- **Comprehensive details**: View repository name, ID, URLs, size, and metadata
- **User-friendly formatting**: Emoji icons and formatted output for better readability
- **Clone URLs**: Display both HTTPS and SSH clone URLs
- **File size formatting**: Automatic conversion to KB/MB for better readability
- **Error handling**: Helpful error messages with suggestions when repository not found

### Pipeline Management Features

#### Pipeline List Feature

The `pipelines list` command allows you to list all pipelines in an Azure DevOps project:

```sh
# List all pipelines in a project
azdocli pipelines list --project MyProject
```

**List Features:**

- **Comprehensive listing**: View all pipelines in a project with IDs and names
- **User-friendly formatting**: Easy-to-read table format
- **Error handling**: Helpful error messages when project not found or access denied

#### Pipeline Runs Feature

The `pipelines runs` command shows all builds (runs) of a specified pipeline:

```sh
# Show all runs for a pipeline
azdocli pipelines runs --id 42 --project MyProject
```

**Runs Features:**

- **Run history**: View all runs for a specific pipeline
- **Status visibility**: See current state and result of each pipeline run
- **User-friendly formatting**: Clear display of run information

#### Pipeline Show Feature

The `pipelines show` command displays detailed information about a specific pipeline build:

```sh
# Show details of a specific pipeline build
azdocli pipelines show --id 42 --project MyProject --build-id 123
```

**Show Features:**

- **Detailed information**: Comprehensive details about a specific pipeline build
- **Debug information**: Access to internal state for troubleshooting purposes
- **Error handling**: Helpful error messages when build not found

#### Pipeline Run Feature

The `pipelines run` command starts a new pipeline run:

```sh
# Run a pipeline
azdocli pipelines run --id 42 --project MyProject
```

**Run Features:**

- **Pipeline execution**: Start a pipeline with a single command
- **Live updates**: See details of the running build in real-time
- **Error handling**: Clear feedback when pipeline cannot be started

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
