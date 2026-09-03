//! URL helpers for Azure DevOps API and web links.
//!
//! Centralizes construction of all organization-scoped URLs so that enterprise
//! installations with a custom base URL work consistently across the CLI.

use ::url::Url;
use anyhow::{anyhow, Result};

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

/// Detects whether the supplied value is a full URL.
///
/// If the input starts with `http://` or `https://`, it is parsed as a URL and
/// validated. The organization or collection name is extracted from the last
/// path segment, if any, and the preceding URL (scheme + host + optional port +
/// any remaining path) is returned as the base URL. When the URL has no path
/// segments, an empty string is returned for the organization so the caller can
/// prompt for it. Non-URL input is returned as the organization name with `None`
/// for the base URL.
///
/// Malformed URLs, URLs without a host, and URLs with query or fragment
/// components are rejected.
pub fn parse_organization_or_url(input: &str) -> Result<(Option<String>, String)> {
    let trimmed = input.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        let parsed = Url::parse(trimmed)
            .map_err(|e| anyhow!("Invalid Azure DevOps base URL '{}': {}", trimmed, e))?;
        if parsed.scheme() != "http" && parsed.scheme() != "https" {
            return Err(anyhow!(
                "Unsupported URL scheme '{}'. Only http and https are allowed.",
                parsed.scheme()
            ));
        }
        let host = parsed
            .host_str()
            .filter(|h| !h.is_empty())
            .ok_or_else(|| anyhow!("Azure DevOps base URL '{}' is missing a host", trimmed))?;
        if parsed.query().is_some() || parsed.fragment().is_some() {
            return Err(anyhow!(
                "Azure DevOps base URL '{}' must not contain query or fragment components",
                trimmed
            ));
        }

        let path = parsed.path().trim_end_matches('/');
        let mut segments = path.split('/').skip(1).collect::<Vec<_>>();
        let organization = segments.pop().unwrap_or("").to_string();
        let base_path = segments.join("/");

        let mut base_url = format!("{}://{}", parsed.scheme(), host);
        if let Some(port) = parsed.port() {
            base_url.push(':');
            base_url.push_str(&port.to_string());
        }
        if !base_path.is_empty() {
            base_url.push('/');
            base_url.push_str(&base_path);
        }

        Ok((Some(base_url), organization))
    } else {
        Ok((None, trimmed.to_string()))
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

/// Percent-encodes a single URL path segment.
pub fn percent_encode_path_segment(segment: &str) -> String {
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
        let (base_url, organization) = parse_organization_or_url("https://devops.mycompany.com/")
            .expect("valid URL should parse");
        assert_eq!(base_url, Some("https://devops.mycompany.com".to_string()));
        assert_eq!(organization, "");
    }

    #[test]
    fn parse_organization_or_url_treats_plain_input_as_organization() {
        let (base_url, organization) =
            parse_organization_or_url("mycompany").expect("plain input should parse");
        assert_eq!(base_url, None);
        assert_eq!(organization, "mycompany");
    }

    #[test]
    fn parse_organization_or_url_extracts_organization_from_path() {
        let (base_url, organization) =
            parse_organization_or_url("https://tfs.mycompany.com/tfs/DefaultCollection")
                .expect("valid enterprise URL should parse");
        assert_eq!(base_url, Some("https://tfs.mycompany.com/tfs".to_string()));
        assert_eq!(organization, "DefaultCollection");
    }

    #[test]
    fn parse_organization_or_url_extracts_cloud_organization_from_path() {
        let (base_url, organization) =
            parse_organization_or_url("https://dev.azure.com/mycompany/")
                .expect("valid cloud URL should parse");
        assert_eq!(base_url, Some("https://dev.azure.com".to_string()));
        assert_eq!(organization, "mycompany");
    }

    #[test]
    fn parse_organization_or_url_preserves_port() {
        let (base_url, organization) =
            parse_organization_or_url("https://devops.mycompany.com:8443/")
                .expect("valid URL with port should parse");
        assert_eq!(
            base_url,
            Some("https://devops.mycompany.com:8443".to_string())
        );
        assert_eq!(organization, "");
    }

    #[test]
    fn parse_organization_or_url_rejects_malformed_url() {
        let result = parse_organization_or_url("https://");
        assert!(
            result.is_err(),
            "malformed URL without host should be rejected"
        );
    }

    #[test]
    fn parse_organization_or_url_rejects_url_with_query() {
        let result =
            parse_organization_or_url("https://devops.mycompany.com/?collection=DefaultCollection");
        assert!(result.is_err(), "URL with query string should be rejected");
    }

    #[test]
    fn parse_organization_or_url_rejects_url_with_fragment() {
        let result = parse_organization_or_url("https://devops.mycompany.com/#section");
        assert!(result.is_err(), "URL with fragment should be rejected");
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
