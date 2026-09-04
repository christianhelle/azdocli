use crate::auth::factory::{ClientFactory, CredentialClientFactory};
use crate::auth::get_credentials;
use crate::project::get_project_or_default;
use anyhow::{anyhow, Result};
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

fn display_pipelines(pipelines: &[models::Pipeline]) {
    if pipelines.is_empty() {
        println!("No pipelines found.");
        return;
    }

    println!("{:<10} {:<40}", "ID".bold(), "Name".bold());
    println!("{}", "-".repeat(50));

    for pipeline in pipelines {
        println!("{:<10} {:<40}", pipeline.id, format!("{:?}", pipeline.name));
    }
}

fn display_pipeline_runs(runs: &[models::Run]) {
    if runs.is_empty() {
        println!("No runs found.");
        return;
    }

    println!("Pipeline Runs:\n");

    runs.iter().for_each(|run| {
        println!("Run #{}", run.run_reference.id);
        println!("State: {:?}", run.state);

        if let Some(ref result) = run.result {
            println!("Result: {result:?}");
        }

        println!();
    });
}

fn display_build_details(run: &models::Run) {
    println!("📋 Pipeline Run Details");
    println!("=====================");

    println!("State: {:?}", run.state);

    if let Some(ref result) = run.result {
        println!("Result: {result:?}");
    }

    println!("\nFull details:");
    println!("{run:#?}");
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
