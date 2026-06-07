use anyhow::Result;
use async_trait::async_trait;

use super::areas::{execute_classification_phase, ClassificationGroup};
use crate::migrate::context::MigrationContext;
use crate::migrate::phase::{Phase, PhaseSummary};

pub struct IterationsPhase;

#[async_trait]
impl Phase for IterationsPhase {
    fn name(&self) -> &'static str {
        "iterations"
    }

    async fn execute(&self, ctx: &mut MigrationContext) -> Result<PhaseSummary> {
        execute_classification_phase(ctx, ClassificationGroup::Iterations, "iterations").await
    }
}
