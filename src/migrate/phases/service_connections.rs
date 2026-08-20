use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::json;
use std::fs;

use crate::auth::factory::ClientFactory;
use crate::migrate::context::MigrationContext;
use crate::migrate::phase::{Phase, PhaseSummary};

pub struct ServiceConnectionsPhase;

#[async_trait]
impl Phase for ServiceConnectionsPhase {
    fn name(&self) -> &'static str {
        "service_connections"
    }

    async fn execute(&self, ctx: &mut MigrationContext) -> Result<PhaseSummary> {
        let output_dir = ctx.output_dir.join("service-connections");
        fs::create_dir_all(&output_dir)
            .with_context(|| format!("Creating output dir '{}'", output_dir.display()))?;

        let source_client = ctx.source_factory()?.build_service_endpoint();
        let endpoints = source_client
            .endpoints_client()
            .get_service_endpoints(&ctx.source_creds.organization, &ctx.opts.source_project)
            .include_failed(true)
            .include_details(true)
            .await
            .map_err(|e| anyhow::anyhow!("Listing source service connections: {e}"))?
            .value;

        let mut summary = PhaseSummary {
            items_total: endpoints.len() as u64,
            ..Default::default()
        };
        let mut summary_items = Vec::with_capacity(endpoints.len());

        for endpoint in endpoints {
            let path = output_dir.join(format!(
                "{}-{}.json",
                sanitize_filename(&endpoint.id),
                sanitize_filename(&endpoint.name)
            ));
            fs::write(&path, serde_json::to_string_pretty(&endpoint)?).with_context(|| {
                format!("Writing service connection export '{}'", path.display())
            })?;

            summary_items.push(json!({
                "id": endpoint.id,
                "name": endpoint.name,
                "type": endpoint.type_,
                "file": path.file_name().and_then(|name| name.to_str()).unwrap_or_default(),
            }));
            summary.record_success();
        }

        let summary_path = output_dir.join("service-connections-summary.json");
        fs::write(
            &summary_path,
            serde_json::to_string_pretty(&json!({ "service_connections": summary_items }))?,
        )
        .with_context(|| {
            format!(
                "Writing service connections summary '{}'",
                summary_path.display()
            )
        })?;

        println!(
            "  ⚠ exported {} service connections; manual reconfiguration required on target",
            summary.items_done
        );

        Ok(summary)
    }
}

fn sanitize_filename(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();

    if sanitized.is_empty() {
        "unnamed".to_string()
    } else {
        sanitized
    }
}
