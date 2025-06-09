use anyhow::anyhow;
use std::fs;

pub fn save_default_project(project: &str) -> anyhow::Result<()> {
    let config_dir = crate::config::get_config_dir()?;
    let config_file = config_dir.join("config.json");
    let mut config = if config_file.exists() {
        let config_content = fs::read_to_string(&config_file)?;
        serde_json::from_str::<serde_json::Value>(&config_content)?
    } else {
        serde_json::json!({})
    };

    config["default_project"] = serde_json::Value::String(project.to_string());

    fs::write(config_file, serde_json::to_string_pretty(&config)?)?;

    Ok(())
}

pub fn get_default_project() -> anyhow::Result<String> {
    let config_dir = crate::config::get_config_dir()?;
    let config_file = config_dir.join("config.json");

    if !config_file.exists() {
        return Err(anyhow!("No default project configured. Please set one using 'ado project <project_name>' or use the --project argument"));
    }

    let config_content = fs::read_to_string(config_file)?;
    let config: serde_json::Value = serde_json::from_str(&config_content)?;

    match config["default_project"].as_str() {
        Some(project) => Ok(project.to_string()),
        None => Err(anyhow!("No default project configured. Please set one using 'ado project <project_name>' or use the --project argument"))
    }
}

pub fn get_project_or_default(project_arg: Option<&str>) -> anyhow::Result<String> {
    match project_arg {
        Some(project) => Ok(project.to_string()),
        None => get_default_project(),
    }
}
