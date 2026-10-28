use crate::auth::factory::{ClientFactory, CredentialClientFactory};
use crate::auth::get_credentials;
use crate::auth::url::{api_work_item_url, web_work_item_url};
use crate::output::{self, OutputFormat};
use crate::project::get_project_or_default;
use crate::text::escape_control_characters;
use anyhow::{anyhow, Result};
use azure_devops_rust_api::wit::models::json_patch_operation::Op;
use azure_devops_rust_api::wit::models::JsonPatchOperation;
use azure_devops_rust_api::wit::{self, models};
use clap::{Subcommand, ValueEnum};
use colored::Colorize;
use serde_json::json;

mod area;
mod classification;
mod iteration;
mod views;

use views::{WorkItemDetails, WorkItemList};

#[derive(Subcommand, Clone)]
pub enum BoardsSubCommands {
    /// Manage work items
    WorkItem {
        #[clap(subcommand)]
        subcommand: WorkItemSubCommands,
    },
    /// Manage area paths of a project and its teams
    Area {
        #[clap(subcommand)]
        subcommand: area::AreaSubCommands,
    },
    /// Manage iterations of a project and its teams
    Iteration {
        #[clap(subcommand)]
        subcommand: iteration::IterationSubCommands,
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
        #[clap(long, default_value = "50", value_parser = clap::value_parser!(i32).range(1..))]
        limit: i32,
    },
}

#[derive(ValueEnum, Clone, Debug)]
pub enum WorkItemType {
    /// Bug work item type
    Bug,
    /// Task work item type
    Task,
    /// User Story work item type
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
        #[clap(long, value_parser = clap::value_parser!(i32).range(1..))]
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
        #[clap(value_enum)]
        work_item_type: WorkItemType,
        /// Work item title
        #[clap(short, long)]
        title: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// ID of the parent work item, e.g. the product backlog item a task belongs to
        #[clap(long, value_parser = clap::value_parser!(i32).range(1..))]
        parent: Option<i32>,
        /// Iteration path for the work item
        #[clap(long = "iteration")]
        iteration_path: Option<String>,
        /// Area path for the work item
        #[clap(long = "area")]
        area_path: Option<String>,
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
    /// List work items assigned to me, to someone else, or to anyone
    List {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// List work items assigned to this person (display name or email; '@me' for yourself)
        #[clap(long, conflicts_with = "all")]
        assigned_to: Option<String>,
        /// List all work items in the project, whoever they are assigned to
        #[clap(long)]
        all: bool,
        /// Filter by work item state (e.g., 'Active', 'New', 'Resolved')
        #[clap(long)]
        state: Option<String>,
        /// Filter by work item type (e.g., 'Bug', 'Task', 'User Story')
        #[clap(long)]
        work_item_type: Option<String>,
        /// Maximum number of work items to return (default: 50)
        #[clap(long, default_value = "50", value_parser = clap::value_parser!(i32).range(1..))]
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
    /// List the work item types available in a project
    Types {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
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
        /// New iteration path for the work item
        #[clap(long = "iteration")]
        iteration_path: Option<String>,
        /// New area path for the work item
        #[clap(long = "area")]
        area_path: Option<String>,
    },
}

async fn list_work_item_types(project: &str) -> Result<Vec<models::WorkItemType>> {
    let creds = get_credentials()?;
    let client = create_wit_client()?;

    Ok(client
        .work_item_types_client()
        .list(creds.organization, project)
        .await?
        .value)
}

fn display_work_item_types(work_item_types: &[models::WorkItemType]) {
    if work_item_types.is_empty() {
        println!("No work item types found.");
        return;
    }

    println!("{:<28} {}", "Name".bold(), "Description".bold());
    println!("{}", "-".repeat(90));

    for work_item_type in work_item_types {
        let description = work_item_type
            .description
            .as_deref()
            .unwrap_or("")
            .lines()
            .next()
            .unwrap_or("");

        println!(
            "{:<28} {}",
            escape_control_characters(work_item_type.name.as_deref().unwrap_or("-")),
            escape_control_characters(description)
        );
    }

    println!("\n{} type(s)", work_item_types.len());
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

fn create_work_client() -> Result<azure_devops_rust_api::work::Client> {
    let creds = get_credentials()?;
    let factory = CredentialClientFactory::new(&creds)?;
    Ok(factory.build_work())
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

/// Builds the patch that creates a work item, optionally as a child of the
/// work item at `parent_url`.
fn create_work_item_patch(
    title: &str,
    parent_url: Option<&str>,
    iteration_path: Option<&str>,
    area_path: Option<&str>,
) -> Vec<JsonPatchOperation> {
    let mut patch = vec![JsonPatchOperation {
        from: None,
        op: Some(Op::Add),
        path: Some("/fields/System.Title".to_owned()),
        value: Some(json!(title)),
    }];

    for (field, value) in [
        ("System.IterationPath", iteration_path),
        ("System.AreaPath", area_path),
    ] {
        if let Some(value) = value {
            patch.push(JsonPatchOperation {
                from: None,
                op: Some(Op::Add),
                path: Some(format!("/fields/{field}")),
                value: Some(json!(value)),
            });
        }
    }

    if let Some(parent_url) = parent_url {
        patch.push(JsonPatchOperation {
            from: None,
            op: Some(Op::Add),
            path: Some("/relations/-".to_owned()),
            value: Some(json!({
                "rel": "System.LinkTypes.Hierarchy-Reverse",
                "url": parent_url,
            })),
        });
    }

    patch
}

async fn create_work_item(
    project: &str,
    work_item_type: &WorkItemType,
    title: &str,
    parent: Option<i32>,
    iteration_path: Option<&str>,
    area_path: Option<&str>,
) -> Result<models::WorkItem> {
    match get_credentials() {
        Ok(creds) => {
            let client = create_wit_client()?;
            let parent_url =
                parent.map(|id| api_work_item_url(&creds.base_url, &creds.organization, id));
            let work_item = client
                .work_items_client()
                .create(
                    creds.organization.clone(),
                    create_work_item_patch(title, parent_url.as_deref(), iteration_path, area_path),
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

fn update_work_item_patch(
    title: Option<&str>,
    description: Option<&str>,
    state: Option<&str>,
    priority: Option<i32>,
    iteration_path: Option<&str>,
    area_path: Option<&str>,
) -> Vec<JsonPatchOperation> {
    let mut patch_operations = Vec::new();

    for (field, value) in [
        ("System.Title", title.map(|value| json!(value))),
        ("System.Description", description.map(|value| json!(value))),
        ("System.State", state.map(|value| json!(value))),
        (
            "Microsoft.VSTS.Common.Priority",
            priority.map(|priority| json!(priority)),
        ),
        (
            "System.IterationPath",
            iteration_path.map(|value| json!(value)),
        ),
        ("System.AreaPath", area_path.map(|value| json!(value))),
    ] {
        if let Some(value) = value {
            patch_operations.push(JsonPatchOperation {
                from: None,
                op: Some(Op::Add),
                path: Some(format!("/fields/{field}")),
                value: Some(value),
            });
        }
    }

    patch_operations
}

async fn update_work_item(
    project: &str,
    id: &str,
    patch_operations: Vec<JsonPatchOperation>,
) -> Result<models::WorkItem> {
    let id_int = parse_work_item_id(id)?;

    match get_credentials() {
        Ok(creds) => {
            let client = create_wit_client()?;
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
                update_work_item(
                    project,
                    id,
                    update_work_item_patch(None, None, Some(state), None, None, None),
                )
                .await?;
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

pub(crate) fn display_work_items_list(heading: &str, work_items: &[models::WorkItem]) {
    // Text-only callers print like `println!` and have no error to return.
    if let Err(e) = print_work_items(heading, work_items, OutputFormat::Text) {
        eprintln!("❌ Failed to print work items: {e}");
    }
}

/// Prints a titled work item list in `format`.
fn print_work_items(
    heading: &str,
    work_items: &[models::WorkItem],
    format: OutputFormat,
) -> Result<()> {
    let list = WorkItemList {
        heading,
        work_items,
    };
    output::emit(&list, format)
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
        // Without $top the query returns every matching id, which for a whole
        // project can run into Azure DevOps' result and time limits.
        .top(limit)
        .await?;

    let mut work_items = Vec::new();
    // A negative limit would widen into a huge `usize`, so it is clamped to
    // nothing rather than to everything. The CLI already rejects one.
    let limit = usize::try_from(limit).unwrap_or(0);

    for work_item_ref in query_result.work_items.iter().take(limit) {
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

/// Whose work items `work-item list` shows.
#[derive(Debug, PartialEq)]
enum Assignee {
    Me,
    Person(String),
    Anyone,
}

impl Assignee {
    fn from_args(assigned_to: Option<&str>, all: bool) -> Self {
        match assigned_to {
            _ if all => Assignee::Anyone,
            None => Assignee::Me,
            Some(person) if person.eq_ignore_ascii_case("@me") => Assignee::Me,
            Some(person) => Assignee::Person(person.to_string()),
        }
    }

    fn heading(&self) -> String {
        match self {
            Assignee::Me => "My Work Items".to_string(),
            Assignee::Person(person) => {
                format!(
                    "Work Items Assigned to {}",
                    escape_control_characters(person)
                )
            }
            Assignee::Anyone => "All Work Items".to_string(),
        }
    }
}

async fn list_work_items(
    project: &str,
    assignee: &Assignee,
    state_filter: Option<&str>,
    work_item_type_filter: Option<&str>,
    limit: i32,
    format: OutputFormat,
) -> Result<()> {
    if format == OutputFormat::Text {
        print_listing_criteria(
            project,
            assignee,
            state_filter,
            work_item_type_filter,
            limit,
        );
    }

    let wiql_query = build_wiql_query(project, assignee, state_filter, work_item_type_filter);
    let heading = assignee.heading();

    match run_wiql_query(project, &wiql_query, limit).await {
        Ok(work_items) => print_work_items(&heading, &work_items, format),
        Err(e) => {
            eprintln!("❌ Failed to execute WIQL query: {e}");
            print_work_items(&heading, &[], format)
        }
    }
}

/// Tells the user which work items a listing is about to fetch.
fn print_listing_criteria(
    project: &str,
    assignee: &Assignee,
    state_filter: Option<&str>,
    work_item_type_filter: Option<&str>,
    limit: i32,
) {
    match assignee {
        Assignee::Me => println!("📋 Listing work items assigned to you in project: {project}"),
        Assignee::Person(person) => println!(
            "📋 Listing work items assigned to {} in project: {project}",
            escape_control_characters(person)
        ),
        Assignee::Anyone => println!("📋 Listing all work items in project: {project}"),
    }

    if let Some(state) = state_filter {
        println!("🔍 Filtering by state: {state}");
    }

    if let Some(wit_type) = work_item_type_filter {
        println!("🔍 Filtering by type: {wit_type}");
    }

    println!("📊 Limit: {limit} items");
}

/// Sanitizes a string for use in WIQL queries by escaping single quotes
fn sanitize_wiql_value(value: &str) -> String {
    value.replace('\'', "''")
}

fn build_wiql_query(
    project: &str,
    assignee: &Assignee,
    state_filter: Option<&str>,
    work_item_type_filter: Option<&str>,
) -> String {
    let sanitized_project = sanitize_wiql_value(project);
    let mut wiql_query = format!(
        "SELECT [System.Id], [System.Title], [System.State], [System.WorkItemType], [System.AssignedTo], [System.CreatedDate], [Microsoft.VSTS.Common.Priority] FROM WorkItems WHERE [System.TeamProject] = '{sanitized_project}'"
    );

    match assignee {
        Assignee::Me => wiql_query.push_str(" AND [System.AssignedTo] = @Me"),
        Assignee::Person(person) => {
            let sanitized_person = sanitize_wiql_value(person);
            wiql_query.push_str(&format!(" AND [System.AssignedTo] = '{sanitized_person}'"));
        }
        Assignee::Anyone => {}
    }

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

pub(crate) fn display_empty_work_items_table(heading: &str) {
    display_work_items_list(heading, &[]);
}

pub async fn handle_command(subcommand: &BoardsSubCommands, format: OutputFormat) -> Result<()> {
    let _credentials = get_credentials()?;
    match subcommand {
        BoardsSubCommands::WorkItem { subcommand } => {
            handle_work_item_command(subcommand, format).await
        }
        BoardsSubCommands::Area { subcommand } => area::handle_command(subcommand).await,
        BoardsSubCommands::Iteration { subcommand } => iteration::handle_command(subcommand).await,
        BoardsSubCommands::Query {
            wiql,
            project,
            limit,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            match run_wiql_query(&project_name, wiql, *limit).await {
                Ok(work_items) => print_work_items("Query Results", &work_items, format)?,
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

async fn handle_work_item_command(
    subcommand: &WorkItemSubCommands,
    format: OutputFormat,
) -> Result<()> {
    match subcommand {
        WorkItemSubCommands::Create {
            work_item_type,
            title,
            project,
            parent,
            iteration_path,
            area_path,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            println!("Creating a {work_item_type:?} work item in project: {project_name}");

            match create_work_item(
                &project_name,
                work_item_type,
                title,
                *parent,
                iteration_path.as_deref(),
                area_path.as_deref(),
            )
            .await
            {
                Ok(work_item) => {
                    println!("{}", "✅ Work item created successfully!".green());
                    println!("Created work item with ID: {}", work_item.id);
                    println!("Title: {title}");
                    if let Some(parent) = parent {
                        println!("Parent: #{parent}");
                    }
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
            assigned_to,
            all,
            state,
            work_item_type,
            limit,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;

            match list_work_items(
                &project_name,
                &Assignee::from_args(assigned_to.as_deref(), *all),
                state.as_deref(),
                work_item_type.as_deref(),
                *limit,
                format,
            )
            .await
            {
                Ok(_) => {
                    if format == OutputFormat::Text {
                        println!("{}", "✅ Work items listed successfully!".green());
                    }
                }
                Err(e) => {
                    eprintln!("❌ Failed to list work items: {e}");
                    return Err(e);
                }
            }
        }
        WorkItemSubCommands::Show { id, project, web } => {
            let project_name = get_project_or_default(project.as_deref())?;
            if format == OutputFormat::Text {
                println!("Showing work item with id: {id} in project: {project_name}");
            }

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
                    output::emit(&WorkItemDetails(&work_item), format)?;
                }
                Err(e) => {
                    eprintln!("❌ Failed to retrieve work item: {e}");
                    return Err(e);
                }
            }
        }
        WorkItemSubCommands::Types { project } => {
            let project_name = get_project_or_default(project.as_deref())?;
            match list_work_item_types(&project_name).await {
                Ok(work_item_types) => display_work_item_types(&work_item_types),
                Err(e) => {
                    eprintln!("❌ Failed to list the work item types of '{project_name}'");
                    eprintln!("   {e}");
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
            iteration_path,
            area_path,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            println!("Updating work item with id: {id} in project: {project_name}");

            match update_work_item(
                &project_name,
                id,
                update_work_item_patch(
                    title.as_deref(),
                    description.as_deref(),
                    state.as_deref(),
                    *priority,
                    iteration_path.as_deref(),
                    area_path.as_deref(),
                ),
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
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[clap(subcommand)]
        command: WorkItemSubCommands,
    }

    fn parse(args: &[&str]) -> Result<WorkItemSubCommands, clap::Error> {
        TestCli::try_parse_from(std::iter::once("work-item").chain(args.iter().copied()))
            .map(|cli| cli.command)
    }

    #[test]
    fn create_accepts_iteration_and_area_paths() {
        let command = parse(&[
            "create",
            "task",
            "--title",
            "Write tests",
            "--iteration",
            "Project\\Sprint 1",
            "--area",
            "Project\\Team A",
        ])
        .unwrap();
        let WorkItemSubCommands::Create {
            iteration_path,
            area_path,
            ..
        } = command
        else {
            panic!("expected Create");
        };
        assert_eq!(iteration_path.as_deref(), Some("Project\\Sprint 1"));
        assert_eq!(area_path.as_deref(), Some("Project\\Team A"));
    }

    #[test]
    fn create_patch_sets_iteration_and_area_paths() {
        let patch = create_work_item_patch(
            "Write tests",
            None,
            Some("Project\\Sprint 1"),
            Some("Project\\Team A"),
        );

        assert_eq!(
            patch[1].path.as_deref(),
            Some("/fields/System.IterationPath")
        );
        assert_eq!(patch[1].value, Some(json!("Project\\Sprint 1")));
        assert_eq!(patch[2].path.as_deref(), Some("/fields/System.AreaPath"));
        assert_eq!(patch[2].value, Some(json!("Project\\Team A")));
    }

    #[test]
    fn update_accepts_iteration_and_area_paths() {
        let command = parse(&[
            "update",
            "--id",
            "42",
            "--iteration",
            "Project\\Sprint 1",
            "--area",
            "Project\\Team A",
        ])
        .unwrap();
        let WorkItemSubCommands::Update {
            iteration_path,
            area_path,
            ..
        } = command
        else {
            panic!("expected Update");
        };
        assert_eq!(iteration_path.as_deref(), Some("Project\\Sprint 1"));
        assert_eq!(area_path.as_deref(), Some("Project\\Team A"));
    }

    #[test]
    fn update_patch_sets_iteration_and_area_paths() {
        let patch = update_work_item_patch(
            None,
            None,
            None,
            None,
            Some("Project\\Sprint 1"),
            Some("Project\\Team A"),
        );

        assert_eq!(patch.len(), 2);
        assert_eq!(
            patch[0].path.as_deref(),
            Some("/fields/System.IterationPath")
        );
        assert_eq!(patch[0].value, Some(json!("Project\\Sprint 1")));
        assert_eq!(patch[1].path.as_deref(), Some("/fields/System.AreaPath"));
        assert_eq!(patch[1].value, Some(json!("Project\\Team A")));
    }

    #[test]
    fn create_accepts_a_parent_work_item() {
        let command =
            parse(&["create", "--title", "Write tests", "--parent", "42", "task"]).unwrap();

        let WorkItemSubCommands::Create {
            work_item_type,
            parent,
            ..
        } = command
        else {
            panic!("expected Create");
        };
        assert!(matches!(work_item_type, WorkItemType::Task));
        assert_eq!(parent, Some(42));
    }

    #[test]
    fn create_accepts_the_type_before_its_options() {
        let command = parse(&[
            "create",
            "user-story",
            "--title",
            "Sign up",
            "--parent",
            "42",
        ])
        .unwrap();

        let WorkItemSubCommands::Create {
            work_item_type,
            title,
            parent,
            ..
        } = command
        else {
            panic!("expected Create");
        };
        assert!(matches!(work_item_type, WorkItemType::UserStory));
        assert_eq!(title, "Sign up");
        assert_eq!(parent, Some(42));
    }

    #[test]
    fn create_without_a_parent_has_none() {
        let command = parse(&["create", "--title", "Fix login", "bug"]).unwrap();

        let WorkItemSubCommands::Create { parent, .. } = command else {
            panic!("expected Create");
        };
        assert_eq!(parent, None);
    }

    #[test]
    fn create_rejects_a_non_numeric_parent() {
        assert!(parse(&[
            "create",
            "--title",
            "Write tests",
            "--parent",
            "abc",
            "task"
        ])
        .is_err());
    }

    #[test]
    fn create_patch_sets_the_title() {
        let patch = create_work_item_patch("Write tests", None, None, None);

        assert_eq!(patch.len(), 1);
        assert_eq!(patch[0].op, Some(Op::Add));
        assert_eq!(patch[0].path.as_deref(), Some("/fields/System.Title"));
        assert_eq!(patch[0].value, Some(json!("Write tests")));
    }

    #[test]
    fn create_patch_links_the_new_work_item_to_its_parent() {
        let patch = create_work_item_patch(
            "Write tests",
            Some("https://dev.azure.com/mycompany/_apis/wit/workItems/42"),
            None,
            None,
        );

        assert_eq!(patch.len(), 2);
        assert_eq!(patch[1].op, Some(Op::Add));
        assert_eq!(patch[1].path.as_deref(), Some("/relations/-"));
        assert_eq!(
            patch[1].value,
            Some(json!({
                "rel": "System.LinkTypes.Hierarchy-Reverse",
                "url": "https://dev.azure.com/mycompany/_apis/wit/workItems/42",
            }))
        );
    }

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
        let query = build_wiql_query("test'project", &Assignee::Me, None, None);
        assert!(query.contains("test''project"));
        assert!(!query.contains("test'project' AND"));
    }

    #[test]
    fn test_build_wiql_query_sanitizes_state() {
        let query = build_wiql_query("project", &Assignee::Me, Some("Active'Hack"), None);
        assert!(query.contains("Active''Hack"));
    }

    #[test]
    fn test_build_wiql_query_sanitizes_work_item_type() {
        let query = build_wiql_query("project", &Assignee::Me, None, Some("Bug'Injection"));
        assert!(query.contains("Bug''Injection"));
    }

    #[test]
    fn list_defaults_to_my_work_items() {
        let WorkItemSubCommands::List {
            assigned_to, all, ..
        } = parse(&["list"]).unwrap()
        else {
            panic!("expected List");
        };
        assert_eq!(
            Assignee::from_args(assigned_to.as_deref(), all),
            Assignee::Me
        );
    }

    #[test]
    fn list_accepts_a_person_to_filter_by() {
        let WorkItemSubCommands::List {
            assigned_to, all, ..
        } = parse(&["list", "--assigned-to", "jane@example.com"]).unwrap()
        else {
            panic!("expected List");
        };
        assert_eq!(
            Assignee::from_args(assigned_to.as_deref(), all),
            Assignee::Person("jane@example.com".to_string())
        );
    }

    #[test]
    fn list_treats_at_me_as_myself() {
        assert_eq!(Assignee::from_args(Some("@Me"), false), Assignee::Me);
    }

    #[test]
    fn list_accepts_all() {
        let WorkItemSubCommands::List {
            assigned_to, all, ..
        } = parse(&["list", "--all"]).unwrap()
        else {
            panic!("expected List");
        };
        assert_eq!(
            Assignee::from_args(assigned_to.as_deref(), all),
            Assignee::Anyone
        );
    }

    #[test]
    fn list_rejects_all_together_with_assigned_to() {
        assert!(parse(&["list", "--all", "--assigned-to", "Jane Doe"]).is_err());
    }

    #[test]
    fn wiql_query_for_me_uses_the_at_me_macro() {
        let query = build_wiql_query("project", &Assignee::Me, None, None);
        assert!(query.contains("[System.AssignedTo] = @Me"));
    }

    #[test]
    fn wiql_query_for_a_person_filters_by_their_name() {
        let query = build_wiql_query(
            "project",
            &Assignee::Person("O'Brien".to_string()),
            None,
            None,
        );
        assert!(query.contains("[System.AssignedTo] = 'O''Brien'"));
    }

    #[test]
    fn wiql_query_for_anyone_has_no_assignee_filter() {
        let query = build_wiql_query("project", &Assignee::Anyone, Some("Active"), None);
        assert!(!query.contains("[System.AssignedTo] ="));
        assert!(query.contains("[System.State] = 'Active'"));
    }
}
