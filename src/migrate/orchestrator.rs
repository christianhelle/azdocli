//! Top-level orchestrator. Runs phases in dependency order, honoring
//! `--phases`, `--skip-phases`, `--dry-run`, `--fail-fast`, and `--resume`.

use anyhow::{anyhow, Result};
use colored::Colorize;
use std::collections::HashSet;
use std::sync::Arc;

use super::context::MigrationContext;
use super::phase::Phase;
use super::phases;
use super::preflight;
use super::progress::ProgressReporter;
use super::report;
use super::state::{PhaseRecord, PhaseStatus};

/// Fixed phase order. See plan.md for rationale.
pub fn build_phases() -> Vec<Arc<dyn Phase>> {
    vec![
        Arc::new(phases::process::ProcessPhase),
        Arc::new(phases::project::ProjectPhase),
        Arc::new(phases::areas::AreasPhase),
        Arc::new(phases::iterations::IterationsPhase),
        Arc::new(phases::teams_create::TeamsCreatePhase),
        Arc::new(phases::teams_configure::TeamsConfigurePhase),
        Arc::new(phases::repos::ReposPhase),
        Arc::new(phases::wikis::WikisPhase),
        Arc::new(phases::work_items::WorkItemsPhase),
        Arc::new(phases::wi_links::WiLinksPhase),
        Arc::new(phases::wi_attachments::WiAttachmentsPhase),
        Arc::new(phases::wi_comments::WiCommentsPhase),
        Arc::new(phases::prs::PrsPhase),
        Arc::new(phases::variable_groups::VariableGroupsPhase),
        Arc::new(phases::service_connections::ServiceConnectionsPhase),
        Arc::new(phases::pipelines_yaml::PipelinesYamlPhase),
        Arc::new(phases::pipelines_classic::PipelinesClassicPhase),
        Arc::new(phases::test_plans::TestPlansPhase),
        Arc::new(phases::dashboards::DashboardsPhase),
    ]
}

pub async fn run(mut ctx: MigrationContext) -> Result<()> {
    let reporter = ProgressReporter::new();

    let title = if ctx.opts.dry_run { "[DRY RUN] " } else { "" };
    reporter.println(format!(
        "{}Migrating '{}' ({}) → '{}' ({})",
        title.yellow().bold(),
        ctx.opts.source_project,
        ctx.source_creds.organization,
        ctx.opts.target_project,
        ctx.target_creds.organization,
    ));

    // Preflight (always runs, even in dry-run)
    let pf = preflight::run(&ctx).await?;
    for w in &pf.warnings {
        reporter.println(format!("⚠ {}", w.yellow()));
    }

    let phases = build_phases();
    let included = phase_filter(&ctx, &phases);

    let mut had_error = false;
    for phase in &phases {
        let name = phase.name();
        if !included.contains(name) {
            mark_skipped(&mut ctx, name);
            ctx.save_state().ok();
            continue;
        }
        if let Some(rec) = ctx.state.get_phase(name) {
            if rec.status == PhaseStatus::Done {
                reporter.println(format!("⏭  {} (already done, resume)", name));
                continue;
            }
        }

        reporter.println(format!("▶  {}", name.bold()));
        ctx.state.upsert_phase(PhaseRecord {
            name: name.to_string(),
            status: PhaseStatus::InProgress,
            error: None,
            items_total: 0,
            items_done: 0,
            items_failed: 0,
        });
        ctx.save_state().ok();

        let result = phase.execute(&mut ctx).await;

        match result {
            Ok(summary) => {
                let phase_failed = summary.items_failed > 0;
                ctx.state.upsert_phase(PhaseRecord {
                    name: name.to_string(),
                    status: if phase_failed {
                        PhaseStatus::Failed
                    } else {
                        PhaseStatus::Done
                    },
                    error: summary.errors.first().cloned(),
                    items_total: summary.items_total,
                    items_done: summary.items_done,
                    items_failed: summary.items_failed,
                });
                ctx.save_state().ok();
                if phase_failed {
                    had_error = true;
                    if ctx.opts.fail_fast {
                        report::write_reports(&ctx.state, &ctx.output_dir).ok();
                        report::print_summary(&ctx.state);
                        return Err(anyhow!(
                            "Phase '{}' had {} item failure(s); aborting (--fail-fast)",
                            name,
                            summary.items_failed
                        ));
                    }
                }
            }
            Err(e) => {
                ctx.state.upsert_phase(PhaseRecord {
                    name: name.to_string(),
                    status: PhaseStatus::Failed,
                    error: Some(format!("{:#}", e)),
                    items_total: 0,
                    items_done: 0,
                    items_failed: 1,
                });
                ctx.save_state().ok();
                had_error = true;
                reporter.println(format!("❌ {} failed: {}", name, e));
                if ctx.opts.fail_fast {
                    report::write_reports(&ctx.state, &ctx.output_dir).ok();
                    report::print_summary(&ctx.state);
                    return Err(e);
                }
            }
        }
    }

    report::write_reports(&ctx.state, &ctx.output_dir)?;
    report::print_summary(&ctx.state);

    if had_error {
        Err(anyhow!(
            "Migration finished with errors. See report.md in '{}'.",
            ctx.output_dir.display()
        ))
    } else {
        Ok(())
    }
}

fn phase_filter(ctx: &MigrationContext, phases: &[Arc<dyn Phase>]) -> HashSet<String> {
    let all_names: HashSet<String> = phases.iter().map(|p| p.name().to_string()).collect();
    let mut included = match &ctx.opts.phases {
        Some(list) => list.iter().cloned().collect::<HashSet<_>>(),
        None => all_names.clone(),
    };
    if let Some(skip) = &ctx.opts.skip_phases {
        for s in skip {
            included.remove(s);
        }
    }
    included
}

fn mark_skipped(ctx: &mut MigrationContext, name: &str) {
    ctx.state.upsert_phase(PhaseRecord {
        name: name.to_string(),
        status: PhaseStatus::Skipped,
        error: None,
        items_total: 0,
        items_done: 0,
        items_failed: 0,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrate::state::DEFAULT_PHASE_NAMES;

    #[test]
    fn build_phases_returns_documented_order() {
        let phases = build_phases();
        let names = phases.iter().map(|phase| phase.name()).collect::<Vec<_>>();

        assert_eq!(names.len(), 19);
        assert_eq!(names, DEFAULT_PHASE_NAMES);
    }
}
