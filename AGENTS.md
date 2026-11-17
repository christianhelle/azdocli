# Repository guidance

This repository contains azdocli, a Rust command-line client for Azure DevOps. Follow existing patterns and keep changes focused.

## Development and validation

Use the stable Rust toolchain (Rust 1.100 or later). The pull-request CI checks are:

```bash
cargo fmt --all -- --check
cargo clippy -- -D warnings
cargo build --verbose
cargo test --verbose
cargo publish --dry-run
```

Run the relevant checks for each change. Format with `cargo fmt` and resolve Clippy warnings before submitting changes. Use `cargo run -- <command>` to exercise CLI commands during development.

## Rust and CLI conventions

- Follow the module and command patterns already present in the codebase.
- Use `snake_case` for functions, variables, and modules; use `PascalCase` for types.
- Use `anyhow::Result` for fallible operations and propagate errors with `?`.
- Make I/O operations asynchronous and await fallible async calls.
- For command enums, follow the existing `clap` derive patterns, including `Subcommand` and `Clone` where appropriate. Add useful documentation to commands and options.
- Support an optional `--project` argument with the existing default-project behavior where applicable.
- Keep terminal output clear and consistent with existing use of `colored` and the project's status icons.
- Update user-facing documentation when command behavior or usage changes.

## CLI and integration testing

Use `cargo run -- --help` and relevant subcommand help to check CLI navigation. Authentication-dependent commands require Azure DevOps credentials.

Ignored integration tests can create, clone, or delete Azure DevOps repositories. Run them only against an isolated test project, never production. Do not run these tests or configure credentials unless the task calls for it. Keep local credential files such as `test_config.json` out of version control and never commit personal access tokens.

## Commit Discipline

Nano-commit discipline: commit often, one logical change per commit, in the smallest independently-testable groups. This adapts the nano-commits workflow so it applies without the skill installed.

- **Slice every goal** into the smallest independently-testable logical groups:
  - One commit per semantic step: add a method, then commit; add its call site, then commit; add tests, then commit.
  - Dependencies stay together: adding a parameter plus updating all call sites is one commit.
  - Genuinely independent additions are always separate commits.
- **Exceptions — skip nano-slicing and commit as one block when:** the work is generated/boilerplate content (scaffolding, migrations, project setup), it is a merge commit or conflict resolution, it is machine-applied auto-formatting across a whole file, or the user explicitly says "one commit".
- **Workflow per commit:** inspect `git status`/`git diff`, stage exactly one logical slice with `git add <path>`, then commit with `git commit -m "<one-liner>"`. Never push (the user controls push) and never amend — always fresh commits.
- **Message format:** one line, plain English imperative (e.g., "extract battery level calculation into helper method"), under 72 characters when possible, no trailing period, no Conventional Commits prefixes (`feat:`, `fix:`, etc.), and no co-author trailers or AI attribution.
- **Build, run tests, and verify before every commit.** Verify using the following checks:
  - cargo fmt --all -- --check
  - cargo clippy -- -D warnings
  - cargo build --verbose
  - cargo test --verbose
- **Never commit broken code.** If tests fail, fix them before committing. If the fix requires additional changes, commit those together as a fix commit.

### Message format

- One line only, no body
- Plain English imperative: `"extract battery level calculation into helper method"`
- No Conventional Commits prefixes (no `feat:`, `fix:`, etc.)
- Under 72 chars when possible
- No trailing period
- No "Co-authored-by:" or any trailer — using `git commit -m` prevents editor-based injection
- No AI attribution

## Pull Request Discipline

Never include any AI tool or model in pull request description or comments. No "Addressed by" lines referencing AI assistants
