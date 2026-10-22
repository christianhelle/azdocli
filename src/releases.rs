use anyhow::{anyhow, bail, Result};
use azure_devops_rust_api::release::models::Artifact;

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
}
