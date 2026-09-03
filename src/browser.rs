//! Opening URLs in the user's default browser.
//!
//! Several commands support a `--web` flag; they all need the same per-OS
//! launcher, so it lives here rather than being repeated in each module.

use anyhow::{Context, Result};
use std::process::Command;

/// Opens `url` in the default browser for the current platform.
///
/// On Windows the URL is handed to `explorer` rather than `cmd /C start`,
/// because `cmd` re-parses its command line and would treat characters such as
/// `&`, `|` and `^` in the URL as shell metacharacters. Parts of these URLs
/// come from the configured base URL and from project and repository names, so
/// the URL must never reach a shell parser.
pub fn open_url(url: &str) -> Result<()> {
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("explorer");
        command.arg(url);
        command
    };

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(url);
        command
    };

    #[cfg(target_os = "linux")]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };

    command
        .spawn()
        .with_context(|| format!("Failed to open '{url}' in a browser"))?;

    Ok(())
}
