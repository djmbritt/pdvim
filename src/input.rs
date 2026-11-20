//! Turning key presses into actions, the way vi does.
//!
//! A command is an optional count, then one or two keys: `j`, `12G`, `gg`,
//! `zi`. [`Pending`] holds what has been typed so far and yields an
//! [`Action`] once a command is complete.

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
    /// Drop whatever is half-typed and hide search highlights.
    Abort,
    Quit,
}

/// The part of a command typed so far.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pending {
    count: Option<u32>,
    prefix: Option<char>,
}

impl Pending {
    /// What has been typed so far, for the status bar.
    pub fn display(&self) -> String {
        let mut shown = self
            .count
            .map(|count| count.to_string())
            .unwrap_or_default();
        shown.extend(self.prefix);
        shown
    }

    /// Takes the next key. Returns an action once a command is complete;
    /// keys that mean nothing clear what was typed.
    pub fn feed(&mut self, key: Key) -> Option<Action> {
        if let Some(prefix) = self.prefix.take() {
            let action = self.after_prefix(prefix, key);
            self.count = None;
            return action;
        }
        if let Key::Char(digit @ '0'..='9') = key
            && let Some(value) = digit.to_digit(10)
            && (self.count.is_some() || value != 0)
        {
            let count = self.count.unwrap_or(0);
            self.count = Some(count.saturating_mul(10).saturating_add(value));
            return None;
        }
        if let Key::Char(prefix @ ('g' | 'z')) = key {
            self.prefix = Some(prefix);
            return None;
        }
        let count = self.count.take();
        Self::single(key, count)
    }

    /// The action for the key that follows `g` or `z`.
    fn after_prefix(&self, prefix: char, key: Key) -> Option<Action> {
        match (prefix, key) {
            ('g', Key::Char('g')) => Some(self.count.map_or(Action::GotoFirst, Action::GotoPage)),
            ('z', Key::Char('i' | 'I')) => Some(Action::ZoomIn),
            ('z', Key::Char('o' | 'O')) => Some(Action::ZoomOut),
            ('z', Key::Char('0')) => Some(Action::ZoomTo(100)),
            ('z', Key::Char('z' | 'Z')) => Some(Action::ZoomTo(self.count.unwrap_or(100))),
            (_, Key::Escape | Key::Ctrl('c' | '[')) => Some(Action::Abort),
            _ => None,
        }
    }

    /// The action for a key that is a whole command by itself.
    fn single(key: Key, count: Option<u32>) -> Option<Action> {
        let times = i32::try_from(count.unwrap_or(1)).unwrap_or(i32::MAX);
        let back = times.saturating_neg();
        let action = match key {
            Key::Char('j') | Key::Down => Action::Scroll {
                right: 0,
                down: times,
            },
            Key::Char('k') | Key::Up => Action::Scroll {
                right: 0,
                down: back,
            },
            Key::Char('l') | Key::Right => Action::Scroll {
                right: times,
                down: 0,
            },
            Key::Char('h') | Key::Left => Action::Scroll {
                right: back,
                down: 0,
            },
            Key::Ctrl('d') => Action::HalfScreen(times),
            Key::Ctrl('u') => Action::HalfScreen(back),
            Key::Ctrl('f') | Key::Char(' ') => Action::FullScreen(times),
            Key::Ctrl('b') | Key::ShiftSpace => Action::FullScreen(back),
            Key::Char('J') | Key::PageDown => Action::Page(times),
            Key::Char('K') | Key::PageUp => Action::Page(back),
            Key::Char('G') => count.map_or(Action::GotoLast, Action::GotoPage),
            Key::Home => Action::GotoFirst,
            Key::End => Action::GotoLast,
            Key::Char('H') => Action::PageTop,
            Key::Char('L') => Action::PageBottom,
            Key::Char('+') => Action::ZoomIn,
            Key::Char('-') => Action::ZoomOut,
            Key::Char('=') => Action::ZoomTo(count.unwrap_or(100)),
            Key::Char('s') => Action::FitWidth,
            Key::Char('a') => Action::FitPage,
            Key::Char('r') => Action::Rotate,
            Key::Ctrl('r') => Action::ToggleRecolor,
            Key::Char('t') => Action::ToggleTrim,
            Key::Char('n') => Action::SearchNext,
            Key::Char('N') => Action::SearchPrevious,
            Key::Char(':') => Action::OpenPrompt(Prompt::Command),
            Key::Char('/') => Action::OpenPrompt(Prompt::SearchForward),
            Key::Char('?') => Action::OpenPrompt(Prompt::SearchBackward),
            Key::Char('o') => Action::ShowRecent,
            Key::Escape | Key::Ctrl('c' | '[') => Action::Abort,
            Key::Char('q') => Action::Quit,
            _ => return None,
        };
        Some(action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feeds each character as a key and returns the actions that result.
    fn typed(keys: &str) -> Vec<Action> {
        let mut pending = Pending::default();
        keys.chars()
            .filter_map(|key| pending.feed(Key::Char(key)))
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
        let mut pending = Pending::default();
        assert_eq!(pending.feed(Key::Char('g')), None);
        assert_eq!(pending.display(), "g");
        assert_eq!(pending.feed(Key::Char('g')), Some(Action::GotoFirst));
        assert_eq!(pending.display(), "");
        assert_eq!(typed("zi"), [Action::ZoomIn]);
        assert_eq!(typed("zo"), [Action::ZoomOut]);
        assert_eq!(typed("z0"), [Action::ZoomTo(100)]);
    }

    #[test]
    fn a_leading_zero_is_not_a_count() {
        let mut pending = Pending::default();
        assert_eq!(pending.feed(Key::Char('0')), None);
        assert_eq!(pending.display(), "");
        assert_eq!(typed("10j"), [Action::Scroll { right: 0, down: 10 }]);
    }

    #[test]
    fn unknown_keys_and_escape_clear_what_was_typed() {
        let mut pending = Pending::default();
        assert_eq!(pending.feed(Key::Char('4')), None);
        assert_eq!(pending.feed(Key::Char('g')), None);
        assert_eq!(pending.display(), "4g");
        assert_eq!(pending.feed(Key::Char('x')), None);
        assert_eq!(pending.display(), "");

        assert_eq!(pending.feed(Key::Char('7')), None);
        assert_eq!(pending.feed(Key::Escape), Some(Action::Abort));
        assert_eq!(
            pending.feed(Key::Char('j')),
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
        let mut pending = Pending::default();
        assert_eq!(pending.feed(Key::Ctrl('d')), Some(Action::HalfScreen(1)));
        assert_eq!(pending.feed(Key::Char('2')), None);
        assert_eq!(pending.feed(Key::Ctrl('u')), Some(Action::HalfScreen(-2)));
        assert_eq!(pending.feed(Key::ShiftSpace), Some(Action::FullScreen(-1)));
        assert_eq!(pending.feed(Key::Ctrl('r')), Some(Action::ToggleRecolor));
    }
}
