use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

use crate::migrate::context::MigrationContext;
use crate::migrate::phase::{Phase, PhaseSummary};

pub struct TestPlansPhase;

#[async_trait]
impl Phase for TestPlansPhase {
    fn name(&self) -> &'static str {
        "test_plans"
    }

    async fn execute(&self, ctx: &mut MigrationContext) -> Result<PhaseSummary> {
        let output_dir = ctx.output_dir.join("test-plans");
        fs::create_dir_all(&output_dir)
            .with_context(|| format!("Creating output dir '{}'", output_dir.display()))?;

        if ctx.opts.dry_run {
            println!("  ⓘ dry-run: exporting test plans only; no target changes will be made");
        }

        let client = Client::new();
        let plans = match list_test_plans(&client, ctx).await {
            Ok(plans) => plans,
            Err(e) => {
                // Export-only v1.0 phase: listing failure is recorded so the orchestrator can
                // continue and the operator can retry/export manually after fixing credentials.
                let mut summary = PhaseSummary {
                    items_total: 1,
                    ..Default::default()
                };
                summary.record_failure(format!("Listing source test plans: {e}"));
                println!(
                    "  ⓘ Test plans exported to {}. Manual import or v1.1 migration required.",
                    output_dir.display()
                );
                return Ok(summary);
            }
        };
        let mut summary = PhaseSummary {
            items_total: plans.len() as u64,
            ..Default::default()
        };

        for plan in plans {
            let plan_id = match json_i32(&plan, "id") {
                Some(id) => id,
                None => {
                    summary.record_failure("Exporting test plan: plan did not include an id");
                    continue;
                }
            };

            match export_test_plan(&client, ctx, &output_dir, plan_id, plan).await {
                Ok(()) => summary.record_success(),
                Err(e) => summary.record_failure(format!("Exporting test plan {plan_id}: {e}")),
            }
        }

        println!(
            "  ⓘ Test plans exported to {}. Manual import or v1.1 migration required.",
            output_dir.display()
        );

        Ok(summary)
    }
}

async fn list_test_plans(client: &Client, ctx: &MigrationContext) -> Result<Vec<Value>> {
    let url = format!(
        "https://dev.azure.com/{}/{}/_apis/testplan/plans",
        percent_encode_path_segment(&ctx.source_creds.organization),
        percent_encode_path_segment(&ctx.opts.source_project)
    );

    get_paginated_values(
        client,
        &ctx.source_creds.pat,
        &url,
        &[
            ("api-version", "7.1-preview"),
            ("includePlanDetails", "true"),
        ],
    )
    .await
    .context("Listing source test plans")
}

async fn export_test_plan(
    client: &Client,
    ctx: &MigrationContext,
    output_dir: &Path,
    plan_id: i32,
    listed_plan: Value,
) -> Result<()> {
    let plan = get_json(
        client,
        &ctx.source_creds.pat,
        &format!(
            "https://dev.azure.com/{}/{}/_apis/testplan/plans/{}",
            percent_encode_path_segment(&ctx.source_creds.organization),
            percent_encode_path_segment(&ctx.opts.source_project),
            plan_id
        ),
        &[("api-version", "7.1-preview")],
    )
    .await
    .with_context(|| format!("Reading test plan {plan_id}"))?;

    let suites = list_suites(client, ctx, plan_id).await?;
    let suite_ids = collect_suite_ids(&suites);
    let mut test_cases_by_suite = Vec::with_capacity(suite_ids.len());
    for suite_id in suite_ids {
        match list_test_cases(client, ctx, plan_id, suite_id).await {
            Ok(test_cases) => {
                test_cases_by_suite.push(json!({ "suiteId": suite_id, "testCases": test_cases }));
            }
            Err(e) => {
                test_cases_by_suite
                    .push(json!({ "suiteId": suite_id, "testCasesError": e.to_string() }));
            }
        }
    }

    let export = json!({
        "plan": plan,
        "listedPlan": listed_plan,
        "suites": suites,
        "testCasesBySuite": test_cases_by_suite,
        "exportOnly": true,
    });
    let path = output_dir.join(format!("plan-{plan_id}.json"));
    fs::write(&path, serde_json::to_string_pretty(&export)?)
        .with_context(|| format!("Writing test plan export '{}'", path.display()))?;

    Ok(())
}

fn collect_suite_ids(suites: &[Value]) -> Vec<i32> {
    let mut ids = Vec::new();
    for suite in suites {
        collect_suite_ids_from_value(suite, &mut ids);
    }
    ids
}

fn collect_suite_ids_from_value(suite: &Value, ids: &mut Vec<i32>) {
    if let Some(id) = json_i32(suite, "id") {
        ids.push(id);
    }

    if let Some(children) = suite.get("children").and_then(Value::as_array) {
        for child in children {
            collect_suite_ids_from_value(child, ids);
        }
    }
}

async fn list_suites(client: &Client, ctx: &MigrationContext, plan_id: i32) -> Result<Vec<Value>> {
    let url = format!(
        "https://dev.azure.com/{}/{}/_apis/testplan/Plans/{}/suites",
        percent_encode_path_segment(&ctx.source_creds.organization),
        percent_encode_path_segment(&ctx.opts.source_project),
        plan_id
    );

    get_paginated_values(
        client,
        &ctx.source_creds.pat,
        &url,
        &[
            ("api-version", "7.1-preview"),
            ("expand", "Children"),
            ("asTreeView", "true"),
        ],
    )
    .await
    .with_context(|| format!("Listing suites for test plan {plan_id}"))
}

async fn list_test_cases(
    client: &Client,
    ctx: &MigrationContext,
    plan_id: i32,
    suite_id: i32,
) -> Result<Vec<Value>> {
    let url = format!(
        "https://dev.azure.com/{}/{}/_apis/testplan/Plans/{}/Suites/{}/TestCase",
        percent_encode_path_segment(&ctx.source_creds.organization),
        percent_encode_path_segment(&ctx.opts.source_project),
        plan_id,
        suite_id
    );

    get_paginated_values(
        client,
        &ctx.source_creds.pat,
        &url,
        &[
            ("api-version", "7.1-preview"),
            ("expand", "true"),
            ("isRecursive", "false"),
        ],
    )
    .await
    .with_context(|| format!("Listing test cases for suite {suite_id}"))
}

async fn get_paginated_values(
    client: &Client,
    pat: &str,
    url: &str,
    query: &[(&str, &str)],
) -> Result<Vec<Value>> {
    let mut continuation_token: Option<String> = None;
    let mut values = Vec::new();

    loop {
        let mut request = client.get(url).query(query).basic_auth("", Some(pat));
        if let Some(token) = &continuation_token {
            request = request.query(&[("continuationToken", token)]);
        }

        let response = request.send().await?.error_for_status()?;
        continuation_token = response
            .headers()
            .get("x-ms-continuationtoken")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let body = response.json::<Value>().await?;
        values.extend(
            body.get("value")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
        );

        if continuation_token.is_none() {
            break;
        }
    }

    Ok(values)
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

fn json_i32(value: &Value, key: &str) -> Option<i32> {
    value
        .get(key)?
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
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
