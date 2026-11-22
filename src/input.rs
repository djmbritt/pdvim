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
    /// A function key: 1 for F1, up to 12.
    Function(u8),
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
    /// The password to unlock a document, shown as dots.
    Password,
}

impl Prompt {
    /// The character shown in front of the typed text.
    pub const fn sigil(self) -> char {
        match self {
            Self::Command => ':',
            Self::SearchForward => '/',
            Self::SearchBackward => '?',
            Self::Password => ' ',
        }
    }
}

/// Which field of a form to go to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldPick {
    /// The first in view, or the next below it.
    First,
    /// The last in view, or the one before it above.
    Last,
    /// The one last gone to.
    Again,
}

/// What to do with the link picked by its label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkGoal {
    /// Follow it; fields and markings are labelled too.
    Follow,
    /// Open a place in the document in a new tab.
    NewTab,
    /// Copy where it goes.
    Copy,
    /// Copy a picture or figure, labelled instead of the links.
    Picture,
}

/// What to copy about the document in view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyWhat {
    /// The path of its file.
    Path,
    /// Its title, or its file's name.
    Title,
    /// The text of the page in view.
    Page,
}

/// What to do with the mark whose letter is typed next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkVerb {
    Set,
    Go,
}

/// What to do with the highlights and other markings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkingAction {
    /// Go to the next marking, or the previous, a number of times over.
    Step { forward: bool, times: u32 },
    /// Take away the marking last gone to.
    Delete,
    /// Change the note on the marking last gone to.
    ChangeNote,
    /// Copy the text of the marking last gone to.
    Yank,
    /// Label the markings in view, to pick one by its label.
    Label,
    /// List them in a sidebar.
    List,
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
    /// Hide the tab bar and status bar, or show them.
    ToggleBars,
    /// Show pages two side by side, or one to a row again.
    ToggleColumns,
    /// Show pages in a column, a page at a time, or two at a time, in turn.
    CyclePageMode,
    /// Draw a line round every page, or stop.
    ToggleBorders,
    /// Fill the screen with the window, or stop.
    ToggleFullscreen,
    SearchNext,
    SearchPrevious,
    OpenPrompt(Prompt),
    /// Show the recently read documents, to pick one to open.
    ShowRecent,
    /// Show the open tabs, to pick one to go to.
    ShowTabs,
    /// List every match of the last search, with its line.
    ShowMatches,
    /// Show the key bindings.
    ShowHelp,
    /// Show what the document says about itself.
    ShowInfo,
    /// Save the changes into the file, like `:w`.
    Write,
    /// Go to the start of the next chapter or section, or the previous, a
    /// number of times over.
    Chapter {
        forward: bool,
        times: u32,
    },
    /// Bring the start of the next paragraph, or the previous, to the top
    /// of the window, a number of times over, as vim's `}` and `{`.
    Paragraph {
        forward: bool,
        times: u32,
    },
    /// Label the links in view, to follow one, open it in a new tab, or
    /// copy it.
    FollowLink(LinkGoal),
    /// Label the lines in view, to start selecting text at one: whole lines
    /// or characters.
    Visual {
        lines: bool,
    },
    /// Ask for text in view to start selecting at, as flash.nvim's `s`.
    Flash,
    /// Select the last selection again, as vim's `gv`.
    SelectAgain,
    /// Go this far through the document, in percent, as vim's `50%`; with
    /// no count, nowhere.
    GotoPercent(Option<u32>),
    /// Go to a field of a form, to fill it in.
    Field(FieldPick),
    /// Copy something about the document in view.
    Copy(CopyWhat),
    /// Show the table of contents in a sidebar.
    ShowOutline,
    /// Show every page small, side by side, to pick one.
    ShowOverview,
    /// Wait for a letter, then put a mark there, or go to it.
    Mark(MarkVerb),
    /// Go back through the places jumped from, a number of times.
    JumpBack(u32),
    /// Go forward again, a number of times.
    JumpForward(u32),
    /// Go between the markings, take one away, or list them.
    Marking(MarkingAction),
    /// Take back the last change to the markings.
    Undo,
    /// Put back the last change taken back.
    Redo,
    /// Go to the next tab, or the first after the last.
    NextTab,
    /// Go back a number of tabs, round to the last from the first.
    PreviousTab(u32),
    /// Go to a tab, counted from 1.
    GotoTab(u32),
    /// Open again the tab closed last, a number of times over.
    ReopenTab(u32),
    /// Run the last command line again, a number of times over.
    Repeat(u32),
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
        /// As vim's `%` with a count.
        Percent = "percent", "Go n percent of the way through the document";
        PageTop = "page-top", "Go to the top of the page";
        PageBottom = "page-bottom", "Go to the bottom of the page";
        NextChapter = "next-chapter", "Go to the next chapter or section";
        PreviousChapter = "previous-chapter", "Go to the previous chapter or section";
        /// As vim's `}`.
        NextParagraph = "next-paragraph", "Bring the next paragraph to the top of the window";
        /// As vim's `{`.
        PreviousParagraph = "previous-paragraph", "Bring the paragraph above to the top of the window";
        /// Show the table of contents in a sidebar.
        Outline = "outline", "Show the table of contents, to go to a chapter";
        Overview = "overview", "Show every page small, side by side, to go to one";
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
        ToggleBars = "toggle-bars", "Hide the tab bar and status bar, or show them";
        ToggleColumns = "toggle-columns", "Show two pages side by side, or one";
        CyclePageMode = "cycle-page-mode", "Show pages continuously, one at a time, or two at a time";
        ToggleBorders = "toggle-borders", "Draw a line round every page, or stop";
        ToggleFullscreen = "toggle-fullscreen", "Fill the screen, or stop";
        SearchForward = "search-forward", "Search forwards";
        SearchBackward = "search-backward", "Search backwards";
        SearchNext = "search-next", "Go to the next match";
        SearchPrevious = "search-previous", "Go to the previous match";
        CommandPrompt = "command-prompt", "Open the command prompt";
        /// The last line typed after `:`, or bound to keys.
        Repeat = "repeat", "Run the last command again; with a count, n times";
        Recent = "recent", "Show recent documents";
        /// Show the key bindings.
        Help = "help", "Show this list";
        /// Label the links in view, to follow one by typing its label.
        FollowLink = "follow-link", "Label the links and markings in view; type a label to follow or pick one";
        /// As Tridactyl's `F`.
        FollowLinkTab = "follow-link-tab", "Label the links in view; type a label to open its place in a new tab";
        /// As Tridactyl's `;y`.
        YankLink = "yank-link", "Label the links in view; type a label to copy where it goes";
        /// Label the lines in view, to start selecting characters at one.
        Visual = "visual", "Select text from a line picked by label";
        /// Label the lines in view, to start selecting whole lines at one.
        VisualLine = "visual-line", "Select whole lines from a line picked by label";
        /// As flash.nvim's `s`.
        Flash = "flash", "Type text in view, then its label, to start selecting there";
        /// As vim's `gv`.
        SelectAgain = "select-again", "Select the last selection again";
        /// As `gi` in Vimium: tab then goes from field to field.
        FirstField = "first-field", "Fill in the first field of a form in view; tab goes to the next";
        LastField = "last-field", "Fill in the last field of a form in view";
        /// The field gone to last, with gi, gI, f or tab.
        FieldAgain = "field-again", "Go back to the field of a form last gone to";
        NextMarking = "next-marking", "Go to the next highlight or other marking";
        PreviousMarking = "previous-marking", "Go to the previous highlight or other marking";
        /// The marking last gone to with next-marking or previous-marking.
        DeleteMarking = "delete-marking", "Take away the marking last gone to";
        /// The text of a note, or the comment on another kind of marking.
        ChangeNote = "change-note", "Change the note on the marking gone to";
        /// The text a highlight or other marking is over, or a note's text.
        YankMarking = "yank-marking", "Copy the text of the marking gone to";
        /// Like `f` for links: a label on each marking in view.
        LabelMarkings = "label-markings", "Label the markings in view; type a label to pick one";
        Markings = "markings", "List the highlights and other markings, to go to one";
        /// Take back the last change to the markings.
        Undo = "undo", "Take back the last marking, or taking one away";
        /// Put back the last change taken back.
        Redo = "redo", "Put back what undo took back";
        /// The next tab, or the tab given by a count.
        NextTab = "next-tab", "Go to the next tab, or tab n";
        PreviousTab = "previous-tab", "Go to the previous tab";
        /// As Tridactyl's `b`.
        Tabs = "tabs", "List the open tabs; type to narrow, enter to go to one";
        /// As vim's quickfix list.
        Matches = "matches", "List every match of the last search; type to narrow, enter to go to one";
        /// As Tridactyl's `u`, which here takes back markings.
        ReopenTab = "reopen-tab", "Open the tab closed last again, where it was";
        /// The letter is the next key: a to z for this document, A to Z
        /// for any.
        SetMark = "set-mark", "Mark this place with the letter typed next";
        GoToMark = "go-to-mark", "Go to the mark typed next; ` or ' for before the last jump";
        JumpBack = "jump-back", "Go back to where the last jump left from, or n jumps";
        JumpForward = "jump-forward", "Go forward again through the jumps";
        /// Like `:w`.
        Write = "write", "Save the highlights and other changes into the file";
        /// As Tridactyl's `yy`.
        YankPath = "yank-path", "Copy the path of the document's file";
        YankTitle = "yank-title", "Copy the document's title, or its file's name";
        YankPage = "yank-page", "Copy the text of the page in view";
        YankPicture = "yank-picture", "Label the pictures and figures in view; type a label to copy one";
        /// Like `:info`.
        Info = "info", "Show the document's title, author, dates and more";
        Abort = "abort", "Cancel a half-typed command, hide highlights";
        Quit = "quit", "Close the tab, and quit after the last one";
    }
}

/// Scrolling by so many steps right and down; left and up are negative.
const fn scroll(right: i32, down: i32) -> Action {
    Action::Scroll { right, down }
}

impl Binding {
    /// What the binding does, given the count typed before its keys.
    pub fn action(self, count: Option<u32>) -> Action {
        let times = i32::try_from(count.unwrap_or(1)).unwrap_or(i32::MAX);
        let back = times.saturating_neg();
        match self {
            Self::ScrollDown => scroll(0, times),
            Self::ScrollUp => scroll(0, back),
            Self::ScrollRight => scroll(times, 0),
            Self::ScrollLeft => scroll(back, 0),
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
            Self::ToggleBars => Action::ToggleBars,
            Self::ToggleColumns => Action::ToggleColumns,
            Self::CyclePageMode => Action::CyclePageMode,
            Self::ToggleBorders => Action::ToggleBorders,
            Self::ToggleFullscreen => Action::ToggleFullscreen,
            Self::SearchNext => Action::SearchNext,
            Self::SearchPrevious => Action::SearchPrevious,
            Self::SearchForward => Action::OpenPrompt(Prompt::SearchForward),
            Self::SearchBackward => Action::OpenPrompt(Prompt::SearchBackward),
            Self::CommandPrompt => Action::OpenPrompt(Prompt::Command),
            Self::Repeat => Action::Repeat(count.unwrap_or(1)),
            Self::Recent => Action::ShowRecent,
            Self::Tabs => Action::ShowTabs,
            Self::Matches => Action::ShowMatches,
            Self::NextTab => count.map_or(Action::NextTab, Action::GotoTab),
            Self::PreviousTab => Action::PreviousTab(count.unwrap_or(1)),
            Self::ReopenTab => Action::ReopenTab(count.unwrap_or(1)),
            Self::Abort => Action::Abort,
            Self::Quit => Action::Quit,
            Self::Help => Action::ShowHelp,
            Self::Info => Action::ShowInfo,
            Self::Write => Action::Write,
            Self::NextChapter => Action::Chapter {
                forward: true,
                times: count.unwrap_or(1),
            },
            Self::PreviousChapter => Action::Chapter {
                forward: false,
                times: count.unwrap_or(1),
            },
            Self::NextParagraph | Self::PreviousParagraph => Action::Paragraph {
                forward: self == Self::NextParagraph,
                times: count.unwrap_or(1),
            },
            Self::FollowLink => Action::FollowLink(LinkGoal::Follow),
            Self::FollowLinkTab => Action::FollowLink(LinkGoal::NewTab),
            Self::YankLink => Action::FollowLink(LinkGoal::Copy),
            Self::YankPicture => Action::FollowLink(LinkGoal::Picture),
            Self::Visual => Action::Visual { lines: false },
            Self::VisualLine => Action::Visual { lines: true },
            Self::Flash => Action::Flash,
            Self::SelectAgain => Action::SelectAgain,
            Self::Percent => Action::GotoPercent(count),
            Self::FirstField => Action::Field(FieldPick::First),
            Self::LastField => Action::Field(FieldPick::Last),
            Self::FieldAgain => Action::Field(FieldPick::Again),
            Self::NextMarking => Action::Marking(MarkingAction::Step {
                forward: true,
                times: count.unwrap_or(1),
            }),
            Self::PreviousMarking => Action::Marking(MarkingAction::Step {
                forward: false,
                times: count.unwrap_or(1),
            }),
            Self::DeleteMarking => Action::Marking(MarkingAction::Delete),
            Self::Markings => Action::Marking(MarkingAction::List),
            Self::ChangeNote => Action::Marking(MarkingAction::ChangeNote),
            Self::YankMarking => Action::Marking(MarkingAction::Yank),
            Self::YankPath => Action::Copy(CopyWhat::Path),
            Self::YankTitle => Action::Copy(CopyWhat::Title),
            Self::YankPage => Action::Copy(CopyWhat::Page),
            Self::LabelMarkings => Action::Marking(MarkingAction::Label),
            Self::Undo => Action::Undo,
            Self::Redo => Action::Redo,
            Self::Outline => Action::ShowOutline,
            Self::Overview => Action::ShowOverview,
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
        Paste = "paste", "Paste the text on the clipboard";
        Older = "older", "Recall an earlier line that starts as this one";
        Newer = "newer", "Recall a later line, at last the one being typed";
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
            Self::Function(number) => write!(formatter, "f{number}"),
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

    #[test]
    fn the_manual_names_every_action_of_normal_mode() {
        use crate::keymap::Bindable;
        // In roff a hyphen is written `\-`.
        let manual = include_str!("../docs/man/pdvim.5").replace("\\-", "-");
        for (_, name, _) in Binding::ALL {
            // Line by line, as Windows checks the file out with \r\n.
            let named = manual
                .lines()
                .any(|line| line.trim_end() == format!(".B {name}"));
            assert!(named, "docs/man/pdvim.5 does not describe {name}");
        }
    }
}
