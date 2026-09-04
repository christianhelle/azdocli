//! Small helpers for rendering text that came from Azure DevOps.

/// Renders control characters in remote text visibly.
///
/// Comment bodies and author names come from Azure DevOps and are stored as
/// plain text, so they can contain terminal control sequences. Printing those
/// unchanged would let a comment author move the cursor, recolour, or erase
/// parts of the terminal, so escapes are shown literally instead. Newlines and
/// tabs are kept, since multi-line comments are ordinary.
pub fn escape_control_characters(text: &str) -> String {
    if !text
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return text.to_string();
    }

    text.chars()
        .map(|c| {
            if c.is_control() && c != '\n' && c != '\t' {
                format!("\\u{{{:04x}}}", c as u32)
            } else {
                c.to_string()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_control_characters_leaves_ordinary_text_alone() {
        assert_eq!(escape_control_characters("looks good"), "looks good");
        assert_eq!(
            escape_control_characters("line one\nline two\tindented"),
            "line one\nline two\tindented"
        );
    }

    #[test]
    fn escape_control_characters_escapes_terminal_escapes() {
        // A comment author must not be able to drive the reader's terminal.
        assert_eq!(
            escape_control_characters("\u{1b}[31mred\u{1b}[0m"),
            "\\u{001b}[31mred\\u{001b}[0m"
        );
    }

    #[test]
    fn escape_control_characters_escapes_carriage_returns_and_delete() {
        assert_eq!(escape_control_characters("a\rb"), "a\\u{000d}b");
        assert_eq!(escape_control_characters("a\u{7f}b"), "a\\u{007f}b");
    }
}
