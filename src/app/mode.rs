//! The mode the reader is in, which the box at the left of the status bar
//! names, in a colour for its kind.

use iced::Color;

use super::{App, Sidebar};
use crate::input::Prompt;
use crate::theme::ModeColours;
use crate::visual::Kind;

/// What keys do now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    Normal,
    /// Just a cursor over the text, after a jump with flash.
    Cursor,
    /// A picture or figure selected in visual mode.
    Figure,
    Visual,
    VisualLine,
    /// Labels on the lines in view, to start selecting at one.
    Select,
    /// Typing text in view to jump to, as flash.nvim's `s`.
    Jump,
    /// Labels on links, fields, and markings, from `f`.
    Follow,
    /// Labels on markings, from `gh`.
    Pick,
    /// Typing into a field of a form.
    Insert,
    Command,
    Search,
    Password,
    /// A question on the bottom line, waiting for yes.
    Confirm,
    Contents,
    /// Every page small, from `g o`.
    Overview,
    Markings,
    Help,
    Recent,
    Tabs,
    /// Every match of the last search, from `g /`.
    Matches,
    Info,
}

impl Mode {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Cursor => "CURSOR",
            Self::Figure => "FIGURE",
            Self::Visual => "VISUAL",
            Self::VisualLine => "V-LINE",
            Self::Select => "SELECT",
            Self::Jump => "JUMP",
            Self::Follow => "FOLLOW",
            Self::Pick => "PICK",
            Self::Insert => "INSERT",
            Self::Command => "COMMAND",
            Self::Search => "SEARCH",
            Self::Password => "PASSWORD",
            Self::Confirm => "CONFIRM",
            Self::Contents => "CONTENTS",
            Self::Overview => "OVERVIEW",
            Self::Markings => "MARKINGS",
            Self::Help => "HELP",
            Self::Recent => "RECENT",
            Self::Tabs => "TABS",
            Self::Matches => "MATCHES",
            Self::Info => "INFO",
        }
    }

    /// The colour of its box: modes of a kind share one.
    pub const fn colour(self, modes: &ModeColours) -> Color {
        match self {
            Self::Normal => modes.normal,
            Self::Cursor
            | Self::Figure
            | Self::Visual
            | Self::VisualLine
            | Self::Select
            | Self::Jump => modes.visual,
            Self::Insert => modes.insert,
            Self::Command | Self::Search | Self::Password | Self::Confirm => modes.command,
            Self::Follow | Self::Pick => modes.label,
            Self::Contents
            | Self::Overview
            | Self::Markings
            | Self::Help
            | Self::Recent
            | Self::Tabs
            | Self::Matches
            | Self::Info => modes.list,
        }
    }
}

impl App {
    /// The mode the reader is in: what takes the next key, in the order
    /// keys are handed out.
    pub(super) fn mode(&self) -> Mode {
        if self.asking.is_some() {
            Mode::Confirm
        } else if self.info.is_some() {
            Mode::Info
        } else if self.field_edit.is_some() {
            Mode::Insert
        } else if self.flash.is_some() {
            Mode::Jump
        } else if self.hints.is_some() {
            Mode::Follow
        } else if let Some(visual) = &self.visual {
            if visual.figure().is_some() {
                return Mode::Figure;
            }
            if !visual.selecting() {
                return Mode::Cursor;
            }
            match visual.kind {
                Kind::Lines => Mode::VisualLine,
                Kind::Characters => Mode::Visual,
            }
        } else if self.line_hints.is_some() {
            Mode::Select
        } else if self.marking_hints.is_some() {
            Mode::Pick
        } else if self.overview().is_some() {
            Mode::Overview
        } else if let Some(sidebar) = &self.sidebar {
            match sidebar {
                Sidebar::Outline(_) => Mode::Contents,
                Sidebar::Markings(_) => Mode::Markings,
            }
        } else if self.help.is_some() {
            Mode::Help
        } else if self.picker.is_some() {
            Mode::Recent
        } else if self.tab_list.is_some() {
            Mode::Tabs
        } else if self.match_list.is_some() {
            Mode::Matches
        } else {
            match self.prompt.as_ref().map(|(prompt, _)| *prompt) {
                Some(Prompt::Command) => Mode::Command,
                Some(Prompt::SearchForward | Prompt::SearchBackward) => Mode::Search,
                Some(Prompt::Password) => Mode::Password,
                None => Mode::Normal,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    #[test]
    fn every_kind_of_mode_has_a_colour_of_its_own_in_every_theme() {
        for (theme, _) in Theme::NAMES {
            let modes = theme.modes();
            let kinds = [
                Mode::Normal,
                Mode::Visual,
                Mode::Insert,
                Mode::Command,
                Mode::Follow,
                Mode::Contents,
            ]
            .map(|mode| mode.colour(&modes));
            for (index, colour) in kinds.iter().enumerate() {
                assert!(
                    !kinds.iter().skip(index + 1).any(|other| other == colour),
                    "{theme:?}"
                );
                assert_ne!(*colour, modes.text, "{theme:?}");
            }
        }
    }
}
