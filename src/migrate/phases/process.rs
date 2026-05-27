use anyhow::Result;
use async_trait::async_trait;
use std::fs;

use crate::migrate::context::MigrationContext;
use crate::migrate::phase::{Phase, PhaseSummary};

/// Process template phase. Per the fidelity contract, this is **export-only**;
/// cross-tenant inherited process clone is not automated. Compatible process
/// must pre-exist on the target.
pub struct ProcessPhase;

#[async_trait]
impl Phase for ProcessPhase {
    fn name(&self) -> &'static str {
        "process"
    }

    async fn execute(&self, ctx: &mut MigrationContext) -> Result<PhaseSummary> {
        let mut summary = PhaseSummary::default();
        // Best-effort: dump a placeholder note. Actual process export via the
        // processes/processadmin API surface is a follow-up task.
        let path = ctx.output_dir.join("process-export.json");
        let note = serde_json::json!({
            "note": "Process export is not yet implemented; the target tenant must have a compatible process with the same name as the source.",
            "source_project": ctx.opts.source_project,
            "target_project": ctx.opts.target_project,
        });
        fs::write(&path, serde_json::to_string_pretty(&note)?)?;
        summary.items_total = 1;
        summary.items_done = 1;
        Ok(summary)
    }
}
