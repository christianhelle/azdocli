use crate::{auth, repos};
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

        /// Source branch for the pull request (e.g., 'feature/my-feature')
        #[clap(short, long)]
        source: String,

        /// Target branch for the pull request (defaults to 'main')
        #[clap(long, default_value = "main")]
        target: String,
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
            source,
            target,
        } => {
            let project_name = auth::get_project_or_default(project.as_deref())?;
            create_pull_request(
                &project_name,
                repo,
                title.as_deref(),
                description.as_deref(),
                source,
                target,
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

async fn create_pull_request(
    project: &str,
    repo: &str,
    title: Option<&str>,
    description: Option<&str>,
    source: &str,
    target: &str,
) -> Result<()> {
    match auth::get_credentials() {
        Ok(creds) => {
            let client = create_client()?;

            let repository = repos::get_repo(project, repo).await?;

            let pr_client = client.pull_requests_client();

            let source_ref = if source.starts_with("refs/heads/") {
                source.to_string()
            } else {
                format!("refs/heads/{}", source)
            };

            let target_ref = if target.starts_with("refs/heads/") {
                target.to_string()
            } else {
                format!("refs/heads/{}", target)
            };
            println!("Creating pull request:");
            println!("  Repository: {}", repo);
            println!("  Source branch: {}", source);
            println!("  Target branch: {}", target);
            println!("  Title: {}", title.unwrap_or("Default title"));

            let pr_options = git::models::GitPullRequestCreateOptions {
                source_ref_name: source_ref.clone(),
                target_ref_name: target_ref.clone(),
                title: title.unwrap_or("Pull Request").to_string(),
                description: description.map(|d| d.to_string()),
                is_draft: Some(false),
                labels: Vec::new(),
                merge_options: None,
                completion_options: None,
                work_item_refs: Vec::new(),
                reviewers: Vec::new(),
            };

            match pr_client
                .create(&creds.organization, &repository.id, project, pr_options)
                .await
            {
                Ok(created_pr) => {
                    println!("✅ Pull request created successfully!");
                    println!("  ID: {}", created_pr.pull_request_id);
                    println!("  Title: {}", created_pr.title.unwrap_or_default());
                    println!("  URL: {}", created_pr.url);
                    Ok(())
                }
                Err(e) => {
                    eprintln!("❌ Failed to create pull request: {}", e);
                    Err(anyhow::anyhow!("Failed to create pull request: {}", e))
                }
            }
        }
        Err(e) => {
            eprintln!("Unable to create pull request");
            Err(e)
        }
    }
}

async fn list_pull_requests(project: &str, repo: &str) -> Result<()> {
    match auth::get_credentials() {
        Ok(creds) => {
            let client = create_client()?;
            let pr_client = client.pull_requests_client();

            let pull_requests = pr_client
                .get_pull_requests_by_project(creds.organization, project)
                .await?;

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
