use crate::auth;
use anyhow::{anyhow, Result};
use azure_devops_rust_api::{artifacts, Credential};
use clap::Subcommand;
use colored::Colorize;
use std::path::Path;

#[derive(Subcommand, Clone)]
pub enum ArtifactsSubCommands {
    /// Download universal artifacts from Azure DevOps
    Download {
        /// Name of the artifact to download
        #[clap(short, long)]
        name: String,
        /// Version of the artifact to download
        #[clap(short, long)]
        version: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Output directory to download artifacts to (optional, defaults to current directory)
        #[clap(short, long)]
        output: Option<String>,
    },
    /// Publish universal artifacts to Azure DevOps
    Publish {
        /// Name of the artifact to publish
        #[clap(short, long)]
        name: String,
        /// Version of the artifact to publish
        #[clap(short, long)]
        version: String,
        /// Path to the file or directory to publish
        #[clap(short, long)]
        file: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Description of the artifact (optional)
        #[clap(short, long)]
        description: Option<String>,
    },
}

/// Creates an artifacts client for Azure DevOps API
fn create_artifacts_client() -> Result<artifacts::Client> {
    match auth::get_credentials() {
        Ok(creds) => {
            let credential = Credential::Pat(creds.pat);
            let client = artifacts::ClientBuilder::new(credential).build();
            Ok(client)
        }
        Err(e) => Err(e),
    }
}

/// Gets the first available feed or creates a default one for universal packages
async fn get_or_create_feed(project: &str) -> Result<String> {
    let credentials = auth::get_credentials()?;
    let client = create_artifacts_client()?;

    // Try to get existing feeds
    match client
        .feed_management_client()
        .get_feeds(&credentials.organization, project)
        .await
    {
        Ok(response) => {
            let feeds = response.value;
            if let Some(feed) = feeds.first() {
                if let Some(feed_id) = &feed.feed_core.id {
                    return Ok(feed_id.clone());
                }
            }
            Err(anyhow!(
                "No feeds found in project '{}'. Please create a feed first.",
                project
            ))
        }
        Err(e) => Err(anyhow!("Failed to retrieve feeds: {}", e)),
    }
}

/// Downloads a universal artifact from Azure DevOps
async fn download_artifact(
    project: &str,
    name: &str,
    version: &str,
    output_dir: &str,
) -> Result<()> {
    // For now, implement a placeholder that explains what's needed
    println!(
        "{}Note: Universal package download requires a feed to be configured.",
        "ℹ️ ".yellow()
    );
    println!(
        "{}This feature will download the artifact '{}' version '{}' from project '{}'",
        "📋 ".blue(),
        name,
        version,
        project
    );
    println!("{}Output directory: {}", "📂 ".blue(), output_dir);

    // Try to get the feed
    match get_or_create_feed(project).await {
        Ok(feed_id) => {
            println!("{}Using feed: {}", "🔑 ".blue(), feed_id);
            // TODO: Implement actual download using the Universal Package REST API
            Err(anyhow!(
                "Universal package download not yet implemented. Found feed: {}",
                feed_id
            ))
        }
        Err(e) => Err(e),
    }
}

/// Publishes a universal artifact to Azure DevOps
async fn publish_artifact(
    project: &str,
    name: &str,
    version: &str,
    file_path: &str,
    description: Option<&str>,
) -> Result<()> {
    // Validate that the file/directory exists
    if !Path::new(file_path).exists() {
        return Err(anyhow!("File or directory '{}' does not exist", file_path));
    }

    println!(
        "{}Note: Universal package publish requires a feed to be configured.",
        "ℹ️ ".yellow()
    );
    println!(
        "{}This feature will publish the artifact '{}' version '{}' to project '{}'",
        "📋 ".blue(),
        name,
        version,
        project
    );
    println!("{}Source: {}", "📂 ".blue(), file_path);
    if let Some(desc) = description {
        println!("{}Description: {}", "📝 ".blue(), desc);
    }

    // Try to get the feed
    match get_or_create_feed(project).await {
        Ok(feed_id) => {
            println!("{}Using feed: {}", "🔑 ".blue(), feed_id);
            // TODO: Implement actual publish using the Universal Package REST API
            Err(anyhow!(
                "Universal package publish not yet implemented. Found feed: {}",
                feed_id
            ))
        }
        Err(e) => Err(e),
    }
}

pub async fn handle_command(subcommand: &ArtifactsSubCommands) -> Result<()> {
    // Ensure user is authenticated
    let credentials = auth::get_credentials()?;
    match subcommand {
        ArtifactsSubCommands::Download {
            name,
            version,
            project,
            output,
        } => {
            let project_name = auth::get_project_or_default(project.as_deref())?;
            let output_dir = output.as_deref().unwrap_or(".");
            println!(
                "{}Downloading artifact '{}' version '{}' from project '{}' in organization '{}' to '{}'",
                "🔄 ".blue(),
                name, version, project_name, credentials.organization, output_dir
            );

            match download_artifact(&project_name, name, version, output_dir).await {
                Ok(_) => {
                    println!("{} Artifact downloaded successfully!", "✅".green());
                }
                Err(e) => {
                    eprintln!("{} Failed to download artifact: {}", "❌".red(), e);
                    return Err(e);
                }
            }
        }
        ArtifactsSubCommands::Publish {
            name,
            version,
            file,
            project,
            description,
        } => {
            let project_name = auth::get_project_or_default(project.as_deref())?;
            println!(
                "{}Publishing artifact '{}' version '{}' from '{}' to project '{}' in organization '{}'",
                "🚀 ".blue(),
                name, version, file, project_name, credentials.organization
            );
            if let Some(desc) = description {
                println!("Description: {}", desc);
            }

            match publish_artifact(&project_name, name, version, file, description.as_deref()).await
            {
                Ok(_) => {
                    println!("{} Artifact published successfully!", "✅".green());
                }
                Err(e) => {
                    eprintln!("{} Failed to publish artifact: {}", "❌".red(), e);
                    return Err(e);
                }
            }
        }
    }
    Ok(())
}
