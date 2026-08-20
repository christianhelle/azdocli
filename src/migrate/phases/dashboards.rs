use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use azure_devops_rust_api::core::models::WebApiTeam;
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::auth::factory::ClientFactory;
use crate::migrate::context::MigrationContext;
use crate::migrate::phase::{Phase, PhaseSummary};

pub struct DashboardsPhase;

#[async_trait]
impl Phase for DashboardsPhase {
    fn name(&self) -> &'static str {
        "dashboards"
    }

    async fn execute(&self, ctx: &mut MigrationContext) -> Result<PhaseSummary> {
        let output_dir = ctx.output_dir.join("dashboards");
        fs::create_dir_all(&output_dir)
            .with_context(|| format!("Creating output dir '{}'", output_dir.display()))?;

        if ctx.opts.dry_run {
            println!("  ⓘ dry-run: exporting dashboards only; no target changes will be made");
        }

        let core_client = ctx.source_factory().build_core();
        let teams = match list_project_teams(
            &core_client.teams_client(),
            &ctx.source_creds.organization,
            &ctx.opts.source_project,
        )
        .await
        {
            Ok(teams) => teams,
            Err(e) => {
                // Export-only v1.0 phase: team listing is the SDK path to dashboards;
                // record the limitation and continue the migration run without target writes.
                let mut summary = PhaseSummary {
                    items_total: 1,
                    ..Default::default()
                };
                summary.record_failure(format!("Listing source teams for dashboard export: {e}"));
                println!(
                    "  ⓘ Dashboards exported to {}. Manual import or v1.1 migration required.",
                    output_dir.display()
                );
                return Ok(summary);
            }
        };

        let client = Client::new();
        let mut dashboard_refs = Vec::new();
        let mut list_errors = Vec::new();
        let mut seen = HashSet::new();

        for team in teams {
            let (team_id, team_name) = match team_id_and_name(&team) {
                Ok(values) => values,
                Err(e) => {
                    println!("  ⚠ skipping dashboard team lookup: {e}");
                    continue;
                }
            };

            let dashboards = match list_dashboards_for_team(&client, ctx, &team_id).await {
                Ok(dashboards) => dashboards,
                Err(_) => match list_dashboards_for_team(&client, ctx, &team_name).await {
                    Ok(dashboards) => dashboards,
                    Err(e) => {
                        list_errors.push(format!("Listing dashboards for team '{team_name}': {e}"));
                        continue;
                    }
                },
            };

            for dashboard in dashboards {
                let Some(dashboard_id) = json_string(&dashboard, "id") else {
                    continue;
                };
                if seen.insert(format!("{team_id}/{dashboard_id}")) {
                    dashboard_refs.push(DashboardRef {
                        team_id: team_id.clone(),
                        team_name: team_name.clone(),
                        dashboard_id,
                        listed_dashboard: dashboard,
                    });
                }
            }
        }

        let mut summary = PhaseSummary {
            items_total: (dashboard_refs.len() + list_errors.len()) as u64,
            ..Default::default()
        };
        for error in list_errors {
            summary.record_failure(error);
        }

        for dashboard_ref in dashboard_refs {
            match export_dashboard(&client, ctx, &output_dir, &dashboard_ref).await {
                Ok(()) => summary.record_success(),
                Err(e) => summary.record_failure(format!(
                    "Exporting dashboard '{}' for team '{}': {e}",
                    dashboard_ref.dashboard_id, dashboard_ref.team_name
                )),
            }
        }

        println!(
            "  ⓘ Dashboards exported to {}. Manual import or v1.1 migration required.",
            output_dir.display()
        );

        Ok(summary)
    }
}

struct DashboardRef {
    team_id: String,
    team_name: String,
    dashboard_id: String,
    listed_dashboard: Value,
}

async fn list_project_teams(
    client: &azure_devops_rust_api::core::teams::Client,
    organization: &str,
    project: &str,
) -> Result<Vec<WebApiTeam>> {
    let page_size = 1000;
    let mut skip = 0;
    let mut teams = Vec::new();

    loop {
        let page = client
            .get_teams(organization, project)
            .top(page_size)
            .skip(skip)
            .await
            .map_err(|e| anyhow!("Listing source teams for dashboard export: {e}"))?
            .value;
        let page_len = page.len();
        teams.extend(page);

        if page_len < page_size as usize {
            break;
        }
        skip += page_size;
    }

    Ok(teams)
}

async fn list_dashboards_for_team(
    client: &Client,
    ctx: &MigrationContext,
    team: &str,
) -> Result<Vec<Value>> {
    // The Azure DevOps dashboard SDK exposes team-scoped routes only; project-only
    // dashboards, if returned by the service separately, require a future API pass.
    let url = format!(
        "{}/{}/{}/{}/_apis/dashboard/dashboards",
        ctx.source_base_url(),
        percent_encode_path_segment(&ctx.source_creds.organization),
        percent_encode_path_segment(&ctx.opts.source_project),
        percent_encode_path_segment(team)
    );

    let body = get_json(
        client,
        &ctx.source_creds.pat,
        &url,
        &[("api-version", "7.1-preview")],
    )
    .await?;

    Ok(body
        .get("value")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

async fn export_dashboard(
    client: &Client,
    ctx: &MigrationContext,
    output_dir: &Path,
    dashboard_ref: &DashboardRef,
) -> Result<()> {
    let dashboard: Value = match get_dashboard(
        client,
        ctx,
        &dashboard_ref.team_id,
        &dashboard_ref.dashboard_id,
    )
    .await
    {
        Ok(dashboard) => dashboard,
        Err(_) => {
            get_dashboard(
                client,
                ctx,
                &dashboard_ref.team_name,
                &dashboard_ref.dashboard_id,
            )
            .await?
        }
    };
    let widgets: Value = match get_widgets(
        client,
        ctx,
        &dashboard_ref.team_id,
        &dashboard_ref.dashboard_id,
    )
    .await
    {
        Ok(widgets) => widgets,
        Err(_) => {
            get_widgets(
                client,
                ctx,
                &dashboard_ref.team_name,
                &dashboard_ref.dashboard_id,
            )
            .await?
        }
    };

    let export = json!({
        "team": {
            "id": dashboard_ref.team_id,
            "name": dashboard_ref.team_name,
        },
        "dashboard": dashboard,
        "listedDashboard": dashboard_ref.listed_dashboard,
        "widgets": widgets,
        "exportOnly": true,
    });
    let path = output_dir.join(format!(
        "dashboard-{}.json",
        sanitize_filename(&dashboard_ref.dashboard_id)
    ));
    fs::write(&path, serde_json::to_string_pretty(&export)?)
        .with_context(|| format!("Writing dashboard export '{}'", path.display()))?;

    Ok(())
}

async fn get_dashboard(
    client: &Client,
    ctx: &MigrationContext,
    team: &str,
    dashboard_id: &str,
) -> Result<Value> {
    let url = format!(
        "{}/{}/{}/{}/_apis/dashboard/dashboards/{}",
        ctx.source_base_url(),
        percent_encode_path_segment(&ctx.source_creds.organization),
        percent_encode_path_segment(&ctx.opts.source_project),
        percent_encode_path_segment(team),
        percent_encode_path_segment(dashboard_id)
    );

    get_json(
        client,
        &ctx.source_creds.pat,
        &url,
        &[("api-version", "7.1-preview")],
    )
    .await
}

async fn get_widgets(
    client: &Client,
    ctx: &MigrationContext,
    team: &str,
    dashboard_id: &str,
) -> Result<Value> {
    let url = format!(
        "{}/{}/{}/{}/_apis/dashboard/dashboards/{}/widgets",
        ctx.source_base_url(),
        percent_encode_path_segment(&ctx.source_creds.organization),
        percent_encode_path_segment(&ctx.opts.source_project),
        percent_encode_path_segment(team),
        percent_encode_path_segment(dashboard_id)
    );

    get_json(
        client,
        &ctx.source_creds.pat,
        &url,
        &[("api-version", "7.1-preview")],
    )
    .await
}

async fn get_json(client: &Client, pat: &str, url: &str, query: &[(&str, &str)]) -> Result<Value> {
    Ok(client
        .get(url)
        .query(query)
        .basic_auth("", Some(pat))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?)
}

fn team_id_and_name(team: &WebApiTeam) -> Result<(String, String)> {
    let name = team
        .web_api_team_ref
        .name
        .clone()
        .ok_or_else(|| anyhow!("Source team is missing a name"))?;
    let id = team
        .web_api_team_ref
        .id
        .clone()
        .ok_or_else(|| anyhow!("Source team '{name}' is missing an id"))?;
    Ok((id, name))
}

fn json_string(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_string)
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

fn percent_encode_path_segment(segment: &str) -> String {
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
