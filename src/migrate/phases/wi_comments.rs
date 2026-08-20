use anyhow::{anyhow, Result};
use async_trait::async_trait;
use azure_devops_rust_api::wit::models;
use serde_json::Value;

use crate::auth::factory::ClientFactory;
use crate::migrate::context::MigrationContext;
use crate::migrate::phase::{Phase, PhaseSummary};

use super::work_item_common::{
    identity_to_plain_text, mapped_work_item_id, rewrite_attachment_references, WORK_ITEM_MAP,
};

pub struct WiCommentsPhase;

#[async_trait]
impl Phase for WiCommentsPhase {
    fn name(&self) -> &'static str {
        "wi_comments"
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

        let source_client = ctx.source_factory().build_wit();
        let target_client = ctx.target_factory().build_wit();

        for source_id in source_ids {
            let _permit = ctx.executor.permit().await;
            match migrate_comments_for_work_item(ctx, &source_client, &target_client, source_id)
                .await
            {
                Ok(_) => summary.record_success(),
                Err(e) => summary.record_failure(format!("Work item comments {source_id}: {e:#}")),
            }
        }

        Ok(summary)
    }
}

async fn migrate_comments_for_work_item(
    ctx: &MigrationContext,
    source_client: &azure_devops_rust_api::wit::Client,
    target_client: &azure_devops_rust_api::wit::Client,
    source_id: i32,
) -> Result<()> {
    let Some(target_id) = mapped_work_item_id(ctx, source_id) else {
        return Err(anyhow!(
            "source work item id is missing from work item id map"
        ));
    };
    let target_id = target_id.parse::<i32>()?;
    let comments = list_comments(ctx, source_client, source_id).await?;

    for comment in comments {
        if let Err(e) = post_comment(ctx, target_client, target_id, &comment).await {
            println!("  ⚠ comment on work item #{source_id} failed: {e:#}");
        }
    }

    Ok(())
}

async fn list_comments(
    ctx: &MigrationContext,
    source_client: &azure_devops_rust_api::wit::Client,
    source_id: i32,
) -> Result<Vec<models::Comment>> {
    let mut comments = Vec::new();
    let mut continuation_token = None;

    loop {
        let mut request = source_client
            .comments_client()
            .get_comments(
                ctx.source_creds.organization.clone(),
                ctx.opts.source_project.clone(),
                source_id,
            )
            .top(200)
            .order("Asc");
        if let Some(token) = continuation_token {
            request = request.continuation_token(token);
        }

        let page = request
            .await
            .map_err(|e| anyhow!("listing comments for source work item {source_id}: {e}"))?;
        continuation_token = page.continuation_token.clone();
        comments.extend(page.comments.into_iter().filter(|comment| {
            !comment.is_deleted.unwrap_or(false)
                && comment.text.as_deref().is_some_and(|text| !text.is_empty())
        }));
        if continuation_token.is_none() {
            break;
        }
    }

    comments.sort_by_key(|comment| comment.created_date);
    Ok(comments)
}

async fn post_comment(
    ctx: &MigrationContext,
    target_client: &azure_devops_rust_api::wit::Client,
    target_id: i32,
    comment: &models::Comment,
) -> Result<()> {
    let body = annotated_comment(ctx, comment);
    if ctx.opts.dry_run {
        println!("  ⓘ would post comment to target work item #{target_id}");
        return Ok(());
    }

    let mut create = models::CommentCreate::new();
    create.text = Some(body);
    target_client
        .comments_client()
        .add_work_item_comment(
            ctx.target_creds.organization.clone(),
            create,
            ctx.opts.target_project.clone(),
            target_id,
            "Markdown",
        )
        .await
        .map_err(|e| anyhow!("posting comment to target work item {target_id}: {e}"))?;
    Ok(())
}

fn annotated_comment(ctx: &MigrationContext, comment: &models::Comment) -> String {
    let author = comment
        .created_by
        .as_ref()
        .map(|identity| serde_json::to_value(identity).unwrap_or(Value::Null))
        .and_then(|value| identity_to_plain_text(&value))
        .unwrap_or_else(|| "unknown".to_string());
    let date = comment
        .created_date
        .map(|date| date.to_string())
        .unwrap_or_else(|| "unknown date".to_string());
    let text = comment.text.as_deref().unwrap_or_default();
    let text = rewrite_attachment_references(text, ctx);
    format!("[Originally by {author} on {date}]\n\n{text}")
}
