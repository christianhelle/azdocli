//! End-of-migration report rendering.

use anyhow::Result;
use chrono::Utc;
use std::fs;
use std::path::Path;

use super::state::{MigrationState, PhaseStatus};

pub fn write_reports(state: &MigrationState, output_dir: &Path) -> Result<()> {
    let json_path = output_dir.join("report.json");
    let md_path = output_dir.join("report.md");

    fs::write(&json_path, serde_json::to_string_pretty(state)?)?;
    fs::write(&md_path, render_markdown(state))?;
    Ok(())
}

pub fn render_markdown(state: &MigrationState) -> String {
    let mut out = String::new();
    out.push_str("# Azure DevOps Migration Report\n\n");
    out.push_str(&format!("- Generated: {}\n", Utc::now().to_rfc3339()));
    out.push_str(&format!("- Source project: `{}`\n", state.source_project));
    out.push_str(&format!("- Target project: `{}`\n\n", state.target_project));

    out.push_str("## Phase summary\n\n");
    out.push_str("| Phase | Status | Items done | Failed | Total | Error |\n");
    out.push_str("|---|---|---:|---:|---:|---|\n");
    for p in &state.phases {
        let status = match p.status {
            PhaseStatus::Pending => "⏳ pending",
            PhaseStatus::InProgress => "▶ in_progress",
            PhaseStatus::Done => "✅ done",
            PhaseStatus::Failed => "❌ failed",
            PhaseStatus::Skipped => "⏭ skipped",
        };
        let err = p.error.as_deref().unwrap_or("");
        out.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | {} |\n",
            p.name, status, p.items_done, p.items_failed, p.items_total, err
        ));
    }
    out.push_str("\n## ID maps\n\n");
    for (kind, map) in &state.id_maps {
        out.push_str(&format!("- `{}`: {} entries\n", kind, map.map.len()));
    }
    out
}

pub fn print_summary(state: &MigrationState) {
    println!();
    println!("=== Migration summary ===");
    println!(
        "Source: {}   Target: {}",
        state.source_project, state.target_project
    );
    for p in &state.phases {
        let status = match p.status {
            PhaseStatus::Pending => "pending",
            PhaseStatus::InProgress => "in_progress",
            PhaseStatus::Done => "done",
            PhaseStatus::Failed => "failed",
            PhaseStatus::Skipped => "skipped",
        };
        println!(
            "  {:<24} {:<12} done={:<5} failed={:<5} total={:<5}",
            p.name, status, p.items_done, p.items_failed, p.items_total
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrate::state::PhaseRecord;
    use serde_json::json;

    #[test]
    fn report_state_serializes_success_and_failure_shape() {
        let state = report_state();

        let value = serde_json::to_value(&state).expect("state should serialize");

        assert_eq!(value["source_project"], json!("Source"));
        assert_eq!(value["target_project"], json!("Target"));
        assert_eq!(value["phases"][0]["name"], json!("process"));
        assert_eq!(value["phases"][0]["status"], json!("done"));
        assert_eq!(value["phases"][1]["name"], json!("repos"));
        assert_eq!(value["phases"][1]["status"], json!("failed"));
        assert_eq!(value["phases"][1]["error"], json!("clone failed"));
    }

    #[test]
    fn render_markdown_contains_expected_headings_and_phase_rows() {
        let markdown = render_markdown(&report_state());

        assert!(markdown.contains("# Azure DevOps Migration Report"));
        assert!(markdown.contains("## Phase summary"));
        assert!(markdown.contains("## ID maps"));
        assert!(markdown.contains("| `process` | ✅ done | 3 | 0 | 3 |  |"));
        assert!(markdown.contains("| `repos` | ❌ failed | 1 | 2 | 3 | clone failed |"));
        assert!(markdown.contains("- `repos`: 1 entries"));
    }

    fn report_state() -> MigrationState {
        let mut state = MigrationState::new("Source", "Target");
        state.phases = vec![
            PhaseRecord {
                name: "process".to_string(),
                status: PhaseStatus::Done,
                error: None,
                items_total: 3,
                items_done: 3,
                items_failed: 0,
            },
            PhaseRecord {
                name: "repos".to_string(),
                status: PhaseStatus::Failed,
                error: Some("clone failed".to_string()),
                items_total: 3,
                items_done: 1,
                items_failed: 2,
            },
        ];
        state
            .id_map_mut("repos")
            .map
            .insert("source".to_string(), "target".to_string());
        state
    }
}
