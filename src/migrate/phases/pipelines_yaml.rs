use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use azure_devops_rust_api::pipelines::models::{pipeline_configuration, Pipeline};
use serde_json::{json, Value};

use crate::auth::factory::ClientFactory;
use crate::migrate::context::MigrationContext;
use crate::migrate::phase::{Phase, PhaseSummary};

pub struct PipelinesYamlPhase;

#[async_trait]
impl Phase for PipelinesYamlPhase {
    fn name(&self) -> &'static str {
        "pipelines_yaml"
    }

    async fn execute(&self, ctx: &mut MigrationContext) -> Result<PhaseSummary> {
        let source_client = ctx.source_factory().build_pipelines();
        let source_build_client = ctx.source_factory().build_build();
        let target_build_client = ctx.target_factory().build_build();
        let http = reqwest::Client::new();

        let listed = ctx
            .executor
            .retry(|| async {
                source_client
                    .pipelines_client()
                    .list(&ctx.source_creds.organization, &ctx.opts.source_project)
                    .await
                    .map(|r| r.value)
                    .map_err(|e| anyhow!("Listing source YAML pipelines: {e}"))
            })
            .await?;

        let mut pipelines = Vec::new();
        for item in listed {
            let pipeline = source_client
                .pipelines_client()
                .get(
                    &ctx.source_creds.organization,
                    &ctx.opts.source_project,
                    item.id,
                )
                .await
                .unwrap_or(item);
            if is_yaml_pipeline(&pipeline) {
                pipelines.push(pipeline);
            }
        }

        let mut summary = PhaseSummary {
            items_total: pipelines.len() as u64,
            ..Default::default()
        };

        for pipeline in pipelines {
            let _permit = ctx.executor.permit().await;
            let source_id = pipeline.id.to_string();
            let name = pipeline.name.clone();

            let mut source_build_definition = match source_build_client
                .definitions_client()
                .get(
                    &ctx.source_creds.organization,
                    &ctx.opts.source_project,
                    pipeline.id,
                )
                .await
            {
                Ok(definition) => Some(definition),
                Err(e) => {
                    println!(
                        "  ⚠ could not inspect YAML pipeline '{}' build definition for variable groups: {e}",
                        name
                    );
                    None
                }
            };

            if pipeline_references_service_connection(&pipeline)
                || source_build_definition
                    .as_ref()
                    .and_then(|definition| serde_json::to_value(definition).ok())
                    .is_some_and(|value| contains_service_connection_reference(&value))
            {
                println!(
                    "  ⚠ YAML pipeline '{}' references a service connection; remap is not available",
                    name
                );
            }

            if let Some(definition) = source_build_definition.as_mut() {
                remap_variable_groups(ctx, &name, &mut definition.variable_groups);
            }

            if ctx.opts.dry_run {
                println!("  ⓘ would migrate YAML pipeline '{}'", name);
                summary.record_success();
                continue;
            }

            match create_yaml_pipeline(ctx, &http, &pipeline).await {
                Ok(created) => {
                    let target_id = pipeline_id_from_value(&created).ok_or_else(|| {
                        anyhow!(
                            "Creating YAML pipeline '{}': target id missing in response",
                            name
                        )
                    });

                    match target_id {
                        Ok(target_id) => {
                            if let Some(definition) = source_build_definition {
                                if let Err(e) = apply_variable_groups_to_target(
                                    ctx,
                                    &target_build_client,
                                    target_id,
                                    definition.variable_groups,
                                )
                                .await
                                {
                                    summary.record_failure(format!(
                                        "YAML pipeline '{}': applying variable groups: {e:#}",
                                        name
                                    ));
                                    continue;
                                }
                            }

                            ctx.state
                                .id_map_mut("pipelines")
                                .map
                                .insert(source_id, target_id.to_string());
                            summary.record_success();
                        }
                        Err(e) => summary.record_failure(e.to_string()),
                    }
                }
                Err(e) => summary.record_failure(format!("YAML pipeline '{}': {e:#}", name)),
            }
        }

        Ok(summary)
    }
}

fn is_yaml_pipeline(pipeline: &Pipeline) -> bool {
    pipeline
        .configuration
        .as_ref()
        .map(|config| matches!(config.type_, pipeline_configuration::Type::Yaml))
        .unwrap_or(false)
}

async fn create_yaml_pipeline(
    ctx: &MigrationContext,
    http: &reqwest::Client,
    pipeline: &Pipeline,
) -> Result<Value> {
    let configuration = pipeline
        .configuration
        .as_ref()
        .ok_or_else(|| anyhow!("pipeline configuration missing"))?;
    let source_repo_id = configuration
        .repository
        .id
        .as_deref()
        .ok_or_else(|| anyhow!("pipeline repository id missing"))?;
    let target_repo_id = ctx
        .state
        .id_map("repos")
        .and_then(|map| map.map.get(source_repo_id))
        .ok_or_else(|| {
            anyhow!(
                "repository id '{}' has no migrated target mapping",
                source_repo_id
            )
        })?;

    let repository_type = configuration
        .repository
        .type_
        .clone()
        .unwrap_or_else(|| "azureReposGit".to_string());
    let body = json!({
        "name": pipeline.name,
        "folder": pipeline.folder,
        "configuration": {
            "type": "yaml",
            "path": configuration.path,
            "repository": {
                "id": target_repo_id,
                "type": repository_type
            }
        }
    });

    post_ado_json(
        http,
        &ctx.target_creds.organization,
        &ctx.target_creds.pat,
        &ctx.opts.target_project,
        "_apis/pipelines?api-version=7.1-preview.1",
        &body,
        ctx.target_base_url(),
    )
    .await
}

fn remap_variable_groups(
    ctx: &MigrationContext,
    pipeline_name: &str,
    groups: &mut [azure_devops_rust_api::build::models::VariableGroup],
) {
    let map = ctx.state.id_map("variable_groups");
    for group in groups {
        let Some(source_id) = group.variable_group_reference.id else {
            continue;
        };
        let source_id_string = source_id.to_string();
        match map.and_then(|map| map.map.get(&source_id_string)) {
            Some(target_id) => match target_id.parse::<i32>() {
                Ok(target_id) => group.variable_group_reference.id = Some(target_id),
                Err(_) => println!(
                    "  ⚠ YAML pipeline '{}' variable group '{}' target id '{}' is not numeric; leaving original id",
                    pipeline_name, source_id, target_id
                ),
            },
            None => println!(
                "  ⚠ YAML pipeline '{}' variable group '{}' has no migrated target mapping; leaving original id",
                pipeline_name, source_id
            ),
        }
    }
}

async fn apply_variable_groups_to_target(
    ctx: &MigrationContext,
    target_build_client: &azure_devops_rust_api::build::Client,
    target_id: i32,
    variable_groups: Vec<azure_devops_rust_api::build::models::VariableGroup>,
) -> Result<()> {
    if variable_groups.is_empty() {
        return Ok(());
    }

    let mut target_definition = target_build_client
        .definitions_client()
        .get(
            &ctx.target_creds.organization,
            &ctx.opts.target_project,
            target_id,
        )
        .await
        .map_err(|e| anyhow!("reading created target definition {target_id}: {e}"))?;
    target_definition.variable_groups = variable_groups;
    target_build_client
        .definitions_client()
        .update(
            &ctx.target_creds.organization,
            target_definition,
            &ctx.opts.target_project,
            target_id,
        )
        .await
        .map_err(|e| anyhow!("updating created target definition {target_id}: {e}"))?;
    Ok(())
}

fn pipeline_references_service_connection(pipeline: &Pipeline) -> bool {
    serde_json::to_value(pipeline)
        .map(|value| contains_service_connection_reference(&value))
        .unwrap_or(false)
}

fn contains_service_connection_reference(value: &Value) -> bool {
    contains_key_fragment(
        value,
        &["serviceconnection", "serviceendpoint", "connectedservice"],
    )
}

fn contains_key_fragment(value: &Value, fragments: &[&str]) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, value)| {
            let key = key.to_ascii_lowercase();
            fragments.iter().any(|fragment| key.contains(fragment))
                || contains_key_fragment(value, fragments)
        }),
        Value::Array(values) => values
            .iter()
            .any(|value| contains_key_fragment(value, fragments)),
        _ => false,
    }
}

fn pipeline_id_from_value(value: &Value) -> Option<i32> {
    value
        .get("id")
        .and_then(Value::as_i64)
        .and_then(|id| i32::try_from(id).ok())
}

async fn post_ado_json(
    http: &reqwest::Client,
    organization: &str,
    pat: &str,
    project: &str,
    path_and_query: &str,
    body: &Value,
    host: &str,
) -> Result<Value> {
    let url = format!(
        "{}/{}/{}/{}",
        host,
        percent_encode(organization),
        percent_encode(project),
        path_and_query
    );
    let response = http
        .post(url)
        .basic_auth("azdocli", Some(pat))
        .json(body)
        .send()
        .await
        .context("sending Azure DevOps create request")?;
    let status = response.status();
    let text = response
        .text()
        .await
        .context("reading Azure DevOps create response")?;
    if !status.is_success() {
        return Err(anyhow!("Azure DevOps create returned {status}: {text}"));
    }
    serde_json::from_str(&text).with_context(|| format!("Parsing Azure DevOps response: {text}"))
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}
