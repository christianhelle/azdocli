//! Saved (shared and personal) work item queries.

use crate::auth::url::percent_encode_path_segment;
use crate::connection::Connection;
use crate::output::{md_table, Render};
use crate::text::escape_control_characters;
use anyhow::{bail, Context, Result};
use azure_devops_rust_api::wit::models::query_hierarchy_item::QueryType;
use azure_devops_rust_api::wit::models::QueryHierarchyItem;
use serde::Serialize;
use std::io::{self, Write};

/// One saved query or folder, flattened out of the hierarchy.
#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(super) struct QueryEntry {
    pub depth: usize,
    pub name: String,
    pub path: String,
    pub id: String,
    pub is_folder: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_type: Option<String>,
}

/// The queries and folders under `items`, parents before their children.
pub(super) fn flatten(items: &[QueryHierarchyItem]) -> Vec<QueryEntry> {
    let mut entries = Vec::new();
    collect(items, 0, &mut entries);
    entries
}

fn collect(items: &[QueryHierarchyItem], depth: usize, out: &mut Vec<QueryEntry>) {
    for item in items {
        let is_folder = item.is_folder.unwrap_or(false);
        out.push(QueryEntry {
            depth,
            name: item.name.clone().unwrap_or_default(),
            path: item.path.clone().unwrap_or_default(),
            id: item.id.clone().unwrap_or_default(),
            is_folder,
            query_type: if is_folder {
                None
            } else {
                item.query_type.as_ref().map(query_type_name)
            },
        });
        collect(&item.children, depth + 1, out);
    }
}

fn query_type_name(query_type: &QueryType) -> String {
    match query_type {
        QueryType::Flat => "flat",
        QueryType::Tree => "tree",
        QueryType::OneHop => "oneHop",
    }
    .to_string()
}

/// `path_or_id` as a URL path, each segment percent-encoded and `/` kept.
fn route(path_or_id: &str) -> String {
    path_or_id
        .split('/')
        .map(percent_encode_path_segment)
        .collect::<Vec<_>>()
        .join("/")
}

/// The WIQL of a saved query, or why it cannot be run as one.
pub(super) fn runnable_wiql(item: &QueryHierarchyItem, requested: &str) -> Result<String> {
    if item.is_folder.unwrap_or(false) {
        bail!(
            "'{requested}' is a query folder, not a query. List its queries with `boards queries`"
        );
    }
    match item.query_type {
        Some(QueryType::Tree) | Some(QueryType::OneHop) => bail!(
            "'{requested}' is a {} query; only flat queries can be run with --saved",
            item.query_type
                .as_ref()
                .map(query_type_name)
                .unwrap_or_default()
        ),
        _ => {}
    }
    match item.wiql.as_deref() {
        Some(wiql) if !wiql.trim().is_empty() => Ok(wiql.to_string()),
        _ => bail!("'{requested}' has no WIQL to run"),
    }
}

/// The project's saved queries, flattened.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct QueryList {
    queries: Vec<QueryEntry>,
}

const HEADERS: [&str; 4] = ["Path", "Kind", "Type", "ID"];

impl QueryList {
    fn rows(&self) -> Vec<Vec<String>> {
        self.queries
            .iter()
            .map(|q| {
                vec![
                    q.path.clone(),
                    if q.is_folder { "folder" } else { "query" }.to_string(),
                    q.query_type.clone().unwrap_or_default(),
                    q.id.clone(),
                ]
            })
            .collect()
    }
}

impl Render for QueryList {
    fn text(&self, w: &mut dyn Write) -> io::Result<()> {
        writeln!(w, "📋 Saved queries ({})", self.queries.len())?;
        writeln!(w, "{}", "=".repeat(100))?;
        if self.queries.is_empty() {
            writeln!(w, "No saved queries found.")?;
        }
        for q in &self.queries {
            let indent = "  ".repeat(q.depth);
            let name = escape_control_characters(&q.name);
            if q.is_folder {
                writeln!(
                    w,
                    "{indent}📁 {name}/  [folder]  {}",
                    escape_control_characters(&q.path)
                )?;
            } else {
                writeln!(
                    w,
                    "{indent}🔍 {name}  [{}]  {}  {}",
                    q.query_type.as_deref().unwrap_or("query"),
                    escape_control_characters(&q.path),
                    q.id
                )?;
            }
        }
        Ok(())
    }

    fn markdown(&self, w: &mut dyn Write) -> io::Result<()> {
        writeln!(w, "## Saved queries ({})\n", self.queries.len())?;
        write!(w, "{}", md_table(&HEADERS, &self.rows()))
    }
}

/// Lists the saved queries of a project, `depth` levels below the roots.
pub(super) async fn list_queries(
    conn: &Connection,
    project: &str,
    depth: i32,
) -> Result<QueryList> {
    let roots = conn
        .wit()
        .queries_client()
        .list(conn.organization(), project)
        .depth(depth)
        .await
        .context("Failed to list saved queries")?
        .value;
    Ok(QueryList {
        queries: flatten(&roots),
    })
}

/// The WIQL of the saved query at `path_or_id`.
pub(super) async fn saved_query_wiql(
    conn: &Connection,
    project: &str,
    path_or_id: &str,
) -> Result<String> {
    let item = conn
        .wit()
        .queries_client()
        .get(conn.organization(), project, route(path_or_id))
        .expand("wiql")
        .await
        .with_context(|| {
            format!("Saved query '{path_or_id}' was not found or could not be read")
        })?;
    runnable_wiql(&item, path_or_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tree() -> Vec<QueryHierarchyItem> {
        serde_json::from_value(json!([
            {"url": "u", "id": "11111111-1111-1111-1111-111111111111", "name": "Shared Queries",
             "path": "Shared Queries", "isFolder": true, "children": [
                {"url": "u", "id": "22222222-2222-2222-2222-222222222222", "name": "Active Bugs",
                 "path": "Shared Queries/Active Bugs", "queryType": "flat",
                 "wiql": "SELECT [System.Id] FROM WorkItems"}
             ]},
            {"url": "u", "id": "33333333-3333-3333-3333-333333333333", "name": "My Queries",
             "path": "My Queries", "isFolder": true}
        ]))
        .unwrap()
    }

    fn item(value: serde_json::Value) -> QueryHierarchyItem {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn flattens_parents_before_children() {
        let entries = flatten(&tree());
        let paths: Vec<_> = entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(
            paths,
            ["Shared Queries", "Shared Queries/Active Bugs", "My Queries"]
        );
        assert_eq!(entries[1].depth, 1);
        assert!(entries[0].is_folder);
        assert_eq!(entries[1].query_type.as_deref(), Some("flat"));
        assert_eq!(entries[0].query_type, None);
    }

    #[test]
    fn flat_query_yields_its_wiql() {
        let flat = &tree()[0].children[0];
        assert_eq!(
            runnable_wiql(flat, "x").unwrap(),
            "SELECT [System.Id] FROM WorkItems"
        );
    }

    #[test]
    fn route_encodes_segments_but_keeps_separators() {
        assert_eq!(route("Shared Queries/Bugs#1"), "Shared%20Queries/Bugs%231");
        assert_eq!(
            route("22222222-2222-2222-2222-222222222222"),
            "22222222-2222-2222-2222-222222222222"
        );
    }

    #[test]
    fn folder_is_refused() {
        let err = runnable_wiql(&tree()[0], "Shared Queries").unwrap_err();
        assert!(err.to_string().contains("folder"));
    }

    #[test]
    fn tree_and_one_hop_queries_are_refused() {
        for kind in ["tree", "oneHop"] {
            let q = item(json!({"url": "u", "name": "q", "queryType": kind, "wiql": "SELECT 1"}));
            let err = runnable_wiql(&q, "q").unwrap_err();
            assert!(err.to_string().contains(kind), "{err}");
        }
    }

    #[test]
    fn query_without_wiql_is_refused() {
        let q = item(json!({"url": "u", "name": "q", "queryType": "flat"}));
        assert!(runnable_wiql(&q, "q").is_err());
    }

    #[test]
    fn text_marks_folders_and_escapes_names() {
        let list = QueryList {
            queries: flatten(&[item(json!({
                "url": "u", "id": "1", "name": "Bad\u{1b}[31m", "path": "p", "isFolder": true
            }))]),
        };
        let mut out = Vec::new();
        list.text(&mut out).unwrap();
        let out = String::from_utf8(out).unwrap();
        assert!(out.contains("[folder]"));
        assert!(!out.contains('\u{1b}'));
    }

    #[test]
    fn markdown_has_a_table_row_per_query() {
        let list = QueryList {
            queries: flatten(&tree()),
        };
        let mut out = Vec::new();
        list.markdown(&mut out).unwrap();
        let out = String::from_utf8(out).unwrap();
        assert!(out.contains("| Shared Queries/Active Bugs | query | flat |"));
    }
}
