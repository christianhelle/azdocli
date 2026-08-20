//! Factory for constructing Azure DevOps API clients from credentials.
//!
//! Each module currently builds its own client with the same pattern:
//! call `get_credentials()`, wrap the PAT in `Credential::Pat`,
//! pass it to the SDK `ClientBuilder`. This module concentrates that
//! logic behind a single **seam** so callers know nothing about
//! credential resolution.

use crate::auth::url::{normalize_base_url, release_base_url, user_entitlements_base_url};
use crate::auth::Credentials;
use anyhow::{Context, Result};

/// Trait for building Azure DevOps API clients.
///
/// The interface is a small set of builder methods; the implementation
/// handles credential wrapping and SDK client construction.
pub trait ClientFactory {
    /// Build a Work Items (WIT) client.
    fn build_wit(&self) -> azure_devops_rust_api::wit::Client;

    /// Build a Git client.
    fn build_git(&self) -> azure_devops_rust_api::git::Client;

    /// Build a Pipelines client.
    fn build_pipelines(&self) -> azure_devops_rust_api::pipelines::Client;

    /// Build a Core (projects) client.
    fn build_core(&self) -> azure_devops_rust_api::core::Client;

    /// Build a Wiki client.
    fn build_wiki(&self) -> azure_devops_rust_api::wiki::Client;

    /// Build a Search client.
    fn build_search(&self) -> azure_devops_rust_api::search::Client;

    /// Build a Member Entitlement Management client.
    fn build_entitlements(&self) -> azure_devops_rust_api::member_entitlement_management::Client;

    /// Build a Build client.
    fn build_build(&self) -> azure_devops_rust_api::build::Client;

    /// Build a Release client.
    fn build_release(&self) -> azure_devops_rust_api::release::Client;

    /// Build a Distributed Task client.
    fn build_distributed_task(&self) -> azure_devops_rust_api::distributed_task::Client;

    /// Build a Work client.
    fn build_work(&self) -> azure_devops_rust_api::work::Client;

    /// Build a Service Endpoint client.
    fn build_service_endpoint(&self) -> azure_devops_rust_api::service_endpoint::Client;
}

/// A concrete factory backed by a `Credentials` object.
pub struct CredentialClientFactory {
    credential: azure_devops_rust_api::Credential,
    endpoint: azure_core::http::Url,
    entitlements_endpoint: azure_core::http::Url,
    release_endpoint: azure_core::http::Url,
}

impl CredentialClientFactory {
    /// Create a new factory from raw credentials.
    pub fn new(creds: &Credentials) -> Result<Self> {
        let endpoint = parse_url(
            &normalize_base_url(&creds.base_url),
            "Invalid base_url in credentials",
        )?;
        let entitlements_endpoint = parse_url(
            &normalize_base_url(&user_entitlements_base_url(&creds.base_url)),
            "Invalid user entitlements base URL derived from credentials",
        )?;
        let release_endpoint = parse_url(
            &normalize_base_url(&release_base_url(&creds.base_url)),
            "Invalid release base URL derived from credentials",
        )?;

        Ok(Self {
            credential: azure_devops_rust_api::Credential::Pat(creds.pat.clone()),
            endpoint,
            entitlements_endpoint,
            release_endpoint,
        })
    }
}

fn parse_url(url: &str, description: &str) -> Result<azure_core::http::Url> {
    url.parse().with_context(|| format!("{description}: {url}"))
}

impl ClientFactory for CredentialClientFactory {
    fn build_wit(&self) -> azure_devops_rust_api::wit::Client {
        azure_devops_rust_api::wit::ClientBuilder::new(self.credential.clone())
            .endpoint(self.endpoint.clone())
            .build()
    }

    fn build_git(&self) -> azure_devops_rust_api::git::Client {
        azure_devops_rust_api::git::ClientBuilder::new(self.credential.clone())
            .endpoint(self.endpoint.clone())
            .build()
    }

    fn build_pipelines(&self) -> azure_devops_rust_api::pipelines::Client {
        azure_devops_rust_api::pipelines::ClientBuilder::new(self.credential.clone())
            .endpoint(self.endpoint.clone())
            .build()
    }

    fn build_core(&self) -> azure_devops_rust_api::core::Client {
        azure_devops_rust_api::core::ClientBuilder::new(self.credential.clone())
            .endpoint(self.endpoint.clone())
            .build()
    }

    fn build_wiki(&self) -> azure_devops_rust_api::wiki::Client {
        azure_devops_rust_api::wiki::ClientBuilder::new(self.credential.clone())
            .endpoint(self.endpoint.clone())
            .build()
    }

    fn build_search(&self) -> azure_devops_rust_api::search::Client {
        azure_devops_rust_api::search::ClientBuilder::new(self.credential.clone())
            .endpoint(self.endpoint.clone())
            .build()
    }

    fn build_entitlements(&self) -> azure_devops_rust_api::member_entitlement_management::Client {
        azure_devops_rust_api::member_entitlement_management::ClientBuilder::new(
            self.credential.clone(),
        )
        .endpoint(self.entitlements_endpoint.clone())
        .build()
    }

    fn build_build(&self) -> azure_devops_rust_api::build::Client {
        azure_devops_rust_api::build::ClientBuilder::new(self.credential.clone())
            .endpoint(self.endpoint.clone())
            .build()
    }

    fn build_release(&self) -> azure_devops_rust_api::release::Client {
        azure_devops_rust_api::release::ClientBuilder::new(self.credential.clone())
            .endpoint(self.release_endpoint.clone())
            .build()
    }

    fn build_distributed_task(&self) -> azure_devops_rust_api::distributed_task::Client {
        azure_devops_rust_api::distributed_task::ClientBuilder::new(self.credential.clone())
            .endpoint(self.endpoint.clone())
            .build()
    }

    fn build_work(&self) -> azure_devops_rust_api::work::Client {
        azure_devops_rust_api::work::ClientBuilder::new(self.credential.clone())
            .endpoint(self.endpoint.clone())
            .build()
    }

    fn build_service_endpoint(&self) -> azure_devops_rust_api::service_endpoint::Client {
        azure_devops_rust_api::service_endpoint::ClientBuilder::new(self.credential.clone())
            .endpoint(self.endpoint.clone())
            .build()
    }
}
