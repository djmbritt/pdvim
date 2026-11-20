//! Turning key presses into actions, the way vi does.
//!
//! A command is an optional count, then a sequence of keys bound in the
//! [`Keymap`]: `j`, `12G`, `gg`, `zi`. [`Pending`] holds what has been typed
//! so far and yields an [`Action`] once a bound sequence is complete.

use crate::keymap::{Keymap, Match};

/// A key press, reduced to what the reader distinguishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A key that types a character, with shift already applied.
    Char(char),
    /// A letter pressed with control held, in lower case.
    Ctrl(char),
    Escape,
    Enter,
    Backspace,
    ShiftSpace,
    Tab,
    ShiftTab,
    /// Tab pressed with control held.
    CtrlTab,
    /// Tab pressed with control and shift held.
    CtrlShiftTab,
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
}

/// What the bottom line is asking for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prompt {
    /// A command, after `:`.
    Command,
    /// A search towards the end of the document, after `/`.
    SearchForward,
    /// A search towards the start of the document, after `?`.
    SearchBackward,
}

impl Prompt {
    /// The character shown in front of the typed text.
    pub const fn sigil(self) -> char {
        match self {
            Self::Command => ':',
            Self::SearchForward => '/',
            Self::SearchBackward => '?',
        }
    }
}

/// Something the reader can be told to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Scroll by a number of steps: right and down are positive.
    Scroll {
        right: i32,
        down: i32,
    },
    /// Scroll by a number of half screens: down is positive.
    HalfScreen(i32),
    /// Scroll by a number of whole screens: down is positive.
    FullScreen(i32),
    /// Move to the top of a page a number of pages away: forward is positive.
    Page(i32),
    /// Go to the top of a page, counted from 1.
    GotoPage(u32),
    GotoFirst,
    GotoLast,
    /// Scroll to the top of the current page.
    PageTop,
    /// Scroll to the bottom of the current page.
    PageBottom,
    ZoomIn,
    ZoomOut,
    /// Zoom to a percentage of the page's real size.
    ZoomTo(u32),
    FitWidth,
    FitPage,
    Rotate,
    ToggleRecolor,
    /// Hide or show the blank margins of every page.
    ToggleTrim,
    SearchNext,
    SearchPrevious,
    OpenPrompt(Prompt),
    /// Show the recently read documents, to pick one to open.
    ShowRecent,
    /// Show the key bindings.
    ShowHelp,
    /// Label the links in view, to follow one.
    FollowLink,
    /// Label the lines in view, to start selecting text at one: whole lines
    /// or characters.
    Visual {
        lines: bool,
    },
    /// Take back the last highlight.
    Undo,
    /// Put back the last highlight taken back.
    Redo,
    /// Go to the next tab, or the first after the last.
    NextTab,
    /// Go back a number of tabs, round to the last from the first.
    PreviousTab(u32),
    /// Go to a tab, counted from 1.
    GotoTab(u32),
    /// Drop whatever is half-typed and hide search highlights.
    Abort,
    /// Close the tab in view, and the program with its last tab.
    Quit,
}

/// The part of a command typed so far.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pending {
    count: Option<u32>,
    /// The keys typed after the count, the start of a bound sequence.
    keys: Vec<Key>,
}

impl Pending {
    /// What has been typed so far, for the status bar.
    pub fn display(&self) -> String {
        let mut shown = self
            .count
            .map(|count| count.to_string())
            .unwrap_or_default();
        let keys: Vec<String> = self.keys.iter().map(ToString::to_string).collect();
        shown.push_str(&keys.join(" "));
        shown
    }

    /// Takes the next key. Returns an action once a bound sequence is
    /// complete; keys that are bound to nothing clear what was typed.
    pub fn feed(&mut self, key: Key, keymap: &Keymap) -> Option<Action> {
        // A digit is part of a count, unless it comes in the middle of a
        // sequence, as in `z0`. A count cannot start with 0.
        if self.keys.is_empty()
            && let Key::Char(digit @ '0'..='9') = key
            && let Some(value) = digit.to_digit(10)
            && (self.count.is_some() || value != 0)
        {
            let count = self.count.unwrap_or(0);
            self.count = Some(count.saturating_mul(10).saturating_add(value));
            return None;
        }
        self.keys.push(key);
        match keymap.lookup(&self.keys) {
            Match::Prefix => None,
            Match::Bound(binding) => {
                let count = self.count.take();
                self.keys.clear();
                Some(binding.action(count))
            }
            Match::None => {
                *self = Self::default();
                None
            }
        }
    }
}

impl std::fmt::Display for Key {
    /// Writes the key as it is written in the configuration.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Char(' ') => formatter.write_str("space"),
            Self::Char(character) => write!(formatter, "{character}"),
            Self::Ctrl(character) => write!(formatter, "ctrl+{character}"),
            Self::Escape => formatter.write_str("escape"),
            Self::Enter => formatter.write_str("enter"),
            Self::Backspace => formatter.write_str("backspace"),
            Self::ShiftSpace => formatter.write_str("shift+space"),
            Self::Tab => formatter.write_str("tab"),
            Self::ShiftTab => formatter.write_str("shift+tab"),
            Self::CtrlTab => formatter.write_str("ctrl+tab"),
            Self::CtrlShiftTab => formatter.write_str("ctrl+shift+tab"),
            Self::Up => formatter.write_str("up"),
            Self::Down => formatter.write_str("down"),
            Self::Left => formatter.write_str("left"),
            Self::Right => formatter.write_str("right"),
            Self::PageUp => formatter.write_str("pageup"),
            Self::PageDown => formatter.write_str("pagedown"),
            Self::Home => formatter.write_str("home"),
            Self::End => formatter.write_str("end"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feeds each character as a key and returns the actions that result.
    fn typed(keys: &str) -> Vec<Action> {
        let keymap = Keymap::default();
        let mut pending = Pending::default();
        keys.chars()
            .filter_map(|key| pending.feed(Key::Char(key), &keymap))
            .collect()
    }

    #[test]
    fn plain_keys_act_at_once() {
        assert_eq!(
            typed("jk"),
            [
                Action::Scroll { right: 0, down: 1 },
                Action::Scroll { right: 0, down: -1 }
            ]
        );
        assert_eq!(typed("G"), [Action::GotoLast]);
        assert_eq!(typed("q"), [Action::Quit]);
    }

    #[test]
    fn counts_multiply_or_address() {
        assert_eq!(typed("5j"), [Action::Scroll { right: 0, down: 5 }]);
        assert_eq!(typed("12G"), [Action::GotoPage(12)]);
        assert_eq!(typed("3gg"), [Action::GotoPage(3)]);
        assert_eq!(typed("150="), [Action::ZoomTo(150)]);
        assert_eq!(typed("10J"), [Action::Page(10)]);
    }

    #[test]
    fn two_key_commands_wait_for_the_second_key() {
        let keymap = Keymap::default();
        let mut pending = Pending::default();
        assert_eq!(pending.feed(Key::Char('g'), &keymap), None);
        assert_eq!(pending.display(), "g");
        assert_eq!(
            pending.feed(Key::Char('g'), &keymap),
            Some(Action::GotoFirst)
        );
        assert_eq!(pending.display(), "");
        assert_eq!(typed("zi"), [Action::ZoomIn]);
        assert_eq!(typed("zo"), [Action::ZoomOut]);
        assert_eq!(typed("z0"), [Action::ZoomTo(100)]);
    }

    #[test]
    fn g_t_moves_between_tabs() {
        assert_eq!(typed("gt"), [Action::NextTab]);
        assert_eq!(typed("3gt"), [Action::GotoTab(3)]);
        assert_eq!(typed("gT"), [Action::PreviousTab(1)]);
        assert_eq!(typed("2gT"), [Action::PreviousTab(2)]);

        let keymap = Keymap::default();
        let mut pending = Pending::default();
        assert_eq!(pending.feed(Key::CtrlTab, &keymap), Some(Action::NextTab));
        assert_eq!(
            pending.feed(Key::CtrlShiftTab, &keymap),
            Some(Action::PreviousTab(1))
        );
        assert_eq!(pending.feed(Key::Char('2'), &keymap), None);
        assert_eq!(
            pending.feed(Key::CtrlTab, &keymap),
            Some(Action::GotoTab(2))
        );
    }

    #[test]
    fn a_leading_zero_is_not_a_count() {
        let keymap = Keymap::default();
        let mut pending = Pending::default();
        assert_eq!(pending.feed(Key::Char('0'), &keymap), None);
        assert_eq!(pending.display(), "");
        assert_eq!(typed("10j"), [Action::Scroll { right: 0, down: 10 }]);
    }

    #[test]
    fn unknown_keys_and_escape_clear_what_was_typed() {
        let keymap = Keymap::default();
        let mut pending = Pending::default();
        assert_eq!(pending.feed(Key::Char('4'), &keymap), None);
        assert_eq!(pending.feed(Key::Char('g'), &keymap), None);
        assert_eq!(pending.display(), "4g");
        assert_eq!(pending.feed(Key::Char('x'), &keymap), None);
        assert_eq!(pending.display(), "");

        assert_eq!(pending.feed(Key::Char('7'), &keymap), None);
        assert_eq!(pending.feed(Key::Escape, &keymap), Some(Action::Abort));
        assert_eq!(
            pending.feed(Key::Char('j'), &keymap),
            Some(Action::Scroll { right: 0, down: 1 })
        );
    }

    #[test]
    fn huge_counts_saturate() {
        assert_eq!(typed("99999999999999G"), [Action::GotoPage(u32::MAX)]);
        assert_eq!(
            typed("99999999999999k"),
            [Action::Scroll {
                right: 0,
                down: -i32::MAX
            }]
        );
    }

    #[test]
    fn control_and_named_keys_scroll() {
        let keymap = Keymap::default();
        let mut pending = Pending::default();
        assert_eq!(
            pending.feed(Key::Ctrl('d'), &keymap),
            Some(Action::HalfScreen(1))
        );
        assert_eq!(pending.feed(Key::Char('2'), &keymap), None);
        assert_eq!(
            pending.feed(Key::Ctrl('u'), &keymap),
            Some(Action::HalfScreen(-2))
        );
        assert_eq!(
            pending.feed(Key::ShiftSpace, &keymap),
            Some(Action::FullScreen(-1))
        );
        assert_eq!(pending.feed(Key::Ctrl('r'), &keymap), Some(Action::Redo));
    }
}
