use crate::auth;
use anyhow::Result;
use azure_devops_rust_api::git::{self, ClientBuilder};
use clap::Subcommand;

#[derive(Subcommand, Clone)]
pub enum PullRequestsSubCommands {
    /// Create new pull request
    Create {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,

        /// Name of the repository to create a pull request in
        #[clap(short, long)]
        repo: String,

        /// Title of the pull request
        #[clap(short, long)]
        title: Option<String>,

        /// Description of the pull request
        #[clap(short, long)]
        description: Option<String>,
    },
    /// List pull requests
    List {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,

        /// Name of the repository to list pull requests from
        #[clap(short, long)]
        repo: String,
    },
    /// Show details of a specific pull request
    Show {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,

        /// Name of the repository to show pull requests from
        #[clap(short, long)]
        repo: String,

        /// ID of the pull request to show
        #[clap(short, long)]
        id: String,
    },
}

/// Creates a git client for Azure DevOps API
fn create_client() -> Result<git::Client> {
    match auth::get_credentials() {
        Ok(creds) => {
            let credential = azure_devops_rust_api::Credential::Pat(creds.pat);
            let client = ClientBuilder::new(credential).build();
            Ok(client)
        }
        Err(e) => Err(e),
    }
}

pub async fn handle_command(subcommand: &PullRequestsSubCommands) -> anyhow::Result<()> {
    match subcommand {
        PullRequestsSubCommands::Create {
            project,
            repo,
            title,
            description,
        } => {
            let project_name = auth::get_project_or_default(project.as_deref())?;
            create_pull_request(
                &project_name,
                repo,
                title.as_deref(),
                description.as_deref(),
            )
            .await?;
        }
        PullRequestsSubCommands::List { project, repo } => {
            let project_name = auth::get_project_or_default(project.as_deref())?;
            list_pull_requests(&project_name, repo).await?;
        }
        PullRequestsSubCommands::Show { project, repo, id } => {
            let project_name = auth::get_project_or_default(project.as_deref())?;
            show_pull_request(&project_name, repo, id).await?;
        }
    }
    Ok(())
}

/// Creates a new pull request
async fn create_pull_request(
    project: &str,
    _repo: &str,
    title: Option<&str>,
    description: Option<&str>,
) -> Result<()> {
    match auth::get_credentials() {
        Ok(_creds) => {
            let _client = create_client()?;
            // let pr_client = client.pull_requests_client();

            // For now, just show that we're attempting to create
            println!("✅ Would create pull request:");
            println!("  Project: {}", project);
            println!("  Title: {}", title.unwrap_or("Default title"));
            println!(
                "  Description: {}",
                description.unwrap_or("Default description")
            );
            println!("  Note: Full implementation requires source/target branch specification");

            Ok(())
        }
        Err(e) => {
            eprintln!("Unable to create pull request");
            Err(e)
        }
    }
}

/// Lists all pull requests for a repository
async fn list_pull_requests(project: &str, repo: &str) -> Result<()> {
    match auth::get_credentials() {
        Ok(creds) => {
            let client = create_client()?;

            // Use the pull_requests_client
            let pr_client = client.pull_requests_client();

            // Get all pull requests in the project
            let pull_requests = pr_client
                .get_pull_requests_by_project(creds.organization, project)
                .await?;

            // Filter by repository name
            let filtered_prs: Vec<_> = pull_requests
                .value
                .into_iter()
                .filter(|pr| pr.repository.name == repo)
                .collect();

            if filtered_prs.is_empty() {
                println!(
                    "No pull requests found for repository '{}' in project '{}'",
                    repo, project
                );
            } else {
                println!(
                    "Pull requests for repository '{}' in project '{}':",
                    repo, project
                );
                for pr in filtered_prs {
                    println!(
                        "  #{} - {}",
                        pr.pull_request_id,
                        pr.title.unwrap_or_default()
                    );
                }
            }

            Ok(())
        }
        Err(e) => {
            eprintln!("Unable to retrieve pull requests");
            Err(e)
        }
    }
}

/// Shows details of a specific pull request
async fn show_pull_request(project: &str, _repo: &str, id: &str) -> Result<()> {
    match auth::get_credentials() {
        Ok(creds) => {
            let client = create_client()?;
            let pr_client = client.pull_requests_client();

            let pr_id = id
                .parse::<i32>()
                .map_err(|_| anyhow::anyhow!("Invalid pull request ID, must be a number"))?;

            let pull_request = pr_client
                .get_pull_request_by_id(creds.organization, pr_id, project)
                .await?;

            println!("Pull Request Details:");
            println!("  ID: {}", pull_request.pull_request_id);
            println!("  Title: {}", pull_request.title.unwrap_or_default());
            if let Some(description) = pull_request.description {
                if !description.is_empty() {
                    println!("  Description: {}", description);
                }
            }
            println!("  Status: {:?}", pull_request.status);
            println!("  Source Branch: {}", pull_request.source_ref_name);
            println!("  Target Branch: {}", pull_request.target_ref_name);
            println!("  Created: {}", pull_request.creation_date);

            Ok(())
        }
        Err(e) => {
            eprintln!("Unable to retrieve pull request");
            Err(e)
        }
    }
}
