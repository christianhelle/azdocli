//! Result models for pipeline commands and how each one prints.

use super::{format_optional, format_timestamp, pipeline_web_url, short_branch_name};
use crate::output::{md_table, Render};
use azure_devops_rust_api::build::models::{Build, BuildDefinition, BuildDefinitionReference};
use colored::Colorize;
use serde::Serialize;
use std::io::{self, Write};

/// The pipelines matching a `pipelines list`.
#[derive(Serialize)]
#[serde(transparent)]
pub struct PipelineList<'a>(pub &'a [BuildDefinitionReference]);

impl Render for PipelineList<'_> {
    fn text(&self, w: &mut dyn Write) -> io::Result<()> {
        if self.0.is_empty() {
            return writeln!(w, "No pipelines found.");
        }

        writeln!(
            w,
            "{:<10} {:<40} {}",
            "ID".bold(),
            "Name".bold(),
            "Folder".bold()
        )?;
        writeln!(w, "{}", "-".repeat(70))?;

        for pipeline in self.0 {
            let definition = &pipeline.definition_reference;
            writeln!(
                w,
                "{:<10} {:<40} {}",
                definition.id,
                definition.name.as_deref().unwrap_or("-"),
                definition.path.as_deref().unwrap_or("-")
            )?;
        }
        Ok(())
    }

    fn markdown(&self, w: &mut dyn Write) -> io::Result<()> {
        let rows: Vec<Vec<String>> = self
            .0
            .iter()
            .map(|pipeline| {
                let definition = &pipeline.definition_reference;
                vec![
                    definition.id.to_string(),
                    definition.name.clone().unwrap_or_else(|| "-".to_string()),
                    definition.path.clone().unwrap_or_else(|| "-".to_string()),
                ]
            })
            .collect();
        write!(w, "{}", md_table(&["ID", "Name", "Folder"], &rows))
    }
}

/// One pipeline definition's details.
#[derive(Serialize)]
#[serde(transparent)]
pub struct PipelineDetails<'a>(pub &'a BuildDefinition);

impl PipelineDetails<'_> {
    /// The label/value pairs both summaries show, in order, after the name.
    fn fields(&self) -> Vec<(&'static str, String)> {
        let pipeline = self.0;
        let reference = &pipeline.build_definition_reference;
        let definition = &reference.definition_reference;

        let mut fields = vec![
            ("ID", definition.id.to_string()),
            ("Folder", or_dash(definition.path.as_deref())),
            ("Revision", definition.revision.to_string()),
            ("Queue status", format!("{:?}", definition.queue_status)),
        ];
        if let Some(description) = &pipeline.description {
            fields.push(("Description", description.clone()));
        }
        if let Some(queue) = reference.queue.as_ref().and_then(|q| q.name.as_deref()) {
            fields.push(("Agent queue", queue.to_string()));
        }
        if let Some(repository) = &pipeline.repository {
            fields.push((
                "Repository",
                format!(
                    "{} ({})",
                    repository.name.as_deref().unwrap_or(&repository.id),
                    repository.type_.as_deref().unwrap_or("-")
                ),
            ));
            if let Some(default_branch) = &repository.default_branch {
                fields.push(("Default branch", default_branch.clone()));
            }
        }
        if let Some(author) = reference
            .authored_by
            .as_ref()
            .and_then(|identity| identity.graph_subject_base.display_name.as_deref())
        {
            fields.push(("Authored by", author.to_string()));
        }
        if let Some(created) = definition.created_date {
            fields.push(("Created", format_timestamp(created)));
        }
        if let Some(url) = pipeline_web_url(pipeline) {
            fields.push(("URL", url.to_string()));
        }
        fields
    }

    fn name(&self) -> String {
        or_dash(
            self.0
                .build_definition_reference
                .definition_reference
                .name
                .as_deref(),
        )
    }
}

impl Render for PipelineDetails<'_> {
    fn text(&self, w: &mut dyn Write) -> io::Result<()> {
        writeln!(w, "🔧 Pipeline")?;
        writeln!(w, "===========")?;
        let mut fields = self.fields();
        // The console summary lists the name second, right after the ID.
        fields.insert(1, ("Name", self.name()));
        for (label, value) in fields {
            writeln!(w, "{label}: {value}")?;
        }
        Ok(())
    }

    fn markdown(&self, w: &mut dyn Write) -> io::Result<()> {
        writeln!(w, "## {}\n", self.name())?;
        for (label, value) in self.fields() {
            writeln!(w, "- **{label}:** {value}")?;
        }
        Ok(())
    }
}

/// The runs matching a `pipelines runs list`.
#[derive(Serialize)]
#[serde(transparent)]
pub struct RunList<'a> {
    pub runs: &'a [Build],
    /// Whether more runs matched than were fetched.
    #[serde(skip)]
    pub more: bool,
}

impl RunList<'_> {
    fn rows(&self) -> Vec<[String; 7]> {
        self.runs
            .iter()
            .map(|run| {
                [
                    run.id.to_string(),
                    or_dash(run.build_number.as_deref()),
                    or_dash(run.definition.name.as_deref()),
                    format_optional(run.status.as_ref()),
                    format_optional(run.result.as_ref()),
                    or_dash(run.source_branch.as_deref().map(short_branch_name)),
                    run.queue_time
                        .map(format_timestamp)
                        .unwrap_or_else(|| "-".to_string()),
                ]
            })
            .collect()
    }
}

impl Render for RunList<'_> {
    fn text(&self, w: &mut dyn Write) -> io::Result<()> {
        if self.runs.is_empty() {
            return writeln!(w, "No runs found.");
        }

        writeln!(
            w,
            "{:<8} {:<16} {:<24} {:<12} {:<20} {:<20} {}",
            "Run".bold(),
            "Number".bold(),
            "Pipeline".bold(),
            "Status".bold(),
            "Result".bold(),
            "Branch".bold(),
            "Queued".bold()
        )?;
        writeln!(w, "{}", "-".repeat(125))?;

        for [id, number, pipeline, status, result, branch, queued] in self.rows() {
            writeln!(
                w,
                "{id:<8} {number:<16} {pipeline:<24} {status:<12} {result:<20} {branch:<20} {queued}"
            )?;
        }
        if self.more {
            writeln!(w, "\nMore runs exist; use --top to list more of them.")?;
        }
        Ok(())
    }

    fn markdown(&self, w: &mut dyn Write) -> io::Result<()> {
        let rows: Vec<Vec<String>> = self.rows().into_iter().map(Vec::from).collect();
        write!(
            w,
            "{}",
            md_table(
                &["Run", "Number", "Pipeline", "Status", "Result", "Branch", "Queued"],
                &rows
            )
        )
    }
}

fn or_dash(value: Option<&str>) -> String {
    value.unwrap_or("-").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::Render;
    use serde_json::json;

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

    fn pipeline_reference() -> BuildDefinitionReference {
        serde_json::from_value(json!({
            "id": 7,
            "name": "ci",
            "path": "\\apps",
            "revision": 3,
            "queueStatus": "enabled",
            "url": "https://dev.azure.com/org/_apis/build/definitions/7",
            "project": { "name": "Fabrikam", "visibility": "private" }
        }))
        .unwrap()
    }

    #[test]
    fn pipeline_list_text_is_a_table() {
        let pipelines = [pipeline_reference()];
        assert_eq!(
            text(&PipelineList(&pipelines)),
            format!(
                "{:<10} {:<40} {}\n{}\n{:<10} {:<40} {}\n",
                "ID",
                "Name",
                "Folder",
                "-".repeat(70),
                7,
                "ci",
                "\\apps"
            )
        );
    }

    #[test]
    fn pipeline_list_text_says_when_there_are_none() {
        assert_eq!(text(&PipelineList(&[])), "No pipelines found.\n");
    }

    #[test]
    fn pipeline_list_markdown_is_a_table() {
        let pipelines = [pipeline_reference()];
        assert_eq!(
            markdown(&PipelineList(&pipelines)),
            "| ID | Name | Folder |\n| --- | --- | --- |\n| 7 | ci | \\\\apps |\n"
        );
    }

    #[test]
    fn pipeline_list_serializes_as_an_array() {
        let pipelines = [pipeline_reference()];
        let value = serde_json::to_value(PipelineList(&pipelines)).unwrap();
        assert_eq!(value[0]["name"], "ci");
    }

    fn pipeline() -> BuildDefinition {
        serde_json::from_value(json!({
            "id": 7,
            "name": "ci",
            "path": "\\apps",
            "revision": 3,
            "queueStatus": "enabled",
            "url": "https://dev.azure.com/org/_apis/build/definitions/7",
            "project": { "name": "Fabrikam", "visibility": "private" },
            "description": "Builds the app",
            "repository": { "id": "repo-id", "name": "api", "type": "TfsGit" },
            "_links": { "web": { "href": "https://dev.azure.com/org/Fabrikam/_build?definitionId=7" } }
        }))
        .unwrap()
    }

    #[test]
    fn pipeline_details_text_matches_the_console_summary() {
        assert_eq!(
            text(&PipelineDetails(&pipeline())),
            "🔧 Pipeline\n\
             ===========\n\
             ID: 7\n\
             Name: ci\n\
             Folder: \\apps\n\
             Revision: 3\n\
             Queue status: Enabled\n\
             Description: Builds the app\n\
             Repository: api (TfsGit)\n\
             URL: https://dev.azure.com/org/Fabrikam/_build?definitionId=7\n"
        );
    }

    #[test]
    fn pipeline_details_markdown_lists_the_fields() {
        assert_eq!(
            markdown(&PipelineDetails(&pipeline())),
            "## ci\n\n\
             - **ID:** 7\n\
             - **Folder:** \\apps\n\
             - **Revision:** 3\n\
             - **Queue status:** Enabled\n\
             - **Description:** Builds the app\n\
             - **Repository:** api (TfsGit)\n\
             - **URL:** https://dev.azure.com/org/Fabrikam/_build?definitionId=7\n"
        );
    }

    #[test]
    fn pipeline_details_serializes_as_the_definition() {
        let value = serde_json::to_value(PipelineDetails(&pipeline())).unwrap();
        assert_eq!(value["name"], "ci");
        assert_eq!(value["repository"]["name"], "api");
    }

    fn run() -> Build {
        serde_json::from_value(json!({
            "id": 42,
            "buildNumber": "20260102.1",
            "definition": {
                "id": 7,
                "name": "ci",
                "revision": 3,
                "queueStatus": "enabled",
                "url": "https://dev.azure.com/org/_apis/build/definitions/7",
                "project": { "name": "Fabrikam", "visibility": "private" }
            },
            "project": { "name": "Fabrikam", "visibility": "private" },
            "priority": "normal",
            "reason": "manual",
            "status": "completed",
            "result": "succeeded",
            "sourceBranch": "refs/heads/main",
            "queueTime": "2026-01-02T03:04:05Z"
        }))
        .unwrap()
    }

    #[test]
    fn run_list_text_is_a_table() {
        let runs = [run()];
        let row_format = |values: [&str; 7]| {
            format!(
                "{:<8} {:<16} {:<24} {:<12} {:<20} {:<20} {}\n",
                values[0], values[1], values[2], values[3], values[4], values[5], values[6]
            )
        };
        let expected = row_format([
            "Run", "Number", "Pipeline", "Status", "Result", "Branch", "Queued",
        ]) + &format!("{}\n", "-".repeat(125))
            + &row_format([
                "42",
                "20260102.1",
                "ci",
                "Completed",
                "Succeeded",
                "main",
                "2026-01-02 03:04:05",
            ]);
        assert_eq!(
            text(&RunList {
                runs: &runs,
                more: false
            }),
            expected
        );
    }

    #[test]
    fn run_list_text_says_when_more_runs_exist() {
        let runs = [run()];
        assert!(text(&RunList {
            runs: &runs,
            more: true
        })
        .ends_with("\nMore runs exist; use --top to list more of them.\n"));
    }

    #[test]
    fn run_list_text_says_when_there_are_none() {
        assert_eq!(
            text(&RunList {
                runs: &[],
                more: false
            }),
            "No runs found.\n"
        );
    }

    #[test]
    fn run_list_markdown_is_a_table() {
        let runs = [run()];
        assert_eq!(
            markdown(&RunList {
                runs: &runs,
                more: false
            }),
            "| Run | Number | Pipeline | Status | Result | Branch | Queued |\n\
             | --- | --- | --- | --- | --- | --- | --- |\n\
             | 42 | 20260102.1 | ci | Completed | Succeeded | main | 2026-01-02 03:04:05 |\n"
        );
    }

    #[test]
    fn run_list_serializes_as_an_array_of_runs() {
        let runs = [run()];
        let value = serde_json::to_value(RunList {
            runs: &runs,
            more: true,
        })
        .unwrap();
        assert_eq!(value[0]["id"], 42);
    }
}
