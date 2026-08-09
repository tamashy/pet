/// Render the `format` config template ("[$description]: $command $tags") for a
/// single snippet, used to build the searchable text handed to the selector.
/// Multiline commands are flattened to a literal `\n` so each snippet stays one line.
///
/// `color`, when true, wraps `$description`/`$tags` in ANSI color codes
/// unconditionally (no TTY check — this text is piped to the selector, e.g. fzf's
/// `--ansi`, not printed directly), matching Go pet's `general.color`/`--color`:
/// force-coloring the text fed to the selector so it renders in color there even
/// though the pipe itself isn't a terminal.
pub fn render_template(
    format: &str,
    description: &str,
    command: &str,
    tags: &[String],
    color: bool,
) -> String {
    use owo_colors::OwoColorize;

    let flattened_command = command.replace('\n', "\\n");
    let tags_str = tags.iter().map(|t| format!("#{t} ")).collect::<String>();

    let (description, tags_str) = if color {
        (
            description.bright_red().to_string(),
            tags_str.bright_cyan().to_string(),
        )
    } else {
        (description.to_string(), tags_str)
    };

    format
        .replacen("$description", &description, 1)
        .replacen("$command", &flattened_command, 1)
        .replacen("$tags", &tags_str, 1)
}

/// Truncate `s` to at most `width` characters (appending "..." if truncated and
/// there's room for it), then right-pad with spaces to exactly `width` characters.
/// Used by `list --oneline`.
pub fn truncate_pad(s: &str, width: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    let truncated: String = if chars.len() > width {
        if width <= 3 {
            chars[..width].iter().collect()
        } else {
            let keep = width - 3;
            let mut out: String = chars[..keep].iter().collect();
            out.push_str("...");
            out
        }
    } else {
        s.to_string()
    };

    let pad = width.saturating_sub(truncated.chars().count());
    format!("{truncated}{}", " ".repeat(pad))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_template_substitutes_all_placeholders() {
        let out = render_template("[$description]: $command $tags", "d", "c", &[], false);
        assert_eq!(out, "[d]: c ");
    }

    #[test]
    fn render_template_color_wraps_description_and_tags_only() {
        use owo_colors::OwoColorize;

        let out = render_template(
            "[$description]: $command $tags",
            "d",
            "c",
            &["t".to_string()],
            true,
        );
        // Command stays plain; description/tags get ANSI codes unconditionally
        // (this text is piped to the selector, not printed to our own stdout, so
        // there's no TTY to detect — compare against the same coloring calls
        // rather than hardcoding owo-colors' exact SGR sequences).
        let expected = format!("[{}]: c {}", "d".bright_red(), "#t ".bright_cyan());
        assert_eq!(out, expected);
    }

    #[test]
    fn truncate_pad_width_zero_never_panics_and_stays_empty() {
        assert_eq!(truncate_pad("anything", 0), "");
        assert_eq!(truncate_pad("", 0), "");
    }

    #[test]
    fn truncate_pad_never_exceeds_width_even_when_smaller_than_ellipsis() {
        for width in 0..=3 {
            let result = truncate_pad("a longer string than the width", width);
            assert_eq!(result.chars().count(), width, "width={width}");
        }
    }

    #[test]
    fn truncate_pad_empty_input_is_fully_padded() {
        assert_eq!(truncate_pad("", 4), "    ");
    }
}
