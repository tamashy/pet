/// Which substituted field a character range of `render_template_fields`'s output
/// came from — used by the built-in picker to color description/command/tags
/// distinctly, the same way `color: true` does for external selectors via ANSI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldRole {
    Description,
    Command,
    Tags,
}

/// Same substitution as `render_template`, but instead of optionally embedding
/// ANSI codes, reports which *character* range (not byte — `fuzzy-matcher`'s
/// match indices are char-based) of the output belongs to each substituted
/// field. `render_template` stays untouched by this — it's a separate,
/// additive function so the well-exercised ANSI/external-selector path can't
/// regress from this change.
pub fn render_template_fields(
    format: &str,
    description: &str,
    command: &str,
    tags: &[String],
) -> (String, Vec<(FieldRole, std::ops::Range<usize>)>) {
    let flattened_command = command.replace('\n', "\\n");
    let tags_str = tags.iter().map(|t| format!("#{t} ")).collect::<String>();

    // Position each placeholder actually appears at in `format` (first
    // occurrence only, matching `render_template`'s `replacen(..., 1)`), then
    // walk them in that order so the output — and each field's range within it
    // — comes out right regardless of how the user has arranged `format`.
    let mut placeholders: Vec<(usize, &str, FieldRole, &str)> = Vec::with_capacity(3);
    if let Some(pos) = format.find("$description") {
        placeholders.push((pos, "$description", FieldRole::Description, description));
    }
    if let Some(pos) = format.find("$command") {
        placeholders.push((
            pos,
            "$command",
            FieldRole::Command,
            flattened_command.as_str(),
        ));
    }
    if let Some(pos) = format.find("$tags") {
        placeholders.push((pos, "$tags", FieldRole::Tags, tags_str.as_str()));
    }
    placeholders.sort_by_key(|(pos, ..)| *pos);

    let mut output = String::new();
    let mut fields = Vec::with_capacity(placeholders.len());
    let mut cursor = 0;

    for (pos, marker, role, value) in placeholders {
        if pos < cursor {
            // A placeholder's marker text got consumed as part of an earlier
            // field's own value (e.g. `$tags` inside a custom `$description`
            // value) — nothing sensible to substitute there, so leave it as
            // literal text rather than double-counting.
            continue;
        }
        output.push_str(&format[cursor..pos]);
        let start = output.chars().count();
        output.push_str(value);
        let end = output.chars().count();
        fields.push((role, start..end));
        cursor = pos + marker.len();
    }
    output.push_str(&format[cursor..]);

    (output, fields)
}

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
    fn render_template_fields_matches_render_template_plain_output() {
        let (text, _) = render_template_fields(
            "[$description]: $command $tags",
            "d",
            "c",
            &["t".to_string()],
        );
        let plain = render_template(
            "[$description]: $command $tags",
            "d",
            "c",
            &["t".to_string()],
            false,
        );
        assert_eq!(text, plain);
    }

    // Ranges below are char, not byte, indices — the assertions slice `text` by
    // byte range directly, which only lines up because every fixture here is
    // ASCII (1 byte per char); `..._uses_char_indices_not_byte_offsets` below
    // is what actually proves the char-vs-byte distinction.

    #[test]
    fn render_template_fields_reports_correct_char_ranges() {
        let (text, fields) = render_template_fields(
            "[$description]: $command $tags",
            "greet",
            "echo hi",
            &["demo".to_string()],
        );
        assert_eq!(text, "[greet]: echo hi #demo ");

        let description = fields
            .iter()
            .find(|(role, _)| *role == FieldRole::Description)
            .unwrap();
        assert_eq!(&text[description.1.start..description.1.end], "greet");

        let command = fields
            .iter()
            .find(|(role, _)| *role == FieldRole::Command)
            .unwrap();
        assert_eq!(&text[command.1.start..command.1.end], "echo hi");

        let tags = fields
            .iter()
            .find(|(role, _)| *role == FieldRole::Tags)
            .unwrap();
        assert_eq!(&text[tags.1.start..tags.1.end], "#demo ");
    }

    #[test]
    fn render_template_fields_uses_char_indices_not_byte_offsets() {
        // "héllo" has a 2-byte 'é', so a byte-index range would land wrong here —
        // this only passes if ranges are counted in chars.
        let (text, fields) = render_template_fields("<$description>", "héllo", "", &[]);
        let (_, range) = &fields[0];
        assert_eq!(*range, 1..6);
        let collected: String = text
            .chars()
            .skip(range.start)
            .take(range.end - range.start)
            .collect();
        assert_eq!(collected, "héllo");
    }

    #[test]
    fn render_template_fields_handles_placeholders_in_any_order() {
        let (text, fields) = render_template_fields(
            "$tags | $command | $description",
            "d",
            "c",
            &["t".to_string()],
        );
        assert_eq!(text, "#t  | c | d");

        let roles: Vec<FieldRole> = fields.iter().map(|(role, _)| *role).collect();
        assert_eq!(
            roles,
            vec![FieldRole::Tags, FieldRole::Command, FieldRole::Description]
        );
        for (role, range) in &fields {
            let slice = &text[range.start..range.end];
            match role {
                FieldRole::Tags => assert_eq!(slice, "#t "),
                FieldRole::Command => assert_eq!(slice, "c"),
                FieldRole::Description => assert_eq!(slice, "d"),
            }
        }
    }

    #[test]
    fn render_template_fields_omits_missing_placeholders() {
        let (text, fields) = render_template_fields("$description: $command", "d", "c", &[]);
        assert_eq!(text, "d: c");
        let roles: Vec<FieldRole> = fields.iter().map(|(role, _)| *role).collect();
        assert_eq!(roles, vec![FieldRole::Description, FieldRole::Command]);
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
