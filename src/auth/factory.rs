//! Factory for constructing Azure DevOps API clients from credentials.
//!
//! Each module currently builds its own client with the same pattern:
//! call `get_credentials()`, wrap the PAT in `Credential::Pat`,
//! pass it to the SDK `ClientBuilder`. This module concentrates that
//! logic behind a single **seam** so callers know nothing about
//! credential resolution.

use crate::auth::url::{normalize_base_url, user_entitlements_base_url};
use crate::auth::Credentials;

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
}

/// A concrete factory backed by a `Credentials` object.
pub struct CredentialClientFactory {
    credential: azure_devops_rust_api::Credential,
    endpoint: azure_core::http::Url,
    entitlements_endpoint: azure_core::http::Url,
}

impl CredentialClientFactory {
    /// Create a new factory from raw credentials.
    pub fn new(creds: &Credentials) -> Self {
        let endpoint = normalize_base_url(&creds.base_url)
            .parse()
            .expect("base_url should be a valid URL");
        let entitlements_endpoint = normalize_base_url(&user_entitlements_base_url(&creds.base_url))
            .parse()
            .expect("entitlements base_url should be a valid URL");

        Self {
            credential: azure_devops_rust_api::Credential::Pat(creds.pat.clone()),
            endpoint,
            entitlements_endpoint,
        }
    }
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
}
