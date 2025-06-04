# Azure DevOps CLI

[![Build](https://github.com/christianhelle/azdocli/actions/workflows/build.yml/badge.svg)](https://github.com/christianhelle/azdocli/actions/workflows/build.yml)
[![Security Audit](https://github.com/christianhelle/azdocli/actions/workflows/security-audit.yml/badge.svg)](https://github.com/christianhelle/azdocli/actions/workflows/security-audit.yml)

CLI tool for interacting with Azure DevOps.

## Features

- **Repository Management**: List, create, delete, clone, view, and manage pull requests in repositories
- **Pipeline Management**: Manage Azure DevOps pipelines
- **Board Management**: Manage Azure DevOps boards
- **Authentication**: Secure login using Personal Access Tokens (PAT)
- **Default Project**: Set a default project to avoid specifying --project for every command

### Default Project Management

The `project` command allows you to set and view a default project,
eliminating the need to specify `--project` for every command:

```sh
# Set a default project
ado project MyDefaultProject

# View the current default project
ado project

# All commands will now use the default project if --project is not specified
ado repos list                  # Uses default project
ado pipelines list              # Uses default project
ado repos list --project Other  # Overrides default with "Other"
```

**Default Project Features:**

- **Persistent storage**: Default project is saved in your user configuration
- **Optional override**: Use `--project` to override the default for any command
- **All modules supported**: Works with repos, pipelines, and boards
- **Helpful error messages**: Clear feedback when no default is set and no --project is provided

### Repository Management Features

#### Repository Clone Feature

The `repos clone` command allows you to clone all repositories from an Azure DevOps project:

```sh
# Set a default project first (optional but recommended)
ado project MyProject

# Clone all repositories from the default project (with confirmation prompt)
ado repos clone

# Or override with a specific project
ado repos clone --project MyProject

# Clone to a specific directory
ado repos clone --target-dir ./repos

# Skip confirmation prompt (useful for automation)
ado repos clone --yes

# Clone repositories in parallel for faster execution
ado repos clone --parallel

# Control the number of concurrent clone operations (default: 4, max: 8)
ado repos clone --parallel --concurrency 6

# Combine all options for maximum efficiency
ado repos clone --target-dir ./repos --yes --parallel --concurrency 8
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
ado repos show --id MyRepository

# Or specify a project explicitly
ado repos show --id MyRepository --project MyProject
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
ado repos delete --id MyRepository

# Or specify a project explicitly
ado repos delete --id MyRepository --project MyProject

# Hard delete - permanently delete after soft delete (requires manual recycle bin cleanup)
ado repos delete --id MyRepository --hard

# Skip confirmation prompt (useful for automation)
ado repos delete --id MyRepository --yes

# Combine options for automated hard delete
ado repos delete --id MyRepository --hard --yes
```

**Delete Features:**

- **Soft delete by default**: Repositories are moved to recycle bin and can be restored
- **Hard delete option**: Use `--hard` flag for permanent deletion (may require manual cleanup)
- **Confirmation prompts**: Interactive confirmation before deletion to prevent accidents
- **Automation support**: Skip prompts with `--yes` flag for CI/CD scenarios
- **Repository validation**: Verify repository exists before attempting deletion
- **Error handling**: Clear feedback when repository not found or access denied
- **Default project support**: Use with default project or specify --project explicitly

#### Pull Request Management Features

The `repos pr` commands allow you to manage pull requests within repositories:

##### List Pull Requests

```sh
# List all pull requests for a repository (using default project)
ado repos pr list --repo MyRepository

# Or specify a project explicitly
ado repos pr list --repo MyRepository --project MyProject
```

##### Show Pull Request Details

```sh
# Show details of a specific pull request (using default project)
ado repos pr show --repo MyRepository --id 123

# Or specify a project explicitly
ado repos pr show --repo MyRepository --id 123 --project MyProject
```

##### Create Pull Request

```sh
# Create a new pull request with source and target branches (using default project)
ado repos pr create --repo MyRepository --source "feature/my-feature" --target "main" --title "My Feature" --description "Description"

# Create with minimal information - target defaults to 'main'
ado repos pr create --repo MyRepository --source "feature/my-feature" --title "My Feature"

# Or specify a project explicitly
ado repos pr create --repo MyRepository --source "feature/my-feature" --target "develop" --title "My Feature" --description "Description" --project MyProject

# Source branch is required, target defaults to 'main' if not specified
ado repos pr create --repo MyRepository --source "bugfix/fix-login"
```

**Pull Request Features:**

- **Repository filtering**: List shows only pull requests for the specified repository
- **Comprehensive details**: Show command displays ID, title, description, status, branches, and creation date
- **Branch specification**: Specify source branch (required) and target branch (defaults to 'main')
- **Flexible creation**: Create pull requests with or without title/description
- **Branch validation**: Automatic formatting of branch names with refs/heads/ prefix
- **Repository validation**: Verify repository exists before creating pull request
- **Authentication handling**: Proper error messages when not logged in
- **Default project support**: Use with default project or specify --project explicitly
- **Error handling**: Clear feedback for invalid pull request IDs or missing repositories

### Pipeline Management Features

#### Pipeline List Feature

The `pipelines list` command allows you to list all pipelines in an Azure DevOps project:

```sh
# List all pipelines in the default project
ado pipelines list

# Or specify a project explicitly
ado pipelines list --project MyProject
```

**List Features:**

- **Comprehensive listing**: View all pipelines in a project with IDs and names
- **User-friendly formatting**: Easy-to-read table format
- **Error handling**: Helpful error messages when project not found or access denied

#### Pipeline Runs Feature

The `pipelines runs` command shows all builds (runs) of a specified pipeline:

```sh
# Show all runs for a pipeline (using default project)
ado pipelines runs --id 42

# Or specify a project explicitly
ado pipelines runs --id 42 --project MyProject
```

**Runs Features:**

- **Run history**: View all runs for a specific pipeline
- **Status visibility**: See current state and result of each pipeline run
- **User-friendly formatting**: Clear display of run information

#### Pipeline Show Feature

The `pipelines show` command displays detailed information about a specific pipeline build:

```sh
# Show details of a specific pipeline build (using default project)
ado pipelines show --id 42 --build-id 123

# Or specify a project explicitly
ado pipelines show --id 42 --project MyProject --build-id 123
```

**Show Features:**

- **Detailed information**: Comprehensive details about a specific pipeline build
- **Debug information**: Access to internal state for troubleshooting purposes
- **Error handling**: Helpful error messages when build not found

#### Pipeline Run Feature

The `pipelines run` command starts a new pipeline run:

```sh
# Run a pipeline (using default project)
ado pipelines run --id 42

# Or specify a project explicitly
ado pipelines run --id 42 --project MyProject
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
ado boards work-item show --id 123

# Open work item directly in web browser
ado boards work-item show --id 123 --web

# Or specify a project explicitly
ado boards work-item show --id 123 --project MyProject

# Create a new work item (using default project)
# Supported types: bug, task, user-story, feature, epic
ado boards work-item create bug --title "Fix login issue" --description "Users cannot login after password change"

# Update a work item (using default project)
ado boards work-item update --id 123 --title "New title" --state "Active" --priority 2

# Delete a work item permanently (using default project)
ado boards work-item delete --id 123

# Soft delete a work item by changing state to "Removed"
ado boards work-item delete --id 123 --soft-delete
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
    ado [SUBCOMMAND]

OPTIONS:
    -h, --help       Print help information
    -V, --version    Print version information

SUBCOMMANDS:
    boards       Manage Azure DevOps boards
    help         Print this message or the help of the given subcommand(s)
    login        Login to Azure DevOps with a Personal Access Token (PAT)
    logout       Logout from Azure DevOps
    pipelines    Manage Azure DevOps pipelines
    repos        Manage Azure DevOps repos
```

## Installation

### Install from crates.io

The easiest way to install azdocli is using Cargo:

```bash
cargo install azdocli
```

This will install the `ado` binary which you can use immediately.

### Install from Snapcraft

For Linux systems, you can install from the Snap Store:

```bash
sudo snap install azdocli
```

This provides automatic updates and easy installation on most Linux distributions.

### Install from GitHub Releases

You can also download pre-built binaries from the [GitHub Releases page](https://github.com/christianhelle/azdocli/releases):

- Windows: `windows-x64.zip` or `windows-arm64.zip`
- macOS: `macos-x64.zip` or `macos-arm64.zip`  
- Linux: `linux-x64.zip` or `linux-arm64.zip`

Extract the binary and add it to your PATH.

## Authentication Setup

Before using the CLI, you need to create a Personal Access Token (PAT) in Azure DevOps:

### Creating a Personal Access Token

1. **Navigate to Azure DevOps**:
   - Sign in to your Azure DevOps organization (`https://dev.azure.com/{yourorganization}`)
   - Click on your profile picture in the top right corner
   - Select **Personal Access Tokens**

2. **Create New Token**:
   - Click **+ New Token**
   - Enter a descriptive name (e.g., "azdocli-token")
   - Select your organization
   - Set expiration date (recommended: 90 days or less)

3. **Configure Required Scopes**:
   - **Code**: Read & write (for repository operations)
   - **Build**: Read & execute (for pipeline operations) 
   - **Work Items**: Read & write (for board operations)
   - **Project and Team**: Read (for project operations)

4. **Save Your Token**:
   - Click **Create**
   - **⚠️ Important**: Copy the token immediately and store it securely
   - The token will not be shown again

**Security Best Practices**:
- Never commit your PAT to version control
- Use environment variables or secure storage for automation
- Regularly rotate your tokens
- Use the minimum required permissions

## Usage

First, login to Azure DevOps using the PAT you created:

```sh
# Login with your Personal Access Token
ado login
# You'll be prompted for:
# - Organization name (e.g., "mycompany" from https://dev.azure.com/mycompany)
# - Personal Access Token (the PAT you created above)

# Set a default project (optional but recommended)
ado project MyProject
```

### Basic Examples

```sh
# Repository management
ado repos list                           # List all repositories
ado repos show --id MyRepo               # Show repository details
ado repos clone                          # Clone all repositories

# Pull request management
ado repos pr list --repo MyRepo          # List pull requests for a repository
ado repos pr show --repo MyRepo --id 123 # Show pull request details
ado repos pr create --repo MyRepo --source "feature/my-feature" --title "My Feature" # Create a new pull request

# Pipeline management
ado pipelines list                       # List all pipelines
ado pipelines runs --id 42               # Show pipeline runs
ado pipelines show --id 42 --build-id 123 # Show build details
```

For detailed examples and features, see the respective sections below.

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

## Testing

The project includes integration tests that verify the core repository management functionality against a real Azure DevOps instance.

### Setting up Test Configuration

To run the integration tests, you need to create a test configuration file with your Azure DevOps credentials:

1. Copy the template file:

   ```bash
   cp test_config.json.template test_config.json
   ```

2. Edit `test_config.json` with your Azure DevOps details:

  ```json
   {
     "organization": "your-organization-name",
     "pat": "your-personal-access-token",
     "project": "your-test-project-name"
   }
   ```

3. Make sure you have:

   - A valid Azure DevOps Personal Access Token (PAT) with repository permissions
   - Access to an Azure DevOps project where you can create/delete test repositories
   - Git installed and available in your PATH (for clone testing)

### Running Tests

The integration tests are marked with `#[ignore]` by default to prevent accidental execution without proper configuration.

```bash
# Run all tests including integration tests
cargo test -- --ignored

# Run only the repository smoke tests
cargo test test_create_show_clone_delete_repository -- --ignored

# Run the repository listing test
cargo test test_list_repositories -- --ignored

# Run regular unit tests only (currently none)
cargo test
```

### Test Coverage

The integration tests cover the following repository operations:

- **Create**: Creates a new repository in your Azure DevOps project
- **Show**: Retrieves and verifies repository details
- **Clone**: Attempts to clone the repository (to temporary directory)
- **Delete**: Performs hard delete to clean up test repositories

⚠️ **Important**:
The tests create and delete actual repositories in your Azure DevOps project.
Make sure to use a test project and not a production environment.

### Security Notes

- The `test_config.json` file is automatically ignored by Git to prevent accidental credential commits
- Store your PAT securely and never commit it to version control
- Use a PAT with minimal required permissions (repository read/write)
- Consider using a dedicated test organization or project for running these tests

## Contributing

Contributions are welcome! Please read our [Contributing Guidelines](CONTRIBUTING.md) for details on:

- Code style and patterns
- Development setup
- Testing procedures  
- PR description requirements
- How to keep documentation updated

Please ensure your PR descriptions are verbose and follow the guidelines in [CONTRIBUTING.md](CONTRIBUTING.md).
