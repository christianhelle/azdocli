//! Raw REST calls for the few pull request operations the SDK models cannot
//! express.
//!
//! Completing a pull request requires `lastMergeSourceCommit`, which
//! `GitPullRequestUpdateOptions` has no field for, so those requests are sent
//! directly. This mirrors the approach already used by
//! [`crate::migrate::http_client`].

use crate::auth::url::{normalize_base_url, percent_encode_path_segment};
use crate::auth::Credentials;
use anyhow::{anyhow, Context, Result};
use serde_json::Value;

const API_VERSION: &str = "7.1";

/// Builds the REST URL for a single pull request.
pub(super) fn pull_request_url(
    creds: &Credentials,
    project: &str,
    repository_id: &str,
    pull_request_id: i32,
) -> String {
    format!(
        "{}/{}/{}/_apis/git/repositories/{}/pullrequests/{}",
        normalize_base_url(&creds.base_url),
        percent_encode_path_segment(&creds.organization),
        percent_encode_path_segment(project),
        percent_encode_path_segment(repository_id),
        pull_request_id
    )
}

/// Sends a PATCH to a pull request and returns the updated resource.
pub(super) async fn patch_pull_request(
    creds: &Credentials,
    project: &str,
    repository_id: &str,
    pull_request_id: i32,
    body: &Value,
) -> Result<Value> {
    let url = pull_request_url(creds, project, repository_id, pull_request_id);

    let response = reqwest::Client::new()
        .patch(url)
        .query(&[("api-version", API_VERSION)])
        .basic_auth("", Some(&creds.pat))
        .json(body)
        .send()
        .await
        .context("Sending pull request update")?;

    let status = response.status();
    let text = response
        .text()
        .await
        .context("Reading pull request update response")?;

    if !status.is_success() {
        return Err(anyhow!(
            "Azure DevOps rejected the pull request update ({}): {}",
            status,
            error_message(&text)
        ));
    }

    serde_json::from_str(&text).context("Parsing pull request update response")
}

/// Returns the identity ID of the user the PAT belongs to.
pub(super) async fn authenticated_user_id(creds: &Credentials) -> Result<String> {
    let url = format!(
        "{}/{}/_apis/connectionData",
        normalize_base_url(&creds.base_url),
        percent_encode_path_segment(&creds.organization)
    );

    let response = reqwest::Client::new()
        .get(url)
        .query(&[("api-version", API_VERSION)])
        .basic_auth("", Some(&creds.pat))
        .send()
        .await
        .context("Requesting connection data")?;

    let status = response.status();
    let text = response
        .text()
        .await
        .context("Reading connection data response")?;

    if !status.is_success() {
        return Err(anyhow!(
            "Unable to determine the signed-in identity ({}): {}",
            status,
            error_message(&text)
        ));
    }

    let body: Value = serde_json::from_str(&text).context("Parsing connection data response")?;

    body.get("authenticatedUser")
        .and_then(|user| user.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow!("Connection data did not include the signed-in identity"))
}

/// Extracts the human-readable message from an Azure DevOps error payload.
fn error_message(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "no response body".to_string();
    }

    serde_json::from_str::<Value>(trimmed)
        .ok()
        .and_then(|value| {
            value
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creds() -> Credentials {
        Credentials {
            organization: "my org".to_string(),
            pat: "pat".to_string(),
            base_url: "https://dev.azure.com/".to_string(),
        }
    }

    #[test]
    fn pull_request_url_encodes_segments() {
        assert_eq!(
            pull_request_url(&creds(), "My Project", "repo-id", 42),
            "https://dev.azure.com/my%20org/My%20Project/_apis/git/repositories/repo-id/pullrequests/42"
        );
    }

    #[test]
    fn error_message_prefers_the_message_field() {
        assert_eq!(
            error_message(r#"{"message":"TF401398: cannot complete","typeKey":"X"}"#),
            "TF401398: cannot complete"
        );
    }

    #[test]
    fn error_message_falls_back_to_the_raw_body() {
        assert_eq!(error_message("  not json  "), "not json");
    }

    #[test]
    fn error_message_handles_an_empty_body() {
        assert_eq!(error_message("   "), "no response body");
    }
}
