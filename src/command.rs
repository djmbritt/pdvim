//! The commands typed after `:`.

use std::path::PathBuf;

use crate::bindings::{Bound, Mode};
use crate::input::{Binding, Key};
use crate::keymap::{self, Bindable};
use crate::recolor::{InvalidColour, Rgb};
use crate::theme::Theme;

/// A command, parsed.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Open a document in a new tab.
    Open(PathBuf),
    /// Show the recently read documents, to pick one to open.
    Recent,
    /// Show the key bindings.
    Help,
    /// Open a new tab, with a document in it or empty.
    TabOpen(Option<PathBuf>),
    /// Close the tab in view, unless it is the last.
    TabClose,
    /// Go to the next tab, or to a tab counted from 1.
    TabNext(Option<u32>),
    /// Go to the previous tab.
    TabPrevious,
    /// Go to a page, counted from 1.
    Goto(u32),
    /// Change a setting.
    Set(Setting),
    /// Hide the search highlights.
    NoHighlight,
    /// Bind a key sequence, in the mode of what it is bound to.
    Map(Vec<Key>, Bound),
    /// Unbind a key sequence in a mode.
    Unmap(Mode, Vec<Key>),
    /// Unbind every key sequence in a mode.
    MapClear(Mode),
    /// Do what a key binding does, as if its keys were pressed after a
    /// count.
    Run(Binding, Option<u32>),
    /// Close the tab in view, and the program with its last tab.
    Quit,
    /// Close every tab and the program.
    QuitAll,
}

/// A setting and its new value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Setting {
    /// Whether pages are recoloured.
    Recolor(Recolor),
    /// Whether the blank margins of pages are hidden.
    Trim(bool),
    /// What light colours such as white paper become when recoloured, or
    /// the theme's colour. Named `recolor-lightcolor`, as in Zathura.
    RecolorPaper(Option<Rgb>),
    /// What dark colours such as black ink become when recoloured, or the
    /// theme's colour. Named `recolor-darkcolor`, as in Zathura.
    RecolorInk(Option<Rgb>),
    /// The theme, or none to follow the system's: `theme-dark` while it is
    /// dark, `theme-light` while it is light.
    Theme(Option<Theme>),
    /// The theme while the system's is dark, when following it.
    ThemeDark(Theme),
    /// The theme while the system's is light, when following it.
    ThemeLight(Theme),
    /// How far one press of `j` scrolls, in pixels.
    ScrollStep(f32),
    /// Whether to ask before quitting.
    ConfirmQuit(bool),
    /// Whether to ask before closing a tab, when others are left.
    ConfirmTabClose(bool),
    /// The colour text is highlighted in.
    HighlightColour(Rgb),
}

/// Whether pages are recoloured for a dark background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recolor {
    On,
    Off,
    /// Recoloured while the system prefers a dark theme, and not while it
    /// prefers a light one.
    Auto,
}

/// Why a command could not be understood.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("not a command: {0}")]
    Unknown(String),
    #[error("{0} needs an argument")]
    MissingArgument(&'static str),
    #[error("unknown setting: {0}")]
    UnknownSetting(String),
    #[error("`{0}` is not true or false")]
    NotBoolean(String),
    #[error("`{0}` is not true, false or auto")]
    NotRecolor(String),
    #[error("unknown theme: {0}")]
    UnknownTheme(String),
    #[error("`{0}` is not a positive number")]
    NotPositive(String),
    #[error(transparent)]
    Colour(#[from] InvalidColour),
    #[error(transparent)]
    Keys(#[from] keymap::Error),
}

/// Every command by its full name, and the shorter names it also goes by.
const NAMES: [(&str, &[&str]); 15] = [
    ("help", &["h"]),
    ("map", &[]),
    ("mapclear", &[]),
    ("nohlsearch", &["noh", "nohl"]),
    ("open", &["o"]),
    ("qall", &["qa", "quita", "quitall"]),
    ("quit", &["q"]),
    ("recent", &["rec"]),
    ("set", &[]),
    ("tabclose", &["tabc"]),
    ("tabedit", &["tabe"]),
    ("tabnew", &[]),
    ("tabnext", &["tabn"]),
    ("tabprevious", &["tabp", "tabN", "tabNext"]),
    ("unmap", &[]),
];

/// The full names of the commands, in alphabetical order.
pub fn names() -> impl Iterator<Item = &'static str> {
    NAMES.iter().map(|(name, _)| *name)
}

/// The full name of a command, given that or a shorter one.
pub fn canonical(name: &str) -> Option<&'static str> {
    NAMES
        .iter()
        .find(|(full, short)| *full == name || short.contains(&name))
        .map(|(full, _)| *full)
}

/// The names of the settings `:set` knows.
pub const SETTINGS: [&str; 11] = [
    "confirm-quit",
    "confirm-tab-close",
    "highlight-color",
    "recolor",
    "recolor-darkcolor",
    "recolor-lightcolor",
    "scroll-step",
    "theme",
    "theme-dark",
    "theme-light",
    "trim",
];

/// Parses the text typed after `:`. Blank input is no command at all.
pub fn parse(line: &str) -> Result<Option<Command>, Error> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(None);
    }
    if let Ok(page) = line.parse::<u32>() {
        return Ok(Some(Command::Goto(page)));
    }
    let (name, argument) = line
        .split_once(char::is_whitespace)
        .map_or((line, ""), |(name, rest)| (name, rest.trim()));
    // Any action can be run by name, with a count after it.
    let Some(canonical) = canonical(name) else {
        let binding = Binding::named(name).ok_or_else(|| Error::Unknown(name.to_owned()))?;
        let count = if argument.is_empty() {
            None
        } else {
            match argument.parse::<u32>() {
                Ok(count) if count > 0 => Some(count),
                _ => return Err(Error::NotPositive(argument.to_owned())),
            }
        };
        return Ok(Some(Command::Run(binding, count)));
    };
    let command = match canonical {
        "quit" => Command::Quit,
        "qall" => Command::QuitAll,
        "tabedit" | "tabnew" => {
            Command::TabOpen((!argument.is_empty()).then(|| expand_home(argument)))
        }
        "tabclose" => Command::TabClose,
        "tabnext" => {
            if argument.is_empty() {
                Command::TabNext(None)
            } else {
                match argument.parse::<u32>() {
                    Ok(tab) if tab > 0 => Command::TabNext(Some(tab)),
                    _ => return Err(Error::NotPositive(argument.to_owned())),
                }
            }
        }
        "tabprevious" => Command::TabPrevious,
        "open" => {
            if argument.is_empty() {
                return Err(Error::MissingArgument("open"));
            }
            Command::Open(expand_home(argument))
        }
        "recent" => Command::Recent,
        "help" => Command::Help,
        "nohlsearch" => Command::NoHighlight,
        "set" => Command::Set(parse_setting(argument)?),
        // The keys may have spaces between them, so the action is the
        // last word.
        "map" => {
            let (mode, argument) = mode(argument);
            let (keys, action) = argument
                .rsplit_once(char::is_whitespace)
                .ok_or(Error::MissingArgument("map"))?;
            let bound = mode
                .bound(action)
                .ok_or_else(|| keymap::Error::UnknownAction(action.to_owned()))?;
            Command::Map(keymap::parse_keys(keys)?, bound)
        }
        "unmap" => {
            let (mode, argument) = mode(argument);
            if argument.is_empty() {
                return Err(Error::MissingArgument("unmap"));
            }
            Command::Unmap(mode, keymap::parse_keys(argument)?)
        }
        "mapclear" => match argument {
            "" => Command::MapClear(Mode::Normal),
            name => Command::MapClear(
                Mode::named(name).ok_or_else(|| keymap::Error::UnknownMode(name.to_owned()))?,
            ),
        },
        _ => return Err(Error::Unknown(name.to_owned())),
    };
    Ok(Some(command))
}

/// The mode named at the start of the arguments to `:map` and `:unmap`,
/// or normal mode if none is, and the arguments after it. No mode's name
/// is also a key's.
fn mode(argument: &str) -> (Mode, &str) {
    argument
        .split_once(char::is_whitespace)
        .and_then(|(first, rest)| Some((Mode::named(first)?, rest.trim_start())))
        .unwrap_or((Mode::Normal, argument))
}

/// Parses the `name value` that follows `:set`. A boolean setting named
/// alone is switched on.
fn parse_setting(argument: &str) -> Result<Setting, Error> {
    let (name, value) = argument
        .split_once(char::is_whitespace)
        .map_or((argument, ""), |(name, rest)| (name, rest.trim()));
    setting(name, value)
}

/// Parses a setting given by name, with its value as text.
pub fn setting(name: &str, value: &str) -> Result<Setting, Error> {
    match name {
        "" => Err(Error::MissingArgument("set")),
        "recolor" => match value {
            "auto" => Ok(Setting::Recolor(Recolor::Auto)),
            value => match parse_switch(value) {
                Ok(true) => Ok(Setting::Recolor(Recolor::On)),
                Ok(false) => Ok(Setting::Recolor(Recolor::Off)),
                Err(_) => Err(Error::NotRecolor(value.to_owned())),
            },
        },
        "trim" => parse_switch(value).map(Setting::Trim),
        "recolor-darkcolor" => Ok(Setting::RecolorInk(themed(value)?)),
        "recolor-lightcolor" => Ok(Setting::RecolorPaper(themed(value)?)),
        "theme" if value == "auto" => Ok(Setting::Theme(None)),
        "theme" => theme(value).map(|theme| Setting::Theme(Some(theme))),
        "theme-dark" => theme(value).map(Setting::ThemeDark),
        "theme-light" => theme(value).map(Setting::ThemeLight),
        "confirm-quit" => parse_switch(value).map(Setting::ConfirmQuit),
        "confirm-tab-close" => parse_switch(value).map(Setting::ConfirmTabClose),
        "highlight-color" => Ok(Setting::HighlightColour(value.parse()?)),
        "scroll-step" => match value.parse::<f32>() {
            Ok(step) if step.is_finite() && step > 0.0 => Ok(Setting::ScrollStep(step)),
            _ => Err(Error::NotPositive(value.to_owned())),
        },
        other => Err(Error::UnknownSetting(other.to_owned())),
    }
}

/// Parses a colour, or `theme` for the theme's.
fn themed(value: &str) -> Result<Option<Rgb>, Error> {
    if value == "theme" {
        return Ok(None);
    }
    Ok(Some(value.parse()?))
}

/// Parses the name of a theme.
fn theme(value: &str) -> Result<Theme, Error> {
    Theme::named(value).ok_or_else(|| Error::UnknownTheme(value.to_owned()))
}

/// Parses the value of a boolean setting. Named alone, it is switched on.
fn parse_switch(value: &str) -> Result<bool, Error> {
    match value {
        "" | "true" => Ok(true),
        "false" => Ok(false),
        other => Err(Error::NotBoolean(other.to_owned())),
    }
}

/// Replaces a leading `~` with the home directory, when there is one.
fn expand_home(path: &str) -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match (path.strip_prefix('~'), home) {
        (Some(""), Some(home)) => home,
        (Some(rest), Some(home)) if rest.starts_with('/') => {
            home.join(rest.trim_start_matches('/'))
        }
        _ => PathBuf::from(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_input_is_nothing() {
        assert_eq!(parse("   "), Ok(None));
    }

    #[test]
    fn a_number_goes_to_that_page() {
        assert_eq!(parse(" 42 "), Ok(Some(Command::Goto(42))));
    }

    #[test]
    fn open_takes_a_path_with_spaces() {
        assert_eq!(
            parse("open /tmp/some file.pdf"),
            Ok(Some(Command::Open(PathBuf::from("/tmp/some file.pdf"))))
        );
        assert_eq!(parse("open"), Err(Error::MissingArgument("open")));
    }

    #[test]
    fn settings_parse() {
        assert_eq!(
            parse("set recolor"),
            Ok(Some(Command::Set(Setting::Recolor(Recolor::On))))
        );
        assert_eq!(
            parse("set recolor false"),
            Ok(Some(Command::Set(Setting::Recolor(Recolor::Off))))
        );
        assert_eq!(
            parse("set recolor auto"),
            Ok(Some(Command::Set(Setting::Recolor(Recolor::Auto))))
        );
        assert_eq!(
            parse("set recolor-darkcolor #102030"),
            Ok(Some(Command::Set(Setting::RecolorInk(Some(Rgb(
                0x10, 0x20, 0x30
            ))))))
        );
        assert_eq!(
            parse("set recolor-darkcolor theme"),
            Ok(Some(Command::Set(Setting::RecolorInk(None))))
        );
        assert_eq!(
            parse("set theme catppuccin-frappe"),
            Ok(Some(Command::Set(Setting::Theme(Some(
                Theme::CatppuccinFrappe
            )))))
        );
        assert_eq!(
            parse("set theme auto"),
            Ok(Some(Command::Set(Setting::Theme(None))))
        );
        assert_eq!(
            parse("set theme-light pdvim-light"),
            Ok(Some(Command::Set(Setting::ThemeLight(Theme::PdvimLight))))
        );
        assert_eq!(
            parse("set theme-dark auto"),
            Err(Error::UnknownTheme("auto".into()))
        );
        assert_eq!(
            parse("set trim"),
            Ok(Some(Command::Set(Setting::Trim(true))))
        );
        assert_eq!(
            parse("set trim false"),
            Ok(Some(Command::Set(Setting::Trim(false))))
        );
        assert_eq!(
            parse("set confirm-quit"),
            Ok(Some(Command::Set(Setting::ConfirmQuit(true))))
        );
        assert_eq!(
            parse("set confirm-tab-close false"),
            Ok(Some(Command::Set(Setting::ConfirmTabClose(false))))
        );
        assert_eq!(
            parse("set scroll-step 60"),
            Ok(Some(Command::Set(Setting::ScrollStep(60.0))))
        );
    }

    #[test]
    fn bad_input_is_explained() {
        assert_eq!(
            parse("frobnicate"),
            Err(Error::Unknown("frobnicate".into()))
        );
        assert_eq!(parse("set"), Err(Error::MissingArgument("set")));
        assert_eq!(
            parse("set colour red"),
            Err(Error::UnknownSetting("colour".into()))
        );
        assert_eq!(
            parse("set recolor maybe"),
            Err(Error::NotRecolor("maybe".into()))
        );
        assert_eq!(
            parse("set scroll-step -3"),
            Err(Error::NotPositive("-3".into()))
        );
        assert!(matches!(
            parse("set recolor-lightcolor red"),
            Err(Error::Colour(_))
        ));
    }

    #[test]
    fn actions_run_by_name_with_a_count() {
        assert_eq!(
            parse("scroll-down"),
            Ok(Some(Command::Run(Binding::ScrollDown, None)))
        );
        assert_eq!(
            parse("zoom-to 150"),
            Ok(Some(Command::Run(Binding::ZoomTo, Some(150))))
        );
        assert_eq!(parse("zoom-to x"), Err(Error::NotPositive("x".into())));
        // A command of the same name comes first.
        assert_eq!(parse("quit"), Ok(Some(Command::Quit)));
        assert_eq!(parse("recent"), Ok(Some(Command::Recent)));
    }

    #[test]
    fn every_listed_setting_is_known() {
        for name in SETTINGS {
            assert!(
                !matches!(setting(name, ""), Err(Error::UnknownSetting(_))),
                "{name}"
            );
        }
    }

    #[test]
    fn every_command_name_parses() {
        for name in names() {
            assert!(!matches!(parse(name), Err(Error::Unknown(_))), "{name}");
        }
    }

    #[test]
    fn quit_and_nohlsearch_have_short_forms() {
        assert_eq!(parse("q"), Ok(Some(Command::Quit)));
        assert_eq!(parse("quit"), Ok(Some(Command::Quit)));
        assert_eq!(parse("noh"), Ok(Some(Command::NoHighlight)));
        assert_eq!(parse("rec"), Ok(Some(Command::Recent)));
        assert_eq!(parse("recent"), Ok(Some(Command::Recent)));
        assert_eq!(parse("qa"), Ok(Some(Command::QuitAll)));
    }

    #[test]
    fn map_commands_parse() {
        assert_eq!(
            parse("map ctrl+n next-tab"),
            Ok(Some(Command::Map(
                vec![Key::Ctrl('n')],
                Bound::Normal(Binding::NextTab)
            )))
        );
        assert_eq!(
            parse("map  g x   quit "),
            Ok(Some(Command::Map(
                vec![Key::Char('g'), Key::Char('x')],
                Bound::Normal(Binding::Quit)
            )))
        );
        assert_eq!(
            parse("unmap g g"),
            Ok(Some(Command::Unmap(
                Mode::Normal,
                vec![Key::Char('g'), Key::Char('g')]
            )))
        );
        assert_eq!(parse("mapclear"), Ok(Some(Command::MapClear(Mode::Normal))));
        assert_eq!(parse("map j"), Err(Error::MissingArgument("map")));
        assert_eq!(parse("unmap"), Err(Error::MissingArgument("unmap")));
        assert_eq!(
            parse("map j fly"),
            Err(Error::Keys(keymap::Error::UnknownAction("fly".into())))
        );
        assert_eq!(
            parse("map nope quit"),
            Err(Error::Keys(keymap::Error::UnknownKey("nope".into())))
        );
        assert_eq!(
            parse("mapclear now"),
            Err(Error::Keys(keymap::Error::UnknownMode("now".into())))
        );
    }

    #[test]
    fn map_commands_take_a_mode_first() {
        assert_eq!(
            parse("map visual Y yank"),
            Ok(Some(Command::Map(
                vec![Key::Char('Y')],
                Bound::Visual(crate::visual::Binding::Yank)
            )))
        );
        assert_eq!(
            parse("unmap outline q"),
            Ok(Some(Command::Unmap(Mode::Outline, vec![Key::Char('q')])))
        );
        assert_eq!(
            parse("mapclear prompt"),
            Ok(Some(Command::MapClear(Mode::Prompt)))
        );
        // Each mode has its own actions.
        assert_eq!(
            parse("map visual j scroll-down"),
            Err(Error::Keys(keymap::Error::UnknownAction(
                "scroll-down".into()
            )))
        );
        // `visual` is also an action in normal mode: a mode is only taken
        // when something follows it.
        assert_eq!(
            parse("map v visual"),
            Ok(Some(Command::Map(
                vec![Key::Char('v')],
                Bound::Normal(Binding::Visual)
            )))
        );
    }

    #[test]
    fn tab_commands_parse() {
        assert_eq!(parse("tabnew"), Ok(Some(Command::TabOpen(None))));
        assert_eq!(
            parse("tabe /tmp/a b.pdf"),
            Ok(Some(Command::TabOpen(Some(PathBuf::from("/tmp/a b.pdf")))))
        );
        assert_eq!(parse("tabc"), Ok(Some(Command::TabClose)));
        assert_eq!(parse("tabn"), Ok(Some(Command::TabNext(None))));
        assert_eq!(parse("tabn 2"), Ok(Some(Command::TabNext(Some(2)))));
        assert_eq!(parse("tabn 0"), Err(Error::NotPositive("0".into())));
        assert_eq!(parse("tabp"), Ok(Some(Command::TabPrevious)));
    }
}
