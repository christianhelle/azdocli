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
- **Default Project**: Set a default project to avoid specifying --project for every command

### Default Project Management

The `project` command allows you to set and view a default project, eliminating the need to specify `--project` for every command:

```sh
# Set a default project
azdocli project MyDefaultProject

# View the current default project
azdocli project

# All commands will now use the default project if --project is not specified
azdocli repos list                    # Uses default project
azdocli pipelines list               # Uses default project
azdocli repos list --project Other  # Overrides default with "Other"
```

**Default Project Features:**

- **Persistent storage**: Default project is saved in your user configuration
- **Optional override**: Use `--project` to override the default for any command
- **All modules supported**: Works with repos, pipelines, boards, and artifacts
- **Helpful error messages**: Clear feedback when no default is set and no --project is provided

### Repository Management Features

#### Repository Clone Feature

The `repos clone` command allows you to clone all repositories from an Azure DevOps project:

```sh
# Set a default project first (optional but recommended)
azdocli project MyProject

# Clone all repositories from the default project (with confirmation prompt)
azdocli repos clone

# Or override with a specific project
azdocli repos clone --project MyProject

# Clone to a specific directory
azdocli repos clone --target-dir ./repos

# Skip confirmation prompt (useful for automation)
azdocli repos clone --yes

# Clone repositories in parallel for faster execution
azdocli repos clone --parallel

# Control the number of concurrent clone operations (default: 4, max: 8)
azdocli repos clone --parallel --concurrency 6

# Combine all options for maximum efficiency
azdocli repos clone --target-dir ./repos --yes --parallel --concurrency 8
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
# Show details of a repository by name (using default project)
azdocli repos show --id MyRepository

# Or specify a project explicitly
azdocli repos show --id MyRepository --project MyProject
```

**Show Features:**

- **Comprehensive details**: View repository name, ID, URLs, size, and metadata
- **User-friendly formatting**: Emoji icons and formatted output for better readability
- **Clone URLs**: Display both HTTPS and SSH clone URLs
- **File size formatting**: Automatic conversion to KB/MB for better readability
- **Error handling**: Helpful error messages with suggestions when repository not found

#### Repository Delete Feature

The `repos delete` command allows you to delete repositories from an Azure DevOps project:

```sh
# Soft delete a repository by name (using default project) - moves to recycle bin
azdocli repos delete --id MyRepository

# Or specify a project explicitly
azdocli repos delete --id MyRepository --project MyProject

# Hard delete - permanently delete after soft delete (requires manual recycle bin cleanup)
azdocli repos delete --id MyRepository --hard

# Skip confirmation prompt (useful for automation)
azdocli repos delete --id MyRepository --yes

# Combine options for automated hard delete
azdocli repos delete --id MyRepository --hard --yes
```

**Delete Features:**

- **Soft delete by default**: Repositories are moved to recycle bin and can be restored
- **Hard delete option**: Use `--hard` flag for permanent deletion (may require manual cleanup)
- **Confirmation prompts**: Interactive confirmation before deletion to prevent accidents
- **Automation support**: Skip prompts with `--yes` flag for CI/CD scenarios
- **Repository validation**: Verify repository exists before attempting deletion
- **Error handling**: Clear feedback when repository not found or access denied
- **Default project support**: Use with default project or specify --project explicitly

### Pipeline Management Features

#### Pipeline List Feature

The `pipelines list` command allows you to list all pipelines in an Azure DevOps project:

```sh
# List all pipelines in the default project
azdocli pipelines list

# Or specify a project explicitly
azdocli pipelines list --project MyProject
```

**List Features:**

- **Comprehensive listing**: View all pipelines in a project with IDs and names
- **User-friendly formatting**: Easy-to-read table format
- **Error handling**: Helpful error messages when project not found or access denied

#### Pipeline Runs Feature

The `pipelines runs` command shows all builds (runs) of a specified pipeline:

```sh
# Show all runs for a pipeline (using default project)
azdocli pipelines runs --id 42

# Or specify a project explicitly
azdocli pipelines runs --id 42 --project MyProject
```

**Runs Features:**

- **Run history**: View all runs for a specific pipeline
- **Status visibility**: See current state and result of each pipeline run
- **User-friendly formatting**: Clear display of run information

#### Pipeline Show Feature

The `pipelines show` command displays detailed information about a specific pipeline build:

```sh
# Show details of a specific pipeline build (using default project)
azdocli pipelines show --id 42 --build-id 123

# Or specify a project explicitly
azdocli pipelines show --id 42 --project MyProject --build-id 123
```

**Show Features:**

- **Detailed information**: Comprehensive details about a specific pipeline build
- **Debug information**: Access to internal state for troubleshooting purposes
- **Error handling**: Helpful error messages when build not found

#### Pipeline Run Feature

The `pipelines run` command starts a new pipeline run:

```sh
# Run a pipeline (using default project)
azdocli pipelines run --id 42

# Or specify a project explicitly
azdocli pipelines run --id 42 --project MyProject
```

**Run Features:**

- **Pipeline execution**: Start a pipeline with a single command
- **Live updates**: See details of the running build in real-time
- **Error handling**: Clear feedback when pipeline cannot be started

### Board Management Features

#### Work Item Management

The `boards work-item` commands allow you to manage work items in an Azure DevOps project:

```sh
# Show details of a specific work item (using default project)
azdocli boards work-item show --id 123

# Open work item directly in web browser
azdocli boards work-item show --id 123 --web

# Or specify a project explicitly
azdocli boards work-item show --id 123 --project MyProject

# Create a new work item (using default project)
# Supported types: bug, task, user-story, feature, epic
azdocli boards work-item create bug --title "Fix login issue" --description "Users cannot login after password change"

# Update a work item (using default project)
azdocli boards work-item update --id 123 --title "New title" --state "Active" --priority 2

# Delete a work item permanently (using default project)
azdocli boards work-item delete --id 123

# Soft delete a work item by changing state to "Removed"
azdocli boards work-item delete --id 123 --soft-delete
```

**Work Item Features:**

- **Full CRUD operations**: Create, read, update, and delete work items
- **Multiple work item types**: Support for bug, task, user story, feature, and epic
- **Web integration**: Open work items directly in browser with `--web` option
- **Soft delete**: Option to change state to "Removed" instead of permanent deletion
- **Field updates**: Update title, description, state, and priority
- **Default project support**: Use with default project or specify --project explicitly
- **Error handling**: Clear feedback when work item not found or access denied

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
