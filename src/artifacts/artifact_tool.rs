//! Publishing Universal Packages through Microsoft's ArtifactTool.
//!
//! Uploads use the dedup protocol, which the SDK does not implement. Like the
//! official `az artifacts universal publish`, the tool is fetched from the
//! organization's client tools API, cached per version and run with the PAT
//! handed over in an environment variable so it never shows up in argv.

use crate::auth::url::{
    is_default_cloud_host, normalize_base_url, percent_encode_path_segment, vsblob_base_url,
};
use crate::auth::Credentials;
use crate::config::get_config_dir;
use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::ffi::OsString;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Environment variable ArtifactTool reads the PAT from (`--patvar`).
const PAT_ENV_VAR: &str = "AZDOCLI_ARTIFACTTOOL_PAT";

#[derive(Deserialize)]
struct ReleaseInfo {
    uri: String,
    version: String,
}

pub struct PublishRequest<'a> {
    pub project: Option<&'a str>,
    pub feed: &'a str,
    pub name: &'a str,
    pub version: &'a str,
    pub path: &'a Path,
    pub description: Option<&'a str>,
}

/// Maps the running platform onto the `osName`/`arch` the client tools API expects.
fn platform(os: &str, arch: &str) -> Result<(&'static str, &'static str)> {
    let os_name = match os {
        "windows" => "windows",
        "linux" => "linux",
        "macos" => "darwin",
        other => return Err(anyhow!("ArtifactTool is not available for {other}")),
    };
    let arch = match arch {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => return Err(anyhow!("ArtifactTool is not available for {other}")),
    };
    Ok((os_name, arch))
}

fn executable_name() -> &'static str {
    if cfg!(windows) {
        "ArtifactTool.exe"
    } else {
        "ArtifactTool"
    }
}

fn release_info_url(creds: &Credentials) -> String {
    format!(
        "{}/{}/_apis/clienttools/ArtifactTool/release",
        vsblob_base_url(&creds.base_url),
        percent_encode_path_segment(&creds.organization)
    )
}

fn organization_url(creds: &Credentials) -> String {
    format!(
        "{}/{}/",
        normalize_base_url(&creds.base_url),
        percent_encode_path_segment(&creds.organization)
    )
}

/// Builds the ArtifactTool arguments. The PAT is deliberately not among them.
fn publish_args(organization_url: &str, request: &PublishRequest<'_>) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "universal",
        "publish",
        "--service",
        organization_url,
        "--patvar",
        PAT_ENV_VAR,
        "--feed",
        request.feed,
        "--package-name",
        request.name,
        "--package-version",
        request.version,
        "--path",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    args.push(request.path.as_os_str().to_owned());
    if let Some(description) = request.description {
        args.push("--description".into());
        args.push(description.into());
    }
    if let Some(project) = request.project {
        args.push("--project".into());
        args.push(project.into());
    }
    args
}

async fn fetch_release_info(creds: &Credentials) -> Result<ReleaseInfo> {
    let (os_name, arch) = platform(std::env::consts::OS, std::env::consts::ARCH)?;
    let response = reqwest::Client::new()
        .get(release_info_url(creds))
        .query(&[("osName", os_name), ("arch", arch)])
        .basic_auth("", Some(&creds.pat))
        .send()
        .await
        .context("Requesting ArtifactTool release information")?;

    let status = response.status();
    let text = response
        .text()
        .await
        .context("Reading ArtifactTool release information")?;
    if !status.is_success() {
        return Err(anyhow!(
            "Could not look up ArtifactTool ({status}): {}",
            text.trim()
        ));
    }
    serde_json::from_str(&text).context("Parsing ArtifactTool release information")
}

/// Staging directory for one installation attempt. It is unique per process
/// so concurrent first-time publishes never extract into each other's files,
/// and the version is kept whole (a dotted version has no "extension").
fn staging_dir(install_dir: &Path, process_id: u32) -> PathBuf {
    let mut name = install_dir.file_name().unwrap_or_default().to_owned();
    name.push(format!(".partial-{process_id}"));
    install_dir.with_file_name(name)
}

fn extract(archive: &[u8], staging: &Path) -> Result<()> {
    zip::ZipArchive::new(Cursor::new(archive))
        .context("Opening the ArtifactTool archive")?
        .extract(staging)
        .context("Extracting ArtifactTool")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            staging.join(executable_name()),
            fs::Permissions::from_mode(0o755),
        )?;
    }
    Ok(())
}

/// Downloads and unpacks the tool into a staging directory that is renamed
/// into place, so an interrupted download never leaves a half-extracted cache.
/// When another process finishes the same installation first, its copy is used.
async fn install(release: &ReleaseInfo, install_dir: &Path) -> Result<()> {
    println!("📦 Downloading ArtifactTool {}...", release.version);
    let archive = reqwest::get(&release.uri)
        .await
        .and_then(|response| response.error_for_status())
        .context("Downloading ArtifactTool")?
        .bytes()
        .await
        .context("Downloading ArtifactTool")?;

    // A leftover with this process id can only be from an earlier, dead process.
    let staging = staging_dir(install_dir, std::process::id());
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }

    let installed =
        extract(&archive, &staging).and_then(|()| match fs::rename(&staging, install_dir) {
            Ok(()) => Ok(()),
            Err(_) if install_dir.join(executable_name()).exists() => Ok(()),
            Err(e) => Err(e).context("Installing ArtifactTool"),
        });
    if staging.exists() {
        let _ = fs::remove_dir_all(&staging);
    }
    installed
}

/// Returns the path to the ArtifactTool executable, downloading the current
/// release when it is not cached yet.
async fn ensure_artifact_tool(creds: &Credentials) -> Result<PathBuf> {
    let release = fetch_release_info(creds).await?;
    let tools_dir = get_config_dir()?.join("tools").join("ArtifactTool");
    fs::create_dir_all(&tools_dir)?;

    let install_dir = tools_dir.join(&release.version);
    if !install_dir.join(executable_name()).exists() {
        install(&release, &install_dir).await?;
    }

    let executable = install_dir.join(executable_name());
    if !executable.exists() {
        return Err(anyhow!(
            "The ArtifactTool archive did not contain {}",
            executable_name()
        ));
    }
    Ok(executable)
}

/// Publishes the contents of a directory as a Universal Package version.
pub async fn publish_universal_package(
    creds: &Credentials,
    request: &PublishRequest<'_>,
) -> Result<()> {
    if !is_default_cloud_host(&creds.base_url) {
        return Err(anyhow!(
            "Universal Packages are only available in Azure DevOps Services (https://dev.azure.com)"
        ));
    }
    if !request.path.is_dir() {
        return Err(anyhow!("'{}' is not a directory", request.path.display()));
    }

    let executable = ensure_artifact_tool(creds).await?;
    let status = Command::new(&executable)
        .args(publish_args(&organization_url(creds), request))
        .env(PAT_ENV_VAR, &creds.pat)
        .status()
        .with_context(|| format!("Running {}", executable.display()))?;

    if !status.success() {
        return Err(anyhow!("ArtifactTool failed ({status})"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creds() -> Credentials {
        Credentials {
            organization: "contoso".to_string(),
            pat: "secret-pat".to_string(),
            base_url: "https://dev.azure.com".to_string(),
        }
    }

    #[test]
    fn platform_maps_rust_names_to_client_tools_names() {
        assert_eq!(platform("windows", "x86_64").unwrap(), ("windows", "amd64"));
        assert_eq!(platform("macos", "aarch64").unwrap(), ("darwin", "arm64"));
        assert_eq!(platform("linux", "x86_64").unwrap(), ("linux", "amd64"));
        assert!(platform("freebsd", "x86_64").is_err());
        assert!(platform("linux", "riscv64").is_err());
    }

    #[test]
    fn staging_dir_keeps_the_whole_version_and_is_unique_per_process() {
        let install_dir = Path::new("tools").join("0.2.576");
        assert_eq!(
            staging_dir(&install_dir, 42),
            Path::new("tools").join("0.2.576.partial-42")
        );
        assert_ne!(
            staging_dir(&Path::new("tools").join("0.2.577"), 42),
            staging_dir(&install_dir, 42)
        );
        assert_ne!(staging_dir(&install_dir, 7), staging_dir(&install_dir, 42));
    }

    #[test]
    fn release_info_url_uses_blob_store_host() {
        assert_eq!(
            release_info_url(&creds()),
            "https://vsblob.dev.azure.com/contoso/_apis/clienttools/ArtifactTool/release"
        );
    }

    #[test]
    fn publish_args_name_the_pat_variable_but_not_the_pat() {
        let request = PublishRequest {
            project: Some("web"),
            feed: "tools",
            name: "cli",
            version: "1.2.3",
            path: Path::new("dist"),
            description: Some("Command line tools"),
        };
        let args: Vec<String> = publish_args(&organization_url(&creds()), &request)
            .into_iter()
            .map(|arg| arg.into_string().unwrap())
            .collect();

        assert_eq!(
            args,
            [
                "universal",
                "publish",
                "--service",
                "https://dev.azure.com/contoso/",
                "--patvar",
                PAT_ENV_VAR,
                "--feed",
                "tools",
                "--package-name",
                "cli",
                "--package-version",
                "1.2.3",
                "--path",
                "dist",
                "--description",
                "Command line tools",
                "--project",
                "web",
            ]
        );
        assert!(!args.iter().any(|arg| arg.contains("secret-pat")));
    }

    #[test]
    fn publish_args_omit_optional_flags() {
        let request = PublishRequest {
            project: None,
            feed: "tools",
            name: "cli",
            version: "1.2.3",
            path: Path::new("dist"),
            description: None,
        };
        let args = publish_args(&organization_url(&creds()), &request);
        assert!(!args.contains(&OsString::from("--project")));
        assert!(!args.contains(&OsString::from("--description")));
    }
}
