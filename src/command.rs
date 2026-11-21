//! The commands typed after `:`.

use std::path::PathBuf;

use crate::backend::Flow;
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
    /// Show the key bindings, at a topic if one is given: a mode, a
    /// heading, or an action.
    Help(Option<String>),
    /// Show only the bindings that mention a word.
    Apropos(String),
    /// Say which theme is in force.
    ShowTheme,
    /// Say which version of pdvim this is, and where it comes from.
    Version,
    /// Put a note with this text where visual mode's cursor was, or at
    /// the top of the window.
    Note(String),
    /// Show what the document in view says about itself.
    Info,
    /// Say which marks are set.
    Marks,
    /// Take marks away: these letters, or with none, all of the
    /// document's a to z.
    DeleteMarks(Option<Vec<char>>),
    /// Open a new tab, with a document in it or empty.
    TabOpen(Option<PathBuf>),
    /// Close the tab in view, unless it is the last.
    TabClose,
    /// Go to the next tab, or to a tab counted from 1.
    TabNext(Option<u32>),
    /// Go to the previous tab.
    TabPrevious,
    /// Close every tab but the one in view; forced, even those with
    /// changes not saved.
    TabOnly { force: bool },
    /// Move the tab in view.
    TabMove(TabPlace),
    /// Go to a page, counted from 1.
    Goto(u32),
    /// Go to the page with this number printed on it, as `xii`, or else
    /// at this place, counted from 1.
    GotoLabel(String),
    /// Change a setting.
    Set(Setting),
    /// Hide the search highlights.
    NoHighlight,
    /// Bind a key sequence, in the mode of what it is bound to.
    Map(Vec<Key>, Bound),
    /// Bind a key sequence in normal mode to a command line, as typed
    /// after `:`; `;` separates several commands.
    MapLine(Vec<Key>, String),
    /// Unbind a key sequence in a mode.
    Unmap(Mode, Vec<Key>),
    /// Unbind every key sequence in a mode.
    MapClear(Mode),
    /// Do what a key binding does, as if its keys were pressed after a
    /// count.
    Run(Binding, Option<u32>),
    /// Read the configuration file again, or another file written the same
    /// way, over the built-in configuration.
    Source(Option<PathBuf>),
    /// Close the tab in view, and the program with its last tab.
    Quit,
    /// Close every tab and the program.
    QuitAll,
    /// Close the tab in view, or the program with its last tab, throwing
    /// away changes not saved: `:q!`.
    ForceQuit,
    /// Close every tab and the program, throwing away changes not saved:
    /// `:qa!`.
    ForceQuitAll,
    /// Save the document's changes: into its own file, or as a new file,
    /// which must not exist already unless forced with `:w!`.
    Write {
        target: Option<PathBuf>,
        force: bool,
    },
    /// Save the document's changes into its file, then close the tab.
    WriteQuit,
    /// Save the changes of every document into its file.
    WriteAll,
    /// Save the changes of every document, then close the program.
    WriteQuitAll,
}

/// Where `:tabmove` moves the tab in view to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabPlace {
    /// To this place, counted from 0, as vim's `:tabmove 0` makes it the
    /// first.
    At(usize),
    /// This many places on, or back when negative.
    By(i64),
    /// To the end.
    Last,
}

/// A setting and its new value.
#[derive(Debug, Clone, PartialEq)]
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
    /// The colour text is underlined in.
    UnderlineColour(Rgb),
    /// The colour of the line through struck-out text.
    StrikeOutColour(Rgb),
    /// The colour of a squiggly underline.
    SquigglyColour(Rgb),
    /// The colour of a note's icon.
    NoteColour(Rgb),
    /// Whether highlights are saved into the file as soon as they are
    /// made or taken back, rather than with `:w`.
    Autosave(bool),
    /// Whether the tab bar shows when there is more than one tab.
    TabBar(bool),
    /// Whether matches are highlighted while a search is typed.
    Incsearch(bool),
    /// The letters labels on links and lines are made of, the first used
    /// most.
    HintChars(Vec<char>),
    /// Whether the status bar shows while reading; off, it shows only
    /// while something is typed or said.
    StatusBar(bool),
    /// The size of type, in points, that books without pages of their
    /// own, as EPUB, are laid out in.
    FontSize(f32),
    /// How many pages sit side by side, from 1 to 4.
    Columns(usize),
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
    #[error("not a mark: {0}; marks are letters")]
    NotAMark(char),
    #[error("unknown theme: {0}")]
    UnknownTheme(String),
    #[error("`{0}` is not a positive number")]
    NotPositive(String),
    #[error("`{0}` is not a number of columns from 1 to 4")]
    NotColumns(String),
    #[error("`{0}` is not a size of type from 4 to 72")]
    NotFontSize(String),
    #[error("`{0}` is not two or more different lower-case letters or digits")]
    NotHintChars(String),
    #[error("commands can only be bound to keys in normal mode")]
    CommandOutsideNormal,
    #[error(transparent)]
    Colour(#[from] InvalidColour),
    #[error(transparent)]
    Keys(#[from] keymap::Error),
}

/// Every command by its full name, and the shorter names it also goes by.
const NAMES: [(&str, &[&str]); 30] = [
    ("apropos", &["apr"]),
    ("delmarks", &["delm"]),
    ("goto", &[]),
    ("help", &["h"]),
    ("info", &["f", "file"]),
    ("map", &[]),
    ("mapclear", &[]),
    ("marks", &[]),
    ("nohlsearch", &["noh", "nohl"]),
    ("note", &[]),
    ("open", &["o"]),
    ("qall", &["qa", "quita", "quitall"]),
    ("quit", &["q"]),
    ("recent", &["rec"]),
    ("set", &[]),
    ("source", &["so"]),
    ("tabclose", &["tabc"]),
    ("tabedit", &["tabe"]),
    ("tabmove", &["tabm"]),
    ("tabnew", &[]),
    ("tabnext", &["tabn"]),
    ("tabonly", &["tabo"]),
    ("tabprevious", &["tabp", "tabN", "tabNext"]),
    ("theme", &["colo", "colorscheme"]),
    ("unmap", &[]),
    ("version", &["ve"]),
    ("wall", &["wa"]),
    ("wq", &["x", "xit"]),
    ("wqall", &["wqa", "xa", "xall"]),
    ("write", &["w"]),
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
pub const SETTINGS: [&str; 22] = [
    "autosave",
    "columns",
    "confirm-quit",
    "confirm-tab-close",
    "fontsize",
    "highlight-color",
    "hintchars",
    "incsearch",
    "note-color",
    "recolor",
    "recolor-darkcolor",
    "recolor-lightcolor",
    "scroll-step",
    "squiggly-color",
    "statusbar",
    "strikeout-color",
    "tabbar",
    "theme",
    "theme-dark",
    "theme-light",
    "trim",
    "underline-color",
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
    // A `!` after `:q`, `:qa` or `:w` forces it, as in vim.
    if let Some(forced) = name.strip_suffix('!')
        && let Some(command) = canonical(forced)
    {
        return forced_command(command, argument)
            .map(Some)
            .ok_or_else(|| Error::Unknown(name.to_owned()));
    }
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
        "write" => Command::Write {
            target: (!argument.is_empty()).then(|| expand_home(argument)),
            force: false,
        },
        "wq" => Command::WriteQuit,
        "wall" => Command::WriteAll,
        "wqall" => Command::WriteQuitAll,
        "qall" => Command::QuitAll,
        "tabedit" | "tabnew" | "tabclose" | "tabnext" | "tabprevious" | "tabonly" | "tabmove" => {
            tab_command(canonical, argument)?
        }
        "open" => {
            if argument.is_empty() {
                return Err(Error::MissingArgument("open"));
            }
            Command::Open(expand_home(argument))
        }
        "recent" => Command::Recent,
        "source" => Command::Source((!argument.is_empty()).then(|| expand_home(argument))),
        "version" => Command::Version,
        "info" => Command::Info,
        "note" if argument.is_empty() => return Err(Error::MissingArgument("note")),
        "note" => Command::Note(argument.to_owned()),
        "goto" if argument.is_empty() => return Err(Error::MissingArgument("goto")),
        "goto" => Command::GotoLabel(argument.to_owned()),
        "marks" => Command::Marks,
        "delmarks" => delete_marks(argument)?,
        // As `:set theme`; with nothing after it, says which is in force.
        "theme" if argument.is_empty() => Command::ShowTheme,
        "theme" => Command::Set(setting("theme", argument)?),
        "help" => Command::Help((!argument.is_empty()).then(|| argument.to_owned())),
        "apropos" if argument.is_empty() => return Err(Error::MissingArgument("apropos")),
        "apropos" => Command::Apropos(argument.to_owned()),
        "nohlsearch" => Command::NoHighlight,
        "set" => Command::Set(parse_setting(argument)?),
        "map" | "unmap" | "mapclear" => map_command(canonical, argument)?,
        _ => return Err(Error::Unknown(name.to_owned())),
    };
    Ok(Some(command))
}

/// Parses a command about tabs, given by its full name.
fn tab_command(name: &str, argument: &str) -> Result<Command, Error> {
    Ok(match name {
        "tabclose" => Command::TabClose,
        "tabprevious" => Command::TabPrevious,
        "tabonly" => Command::TabOnly { force: false },
        "tabmove" => Command::TabMove(tab_place(argument)?),
        "tabnext" if argument.is_empty() => Command::TabNext(None),
        "tabnext" => match argument.parse::<u32>() {
            Ok(tab) if tab > 0 => Command::TabNext(Some(tab)),
            _ => return Err(Error::NotPositive(argument.to_owned())),
        },
        _ => Command::TabOpen((!argument.is_empty()).then(|| expand_home(argument))),
    })
}

/// Parses where `:tabmove` moves to: nothing or `$` for the end, `+n` or
/// `-n` for n places on or back, and `n` for place n counted from 0.
fn tab_place(argument: &str) -> Result<TabPlace, Error> {
    let bad = || Error::NotPositive(argument.to_owned());
    match argument {
        "" | "$" => Ok(TabPlace::Last),
        _ if let Some(by) = argument.strip_prefix('+') => {
            by.parse::<i64>().map(TabPlace::By).map_err(|_| bad())
        }
        _ if argument.starts_with('-') => {
            argument.parse::<i64>().map(TabPlace::By).map_err(|_| bad())
        }
        _ => argument
            .parse::<usize>()
            .map(TabPlace::At)
            .map_err(|_| bad()),
    }
}

/// Parses `:map`, `:unmap` or `:mapclear`, given by its full name.
fn map_command(name: &str, argument: &str) -> Result<Command, Error> {
    let command = match name {
        // A command line to run starts with `:`, after the keys; a key
        // that is `:` alone is no command.
        "map" if let Some(start) = command_start(argument) => {
            let (keys, line) = argument.split_at(start);
            let (mode, keys) = mode(keys.trim_end());
            if mode != Mode::Normal {
                return Err(Error::CommandOutsideNormal);
            }
            if keys.is_empty() {
                return Err(Error::MissingArgument("map"));
            }
            let line = line.trim_start_matches(':').trim();
            Command::MapLine(keymap::parse_keys(keys)?, line.to_owned())
        }
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
        _ => match argument {
            "" => Command::MapClear(Mode::Normal),
            name => Command::MapClear(
                Mode::named(name).ok_or_else(|| keymap::Error::UnknownMode(name.to_owned()))?,
            ),
        },
    };
    Ok(command)
}

/// Where the command line starts in the arguments to `:map`, if they bind
/// keys to one: at the first word after the keys that starts with `:` and
/// has more after it.
fn command_start(argument: &str) -> Option<usize> {
    let mut offset = 0;
    for word in argument.split_inclusive(char::is_whitespace) {
        let trimmed = word.trim_end();
        if offset > 0 && trimmed.len() > 1 && trimmed.starts_with(':') {
            return Some(offset);
        }
        offset = offset.saturating_add(word.len());
    }
    None
}

/// Splits a command line into the commands in it, at each `;`; `;;` is a
/// `;` that does not split. After `map` and `note`, the rest of the line is
/// theirs, `;` and all, so that a line can be bound to keys and a note can
/// say anything.
pub fn split(line: &str) -> Vec<String> {
    let mut commands = Vec::new();
    let mut current = String::new();
    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        let whole = matches!(
            current.split_whitespace().next().and_then(canonical),
            Some("map" | "note")
        );
        match character {
            ';' if characters.peek() == Some(&';') && !whole => {
                characters.next();
                current.push(';');
            }
            ';' if !whole => commands.push(std::mem::take(&mut current)),
            _ => current.push(character),
        }
    }
    commands.push(current);
    commands
        .into_iter()
        .map(|command| command.trim().to_owned())
        .filter(|command| !command.is_empty())
        .collect()
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

/// The command a `!` forces, given by its full name, if it takes one.
fn forced_command(name: &str, argument: &str) -> Option<Command> {
    match name {
        "quit" => Some(Command::ForceQuit),
        "qall" => Some(Command::ForceQuitAll),
        "tabonly" => Some(Command::TabOnly { force: true }),
        "write" => Some(Command::Write {
            target: (!argument.is_empty()).then(|| expand_home(argument)),
            force: true,
        }),
        _ => None,
    }
}

/// Parses the letters after `:delmarks`, or `!` for all.
fn delete_marks(argument: &str) -> Result<Command, Error> {
    match argument {
        "!" => Ok(Command::DeleteMarks(None)),
        "" => Err(Error::MissingArgument("delmarks")),
        letters => {
            let names: Vec<char> = letters.chars().filter(|c| !c.is_whitespace()).collect();
            if let Some(bad) = names.iter().find(|name| !name.is_ascii_alphabetic()) {
                return Err(Error::NotAMark(*bad));
            }
            Ok(Command::DeleteMarks(Some(names)))
        }
    }
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
        "autosave" => parse_switch(value).map(Setting::Autosave),
        "confirm-tab-close" => parse_switch(value).map(Setting::ConfirmTabClose),
        "tabbar" => parse_switch(value).map(Setting::TabBar),
        "incsearch" => parse_switch(value).map(Setting::Incsearch),
        "hintchars" => hint_chars(value).map(Setting::HintChars),
        "statusbar" => parse_switch(value).map(Setting::StatusBar),
        "highlight-color" => Ok(Setting::HighlightColour(value.parse()?)),
        "underline-color" => Ok(Setting::UnderlineColour(value.parse()?)),
        "strikeout-color" => Ok(Setting::StrikeOutColour(value.parse()?)),
        "squiggly-color" => Ok(Setting::SquigglyColour(value.parse()?)),
        "note-color" => Ok(Setting::NoteColour(value.parse()?)),
        "columns" => match value.parse::<usize>() {
            Ok(columns @ 1..=4) => Ok(Setting::Columns(columns)),
            _ => Err(Error::NotColumns(value.to_owned())),
        },
        "fontsize" => match value.parse::<f32>() {
            Ok(size) if (Flow::EM.0..=Flow::EM.1).contains(&size) => Ok(Setting::FontSize(size)),
            _ => Err(Error::NotFontSize(value.to_owned())),
        },
        "scroll-step" => match value.parse::<f32>() {
            Ok(step) if step.is_finite() && step > 0.0 => Ok(Setting::ScrollStep(step)),
            _ => Err(Error::NotPositive(value.to_owned())),
        },
        other => Err(Error::UnknownSetting(other.to_owned())),
    }
}

/// Parses the letters labels are made of: two or more lower-case letters
/// or digits, each once.
fn hint_chars(value: &str) -> Result<Vec<char>, Error> {
    let letters: Vec<char> = value.chars().collect();
    let mut unique = letters.clone();
    unique.sort_unstable();
    unique.dedup();
    let typable = letters
        .iter()
        .all(|letter| letter.is_ascii_lowercase() || letter.is_ascii_digit());
    if letters.len() < 2 || unique.len() != letters.len() || !typable {
        return Err(Error::NotHintChars(value.to_owned()));
    }
    Ok(letters)
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
    // Windows names the home directory differently.
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
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
        assert_eq!(
            parse("set fontsize 14.5"),
            Ok(Some(Command::Set(Setting::FontSize(14.5))))
        );
    }

    #[test]
    fn hint_letters_are_lower_case_and_different() {
        assert_eq!(
            parse("set hintchars hjkl"),
            Ok(Some(Command::Set(Setting::HintChars(vec!['h', 'j', 'k', 'l']))))
        );
        for bad in ["a", "aa", "aB", "a;", ""] {
            assert_eq!(
                setting("hintchars", bad),
                Err(Error::NotHintChars(bad.into())),
                "{bad}"
            );
        }
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
        assert_eq!(parse("set fontsize 3"), Err(Error::NotFontSize("3".into())));
        assert_eq!(parse("set columns 0"), Err(Error::NotColumns("0".into())));
        assert_eq!(parse("set columns 5"), Err(Error::NotColumns("5".into())));
        assert_eq!(
            parse("set columns 2"),
            Ok(Some(Command::Set(Setting::Columns(2))))
        );
        assert_eq!(
            parse("set fontsize big"),
            Err(Error::NotFontSize("big".into()))
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
    fn write_and_quit_commands_parse() {
        assert_eq!(
            parse("w"),
            Ok(Some(Command::Write {
                target: None,
                force: false
            }))
        );
        assert_eq!(
            parse("write /tmp/a copy.pdf"),
            Ok(Some(Command::Write {
                target: Some(PathBuf::from("/tmp/a copy.pdf")),
                force: false
            }))
        );
        assert_eq!(
            parse("w! /tmp/b.pdf"),
            Ok(Some(Command::Write {
                target: Some(PathBuf::from("/tmp/b.pdf")),
                force: true
            }))
        );
        assert_eq!(parse("wq"), Ok(Some(Command::WriteQuit)));
        assert_eq!(parse("x"), Ok(Some(Command::WriteQuit)));
        assert_eq!(parse("wa"), Ok(Some(Command::WriteAll)));
        assert_eq!(parse("xa"), Ok(Some(Command::WriteQuitAll)));
        assert_eq!(parse("q!"), Ok(Some(Command::ForceQuit)));
        assert_eq!(parse("qa!"), Ok(Some(Command::ForceQuitAll)));
        assert_eq!(parse("tabnew!"), Err(Error::Unknown("tabnew!".into())));
        assert_eq!(
            parse("set autosave"),
            Ok(Some(Command::Set(Setting::Autosave(true))))
        );
    }

    #[test]
    fn mark_commands_parse() {
        assert_eq!(parse("marks"), Ok(Some(Command::Marks)));
        assert_eq!(
            parse("delm a B c"),
            Ok(Some(Command::DeleteMarks(Some(vec!['a', 'B', 'c']))))
        );
        assert_eq!(parse("delmarks !"), Ok(Some(Command::DeleteMarks(None))));
        assert_eq!(parse("delmarks 1"), Err(Error::NotAMark('1')));
        assert_eq!(parse("delmarks"), Err(Error::MissingArgument("delmarks")));
    }

    #[test]
    fn version_parses() {
        assert_eq!(parse("version"), Ok(Some(Command::Version)));
        assert_eq!(parse("ve"), Ok(Some(Command::Version)));
        assert_eq!(parse("info"), Ok(Some(Command::Info)));
        assert_eq!(
            parse("note  check this  "),
            Ok(Some(Command::Note("check this".into())))
        );
        assert_eq!(parse("note"), Err(Error::MissingArgument("note")));
        assert_eq!(
            parse("goto xii"),
            Ok(Some(Command::GotoLabel("xii".into())))
        );
        assert_eq!(
            parse("goto A-3"),
            Ok(Some(Command::GotoLabel("A-3".into())))
        );
        assert_eq!(parse("goto"), Err(Error::MissingArgument("goto")));
        assert_eq!(parse("f"), Ok(Some(Command::Info)));
        assert_eq!(parse("file"), Ok(Some(Command::Info)));
    }

    #[test]
    fn theme_commands_parse() {
        assert_eq!(parse("theme"), Ok(Some(Command::ShowTheme)));
        assert_eq!(
            parse("colo catppuccin-mocha"),
            Ok(Some(Command::Set(Setting::Theme(Some(
                Theme::CatppuccinMocha
            )))))
        );
        assert_eq!(
            parse("theme auto"),
            Ok(Some(Command::Set(Setting::Theme(None))))
        );
        assert_eq!(
            parse("colorscheme nord"),
            Err(Error::UnknownTheme("nord".into()))
        );
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
    fn keys_map_to_command_lines() {
        assert_eq!(
            parse("map ctrl+t :theme catppuccin-latte"),
            Ok(Some(Command::MapLine(
                vec![Key::Ctrl('t')],
                "theme catppuccin-latte".into()
            )))
        );
        assert_eq!(
            parse("map g x :set recolor; zoom-to 150"),
            Ok(Some(Command::MapLine(
                vec![Key::Char('g'), Key::Char('x')],
                "set recolor; zoom-to 150".into()
            )))
        );
        // `:` alone is a key.
        assert_eq!(
            parse("map : command-prompt"),
            Ok(Some(Command::Map(
                vec![Key::Char(':')],
                Bound::Normal(Binding::CommandPrompt)
            )))
        );
        assert_eq!(
            parse("map visual x :help"),
            Err(Error::CommandOutsideNormal)
        );
    }

    #[test]
    fn lines_split_into_commands_at_semicolons() {
        assert_eq!(split("w; q"), ["w", "q"]);
        assert_eq!(split(" set trim ;; ; ;"), ["set trim ;"]);
        assert_eq!(split("open a;;b.pdf;help"), ["open a;b.pdf", "help"]);
        // The rest of the line is the binding's, or the note's.
        assert_eq!(split("map x :w; q"), ["map x :w; q"]);
        assert_eq!(split("note a; b"), ["note a; b"]);
        assert_eq!(split(""), Vec::<String>::new());
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
    fn help_takes_a_topic_and_apropos_a_word() {
        assert_eq!(parse("h"), Ok(Some(Command::Help(None))));
        assert_eq!(
            parse("help visual"),
            Ok(Some(Command::Help(Some("visual".into()))))
        );
        assert_eq!(
            parse("apropos zoom"),
            Ok(Some(Command::Apropos("zoom".into())))
        );
        assert_eq!(parse("apropos"), Err(Error::MissingArgument("apropos")));
    }

    #[test]
    fn source_takes_a_file_or_none() {
        assert_eq!(parse("source"), Ok(Some(Command::Source(None))));
        assert_eq!(
            parse("so /tmp/other.toml"),
            Ok(Some(Command::Source(Some(PathBuf::from(
                "/tmp/other.toml"
            )))))
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
        assert_eq!(parse("tabo"), Ok(Some(Command::TabOnly { force: false })));
        assert_eq!(
            parse("tabonly!"),
            Ok(Some(Command::TabOnly { force: true }))
        );
        assert_eq!(parse("tabm"), Ok(Some(Command::TabMove(TabPlace::Last))));
        assert_eq!(parse("tabm $"), Ok(Some(Command::TabMove(TabPlace::Last))));
        assert_eq!(parse("tabm 0"), Ok(Some(Command::TabMove(TabPlace::At(0)))));
        assert_eq!(
            parse("tabm +2"),
            Ok(Some(Command::TabMove(TabPlace::By(2))))
        );
        assert_eq!(
            parse("tabm -1"),
            Ok(Some(Command::TabMove(TabPlace::By(-1))))
        );
        assert_eq!(parse("tabm x"), Err(Error::NotPositive("x".into())));
    }
}
