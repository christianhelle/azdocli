use crate::auth::factory::{ClientFactory, CredentialClientFactory};
use crate::auth::get_credentials;
use crate::auth::url::{api_work_item_url, web_work_item_url};
use crate::connection::Connection;
use crate::output::{self, OutputFormat};
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
mod backlog;
mod capacity;
mod classification;
mod history;
mod iteration;
mod link;
mod queries;
mod sprint;
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
    /// See how a team's current sprint is going
    Sprint {
        #[clap(subcommand)]
        subcommand: sprint::SprintSubCommands,
    },
    /// List a team's product backlog in backlog order
    Backlog {
        #[clap(subcommand)]
        subcommand: backlog::BacklogSubCommands,
    },
    /// Run a WIQL query and list the work items it returns
    #[clap(group(clap::ArgGroup::new("source").required(true).args(["wiql", "saved"])))]
    Query {
        /// The WIQL query to run
        #[clap(short, long)]
        wiql: Option<String>,
        /// A saved query to run, by path (e.g. "Shared Queries/Active Bugs") or id
        #[clap(long)]
        saved: Option<String>,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Maximum number of work items to return (default: 50)
        #[clap(long, default_value = "50", value_parser = clap::value_parser!(i32).range(1..))]
        limit: i32,
    },
    /// List the project's saved queries
    Queries {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Levels of folders to list below the root folders (1 or 2)
        #[clap(long, default_value = "2", value_parser = clap::value_parser!(i32).range(1..=2))]
        depth: i32,
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
        /// Child task to create under the new work item, in the same sprint and
        /// area; repeat for more tasks
        #[clap(long = "task", value_name = "TITLE")]
        tasks: Vec<String>,
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
    /// List the states a work item type moves through, e.g. New, Approved,
    /// Committed and Done for a Scrum product backlog item
    States {
        /// Work item type: a short name such as pbi or bug, or any type name
        work_item_type: String,
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
    /// List, add and remove links to other work items, such as a parent,
    /// children, related items and dependencies
    Link {
        #[clap(subcommand)]
        subcommand: link::LinkSubCommands,
    },
    /// Show who changed a work item's fields, and when
    History {
        /// ID of the work item
        #[clap(short, long)]
        id: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Only show changes to this field (reference or short name); repeatable
        #[clap(long = "field")]
        fields: Vec<String>,
        /// Show only the newest N updates
        #[clap(long, value_parser = clap::value_parser!(usize))]
        top: Option<usize>,
    },
    /// Move a work item up or down the team's product backlog
    #[clap(group(
        clap::ArgGroup::new("placement")
            .required(true)
            .multiple(false)
            .args(["before", "after", "top"])
    ))]
    Move {
        /// ID of the work item to move
        #[clap(short, long, value_parser = clap::value_parser!(i32).range(1..))]
        id: i32,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Team whose backlog to reorder (default: the project's default team)
        #[clap(long)]
        team: Option<String>,
        /// Put the item directly above this work item
        #[clap(long, value_name = "ID")]
        before: Option<i32>,
        /// Put the item directly below this work item
        #[clap(long, value_name = "ID")]
        after: Option<i32>,
        /// Put the item at the top of the backlog
        #[clap(long)]
        top: bool,
    },
    /// Update one or more work items
    Update {
        /// ID of the work item to update; repeat the flag or separate IDs with
        /// commas to make the same change to several work items
        #[clap(
            short = 'i',
            long = "id",
            value_name = "ID",
            value_delimiter = ',',
            required = true
        )]
        ids: Vec<i32>,
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
        /// Tag to remove; repeat the flag or separate tags with commas
        #[clap(long = "remove-tag", value_name = "TAG", value_delimiter = ',')]
        remove_tags: Vec<String>,
        /// ID of the new parent work item; replaces the current parent, e.g.
        /// to move a backlog item to another feature
        #[clap(long)]
        parent: Option<i32>,
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
    /// User to assign the work item to, e.g. ada@example.com or @me; "" unassigns
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
    /// Hours a task was first estimated to take
    #[clap(long, value_name = "HOURS", value_parser = parse_non_negative, allow_hyphen_values = true)]
    original_estimate: Option<f64>,
    /// Hours of work done on a task
    #[clap(long, value_name = "HOURS", value_parser = parse_non_negative, allow_hyphen_values = true)]
    completed_work: Option<f64>,
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

/// `--assigned-to` value meaning the signed-in user.
fn is_me(value: &str) -> bool {
    value.trim().eq_ignore_ascii_case("@me")
}

/// The account name (usually the email) of the user `connectionData` describes.
fn signed_in_account(connection_data: &serde_json::Value) -> Option<String> {
    connection_data
        .pointer("/authenticatedUser/properties/Account/$value")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

/// `assignee`, with `@me` swapped for the signed-in user's account name.
async fn resolve_assignee(conn: &Connection, assignee: Option<&str>) -> Result<Option<String>> {
    match assignee {
        Some(value) if is_me(value) => {
            let data = crate::auth::connection_data(conn.credentials()).await?;
            signed_in_account(&data)
                .map(Some)
                .ok_or_else(|| anyhow!("Could not tell which account the PAT belongs to"))
        }
        other => Ok(other.map(str::to_string)),
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

async fn list_work_item_types(
    conn: &Connection,
    project: &str,
) -> Result<Vec<models::WorkItemType>> {
    let client = conn.wit();

    Ok(client
        .work_item_types_client()
        .list(conn.organization(), project)
        .await?
        .value)
}

async fn list_work_item_states(
    conn: &Connection,
    project: &str,
    work_item_type: &str,
) -> Result<Vec<models::WorkItemStateColor>> {
    Ok(conn
        .wit()
        .work_item_type_states_client()
        .list(conn.organization(), project, work_item_type)
        .await?
        .value)
}

/// `states`, or an error naming the type when there are none: the API answers
/// an unknown type with an empty list, and every real type has states.
fn require_states(
    states: Vec<models::WorkItemStateColor>,
    work_item_type: &str,
) -> Result<Vec<models::WorkItemStateColor>> {
    if states.is_empty() {
        return Err(anyhow!(
            "Work item type '{work_item_type}' was not found; 'boards work-item types' lists them"
        ));
    }
    Ok(states)
}

/// The states as a name and category table, in workflow order.
fn states_table(states: &[models::WorkItemStateColor]) -> String {
    let mut table = format!(
        "{:<20} {}\n{}\n",
        "Name".bold(),
        "Category".bold(),
        "-".repeat(40)
    );
    for state in states {
        table += &format!(
            "{:<20} {}\n",
            escape_control_characters(state.name.as_deref().unwrap_or("-")),
            escape_control_characters(state.category.as_deref().unwrap_or("")),
        );
    }
    table + &format!("\n{} state(s)\n", states.len())
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
    conn: &Connection,
    project: &str,
    id: &str,
    top: Option<i32>,
) -> Result<Vec<models::Comment>> {
    let client = conn.wit();

    let mut request = client.comments_client().get_comments(
        conn.organization(),
        project,
        parse_work_item_id(id)?,
    );

    if let Some(top) = top {
        request = request.top(top);
    }

    Ok(request.await?.comments)
}

async fn add_work_item_comment(
    conn: &Connection,
    project: &str,
    id: &str,
    message: &str,
) -> Result<models::Comment> {
    let client = conn.wit();

    Ok(client
        .comments_client()
        .add_comment(
            conn.organization(),
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

pub(super) fn create_wit_client() -> Result<wit::Client> {
    let creds = get_credentials()?;
    let factory = CredentialClientFactory::new(&creds)?;
    Ok(factory.build_wit())
}

async fn get_work_item(conn: &Connection, project: &str, id: &str) -> Result<models::WorkItem> {
    let id_int = parse_work_item_id(id)?;

    let client = conn.wit();
    let work_item = client
        .work_items_client()
        .get_work_item(conn.organization(), id_int, project)
        .await?;

    Ok(work_item)
}

/// The fields a create or update sets. Unset fields are left alone.
#[derive(Clone, Default, Debug)]
struct WorkItemFields {
    title: Option<String>,
    description: Option<String>,
    acceptance_criteria: Option<String>,
    state: Option<String>,
    assigned_to: Option<String>,
    priority: Option<i32>,
    remaining_work: Option<f64>,
    original_estimate: Option<f64>,
    completed_work: Option<f64>,
    /// The work item type's effort field and the value for it.
    effort: Option<(&'static str, f64)>,
    iteration_path: Option<String>,
    area_path: Option<String>,
    /// `System.Tags`, already joined with "; ".
    tags: Option<String>,
    /// Whether `tags` replaces the item's tags. Azure DevOps merges tags set
    /// with `add`, so dropping one takes `replace`, or `remove` to clear them.
    replace_tags: bool,
    /// Further fields as (reference name, value), set after the named ones.
    extra: Vec<(String, String)>,
}

/// Joins `existing` tags (as Azure DevOps stores them, "a; b") with `added`,
/// dropping blanks and case-insensitive duplicates. `None` when there are no
/// tags to add.
fn merge_tags(existing: Option<&str>, added: &[String]) -> Option<String> {
    edit_tags(existing, added, &[])
}

/// Whether two tags are the same, ignoring case in any script. Like .NET's
/// OrdinalIgnoreCase, which Azure DevOps uses, each character is uppercased on
/// its own: σ, ς and Σ match, while ß (whose uppercase is two letters) stays ß.
fn same_tag(a: &str, b: &str) -> bool {
    fn upper(c: char) -> char {
        let mut upper = c.to_uppercase();
        match (upper.next(), upper.next()) {
            (Some(single), None) => single,
            _ => c,
        }
    }
    a.chars().map(upper).eq(b.chars().map(upper))
}

/// `existing` tags with `added` joined and `removed` dropped, both matched
/// case-insensitively. `Some("")` when no tags are left; `None` when there is
/// nothing to add or remove.
fn edit_tags(existing: Option<&str>, added: &[String], removed: &[String]) -> Option<String> {
    let given = |tags: &[String]| -> Vec<String> {
        tags.iter()
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
            .collect()
    };
    let (added, removed) = (given(added), given(removed));
    if added.is_empty() && removed.is_empty() {
        return None;
    }
    let mut tags: Vec<&str> = Vec::new();
    for tag in existing
        .unwrap_or("")
        .split(';')
        .map(str::trim)
        .chain(added.iter().map(String::as_str))
        .filter(|tag| !tag.is_empty())
    {
        let dropped = removed.iter().any(|gone| same_tag(gone, tag));
        if !dropped && !tags.iter().any(|kept| same_tag(kept, tag)) {
            tags.push(tag);
        }
    }
    Some(tags.join("; "))
}

impl WorkItemFields {
    /// The patch operations that set each field given.
    fn patch(&self) -> Vec<JsonPatchOperation> {
        let text = |value: &Option<String>| value.as_deref().map(|value| json!(value));
        // A type change goes first, so the fields after it are set on the new type.
        let (type_change, extra): (Vec<_>, Vec<_>) = self
            .extra
            .iter()
            .partition(|(field, _)| is_work_item_type_field(field));
        let extra_op = |(field, value): &(String, String)| set_field_op(field, json!(value));
        type_change
            .into_iter()
            .map(extra_op)
            .chain(
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
                        "Microsoft.VSTS.Scheduling.OriginalEstimate",
                        self.original_estimate.map(|hours| json!(hours)),
                    ),
                    (
                        "Microsoft.VSTS.Scheduling.RemainingWork",
                        self.remaining_work.map(|hours| json!(hours)),
                    ),
                    (
                        "Microsoft.VSTS.Scheduling.CompletedWork",
                        self.completed_work.map(|hours| json!(hours)),
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
                ])
                .filter_map(|(field, value)| value.map(|value| set_field_op(field, value))),
            )
            .chain(self.tags.as_deref().map(|tags| self.tags_op(tags)))
            .chain(extra.into_iter().map(extra_op))
            .collect()
    }
}

impl WorkItemFields {
    fn tags_op(&self, tags: &str) -> JsonPatchOperation {
        let mut op = set_field_op("System.Tags", json!(tags));
        if self.replace_tags {
            if tags.is_empty() {
                op.op = Some(Op::Remove);
                op.value = None;
            } else {
                op.op = Some(Op::Replace);
            }
        }
        op
    }
}

fn is_work_item_type_field(field: &str) -> bool {
    field.eq_ignore_ascii_case("System.WorkItemType")
}

/// The type a work item will have after an update: one set with `--field`,
/// or else its `current` type.
fn requested_type<'a>(fields: &'a [(String, String)], current: Option<&'a str>) -> Option<&'a str> {
    fields
        .iter()
        .rev()
        .find(|(field, _)| is_work_item_type_field(field))
        .map(|(_, value)| value.as_str())
        .or(current)
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
    conn: &Connection,
    project: &str,
    work_item_type: &str,
    effort: Option<f64>,
) -> Result<Option<(&'static str, f64)>> {
    let Some(effort) = effort else {
        return Ok(None);
    };
    let fields = conn
        .wit()
        .work_item_types_field_client()
        .list(conn.organization(), project, work_item_type)
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

/// The patch that creates a task called `title` under the created `parent`
/// at `parent_url`, in the iteration and area the parent ended up in, even
/// when those came from `--field` or the project's defaults.
fn child_task_patch(
    title: &str,
    parent: &models::WorkItem,
    parent_url: &str,
) -> Vec<JsonPatchOperation> {
    let path = |field: &str| {
        parent
            .fields
            .get(field)
            .and_then(|value| value.as_str())
            .map(str::to_string)
    };
    let fields = WorkItemFields {
        title: Some(title.to_string()),
        iteration_path: path("System.IterationPath"),
        area_path: path("System.AreaPath"),
        ..Default::default()
    };
    create_work_item_patch(&fields, Some(parent_url))
}

/// What `work-item update` changes on each of the work items it is given.
struct UpdatePlan<'a> {
    project: &'a str,
    /// The fields set the same way on each work item. Tags and effort depend
    /// on the work item, so they are worked out per item.
    fields: WorkItemFields,
    added_tags: &'a [String],
    removed_tags: &'a [String],
    effort: Option<f64>,
    /// The new parent's ID and URL.
    parent: Option<(i32, String)>,
    /// Operations that attach the files uploaded for this update.
    attachments: Vec<JsonPatchOperation>,
}

async fn update_one(conn: &Connection, plan: &UpdatePlan<'_>, id: i32) -> Result<models::WorkItem> {
    let project = plan.project;
    // Setting System.Tags replaces them, so new tags join the existing
    // ones, and --effort needs the item's type to pick its effort field.
    // The revision these were read at guards the update, so a change by
    // another client in between fails it instead of being lost or misread.
    // A new parent replaces the old one, which is found among the relations.
    let current = if plan.parent.is_some() {
        Some(get_work_item_with_relations(conn, project, id).await?)
    } else if plan.added_tags.is_empty() && plan.removed_tags.is_empty() && plan.effort.is_none() {
        None
    } else {
        Some(get_work_item(conn, project, &id.to_string()).await?)
    };
    let field = |name| {
        current
            .as_ref()
            .and_then(|item| item.fields.get(name))
            .and_then(|v| v.as_str())
    };
    let effort = match requested_type(&plan.fields.extra, field("System.WorkItemType")) {
        Some(work_item_type) => {
            effort_assignment(conn, project, work_item_type, plan.effort).await?
        }
        None => None,
    };
    let fields = WorkItemFields {
        tags: edit_tags(field("System.Tags"), plan.added_tags, plan.removed_tags),
        replace_tags: !plan.removed_tags.is_empty(),
        effort,
        ..plan.fields.clone()
    };

    let mut patch: Vec<_> = current
        .as_ref()
        .and_then(|item| item.rev)
        .map(revision_test_op)
        .into_iter()
        .collect();
    patch.extend(fields.patch());
    if let (Some((parent, parent_url)), Some(current)) = (&plan.parent, &current) {
        patch.extend(link::set_parent_ops(current, *parent, parent_url)?);
    }
    patch.extend(plan.attachments.iter().cloned());
    update_work_item(conn, project, &id.to_string(), patch).await
}

fn work_item_title(work_item: &models::WorkItem) -> &str {
    work_item
        .fields
        .get("System.Title")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
}

fn print_updated(work_item: &models::WorkItem) {
    println!("{}", "✅ Work item updated successfully!".green());
    println!("Updated work item with ID: {}", work_item.id);
    println!("Updated Title: {}", work_item_title(work_item));
    if let Some(description) = work_item
        .fields
        .get("System.Description")
        .and_then(|v| v.as_str())
    {
        println!("Updated Description: {description}");
    }
}

/// Runs `update` on each of `ids` in turn, telling `report` how each went.
/// One failure does not stop the others. Returns the updated work items, and
/// an error naming the failed IDs if there were any.
async fn update_each<F, Fut>(
    ids: &[i32],
    update: F,
    mut report: impl FnMut(Result<&models::WorkItem, (i32, &anyhow::Error)>),
) -> (Vec<models::WorkItem>, Result<()>)
where
    F: Fn(i32) -> Fut,
    Fut: std::future::Future<Output = Result<models::WorkItem>>,
{
    let mut updated = Vec::new();
    let mut failed = Vec::new();
    for id in ids {
        match update(*id).await {
            Ok(work_item) => {
                report(Ok(&work_item));
                updated.push(work_item);
            }
            Err(e) => {
                report(Err((*id, &e)));
                failed.push(*id);
            }
        }
    }
    let result = if failed.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(bulk_failure(&failed, ids.len())))
    };
    (updated, result)
}

/// The error for an update of several work items that some failed.
fn bulk_failure(failed: &[i32], total: usize) -> String {
    let ids = failed
        .iter()
        .map(|id| format!("#{id}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{} of {total} work items failed to update: {ids}",
        failed.len()
    )
}

/// Says what a create already made when reading the result back fails, so
/// the user knows not to create it again.
fn read_back_failure(parent: i32, tasks: &[i32]) -> String {
    let tasks = tasks
        .iter()
        .map(|id| format!("#{id}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("Created #{parent} with tasks {tasks}, but could not read it back")
}

async fn get_work_item_with_relations(
    conn: &Connection,
    project: &str,
    id: i32,
) -> Result<models::WorkItem> {
    Ok(conn
        .wit()
        .work_items_client()
        .get_work_item(conn.organization(), id, project)
        .expand("relations")
        .await?)
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
    conn: &Connection,
    project: &str,
    work_item_type: &str,
    patch: Vec<JsonPatchOperation>,
) -> Result<models::WorkItem> {
    Ok(conn
        .wit()
        .work_items_client()
        .create(
            conn.organization(),
            patch,
            project.to_string(),
            work_item_type,
        )
        .await?)
}

async fn update_work_item(
    conn: &Connection,
    project: &str,
    id: &str,
    patch_operations: Vec<JsonPatchOperation>,
) -> Result<models::WorkItem> {
    let id_int = parse_work_item_id(id)?;

    let client = conn.wit();
    let work_item = client
        .work_items_client()
        .update(
            conn.organization(),
            patch_operations,
            id_int,
            project.to_string(),
        )
        .await?;
    Ok(work_item)
}

async fn delete_work_item(
    conn: &Connection,
    project: &str,
    id: &str,
    soft_delete: bool,
) -> Result<()> {
    let id_int = parse_work_item_id(id)?;
    if soft_delete {
        let work_item = get_work_item(conn, project, id).await?;
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
            conn,
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
        conn.wit()
            .work_items_client()
            .delete(conn.organization(), id_int, project.to_string())
            .await?;
    }
    Ok(())
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
async fn run_wiql_query(
    conn: &Connection,
    project: &str,
    wiql: &str,
    limit: i32,
) -> Result<Vec<models::WorkItem>> {
    let client = conn.wit();

    let query_result = client
        .wiql_client()
        .query_by_wiql(
            conn.organization(),
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
                .get_work_item(conn.organization(), id, project)
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
    conn: &Connection,
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

    match run_wiql_query(conn, project, &wiql_query, limit).await {
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

pub async fn handle_command(
    conn: &Connection,
    subcommand: &BoardsSubCommands,
    format: OutputFormat,
) -> Result<()> {
    match subcommand {
        BoardsSubCommands::WorkItem { subcommand } => {
            handle_work_item_command(conn, subcommand, format).await
        }
        BoardsSubCommands::Area { subcommand } => area::handle_command(conn, subcommand).await,
        BoardsSubCommands::Iteration { subcommand } => {
            iteration::handle_command(conn, subcommand).await
        }
        BoardsSubCommands::Sprint { subcommand } => {
            sprint::handle_command(conn, subcommand, format).await
        }
        BoardsSubCommands::Backlog { subcommand } => {
            backlog::handle_command(conn, subcommand, format).await
        }
        BoardsSubCommands::Queries { project, depth } => {
            let project_name = conn.project(project.as_deref())?;
            output::emit(&queries::list_queries(&project_name, *depth).await?, format)
        }
        BoardsSubCommands::Query {
            wiql,
            saved,
            project,
            limit,
        } => {
            let project_name = conn.project(project.as_deref())?;
            let wiql = match (wiql, saved) {
                (Some(wiql), _) => wiql.clone(),
                (None, Some(saved)) => queries::saved_query_wiql(&project_name, saved).await?,
                (None, None) => return Err(anyhow!("Provide --wiql or --saved")),
            };
            match run_wiql_query(conn, &project_name, &wiql, *limit).await {
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

async fn handle_work_item_comment_command(
    conn: &Connection,
    subcommand: &WorkItemCommentSubCommands,
) -> Result<()> {
    match subcommand {
        WorkItemCommentSubCommands::List { id, project, top } => {
            let project_name = conn.project(project.as_deref())?;
            match list_work_item_comments(conn, &project_name, id, *top).await {
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
            let project_name = conn.project(project.as_deref())?;
            match add_work_item_comment(conn, &project_name, id, message).await {
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
    conn: &Connection,
    subcommand: &WorkItemSubCommands,
    format: OutputFormat,
) -> Result<()> {
    match subcommand {
        WorkItemSubCommands::Create {
            work_item_type,
            title,
            project,
            parent,
            tasks,
            iteration_path,
            area_path,
            content,
        } => {
            let project_name = conn.project(project.as_deref())?;
            let work_item_type = resolve_work_item_type(work_item_type);
            if format == OutputFormat::Text {
                println!("Creating a {work_item_type} work item in project: {project_name}");
            }

            let parent_url = parent
                .map(|id| api_work_item_url(&conn.credentials().base_url, conn.organization(), id));
            let iteration_path = iteration::resolve_iteration_path(
                conn,
                &project_name,
                content.team.as_deref(),
                iteration_path.as_deref(),
            )
            .await?;
            let effort =
                effort_assignment(conn, &project_name, &work_item_type, content.effort).await?;
            let fields = WorkItemFields {
                title: Some(title.clone()),
                effort,
                description: content.description()?,
                acceptance_criteria: content.acceptance_criteria()?,
                tags: merge_tags(None, &content.tags),
                assigned_to: resolve_assignee(conn, content.assigned_to.as_deref()).await?,
                priority: content.priority,
                remaining_work: content.remaining_work,
                original_estimate: content.original_estimate,
                completed_work: content.completed_work,
                extra: content.fields.clone(),
                iteration_path,
                area_path: area_path.clone(),
                ..Default::default()
            };

            let mut patch = create_work_item_patch(&fields, parent_url.as_deref());
            patch
                .extend(attachment::upload_all(&project_name, &content.attachments, format).await?);

            let work_item =
                match create_work_item(conn, &project_name, &work_item_type, patch).await {
                    Ok(work_item) => work_item,
                    Err(e) => {
                        eprintln!("❌ Failed to create work item: {e}");
                        return Err(e);
                    }
                };

            let new_url = api_work_item_url(
                &conn.credentials().base_url,
                conn.organization(),
                work_item.id,
            );
            let mut created_tasks = Vec::with_capacity(tasks.len());
            for task in tasks {
                let created = create_work_item(
                    conn,
                    &project_name,
                    "Task",
                    child_task_patch(task, &work_item, &new_url),
                )
                .await
                .map_err(|e| {
                    e.context(format!(
                        "Created #{} but not its task '{task}'",
                        work_item.id
                    ))
                })?;
                created_tasks.push(created);
            }

            if format != OutputFormat::Text {
                // With tasks, read the item back so its child links are included.
                let work_item = if tasks.is_empty() {
                    work_item
                } else {
                    get_work_item_with_relations(conn, &project_name, work_item.id)
                        .await
                        .map_err(|e| {
                            let task_ids: Vec<i32> =
                                created_tasks.iter().map(|task| task.id).collect();
                            e.context(read_back_failure(work_item.id, &task_ids))
                        })?
                };
                output::emit(&WorkItemDetails(&work_item), format)?;
            } else {
                println!("{}", "✅ Work item created successfully!".green());
                println!("Created work item with ID: {}", work_item.id);
                println!("Title: {title}");
                if let Some(parent) = parent {
                    println!("Parent: #{parent}");
                }
                if let Some(fields) = work_item.fields.as_object() {
                    if let Some(desc) = fields.get("System.Description").and_then(|v| v.as_str()) {
                        println!("Description: {desc}");
                    }
                }
                for (task, title) in created_tasks.iter().zip(tasks) {
                    println!("  ↳ Task #{}: {title}", task.id);
                }
            }
        }
        WorkItemSubCommands::Delete {
            id,
            project,
            soft_delete,
        } => {
            let project_name = conn.project(project.as_deref())?;
            println!(
                "{}Deleting work item with id: {} in project: {}",
                if *soft_delete { "Soft " } else { "" },
                id,
                project_name
            );

            match delete_work_item(conn, &project_name, id, *soft_delete).await {
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
            let project_name = conn.project(project.as_deref())?;

            match list_work_items(
                conn,
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
            let project_name = conn.project(project.as_deref())?;
            if format == OutputFormat::Text {
                println!("Showing work item with id: {id} in project: {project_name}");
            }

            // Open in browser if requested
            if *web {
                if let Err(e) = open_work_item_in_browser(
                    &conn.credentials().base_url,
                    conn.organization(),
                    &project_name,
                    id,
                ) {
                    eprintln!("❌ Failed to open work item in browser: {e}");
                }
                return Ok(());
            }

            // Otherwise show in terminal
            match get_work_item(conn, &project_name, id).await {
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
            let project_name = conn.project(project.as_deref())?;
            match list_work_item_types(conn, &project_name).await {
                Ok(work_item_types) => display_work_item_types(&work_item_types),
                Err(e) => {
                    eprintln!("❌ Failed to list the work item types of '{project_name}'");
                    eprintln!("   {e}");
                    return Err(e);
                }
            }
        }
        WorkItemSubCommands::States {
            work_item_type,
            project,
        } => {
            let project_name = conn.project(project.as_deref())?;
            let work_item_type = resolve_work_item_type(work_item_type);
            match list_work_item_states(conn, &project_name, &work_item_type)
                .await
                .and_then(|states| require_states(states, &work_item_type))
            {
                Ok(states) => print!("{}", states_table(&states)),
                Err(e) => {
                    eprintln!("❌ Failed to list the states of '{work_item_type}'");
                    eprintln!("   {e}");
                    return Err(e);
                }
            }
        }
        WorkItemSubCommands::Comment { subcommand } => {
            return handle_work_item_comment_command(conn, subcommand).await;
        }
        WorkItemSubCommands::Attachment { subcommand } => {
            return attachment::handle_command(subcommand, format).await;
        }
        WorkItemSubCommands::History {
            id,
            project,
            fields,
            top,
        } => {
            let project = conn.project(project.as_deref())?;
            return history::show(conn, id, &project, fields, *top, format).await;
        }
        WorkItemSubCommands::Move {
            id,
            project,
            team,
            before,
            after,
            top: _,
        } => {
            let project = conn.project(project.as_deref())?;
            let placement = match (before, after) {
                (Some(target), _) => backlog::Placement::Before(*target),
                (_, Some(target)) => backlog::Placement::After(*target),
                _ => backlog::Placement::Top,
            };
            return backlog::move_item(conn, &project, team.as_deref(), *id, placement, format)
                .await;
        }
        WorkItemSubCommands::Link { subcommand } => {
            return link::handle_command(conn, subcommand, format).await;
        }
        WorkItemSubCommands::Update {
            ids,
            project,
            title,
            state,
            iteration_path,
            area_path,
            remove_tags,
            parent,
            content,
        } => {
            let project_name = conn.project(project.as_deref())?;
            if format == OutputFormat::Text {
                match ids.as_slice() {
                    [id] => println!("Updating work item with id: {id} in project: {project_name}"),
                    _ => println!(
                        "Updating {} work items in project: {project_name}",
                        ids.len()
                    ),
                }
            }

            // Everything that is the same for each work item is worked out once.
            let parent = match parent {
                Some(parent) => {
                    let url = api_work_item_url(
                        &conn.credentials().base_url,
                        conn.organization(),
                        *parent,
                    );
                    Some((*parent, url))
                }
                None => None,
            };
            let plan = UpdatePlan {
                project: &project_name,
                fields: WorkItemFields {
                    title: title.clone(),
                    description: content.description()?,
                    acceptance_criteria: content.acceptance_criteria()?,
                    state: state.clone(),
                    assigned_to: resolve_assignee(conn, content.assigned_to.as_deref()).await?,
                    priority: content.priority,
                    remaining_work: content.remaining_work,
                    original_estimate: content.original_estimate,
                    completed_work: content.completed_work,
                    extra: content.fields.clone(),
                    iteration_path: iteration::resolve_iteration_path(
                        conn,
                        &project_name,
                        content.team.as_deref(),
                        iteration_path.as_deref(),
                    )
                    .await?,
                    area_path: area_path.clone(),
                    ..Default::default()
                },
                added_tags: &content.tags,
                removed_tags: remove_tags,
                effort: content.effort,
                parent,
                attachments: attachment::upload_all(&project_name, &content.attachments, format)
                    .await?,
            };

            if let [id] = ids.as_slice() {
                return match update_one(conn, &plan, *id).await {
                    Ok(work_item) if format != OutputFormat::Text => {
                        output::emit(&WorkItemDetails(&work_item), format)
                    }
                    Ok(work_item) => {
                        print_updated(&work_item);
                        Ok(())
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to update work item: {e}");
                        Err(e)
                    }
                };
            }

            let (updated, result) = update_each(
                ids,
                |id| update_one(conn, &plan, id),
                |outcome| match outcome {
                    Ok(work_item) if format == OutputFormat::Text => {
                        println!(
                            "✅ Updated #{}: {}",
                            work_item.id,
                            work_item_title(work_item)
                        )
                    }
                    Ok(_) => {}
                    Err((id, e)) => eprintln!("❌ Failed to update work item {id}: {e}"),
                },
            )
            .await;
            if format == OutputFormat::Text {
                println!("Updated {} of {} work items", updated.len(), ids.len());
            } else {
                print_work_items("Updated Work Items", &updated, format)?;
            }
            return result;
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
    fn a_type_change_is_patched_first() {
        let patch = WorkItemFields {
            effort: Some(("Microsoft.VSTS.Scheduling.Effort", 5.0)),
            extra: vec![
                ("Custom.Team".into(), "Web".into()),
                ("System.WorkItemType".into(), "Bug".into()),
            ],
            ..Default::default()
        }
        .patch();
        let paths: Vec<_> = patch.iter().filter_map(|op| op.path.as_deref()).collect();
        assert_eq!(
            paths,
            [
                "/fields/System.WorkItemType",
                "/fields/Microsoft.VSTS.Scheduling.Effort",
                "/fields/Custom.Team"
            ]
        );
    }

    #[test]
    fn requested_type_overrides_the_current_one() {
        let fields = [("system.workitemtype".to_string(), "Bug".to_string())];
        assert_eq!(requested_type(&fields, Some("Task")), Some("Bug"));
        assert_eq!(requested_type(&[], Some("Task")), Some("Task"));
        assert_eq!(requested_type(&[], None), None);
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
    fn task_estimates_set_their_fields() {
        let WorkItemSubCommands::Update { content, .. } = parse(&[
            "update",
            "--id",
            "1",
            "--original-estimate",
            "8",
            "--completed-work",
            "3.5",
        ])
        .unwrap() else {
            panic!("expected Update");
        };
        let patch = WorkItemFields {
            original_estimate: content.original_estimate,
            completed_work: content.completed_work,
            ..Default::default()
        }
        .patch();
        let set: Vec<_> = patch
            .iter()
            .map(|op| (op.path.as_deref().unwrap(), op.value.clone().unwrap()))
            .collect();
        assert_eq!(
            set,
            [
                (
                    "/fields/Microsoft.VSTS.Scheduling.OriginalEstimate",
                    json!(8.0)
                ),
                (
                    "/fields/Microsoft.VSTS.Scheduling.CompletedWork",
                    json!(3.5)
                ),
            ]
        );
        assert!(parse(&["update", "--id", "1", "--completed-work", "NaN"]).is_err());
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
    fn read_back_failure_names_what_was_created() {
        assert_eq!(
            read_back_failure(314, &[315, 316]),
            "Created #314 with tasks #315, #316, but could not read it back"
        );
    }

    #[test]
    fn task_is_repeatable_on_create() {
        let WorkItemSubCommands::Create { tasks, .. } = parse(&[
            "create",
            "pbi",
            "--title",
            "Sign up",
            "--task",
            "Write tests",
            "--task",
            "Implement, then review",
        ])
        .unwrap() else {
            panic!("expected Create");
        };
        assert_eq!(tasks, ["Write tests", "Implement, then review"]);
    }

    #[test]
    fn child_task_shares_the_parents_sprint_and_area() {
        // The created parent's paths, which may come from --field or defaults.
        let parent: models::WorkItem = serde_json::from_value(json!({
            "id": 9,
            "url": "https://parent",
            "fields": {
                "System.Title": "Sign up",
                "System.IterationPath": "Proj\\Sprint 3",
                "System.AreaPath": "Proj\\Web",
                "System.Tags": "auth"
            }
        }))
        .unwrap();
        let patch = child_task_patch("Write tests", &parent, "https://parent");
        let ops: Vec<_> = patch
            .iter()
            .map(|op| (op.path.clone().unwrap(), op.value.clone().unwrap()))
            .collect();
        assert_eq!(
            ops,
            [
                ("/fields/System.Title".to_string(), json!("Write tests")),
                (
                    "/fields/System.IterationPath".to_string(),
                    json!("Proj\\Sprint 3")
                ),
                ("/fields/System.AreaPath".to_string(), json!("Proj\\Web")),
                (
                    "/relations/-".to_string(),
                    json!({"rel": "System.LinkTypes.Hierarchy-Reverse", "url": "https://parent"})
                ),
            ]
        );
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
    fn signed_in_account_comes_from_connection_data() {
        let data = json!({
            "authenticatedUser": {
                "id": "3d22",
                "properties": {"Account": {"$type": "System.String", "$value": "ada@example.com"}}
            }
        });
        assert_eq!(signed_in_account(&data).as_deref(), Some("ada@example.com"));
        assert_eq!(signed_in_account(&json!({"authenticatedUser": {}})), None);
    }

    #[test]
    fn only_at_me_needs_a_lookup() {
        assert!(is_me("@me"));
        assert!(is_me(" @Me "));
        assert!(!is_me("ada@example.com"));
        assert!(!is_me(""));
    }

    #[test]
    fn states_takes_a_type_alias() {
        let WorkItemSubCommands::States { work_item_type, .. } =
            parse(&["states", "pbi", "-p", "Proj"]).unwrap()
        else {
            panic!("expected States");
        };
        assert_eq!(
            resolve_work_item_type(&work_item_type),
            "Product Backlog Item"
        );
    }

    #[test]
    fn no_states_means_an_unknown_type() {
        let error = require_states(Vec::new(), "Story").unwrap_err();
        assert!(error.to_string().contains("'Story'"));
        assert!(error.to_string().contains("boards work-item types"));
        assert_eq!(
            require_states(vec![models::WorkItemStateColor::new()], "Bug")
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn states_table_lists_each_state_with_its_category() {
        let state = |name: &str, category: &str| {
            let mut state = models::WorkItemStateColor::new();
            state.name = Some(name.into());
            state.category = Some(category.into());
            state
        };
        let table = states_table(&[state("New", "Proposed"), state("Committed", "InProgress")]);
        assert!(table.contains(&format!("{:<20} {}\n", "New", "Proposed")));
        assert!(table.contains(&format!("{:<20} {}\n", "Committed", "InProgress")));
        assert!(table.ends_with("\n2 state(s)\n"));
    }

    #[test]
    fn replacing_tags_uses_replace_or_remove() {
        let tags_op = |tags: &str, replace_tags| {
            WorkItemFields {
                tags: Some(tags.into()),
                replace_tags,
                ..Default::default()
            }
            .patch()
            .remove(0)
        };
        let added = tags_op("a; b", false);
        assert_eq!(
            (added.op, added.value),
            (Some(Op::Add), Some(json!("a; b")))
        );
        let replaced = tags_op("a", true);
        assert_eq!(
            (replaced.op, replaced.value),
            (Some(Op::Replace), Some(json!("a")))
        );
        let cleared = tags_op("", true);
        assert_eq!((cleared.op, cleared.value), (Some(Op::Remove), None));
        assert_eq!(cleared.path.as_deref(), Some("/fields/System.Tags"));
    }

    #[test]
    fn edit_tags_removes_case_insensitively() {
        assert_eq!(
            edit_tags(
                Some("api; Backend; auth"),
                &["new".into()],
                &["backend".into()]
            )
            .as_deref(),
            Some("api; auth; new")
        );
    }

    #[test]
    fn edit_tags_matches_non_ascii_case_insensitively() {
        assert_eq!(
            edit_tags(Some("Équipe; api"), &["API".into()], &["équipe".into()]).as_deref(),
            Some("api")
        );
    }

    #[test]
    fn same_tag_compares_like_dotnet_ignore_case() {
        assert!(same_tag("ΟΔΟΣ", "οδος"));
        assert!(same_tag("σ", "ς"));
        assert!(!same_tag("ß", "ss"));
        assert!(!same_tag("api", "apis"));
    }

    #[test]
    fn edit_tags_removing_the_last_tag_clears_them() {
        assert_eq!(
            edit_tags(Some("api"), &[], &["API".into()]).as_deref(),
            Some("")
        );
    }

    #[test]
    fn edit_tags_with_nothing_to_change_is_none() {
        assert_eq!(edit_tags(Some("api"), &[], &[" ".into()]), None);
    }

    #[test]
    fn update_takes_several_ids() {
        let WorkItemSubCommands::Update { ids, .. } =
            parse(&["update", "--id", "1,2", "-i", "3"]).unwrap()
        else {
            panic!("expected Update");
        };
        assert_eq!(ids, [1, 2, 3]);
        assert!(parse(&["update", "--id", "x"]).is_err());
        assert!(parse(&["update", "--title", "t"]).is_err());
    }

    #[tokio::test]
    async fn update_each_keeps_going_after_a_failure() {
        let attempted = std::cell::RefCell::new(Vec::new());
        let mut reported = Vec::new();

        let (updated, result) = update_each(
            &[1, 2, 3],
            |id| {
                attempted.borrow_mut().push(id);
                async move {
                    if id == 2 {
                        Err(anyhow!("TF401232"))
                    } else {
                        Ok(
                            serde_json::from_value(json!({ "id": id, "url": "u", "fields": {} }))
                                .unwrap(),
                        )
                    }
                }
            },
            |outcome| reported.push(outcome.map(|w| w.id).map_err(|(id, _)| id)),
        )
        .await;

        assert_eq!(*attempted.borrow(), [1, 2, 3]);
        assert_eq!(updated.iter().map(|w| w.id).collect::<Vec<_>>(), [1, 3]);
        assert_eq!(reported, [Ok(1), Err(2), Ok(3)]);
        assert_eq!(
            result.unwrap_err().to_string(),
            "1 of 3 work items failed to update: #2"
        );
    }

    #[tokio::test]
    async fn update_each_succeeds_when_every_item_updates() {
        let (updated, result) = update_each(
            &[4],
            |id| async move {
                Ok(serde_json::from_value(json!({ "id": id, "url": "u", "fields": {} })).unwrap())
            },
            |_| {},
        )
        .await;
        assert_eq!(updated.len(), 1);
        assert!(result.is_ok());
    }

    #[test]
    fn bulk_failure_names_the_failed_items() {
        assert_eq!(
            bulk_failure(&[2, 5], 4),
            "2 of 4 work items failed to update: #2, #5"
        );
    }

    #[test]
    fn update_accepts_a_new_parent() {
        let WorkItemSubCommands::Update { parent, .. } =
            parse(&["update", "--id", "1", "--parent", "42"]).unwrap()
        else {
            panic!("expected Update");
        };
        assert_eq!(parent, Some(42));
    }

    #[test]
    fn remove_tag_is_repeatable_and_comma_separated() {
        let WorkItemSubCommands::Update { remove_tags, .. } = parse(&[
            "update",
            "--id",
            "1",
            "--remove-tag",
            "a",
            "--remove-tag",
            "b,c",
        ])
        .unwrap() else {
            panic!("expected Update");
        };
        assert_eq!(remove_tags, ["a", "b", "c"]);
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

    #[tokio::test]
    async fn get_work_item_requests_the_work_item_from_the_connection_endpoint() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 42,
                "rev": 1,
                "fields": { "System.Title": "Stubbed title" },
                "url": "http://example.invalid/_apis/wit/workItems/42"
            })))
            .mount(&server)
            .await;
        let conn = Connection::new(
            crate::auth::Credentials {
                organization: "contoso".to_string(),
                pat: "secret-pat".to_string(),
                base_url: server.uri(),
            },
            None,
        )
        .unwrap();

        let work_item = get_work_item(&conn, "Fabrikam", "42").await.unwrap();

        assert_eq!(work_item.id, 42);
        assert_eq!(work_item.fields["System.Title"], "Stubbed title");
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            // The SDK prefixes the organization with an extra slash.
            requests[0].url.path().trim_start_matches('/'),
            "contoso/Fabrikam/_apis/wit/workitems/42"
        );
        let auth = requests[0].headers.get("authorization").unwrap();
        assert!(auth.to_str().unwrap().starts_with("Basic "));
    }
}
