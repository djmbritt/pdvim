//! Key presses, and what they can be bound to in normal mode, at the
//! prompt and when asked a question.
//!
//! A command in normal mode is an optional count, then a sequence of keys
//! bound to a [`Binding`]: `j`, `12G`, `gg`, `zi`. With the count, the
//! binding becomes an [`Action`].

use crate::keymap::bindable;

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

/// What to do with the mark whose letter is typed next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkVerb {
    Set,
    Go,
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
    /// Show the table of contents in a sidebar.
    ShowOutline,
    /// Wait for a letter, then put a mark there, or go to it.
    Mark(MarkVerb),
    /// Go back through the places jumped from, a number of times.
    JumpBack(u32),
    /// Go forward again, a number of times.
    JumpForward(u32),
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

bindable! {
    /// What keys can be bound to in normal mode, reading.
    pub enum Binding {
        ScrollDown = "scroll-down", "Scroll down";
        ScrollUp = "scroll-up", "Scroll up";
        ScrollLeft = "scroll-left", "Scroll left";
        ScrollRight = "scroll-right", "Scroll right";
        ScrollHalfDown = "scroll-half-down", "Scroll half a screen down";
        ScrollHalfUp = "scroll-half-up", "Scroll half a screen up";
        ScrollFullDown = "scroll-full-down", "Scroll a screen down";
        ScrollFullUp = "scroll-full-up", "Scroll a screen up";
        NextPage = "next-page", "Go to the next page";
        PreviousPage = "previous-page", "Go to the previous page";
        /// The first page, or the page given by a count.
        FirstPage = "first-page", "Go to the first page, or page n";
        /// The last page, or the page given by a count.
        LastPage = "last-page", "Go to the last page, or page n";
        PageTop = "page-top", "Go to the top of the page";
        PageBottom = "page-bottom", "Go to the bottom of the page";
        /// Show the table of contents in a sidebar.
        Outline = "outline", "Show the table of contents, to go to a chapter";
        ZoomIn = "zoom-in", "Zoom in";
        ZoomOut = "zoom-out", "Zoom out";
        /// Zoom to the percentage given by a count, or to 100%.
        ZoomTo = "zoom-to", "Zoom to 100%, or to n%";
        /// Zoom to 100%, whatever the count.
        ZoomReset = "zoom-reset", "Zoom to 100%";
        FitWidth = "fit-width", "Fit the page width to the window";
        FitPage = "fit-page", "Fit the whole page in the window";
        Rotate = "rotate", "Rotate";
        ToggleRecolor = "toggle-recolor", "Turn recoloring on or off";
        ToggleTrim = "toggle-trim", "Turn margin trimming on or off";
        SearchForward = "search-forward", "Search forwards";
        SearchBackward = "search-backward", "Search backwards";
        SearchNext = "search-next", "Go to the next match";
        SearchPrevious = "search-previous", "Go to the previous match";
        CommandPrompt = "command-prompt", "Open the command prompt";
        Recent = "recent", "Show recent documents";
        /// Show the key bindings.
        Help = "help", "Show this list";
        /// Label the links in view, to follow one by typing its label.
        FollowLink = "follow-link", "Label the links in view; type a label to follow it";
        /// Label the lines in view, to start selecting characters at one.
        Visual = "visual", "Select text from a line picked by label";
        /// Label the lines in view, to start selecting whole lines at one.
        VisualLine = "visual-line", "Select whole lines from a line picked by label";
        /// Take back the last highlight.
        Undo = "undo", "Take back the last highlight";
        /// Put back the last highlight taken back.
        Redo = "redo", "Put back the last highlight taken back";
        /// The next tab, or the tab given by a count.
        NextTab = "next-tab", "Go to the next tab, or tab n";
        PreviousTab = "previous-tab", "Go to the previous tab";
        /// The letter is the next key: a to z for this document, A to Z
        /// for any.
        SetMark = "set-mark", "Mark this place with the letter typed next";
        GoToMark = "go-to-mark", "Go to the mark typed next; ` or ' for before the last jump";
        JumpBack = "jump-back", "Go back to where the last jump left from, or n jumps";
        JumpForward = "jump-forward", "Go forward again through the jumps";
        Abort = "abort", "Cancel a half-typed command, hide highlights";
        Quit = "quit", "Close the tab, and quit after the last one";
    }
}

impl Binding {
    /// What the binding does, given the count typed before its keys.
    pub fn action(self, count: Option<u32>) -> Action {
        let times = i32::try_from(count.unwrap_or(1)).unwrap_or(i32::MAX);
        let back = times.saturating_neg();
        match self {
            Self::ScrollDown => Action::Scroll {
                right: 0,
                down: times,
            },
            Self::ScrollUp => Action::Scroll {
                right: 0,
                down: back,
            },
            Self::ScrollRight => Action::Scroll {
                right: times,
                down: 0,
            },
            Self::ScrollLeft => Action::Scroll {
                right: back,
                down: 0,
            },
            Self::ScrollHalfDown => Action::HalfScreen(times),
            Self::ScrollHalfUp => Action::HalfScreen(back),
            Self::ScrollFullDown => Action::FullScreen(times),
            Self::ScrollFullUp => Action::FullScreen(back),
            Self::NextPage => Action::Page(times),
            Self::PreviousPage => Action::Page(back),
            Self::FirstPage => count.map_or(Action::GotoFirst, Action::GotoPage),
            Self::LastPage => count.map_or(Action::GotoLast, Action::GotoPage),
            Self::PageTop => Action::PageTop,
            Self::PageBottom => Action::PageBottom,
            Self::ZoomIn => Action::ZoomIn,
            Self::ZoomOut => Action::ZoomOut,
            Self::ZoomTo => Action::ZoomTo(count.unwrap_or(100)),
            Self::ZoomReset => Action::ZoomTo(100),
            Self::FitWidth => Action::FitWidth,
            Self::FitPage => Action::FitPage,
            Self::Rotate => Action::Rotate,
            Self::ToggleRecolor => Action::ToggleRecolor,
            Self::ToggleTrim => Action::ToggleTrim,
            Self::SearchNext => Action::SearchNext,
            Self::SearchPrevious => Action::SearchPrevious,
            Self::SearchForward => Action::OpenPrompt(Prompt::SearchForward),
            Self::SearchBackward => Action::OpenPrompt(Prompt::SearchBackward),
            Self::CommandPrompt => Action::OpenPrompt(Prompt::Command),
            Self::Recent => Action::ShowRecent,
            Self::NextTab => count.map_or(Action::NextTab, Action::GotoTab),
            Self::PreviousTab => Action::PreviousTab(count.unwrap_or(1)),
            Self::Abort => Action::Abort,
            Self::Quit => Action::Quit,
            Self::Help => Action::ShowHelp,
            Self::FollowLink => Action::FollowLink,
            Self::Visual => Action::Visual { lines: false },
            Self::VisualLine => Action::Visual { lines: true },
            Self::Undo => Action::Undo,
            Self::Redo => Action::Redo,
            Self::Outline => Action::ShowOutline,
            Self::SetMark => Action::Mark(MarkVerb::Set),
            Self::GoToMark => Action::Mark(MarkVerb::Go),
            Self::JumpBack => Action::JumpBack(count.unwrap_or(1)),
            Self::JumpForward => Action::JumpForward(count.unwrap_or(1)),
        }
    }
}

bindable! {
    /// What keys can be bound to while typing into the prompt, after `:`,
    /// `/` or `?`. Keys bound to nothing type themselves.
    pub enum PromptBinding {
        Submit = "submit", "Run the command, or search";
        Cancel = "cancel", "Leave the prompt";
        /// Rubbing out the sigil itself leaves the prompt, as in vi.
        RubOut = "rub-out", "Rub out a character; on an empty line, leave";
        RubOutWord = "rub-out-word", "Rub out the word before the cursor";
        ClearLine = "clear-line", "Rub out the whole line";
        Complete = "complete", "Complete a command, or go to the next candidate";
        CompletePrevious = "complete-previous", "Go to the previous candidate";
    }
}

bindable! {
    /// What keys can be bound to when asked a question, such as whether to
    /// quit. Any key bound to nothing is no.
    pub enum ConfirmBinding {
        Yes = "yes", "Go ahead; any other key thinks better of it";
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
    use crate::bindings::Keymaps;
    use crate::keymap::{Counts, Fed, Pending};

    /// Feeds each key in turn and returns the actions that result.
    fn pressed(keys: &[Key]) -> Vec<Action> {
        let keymap = Keymaps::default().normal;
        let mut pending = Pending::default();
        keys.iter()
            .filter_map(|&key| match pending.feed(key, &keymap, Counts::Taken) {
                Fed::Bound(binding, count) => Some(binding.action(count)),
                _ => None,
            })
            .collect()
    }

    /// Feeds each character as a key.
    fn typed(keys: &str) -> Vec<Action> {
        pressed(&keys.chars().map(Key::Char).collect::<Vec<_>>())
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
        assert_eq!(typed("gg"), [Action::GotoFirst]);
        assert_eq!(typed("zi"), [Action::ZoomIn]);
        assert_eq!(typed("z0"), [Action::ZoomTo(100)]);
    }

    #[test]
    fn g_t_moves_between_tabs() {
        assert_eq!(typed("gt"), [Action::NextTab]);
        assert_eq!(typed("3gt"), [Action::GotoTab(3)]);
        assert_eq!(typed("gT"), [Action::PreviousTab(1)]);
        assert_eq!(typed("2gT"), [Action::PreviousTab(2)]);
        assert_eq!(
            pressed(&[
                Key::CtrlTab,
                Key::CtrlShiftTab,
                Key::Char('2'),
                Key::CtrlTab
            ]),
            [Action::NextTab, Action::PreviousTab(1), Action::GotoTab(2)]
        );
    }

    #[test]
    fn marks_and_jumps_have_keys() {
        assert_eq!(typed("m"), [Action::Mark(MarkVerb::Set)]);
        assert_eq!(typed("`"), [Action::Mark(MarkVerb::Go)]);
        assert_eq!(typed("'"), [Action::Mark(MarkVerb::Go)]);
        assert_eq!(
            pressed(&[Key::Char('3'), Key::Ctrl('o'), Key::Ctrl('i')]),
            [Action::JumpBack(3), Action::JumpForward(1)]
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
        assert_eq!(
            pressed(&[
                Key::Ctrl('d'),
                Key::Char('2'),
                Key::Ctrl('u'),
                Key::ShiftSpace,
                Key::Ctrl('r'),
                Key::Char('7'),
                Key::Escape,
            ]),
            [
                Action::HalfScreen(1),
                Action::HalfScreen(-2),
                Action::FullScreen(-1),
                Action::Redo,
                Action::Abort,
            ]
        );
    }
}
