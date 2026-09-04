use crate::auth::factory::{ClientFactory, CredentialClientFactory};
use crate::auth::get_credentials;
use crate::project::get_project_or_default;
use anyhow::{anyhow, Context, Result};
use azure_devops_rust_api::build::models::BuildArtifact;
use azure_devops_rust_api::pipelines::{self, models};
use clap::Subcommand;
use colored::Colorize;
use serde_json::{json, Value};

#[derive(Subcommand, Clone)]
pub enum PipelinesSubCommands {
    /// List all pipelines in a project
    List {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
    /// Show builds of a pipeline
    Runs {
        /// ID of the pipeline to show runs for
        #[clap(short, long)]
        id: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
    /// Show details of a pipeline build
    Show {
        /// ID of the pipeline to show
        #[clap(short, long)]
        id: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Build ID to show details for
        #[clap(short = 'b', long)]
        build_id: String,
    },
    /// List the logs of a pipeline run, or print one of them
    Logs {
        /// ID of the pipeline
        #[clap(short, long)]
        id: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Run (build) ID to read logs from
        #[clap(short = 'b', long)]
        build_id: String,
        /// Print the contents of this log instead of listing the logs
        #[clap(long)]
        log_id: Option<String>,
    },
    /// List the artifacts published by a pipeline run
    Artifacts {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Run (build) ID to list artifacts for
        #[clap(short = 'b', long)]
        build_id: String,
    },
    /// Run a pipeline
    Run {
        /// ID of the pipeline to start
        #[clap(short, long)]
        id: String,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Branch to run the pipeline from (defaults to the pipeline default branch)
        #[clap(short, long)]
        branch: Option<String>,
        /// Pipeline variable to set, in NAME=VALUE form (repeatable)
        #[clap(long = "variable", value_name = "NAME=VALUE")]
        variables: Vec<String>,
    },
}

/// Pipeline, run and log ids all reach us as strings from the command line.
fn parse_id(value: &str, label: &str) -> Result<i32> {
    value
        .parse::<i32>()
        .map_err(|_| anyhow!("Invalid {label} ID '{value}', must be a number"))
}

fn create_pipelines_client() -> Result<pipelines::Client> {
    let creds = get_credentials()?;
    let factory = CredentialClientFactory::new(&creds)?;
    Ok(factory.build_pipelines())
}

async fn list_pipelines(project: &str) -> Result<Vec<models::Pipeline>> {
    match get_credentials() {
        Ok(creds) => {
            let client = create_pipelines_client()?;
            Ok(client
                .pipelines_client()
                .list(creds.organization, project)
                .await?
                .value)
        }
        Err(e) => {
            eprintln!("Unable to retrieve pipelines");
            Err(e)
        }
    }
}

async fn get_pipeline_runs(project: &str, pipeline_id: &str) -> Result<Vec<models::Run>> {
    match get_credentials() {
        Ok(creds) => {
            let client = create_pipelines_client()?;
            let pipeline_id_int = parse_id(pipeline_id, "pipeline")?;

            Ok(client
                .runs_client()
                .list(creds.organization, project, pipeline_id_int)
                .await?
                .value)
        }
        Err(e) => {
            eprintln!("Unable to retrieve pipeline runs");
            Err(e)
        }
    }
}

async fn get_build(project: &str, pipeline_id: &str, build_id: &str) -> Result<models::Run> {
    match get_credentials() {
        Ok(creds) => {
            let client = create_pipelines_client()?;
            let pipeline_id_int = parse_id(pipeline_id, "pipeline")?;
            let build_id_int = parse_id(build_id, "build")?;

            let run = client
                .runs_client()
                .get(creds.organization, project, pipeline_id_int, build_id_int)
                .await?;

            Ok(run)
        }
        Err(e) => {
            eprintln!("Unable to retrieve build details");
            Err(e)
        }
    }
}

/// Azure DevOps expects a full ref, but users think in branch names.
fn full_ref_name(branch: &str) -> String {
    if branch.starts_with("refs/") {
        branch.to_string()
    } else {
        format!("refs/heads/{branch}")
    }
}

fn parse_variable(variable: &str) -> Result<(&str, &str)> {
    variable
        .split_once('=')
        .filter(|(name, _)| !name.is_empty())
        .ok_or_else(|| anyhow!("Invalid variable '{variable}', expected NAME=VALUE"))
}

/// Builds the run request body from the branch and variable arguments.
fn build_run_parameters(
    branch: Option<&str>,
    variables: &[String],
) -> Result<models::RunPipelineParameters> {
    let mut parameters = models::RunPipelineParameters::new();

    if let Some(branch) = branch {
        let mut resources = models::RunResourcesParameters::new();
        resources.repositories = Some(json!({ "self": { "refName": full_ref_name(branch) } }));
        parameters.resources = Some(resources);
    }

    if !variables.is_empty() {
        let mut values = serde_json::Map::new();
        for variable in variables {
            let (name, value) = parse_variable(variable)?;
            values.insert(name.to_string(), json!({ "value": value }));
        }
        parameters.variables = Some(Value::Object(values));
    }

    Ok(parameters)
}

async fn run_pipeline(
    project: &str,
    pipeline_id: &str,
    branch: Option<&str>,
    variables: &[String],
) -> Result<models::Run> {
    let creds = get_credentials()?;
    let client = create_pipelines_client()?;
    let pipeline_id_int = parse_id(pipeline_id, "pipeline")?;
    let parameters = build_run_parameters(branch, variables)?;

    Ok(client
        .runs_client()
        .run_pipeline(creds.organization, parameters, project, pipeline_id_int)
        .await?)
}

async fn list_run_logs(
    project: &str,
    pipeline_id: &str,
    build_id: &str,
) -> Result<Vec<models::Log>> {
    let creds = get_credentials()?;
    let client = create_pipelines_client()?;

    Ok(client
        .logs_client()
        .list(
            creds.organization,
            project,
            parse_id(pipeline_id, "pipeline")?,
            parse_id(build_id, "build")?,
        )
        .await?
        .logs)
}

/// Log text lives behind a short-lived signed URL rather than in the API
/// response, so it is fetched separately. That URL carries its own token, so no
/// credentials are attached to the download.
async fn get_run_log_content(
    project: &str,
    pipeline_id: &str,
    build_id: &str,
    log_id: &str,
) -> Result<String> {
    let creds = get_credentials()?;
    let client = create_pipelines_client()?;

    let log = client
        .logs_client()
        .get(
            creds.organization,
            project,
            parse_id(pipeline_id, "pipeline")?,
            parse_id(build_id, "build")?,
            parse_id(log_id, "log")?,
        )
        .expand("signedContent")
        .await?;

    let url = log
        .signed_content
        .and_then(|content| content.url)
        .ok_or_else(|| anyhow!("Azure DevOps returned no download URL for log {log_id}"))?;

    let response = reqwest::get(url)
        .await
        .context("Downloading the pipeline log")?
        .error_for_status()
        .context("Downloading the pipeline log")?;

    response
        .text()
        .await
        .context("Reading the pipeline log content")
}

fn display_run_logs(logs: &[models::Log]) {
    if logs.is_empty() {
        println!("No logs found.");
        return;
    }

    println!(
        "{:<10} {:<10} {}",
        "Log".bold(),
        "Lines".bold(),
        "Created".bold()
    );
    println!("{}", "-".repeat(40));

    for log in logs {
        println!(
            "{:<10} {:<10} {}",
            log.id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "-".to_string()),
            log.line_count
                .map(|lines| lines.to_string())
                .unwrap_or_else(|| "-".to_string()),
            log.created_on
                .map(|created| created.date().to_string())
                .unwrap_or_else(|| "-".to_string())
        );
    }
}

/// Artifacts are published against the build that a pipeline run creates, so
/// they are read through the build API rather than the pipelines API.
async fn list_build_artifacts(project: &str, build_id: &str) -> Result<Vec<BuildArtifact>> {
    let creds = get_credentials()?;
    let factory = CredentialClientFactory::new(&creds)?;

    Ok(factory
        .build_build()
        .artifacts_client()
        .list(creds.organization, project, parse_id(build_id, "build")?)
        .await?
        .value)
}

fn display_build_artifacts(artifacts: &[BuildArtifact]) {
    if artifacts.is_empty() {
        println!("No artifacts found.");
        return;
    }

    for artifact in artifacts {
        println!("📦 {}", artifact.name.as_deref().unwrap_or("-"));

        if let Some(resource) = &artifact.resource {
            if let Some(type_) = &resource.type_ {
                println!("   Type: {type_}");
            }
            if let Some(download_url) = &resource.download_url {
                println!("   Download: {download_url}");
            }
        }
    }

    println!("\n{} artifact(s)", artifacts.len());
}

fn display_pipelines(pipelines: &[models::Pipeline]) {
    if pipelines.is_empty() {
        println!("No pipelines found.");
        return;
    }

    println!(
        "{:<10} {:<40} {}",
        "ID".bold(),
        "Name".bold(),
        "Folder".bold()
    );
    println!("{}", "-".repeat(70));

    for pipeline in pipelines {
        println!(
            "{:<10} {:<40} {}",
            pipeline.id, pipeline.name, pipeline.folder
        );
    }
}

fn display_pipeline_runs(runs: &[models::Run]) {
    if runs.is_empty() {
        println!("No runs found.");
        return;
    }

    println!(
        "{:<10} {:<24} {:<14} {:<12} {}",
        "Run".bold(),
        "Name".bold(),
        "State".bold(),
        "Result".bold(),
        "Created".bold()
    );
    println!("{}", "-".repeat(75));

    for run in runs {
        println!(
            "{:<10} {:<24} {:<14} {:<12} {}",
            run.run_reference.id,
            run.run_reference.name,
            format!("{:?}", run.state),
            run.result
                .as_ref()
                .map(|result| format!("{result:?}"))
                .unwrap_or_else(|| "-".to_string()),
            run.created_date.date()
        );
    }
}

fn display_build_details(run: &models::Run) {
    println!("📋 Pipeline Run Details");
    println!("=======================");
    println!(
        "🆔 Run: #{} ({})",
        run.run_reference.id, run.run_reference.name
    );
    println!("📛 Pipeline: {}", run.pipeline.pipeline_base.name);
    println!("🚦 State: {:?}", run.state);

    if let Some(ref result) = run.result {
        println!("🎯 Result: {result:?}");
    }

    println!("🕐 Created: {}", run.created_date);

    if let Some(finished_date) = run.finished_date {
        println!("🏁 Finished: {finished_date}");
    }

    if let Some(web) = &run.links.web {
        println!("🌐 URL: {}", web.href);
    }
}

pub async fn handle_command(subcommand: &PipelinesSubCommands) -> Result<()> {
    match subcommand {
        PipelinesSubCommands::List { project } => {
            let project_name = get_project_or_default(project.as_deref())?;
            let pipelines = list_pipelines(&project_name).await?;
            display_pipelines(&pipelines);
        }
        PipelinesSubCommands::Runs { id, project } => {
            let project_name = get_project_or_default(project.as_deref())?;
            let runs = get_pipeline_runs(&project_name, id).await?;
            display_pipeline_runs(&runs);
        }
        PipelinesSubCommands::Logs {
            id,
            project,
            build_id,
            log_id,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            match log_id {
                Some(log_id) => {
                    match get_run_log_content(&project_name, id, build_id, log_id).await {
                        Ok(content) => print!("{content}"),
                        Err(e) => {
                            eprintln!("❌ Failed to read log {log_id} of build {build_id}");
                            eprintln!("   {e}");
                            return Err(e);
                        }
                    }
                }
                None => match list_run_logs(&project_name, id, build_id).await {
                    Ok(logs) => display_run_logs(&logs),
                    Err(e) => {
                        eprintln!("❌ Failed to list logs of build {build_id}");
                        eprintln!("   {e}");
                        return Err(e);
                    }
                },
            }
        }
        PipelinesSubCommands::Artifacts { project, build_id } => {
            let project_name = get_project_or_default(project.as_deref())?;
            match list_build_artifacts(&project_name, build_id).await {
                Ok(artifacts) => display_build_artifacts(&artifacts),
                Err(e) => {
                    eprintln!("❌ Failed to list artifacts of build {build_id}");
                    eprintln!("   {e}");
                    return Err(e);
                }
            }
        }
        PipelinesSubCommands::Run {
            id,
            project,
            branch,
            variables,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            println!("Starting pipeline with ID: {id} in project: {project_name}");
            match run_pipeline(&project_name, id, branch.as_deref(), variables).await {
                Ok(run) => {
                    println!("{}", "✅ Pipeline started successfully".green());
                    display_build_details(&run);
                }
                Err(e) => {
                    eprintln!("{}", format!("❌ Failed to start pipeline: {e}").red());
                    return Err(e);
                }
            }
        }
        PipelinesSubCommands::Show {
            id,
            project,
            build_id,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            println!(
                "Showing details for build {build_id} of pipeline {id} in project {project_name}"
            );
            match get_build(&project_name, id, build_id).await {
                Ok(build) => {
                    display_build_details(&build);
                }
                Err(e) => {
                    eprintln!("❌ Failed to retrieve build details: {e}");
                    return Err(e);
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_ref_name_expands_plain_branch_names() {
        assert_eq!(full_ref_name("main"), "refs/heads/main");
        assert_eq!(full_ref_name("feature/x"), "refs/heads/feature/x");
        assert_eq!(full_ref_name("refs/heads/main"), "refs/heads/main");
        assert_eq!(full_ref_name("refs/tags/v1"), "refs/tags/v1");
    }

    #[test]
    fn parse_variable_splits_on_the_first_equals_sign() {
        assert_eq!(parse_variable("name=value").unwrap(), ("name", "value"));
        assert_eq!(parse_variable("name=a=b").unwrap(), ("name", "a=b"));
        assert_eq!(parse_variable("name=").unwrap(), ("name", ""));
    }

    #[test]
    fn parse_variable_rejects_input_without_a_name() {
        assert!(parse_variable("novalue").is_err());
        assert!(parse_variable("=value").is_err());
    }

    #[test]
    fn build_run_parameters_sets_the_branch_and_variables() {
        let parameters = build_run_parameters(Some("develop"), &["env=prod".to_string()]).unwrap();

        let repositories = parameters.resources.unwrap().repositories.unwrap();
        assert_eq!(repositories["self"]["refName"], "refs/heads/develop");
        assert_eq!(parameters.variables.unwrap()["env"]["value"], "prod");
    }

    #[test]
    fn build_run_parameters_leaves_unset_fields_empty() {
        let parameters = build_run_parameters(None, &[]).unwrap();

        assert!(parameters.resources.is_none());
        assert!(parameters.variables.is_none());
    }

    #[test]
    fn parse_id_rejects_non_numeric_input() {
        assert_eq!(parse_id("12", "pipeline").unwrap(), 12);
        assert!(parse_id("abc", "pipeline").is_err());
    }
}
