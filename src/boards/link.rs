//! Links between work items: parents and children, related items and
//! dependencies.

use super::{create_wit_client, parse_work_item_id};
use crate::auth::get_credentials;
use crate::output::{self, md_table, OutputFormat, Render};
use crate::project::get_project_or_default;
use crate::text::escape_control_characters;
use anyhow::Result;
use azure_devops_rust_api::wit::models::WorkItem;
use clap::{Subcommand, ValueEnum};
use serde::Serialize;
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
    work_item
        .relations
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
    let client = create_wit_client()?.work_items_client();
    let work_item = client
        .get_work_item(creds.organization.clone(), parse_work_item_id(id)?, project)
        .expand("relations")
        .await?;
    let mut links = links(&work_item);
    if links.is_empty() {
        return Ok(links);
    }
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
    fn every_relation_maps_to_its_link_type() {
        for (link_type, rel) in LINK_TYPES {
            assert_eq!(LinkType::from_rel(rel), Some(*link_type));
        }
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
