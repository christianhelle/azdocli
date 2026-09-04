# Azure DevOps CLI

CLI tool for interacting with Azure DevOps.

## Features

- **Repository Management**: List, create, delete, clone, view, browse and manage pull requests in repositories
- **Pipeline Management**: Manage Azure DevOps pipelines
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
- **All modules supported**: Works with repos, pipelines, and boards
- **Helpful error messages**: Clear feedback when no default is set and no --project is provided

### Migrate

The `migrate` command clones one or more Azure DevOps team projects from a source organization to a target organization. It supports a single-project run or a batch run driven by a JSON manifest, and it records migration artifacts such as state, ID maps, exports, and reports under the selected output directory.

#### Prerequisites

Create two named credential profiles, one for each side of the migration. You can use either cloud organizations or enterprise/on-premises Azure DevOps base URLs when logging in. For enterprise servers, provide the base URL without the organization or collection segment (for example, `https://devops.mycompany.com` or `https://tfs.mycompany.com/tfs`), or paste the full URL including the collection (for example, `https://tfs.mycompany.com/tfs/DefaultCollection`) so the last path segment is used as the organization/collection:

```sh
azdocli login --profile source
azdocli login --profile target
```

Use PATs with the scopes needed for the assets you migrate: **Code** read & write, **Build** read & execute, **Work Items** read & write, and **Project and Team** read. The target account also needs permission to create target projects when using `--create-target`; work item migration phases require the Azure DevOps **Bypass rules on work item updates** permission when those phases are implemented.

#### Single-project usage

```sh
azdocli migrate project --source-profile source --target-profile target --source SourceProject
```

```sh
azdocli migrate project \
  --source-profile source \
  --target-profile target \
  --source SourceProject \
  --target TargetProject \
  --create-target \
  --dry-run \
  --resume
```

Useful flags include `--phases <PHASES>`, `--skip-phases <SKIP_PHASES>`, `--fail-fast`, `--state-file <STATE_FILE>`, `--output-dir <OUTPUT_DIR>`, `--concurrency <CONCURRENCY>`, and `--yes`.

#### Batch usage

```json
{
  "source_profile": "source",
  "target_profile": "target",
  "output_dir": "./migrations/",
  "default_options": {
    "create_target": true,
    "concurrency": 4,
    "fail_fast": false,
    "skip_phases": ["dashboards"]
  },
  "projects": [
    { "source": "ProjA", "target": "ProjA" },
    {
      "source": "ProjB",
      "target": "ProjB-Migrated",
      "options": { "skip_phases": ["test_plans"] }
    }
  ]
}
```

```sh
azdocli migrate batch --config manifest.json --resume --yes
```

Batch runs also support `--dry-run`, `--fail-fast`, `--resume`, and `--yes`.

#### Fidelity contract

| Asset | Fidelity | Notes |
|---|---|---|
| Project | Full | Creates the target project when `--create-target` is set; copies name, visibility, and description. Target tenant must already have a compatible process. |
| Process template | Export-only | Currently writes a placeholder `process-export.json`; process clone/import is not automated. |
| Area paths | Full | Recreates the area path tree and records path mappings. |
| Iteration paths | Full | Recreates the iteration path tree with attributes, including dates when returned by Azure DevOps. |
| Teams | Partial | Creates teams and maps IDs; members are not migrated because identities do not map cross-tenant. |
| Team board config | Out-of-scope | `teams_configure` is currently a stub that logs "not yet implemented; skipping". |
| Repos (git) | Full | Uses `git clone --mirror` and `git push --mirror`; target repositories must be empty and Git LFS is not handled. |
| Wiki | Full | Mirrors the project wiki backing repository; target wiki backing repo must be empty. |
| Work items | Out-of-scope | `work_items` is currently a stub; planned fidelity is single revision plus history snapshot and annotations. |
| Work item links | Out-of-scope | `wi_links` is currently a stub; planned cross-project links are dropped. |
| Work item attachments | Out-of-scope | `wi_attachments` is currently a stub. |
| Work item comments | Out-of-scope | `wi_comments` is currently a stub; planned comments are re-posted with original author/date text. |
| Pull requests (active) | Out-of-scope | `prs` is currently a stub; planned active PR recreation is lossy. |
| Pull requests (closed/abandoned/completed) | Out-of-scope | `prs` is currently a stub; planned behavior is JSON archive only. |
| Variable groups (non-secret) | Partial | Exports each group to JSON and recreates variable groups; secret values are blanked with warnings. |
| Service connections | Export-only | Exports service connection JSON only; manual reconfiguration is required on the target. |
| YAML pipelines | Out-of-scope | `pipelines_yaml` is currently a stub. |
| Classic pipelines | Out-of-scope | `pipelines_classic` is currently a stub. |
| Test plans | Out-of-scope | `test_plans` is currently a stub. |
| Dashboards | Out-of-scope | `dashboards` is currently a stub. |

#### Resumability

Each run writes a `state.json` file in the migration output directory, or to `--state-file` when specified. Re-run with `--resume` to skip phases already marked done and continue from the saved state.

#### Out of scope

The migration does not migrate permissions/security groups, repo permissions, branch policies, approvals/checks/environments, service hooks, artifacts/feeds, shared queries, agent pools/queues, identity or group membership mapping, secrets, or Git LFS objects.

#### Known limitations

- `git push --mirror` is destructive, so the implementation refuses to push when the target repository or wiki backing repository is not empty.
- Work item migration requires the Azure DevOps **Bypass rules on work item updates** permission when those phases are implemented.
- Secrets are not migrated. Variable group secrets are blanked, and service connections are exported for documentation/manual recreation only.

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

```sh
CLI tool for interacting with Azure DevOps

USAGE:
    azdocli [SUBCOMMAND]

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
    user         Manage users
```

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
azdocli login
# You'll be prompted for:
# - Organization name (e.g., "mycompany" from https://dev.azure.com/mycompany)
# - Personal Access Token (the PAT you created above)

# Set a default project (optional but recommended)
azdocli project MyProject
```

### Basic Examples

```sh
# Repository management
azdocli repos list                           # List all repositories
azdocli repos show --id MyRepo               # Show repository details
azdocli repos clone                          # Clone all repositories
azdocli repos branches --id MyRepo            # List branches
azdocli repos commits --id MyRepo             # List recent commits
azdocli repos files --id MyRepo               # List files and folders
azdocli repos file --id MyRepo --path /README.md # Print a file

# Pull request management
azdocli repos pr list --repo MyRepo          # List active pull requests for a repository
azdocli repos pr show --repo MyRepo --id 123 # Show pull request details
azdocli repos pr create --repo MyRepo --source "feature/my-feature" --title "My Feature" # Create a new pull request
azdocli repos pr complete --repo MyRepo --id 123 --merge-strategy squash # Merge a pull request
azdocli repos pr abandon --repo MyRepo --id 123 # Close a pull request without merging
azdocli repos pr reviewers add --repo MyRepo --id 123 --reviewer alice@example.com # Assign a reviewer
azdocli repos pr reviewers vote --repo MyRepo --id 123 --vote approve # Approve a pull request
azdocli repos pr threads --repo MyRepo --id 123 # Read the discussion
azdocli repos pr comment add --repo MyRepo --id 123 --message "Looks good" # Start a comment thread

# Pipeline management
azdocli pipelines list                       # List all pipelines
azdocli pipelines runs --id 42               # Show pipeline runs
azdocli pipelines show --id 42 --build-id 123 # Show build details
azdocli pipelines run --id 42 --branch develop # Queue a run of a branch
azdocli pipelines logs --id 42 --build-id 123 # List the logs of a run
azdocli pipelines artifacts --build-id 123   # List the artifacts of a run

# User management
azdocli user list                            # List organization users
azdocli user show --email user@contoso.com   # Show user details
azdocli user update --email user@contoso.com --license advanced # Update a user's license
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

1. Make sure you have:
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
