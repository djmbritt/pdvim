//! The overview: every page of a document small, side by side in rows, to
//! move through from the keyboard and read from the one picked.
//!
//! What it keeps is which page is selected and which row is at the top.
//! How many pages fit in a row, and how many rows on the screen, depend on
//! the window, so each move is told them.

use std::ops::Range;

use crate::input::Key;
use crate::keymap::{Counts, Fed, Keymap, Pending, bindable};

bindable! {
    /// What keys can be bound to in the overview.
    pub enum Binding {
        Down = "down", "Go a row down";
        Up = "up", "Go a row up";
        Left = "left", "Go to the page before";
        Right = "right", "Go to the page after";
        ScreenDown = "screen-down", "Go a screen of rows down";
        ScreenUp = "screen-up", "Go a screen of rows up";
        First = "first", "Go to the first page, or page n";
        Last = "last", "Go to the last page, or page n";
        Go = "go", "Read from the page selected";
        Close = "close", "Close the overview, and read on where it was";
    }
}

/// What a key typed in the overview comes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Stay,
    /// Read from this page, counted from 0.
    Go(u32),
    Close,
}

/// The pages of a document, small, and the one selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overview {
    /// How many pages the document has.
    count: u32,
    /// The page selected, counted from 0.
    selected: u32,
    /// The row at the top of the screen, counted from 0.
    top: u32,
    pending: Pending<Binding>,
}

impl Overview {
    /// The pages of a document of `count` pages, the one given selected.
    pub fn new(count: u32, page: u32) -> Self {
        Self {
            count,
            selected: page.min(count.saturating_sub(1)),
            top: 0,
            pending: Pending::new(),
        }
    }

    pub const fn selected(&self) -> u32 {
        self.selected
    }

    /// What has been typed so far, for the status bar.
    pub fn typed(&self) -> String {
        self.pending.display()
    }

    /// The pages shown in rows of `columns`, `rows` of them on the screen:
    /// from the top row, moved as little as it takes for the page selected
    /// to be among them.
    pub fn shown(&self, columns: u32, rows: u32) -> Range<u32> {
        let columns = columns.max(1);
        let top = self.top_row(columns, rows.max(1));
        let first = top.saturating_mul(columns).min(self.count);
        let last = top
            .saturating_add(rows.max(1))
            .saturating_mul(columns)
            .min(self.count);
        first..last
    }

    /// The row at the top once the page selected is in view.
    const fn top_row(&self, columns: u32, rows: u32) -> u32 {
        let row = match self.selected.checked_div(columns) {
            Some(row) => row,
            None => 0,
        };
        if row < self.top {
            row
        } else if row >= self.top.saturating_add(rows) {
            row.saturating_add(1).saturating_sub(rows)
        } else {
            self.top
        }
    }

    /// Takes a key, bound as in the keymap, with the pages in rows of
    /// `columns` and `rows` of them on the screen.
    pub fn feed(&mut self, key: Key, keymap: &Keymap<Binding>, columns: u32, rows: u32) -> Outcome {
        let columns = columns.max(1);
        let rows = rows.max(1);
        let (binding, count) = match self.pending.feed(key, keymap, Counts::Taken) {
            Fed::Bound(binding, count) => (binding, count),
            Fed::Waiting | Fed::Unbound(_) | Fed::Command(..) => return Outcome::Stay,
        };
        let times = count.unwrap_or(1);
        let last = self.count.saturating_sub(1);
        // Up past the first row stays in the same column, as `k` does.
        let up = |by: u32| {
            self.selected
                .checked_sub(by)
                .unwrap_or_else(|| self.selected.checked_rem(columns).unwrap_or(0))
        };
        self.selected = match binding {
            Binding::Go => return Outcome::Go(self.selected),
            Binding::Close => return Outcome::Close,
            Binding::Down => self.selected.saturating_add(times.saturating_mul(columns)),
            Binding::Up => up(times.saturating_mul(columns)),
            Binding::Left => self.selected.saturating_sub(times),
            Binding::Right => self.selected.saturating_add(times),
            Binding::ScreenDown => self
                .selected
                .saturating_add(times.saturating_mul(rows).saturating_mul(columns)),
            Binding::ScreenUp => up(times.saturating_mul(rows).saturating_mul(columns)),
            Binding::First => count.map_or(0, |page| page.saturating_sub(1)),
            Binding::Last => count.map_or(last, |page| page.saturating_sub(1)),
        }
        .min(last);
        self.top = self.top_row(columns, rows);
        Outcome::Stay
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(overview: &mut Overview, typed: &str, columns: u32, rows: u32) -> Outcome {
        let keymap = crate::bindings::Keymaps::default().overview;
        let mut outcome = Outcome::Stay;
        for key in typed.chars() {
            outcome = overview.feed(Key::Char(key), &keymap, columns, rows);
        }
        outcome
    }

    #[test]
    fn the_keys_move_through_rows_and_pages() {
        // Twenty-three pages in rows of five: the last row has three.
        let mut overview = Overview::new(23, 0);
        keys(&mut overview, "j", 5, 2);
        assert_eq!(overview.selected(), 5);
        keys(&mut overview, "2l", 5, 2);
        assert_eq!(overview.selected(), 7);
        keys(&mut overview, "k", 5, 2);
        assert_eq!(overview.selected(), 2);
        // Up from the first row stays where it is.
        keys(&mut overview, "k", 5, 2);
        assert_eq!(overview.selected(), 2);
        // Down past the last row stops at the last page.
        keys(&mut overview, "9j", 5, 2);
        assert_eq!(overview.selected(), 22);
        keys(&mut overview, "gg", 5, 2);
        assert_eq!(overview.selected(), 0);
        keys(&mut overview, "12G", 5, 2);
        assert_eq!(overview.selected(), 11);
        keys(&mut overview, "G", 5, 2);
        assert_eq!(overview.selected(), 22);
        assert_eq!(keys(&mut overview, "h\r", 5, 2), Outcome::Stay);
    }

    #[test]
    fn the_rows_shown_follow_the_page_selected() {
        let mut overview = Overview::new(23, 0);
        assert_eq!(overview.shown(5, 2), 0..10);
        // Two rows down, the screen moves one row.
        keys(&mut overview, "2j", 5, 2);
        assert_eq!(overview.shown(5, 2), 5..15);
        // The last rows, short of a full screen.
        keys(&mut overview, "G", 5, 2);
        assert_eq!(overview.shown(5, 2), 15..23);
        // Back up, the screen moves only as far as it must.
        keys(&mut overview, "k", 5, 2);
        assert_eq!(overview.shown(5, 2), 15..23);
        keys(&mut overview, "k", 5, 2);
        assert_eq!(overview.shown(5, 2), 10..20);
        // Opened at a page, it is in view; a narrower window shows it too.
        let opened = Overview::new(100, 57);
        assert!(opened.shown(4, 3).contains(&57));
        assert!(opened.shown(1, 1).contains(&57));
    }

    #[test]
    fn enter_reads_from_the_page_and_escape_goes_back() {
        let keymap = crate::bindings::Keymaps::default().overview;
        let mut overview = Overview::new(10, 3);
        assert_eq!(overview.feed(Key::Enter, &keymap, 4, 2), Outcome::Go(3));
        assert_eq!(overview.feed(Key::Escape, &keymap, 4, 2), Outcome::Close);
        // A document of no pages has nothing to select or show.
        let empty = Overview::new(0, 0);
        assert_eq!(empty.shown(4, 2), 0..0);
    }
}
