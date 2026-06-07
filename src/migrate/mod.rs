//! Cross-tenant team project migration.
//!
//! See `src/README.md` and the design plan for the high-level architecture
//! and fidelity contract. This module orchestrates dependency-ordered phases
//! against a source and target Azure DevOps organization, with named
//! credential profiles, a JSON manifest for batch runs, a per-migration
//! state file enabling `--resume`, dry-run support, and per-phase progress
//! bars.

pub mod context;
pub mod executor;
pub mod http_client;
pub mod manifest;
pub mod orchestrator;
pub mod phase;
pub mod phases;
pub mod preflight;
pub mod progress;
pub mod report;
pub mod state;

use anyhow::{anyhow, Result};
use clap::{Args, Subcommand};
use std::path::PathBuf;

use crate::auth::get_credentials_for;
use context::{MigrationContext, MigrationOptions};
use manifest::Manifest;

#[derive(Subcommand, Clone)]
pub enum MigrateSubCommands {
    /// Migrate a single team project from a source org to a target org
    Project(ProjectArgs),
    /// Migrate multiple team projects from a JSON manifest file
    Batch(BatchArgs),
}

#[derive(Args, Clone)]
pub struct ProjectArgs {
    /// Source credential profile name (created via `azdocli login --profile NAME`)
    #[clap(long)]
    pub source_profile: String,
    /// Target credential profile name
    #[clap(long)]
    pub target_profile: String,
    /// Source team project name
    #[clap(long)]
    pub source: String,
    /// Target team project name (defaults to the source name)
    #[clap(long)]
    pub target: Option<String>,
    /// Create the target project if it does not already exist
    #[clap(long)]
    pub create_target: bool,
    /// Comma-separated phase names to include (default: all)
    #[clap(long, value_delimiter = ',')]
    pub phases: Option<Vec<String>>,
    /// Comma-separated phase names to skip
    #[clap(long, value_delimiter = ',')]
    pub skip_phases: Option<Vec<String>>,
    /// Enumerate work without writing to the target
    #[clap(long)]
    pub dry_run: bool,
    /// Stop the migration on the first error (default: continue and log)
    #[clap(long)]
    pub fail_fast: bool,
    /// Continue from the previous run's state file
    #[clap(long)]
    pub resume: bool,
    /// Override the default state-file path
    #[clap(long)]
    pub state_file: Option<PathBuf>,
    /// Directory for migration artifacts (state, ID maps, reports, archives).
    /// Default: `./azdocli-migration-<src-org>-<src-project>-<ts>/`
    #[clap(long)]
    pub output_dir: Option<PathBuf>,
    /// Max concurrent API operations per phase
    #[clap(long, default_value_t = 4)]
    pub concurrency: usize,
    /// Skip interactive confirmations
    #[clap(short = 'y', long)]
    pub yes: bool,
}

#[derive(Args, Clone)]
pub struct BatchArgs {
    /// Path to a JSON manifest describing the migration set
    #[clap(long)]
    pub config: PathBuf,
    /// Enumerate work without writing to the target
    #[clap(long)]
    pub dry_run: bool,
    /// Stop on the first error
    #[clap(long)]
    pub fail_fast: bool,
    /// Continue from each project's previous state file
    #[clap(long)]
    pub resume: bool,
    /// Skip interactive confirmations
    #[clap(short = 'y', long)]
    pub yes: bool,
}

pub async fn handle_command(cmd: &MigrateSubCommands) -> Result<()> {
    match cmd {
        MigrateSubCommands::Project(args) => run_project(args).await,
        MigrateSubCommands::Batch(args) => run_batch(args).await,
    }
}

async fn run_project(args: &ProjectArgs) -> Result<()> {
    let source_creds = get_credentials_for(Some(&args.source_profile))?;
    let target_creds = get_credentials_for(Some(&args.target_profile))?;

    let target_project = args.target.clone().unwrap_or_else(|| args.source.clone());

    let opts = MigrationOptions {
        source_project: args.source.clone(),
        target_project,
        create_target: args.create_target,
        phases: args.phases.clone(),
        skip_phases: args.skip_phases.clone(),
        dry_run: args.dry_run,
        fail_fast: args.fail_fast,
        resume: args.resume,
        state_file: args.state_file.clone(),
        output_dir: args.output_dir.clone(),
        concurrency: args.concurrency.max(1),
        yes: args.yes,
    };

    let ctx = MigrationContext::new(source_creds, target_creds, opts)?;
    orchestrator::run(ctx).await
}

async fn run_batch(args: &BatchArgs) -> Result<()> {
    let manifest = Manifest::load(&args.config)?;
    let source_creds = get_credentials_for(Some(&manifest.source_profile))?;
    let target_creds = get_credentials_for(Some(&manifest.target_profile))?;

    if manifest.projects.is_empty() {
        return Err(anyhow!("Manifest contains no projects to migrate"));
    }

    let mut overall_errors = 0usize;
    for project in &manifest.projects {
        let merged = manifest.merged_options_for(project);

        let opts = MigrationOptions {
            source_project: project.source.clone(),
            target_project: project
                .target
                .clone()
                .unwrap_or_else(|| project.source.clone()),
            create_target: merged.create_target,
            phases: merged.phases.clone(),
            skip_phases: merged.skip_phases.clone(),
            dry_run: args.dry_run,
            fail_fast: args.fail_fast || merged.fail_fast,
            resume: args.resume,
            state_file: None,
            output_dir: manifest.output_dir.clone(),
            concurrency: merged.concurrency.max(1),
            yes: args.yes,
        };

        let ctx = MigrationContext::new(source_creds.clone(), target_creds.clone(), opts)?;
        match orchestrator::run(ctx).await {
            Ok(()) => {}
            Err(e) => {
                eprintln!("❌ Project '{}' failed: {}", project.source, e);
                overall_errors += 1;
                if args.fail_fast {
                    return Err(e);
                }
            }
        }
    }

    if overall_errors > 0 {
        Err(anyhow!(
            "Batch migration finished with {} project failure(s)",
            overall_errors
        ))
    } else {
        Ok(())
    }
}
