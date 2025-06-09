use crate::config::get_config_dir;
use anyhow::{anyhow, Result};
use colored::Colorize;
use dialoguer::{Input, Password};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Credentials {
    pub organization: String,
    pub pat: String,
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

fn save_pat(pat: &str) -> Result<()> {
    let credentials = Credentials {
        organization: get_organization()?,
        pat: pat.to_string(),
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

fn get_pat() -> Result<String> {
    let creds_path = get_credentials_file_path()?;

    if !creds_path.exists() {
        return Err(anyhow!(
            "Not logged in. Please login first with 'ado login'"
        ));
    }

    let credentials_json = fs::read_to_string(creds_path)?;
    let credentials: Credentials = serde_json::from_str(&credentials_json)?;

    Ok(credentials.pat)
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

pub async fn login() -> Result<()> {
    println!("{}", "Login to Azure DevOps".bold());

    show_pat_instructions();

    let organization: String = Input::new()
        .with_prompt("Azure DevOps organization name")
        .interact_text()?;

    let pat: String = Password::new()
        .with_prompt("Personal Access Token (PAT)")
        .with_confirmation("Confirm PAT", "PATs don't match")
        .interact()?;
    println!("Validating credentials...");
    // In a real implementation, you would validate the PAT with Azure DevOps API
    // For now, we'll just save the credentials

    save_organization(&organization)?;
    save_pat(&pat)?;

    println!("{}", "Login successful!".green());
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

pub fn get_credentials() -> Result<Credentials> {
    let organization = get_organization()?;
    let pat = get_pat()?;

    Ok(Credentials { organization, pat })
}
