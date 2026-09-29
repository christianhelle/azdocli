pub mod factory;
pub mod url;

use crate::auth::url::{default_base_url, normalize_base_url, parse_organization_or_url};
use crate::config::get_config_dir;
use anyhow::{anyhow, Result};
use colored::Colorize;
use dialoguer::Input;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Credentials {
    pub organization: String,
    pub pat: String,
    #[serde(default = "default_base_url")]
    pub base_url: String,
}

fn profiles_dir() -> Result<PathBuf> {
    let dir = get_config_dir()?.join("profiles");
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

fn profile_file(name: &str) -> Result<PathBuf> {
    Ok(profiles_dir()?.join(format!("{}.json", sanitize_profile_name(name))))
}

fn sanitize_profile_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Save credentials for a named profile.
fn save_profile(name: &str, creds: &Credentials) -> Result<()> {
    let path = profile_file(name)?;
    let json = serde_json::to_string_pretty(creds)?;
    let mut file = fs::File::create(&path)?;
    file.write_all(json.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&path)?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&path, perms)?;
    }
    Ok(())
}

/// Load credentials for a named profile.
fn load_profile(name: &str) -> Result<Credentials> {
    let path = profile_file(name)?;
    if !path.exists() {
        return Err(anyhow!(
            "Profile '{}' not found. Run 'azdocli login --profile {}' first.",
            name,
            name
        ));
    }
    let s = fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&s)?)
}

/// List known profile names (excluding the legacy default).
#[allow(dead_code)]
pub fn list_profiles() -> Result<Vec<String>> {
    let dir = profiles_dir()?;
    let mut out = Vec::new();
    if !dir.exists() {
        return Ok(out);
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                out.push(stem.to_string());
            }
        }
    }
    out.sort();
    Ok(out)
}

#[allow(dead_code)]
pub fn delete_profile(name: &str) -> Result<()> {
    let path = profile_file(name)?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn save_organization(organization: &str) -> Result<()> {
    let config_dir = get_config_dir()?;
    let config_file = config_dir.join("config.json");

    let config = serde_json::json!({
        "organization": organization
    });

    fs::write(config_file, serde_json::to_string_pretty(&config)?)?;

    Ok(())
}

fn get_organization() -> Result<String> {
    let config_dir = get_config_dir()?;
    let config_file = config_dir.join("config.json");

    if !config_file.exists() {
        return Err(anyhow!(
            "Not logged in. Please login first with 'azdocli login'"
        ));
    }

    let config_content = fs::read_to_string(config_file)?;
    let config: serde_json::Value = serde_json::from_str(&config_content)?;

    let organization = config["organization"]
        .as_str()
        .ok_or_else(|| anyhow!("Invalid config file format"))?
        .to_string();

    Ok(organization)
}

fn get_credentials_file_path() -> Result<PathBuf> {
    let config_dir = get_config_dir()?;
    Ok(config_dir.join("credentials.json"))
}

fn save_pat(pat: &str, base_url: &str) -> Result<()> {
    let credentials = Credentials {
        organization: get_organization()?,
        pat: pat.to_string(),
        base_url: normalize_base_url(base_url),
    };

    let creds_path = get_credentials_file_path()?;
    let credentials_json = serde_json::to_string_pretty(&credentials)?;

    let mut file = fs::File::create(&creds_path)?;
    file.write_all(credentials_json.as_bytes())?;

    // Set restrictive permissions on the file
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&creds_path)?.permissions();
        perms.set_mode(0o600); // Read/write for owner only
        fs::set_permissions(&creds_path, perms)?;
    }

    Ok(())
}

/// Rejects a blank Personal Access Token so the user is re-prompted instead of
/// storing an empty credential.
fn validate_pat(pat: &str) -> Result<(), &'static str> {
    if pat.trim().is_empty() {
        return Err("PAT cannot be empty");
    }
    Ok(())
}

fn show_pat_instructions() {
    println!();
    println!("{}", "Create a Personal Access Token".bold());
    println!(
        "Before using the CLI, you need to create a Personal Access Token (PAT) in Azure DevOps:"
    );
    println!();
    println!("1. Navigate to Azure DevOps → User Settings → Personal Access Tokens");
    println!("2. Click \"New Token\"");
    println!("3. Set a descriptive name (e.g., \"azdocli\")");
    println!("4. Configure the required scopes: Code (read & write), Build (read & execute), Work Items (read & write)");
    println!("5. Click \"Create\" and copy the token securely");
    println!();
    println!(
        "{} Store your PAT securely and never commit it to version control.",
        "⚠️ Important:".yellow().bold()
    );
    println!();
}

/// Interactive login. If `profile` is provided, credentials are saved under
/// that named profile; otherwise the legacy default location is used.
///
/// The organization prompt accepts either an organization name or a full base
/// URL for enterprise Azure DevOps installations (e.g.
/// `https://devops.mycompany.com`). When a URL with a path is supplied, the
/// last path segment is treated as the organization/collection and the
/// preceding path is stored as the base URL.
pub async fn login(profile: Option<&str>) -> Result<()> {
    println!("{}", "Login to Azure DevOps".bold());

    show_pat_instructions();

    let input: String = Input::new()
        .with_prompt("Azure DevOps organization name or base URL")
        .interact_text()?;

    let (base_url, mut organization) = parse_organization_or_url(&input)?;
    let base_url = base_url.unwrap_or_else(default_base_url);

    if organization.is_empty() {
        organization = Input::new()
            .with_prompt("Azure DevOps organization name")
            .interact_text()?;
    }

    let pat: String = Input::new()
        .with_prompt("Personal Access Token (PAT)")
        .validate_with(|pat: &String| validate_pat(pat))
        .interact_text()?;
    let pat = pat.trim().to_string();
    println!("Validating credentials...");

    match profile {
        Some(name) => {
            save_profile(
                name,
                &Credentials {
                    organization,
                    pat,
                    base_url,
                },
            )?;
            println!("{} (profile: {})", "Login successful!".green(), name);
        }
        None => {
            save_organization(&organization)?;
            save_pat(&pat, &base_url)?;
            println!("{}", "Login successful!".green());
        }
    }
    Ok(())
}

pub fn logout() -> Result<()> {
    let config_dir = get_config_dir()?;
    let config_file = config_dir.join("config.json");

    if config_file.exists() {
        fs::remove_file(config_file)?;
    }
    let creds_path = get_credentials_file_path()?;
    if creds_path.exists() {
        fs::remove_file(creds_path)?;
    }

    println!("{}", "Logged out successfully".green());
    Ok(())
}

/// Load credentials for the default profile (legacy single-org store).
pub fn get_credentials() -> Result<Credentials> {
    let organization = get_organization()?;
    let stored = get_stored_credentials()?;

    Ok(Credentials {
        organization,
        pat: stored.pat,
        base_url: stored.base_url,
    })
}

fn get_stored_credentials() -> Result<Credentials> {
    let creds_path = get_credentials_file_path()?;

    if !creds_path.exists() {
        return Err(anyhow!(
            "Not logged in. Please login first with 'azdocli login'"
        ));
    }

    let credentials_json = fs::read_to_string(creds_path)?;
    let credentials: Credentials = serde_json::from_str(&credentials_json)?;

    Ok(credentials)
}

/// Load credentials for a specific named profile, or the default if `None`.
pub fn get_credentials_for(profile: Option<&str>) -> Result<Credentials> {
    match profile {
        Some(name) => load_profile(name),
        None => get_credentials(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_pat_accepts_a_token() {
        assert!(validate_pat("abc123").is_ok());
    }

    #[test]
    fn validate_pat_rejects_an_empty_token() {
        assert!(validate_pat("").is_err());
    }

    #[test]
    fn validate_pat_rejects_a_whitespace_only_token() {
        assert!(validate_pat("   ").is_err());
    }
}
