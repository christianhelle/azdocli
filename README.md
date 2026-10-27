# Azure DevOps CLI

[![Build](https://github.com/christianhelle/azdocli/actions/workflows/build.yml/badge.svg)](https://github.com/christianhelle/azdocli/actions/workflows/build.yml)
[![Security Audit](https://github.com/christianhelle/azdocli/actions/workflows/security-audit.yml/badge.svg)](https://github.com/christianhelle/azdocli/actions/workflows/security-audit.yml)
![Crates.io Version](https://img.shields.io/crates/v/azdocli)

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

**How azdocli stores your PAT**:

`azdocli login` never writes the PAT to disk in plaintext. The `pat` field in `~/.azdocli/credentials.json` and `~/.azdocli/profiles/*.json` holds an encrypted value (`enc:v1:...`) bound to the machine where you logged in:

- **Windows**: encrypted with DPAPI using your Windows account's keys, plus this machine's id. Another user account, or the same file on another machine, cannot decrypt it.
- **macOS / Linux**: encrypted with AES-256-GCM using a key derived from this machine's id and your home directory path. These inputs are not secret, so this is machine binding, not secret-keyed encryption. The real access control is the file permission: the file is only readable by your user (`0600`).

Copying the value out of the file does not give you a usable token, and azdocli on another machine cannot decrypt a copied file, so there you run `azdocli login` again. (See the limits below for what an attacker can still do on macOS/Linux.) Credentials saved by older versions in plaintext are encrypted automatically the next time azdocli reads them.

Limits: any program running as your user on the same machine can decrypt the PAT, just as azdocli does. On macOS/Linux, someone who has both the file and the machine's id (for example from a full-disk backup) can also decrypt it.

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
    artifacts    Manage Azure Artifacts feeds and Universal Packages
    boards       Manage Azure DevOps boards
    login        Login to Azure DevOps with a Personal Access Token (PAT)
    logout       Logout from Azure DevOps
    migrate      Migrate one or more team projects between organizations
    pipelines    Manage Azure DevOps pipelines
    project      Set or view the default project
    releases     Manage Azure DevOps classic releases
    repos        Manage Azure DevOps repos
    user         Manage user entitlements
```

## Features

- **Repository Management**: List, create, delete, clone, view, browse and manage pull requests in repositories
- **Pipeline Management**: Manage Azure DevOps pipelines
- **Release Management**: List release definitions and releases, show a release, and create a release from a specific build
- **Artifacts**: Browse Azure Artifacts feeds and packages, and download or publish Universal Packages
- **Project Management**: Create, delete, list, and show Azure DevOps team projects, their teams, team members and process templates
- **Migration**: Cross-tenant team-project migration with `azdocli migrate` (see [docs/migrate.md](docs/migrate.md) for the full guide)
- **Board Management**: Manage Azure DevOps boards
- **User Management**: Add, list, show, remove, and update organization users
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

### Team Project Features

The `projects` commands manage the team projects of your organization, and the teams inside them:

```sh
# List every team in the default project
azdocli projects teams

# Only the teams you belong to, capped at 10 results
azdocli projects teams --mine --top 10

# List the members of a team (administrators are marked)
azdocli projects members --team "MyProject Team"

# Or specify a project explicitly
azdocli projects members --team "MyProject Team" --project MyProject

# List the process templates, with the names and IDs accepted by projects create --process
azdocli projects processes
```

**Team Project Features:**

- **Team discovery**: List the teams of a project, optionally only your own
- **Team membership**: See who is on a team and which of them are administrators
- **Process templates**: Discover the processes available before creating a project
- **Default project support**: Use with default project or specify --project explicitly

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

# Leave out --repo to list pull requests across every repository in the project
azdocli repos pr list

# Your pull requests, like the "Mine" tab in the web portal: the ones you
# created and the ones assigned to you for review, with your vote
azdocli repos pr list --mine
azdocli repos pr list --mine --status completed --repo MyRepository

# Filter by state - active (the default), completed, abandoned, or all.
# Statuses other than active return at most 50 pull requests unless --top is given
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

##### Manage Linked Work Items

Work items can also be linked when the pull request is created, with
`repos pr create --work-item`.

```sh
# List the linked work items with their type, state and title
azdocli repos pr work-item list --repo MyRepository --id 123

# Link one or more work items, repeating the flag or separating IDs with commas
azdocli repos pr work-item add --repo MyRepository --id 123 --work-item 42 --work-item 43
azdocli repos pr work-item add --repo MyRepository --id 123 --work-item 42,43

# Unlink a work item
azdocli repos pr work-item remove --repo MyRepository --id 123 --work-item 42
```

A work item that is already linked is reported and skipped, so `add` can be run
again safely.

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

The `pipelines` commands follow the shape of the official Azure DevOps CLI (`az pipelines`) for the
read commands and `run`: the commands below take the same names and arguments as their `az pipelines`
counterparts. Commands that change pipelines (such as `create`, `update`, `delete`, `folder create`,
`runs tag add` and `variable-group create`), the `build`, `release`, `pool`, `agent` and `queue`
groups, and global arguments such as `--output` and `--query` are not supported.

> **Breaking change:** earlier versions used their own command shape. The old forms map to the new
> ones like this:
>
> | Before | Now |
> | --- | --- |
> | `pipelines runs --id 42` | `pipelines runs list --pipeline-ids 42` |
> | `pipelines show --id 42 --build-id 123` | `pipelines runs show --id 123` |
> | `pipelines logs --id 42 --build-id 123` | `pipelines runs logs --run-id 123` |
> | `pipelines artifacts --build-id 123` | `pipelines runs artifact list --run-id 123` |
> | `pipelines run --id 42 --variable a=1 --variable b=2` | `pipelines run --id 42 --variables a=1 b=2` |
> | `pipelines variable-group list --name x` | `pipelines variable-group list --group-name x` (`--name` still works) |
>
> `pipelines show` now shows the pipeline itself rather than one of its runs.

#### Pipelines

```sh
# List the pipelines of the default project (or pass --project MyProject)
azdocli pipelines list

# Filter by name (use * as a wildcard), folder or repository, and order or cap the results
azdocli pipelines list --name "web*" --folder-path team/web --top 10 --query-order NameAsc
azdocli pipelines list --repository MyRepo
azdocli pipelines list --repository owner/repo --repository-type github

# Show a pipeline by ID, or by name (add --folder-path when names repeat across folders)
azdocli pipelines show --id 42
azdocli pipelines show --name web-ci --folder-path team/web

# Open the pipeline in the browser
azdocli pipelines show --id 42 --open

# Queue a run, by ID or by name
azdocli pipelines run --id 42
azdocli pipelines run --name web-ci --branch develop
azdocli pipelines run --id 42 --commit-id 1a2b3c4d

# Set variables and template parameters (space separated NAME=VALUE pairs)
azdocli pipelines run --id 42 --variables environment=staging verbose=true --parameters image=ubuntu-latest

# Open the new run in the browser
azdocli pipelines run --id 42 --open

# List the folders pipelines are organized in
azdocli pipelines folder list
azdocli pipelines folder list --path team --query-order Asc

# List the variables defined on a pipeline in Azure DevOps (the Variables button of the pipeline
# editor). Variables declared in the YAML file are not included. Secret values are never returned.
azdocli pipelines variable list --pipeline-id 42
azdocli pipelines variable list --pipeline-name web-ci
```

#### Pipeline Runs

```sh
# List the runs of the project, or of one or more pipelines. Without --top only the first page of
# results is shown, with a note when more runs exist
azdocli pipelines runs list
azdocli pipelines runs list --pipeline-ids 42 43 --top 10

# Filter by branch, status, result, reason, requester or tags, and choose the order
azdocli pipelines runs list --branch main --result failed --query-order QueueTimeDesc
azdocli pipelines runs list --status inProgress --reason pullRequest
azdocli pipelines runs list --requested-for "someone@example.com" --tags nightly release

# Show a run, or open it in the browser
azdocli pipelines runs show --id 123
azdocli pipelines runs show --id 123 --open

# List the artifacts and tags of a run
azdocli pipelines runs artifact list --run-id 123
azdocli pipelines runs tag list --run-id 123

# List the logs of a run, or print one of them (not part of az pipelines)
azdocli pipelines runs logs --run-id 123
azdocli pipelines runs logs --run-id 123 --log-id 7
```

**Pipeline Features:**

- **Same shape as `az pipelines`**: `list`, `show`, `run`, `runs list/show`, `runs artifact list`, `runs tag list`, `folder list`, `variable list` and `variable-group list/show` take the same arguments as in `az pipelines`
- **Pipelines by name**: `show`, `run` and `variable list` accept a pipeline name instead of an ID
- **Run filters**: Filter runs across the project by pipeline, branch, status, result, reason, requester and tags
- **Full log text**: Print a log to stdout so it can be piped, searched or saved
- **Browser integration**: `--open` (or `--web`) opens a pipeline or run in the Azure DevOps web interface
- **Default project support**: Use with default project or specify --project explicitly

#### Variable Groups and Service Connections

The `pipelines variable-group` and `pipelines service-connection` commands inspect the resources a
pipeline consumes:

```sh
# List the variable groups of the default project
azdocli pipelines variable-group list

# Filter by name, order by ID and cap the number of results
azdocli pipelines variable-group list --group-name "release" --query-order Asc --top 10

# Use * for a pattern instead of a substring match
azdocli pipelines variable-group list --group-name "release-*"

# Only groups you can use (or manage) in a pipeline
azdocli pipelines variable-group list --action use

# Show a variable group and its variables (secret values are never returned by Azure DevOps)
azdocli pipelines variable-group show --group-id 7

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

### Release Management Features

The `releases` command works with classic release pipelines. Its main use is creating a release that
deploys a specific build, rather than the latest build of the linked artifact:

```sh
# Find the ID of the release definition (release pipeline)
azdocli releases definitions
azdocli releases definitions --name "web"

# Find the build to release, e.g. from the runs of its build pipeline
azdocli pipelines runs list --pipeline-ids 42

# Create a release of build 1234 from release definition 5
azdocli releases create --definition-id 5 --build-id 1234

# When the definition links several build artifacts, name the one the build belongs to
azdocli releases create --definition-id 5 --build-id 1234 --artifact-alias _web-ci --description "Hotfix for #321"

# List releases, optionally for one definition
azdocli releases list
azdocli releases list --definition-id 5 --top 10

# Show a release and the status of each of its stages
azdocli releases show --id 87
```

**Release Features:**

- **Release a specific build**: The build is pinned to the definition's primary build artifact, or to the artifact named with `--artifact-alias`
- **Helpful errors**: An unknown alias, or a definition without a primary build artifact, reports the aliases you can choose from
- **Stage overview**: `releases show` lists every stage with its deployment status
- **Default project support**: Use with default project or specify --project explicitly

### Artifacts Features

The `artifacts` command browses Azure Artifacts feeds and transfers Universal Packages, mirroring
`az artifacts universal`:

```sh
# List every feed in the organization, or only the feeds of a project
azdocli artifacts feeds list
azdocli artifacts feeds list --project MyProject

# List the packages of a feed and the versions of one package
azdocli artifacts packages list --feed tools
azdocli artifacts packages list --feed tools --protocol upack --name cli
azdocli artifacts packages versions --feed tools --name cli

# Download a Universal Package, optionally only some of its files
azdocli artifacts universal download --feed tools --name cli --version 1.2.3 --path ./cli
azdocli artifacts universal download --feed tools --name cli --version 1.2.3 --path ./cli --file-filter "*.dll"

# Publish a directory as a Universal Package
azdocli artifacts universal publish --feed tools --name cli --version 1.2.4 --path ./dist --description "CLI tools"
```

**Artifacts Features:**

- **Feed scope**: Feeds are organization-scoped by default, like in `az`; pass `--scope project` or `--project` for a feed created in a project (`--scope project` alone uses the default project)
- **Native download**: Universal Packages are downloaded directly over the Azure Artifacts dedup protocol, without any extra tools. Package paths that would escape `--path` are refused
- **Publish via ArtifactTool**: Publishing uses Microsoft's ArtifactTool, just like `az artifacts universal publish`. The tool is downloaded from your organization on first use and cached in `~/.azdocli/tools/ArtifactTool/<version>`; the PAT is handed to it through an environment variable, never on the command line
- **Azure DevOps Services only**: Universal Packages are not available on Azure DevOps Server, so `universal download` and `universal publish` require the `https://dev.azure.com` host

### User Management Features

The `user` commands allow you to manage users and licenses in your Azure DevOps organization:

```sh
# Add a user with a license
azdocli user add --email user@contoso.com --license express

# List users (excluding users added via AAD groups)
azdocli user list

# Show user details by ID or email
azdocli user show --id 00000000-0000-0000-0000-000000000000
azdocli user show --email user@contoso.com

# Remove a user by ID or email
azdocli user remove --id 00000000-0000-0000-0000-000000000000
azdocli user remove --email user@contoso.com

# Update a user's license type
azdocli user update --email user@contoso.com --license stakeholder
```

**User Features:**

- **Organization-wide management**: Manage user access at the organization level
- **Flexible user targeting**: Use either user ID or email for show, remove, and update
- **License updates**: Set raw Azure DevOps account license types (`none`, `earlyAdopter`, `express`, `professional`, `advanced`, `stakeholder`)
- **AAD-group filtering**: User list excludes accounts whose entitlement is inherited from AAD group rules
- **Error handling**: Clear guidance for missing users and ambiguous email matches


### Board Management Features

#### Work Item Management

The `boards work-item` commands allow you to manage work items in an Azure DevOps project:

```sh
# List work items assigned to me (using default project)
azdocli boards work-item list

# List work items assigned to someone else, by display name or email
azdocli boards work-item list --assigned-to "jane@contoso.com"

# List all work items in the project, whoever they are assigned to
azdocli boards work-item list --all

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

# Create a task under an existing work item, such as a product backlog item
azdocli boards work-item create task --title "Write integration tests" --parent 1234

# Set iteration and area paths on create or update
azdocli boards work-item create task --title "Write tests" --iteration "MyProject\Sprint 1" --area "MyProject\Team A"
azdocli boards work-item update --id 123 --iteration "MyProject\Sprint 1" --area "MyProject\Team A"

# Update a work item (using default project)
azdocli boards work-item update --id 123 --title "New title" --state "Active" --priority 2

# Delete a work item permanently (using default project)
azdocli boards work-item delete --id 123

# Soft delete a work item by changing state to "Removed"
azdocli boards work-item delete --id 123 --soft-delete
```

**Work Item Features:**

- **Full CRUD operations**: Create, read, update, and delete work items
- **List work items**: View work items assigned to you (default), to someone else with `--assigned-to <name or email>`, or to anyone with `--all`
- **Multiple work item types**: Support for bug, task, user story, feature, and epic
- **Parent links**: Create a work item as a child of another with `--parent <ID>`, e.g. tasks under a product backlog item
- **Filtering**: Filter by state, work item type, and limit number of results
- **Web integration**: Open work items directly in browser with `--web` option
- **Soft delete**: Option to change state to "Removed" instead of permanent deletion
- **Iteration and area paths**: Set `System.IterationPath` and `System.AreaPath` on create or update with `--iteration` and `--area`
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

#### Areas

The `boards area` commands manage the area paths of a project and the areas each team owns:

```sh
# List the area tree of the default project, three levels deep
azdocli boards area project list --depth 3

# Show, create, rename and delete area paths
azdocli boards area project show --path "\MyProject\Area\Team A"
azdocli boards area project create --name "Backend" --path "MyProject\Team A"
azdocli boards area project update --path "MyProject\Team A\Backend" --name "Services"
azdocli boards area project delete --path "MyProject\Team A\Services" --reclassify-id 12

# Move the area with ID 15, and everything below it, under Team A
azdocli boards area project update --path "MyProject\Team A" --child-id 15

# Manage the areas a team owns
azdocli boards area team list --team "Team A"
azdocli boards area team add --team "Team A" --path "MyProject\Team A" --include-sub-areas --set-as-default
azdocli boards area team update --team "Team A" --path "MyProject\Team A" --include-sub-areas false
azdocli boards area team remove --team "Team A" --path "MyProject\Shared"
```

#### Iterations

The `boards iteration` commands manage the iterations of a project and the iterations each team works in:

```sh
# List the iteration tree of the default project, two levels deep
azdocli boards iteration project list --depth 2

# Create, reschedule, rename and delete iterations
azdocli boards iteration project create --name "Sprint 1" --start-date 2026-06-01 --finish-date 2026-06-14
azdocli boards iteration project update --path "MyProject\Sprint 1" --start-date 2026-06-02 --finish-date 2026-06-15
azdocli boards iteration project update --path "MyProject\Sprint 1" --name "Sprint 01"
azdocli boards iteration project show --path "\MyProject\Iteration\Sprint 01"
azdocli boards iteration project delete --path "MyProject\Sprint 01"

# Manage the iterations a team works in
azdocli boards iteration team list --team "Team A"
azdocli boards iteration team list --team "Team A" --timeframe current
azdocli boards iteration team add --team "Team A" --path "MyProject\Sprint 1"
azdocli boards iteration team remove --team "Team A" --id 5a8e3c5e-8d61-4b0f-b5c4-1b9a6a9c5e3e
azdocli boards iteration team list-work-items --team "Team A" --path "MyProject\Sprint 1"

# Default and backlog iterations of a team
azdocli boards iteration team show-default-iteration --team "Team A"
azdocli boards iteration team set-default-iteration --team "Team A" --default-iteration-macro @CurrentIteration
azdocli boards iteration team show-backlog-iteration --team "Team A"
azdocli boards iteration team set-backlog-iteration --team "Team A" --path "MyProject"
```

**Area and Iteration Features:**

- **Flexible paths**: `--path` accepts the absolute form Azure DevOps shows (`\MyProject\Area\Team A`), the form work items use (`MyProject\Team A`), or a path relative to the project's root (`Team A`)
- **Iterations by path or ID**: Team iteration commands take either the iteration's identifier (`--id`) or its path (`--path`)
- **Safe team edits**: Adding an area that a team already owns, or removing a team's default area, is refused with a clear message

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
