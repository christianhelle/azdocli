//! Preflight checks. Validates credentials, target tenant reachability,
//! and project state before any writes.

#![allow(dead_code)]

use anyhow::{anyhow, Result};
use azure_devops_rust_api::core::ClientBuilder as CoreClientBuilder;

use super::context::MigrationContext;

pub struct PreflightReport {
    pub source_project_exists: bool,
    pub target_project_exists: bool,
    pub warnings: Vec<String>,
}

pub async fn run(ctx: &MigrationContext) -> Result<PreflightReport> {
    let mut warnings = Vec::new();

    // Source project must exist.
    let source_client = CoreClientBuilder::new(ctx.source_credential.clone()).build();
    let source_projects = source_client
        .projects_client()
        .list(&ctx.source_creds.organization)
        .await
        .map_err(|e| {
            anyhow!(
                "Failed to reach source org '{}': {e}",
                ctx.source_creds.organization
            )
        })?
        .value;
    let source_project_exists = source_projects
        .iter()
        .any(|p| p.name == ctx.opts.source_project);
    if !source_project_exists {
        return Err(anyhow!(
            "Source project '{}' not found in organization '{}'",
            ctx.opts.source_project,
            ctx.source_creds.organization
        ));
    }

    // Target org must be reachable; record whether the project exists.
    let target_client = CoreClientBuilder::new(ctx.target_credential.clone()).build();
    let target_projects = target_client
        .projects_client()
        .list(&ctx.target_creds.organization)
        .await
        .map_err(|e| {
            anyhow!(
                "Failed to reach target org '{}': {e}",
                ctx.target_creds.organization
            )
        })?
        .value;
    let target_project_exists = target_projects
        .iter()
        .any(|p| p.name == ctx.opts.target_project);

    if target_project_exists && ctx.opts.create_target {
        warnings.push(format!(
            "Target project '{}' already exists; --create-target will be a no-op",
            ctx.opts.target_project
        ));
    }
    if !target_project_exists && !ctx.opts.create_target {
        return Err(anyhow!(
            "Target project '{}' does not exist in '{}'. Use --create-target to create it.",
            ctx.opts.target_project,
            ctx.target_creds.organization
        ));
    }

    // Many ADO assets require `bypassRules` permission. The SDK does not
    // expose a probe-only call; we surface a warning instead of a hard
    // failure here so dry-run remains safe.
    warnings.push(
        "Work item migration uses bypassRules; the target PAT user must have 'Bypass rules on work item updates' permission."
            .to_string(),
    );
    warnings.push(
        "Service connection secrets, variable group secrets, and pipeline secrets cannot be migrated and must be reconfigured on the target."
            .to_string(),
    );
    warnings
        .push("Repository mirror push requires the target repositories to be empty.".to_string());
    warnings.push(
        "Cross-tenant user identities are preserved as plain text annotations only; AssignedTo and similar fields will not be set to real target-tenant users."
            .to_string(),
    );

    Ok(PreflightReport {
        source_project_exists,
        target_project_exists,
        warnings,
    })
}
