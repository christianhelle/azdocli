use anyhow::{anyhow, Result};
use async_trait::async_trait;
use azure_devops_rust_api::wit::{models, ClientBuilder as WitClientBuilder};

use crate::migrate::context::MigrationContext;
use crate::migrate::phase::{Phase, PhaseSummary};

use super::work_item_common::{
    add_op, get_source_work_item, json_relation, mapped_work_item_id, target_work_item_url,
    WORK_ITEM_MAP,
};

pub struct WiLinksPhase;

#[async_trait]
impl Phase for WiLinksPhase {
    fn name(&self) -> &'static str {
        "wi_links"
    }

    async fn execute(&self, ctx: &mut MigrationContext) -> Result<PhaseSummary> {
        let mappings = ctx
            .state
            .id_map(WORK_ITEM_MAP)
            .map(|map| map.map.clone())
            .unwrap_or_default();
        let source_ids = mappings
            .keys()
            .filter_map(|id| id.parse::<i32>().ok())
            .collect::<Vec<_>>();
        let mut summary = PhaseSummary {
            items_total: source_ids.len() as u64,
            ..Default::default()
        };

        let source_client = WitClientBuilder::new(ctx.source_credential.clone()).build();
        let target_client = WitClientBuilder::new(ctx.target_credential.clone()).build();

        for source_id in source_ids {
            let _permit = ctx.executor.permit().await;
            match migrate_links_for_work_item(ctx, &source_client, &target_client, source_id).await
            {
                Ok(count) => {
                    if count == 0 {
                        println!("  ⓘ no internal links for work item #{source_id}");
                    }
                    summary.record_success();
                }
                Err(e) => summary.record_failure(format!("Work item links {source_id}: {e:#}")),
            }
        }

        Ok(summary)
    }
}

async fn migrate_links_for_work_item(
    ctx: &MigrationContext,
    source_client: &azure_devops_rust_api::wit::Client,
    target_client: &azure_devops_rust_api::wit::Client,
    source_id: i32,
) -> Result<usize> {
    let Some(target_id) = mapped_work_item_id(ctx, source_id) else {
        return Err(anyhow!(
            "source work item id is missing from work item id map"
        ));
    };
    let target_id = target_id.parse::<i32>()?;
    let source_item = get_source_work_item(ctx, source_client, source_id).await?;
    let mut added = 0usize;

    for relation in &source_item.relations {
        if !is_work_item_link(relation) {
            continue;
        }
        let Some(linked_source_id) = work_item_id_from_url(&relation.link.url) else {
            println!(
                "  ⚠ skipping unsupported relation URL on work item #{}: {}",
                source_id, relation.link.url
            );
            continue;
        };
        let Some(linked_target_id) = mapped_work_item_id(ctx, linked_source_id) else {
            println!(
                "  ⚠ skipping cross-project/external relation from #{} to #{}",
                source_id, linked_source_id
            );
            continue;
        };

        if ctx.opts.dry_run {
            added += 1;
            continue;
        }

        let target_url = target_work_item_url(ctx, &linked_target_id);
        let patch = vec![add_op(
            "/relations/-",
            json_relation(
                &relation.link.rel,
                &target_url,
                relation.link.attributes.clone(),
            ),
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
            .map_err(|e| {
                anyhow!(
                    "adding {} link to target work item {target_id}: {e}",
                    relation.link.rel
                )
            })?;
        added += 1;
    }

    Ok(added)
}

fn is_work_item_link(relation: &models::WorkItemRelation) -> bool {
    relation.link.rel.starts_with("System.LinkTypes.")
}

fn work_item_id_from_url(url: &str) -> Option<i32> {
    let id = url.rsplit('/').next()?;
    id.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use azure_devops_rust_api::wit::models::{Link, WorkItemRelation};
    use serde_json::json;

    #[test]
    fn work_item_id_from_url_reads_last_segment() {
        assert_eq!(
            work_item_id_from_url("https://dev.azure.com/org/proj/_apis/wit/workItems/42"),
            Some(42)
        );
    }

    #[test]
    fn is_work_item_link_filters_attachment_links() {
        let work_link = WorkItemRelation::new(Link::new(
            json!({}),
            "System.LinkTypes.Related".to_string(),
            "url".to_string(),
        ));
        let attachment = WorkItemRelation::new(Link::new(
            json!({}),
            "AttachedFile".to_string(),
            "url".to_string(),
        ));

        assert!(is_work_item_link(&work_link));
        assert!(!is_work_item_link(&attachment));
    }
}
