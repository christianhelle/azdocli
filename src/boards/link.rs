//! Links between work items: parents and children, related items and
//! dependencies.

use super::{
    add_relation_op, create_wit_client, parse_work_item_id, revision_test_op, update_work_item,
};
use crate::auth::get_credentials;
use crate::auth::url::api_work_item_url;
use crate::output::{self, md_table, OutputFormat, Render};
use crate::project::get_project_or_default;
use crate::text::escape_control_characters;
use anyhow::{bail, Result};
use azure_devops_rust_api::wit::models::json_patch_operation::Op;
use azure_devops_rust_api::wit::models::{JsonPatchOperation, WorkItem, WorkItemRelation};
use clap::{Subcommand, ValueEnum};
use colored::Colorize;
use serde::Serialize;
use serde_json::json;
use std::io::{self, Write};

#[derive(Subcommand, Clone)]
pub enum LinkSubCommands {
    /// List the work items a work item is linked to
    List {
        /// ID of the work item
        #[clap(short, long)]
        id: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
    /// Link a work item to other work items
    Add {
        /// ID of the work item to add the links to
        #[clap(short, long)]
        id: String,
        /// What the other work items are to this one
        #[clap(long = "type", value_enum)]
        link_type: LinkType,
        /// IDs of the work items to link to; repeat the flag or separate IDs
        /// with commas
        #[clap(
            long = "target",
            value_name = "ID",
            value_delimiter = ',',
            required = true
        )]
        targets: Vec<i32>,
        /// Comment to put on the links
        #[clap(long)]
        comment: Option<String>,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
    /// Remove the links between a work item and other work items
    Remove {
        /// ID of the work item to remove the links from
        #[clap(short, long)]
        id: String,
        /// IDs of the linked work items; repeat the flag or separate IDs with
        /// commas
        #[clap(
            long = "target",
            value_name = "ID",
            value_delimiter = ',',
            required = true
        )]
        targets: Vec<i32>,
        /// Only remove links of this type (default: every link to the targets)
        #[clap(long = "type", value_enum)]
        link_type: Option<LinkType>,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
}

/// The kinds of link between work items, seen from the work item they are on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LinkType {
    /// The work item this one belongs to
    Parent,
    /// A work item that belongs to this one
    Child,
    /// A related work item
    Related,
    /// A work item that must be done before this one
    Predecessor,
    /// A work item that waits for this one
    Successor,
    /// A work item that duplicates this one
    Duplicate,
    /// The work item this one duplicates
    DuplicateOf,
}

const LINK_TYPES: &[(LinkType, &str)] = &[
    (LinkType::Parent, "System.LinkTypes.Hierarchy-Reverse"),
    (LinkType::Child, "System.LinkTypes.Hierarchy-Forward"),
    (LinkType::Related, "System.LinkTypes.Related"),
    (LinkType::Predecessor, "System.LinkTypes.Dependency-Reverse"),
    (LinkType::Successor, "System.LinkTypes.Dependency-Forward"),
    (LinkType::Duplicate, "System.LinkTypes.Duplicate-Forward"),
    (LinkType::DuplicateOf, "System.LinkTypes.Duplicate-Reverse"),
];

impl LinkType {
    /// The relation type Azure DevOps stores this link as.
    fn rel(self) -> &'static str {
        LINK_TYPES
            .iter()
            .find(|(link_type, _)| *link_type == self)
            .map(|(_, rel)| *rel)
            .expect("every link type has a relation")
    }

    /// What the linked item is, as in "#5 is now the parent of #7".
    fn role(self) -> &'static str {
        match self {
            LinkType::Parent => "the parent of",
            LinkType::Child => "a child of",
            LinkType::Related => "related to",
            LinkType::Predecessor => "a predecessor of",
            LinkType::Successor => "a successor of",
            LinkType::Duplicate => "a duplicate of",
            LinkType::DuplicateOf => "the original of",
        }
    }

    fn from_rel(rel: &str) -> Option<Self> {
        LINK_TYPES
            .iter()
            .find(|(_, known)| known.eq_ignore_ascii_case(rel))
            .map(|(link_type, _)| *link_type)
    }

    fn name(self) -> &'static str {
        match self {
            LinkType::Parent => "Parent",
            LinkType::Child => "Child",
            LinkType::Related => "Related",
            LinkType::Predecessor => "Predecessor",
            LinkType::Successor => "Successor",
            LinkType::Duplicate => "Duplicate",
            LinkType::DuplicateOf => "Duplicate of",
        }
    }
}

/// A link from a work item to another one.
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkItemLink {
    link_type: LinkType,
    id: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    work_item_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    comment: Option<String>,
}

/// The work item id in an `.../_apis/wit/workItems/{id}` URL.
fn work_item_id_from_url(url: &str) -> Option<i32> {
    let marker = "/_apis/wit/workitems/";
    let start = url.to_ascii_lowercase().rfind(marker)? + marker.len();
    let rest = &url[start..];
    rest[..rest.find(['?', '/', '&']).unwrap_or(rest.len())]
        .parse()
        .ok()
}

/// The links among a work item's relations, without its attachments,
/// hyperlinks and links to commits or builds.
fn links(work_item: &WorkItem) -> Vec<WorkItemLink> {
    links_of(&work_item.relations)
}

fn links_of(relations: &[WorkItemRelation]) -> Vec<WorkItemLink> {
    relations
        .iter()
        .map(|relation| &relation.link)
        .filter_map(|link| {
            Some(WorkItemLink {
                link_type: LinkType::from_rel(&link.rel)?,
                id: work_item_id_from_url(&link.url)?,
                comment: link
                    .attributes
                    .get("comment")
                    .and_then(|comment| comment.as_str())
                    .filter(|comment| !comment.is_empty())
                    .map(str::to_string),
                work_item_type: None,
                state: None,
                title: None,
            })
        })
        .collect()
}

fn field(work_item: &WorkItem, name: &str) -> Option<String> {
    work_item
        .fields
        .get(name)
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

/// Fills in the type, state and title of each linked work item.
fn describe(links: &mut [WorkItemLink], linked: &[WorkItem]) {
    for link in links {
        if let Some(work_item) = linked.iter().find(|work_item| work_item.id == link.id) {
            link.work_item_type = field(work_item, "System.WorkItemType");
            link.state = field(work_item, "System.State");
            link.title = field(work_item, "System.Title");
        }
    }
}

async fn get_links(project: &str, id: &str) -> Result<Vec<WorkItemLink>> {
    let creds = get_credentials()?;
    let work_item = create_wit_client()?
        .work_items_client()
        .get_work_item(creds.organization, parse_work_item_id(id)?, project)
        .expand("relations")
        .await?;
    with_details(project, links(&work_item)).await
}

/// The patch that links work item `id` to each of `targets` as `link_type`.
fn add_links_patch(
    id: i32,
    link_type: LinkType,
    targets: &[i32],
    target_url: impl Fn(i32) -> String,
    comment: Option<&str>,
) -> Result<Vec<JsonPatchOperation>> {
    if targets.contains(&id) {
        bail!("Work item {id} cannot be linked to itself");
    }
    let attributes = comment.map(|comment| json!({ "comment": comment }));
    Ok(targets
        .iter()
        .map(|target| add_relation_op(link_type.rel(), &target_url(*target), attributes.clone()))
        .collect())
}

async fn add_links(
    project: &str,
    id: &str,
    link_type: LinkType,
    targets: &[i32],
    comment: Option<&str>,
) -> Result<Vec<WorkItemLink>> {
    let creds = get_credentials()?;
    let target_url = |target| api_work_item_url(&creds.base_url, &creds.organization, target);
    let patch = add_links_patch(
        parse_work_item_id(id)?,
        link_type,
        targets,
        target_url,
        comment,
    )?;
    let updated = update_work_item(project, id, patch).await?;
    let added = links(&updated)
        .into_iter()
        .filter(|link| link.link_type == link_type && targets.contains(&link.id))
        .collect();
    with_details(project, added).await
}

/// The patch that removes the links from `work_item` to each of `targets`,
/// of `link_type` only if one is given, and the links it removes. The patch
/// fails if the work item changed since it was read, as the relations are
/// removed by position.
fn remove_links_patch(
    work_item: &WorkItem,
    targets: &[i32],
    link_type: Option<LinkType>,
) -> Result<(Vec<JsonPatchOperation>, Vec<WorkItemLink>)> {
    let (indices, removed): (Vec<_>, Vec<_>) = matching_links(work_item, |link| {
        targets.contains(&link.id) && link_type.is_none_or(|t| t == link.link_type)
    })
    .into_iter()
    .unzip();
    if let Some(target) = targets
        .iter()
        .find(|target| !removed.iter().any(|link| link.id == **target))
    {
        let kind = link_type.map_or(String::new(), |t| format!("{} ", t.name().to_lowercase()));
        bail!("Work item {} has no {kind}link to #{target}", work_item.id);
    }
    let mut patch: Vec<_> = work_item.rev.map(revision_test_op).into_iter().collect();
    patch.extend(remove_relation_ops(&indices));
    Ok((patch, removed))
}

/// The links of `work_item` that `keep` picks, with their relation positions.
fn matching_links(
    work_item: &WorkItem,
    keep: impl Fn(&WorkItemLink) -> bool,
) -> Vec<(usize, WorkItemLink)> {
    work_item
        .relations
        .iter()
        .enumerate()
        .filter_map(|(index, relation)| {
            let link = links_of(std::slice::from_ref(relation)).pop()?;
            keep(&link).then_some((index, link))
        })
        .collect()
}

/// Removes the relations at `indices`, which are in ascending order. They
/// go from the last back, so each removal leaves the positions of the ones
/// still to remove as they were.
fn remove_relation_ops(indices: &[usize]) -> impl Iterator<Item = JsonPatchOperation> + '_ {
    indices.iter().rev().map(|index| JsonPatchOperation {
        from: None,
        op: Some(Op::Remove),
        path: Some(format!("/relations/{index}")),
        value: None,
    })
}

/// The operations that make `parent_id`, at `parent_url`, the only parent of
/// `work_item`; none when it already is.
pub(super) fn set_parent_ops(
    work_item: &WorkItem,
    parent_id: i32,
    parent_url: &str,
) -> Result<Vec<JsonPatchOperation>> {
    if parent_id == work_item.id {
        bail!("Work item {parent_id} cannot be its own parent");
    }
    let parents = matching_links(work_item, |link| link.link_type == LinkType::Parent);
    if let [(_, parent)] = parents.as_slice() {
        if parent.id == parent_id {
            return Ok(Vec::new());
        }
    }
    let indices: Vec<usize> = parents.iter().map(|(index, _)| *index).collect();
    let mut ops: Vec<_> = remove_relation_ops(&indices).collect();
    ops.push(add_relation_op(LinkType::Parent.rel(), parent_url, None));
    Ok(ops)
}

async fn remove_links(
    project: &str,
    id: &str,
    targets: &[i32],
    link_type: Option<LinkType>,
) -> Result<Vec<WorkItemLink>> {
    let creds = get_credentials()?;
    let work_item = create_wit_client()?
        .work_items_client()
        .get_work_item(creds.organization, parse_work_item_id(id)?, project)
        .expand("relations")
        .await?;
    let (patch, removed) = remove_links_patch(&work_item, targets, link_type)?;
    update_work_item(project, id, patch).await?;
    with_details(project, removed).await
}

/// Fills in the type, state and title of the linked work items.
async fn with_details(project: &str, mut links: Vec<WorkItemLink>) -> Result<Vec<WorkItemLink>> {
    if links.is_empty() {
        return Ok(links);
    }
    let creds = get_credentials()?;
    let client = create_wit_client()?.work_items_client();
    // One request for up to 200 linked items, which covers any real backlog
    // item; the rest are listed by id alone.
    let ids: Vec<String> = links.iter().take(200).map(|l| l.id.to_string()).collect();
    match client
        .list(creds.organization, ids.join(","), project)
        .error_policy("omit")
        .fields("System.WorkItemType,System.State,System.Title")
        .await
    {
        Ok(linked) => describe(&mut links, &linked.value),
        // A linked item in another project or one the user cannot read
        // should not hide the links themselves.
        Err(e) => eprintln!("⚠️ Could not read the linked work items: {e}"),
    }
    Ok(links)
}

/// The links of a work item, as `link list` prints them.
#[derive(Serialize)]
#[serde(transparent)]
struct LinkList(Vec<WorkItemLink>);

fn text_or_dash(value: &Option<String>) -> String {
    value
        .as_deref()
        .map(escape_control_characters)
        .unwrap_or_else(|| "-".into())
}

impl Render for LinkList {
    fn text(&self, w: &mut dyn Write) -> io::Result<()> {
        if self.0.is_empty() {
            return writeln!(w, "No links found.");
        }
        writeln!(
            w,
            "{:<14} {:<8} {:<20} {:<12} Title",
            "Link", "ID", "Type", "State"
        )?;
        writeln!(w, "{}", "-".repeat(80))?;
        for link in &self.0 {
            writeln!(
                w,
                "{:<14} {:<8} {:<20} {:<12} {}",
                link.link_type.name(),
                link.id,
                text_or_dash(&link.work_item_type),
                text_or_dash(&link.state),
                text_or_dash(&link.title)
            )?;
        }
        writeln!(w, "\n{} link(s)", self.0.len())
    }

    fn markdown(&self, w: &mut dyn Write) -> io::Result<()> {
        let rows: Vec<Vec<String>> = self
            .0
            .iter()
            .map(|link| {
                vec![
                    link.link_type.name().to_string(),
                    link.id.to_string(),
                    text_or_dash(&link.work_item_type),
                    text_or_dash(&link.state),
                    text_or_dash(&link.title),
                ]
            })
            .collect();
        write!(
            w,
            "{}",
            md_table(&["Link", "ID", "Type", "State", "Title"], &rows)
        )
    }
}

pub async fn handle_command(subcommand: &LinkSubCommands, format: OutputFormat) -> Result<()> {
    match subcommand {
        LinkSubCommands::List { id, project } => {
            let project_name = get_project_or_default(project.as_deref())?;
            match get_links(&project_name, id).await {
                Ok(links) => output::emit(&LinkList(links), format),
                Err(e) => {
                    eprintln!("❌ Failed to list the links of work item {id}");
                    eprintln!("   {e}");
                    Err(e)
                }
            }
        }
        LinkSubCommands::Add {
            id,
            link_type,
            targets,
            comment,
            project,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            match add_links(&project_name, id, *link_type, targets, comment.as_deref()).await {
                Ok(added) if format == OutputFormat::Text => {
                    for link in &added {
                        println!("🔗 #{} is now {} #{id}", link.id, link_type.role());
                    }
                    println!("{}", format!("✅ Added {} link(s)", added.len()).green());
                    Ok(())
                }
                Ok(added) => output::emit(&LinkList(added), format),
                Err(e) => {
                    eprintln!("❌ Failed to link work item {id}");
                    eprintln!("   {e}");
                    Err(e)
                }
            }
        }
        LinkSubCommands::Remove {
            id,
            targets,
            link_type,
            project,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            match remove_links(&project_name, id, targets, *link_type).await {
                Ok(removed) if format == OutputFormat::Text => {
                    for link in &removed {
                        println!(
                            "✂️ #{} is no longer {} #{id}",
                            link.id,
                            link.link_type.role()
                        );
                    }
                    println!(
                        "{}",
                        format!("✅ Removed {} link(s)", removed.len()).green()
                    );
                    Ok(())
                }
                Ok(removed) => output::emit(&LinkList(removed), format),
                Err(e) => {
                    eprintln!("❌ Failed to remove the links of work item {id}");
                    eprintln!("   {e}");
                    Err(e)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work_item(value: serde_json::Value) -> WorkItem {
        serde_json::from_value(value).unwrap()
    }

    fn with_relations(relations: serde_json::Value) -> WorkItem {
        work_item(json!({
            "id": 7,
            "url": "https://dev.azure.com/org/_apis/wit/workItems/7",
            "fields": {},
            "relations": relations,
        }))
    }

    #[test]
    fn every_link_type_round_trips_through_its_relation() {
        for (link_type, rel) in LINK_TYPES {
            assert_eq!(LinkType::from_rel(rel), Some(*link_type));
            assert_eq!(link_type.rel(), *rel);
        }
    }

    #[test]
    fn add_links_patch_adds_a_relation_per_target() {
        let patch = add_links_patch(
            7,
            LinkType::Predecessor,
            &[1, 2],
            |id| format!("https://x/_apis/wit/workItems/{id}"),
            Some("API first"),
        )
        .unwrap();

        assert_eq!(patch.len(), 2);
        assert_eq!(patch[0].path.as_deref(), Some("/relations/-"));
        assert_eq!(
            patch[1].value,
            Some(json!({
                "rel": "System.LinkTypes.Dependency-Reverse",
                "url": "https://x/_apis/wit/workItems/2",
                "attributes": { "comment": "API first" }
            }))
        );
    }

    fn linked_item() -> WorkItem {
        work_item(json!({
            "id": 7,
            "rev": 4,
            "url": "https://x/_apis/wit/workItems/7",
            "fields": {},
            "relations": [
                { "rel": "System.LinkTypes.Related", "url": "https://x/_apis/wit/workItems/1", "attributes": {} },
                { "rel": "AttachedFile", "url": "https://x/_apis/wit/attachments/a", "attributes": {} },
                { "rel": "System.LinkTypes.Dependency-Reverse", "url": "https://x/_apis/wit/workItems/1", "attributes": {} },
                { "rel": "System.LinkTypes.Hierarchy-Reverse", "url": "https://x/_apis/wit/workItems/2", "attributes": {} }
            ]
        }))
    }

    fn paths(patch: &[JsonPatchOperation]) -> Vec<&str> {
        patch.iter().filter_map(|op| op.path.as_deref()).collect()
    }

    #[test]
    fn remove_links_patch_removes_every_link_to_the_targets_last_first() {
        let (patch, removed) = remove_links_patch(&linked_item(), &[1, 2], None).unwrap();

        assert_eq!(
            paths(&patch),
            ["/rev", "/relations/3", "/relations/2", "/relations/0"]
        );
        assert_eq!(patch[1].op, Some(Op::Remove));
        assert_eq!(removed.len(), 3);
    }

    #[test]
    fn remove_links_patch_can_keep_other_link_types() {
        let (patch, removed) =
            remove_links_patch(&linked_item(), &[1], Some(LinkType::Predecessor)).unwrap();

        assert_eq!(paths(&patch), ["/rev", "/relations/2"]);
        assert_eq!(removed[0].link_type, LinkType::Predecessor);
    }

    #[test]
    fn remove_links_patch_names_a_target_that_is_not_linked() {
        let err = remove_links_patch(&linked_item(), &[1, 9], None).unwrap_err();
        assert_eq!(err.to_string(), "Work item 7 has no link to #9");

        let err = remove_links_patch(&linked_item(), &[2], Some(LinkType::Child)).unwrap_err();
        assert_eq!(err.to_string(), "Work item 7 has no child link to #2");
    }

    #[test]
    fn set_parent_ops_replace_the_current_parent() {
        let ops = set_parent_ops(&linked_item(), 5, "https://x/_apis/wit/workItems/5").unwrap();

        assert_eq!(paths(&ops), ["/relations/3", "/relations/-"]);
        assert_eq!(ops[0].op, Some(Op::Remove));
        assert_eq!(
            ops[1].value,
            Some(json!({
                "rel": "System.LinkTypes.Hierarchy-Reverse",
                "url": "https://x/_apis/wit/workItems/5"
            }))
        );
    }

    #[test]
    fn set_parent_ops_add_a_first_parent() {
        let orphan = with_relations(json!([]));
        let ops = set_parent_ops(&orphan, 5, "https://p").unwrap();
        assert_eq!(paths(&ops), ["/relations/-"]);
    }

    #[test]
    fn set_parent_ops_leave_the_same_parent_alone() {
        assert!(set_parent_ops(&linked_item(), 2, "https://p")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn set_parent_ops_refuse_the_item_itself() {
        assert!(set_parent_ops(&linked_item(), 7, "https://p").is_err());
    }

    #[test]
    fn add_links_patch_refuses_a_link_to_itself() {
        let err =
            add_links_patch(7, LinkType::Related, &[3, 7], |id| id.to_string(), None).unwrap_err();
        assert!(err.to_string().contains("cannot be linked to itself"));
    }

    #[test]
    fn work_item_id_is_read_from_its_url() {
        assert_eq!(
            work_item_id_from_url("https://dev.azure.com/org/_apis/wit/workItems/42"),
            Some(42)
        );
        assert_eq!(
            work_item_id_from_url("https://x/_apis/wit/workitems/42?x=1"),
            Some(42)
        );
        assert_eq!(
            work_item_id_from_url("https://x/_apis/wit/attachments/abc"),
            None
        );
    }

    #[test]
    fn links_skip_attachments_and_hyperlinks() {
        let work_item = with_relations(json!([
            {
                "rel": "System.LinkTypes.Hierarchy-Reverse",
                "url": "https://x/_apis/wit/workItems/1",
                "attributes": { "isLocked": false, "name": "Parent" }
            },
            {
                "rel": "System.LinkTypes.Dependency-Reverse",
                "url": "https://x/_apis/wit/workItems/2",
                "attributes": { "comment": "API first" }
            },
            {
                "rel": "AttachedFile",
                "url": "https://x/_apis/wit/attachments/abc",
                "attributes": { "name": "spec.md" }
            },
            { "rel": "Hyperlink", "url": "https://example.com", "attributes": {} }
        ]));

        let links = links(&work_item);

        assert_eq!(links.len(), 2);
        assert_eq!((links[0].link_type, links[0].id), (LinkType::Parent, 1));
        assert_eq!(links[0].comment, None);
        assert_eq!(
            (links[1].link_type, links[1].id),
            (LinkType::Predecessor, 2)
        );
        assert_eq!(links[1].comment.as_deref(), Some("API first"));
    }

    #[test]
    fn describe_fills_in_the_linked_items() {
        let mut links = links(&with_relations(json!([
            {
                "rel": "System.LinkTypes.Related",
                "url": "https://x/_apis/wit/workItems/3",
                "attributes": {}
            },
            {
                "rel": "System.LinkTypes.Related",
                "url": "https://x/_apis/wit/workItems/4",
                "attributes": {}
            }
        ])));
        let linked = [work_item(json!({
            "id": 3,
            "url": "https://x/_apis/wit/workItems/3",
            "fields": {
                "System.WorkItemType": "Bug",
                "System.State": "Active",
                "System.Title": "Crash on save"
            }
        }))];

        describe(&mut links, &linked);

        assert_eq!(links[0].title.as_deref(), Some("Crash on save"));
        assert_eq!(links[0].state.as_deref(), Some("Active"));
        assert_eq!(links[1].title, None);
    }

    #[test]
    fn link_list_text_shows_the_link_and_the_item() {
        let mut out = Vec::new();
        LinkList(vec![WorkItemLink {
            link_type: LinkType::DuplicateOf,
            id: 9,
            work_item_type: Some("Bug".into()),
            state: None,
            title: Some("Crash".into()),
            comment: None,
        }])
        .text(&mut out)
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("Duplicate of"));
        assert!(text.contains("Crash"));
        assert!(text.contains("1 link(s)"));
    }

    #[test]
    fn link_json_uses_kebab_case_types() {
        let json = serde_json::to_value(WorkItemLink {
            link_type: LinkType::DuplicateOf,
            id: 9,
            work_item_type: None,
            state: None,
            title: None,
            comment: None,
        })
        .unwrap();
        assert_eq!(json, json!({ "linkType": "duplicate-of", "id": 9 }));
    }
}
