use crate::auth::get_credentials;
use anyhow::Result;
use azure_devops_rust_api::core::ClientBuilder;
use clap::Subcommand;

#[derive(Subcommand, Clone)]
pub enum ProjectsSubCommands {
    /// List all projects in the organization
    List,
}

pub async fn handle_command(subcommand: &ProjectsSubCommands) -> Result<()> {
    match subcommand {
        ProjectsSubCommands::List => {
            list_projects().await?;
        }
    }
    Ok(())
}

async fn list_projects() -> Result<()> {
    let creds = get_credentials()?;
    let credential = azure_devops_rust_api::Credential::Pat(creds.pat);
    let client = ClientBuilder::new(credential).build();

    let projects = client
        .projects_client()
        .list(&creds.organization)
        .await?
        .value;

    if projects.is_empty() {
        println!("No projects found in organization '{}'", creds.organization);
        return Ok(());
    }

    println!(
        "{:<30} {:<38} {:<12} {:<12} Description",
        "Name", "ID", "State", "Visibility"
    );
    println!("{}", "-".repeat(120));

    for project in &projects {
        let name = &project.name;
        let id = project.id.as_deref().unwrap_or("-");
        let state = project
            .state
            .as_ref()
            .map(|s| format!("{:?}", s))
            .unwrap_or_else(|| "-".to_string());
        let visibility = format!("{:?}", project.visibility);
        let description = project
            .description
            .as_deref()
            .unwrap_or("")
            .chars()
            .take(40)
            .collect::<String>();

        println!(
            "{:<30} {:<38} {:<12} {:<12} {}",
            name, id, state, visibility, description
        );
    }

    println!("\nTotal: {} projects", projects.len());
    Ok(())
}
