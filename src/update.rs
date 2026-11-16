//! Whether a newer azdocli has been released, and putting it in place of the
//! running one. An azdocli a package manager installed is left for that
//! package manager to update.

use std::{
    io::{Cursor, Read},
    path::{Path, PathBuf},
    time::Duration as StdDuration,
};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, Utc};
use reqwest::Client;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The target triple this azdocli was built for.
pub const TARGET: &str = env!("AZDOCLI_TARGET");

/// How long a check is good for before azdocli asks GitHub again on its own.
const CHECK_EVERY_HOURS: i64 = 24;

/// A published release of azdocli.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    /// The page to read about it and download it from.
    pub url: String,
    /// The archives built for it, one per platform.
    pub assets: Vec<Asset>,
}

impl Release {
    /// The archive built for `target`, if there is one.
    pub fn asset(&self, target: &str) -> Option<&Asset> {
        let name = asset_name(target)?;
        self.assets.iter().find(|asset| asset.name == name)
    }
}

/// A file published with a release.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Asset {
    pub name: String,
    #[serde(rename = "browser_download_url")]
    pub url: String,
    /// GitHub's checksum of it, such as `sha256:…`.
    #[serde(default)]
    pub digest: Option<String>,
}

/// The name the release workflow gives the archive built for `target`,
/// such as `windows-x64.zip`. `None` for a target it does not build.
pub fn asset_name(target: &str) -> Option<String> {
    let os = if target.contains("windows") {
        "windows"
    } else if target.contains("apple-darwin") {
        "macos"
    } else if target.contains("linux") {
        "linux"
    } else {
        return None;
    };
    let arch = match target.split('-').next()? {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        _ => return None,
    };
    Some(format!("{os}-{arch}.zip"))
}

/// The name of azdocli's binary built for `target`.
pub fn binary_name(target: &str) -> &'static str {
    if target.contains("windows") {
        "azdocli.exe"
    } else {
        "azdocli"
    }
}

/// The last check that worked, kept so that azdocli does not ask GitHub on
/// every run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    pub checked_at: DateTime<Utc>,
    pub latest: Version,
}

impl Saved {
    /// What was saved at `path`. A missing or unreadable file is the same as
    /// never having checked, which the next check puts right.
    pub fn load(path: &Path) -> Option<Self> {
        let json = std::fs::read(path).ok()?;
        serde_json::from_slice(&json).ok()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_vec(self)?)?;
        Ok(())
    }

    /// Whether it is time to ask again. A check from the future means the
    /// clock was moved, so its age says nothing.
    fn is_due(&self, now: DateTime<Utc>) -> bool {
        let age = now.signed_duration_since(self.checked_at);
        age < Duration::zero() || age >= Duration::hours(CHECK_EVERY_HOURS)
    }
}

/// The version of azdocli that is running.
pub fn current() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("Cargo only accepts semver package versions")
}

/// Whether `latest` is a newer version than `current`.
pub fn newer(current: &Version, latest: &Version) -> bool {
    latest > current
}

/// A package manager that installed azdocli and so keeps it up to date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageManager {
    Cargo,
    Snap,
    Homebrew,
    Chocolatey,
}

impl PackageManager {
    /// The command that updates an azdocli this package manager installed.
    pub fn update_command(self) -> &'static str {
        match self {
            PackageManager::Cargo => "cargo install azdocli",
            PackageManager::Snap => "snap refresh azdocli",
            PackageManager::Homebrew => "brew upgrade azdocli",
            PackageManager::Chocolatey => "choco upgrade azdocli",
        }
    }
}

/// Where `cargo install` puts binaries: `$CARGO_HOME/bin`, or `~/.cargo/bin`.
pub fn cargo_bin() -> Option<PathBuf> {
    let home = match std::env::var_os("CARGO_HOME") {
        Some(home) if !home.is_empty() => PathBuf::from(home),
        _ => dirs::home_dir()?.join(".cargo"),
    };
    Some(home.join("bin"))
}

/// The package manager that installed `exe`, if any. `cargo_bin` is where
/// Cargo puts the binaries it installs.
pub fn package_manager(exe: &Path, cargo_bin: Option<&Path>) -> Option<PackageManager> {
    let canonical = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.into());
    if let (Some(dir), Some(bin)) = (exe.parent(), cargo_bin) {
        if canonical(dir) == canonical(bin) {
            return Some(PackageManager::Cargo);
        }
    }
    let path = canonical(exe)
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase();
    if path.starts_with("/snap/") {
        Some(PackageManager::Snap)
    } else if path.contains("/cellar/") || path.contains("/homebrew/") {
        Some(PackageManager::Homebrew)
    } else if path.contains("/chocolatey/") {
        Some(PackageManager::Chocolatey)
    } else {
        None
    }
}

/// Fails unless `bytes` match GitHub's `digest` of them. An asset without a
/// digest cannot be checked, so it is not trusted either.
pub fn verify(bytes: &[u8], digest: Option<&str>) -> Result<()> {
    let expected = digest
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .context("GitHub gave no SHA-256 checksum for the download")?;
    let actual: String = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if !actual.eq_ignore_ascii_case(expected) {
        bail!("the download does not match its checksum; try again");
    }
    Ok(())
}

/// Takes the file called `binary` out of the zip `archive`, wherever it is
/// in it.
pub fn extract(archive: &[u8], binary: &str) -> Result<Vec<u8>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(archive))?;
    for index in 0..zip.len() {
        let mut file = zip.by_index(index)?;
        let is_binary = Path::new(&*file.name()?)
            .file_name()
            .is_some_and(|name| name == binary);
        if file.is_file() && is_binary {
            let mut contents = Vec::new();
            file.read_to_end(&mut contents)?;
            return Ok(contents);
        }
    }
    bail!("the release archive has no {binary} in it")
}

/// Where azdocli's releases are published.
pub const GITHUB_API: &str = "https://api.github.com";

/// How long to wait for GitHub when asked to check.
const TIMEOUT: StdDuration = StdDuration::from_secs(10);

/// How long to wait for GitHub when checking unasked, which must never hold
/// up a command for long.
const QUIET_TIMEOUT: StdDuration = StdDuration::from_secs(2);

/// How long a download of a release may take.
const DOWNLOAD_TIMEOUT: StdDuration = StdDuration::from_secs(300);

/// The part of GitHub's answer azdocli needs.
#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

/// A client GitHub accepts, which needs a user agent.
pub fn http_client() -> Client {
    Client::builder()
        .user_agent(concat!("azdocli/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("a client with only a user agent set always builds")
}

/// Where the last check is kept.
pub fn check_path() -> Result<PathBuf> {
    let cache = dirs::cache_dir().context("no cache directory to keep the update check in")?;
    Ok(cache.join("azdocli").join("update-check.json"))
}

/// Asks GitHub for the latest release. GitHub leaves out drafts and
/// prereleases.
pub async fn latest(client: &Client, base_url: &str, timeout: StdDuration) -> Result<Release> {
    let url = format!("{base_url}/repos/christianhelle/azdocli/releases/latest");
    let release: GithubRelease = client
        .get(url)
        .timeout(timeout)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let tag = release.tag_name;
    let version = Version::parse(tag.strip_prefix('v').unwrap_or(&tag))
        .with_context(|| format!("the latest release has a strange tag: {tag}"))?;
    Ok(Release {
        version,
        url: release.html_url,
        assets: release.assets,
    })
}

/// Asks GitHub for the latest release and remembers the answer at `path`.
async fn check_with(
    client: &Client,
    base_url: &str,
    path: &Path,
    now: DateTime<Utc>,
    timeout: StdDuration,
) -> Result<Release> {
    let release = latest(client, base_url, timeout).await?;
    let saved = Saved {
        checked_at: now,
        latest: release.version.clone(),
    };
    // Failing to remember only means asking again next time.
    let _ = saved.save(path);
    Ok(release)
}

/// Asks GitHub for the latest release and remembers the answer at `path`.
pub async fn check(
    client: &Client,
    base_url: &str,
    path: &Path,
    now: DateTime<Utc>,
) -> Result<Release> {
    check_with(client, base_url, path, now, TIMEOUT).await
}

/// The newer version to tell the user about, if any, asking GitHub only when
/// the check saved at `path` has gone stale. Nothing here is worth failing a
/// command for, so a failed check goes unmentioned and the last answer stands.
pub async fn available(
    client: &Client,
    base_url: &str,
    path: &Path,
    current: &Version,
    now: DateTime<Utc>,
) -> Option<Version> {
    let latest = match Saved::load(path) {
        Some(saved) if !saved.is_due(now) => saved.latest,
        saved => match check_with(client, base_url, path, now, QUIET_TIMEOUT).await {
            Ok(release) => release.version,
            Err(_) => saved?.latest,
        },
    };
    newer(current, &latest).then_some(latest)
}

/// Downloads `asset` and checks it is what GitHub published.
pub async fn download(client: &Client, asset: &Asset) -> Result<Vec<u8>> {
    let bytes = client
        .get(&asset.url)
        .timeout(DOWNLOAD_TIMEOUT)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    verify(&bytes, asset.digest.as_deref())?;
    Ok(bytes.to_vec())
}

/// What updating did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The running version is already the latest.
    UpToDate(Version),
    /// This version replaced the running binary.
    Installed(Version),
}

/// Asks GitHub for the latest release, remembering the answer at `path`, and
/// when it is newer than `current`, downloads the archive built for `target`
/// and returns its version and binary.
pub async fn fetch_newer(
    client: &Client,
    base_url: &str,
    path: &Path,
    current: &Version,
    target: &str,
    now: DateTime<Utc>,
) -> Result<Option<(Version, Vec<u8>)>> {
    let release = check(client, base_url, path, now).await?;
    if !newer(current, &release.version) {
        return Ok(None);
    }
    let asset = release.asset(target).with_context(|| {
        format!(
            "azdocli {} has no build for {target}; see {}",
            release.version, release.url
        )
    })?;
    let archive = download(client, asset).await?;
    let binary = extract(&archive, binary_name(target))?;
    Ok(Some((release.version, binary)))
}

/// Puts `binary` in place of the running `exe`. It is written next to it
/// first, so the swap never crosses file systems.
pub fn replace_running(exe: &Path, binary: &[u8]) -> Result<()> {
    let staged = exe.with_file_name(".azdocli-update");
    std::fs::write(&staged, binary)
        .with_context(|| format!("could not write to {}", staged.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))?;
    }
    let replaced = self_replace::self_replace(&staged)
        .with_context(|| format!("could not replace {}", exe.display()));
    let _ = std::fs::remove_file(&staged);
    replaced
}

/// Replaces the running azdocli with the latest release, unless it already
/// is the latest or a package manager installed it. `path` keeps the answer,
/// as a check does.
pub async fn update_running(client: &Client, base_url: &str, path: &Path) -> Result<Outcome> {
    let exe = std::env::current_exe().context("could not find the running azdocli")?;
    if let Some(manager) = package_manager(&exe, cargo_bin().as_deref()) {
        bail!(
            "{manager:?} installed azdocli; update it with `{}`",
            manager.update_command()
        );
    }
    let current = current();
    match fetch_newer(client, base_url, path, &current, TARGET, Utc::now()).await? {
        None => Ok(Outcome::UpToDate(current)),
        Some((version, binary)) => {
            replace_running(&exe, &binary)?;
            Ok(Outcome::Installed(version))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(version: &str) -> Version {
        Version::parse(version).unwrap()
    }

    #[test]
    fn only_a_later_version_is_newer() {
        let current = version("0.6.4");

        assert!(newer(&current, &version("0.6.5")));
        assert!(newer(&current, &version("0.7.0")));
        assert!(!newer(&current, &version("0.6.4")));
        assert!(!newer(&current, &version("0.6.3")));
    }

    #[test]
    fn versions_compare_as_numbers_not_text() {
        assert!(newer(&version("0.9.0"), &version("0.10.0")));
    }

    #[test]
    fn the_running_version_is_the_packages() {
        assert_eq!(current().to_string(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn the_target_is_a_triple() {
        assert!(TARGET.split('-').count() >= 3, "{TARGET}");
    }

    #[test]
    fn archives_are_named_for_os_and_arch() {
        for (target, name) in [
            ("x86_64-pc-windows-msvc", "windows-x64.zip"),
            ("aarch64-pc-windows-msvc", "windows-arm64.zip"),
            ("x86_64-apple-darwin", "macos-x64.zip"),
            ("aarch64-apple-darwin", "macos-arm64.zip"),
            ("x86_64-unknown-linux-gnu", "linux-x64.zip"),
            ("aarch64-unknown-linux-gnu", "linux-arm64.zip"),
        ] {
            assert_eq!(asset_name(target).as_deref(), Some(name), "{target}");
        }
    }

    #[test]
    fn unbuilt_targets_have_no_archive() {
        assert_eq!(asset_name("riscv64gc-unknown-linux-gnu"), None);
        assert_eq!(asset_name("x86_64-unknown-freebsd"), None);
    }

    #[test]
    fn the_running_target_has_an_archive() {
        assert!(asset_name(TARGET).is_some(), "{TARGET}");
    }

    #[test]
    fn a_release_finds_the_archive_for_a_target() {
        let asset = |name: &str| Asset {
            name: name.into(),
            url: format!("https://example.com/{name}"),
            digest: None,
        };
        let release = Release {
            version: version("0.6.5"),
            url: String::new(),
            assets: vec![asset("windows-x64.zip"), asset("linux-x64.zip")],
        };

        assert_eq!(
            release
                .asset("x86_64-unknown-linux-gnu")
                .map(|a| &a.name[..]),
            Some("linux-x64.zip")
        );
        assert_eq!(release.asset("aarch64-apple-darwin"), None);
    }

    #[test]
    fn the_binary_is_named_for_its_target() {
        assert_eq!(binary_name("x86_64-pc-windows-msvc"), "azdocli.exe");
        assert_eq!(binary_name("aarch64-apple-darwin"), "azdocli");
    }

    #[test]
    fn a_binary_in_cargos_bin_is_cargos() {
        let cargo = tempfile::tempdir().unwrap();
        let bin = cargo.path().join("bin");
        std::fs::create_dir(&bin).unwrap();

        assert_eq!(
            package_manager(&bin.join("azdocli"), Some(&bin)),
            Some(PackageManager::Cargo)
        );
        assert_eq!(
            package_manager(&bin.join("azdocli.exe"), Some(&bin)),
            Some(PackageManager::Cargo)
        );
    }

    #[test]
    fn package_managers_are_recognised_by_path() {
        for (exe, manager) in [
            ("/snap/azdocli/12/bin/azdocli", PackageManager::Snap),
            (
                "/opt/homebrew/Cellar/azdocli/0.6.5/bin/azdocli",
                PackageManager::Homebrew,
            ),
            (
                "/usr/local/Cellar/azdocli/0.6.5/bin/azdocli",
                PackageManager::Homebrew,
            ),
            (
                r"C:\ProgramData\chocolatey\lib\azdocli\tools\azdocli.exe",
                PackageManager::Chocolatey,
            ),
        ] {
            assert_eq!(
                package_manager(Path::new(exe), None),
                Some(manager),
                "{exe}"
            );
        }
    }

    #[test]
    fn a_binary_elsewhere_has_no_package_manager() {
        let cargo = tempfile::tempdir().unwrap();
        let bin = cargo.path().join("bin");
        let local = cargo.path().join("local");

        assert_eq!(package_manager(&local.join("azdocli"), Some(&bin)), None);
        assert_eq!(
            package_manager(&bin.join("nested").join("azdocli"), Some(&bin)),
            None
        );
        assert_eq!(
            package_manager(Path::new("/usr/local/bin/azdocli"), None),
            None
        );
    }

    /// The SHA-256 of `abc`.
    const ABC: &str = "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn bytes_matching_their_digest_pass() {
        assert!(verify(b"abc", Some(ABC)).is_ok());
        let upper = format!("sha256:{}", ABC["sha256:".len()..].to_uppercase());
        assert!(verify(b"abc", Some(&upper)).is_ok());
    }

    #[test]
    fn bytes_not_matching_their_digest_fail() {
        assert!(verify(b"abd", Some(ABC)).is_err());
    }

    #[test]
    fn bytes_without_a_sha256_digest_fail() {
        assert!(verify(b"abc", None).is_err());
        assert!(verify(b"abc", Some("md5:900150983cd24fb0d6963f7d28e17f72")).is_err());
    }

    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, contents) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(contents).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn the_binary_comes_out_of_the_zip() {
        let archive = zip_of(&[
            ("README.md", b"readme"),
            ("azdocli.exe", b"binary"),
            ("LICENSE", b"license"),
        ]);

        assert_eq!(extract(&archive, "azdocli.exe").unwrap(), b"binary");
    }

    #[test]
    fn the_binary_is_found_in_a_folder() {
        let archive = zip_of(&[("linux-x64/azdocli", b"binary")]);

        assert_eq!(extract(&archive, "azdocli").unwrap(), b"binary");
    }

    #[test]
    fn an_archive_without_the_binary_is_an_error() {
        let archive = zip_of(&[("README.md", b"readme")]);

        assert!(extract(&archive, "azdocli").is_err());
    }

    #[test]
    fn a_corrupt_archive_is_an_error() {
        assert!(extract(b"nope", "azdocli.exe").is_err());
    }

    fn saved_at(checked_at: DateTime<Utc>) -> Saved {
        Saved {
            checked_at,
            latest: version("1.0.0"),
        }
    }

    #[test]
    fn a_check_lasts_a_day() {
        let checked_at = DateTime::from_timestamp(1_000_000, 0).unwrap();
        let saved = saved_at(checked_at);
        let later = |hours| checked_at + Duration::hours(hours);

        assert!(!saved.is_due(checked_at));
        assert!(!saved.is_due(later(23)));
        assert!(saved.is_due(later(24)));
        assert!(saved.is_due(later(25)));
    }

    #[test]
    fn a_check_from_the_future_is_due() {
        let now = DateTime::from_timestamp(1_000_000, 0).unwrap();

        assert!(saved_at(now + Duration::hours(1)).is_due(now));
    }

    #[test]
    fn a_saved_check_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("update-check.json");
        let saved = saved_at(DateTime::from_timestamp(1_000_000, 0).unwrap());

        saved.save(&path).unwrap();

        assert_eq!(Saved::load(&path), Some(saved));
    }

    /// Nothing listens here, so asking it fails at once.
    const UNREACHABLE: &str = "http://127.0.0.1:9";

    #[tokio::test]
    async fn a_fresh_check_is_used_without_asking_github() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("update-check.json");
        let now = DateTime::from_timestamp(1_000_000, 0).unwrap();
        saved_at(now).save(&path).unwrap();

        let client = http_client();
        let found = available(&client, UNREACHABLE, &path, &version("0.9.0"), now).await;
        let none = available(&client, UNREACHABLE, &path, &version("1.0.0"), now).await;

        assert_eq!(found, Some(version("1.0.0")));
        assert_eq!(none, None);
    }

    #[tokio::test]
    async fn a_failed_check_falls_back_to_the_last_answer() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("update-check.json");
        let checked_at = DateTime::from_timestamp(1_000_000, 0).unwrap();
        saved_at(checked_at).save(&path).unwrap();
        let later = checked_at + Duration::hours(48);

        let found = available(&http_client(), UNREACHABLE, &path, &version("0.9.0"), later).await;

        assert_eq!(found, Some(version("1.0.0")));
    }

    #[tokio::test]
    async fn a_failed_first_check_finds_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("update-check.json");
        let now = DateTime::from_timestamp(1_000_000, 0).unwrap();

        let found = available(&http_client(), UNREACHABLE, &path, &version("0.9.0"), now).await;

        assert_eq!(found, None);
    }

    #[test]
    fn a_missing_check_is_none() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(Saved::load(&dir.path().join("nope.json")), None);
    }
}
