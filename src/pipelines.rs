use crate::auth::get_credentials;
use crate::project::get_project_or_default;
use anyhow::{anyhow, Result};
use azure_devops_rust_api::pipelines::{self, models, ClientBuilder};
use clap::Subcommand;
use colored::Colorize;

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
    },
}

fn create_client() -> Result<pipelines::Client> {
    match get_credentials() {
        Ok(creds) => {
            let credential = azure_devops_rust_api::Credential::Pat(creds.pat);
            let client = ClientBuilder::new(credential).build();
            Ok(client)
        }
        Err(e) => Err(e),
    }
}

async fn list_pipelines(project: &str) -> Result<Vec<models::Pipeline>> {
    match get_credentials() {
        Ok(creds) => {
            let client = create_client()?;
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
            let client = create_client()?;
            let pipeline_id_int = pipeline_id
                .parse::<i32>()
                .map_err(|_| anyhow!("Invalid pipeline ID, must be a number"))?;

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
            let client = create_client()?;
            let pipeline_id_int = pipeline_id
                .parse::<i32>()
                .map_err(|_| anyhow!("Invalid pipeline ID, must be a number"))?;
            let build_id_int = build_id
                .parse::<i32>()
                .map_err(|_| anyhow!("Invalid build ID, must be a number"))?;

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

fn run_pipeline(_project: &str, _pipeline_id: &str) -> Result<models::Run> {
    // Not yet implemented
    Err(anyhow!("Running pipelines is not yet fully implemented"))
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
        PipelinesSubCommands::Run { id, project } => {
            let project_name = get_project_or_default(project.as_deref())?;
            println!("Starting pipeline with ID: {id} in project: {project_name}");
            match run_pipeline(&project_name, id) {
                Ok(run) => {
                    println!("Pipeline started successfully!");
                    display_build_details(&run);
                }
                Err(e) => {
                    eprintln!("❌ Failed to start pipeline: {e}");
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
