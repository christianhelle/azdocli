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
use clap::{Args, Subcommand};
use colored::Colorize;
use serde_json::json;
use std::path::{Path, PathBuf};

mod area;
mod attachment;
mod classification;
mod iteration;
mod views;

use views::{WorkItemDetails, WorkItemList};

#[derive(Subcommand, Clone)]
pub enum BoardsSubCommands {
    /// Manage work items
    WorkItem {
        #[clap(subcommand)]
        subcommand: Box<WorkItemSubCommands>,
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

/// Short names for the work item types of the built-in processes, as
/// (alias, Azure DevOps name).
const WORK_ITEM_TYPE_ALIASES: &[(&str, &str)] = &[
    ("bug", "Bug"),
    ("task", "Task"),
    ("user-story", "User Story"),
    ("feature", "Feature"),
    ("epic", "Epic"),
    ("pbi", "Product Backlog Item"),
    ("product-backlog-item", "Product Backlog Item"),
    ("issue", "Issue"),
    ("impediment", "Impediment"),
    ("requirement", "Requirement"),
    ("test-case", "Test Case"),
];

/// Maps a short alias such as `pbi` to its Azure DevOps type name. Any other
/// name, e.g. one from `work-item types`, is used as given.
fn resolve_work_item_type(name: &str) -> String {
    WORK_ITEM_TYPE_ALIASES
        .iter()
        .find(|(alias, _)| alias.eq_ignore_ascii_case(name))
        .map(|(_, type_name)| type_name.to_string())
        .unwrap_or_else(|| name.to_string())
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
        /// Work item type: bug, task, user-story, feature, epic, pbi, issue,
        /// impediment, requirement, test-case, or any type name listed by
        /// `boards work-item types` (e.g. "Product Backlog Item")
        work_item_type: String,
        /// Work item title
        #[clap(short, long)]
        title: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// ID of the parent work item, e.g. the product backlog item a task belongs to
        #[clap(long, value_parser = clap::value_parser!(i32).range(1..))]
        parent: Option<i32>,
        /// Iteration path for the work item, or @current for the team's current sprint
        #[clap(long = "iteration")]
        iteration_path: Option<String>,
        /// Area path for the work item
        #[clap(long = "area")]
        area_path: Option<String>,
        #[clap(flatten)]
        content: WorkItemContentArgs,
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
    /// List and download the files attached to a work item
    Attachment {
        #[clap(subcommand)]
        subcommand: attachment::AttachmentSubCommands,
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
        /// New state for the work item (e.g., 'New', 'Active', 'Resolved', 'Closed')
        #[clap(long)]
        state: Option<String>,
        /// New iteration path for the work item, or @current for the team's current sprint
        #[clap(long = "iteration")]
        iteration_path: Option<String>,
        /// New area path for the work item
        #[clap(long = "area")]
        area_path: Option<String>,
        #[clap(flatten)]
        content: WorkItemContentArgs,
    },
}

/// Content options shared by `work-item create` and `work-item update`.
#[derive(Args, Clone, Debug, Default)]
pub struct WorkItemContentArgs {
    /// Priority (e.g., 1, 2, 3, 4)
    #[clap(long)]
    priority: Option<i32>,
    /// User to assign the work item to, e.g. ada@example.com; "" unassigns
    #[clap(long, value_name = "USER")]
    assigned_to: Option<String>,
    /// Team whose current sprint `--iteration @current` means (default: the project's default team)
    #[clap(long)]
    team: Option<String>,
    /// Size of the item: sets Effort (Scrum), Story Points (Agile) or Size (CMMI),
    /// whichever the work item type has
    #[clap(long, value_name = "POINTS", value_parser = parse_non_negative, allow_hyphen_values = true)]
    effort: Option<f64>,
    /// Hours of work left on a task
    #[clap(long, value_name = "HOURS", value_parser = parse_non_negative, allow_hyphen_values = true)]
    remaining_work: Option<f64>,
    /// Description (HTML or plain text)
    #[clap(long, conflicts_with = "description_file")]
    description: Option<String>,
    /// Read the description from a file
    #[clap(long, value_name = "PATH")]
    description_file: Option<PathBuf>,
    /// Acceptance criteria (HTML or plain text)
    #[clap(long, conflicts_with = "acceptance_criteria_file")]
    acceptance_criteria: Option<String>,
    /// Read the acceptance criteria from a file
    #[clap(long, value_name = "PATH")]
    acceptance_criteria_file: Option<PathBuf>,
    /// Tag to add; repeat the flag or separate tags with commas. Update keeps existing tags
    #[clap(long = "tag", value_name = "TAG", value_delimiter = ',')]
    tags: Vec<String>,
    /// Any field by reference name, e.g. Microsoft.VSTS.Scheduling.Effort=5; repeatable
    #[clap(long = "field", value_name = "REFERENCE_NAME=VALUE", value_parser = parse_field_assignment)]
    fields: Vec<(String, String)>,
    /// File to upload and attach; repeatable
    #[clap(long = "attachment", value_name = "PATH")]
    attachments: Vec<PathBuf>,
}

/// A finite, non-negative number. JSON has no NaN or infinity, so those would
/// be sent as null and clear the field instead of setting it.
fn parse_non_negative(input: &str) -> Result<f64, String> {
    match input.trim().parse::<f64>() {
        Ok(value) if value.is_finite() && value >= 0.0 => Ok(value),
        _ => Err(format!("expected a non-negative number, got '{input}'")),
    }
}

/// Splits `--field` input at its first `=` into reference name and value.
fn parse_field_assignment(input: &str) -> Result<(String, String), String> {
    match input.split_once('=') {
        Some((name, value)) if !name.trim().is_empty() => {
            Ok((name.trim().to_string(), value.to_string()))
        }
        _ => Err(format!(
            "expected REFERENCE_NAME=VALUE, e.g. Microsoft.VSTS.Scheduling.Effort=5, got '{input}'"
        )),
    }
}

impl WorkItemContentArgs {
    fn description(&self) -> Result<Option<String>> {
        text_or_file(
            self.description.as_deref(),
            self.description_file.as_deref(),
        )
    }

    fn acceptance_criteria(&self) -> Result<Option<String>> {
        text_or_file(
            self.acceptance_criteria.as_deref(),
            self.acceptance_criteria_file.as_deref(),
        )
    }
}

/// The text given inline, or else the contents of the file given.
fn text_or_file(text: Option<&str>, file: Option<&Path>) -> Result<Option<String>> {
    match (text, file) {
        (Some(text), _) => Ok(Some(text.to_string())),
        (None, Some(file)) => std::fs::read_to_string(file)
            .map(Some)
            .map_err(|e| anyhow!("Failed to read '{}': {e}", file.display())),
        (None, None) => Ok(None),
    }
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

/// The fields a create or update sets. Unset fields are left alone.
#[derive(Default, Debug)]
struct WorkItemFields {
    title: Option<String>,
    description: Option<String>,
    acceptance_criteria: Option<String>,
    state: Option<String>,
    assigned_to: Option<String>,
    priority: Option<i32>,
    remaining_work: Option<f64>,
    /// The work item type's effort field and the value for it.
    effort: Option<(&'static str, f64)>,
    iteration_path: Option<String>,
    area_path: Option<String>,
    /// `System.Tags`, already joined with "; ".
    tags: Option<String>,
    /// Further fields as (reference name, value), set after the named ones.
    extra: Vec<(String, String)>,
}

/// Joins `existing` tags (as Azure DevOps stores them, "a; b") with `added`,
/// dropping blanks and case-insensitive duplicates. `None` when there are no
/// tags to add.
fn merge_tags(existing: Option<&str>, added: &[String]) -> Option<String> {
    if added.iter().all(|tag| tag.trim().is_empty()) {
        return None;
    }
    let mut tags: Vec<&str> = Vec::new();
    for tag in existing
        .unwrap_or("")
        .split(';')
        .chain(added.iter().map(String::as_str))
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
    {
        if !tags.iter().any(|kept| kept.eq_ignore_ascii_case(tag)) {
            tags.push(tag);
        }
    }
    Some(tags.join("; "))
}

impl WorkItemFields {
    /// The patch operations that set each field given.
    fn patch(&self) -> Vec<JsonPatchOperation> {
        let text = |value: &Option<String>| value.as_deref().map(|value| json!(value));
        [
            ("System.Title", text(&self.title)),
            ("System.Description", text(&self.description)),
            (
                "Microsoft.VSTS.Common.AcceptanceCriteria",
                text(&self.acceptance_criteria),
            ),
            ("System.State", text(&self.state)),
            ("System.AssignedTo", text(&self.assigned_to)),
            (
                "Microsoft.VSTS.Common.Priority",
                self.priority.map(|priority| json!(priority)),
            ),
            (
                "Microsoft.VSTS.Scheduling.RemainingWork",
                self.remaining_work.map(|hours| json!(hours)),
            ),
        ]
        .into_iter()
        .chain(
            self.effort
                .map(|(field, points)| (field, Some(json!(points)))),
        )
        .chain([
            ("System.IterationPath", text(&self.iteration_path)),
            ("System.AreaPath", text(&self.area_path)),
            ("System.Tags", text(&self.tags)),
        ])
        .filter_map(|(field, value)| value.map(|value| set_field_op(field, value)))
        .chain(
            self.extra
                .iter()
                .map(|(field, value)| set_field_op(field, json!(value))),
        )
        .collect()
    }
}

/// The fields that size a backlog item, one per process: Scrum, Agile, CMMI.
const EFFORT_FIELDS: [&str; 3] = [
    "Microsoft.VSTS.Scheduling.Effort",
    "Microsoft.VSTS.Scheduling.StoryPoints",
    "Microsoft.VSTS.Scheduling.Size",
];

/// The effort field among a work item type's `fields`, if it has one.
fn effort_field(fields: &[&str]) -> Option<&'static str> {
    EFFORT_FIELDS
        .into_iter()
        .find(|effort| fields.contains(effort))
}

/// Pairs `effort` with the field that holds it for `work_item_type`.
async fn effort_assignment(
    project: &str,
    work_item_type: &str,
    effort: Option<f64>,
) -> Result<Option<(&'static str, f64)>> {
    let Some(effort) = effort else {
        return Ok(None);
    };
    let creds = get_credentials()?;
    let fields = create_wit_client()?
        .work_item_types_field_client()
        .list(creds.organization, project, work_item_type)
        .await?
        .value;
    let names: Vec<&str> = fields
        .iter()
        .filter_map(|field| {
            field
                .work_item_type_field_instance_base
                .work_item_field_reference
                .reference_name
                .as_deref()
        })
        .collect();
    effort_field(&names)
        .map(|field| Some((field, effort)))
        .ok_or_else(|| {
            anyhow!(
                "Work item type '{work_item_type}' has no Effort, Story Points or Size field; \
                 set one with --field"
            )
        })
}

/// Fails the patch unless the work item is still at revision `rev`.
fn revision_test_op(rev: i32) -> JsonPatchOperation {
    JsonPatchOperation {
        from: None,
        op: Some(Op::Test),
        path: Some("/rev".to_owned()),
        value: Some(json!(rev)),
    }
}

fn set_field_op(field: &str, value: serde_json::Value) -> JsonPatchOperation {
    JsonPatchOperation {
        from: None,
        op: Some(Op::Add),
        path: Some(format!("/fields/{field}")),
        value: Some(value),
    }
}

/// Adds a relation of type `rel` to `url`, with optional link attributes.
fn add_relation_op(
    rel: &str,
    url: &str,
    attributes: Option<serde_json::Value>,
) -> JsonPatchOperation {
    let mut relation = json!({ "rel": rel, "url": url });
    if let Some(attributes) = attributes {
        relation["attributes"] = attributes;
    }
    JsonPatchOperation {
        from: None,
        op: Some(Op::Add),
        path: Some("/relations/-".to_owned()),
        value: Some(relation),
    }
}

/// Builds the patch that creates a work item, optionally as a child of the
/// work item at `parent_url`.
fn create_work_item_patch(
    fields: &WorkItemFields,
    parent_url: Option<&str>,
) -> Vec<JsonPatchOperation> {
    let mut patch = fields.patch();
    if let Some(parent_url) = parent_url {
        patch.push(add_relation_op(
            "System.LinkTypes.Hierarchy-Reverse",
            parent_url,
            None,
        ));
    }
    patch
}

async fn create_work_item(
    project: &str,
    work_item_type: &str,
    patch: Vec<JsonPatchOperation>,
) -> Result<models::WorkItem> {
    let creds = get_credentials()?;
    Ok(create_wit_client()?
        .work_items_client()
        .create(
            creds.organization,
            patch,
            project.to_string(),
            work_item_type,
        )
        .await?)
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
                        if [
                            "Task",
                            "User Story",
                            "Product Backlog Item",
                            "Feature",
                            "Epic",
                        ]
                        .contains(&wt)
                        {
                            "Removed"
                        } else {
                            "Closed"
                        }
                    })
                    .unwrap_or("Closed");
                update_work_item(
                    project,
                    id,
                    WorkItemFields {
                        state: Some(state.to_string()),
                        ..Default::default()
                    }
                    .patch(),
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
            content,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            let work_item_type = resolve_work_item_type(work_item_type);
            if format == OutputFormat::Text {
                println!("Creating a {work_item_type} work item in project: {project_name}");
            }

            let creds = get_credentials()?;
            let parent_url =
                parent.map(|id| api_work_item_url(&creds.base_url, &creds.organization, id));
            let iteration_path = iteration::resolve_iteration_path(
                &project_name,
                content.team.as_deref(),
                iteration_path.as_deref(),
            )
            .await?;
            let effort = effort_assignment(&project_name, &work_item_type, content.effort).await?;
            let fields = WorkItemFields {
                title: Some(title.clone()),
                effort,
                description: content.description()?,
                acceptance_criteria: content.acceptance_criteria()?,
                tags: merge_tags(None, &content.tags),
                assigned_to: content.assigned_to.clone(),
                priority: content.priority,
                remaining_work: content.remaining_work,
                extra: content.fields.clone(),
                iteration_path,
                area_path: area_path.clone(),
                ..Default::default()
            };

            let mut patch = create_work_item_patch(&fields, parent_url.as_deref());
            patch
                .extend(attachment::upload_all(&project_name, &content.attachments, format).await?);

            match create_work_item(&project_name, &work_item_type, patch).await {
                Ok(work_item) if format != OutputFormat::Text => {
                    output::emit(&WorkItemDetails(&work_item), format)?;
                }
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
        WorkItemSubCommands::Attachment { subcommand } => {
            return attachment::handle_command(subcommand, format).await;
        }
        WorkItemSubCommands::Update {
            id,
            project,
            title,
            state,
            iteration_path,
            area_path,
            content,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            if format == OutputFormat::Text {
                println!("Updating work item with id: {id} in project: {project_name}");
            }

            // Setting System.Tags replaces them, so new tags join the existing
            // ones. The revision they were read at guards the update, so tags
            // another client adds in between fail the update instead of being lost.
            // --effort needs the item's type to pick its effort field.
            let current = if content.tags.is_empty() && content.effort.is_none() {
                None
            } else {
                Some(get_work_item(&project_name, id).await?)
            };
            let field = |name| {
                current
                    .as_ref()
                    .and_then(|item| item.fields.get(name))
                    .and_then(|v| v.as_str())
            };
            let (tags, read_at) = if content.tags.is_empty() {
                (None, None)
            } else {
                let tags = merge_tags(field("System.Tags"), &content.tags);
                (tags, current.as_ref().and_then(|item| item.rev))
            };
            let effort = match field("System.WorkItemType") {
                Some(work_item_type) => {
                    effort_assignment(&project_name, work_item_type, content.effort).await?
                }
                None => None,
            };

            let iteration_path = iteration::resolve_iteration_path(
                &project_name,
                content.team.as_deref(),
                iteration_path.as_deref(),
            )
            .await?;
            let mut patch: Vec<_> = read_at.map(revision_test_op).into_iter().collect();
            patch.extend(
                WorkItemFields {
                    title: title.clone(),
                    description: content.description()?,
                    acceptance_criteria: content.acceptance_criteria()?,
                    tags,
                    state: state.clone(),
                    effort,
                    assigned_to: content.assigned_to.clone(),
                    priority: content.priority,
                    remaining_work: content.remaining_work,
                    extra: content.fields.clone(),
                    iteration_path,
                    area_path: area_path.clone(),
                }
                .patch(),
            );
            patch
                .extend(attachment::upload_all(&project_name, &content.attachments, format).await?);

            match update_work_item(&project_name, id, patch).await {
                Ok(work_item) if format != OutputFormat::Text => {
                    output::emit(&WorkItemDetails(&work_item), format)?;
                }
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

    fn titled(title: &str) -> WorkItemFields {
        WorkItemFields {
            title: Some(title.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn create_accepts_a_description() {
        let WorkItemSubCommands::Create { content, .. } = parse(&[
            "create",
            "bug",
            "--title",
            "Crash",
            "--description",
            "It breaks",
        ])
        .unwrap() else {
            panic!("expected Create");
        };
        assert_eq!(content.description().unwrap().as_deref(), Some("It breaks"));
    }

    #[test]
    fn description_file_is_read() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), "# Spec\nDetails").unwrap();
        let path = file.path().to_str().unwrap();
        let WorkItemSubCommands::Update { content, .. } =
            parse(&["update", "--id", "1", "--description-file", path]).unwrap()
        else {
            panic!("expected Update");
        };
        assert_eq!(
            content.description().unwrap().as_deref(),
            Some("# Spec\nDetails")
        );
    }

    #[test]
    fn description_and_description_file_conflict() {
        assert!(parse(&[
            "update",
            "--id",
            "1",
            "--description",
            "x",
            "--description-file",
            "y.md"
        ])
        .is_err());
    }

    #[test]
    fn acceptance_criteria_sets_its_field() {
        let WorkItemSubCommands::Create { content, .. } = parse(&[
            "create",
            "pbi",
            "--title",
            "Sign up",
            "--acceptance-criteria",
            "Given a user",
        ])
        .unwrap() else {
            panic!("expected Create");
        };
        let patch = WorkItemFields {
            acceptance_criteria: content.acceptance_criteria().unwrap(),
            ..Default::default()
        }
        .patch();
        assert_eq!(
            patch[0].path.as_deref(),
            Some("/fields/Microsoft.VSTS.Common.AcceptanceCriteria")
        );
        assert_eq!(patch[0].value, Some(json!("Given a user")));
    }

    #[test]
    fn acceptance_criteria_and_its_file_conflict() {
        assert!(parse(&[
            "update",
            "--id",
            "1",
            "--acceptance-criteria",
            "x",
            "--acceptance-criteria-file",
            "y.md"
        ])
        .is_err());
    }

    #[test]
    fn assigned_to_sets_its_field() {
        let WorkItemSubCommands::Update { content, .. } =
            parse(&["update", "--id", "1", "--assigned-to", "ada@example.com"]).unwrap()
        else {
            panic!("expected Update");
        };
        let patch = WorkItemFields {
            assigned_to: content.assigned_to.clone(),
            ..Default::default()
        }
        .patch();
        assert_eq!(patch[0].path.as_deref(), Some("/fields/System.AssignedTo"));
        assert_eq!(patch[0].value, Some(json!("ada@example.com")));
    }

    #[test]
    fn effort_field_follows_the_process() {
        let scrum = ["System.Title", "Microsoft.VSTS.Scheduling.Effort"];
        let agile = ["System.Title", "Microsoft.VSTS.Scheduling.StoryPoints"];
        let cmmi = ["Microsoft.VSTS.Scheduling.Size"];
        assert_eq!(
            effort_field(&scrum),
            Some("Microsoft.VSTS.Scheduling.Effort")
        );
        assert_eq!(
            effort_field(&agile),
            Some("Microsoft.VSTS.Scheduling.StoryPoints")
        );
        assert_eq!(effort_field(&cmmi), Some("Microsoft.VSTS.Scheduling.Size"));
        assert_eq!(effort_field(&["System.Title"]), None);
    }

    #[test]
    fn effort_parses_as_a_number() {
        let WorkItemSubCommands::Create { content, .. } =
            parse(&["create", "pbi", "--title", "Sign up", "--effort", "5"]).unwrap()
        else {
            panic!("expected Create");
        };
        assert_eq!(content.effort, Some(5.0));
        assert!(parse(&["create", "pbi", "--title", "x", "--effort", "big"]).is_err());
    }

    #[test]
    fn numeric_flags_reject_non_finite_and_negative_values() {
        for flag in ["--effort", "--remaining-work"] {
            for bad in ["NaN", "inf", "1e999", "-1"] {
                assert!(
                    parse(&["update", "--id", "1", flag, bad]).is_err(),
                    "{flag} {bad} should be rejected"
                );
            }
            assert!(parse(&["update", "--id", "1", flag, "0"]).is_ok());
        }
    }

    #[test]
    fn remaining_work_sets_its_field() {
        let WorkItemSubCommands::Update { content, .. } =
            parse(&["update", "--id", "1", "--remaining-work", "2.5"]).unwrap()
        else {
            panic!("expected Update");
        };
        let patch = WorkItemFields {
            remaining_work: content.remaining_work,
            ..Default::default()
        }
        .patch();
        assert_eq!(
            patch[0].path.as_deref(),
            Some("/fields/Microsoft.VSTS.Scheduling.RemainingWork")
        );
        assert_eq!(patch[0].value, Some(json!(2.5)));
    }

    #[test]
    fn team_picks_whose_current_iteration_to_use() {
        let WorkItemSubCommands::Create {
            iteration_path,
            content,
            ..
        } = parse(&[
            "create",
            "pbi",
            "--title",
            "Sign up",
            "--iteration",
            "@current",
            "--team",
            "Web",
        ])
        .unwrap()
        else {
            panic!("expected Create");
        };
        assert_eq!(iteration_path.as_deref(), Some("@current"));
        assert_eq!(content.team.as_deref(), Some("Web"));
    }

    #[test]
    fn tags_are_repeatable_and_comma_separated() {
        let WorkItemSubCommands::Create { content, .. } = parse(&[
            "create", "pbi", "--title", "Sign up", "--tag", "backend", "--tag", "api,auth",
        ])
        .unwrap() else {
            panic!("expected Create");
        };
        assert_eq!(content.tags, ["backend", "api", "auth"]);
    }

    #[test]
    fn merge_tags_without_existing_joins_the_new_ones() {
        assert_eq!(
            merge_tags(None, &["a".into(), "b".into()]).as_deref(),
            Some("a; b")
        );
    }

    #[test]
    fn merge_tags_keeps_existing_and_drops_duplicates() {
        assert_eq!(
            merge_tags(Some("api; Backend"), &["backend".into(), "auth".into()]).as_deref(),
            Some("api; Backend; auth")
        );
    }

    #[test]
    fn merge_tags_with_nothing_to_add_is_none() {
        assert_eq!(merge_tags(Some("api"), &[]), None);
        assert_eq!(merge_tags(None, &[" ".into()]), None);
    }

    #[test]
    fn revision_test_op_checks_the_revision() {
        let op = revision_test_op(7);
        assert_eq!(op.op, Some(Op::Test));
        assert_eq!(op.path.as_deref(), Some("/rev"));
        assert_eq!(op.value, Some(json!(7)));
    }

    #[test]
    fn tags_set_the_tags_field() {
        let patch = WorkItemFields {
            tags: Some("a; b".into()),
            ..Default::default()
        }
        .patch();
        assert_eq!(patch[0].path.as_deref(), Some("/fields/System.Tags"));
        assert_eq!(patch[0].value, Some(json!("a; b")));
    }

    #[test]
    fn create_accepts_a_priority() {
        let WorkItemSubCommands::Create { content, .. } =
            parse(&["create", "bug", "--title", "Crash", "--priority", "1"]).unwrap()
        else {
            panic!("expected Create");
        };
        assert_eq!(content.priority, Some(1));
    }

    #[test]
    fn fields_are_parsed_at_the_first_equals_sign() {
        let WorkItemSubCommands::Update { content, .. } = parse(&[
            "update",
            "--id",
            "1",
            "--field",
            "Microsoft.VSTS.Scheduling.Effort=5",
            "--field",
            "Custom.Formula=a=b",
        ])
        .unwrap() else {
            panic!("expected Update");
        };
        assert_eq!(
            content.fields,
            [
                ("Microsoft.VSTS.Scheduling.Effort".into(), "5".into()),
                ("Custom.Formula".into(), "a=b".into())
            ]
        );
    }

    #[test]
    fn fields_without_a_name_are_rejected() {
        assert!(parse_field_assignment("NoEquals").is_err());
        assert!(parse_field_assignment("=5").is_err());
    }

    #[test]
    fn fields_follow_the_named_fields_in_the_patch() {
        let patch = WorkItemFields {
            title: Some("T".into()),
            extra: vec![("Custom.Thing".into(), "x".into())],
            ..Default::default()
        }
        .patch();
        assert_eq!(patch[1].path.as_deref(), Some("/fields/Custom.Thing"));
        assert_eq!(patch[1].value, Some(json!("x")));
    }

    #[test]
    fn attachments_are_repeatable() {
        let WorkItemSubCommands::Create { content, .. } = parse(&[
            "create",
            "pbi",
            "--title",
            "Sign up",
            "--attachment",
            "spec.md",
            "--attachment",
            "diagram.png",
        ])
        .unwrap() else {
            panic!("expected Create");
        };
        assert_eq!(
            content.attachments,
            [PathBuf::from("spec.md"), PathBuf::from("diagram.png")]
        );
    }

    #[test]
    fn missing_description_file_is_an_error() {
        assert!(text_or_file(None, Some(Path::new("does-not-exist.md"))).is_err());
    }

    #[test]
    fn work_item_type_aliases_resolve_to_azure_devops_names() {
        assert_eq!(resolve_work_item_type("bug"), "Bug");
        assert_eq!(resolve_work_item_type("user-story"), "User Story");
        assert_eq!(resolve_work_item_type("pbi"), "Product Backlog Item");
        assert_eq!(
            resolve_work_item_type("product-backlog-item"),
            "Product Backlog Item"
        );
        assert_eq!(resolve_work_item_type("Impediment"), "Impediment");
        assert_eq!(resolve_work_item_type("test-case"), "Test Case");
    }

    #[test]
    fn unknown_work_item_types_pass_through_verbatim() {
        assert_eq!(
            resolve_work_item_type("Product Backlog Item"),
            "Product Backlog Item"
        );
        assert_eq!(resolve_work_item_type("Custom Thing"), "Custom Thing");
    }

    #[test]
    fn create_accepts_any_work_item_type_name() {
        let WorkItemSubCommands::Create { work_item_type, .. } =
            parse(&["create", "Product Backlog Item", "--title", "Sign up"]).unwrap()
        else {
            panic!("expected Create");
        };
        assert_eq!(work_item_type, "Product Backlog Item");
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
            &WorkItemFields {
                title: Some("Write tests".into()),
                iteration_path: Some("Project\\Sprint 1".into()),
                area_path: Some("Project\\Team A".into()),
                ..Default::default()
            },
            None,
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
        let patch = WorkItemFields {
            iteration_path: Some("Project\\Sprint 1".into()),
            area_path: Some("Project\\Team A".into()),
            ..Default::default()
        }
        .patch();

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
        assert_eq!(work_item_type, "task");
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
        assert_eq!(work_item_type, "user-story");
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
        let patch = create_work_item_patch(&titled("Write tests"), None);

        assert_eq!(patch.len(), 1);
        assert_eq!(patch[0].op, Some(Op::Add));
        assert_eq!(patch[0].path.as_deref(), Some("/fields/System.Title"));
        assert_eq!(patch[0].value, Some(json!("Write tests")));
    }

    #[test]
    fn create_patch_links_the_new_work_item_to_its_parent() {
        let patch = create_work_item_patch(
            &titled("Write tests"),
            Some("https://dev.azure.com/mycompany/_apis/wit/workItems/42"),
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
