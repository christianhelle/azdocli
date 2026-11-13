//! Area and iteration paths of a project, which Azure DevOps calls
//! classification nodes. Both kinds share one API, so the commands for areas
//! and iterations share the code here.

use super::create_wit_client;
use crate::auth::get_credentials;
use crate::auth::url::percent_encode_path_segment;
use crate::text::escape_control_characters;
use anyhow::{anyhow, Result};
use azure_devops_rust_api::wit::models::{
    work_item_classification_node::StructureType, WorkItemClassificationNode,
    WorkItemTrackingResource, WorkItemTrackingResourceReference,
};
use chrono::NaiveDate;
use colored::Colorize;
use serde_json::json;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StructureGroup {
    Areas,
    Iterations,
}

impl StructureGroup {
    /// The route segment the classification node API expects.
    fn route(self) -> &'static str {
        match self {
            Self::Areas => "areas",
            Self::Iterations => "iterations",
        }
    }

    /// The segment Azure DevOps puts after the project in an absolute node path,
    /// e.g. `Area` in `\MyProject\Area\Team A`.
    fn path_segment(self) -> &'static str {
        match self {
            Self::Areas => "Area",
            Self::Iterations => "Iteration",
        }
    }

    pub(super) fn noun(self) -> &'static str {
        match self {
            Self::Areas => "area",
            Self::Iterations => "iteration",
        }
    }

    fn structure_type(self) -> StructureType {
        match self {
            Self::Areas => StructureType::Area,
            Self::Iterations => StructureType::Iteration,
        }
    }
}

/// Path of a node below its project's root node, e.g. `["Team A", "Backend"]`.
/// The root node itself has no segments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NodePath(Vec<String>);

impl NodePath {
    pub(super) fn root() -> Self {
        Self(Vec::new())
    }

    /// Accepts the absolute paths Azure DevOps shows (`\MyProject\Area\Team A`),
    /// the paths work items use (`MyProject\Team A`) and paths relative to the
    /// project's root node (`Team A`). Only the absolute form, with its leading
    /// separator, carries the `Area` or `Iteration` segment, so a child node
    /// named `Area` in `MyProject\Area\Team A` is kept.
    pub(super) fn parse(path: &str, project: &str, group: StructureGroup) -> Self {
        let is_absolute = path.starts_with(['\\', '/']);
        let mut segments: Vec<&str> = path
            .split(['\\', '/'])
            .filter(|segment| !segment.is_empty())
            .collect();

        if segments
            .first()
            .is_some_and(|segment| segment.eq_ignore_ascii_case(project))
        {
            segments.remove(0);
            if is_absolute
                && segments
                    .first()
                    .is_some_and(|segment| segment.eq_ignore_ascii_case(group.path_segment()))
            {
                segments.remove(0);
            }
        }

        Self(segments.into_iter().map(String::from).collect())
    }

    pub(super) fn parse_or_root(path: Option<&str>, project: &str, group: StructureGroup) -> Self {
        path.map(|path| Self::parse(path, project, group))
            .unwrap_or_else(Self::root)
    }

    /// The node's path as the classification node API routes it.
    fn route(&self) -> String {
        self.0
            .iter()
            .map(|segment| percent_encode_path_segment(segment))
            .collect::<Vec<_>>()
            .join("/")
    }

    /// The path in the form work items and team settings use,
    /// e.g. `MyProject\Team A`.
    pub(super) fn logical(&self, project: &str) -> String {
        std::iter::once(project)
            .chain(self.0.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join("\\")
    }
}

/// Parses a date given as `YYYY-MM-DD` on the command line.
pub(super) fn parse_date(value: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| format!("'{value}' is not a date in the form YYYY-MM-DD"))
}

/// The attributes that give an iteration its start and finish dates.
pub(super) fn iteration_dates(start: NaiveDate, finish: NaiveDate) -> Result<serde_json::Value> {
    if start > finish {
        return Err(anyhow!(
            "The start date {start} is after the finish date {finish}"
        ));
    }

    Ok(json!({
        "startDate": format!("{start}T00:00:00Z"),
        "finishDate": format!("{finish}T00:00:00Z"),
    }))
}

fn new_node(
    name: Option<&str>,
    group: StructureGroup,
    attributes: Option<serde_json::Value>,
) -> WorkItemClassificationNode {
    let mut node = WorkItemClassificationNode::new(WorkItemTrackingResource::new(
        WorkItemTrackingResourceReference::new(String::new()),
    ));
    node.name = name.map(str::to_string);
    node.structure_type = Some(group.structure_type());
    node.attributes = attributes;
    node
}

pub(super) async fn get_node(
    project: &str,
    group: StructureGroup,
    path: &NodePath,
    depth: Option<i32>,
) -> Result<WorkItemClassificationNode> {
    let creds = get_credentials()?;
    let mut request = create_wit_client()?.classification_nodes_client().get(
        creds.organization,
        project,
        group.route(),
        path.route(),
    );

    if let Some(depth) = depth {
        request = request.depth(depth);
    }

    Ok(request.await?)
}

pub(super) async fn create_node(
    project: &str,
    group: StructureGroup,
    parent: &NodePath,
    name: &str,
    attributes: Option<serde_json::Value>,
) -> Result<WorkItemClassificationNode> {
    let creds = get_credentials()?;
    Ok(create_wit_client()?
        .classification_nodes_client()
        .create_or_update(
            creds.organization,
            new_node(Some(name), group, attributes),
            project,
            group.route(),
            parent.route(),
        )
        .await?)
}

/// Moves the node with id `child_id`, and everything below it, under `parent`.
pub(super) async fn move_node(
    project: &str,
    group: StructureGroup,
    parent: &NodePath,
    child_id: i32,
) -> Result<WorkItemClassificationNode> {
    let creds = get_credentials()?;
    let mut node = new_node(None, group, None);
    node.id = Some(child_id);

    Ok(create_wit_client()?
        .classification_nodes_client()
        .create_or_update(
            creds.organization,
            node,
            project,
            group.route(),
            parent.route(),
        )
        .await?)
}

/// Renames a node and/or changes its attributes.
pub(super) async fn update_node(
    project: &str,
    group: StructureGroup,
    path: &NodePath,
    name: Option<&str>,
    attributes: Option<serde_json::Value>,
) -> Result<WorkItemClassificationNode> {
    let creds = get_credentials()?;
    Ok(create_wit_client()?
        .classification_nodes_client()
        .update(
            creds.organization,
            new_node(name, group, attributes),
            project,
            group.route(),
            path.route(),
        )
        .await?)
}

pub(super) async fn delete_node(
    project: &str,
    group: StructureGroup,
    path: &NodePath,
    reclassify_id: Option<i32>,
) -> Result<()> {
    let creds = get_credentials()?;
    let mut request = create_wit_client()?.classification_nodes_client().delete(
        creds.organization,
        project,
        group.route(),
        path.route(),
    );

    if let Some(reclassify_id) = reclassify_id {
        request = request.reclassify_id(reclassify_id);
    }

    request.await?;
    Ok(())
}

/// The `YYYY-MM-DD` part of a date attribute of an iteration.
fn date_attribute<'a>(node: &'a WorkItemClassificationNode, key: &str) -> Option<&'a str> {
    node.attributes
        .as_ref()?
        .get(key)?
        .as_str()
        .map(|date| date.get(..10).unwrap_or(date))
}

fn display_tree_node(node: &WorkItemClassificationNode, depth: usize, count: &mut usize) {
    *count += 1;
    let name = escape_control_characters(node.name.as_deref().unwrap_or("-"));
    let id = node
        .id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "-".to_string());

    let mut line = format!("{}{name} (ID: {id})", "  ".repeat(depth));
    if let (Some(start), Some(finish)) = (
        date_attribute(node, "startDate"),
        date_attribute(node, "finishDate"),
    ) {
        line.push_str(&format!("  {start} → {finish}"));
    }
    if depth > 0 && node.children.is_empty() && node.has_children == Some(true) {
        line.push_str(" …");
    }
    println!("{line}");

    for child in &node.children {
        display_tree_node(child, depth + 1, count);
    }
}

pub(super) fn display_tree(group: StructureGroup, node: &WorkItemClassificationNode) {
    let heading = match group {
        StructureGroup::Areas => "🗂️  Areas",
        StructureGroup::Iterations => "🗓️  Iterations",
    };
    println!("{}", heading.bold());
    println!("{}", "=".repeat(80));

    let mut count = 0;
    display_tree_node(node, 0, &mut count);

    println!("\n{count} {}(s)", group.noun());
    if has_unfetched_children(node) {
        println!("💡 '…' marks nodes with children below the requested --depth");
    }
}

fn has_unfetched_children(node: &WorkItemClassificationNode) -> bool {
    node.children.iter().any(|child| {
        (child.children.is_empty() && child.has_children == Some(true))
            || has_unfetched_children(child)
    })
}

pub(super) fn display_node(group: StructureGroup, node: &WorkItemClassificationNode) {
    let heading = match group {
        StructureGroup::Areas => "🗂️  Area Details",
        StructureGroup::Iterations => "🗓️  Iteration Details",
    };
    println!("{}", heading.bold());
    println!("{}", "=".repeat(30));

    if let Some(id) = node.id {
        println!("🆔 ID: {id}");
    }
    if let Some(identifier) = &node.identifier {
        println!("🔑 Identifier: {identifier}");
    }
    if let Some(name) = &node.name {
        println!("📝 Name: {}", escape_control_characters(name));
    }
    if let Some(path) = &node.path {
        println!("📂 Path: {}", escape_control_characters(path));
    }
    if let Some(start) = date_attribute(node, "startDate") {
        println!("📅 Start Date: {start}");
    }
    if let Some(finish) = date_attribute(node, "finishDate") {
        println!("📅 Finish Date: {finish}");
    }
    if let Some(has_children) = node.has_children {
        println!("🌳 Has Children: {has_children}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segments(path: &NodePath) -> Vec<&str> {
        path.0.iter().map(String::as_str).collect()
    }

    #[test]
    fn parses_the_absolute_path_azure_devops_shows() {
        let path = NodePath::parse(
            "\\MyProject\\Area\\Team A\\Backend",
            "MyProject",
            StructureGroup::Areas,
        );
        assert_eq!(segments(&path), ["Team A", "Backend"]);
    }

    #[test]
    fn parses_the_path_work_items_use() {
        let path = NodePath::parse(
            "MyProject\\Sprint 1",
            "myproject",
            StructureGroup::Iterations,
        );
        assert_eq!(segments(&path), ["Sprint 1"]);
    }

    #[test]
    fn parses_a_path_relative_to_the_root() {
        let path = NodePath::parse("Team A/Backend", "MyProject", StructureGroup::Areas);
        assert_eq!(segments(&path), ["Team A", "Backend"]);
    }

    #[test]
    fn keeps_a_child_named_like_the_other_structure_group() {
        let path = NodePath::parse("\\MyProject\\Iteration", "MyProject", StructureGroup::Areas);
        assert_eq!(segments(&path), ["Iteration"]);
    }

    #[test]
    fn keeps_a_child_named_area_in_the_path_work_items_use() {
        let path = NodePath::parse("MyProject\\Area\\Child", "MyProject", StructureGroup::Areas);
        assert_eq!(segments(&path), ["Area", "Child"]);
        assert_eq!(path.logical("MyProject"), "MyProject\\Area\\Child");
    }

    #[test]
    fn keeps_a_child_named_iteration_in_the_path_work_items_use() {
        let path = NodePath::parse(
            "MyProject\\Iteration",
            "MyProject",
            StructureGroup::Iterations,
        );
        assert_eq!(segments(&path), ["Iteration"]);
    }

    #[test]
    fn strips_the_structure_group_from_an_absolute_path_with_forward_slashes() {
        let path = NodePath::parse(
            "/MyProject/Iteration/Sprint 1",
            "MyProject",
            StructureGroup::Iterations,
        );
        assert_eq!(segments(&path), ["Sprint 1"]);
    }

    #[test]
    fn parses_the_project_itself_as_the_root() {
        assert_eq!(
            NodePath::parse("\\MyProject\\Area", "MyProject", StructureGroup::Areas),
            NodePath::root()
        );
        assert_eq!(
            NodePath::parse("MyProject", "MyProject", StructureGroup::Areas),
            NodePath::root()
        );
    }

    #[test]
    fn routes_with_percent_encoded_segments() {
        let path = NodePath::parse("Team A\\R&D", "MyProject", StructureGroup::Areas);
        assert_eq!(path.route(), "Team%20A/R%26D");
        assert_eq!(NodePath::root().route(), "");
    }

    #[test]
    fn logical_path_starts_with_the_project() {
        let path = NodePath::parse(
            "\\MyProject\\Area\\Team A",
            "MyProject",
            StructureGroup::Areas,
        );
        assert_eq!(path.logical("MyProject"), "MyProject\\Team A");
        assert_eq!(NodePath::root().logical("MyProject"), "MyProject");
    }

    #[test]
    fn parses_dates() {
        assert_eq!(
            parse_date("2026-06-03"),
            Ok(NaiveDate::from_ymd_opt(2026, 6, 3).unwrap())
        );
        assert!(parse_date("03/06/2026").is_err());
    }

    #[test]
    fn iteration_dates_are_sent_as_utc_midnight() {
        let start = NaiveDate::from_ymd_opt(2026, 6, 3).unwrap();
        let finish = NaiveDate::from_ymd_opt(2026, 6, 14).unwrap();
        assert_eq!(
            iteration_dates(start, finish).unwrap(),
            json!({
                "startDate": "2026-06-03T00:00:00Z",
                "finishDate": "2026-06-14T00:00:00Z",
            })
        );
    }

    #[test]
    fn iteration_dates_reject_a_start_after_the_finish() {
        let start = NaiveDate::from_ymd_opt(2026, 6, 14).unwrap();
        let finish = NaiveDate::from_ymd_opt(2026, 6, 3).unwrap();
        assert!(iteration_dates(start, finish).is_err());
    }

    #[test]
    fn date_attributes_show_only_the_date() {
        let mut node = new_node(Some("Sprint 1"), StructureGroup::Iterations, None);
        node.attributes = Some(json!({ "startDate": "2026-06-03T00:00:00Z" }));
        assert_eq!(date_attribute(&node, "startDate"), Some("2026-06-03"));
        assert_eq!(date_attribute(&node, "finishDate"), None);
    }
}
