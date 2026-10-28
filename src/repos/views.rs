//! Result models for repository commands and how each one prints.

use crate::output::{md_table, Render};
use azure_devops_rust_api::git::models::GitRepository;
use serde::Serialize;
use std::io::{self, Write};

/// The repositories in a project.
#[derive(Serialize)]
#[serde(transparent)]
pub struct RepoList<'a>(pub &'a [GitRepository]);

impl Render for RepoList<'_> {
    fn text(&self, w: &mut dyn Write) -> io::Result<()> {
        for repo in self.0 {
            writeln!(w, "{}", repo.name)?;
        }
        Ok(())
    }

    fn markdown(&self, w: &mut dyn Write) -> io::Result<()> {
        let rows: Vec<Vec<String>> = self
            .0
            .iter()
            .map(|repo| {
                vec![
                    repo.name.clone(),
                    short_branch(repo.default_branch.as_deref()),
                    repo.id.clone(),
                ]
            })
            .collect();
        write!(w, "{}", md_table(&["Name", "Default branch", "ID"], &rows))
    }
}

/// One repository's details.
#[derive(Serialize)]
#[serde(transparent)]
pub struct RepoDetails<'a>(pub &'a GitRepository);

impl Render for RepoDetails<'_> {
    fn text(&self, w: &mut dyn Write) -> io::Result<()> {
        let repo = self.0;
        writeln!(w, "📋 Repository Details")?;
        writeln!(w, "=====================")?;
        writeln!(w, "📁 Name: {}", repo.name)?;
        writeln!(w, "🆔 ID: {}", repo.id)?;

        if let Some(url) = &repo.web_url {
            writeln!(w, "🌐 Web URL: {url}")?;
        }

        if let Some(remote_url) = &repo.remote_url {
            writeln!(w, "🔗 Remote URL (HTTPS): {remote_url}")?;
        }

        if let Some(ssh_url) = &repo.ssh_url {
            writeln!(w, "🔑 Clone URL (SSH): {ssh_url}")?;
        }

        if let Some(size) = repo.size {
            writeln!(w, "📦 Size: {}", format_size(size))?;
        }

        if let Some(default_branch) = &repo.default_branch {
            writeln!(w, "🌿 Default Branch: {default_branch}")?;
        }

        writeln!(w, "🎯 Project: {}", repo.project.name)?;

        if let Some(is_fork) = repo.is_fork {
            writeln!(w, "🍴 Is Fork: {}", yes_no(is_fork))?;
        }

        if let Some(is_disabled) = repo.is_disabled {
            writeln!(w, "⚠️  Is Disabled: {}", yes_no(is_disabled))?;
        }
        Ok(())
    }

    fn markdown(&self, w: &mut dyn Write) -> io::Result<()> {
        let repo = self.0;
        writeln!(w, "## {}\n", repo.name)?;
        writeln!(w, "- **ID:** {}", repo.id)?;
        if let Some(url) = &repo.web_url {
            writeln!(w, "- **Web URL:** {url}")?;
        }
        if let Some(remote_url) = &repo.remote_url {
            writeln!(w, "- **Remote URL (HTTPS):** {remote_url}")?;
        }
        if let Some(ssh_url) = &repo.ssh_url {
            writeln!(w, "- **Clone URL (SSH):** {ssh_url}")?;
        }
        if let Some(size) = repo.size {
            writeln!(w, "- **Size:** {}", format_size(size))?;
        }
        if repo.default_branch.is_some() {
            writeln!(
                w,
                "- **Default branch:** {}",
                short_branch(repo.default_branch.as_deref())
            )?;
        }
        writeln!(w, "- **Project:** {}", repo.project.name)?;
        if let Some(is_fork) = repo.is_fork {
            writeln!(w, "- **Fork:** {}", yes_no(is_fork))?;
        }
        if let Some(is_disabled) = repo.is_disabled {
            writeln!(w, "- **Disabled:** {}", yes_no(is_disabled))?;
        }
        Ok(())
    }
}

/// A repository size in MB, or KB when it is under a megabyte.
fn format_size(size: i64) -> String {
    let size_kilobytes = size as f64 / 1024.0;
    let size_megabytes = size_kilobytes / 1024.0;
    if size_megabytes >= 1.0 {
        format!("{size_megabytes:.2} MB ({size} bytes)")
    } else {
        format!("{size_kilobytes:.2} KB ({size} bytes)")
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "Yes"
    } else {
        "No"
    }
}

/// A default branch without its `refs/heads/` prefix, or `-` when unset.
fn short_branch(ref_name: Option<&str>) -> String {
    ref_name
        .map(|name| name.strip_prefix("refs/heads/").unwrap_or(name))
        .unwrap_or("-")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::Render;
    use serde_json::json;

    fn repo(name: &str) -> GitRepository {
        serde_json::from_value(json!({
            "id": format!("{name}-id"),
            "name": name,
            "url": format!("https://dev.azure.com/org/_apis/git/repositories/{name}"),
            "defaultBranch": "refs/heads/main",
            "project": { "name": "Fabrikam", "visibility": "private" }
        }))
        .unwrap()
    }

    fn text(value: &impl Render) -> String {
        let mut out = Vec::new();
        value.text(&mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    fn markdown(value: &impl Render) -> String {
        let mut out = Vec::new();
        value.markdown(&mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn repo_list_text_prints_one_name_per_line() {
        let repos = [repo("api"), repo("web")];
        assert_eq!(text(&RepoList(&repos)), "api\nweb\n");
    }

    #[test]
    fn repo_list_markdown_is_a_table() {
        let repos = [repo("api")];
        assert_eq!(
            markdown(&RepoList(&repos)),
            "| Name | Default branch | ID |\n| --- | --- | --- |\n| api | main | api-id |\n"
        );
    }

    #[test]
    fn repo_list_serializes_as_an_array_of_repositories() {
        let repos = [repo("api")];
        let value = serde_json::to_value(RepoList(&repos)).unwrap();
        assert_eq!(value[0]["name"], "api");
    }

    fn detailed_repo() -> GitRepository {
        let mut repo = repo("api");
        repo.web_url = Some("https://dev.azure.com/org/Fabrikam/_git/api".to_string());
        repo.ssh_url = Some("git@ssh.dev.azure.com:v3/org/Fabrikam/api".to_string());
        repo.size = Some(2 * 1024 * 1024);
        repo.is_fork = Some(false);
        repo
    }

    #[test]
    fn repo_details_text_matches_the_console_summary() {
        let repo = detailed_repo();
        assert_eq!(
            text(&RepoDetails(&repo)),
            "📋 Repository Details\n\
             =====================\n\
             📁 Name: api\n\
             🆔 ID: api-id\n\
             🌐 Web URL: https://dev.azure.com/org/Fabrikam/_git/api\n\
             🔑 Clone URL (SSH): git@ssh.dev.azure.com:v3/org/Fabrikam/api\n\
             📦 Size: 2.00 MB (2097152 bytes)\n\
             🌿 Default Branch: refs/heads/main\n\
             🎯 Project: Fabrikam\n\
             🍴 Is Fork: No\n"
        );
    }

    #[test]
    fn repo_details_text_shows_small_repos_in_kilobytes() {
        let mut repo = repo("api");
        repo.size = Some(1536);
        assert!(text(&RepoDetails(&repo)).contains("📦 Size: 1.50 KB (1536 bytes)\n"));
    }

    #[test]
    fn repo_details_markdown_lists_the_fields() {
        let repo = detailed_repo();
        assert_eq!(
            markdown(&RepoDetails(&repo)),
            "## api\n\n\
             - **ID:** api-id\n\
             - **Web URL:** https://dev.azure.com/org/Fabrikam/_git/api\n\
             - **Clone URL (SSH):** git@ssh.dev.azure.com:v3/org/Fabrikam/api\n\
             - **Size:** 2.00 MB (2097152 bytes)\n\
             - **Default branch:** main\n\
             - **Project:** Fabrikam\n\
             - **Fork:** No\n"
        );
    }

    #[test]
    fn repo_details_serializes_as_the_repository() {
        let repo = detailed_repo();
        let value = serde_json::to_value(RepoDetails(&repo)).unwrap();
        assert_eq!(value["name"], "api");
        assert_eq!(value["size"], 2097152);
    }
}
