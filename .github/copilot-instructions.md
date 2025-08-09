# Azure DevOps CLI (azdocli)

**ALWAYS FOLLOW THESE INSTRUCTIONS FIRST** - Only search for additional context if the information here is incomplete or found to be in error.

Azure DevOps CLI (azdocli) is a Rust-based command-line tool for interacting with Azure DevOps services including repository management, pipeline operations, and board/work item management. It uses Personal Access Token (PAT) authentication and supports default project configuration to streamline workflows.

## Core Development Requirements

**Prerequisites (ALWAYS install these first):**
- Rust 1.70+ (Currently validated with Rust 1.88.0)
- Git (required for repository clone operations)
- Cargo package manager (comes with Rust)

## Build and Test Commands (VALIDATED TIMINGS)

**CRITICAL TIMING - NEVER CANCEL these operations:**

### Initial Build
```bash
cargo build --verbose
# Takes ~3 minutes. NEVER CANCEL. Use timeout 300+ seconds.
```

### Release Build  
```bash
cargo build --release --verbose
# Takes ~1.5 minutes. NEVER CANCEL. Use timeout 200+ seconds.
```

### Run Tests
```bash
cargo test --verbose
# Takes ~3 seconds (only unit tests run by default)
# Integration tests require Azure DevOps credentials and are marked with #[ignore]
```

### Linting and Formatting
```bash
cargo clippy --verbose
# Takes ~25 seconds. Use timeout 60+ seconds.

cargo fmt --check
# Takes <1 second. Quick validation.
```

### Security Audit
```bash
cargo audit
# Requires: cargo install cargo-audit
# Used in CI for security vulnerability checking
```

## Development Workflow

**Bootstrap the repository (run these commands first):**
```bash
# 1. Build the project
cargo build --verbose

# 2. Run tests  
cargo test --verbose

# 3. Check linting (ALWAYS run before committing)
cargo clippy --verbose

# 4. Check formatting (ALWAYS run before committing)  
cargo fmt --check

# 5. Test CLI functionality
./target/debug/azdocli --help
./target/debug/azdocli repos --help
./target/debug/azdocli pipelines --help
./target/debug/azdocli boards --help
```

**ALWAYS run before committing:**
```bash
cargo fmt && cargo clippy --verbose
# The CI build (.github/workflows/build.yml) will fail if these don't pass
```

## User Scenario Validation

**CRITICAL: Always test these user scenarios after making changes:**

### 1. CLI Help and Navigation
```bash
# Test basic CLI functionality
./target/release/azdocli --help
./target/release/azdocli repos --help  
./target/release/azdocli pipelines --help
./target/release/azdocli boards --help
./target/release/azdocli project --help
```

### 2. Authentication Flow
```bash
# Test login workflow (will prompt for organization and PAT)
./target/release/azdocli login
# User enters organization name (e.g., "mycompany") 
# User enters Personal Access Token
```

### 3. Project Management
```bash
# Set default project
./target/release/azdocli project MyProject

# View current default project  
./target/release/azdocli project
```

### 4. Repository Operations (requires authentication)
```bash
# List repositories (requires default project or --project flag)
./target/release/azdocli repos list

# Show repository details
./target/release/azdocli repos show --id MyRepo

# Clone repositories with various options
./target/release/azdocli repos clone --help
./target/release/azdocli repos clone --target-dir ./repos --parallel --yes
```

### 5. Error Handling Validation
```bash
# Test error when not authenticated
./target/release/azdocli repos list
# Should show: "Error: No default project configured..."

# Test error when no default project  
./target/release/azdocli repos list --project NonExistentProject
# Should show appropriate authentication error
```

## Integration Tests

**Integration test setup (requires Azure DevOps credentials):**
```bash
# 1. Copy template configuration
cp test_config.json.template test_config.json

# 2. Edit test_config.json with your Azure DevOps details:
# {
#   "organization": "your-organization-name", 
#   "pat": "your-personal-access-token",
#   "project": "your-test-project-name"
# }

# 3. Run integration tests (marked with #[ignore])
cargo test -- --ignored
# These tests create/clone/delete repositories in your Azure DevOps project
# Use a TEST PROJECT only - never run against production
```

## Key Project Structure

```
azdocli/
├── src/
│   ├── main.rs          # Entry point and command parsing
│   ├── auth.rs          # Authentication and credential management  
│   ├── repos.rs         # Repository operations (list, clone, show, delete)
│   ├── pipelines.rs     # Pipeline operations (list, runs, show, run)
│   ├── boards.rs        # Board/work item operations
│   ├── project.rs       # Default project management
│   └── config.rs        # Configuration handling
├── Cargo.toml           # Dependencies and project metadata
├── .github/workflows/   # CI/CD pipelines
│   ├── build.yml        # Cross-platform builds and tests
│   ├── release.yml      # Release automation
│   └── security-audit.yml # Security vulnerability checks
└── test_config.json.template # Template for integration tests
```

## CI/CD Integration

**GitHub Actions workflows:**
- **build.yml**: Builds on Windows, Linux, macOS (x64 and ARM64), runs tests, validates publish
- **release.yml**: Automated releases to GitHub, crates.io, generates package configs  
- **security-audit.yml**: Weekly security vulnerability scanning
- **pr-verification.yml**: PR validation checks

**The build workflow will fail if:**
- Code doesn't compile
- Tests fail  
- `cargo clippy` reports warnings
- `cargo fmt --check` shows formatting issues
- `cargo publish --dry-run` fails

## Dependencies and Features

**Key dependencies (from Cargo.toml):**
- `clap` (CLI parsing with derive features)
- `tokio` (async runtime with full features)  
- `azure_devops_rust_api` (Azure DevOps API client with git, pipelines, wit features)
- `anyhow` (error handling)
- `serde/serde_json` (serialization)
- `chrono` (date/time handling)
- `dialoguer` (interactive prompts)
- `colored` (terminal output formatting)
- `dirs` (user directory discovery)

**Feature flags used:**
- `azure_devops_rust_api` with features: `["git", "pipelines", "wit"]`
- `clap` with features: `["derive"]` 
- `tokio` with features: `["full"]`
- `chrono` with features: `["serde"]`

## Authentication Requirements

**Creating a Personal Access Token (PAT):**
1. Navigate to Azure DevOps → User Settings → Personal Access Tokens
2. Create new token with scopes:
   - **Code**: Read & write (for repository operations)
   - **Build**: Read & execute (for pipeline operations)  
   - **Work Items**: Read & write (for board operations)
   - **Project and Team**: Read (for project operations)
3. Store securely - never commit to version control

**User workflow:**
```bash
azdocli login                    # Prompts for organization and PAT
azdocli project MyProject        # Set default project
azdocli repos list               # Now works with default project
```

## Common Development Tasks

**Adding new commands:**
- Follow existing patterns in repos.rs, pipelines.rs, boards.rs
- Use `#[derive(Subcommand, Clone)]` for command enums
- Include `--project` parameter support with `auth::get_project_or_default()`
- Add comprehensive help documentation
- Use `colored` crate for user-friendly output with emoji icons (✓ ❌ 📋 🚀 🔑)

**Debugging tips:**
- Use `cargo run -- <command>` for development testing
- Enable verbose logging with the Azure DevOps API client
- Test both authenticated and unauthenticated flows
- Validate error messages are user-friendly

**Code style requirements:**  
- Use `anyhow::Result<()>` for error handling
- Async functions for I/O operations  
- Consistent emoji usage in output
- Follow existing module patterns and imports

**Distribution and installation:**
- Users install via: `cargo install azdocli` or install scripts
- Cross-platform releases automated via GitHub Actions
- Package configurations generated for Snap, Homebrew, Chocolatey, WinGet

## NEVER DO

- Skip `cargo fmt` and `cargo clippy` before committing
- Cancel long-running builds (they may take several minutes)
- Run integration tests against production Azure DevOps projects
- Commit test_config.json with real credentials
- Make breaking changes without updating help documentation
- Skip manual validation scenarios after code changes

## Validation Checklist

Before completing any changes, ALWAYS verify:
- [ ] `cargo build --verbose` succeeds (3+ min timeout)
- [ ] `cargo test --verbose` passes  
- [ ] `cargo clippy --verbose` reports no warnings (25+ sec timeout)
- [ ] `cargo fmt --check` passes
- [ ] `./target/release/azdocli --help` works
- [ ] CLI subcommands show proper help
- [ ] Error messages are user-friendly when not authenticated
- [ ] Authentication workflow prompts correctly
- [ ] Project management commands work as expected

This tool provides comprehensive Azure DevOps management capabilities through a well-structured Rust CLI with robust error handling, cross-platform support, and extensive CI/CD automation.