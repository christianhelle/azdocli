use crate::auth::get_credentials;
use crate::project::get_project_or_default;
use anyhow::{anyhow, Context, Result};
use azure_devops_rust_api::artifacts_download::{self, PackageMetadata};
use clap::{Subcommand, ValueEnum};
use glob::Pattern;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Subcommand, Clone)]
pub enum ArtifactsSubCommands {
    /// Manage Universal Packages
    Universal {
        #[clap(subcommand)]
        subcommand: ArtifactsUniversalSubCommands,
    },
}

#[derive(Subcommand, Clone)]
pub enum ArtifactsUniversalSubCommands {
    /// Download a universal package
    Download {
        /// Name or ID of the feed
        #[clap(long)]
        feed: String,
        /// Name of the package
        #[clap(short, long)]
        name: String,
        /// Directory to place the package contents
        #[clap(long)]
        path: String,
        /// Package version (e.g. 1.0.0)
        #[clap(short, long)]
        version: String,
        /// Wildcard filter for file download (applied after extraction)
        #[clap(long)]
        file_filter: Option<String>,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Feed scope
        #[clap(long, value_enum, default_value_t = FeedScope::Organization)]
        scope: FeedScope,
    },
    /// Publish a package to a feed
    Publish {
        /// Name or ID of the feed
        #[clap(long)]
        feed: String,
        /// Name of the package
        #[clap(short, long)]
        name: String,
        /// Directory containing package contents
        #[clap(long)]
        path: String,
        /// Package version (e.g. 1.0.0)
        #[clap(short, long)]
        version: String,
        /// Description of the package
        #[clap(short, long)]
        description: Option<String>,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Feed scope
        #[clap(long, value_enum, default_value_t = FeedScope::Organization)]
        scope: FeedScope,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum FeedScope {
    Organization,
    Project,
}

pub async fn handle_command(subcommand: &ArtifactsSubCommands) -> Result<()> {
    match subcommand {
        ArtifactsSubCommands::Universal { subcommand } => {
            handle_universal_command(subcommand).await
        }
    }
}

async fn handle_universal_command(subcommand: &ArtifactsUniversalSubCommands) -> Result<()> {
    match subcommand {
        ArtifactsUniversalSubCommands::Download {
            feed,
            name,
            path,
            version,
            file_filter,
            project,
            scope,
        } => {
            let output_path = PathBuf::from(path);
            let metadata = download_universal_package(
                feed,
                name,
                &output_path,
                version,
                file_filter.as_deref(),
                project.as_deref(),
                *scope,
            )
            .await?;
            println!(
                "✅ Downloaded package '{name}@{version}' from feed '{feed}' into '{}'",
                output_path.display()
            );
            println!("Package size: {} bytes", metadata.package_size);
        }
        ArtifactsUniversalSubCommands::Publish {
            feed,
            name,
            path,
            version,
            description,
            project,
            scope,
        } => {
            publish_universal_package(
                feed,
                name,
                Path::new(path),
                version,
                description.as_deref(),
                project.as_deref(),
                *scope,
            )
            .await?;
        }
    }

    Ok(())
}

fn resolve_project(project_arg: Option<&str>, scope: FeedScope) -> Result<String> {
    let project = get_project_or_default(project_arg).context(
        "Azure Artifacts currently requires a project context in azdocli. Set a default project with 'azdocli project <project_name>' or pass --project.",
    )?;

    if matches!(scope, FeedScope::Project) && project.trim().is_empty() {
        return Err(anyhow!(
            "A project-scoped feed requires a non-empty project name"
        ));
    }

    Ok(project)
}

async fn download_universal_package(
    feed: &str,
    name: &str,
    output_path: &Path,
    version: &str,
    file_filter: Option<&str>,
    project_arg: Option<&str>,
    scope: FeedScope,
) -> Result<PackageMetadata> {
    if output_path.exists() && !output_path.is_dir() {
        return Err(anyhow!(
            "Download path '{}' is not a directory",
            output_path.display()
        ));
    }

    let project = resolve_project(project_arg, scope)?;
    let creds = get_credentials()?;
    let credential = azure_devops_rust_api::Credential::Pat(creds.pat);
    let client = artifacts_download::ClientBuilder::new(credential).build();

    let metadata = client
        .download_universal_package(
            &creds.organization,
            &project,
            feed,
            name,
            version,
            output_path,
        )
        .await
        .map_err(|e| anyhow!("Failed to download universal package: {e}"))?;

    if let Some(filter) = file_filter {
        let removed = apply_file_filter(output_path, filter)?;
        println!("Applied file filter '{filter}' and removed {removed} files");
    }

    Ok(metadata)
}

async fn publish_universal_package(
    feed: &str,
    name: &str,
    source_path: &Path,
    version: &str,
    description: Option<&str>,
    project_arg: Option<&str>,
    scope: FeedScope,
) -> Result<()> {
    if !source_path.exists() {
        return Err(anyhow!(
            "Publish path '{}' does not exist",
            source_path.display()
        ));
    }
    if !source_path.is_dir() {
        return Err(anyhow!(
            "Publish path '{}' is not a directory",
            source_path.display()
        ));
    }

    let project = resolve_project(project_arg, scope)?;

    let mut message = format!(
        "Native universal publish is not yet available in azdocli. The current azure_devops_rust_api surface exposes universal metadata APIs and download protocol, but not a stable end-to-end upload contract for universal package publish.\n\nRequested publish:\n  Feed: {feed}\n  Package: {name}\n  Version: {version}\n  Scope: {scope:?}\n  Project: {project}\n  Path: {}",
        source_path.display()
    );

    if let Some(description) = description {
        message.push_str(&format!("\n  Description: {description}"));
    }

    Err(anyhow!(message))
}

fn apply_file_filter(root: &Path, filter: &str) -> Result<usize> {
    let pattern =
        Pattern::new(filter).map_err(|e| anyhow!("Invalid --file-filter pattern: {e}"))?;
    let files = collect_files(root)?;
    let mut removed = 0usize;

    for file in files {
        let relative = file.strip_prefix(root).with_context(|| {
            format!("Failed to evaluate relative path for '{}'", file.display())
        })?;

        let relative_normalized = relative.to_string_lossy().replace('\\', "/");
        let file_name = relative
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();

        let matches = pattern.matches(&relative_normalized) || pattern.matches(file_name);
        if !matches {
            fs::remove_file(&file)
                .with_context(|| format!("Failed to remove filtered file '{}'", file.display()))?;
            removed += 1;
        }
    }

    remove_empty_directories(root)?;
    Ok(removed)
}

fn collect_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if !root.exists() {
        return Ok(files);
    }

    for entry in fs::read_dir(root)
        .with_context(|| format!("Failed to read directory '{}'", root.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            files.extend(collect_files(&path)?);
        } else if path.is_file() {
            files.push(path);
        }
    }

    Ok(files)
}

fn remove_empty_directories(root: &Path) -> Result<bool> {
    if !root.is_dir() {
        return Ok(false);
    }

    let mut is_empty = true;
    for entry in fs::read_dir(root)
        .with_context(|| format!("Failed to read directory '{}'", root.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if remove_empty_directories(&path)? {
                fs::remove_dir(&path).with_context(|| {
                    format!("Failed to remove empty directory '{}'", path.display())
                })?;
            } else {
                is_empty = false;
            }
        } else {
            is_empty = false;
        }
    }

    Ok(is_empty)
}
