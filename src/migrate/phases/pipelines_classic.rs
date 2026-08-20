use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::fs;

use crate::auth::factory::ClientFactory;
use crate::auth::url::release_base_url;
use crate::migrate::context::MigrationContext;
use crate::migrate::phase::{Phase, PhaseSummary};

pub struct PipelinesClassicPhase;

#[async_trait]
impl Phase for PipelinesClassicPhase {
    fn name(&self) -> &'static str {
        "pipelines_classic"
    }

    async fn execute(&self, ctx: &mut MigrationContext) -> Result<PhaseSummary> {
        let output_dir = ctx.output_dir.join("pipelines-classic");
        fs::create_dir_all(&output_dir)
            .with_context(|| format!("Creating output dir '{}'", output_dir.display()))?;

        let source_build_client = ctx.source_factory()?.build_build();
        let source_release_client = ctx.source_factory()?.build_release();
        let http = reqwest::Client::new();

        let build_refs = source_build_client
            .definitions_client()
            .list(&ctx.source_creds.organization, &ctx.opts.source_project)
            .include_all_properties(true)
            .await
            .map_err(|e| anyhow!("Listing source classic build definitions: {e}"))?
            .value;
        let release_refs = source_release_client
            .definitions_client()
            .list(&ctx.source_creds.organization, &ctx.opts.source_project)
            .await
            .map_err(|e| anyhow!("Listing source classic release definitions: {e}"))?
            .value;

        let mut summary = PhaseSummary {
            items_total: (build_refs.len() + release_refs.len()) as u64,
            ..Default::default()
        };

        for build_ref in build_refs {
            let _permit = ctx.executor.permit().await;
            let source_id = build_ref.definition_reference.id;
            let name = build_ref
                .definition_reference
                .name
                .clone()
                .unwrap_or_else(|| "unnamed".to_string());

            let definition = match source_build_client
                .definitions_client()
                .get(
                    &ctx.source_creds.organization,
                    &ctx.opts.source_project,
                    source_id,
                )
                .await
            {
                Ok(definition) => definition,
                Err(e) => {
                    summary.record_failure(format!(
                        "Reading classic build definition '{}': {e}",
                        name
                    ));
                    continue;
                }
            };

            let mut value = serde_json::to_value(&definition)?;
            if is_yaml_build_definition(&value) {
                summary.items_total = summary.items_total.saturating_sub(1);
                continue;
            }

            write_definition_export(&output_dir, "build", source_id, &name, &value)?;
            warn_for_classic_refs("classic build definition", &name, &value);
            remap_repository_references(ctx, &mut value);

            if ctx.opts.dry_run {
                println!("  ⓘ would migrate classic build definition '{}'", name);
                summary.record_success();
                continue;
            }

            let mut payload = value.clone();
            sanitize_build_definition_for_create(&mut payload);
            match post_ado_json(
                &http,
                &ctx.target_creds.organization,
                &ctx.target_creds.pat,
                &ctx.opts.target_project,
                "_apis/build/definitions?api-version=7.1",
                &payload,
                ctx.target_base_url(),
            )
            .await
            {
                Ok(created) => {
                    if let Some(target_id) = id_from_value(&created) {
                        ctx.state
                            .id_map_mut("pipelines_classic")
                            .map
                            .insert(format!("build:{source_id}"), format!("build:{target_id}"));
                        summary.record_success();
                    } else {
                        summary.record_failure(format!(
                            "Creating classic build definition '{}': target id missing in response",
                            name
                        ));
                    }
                }
                Err(e) => summary.record_failure(format!(
                    "Creating classic build definition '{}': {e:#}",
                    name
                )),
            }
        }

        for release_ref in release_refs {
            let _permit = ctx.executor.permit().await;
            let source_id = release_ref
                .release_definition_shallow_reference
                .id
                .unwrap_or_default();
            let name = release_ref
                .release_definition_shallow_reference
                .name
                .clone()
                .unwrap_or_else(|| "unnamed".to_string());

            let definition = match source_release_client
                .definitions_client()
                .get(
                    &ctx.source_creds.organization,
                    &ctx.opts.source_project,
                    source_id,
                )
                .await
            {
                Ok(definition) => definition,
                Err(e) => {
                    summary.record_failure(format!(
                        "Reading classic release definition '{}': {e}",
                        name
                    ));
                    continue;
                }
            };

            let mut value = serde_json::to_value(&definition)?;
            write_definition_export(&output_dir, "release", source_id, &name, &value)?;
            warn_for_classic_refs("classic release definition", &name, &value);
            remap_repository_references(ctx, &mut value);

            if ctx.opts.dry_run {
                println!("  ⓘ would migrate classic release definition '{}'", name);
                summary.record_success();
                continue;
            }

            let mut payload = value.clone();
            sanitize_release_definition_for_create(&mut payload);
            let release_host = release_base_url(ctx.target_base_url());
            match post_ado_json(
                &http,
                &ctx.target_creds.organization,
                &ctx.target_creds.pat,
                &ctx.opts.target_project,
                "_apis/release/definitions?api-version=7.1",
                &payload,
                &release_host,
            )
            .await
            {
                Ok(created) => {
                    if let Some(target_id) = id_from_value(&created) {
                        ctx.state.id_map_mut("pipelines_classic").map.insert(
                            format!("release:{source_id}"),
                            format!("release:{target_id}"),
                        );
                        summary.record_success();
                    } else {
                        summary.record_failure(format!(
                            "Creating classic release definition '{}': target id missing in response",
                            name
                        ));
                    }
                }
                Err(e) => summary.record_failure(format!(
                    "Creating classic release definition '{}': {e:#}",
                    name
                )),
            }
        }

        Ok(summary)
    }
}

fn write_definition_export(
    output_dir: &std::path::Path,
    kind: &str,
    id: i32,
    name: &str,
    value: &Value,
) -> Result<()> {
    let path = output_dir.join(format!("{}-{}-{}.json", kind, id, sanitize_filename(name)));
    fs::write(&path, serde_json::to_string_pretty(value)?)
        .with_context(|| format!("Writing classic pipeline export '{}'", path.display()))
}

fn warn_for_classic_refs(kind: &str, name: &str, value: &Value) {
    if contains_key_fragment(value, &["queue", "pool", "agentpool"]) {
        println!(
            "  ⚠ {kind} '{name}' references an agent pool or queue; verify it exists on target"
        );
    }
    if contains_service_connection_reference(value) {
        println!("  ⚠ {kind} '{name}' references a service connection; verify it exists on target");
    }
    if contains_task_guid_reference(value) {
        println!(
            "  ⚠ {kind} '{name}' references task GUIDs; verify tasks/extensions exist on target"
        );
    }
}

fn is_yaml_build_definition(value: &Value) -> bool {
    value
        .pointer("/process/yamlFilename")
        .and_then(Value::as_str)
        .is_some()
        || value
            .pointer("/process/type")
            .and_then(Value::as_i64)
            .is_some_and(|process_type| process_type == 2)
}

fn remap_repository_references(ctx: &MigrationContext, value: &mut Value) {
    let Some(repo_map) = ctx.state.id_map("repos") else {
        return;
    };
    remap_repository_references_recursive(value, &repo_map.map);
}

fn remap_repository_references_recursive(
    value: &mut Value,
    repo_map: &std::collections::HashMap<String, String>,
) {
    match value {
        Value::Object(map) => {
            if map
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|repo_type| repo_type.eq_ignore_ascii_case("TfsGit"))
            {
                if let Some(source_id) = map.get("id").and_then(Value::as_str) {
                    if let Some(target_id) = repo_map.get(source_id) {
                        map.insert("id".to_string(), Value::String(target_id.clone()));
                    }
                }
            }
            for child in map.values_mut() {
                remap_repository_references_recursive(child, repo_map);
            }
        }
        Value::Array(values) => {
            for child in values {
                remap_repository_references_recursive(child, repo_map);
            }
        }
        _ => {}
    }
}

fn sanitize_build_definition_for_create(value: &mut Value) {
    remove_object_keys(
        value,
        &[
            "_links",
            "authoredBy",
            "createdDate",
            "draftOf",
            "drafts",
            "id",
            "latestBuild",
            "latestCompletedBuild",
            "metrics",
            "project",
            "revision",
            "uri",
            "url",
        ],
    );
}

fn sanitize_release_definition_for_create(value: &mut Value) {
    remove_object_keys(
        value,
        &[
            "_links",
            "createdBy",
            "createdOn",
            "id",
            "isDeleted",
            "lastRelease",
            "modifiedBy",
            "modifiedOn",
            "projectReference",
            "revision",
            "url",
        ],
    );
}

fn remove_object_keys(value: &mut Value, keys: &[&str]) {
    if let Value::Object(map) = value {
        for key in keys {
            map.remove(*key);
        }
    }
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

fn contains_task_guid_reference(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, value)| {
            let key_matches =
                key.eq_ignore_ascii_case("task") || key.eq_ignore_ascii_case("taskId");
            (key_matches && value_contains_guid(value)) || contains_task_guid_reference(value)
        }),
        Value::Array(values) => values.iter().any(contains_task_guid_reference),
        _ => false,
    }
}

fn value_contains_guid(value: &Value) -> bool {
    match value {
        Value::String(value) => is_guid_like(value),
        Value::Object(map) => map.values().any(value_contains_guid),
        Value::Array(values) => values.iter().any(value_contains_guid),
        _ => false,
    }
}

fn is_guid_like(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && [8, 13, 18, 23].iter().all(|&idx| bytes[idx] == b'-')
        && bytes
            .iter()
            .enumerate()
            .all(|(idx, byte)| [8, 13, 18, 23].contains(&idx) || byte.is_ascii_hexdigit())
}

fn id_from_value(value: &Value) -> Option<i32> {
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
