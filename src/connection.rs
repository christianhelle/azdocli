//! The resolved organization, credentials and API clients for one invocation.

use crate::auth::factory::{ClientFactory, CredentialClientFactory};
use crate::auth::Credentials;
use anyhow::{anyhow, Result};

pub struct Connection {
    credentials: Credentials,
    default_project: Option<String>,
    factory: CredentialClientFactory,
}

impl Connection {
    /// Reads config.json and credentials.json once.
    pub fn load() -> Result<Self> {
        let (credentials, default_project) = crate::auth::get_credentials_and_default_project()?;
        Self::new(credentials, default_project)
    }

    /// Builds a connection without touching disk.
    pub fn new(credentials: Credentials, default_project: Option<String>) -> Result<Self> {
        let factory = CredentialClientFactory::new(&credentials)?;
        Ok(Self {
            credentials,
            default_project,
            factory,
        })
    }

    pub fn organization(&self) -> &str {
        &self.credentials.organization
    }

    pub fn credentials(&self) -> &Credentials {
        &self.credentials
    }

    /// The explicit project argument, else the configured default project.
    pub fn project(&self, arg: Option<&str>) -> Result<String> {
        match arg.or(self.default_project.as_deref()) {
            Some(project) => Ok(project.to_string()),
            None => Err(anyhow!("No default project configured. Please set one using 'azdocli project <project_name>' or use the --project argument")),
        }
    }

    pub fn wit(&self) -> azure_devops_rust_api::wit::Client {
        self.factory.build_wit()
    }

    pub fn work(&self) -> azure_devops_rust_api::work::Client {
        self.factory.build_work()
    }

    pub fn core(&self) -> azure_devops_rust_api::core::Client {
        self.factory.build_core()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connection(default_project: Option<&str>) -> Connection {
        let creds = Credentials {
            organization: "org".to_string(),
            pat: "pat".to_string(),
            base_url: "https://dev.azure.com".to_string(),
        };
        Connection::new(creds, default_project.map(str::to_string)).unwrap()
    }

    #[test]
    fn project_arg_wins_over_default() {
        let conn = connection(Some("Default"));
        assert_eq!(conn.project(Some("Arg")).unwrap(), "Arg");
    }

    #[test]
    fn project_falls_back_to_default() {
        let conn = connection(Some("Default"));
        assert_eq!(conn.project(None).unwrap(), "Default");
    }

    #[test]
    fn project_errors_without_arg_or_default() {
        let conn = connection(None);
        let err = conn.project(None).unwrap_err();
        assert_eq!(
            err.to_string(),
            "No default project configured. Please set one using 'azdocli project <project_name>' or use the --project argument"
        );
    }
}
