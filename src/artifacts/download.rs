//! Native download of Universal Packages.
//!
//! The SDK implements the dedup download protocol, but its one-shot
//! `download_universal_package` always addresses the feed through a project
//! and writes every file. The building blocks are public, so the download is
//! assembled here to support organization-scoped feeds, a file filter and a
//! guard against package paths that would escape the target directory.

use crate::auth::url::is_default_cloud_host;
use crate::auth::Credentials;
use anyhow::{anyhow, Context, Result};
use azure_devops_rust_api::artifacts_download::{
    decompress_chunk, Client, ClientBuilder, ManifestItem,
};
use azure_devops_rust_api::Credential;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

/// The dedup service resolves blob ids to download URLs in batches; this keeps
/// a large file's request body to a sensible size.
const BLOB_URL_BATCH_SIZE: usize = 500;

pub struct DownloadRequest<'a> {
    pub project: Option<&'a str>,
    pub feed: &'a str,
    pub name: &'a str,
    pub version: &'a str,
    pub path: &'a Path,
    pub file_filter: Option<&'a str>,
}

/// Splits the packaging service URL into the base and the segment that goes
/// in front of `_packaging`, which is the project for a project-scoped feed
/// and the organization itself for an organization-scoped one.
fn packaging_location(packages_url: &str, project: Option<&str>) -> Result<(String, String)> {
    let packages_url = packages_url.trim_end_matches('/');
    match project {
        Some(project) => Ok((packages_url.to_string(), project.to_string())),
        None => packages_url
            .rsplit_once('/')
            .map(|(base, organization)| (base.to_string(), organization.to_string()))
            .ok_or_else(|| anyhow!("Unexpected packaging service URL: {packages_url}")),
    }
}

/// Matches `text` against a wildcard pattern where `*` matches any run of
/// characters (including `/`) and `?` matches exactly one character.
fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0, 0);
    let mut backtrack: Option<(usize, usize)> = None;

    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            backtrack = Some((p, t));
            p += 1;
        } else if let Some((star, matched)) = backtrack {
            p = star + 1;
            t = matched + 1;
            backtrack = Some((star, matched + 1));
        } else {
            return false;
        }
    }

    pattern[p..].iter().all(|c| *c == '*')
}

/// Package paths come from the service and are written below the target
/// directory, so anything that could step outside it is refused.
fn safe_relative_path(package_path: &str) -> Result<PathBuf> {
    let relative = Path::new(package_path.trim_start_matches('/'));
    if relative.as_os_str().is_empty()
        || !relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(anyhow!(
            "Refusing to write package file with unsafe path '{package_path}'"
        ));
    }
    Ok(relative.to_path_buf())
}

fn selected_items<'a>(
    items: &'a [ManifestItem],
    file_filter: Option<&str>,
) -> Vec<&'a ManifestItem> {
    items
        .iter()
        .filter(|item| {
            file_filter
                .is_none_or(|filter| wildcard_match(filter, item.path.trim_start_matches('/')))
        })
        .collect()
}

async fn resolve_one(client: &Client, blob_service_url: &str, blob_id: &str) -> Result<String> {
    client
        .resolve_blob_urls(blob_service_url, &[blob_id.to_string()])
        .await?
        .remove(blob_id)
        .ok_or_else(|| anyhow!("The blob store returned no URL for blob {blob_id}"))
}

/// Writes one package file. Small files are a single content blob; larger
/// ones are a node blob listing compressed chunks, which are streamed to disk
/// in order.
async fn download_file(
    client: &Client,
    blob_service_url: &str,
    item: &ManifestItem,
    target: &Path,
    progress: &ProgressBar,
) -> Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }
    let mut file = File::create(target)
        .with_context(|| format!("Failed to create file {}", target.display()))?;

    let root_url = resolve_one(client, blob_service_url, &item.blob.id).await?;
    let root = client.download_blob(&root_url).await?;

    if !item.blob.id.ends_with("02") {
        file.write_all(&root)?;
        progress.inc(root.len() as u64);
        return Ok(());
    }

    let chunk_ids = Client::parse_dedup_node(&root)?;
    for batch in chunk_ids.chunks(BLOB_URL_BATCH_SIZE) {
        let mut urls = client.resolve_blob_urls(blob_service_url, batch).await?;
        for chunk_id in batch {
            let url = urls
                .remove(chunk_id)
                .ok_or_else(|| anyhow!("The blob store returned no URL for chunk {chunk_id}"))?;
            let chunk = decompress_chunk(&client.download_blob(&url).await?)?;
            file.write_all(&chunk)?;
            progress.inc(chunk.len() as u64);
        }
    }

    Ok(())
}

/// Downloads a Universal Package version and returns the number of files written.
pub async fn download_universal_package(
    creds: &Credentials,
    request: &DownloadRequest<'_>,
) -> Result<usize> {
    if !is_default_cloud_host(&creds.base_url) {
        return Err(anyhow!(
            "Universal Packages are only available in Azure DevOps Services (https://dev.azure.com)"
        ));
    }

    let client = ClientBuilder::new(Credential::Pat(creds.pat.clone())).build();
    let services = client.discover_services(&creds.organization).await?;
    let packages_url = Client::find_packages_url(&services, &creds.organization);
    let blob_service_url = Client::find_blob_url(&services)?;

    let (packaging_base, packaging_segment) = packaging_location(&packages_url, request.project)?;
    let metadata = client
        .get_package_metadata(
            &packaging_base,
            &packaging_segment,
            request.feed,
            request.name,
            request.version,
        )
        .await?;

    let manifest_url = resolve_one(&client, &blob_service_url, &metadata.manifest_id).await?;
    let manifest = Client::parse_manifest(&client.download_blob(&manifest_url).await?)?;

    let items = selected_items(&manifest.items, request.file_filter);
    if items.is_empty() {
        return Err(anyhow!(
            "No files in {} {} match the file filter",
            request.name,
            request.version
        ));
    }
    let targets = items
        .iter()
        .map(|item| Ok(request.path.join(safe_relative_path(&item.path)?)))
        .collect::<Result<Vec<_>>>()?;

    fs::create_dir_all(request.path)
        .with_context(|| format!("Failed to create directory {}", request.path.display()))?;

    let progress = ProgressBar::new(items.iter().map(|item| item.blob.size).sum());
    progress.set_style(
        ProgressStyle::with_template("[{bar:30.cyan/blue}] {bytes}/{total_bytes} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_bar())
            .progress_chars("##-"),
    );
    for (item, target) in items.iter().zip(&targets) {
        progress.set_message(item.path.trim_start_matches('/').to_string());
        download_file(&client, &blob_service_url, item, target, &progress).await?;
    }
    progress.finish_and_clear();

    Ok(items.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packaging_location_uses_project_for_project_scope() {
        assert_eq!(
            packaging_location("https://pkgs.dev.azure.com/contoso/", Some("web")).unwrap(),
            (
                "https://pkgs.dev.azure.com/contoso".to_string(),
                "web".to_string()
            )
        );
    }

    #[test]
    fn packaging_location_uses_organization_for_organization_scope() {
        assert_eq!(
            packaging_location("https://pkgs.dev.azure.com/contoso/", None).unwrap(),
            (
                "https://pkgs.dev.azure.com".to_string(),
                "contoso".to_string()
            )
        );
    }

    #[test]
    fn wildcard_match_supports_star_and_question_mark() {
        assert!(wildcard_match("*.dll", "bin/tool.dll"));
        assert!(wildcard_match("bin/*", "bin/tool.dll"));
        assert!(wildcard_match("tool.???", "tool.exe"));
        assert!(wildcard_match("*", "anything"));
        assert!(!wildcard_match("*.dll", "bin/tool.exe"));
        assert!(!wildcard_match("tool.??", "tool.exe"));
    }

    #[test]
    fn safe_relative_path_accepts_nested_paths() {
        assert_eq!(
            safe_relative_path("/bin/tool.dll").unwrap(),
            Path::new("bin").join("tool.dll")
        );
    }

    #[test]
    fn safe_relative_path_rejects_paths_that_escape() {
        assert!(safe_relative_path("/../outside.txt").is_err());
        assert!(safe_relative_path("bin/../../outside.txt").is_err());
        assert!(safe_relative_path("/").is_err());
    }
}
