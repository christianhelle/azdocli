#![allow(dead_code)]

use anyhow::{anyhow, Result};
use azure_devops_rust_api::wit::{models, Client as WitClient};
use serde_json::{json, Value};

use crate::migrate::context::MigrationContext;

pub(super) const WORK_ITEM_MAP: &str = "work_items";
pub(super) const ATTACHMENT_MAP: &str = "work_item_attachments";

pub(super) async fn query_source_work_item_ids(
    ctx: &MigrationContext,
    client: &WitClient,
) -> Result<Vec<i32>> {
    let query = format!(
        "SELECT [System.Id] FROM WorkItems WHERE [System.TeamProject] = '{}' ORDER BY [System.Id]",
        escape_wiql_literal(&ctx.opts.source_project)
    );
    let result = client
        .wiql_client()
        .query_by_wiql(
            ctx.source_creds.organization.clone(),
            models::Wiql { query: Some(query) },
            ctx.opts.source_project.clone(),
            String::new(),
        )
        .time_precision(true)
        .await
        .map_err(|e| anyhow!("Querying source work items: {e}"))?;

    Ok(result
        .work_items
        .into_iter()
        .filter_map(|item| item.id)
        .collect())
}

pub(super) async fn get_source_work_item(
    ctx: &MigrationContext,
    client: &WitClient,
    id: i32,
) -> Result<models::WorkItem> {
    client
        .work_items_client()
        .get_work_item(
            ctx.source_creds.organization.clone(),
            id,
            ctx.opts.source_project.clone(),
        )
        .expand("All")
        .await
        .map_err(|e| anyhow!("Fetching source work item {id}: {e}"))
}

pub(super) async fn get_target_work_item(
    ctx: &MigrationContext,
    client: &WitClient,
    id: i32,
) -> Result<models::WorkItem> {
    client
        .work_items_client()
        .get_work_item(
            ctx.target_creds.organization.clone(),
            id,
            ctx.opts.target_project.clone(),
        )
        .expand("All")
        .await
        .map_err(|e| anyhow!("Fetching target work item {id}: {e}"))
}

pub(super) fn target_work_item_url(ctx: &MigrationContext, id: &str) -> String {
    format!(
        "{}/{}/{}/_apis/wit/workItems/{}",
        ctx.target_base_url(),
        percent_encode_path_segment(&ctx.target_creds.organization),
        percent_encode_path_segment(&ctx.opts.target_project),
        id
    )
}

pub(super) fn mapped_work_item_id(ctx: &MigrationContext, source_id: i32) -> Option<String> {
    ctx.state
        .id_map(WORK_ITEM_MAP)
        .and_then(|map| map.map.get(&source_id.to_string()))
        .cloned()
}

pub(super) fn add_op(path: impl Into<String>, value: Value) -> models::JsonPatchOperation {
    models::JsonPatchOperation {
        from: None,
        op: Some(models::json_patch_operation::Op::Add),
        path: Some(path.into()),
        value: Some(value),
    }
}

pub(super) fn replace_op(path: impl Into<String>, value: Value) -> models::JsonPatchOperation {
    models::JsonPatchOperation {
        from: None,
        op: Some(models::json_patch_operation::Op::Replace),
        path: Some(path.into()),
        value: Some(value),
    }
}

pub(super) fn field_string<'a>(work_item: &'a models::WorkItem, field: &str) -> Option<&'a str> {
    work_item.fields.get(field).and_then(Value::as_str)
}

pub(super) fn relation_name(relation: &models::WorkItemRelation) -> String {
    relation
        .link
        .attributes
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("attachment")
        .to_string()
}

pub(super) fn attachment_id_from_url(url: &str) -> Option<String> {
    let marker = "/attachments/";
    let start = url.find(marker)? + marker.len();
    let rest = &url[start..];
    let end = rest.find(['?', '/', '&']).unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

pub(super) fn rewrite_attachment_references(text: &str, ctx: &MigrationContext) -> String {
    let Some(map) = ctx.state.id_map(ATTACHMENT_MAP) else {
        return text.to_string();
    };

    let mut rewritten = text.to_string();
    for (source, target) in &map.map {
        rewritten = rewritten.replace(source, target);
    }
    rewritten
}

pub(super) fn identity_to_plain_text(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Object(map) => {
            let display = map
                .get("displayName")
                .or_else(|| map.get("DisplayName"))
                .and_then(Value::as_str);
            let unique = map
                .get("uniqueName")
                .or_else(|| map.get("UniqueName"))
                .and_then(Value::as_str);
            match (display, unique) {
                (Some(display), Some(unique)) if !unique.is_empty() => {
                    Some(format!("{display} <{unique}>"))
                }
                (Some(display), _) => Some(display.to_string()),
                (_, Some(unique)) => Some(unique.to_string()),
                _ => None,
            }
        }
        _ => None,
    }
}

pub(super) fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub(super) fn field_names_from_update(update: &models::WorkItemUpdate) -> Vec<String> {
    update
        .fields
        .as_ref()
        .and_then(Value::as_object)
        .map(|fields| fields.keys().cloned().collect())
        .unwrap_or_default()
}

pub(super) fn update_identity(update: &models::WorkItemUpdate) -> String {
    update
        .revised_by
        .as_ref()
        .map(|identity| {
            let value = serde_json::to_value(identity).unwrap_or(Value::Null);
            identity_to_plain_text(&value).unwrap_or_else(|| "unknown".to_string())
        })
        .unwrap_or_else(|| "unknown".to_string())
}

pub(super) fn percent_encode_path_segment(segment: &str) -> String {
    let mut encoded = String::new();
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

pub(super) fn json_relation(rel: &str, url: &str, attributes: Value) -> Value {
    json!({
        "rel": rel,
        "url": url,
        "attributes": attributes,
    })
}

fn escape_wiql_literal(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_to_plain_text_prefers_display_name_with_unique_name() {
        let value = json!({"displayName":"Ada Lovelace","uniqueName":"ada@example.test"});

        assert_eq!(
            identity_to_plain_text(&value),
            Some("Ada Lovelace <ada@example.test>".to_string())
        );
    }

    #[test]
    fn attachment_id_from_url_extracts_id_before_query() {
        let url = "https://example.com/org/project/_apis/wit/attachments/abc-123?fileName=a.png";

        assert_eq!(attachment_id_from_url(url), Some("abc-123".to_string()));
    }
}
