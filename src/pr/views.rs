//! Result models for pull request commands and how each one prints.

use super::list::{created_on, draft_marker, header_width, short_branch, vote_of, Columns};
use crate::output::{md_table, Render};
use azure_devops_rust_api::git::models::GitPullRequest;
use colored::Colorize;
use serde::Serialize;
use std::io::{self, Write};

/// The pull requests a `repos pr list` found.
#[derive(Serialize)]
#[serde(transparent)]
pub(super) struct PullRequestList<'a> {
    pub pull_requests: &'a [GitPullRequest],
    #[serde(skip)]
    pub columns: Columns<'a>,
    /// What the console summary says when nothing matched.
    #[serde(skip)]
    pub empty_message: String,
}

impl Render for PullRequestList<'_> {
    fn text(&self, w: &mut dyn Write) -> io::Result<()> {
        if self.pull_requests.is_empty() {
            return writeln!(w, "{}", self.empty_message);
        }
        write_table(w, self.pull_requests, &self.columns)
    }

    fn markdown(&self, w: &mut dyn Write) -> io::Result<()> {
        if self.pull_requests.is_empty() {
            return writeln!(w, "_{}_", self.empty_message);
        }
        write_markdown_table(w, self.pull_requests, &self.columns)
    }
}

/// The signed-in user's pull requests, grouped the way the web portal's
/// "Mine" tab groups them.
#[derive(Serialize)]
pub(super) struct MyPullRequests<'a> {
    pub created: &'a [GitPullRequest],
    pub assigned: &'a [GitPullRequest],
    #[serde(skip)]
    pub columns: Columns<'a>,
    /// The signed-in user's identity ID, whose vote the assigned list shows.
    #[serde(skip)]
    pub me: &'a str,
}

impl MyPullRequests<'_> {
    fn sections(&self) -> [(&'static str, &[GitPullRequest], Columns<'_>); 2] {
        [
            ("Created by me", self.created, self.columns),
            (
                "Assigned to me",
                self.assigned,
                Columns {
                    vote_of: Some(self.me),
                    ..self.columns
                },
            ),
        ]
    }
}

impl Render for MyPullRequests<'_> {
    fn text(&self, w: &mut dyn Write) -> io::Result<()> {
        for (index, (title, pull_requests, columns)) in self.sections().into_iter().enumerate() {
            if index > 0 {
                writeln!(w)?;
            }
            writeln!(w, "{}", format!("{title} ({})", pull_requests.len()).bold())?;
            if pull_requests.is_empty() {
                writeln!(w, "  none")?;
            } else {
                write_table(w, pull_requests, &columns)?;
            }
        }
        Ok(())
    }

    fn markdown(&self, w: &mut dyn Write) -> io::Result<()> {
        for (index, (title, pull_requests, columns)) in self.sections().into_iter().enumerate() {
            if index > 0 {
                writeln!(w)?;
            }
            writeln!(w, "## {title} ({})\n", pull_requests.len())?;
            if pull_requests.is_empty() {
                writeln!(w, "_none_")?;
            } else {
                write_markdown_table(w, pull_requests, &columns)?;
            }
        }
        Ok(())
    }
}

/// The cells of one pull request row, in column order.
fn row_cells(pr: &GitPullRequest, columns: &Columns) -> Vec<String> {
    let mut cells = vec![
        pr.pull_request_id.to_string(),
        format!("{:?}", pr.status).to_lowercase(),
        draft_marker(pr).to_string(),
        pr.title.as_deref().unwrap_or("-").to_string(),
    ];
    if columns.repository {
        cells.push(pr.repository.name.clone());
    }
    cells.push(
        pr.created_by
            .graph_subject_base
            .display_name
            .as_deref()
            .unwrap_or("-")
            .to_string(),
    );
    if let Some(identity_id) = columns.vote_of {
        cells.push(vote_of(pr, identity_id).to_string());
    }
    cells.push(created_on(pr));
    cells.push(format!(
        "{} -> {}",
        short_branch(&pr.source_ref_name),
        short_branch(&pr.target_ref_name)
    ));
    cells
}

fn headers(columns: &Columns) -> Vec<&'static str> {
    let mut headers = vec!["ID", "Status", "Draft", "Title"];
    if columns.repository {
        headers.push("Repository");
    }
    headers.push("Author");
    if columns.vote_of.is_some() {
        headers.push("My vote");
    }
    headers.extend(["Created", "Branches"]);
    headers
}

/// Pads each cell to its column width, leaving the last column unpadded, and
/// styles each padded cell. Padding before styling keeps the escape codes out
/// of the width.
fn padded_line(cells: &[String], widths: &[usize], style: fn(String) -> String) -> String {
    let mut line = String::new();
    for (cell, width) in cells.iter().zip(widths) {
        line.push_str(&style(format!("{cell:<width$}")));
        line.push(' ');
    }
    if let Some(last) = cells.last() {
        line.push_str(&style(last.clone()));
    }
    line
}

fn column_widths(columns: &Columns) -> Vec<usize> {
    let mut widths = vec![8, 11, 6, 50];
    if columns.repository {
        widths.push(24);
    }
    widths.push(24);
    if columns.vote_of.is_some() {
        widths.push(26);
    }
    widths.push(11);
    widths
}

fn write_table(
    w: &mut dyn Write,
    pull_requests: &[GitPullRequest],
    columns: &Columns,
) -> io::Result<()> {
    let widths = column_widths(columns);
    let header: Vec<String> = headers(columns).into_iter().map(String::from).collect();
    writeln!(
        w,
        "{}",
        padded_line(&header, &widths, |cell| cell.bold().to_string())
    )?;
    writeln!(w, "{}", "-".repeat(header_width(columns)))?;
    for pr in pull_requests {
        writeln!(
            w,
            "{}",
            padded_line(&row_cells(pr, columns), &widths, |cell| cell)
        )?;
    }
    Ok(())
}

fn write_markdown_table(
    w: &mut dyn Write,
    pull_requests: &[GitPullRequest],
    columns: &Columns,
) -> io::Result<()> {
    let rows: Vec<Vec<String>> = pull_requests
        .iter()
        .map(|pr| row_cells(pr, columns))
        .collect();
    write!(w, "{}", md_table(&headers(columns), &rows))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::Render;

    fn text(value: &impl Render) -> String {
        colored::control::set_override(false);
        let mut out = Vec::new();
        value.text(&mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    fn markdown(value: &impl Render) -> String {
        let mut out = Vec::new();
        value.markdown(&mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    fn pull_request(id: i32) -> GitPullRequest {
        serde_json::from_value(serde_json::json!({
            "pullRequestId": id,
            "title": "Add output formats",
            "createdBy": { "id": "author", "displayName": "Ada" },
            "creationDate": "2026-10-20T13:45:00Z",
            "isDraft": true,
            "repository": {
                "id": "repo-id",
                "name": "api",
                "url": "url",
                "project": { "name": "project", "visibility": "private" }
            },
            "sourceRefName": "refs/heads/feature",
            "targetRefName": "refs/heads/main",
            "status": "active",
            "url": "url",
            "reviewers": [{ "id": "me", "displayName": "Me", "vote": 10 }]
        }))
        .expect("fixture should deserialize")
    }

    const NO_EXTRA_COLUMNS: Columns = Columns {
        repository: false,
        vote_of: None,
    };

    fn list<'a>(prs: &'a [GitPullRequest], columns: Columns<'a>) -> PullRequestList<'a> {
        PullRequestList {
            pull_requests: prs,
            columns,
            empty_message: "No pull requests found in project 'project'".to_string(),
        }
    }

    #[test]
    fn pull_request_list_text_is_a_table() {
        let prs = [pull_request(12)];
        let expected = format!(
            "{:<8} {:<11} {:<6} {:<50} {:<24} {:<11} {}\n{}\n\
             {:<8} {:<11} {:<6} {:<50} {:<24} {:<11} {}\n",
            "ID",
            "Status",
            "Draft",
            "Title",
            "Author",
            "Created",
            "Branches",
            "-".repeat(135),
            12,
            "active",
            "draft",
            "Add output formats",
            "Ada",
            "2026-10-20",
            "feature -> main"
        );
        assert_eq!(text(&list(&prs, NO_EXTRA_COLUMNS)), expected);
    }

    #[test]
    fn pull_request_list_text_adds_the_optional_columns() {
        let prs = [pull_request(12)];
        let columns = Columns {
            repository: true,
            vote_of: Some("me"),
        };
        let output = text(&list(&prs, columns));
        let mut lines = output.lines();
        let header = lines.next().unwrap();
        assert!(header.contains("Repository") && header.contains("My vote"));
        assert_eq!(lines.next().unwrap(), "-".repeat(135 + 25 + 27));
        let row = lines.next().unwrap();
        assert!(row.contains(&format!("{:<24} ", "api")) && row.contains("approved"));
    }

    #[test]
    fn pull_request_list_text_prints_the_empty_message() {
        assert_eq!(
            text(&list(&[], NO_EXTRA_COLUMNS)),
            "No pull requests found in project 'project'\n"
        );
    }

    #[test]
    fn pull_request_list_markdown_is_a_table() {
        let prs = [pull_request(12)];
        assert_eq!(
            markdown(&list(&prs, NO_EXTRA_COLUMNS)),
            "| ID | Status | Draft | Title | Author | Created | Branches |\n\
             | --- | --- | --- | --- | --- | --- | --- |\n\
             | 12 | active | draft | Add output formats | Ada | 2026-10-20 | feature -> main |\n"
        );
    }

    #[test]
    fn pull_request_list_serializes_as_an_array() {
        let prs = [pull_request(12)];
        let value = serde_json::to_value(list(&prs, NO_EXTRA_COLUMNS)).unwrap();
        assert_eq!(value[0]["pullRequestId"], 12);
    }

    fn mine<'a>(
        created: &'a [GitPullRequest],
        assigned: &'a [GitPullRequest],
    ) -> MyPullRequests<'a> {
        MyPullRequests {
            created,
            assigned,
            columns: NO_EXTRA_COLUMNS,
            me: "me",
        }
    }

    #[test]
    fn my_pull_requests_text_has_both_sections() {
        let created = [pull_request(1)];
        let output = text(&mine(&created, &[]));
        assert!(output.starts_with("Created by me (1)\n"));
        assert!(output.ends_with("\n\nAssigned to me (0)\n  none\n"));
    }

    #[test]
    fn my_pull_requests_markdown_has_both_sections() {
        let created = [pull_request(1)];
        let output = markdown(&mine(&created, &[]));
        assert!(output.starts_with("## Created by me (1)\n\n| ID |"));
        assert!(output.ends_with("\n## Assigned to me (0)\n\n_none_\n"));
    }

    #[test]
    fn my_pull_requests_serialize_as_two_named_lists() {
        let created = [pull_request(1)];
        let assigned = [pull_request(2)];
        let value = serde_json::to_value(mine(&created, &assigned)).unwrap();
        assert_eq!(value["created"][0]["pullRequestId"], 1);
        assert_eq!(value["assigned"][0]["pullRequestId"], 2);
        assert!(value.get("me").is_none());
    }
}
