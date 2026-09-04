use crate::auth::factory::{ClientFactory, CredentialClientFactory};
use crate::auth::get_credentials;
use crate::auth::url::web_work_item_url;
use crate::project::get_project_or_default;
use crate::text::escape_control_characters;
use anyhow::{anyhow, Result};
use azure_devops_rust_api::wit::models::json_patch_operation::Op;
use azure_devops_rust_api::wit::models::JsonPatchOperation;
use azure_devops_rust_api::wit::{self, models};
use clap::Subcommand;
use colored::Colorize;
use serde_json::json;

#[derive(Subcommand, Clone)]
pub enum BoardsSubCommands {
    /// Manage work items
    WorkItem {
        #[clap(subcommand)]
        subcommand: WorkItemSubCommands,
    },
    /// Run a WIQL query and list the work items it returns
    Query {
        /// The WIQL query to run
        #[clap(short, long)]
        wiql: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Maximum number of work items to return (default: 50)
        #[clap(long, default_value = "50")]
        limit: i32,
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
pub enum WorkItemCommentSubCommands {
    /// List the comments on a work item
    List {
        /// ID of the work item
        #[clap(short, long)]
        id: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Maximum number of comments to return
        #[clap(long)]
        top: Option<i32>,
    },
    /// Add a comment to a work item
    Add {
        /// ID of the work item
        #[clap(short, long)]
        id: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// The comment text
        #[clap(short, long)]
        message: String,
    },
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
    /// Read and write the comments on a work item
    Comment {
        #[clap(subcommand)]
        subcommand: WorkItemCommentSubCommands,
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

async fn list_work_item_comments(
    project: &str,
    id: &str,
    top: Option<i32>,
) -> Result<Vec<models::Comment>> {
    let creds = get_credentials()?;
    let client = create_wit_client()?;

    let mut request =
        client
            .comments_client()
            .get_comments(creds.organization, project, parse_work_item_id(id)?);

    if let Some(top) = top {
        request = request.top(top);
    }

    Ok(request.await?.comments)
}

async fn add_work_item_comment(project: &str, id: &str, message: &str) -> Result<models::Comment> {
    let creds = get_credentials()?;
    let client = create_wit_client()?;

    Ok(client
        .comments_client()
        .add_comment(
            creds.organization,
            models::CommentCreate {
                text: Some(message.to_string()),
            },
            project,
            parse_work_item_id(id)?,
        )
        .await?)
}

fn display_work_item_comments(comments: &[models::Comment]) {
    if comments.is_empty() {
        println!("No comments found.");
        return;
    }

    for comment in comments {
        let author = comment
            .created_by
            .as_ref()
            .and_then(|identity| identity.graph_subject_base.display_name.as_deref())
            .unwrap_or("Unknown");
        let created = comment
            .created_date
            .map(|created| created.to_string())
            .unwrap_or_else(|| "-".to_string());

        println!(
            "💬 {} - {}",
            escape_control_characters(author).bold(),
            created
        );

        if let Some(id) = comment.id {
            println!("   Comment ID: {id}");
        }

        println!(
            "{}",
            escape_control_characters(comment.text.as_deref().unwrap_or(""))
        );
        println!();
    }

    println!("{} comment(s)", comments.len());
}

/// Work item ids reach us as strings from the command line.
fn parse_work_item_id(id: &str) -> Result<i32> {
    id.parse::<i32>()
        .map_err(|_| anyhow!("Invalid work item ID '{id}', must be a number"))
}

fn create_wit_client() -> Result<wit::Client> {
    let creds = get_credentials()?;
    let factory = CredentialClientFactory::new(&creds)?;
    Ok(factory.build_wit())
}

async fn get_work_item(project: &str, id: &str) -> Result<models::WorkItem> {
    let id_int = parse_work_item_id(id)?;

    match get_credentials() {
        Ok(creds) => {
            let client = create_wit_client()?;
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
) -> Result<models::WorkItem> {
    match get_credentials() {
        Ok(creds) => {
            let client = create_wit_client()?;
            let work_item = client
                .work_items_client()
                .create(
                    creds.organization.clone(),
                    vec![JsonPatchOperation {
                        from: None,
                        op: Some(Op::Add),
                        path: Some("/fields/System.Title".to_owned()),
                        value: Some(json!(title)),
                    }],
                    project.to_string(),
                    match work_item_type {
                        WorkItemType::Bug => "Bug",
                        WorkItemType::Task => "Task",
                        WorkItemType::UserStory => "User Story",
                        WorkItemType::Feature => "Feature",
                        WorkItemType::Epic => "Epic",
                    },
                )
                .await?;

            Ok(work_item)
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
) -> Result<models::WorkItem> {
    let id_int = parse_work_item_id(id)?;

    match get_credentials() {
        Ok(creds) => {
            let client = create_wit_client()?;
            let mut patch_operations = Vec::new();

            if let Some(title) = title {
                patch_operations.push(JsonPatchOperation {
                    from: None,
                    op: Some(Op::Add),
                    path: Some("/fields/System.Title".to_owned()),
                    value: Some(json!(title)),
                });
            }

            if let Some(description) = description {
                patch_operations.push(JsonPatchOperation {
                    from: None,
                    op: Some(Op::Add),
                    path: Some("/fields/System.Description".to_owned()),
                    value: Some(json!(description)),
                });
            }

            if let Some(state) = state {
                patch_operations.push(JsonPatchOperation {
                    from: None,
                    op: Some(Op::Add),
                    path: Some("/fields/System.State".to_owned()),
                    value: Some(json!(state)),
                });
            }

            if let Some(priority) = priority {
                patch_operations.push(JsonPatchOperation {
                    from: None,
                    op: Some(Op::Add),
                    path: Some("/fields/Microsoft.VSTS.Common.Priority".to_owned()),
                    value: Some(json!(priority)),
                });
            }

            let work_item = client
                .work_items_client()
                .update(
                    creds.organization,
                    patch_operations,
                    id_int,
                    project.to_string(),
                )
                .await?;
            Ok(work_item)
        }
        Err(e) => {
            eprintln!("Unable to update work item");
            Err(e)
        }
    }
}

async fn delete_work_item(project: &str, id: &str, soft_delete: bool) -> Result<()> {
    let id_int = parse_work_item_id(id)?;
    match get_credentials() {
        Ok(creds) => {
            if soft_delete {
                let work_item = get_work_item(project, id).await?;
                let work_item_type = work_item
                    .fields
                    .get("System.WorkItemType")
                    .and_then(|v| v.as_str());
                let state = work_item_type
                    .map(|wt| {
                        if wt == "Task" || wt == "User Story" || wt == "Feature" || wt == "Epic" {
                            "Removed"
                        } else {
                            "Closed"
                        }
                    })
                    .unwrap_or("Closed");
                update_work_item(project, id, None, None, Some(state), None).await?;
            } else {
                create_wit_client()?
                    .work_items_client()
                    .delete(creds.organization, id_int, project.to_string())
                    .await?;
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("Unable to delete work item");
            Err(e)
        }
    }
}

fn open_work_item_in_browser(
    base_url: &str,
    organization: &str,
    project: &str,
    id: &str,
) -> Result<()> {
    let url = web_work_item_url(base_url, organization, project, id);

    crate::browser::open_url(&url)?;

    println!("Opening work item in browser: {url}");
    Ok(())
}

fn display_work_item(work_item: &models::WorkItem) {
    println!("📋 Work Item Details");
    println!("=====================");

    println!("🆔 ID: {}", work_item.id);

    if let Some(rev) = work_item.rev {
        println!("📚 Revision: {rev}");
    }

    if let Some(fields) = work_item.fields.as_object() {
        if let Some(title) = fields.get("System.Title").and_then(|v| v.as_str()) {
            println!("📝 Title: {title}");
        }

        if let Some(state) = fields.get("System.State").and_then(|v| v.as_str()) {
            println!("🔄 State: {state}");
        }

        if let Some(work_item_type) = fields.get("System.WorkItemType").and_then(|v| v.as_str()) {
            println!("📌 Type: {work_item_type}");
        }

        if let Some(created_by) = fields.get("System.CreatedBy").and_then(|v| v.as_str()) {
            println!("👤 Created By: {created_by}");
        }

        if let Some(created_date) = fields.get("System.CreatedDate").and_then(|v| v.as_str()) {
            println!("📅 Created Date: {created_date}");
        }

        if let Some(changed_by) = fields.get("System.ChangedBy").and_then(|v| v.as_str()) {
            println!("🔄 Changed By: {changed_by}");
        }

        if let Some(changed_date) = fields.get("System.ChangedDate").and_then(|v| v.as_str()) {
            println!("📅 Changed Date: {changed_date}");
        }

        if let Some(priority) = fields
            .get("Microsoft.VSTS.Common.Priority")
            .and_then(serde_json::Value::as_i64)
        {
            println!("🔝 Priority: {priority}");
        }

        if let Some(desc) = fields.get("System.Description").and_then(|v| v.as_str()) {
            println!("\n📄 Description:");
            println!("{desc}");
        }
    }
}

fn display_work_items_list(heading: &str, work_items: &[models::WorkItem]) {
    println!();
    println!("📋 {heading} ({} items)", work_items.len());
    let separator = "=".repeat(80);
    println!("{separator}");
    println!("{:<8} {:<15} {:<20} {:<30}", "ID", "Type", "State", "Title");
    let dash_separator = "-".repeat(80);
    println!("{dash_separator}");

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

        println!("{id:<8} {work_item_type:<15} {state:<20} {truncated_title:<30}");
    }

    println!();
    println!("💡 Use 'azdocli boards work-item show --id <ID>' for detailed information");
    println!("💡 Use 'azdocli boards work-item show --id <ID> --web' to open in browser");
}

/// Runs a WIQL query and fetches the full work items behind the ids it returns.
async fn run_wiql_query(project: &str, wiql: &str, limit: i32) -> Result<Vec<models::WorkItem>> {
    let creds = get_credentials()?;
    let client = create_wit_client()?;

    let query_result = client
        .wiql_client()
        .query_by_wiql(
            creds.organization.clone(),
            models::Wiql {
                query: Some(wiql.to_string()),
            },
            project.to_string(),
            String::new(),
        )
        .await?;

    let mut work_items = Vec::new();
    for work_item_ref in query_result.work_items.iter().take(limit as usize) {
        if let Some(id) = work_item_ref.id {
            match client
                .work_items_client()
                .get_work_item(creds.organization.clone(), id, project)
                .await
            {
                Ok(work_item) => work_items.push(work_item),
                Err(e) => eprintln!("❌ Failed to get details for work item {id}: {e}"),
            }
        }
    }

    Ok(work_items)
}

async fn list_my_work_items(
    project: &str,
    state_filter: Option<&str>,
    work_item_type_filter: Option<&str>,
    limit: i32,
) -> Result<()> {
    println!("📋 Listing work items assigned to you in project: {project}");

    if let Some(state) = state_filter {
        println!("🔍 Filtering by state: {state}");
    }

    if let Some(wit_type) = work_item_type_filter {
        println!("🔍 Filtering by type: {wit_type}");
    }

    println!("📊 Limit: {limit} items");

    let wiql_query = build_wiql_query(project, state_filter, work_item_type_filter);

    match run_wiql_query(project, &wiql_query, limit).await {
        Ok(work_items) if work_items.is_empty() => display_empty_work_items_table("My Work Items"),
        Ok(work_items) => display_work_items_list("My Work Items", &work_items),
        Err(e) => {
            eprintln!("❌ Failed to execute WIQL query: {e}");
            display_empty_work_items_table("My Work Items");
        }
    }

    Ok(())
}

/// Sanitizes a string for use in WIQL queries by escaping single quotes
fn sanitize_wiql_value(value: &str) -> String {
    value.replace('\'', "''")
}

fn build_wiql_query(
    project: &str,
    state_filter: Option<&str>,
    work_item_type_filter: Option<&str>,
) -> String {
    let sanitized_project = sanitize_wiql_value(project);
    let mut wiql_query = format!(
        "SELECT [System.Id], [System.Title], [System.State], [System.WorkItemType], [System.AssignedTo], [System.CreatedDate], [Microsoft.VSTS.Common.Priority] FROM WorkItems WHERE [System.TeamProject] = '{sanitized_project}' AND [System.AssignedTo] = @Me"
    );

    // Add state filter if provided
    if let Some(state) = state_filter {
        let sanitized_state = sanitize_wiql_value(state);
        wiql_query.push_str(&format!(" AND [System.State] = '{sanitized_state}'"));
    }

    // Add work item type filter if provided
    if let Some(wit_type) = work_item_type_filter {
        let sanitized_wit_type = sanitize_wiql_value(wit_type);
        wiql_query.push_str(&format!(
            " AND [System.WorkItemType] = '{sanitized_wit_type}'"
        ));
    }

    wiql_query.push_str(" ORDER BY [System.CreatedDate] DESC");
    wiql_query
}

fn display_empty_work_items_table(heading: &str) {
    println!();
    println!("📋 {heading} (0 items)");
    let separator = "=".repeat(80);
    println!("{separator}");
    println!("{:<8} {:<15} {:<20} {:<30}", "ID", "Type", "State", "Title");
    let dash_separator = "-".repeat(80);
    println!("{dash_separator}");
    println!("No work items found.");
    println!();
    println!("💡 Use 'azdocli boards work-item show --id <ID>' for detailed information");
    println!("💡 Use 'azdocli boards work-item show --id <ID> --web' to open in browser");
}

pub async fn handle_command(subcommand: &BoardsSubCommands) -> Result<()> {
    let _credentials = get_credentials()?;
    match subcommand {
        BoardsSubCommands::WorkItem { subcommand } => handle_work_item_command(subcommand).await,
        BoardsSubCommands::Query {
            wiql,
            project,
            limit,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            match run_wiql_query(&project_name, wiql, *limit).await {
                Ok(work_items) if work_items.is_empty() => {
                    display_empty_work_items_table("Query Results")
                }
                Ok(work_items) => display_work_items_list("Query Results", &work_items),
                Err(e) => {
                    eprintln!("❌ Failed to execute WIQL query");
                    eprintln!("   {e}");
                    return Err(e);
                }
            }
            Ok(())
        }
    }
}

async fn handle_work_item_comment_command(subcommand: &WorkItemCommentSubCommands) -> Result<()> {
    match subcommand {
        WorkItemCommentSubCommands::List { id, project, top } => {
            let project_name = get_project_or_default(project.as_deref())?;
            match list_work_item_comments(&project_name, id, *top).await {
                Ok(comments) => display_work_item_comments(&comments),
                Err(e) => {
                    eprintln!("❌ Failed to list comments on work item {id}");
                    eprintln!("   {e}");
                    return Err(e);
                }
            }
        }
        WorkItemCommentSubCommands::Add {
            id,
            project,
            message,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            match add_work_item_comment(&project_name, id, message).await {
                Ok(comment) => {
                    println!("{}", "✅ Comment added successfully!".green());
                    if let Some(comment_id) = comment.id {
                        println!("Created comment with ID: {comment_id}");
                    }
                }
                Err(e) => {
                    eprintln!("❌ Failed to add a comment to work item {id}");
                    eprintln!("   {e}");
                    return Err(e);
                }
            }
        }
    }

    Ok(())
}

async fn handle_work_item_command(subcommand: &WorkItemSubCommands) -> Result<()> {
    match subcommand {
        WorkItemSubCommands::Create {
            work_item_type,
            title,
            project,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            println!("Creating a {work_item_type:?} work item in project: {project_name}");

            match create_work_item(&project_name, work_item_type, title).await {
                Ok(work_item) => {
                    println!("{}", "✅ Work item created successfully!".green());
                    println!("Created work item with ID: {}", work_item.id);
                    println!("Title: {title}");
                    if let Some(fields) = work_item.fields.as_object() {
                        if let Some(desc) =
                            fields.get("System.Description").and_then(|v| v.as_str())
                        {
                            println!("Description: {desc}");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("❌ Failed to create work item: {e}");
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
                    eprintln!("❌ Failed to delete work item: {e}");
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
                    eprintln!("❌ Failed to list work items: {e}");
                    return Err(e);
                }
            }
        }
        WorkItemSubCommands::Show { id, project, web } => {
            let project_name = get_project_or_default(project.as_deref())?;
            println!("Showing work item with id: {id} in project: {project_name}");

            // Open in browser if requested
            if *web {
                match get_credentials() {
                    Ok(creds) => {
                        if let Err(e) = open_work_item_in_browser(
                            &creds.base_url,
                            &creds.organization,
                            &project_name,
                            id,
                        ) {
                            eprintln!("❌ Failed to open work item in browser: {e}");
                        }
                        return Ok(());
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to get credentials: {e}");
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
                    eprintln!("❌ Failed to retrieve work item: {e}");
                    return Err(e);
                }
            }
        }
        WorkItemSubCommands::Comment { subcommand } => {
            return handle_work_item_comment_command(subcommand).await;
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
            println!("Updating work item with id: {id} in project: {project_name}");

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
                Ok(work_item) => {
                    println!("{}", "✅ Work item updated successfully!".green());
                    println!("Updated work item with ID: {}", work_item.id);
                    if let Some(fields) = work_item.fields.as_object() {
                        if let Some(updated_title) =
                            fields.get("System.Title").and_then(|v| v.as_str())
                        {
                            println!("Updated Title: {updated_title}");
                        }
                        if let Some(updated_desc) =
                            fields.get("System.Description").and_then(|v| v.as_str())
                        {
                            println!("Updated Description: {updated_desc}");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("❌ Failed to update work item: {e}");
                    return Err(e);
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_wiql_value_escapes_single_quotes() {
        assert_eq!(sanitize_wiql_value("test'value"), "test''value");
        assert_eq!(sanitize_wiql_value("test''value"), "test''''value");
        assert_eq!(sanitize_wiql_value("O'Brien"), "O''Brien");
    }

    #[test]
    fn test_sanitize_wiql_value_preserves_safe_strings() {
        assert_eq!(sanitize_wiql_value("Active"), "Active");
        assert_eq!(sanitize_wiql_value("Bug"), "Bug");
        assert_eq!(sanitize_wiql_value("User Story"), "User Story");
    }

    #[test]
    fn test_sanitize_wiql_value_prevents_injection() {
        // Attempt to inject WIQL by closing the quote
        let malicious_input = "'; DROP TABLE WorkItems; --";
        let sanitized = sanitize_wiql_value(malicious_input);
        assert_eq!(sanitized, "''; DROP TABLE WorkItems; --");
        // The doubled quote prevents breaking out of the string context
    }

    #[test]
    fn test_build_wiql_query_sanitizes_project() {
        let query = build_wiql_query("test'project", None, None);
        assert!(query.contains("test''project"));
        assert!(!query.contains("test'project' AND"));
    }

    #[test]
    fn test_build_wiql_query_sanitizes_state() {
        let query = build_wiql_query("project", Some("Active'Hack"), None);
        assert!(query.contains("Active''Hack"));
    }

    #[test]
    fn test_build_wiql_query_sanitizes_work_item_type() {
        let query = build_wiql_query("project", None, Some("Bug'Injection"));
        assert!(query.contains("Bug''Injection"));
    }
}
