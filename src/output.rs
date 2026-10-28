//! The rendering layer: commands build a result model, and this module prints
//! it in the format the user picked with `--output`.

use std::io::{self, Write};

/// How a command prints its result.
#[derive(clap::ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OutputFormat {
    /// The human-readable console summary.
    #[default]
    Text,
    /// The full result serialized as JSON.
    Json,
    /// The full result serialized as YAML.
    Yaml,
    /// The console summary as Markdown.
    Markdown,
}

/// A command result that knows how to print itself for people. JSON and YAML
/// come from its `Serialize` impl.
pub trait Render: serde::Serialize {
    /// Writes the console summary.
    fn text(&self, w: &mut dyn Write) -> io::Result<()>;
    /// Writes the console summary as Markdown.
    fn markdown(&self, w: &mut dyn Write) -> io::Result<()>;
}

/// Writes `value` to `w` in `format`.
pub fn render<T: Render>(value: &T, format: OutputFormat, w: &mut dyn Write) -> anyhow::Result<()> {
    match format {
        OutputFormat::Text => value.text(w)?,
        OutputFormat::Markdown => value.markdown(w)?,
        OutputFormat::Json => {
            serde_json::to_writer_pretty(&mut *w, value)?;
            writeln!(w)?;
        }
        OutputFormat::Yaml => serde_norway::to_writer(&mut *w, value)?,
    }
    Ok(())
}

/// Writes `value` to stdout in `format`.
pub fn emit<T: Render>(value: &T, format: OutputFormat) -> anyhow::Result<()> {
    render(value, format, &mut io::stdout().lock())
}

/// Fails for any format but text, for commands not yet moved onto `Render`.
pub fn require_text(format: OutputFormat, command: &str) -> anyhow::Result<()> {
    if format == OutputFormat::Text {
        return Ok(());
    }
    let name = clap::ValueEnum::to_possible_value(&format)
        .map(|value| value.get_name().to_string())
        .unwrap_or_default();
    anyhow::bail!("--output {name} is not supported for '{command}' yet")
}

/// Builds a Markdown table, escaping cell text so it cannot break the layout.
pub fn md_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let line = |cells: Vec<String>| format!("| {} |\n", cells.join(" | "));
    let mut table = line(headers.iter().map(|h| md_cell(h)).collect());
    table.push_str(&line(vec!["---".to_string(); headers.len()]));
    for row in rows {
        table.push_str(&line(row.iter().map(|cell| md_cell(cell)).collect()));
    }
    table
}

/// Escapes `|` and flattens line breaks so text fits in one table cell.
fn md_cell(text: &str) -> String {
    text.replace("\r\n", " ")
        .replace(['\r', '\n'], " ")
        .replace('|', "\\|")
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Args {
        #[clap(short, long, value_enum, default_value_t)]
        output: OutputFormat,
    }

    #[test]
    fn output_format_defaults_to_text() {
        let args = Args::try_parse_from(["azdocli"]).unwrap();
        assert_eq!(args.output, OutputFormat::Text);
    }

    #[test]
    fn output_format_parses_every_format() {
        for (value, expected) in [
            ("text", OutputFormat::Text),
            ("json", OutputFormat::Json),
            ("yaml", OutputFormat::Yaml),
            ("markdown", OutputFormat::Markdown),
        ] {
            let args = Args::try_parse_from(["azdocli", "-o", value]).unwrap();
            assert_eq!(args.output, expected);
        }
    }

    #[test]
    fn output_format_rejects_unknown_values() {
        assert!(Args::try_parse_from(["azdocli", "--output", "xml"]).is_err());
    }

    #[derive(serde::Serialize)]
    struct Sample {
        name: String,
        count: u32,
    }

    impl Render for Sample {
        fn text(&self, w: &mut dyn Write) -> io::Result<()> {
            writeln!(w, "{} has {}", self.name, self.count)
        }

        fn markdown(&self, w: &mut dyn Write) -> io::Result<()> {
            writeln!(w, "**{}** has {}", self.name, self.count)
        }
    }

    fn sample() -> Sample {
        Sample {
            name: "repo".to_string(),
            count: 3,
        }
    }

    fn rendered(format: OutputFormat) -> String {
        let mut out = Vec::new();
        render(&sample(), format, &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn render_json_serializes_the_whole_model() {
        assert_eq!(
            rendered(OutputFormat::Json),
            "{\n  \"name\": \"repo\",\n  \"count\": 3\n}\n"
        );
    }

    #[test]
    fn render_yaml_serializes_the_whole_model() {
        assert_eq!(rendered(OutputFormat::Yaml), "name: repo\ncount: 3\n");
    }

    #[test]
    fn render_text_uses_the_console_summary() {
        assert_eq!(rendered(OutputFormat::Text), "repo has 3\n");
    }

    #[test]
    fn render_markdown_uses_the_markdown_summary() {
        assert_eq!(rendered(OutputFormat::Markdown), "**repo** has 3\n");
    }

    #[test]
    fn md_table_writes_header_separator_and_rows() {
        let table = md_table(
            &["ID", "Name"],
            &[
                vec!["1".to_string(), "alpha".to_string()],
                vec!["2".to_string(), "beta".to_string()],
            ],
        );
        assert_eq!(
            table,
            "| ID | Name |\n| --- | --- |\n| 1 | alpha |\n| 2 | beta |\n"
        );
    }

    #[test]
    fn md_table_escapes_pipes_and_newlines_in_cells() {
        let table = md_table(&["Title"], &[vec!["a|b\r\nc".to_string()]]);
        assert_eq!(table, "| Title |\n| --- |\n| a\\|b c |\n");
    }

    #[test]
    fn require_text_allows_text_output() {
        assert!(require_text(OutputFormat::Text, "wiki list").is_ok());
    }

    #[test]
    fn require_text_rejects_other_formats_with_a_clear_message() {
        let err = require_text(OutputFormat::Json, "wiki list").unwrap_err();
        assert_eq!(
            err.to_string(),
            "--output json is not supported for 'wiki list' yet"
        );
    }
}
