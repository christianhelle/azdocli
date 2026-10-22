use anyhow::Result;
use azure_devops_rust_api::release::models::Artifact;

/// Picks the build artifact a release should be created from.
fn resolve_build_artifact_alias(artifacts: &[Artifact], requested: Option<&str>) -> Result<String> {
    let _ = (artifacts, requested);
    todo!()
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
