use anyhow::anyhow;
use std::fs;
use std::path::PathBuf;

pub fn get_config_dir() -> anyhow::Result<PathBuf> {
    let home_dir = get_home_dir().ok_or_else(|| anyhow!("Could not find home directory"))?;
    let config_dir = home_dir.join(".azdocli");

    if !config_dir.exists() {
        fs::create_dir_all(&config_dir)?;
    }

    Ok(config_dir)
}

pub fn get_home_dir() -> Option<PathBuf> {
    dirs::home_dir()
}
