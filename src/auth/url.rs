//! URL helpers for Azure DevOps API and web links.
//!
//! Centralizes construction of all organization-scoped URLs so that enterprise
//! installations with a custom base URL work consistently across the CLI.

const DEFAULT_BASE_URL: &str = "https://dev.azure.com";
const DEFAULT_VSAEX_BASE_URL: &str = "https://vsaex.dev.azure.com";
const DEFAULT_VSRM_BASE_URL: &str = "https://vsrm.dev.azure.com";

/// Returns the default Azure DevOps cloud base URL.
pub fn default_base_url() -> String {
    DEFAULT_BASE_URL.to_string()
}

/// Trims trailing slashes from a base URL so path joining is predictable.
pub fn normalize_base_url(base_url: &str) -> String {
    base_url.trim_end_matches('/').to_string()
}

/// Detects whether the supplied value is a full URL. If it is, the value is
/// treated as the base URL and `None` is returned for the organization name.
/// Otherwise the value is the organization name and `None` is returned for the
/// base URL.
pub fn parse_organization_or_url(input: &str) -> (Option<String>, String) {
    let trimmed = input.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        (Some(normalize_base_url(trimmed)), String::new())
    } else {
        (None, trimmed.to_string())
    }
}

/// Builds the web URL for a project.
pub fn web_project_url(base_url: &str, organization: &str, project: &str) -> String {
    format!(
        "{}/{}/{}",
        normalize_base_url(base_url),
        percent_encode_path_segment(organization),
        percent_encode_path_segment(project)
    )
}

/// Builds the web URL for editing a work item.
pub fn web_work_item_url(
    base_url: &str,
    organization: &str,
    project: &str,
    work_item_id: &str,
) -> String {
    format!(
        "{}/{}/{}/_workitems/edit/{}",
        normalize_base_url(base_url),
        percent_encode_path_segment(organization),
        percent_encode_path_segment(project),
        percent_encode_path_segment(work_item_id)
    )
}

/// Returns true when the configured base URL is the default Azure DevOps cloud host.
pub fn is_default_cloud_host(base_url: &str) -> bool {
    normalize_base_url(base_url) == normalize_base_url(DEFAULT_BASE_URL)
}

/// Base URL for user entitlement APIs. Defaults to `https://vsaex.dev.azure.com`
/// because that is the cloud host for those APIs.
pub fn user_entitlements_base_url(base_url: &str) -> String {
    if is_default_cloud_host(base_url) {
        DEFAULT_VSAEX_BASE_URL.to_string()
    } else {
        normalize_base_url(base_url)
    }
}

/// Full URL for a single user entitlement.
pub fn user_entitlements_url(base_url: &str, organization: &str, user_id: &str) -> String {
    format!(
        "{}/{}/_apis/userentitlements/{}?api-version=7.1-preview",
        user_entitlements_base_url(base_url),
        percent_encode_path_segment(organization),
        percent_encode_path_segment(user_id)
    )
}

/// Base URL for release (classic pipeline) APIs. On the cloud host this lives
/// on `https://vsrm.dev.azure.com`; otherwise it is assumed to share the same
/// custom base URL.
pub fn release_base_url(base_url: &str) -> String {
    if is_default_cloud_host(base_url) {
        DEFAULT_VSRM_BASE_URL.to_string()
    } else {
        normalize_base_url(base_url)
    }
}

fn percent_encode_path_segment(segment: &str) -> String {
    let mut encoded = String::new();
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_base_url_is_cloud_host() {
        assert_eq!(default_base_url(), "https://dev.azure.com");
    }

    #[test]
    fn normalize_base_url_trims_trailing_slashes() {
        assert_eq!(
            normalize_base_url("https://dev.azure.com/"),
            "https://dev.azure.com"
        );
        assert_eq!(
            normalize_base_url("https://dev.azure.com//"),
            "https://dev.azure.com"
        );
        assert_eq!(
            normalize_base_url("https://tfs.mycompany.com/tfs"),
            "https://tfs.mycompany.com/tfs"
        );
    }

    #[test]
    fn parse_organization_or_url_detects_url() {
        let (base_url, organization) = parse_organization_or_url("https://devops.mycompany.com/");
        assert_eq!(base_url, Some("https://devops.mycompany.com".to_string()));
        assert_eq!(organization, "");
    }

    #[test]
    fn parse_organization_or_url_treats_plain_input_as_organization() {
        let (base_url, organization) = parse_organization_or_url("mycompany");
        assert_eq!(base_url, None);
        assert_eq!(organization, "mycompany");
    }

    #[test]
    fn web_project_url_encodes_path_segments() {
        assert_eq!(
            web_project_url("https://dev.azure.com", "my company", "My Project"),
            "https://dev.azure.com/my%20company/My%20Project"
        );
    }

    #[test]
    fn web_work_item_url_includes_project() {
        assert_eq!(
            web_work_item_url("https://dev.azure.com", "mycompany", "MyProject", "42"),
            "https://dev.azure.com/mycompany/MyProject/_workitems/edit/42"
        );
    }

    #[test]
    fn is_default_cloud_host_matches_default() {
        assert!(is_default_cloud_host("https://dev.azure.com"));
        assert!(is_default_cloud_host("https://dev.azure.com/"));
        assert!(!is_default_cloud_host("https://devops.mycompany.com"));
    }

    #[test]
    fn user_entitlements_base_url_uses_vsaex_for_cloud() {
        assert_eq!(
            user_entitlements_base_url("https://dev.azure.com"),
            "https://vsaex.dev.azure.com"
        );
    }

    #[test]
    fn user_entitlements_base_url_uses_custom_host_for_enterprise() {
        assert_eq!(
            user_entitlements_base_url("https://devops.mycompany.com"),
            "https://devops.mycompany.com"
        );
    }

    #[test]
    fn user_entitlements_url_uses_vsaex_for_cloud() {
        assert_eq!(
            user_entitlements_url("https://dev.azure.com", "mycompany", "user-1"),
            "https://vsaex.dev.azure.com/mycompany/_apis/userentitlements/user-1?api-version=7.1-preview"
        );
    }

    #[test]
    fn user_entitlements_url_uses_custom_host_for_enterprise() {
        assert_eq!(
            user_entitlements_url("https://devops.mycompany.com", "mycompany", "user-1"),
            "https://devops.mycompany.com/mycompany/_apis/userentitlements/user-1?api-version=7.1-preview"
        );
    }

    #[test]
    fn release_base_url_uses_vsrm_for_cloud() {
        assert_eq!(
            release_base_url("https://dev.azure.com"),
            "https://vsrm.dev.azure.com"
        );
    }

    #[test]
    fn release_base_url_uses_custom_host_for_enterprise() {
        assert_eq!(
            release_base_url("https://devops.mycompany.com"),
            "https://devops.mycompany.com"
        );
    }
}
