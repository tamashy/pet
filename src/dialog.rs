use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::LazyLock;

use regex::Regex;

/// Matches `<name=default>` where `default` may itself embed complete `<ref>`
/// reference tokens (see `REF_RE`), or falls back to the original flat
/// `<...>` shape ported verbatim from Go pet's `dialog.parameterStringRegex` (see
/// params.go) when it doesn't. The flat alternative is deliberately permissive
/// about what its captured content can contain (including further `<`/`>` as the
/// very last character) to match Go's exact matching behavior around
/// nested/broken brackets; see the `extract_params` tests ported from Go's
/// params_test.go.
///
/// The two alternatives are tried in this order at every position (the `regex`
/// crate is leftmost-first among alternatives, like a backtracking engine), so the
/// nested/embedded-reference shape only ever "wins" where it actually applies —
/// everywhere else this degenerates to exactly the old flat match. Capture groups:
/// 1 = nested name, 2 = nested raw default (verbatim, embedded `<ref>` substrings
/// intact); 3 = the original flat capture, unchanged, still split on `=` by the
/// caller exactly as before.
static COMBINED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<([^<>=\s]+)=((?:[^<>]|<[^<>=\s]+>)*(?:[^\s<>]|<[^<>=\s]+>))>|<([^<>]*[^\s])>")
        .unwrap()
});

/// Matches a bare `<name>` reference embedded in another parameter's default, e.g.
/// the `<env>` in `<aws-profile=<env>-prod>`. Deliberately one level only — a
/// reference is just a name (no `=`, whitespace, or brackets inside), never a
/// nested `<name=default>` definition.
static REF_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<([^<>=\s]+)>").unwrap());

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
///
/// A default may itself embed `<ref>` tokens (see `REF_RE`) naming other params —
/// see the module docs on `COMBINED_RE`. Any referenced name that never appears as
/// its own top-level `<name>` / `<name=default>` elsewhere in `command` is
/// synthesized as an implicit empty-default param, inserted immediately before the
/// first param that references it, so it always becomes a real, visible, editable
/// field rather than silently resolving to an empty string (a plain typo is easier
/// to spot as an empty field titled with the typo'd name than as a blank
/// substitution). Already-declared params keep their natural source order even if
/// that puts a dependent before its dependency — see `dependency_order`, which is
/// what actually makes resolution order-independent.
pub fn extract_params(command: &str) -> Vec<Param> {
    let mut order: Vec<String> = Vec::new();
    let mut defaults: HashMap<String, String> = HashMap::new();
    let mut seen: HashSet<String> = HashSet::new();

    for caps in COMBINED_RE.captures_iter(command) {
        let (name, default) = if let Some(nested_name) = caps.get(1) {
            (nested_name.as_str().to_string(), Some(caps[2].to_string()))
        } else {
            let matched = &caps[3];
            match matched.split_once('=') {
                Some((name, default)) => (name.to_string(), Some(default.to_string())),
                None => (matched.to_string(), None),
            }
        };

        if seen.insert(name.clone()) {
            order.push(name.clone());
            defaults.insert(name, default.unwrap_or_default());
        } else if let Some(default) = default {
            defaults.insert(name, default);
        }
    }

    let declared = seen;
    let mut synthesized: HashSet<String> = HashSet::new();
    let mut params = Vec::with_capacity(order.len());
    for name in order {
        let default = defaults.remove(&name).unwrap_or_default();
        for reference in param_refs(&default) {
            if !declared.contains(&reference) && synthesized.insert(reference.clone()) {
                params.push(Param {
                    name: reference,
                    default: String::new(),
                });
            }
        }
        params.push(Param { name, default });
    }
    params
}

/// Extract the `<name>` references embedded in `default` (see `REF_RE`), deduped,
/// in first-seen order.
fn param_refs(default: &str) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut refs = Vec::new();
    for caps in REF_RE.captures_iter(default) {
        let name = caps[1].to_string();
        if seen.insert(name.clone()) {
            refs.push(name);
        }
    }
    refs
}

/// Replace every `<name>` / `<name=default>` occurrence in `command` with its
/// resolved value from `values` (missing entries substitute as empty, matching Go's
/// zero-value map access in `dialog.insertParams`). Uses the same `COMBINED_RE` as
/// `extract_params` so a nested token like `<aws-profile=<env>-prod>` collapses as
/// one unit to `values["aws-profile"]`, rather than only its embedded `<env>`
/// reference getting replaced and leaving the outer token's literal text behind.
pub fn substitute(command: &str, values: &HashMap<String, String>) -> String {
    COMBINED_RE
        .replace_all(command, |caps: &regex::Captures| {
            let name = if let Some(nested_name) = caps.get(1) {
                nested_name.as_str()
            } else {
                let matched = &caps[3];
                matched.split_once('=').map_or(matched, |(name, _)| name)
            };
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

/// A field whose default embeds `<ref>` tokens (see `REF_RE`) naming other fields.
/// While `overridden` is false, the field's displayed/resolved value is
/// `raw` re-interpolated against the referenced fields' *current* values on every
/// render/resolve — see `DialogState::values` — so it tracks them live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldTemplate {
    /// The raw default text, verbatim, with embedded `<ref>` tokens intact.
    raw: String,
    /// Names this field's `raw` references, deduped, first-seen order.
    depends_on: Vec<String>,
    /// Set once the user edits this field directly (any of `Char`/`Backspace`/
    /// `Delete`) — from then on it behaves exactly like a plain `Text` field and
    /// permanently stops tracking its references. See `handle_key`.
    overridden: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub kind: FieldKind,
    /// Whether the source `<name=default>` had a non-empty default. Used only to
    /// decide whether to flag the field as needing input while its buffer is still
    /// empty — Go pet never enforces this (an empty value substitutes as ""), so
    /// this is advisory styling only, not validation.
    has_default: bool,
    /// `Some` only for a `FieldKind::Text` field whose default embedded a
    /// reference to another field (mutually exclusive with `FieldKind::Options` —
    /// `parse_options` is tried first, unchanged precedence) and that isn't
    /// currently part of a dependency cycle (see `dependency_order`, which strips
    /// this back to `None` for cycle-involved fields).
    template: Option<FieldTemplate>,
}

impl Field {
    fn new(param: &Param) -> Self {
        let has_default = !param.default.is_empty();
        match parse_options(&param.default) {
            Some(options) => Field {
                name: param.name.clone(),
                kind: FieldKind::Options {
                    options,
                    selected: 0,
                },
                has_default,
                template: None,
            },
            None => {
                let depends_on = param_refs(&param.default);
                if depends_on.is_empty() {
                    let buffer = param.default.clone();
                    let cursor = buffer.chars().count();
                    return Field {
                        name: param.name.clone(),
                        kind: FieldKind::Text { buffer, cursor },
                        has_default,
                        template: None,
                    };
                }
                Field {
                    name: param.name.clone(),
                    kind: FieldKind::Text {
                        buffer: String::new(),
                        cursor: 0,
                    },
                    has_default,
                    template: Some(FieldTemplate {
                        raw: param.default.clone(),
                        depends_on,
                        overridden: false,
                    }),
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

    /// A text field with no default, still empty — the one case worth flagging
    /// visually since there's no value to fall back to (Options fields always
    /// have a selected value, so they're never "empty").
    fn needs_input(&self) -> bool {
        match &self.kind {
            FieldKind::Text { buffer, .. } => !self.has_default && buffer.is_empty(),
            FieldKind::Options { .. } => false,
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
    /// Field indices in dependency order (a templated field's referenced fields
    /// always precede it), computed once in `new` — see `dependency_order`. Used by
    /// `values` to resolve templated fields against already-resolved values.
    recompute_order: Vec<usize>,
}

impl DialogState {
    pub fn new(params: &[Param]) -> Self {
        let mut fields: Vec<Field> = params.iter().map(Field::new).collect();
        let recompute_order = dependency_order(&mut fields);
        DialogState {
            fields,
            focus: 0,
            recompute_order,
        }
    }

    /// Resolve every field's current value. A non-overridden templated field's
    /// value is its raw default re-interpolated against the other fields'
    /// (already-resolved, per `recompute_order`) current values — so this reflects
    /// live edits/cycling of the fields it depends on, not just its original
    /// default. Everything else is `Field::current_value` as before.
    pub fn values(&self) -> HashMap<String, String> {
        let mut resolved: HashMap<String, String> = HashMap::with_capacity(self.fields.len());
        for &i in &self.recompute_order {
            let field = &self.fields[i];
            let value = match &field.template {
                Some(t) if !t.overridden => interpolate(&t.raw, &resolved),
                _ => field.current_value(),
            };
            resolved.insert(field.name.clone(), value);
        }
        resolved
    }
}

/// Replace every embedded `<ref>` token in `raw` (see `REF_RE`) with its value from
/// `values`, defaulting a missing reference to "" — mirrors `substitute`'s own
/// "missing entries substitute as empty" convention.
fn interpolate(raw: &str, values: &HashMap<String, String>) -> String {
    REF_RE
        .replace_all(raw, |caps: &regex::Captures| {
            values.get(&caps[1]).cloned().unwrap_or_default()
        })
        .into_owned()
}

/// Topologically order `fields` by their `template.depends_on` edges (Kahn's
/// algorithm: a referenced field must resolve before the field that depends on
/// it). Fields whose indegree never reaches 0 are part of a dependency cycle —
/// fail open rather than error: their `template` is stripped (falling back to a
/// plain independent `Text` field pre-filled with the raw, un-interpolated literal
/// text) so one broken pair of fields never blocks the rest of the dialog from
/// resolving. A reference to a name with no matching field (only reachable when
/// `DialogState::new` is called directly, bypassing `extract_params`'s synthesis
/// of implicit fields for undeclared references) is simply not turned into an
/// edge — it interpolates to "" via `interpolate`'s missing-key default.
///
/// Returns a permutation of `0..fields.len()` — every field appears exactly once,
/// dependencies before dependents.
fn dependency_order(fields: &mut [Field]) -> Vec<usize> {
    let index_of: HashMap<&str, usize> = fields
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.as_str(), i))
        .collect();

    let mut indegree = vec![0usize; fields.len()];
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); fields.len()];
    for (i, field) in fields.iter().enumerate() {
        if let Some(t) = &field.template {
            for dep in &t.depends_on {
                if let Some(&j) = index_of.get(dep.as_str()) {
                    dependents[j].push(i);
                    indegree[i] += 1;
                }
            }
        }
    }

    let mut order = Vec::with_capacity(fields.len());
    let mut queue: VecDeque<usize> = (0..fields.len()).filter(|&i| indegree[i] == 0).collect();
    while let Some(u) = queue.pop_front() {
        order.push(u);
        for &v in &dependents[u] {
            indegree[v] -= 1;
            if indegree[v] == 0 {
                queue.push_back(v);
            }
        }
    }

    if order.len() < fields.len() {
        let resolved: HashSet<usize> = order.iter().copied().collect();
        for (i, field) in fields.iter_mut().enumerate() {
            if resolved.contains(&i) {
                continue;
            }
            if let Some(template) = field.template.take() {
                let buffer = template.raw;
                let cursor = buffer.chars().count();
                field.kind = FieldKind::Text { buffer, cursor };
            }
            order.push(i);
        }
    }

    order
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
            if is_editing_key(code) {
                materialize_if_templated(&mut state);
            }
            if let Some(field) = state.fields.get_mut(state.focus) {
                apply_key_to_field(field, code);
            }
        }
    }
    DialogStep::Continue(state)
}

/// Whether `code` edits a field's text (as opposed to just moving a cursor or
/// cycling an `Options` selection) — the trigger for `materialize_if_templated`.
fn is_editing_key(code: crossterm::event::KeyCode) -> bool {
    use crossterm::event::KeyCode;

    matches!(
        code,
        KeyCode::Char(_) | KeyCode::Backspace | KeyCode::Delete
    )
}

/// If the focused field is a not-yet-overridden templated field, freeze its
/// current live-interpolated value into `buffer` and mark it `overridden` — from
/// this point on it's indistinguishable from a plain `Text` field and permanently
/// stops tracking the fields it used to reference. Called once, right before an
/// editing key is applied, so the field the user is about to type into always
/// starts from what they can currently see rather than snapping back to the raw
/// template text.
fn materialize_if_templated(state: &mut DialogState) {
    let Some(field) = state.fields.get(state.focus) else {
        return;
    };
    if !matches!(&field.template, Some(t) if !t.overridden) {
        return;
    }

    let values = state.values();
    let field = &mut state.fields[state.focus];
    let current = values.get(&field.name).cloned().unwrap_or_default();
    if let FieldKind::Text { buffer, cursor } = &mut field.kind {
        *cursor = current.chars().count();
        *buffer = current;
    }
    if let Some(template) = &mut field.template {
        template.overridden = true;
    }
}

fn apply_key_to_field(field: &mut Field, code: crossterm::event::KeyCode) {
    use crossterm::event::KeyCode;

    use crate::tui::char_byte_index;

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

const FIELD_HEIGHT: u16 = 3;
const PREVIEW_HEIGHT: u16 = 3;
const FOOTER_HEIGHT: u16 = 1;
const SCROLL_HINT_HEIGHT: u16 = 1;

fn render(frame: &mut ratatui::Frame, state: &DialogState, command: &str) {
    use crate::tui::visible_window;
    use ratatui::layout::{Alignment, Constraint, Direction, Layout};
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Paragraph};

    let values = state.values();
    let preview = substitute(command, &values);
    let area = frame.area();

    // Two passes: first assume no scroll-hint lines are needed. If that lets every
    // field fit, great — no hints will be drawn, so that capacity is correct. If
    // not, scrolling kicks in and up to two one-line hints ("N more above/below")
    // eat into the space actually available for fields; recompute reserving room
    // for both so the fixed-height Preview/field boxes below never get squeezed by
    // the layout solver running short of rows.
    let fields_len = state.fields.len();
    let unhinted_capacity = area
        .height
        .saturating_sub(PREVIEW_HEIGHT + FOOTER_HEIGHT)
        .checked_div(FIELD_HEIGHT)
        .unwrap_or(0) as usize;
    let capacity = if fields_len <= unhinted_capacity {
        unhinted_capacity.max(1)
    } else {
        area.height
            .saturating_sub(PREVIEW_HEIGHT + FOOTER_HEIGHT + 2 * SCROLL_HINT_HEIGHT)
            .checked_div(FIELD_HEIGHT)
            .unwrap_or(0)
            .max(1) as usize
    };
    let (start, end) = visible_window(fields_len, state.focus, capacity);
    let more_above = start > 0;
    let more_below = end < state.fields.len();

    let mut constraints = vec![Constraint::Length(PREVIEW_HEIGHT)];
    if more_above {
        constraints.push(Constraint::Length(SCROLL_HINT_HEIGHT));
    }
    constraints.extend(std::iter::repeat_n(
        Constraint::Length(FIELD_HEIGHT),
        end - start,
    ));
    if more_below {
        constraints.push(Constraint::Length(SCROLL_HINT_HEIGHT));
    }
    constraints.push(Constraint::Min(0));
    constraints.push(Constraint::Length(FOOTER_HEIGHT));

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let preview_widget = Paragraph::new(preview)
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(Span::styled(
                    " Preview ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )),
        );
    frame.render_widget(preview_widget, chunks[0]);

    let mut row = 1;
    if more_above {
        frame.render_widget(
            Paragraph::new(format!("▲ {start} more above"))
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center),
            chunks[row],
        );
        row += 1;
    }

    for (offset, i) in (start..end).enumerate() {
        let field = &state.fields[i];
        let focused = i == state.focus;
        let needs_input = field.needs_input();
        let chunk = chunks[row + offset];

        let border_color = match (focused, needs_input) {
            (_, true) => Color::Red,
            (true, false) => Color::Yellow,
            (false, false) => Color::DarkGray,
        };
        let mut title_style = Style::default().fg(border_color);
        if focused {
            title_style = title_style.add_modifier(Modifier::BOLD);
        }
        let title = if needs_input {
            format!(" {} (required) ", field.name)
        } else {
            format!(" {} ", field.name)
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(Span::styled(title, title_style));

        // A not-yet-overridden templated field shows its live-interpolated value
        // (from the same `values` map used for the Preview line above) rather than
        // its own `buffer`, which stays empty until the user edits it directly —
        // see `FieldTemplate`/`materialize_if_templated`.
        let live_text = match &field.template {
            Some(t) if !t.overridden => values.get(&field.name).map(String::as_str),
            _ => None,
        };

        match &field.kind {
            FieldKind::Text { buffer, .. } => {
                let style = if focused {
                    Style::default().fg(Color::Black).bg(Color::Yellow)
                } else {
                    Style::default().fg(Color::White)
                };
                let text = live_text.unwrap_or(buffer.as_str());
                frame.render_widget(
                    Paragraph::new(text.to_string()).style(style).block(block),
                    chunk,
                );
            }
            FieldKind::Options { options, selected } => {
                let mut spans = Vec::with_capacity(options.len() * 2);
                for (opt_i, opt) in options.iter().enumerate() {
                    if opt_i > 0 {
                        spans.push(Span::raw("  "));
                    }
                    if opt_i == *selected {
                        spans.push(Span::styled(
                            format!(" {opt} "),
                            Style::default()
                                .fg(Color::Black)
                                .bg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        ));
                    } else {
                        spans.push(Span::styled(
                            opt.clone(),
                            Style::default().fg(Color::DarkGray),
                        ));
                    }
                }
                spans.push(Span::styled(
                    format!("   (←/→ {}/{})", selected + 1, options.len()),
                    Style::default().fg(Color::DarkGray),
                ));
                frame.render_widget(Paragraph::new(Line::from(spans)).block(block), chunk);
            }
        }

        if focused && let FieldKind::Text { cursor, .. } = &field.kind {
            let cursor_x = match live_text {
                Some(text) => text.chars().count(),
                None => *cursor,
            };
            frame.set_cursor_position((chunk.x + 1 + cursor_x as u16, chunk.y + 1));
        }
    }
    row += end - start;

    if more_below {
        frame.render_widget(
            Paragraph::new(format!("▼ {} more below", state.fields.len() - end))
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center),
            chunks[row],
        );
    }

    let footer = Line::from(vec![
        Span::styled(
            format!("Field {}/{}", state.focus + 1, state.fields.len()),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "   Tab/Shift-Tab move   ↑/↓ edit or cycle   Enter run   Esc cancel",
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(footer).alignment(Alignment::Center),
        chunks[chunks.len() - 1],
    );
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

    // Nested `<ref>` reference support — see `COMBINED_RE`/`REF_RE`.

    #[test]
    fn nested_reference_extracted_verbatim_in_default() {
        assert_eq!(
            extract_params("<env=|_dev_||_explo_|> <aws-profile=<env>-prod>"),
            params(&[("env", "|_dev_||_explo_|"), ("aws-profile", "<env>-prod")])
        );
    }

    #[test]
    fn nested_reference_to_undeclared_param_synthesizes_implicit_field() {
        assert_eq!(
            extract_params("<aws-profile=<env>-prod>"),
            params(&[("env", ""), ("aws-profile", "<env>-prod")])
        );
    }

    #[test]
    fn nested_reference_coexists_with_confusing_brackets() {
        // Same heredoc shape as `confusing_brackets`, plus an unrelated nested
        // param elsewhere in the command — neither quirk should interfere with
        // the other.
        assert_eq!(
            extract_params("cat <<EOF > <file=path/to/file>\nEOF\n<aws-profile=<env>-prod>"),
            params(&[
                ("file", "path/to/file"),
                ("env", ""),
                ("aws-profile", "<env>-prod"),
            ])
        );
    }

    #[test]
    fn param_refs_extracts_bare_names_only() {
        assert_eq!(param_refs("<env>-prod"), vec!["env".to_string()]);
        // `<b=1>` isn't a bare reference (it has `=`), so it's not picked up.
        assert!(param_refs("<b=1>-prod").is_empty());
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
    fn substitute_resolves_full_nested_token_not_just_inner_ref() {
        // A naive substitution over the old flat regex would only rewrite the
        // embedded `<env>` and leave the outer `aws-profile=...` token untouched.
        let mut values = HashMap::new();
        values.insert("aws-profile".to_string(), "dev-prod".to_string());
        assert_eq!(substitute("<aws-profile=<env>-prod>", &values), "dev-prod");
    }

    #[test]
    fn dependency_order_detects_simple_cycle() {
        let mut fields: Vec<Field> = params(&[("a", "<b>"), ("b", "<a>")])
            .iter()
            .map(Field::new)
            .collect();
        let order = dependency_order(&mut fields);

        assert_eq!(order.len(), 2);
        assert!(fields.iter().all(|f| f.template.is_none()));
        assert_eq!(
            fields[0].kind,
            FieldKind::Text {
                buffer: "<b>".to_string(),
                cursor: 3,
            }
        );
        assert_eq!(
            fields[1].kind,
            FieldKind::Text {
                buffer: "<a>".to_string(),
                cursor: 3,
            }
        );
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

    // Field::needs_input: advisory "empty and no default" styling used by render().

    #[test]
    fn text_field_with_no_default_needs_input_while_empty() {
        let state = DialogState::new(&params(&[("name", "")]));
        assert!(state.fields[0].needs_input());
    }

    #[test]
    fn text_field_with_default_never_needs_input() {
        let state = DialogState::new(&params(&[("name", "world")]));
        assert!(!state.fields[0].needs_input());
    }

    #[test]
    fn text_field_with_no_default_stops_needing_input_once_typed() {
        let state = DialogState::new(&params(&[("name", "")]));
        let state = continuing(handle_key(state, KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(!state.fields[0].needs_input());
    }

    #[test]
    fn text_field_with_no_default_needs_input_again_once_cleared() {
        let state = DialogState::new(&params(&[("name", "")]));
        let typed = continuing(handle_key(state, KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(!typed.fields[0].needs_input());
        let cleared = continuing(handle_key(typed, KeyCode::Backspace, KeyModifiers::NONE));
        assert!(cleared.fields[0].needs_input());
    }

    #[test]
    fn options_field_never_needs_input() {
        let state = DialogState::new(&params(&[("color", "|_red_||_blue_|")]));
        assert!(!state.fields[0].needs_input());
    }

    // Live-tracking templated fields (`<name=<ref>...>`) in the dialog.

    #[test]
    fn templated_field_initial_value_is_interpolated_before_any_key_event() {
        let state = DialogState::new(&params(&[("env", "dev"), ("aws-profile", "<env>-prod")]));
        assert_eq!(
            state.values().get("aws-profile").map(String::as_str),
            Some("dev-prod")
        );
    }

    #[test]
    fn templated_field_updates_live_as_text_dependency_changes() {
        let state = DialogState::new(&params(&[("env", "dev"), ("aws-profile", "<env>-prod")]));
        // Focus starts on `env`; appending "2" makes it "dev2".
        let state = continuing(handle_key(state, KeyCode::Char('2'), KeyModifiers::NONE));
        assert_eq!(state.values().get("env").map(String::as_str), Some("dev2"));
        assert_eq!(
            state.values().get("aws-profile").map(String::as_str),
            Some("dev2-prod")
        );
    }

    #[test]
    fn templated_field_updates_live_as_options_dependency_cycles() {
        let state = DialogState::new(&params(&[
            ("env", "|_dev_||_explo_|"),
            ("aws-profile", "<env>-prod"),
        ]));
        assert_eq!(
            state.values().get("aws-profile").map(String::as_str),
            Some("dev-prod")
        );

        // Focus starts on `env` (an Options field); Down cycles its selection with
        // no direct edit to `aws-profile` at all.
        let state = continuing(handle_key(state, KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(state.values().get("env").map(String::as_str), Some("explo"));
        assert_eq!(
            state.values().get("aws-profile").map(String::as_str),
            Some("explo-prod")
        );
    }

    #[test]
    fn templated_field_stops_tracking_once_user_edits_it_directly() {
        let mut state = DialogState::new(&params(&[("env", "dev"), ("aws-profile", "<env>-prod")]));
        state.focus = 1; // aws-profile

        // The first editing keystroke materializes "dev-prod" then appends "!".
        let state = continuing(handle_key(state, KeyCode::Char('!'), KeyModifiers::NONE));
        assert_eq!(
            state.values().get("aws-profile").map(String::as_str),
            Some("dev-prod!")
        );

        // Changing env no longer affects the now-detached aws-profile field.
        let mut state = state;
        state.focus = 0;
        let state = continuing(handle_key(state, KeyCode::Char('2'), KeyModifiers::NONE));
        assert_eq!(
            state.values().get("aws-profile").map(String::as_str),
            Some("dev-prod!")
        );
    }

    #[test]
    fn templated_field_missing_reference_becomes_its_own_empty_field() {
        let ps = extract_params("<aws-profile=<env>-prod>");
        let state = DialogState::new(&ps);
        assert_eq!(state.fields[0].name, "env");
        assert!(state.fields[0].needs_input());
        assert_eq!(
            state.values().get("aws-profile").map(String::as_str),
            Some("-prod")
        );
    }

    #[test]
    fn templated_field_with_cyclic_dependency_falls_back_to_literal_independent_text() {
        let state = DialogState::new(&params(&[("a", "<b>"), ("b", "<a>")]));
        assert!(state.fields.iter().all(|f| f.template.is_none()));
        assert_eq!(state.values().get("a").map(String::as_str), Some("<b>"));
        assert_eq!(state.values().get("b").map(String::as_str), Some("<a>"));
    }
}
