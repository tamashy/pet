//! A built-in fuzzy-finder, so `pet` has a selector out of the box instead of
//! requiring `fzf` (or another external tool) to be installed. Used when
//! `general.selectcmd` is the sentinel value `"builtin"` — see `selector.rs`.
//!
//! Same split as `dialog.rs`: pure, terminal-free state/transition logic
//! (`PickerState`, `handle_key`) that's fully unit-tested, plus a thin
//! `ratatui`+`crossterm` event loop (`pick`) that isn't.

use std::collections::HashSet;

use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;

use crate::tui::char_byte_index;

#[derive(Debug, Clone)]
struct Match {
    /// Index into `PickerState::items` — the original, unfiltered list.
    index: usize,
    /// Char indices within the matched item's text that the query matched,
    /// for highlighting. Empty (not "every position") when the query is empty.
    positions: Vec<usize>,
}

/// Score and locate every item in `items` that fuzzy-matches `query`, best
/// match first; every item, in original order, if `query` is empty (mirrors
/// fzf showing the full list before you start typing).
fn compute_matches(items: &[String], query: &str) -> Vec<Match> {
    if query.is_empty() {
        return (0..items.len())
            .map(|index| Match {
                index,
                positions: Vec::new(),
            })
            .collect();
    }

    let matcher = SkimMatcherV2::default();
    let mut scored: Vec<(i64, Match)> = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            matcher
                .fuzzy_indices(item, query)
                .map(|(score, positions)| (score, Match { index, positions }))
        })
        .collect();
    // Highest score first; break ties by original position so the ordering is
    // stable and predictable rather than however the sort happens to land.
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.index.cmp(&b.1.index)));
    scored.into_iter().map(|(_, m)| m).collect()
}

#[derive(Debug, Clone)]
pub(crate) struct PickerState {
    items: Vec<String>,
    query: String,
    /// Char index, not byte offset — matches `dialog::Field::Text`'s cursor.
    cursor: usize,
    matches: Vec<Match>,
    /// Index into `matches`, not `items`.
    focus: usize,
    /// Indices into `items`, toggled via Tab.
    selected: HashSet<usize>,
}

impl PickerState {
    pub(crate) fn new(items: Vec<String>, initial_query: &str) -> Self {
        let query = initial_query.to_string();
        let matches = compute_matches(&items, &query);
        let cursor = query.chars().count();
        PickerState {
            items,
            query,
            cursor,
            matches,
            focus: 0,
            selected: HashSet::new(),
        }
    }

    fn refilter(&mut self) {
        self.matches = compute_matches(&self.items, &self.query);
        self.focus = 0;
    }

    /// Moves focus by `delta` rows, wrapping at either end — matches the
    /// `--cycle` behavior already baked into the default external `selectcmd`.
    fn move_focus(&mut self, delta: i64) {
        if self.matches.is_empty() {
            return;
        }
        let len = self.matches.len() as i64;
        let next = (self.focus as i64 + delta).rem_euclid(len);
        self.focus = next as usize;
    }

    fn toggle_focused(&mut self) {
        if let Some(m) = self.matches.get(self.focus) {
            let idx = m.index;
            if !self.selected.remove(&idx) {
                self.selected.insert(idx);
            }
        }
    }

    /// What Enter should return: every toggled item if any are toggled,
    /// otherwise just whatever's currently focused (mirrors fzf: Enter with
    /// nothing explicitly multi-selected still picks the highlighted line).
    fn confirm(&self) -> Vec<usize> {
        if self.selected.is_empty() {
            self.matches
                .get(self.focus)
                .map(|m| vec![m.index])
                .unwrap_or_default()
        } else {
            let mut indices: Vec<usize> = self.selected.iter().copied().collect();
            indices.sort_unstable();
            indices
        }
    }
}

pub(crate) enum PickStep {
    Continue(PickerState),
    Done(Vec<usize>),
    Cancelled,
}

/// Pure key-event transition, mirroring `dialog::handle_key`.
pub(crate) fn handle_key(
    mut state: PickerState,
    code: crossterm::event::KeyCode,
    modifiers: crossterm::event::KeyModifiers,
) -> PickStep {
    use crossterm::event::{KeyCode, KeyModifiers};

    match code {
        KeyCode::Esc => return PickStep::Cancelled,
        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
            return PickStep::Cancelled;
        }
        KeyCode::Enter => return PickStep::Done(state.confirm()),
        KeyCode::Tab => {
            state.toggle_focused();
            state.move_focus(1);
        }
        KeyCode::Up => state.move_focus(-1),
        KeyCode::Down => state.move_focus(1),
        KeyCode::Left => {
            if state.cursor > 0 {
                state.cursor -= 1;
            }
        }
        KeyCode::Right => {
            if state.cursor < state.query.chars().count() {
                state.cursor += 1;
            }
        }
        KeyCode::Backspace => {
            if state.cursor > 0 {
                let start = char_byte_index(&state.query, state.cursor - 1);
                let end = char_byte_index(&state.query, state.cursor);
                state.query.replace_range(start..end, "");
                state.cursor -= 1;
                state.refilter();
            }
        }
        KeyCode::Delete => {
            if state.cursor < state.query.chars().count() {
                let start = char_byte_index(&state.query, state.cursor);
                let end = char_byte_index(&state.query, state.cursor + 1);
                state.query.replace_range(start..end, "");
                state.refilter();
            }
        }
        KeyCode::Char(c) => {
            let at = char_byte_index(&state.query, state.cursor);
            state.query.insert(at, c);
            state.cursor += 1;
            state.refilter();
        }
        _ => {}
    }
    PickStep::Continue(state)
}

/// Interactively pick from `items`, returning the indices of whatever got
/// selected (empty if the user cancelled with Esc/Ctrl-C, or if `items` was
/// empty to begin with — no terminal is opened in that case).
///
/// Draws to stderr, not stdout, and deliberately doesn't use `ratatui::try_init`
/// (which is hardcoded to stdout) — `pet search`/`exec`/`clip` are commonly used
/// inside `$(...)` command substitution (see the README's shell-integration
/// snippets), which redirects stdout but leaves the controlling terminal
/// otherwise intact. Drawing the picker there instead keeps that working,
/// matching how external selectors like fzf behave under the same redirection.
pub(crate) fn pick(items: &[String], initial_query: Option<&str>) -> anyhow::Result<Vec<usize>> {
    if items.is_empty() {
        return Ok(Vec::new());
    }

    let mut terminal = init_terminal()?;
    let outcome = run_picker_loop(&mut terminal, items, initial_query.unwrap_or(""));
    restore_terminal();
    outcome
}

type PickerBackend = ratatui::backend::CrosstermBackend<std::io::Stderr>;

fn init_terminal() -> anyhow::Result<ratatui::Terminal<PickerBackend>> {
    use anyhow::Context;
    use crossterm::execute;
    use crossterm::terminal::{EnterAlternateScreen, enable_raw_mode};

    enable_raw_mode().context("failed to enable raw mode for the snippet picker")?;
    execute!(std::io::stderr(), EnterAlternateScreen)
        .context("failed to enter alternate screen for the snippet picker")?;

    // Mirrors the panic-safety net `ratatui::try_init` installs for the stdout
    // case: without this, a panic mid-picker would leave the terminal stuck in
    // raw mode / the alternate screen.
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        previous_hook(info);
    }));

    ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(std::io::stderr()))
        .context("failed to initialize terminal for the snippet picker")
}

fn restore_terminal() {
    use crossterm::execute;
    use crossterm::terminal::{LeaveAlternateScreen, disable_raw_mode};

    let _ = disable_raw_mode();
    let _ = execute!(std::io::stderr(), LeaveAlternateScreen);
}

fn run_picker_loop(
    terminal: &mut ratatui::Terminal<PickerBackend>,
    items: &[String],
    initial_query: &str,
) -> anyhow::Result<Vec<usize>> {
    use anyhow::Context;
    use crossterm::event::{self, Event, KeyEventKind};

    let mut state = PickerState::new(items.to_vec(), initial_query);

    loop {
        terminal
            .draw(|frame| render(frame, &state))
            .context("failed to draw snippet picker")?;

        let Event::Key(key) = event::read().context("failed to read terminal event")? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        match handle_key(state, key.code, key.modifiers) {
            PickStep::Continue(next) => state = next,
            PickStep::Done(indices) => return Ok(indices),
            PickStep::Cancelled => return Ok(Vec::new()),
        }
    }
}

fn render(frame: &mut ratatui::Frame, state: &PickerState) {
    use ratatui::layout::{Alignment, Constraint, Direction, Layout};
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Paragraph};

    use crate::tui::visible_window;

    const INPUT_HEIGHT: u16 = 3;
    const FOOTER_HEIGHT: u16 = 1;

    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(INPUT_HEIGHT),
            Constraint::Min(0),
            Constraint::Length(FOOTER_HEIGHT),
        ])
        .split(area);

    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(
            " Snippets ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
    frame.render_widget(
        Paragraph::new(state.query.as_str()).block(input_block),
        chunks[0],
    );
    frame.set_cursor_position((chunks[0].x + 1 + state.cursor as u16, chunks[0].y + 1));

    let list_area = chunks[1];
    let capacity = list_area.height.saturating_sub(2).max(1) as usize; // minus its own border
    let (start, end) = visible_window(state.matches.len(), state.focus, capacity);

    let lines: Vec<Line> = if state.matches.is_empty() {
        vec![Line::styled(
            "No matches",
            Style::default().fg(Color::DarkGray),
        )]
    } else {
        (start..end)
            .map(|i| {
                let m = &state.matches[i];
                let item = state.items[m.index].as_str();
                let focused = i == state.focus;
                let selected = state.selected.contains(&m.index);

                let marker = if selected { "» " } else { "  " };
                let marker_color = if selected {
                    Color::Green
                } else {
                    Color::DarkGray
                };
                let mut spans = vec![Span::styled(
                    marker,
                    Style::default()
                        .fg(marker_color)
                        .add_modifier(Modifier::BOLD),
                )];

                for (ci, ch) in item.chars().enumerate() {
                    let style = if m.positions.contains(&ci) {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    spans.push(Span::styled(ch.to_string(), style));
                }

                let mut line = Line::from(spans);
                if focused {
                    line = line.style(Style::default().bg(Color::Rgb(40, 40, 40)));
                }
                line
            })
            .collect()
    };

    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(format!(" {}/{} ", state.matches.len(), state.items.len()));
    frame.render_widget(Paragraph::new(lines).block(list_block), list_area);

    let footer = Line::from(Span::styled(
        format!(
            "{} selected   ↑/↓ move   Tab select   Enter confirm   Esc cancel",
            state.selected.len()
        ),
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(
        Paragraph::new(footer).alignment(Alignment::Center),
        chunks[2],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};

    fn items(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }

    fn continuing(step: PickStep) -> PickerState {
        match step {
            PickStep::Continue(state) => state,
            _ => panic!("expected Continue"),
        }
    }

    fn done(step: PickStep) -> Vec<usize> {
        match step {
            PickStep::Done(indices) => indices,
            _ => panic!("expected Done"),
        }
    }

    // compute_matches / fuzzy scoring

    #[test]
    fn empty_query_returns_every_item_in_original_order() {
        let matches = compute_matches(&items(&["b", "a", "c"]), "");
        let indices: Vec<_> = matches.iter().map(|m| m.index).collect();
        assert_eq!(indices, vec![0, 1, 2]);
        assert!(matches.iter().all(|m| m.positions.is_empty()));
    }

    #[test]
    fn query_filters_out_non_matching_items() {
        let matches = compute_matches(&items(&["docker ps", "ping host", "list files"]), "dkr");
        let indices: Vec<_> = matches.iter().map(|m| m.index).collect();
        assert_eq!(indices, vec![0]);
    }

    #[test]
    fn query_ranks_stronger_matches_first() {
        // "docker" matches the first item as a contiguous substring (strong
        // match) and the second only as scattered characters (weak match).
        let matches = compute_matches(
            &items(&["run docker container", "detect on cellar keeper"]),
            "docker",
        );
        let indices: Vec<_> = matches.iter().map(|m| m.index).collect();
        assert_eq!(indices, vec![0, 1]);
    }

    #[test]
    fn query_matching_nothing_returns_empty() {
        let matches = compute_matches(&items(&["a", "b"]), "zzz-no-match-zzz");
        assert!(matches.is_empty());
    }

    // PickerState::new

    #[test]
    fn new_seeds_query_and_cursor_from_initial_query() {
        let state = PickerState::new(items(&["a", "b"]), "hi");
        assert_eq!(state.query, "hi");
        assert_eq!(state.cursor, 2);
    }

    // handle_key: navigation

    #[test]
    fn down_then_up_returns_focus_to_start() {
        let state = PickerState::new(items(&["a", "b", "c"]), "");
        let state = continuing(handle_key(state, KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(state.focus, 1);
        let state = continuing(handle_key(state, KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(state.focus, 0);
    }

    #[test]
    fn focus_wraps_around_in_both_directions() {
        let state = PickerState::new(items(&["a", "b", "c"]), "");
        let state = continuing(handle_key(state, KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(
            state.focus, 2,
            "up from the first item should wrap to the last"
        );
        let state = continuing(handle_key(state, KeyCode::Down, KeyModifiers::NONE));
        let state = continuing(handle_key(state, KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(state.focus, 1);
        let state = continuing(handle_key(state, KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(state.focus, 2);
        let state = continuing(handle_key(state, KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(
            state.focus, 0,
            "down from the last item should wrap to the first"
        );
    }

    #[test]
    fn navigation_on_empty_matches_does_not_panic() {
        let state = PickerState::new(items(&["a"]), "no-such-match");
        assert!(state.matches.is_empty());
        let state = continuing(handle_key(state, KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(state.focus, 0);
    }

    // handle_key: typing / editing the query

    #[test]
    fn typing_appends_to_query_and_refilters() {
        let state = PickerState::new(items(&["docker ps", "ping host"]), "");
        let state = continuing(handle_key(state, KeyCode::Char('d'), KeyModifiers::NONE));
        let state = continuing(handle_key(state, KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(state.query, "dk");
        let matched: Vec<_> = state.matches.iter().map(|m| m.index).collect();
        assert_eq!(matched, vec![0]);
    }

    #[test]
    fn backspace_removes_before_cursor_and_refilters() {
        let state = PickerState::new(items(&["docker ps", "ping host"]), "dkr");
        assert!(state.matches.len() == 1);
        let state = continuing(handle_key(state, KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(state.query, "dk");
        assert_eq!(state.cursor, 2);
    }

    #[test]
    fn backspace_at_start_of_query_is_a_noop() {
        let state = PickerState::new(items(&["a"]), "");
        let state = continuing(handle_key(state, KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(state.query, "");
    }

    #[test]
    fn left_arrow_moves_cursor_and_insert_happens_there() {
        let state = PickerState::new(items(&["a"]), "ac");
        let state = continuing(handle_key(state, KeyCode::Left, KeyModifiers::NONE));
        let state = continuing(handle_key(state, KeyCode::Char('b'), KeyModifiers::NONE));
        assert_eq!(state.query, "abc");
        assert_eq!(state.cursor, 2);
    }

    #[test]
    fn editing_the_query_resets_focus_to_the_top_match() {
        let state = PickerState::new(items(&["a", "b", "c"]), "");
        let state = continuing(handle_key(state, KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(state.focus, 1);
        let state = continuing(handle_key(state, KeyCode::Char('a'), KeyModifiers::NONE));
        assert_eq!(state.focus, 0);
    }

    // handle_key: selection / confirmation

    #[test]
    fn enter_with_nothing_toggled_picks_the_focused_item() {
        let state = PickerState::new(items(&["a", "b", "c"]), "");
        let state = continuing(handle_key(state, KeyCode::Down, KeyModifiers::NONE));
        let indices = done(handle_key(state, KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(indices, vec![1]);
    }

    #[test]
    fn enter_on_empty_matches_confirms_nothing() {
        let state = PickerState::new(items(&["a"]), "no-such-match");
        let indices = done(handle_key(state, KeyCode::Enter, KeyModifiers::NONE));
        assert!(indices.is_empty());
    }

    #[test]
    fn tab_toggles_selection_and_advances_focus() {
        let state = PickerState::new(items(&["a", "b", "c"]), "");
        let state = continuing(handle_key(state, KeyCode::Tab, KeyModifiers::NONE));
        assert!(state.selected.contains(&0));
        assert_eq!(state.focus, 1, "Tab should advance focus after toggling");
    }

    #[test]
    fn tab_twice_on_the_same_item_untoggles_it() {
        let state = PickerState::new(items(&["a", "b"]), "");
        let state = continuing(handle_key(state, KeyCode::Tab, KeyModifiers::NONE));
        let state = continuing(handle_key(state, KeyCode::Up, KeyModifiers::NONE));
        let state = continuing(handle_key(state, KeyCode::Tab, KeyModifiers::NONE));
        assert!(state.selected.is_empty());
    }

    #[test]
    fn enter_with_toggled_items_returns_all_of_them_sorted() {
        let state = PickerState::new(items(&["a", "b", "c"]), "");
        let state = continuing(handle_key(state, KeyCode::Tab, KeyModifiers::NONE)); // toggles 0, focus -> 1
        let state = continuing(handle_key(state, KeyCode::Tab, KeyModifiers::NONE)); // toggles 1, focus -> 2
        let indices = done(handle_key(state, KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(indices, vec![0, 1]);
    }

    // handle_key: cancellation

    #[test]
    fn esc_cancels() {
        let state = PickerState::new(items(&["a"]), "");
        assert!(matches!(
            handle_key(state, KeyCode::Esc, KeyModifiers::NONE),
            PickStep::Cancelled
        ));
    }

    #[test]
    fn ctrl_c_cancels() {
        let state = PickerState::new(items(&["a"]), "");
        assert!(matches!(
            handle_key(state, KeyCode::Char('c'), KeyModifiers::CONTROL),
            PickStep::Cancelled
        ));
    }

    #[test]
    fn plain_c_does_not_cancel() {
        let state = PickerState::new(items(&["a"]), "");
        let state = continuing(handle_key(state, KeyCode::Char('c'), KeyModifiers::NONE));
        assert_eq!(state.query, "c");
    }
}
