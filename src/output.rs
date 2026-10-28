//! The rendering layer: commands build a result model, and this module prints
//! it in the format the user picked with `--output`.

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
}
