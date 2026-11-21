//! The cheat sheet: everything keys can be bound to in every mode, the
//! keys bound to it now, and what it does, shown over the pages by `:help`.

use crate::bindings::{Keymaps, Mode};
use crate::input::{self, Key};
use crate::keymap::{Bindable, Counts, Fed, Keymap, Pending, bindable};

bindable! {
    /// What keys can be bound to while the sheet is shown.
    pub enum Binding {
        Down = "down", "Scroll down a line";
        Up = "up", "Scroll up a line";
        HalfDown = "half-down", "Scroll half a screen down";
        HalfUp = "half-up", "Scroll half a screen up";
        PageDown = "page-down", "Scroll a screen down";
        PageUp = "page-up", "Scroll a screen up";
        Top = "top", "Go to the top";
        Bottom = "bottom", "Go to the bottom";
        Close = "close", "Close the list";
    }
}

/// Normal mode, in groups, in the order the sheet shows them.
const NORMAL: [(&str, &[input::Binding]); 10] = {
    use input::Binding as B;
    [
        (
            "Scrolling",
            &[
                B::ScrollDown,
                B::ScrollUp,
                B::ScrollLeft,
                B::ScrollRight,
                B::ScrollHalfDown,
                B::ScrollHalfUp,
                B::ScrollFullDown,
                B::ScrollFullUp,
            ],
        ),
        (
            "Pages",
            &[
                B::NextPage,
                B::PreviousPage,
                B::FirstPage,
                B::LastPage,
                B::PageTop,
                B::PageBottom,
                B::NextChapter,
                B::PreviousChapter,
                B::Outline,
            ],
        ),
        (
            "Zoom and view",
            &[
                B::ZoomIn,
                B::ZoomOut,
                B::ZoomTo,
                B::ZoomReset,
                B::FitWidth,
                B::FitPage,
                B::Rotate,
                B::ToggleRecolor,
                B::ToggleTrim,
            ],
        ),
        (
            "Search and prompts",
            &[
                B::SearchForward,
                B::SearchBackward,
                B::SearchNext,
                B::SearchPrevious,
                B::CommandPrompt,
                B::Recent,
                B::Help,
            ],
        ),
        (
            "Links and forms",
            &[B::FollowLink, B::FirstField, B::LastField, B::FieldAgain],
        ),
        (
            "Marks and jumps",
            &[B::SetMark, B::GoToMark, B::JumpBack, B::JumpForward],
        ),
        ("Selecting text", &[B::Visual, B::VisualLine, B::Flash]),
        (
            "Highlights and other markings",
            &[
                B::NextMarking,
                B::PreviousMarking,
                B::LabelMarkings,
                B::DeleteMarking,
                B::ChangeNote,
                B::YankMarking,
                B::Markings,
                B::Undo,
                B::Redo,
                B::Write,
            ],
        ),
        ("Tabs", &[B::NextTab, B::PreviousTab, B::ReopenTab]),
        (
            "Other",
            &[
                B::Info,
                B::YankPath,
                B::YankTitle,
                B::YankPage,
                B::Abort,
                B::Quit,
            ],
        ),
    ]
};

/// The heading of the part of the sheet for a mode other than normal,
/// which comes in groups.
const fn heading(mode: Mode) -> &'static str {
    match mode {
        Mode::Normal => "Reading",
        Mode::Visual => "Visual mode: selecting text",
        Mode::Hints => "Labels on links and lines",
        Mode::Outline => "Table of contents",
        Mode::Markings => "List of markings",
        Mode::Recent => "Recent documents; a digit opens that one",
        Mode::Help => "This list",
        Mode::Prompt => "Typing a command or search",
        Mode::Confirm => "Questions such as quit? (y/n)",
    }
}

/// A line of the sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    Heading(&'static str),
    Entry {
        /// The mode the keys are bound in.
        mode: Mode,
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
    pending: Pending<Binding>,
}

impl Help {
    /// The sheet for the bindings in force.
    pub fn new(keymaps: &Keymaps) -> Self {
        let mut lines = Vec::new();
        for (heading, bindings) in NORMAL {
            let bindings = bindings.iter().copied();
            section(
                &mut lines,
                (Mode::Normal, heading),
                &keymaps.normal,
                bindings,
            );
        }
        for &mode in Mode::ALL {
            match mode {
                Mode::Normal => {}
                Mode::Visual => all(&mut lines, mode, &keymaps.visual),
                Mode::Hints => all(&mut lines, mode, &keymaps.hints),
                Mode::Outline => all(&mut lines, mode, &keymaps.outline),
                Mode::Markings => all(&mut lines, mode, &keymaps.markings),
                Mode::Recent => all(&mut lines, mode, &keymaps.recent),
                Mode::Help => all(&mut lines, mode, &keymaps.help),
                Mode::Prompt => all(&mut lines, mode, &keymaps.prompt),
                Mode::Confirm => all(&mut lines, mode, &keymaps.confirm),
            }
        }
        Self {
            lines,
            top: 0,
            pending: Pending::new(),
        }
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

    /// Takes a key, bound as in the keymap, given how many rows are shown.
    /// Returns whether the sheet stays open.
    pub fn feed(&mut self, key: Key, keymap: &Keymap<Binding>, rows: usize) -> bool {
        let Fed::Bound(binding, count) = self.pending.feed(key, keymap, Counts::Taken) else {
            return true;
        };
        let times = count
            .and_then(|count| usize::try_from(count).ok())
            .unwrap_or(1);
        let half = rows.div_ceil(2).max(1);
        let bottom = self.lines.len().saturating_sub(rows);
        self.top = match binding {
            Binding::Down => self.top.saturating_add(times),
            Binding::Up => self.top.saturating_sub(times),
            Binding::HalfDown => self.top.saturating_add(half),
            Binding::HalfUp => self.top.saturating_sub(half),
            Binding::PageDown => self.top.saturating_add(rows),
            Binding::PageUp => self.top.saturating_sub(rows),
            Binding::Top => 0,
            Binding::Bottom => bottom,
            Binding::Close => return false,
        }
        .min(bottom);
        true
    }
}

/// Adds a part of the sheet: a heading, then a line for each of these,
/// with the keys bound to it.
fn section<A: Bindable>(
    lines: &mut Vec<Line>,
    (mode, heading): (Mode, &'static str),
    keymap: &Keymap<A>,
    bindings: impl Iterator<Item = A>,
) {
    if !lines.is_empty() {
        lines.push(Line::Blank);
    }
    lines.push(Line::Heading(heading));
    for binding in bindings {
        let keys: Vec<String> = keymap.keys_for(binding).map(written).collect();
        lines.push(Line::Entry {
            mode,
            keys: keys.join(", "),
            action: binding.name(),
            description: binding.description(),
        });
    }
}

/// Adds a part of the sheet for everything in a mode.
fn all<A: Bindable>(lines: &mut Vec<Line>, mode: Mode, keymap: &Keymap<A>) {
    let bindings = A::ALL.iter().map(|&(binding, ..)| binding);
    section(lines, (mode, heading(mode)), keymap, bindings);
}

/// A key sequence as written in the configuration.
fn written(keys: &[Key]) -> String {
    crate::keymap::written(keys)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bindings::Bound;
    use crate::keymap::parse_keys;

    fn entry<'a>(help: &'a Help, in_mode: Mode, name: &str) -> Option<&'a str> {
        help.lines.iter().find_map(|line| match line {
            Line::Entry {
                mode, keys, action, ..
            } if *mode == in_mode && *action == name => Some(keys.as_str()),
            _ => None,
        })
    }

    fn feed(help: &mut Help, key: Key, rows: usize) -> bool {
        help.feed(key, &Keymaps::default().help, rows)
    }

    #[test]
    fn everything_bindable_is_on_the_sheet_once() {
        let help = Help::new(&Keymaps::default());
        for &mode in Mode::ALL {
            for name in mode.names() {
                let count = help
                    .lines
                    .iter()
                    .filter(|line| {
                        matches!(line, Line::Entry { mode: in_mode, action, .. }
                            if *in_mode == mode && *action == name)
                    })
                    .count();
                assert_eq!(count, 1, "{} {name}", mode.name());
            }
        }
    }

    #[test]
    fn the_sheet_shows_the_keys_in_force() {
        let mut keymaps = Keymaps::default();
        let help = Help::new(&keymaps);
        assert_eq!(entry(&help, Mode::Normal, "scroll-down"), Some("j, down"));
        assert_eq!(entry(&help, Mode::Normal, "first-page"), Some("g g, home"));
        assert_eq!(entry(&help, Mode::Normal, "help"), Some(""));
        assert_eq!(entry(&help, Mode::Visual, "yank"), Some("y"));
        assert_eq!(entry(&help, Mode::Confirm, "yes"), Some("y, Y, enter"));

        keymaps.unbind(Mode::Normal, &parse_keys("q").unwrap());
        keymaps.bind(
            parse_keys("shift+q").unwrap(),
            Bound::Normal(input::Binding::Quit),
        );
        keymaps.bind(
            parse_keys("ctrl+h").unwrap(),
            Bound::Normal(input::Binding::Help),
        );
        keymaps.bind(
            parse_keys("Y").unwrap(),
            Bound::Visual(crate::visual::Binding::Yank),
        );
        let help = Help::new(&keymaps);
        assert_eq!(entry(&help, Mode::Normal, "quit"), Some("Q"));
        assert_eq!(entry(&help, Mode::Normal, "help"), Some("ctrl+h"));
        assert_eq!(entry(&help, Mode::Visual, "yank"), Some("y, Y"));
    }

    #[test]
    fn scrolling_stops_at_both_ends() {
        let mut help = Help::new(&Keymaps::default());
        let (_, count) = help.position();
        let rows = 10;
        assert!(feed(&mut help, Key::Char('k'), rows));
        assert_eq!(help.position().0, 0);
        feed(&mut help, Key::Char('j'), rows);
        feed(&mut help, Key::Ctrl('d'), rows);
        assert_eq!(help.position().0, 6);
        feed(&mut help, Key::Char('3'), rows);
        feed(&mut help, Key::Char('j'), rows);
        assert_eq!(help.position().0, 9);
        feed(&mut help, Key::Char('G'), rows);
        assert_eq!(help.position().0, count - rows);
        assert_eq!(help.shown(rows).len(), rows);
        feed(&mut help, Key::Char(' '), rows);
        assert_eq!(help.position().0, count - rows);
        feed(&mut help, Key::Char('g'), rows);
        assert_eq!(help.position().0, 0);
        assert!(!feed(&mut help, Key::Char('q'), rows));
        assert!(!feed(&mut help, Key::Escape, rows));
    }

    #[test]
    fn a_tall_window_shows_everything() {
        let mut help = Help::new(&Keymaps::default());
        let (_, count) = help.position();
        feed(&mut help, Key::Char('G'), count + 50);
        assert_eq!(help.position().0, 0);
        assert_eq!(help.shown(count + 50).len(), count);
    }
}
