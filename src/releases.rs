use anyhow::{anyhow, Result};
use azure_devops_rust_api::release::models::Artifact;

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
        .ok_or_else(|| todo!())
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
}
