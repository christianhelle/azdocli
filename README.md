# Azure DevOps CLI

[![Build](https://github.com/christianhelle/azdocli/actions/workflows/build.yml/badge.svg)](https://github.com/christianhelle/azdocli/actions/workflows/build.yml)
[![Security Audit](https://github.com/christianhelle/azdocli/actions/workflows/security-audit.yml/badge.svg)](https://github.com/christianhelle/azdocli/actions/workflows/security-audit.yml)

CLI tool for interacting with Azure DevOps.

## Table of Contents

- [Installation](#installation)
- [Authentication Setup](#authentication-setup)
- [Quick Start](#quick-start)
- [Features](#features)
- [Testing](#testing)
- [Contributing](#contributing)

## Installation

### Quick Install (Recommended)

**Linux and macOS:**

```bash
curl -sSL https://christianhelle.com/azdocli/install | bash
```

**Windows (PowerShell):**

```powershell
iwr -useb https://christianhelle.com/azdocli/install.ps1 | iex
```

These one-liner commands will automatically download and install the latest release for your platform.

### Install from crates.io

You can also install azdocli using Cargo:

```bash
cargo install azdocli
```

This will install the `azdocli` binary, which you can use immediately.

### Install using Snap (Linux)

On Linux systems with Snap support, you can install azdocli directly from the Snap Store:

```bash
snap install azdocli
```

This will install the latest stable version and automatically handle updates.

### Install from GitHub Releases

You can also download pre-built binaries from the [GitHub Releases page](https://github.com/christianhelle/azdocli/releases):

- Windows: `windows-x64.zip` or `windows-arm64.zip`
- macOS: `macos-x64.zip` or `macos-arm64.zip`
- Linux: `linux-x64.zip` or `linux-arm64.zip`

Extract the binary and add it to your PATH.

### Building from Source

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

### Enterprise Azure DevOps Installations

By default `azdocli` targets the Azure DevOps cloud (`https://dev.azure.com`). You can log in to an enterprise or on-premises Azure DevOps installation by providing the server URL instead of a cloud organization name.

The stored base URL must not include the organization or collection segment. Provide the base URL including any application path (for example, `https://devops.mycompany.com` or `https://tfs.mycompany.com/tfs`) and then enter the organization/collection when prompted. You can also paste the full URL including the collection (for example, `https://tfs.mycompany.com/tfs/DefaultCollection`); the last path segment is treated as the organization/collection and is not stored as part of the base URL.

```sh
# Login to an enterprise Azure DevOps Server
azdocli login
# When prompted for organization, enter the server URL, for example:
# - https://devops.mycompany.com
# - https://tfs.mycompany.com/tfs/DefaultCollection
# Then enter the organization/collection name when prompted (not needed if it was part of the URL).
```

The base URL is stored alongside your PAT, so all subsequent commands use that server. You can also create named profiles for cloud and enterprise servers side-by-side:

```sh
azdocli login --profile cloud
azdocli login --profile onprem
```

**Note:** User entitlement management (`azdocli user`) is only supported against the default Azure DevOps cloud host. Commands that manage user licenses will fail with a clear message when a custom base URL is configured.

## Quick Start

```sh
# Login with your Personal Access Token
azdocli login
# You'll be prompted for:
# - Organization name (e.g., "mycompany" from https://dev.azure.com/mycompany)
# - Personal Access Token (the PAT you created above)

# Set a default project (optional but recommended)
azdocli project MyProject

# List repositories
azdocli repos list

# List pipelines
azdocli pipelines list
```

### All Subcommands

```text
azdocli [SUBCOMMAND]

SUBCOMMANDS:
    boards       Manage Azure DevOps boards
    login        Login to Azure DevOps with a Personal Access Token (PAT)
    logout       Logout from Azure DevOps
    migrate      Migrate one or more team projects between organizations
    pipelines    Manage Azure DevOps pipelines
    project      Set or view the default project
    repos        Manage Azure DevOps repos
    user         Manage user entitlements
```

## Features

- **Repository Management**: List, create, delete, clone, view, browse and manage pull requests in repositories
- **Pipeline Management**: Manage Azure DevOps pipelines
- **Project Management**: Create, delete, list, and show Azure DevOps team projects in your organization
- **Migration**: Cross-tenant team-project migration with `azdocli migrate` (see [src/README.md](src/README.md#migrate) for the full guide)
- **Board Management**: Manage Azure DevOps boards
- **Authentication**: Secure login using Personal Access Tokens (PAT)
- **Default Project**: Set a default project to avoid specifying --project for every command

### Default Project Management

The `project` command allows you to set and view a default project,
eliminating the need to specify `--project` for every command:

```sh
# Set a default project
azdocli project MyDefaultProject

# View the current default project
azdocli project

# All commands will now use the default project if --project is not specified
azdocli repos list                  # Uses default project
azdocli pipelines list              # Uses default project
azdocli repos list --project Other  # Overrides default with "Other"
```

**Default Project Features:**

- **Persistent storage**: Default project is saved in your user configuration
- **Optional override**: Use `--project` to override the default for any command
- **All modules supported**: Works with repos, pipelines, boards, and projects
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

#### Repository Browsing Features

The `repos branches`, `repos commits`, `repos files` and `repos file` commands let you inspect the
contents of a repository without cloning it:

```sh
# List the branches of a repository (the default branch is marked)
azdocli repos branches --id MyRepository

# Only show branches matching some text, and cap the number of results
azdocli repos branches --id MyRepository --filter feature --top 20

# List the 25 most recent commits on the default branch
azdocli repos commits --id MyRepository

# Read the history of a specific branch, author or path
azdocli repos commits --id MyRepository --branch develop --author "Christian Helle" --path src --top 50

# List the files and folders at the root of the repository
azdocli repos files --id MyRepository

# List a subfolder, on a specific branch, recursively
azdocli repos files --id MyRepository --path /src --branch develop --recursive

# Print the contents of a single file
azdocli repos file --id MyRepository --path /README.md --branch develop
```

**Browsing Features:**

- **No clone required**: Inspect branches, history and files straight from the CLI
- **Server-side filtering**: Branch, author, path and result limits are applied by Azure DevOps
- **Default branch aware**: Commands fall back to the repository default branch when `--branch` is omitted
- **Pipe friendly**: `repos file` writes the raw file contents to stdout

#### Pull Request Management Features

The `repos pr` commands allow you to manage pull requests within repositories:

##### List Pull Requests

```sh
# List active pull requests for a repository (using default project)
azdocli repos pr list --repo MyRepository

# Or specify a project explicitly
azdocli repos pr list --repo MyRepository --project MyProject

# Filter by state - active (the default), completed, abandoned, or all
azdocli repos pr list --repo MyRepository --status completed

# Filter by author, reviewer or branch, and cap the number of results
azdocli repos pr list --repo MyRepository --creator @me
azdocli repos pr list --repo MyRepository --reviewer alice@example.com
azdocli repos pr list --repo MyRepository --source "feature/my-feature" --target main
azdocli repos pr list --repo MyRepository --top 10
```

##### Show Pull Request Details

```sh
# Show details of a specific pull request (using default project)
azdocli repos pr show --repo MyRepository --id 123

# Or specify a project explicitly
azdocli repos pr show --repo MyRepository --id 123 --project MyProject

# Open the pull request in a browser instead
azdocli repos pr show --repo MyRepository --id 123 --web
```

##### Create Pull Request

```sh
# Create a new pull request with source and target branches (using default project)
azdocli repos pr create --repo MyRepository --source "feature/my-feature" --target "main" --title "My Feature" --description "Description"

# Create with minimal information - target defaults to 'main'
azdocli repos pr create --repo MyRepository --source "feature/my-feature" --title "My Feature"

# Or specify a project explicitly
azdocli repos pr create --repo MyRepository --source "feature/my-feature" --target "develop" --title "My Feature" --description "Description" --project MyProject

# Source branch is required, target defaults to 'main' if not specified
azdocli repos pr create --repo MyRepository --source "bugfix/fix-login"

# Open as a draft, with reviewers, linked work items and labels
azdocli repos pr create --repo MyRepository --source "feature/my-feature" --title "My Feature" \
  --draft --reviewer alice@example.com --reviewer bob@example.com \
  --work-item 1234 --work-item 1235 --label "needs-review"

# Merge automatically once policies pass, then delete the source branch
azdocli repos pr create --repo MyRepository --source "feature/my-feature" --title "My Feature" \
  --auto-complete --delete-source-branch
```

##### Update Pull Request

```sh
# Update title and description (using default project)
azdocli repos pr update --repo MyRepository --id 123 --title "New title" --description "New description"

# Update title only
azdocli repos pr update --repo MyRepository --id 123 --title "New title"

# Update description from markdown file
azdocli repos pr update --repo MyRepository --id 123 --description-file ./description.md

# Update both with file description
azdocli repos pr update --repo MyRepository --id 123 --title "New title" --description-file ./description.md

# Or specify a project explicitly
azdocli repos pr update --repo MyRepository --id 123 --title "New title" --project MyProject
```

##### Show Pull Request Commits

```sh
# Show commits in a specific pull request (using default project)
azdocli repos pr commits --repo MyRepository --id 123

# Or specify a project explicitly
azdocli repos pr commits --repo MyRepository --id 123 --project MyProject
```

##### Complete, Abandon or Reactivate a Pull Request

```sh
# Merge the pull request, squashing the source commits and deleting the branch
azdocli repos pr complete --repo MyRepository --id 123 --merge-strategy squash --delete-source-branch

# Merge without the confirmation prompt (for CI/CD)
azdocli repos pr complete --repo MyRepository --id 123 --yes

# Set the pull request to complete automatically once policies pass
azdocli repos pr complete --repo MyRepository --id 123 --auto-complete --yes

# Complete despite failing branch policies, recording a reason
azdocli repos pr complete --repo MyRepository --id 123 --bypass-policy --bypass-reason "hotfix"

# Close a pull request without merging, and reopen it later
azdocli repos pr abandon --repo MyRepository --id 123 --yes
azdocli repos pr reactivate --repo MyRepository --id 123
```

##### Manage Reviewers

Reviewers can be given as an email address, an identity ID, or `@me` for the
signed-in user.

```sh
# List reviewers with their votes
azdocli repos pr reviewers list --repo MyRepository --id 123

# Add one or more reviewers, optionally as required reviewers
azdocli repos pr reviewers add --repo MyRepository --id 123 --reviewer alice@example.com --reviewer bob@example.com
azdocli repos pr reviewers add --repo MyRepository --id 123 --reviewer alice@example.com --required

# Remove a reviewer
azdocli repos pr reviewers remove --repo MyRepository --id 123 --reviewer alice@example.com

# Cast your own vote
azdocli repos pr reviewers vote --repo MyRepository --id 123 --vote approve
azdocli repos pr reviewers vote --repo MyRepository --id 123 --vote wait-for-author
```

Valid votes are `approve`, `approve-with-suggestions`, `reset`, `wait-for-author` and `reject`.

##### Read and Write Comments

```sh
# Read the discussion (system-generated threads are hidden unless --all is given)
azdocli repos pr threads --repo MyRepository --id 123
azdocli repos pr threads --repo MyRepository --id 123 --all

# Start a new thread
azdocli repos pr comment add --repo MyRepository --id 123 --message "Looks good to me"

# Start a thread anchored to a file and line
azdocli repos pr comment add --repo MyRepository --id 123 --message "Needs a null check" --file "/src/main.rs" --line 42

# Reply to an existing thread, then resolve it
azdocli repos pr comment reply --repo MyRepository --id 123 --thread 7 --message "Fixed in the latest push"
azdocli repos pr comment resolve --repo MyRepository --id 123 --thread 7
azdocli repos pr comment resolve --repo MyRepository --id 123 --thread 7 --status wont-fix
```

Valid thread statuses are `fixed`, `wont-fix`, `closed`, `by-design`, `active` and `pending`.

**Pull Request Features:**

- **Server-side filtering**: List by state, author, reviewer, source branch or target branch
- **Comprehensive details**: Show displays status, draft flag, merge status, reviewers and their votes, labels, linked work items and open comment threads
- **Branch specification**: Specify source branch (required) and target branch (defaults to 'main')
- **Flexible creation**: Create pull requests with drafts, reviewers, linked work items, labels and auto-complete
- **Flexible updates**: Update pull request title and/or description (including from markdown file)
- **Merge control**: Complete with a chosen merge strategy, delete the source branch, or bypass policy with a recorded reason
- **Review workflow**: Add and remove reviewers, and cast votes, using email addresses, identity IDs or `@me`
- **Discussion**: Read threads, start file- and line-anchored comments, reply, and resolve
- **Branch validation**: Automatic formatting of branch names with refs/heads/ prefix
- **Repository validation**: Verify repository exists before creating or updating pull request
- **Authentication handling**: Proper error messages when not logged in
- **Default project support**: Use with default project or specify --project explicitly
- **Error handling**: Clear feedback for invalid pull request IDs, unmergeable pull requests, or missing repositories
- **Commit tracking**: View all commits included in a pull request with detailed information

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

- **Detailed information**: Run number, pipeline, state, result and timestamps
- **Web link**: The URL of the run in the Azure DevOps web interface
- **Error handling**: Helpful error messages when build not found

#### Pipeline Run Feature

The `pipelines run` command queues a new pipeline run:

```sh
# Run a pipeline (using default project)
azdocli pipelines run --id 42

# Or specify a project explicitly
azdocli pipelines run --id 42 --project MyProject

# Run a specific branch
azdocli pipelines run --id 42 --branch develop

# Pass pipeline variables (repeat --variable for more than one)
azdocli pipelines run --id 42 --variable environment=staging --variable verbose=true
```

**Run Features:**

- **Pipeline execution**: Start a pipeline with a single command
- **Branch selection**: Queue the run against any branch with `--branch`
- **Runtime variables**: Set pipeline variables with repeatable `--variable NAME=VALUE` arguments
- **Run summary**: Prints the new run number, state and web URL
- **Error handling**: Clear feedback when pipeline cannot be started

#### Pipeline Logs Feature

The `pipelines logs` command lists the logs of a run, or prints one of them:

```sh
# List the logs produced by a run
azdocli pipelines logs --id 42 --build-id 123

# Print the contents of a single log
azdocli pipelines logs --id 42 --build-id 123 --log-id 7
```

**Logs Features:**

- **Log inventory**: See every log a run produced, with line counts and timestamps
- **Full log text**: Print a log to stdout so it can be piped, searched or saved
- **Error handling**: Clear feedback when the run or log does not exist

#### Pipeline Artifacts Feature

The `pipelines artifacts` command lists the artifacts a run published:

```sh
# List the artifacts of a run
azdocli pipelines artifacts --build-id 123

# Or specify a project explicitly
azdocli pipelines artifacts --build-id 123 --project MyProject
```

**Artifacts Features:**

- **Artifact inventory**: See every artifact published by a run
- **Download URLs**: Print the download URL of each artifact
- **Error handling**: Clear feedback when the run does not exist

#### Variable Groups and Service Connections

The `pipelines variable-group` and `pipelines service-connection` commands inspect the resources a
pipeline consumes:

```sh
# List the variable groups of the default project
azdocli pipelines variable-group list

# Filter by name and cap the number of results
azdocli pipelines variable-group list --name "release" --top 10

# Show a variable group and its variables (secret values are never returned by Azure DevOps)
azdocli pipelines variable-group show --id 7

# List the service connections of the default project
azdocli pipelines service-connection list

# Only connections of one type
azdocli pipelines service-connection list --type azurerm

# Show a single service connection
azdocli pipelines service-connection show --id 00000000-0000-0000-0000-000000000000
```

**Library Features:**

- **Variable discovery**: See which variable groups exist and what they define
- **Secret safety**: Secret variables are shown as `<secret>`; the API never returns their values
- **Service connection inventory**: List connections with their type and readiness, filtered by type
- **Default project support**: Use with default project or specify --project explicitly

### Board Management Features

#### Work Item Management

The `boards work-item` commands allow you to manage work items in an Azure DevOps project:

```sh
# List work items assigned to me (using default project)
azdocli boards work-item list

# List work items with filters
azdocli boards work-item list --state "Active" --work-item-type "Bug" --limit 20

# Or specify a project explicitly
azdocli boards work-item list --project MyProject

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
- **List my work items**: View work items assigned to you with filtering options
- **Multiple work item types**: Support for bug, task, user story, feature, and epic
- **Filtering**: Filter by state, work item type, and limit number of results
- **Web integration**: Open work items directly in browser with `--web` option
- **Soft delete**: Option to change state to "Removed" instead of permanent deletion
- **Field updates**: Update title, description, state, and priority
- **Default project support**: Use with default project or specify --project explicitly
- **Error handling**: Clear feedback when work item not found or access denied

#### Work Item Comments

The `boards work-item comment` commands read and write the discussion on a work item:

```sh
# List the comments on a work item
azdocli boards work-item comment list --id 123

# Only show the most recent comments
azdocli boards work-item comment list --id 123 --top 5

# Add a comment
azdocli boards work-item comment add --id 123 --message "Reproduced on the staging build"
```

**Comment Features:**

- **Read the discussion**: See every comment with its author and timestamp
- **Add comments**: Post a comment from the command line or a script
- **Safe rendering**: Terminal control characters in remote text are escaped rather than executed

#### Work Item Types

The `boards work-item types` command lists the work item types a project defines:

```sh
# List the work item types of the default project
azdocli boards work-item types

# Or specify a project explicitly
azdocli boards work-item types --project MyProject
```

#### WIQL Queries

The `boards query` command runs any WIQL query and lists the work items it returns:

```sh
# Run a WIQL query against the default project
azdocli boards query --wiql "SELECT [System.Id] FROM WorkItems WHERE [System.State] = 'Active'"

# Cap the number of results
azdocli boards query --wiql "SELECT [System.Id] FROM WorkItems" --limit 10
```

**Query Features:**

- **Arbitrary WIQL**: Anything the Azure DevOps query editor accepts
- **Full work item details**: Results are shown in the same table as `work-item list`
- **Result limits**: Cap the number of work items fetched with `--limit`

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
