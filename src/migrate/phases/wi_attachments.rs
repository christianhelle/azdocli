use anyhow::{anyhow, Result};
use async_trait::async_trait;
use azure_devops_rust_api::wit::models;
use futures::TryStreamExt;
use serde_json::Value;

use crate::auth::factory::ClientFactory;
use crate::migrate::context::MigrationContext;
use crate::migrate::http_client;
use crate::migrate::phase::{Phase, PhaseSummary};

use super::work_item_common::{
    add_op, attachment_id_from_url, field_string, get_source_work_item, get_target_work_item,
    json_relation, mapped_work_item_id, relation_name, replace_op, rewrite_attachment_references,
    ATTACHMENT_MAP, WORK_ITEM_MAP,
};

pub struct WiAttachmentsPhase;

#[async_trait]
impl Phase for WiAttachmentsPhase {
    fn name(&self) -> &'static str {
        "wi_attachments"
    }

    async fn execute(&self, ctx: &mut MigrationContext) -> Result<PhaseSummary> {
        let source_ids = ctx
            .state
            .id_map(WORK_ITEM_MAP)
            .map(|map| {
                map.map
                    .keys()
                    .filter_map(|id| id.parse::<i32>().ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut summary = PhaseSummary {
            items_total: source_ids.len() as u64,
            ..Default::default()
        };

        let source_client = ctx.source_factory()?.build_wit();
        let target_client = ctx.target_factory()?.build_wit();
        let http = http_client::client();

        for source_id in source_ids {
            let _permit = ctx.executor.permit().await;
            match migrate_attachments_for_work_item(
                ctx,
                &source_client,
                &target_client,
                &http,
                source_id,
            )
            .await
            {
                Ok(_) => summary.record_success(),
                Err(e) => {
                    summary.record_failure(format!("Work item attachments {source_id}: {e:#}"))
                }
            }
        }

        Ok(summary)
    }
}

async fn migrate_attachments_for_work_item(
    ctx: &mut MigrationContext,
    source_client: &azure_devops_rust_api::wit::Client,
    target_client: &azure_devops_rust_api::wit::Client,
    http: &reqwest::Client,
    source_id: i32,
) -> Result<()> {
    let Some(target_id) = mapped_work_item_id(ctx, source_id) else {
        return Err(anyhow!(
            "source work item id is missing from work item id map"
        ));
    };
    let target_id = target_id.parse::<i32>()?;
    let source_item = get_source_work_item(ctx, source_client, source_id).await?;
    let attachments = source_item
        .relations
        .iter()
        .filter(|relation| relation.link.rel == "AttachedFile")
        .collect::<Vec<_>>();

    for relation in attachments {
        if let Err(e) = migrate_attachment(ctx, target_client, http, target_id, relation).await {
            println!("  ⚠ attachment on work item #{source_id} failed: {e:#}");
        }
    }

    if !ctx.opts.dry_run {
        rewrite_target_attachment_references(ctx, target_client, target_id).await?;
    }

    Ok(())
}

async fn migrate_attachment(
    ctx: &mut MigrationContext,
    target_client: &azure_devops_rust_api::wit::Client,
    http: &reqwest::Client,
    target_id: i32,
    relation: &models::WorkItemRelation,
) -> Result<()> {
    let source_url = &relation.link.url;
    let source_attachment_id = attachment_id_from_url(source_url)
        .ok_or_else(|| anyhow!("could not parse source attachment id from {source_url}"))?;
    let file_name = relation_name(relation);

    if ctx.opts.dry_run {
        println!("  ⓘ would migrate attachment '{file_name}' ({source_attachment_id})");
        return Ok(());
    }

    let download = http_client::download_attachment(
        http,
        &ctx.source_creds,
        &ctx.opts.source_project,
        &source_attachment_id,
    )
    .await?;
    let uploaded = http_client::upload_attachment_stream(
        http,
        &ctx.target_creds,
        &ctx.opts.target_project,
        &file_name,
        download.bytes_stream().map_ok(|bytes| bytes.to_vec()),
    )
    .await?;
    let target_url = uploaded
        .url
        .ok_or_else(|| anyhow!("target attachment upload did not return a URL"))?;
    let target_attachment_id = uploaded.id.clone().unwrap_or_else(|| target_url.clone());

    let mut attributes = relation.link.attributes.clone();
    if attributes.is_null() {
        attributes = Value::Object(Default::default());
    }
    let patch = vec![add_op(
        "/relations/-",
        json_relation("AttachedFile", &target_url, attributes),
    )];
    target_client
        .work_items_client()
        .update(
            ctx.target_creds.organization.clone(),
            patch,
            target_id,
            ctx.opts.target_project.clone(),
        )
        .bypass_rules(true)
        .suppress_notifications(true)
        .await
        .map_err(|e| anyhow!("adding attachment relation to target work item {target_id}: {e}"))?;

    let map = ctx.state.id_map_mut(ATTACHMENT_MAP);
    map.map.insert(source_attachment_id, target_url.clone());
    map.map.insert(source_url.clone(), target_url.clone());
    if let Some(id) = uploaded.id {
        map.map.insert(target_attachment_id, id);
    }

    Ok(())
}

async fn rewrite_target_attachment_references(
    ctx: &MigrationContext,
    target_client: &azure_devops_rust_api::wit::Client,
    target_id: i32,
) -> Result<()> {
    let target_item = get_target_work_item(ctx, target_client, target_id).await?;
    let mut patch = Vec::new();
    for field in [
        "System.Description",
        "Microsoft.VSTS.TCM.ReproSteps",
        "Microsoft.VSTS.Common.AcceptanceCriteria",
    ] {
        let Some(text) = field_string(&target_item, field) else {
            continue;
        };
        let rewritten = rewrite_attachment_references(text, ctx);
        if rewritten != text {
            patch.push(replace_op(
                format!("/fields/{field}"),
                Value::String(rewritten),
            ));
        }
    }

    if !patch.is_empty() {
        target_client
            .work_items_client()
            .update(
                ctx.target_creds.organization.clone(),
                patch,
                target_id,
                ctx.opts.target_project.clone(),
            )
            .bypass_rules(true)
            .suppress_notifications(true)
            .await
            .map_err(|e| anyhow!("rewriting target work item attachment URLs: {e}"))?;
    }

    Ok(())
}
