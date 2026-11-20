//! The configuration file, read once at startup.
//!
//! It lives at `$XDG_CONFIG_HOME/pdvim/config.toml`, or
//! `~/.config/pdvim/config.toml` when that is not set, and has two tables:
//!
//! ```toml
//! [settings]
//! recolor = true
//! scroll-step = 60
//!
//! [keys]
//! "ctrl+n" = "next-tab"
//! "q" = "none"
//! ```
//!
//! Settings take the names and values of `:set`. Keys are written as for
//! `:map`, and bound to an action by name, or to `none` to unbind them.

use std::path::PathBuf;

use toml::{Table, Value};

use crate::command::{self, Command};
use crate::keymap::{self, Binding};

/// A part of the file that could not be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct Problem(pub String);

/// Why the file could not be read at all.
#[derive(Debug, thiserror::Error)]
#[error("cannot read {0}: {1}")]
pub struct Unreadable(pub PathBuf, pub std::io::Error);

/// Reads the configuration file. A missing file is no configuration at all.
pub fn read() -> Result<String, Unreadable> {
    let Some(path) = path() else {
        return Ok(String::new());
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(Unreadable(path, error)),
    }
}

/// The commands the text of a configuration file amounts to, settings
/// first, and the parts that could not be used. A file that is not valid
/// TOML is used not at all.
pub fn parse(text: &str) -> (Vec<Command>, Vec<Problem>) {
    let mut commands = Vec::new();
    let mut problems = Vec::new();
    let table = match text.parse::<Table>() {
        Ok(table) => table,
        Err(error) => {
            // The message ends with the offending line, quoted; the first
            // line says where.
            let message = error.to_string();
            let first = message.lines().next().unwrap_or_default().trim();
            problems.push(Problem(format!("not valid TOML: {first}")));
            return (commands, problems);
        }
    };
    for (section, content) in &table {
        let entries = match (section.as_str(), content) {
            ("settings" | "keys", Value::Table(entries)) => entries,
            ("settings" | "keys", _) => {
                problems.push(Problem(format!("{section} is not a table")));
                continue;
            }
            _ => {
                problems.push(Problem(format!("unknown section: {section}")));
                continue;
            }
        };
        let read = if section == "settings" {
            settings
        } else {
            keys
        };
        for (name, value) in entries {
            match read(name, value) {
                Ok(command) => commands.push(command),
                Err(message) => problems.push(Problem(format!("{section}.{name}: {message}"))),
            }
        }
    }
    // Settings go first whatever order the tables came in, though nothing
    // yet depends on it.
    commands.sort_by_key(|command| !matches!(command, Command::Set(_)));
    (commands, problems)
}

/// The command for one entry of `[settings]`.
fn settings(name: &str, value: &Value) -> Result<Command, String> {
    let text = match value {
        Value::String(text) => text.clone(),
        Value::Boolean(flag) => flag.to_string(),
        Value::Integer(number) => number.to_string(),
        Value::Float(number) => number.to_string(),
        _ => return Err("not a string, number or boolean".to_owned()),
    };
    command::setting(name, &text)
        .map(Command::Set)
        .map_err(|error| error.to_string())
}

/// The command for one entry of `[keys]`.
fn keys(written: &str, value: &Value) -> Result<Command, String> {
    let keys = keymap::parse_keys(written).map_err(|error| error.to_string())?;
    let Value::String(action) = value else {
        return Err("not the name of an action".to_owned());
    };
    if action == "none" {
        return Ok(Command::Unmap(keys));
    }
    let binding = Binding::named(action)
        .ok_or_else(|| keymap::Error::UnknownAction(action.clone()).to_string())?;
    Ok(Command::Map(keys, binding))
}

/// Where the configuration file is looked for.
pub fn path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("pdvim").join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::DEFAULT_SCROLL_STEP;
    use crate::command::Setting;
    use crate::input::Key;
    use crate::keymap::Keymap;
    use crate::recolor::{Palette, Rgb};

    #[test]
    fn settings_and_keys_are_read() {
        let text = r##"
            # A comment.
            [keys]
            "ctrl+n" = "next-tab"
            "g g" = "none"

            [settings]
            scroll-step = 60
            recolor = true
            recolor-darkcolor = "#102030"
        "##;
        let (commands, problems) = parse(text);
        assert_eq!(problems, []);
        assert_eq!(
            commands,
            [
                Command::Set(Setting::Recolor(true)),
                Command::Set(Setting::RecolorInk(Rgb(0x10, 0x20, 0x30))),
                Command::Set(Setting::ScrollStep(60.0)),
                Command::Map(vec![Key::Ctrl('n')], Binding::NextTab),
                Command::Unmap(vec![Key::Char('g'), Key::Char('g')]),
            ]
        );
    }

    #[test]
    fn bad_entries_are_reported_and_skipped() {
        let text = r#"
            colours = 3
            [settings]
            colour = "red"
            trim = "maybe"
            scroll-step = 0.5
            [keys]
            "j" = "fly"
            "nope" = "quit"
            "k" = 3
            "q" = "quit"
        "#;
        let (commands, problems) = parse(text);
        assert_eq!(
            commands,
            [
                Command::Set(Setting::ScrollStep(0.5)),
                Command::Map(vec![Key::Char('q')], Binding::Quit),
            ]
        );
        let messages: Vec<_> = problems.iter().map(ToString::to_string).collect();
        assert_eq!(
            messages,
            [
                "unknown section: colours",
                "keys.j: unknown action: fly",
                "keys.k: not the name of an action",
                "keys.nope: unknown key: nope",
                "settings.colour: unknown setting: colour",
                "settings.trim: `maybe` is not true or false",
            ]
        );
    }

    /// The example shipped with pdvim must say exactly what the defaults
    /// are, so that it is never out of date.
    #[test]
    fn the_example_is_the_defaults() {
        let (commands, problems) = parse(include_str!("../config.example.toml"));
        assert_eq!(problems, []);

        let mut keymap = Keymap::default();
        keymap.clear();
        let mut settings = Vec::new();
        for command in commands {
            match command {
                Command::Map(keys, binding) => keymap.bind(keys, binding),
                Command::Set(setting) => settings.push(setting),
                other => panic!("unexpected in the example: {other:?}"),
            }
        }
        assert_eq!(keymap.sorted(), Keymap::default().sorted());

        // The entries of a table come in order of name.
        let palette = Palette::default();
        assert_eq!(
            settings,
            [
                Setting::ConfirmQuit(false),
                Setting::ConfirmTabClose(false),
                Setting::Recolor(false),
                Setting::RecolorInk(palette.ink),
                Setting::RecolorPaper(palette.paper),
                Setting::ScrollStep(DEFAULT_SCROLL_STEP),
                Setting::Trim(false),
            ]
        );
    }

    #[test]
    fn a_file_that_is_not_toml_is_not_used() {
        let (commands, problems) = parse("[settings]\nrecolor = \n");
        assert_eq!(commands, []);
        assert_eq!(problems.len(), 1);
        assert!(
            problems[0].0.starts_with("not valid TOML: "),
            "{}",
            problems[0]
        );
    }
}
