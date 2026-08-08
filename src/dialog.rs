use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;

/// Matches `<...>`, capturing everything inside except a trailing whitespace char
/// right before `>` (so `<foo >` doesn't match). Ported verbatim from Go pet's
/// `dialog.parameterStringRegex` — see params.go. Deliberately permissive about
/// what the captured content can contain (including further `<`/`>` as the very
/// last character) to match Go's exact matching behavior around nested/broken
/// brackets; see the `extract_params` tests ported from Go's params_test.go.
static PARAM_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<([^<>]*[^\s])>").unwrap());

/// Matches one `|_..._|` segment of a pipe-delimited multi-default value, e.g. the
/// `|_John_|` in `<subject=|_John_||_Sam_|>`. Ported from Go pet's
/// `dialog.parameterMultipleValueRegex` (view.go).
static MULTI_DEFAULT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\|_(.*?)_\|").unwrap());

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    /// Raw default text, "" if the param had no `=`. May itself be a pipe-delimited
    /// multi-default string (see `parse_options`) — that parsing is a separate step,
    /// matching Go pet's split between params.go (extraction) and view.go (display).
    pub default: String,
}

/// Extract the unique `<name>` / `<name=default>` placeholders from `command`, in
/// first-seen order. Mirrors Go pet's `dialog.SearchForParams` exactly, including
/// its one subtle rule: among multiple occurrences of the same name, only
/// occurrences *with* an explicit default can change it (a later `<name>` with no
/// default never clears an earlier default), and the *last* occurrence that does
/// supply one wins — not the first.
pub fn extract_params(command: &str) -> Vec<Param> {
    let mut order: Vec<String> = Vec::new();
    let mut defaults: HashMap<String, String> = HashMap::new();
    let mut seen: HashSet<String> = HashSet::new();

    for caps in PARAM_RE.captures_iter(command) {
        let matched = &caps[1];
        let (name, default) = match matched.split_once('=') {
            Some((name, default)) => (name.to_string(), Some(default.to_string())),
            None => (matched.to_string(), None),
        };

        if seen.insert(name.clone()) {
            order.push(name.clone());
            defaults.insert(name, default.unwrap_or_default());
        } else if let Some(default) = default {
            defaults.insert(name, default);
        }
    }

    order
        .into_iter()
        .map(|name| {
            let default = defaults.remove(&name).unwrap_or_default();
            Param { name, default }
        })
        .collect()
}

/// Replace every `<name>` / `<name=default>` occurrence in `command` with its
/// resolved value from `values` (missing entries substitute as empty, matching Go's
/// zero-value map access in `dialog.insertParams`).
pub fn substitute(command: &str, values: &HashMap<String, String>) -> String {
    PARAM_RE
        .replace_all(command, |caps: &regex::Captures| {
            let matched = &caps[1];
            let name = matched.split_once('=').map_or(matched, |(name, _)| name);
            values.get(name).cloned().unwrap_or_default()
        })
        .into_owned()
}

/// If `default` is a pipe-delimited multi-default value (`|_opt1_||_opt2_|...`),
/// return its options in order; otherwise `None` (it's a plain single default, or
/// empty). Mirrors Go pet's `view.go` `generateMultipleParameterView` detection.
pub fn parse_options(default: &str) -> Option<Vec<String>> {
    let options: Vec<String> = MULTI_DEFAULT_RE
        .captures_iter(default)
        .map(|caps| caps[1].to_string())
        .collect();
    if options.is_empty() {
        None
    } else {
        Some(options)
    }
}

/// One param's current input state in the resolution dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldKind {
    /// Free-text entry, pre-filled with the default (empty if there wasn't one).
    /// `cursor` is a char index, not a byte offset.
    Text { buffer: String, cursor: usize },
    /// Pipe-delimited multi-default: cycle-only (←/→ or ↑/↓), no free text entry —
    /// a deliberate simplification vs. Go's dialog, where these fields are also
    /// technically free-text-editable, which makes the "currently selected option"
    /// concept ambiguous once the user types. See dialog.rs module docs.
    Options {
        options: Vec<String>,
        selected: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub kind: FieldKind,
}

impl Field {
    fn new(param: &Param) -> Self {
        match parse_options(&param.default) {
            Some(options) => Field {
                name: param.name.clone(),
                kind: FieldKind::Options {
                    options,
                    selected: 0,
                },
            },
            None => {
                let buffer = param.default.clone();
                let cursor = buffer.chars().count();
                Field {
                    name: param.name.clone(),
                    kind: FieldKind::Text { buffer, cursor },
                }
            }
        }
    }

    pub fn current_value(&self) -> String {
        match &self.kind {
            FieldKind::Text { buffer, .. } => buffer.clone(),
            FieldKind::Options { options, selected } => options[*selected].clone(),
        }
    }
}

/// State for the single-screen parameter resolution form (the Rust port's
/// `ratatui` replacement for Go pet's termbox `dialog` TUI): one field per unique
/// param, Tab/Shift-Tab moves focus, Enter confirms from any field (matching Go,
/// which lets Enter finalize regardless of which view is focused), Esc/Ctrl-C
/// cancels. Kept free of any terminal I/O so it's unit-testable — see
/// `resolve_params` for the actual event loop and rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogState {
    pub fields: Vec<Field>,
    pub focus: usize,
}

impl DialogState {
    pub fn new(params: &[Param]) -> Self {
        DialogState {
            fields: params.iter().map(Field::new).collect(),
            focus: 0,
        }
    }

    pub fn values(&self) -> HashMap<String, String> {
        self.fields
            .iter()
            .map(|f| (f.name.clone(), f.current_value()))
            .collect()
    }
}

pub enum DialogStep {
    Continue(DialogState),
    Done(HashMap<String, String>),
    Cancelled,
}

/// Pure key-event transition. `key` takes crossterm's `KeyCode`/`KeyModifiers`
/// directly (not the whole `KeyEvent`) so tests don't need a real terminal or even
/// the `crossterm` dependency's event-reading machinery — just these two enums.
pub fn handle_key(
    mut state: DialogState,
    code: crossterm::event::KeyCode,
    modifiers: crossterm::event::KeyModifiers,
) -> DialogStep {
    use crossterm::event::{KeyCode, KeyModifiers};

    match code {
        KeyCode::Esc => return DialogStep::Cancelled,
        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
            return DialogStep::Cancelled;
        }
        KeyCode::Enter => return DialogStep::Done(state.values()),
        KeyCode::Tab => {
            if !state.fields.is_empty() {
                state.focus = (state.focus + 1) % state.fields.len();
            }
        }
        KeyCode::BackTab => {
            if !state.fields.is_empty() {
                state.focus = (state.focus + state.fields.len() - 1) % state.fields.len();
            }
        }
        _ => {
            if let Some(field) = state.fields.get_mut(state.focus) {
                apply_key_to_field(field, code);
            }
        }
    }
    DialogStep::Continue(state)
}

fn apply_key_to_field(field: &mut Field, code: crossterm::event::KeyCode) {
    use crossterm::event::KeyCode;

    match &mut field.kind {
        FieldKind::Options { options, selected } => match code {
            KeyCode::Up | KeyCode::Left => {
                *selected = if *selected == 0 {
                    options.len() - 1
                } else {
                    *selected - 1
                };
            }
            KeyCode::Down | KeyCode::Right => {
                *selected = (*selected + 1) % options.len();
            }
            _ => {}
        },
        FieldKind::Text { buffer, cursor } => match code {
            KeyCode::Left => {
                if *cursor > 0 {
                    *cursor -= 1;
                }
            }
            KeyCode::Right => {
                if *cursor < buffer.chars().count() {
                    *cursor += 1;
                }
            }
            KeyCode::Backspace => {
                if *cursor > 0 {
                    let start = char_byte_index(buffer, *cursor - 1);
                    let end = char_byte_index(buffer, *cursor);
                    buffer.replace_range(start..end, "");
                    *cursor -= 1;
                }
            }
            KeyCode::Delete => {
                if *cursor < buffer.chars().count() {
                    let start = char_byte_index(buffer, *cursor);
                    let end = char_byte_index(buffer, *cursor + 1);
                    buffer.replace_range(start..end, "");
                }
            }
            KeyCode::Char(c) => {
                let at = char_byte_index(buffer, *cursor);
                buffer.insert(at, c);
                *cursor += 1;
            }
            _ => {}
        },
    }
}

fn char_byte_index(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map_or(s.len(), |(byte_idx, _)| byte_idx)
}

/// Interactively resolve `params` found in `command`, returning the chosen values
/// (`Ok(Some(_))`), `Ok(None)` if the user cancelled (Esc/Ctrl-C — treated the same
/// as a cancelled selector pick: the caller should print/run/copy nothing, not
/// treat it as a hard error), or `Err` if the terminal itself couldn't be driven.
///
/// This is the Rust port's `ratatui`+`crossterm` replacement for Go pet's termbox
/// `dialog.GenerateParamsLayout`: a single screen with a live command preview
/// (substituted as fields are edited, unlike Go's static preview) and one box per
/// unique param. `ratatui::try_init` installs a panic hook that restores the
/// terminal before any panic elsewhere in the process propagates, so a crash mid-
/// dialog doesn't leave the user's terminal in raw mode.
pub fn resolve_params(
    params: &[Param],
    command: &str,
) -> anyhow::Result<Option<HashMap<String, String>>> {
    use anyhow::Context;

    if params.is_empty() {
        return Ok(Some(HashMap::new()));
    }

    let mut terminal =
        ratatui::try_init().context("failed to initialize terminal for parameter dialog")?;
    let outcome = run_dialog_loop(&mut terminal, params, command);
    ratatui::restore();
    outcome
}

fn run_dialog_loop(
    terminal: &mut ratatui::DefaultTerminal,
    params: &[Param],
    command: &str,
) -> anyhow::Result<Option<HashMap<String, String>>> {
    use anyhow::Context;
    use crossterm::event::{self, Event, KeyEventKind};

    let mut state = DialogState::new(params);

    loop {
        terminal
            .draw(|frame| render(frame, &state, command))
            .context("failed to draw parameter dialog")?;

        let Event::Key(key) = event::read().context("failed to read terminal event")? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        match handle_key(state, key.code, key.modifiers) {
            DialogStep::Continue(next) => state = next,
            DialogStep::Done(values) => return Ok(Some(values)),
            DialogStep::Cancelled => return Ok(None),
        }
    }
}

fn render(frame: &mut ratatui::Frame, state: &DialogState, command: &str) {
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::style::{Modifier, Style};
    use ratatui::widgets::{Block, Borders, Paragraph};

    let preview = substitute(command, &state.values());

    let mut constraints = vec![Constraint::Length(3)];
    constraints.extend(std::iter::repeat_n(
        Constraint::Length(3),
        state.fields.len(),
    ));
    constraints.push(Constraint::Min(0));

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(frame.area());

    let preview_widget = Paragraph::new(preview).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Command  (Tab/Shift-Tab: switch field · Enter: run · Esc: cancel)"),
    );
    frame.render_widget(preview_widget, chunks[0]);

    for (i, field) in state.fields.iter().enumerate() {
        let focused = i == state.focus;
        let style = if focused {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };

        let content = match &field.kind {
            FieldKind::Text { buffer, .. } => buffer.clone(),
            FieldKind::Options {
                options, selected, ..
            } => format!(
                "{}   (↑/↓ {}/{})",
                options[*selected],
                selected + 1,
                options.len()
            ),
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(field.name.clone());
        frame.render_widget(
            Paragraph::new(content).style(style).block(block),
            chunks[i + 1],
        );

        if focused && let FieldKind::Text { cursor, .. } = &field.kind {
            let rect = chunks[i + 1];
            frame.set_cursor_position((rect.x + 1 + *cursor as u16, rect.y + 1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(pairs: &[(&str, &str)]) -> Vec<Param> {
        pairs
            .iter()
            .map(|(name, default)| Param {
                name: name.to_string(),
                default: default.to_string(),
            })
            .collect()
    }

    // Ported from Go pet's dialog/params_test.go (TestSearchForParams*) to keep the
    // placeholder grammar byte-for-byte compatible with existing pet snippets.

    #[test]
    fn basic_params() {
        assert_eq!(
            extract_params("<a=1> <b> hello"),
            params(&[("a", "1"), ("b", "")])
        );
    }

    #[test]
    fn no_params() {
        assert!(extract_params("no params").is_empty());
    }

    #[test]
    fn multiple_params() {
        assert_eq!(
            extract_params("<a=1> <b> <c=3>"),
            params(&[("a", "1"), ("b", ""), ("c", "3")])
        );
    }

    #[test]
    fn empty_command() {
        assert!(extract_params("").is_empty());
    }

    #[test]
    fn with_newline() {
        assert_eq!(
            extract_params("<a=1> <b> hello\n<c=3>"),
            params(&[("a", "1"), ("b", ""), ("c", "3")])
        );
    }

    #[test]
    fn value_with_spaces() {
        assert_eq!(
            extract_params("example_function --flag=<param=Lots of Bananas>"),
            params(&[("param", "Lots of Bananas")])
        );
    }

    #[test]
    fn invalid_param_format() {
        assert_eq!(extract_params("<a=1 <b> hello"), params(&[("b", "")]));
    }

    #[test]
    fn invalid_param_format_without_spaces() {
        assert_eq!(extract_params("<a=1<b>hello"), params(&[("b", "")]));
    }

    #[test]
    fn confusing_brackets() {
        assert_eq!(
            extract_params("cat <<EOF > <file=path/to/file>\nEOF"),
            params(&[("file", "path/to/file")])
        );
    }

    #[test]
    fn multiple_params_same_key() {
        assert_eq!(extract_params("<a=1> <a=2> <a=3>"), params(&[("a", "3")]));
    }

    #[test]
    fn multiple_params_same_key_multiple_lines() {
        assert_eq!(
            extract_params("<a=1> <a=2> <a=3>\n<b=4>"),
            params(&[("a", "3"), ("b", "4")])
        );
    }

    #[test]
    fn multiple_params_same_key_invalid_format() {
        assert_eq!(extract_params("<a=1> <a=2 <a=3>"), params(&[("a", "3")]));
    }

    #[test]
    fn multiple_params_same_key_invalid_format_multiple_lines() {
        assert_eq!(
            extract_params("<a=1> <a=2> <a=3 \n<b=4>"),
            params(&[("a", "2"), ("b", "4")])
        );
    }

    #[test]
    fn multiple_params_same_key_invalid_format_multiple_lines2() {
        assert_eq!(
            extract_params("<a=1> <a=2> <a=3>\n<b=4"),
            params(&[("a", "3")])
        );
    }

    #[test]
    fn equals_in_default_value_ignored() {
        assert_eq!(
            extract_params("echo \"<param=Hello == World!===>\""),
            params(&[("param", "Hello == World!===")])
        );
    }

    #[test]
    fn multiple_default_values_do_not_break_extraction() {
        assert_eq!(
            extract_params(
                "echo \"<param=|_Hello_||_Hello world_||_How are you?_|> <second=Hello>, <third>\""
            ),
            params(&[
                ("param", "|_Hello_||_Hello world_||_How are you?_|"),
                ("second", "Hello"),
                ("third", ""),
            ])
        );
    }

    // Ported from Go pet's TestInsertParams*.

    #[test]
    fn substitute_repeated_and_distinct_names() {
        let mut values = HashMap::new();
        values.insert("a".to_string(), "test".to_string());
        values.insert("b".to_string(), "case".to_string());
        assert_eq!(
            substitute("<a=1> <a> <b> hello", &values),
            "test test case hello"
        );
    }

    #[test]
    fn substitute_unique_parameters() {
        let mut values = HashMap::new();
        values.insert("host".to_string(), "localhost:9200".to_string());
        values.insert("index".to_string(), "test".to_string());
        assert_eq!(
            substitute(
                "curl -X POST \"<host=http://localhost:9200>/<index>\" -H 'Content-Type: application/json'",
                &values
            ),
            "curl -X POST \"localhost:9200/test\" -H 'Content-Type: application/json'"
        );
    }

    #[test]
    fn substitute_complex_repeated_name() {
        let mut values = HashMap::new();
        values.insert("host".to_string(), "localhost:9200".to_string());
        values.insert("test".to_string(), "case".to_string());
        assert_eq!(
            substitute(
                "something <host=http://localhost:9200>/<test>/_delete_by_query/<host>",
                &values
            ),
            "something localhost:9200/case/_delete_by_query/localhost:9200"
        );
    }

    #[test]
    fn substitute_equals_in_default_value_ignored() {
        let mut values = HashMap::new();
        values.insert("param".to_string(), "something == something".to_string());
        assert_eq!(
            substitute("echo \"<param=Hello == World!===>\"", &values),
            "echo \"something == something\""
        );
    }

    #[test]
    fn substitute_missing_value_becomes_empty() {
        let values = HashMap::new();
        assert_eq!(substitute("echo <missing>", &values), "echo ");
    }

    #[test]
    fn parse_options_none_for_plain_default() {
        assert_eq!(parse_options("world"), None);
        assert_eq!(parse_options(""), None);
    }

    #[test]
    fn parse_options_splits_pipe_delimited_segments() {
        assert_eq!(
            parse_options("|_John_||_Sam_||_Jane Doe = special #chars_|"),
            Some(vec![
                "John".to_string(),
                "Sam".to_string(),
                "Jane Doe = special #chars".to_string(),
            ])
        );
    }

    // DialogState / handle_key: the interactive resolution form's pure transition
    // logic, tested without any terminal.

    use crossterm::event::{KeyCode, KeyModifiers};

    fn done_values(step: DialogStep) -> HashMap<String, String> {
        match step {
            DialogStep::Done(values) => values,
            _ => panic!("expected Done"),
        }
    }

    fn continuing(step: DialogStep) -> DialogState {
        match step {
            DialogStep::Continue(state) => state,
            _ => panic!("expected Continue"),
        }
    }

    #[test]
    fn new_dialog_state_seeds_text_fields_with_defaults_cursor_at_end() {
        let state = DialogState::new(&params(&[("name", "world")]));
        assert_eq!(
            state.fields[0].kind,
            FieldKind::Text {
                buffer: "world".to_string(),
                cursor: 5,
            }
        );
    }

    #[test]
    fn new_dialog_state_detects_multi_default_fields() {
        let state = DialogState::new(&params(&[("color", "|_red_||_blue_|")]));
        assert_eq!(
            state.fields[0].kind,
            FieldKind::Options {
                options: vec!["red".to_string(), "blue".to_string()],
                selected: 0,
            }
        );
    }

    #[test]
    fn enter_confirms_from_any_focused_field_with_current_values() {
        let state = DialogState::new(&params(&[("a", "1"), ("b", "2")]));
        // Focus is on the first field, but Enter should still confirm everything —
        // matches Go pet, where Enter finalizes regardless of which view is active.
        let values = done_values(handle_key(state, KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(values.get("a").map(String::as_str), Some("1"));
        assert_eq!(values.get("b").map(String::as_str), Some("2"));
    }

    #[test]
    fn esc_cancels() {
        let state = DialogState::new(&params(&[("a", "1")]));
        assert!(matches!(
            handle_key(state, KeyCode::Esc, KeyModifiers::NONE),
            DialogStep::Cancelled
        ));
    }

    #[test]
    fn ctrl_c_cancels() {
        let state = DialogState::new(&params(&[("a", "1")]));
        assert!(matches!(
            handle_key(state, KeyCode::Char('c'), KeyModifiers::CONTROL),
            DialogStep::Cancelled
        ));
    }

    #[test]
    fn plain_c_does_not_cancel() {
        let state = DialogState::new(&params(&[("a", "")]));
        let state = continuing(handle_key(state, KeyCode::Char('c'), KeyModifiers::NONE));
        assert_eq!(
            state.fields[0].kind,
            FieldKind::Text {
                buffer: "c".to_string(),
                cursor: 1,
            }
        );
    }

    #[test]
    fn tab_wraps_focus_forward() {
        let state = DialogState::new(&params(&[("a", ""), ("b", "")]));
        let state = continuing(handle_key(state, KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(state.focus, 1);
        let state = continuing(handle_key(state, KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(state.focus, 0);
    }

    #[test]
    fn shift_tab_wraps_focus_backward() {
        let state = DialogState::new(&params(&[("a", ""), ("b", "")]));
        assert_eq!(state.focus, 0);
        let state = continuing(handle_key(state, KeyCode::BackTab, KeyModifiers::NONE));
        assert_eq!(state.focus, 1);
    }

    #[test]
    fn typing_inserts_at_cursor() {
        let state = DialogState::new(&params(&[("name", "")]));
        let state = continuing(handle_key(state, KeyCode::Char('h'), KeyModifiers::NONE));
        let state = continuing(handle_key(state, KeyCode::Char('i'), KeyModifiers::NONE));
        assert_eq!(
            state.fields[0].kind,
            FieldKind::Text {
                buffer: "hi".to_string(),
                cursor: 2,
            }
        );
    }

    #[test]
    fn left_arrow_moves_cursor_and_insert_happens_there() {
        let state = DialogState::new(&params(&[("name", "ac")]));
        let state = continuing(handle_key(state, KeyCode::Left, KeyModifiers::NONE));
        let state = continuing(handle_key(state, KeyCode::Char('b'), KeyModifiers::NONE));
        assert_eq!(
            state.fields[0].kind,
            FieldKind::Text {
                buffer: "abc".to_string(),
                cursor: 2,
            }
        );
    }

    #[test]
    fn backspace_deletes_before_cursor() {
        let state = DialogState::new(&params(&[("name", "abc")]));
        let state = continuing(handle_key(state, KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(
            state.fields[0].kind,
            FieldKind::Text {
                buffer: "ab".to_string(),
                cursor: 2,
            }
        );
    }

    #[test]
    fn backspace_at_start_of_buffer_is_a_noop() {
        let state = DialogState::new(&params(&[("name", "abc")]));
        let state = continuing(handle_key(state, KeyCode::Left, KeyModifiers::NONE));
        let state = continuing(handle_key(state, KeyCode::Left, KeyModifiers::NONE));
        let state = continuing(handle_key(state, KeyCode::Left, KeyModifiers::NONE));
        let state = continuing(handle_key(state, KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(
            state.fields[0].kind,
            FieldKind::Text {
                buffer: "abc".to_string(),
                cursor: 0,
            }
        );
    }

    #[test]
    fn delete_removes_char_at_cursor() {
        let state = DialogState::new(&params(&[("name", "abc")]));
        let state = continuing(handle_key(state, KeyCode::Left, KeyModifiers::NONE));
        let state = continuing(handle_key(state, KeyCode::Left, KeyModifiers::NONE));
        let state = continuing(handle_key(state, KeyCode::Delete, KeyModifiers::NONE));
        assert_eq!(
            state.fields[0].kind,
            FieldKind::Text {
                buffer: "ac".to_string(),
                cursor: 1,
            }
        );
    }

    #[test]
    fn options_field_up_down_cycles_with_wraparound() {
        let state = DialogState::new(&params(&[("color", "|_red_||_green_||_blue_|")]));
        let state = continuing(handle_key(state, KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(state.fields[0].current_value(), "green");
        let state = continuing(handle_key(state, KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(state.fields[0].current_value(), "blue");
        let state = continuing(handle_key(state, KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(state.fields[0].current_value(), "red");
        let state = continuing(handle_key(state, KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(state.fields[0].current_value(), "blue");
    }

    #[test]
    fn options_field_ignores_typed_characters() {
        let state = DialogState::new(&params(&[("color", "|_red_||_blue_|")]));
        let state = continuing(handle_key(state, KeyCode::Char('x'), KeyModifiers::NONE));
        assert_eq!(state.fields[0].current_value(), "red");
    }

    #[test]
    fn values_snapshot_uses_current_field_state_not_original_defaults() {
        let state = DialogState::new(&params(&[("greeting", "hi"), ("color", "|_red_||_blue_|")]));
        let state = continuing(handle_key(state, KeyCode::Char('!'), KeyModifiers::NONE));
        let state = continuing(handle_key(state, KeyCode::Tab, KeyModifiers::NONE));
        let state = continuing(handle_key(state, KeyCode::Down, KeyModifiers::NONE));
        let values = done_values(handle_key(state, KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(values.get("greeting").map(String::as_str), Some("hi!"));
        assert_eq!(values.get("color").map(String::as_str), Some("blue"));
    }
}
