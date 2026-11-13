//! The commands typed after `:`.

use std::path::PathBuf;

use crate::recolor::{InvalidColour, Rgb};

/// A command, parsed.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Open a document.
    Open(PathBuf),
    /// Go to a page, counted from 1.
    Goto(u32),
    /// Change a setting.
    Set(Setting),
    /// Hide the search highlights.
    NoHighlight,
    Quit,
}

/// A setting and its new value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Setting {
    /// Whether pages are recoloured.
    Recolor(bool),
    /// Whether the blank margins of pages are hidden.
    Trim(bool),
    /// What light colours such as white paper become when recoloured. Named
    /// `recolor-lightcolor`, as in Zathura.
    RecolorPaper(Rgb),
    /// What dark colours such as black ink become when recoloured. Named
    /// `recolor-darkcolor`, as in Zathura.
    RecolorInk(Rgb),
    /// How far one press of `j` scrolls, in pixels.
    ScrollStep(f32),
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
    #[error("`{0}` is not a positive number")]
    NotPositive(String),
    #[error(transparent)]
    Colour(#[from] InvalidColour),
}

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
    let command = match name {
        "q" | "quit" => Command::Quit,
        "o" | "open" => {
            if argument.is_empty() {
                return Err(Error::MissingArgument("open"));
            }
            Command::Open(expand_home(argument))
        }
        "noh" | "nohl" | "nohlsearch" => Command::NoHighlight,
        "set" => Command::Set(parse_setting(argument)?),
        _ => return Err(Error::Unknown(name.to_owned())),
    };
    Ok(Some(command))
}

/// Parses the `name value` that follows `:set`. A boolean setting named
/// alone is switched on.
fn parse_setting(argument: &str) -> Result<Setting, Error> {
    let (name, value) = argument
        .split_once(char::is_whitespace)
        .map_or((argument, ""), |(name, rest)| (name, rest.trim()));
    match name {
        "" => Err(Error::MissingArgument("set")),
        "recolor" => parse_switch(value).map(Setting::Recolor),
        "trim" => parse_switch(value).map(Setting::Trim),
        "recolor-darkcolor" => Ok(Setting::RecolorInk(value.parse()?)),
        "recolor-lightcolor" => Ok(Setting::RecolorPaper(value.parse()?)),
        "scroll-step" => match value.parse::<f32>() {
            Ok(step) if step.is_finite() && step > 0.0 => Ok(Setting::ScrollStep(step)),
            _ => Err(Error::NotPositive(value.to_owned())),
        },
        other => Err(Error::UnknownSetting(other.to_owned())),
    }
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
            Ok(Some(Command::Set(Setting::Recolor(true))))
        );
        assert_eq!(
            parse("set recolor false"),
            Ok(Some(Command::Set(Setting::Recolor(false))))
        );
        assert_eq!(
            parse("set recolor-darkcolor #102030"),
            Ok(Some(Command::Set(Setting::RecolorInk(Rgb(
                0x10, 0x20, 0x30
            )))))
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
            Err(Error::NotBoolean("maybe".into()))
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
    fn quit_and_nohlsearch_have_short_forms() {
        assert_eq!(parse("q"), Ok(Some(Command::Quit)));
        assert_eq!(parse("quit"), Ok(Some(Command::Quit)));
        assert_eq!(parse("noh"), Ok(Some(Command::NoHighlight)));
    }
}
