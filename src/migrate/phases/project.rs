use anyhow::{anyhow, Result};
use async_trait::async_trait;
use azure_devops_rust_api::core::models::{
    team_project_reference, TeamProject, TeamProjectReference,
};

use crate::auth::factory::ClientFactory;
use crate::migrate::context::MigrationContext;
use crate::migrate::phase::{Phase, PhaseSummary};

/// Create the target project if missing. Idempotent: if a project with the
/// target name already exists, the phase is a no-op.
///
/// Process template / capabilities are NOT set by this v1 phase — the
/// target tenant must already have a compatible process configured (see
/// the fidelity contract). Visibility and description are copied from
/// source.
pub struct ProjectPhase;

#[async_trait]
impl Phase for ProjectPhase {
    fn name(&self) -> &'static str {
        "project"
    }

    async fn execute(&self, ctx: &mut MigrationContext) -> Result<PhaseSummary> {
        let mut summary = PhaseSummary {
            items_total: 1,
            ..Default::default()
        };

        let target_client = ctx.target_factory()?.build_core();
        let projects = target_client
            .projects_client()
            .list(&ctx.target_creds.organization)
            .await
            .map_err(|e| anyhow!("Listing target projects: {e}"))?
            .value;

        if projects.iter().any(|p| p.name == ctx.opts.target_project) {
            summary.items_done = 1;
            return Ok(summary);
        }

        if !ctx.opts.create_target {
            summary.record_failure(format!(
                "Target project '{}' does not exist and --create-target was not set",
                ctx.opts.target_project
            ));
            return Ok(summary);
        }

        if ctx.opts.dry_run {
            summary.items_done = 1;
            return Ok(summary);
        }

        let source_client = ctx.source_factory()?.build_core();
        let source_projects = source_client
            .projects_client()
            .list(&ctx.source_creds.organization)
            .await
            .map_err(|e| anyhow!("Listing source projects: {e}"))?
            .value;
        let source = source_projects
            .iter()
            .find(|p| p.name == ctx.opts.source_project)
            .ok_or_else(|| anyhow!("Source project '{}' not found", ctx.opts.source_project))?;

        let visibility = match &source.visibility {
            team_project_reference::Visibility::Unchanged => {
                team_project_reference::Visibility::Private
            }
            other => other.clone(),
        };

        let mut reference = TeamProjectReference::new(ctx.opts.target_project.clone(), visibility);
        reference.description = source.description.clone();

        let payload = TeamProject::new(reference);

        match target_client
            .projects_client()
            .create(&ctx.target_creds.organization, payload)
            .await
        {
            Ok(_op) => {
                // ADO project creation is asynchronous. Downstream phases
                // will retry against the new project via the executor's
                // transient-error retry logic.
                summary.items_done = 1;
            }
            Err(e) => {
                summary.record_failure(format!("Creating target project: {e}"));
            }
        }
        Ok(summary)
    }
}
