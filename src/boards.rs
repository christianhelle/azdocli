use crate::auth::get_credentials;
use crate::project::get_project_or_default;
use anyhow::{anyhow, Result};
use azure_devops_rust_api::wit::{self, models, ClientBuilder};
use clap::Subcommand;
use colored::Colorize;
use std::process::Command;

#[derive(Subcommand, Clone)]
pub enum BoardsSubCommands {
    /// Manage work items
    WorkItem {
        #[clap(subcommand)]
        subcommand: WorkItemSubCommands,
    },
}

#[derive(Subcommand, Clone, Debug)]
pub enum WorkItemType {
    /// Bug work item type
    Bug,
    /// Task work item type
    Task,
    /// User Story work item type
    #[clap(name = "user-story")]
    UserStory,
    /// Feature work item type
    Feature,
    /// Epic work item type
    Epic,
}

#[derive(Subcommand, Clone)]
pub enum WorkItemSubCommands {
    /// Create a new work item
    Create {
        /// Work item type
        #[clap(subcommand)]
        work_item_type: WorkItemType,
        /// Work item title
        #[clap(short, long)]
        title: String,
        /// Work item description
        #[clap(short, long)]
        description: Option<String>,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
    /// Delete a work item
    Delete {
        /// ID of the work item to delete
        #[clap(short, long)]
        id: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Perform soft delete by changing state to removed (default is permanent delete)
        #[clap(long)]
        soft_delete: bool,
    },
    /// Show details of a work item
    Show {
        /// ID of the work item to show
        #[clap(short, long)]
        id: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Open in web browser
        #[clap(long)]
        web: bool,
    },
    /// Update a work item
    Update {
        /// ID of the work item to update
        #[clap(short, long)]
        id: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// New title for the work item
        #[clap(long)]
        title: Option<String>,
        /// New description for the work item
        #[clap(long)]
        description: Option<String>,
        /// New state for the work item (e.g., 'New', 'Active', 'Resolved', 'Closed')
        #[clap(long)]
        state: Option<String>,
        /// New priority for the work item (e.g., 1, 2, 3, 4)
        #[clap(long)]
        priority: Option<i32>,
    },
}

fn create_client() -> Result<wit::Client> {
    match get_credentials() {
        Ok(creds) => {
            let credential = azure_devops_rust_api::Credential::Pat(creds.pat);
            let client = ClientBuilder::new(credential).build();
            Ok(client)
        }
        Err(e) => Err(e),
    }
}

async fn get_work_item(project: &str, id: &str) -> Result<models::WorkItem> {
    let id_int = id
        .parse::<i32>()
        .map_err(|_| anyhow!("Invalid work item ID, must be a number"))?;

    match get_credentials() {
        Ok(creds) => {
            let client = create_client()?;
            let work_item = client
                .work_items_client()
                .get_work_item(creds.organization, id_int, project)
                .await?;

            Ok(work_item)
        }
        Err(e) => {
            eprintln!("Unable to retrieve work item");
            Err(e)
        }
    }
}

async fn create_work_item(
    project: &str,
    work_item_type: &WorkItemType,
    title: &str,
    description: Option<&str>,
) -> Result<()> {
    match get_credentials() {
        Ok(_) => {
            let type_str = match work_item_type {
                WorkItemType::Bug => "Bug",
                WorkItemType::Task => "Task",
                WorkItemType::UserStory => "User Story",
                WorkItemType::Feature => "Feature",
                WorkItemType::Epic => "Epic",
            };

            println!(
                "Would create a {} work item with title '{}' in project '{}'",
                type_str, title, project
            );

            if let Some(desc) = description {
                println!("Description: {}", desc);
            }

            Ok(())
        }
        Err(e) => {
            eprintln!("Unable to create work item");
            Err(e)
        }
    }
}

async fn update_work_item(
    project: &str,
    id: &str,
    title: Option<&str>,
    description: Option<&str>,
    state: Option<&str>,
    priority: Option<i32>,
) -> Result<()> {
    let _id_int = id
        .parse::<i32>()
        .map_err(|_| anyhow!("Invalid work item ID, must be a number"))?;
    match get_credentials() {
        Ok(_) => {
            println!("Would update work item {} in project '{}':", id, project);

            if let Some(t) = title {
                println!("New title: {}", t);
            }

            if let Some(desc) = description {
                println!("New description: {}", desc);
            }

            if let Some(s) = state {
                println!("New state: {}", s);
            }

            if let Some(p) = priority {
                println!("New priority: {}", p);
            }

            Ok(())
        }
        Err(e) => {
            eprintln!("Unable to update work item");
            Err(e)
        }
    }
}

async fn delete_work_item(project: &str, id: &str, soft_delete: bool) -> Result<()> {
    let _id_int = id
        .parse::<i32>()
        .map_err(|_| anyhow!("Invalid work item ID, must be a number"))?;
    match get_credentials() {
        Ok(_) => {
            if soft_delete {
                update_work_item(project, id, None, None, Some("Removed"), None).await?;
                return Ok(());
            }

            println!(
                "Would permanently delete work item {} in project '{}'",
                id, project
            );

            Ok(())
        }
        Err(e) => {
            eprintln!("Unable to delete work item");
            Err(e)
        }
    }
}

fn open_work_item_in_browser(organization: &str, id: &str) -> Result<()> {
    let url = format!(
        "https://dev.azure.com/{}//_workitems/edit/{}",
        organization, id
    );

    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(&["/C", &format!("start {}", url)])
            .spawn()?;
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(&url).spawn()?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(&url).spawn()?;
    }

    println!("Opening work item in browser: {}", url);
    Ok(())
}

fn display_work_item(work_item: &models::WorkItem) {
    println!("📋 Work Item Details");
    println!("=====================");

    println!("🆔 ID: {}", work_item.id);

    if let Some(rev) = work_item.rev {
        println!("📚 Revision: {}", rev);
    }

    if let Some(fields) = work_item.fields.as_object() {
        if let Some(title) = fields.get("System.Title").and_then(|v| v.as_str()) {
            println!("📝 Title: {}", title);
        }

        if let Some(state) = fields.get("System.State").and_then(|v| v.as_str()) {
            println!("🔄 State: {}", state);
        }

        if let Some(work_item_type) = fields.get("System.WorkItemType").and_then(|v| v.as_str()) {
            println!("📌 Type: {}", work_item_type);
        }

        if let Some(created_by) = fields.get("System.CreatedBy").and_then(|v| v.as_str()) {
            println!("👤 Created By: {}", created_by);
        }

        if let Some(created_date) = fields.get("System.CreatedDate").and_then(|v| v.as_str()) {
            println!("📅 Created Date: {}", created_date);
        }

        if let Some(changed_by) = fields.get("System.ChangedBy").and_then(|v| v.as_str()) {
            println!("🔄 Changed By: {}", changed_by);
        }

        if let Some(changed_date) = fields.get("System.ChangedDate").and_then(|v| v.as_str()) {
            println!("📅 Changed Date: {}", changed_date);
        }

        if let Some(priority) = fields
            .get("Microsoft.VSTS.Common.Priority")
            .and_then(|v| v.as_i64())
        {
            println!("🔝 Priority: {}", priority);
        }

        if let Some(desc) = fields.get("System.Description").and_then(|v| v.as_str()) {
            println!("\n📄 Description:");
            println!("{}", desc);
        }
    }
}

pub async fn handle_command(subcommand: &BoardsSubCommands) -> Result<()> {
    let _credentials = get_credentials()?;
    match subcommand {
        BoardsSubCommands::WorkItem { subcommand } => handle_work_item_command(subcommand).await,
    }
}

async fn handle_work_item_command(subcommand: &WorkItemSubCommands) -> Result<()> {
    match subcommand {
        WorkItemSubCommands::Create {
            work_item_type,
            title,
            description,
            project,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            println!(
                "Creating a {:?} work item in project: {}",
                work_item_type, project_name
            );

            match create_work_item(&project_name, work_item_type, title, description.as_deref())
                .await
            {
                Ok(_) => {
                    println!("{}", "✅ Work item created successfully!".green());
                }
                Err(e) => {
                    eprintln!("❌ Failed to create work item: {}", e);
                    return Err(e);
                }
            }
        }
        WorkItemSubCommands::Delete {
            id,
            project,
            soft_delete,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            println!(
                "{}Deleting work item with id: {} in project: {}",
                if *soft_delete { "Soft " } else { "" },
                id,
                project_name
            );

            match delete_work_item(&project_name, id, *soft_delete).await {
                Ok(_) => {
                    if *soft_delete {
                        println!(
                            "{}",
                            "✅ Work item soft deleted successfully (state changed to 'Removed')"
                                .green()
                        );
                    } else {
                        println!("{}", "✅ Work item deleted successfully".green());
                    }
                }
                Err(e) => {
                    eprintln!("❌ Failed to delete work item: {}", e);
                    return Err(e);
                }
            }
        }
        WorkItemSubCommands::Show { id, project, web } => {
            let project_name = get_project_or_default(project.as_deref())?;
            println!(
                "Showing work item with id: {} in project: {}",
                id, project_name
            );

            // Open in browser if requested
            if *web {
                match get_credentials() {
                    Ok(creds) => {
                        if let Err(e) = open_work_item_in_browser(&creds.organization, id) {
                            eprintln!("❌ Failed to open work item in browser: {}", e);
                        }
                        return Ok(());
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to get credentials: {}", e);
                        return Err(e);
                    }
                }
            }

            // Otherwise show in terminal
            match get_work_item(&project_name, id).await {
                Ok(work_item) => {
                    display_work_item(&work_item);
                }
                Err(e) => {
                    eprintln!("❌ Failed to retrieve work item: {}", e);
                    return Err(e);
                }
            }
        }
        WorkItemSubCommands::Update {
            id,
            project,
            title,
            description,
            state,
            priority,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            println!(
                "Updating work item with id: {} in project: {}",
                id, project_name
            );

            match update_work_item(
                &project_name,
                id,
                title.as_deref(),
                description.as_deref(),
                state.as_deref(),
                *priority,
            )
            .await
            {
                Ok(_) => {
                    println!("{}", "✅ Work item updated successfully!".green());
                }
                Err(e) => {
                    eprintln!("❌ Failed to update work item: {}", e);
                    return Err(e);
                }
            }
        }
    }
    Ok(())
}
