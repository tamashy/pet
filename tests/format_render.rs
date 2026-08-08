use pet::format::{render_template, truncate_pad};

#[test]
fn renders_default_template() {
    let out = render_template(
        "[$description]: $command $tags",
        "greet",
        "echo hi",
        &["demo".to_string(), "sample".to_string()],
    );
    assert_eq!(out, "[greet]: echo hi #demo #sample ");
}

#[test]
fn renders_template_with_no_tags() {
    let out = render_template("[$description]: $command $tags", "greet", "echo hi", &[]);
    assert_eq!(out, "[greet]: echo hi ");
}

#[test]
fn flattens_multiline_commands_to_literal_backslash_n() {
    let out = render_template(
        "[$description]: $command $tags",
        "multi",
        "echo one\necho two",
        &[],
    );
    assert_eq!(out, "[multi]: echo one\\necho two ");
}

#[test]
fn truncate_pad_leaves_short_strings_padded() {
    assert_eq!(truncate_pad("hi", 5), "hi   ");
}

#[test]
fn truncate_pad_truncates_long_strings_with_ellipsis() {
    let result = truncate_pad("this is a long description", 10);
    assert_eq!(result.chars().count(), 10);
    assert_eq!(result, "this is...");
}

#[test]
fn truncate_pad_exact_width_is_unchanged() {
    assert_eq!(truncate_pad("exact", 5), "exact");
}
