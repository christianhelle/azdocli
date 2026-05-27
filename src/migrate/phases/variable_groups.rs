use anyhow::{Context, Result};
use async_trait::async_trait;
use azure_devops_rust_api::core::ClientBuilder as CoreClientBuilder;
use azure_devops_rust_api::distributed_task::{
    models::{
        ProjectReference, VariableGroup, VariableGroupParameters, VariableGroupProjectReference,
    },
    ClientBuilder as DistributedTaskClientBuilder,
};
use serde_json::Value;
use std::fs;

use crate::migrate::context::MigrationContext;
use crate::migrate::phase::{Phase, PhaseSummary};

pub struct VariableGroupsPhase;

#[async_trait]
impl Phase for VariableGroupsPhase {
    fn name(&self) -> &'static str {
        "variable_groups"
    }

    async fn execute(&self, ctx: &mut MigrationContext) -> Result<PhaseSummary> {
        let output_dir = ctx.output_dir.join("variable-groups");
        fs::create_dir_all(&output_dir)
            .with_context(|| format!("Creating output dir '{}'", output_dir.display()))?;

        let source_client =
            DistributedTaskClientBuilder::new(ctx.source_credential.clone()).build();
        let groups = source_client
            .variablegroups_client()
            .get_variable_groups(&ctx.source_creds.organization, &ctx.opts.source_project)
            .await
            .map_err(|e| anyhow::anyhow!("Listing source variable groups: {e}"))?
            .value;

        let mut summary = PhaseSummary {
            items_total: groups.len() as u64,
            ..Default::default()
        };

        let target_project_ref = if ctx.opts.dry_run {
            None
        } else {
            Some(target_project_reference(ctx).await?)
        };

        let target_client =
            DistributedTaskClientBuilder::new(ctx.target_credential.clone()).build();

        for group in groups {
            let source_id = group
                .id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            let name = group.name.as_deref().unwrap_or("unnamed");
            let path = output_dir.join(format!("{}-{}.json", source_id, sanitize_filename(name)));
            fs::write(&path, serde_json::to_string_pretty(&group)?)
                .with_context(|| format!("Writing variable group export '{}'", path.display()))?;

            if ctx.opts.dry_run {
                summary.record_success();
                continue;
            }

            let mut payload = to_parameters(&group, target_project_ref.clone());
            blank_secret_values(name, payload.variables.as_mut());

            match target_client
                .variablegroups_client()
                .add(&ctx.target_creds.organization, payload)
                .await
            {
                Ok(created) => {
                    if let Some(target_id) = created.id {
                        ctx.state
                            .id_map_mut("variable_groups")
                            .map
                            .insert(source_id, target_id.to_string());
                        summary.record_success();
                    } else {
                        summary.record_failure(format!(
                            "Creating variable group '{}': target id missing in response",
                            name
                        ));
                    }
                }
                Err(e) => {
                    summary.record_failure(format!("Creating variable group '{}': {e}", name))
                }
            }
        }

        Ok(summary)
    }
}

async fn target_project_reference(ctx: &MigrationContext) -> Result<ProjectReference> {
    let target_core = CoreClientBuilder::new(ctx.target_credential.clone()).build();
    let projects = target_core
        .projects_client()
        .list(&ctx.target_creds.organization)
        .await
        .map_err(|e| anyhow::anyhow!("Listing target projects for variable group references: {e}"))?
        .value;

    let target = projects
        .iter()
        .find(|project| project.name == ctx.opts.target_project);

    let mut reference = ProjectReference::new();
    reference.name = Some(ctx.opts.target_project.clone());
    reference.id = target.and_then(|project| project.id.clone());
    Ok(reference)
}

fn to_parameters(
    group: &VariableGroup,
    target_project_ref: Option<ProjectReference>,
) -> VariableGroupParameters {
    let mut parameters = VariableGroupParameters::new();
    parameters.description = group.description.clone();
    parameters.name = group.name.clone();
    parameters.provider_data = group.provider_data.clone();
    parameters.type_ = group.type_.clone();
    parameters.variables = group.variables.clone();

    if let Some(project_reference) = target_project_ref {
        let mut reference = VariableGroupProjectReference::new();
        reference.description = group.description.clone();
        reference.name = group.name.clone();
        reference.project_reference = Some(project_reference);
        parameters.variable_group_project_references = vec![reference];
    }

    parameters
}

fn blank_secret_values(group_name: &str, variables: Option<&mut Value>) {
    let Some(Value::Object(map)) = variables else {
        return;
    };

    for (name, variable) in map {
        let Value::Object(fields) = variable else {
            continue;
        };

        let is_secret = fields
            .get("isSecret")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if is_secret {
            fields.insert("value".to_string(), Value::String(String::new()));
            println!(
                "  ⚠ blanked secret variable '{}' in variable group '{}'",
                name, group_name
            );
        }
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
