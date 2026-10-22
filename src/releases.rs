use anyhow::{anyhow, Result};
use azure_devops_rust_api::release::models::{
    Artifact, ArtifactMetadata, BuildVersion, ReleaseStartMetadata,
};

fn artifact_aliases(artifacts: &[Artifact]) -> String {
    artifacts
        .iter()
        .filter_map(|artifact| artifact.alias.as_deref())
        .collect::<Vec<_>>()
        .join(", ")
}

/// A release definition can link several artifacts, so the build has to be
/// attached to one of them by its alias.
fn resolve_artifact_alias(artifacts: &[Artifact], alias: Option<&str>) -> Result<String> {
    if let Some(alias) = alias {
        if !artifacts
            .iter()
            .any(|artifact| artifact.alias.as_deref() == Some(alias))
        {
            return Err(anyhow!(
                "The release definition has no artifact with alias '{alias}'. Available aliases: {}",
                artifact_aliases(artifacts)
            ));
        }
        return Ok(alias.to_string());
    }

    artifacts
        .iter()
        .find(|artifact| {
            artifact.is_primary == Some(true) && artifact.type_.as_deref() == Some("Build")
        })
        .and_then(|artifact| artifact.alias.clone())
        .ok_or_else(|| {
            anyhow!(
                "The release definition has no primary build artifact. Pass --artifact-alias with one of: {}",
                artifact_aliases(artifacts)
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(error.to_string().contains("_repo, _app"));
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
