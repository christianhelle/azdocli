# Contributing to Azure DevOps CLI (azdocli)

Thank you for your interest in contributing to azdocli! This document provides guidelines for contributing to the project.

## Code of Conduct

Please be respectful and constructive in all interactions. We welcome contributions from everyone.

## Development Setup

### Prerequisites

- Rust 1.70+ (latest stable recommended)
- Git

### Getting Started

1. Fork the repository on GitHub
2. Clone your fork locally:
   ```bash
   git clone https://github.com/your-username/azdocli.git
   cd azdocli
   ```

3. Build the project:
   ```bash
   cargo build
   ```

4. Run tests:
   ```bash
   cargo test
   ```

5. Test the CLI manually:
   ```bash
   cargo run -- --help
   cargo run -- login
   ```

## Code Style and Patterns

### Follow Existing Patterns

This project follows standard Rust conventions and established patterns. Please ensure your code is consistent with the existing codebase:

#### Project Structure
- **Modular design**: Each Azure DevOps service area has its own module (`auth.rs`, `boards.rs`, `pipelines.rs`, `repos.rs`)
- **Command handling**: Each module has a `handle_command()` function that takes subcommands and delegates to specific functions
- **Consistent imports**: Group imports by: standard library, external crates, internal modules

#### Code Conventions

**Function and Variable Naming:**
- Use `snake_case` for functions, variables, and modules
- Use `PascalCase` for structs, enums, and traits
- Use descriptive names that clearly indicate purpose

**Error Handling:**
- Use `anyhow::Result<()>` for functions that can fail
- Use `anyhow!()` macro for creating custom error messages
- Provide helpful error messages to users
- Use `?` operator for error propagation

**CLI Structure (using clap):**
- Use `#[derive(Subcommand, Clone)]` for command enums
- Include helpful documentation comments (`///`) for all commands and options
- Support optional `--project` parameter with default project fallback via `auth::get_project_or_default()`
- Use short (`-p`) and long (`--project`) options where appropriate

**Async Patterns:**
- Mark functions as `async` when they perform I/O operations
- Use `#[tokio::main]` for the main function
- Use `.await?` for async operations that can fail

**Output Formatting:**
- Use the `colored` crate for terminal output formatting
- Use emoji icons consistently for different types of output:
  - ✓ for success messages
  - ❌ for error messages  
  - 📋 for lists
  - 🚀 for actions
  - 🔑 for authentication
- Provide clear, user-friendly messages

**Example Function Pattern:**
```rust
use crate::auth;
use anyhow::{anyhow, Result};
use azure_devops_rust_api::example::{self, models, ClientBuilder};
use clap::Subcommand;
use colored::Colorize;

#[derive(Subcommand, Clone)]
pub enum ExampleSubCommands {
    /// List all items
    List {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
}

pub async fn handle_command(subcommand: &ExampleSubCommands) -> Result<()> {
    // Ensure user is authenticated
    let _credentials = auth::get_credentials()?;
    
    match subcommand {
        ExampleSubCommands::List { project } => {
            let project_name = auth::get_project_or_default(project.as_deref())?;
            println!("Listing items for project: {}", project_name);
            // Implementation here
        }
    }
    Ok(())
}
```

### Code Quality

**Formatting and Linting:**
- Run `cargo fmt` before committing to ensure consistent formatting
- Run `cargo clippy` and fix all warnings before submitting
- Our CI will reject PRs that don't pass these checks

**Documentation:**
- Add docstring comments (`///`) for public functions, especially CLI commands
- Keep inline comments minimal but use them to explain complex logic
- Update README.md if your changes affect usage or add new features

## Testing

### Running Tests

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Check code compiles
cargo check

# Format code
cargo fmt

# Run linter
cargo clippy
```

### Test Guidelines

- Write tests for new functionality when possible
- Test CLI commands manually using `cargo run -- <command>`
- Use the provided `test_commands.ps1` script for basic CLI validation
- Ensure your changes don't break existing functionality

### Manual Testing

Use the PowerShell test script to verify CLI functionality:
```powershell
./test_commands.ps1
```

This validates that command-line interfaces work correctly without requiring Azure DevOps authentication.

## Pull Request Guidelines

### PR Description Requirements

**Your PR description MUST be verbose and include:**

1. **Clear Summary**: What does this PR do?
2. **Motivation**: Why is this change needed?
3. **Changes Made**: List the specific changes (files modified, new features, bug fixes)
4. **Testing Done**: How did you test your changes?
5. **Breaking Changes**: Any breaking changes to existing functionality?
6. **Documentation Updates**: What documentation was updated?

**PR Description Template:**
```markdown
## Summary
Brief description of what this PR accomplishes.

## Motivation
Explain why this change is needed. Reference any relevant issues.

## Changes Made
- [ ] Added new feature X
- [ ] Fixed bug Y
- [ ] Updated documentation Z
- [ ] Modified file A to do B

## Testing Done
- [ ] Ran `cargo test` - all tests pass
- [ ] Ran `cargo clippy` - no warnings
- [ ] Ran `cargo fmt` - code is formatted
- [ ] Manually tested CLI commands: `cargo run -- <commands>`
- [ ] Tested on [Windows/Linux/macOS]

## Breaking Changes
List any breaking changes or "None"

## Documentation Updates
- [ ] Updated README.md
- [ ] Updated inline documentation
- [ ] Updated CONTRIBUTING.md (if process changes)

Fixes #<issue-number>
```

### Before Submitting

1. **Test thoroughly**: Ensure your changes work and don't break existing functionality
2. **Update documentation**: Keep README.md current with any new features or changes
3. **Check CI requirements**: Ensure your code passes formatting and linting
4. **Create focused commits**: Make commits atomic and with clear messages
5. **Rebase if needed**: Keep a clean commit history

### README Maintenance

**When to update README.md:**
- Adding new commands or features
- Changing existing command behavior
- Adding new installation methods
- Updating usage examples
- Adding new dependencies that affect users

**README sections to maintain:**
- **Features**: Update feature list for new capabilities
- **Usage**: Add examples for new commands
- **Building from Source**: Update if build process changes
- **Examples**: Ensure all examples work with current code

## Getting Help

- Create an issue for bugs or feature requests
- Use discussions for questions about contributing
- Reference existing code patterns when in doubt
- Ask questions in your PR if you're unsure about implementation details

## Release Process

Maintainers handle releases. Contributors should:
- Ensure changes are backward compatible when possible
- Document breaking changes clearly
- Update version numbers only if instructed

### Distribution Process

The release workflow automatically handles distribution to multiple package managers:

**Automated Distribution:**
- **GitHub Releases**: Binaries for Windows, macOS, and Linux (x64 and ARM64)
- **crates.io**: Rust package registry

**Generated Package Configurations:**
- **Snapcraft**: Linux package configuration (requires manual publication)
- **Homebrew**: macOS formula (requires submission to homebrew-core or tap)
- **Chocolatey**: Windows package (requires manual publication)
- **WinGet**: Windows package manager manifests (requires PR to microsoft/winget-pkgs)

The release workflow generates all necessary configuration files and calculates checksums automatically. Package maintainers can use these generated files for publication to their respective package managers.

**Required Secrets for Full Automation:**
- `CRATES_TOKEN`: For crates.io publication
- `SNAPCRAFT_TOKEN`: For Snapcraft publication (optional)
- `CHOCOLATEY_API_KEY`: For Chocolatey publication (optional)
- `HOMEBREW_GITHUB_TOKEN`: For Homebrew tap automation (optional)
- `WINGET_GITHUB_TOKEN`: For WinGet submission automation (optional)

Thank you for contributing to azdocli! 🚀