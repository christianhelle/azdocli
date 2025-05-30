use crate::auth;
use anyhow::{anyhow, Result};
use azure_devops_rust_api::{artifacts, Credential};
use clap::Subcommand;
use colored::Colorize;
use reqwest;
use std::fs;
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
    let credentials = auth::get_credentials()?;
    println!(
        "{}Finding feed for universal packages in project '{}'...",
        "🔍 ".blue(),
        project
    );

    // Get the feed ID
    let feed_id = get_or_create_feed(project).await?;
    println!("{}Using feed: {}", "🔑 ".blue(), feed_id);

    // Build the download URL for Universal Package
    let url = format!(
        "https://pkgs.dev.azure.com/{}/_packaging/{}/upack/download/{}/{}",
        credentials.organization, feed_id, name, version
    );

    println!("{}Downloading from: {}", "🌐 ".blue(), url);

    // Create HTTP client with authentication
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .basic_auth("", Some(&credentials.pat))
        .send()
        .await?;

    if response.status().is_success() {
        // Create output directory if it doesn't exist
        fs::create_dir_all(output_dir)?;

        // Save the file
        let filename = format!("{}-{}.zip", name, version);
        let output_path = Path::new(output_dir).join(filename);

        println!("{}Saving to: {}", "💾 ".blue(), output_path.display());

        let bytes = response.bytes().await?;
        fs::write(&output_path, bytes)?;

        println!(
            "{}Successfully downloaded {} bytes",
            "✅ ".green(),
            fs::metadata(&output_path)?.len()
        );
        Ok(())
    } else {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        Err(anyhow!(
            "Failed to download artifact: HTTP {} - {}",
            status,
            error_text
        ))
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
    let _credentials = auth::get_credentials()?;
    println!(
        "{}Finding feed for universal packages in project '{}'...",
        "🔍 ".blue(),
        project
    );

    let feed_id = get_or_create_feed(project).await?;
    println!("{}Using feed: {}", "🔑 ".blue(), feed_id);

    // For now, implement a simplified version that explains the process
    println!(
        "{}Universal package publishing requires creating a .upack file and uploading via REST API",
        "ℹ️ ".yellow()
    );
    println!(
        "{}Would upload '{}' as package '{}' version '{}' to feed '{}'",
        "📦 ".blue(),
        file_path,
        name,
        version,
        feed_id
    );

    // Implement basic validation
    let path = Path::new(file_path);
    if path.is_file() {
        let size = fs::metadata(path)?.len();
        println!("{}File size: {} bytes", "📊 ".blue(), size);
    } else if path.is_dir() {
        let entries = fs::read_dir(path)?;
        let count = entries.count();
        println!("{}Directory contains {} entries", "📊 ".blue(), count);
    }

    // For now, return an informative error rather than implementing the full upload
    Err(anyhow!(
        "Universal package publishing is not yet fully implemented. Would publish to feed: {}",
        feed_id
    ))
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
