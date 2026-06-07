//! Migration state file. Tracks per-phase status, ID maps, and error counts
//! so a failed migration can be resumed without re-doing successful work.

#![allow(dead_code)]

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub const STATE_SCHEMA_VERSION: u32 = 1;

pub const DEFAULT_PHASE_NAMES: [&str; 19] = [
    "process",
    "project",
    "areas",
    "iterations",
    "teams_create",
    "teams_configure",
    "repos",
    "wikis",
    "work_items",
    "wi_links",
    "wi_attachments",
    "wi_comments",
    "prs",
    "variable_groups",
    "service_connections",
    "pipelines_yaml",
    "pipelines_classic",
    "test_plans",
    "dashboards",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PhaseStatus {
    Pending,
    InProgress,
    Done,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseRecord {
    pub name: String,
    pub status: PhaseStatus,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub items_total: u64,
    #[serde(default)]
    pub items_done: u64,
    #[serde(default)]
    pub items_failed: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IdMap {
    /// source identifier -> target identifier
    #[serde(flatten)]
    pub map: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationState {
    pub schema_version: u32,
    pub source_project: String,
    pub target_project: String,
    pub phases: Vec<PhaseRecord>,
    #[serde(default)]
    pub id_maps: HashMap<String, IdMap>,
}

impl MigrationState {
    pub fn new(source_project: &str, target_project: &str) -> Self {
        Self {
            schema_version: STATE_SCHEMA_VERSION,
            source_project: source_project.to_string(),
            target_project: target_project.to_string(),
            phases: DEFAULT_PHASE_NAMES
                .iter()
                .map(|name| PhaseRecord {
                    name: (*name).to_string(),
                    status: PhaseStatus::Pending,
                    error: None,
                    items_total: 0,
                    items_done: 0,
                    items_failed: 0,
                })
                .collect(),
            id_maps: HashMap::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Reading state file '{}'", path.display()))?;
        let state: MigrationState = serde_json::from_str(&content)
            .with_context(|| format!("Parsing state file '{}'", path.display()))?;
        if state.schema_version != STATE_SCHEMA_VERSION {
            anyhow::bail!(
                "Incompatible state file schema version: got {}, expected {}",
                state.schema_version,
                STATE_SCHEMA_VERSION
            );
        }
        Ok(state)
    }

    /// Atomic save: write to <path>.tmp then rename.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(self)?;
        fs::write(&tmp, json).with_context(|| format!("Writing state file '{}'", tmp.display()))?;
        fs::rename(&tmp, path)
            .with_context(|| format!("Renaming state file to '{}'", path.display()))?;
        Ok(())
    }

    pub fn get_phase(&self, name: &str) -> Option<&PhaseRecord> {
        self.phases.iter().find(|p| p.name == name)
    }

    pub fn upsert_phase(&mut self, record: PhaseRecord) {
        if let Some(existing) = self.phases.iter_mut().find(|p| p.name == record.name) {
            *existing = record;
        } else {
            self.phases.push(record);
        }
    }

    pub fn mark_phase_done(&mut self, name: &str, items_total: u64, items_done: u64) {
        self.upsert_phase(PhaseRecord {
            name: name.to_string(),
            status: PhaseStatus::Done,
            error: None,
            items_total,
            items_done,
            items_failed: 0,
        });
    }

    pub fn mark_phase_failed(&mut self, name: &str, error: impl Into<String>, items_failed: u64) {
        self.upsert_phase(PhaseRecord {
            name: name.to_string(),
            status: PhaseStatus::Failed,
            error: Some(error.into()),
            items_total: 0,
            items_done: 0,
            items_failed,
        });
    }

    pub fn id_map_mut(&mut self, kind: &str) -> &mut IdMap {
        self.id_maps.entry(kind.to_string()).or_default()
    }

    pub fn id_map(&self, kind: &str) -> Option<&IdMap> {
        self.id_maps.get(kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn new_state_starts_with_all_phases_pending() {
        let state = MigrationState::new("Source", "Target");

        assert_eq!(state.schema_version, STATE_SCHEMA_VERSION);
        assert_eq!(state.source_project, "Source");
        assert_eq!(state.target_project, "Target");
        assert_eq!(state.phases.len(), DEFAULT_PHASE_NAMES.len());
        assert!(state
            .phases
            .iter()
            .all(|phase| phase.status == PhaseStatus::Pending));
        assert_eq!(
            state
                .phases
                .iter()
                .map(|phase| phase.name.as_str())
                .collect::<Vec<_>>(),
            DEFAULT_PHASE_NAMES
        );
    }

    #[test]
    fn save_writes_state_that_load_can_read() {
        let mut state = MigrationState::new("Source", "Target");
        state.mark_phase_done("process", 2, 2);
        state
            .id_map_mut("repos")
            .map
            .insert("source-repo".to_string(), "target-repo".to_string());
        let path = unique_state_path("save_writes_state_that_load_can_read");

        state.save(&path).expect("state should save atomically");
        let loaded = MigrationState::load(&path).expect("state should load");

        assert_eq!(loaded.schema_version, state.schema_version);
        assert_eq!(loaded.source_project, state.source_project);
        assert_eq!(loaded.target_project, state.target_project);
        assert_eq!(loaded.phases.len(), state.phases.len());
        assert_eq!(
            loaded.get_phase("process").map(|phase| &phase.status),
            Some(&PhaseStatus::Done)
        );
        assert_eq!(
            loaded
                .id_map("repos")
                .and_then(|map| map.map.get("source-repo")),
            Some(&"target-repo".to_string())
        );

        cleanup_state_path(path);
    }

    #[test]
    fn mark_phase_done_and_failed_update_status() {
        let mut state = MigrationState::new("Source", "Target");

        state.mark_phase_done("process", 3, 3);
        state.mark_phase_failed("repos", "clone failed", 2);

        let done = state.get_phase("process").expect("process phase exists");
        assert_eq!(done.status, PhaseStatus::Done);
        assert_eq!(done.items_total, 3);
        assert_eq!(done.items_done, 3);
        assert_eq!(done.items_failed, 0);

        let failed = state.get_phase("repos").expect("repos phase exists");
        assert_eq!(failed.status, PhaseStatus::Failed);
        assert_eq!(failed.error.as_deref(), Some("clone failed"));
        assert_eq!(failed.items_failed, 2);
    }

    #[test]
    fn id_map_insert_and_lookup_round_trips() {
        let mut state = MigrationState::new("Source", "Target");

        state
            .id_map_mut("work_items")
            .map
            .insert("42".to_string(), "1001".to_string());

        assert_eq!(
            state.id_map("work_items").and_then(|map| map.map.get("42")),
            Some(&"1001".to_string())
        );
    }

    fn unique_state_path(test_name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        PathBuf::from("target")
            .join("unit-test-artifacts")
            .join(format!("{test_name}-{nanos}"))
            .join("state.json")
    }

    fn cleanup_state_path(path: PathBuf) {
        if let Some(root) = path.parent().and_then(|dir| dir.parent()) {
            let _ = fs::remove_dir_all(root);
        }
    }
}
