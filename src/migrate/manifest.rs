//! JSON manifest schema for batch migrations.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub source_profile: String,
    pub target_profile: String,
    #[serde(default)]
    pub output_dir: Option<PathBuf>,
    #[serde(default)]
    pub default_options: ManifestOptions,
    pub projects: Vec<ProjectEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManifestOptions {
    #[serde(default)]
    pub create_target: bool,
    #[serde(default)]
    pub concurrency: Option<usize>,
    #[serde(default)]
    pub fail_fast: Option<bool>,
    #[serde(default)]
    pub phases: Option<Vec<String>>,
    #[serde(default)]
    pub skip_phases: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub source: String,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub options: Option<ManifestOptions>,
}

#[derive(Debug, Clone)]
pub struct MergedOptions {
    pub create_target: bool,
    pub concurrency: usize,
    pub fail_fast: bool,
    pub phases: Option<Vec<String>>,
    pub skip_phases: Option<Vec<String>>,
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Reading manifest file '{}'", path.display()))?;
        let manifest: Manifest = serde_json::from_str(&content)
            .with_context(|| format!("Parsing manifest file '{}'", path.display()))?;
        Ok(manifest)
    }

    pub fn merged_options_for(&self, project: &ProjectEntry) -> MergedOptions {
        let d = &self.default_options;
        let p = project.options.clone().unwrap_or_default();
        MergedOptions {
            create_target: p.create_target || d.create_target,
            concurrency: p.concurrency.or(d.concurrency).unwrap_or(4),
            fail_fast: p.fail_fast.or(d.fail_fast).unwrap_or(false),
            phases: p.phases.or_else(|| d.phases.clone()),
            skip_phases: p.skip_phases.or_else(|| d.skip_phases.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_manifest_with_project_mappings() {
        let json = r#"
        {
            "source_profile": "source-profile",
            "target_profile": "target-profile",
            "output_dir": "migration-output",
            "default_options": {
                "create_target": true,
                "concurrency": 8,
                "fail_fast": true,
                "phases": ["process", "repos"],
                "skip_phases": ["dashboards"]
            },
            "projects": [
                {
                    "source": "SourceProject",
                    "target": "TargetProject",
                    "options": {
                        "concurrency": 2,
                        "skip_phases": ["wikis"]
                    }
                }
            ]
        }
        "#;

        let manifest: Manifest = serde_json::from_str(json).expect("manifest should parse");

        assert_eq!(manifest.source_profile, "source-profile");
        assert_eq!(manifest.target_profile, "target-profile");
        assert_eq!(manifest.output_dir, Some(PathBuf::from("migration-output")));
        assert_eq!(manifest.projects.len(), 1);
        assert_eq!(manifest.projects[0].source, "SourceProject");
        assert_eq!(
            manifest.projects[0].target.as_deref(),
            Some("TargetProject")
        );

        let options = manifest.merged_options_for(&manifest.projects[0]);
        assert!(options.create_target);
        assert_eq!(options.concurrency, 2);
        assert!(options.fail_fast);
        assert_eq!(
            options.phases,
            Some(vec!["process".to_string(), "repos".to_string()])
        );
        assert_eq!(options.skip_phases, Some(vec!["wikis".to_string()]));
    }

    #[test]
    fn rejects_manifest_missing_required_fields() {
        let json = r#"
        {
            "source_profile": "source-profile",
            "projects": [{ "source": "SourceProject" }]
        }
        "#;

        let err = serde_json::from_str::<Manifest>(json).expect_err("target_profile is required");

        assert!(
            err.to_string().contains("missing field `target_profile`"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn round_trip_serialize_and_deserialize_preserves_manifest() {
        let manifest = Manifest {
            source_profile: "source-profile".to_string(),
            target_profile: "target-profile".to_string(),
            output_dir: Some(PathBuf::from("out")),
            default_options: ManifestOptions {
                create_target: true,
                concurrency: Some(6),
                fail_fast: Some(true),
                phases: Some(vec!["process".to_string(), "repos".to_string()]),
                skip_phases: Some(vec!["dashboards".to_string()]),
            },
            projects: vec![ProjectEntry {
                source: "SourceProject".to_string(),
                target: Some("TargetProject".to_string()),
                options: Some(ManifestOptions {
                    create_target: false,
                    concurrency: Some(3),
                    fail_fast: Some(false),
                    phases: None,
                    skip_phases: Some(vec!["wikis".to_string()]),
                }),
            }],
        };

        let serialized = serde_json::to_string(&manifest).expect("manifest should serialize");
        let deserialized: Manifest =
            serde_json::from_str(&serialized).expect("manifest should deserialize");

        assert_eq!(deserialized.source_profile, manifest.source_profile);
        assert_eq!(deserialized.target_profile, manifest.target_profile);
        assert_eq!(deserialized.output_dir, manifest.output_dir);
        assert_eq!(deserialized.projects[0].source, manifest.projects[0].source);
        assert_eq!(deserialized.projects[0].target, manifest.projects[0].target);
        assert_eq!(deserialized.default_options.concurrency, Some(6));
        assert_eq!(
            deserialized.projects[0]
                .options
                .as_ref()
                .and_then(|options| options.concurrency),
            Some(3)
        );
    }
}
