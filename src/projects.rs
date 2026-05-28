use crate::auth::get_credentials;
use anyhow::{anyhow, Result};
use azure_devops_rust_api::core::{models, ClientBuilder};
use clap::{Subcommand, ValueEnum};
use dialoguer::Confirm;
use serde_json::json;
use std::process::Command;
use tokio::time::{sleep, Duration};

const CREATE_OPEN_WAIT_RETRIES: usize = 30;
const CREATE_OPEN_WAIT_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Subcommand, Clone)]
pub enum ProjectsSubCommands {
    /// List all projects in the organization
    List,
    /// Create a team project
    Create {
        /// Name of the new project
        #[clap(long)]
        name: String,
        /// Description for the new project
        #[clap(short, long)]
        description: Option<String>,
        /// Process to use (name or ID)
        #[clap(short, long)]
        process: Option<String>,
        /// Source control type of the initial repository
        #[clap(short = 's', long, value_enum, default_value_t = ProjectSourceControl::Git)]
        source_control: ProjectSourceControl,
        /// Project visibility
        #[clap(long, value_enum, default_value_t = ProjectVisibility::Private)]
        visibility: ProjectVisibility,
        /// Open the team project in the default web browser
        #[clap(long)]
        open: bool,
    },
    /// Delete team project
    Delete {
        /// The GUID of the project to delete
        #[clap(long)]
        id: String,
        /// Do not prompt for confirmation
        #[clap(short = 'y', long)]
        yes: bool,
    },
    /// Show team project
    Show {
        /// Name or ID of the project
        #[clap(short = 'p', long)]
        project: String,
        /// Open the team project in the default web browser
        #[clap(long)]
        open: bool,
    },
}

#[derive(Clone, Debug, ValueEnum)]
pub enum ProjectSourceControl {
    Git,
    Tfvc,
}

impl ProjectSourceControl {
    fn as_api_value(&self) -> &'static str {
        match self {
            ProjectSourceControl::Git => "Git",
            ProjectSourceControl::Tfvc => "Tfvc",
        }
    }
}

#[derive(Clone, Debug, ValueEnum)]
pub enum ProjectVisibility {
    Private,
    Public,
}

impl ProjectVisibility {
    fn as_api_value(&self) -> models::team_project_reference::Visibility {
        match self {
            ProjectVisibility::Private => models::team_project_reference::Visibility::Private,
            ProjectVisibility::Public => models::team_project_reference::Visibility::Public,
        }
    }
}

pub async fn handle_command(subcommand: &ProjectsSubCommands) -> Result<()> {
    match subcommand {
        ProjectsSubCommands::List => {
            list_projects().await?;
        }
        ProjectsSubCommands::Create {
            name,
            description,
            process,
            source_control,
            visibility,
            open,
        } => {
            let operation = create_project(
                name,
                description.as_deref(),
                process.as_deref(),
                source_control,
                visibility,
            )
            .await?;

            display_operation_reference("Project creation queued", &operation);

            if *open {
                let project = wait_for_project_to_be_ready(name).await?;
                let organization = get_credentials()?.organization;
                open_project_in_browser(&organization, &project.team_project_reference.name)?;
            }
        }
        ProjectsSubCommands::Delete { id, yes } => {
            if !is_valid_project_id(id) {
                return Err(anyhow!(
                    "Invalid project ID '{}'. The --id flag only accepts a GUID.",
                    id
                ));
            }

            let project = get_project(id).await?;
            let project_id = project
                .team_project_reference
                .id
                .clone()
                .unwrap_or(id.clone());
            let project_name = project.team_project_reference.name.clone();

            if !*yes
                && !Confirm::new()
                    .with_prompt(format!(
                        "Are you sure you want to delete project '{project_name}' ({project_id})?"
                    ))
                    .default(false)
                    .interact()?
            {
                println!("Delete operation cancelled.");
                return Ok(());
            }

            let operation = delete_project(&project_id).await?;
            display_operation_reference("Project delete queued", &operation);
        }
        ProjectsSubCommands::Show { project, open } => {
            let team_project = get_project(project).await?;
            display_project_details(&team_project);

            if *open {
                let organization = get_credentials()?.organization;
                open_project_in_browser(&organization, &team_project.team_project_reference.name)?;
            }
        }
    }
    Ok(())
}

async fn create_project(
    name: &str,
    description: Option<&str>,
    process: Option<&str>,
    source_control: &ProjectSourceControl,
    visibility: &ProjectVisibility,
) -> Result<models::OperationReference> {
    let creds = get_credentials()?;
    let credential = azure_devops_rust_api::Credential::Pat(creds.pat);
    let client = ClientBuilder::new(credential).build();
    let process_template_id =
        resolve_process_template_id(&client, &creds.organization, process).await?;

    let mut project_reference =
        models::TeamProjectReference::new(name.to_string(), visibility.as_api_value());
    project_reference.description = description.map(str::to_owned);

    let mut team_project = models::TeamProject::new(project_reference);
    team_project.capabilities = Some(json!({
        "versioncontrol": {
            "sourceControlType": source_control.as_api_value(),
        },
        "processTemplate": {
            "templateTypeId": process_template_id,
        }
    }));

    Ok(client
        .projects_client()
        .create(&creds.organization, team_project)
        .await?)
}

async fn delete_project(project_id: &str) -> Result<models::OperationReference> {
    let creds = get_credentials()?;
    let credential = azure_devops_rust_api::Credential::Pat(creds.pat);
    let client = ClientBuilder::new(credential).build();

    Ok(client
        .projects_client()
        .delete(&creds.organization, project_id)
        .await?)
}

async fn get_project(project: &str) -> Result<models::TeamProject> {
    let creds = get_credentials()?;
    let credential = azure_devops_rust_api::Credential::Pat(creds.pat);
    let client = ClientBuilder::new(credential).build();

    Ok(client
        .projects_client()
        .get(&creds.organization, project)
        .await?)
}

async fn resolve_process_template_id(
    client: &azure_devops_rust_api::core::Client,
    organization: &str,
    process: Option<&str>,
) -> Result<String> {
    let processes = client.processes_client().list(organization).await?.value;

    if processes.is_empty() {
        return Err(anyhow!(
            "No process templates found in organization '{}'",
            organization
        ));
    }

    if let Some(process_name_or_id) = process {
        if let Some(process_id) = processes
            .iter()
            .find(|p| p.id.as_deref() == Some(process_name_or_id))
            .and_then(|p| p.id.clone())
        {
            return Ok(process_id);
        }

        if let Some(process_id) = processes
            .iter()
            .find(|p| {
                p.process_reference
                    .name
                    .as_deref()
                    .is_some_and(|name| name.eq_ignore_ascii_case(process_name_or_id))
            })
            .and_then(|p| p.id.clone())
        {
            return Ok(process_id);
        }

        let available_processes = processes
            .iter()
            .filter_map(|p| p.process_reference.name.clone())
            .collect::<Vec<_>>();

        let available = if available_processes.is_empty() {
            "<none>".to_string()
        } else {
            available_processes.join(", ")
        };

        return Err(anyhow!(
            "Process '{}' not found. Available processes: {}",
            process_name_or_id,
            available
        ));
    }

    if let Some(default_process_id) = processes
        .iter()
        .find(|p| p.is_default == Some(true))
        .and_then(|p| p.id.clone())
    {
        return Ok(default_process_id);
    }

    processes
        .iter()
        .find_map(|p| p.id.clone())
        .ok_or_else(|| anyhow!("No valid process template IDs found"))
}

async fn wait_for_project_to_be_ready(project: &str) -> Result<models::TeamProject> {
    let mut last_error = None;

    for attempt in 0..CREATE_OPEN_WAIT_RETRIES {
        match get_project(project).await {
            Ok(team_project) => {
                let state = team_project.team_project_reference.state.clone();
                if matches!(
                    state,
                    Some(models::team_project_reference::State::WellFormed)
                ) {
                    return Ok(team_project);
                }
            }
            Err(error) => {
                last_error = Some(error);
            }
        }

        if attempt + 1 < CREATE_OPEN_WAIT_RETRIES {
            sleep(CREATE_OPEN_WAIT_INTERVAL).await;
        }
    }

    if let Some(error) = last_error {
        return Err(anyhow!(
            "Project '{}' was created, but is not ready yet: {}",
            project,
            error
        ));
    }

    Err(anyhow!(
        "Project '{}' was created, but is not ready yet. Try again with 'azdocli projects show --project {}'",
        project,
        project
    ))
}

fn display_project_details(project: &models::TeamProject) {
    let project_ref = &project.team_project_reference;

    println!("📁 Team Project Details");
    println!("=======================");
    println!("Name: {}", project_ref.name);
    println!("ID: {}", project_ref.id.as_deref().unwrap_or("-"));
    println!(
        "State: {}",
        project_ref
            .state
            .as_ref()
            .map(|state| format!("{state:?}"))
            .unwrap_or_else(|| "-".to_string())
    );
    println!("Visibility: {:?}", project_ref.visibility);
    if let Some(description) = &project_ref.description {
        println!("Description: {description}");
    }
    if let Some(url) = &project_ref.url {
        println!("URL: {url}");
    }
}

fn display_operation_reference(label: &str, operation: &models::OperationReference) {
    println!("✅ {label}");
    if let Some(status) = &operation.status {
        println!("Status: {status:?}");
    }
    if let Some(id) = &operation.id {
        println!("Operation ID: {id}");
    }
}

fn open_project_in_browser(organization: &str, project_name: &str) -> Result<()> {
    let encoded_organization = percent_encode_path_segment(organization);
    let encoded_project_name = percent_encode_path_segment(project_name);
    let project_url =
        format!("https://dev.azure.com/{encoded_organization}/{encoded_project_name}");

    #[cfg(target_os = "windows")]
    {
        Command::new("explorer").arg(&project_url).spawn()?;
        println!("Opening project in browser: {project_url}");
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(&project_url).spawn()?;
        println!("Opening project in browser: {project_url}");
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(&project_url).spawn()?;
        println!("Opening project in browser: {project_url}");
        return Ok(());
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        Err(anyhow!(
            "Opening project in browser is not supported on this platform: {}",
            std::env::consts::OS
        ))
    }
}

fn is_valid_project_id(id: &str) -> bool {
    if id.len() != 36 {
        return false;
    }

    for (index, character) in id.chars().enumerate() {
        match index {
            8 | 13 | 18 | 23 if character == '-' => continue,
            8 | 13 | 18 | 23 => return false,
            _ if character.is_ascii_hexdigit() => continue,
            _ => return false,
        }
    }

    true
}

fn percent_encode_path_segment(input: &str) -> String {
    input
        .as_bytes()
        .iter()
        .flat_map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'.' | b'_' | b'~') {
                vec![(*byte) as char]
            } else {
                format!("%{byte:02X}").chars().collect::<Vec<_>>()
            }
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::{is_valid_project_id, percent_encode_path_segment};

    #[test]
    fn test_is_valid_project_id_accepts_guid() {
        assert!(is_valid_project_id("123e4567-e89b-12d3-a456-426614174000"));
    }

    #[test]
    fn test_is_valid_project_id_rejects_non_guid() {
        assert!(!is_valid_project_id("MyProject"));
    }

    #[test]
    fn test_percent_encode_path_segment_encodes_unsafe_characters() {
        assert_eq!(
            percent_encode_path_segment("My Org/Projéct"),
            "My%20Org%2FProj%C3%A9ct"
        );
    }
}
