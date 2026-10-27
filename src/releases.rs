use crate::auth::factory::{ClientFactory, CredentialClientFactory};
use crate::auth::get_credentials;
use crate::project::get_project_or_default;
use crate::text::escape_control_characters;
use anyhow::{anyhow, Result};
use azure_core::http::headers::HeaderName;
use azure_core::http::RawResponse;
use azure_devops_rust_api::release::models::{
    Artifact, ArtifactMetadata, BuildVersion, Release, ReleaseDefinition, ReleaseDefinitionList,
    ReleaseList, ReleaseStartMetadata,
};
use azure_devops_rust_api::release::Client;
use clap::Subcommand;
use colored::Colorize;
use serde_json::Value;

#[derive(Subcommand, Clone)]
pub enum ReleasesSubCommands {
    /// List the release definitions (release pipelines) of a project
    Definitions {
        /// Only list definitions whose name contains this text
        #[clap(long)]
        name: Option<String>,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
    /// List the releases of a project
    List {
        /// Only list releases of this release definition
        #[clap(short, long)]
        definition_id: Option<i32>,
        /// Maximum number of releases to return
        #[clap(long)]
        top: Option<i32>,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
    /// Show a release and the status of its stages
    Show {
        /// ID of the release
        #[clap(short, long)]
        id: i32,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
    /// Create a release that deploys a specific build
    Create {
        /// ID of the release definition (release pipeline) to create the release from
        #[clap(short, long)]
        definition_id: i32,
        /// ID of the build to release
        #[clap(short, long)]
        build_id: i32,
        /// Alias of the definition artifact the build belongs to (defaults to the primary build artifact)
        #[clap(short, long)]
        artifact_alias: Option<String>,
        /// Description of the release
        #[clap(long)]
        description: Option<String>,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
}

fn is_build(artifact: &Artifact) -> bool {
    artifact.type_.as_deref() == Some("Build")
}

fn artifact_aliases<'a>(artifacts: impl Iterator<Item = &'a Artifact>) -> String {
    artifacts
        .filter_map(|artifact| artifact.alias.as_deref())
        .collect::<Vec<_>>()
        .join(", ")
}

/// A release definition can link several artifacts, so the build has to be
/// attached to one of them by its alias.
fn resolve_artifact_alias(artifacts: &[Artifact], alias: Option<&str>) -> Result<String> {
    if let Some(alias) = alias {
        // The build id only means something to a Build artifact, so pinning it
        // to a Git or other artifact would leave the build itself unpinned.
        if !artifacts
            .iter()
            .any(|artifact| artifact.alias.as_deref() == Some(alias) && is_build(artifact))
        {
            return Err(anyhow!(
                "The release definition has no build artifact with alias '{alias}'. Available build aliases: {}",
                artifact_aliases(artifacts.iter().filter(|artifact| is_build(artifact)))
            ));
        }
        return Ok(alias.to_string());
    }

    artifacts
        .iter()
        .find(|artifact| artifact.is_primary == Some(true) && is_build(artifact))
        .and_then(|artifact| artifact.alias.clone())
        .ok_or_else(|| {
            anyhow!(
                "The release definition has no primary build artifact. Pass --artifact-alias with one of: {}",
                artifact_aliases(artifacts.iter().filter(|artifact| is_build(artifact)))
            )
        })
}

/// Pinning the artifact's instance reference to the build id is what makes the
/// release deploy that build instead of the artifact's default version.
fn release_start_metadata(
    definition_id: i32,
    alias: &str,
    build_id: i32,
    description: Option<&str>,
) -> ReleaseStartMetadata {
    let mut instance_reference = BuildVersion::new();
    instance_reference.id = Some(build_id.to_string());

    let mut artifact = ArtifactMetadata::new();
    artifact.alias = Some(alias.to_string());
    artifact.instance_reference = Some(instance_reference);

    let mut metadata = ReleaseStartMetadata::new();
    metadata.definition_id = Some(definition_id);
    metadata.description = description.map(str::to_string);
    metadata.artifacts = vec![artifact];
    metadata
}

fn create_release_client() -> Result<(Client, String)> {
    let creds = get_credentials()?;
    let factory = CredentialClientFactory::new(&creds)?;
    Ok((factory.build_release(), creds.organization))
}

/// The release API returns one page at a time and only signals that more
/// pages exist through this response header.
pub(crate) fn continuation_token(response: &RawResponse) -> Option<String> {
    response
        .headers()
        .get_optional_string(&HeaderName::from_static("x-ms-continuationtoken"))
        .filter(|token| !token.is_empty())
}

async fn list_release_definitions(
    project: &str,
    name: Option<&str>,
) -> Result<Vec<ReleaseDefinition>> {
    let (client, organization) = create_release_client()?;
    let mut definitions = Vec::new();
    let mut token = None::<String>;

    loop {
        let mut request = client.definitions_client().list(&organization, project);
        if let Some(name) = name {
            request = request.search_text(name);
        }
        if let Some(token) = token.take() {
            request = request.continuation_token(token);
        }

        let response = request.send().await?.into_raw_response();
        token = continuation_token(&response);
        let page: ReleaseDefinitionList = response.into_body().json()?;
        definitions.extend(page.value);

        if token.is_none() {
            break;
        }
    }

    Ok(definitions)
}

async fn list_releases(
    project: &str,
    definition_id: Option<i32>,
    top: Option<i32>,
) -> Result<Vec<Release>> {
    let (client, organization) = create_release_client()?;
    let mut releases = Vec::new();
    let mut token = None::<i32>;

    loop {
        let mut request = client.releases_client().list(&organization, project);
        if let Some(definition_id) = definition_id {
            request = request.definition_id(definition_id);
        }
        if let Some(top) = top {
            request = request.top(top - releases.len() as i32);
        }
        if let Some(token) = token.take() {
            request = request.continuation_token(token);
        }

        let response = request.send().await?.into_raw_response();
        token = continuation_token(&response).and_then(|token| token.parse().ok());
        let page: ReleaseList = response.into_body().json()?;
        releases.extend(page.value);

        if let Some(top) = top {
            if releases.len() >= top.max(0) as usize {
                releases.truncate(top.max(0) as usize);
                break;
            }
        }
        if token.is_none() {
            break;
        }
    }

    Ok(releases)
}

async fn get_release(project: &str, release_id: i32) -> Result<Release> {
    let (client, organization) = create_release_client()?;

    Ok(client
        .releases_client()
        .get_release(&organization, project, release_id)
        .await?)
}

async fn create_release(
    project: &str,
    definition_id: i32,
    build_id: i32,
    artifact_alias: Option<&str>,
    description: Option<&str>,
) -> Result<Release> {
    let (client, organization) = create_release_client()?;

    let definition = client
        .definitions_client()
        .get(&organization, project, definition_id)
        .await?;
    let alias = resolve_artifact_alias(&definition.artifacts, artifact_alias)?;

    Ok(client
        .releases_client()
        .create(
            &organization,
            release_start_metadata(definition_id, &alias, build_id, description),
            project,
        )
        .await?)
}

fn display_release_definitions(definitions: &[ReleaseDefinition]) {
    if definitions.is_empty() {
        println!("No release definitions found.");
        return;
    }

    println!(
        "{:<10} {:<40} {}",
        "ID".bold(),
        "Name".bold(),
        "Path".bold()
    );
    println!("{}", "-".repeat(70));

    for definition in definitions {
        let reference = &definition.release_definition_shallow_reference;
        println!(
            "{:<10} {:<40} {}",
            reference
                .id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "-".to_string()),
            reference.name.as_deref().unwrap_or("-"),
            reference.path.as_deref().unwrap_or("-")
        );
    }
}

fn display_releases(releases: &[Release]) {
    if releases.is_empty() {
        println!("No releases found.");
        return;
    }

    println!(
        "{:<10} {:<24} {:<12} {:<30} {}",
        "ID".bold(),
        "Name".bold(),
        "Status".bold(),
        "Definition".bold(),
        "Created".bold()
    );
    println!("{}", "-".repeat(90));

    for release in releases {
        println!(
            "{:<10} {:<24} {:<12} {:<30} {}",
            release
                .id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "-".to_string()),
            release.name.as_deref().unwrap_or("-"),
            release
                .status
                .as_ref()
                .map(|status| format!("{status:?}"))
                .unwrap_or_else(|| "-".to_string()),
            release
                .release_definition
                .as_ref()
                .and_then(|definition| definition.name.as_deref())
                .unwrap_or("-"),
            release
                .created_on
                .map(|created| created.date().to_string())
                .unwrap_or_else(|| "-".to_string())
        );
    }
}

/// A freshly created release can report an empty version name, so the
/// version id is the only way to see which build it pinned.
fn artifact_version(definition_reference: &Value) -> Option<&str> {
    let version = definition_reference.get("version")?;
    ["name", "id"]
        .iter()
        .filter_map(|field| version.get(field)?.as_str())
        .find(|value| !value.is_empty())
}

fn web_url(links: Option<&Value>) -> Option<&str> {
    links?.get("web")?.get("href")?.as_str()
}

fn display_release(release: &Release) {
    println!("🚀 Release");
    println!("==========");
    println!(
        "ID: {}",
        release
            .id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    println!("Name: {}", release.name.as_deref().unwrap_or("-"));

    if let Some(definition) = &release.release_definition {
        println!("Definition: {}", definition.name.as_deref().unwrap_or("-"));
    }
    if let Some(status) = &release.status {
        println!("Status: {status:?}");
    }
    if let Some(description) = &release.description {
        println!("Description: {}", escape_control_characters(description));
    }
    if let Some(created_on) = release.created_on {
        println!("Created: {created_on}");
    }

    for artifact in &release.artifacts {
        let version = artifact
            .definition_reference
            .as_ref()
            .and_then(|reference| artifact_version(reference))
            .unwrap_or("-");
        println!(
            "Artifact: {} ({version})",
            artifact.alias.as_deref().unwrap_or("-")
        );
    }

    if !release.environments.is_empty() {
        println!("\nStages:");
        for environment in &release.environments {
            println!(
                "  {} - {}",
                environment.name.as_deref().unwrap_or("-"),
                environment
                    .status
                    .as_ref()
                    .map(|status| format!("{status:?}"))
                    .unwrap_or_else(|| "-".to_string())
            );
        }
    }

    if let Some(url) = web_url(release.links.as_ref()) {
        println!("URL: {url}");
    }
}

pub async fn handle_command(subcommand: &ReleasesSubCommands) -> Result<()> {
    match subcommand {
        ReleasesSubCommands::Definitions { name, project } => {
            let project_name = get_project_or_default(project.as_deref())?;
            match list_release_definitions(&project_name, name.as_deref()).await {
                Ok(definitions) => display_release_definitions(&definitions),
                Err(e) => {
                    eprintln!("❌ Failed to list the release definitions of '{project_name}'");
                    eprintln!("   {e}");
                    return Err(e);
                }
            }
        }
        ReleasesSubCommands::List {
            definition_id,
            top,
            project,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            match list_releases(&project_name, *definition_id, *top).await {
                Ok(releases) => display_releases(&releases),
                Err(e) => {
                    eprintln!("❌ Failed to list the releases of '{project_name}'");
                    eprintln!("   {e}");
                    return Err(e);
                }
            }
        }
        ReleasesSubCommands::Show { id, project } => {
            let project_name = get_project_or_default(project.as_deref())?;
            match get_release(&project_name, *id).await {
                Ok(release) => display_release(&release),
                Err(e) => {
                    eprintln!("❌ Failed to retrieve release {id}");
                    eprintln!("   {e}");
                    return Err(e);
                }
            }
        }
        ReleasesSubCommands::Create {
            definition_id,
            build_id,
            artifact_alias,
            description,
            project,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            match create_release(
                &project_name,
                *definition_id,
                *build_id,
                artifact_alias.as_deref(),
                description.as_deref(),
            )
            .await
            {
                Ok(release) => {
                    println!("{}", "✅ Release created successfully".green());
                    display_release(&release);
                }
                Err(e) => {
                    eprintln!(
                        "{}",
                        format!("❌ Failed to create a release of build {build_id}: {e}").red()
                    );
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
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[clap(subcommand)]
        command: ReleasesSubCommands,
    }

    fn parse(args: &[&str]) -> Result<ReleasesSubCommands, clap::Error> {
        TestCli::try_parse_from(std::iter::once("releases").chain(args.iter().copied()))
            .map(|cli| cli.command)
    }

    #[test]
    fn create_takes_the_definition_and_the_build_to_release() {
        let command = parse(&[
            "create",
            "--definition-id",
            "5",
            "--build-id",
            "123",
            "--artifact-alias",
            "_app",
            "--description",
            "Hotfix",
        ])
        .unwrap();

        let ReleasesSubCommands::Create {
            definition_id,
            build_id,
            artifact_alias,
            description,
            project,
        } = command
        else {
            panic!("expected the create command");
        };
        assert_eq!(definition_id, 5);
        assert_eq!(build_id, 123);
        assert_eq!(artifact_alias.as_deref(), Some("_app"));
        assert_eq!(description.as_deref(), Some("Hotfix"));
        assert_eq!(project, None);
    }

    #[test]
    fn definitions_accepts_a_name_filter() {
        let command = parse(&["definitions", "--name", "web", "-p", "Shop"]).unwrap();

        let ReleasesSubCommands::Definitions { name, project } = command else {
            panic!("expected the definitions command");
        };
        assert_eq!(name.as_deref(), Some("web"));
        assert_eq!(project.as_deref(), Some("Shop"));
    }

    #[test]
    fn list_filters_by_definition_and_caps_the_results() {
        let command = parse(&["list", "--definition-id", "5", "--top", "10"]).unwrap();

        let ReleasesSubCommands::List {
            definition_id,
            top,
            project,
        } = command
        else {
            panic!("expected the list command");
        };
        assert_eq!(definition_id, Some(5));
        assert_eq!(top, Some(10));
        assert_eq!(project, None);
    }

    #[test]
    fn show_takes_a_numeric_release_id() {
        let ReleasesSubCommands::Show { id, project } = parse(&["show", "--id", "42"]).unwrap()
        else {
            panic!("expected the show command");
        };
        assert_eq!(id, 42);
        assert_eq!(project, None);

        assert!(parse(&["show", "--id", "abc"]).is_err());
    }

    #[test]
    fn create_requires_a_build_id() {
        assert!(parse(&["create", "--definition-id", "5"]).is_err());
    }

    #[test]
    fn create_rejects_a_non_numeric_build_id() {
        assert!(parse(&["create", "--definition-id", "5", "--build-id", "latest"]).is_err());
    }

    fn artifact(alias: &str, type_: &str, is_primary: bool) -> Artifact {
        let mut artifact = Artifact::new();
        artifact.alias = Some(alias.to_string());
        artifact.type_ = Some(type_.to_string());
        artifact.is_primary = Some(is_primary);
        artifact
    }

    #[test]
    fn resolve_artifact_alias_uses_the_requested_alias() {
        let artifacts = [
            artifact("_app", "Build", true),
            artifact("_infra", "Build", false),
        ];

        assert_eq!(
            resolve_artifact_alias(&artifacts, Some("_infra")).unwrap(),
            "_infra"
        );
    }

    #[test]
    fn resolve_artifact_alias_rejects_an_alias_the_definition_does_not_link() {
        let artifacts = [artifact("_app", "Build", true)];

        let error = resolve_artifact_alias(&artifacts, Some("_missing")).unwrap_err();

        assert!(error.to_string().contains("_missing"));
        assert!(error.to_string().contains("_app"));
    }

    #[test]
    fn resolve_artifact_alias_defaults_to_the_primary_build_artifact() {
        let artifacts = [
            artifact("_infra", "Build", false),
            artifact("_app", "Build", true),
        ];

        assert_eq!(resolve_artifact_alias(&artifacts, None).unwrap(), "_app");
    }

    #[test]
    fn resolve_artifact_alias_asks_for_an_alias_when_the_primary_is_not_a_build() {
        let artifacts = [
            artifact("_repo", "Git", true),
            artifact("_app", "Build", false),
        ];

        let error = resolve_artifact_alias(&artifacts, None).unwrap_err();

        assert!(error.to_string().contains("--artifact-alias"));
        assert!(error.to_string().contains("one of: _app"));
    }

    #[test]
    fn resolve_artifact_alias_rejects_an_alias_that_is_not_a_build() {
        let artifacts = [
            artifact("_repo", "Git", false),
            artifact("_app", "Build", true),
        ];

        let error = resolve_artifact_alias(&artifacts, Some("_repo")).unwrap_err();

        assert!(error.to_string().contains("'_repo'"));
        assert!(error.to_string().contains("Available build aliases: _app"));
    }

    #[test]
    fn artifact_version_prefers_the_version_name() {
        let reference = serde_json::json!({ "version": { "id": "123", "name": "20261022.1" } });

        assert_eq!(artifact_version(&reference), Some("20261022.1"));
    }

    #[test]
    fn artifact_version_falls_back_to_the_id_when_the_name_is_empty() {
        let reference = serde_json::json!({ "version": { "id": "123", "name": "" } });

        assert_eq!(artifact_version(&reference), Some("123"));
    }

    #[test]
    fn artifact_version_is_missing_without_a_usable_value() {
        let reference = serde_json::json!({ "version": { "id": "", "name": "" } });

        assert_eq!(artifact_version(&reference), None);
    }

    #[test]
    fn release_start_metadata_pins_the_artifact_to_the_build() {
        let metadata = release_start_metadata(5, "_app", 123, Some("Hotfix"));

        assert_eq!(
            serde_json::to_value(&metadata).unwrap(),
            serde_json::json!({
                "definitionId": 5,
                "description": "Hotfix",
                "artifacts": [
                    { "alias": "_app", "instanceReference": { "id": "123" } }
                ]
            })
        );
    }
}
