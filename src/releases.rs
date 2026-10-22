use crate::auth::factory::{ClientFactory, CredentialClientFactory};
use crate::auth::get_credentials;
use anyhow::{anyhow, bail, Result};
use azure_devops_rust_api::release::models::{
    Artifact, ArtifactMetadata, BuildVersion, Release, ReleaseDefinition, ReleaseStartMetadata,
};

fn parse_id(value: &str, label: &str) -> Result<i32> {
    value
        .parse::<i32>()
        .map_err(|_| anyhow!("Invalid {label} ID '{value}', must be a number"))
}

fn build_artifact_aliases(artifacts: &[Artifact]) -> Vec<&str> {
    artifacts
        .iter()
        .filter(|artifact| {
            artifact
                .type_
                .as_deref()
                .is_some_and(|type_| type_.eq_ignore_ascii_case("Build"))
        })
        .filter_map(|artifact| artifact.alias.as_deref())
        .collect()
}

/// Picks the build artifact a release should be created from.
fn resolve_build_artifact_alias(artifacts: &[Artifact], requested: Option<&str>) -> Result<String> {
    let aliases = build_artifact_aliases(artifacts);
    if aliases.is_empty() {
        bail!("The release definition has no build artifacts");
    }

    match requested {
        Some(requested) => aliases
            .iter()
            .find(|alias| **alias == requested)
            .map(|alias| alias.to_string())
            .ok_or_else(|| {
                anyhow!(
                    "Build artifact '{requested}' not found, available aliases: {}",
                    aliases.join(", ")
                )
            }),
        None if aliases.len() == 1 => Ok(aliases[0].to_string()),
        None => bail!(
            "The release definition has several build artifacts, choose one with --alias: {}",
            aliases.join(", ")
        ),
    }
}

/// Builds the request that starts a release of `definition_id` from `build_id`.
fn build_release_start_metadata(
    definition_id: i32,
    alias: &str,
    build_id: i32,
    description: Option<&str>,
) -> ReleaseStartMetadata {
    let mut build = BuildVersion::new();
    build.id = Some(build_id.to_string());

    let mut artifact = ArtifactMetadata::new();
    artifact.alias = Some(alias.to_string());
    artifact.instance_reference = Some(build);

    let mut metadata = ReleaseStartMetadata::new();
    metadata.definition_id = Some(definition_id);
    metadata.artifacts = vec![artifact];
    metadata.description = description.map(str::to_string);
    metadata
}

async fn list_definitions(project: &str) -> Result<Vec<ReleaseDefinition>> {
    let creds = get_credentials()?;
    let factory = CredentialClientFactory::new(&creds)?;

    Ok(factory
        .build_release()
        .definitions_client()
        .list(creds.organization, project)
        .expand("artifacts")
        .await?
        .value)
}

async fn get_definition(project: &str, definition_id: i32) -> Result<ReleaseDefinition> {
    let creds = get_credentials()?;
    let factory = CredentialClientFactory::new(&creds)?;

    Ok(factory
        .build_release()
        .definitions_client()
        .get(creds.organization, project, definition_id)
        .await?)
}

async fn list_releases(
    project: &str,
    definition_id: Option<i32>,
    top: Option<i32>,
) -> Result<Vec<Release>> {
    let creds = get_credentials()?;
    let factory = CredentialClientFactory::new(&creds)?;

    let mut request = factory
        .build_release()
        .releases_client()
        .list(creds.organization, project);
    if let Some(definition_id) = definition_id {
        request = request.definition_id(definition_id);
    }
    if let Some(top) = top {
        request = request.top(top);
    }

    Ok(request.await?.value)
}

async fn get_release(project: &str, release_id: i32) -> Result<Release> {
    let creds = get_credentials()?;
    let factory = CredentialClientFactory::new(&creds)?;

    Ok(factory
        .build_release()
        .releases_client()
        .get_release(creds.organization, project, release_id)
        .await?)
}

async fn create_release(
    project: &str,
    definition_id: i32,
    build_id: i32,
    alias: Option<&str>,
    description: Option<&str>,
) -> Result<Release> {
    let definition = get_definition(project, definition_id).await?;
    let alias = resolve_build_artifact_alias(&definition.artifacts, alias)?;
    let metadata = build_release_start_metadata(definition_id, &alias, build_id, description);

    let creds = get_credentials()?;
    let factory = CredentialClientFactory::new(&creds)?;

    Ok(factory
        .build_release()
        .releases_client()
        .create(creds.organization, metadata, project)
        .await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artifact(alias: &str, type_: &str) -> Artifact {
        let mut artifact = Artifact::new();
        artifact.alias = Some(alias.to_string());
        artifact.type_ = Some(type_.to_string());
        artifact
    }

    #[test]
    fn resolve_alias_picks_the_only_build_artifact() {
        let artifacts = vec![artifact("_app", "Build"), artifact("_repo", "Git")];
        assert_eq!(
            resolve_build_artifact_alias(&artifacts, None).unwrap(),
            "_app"
        );
    }

    #[test]
    fn resolve_alias_uses_the_requested_build_artifact() {
        let artifacts = vec![artifact("_app", "Build"), artifact("_api", "Build")];
        assert_eq!(
            resolve_build_artifact_alias(&artifacts, Some("_api")).unwrap(),
            "_api"
        );
    }

    #[test]
    fn resolve_alias_rejects_an_unknown_alias() {
        let artifacts = vec![artifact("_app", "Build")];
        let error = resolve_build_artifact_alias(&artifacts, Some("_web")).unwrap_err();
        assert!(error.to_string().contains("_web"));
        assert!(error.to_string().contains("_app"));
    }

    #[test]
    fn resolve_alias_rejects_a_non_build_artifact() {
        let artifacts = vec![artifact("_app", "Build"), artifact("_repo", "Git")];
        assert!(resolve_build_artifact_alias(&artifacts, Some("_repo")).is_err());
    }

    #[test]
    fn resolve_alias_requires_a_choice_between_several_build_artifacts() {
        let artifacts = vec![artifact("_app", "Build"), artifact("_api", "Build")];
        let error = resolve_build_artifact_alias(&artifacts, None).unwrap_err();
        assert!(error.to_string().contains("--alias"));
        assert!(error.to_string().contains("_app, _api"));
    }

    #[test]
    fn resolve_alias_fails_without_build_artifacts() {
        let artifacts = vec![artifact("_repo", "Git")];
        assert!(resolve_build_artifact_alias(&artifacts, None).is_err());
    }

    #[test]
    fn start_metadata_pins_the_artifact_to_the_build() {
        let metadata = build_release_start_metadata(7, "_app", 1234, None);
        let body = serde_json::to_value(&metadata).unwrap();

        assert_eq!(
            body,
            serde_json::json!({
                "definitionId": 7,
                "artifacts": [
                    { "alias": "_app", "instanceReference": { "id": "1234" } }
                ]
            })
        );
    }

    #[test]
    fn start_metadata_includes_the_description() {
        let metadata = build_release_start_metadata(7, "_app", 1234, Some("Hotfix"));
        assert_eq!(metadata.description.as_deref(), Some("Hotfix"));
    }

    #[test]
    fn parse_id_rejects_non_numeric_input() {
        assert_eq!(parse_id("42", "build").unwrap(), 42);
        assert!(parse_id("abc", "build").is_err());
    }
}
