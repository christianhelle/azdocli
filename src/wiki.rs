use crate::auth::get_credentials;
use crate::project::get_project_or_default;
use anyhow::{anyhow, Result};
use azure_devops_rust_api::search::{self, models as search_models};
use azure_devops_rust_api::wiki::{self, models};
use clap::Subcommand;
use colored::Colorize;
use tokio::fs;

#[derive(Subcommand, Clone)]
pub enum WikiSubCommands {
    /// List all wikis in a project
    List {
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
    /// Show details of a wiki
    Show {
        /// Wiki ID or name (optional if only one wiki exists in project)
        id: Option<String>,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
    /// Manage wiki pages
    Page {
        #[clap(subcommand)]
        subcommand: WikiPageSubCommands,
    },
}

#[derive(Subcommand, Clone)]
pub enum WikiPageSubCommands {
    /// List pages in a wiki
    List {
        /// Root path to list from (optional)
        path: Option<String>,
        /// Wiki ID or name (optional if only one wiki exists in project)
        #[clap(short, long)]
        wiki: Option<String>,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
    /// Show page content
    Show {
        /// Page path (e.g., /My-Page)
        path: String,
        /// Wiki ID or name
        #[clap(short, long)]
        wiki: Option<String>,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
        /// Open in web browser
        #[clap(long)]
        web: bool,
    },
    /// Download page content to a file
    Download {
        /// Page path (e.g., /My-Page)
        path: String,
        /// Output folder (defaults to current directory)
        #[clap(long)]
        dir: Option<String>,
        /// Output file name (defaults to page name)
        #[clap(long)]
        name: Option<String>,
        /// Overwrite existing file
        #[clap(long)]
        overwrite: bool,
        /// Wiki ID or name
        #[clap(short, long)]
        wiki: Option<String>,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
    /// Search wiki content
    Search {
        /// Search query
        query: String,
        /// Show snippets of content
        #[clap(long)]
        show_contents: bool,
        /// Limit the number of results (default: 3)
        #[clap(short, long, default_value_t = 3)]
        limit: i32,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
    /// Move or rename a page
    Move {
        /// Current page path
        path: String,
        /// New page path
        new_path: String,
        /// Wiki ID or name
        #[clap(short, long)]
        wiki: Option<String>,
        /// Team project name (optional if default project is set)
        #[clap(short, long)]
        project: Option<String>,
    },
}

pub async fn handle_command(subcommand: &WikiSubCommands) -> Result<()> {
    match subcommand {
        WikiSubCommands::List { project } => {
            let project_name = get_project_or_default(project.as_deref())?;
            let wikis = list_wikis(&project_name).await?;
            display_wikis(&wikis);
        }
        WikiSubCommands::Show { id, project } => {
            let project_name = get_project_or_default(project.as_deref())?;
            let wiki = resolve_wiki(&project_name, id.as_deref()).await?;
            display_wiki_details(&wiki);
        }
        WikiSubCommands::Page { subcommand } => {
            handle_page_command(subcommand).await?;
        }
    }
    Ok(())
}

fn normalize_name(name: &str) -> String {
    name.replace('-', " ").to_lowercase()
}

fn create_wiki_client() -> Result<wiki::Client> {
    let creds = get_credentials()?;
    let credential = azure_devops_rust_api::Credential::Pat(creds.pat);
    Ok(wiki::ClientBuilder::new(credential).build())
}

fn create_search_client() -> Result<search::Client> {
    let creds = get_credentials()?;
    let credential = azure_devops_rust_api::Credential::Pat(creds.pat);
    Ok(search::ClientBuilder::new(credential).build())
}

async fn list_wikis(project: &str) -> Result<Vec<models::WikiV2>> {
    let creds = get_credentials()?;
    let client = create_wiki_client()?;
    Ok(client
        .wikis_client()
        .list(creds.organization, project)
        .await?
        .value)
}

fn display_wikis(wikis: &[models::WikiV2]) {
    if wikis.is_empty() {
        println!("No wikis found.");
        return;
    }

    println!(
        "{:<40} {:<40} {:<10}",
        "ID".bold(),
        "Name".bold(),
        "Type".bold()
    );
    println!("{}", "-".repeat(95));

    for wiki in wikis {
        println!(
            "{:<40} {:<40} {:?}",
            wiki.id
                .as_ref()
                .map(|id| id.to_string())
                .unwrap_or_default(),
            wiki.wiki_create_base_parameters
                .name
                .as_deref()
                .unwrap_or_default(),
            wiki.wiki_create_base_parameters.type_
        );
    }
}

fn display_wiki_details(wiki: &models::WikiV2) {
    println!("{}", "Wiki Details".bold().underline());
    println!(
        "{:<15} {}",
        "ID:",
        wiki.id
            .as_ref()
            .map(|id| id.to_string())
            .unwrap_or_default()
    );
    println!(
        "{:<15} {}",
        "Name:",
        wiki.wiki_create_base_parameters
            .name
            .as_deref()
            .unwrap_or_default()
    );
    println!(
        "{:<15} {:?}",
        "Type:", wiki.wiki_create_base_parameters.type_
    );
    if let Some(ref remote_url) = wiki.remote_url {
        println!("{:<15} {}", "Remote URL:", remote_url);
    }
}

async fn resolve_wiki(project: &str, wiki_identifier: Option<&str>) -> Result<models::WikiV2> {
    let wikis = list_wikis(project).await?;
    if wikis.is_empty() {
        return Err(anyhow!("No wikis found in project {}", project));
    }

    if let Some(identifier) = wiki_identifier {
        // Try to find by ID first
        if let Some(wiki) = wikis
            .iter()
            .find(|w| w.id.as_ref().map(|id| *id == identifier).unwrap_or(false))
        {
            return Ok(wiki.clone());
        }
        // Try to find by exact name
        if let Some(wiki) = wikis.iter().find(|w| {
            w.wiki_create_base_parameters
                .name
                .as_deref()
                .map(|n| n == identifier)
                .unwrap_or(false)
        }) {
            return Ok(wiki.clone());
        }
        // Try to find by normalized name (spaces/dashes treated as equivalent)
        let normalized_identifier = normalize_name(identifier);
        if let Some(wiki) = wikis.iter().find(|w| {
            w.wiki_create_base_parameters
                .name
                .as_deref()
                .map(|n| normalize_name(n) == normalized_identifier)
                .unwrap_or(false)
        }) {
            return Ok(wiki.clone());
        }
        Err(anyhow!("Wiki '{}' not found", identifier))
    } else if wikis.len() == 1 {
        Ok(wikis[0].clone())
    } else {
        Err(anyhow!(
            "Multiple wikis found. Please specify which one to use with --wiki"
        ))
    }
}

async fn handle_page_command(subcommand: &WikiPageSubCommands) -> Result<()> {
    match subcommand {
        WikiPageSubCommands::List {
            wiki,
            project,
            path,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            let wiki_obj = resolve_wiki(&project_name, wiki.as_deref()).await?;
            let wiki_id = wiki_obj
                .id
                .ok_or_else(|| anyhow!("Wiki ID is missing"))?
                .to_string();
            let pages = list_pages(&project_name, &wiki_id, path.as_deref()).await?;
            display_pages(&pages);
        }
        WikiPageSubCommands::Show {
            wiki,
            path,
            project,
            web,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            let wiki_obj = resolve_wiki(&project_name, wiki.as_deref()).await?;
            let wiki_id = wiki_obj
                .id
                .ok_or_else(|| anyhow!("Wiki ID is missing"))?
                .to_string();
            let page = get_page(&project_name, &wiki_id, path).await?;
            if *web {
                if let Some(ref remote_url) = page.remote_url {
                    println!("Opening {} in browser...", remote_url);
                    #[cfg(target_os = "macos")]
                    std::process::Command::new("open").arg(remote_url).spawn()?;
                    #[cfg(target_os = "linux")]
                    std::process::Command::new("xdg-open")
                        .arg(remote_url)
                        .spawn()?;
                    #[cfg(target_os = "windows")]
                    std::process::Command::new("explorer")
                        .arg(remote_url)
                        .spawn()?;
                } else {
                    println!("Web URL not available for this page.");
                }
            } else {
                display_page_content(&page);
            }
        }
        WikiPageSubCommands::Download {
            wiki,
            path,
            dir,
            name,
            overwrite,
            project,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            let wiki_obj = resolve_wiki(&project_name, wiki.as_deref()).await?;
            let wiki_id = wiki_obj
                .id
                .ok_or_else(|| anyhow!("Wiki ID is missing"))?
                .to_string();
            let output_dir = dir.as_deref().unwrap_or(".");
            download_page_single(
                &project_name,
                &wiki_id,
                path,
                output_dir,
                name.as_deref(),
                *overwrite,
            )
            .await?;
        }
        WikiPageSubCommands::Search {
            query,
            show_contents,
            limit,
            project,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            let results = search_wiki(&project_name, query, *limit).await?;
            display_search_results(&results, *show_contents);
        }
        WikiPageSubCommands::Move {
            wiki,
            path,
            new_path,
            project,
        } => {
            let project_name = get_project_or_default(project.as_deref())?;
            let wiki_obj = resolve_wiki(&project_name, wiki.as_deref()).await?;
            let wiki_id = wiki_obj
                .id
                .ok_or_else(|| anyhow!("Wiki ID is missing"))?
                .to_string();
            move_page(&project_name, &wiki_id, path, new_path).await?;
            println!("Page moved from {} to {} successfully.", path, new_path);
        }
    }
    Ok(())
}

async fn list_pages(
    project: &str,
    wiki_id: &str,
    path: Option<&str>,
) -> Result<Vec<models::WikiPage>> {
    let creds = get_credentials()?;
    let client = create_wiki_client()?;
    let search_path = path.unwrap_or("/");

    // Try exact path first
    match client
        .pages_client()
        .get_page(&creds.organization, project, wiki_id)
        .path(search_path)
        .recursion_level("oneLevel")
        .await
    {
        Ok(page) => return Ok(page.sub_pages),
        Err(_) if search_path != "/" => {
            // Fallback: search for normalized path match
            let normalized_search = normalize_name(search_path);
            let found_path = find_normalized_path(
                &client,
                &creds.organization,
                project,
                wiki_id,
                search_path,
                &normalized_search,
            )
            .await?;

            let page = client
                .pages_client()
                .get_page(&creds.organization, project, wiki_id)
                .path(&found_path)
                .recursion_level("oneLevel")
                .await?;
            return Ok(page.sub_pages);
        }
        Err(_) => {}
    }

    Err(anyhow!("Failed to list pages at path: {}", search_path))
}

async fn get_page(project: &str, wiki_id: &str, path: &str) -> Result<models::WikiPage> {
    let creds = get_credentials()?;
    let client = create_wiki_client()?;

    // Try exact path first
    match client
        .pages_client()
        .get_page(&creds.organization, project, wiki_id)
        .path(path)
        .include_content(true)
        .await
    {
        Ok(page) => Ok(page),
        Err(_) => {
            // Fallback: try to find by normalized path
            let normalized_search = normalize_name(path);
            let found_path = find_normalized_path(
                &client,
                &creds.organization,
                project,
                wiki_id,
                path,
                &normalized_search,
            )
            .await?;

            client
                .pages_client()
                .get_page(&creds.organization, project, wiki_id)
                .path(&found_path)
                .include_content(true)
                .await
                .map_err(|_| anyhow!("Failed to get page: {}", path))
        }
    }
}

async fn download_page_single(
    project: &str,
    wiki_id: &str,
    path: &str,
    output_dir: &str,
    output_name: Option<&str>,
    overwrite: bool,
) -> Result<()> {
    let page = get_page(project, wiki_id, path).await?;

    if let Some(ref content) = page.wiki_page_create_or_update_parameters.content {
        // Determine output filename
        let filename = if let Some(name) = output_name {
            // Use provided name as-is
            name.to_string()
        } else {
            // Extract filename from page path
            let page_name = path.trim_start_matches('/').trim_end_matches('/');
            if page_name.is_empty() || path == "/" {
                "index.md".to_string()
            } else {
                // Get the last component and add .md extension if not present
                let last_component = page_name.split('/').next_back().unwrap_or("index");
                if last_component.ends_with(".md") {
                    last_component.to_string()
                } else {
                    format!("{}.md", last_component)
                }
            }
        };

        // Create output directory if it doesn't exist
        let dir_path = std::path::Path::new(output_dir);
        if !dir_path.exists() {
            fs::create_dir_all(dir_path).await?;
        }

        let file_path = dir_path.join(&filename);

        // Check if file exists and handle overwrite
        if file_path.exists() && !overwrite {
            return Err(anyhow!(
                "File already exists: {}. Use --overwrite to replace it.",
                file_path.display()
            ));
        }

        // Write the file
        fs::write(&file_path, content).await?;
        println!("Downloaded: {} -> {}", path, file_path.display());
        Ok(())
    } else {
        Err(anyhow!("Page {} has no content", path))
    }
}

async fn find_normalized_path(
    client: &wiki::Client,
    org: &str,
    project: &str,
    wiki_id: &str,
    search_path: &str,
    normalized_search: &str,
) -> Result<String> {
    let mut to_check = vec!["/".to_string()];

    while let Some(current_path) = to_check.pop() {
        let page = client
            .pages_client()
            .get_page(org, project, wiki_id)
            .path(&current_path)
            .recursion_level("oneLevel")
            .await?;

        for sub_page in &page.sub_pages {
            if let Some(sub_path) = &sub_page.path {
                if normalize_name(sub_path) == *normalized_search {
                    return Ok(sub_path.clone());
                }
                // Queue this path for deeper search
                to_check.push(sub_path.clone());
            }
        }
    }

    Err(anyhow!("Page not found: {}", search_path))
}

async fn search_wiki(
    project: &str,
    query: &str,
    limit: i32,
) -> Result<search_models::WikiSearchResponse> {
    let creds = get_credentials()?;
    let client = create_search_client()?;
    let request = search_models::WikiSearchRequest {
        entity_search_request: search_models::EntitySearchRequest {
            entity_search_request_base: search_models::EntitySearchRequestBase {
                search_text: Some(query.to_string()),
                ..Default::default()
            },
            top: Some(limit),
            ..Default::default()
        },
    };
    client
        .wiki_search_results_client()
        .fetch_wiki_search_results(creds.organization, request, project)
        .await
        .map_err(|e| anyhow!("Search failed: {}", e))
}

async fn move_page(
    project: &str,
    wiki_id: &str,
    path: &str,
    new_path: &str,
) -> Result<models::WikiPageMove> {
    let creds = get_credentials()?;
    let client = create_wiki_client()?;

    // Resolve source path (try exact, fallback to normalized)
    let resolved_path = match client
        .pages_client()
        .get_page(&creds.organization, project, wiki_id)
        .path(path)
        .await
    {
        Ok(_) => path.to_string(),
        Err(_) => {
            let normalized = normalize_name(path);
            find_normalized_path(
                &client,
                &creds.organization,
                project,
                wiki_id,
                path,
                &normalized,
            )
            .await?
        }
    };

    // Resolve target path (try exact, fallback to normalized)
    let resolved_new_path = match client
        .pages_client()
        .get_page(&creds.organization, project, wiki_id)
        .path(new_path)
        .await
    {
        Ok(_) => new_path.to_string(),
        Err(_) => {
            let normalized = normalize_name(new_path);
            find_normalized_path(
                &client,
                &creds.organization,
                project,
                wiki_id,
                new_path,
                &normalized,
            )
            .await?
        }
    };

    let request = models::WikiPageMoveParameters {
        path: Some(resolved_path),
        new_path: Some(resolved_new_path),
        ..Default::default()
    };
    client
        .page_moves_client()
        .create(&creds.organization, request, project, wiki_id)
        .await
        .map_err(|e| anyhow!("Failed to move page: {}", e))
}

fn display_pages(pages: &[models::WikiPage]) {
    if pages.is_empty() {
        println!("No pages found.");
        return;
    }

    println!("{:<40} {:<40}", "Path".bold(), "ID".bold());
    println!("{}", "-".repeat(85));

    for page in pages {
        println!(
            "{:<40} {}",
            page.path.as_deref().unwrap_or_default(),
            page.id
                .as_ref()
                .map(|id| id.to_string())
                .unwrap_or_default()
        );
    }
}

fn display_page_content(page: &models::WikiPage) {
    if let Some(ref content) = page.wiki_page_create_or_update_parameters.content {
        println!("{}", content);
    } else {
        println!("No content available for this page.");
    }
}

fn display_search_results(results: &search_models::WikiSearchResponse, show_contents: bool) {
    if let Some(count) = results.count {
        if count == 0 {
            println!("No results found.");
            return;
        }
        println!("Found {} results:\n", count);
    }

    for m in &results.results {
        let path = m.path.as_deref().unwrap_or_default();
        if show_contents {
            if let Some(ref wiki) = m.wiki {
                println!(
                    "{} (Wiki: {})",
                    path.bold(),
                    wiki.name.as_deref().unwrap_or_default()
                );
            } else {
                println!("{}", path.bold());
            }

            // Use a Set to avoid duplicate highlights for the same result
            let mut seen_highlights = std::collections::HashSet::new();
            for hit in &m.hits {
                for highlight in &hit.highlights {
                    let clean_highlight = highlight
                        .replace("<highlighthit>", "")
                        .replace("</highlighthit>", "")
                        .trim()
                        .to_string();

                    if !clean_highlight.is_empty()
                        && seen_highlights.insert(clean_highlight.clone())
                    {
                        println!("Snippet: {}\n", clean_highlight);
                    }
                }
            }
        } else {
            println!("{}", path);
        }
    }
}
