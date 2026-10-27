mod artifact_tool;
mod download;

use crate::auth::factory::{ClientFactory, CredentialClientFactory};
use crate::auth::get_credentials;
use crate::project::get_project_or_default;
use crate::text::escape_control_characters;
use anyhow::{anyhow, Result};
use azure_devops_rust_api::artifacts::models::{Feed, Package, PackageVersion};
use azure_devops_rust_api::artifacts::Client;
use clap::{Args, Subcommand, ValueEnum};
use colored::Colorize;
use std::path::PathBuf;

#[derive(Subcommand, Clone)]
pub enum ArtifactsSubCommands {
    /// Manage Azure Artifacts feeds
    Feeds {
        #[clap(subcommand)]
        subcommand: FeedsSubCommands,
    },
    /// Browse the packages of a feed
    Packages {
        #[clap(subcommand)]
        subcommand: PackagesSubCommands,
    },
    /// Manage Universal Packages
    Universal {
        #[clap(subcommand)]
        subcommand: UniversalSubCommands,
    },
}

#[derive(Subcommand, Clone)]
pub enum UniversalSubCommands {
    /// Download a Universal Package
    Download {
        /// Name or ID of the feed
        #[clap(long)]
        feed: String,
        /// Name of the package, e.g. 'foo-package'
        #[clap(short, long)]
        name: String,
        /// Version of the package, e.g. '1.0.0'
        #[clap(short, long)]
        version: String,
        /// Directory to place the package contents
        #[clap(long)]
        path: PathBuf,
        /// Only download files whose path matches this wildcard ('*' and '?'), e.g. '*.dll'
        #[clap(long)]
        file_filter: Option<String>,
        #[clap(flatten)]
        scope: ScopeArgs,
    },
    /// Publish a directory as a Universal Package (uses Microsoft's ArtifactTool)
    Publish {
        /// Name or ID of the feed
        #[clap(long)]
        feed: String,
        /// Name of the package, e.g. 'foo-package'
        #[clap(short, long)]
        name: String,
        /// Version of the package, e.g. '1.0.0'
        #[clap(short, long)]
        version: String,
        /// Directory containing the package contents
        #[clap(long)]
        path: PathBuf,
        /// Description of the package
        #[clap(short, long)]
        description: Option<String>,
        #[clap(flatten)]
        scope: ScopeArgs,
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

#[derive(Subcommand, Clone)]
pub enum PackagesSubCommands {
    /// List the packages of a feed
    List {
        /// Name or ID of the feed
        #[clap(long)]
        feed: String,
        /// Only list packages of this protocol (e.g. upack, nuget, npm, maven, pypi, cargo)
        #[clap(long)]
        protocol: Option<String>,
        /// Only list packages whose name contains this text
        #[clap(long)]
        name: Option<String>,
        #[clap(flatten)]
        scope: ScopeArgs,
    },
    /// List the versions of a package
    Versions {
        /// Name or ID of the feed
        #[clap(long)]
        feed: String,
        /// Name of the package
        #[clap(short, long)]
        name: String,
        /// Protocol of the package, needed when packages of different protocols share the name
        #[clap(long)]
        protocol: Option<String>,
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

async fn list_packages(
    project: Option<&str>,
    feed: &str,
    protocol: Option<&str>,
    name: Option<&str>,
) -> Result<Vec<Package>> {
    let (client, organization) = create_artifacts_client()?;
    let (organization, project) = scoped_path(&organization, project);

    let mut request = client
        .artifact_details_client()
        .get_packages(organization, feed, project);
    if let Some(protocol) = protocol {
        request = request.protocol_type(protocol);
    }
    if let Some(name) = name {
        request = request.package_name_query(name);
    }

    Ok(request.await?.value)
}

/// The versions API addresses a package by its id, so the name is looked up
/// first. A name search matches substrings, hence the exact comparison.
fn find_package<'a>(
    packages: &'a [Package],
    name: &str,
    protocol: Option<&str>,
) -> Result<&'a Package> {
    let matches: Vec<&Package> = packages
        .iter()
        .filter(|package| {
            package
                .name
                .as_deref()
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(name))
        })
        .filter(|package| {
            protocol.is_none_or(|protocol| {
                package
                    .protocol_type
                    .as_deref()
                    .is_some_and(|candidate| candidate.eq_ignore_ascii_case(protocol))
            })
        })
        .collect();

    match matches.as_slice() {
        [package] => Ok(package),
        [] => Err(anyhow!("No package named '{name}' was found in the feed")),
        _ => Err(anyhow!(
            "Several packages are named '{name}' ({}). Pass --protocol to pick one",
            matches
                .iter()
                .filter_map(|package| package.protocol_type.as_deref())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

async fn list_package_versions(
    project: Option<&str>,
    feed: &str,
    name: &str,
    protocol: Option<&str>,
) -> Result<Vec<PackageVersion>> {
    let packages = list_packages(project, feed, protocol, Some(name)).await?;
    let package_id = find_package(&packages, name, protocol)?
        .id
        .clone()
        .ok_or_else(|| anyhow!("The package '{name}' has no id"))?;

    let (client, organization) = create_artifacts_client()?;
    let (organization, project) = scoped_path(&organization, project);

    Ok(client
        .artifact_details_client()
        .get_package_versions(organization, feed, package_id, project)
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

fn latest_version(package: &Package) -> &str {
    package
        .versions
        .iter()
        .find(|version| version.is_latest == Some(true))
        .or_else(|| package.versions.first())
        .and_then(|version| version.version.as_deref())
        .unwrap_or("-")
}

fn display_packages(packages: &[Package]) {
    if packages.is_empty() {
        println!("No packages found.");
        return;
    }

    println!(
        "{:<50} {:<10} {}",
        "Name".bold(),
        "Protocol".bold(),
        "Latest version".bold()
    );
    println!("{}", "-".repeat(80));

    for package in packages {
        println!(
            "{:<50} {:<10} {}",
            package.name.as_deref().unwrap_or("-"),
            package.protocol_type.as_deref().unwrap_or("-"),
            latest_version(package)
        );
    }
}

fn display_package_versions(versions: &[PackageVersion]) {
    if versions.is_empty() {
        println!("No versions found.");
        return;
    }

    println!(
        "{:<24} {:<12} {:<8} {}",
        "Version".bold(),
        "Published".bold(),
        "Latest".bold(),
        "Description".bold()
    );
    println!("{}", "-".repeat(80));

    for version in versions {
        let version = &version.minimal_package_version;
        println!(
            "{:<24} {:<12} {:<8} {}",
            version.version.as_deref().unwrap_or("-"),
            version
                .publish_date
                .map(|published| published.date().to_string())
                .unwrap_or_else(|| "-".to_string()),
            if version.is_latest == Some(true) {
                "yes"
            } else {
                ""
            },
            version
                .package_description
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
        ArtifactsSubCommands::Packages {
            subcommand:
                PackagesSubCommands::List {
                    feed,
                    protocol,
                    name,
                    scope,
                },
        } => {
            let project = scope.resolve_project()?;
            match list_packages(
                project.as_deref(),
                feed,
                protocol.as_deref(),
                name.as_deref(),
            )
            .await
            {
                Ok(packages) => display_packages(&packages),
                Err(e) => {
                    eprintln!(
                        "❌ Failed to list the packages of feed '{feed}' in {}",
                        scope_label(project.as_deref())
                    );
                    eprintln!("   {e}");
                    return Err(e);
                }
            }
        }
        ArtifactsSubCommands::Packages {
            subcommand:
                PackagesSubCommands::Versions {
                    feed,
                    name,
                    protocol,
                    scope,
                },
        } => {
            let project = scope.resolve_project()?;
            match list_package_versions(project.as_deref(), feed, name, protocol.as_deref()).await {
                Ok(versions) => display_package_versions(&versions),
                Err(e) => {
                    eprintln!(
                        "❌ Failed to list the versions of package '{name}' in feed '{feed}'"
                    );
                    eprintln!("   {e}");
                    return Err(e);
                }
            }
        }
        ArtifactsSubCommands::Universal {
            subcommand:
                UniversalSubCommands::Download {
                    feed,
                    name,
                    version,
                    path,
                    file_filter,
                    scope,
                },
        } => {
            let project = scope.resolve_project()?;
            let request = download::DownloadRequest {
                project: project.as_deref(),
                feed,
                name,
                version,
                path,
                file_filter: file_filter.as_deref(),
            };
            println!("⬇️  Downloading {name} {version} from feed '{feed}'...");
            match download::download_universal_package(&get_credentials()?, &request).await {
                Ok(files) => println!("✅ Downloaded {files} file(s) to {}", path.display()),
                Err(e) => {
                    eprintln!("❌ Failed to download {name} {version} from feed '{feed}'");
                    eprintln!("   {e}");
                    return Err(e);
                }
            }
        }
        ArtifactsSubCommands::Universal {
            subcommand:
                UniversalSubCommands::Publish {
                    feed,
                    name,
                    version,
                    path,
                    description,
                    scope,
                },
        } => {
            let project = scope.resolve_project()?;
            let request = artifact_tool::PublishRequest {
                project: project.as_deref(),
                feed,
                name,
                version,
                path,
                description: description.as_deref(),
            };
            println!("⬆️  Publishing {name} {version} to feed '{feed}'...");
            match artifact_tool::publish_universal_package(&get_credentials()?, &request).await {
                Ok(()) => println!("✅ Published {name} {version} to feed '{feed}'"),
                Err(e) => {
                    eprintln!("❌ Failed to publish {name} {version} to feed '{feed}'");
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

    fn package(name: &str, protocol: &str) -> Package {
        let mut package = Package::new();
        package.name = Some(name.to_string());
        package.protocol_type = Some(protocol.to_string());
        package
    }

    #[test]
    fn find_package_matches_whole_name_ignoring_case() {
        let packages = [package("tool-extras", "upack"), package("Tool", "upack")];
        let found = find_package(&packages, "tool", None).unwrap();
        assert_eq!(found.name.as_deref(), Some("Tool"));
    }

    #[test]
    fn find_package_reports_missing_package() {
        let packages = [package("tool-extras", "upack")];
        assert!(find_package(&packages, "tool", None).is_err());
    }

    #[test]
    fn find_package_needs_protocol_when_names_collide() {
        let packages = [package("tool", "upack"), package("tool", "NuGet")];
        let error = find_package(&packages, "tool", None).unwrap_err();
        assert!(error.to_string().contains("--protocol"));

        let found = find_package(&packages, "tool", Some("nuget")).unwrap();
        assert_eq!(found.protocol_type.as_deref(), Some("NuGet"));
    }
}
