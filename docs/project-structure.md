# Project Structure

## Overview

azdocli is a modular Rust CLI for interacting with Azure DevOps. Each Azure DevOps service area (repos, pipelines, boards, projects) is implemented as its own module following a consistent pattern.

## Directory Layout

```
azdocli/
├── src/
│   ├── main.rs            # CLI entrypoint & command routing
│   ├── auth.rs            # Credential storage/retrieval, login/logout
│   ├── config.rs          # Config directory (~/.azdocli/) management
│   ├── project.rs         # Default project get/set
│   ├── projects.rs        # "projects list" command (core API)
│   ├── repos.rs           # Repository CRUD, bulk clone
│   ├── pr.rs              # Pull request operations
│   ├── pipelines.rs       # Pipeline & build management
│   └── boards.rs          # Work item CRUD, WIQL queries
├── docs/                  # Static documentation site (GitHub Pages)
├── scripts/               # Package manager generation scripts
├── .github/workflows/     # CI/CD (build, PR gate, release, audit)
├── Cargo.toml             # Dependencies & build config
├── build.rs               # Windows icon resource compilation
├── install.sh             # Linux/macOS installer
└── install.ps1            # Windows installer
```

## Module Architecture

### Command Module Pattern

Every command module follows the same structure:

```rust
// 1. Subcommand enum with clap derive
#[derive(Subcommand, Clone)]
pub enum FooSubCommands {
    /// Description shown in --help
    List {
        #[clap(short, long)]
        project: Option<String>,
    },
}

// 2. Command router
pub async fn handle_command(subcommand: &FooSubCommands) -> Result<()> {
    match subcommand {
        FooSubCommands::List { project } => {
            let project_name = get_project_or_default(project.as_deref())?;
            list_items(&project_name).await?;
        }
    }
    Ok(())
}

// 3. API call functions (private)
async fn list_items(project: &str) -> Result<Vec<Item>> {
    let creds = get_credentials()?;
    let credential = azure_devops_rust_api::Credential::Pat(creds.pat);
    let client = ClientBuilder::new(credential).build();
    // ... API call ...
}
```

### Registration in main.rs

Each module is wired up in three places in `main.rs`:

```rust
mod foo;                          // 1. Module declaration

enum Commands {
    Foo {                          // 2. Variant in Commands enum
        #[clap(subcommand)]
        subcommand: foo::FooSubCommands,
    },
}

// In main():
Some(Commands::Foo { subcommand }) => {   // 3. Match arm
    foo::handle_command(subcommand).await?;
}
```

## Authentication

Credentials are stored in `~/.azdocli/`:

| File | Contents | Permissions |
|------|----------|-------------|
| `config.json` | Organization name | World-readable |
| `credentials.json` | PAT + organization | 0600 (Unix) |

All modules retrieve credentials via `auth::get_credentials()` which returns a `Credentials { organization, pat }` struct.

## Key Dependencies

| Crate | Purpose |
|-------|---------|
| `clap` (derive) | CLI argument parsing |
| `azure_devops_rust_api` | Azure DevOps REST API client |
| `tokio` | Async runtime |
| `anyhow` | Error handling |
| `serde` / `serde_json` | JSON serialization |
| `dialoguer` | Interactive prompts |
| `colored` | Terminal colors |

The `azure_devops_rust_api` crate uses feature flags to enable specific API areas. Current features: `git`, `pipelines`, `wit`, `core`.

---

## Adding Support for a New Azure DevOps API

Follow these steps to add a new command (e.g., `azdocli artifacts list`):

### Step 1: Enable the API feature in Cargo.toml

Add the corresponding feature flag to the `azure_devops_rust_api` dependency:

```toml
azure_devops_rust_api = { version = "0.33.0", features = ["git", "pipelines", "wit", "core", "artifacts"], default-features = false }
```

Available features are listed in the [azure_devops_rust_api docs](https://docs.rs/azure_devops_rust_api). Common ones include: `build`, `core`, `git`, `pipelines`, `policy`, `release`, `test_runs`, `wit`, `wiki`, `artifacts`, `search`.

### Step 2: Create the module file

Create `src/artifacts.rs` with:

1. **A subcommand enum** deriving `Subcommand` and `Clone`:

```rust
use crate::auth::get_credentials;
use crate::project::get_project_or_default;
use anyhow::Result;
use azure_devops_rust_api::artifacts::ClientBuilder;
use clap::Subcommand;

#[derive(Subcommand, Clone)]
pub enum ArtifactsSubCommands {
    /// List all feeds
    List {
        #[clap(short, long)]
        project: Option<String>,
    },
}
```

2. **A `handle_command()` function** that routes subcommands:

```rust
pub async fn handle_command(subcommand: &ArtifactsSubCommands) -> Result<()> {
    match subcommand {
        ArtifactsSubCommands::List { project } => {
            let project_name = get_project_or_default(project.as_deref())?;
            list_feeds(&project_name).await?;
        }
    }
    Ok(())
}
```

3. **Private async functions** for each API operation:

```rust
async fn list_feeds(project: &str) -> Result<()> {
    let creds = get_credentials()?;
    let credential = azure_devops_rust_api::Credential::Pat(creds.pat);
    let client = ClientBuilder::new(credential).build();

    let feeds = client
        .feed_client()
        .list(&creds.organization, project)
        .await?
        .value;

    for feed in &feeds {
        println!("{}", feed.name);
    }
    Ok(())
}
```

### Step 3: Register the module in main.rs

Add three things:

```rust
// 1. Module declaration (alphabetical order)
mod artifacts;

// 2. Variant in Commands enum
/// Manage Azure DevOps artifacts
Artifacts {
    #[clap(subcommand)]
    subcommand: artifacts::ArtifactsSubCommands,
},

// 3. Match arm in main()
Some(Commands::Artifacts { subcommand }) => {
    artifacts::handle_command(subcommand).await?;
}
```

### Step 4: Build and verify

```bash
cargo build          # Ensure it compiles
cargo test           # Ensure existing tests pass
cargo clippy         # Lint check
azdocli --help       # Verify new subcommand appears
azdocli artifacts list  # Test against a real organization
```

### Conventions to Follow

- **Project argument**: Commands scoped to a project should accept `--project` / `-p` and fall back to `get_project_or_default()`.
- **Organization-scoped commands**: Commands that only need the organization (like `projects list`) can skip the project argument entirely and read it from `get_credentials().organization`.
- **Error messages**: Use `eprintln!("...")` for errors. Prefix with a relevant emoji if appropriate.
- **Output format**: Use `println!` with fixed-width columns for tabular data. Truncate long fields.
- **Confirmation prompts**: Destructive operations (delete, etc.) should use `dialoguer::Confirm` and support a `--yes` / `-y` flag to skip.
- **Tests**: Add `#[ignore]` integration tests that use `test_config.json` for operations requiring real API calls. Add unit tests for any logic that can be tested without credentials.
