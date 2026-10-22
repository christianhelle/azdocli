use anyhow::Result;
use azure_devops_rust_api::release::models::Artifact;

/// A release definition can link several artifacts, so the build has to be
/// attached to one of them by its alias.
fn resolve_artifact_alias(artifacts: &[Artifact], alias: Option<&str>) -> Result<String> {
    if let Some(alias) = alias {
        return Ok(alias.to_string());
    }

    todo!()
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
}
