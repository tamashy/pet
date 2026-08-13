//! Small, pure helpers shared by the terminal-UI screens (`dialog`'s parameter
//! resolution form and `picker`'s native fuzzy selector) — kept free of any
//! `ratatui`/`crossterm` I/O so they're trivially unit-testable.

/// Which items are on screen, as a `[start, end)` range into a list of `total`
/// items, given how many rows fit (`capacity`). Keeps `focus` inside the window,
/// scrolling the minimum amount needed rather than re-centering every frame.
pub(crate) fn visible_window(total: usize, focus: usize, capacity: usize) -> (usize, usize) {
    if capacity == 0 || total <= capacity {
        return (0, total);
    }
    let max_start = total - capacity;
    let start = if focus >= capacity {
        (focus + 1 - capacity).min(max_start)
    } else {
        0
    };
    (start, start + capacity)
}

/// Byte offset of the `char_idx`-th character in `s` (or `s.len()` past the end),
/// for converting a char-index text cursor into a byte range for `str` slicing.
pub(crate) fn char_byte_index(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map_or(s.len(), |(byte_idx, _)| byte_idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_window_shows_everything_when_it_all_fits() {
        assert_eq!(visible_window(3, 0, 5), (0, 3));
        assert_eq!(visible_window(3, 2, 3), (0, 3));
    }

    #[test]
    fn visible_window_stays_at_top_while_focus_is_within_the_first_page() {
        assert_eq!(visible_window(10, 0, 3), (0, 3));
        assert_eq!(visible_window(10, 2, 3), (0, 3));
    }

    #[test]
    fn visible_window_scrolls_forward_to_keep_focus_in_view() {
        assert_eq!(visible_window(10, 3, 3), (1, 4));
        assert_eq!(visible_window(10, 9, 3), (7, 10));
    }

    #[test]
    fn visible_window_never_scrolls_past_the_last_page() {
        // Even at the very last item, the window shouldn't run off the end.
        assert_eq!(visible_window(5, 4, 3), (2, 5));
    }

    #[test]
    fn char_byte_index_handles_multibyte_characters() {
        let s = "héllo";
        assert_eq!(char_byte_index(s, 0), 0);
        assert_eq!(char_byte_index(s, 1), 1);
        // 'é' is 2 bytes, so char 2 ('l') starts at byte 3.
        assert_eq!(char_byte_index(s, 2), 3);
        assert_eq!(char_byte_index(s, 5), s.len());
    }
}
