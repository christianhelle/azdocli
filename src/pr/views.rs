//! Result models for pull request commands and how each one prints.

use super::list::{created_on, draft_marker, header_width, short_branch, vote_of, Columns};
use super::reviewers::{reviewer_name, vote_label};
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

/// One pull request, with the work items and open threads `show` looks up.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PullRequestDetails<'a> {
    #[serde(flatten)]
    pub pull_request: &'a GitPullRequest,
    pub work_item_ids: Vec<String>,
    pub open_comment_threads: usize,
}

impl PullRequestDetails<'_> {
    fn description(&self) -> Option<&str> {
        self.pull_request
            .description
            .as_deref()
            .filter(|description| !description.is_empty())
    }

    fn author(&self) -> &str {
        self.pull_request
            .created_by
            .graph_subject_base
            .display_name
            .as_deref()
            .unwrap_or("-")
    }

    fn labels(&self) -> Vec<&str> {
        self.pull_request
            .labels
            .iter()
            .filter_map(|label| label.name.as_deref())
            .collect()
    }

    fn reviewer_lines(&self) -> Vec<String> {
        self.pull_request
            .reviewers
            .iter()
            .map(|reviewer| {
                format!(
                    "{} ({}){}",
                    reviewer_name(reviewer),
                    vote_label(reviewer.vote),
                    if reviewer.is_required.unwrap_or(false) {
                        ", required"
                    } else {
                        ""
                    }
                )
            })
            .collect()
    }
}

impl Render for PullRequestDetails<'_> {
    fn text(&self, w: &mut dyn Write) -> io::Result<()> {
        let pr = self.pull_request;
        writeln!(w, "📋 Pull Request Details")?;
        writeln!(w, "=======================")?;
        writeln!(w, "  ID: {}", pr.pull_request_id)?;
        writeln!(w, "  Title: {}", pr.title.as_deref().unwrap_or_default())?;
        if let Some(description) = self.description() {
            writeln!(w, "  Description: {description}")?;
        }
        writeln!(w, "  Status: {:?}", pr.status)?;
        if pr.is_draft {
            writeln!(w, "  Draft: {}", "yes".yellow())?;
        }
        if let Some(merge_status) = pr.merge_status.as_ref() {
            writeln!(w, "  Merge status: {merge_status:?}")?;
        }
        if let Some(message) = pr.merge_failure_message.as_deref() {
            writeln!(w, "  Merge failure: {}", message.red())?;
        }
        if pr.auto_complete_set_by.is_some() {
            writeln!(w, "  Auto-complete: enabled")?;
        }
        writeln!(w, "  Source Branch: {}", short_branch(&pr.source_ref_name))?;
        writeln!(w, "  Target Branch: {}", short_branch(&pr.target_ref_name))?;
        writeln!(w, "  Created by: {}", self.author())?;
        writeln!(w, "  Created: {}", pr.creation_date)?;
        let labels = self.labels();
        if !labels.is_empty() {
            writeln!(w, "  Labels: {}", labels.join(", "))?;
        }
        let reviewers = self.reviewer_lines();
        if reviewers.is_empty() {
            writeln!(w, "  Reviewers: none")?;
        } else {
            writeln!(w, "  Reviewers:")?;
            for reviewer in reviewers {
                writeln!(w, "    • {reviewer}")?;
            }
        }
        if !self.work_item_ids.is_empty() {
            writeln!(w, "  Work Items: {}", self.work_item_ids.join(", "))?;
        }
        writeln!(w, "  Open comment threads: {}", self.open_comment_threads)
    }

    fn markdown(&self, w: &mut dyn Write) -> io::Result<()> {
        let pr = self.pull_request;
        writeln!(
            w,
            "## !{} {}\n",
            pr.pull_request_id,
            pr.title.as_deref().unwrap_or_default()
        )?;
        writeln!(w, "- **Status:** {:?}", pr.status)?;
        if pr.is_draft {
            writeln!(w, "- **Draft:** yes")?;
        }
        if let Some(merge_status) = pr.merge_status.as_ref() {
            writeln!(w, "- **Merge status:** {merge_status:?}")?;
        }
        if let Some(message) = pr.merge_failure_message.as_deref() {
            writeln!(w, "- **Merge failure:** {message}")?;
        }
        if pr.auto_complete_set_by.is_some() {
            writeln!(w, "- **Auto-complete:** enabled")?;
        }
        writeln!(
            w,
            "- **Branches:** {} -> {}",
            short_branch(&pr.source_ref_name),
            short_branch(&pr.target_ref_name)
        )?;
        writeln!(w, "- **Created by:** {}", self.author())?;
        writeln!(w, "- **Created:** {}", created_on(pr))?;
        let labels = self.labels();
        if !labels.is_empty() {
            writeln!(w, "- **Labels:** {}", labels.join(", "))?;
        }
        if let Some(description) = self.description() {
            writeln!(w, "\n### Description\n\n{description}")?;
        }
        writeln!(w, "\n### Reviewers\n")?;
        let reviewers = self.reviewer_lines();
        if reviewers.is_empty() {
            writeln!(w, "_none_")?;
        }
        for reviewer in reviewers {
            writeln!(w, "- {reviewer}")?;
        }
        writeln!(w)?;
        if !self.work_item_ids.is_empty() {
            writeln!(w, "- **Work items:** {}", self.work_item_ids.join(", "))?;
        }
        writeln!(
            w,
            "- **Open comment threads:** {}",
            self.open_comment_threads
        )
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

    fn details(pr: &GitPullRequest) -> PullRequestDetails<'_> {
        PullRequestDetails {
            pull_request: pr,
            work_item_ids: vec!["101".to_string(), "102".to_string()],
            open_comment_threads: 2,
        }
    }

    #[test]
    fn pull_request_details_text_matches_the_console_summary() {
        let pr = pull_request(12);
        let expected = format!(
            "📋 Pull Request Details\n\
             =======================\n\
             \x20 ID: 12\n\
             \x20 Title: Add output formats\n\
             \x20 Status: Active\n\
             \x20 Draft: yes\n\
             \x20 Source Branch: feature\n\
             \x20 Target Branch: main\n\
             \x20 Created by: Ada\n\
             \x20 Created: {}\n\
             \x20 Reviewers:\n\
             \x20   • Me (approved)\n\
             \x20 Work Items: 101, 102\n\
             \x20 Open comment threads: 2\n",
            pr.creation_date
        );
        assert_eq!(text(&details(&pr)), expected);
    }

    #[test]
    fn pull_request_details_text_skips_work_items_when_there_are_none() {
        let pr = pull_request(12);
        let mut view = details(&pr);
        view.work_item_ids.clear();
        assert!(!text(&view).contains("Work Items"));
    }

    #[test]
    fn pull_request_details_markdown_lists_the_fields() {
        let pr = pull_request(12);
        let output = markdown(&details(&pr));
        assert!(output
            .starts_with("## !12 Add output formats\n\n- **Status:** Active\n- **Draft:** yes\n"));
        assert!(output.contains("- **Branches:** feature -> main\n"));
        assert!(output.contains("\n### Reviewers\n\n- Me (approved)\n"));
        assert!(output.ends_with("- **Work items:** 101, 102\n- **Open comment threads:** 2\n"));
    }

    #[test]
    fn pull_request_details_serialize_the_pull_request_with_its_extras() {
        let pr = pull_request(12);
        let value = serde_json::to_value(details(&pr)).unwrap();
        assert_eq!(value["pullRequestId"], 12);
        assert_eq!(value["workItemIds"][1], "102");
        assert_eq!(value["openCommentThreads"], 2);
    }
}
