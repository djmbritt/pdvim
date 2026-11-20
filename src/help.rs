//! The cheat sheet: every action, the keys bound to it now, and what it
//! does, shown over the pages by `:help`.

use crate::input::Key;
use crate::keymap::{Binding, Keymap};

/// The actions, in groups, in the order the sheet shows them.
const GROUPS: [(&str, &[Binding]); 8] = [
    (
        "Scrolling",
        &[
            Binding::ScrollDown,
            Binding::ScrollUp,
            Binding::ScrollLeft,
            Binding::ScrollRight,
            Binding::ScrollHalfDown,
            Binding::ScrollHalfUp,
            Binding::ScrollFullDown,
            Binding::ScrollFullUp,
        ],
    ),
    (
        "Pages",
        &[
            Binding::NextPage,
            Binding::PreviousPage,
            Binding::FirstPage,
            Binding::LastPage,
            Binding::PageTop,
            Binding::PageBottom,
        ],
    ),
    (
        "Zoom and view",
        &[
            Binding::ZoomIn,
            Binding::ZoomOut,
            Binding::ZoomTo,
            Binding::ZoomReset,
            Binding::FitWidth,
            Binding::FitPage,
            Binding::Rotate,
            Binding::ToggleRecolor,
            Binding::ToggleTrim,
        ],
    ),
    (
        "Search and prompts",
        &[
            Binding::SearchForward,
            Binding::SearchBackward,
            Binding::SearchNext,
            Binding::SearchPrevious,
            Binding::CommandPrompt,
            Binding::Recent,
            Binding::Help,
        ],
    ),
    ("Links", &[Binding::FollowLink]),
    ("Selecting text", &[Binding::Visual, Binding::VisualLine]),
    ("Tabs", &[Binding::NextTab, Binding::PreviousTab]),
    ("Other", &[Binding::Abort, Binding::Quit]),
];

/// What an action does, in a few words.
const fn describe(binding: Binding) -> &'static str {
    match binding {
        Binding::ScrollDown => "Scroll down",
        Binding::ScrollUp => "Scroll up",
        Binding::ScrollLeft => "Scroll left",
        Binding::ScrollRight => "Scroll right",
        Binding::ScrollHalfDown => "Scroll half a screen down",
        Binding::ScrollHalfUp => "Scroll half a screen up",
        Binding::ScrollFullDown => "Scroll a screen down",
        Binding::ScrollFullUp => "Scroll a screen up",
        Binding::NextPage => "Go to the next page",
        Binding::PreviousPage => "Go to the previous page",
        Binding::FirstPage => "Go to the first page, or page n",
        Binding::LastPage => "Go to the last page, or page n",
        Binding::PageTop => "Go to the top of the page",
        Binding::PageBottom => "Go to the bottom of the page",
        Binding::ZoomIn => "Zoom in",
        Binding::ZoomOut => "Zoom out",
        Binding::ZoomTo => "Zoom to 100%, or to n%",
        Binding::ZoomReset => "Zoom to 100%",
        Binding::FitWidth => "Fit the page width to the window",
        Binding::FitPage => "Fit the whole page in the window",
        Binding::Rotate => "Rotate",
        Binding::ToggleRecolor => "Turn recoloring on or off",
        Binding::ToggleTrim => "Turn margin trimming on or off",
        Binding::SearchForward => "Search forwards",
        Binding::SearchBackward => "Search backwards",
        Binding::SearchNext => "Go to the next match",
        Binding::SearchPrevious => "Go to the previous match",
        Binding::CommandPrompt => "Open the command prompt",
        Binding::Recent => "Show recent documents",
        Binding::Help => "Show this list",
        Binding::FollowLink => "Label the links in view; type a label to follow it",
        Binding::Visual => "Select text from a line picked by label; y copies",
        Binding::VisualLine => "Select whole lines from a line picked by label",
        Binding::NextTab => "Go to the next tab, or tab n",
        Binding::PreviousTab => "Go to the previous tab",
        Binding::Abort => "Cancel a half-typed command, hide highlights",
        Binding::Quit => "Close the tab, and quit after the last one",
    }
}

/// A line of the sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    Heading(&'static str),
    Entry {
        /// The keys bound to the action, or nothing.
        keys: String,
        action: &'static str,
        description: &'static str,
    },
    Blank,
}

/// The sheet on screen, and how far down it is scrolled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Help {
    lines: Vec<Line>,
    /// The first line shown.
    top: usize,
}

impl Help {
    /// The sheet for the bindings in force.
    pub fn new(keymap: &Keymap) -> Self {
        let mut lines = Vec::new();
        for (heading, bindings) in GROUPS {
            if !lines.is_empty() {
                lines.push(Line::Blank);
            }
            lines.push(Line::Heading(heading));
            for &binding in bindings {
                let keys: Vec<String> = keymap.keys_for(binding).map(written).collect();
                lines.push(Line::Entry {
                    keys: keys.join(", "),
                    action: binding.name(),
                    description: describe(binding),
                });
            }
        }
        Self { lines, top: 0 }
    }

    /// The lines that fit in this many rows, from the top shown.
    pub fn shown(&self, rows: usize) -> &[Line] {
        let end = self.top.saturating_add(rows).min(self.lines.len());
        self.lines.get(self.top..end).unwrap_or_default()
    }

    /// The first line shown, and how many there are.
    pub const fn position(&self) -> (usize, usize) {
        (self.top, self.lines.len())
    }

    /// Takes a key, given how many rows are shown. Returns whether the
    /// sheet stays open.
    pub fn feed(&mut self, key: Key, rows: usize) -> bool {
        let half = rows.div_ceil(2).max(1);
        let bottom = self.lines.len().saturating_sub(rows);
        self.top = match key {
            Key::Char('j') | Key::Down | Key::Ctrl('n' | 'e') => self.top.saturating_add(1),
            Key::Char('k') | Key::Up | Key::Ctrl('p' | 'y') => self.top.saturating_sub(1),
            Key::Ctrl('d') => self.top.saturating_add(half),
            Key::Ctrl('u') => self.top.saturating_sub(half),
            Key::Char(' ') | Key::Ctrl('f') | Key::PageDown => self.top.saturating_add(rows),
            Key::ShiftSpace | Key::Ctrl('b') | Key::PageUp => self.top.saturating_sub(rows),
            Key::Char('g') | Key::Home => 0,
            Key::Char('G') | Key::End => bottom,
            Key::Char('q') | Key::Escape | Key::Enter | Key::Ctrl('c' | '[') => return false,
            _ => self.top,
        }
        .min(bottom);
        true
    }
}

/// A key sequence as written in the configuration.
fn written(keys: &[Key]) -> String {
    let keys: Vec<String> = keys.iter().map(ToString::to_string).collect();
    keys.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::parse_keys;

    fn entry<'a>(help: &'a Help, action: &str) -> Option<&'a str> {
        help.lines.iter().find_map(|line| match line {
            Line::Entry {
                keys, action: name, ..
            } if *name == action => Some(keys.as_str()),
            _ => None,
        })
    }

    #[test]
    fn every_action_is_on_the_sheet_once() {
        let help = Help::new(&Keymap::default());
        for name in Binding::names() {
            let count = help
                .lines
                .iter()
                .filter(|line| matches!(line, Line::Entry { action, .. } if *action == name))
                .count();
            assert_eq!(count, 1, "{name}");
        }
    }

    #[test]
    fn the_sheet_shows_the_keys_in_force() {
        let mut keymap = Keymap::default();
        let help = Help::new(&keymap);
        assert_eq!(entry(&help, "scroll-down"), Some("j, down"));
        assert_eq!(entry(&help, "first-page"), Some("g g, home"));
        assert_eq!(entry(&help, "help"), Some(""));

        keymap.unbind(&parse_keys("q").unwrap());
        keymap.bind(parse_keys("shift+q").unwrap(), Binding::Quit);
        keymap.bind(parse_keys("ctrl+h").unwrap(), Binding::Help);
        let help = Help::new(&keymap);
        assert_eq!(entry(&help, "quit"), Some("Q"));
        assert_eq!(entry(&help, "help"), Some("ctrl+h"));
    }

    #[test]
    fn scrolling_stops_at_both_ends() {
        let mut help = Help::new(&Keymap::default());
        let (_, count) = help.position();
        let rows = 10;
        assert!(help.feed(Key::Char('k'), rows));
        assert_eq!(help.position().0, 0);
        help.feed(Key::Char('j'), rows);
        help.feed(Key::Ctrl('d'), rows);
        assert_eq!(help.position().0, 6);
        help.feed(Key::Char('G'), rows);
        assert_eq!(help.position().0, count - rows);
        assert_eq!(help.shown(rows).len(), rows);
        help.feed(Key::Char(' '), rows);
        assert_eq!(help.position().0, count - rows);
        help.feed(Key::Char('g'), rows);
        assert_eq!(help.position().0, 0);
        assert!(!help.feed(Key::Char('q'), rows));
        assert!(!help.feed(Key::Escape, rows));
    }

    #[test]
    fn a_tall_window_shows_everything() {
        let mut help = Help::new(&Keymap::default());
        let (_, count) = help.position();
        help.feed(Key::Char('G'), count + 50);
        assert_eq!(help.position().0, 0);
        assert_eq!(help.shown(count + 50).len(), count);
    }
}
