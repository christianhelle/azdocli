use crate::auth;
use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use clap::Subcommand;
use colored::Colorize;
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
                "🔍 Downloading artifact '{}' version '{}' from project '{}' in organization '{}'",
                name.bright_cyan(),
                version.bright_green(),
                project_name.bright_yellow(),
                credentials.organization.bright_blue()
            );

            download_universal_artifact(
                &credentials.organization,
                &project_name,
                name,
                version,
                output_dir,
                &credentials.pat,
            )
            .await?;

            println!("✓ {}", "Artifact downloaded successfully".bright_green());
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
                "📦 Publishing artifact '{}' version '{}' from '{}' to project '{}' in organization '{}'",
                name.bright_cyan(),
                version.bright_green(),
                file.bright_white(),
                project_name.bright_yellow(),
                credentials.organization.bright_blue()
            );

            if let Some(desc) = description {
                println!("📝 Description: {}", desc.bright_white());
            }

            publish_universal_artifact(
                &credentials.organization,
                &project_name,
                name,
                version,
                file,
                description.as_deref(),
                &credentials.pat,
            )
            .await?;

            println!("✓ {}", "Artifact published successfully".bright_green());
        }
    }
    Ok(())
}

/// Downloads a universal artifact from Azure DevOps
async fn download_universal_artifact(
    organization: &str,
    project: &str,
    name: &str,
    version: &str,
    output_dir: &str,
    pat: &str,
) -> Result<()> {
    let client = reqwest::Client::new();

    // Universal Package (UPack) download URL pattern
    // Try project-scoped Universal feed first
    let url = format!(
        "https://pkgs.dev.azure.com/{}/{}/_packaging/Universal/upack/download/{}/{}",
        organization, project, name, version
    );

    // Create basic auth header
    let auth = STANDARD.encode(format!(":{}", pat));

    println!("🔍 Attempting to download from: {}", url.bright_cyan());

    let response = client
        .get(&url)
        .header("Authorization", format!("Basic {}", auth))
        .header("Accept", "application/octet-stream")
        .send()
        .await?;

    if !response.status().is_success() {
        // Try organization-scoped feed as fallback
        let org_url = format!(
            "https://pkgs.dev.azure.com/{}/_packaging/Universal/upack/download/{}/{}",
            organization, name, version
        );

        println!(
            "🔄 Trying organization-scoped feed: {}",
            org_url.bright_cyan()
        );

        let org_response = client
            .get(&org_url)
            .header("Authorization", format!("Basic {}", auth))
            .header("Accept", "application/octet-stream")
            .send()
            .await?;

        if !org_response.status().is_success() {
            return Err(anyhow!(
                "Failed to download artifact from both project and organization feeds.\nProject feed ({}): HTTP {}\nOrganization feed ({}): HTTP {}\n\nTip: Ensure the artifact exists and you have access to the feed.",
                url,
                response.status(),
                org_url,
                org_response.status()
            ));
        }

        return download_and_save_artifact(org_response, name, version, output_dir).await;
    }

    download_and_save_artifact(response, name, version, output_dir).await
}

/// Helper function to download and save artifact content
async fn download_and_save_artifact(
    response: reqwest::Response,
    name: &str,
    version: &str,
    output_dir: &str,
) -> Result<()> {
    // Ensure output directory exists
    fs::create_dir_all(output_dir)?;

    // Save the artifact file
    let filename = format!("{}-{}.zip", name, version);
    let file_path = Path::new(output_dir).join(filename);

    let content = response.bytes().await?;
    fs::write(&file_path, content)?;

    println!(
        "📁 Saved artifact to: {}",
        file_path.display().to_string().bright_green()
    );

    Ok(())
}

/// Publishes a universal artifact to Azure DevOps
async fn publish_universal_artifact(
    organization: &str,
    project: &str,
    name: &str,
    version: &str,
    file_path: &str,
    description: Option<&str>,
    pat: &str,
) -> Result<()> {
    // Check if file exists
    if !Path::new(file_path).exists() {
        return Err(anyhow!("File not found: {}", file_path));
    }

    let client = reqwest::Client::new();

    // Universal Package (UPack) publish URL pattern
    // Try project-scoped Universal feed first
    let url = format!(
        "https://pkgs.dev.azure.com/{}/{}/_packaging/Universal/upack/upload",
        organization, project
    );

    // Create basic auth header
    let auth = STANDARD.encode(format!(":{}", pat));

    // Read file content
    let file_content = fs::read(file_path)?;

    println!(
        "📤 Uploading {} bytes to {}",
        file_content.len().to_string().bright_white(),
        url.bright_cyan()
    );

    let mut request = client
        .put(&url)
        .header("Authorization", format!("Basic {}", auth))
        .header("Content-Type", "application/octet-stream")
        .header("X-NuGet-Package-Name", name)
        .header("X-NuGet-Package-Version", version);

    if let Some(desc) = description {
        request = request.header("X-Package-Description", desc);
    }

    let response = request.body(file_content).send().await?;

    if !response.status().is_success() {
        // Try organization-scoped feed as fallback
        let org_url = format!(
            "https://pkgs.dev.azure.com/{}/_packaging/Universal/upack/upload",
            organization
        );

        println!(
            "🔄 Trying organization-scoped feed: {}",
            org_url.bright_cyan()
        );

        let file_content = fs::read(file_path)?;
        let mut org_request = client
            .put(&org_url)
            .header("Authorization", format!("Basic {}", auth))
            .header("Content-Type", "application/octet-stream")
            .header("X-NuGet-Package-Name", name)
            .header("X-NuGet-Package-Version", version);

        if let Some(desc) = description {
            org_request = org_request.header("X-Package-Description", desc);
        }

        let org_response = org_request.body(file_content).send().await?;

        if !org_response.status().is_success() {
            return Err(anyhow!(
                "Failed to publish artifact to both project and organization feeds.\nProject feed ({}): HTTP {}\nOrganization feed ({}): HTTP {}\n\nTip: Ensure you have publish permissions to the feed.",
                url,
                response.status(),
                org_url,
                org_response.status()
            ));
        }
    }

    Ok(())
}
