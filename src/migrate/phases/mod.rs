//! Per-phase implementations. Each phase implements the `Phase` trait.
//!
//! The phases in this v1 are a mix of complete implementations and
//! placeholders. See plan.md for the full status matrix. Placeholder
//! phases log a clear "not yet implemented" message and mark the phase
//! as skipped so the orchestrator can still produce a useful end-to-end
//! report.

pub mod areas;
pub mod dashboards;
pub mod iterations;
pub mod pipelines_classic;
pub mod pipelines_yaml;
pub mod process;
pub mod project;
pub mod prs;
pub mod repos;
pub mod service_connections;
pub mod teams_configure;
pub mod teams_create;
pub mod test_plans;
pub mod variable_groups;
pub mod wi_attachments;
pub mod wi_comments;
pub mod wi_links;
pub mod wikis;
mod work_item_common;
pub mod work_items;
