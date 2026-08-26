use anyhow::{anyhow, Result};
use async_trait::async_trait;
use azure_devops_rust_api::wit::models;
use serde_json::Value;

use crate::auth::factory::ClientFactory;
use crate::migrate::context::MigrationContext;
use crate::migrate::phase::{Phase, PhaseSummary};

use super::work_item_common::{
    add_op, escape_html, field_names_from_update, field_string, identity_to_plain_text,
    update_identity, WORK_ITEM_MAP,
};

const STANDARD_FIELDS: &[&str] = &[
    "System.Title",
    "System.State",
    "System.Reason",
    "System.AreaPath",
    "System.IterationPath",
    "System.AssignedTo",
    "System.CreatedBy",
    "System.CreatedDate",
    "System.ChangedBy",
    "System.ChangedDate",
    "System.Tags",
    "System.Description",
    "Microsoft.VSTS.Common.Priority",
    "Microsoft.VSTS.TCM.ReproSteps",
    "Microsoft.VSTS.Common.AcceptanceCriteria",
];

const IDENTITY_FIELDS: &[&str] = &[
    "System.AssignedTo",
    "System.CreatedBy",
    "System.ChangedBy",
    "System.AuthorizedAs",
];

pub struct WorkItemsPhase;

#[async_trait]
impl Phase for WorkItemsPhase {
    fn name(&self) -> &'static str {
        "work_items"
    }

    async fn execute(&self, ctx: &mut MigrationContext) -> Result<PhaseSummary> {
        let source_client = ctx.source_factory()?.build_wit();
        let target_client = ctx.target_factory()?.build_wit();
        let source_ids =
            super::work_item_common::query_source_work_item_ids(ctx, &source_client).await?;
        let mut summary = PhaseSummary {
            items_total: source_ids.len() as u64,
            ..Default::default()
        };

        for source_id in source_ids {
            let _permit = ctx.executor.permit().await;
            match migrate_work_item(ctx, &source_client, &target_client, source_id).await {
                Ok(target_id) => {
                    ctx.state
                        .id_map_mut(WORK_ITEM_MAP)
                        .map
                        .insert(source_id.to_string(), target_id.to_string());
                    summary.record_success();
                }
                Err(e) => summary.record_failure(format!("Work item {source_id}: {e:#}")),
            }
        }

        Ok(summary)
    }
}

async fn migrate_work_item(
    ctx: &MigrationContext,
    source_client: &azure_devops_rust_api::wit::Client,
    target_client: &azure_devops_rust_api::wit::Client,
    source_id: i32,
) -> Result<i32> {
    let source_item =
        super::work_item_common::get_source_work_item(ctx, source_client, source_id).await?;
    let work_item_type = field_string(&source_item, "System.WorkItemType")
        .ok_or_else(|| anyhow!("missing System.WorkItemType"))?;
    let patch = build_create_patch(ctx, source_client, &source_item).await?;

    if ctx.opts.dry_run {
        println!("  ⓘ would migrate work item #{source_id} ({work_item_type})");
        return Ok(source_id);
    }

    let created = target_client
        .work_items_client()
        .create(
            ctx.target_creds.organization.clone(),
            patch,
            ctx.opts.target_project.clone(),
            work_item_type.to_string(),
        )
        .bypass_rules(true)
        .suppress_notifications(true)
        .await
        .map_err(|e| anyhow!("creating target work item: {e}"))?;
    Ok(created.id)
}

async fn build_create_patch(
    ctx: &MigrationContext,
    source_client: &azure_devops_rust_api::wit::Client,
    source_item: &models::WorkItem,
) -> Result<Vec<models::JsonPatchOperation>> {
    let mut patch = Vec::new();
    let fields = source_item
        .fields
        .as_object()
        .ok_or_else(|| anyhow!("source work item fields were not an object"))?;

    for (field, value) in fields {
        if !should_copy_field(field) || value.is_null() {
            continue;
        }

        let mapped_value = map_field_value(ctx, field, value);
        let Some(mapped_value) = mapped_value else {
            continue;
        };
        patch.push(add_op(format!("/fields/{field}"), mapped_value));
    }

    let description = field_string(source_item, "System.Description").unwrap_or_default();
    let history = history_snapshot(ctx, source_client, source_item.id).await?;
    let description_with_history = format!("{description}{history}");
    push_or_replace_field(
        &mut patch,
        "System.Description",
        Value::String(description_with_history),
    );

    Ok(patch)
}

fn should_copy_field(field: &str) -> bool {
    if matches!(
        field,
        "System.Id"
            | "System.TeamProject"
            | "System.WorkItemType"
            | "System.BoardColumn"
            | "System.BoardColumnDone"
            | "System.BoardLane"
            | "System.Rev"
            | "System.Watermark"
            | "System.AuthorizedDate"
    ) {
        return false;
    }

    STANDARD_FIELDS.contains(&field)
        || field.starts_with("Custom.")
        || field.starts_with("MyCustom.")
}

fn map_field_value(ctx: &MigrationContext, field: &str, value: &Value) -> Option<Value> {
    match field {
        "System.AreaPath" => Some(Value::String(map_classification_path(ctx, "areas", value))),
        "System.IterationPath" => Some(Value::String(map_classification_path(
            ctx,
            "iterations",
            value,
        ))),
        identity if IDENTITY_FIELDS.contains(&identity) => {
            identity_to_plain_text(value).map(Value::String)
        }
        _ => Some(value.clone()),
    }
}

fn map_classification_path(ctx: &MigrationContext, kind: &str, value: &Value) -> String {
    let source_path = value.as_str().unwrap_or(&ctx.opts.source_project);
    ctx.state
        .id_map(kind)
        .and_then(|map| map.map.get(source_path))
        .cloned()
        .unwrap_or_else(|| ctx.opts.target_project.clone())
}

fn push_or_replace_field(patch: &mut Vec<models::JsonPatchOperation>, field: &str, value: Value) {
    let path = format!("/fields/{field}");
    if let Some(op) = patch
        .iter_mut()
        .find(|op| op.path.as_deref() == Some(&path))
    {
        op.value = Some(value);
    } else {
        patch.push(add_op(path, value));
    }
}

async fn history_snapshot(
    ctx: &MigrationContext,
    source_client: &azure_devops_rust_api::wit::Client,
    source_id: i32,
) -> Result<String> {
    let updates = source_client
        .updates_client()
        .list(
            ctx.source_creds.organization.clone(),
            source_id,
            ctx.opts.source_project.clone(),
        )
        .top(50)
        .await
        .map_err(|e| anyhow!("fetching work item updates: {e}"))?
        .value;

    let start = updates.len().saturating_sub(50);
    let mut html = format!(
        "<hr/><div><h3>Migrated from {}/{} work item #{}</h3><ul>",
        escape_html(&ctx.source_creds.organization),
        escape_html(&ctx.opts.source_project),
        source_id
    );

    for update in updates.into_iter().skip(start) {
        let fields = field_names_from_update(&update);
        let changed = if fields.is_empty() {
            "no field changes recorded".to_string()
        } else {
            fields.join(", ")
        };
        let date = update
            .revised_date
            .map(|date| date.to_string())
            .unwrap_or_else(|| "unknown date".to_string());
        html.push_str(&format!(
            "<li>Rev {} by {} on {} — fields changed: {}</li>",
            update.rev.unwrap_or_default(),
            escape_html(&update_identity(&update)),
            escape_html(&date),
            escape_html(&changed)
        ));
    }
    html.push_str("</ul></div>");
    Ok(html)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn should_copy_standard_and_custom_fields() {
        assert!(should_copy_field("System.Title"));
        assert!(should_copy_field("Custom.ReleaseTrain"));
        assert!(should_copy_field("MyCustom.Score"));
        assert!(!should_copy_field("System.Id"));
        assert!(!should_copy_field("Microsoft.VSTS.Scheduling.Effort"));
    }

    #[test]
    fn push_or_replace_field_does_not_duplicate_description() {
        let mut patch = vec![add_op("/fields/System.Description", json!("old"))];

        push_or_replace_field(&mut patch, "System.Description", json!("new"));

        assert_eq!(patch.len(), 1);
        assert_eq!(patch[0].value, Some(json!("new")));
    }
}
