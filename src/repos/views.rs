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
}
