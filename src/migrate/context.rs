#![allow(dead_code)]
//! Shared migration context (creds, clients, state, output dir, options).

use anyhow::{Context, Result};
use azure_devops_rust_api::Credential;
use chrono::Utc;
use std::path::PathBuf;

use crate::auth::factory::CredentialClientFactory;
use crate::auth::Credentials;

use super::executor::Executor;
use super::state::MigrationState;

#[derive(Debug, Clone)]
pub struct MigrationOptions {
    pub source_project: String,
    pub target_project: String,
    pub create_target: bool,
    pub phases: Option<Vec<String>>,
    pub skip_phases: Option<Vec<String>>,
    pub dry_run: bool,
    pub fail_fast: bool,
    pub resume: bool,
    pub state_file: Option<PathBuf>,
    pub output_dir: Option<PathBuf>,
    pub concurrency: usize,
    pub yes: bool,
}

pub struct MigrationContext {
    pub source_creds: Credentials,
    pub target_creds: Credentials,
    pub source_credential: Credential,
    pub target_credential: Credential,
    pub opts: MigrationOptions,
    pub state: MigrationState,
    pub output_dir: PathBuf,
    pub state_path: PathBuf,
    pub executor: Executor,
}

impl MigrationContext {
    pub fn new(
        source_creds: Credentials,
        target_creds: Credentials,
        opts: MigrationOptions,
    ) -> Result<Self> {
        let output_dir = opts.output_dir.clone().unwrap_or_else(|| {
            let ts = Utc::now().format("%Y%m%d-%H%M%S");
            PathBuf::from(format!(
                "azdocli-migration-{}-{}-{}",
                sanitize(&source_creds.organization),
                sanitize(&opts.source_project),
                ts
            ))
        });
        let project_out = output_dir.join(sanitize(&opts.source_project));
        std::fs::create_dir_all(&project_out)
            .with_context(|| format!("Creating output dir '{}'", project_out.display()))?;

        let state_path = opts
            .state_file
            .clone()
            .unwrap_or_else(|| project_out.join("state.json"));

        let state = if opts.resume && state_path.exists() {
            MigrationState::load(&state_path)?
        } else {
            MigrationState::new(&opts.source_project, &opts.target_project)
        };

        let source_credential = Credential::Pat(source_creds.pat.clone());
        let target_credential = Credential::Pat(target_creds.pat.clone());

        let executor = Executor::new(opts.concurrency);

        Ok(Self {
            source_creds,
            target_creds,
            source_credential,
            target_credential,
            opts,
            state,
            output_dir: project_out,
            state_path,
            executor,
        })
    }

    pub fn save_state(&self) -> Result<()> {
        self.state.save(&self.state_path)
    }

    pub fn source_factory(&self) -> Result<CredentialClientFactory> {
        CredentialClientFactory::new(&self.source_creds)
    }

    pub fn target_factory(&self) -> Result<CredentialClientFactory> {
        CredentialClientFactory::new(&self.target_creds)
    }

    pub fn source_base_url(&self) -> &str {
        &self.source_creds.base_url
    }

    pub fn target_base_url(&self) -> &str {
        &self.target_creds.base_url
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
