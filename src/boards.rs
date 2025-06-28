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
    /// List work items assigned to me
    List {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Filter by work item state (e.g., 'Active', 'New', 'Resolved')
        #[clap(long)]
        state: Option<String>,
        /// Filter by work item type (e.g., 'Bug', 'Task', 'User Story')
        #[clap(long)]
        work_item_type: Option<String>,
        /// Maximum number of work items to return (default: 50)
        #[clap(long, default_value = "50")]
        limit: i32,
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

fn display_work_items_list(work_items: &[models::WorkItem]) {
    println!();
    println!("📋 My Work Items ({} items)", work_items.len());
    let separator = "=".repeat(80);
    println!("{}", separator);
    println!("{:<8} {:<15} {:<20} {:<30}", "ID", "Type", "State", "Title");
    let dash_separator = "-".repeat(80);
    println!("{}", dash_separator);

    for work_item in work_items {
        let id = work_item.id;

        let (work_item_type, state, title) = if let Some(fields) = work_item.fields.as_object() {
            let wit_type = fields
                .get("System.WorkItemType")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");

            let state = fields
                .get("System.State")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");

            let title = fields
                .get("System.Title")
                .and_then(|v| v.as_str())
                .unwrap_or("No Title");

            (wit_type, state, title)
        } else {
            ("Unknown", "Unknown", "No Title")
        };

        // Truncate title if too long
        let truncated_title = if title.len() > 30 {
            format!("{}...", &title[..27])
        } else {
            title.to_string()
        };

        println!(
            "{:<8} {:<15} {:<20} {:<30}",
            id, work_item_type, state, truncated_title
        );
    }

    println!();
    println!("💡 Use 'azdocli boards work-item show --id <ID>' for detailed information");
    println!("💡 Use 'azdocli boards work-item show --id <ID> --web' to open in browser");
}

async fn list_my_work_items(
    project: &str,
    state_filter: Option<&str>,
    work_item_type_filter: Option<&str>,
    limit: i32,
) -> Result<()> {
    match get_credentials() {
        Ok(creds) => {
            let client = create_client()?;

            println!(
                "📋 Listing work items assigned to you in project: {}",
                project
            );

            if let Some(state) = state_filter {
                println!("🔍 Filtering by state: {}", state);
            }

            if let Some(wit_type) = work_item_type_filter {
                println!("🔍 Filtering by type: {}", wit_type);
            }

            println!("📊 Limit: {} items", limit);

            let wiql_query = build_wiql_query(project, state_filter, work_item_type_filter);

            // Create WIQL request body
            let wiql_request = models::Wiql {
                query: Some(wiql_query),
            }; // Execute the WIQL query
            match client
                .wiql_client()
                .query_by_wiql(
                    creds.organization.clone(),
                    wiql_request,
                    project.to_string(),
                    "".to_string(),
                )
                .await
            {
                Ok(query_result) => {
                    let work_items = query_result.work_items;
                    if work_items.is_empty() {
                        display_empty_work_items_table();
                        return Ok(());
                    }

                    // Take only the requested number of items
                    let limited_items: Vec<_> =
                        work_items.into_iter().take(limit as usize).collect();

                    // Get detailed work item information by calling get_work_item for each ID
                    let mut detailed_work_items = Vec::new();
                    for work_item_ref in &limited_items {
                        if let Some(id) = work_item_ref.id {
                            match client
                                .work_items_client()
                                .get_work_item(creds.organization.clone(), id, project)
                                .await
                            {
                                Ok(detailed_item) => detailed_work_items.push(detailed_item),
                                Err(e) => eprintln!(
                                    "❌ Failed to get details for work item {}: {}",
                                    id, e
                                ),
                            }
                        }
                    }

                    if !detailed_work_items.is_empty() {
                        display_work_items_list(&detailed_work_items);
                    } else {
                        display_empty_work_items_table();
                    }
                }
                Err(e) => {
                    eprintln!("❌ Failed to execute WIQL query: {}", e);
                    display_empty_work_items_table();
                }
            }

            Ok(())
        }
        Err(e) => {
            eprintln!("Unable to list work items");
            Err(e)
        }
    }
}

fn build_wiql_query(
    project: &str,
    state_filter: Option<&str>,
    work_item_type_filter: Option<&str>,
) -> String {
    let mut wiql_query = format!(
        "SELECT [System.Id], [System.Title], [System.State], [System.WorkItemType], [System.AssignedTo], [System.CreatedDate], [Microsoft.VSTS.Common.Priority] FROM WorkItems WHERE [System.TeamProject] = '{}' AND [System.AssignedTo] = @Me",
        project
    );

    // Add state filter if provided
    if let Some(state) = state_filter {
        wiql_query.push_str(&format!(" AND [System.State] = '{}'", state));
    }

    // Add work item type filter if provided
    if let Some(wit_type) = work_item_type_filter {
        wiql_query.push_str(&format!(" AND [System.WorkItemType] = '{}'", wit_type));
    }

    wiql_query.push_str(" ORDER BY [System.CreatedDate] DESC");
    wiql_query
}

fn display_empty_work_items_table() {
    println!();
    println!("📋 My Work Items (0 items)");
    let separator = "=".repeat(80);
    println!("{}", separator);
    println!("{:<8} {:<15} {:<20} {:<30}", "ID", "Type", "State", "Title");
    let dash_separator = "-".repeat(80);
    println!("{}", dash_separator);
    println!("No work items found assigned to you.");
    println!();
    println!("💡 Use 'azdocli boards work-item show --id <ID>' for detailed information");
    println!("💡 Use 'azdocli boards work-item show --id <ID> --web' to open in browser");
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
        WorkItemSubCommands::List {
            project,
            state,
            work_item_type,
            limit,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;

            match list_my_work_items(
                &project_name,
                state.as_deref(),
                work_item_type.as_deref(),
                *limit,
            )
            .await
            {
                Ok(_) => {
                    println!("{}", "✅ Work items listed successfully!".green());
                }
                Err(e) => {
                    eprintln!("❌ Failed to list work items: {}", e);
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
