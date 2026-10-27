use crate::auth::factory::{ClientFactory, CredentialClientFactory};
use crate::auth::get_credentials;
use crate::project::get_project_or_default;
use crate::text::escape_control_characters;
use anyhow::Result;
use azure_devops_rust_api::artifacts::models::Feed;
use azure_devops_rust_api::artifacts::Client;
use clap::{Args, Subcommand, ValueEnum};
use colored::Colorize;

#[derive(Subcommand, Clone)]
pub enum ArtifactsSubCommands {
    /// Manage Azure Artifacts feeds
    Feeds {
        #[clap(subcommand)]
        subcommand: FeedsSubCommands,
    },
}

#[derive(Subcommand, Clone)]
pub enum FeedsSubCommands {
    /// List the feeds visible in the organization, or only those of a project
    List {
        #[clap(flatten)]
        scope: ScopeArgs,
    },
}

/// Where a feed lives: organization-scoped feeds are addressed without a
/// project, project-scoped feeds only through their project.
#[derive(ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FeedScope {
    #[default]
    Organization,
    Project,
}

#[derive(Args, Clone, Debug, Default)]
pub struct ScopeArgs {
    /// Scope of the feed: 'project' if the feed was created in a project, 'organization' otherwise
    #[clap(long, value_enum, default_value_t = FeedScope::Organization)]
    pub scope: FeedScope,
    /// Team project name (implies --scope project; defaults to the default project)
    #[clap(short, long)]
    pub project: Option<String>,
}

impl ScopeArgs {
    /// Returns the project that addresses the feed, or `None` for an
    /// organization-scoped feed. Naming a project selects project scope.
    pub fn resolve_project(&self) -> Result<Option<String>> {
        if self.project.is_none() && self.scope == FeedScope::Organization {
            return Ok(None);
        }
        get_project_or_default(self.project.as_deref()).map(Some)
    }
}

fn create_artifacts_client() -> Result<(Client, String)> {
    let creds = get_credentials()?;
    let factory = CredentialClientFactory::new(&creds)?;
    Ok((factory.build_artifacts(), creds.organization))
}

/// The generated client always puts a project segment after the organization,
/// so an organization-scoped request is addressed with an empty organization
/// and the organization in the project position, which keeps a single slash
/// before `_apis`.
fn scoped_path(organization: &str, project: Option<&str>) -> (String, String) {
    match project {
        Some(project) => (organization.to_string(), project.to_string()),
        None => (String::new(), organization.to_string()),
    }
}

async fn list_feeds(project: Option<&str>) -> Result<Vec<Feed>> {
    let (client, organization) = create_artifacts_client()?;
    let (organization, project) = scoped_path(&organization, project);

    Ok(client
        .feed_management_client()
        .get_feeds(organization, project)
        .await?
        .value)
}

fn display_feeds(feeds: &[Feed]) {
    if feeds.is_empty() {
        println!("No feeds found.");
        return;
    }

    println!(
        "{:<38} {:<30} {:<28} {}",
        "ID".bold(),
        "Name".bold(),
        "Project".bold(),
        "Description".bold()
    );
    println!("{}", "-".repeat(120));

    for feed in feeds {
        let core = &feed.feed_core;
        println!(
            "{:<38} {:<30} {:<28} {}",
            core.id.as_deref().unwrap_or("-"),
            core.name.as_deref().unwrap_or("-"),
            core.project
                .as_ref()
                .and_then(|project| project.name.as_deref())
                .unwrap_or("-"),
            feed.description
                .as_deref()
                .map(escape_control_characters)
                .unwrap_or_else(|| "-".to_string())
        );
    }
}

fn scope_label(project: Option<&str>) -> String {
    match project {
        Some(project) => format!("project '{project}'"),
        None => "the organization".to_string(),
    }
}

pub async fn handle_command(subcommand: &ArtifactsSubCommands) -> Result<()> {
    match subcommand {
        ArtifactsSubCommands::Feeds {
            subcommand: FeedsSubCommands::List { scope },
        } => {
            let project = scope.resolve_project()?;
            match list_feeds(project.as_deref()).await {
                Ok(feeds) => display_feeds(&feeds),
                Err(e) => {
                    eprintln!(
                        "❌ Failed to list the feeds of {}",
                        scope_label(project.as_deref())
                    );
                    eprintln!("   {e}");
                    return Err(e);
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoped_path_keeps_project_after_organization() {
        assert_eq!(
            scoped_path("contoso", Some("web")),
            ("contoso".to_string(), "web".to_string())
        );
    }

    #[test]
    fn scoped_path_moves_organization_into_project_slot_for_org_scope() {
        assert_eq!(
            scoped_path("contoso", None),
            (String::new(), "contoso".to_string())
        );
    }

    #[test]
    fn organization_scope_without_project_resolves_to_no_project() {
        let scope = ScopeArgs::default();
        assert_eq!(scope.resolve_project().unwrap(), None);
    }

    #[test]
    fn explicit_project_selects_project_scope() {
        let scope = ScopeArgs {
            scope: FeedScope::Organization,
            project: Some("web".to_string()),
        };
        assert_eq!(scope.resolve_project().unwrap(), Some("web".to_string()));
    }
}
