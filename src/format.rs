/// Render the `format` config template ("[$description]: $command $tags") for a
/// single snippet, used to build the searchable text handed to the selector.
/// Multiline commands are flattened to a literal `\n` so each snippet stays one line.
pub fn render_template(format: &str, description: &str, command: &str, tags: &[String]) -> String {
    let flattened_command = command.replace('\n', "\\n");
    let tags_str = tags.iter().map(|t| format!("#{t} ")).collect::<String>();

    format
        .replacen("$description", description, 1)
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
        let out = render_template("[$description]: $command $tags", "d", "c", &[]);
        assert_eq!(out, "[d]: c ");
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
