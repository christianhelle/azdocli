//! Phase trait. Each migration phase implements this and is wired into
//! the orchestrator in a fixed order.

#![allow(dead_code)]

use anyhow::Result;
use async_trait::async_trait;

use super::context::MigrationContext;

#[derive(Debug, Clone, Default)]
pub struct PhaseSummary {
    pub items_total: u64,
    pub items_done: u64,
    pub items_failed: u64,
    pub errors: Vec<String>,
}

impl PhaseSummary {
    pub fn record_success(&mut self) {
        self.items_done += 1;
    }
    pub fn record_failure(&mut self, msg: impl Into<String>) {
        self.items_failed += 1;
        self.errors.push(msg.into());
    }
}

#[async_trait]
pub trait Phase: Send + Sync {
    fn name(&self) -> &'static str;

    /// Optional pre-flight check. Default is a no-op.
    async fn preflight(&self, _ctx: &MigrationContext) -> Result<()> {
        Ok(())
    }

    /// Run the phase. Implementations should:
    /// - honour `ctx.opts.dry_run` (read but do not write to target)
    /// - update `ctx.state.id_maps` for downstream phases
    /// - aggregate per-item results into `PhaseSummary`
    async fn execute(&self, ctx: &mut MigrationContext) -> Result<PhaseSummary>;
}
