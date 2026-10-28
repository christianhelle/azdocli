//! Result models for pipeline commands and how each one prints.

use crate::output::{md_table, Render};
use azure_devops_rust_api::build::models::BuildDefinitionReference;
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
            "| ID | Name | Folder |\n| --- | --- | --- |\n| 7 | ci | \\apps |\n"
        );
    }

    #[test]
    fn pipeline_list_serializes_as_an_array() {
        let pipelines = [pipeline_reference()];
        let value = serde_json::to_value(PipelineList(&pipelines)).unwrap();
        assert_eq!(value[0]["name"], "ci");
    }
}
