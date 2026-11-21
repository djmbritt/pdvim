//! The configuration file, read once at startup.
//!
//! It lives at `$XDG_CONFIG_HOME/pdvim/config.toml`, or
//! `~/.config/pdvim/config.toml` when that is not set
//! (`%APPDATA%\pdvim\config.toml` on Windows), and has two tables:
//!
//! ```toml
//! [settings]
//! recolor = true
//! scroll-step = 60
//!
//! [keys.normal]
//! "ctrl+n" = "next-tab"
//! "q" = "none"
//!
//! [keys.visual]
//! "Y" = "yank"
//! ```
//!
//! Settings take the names and values of `:set`. Keys are bound in a mode,
//! one table to a mode, as with `:map`: to an action by name, to `none` to
//! unbind them, or in normal mode to a command line after `:`, such as
//! `":set recolor; zoom-to 150"`. Keys put straight in `[keys]` are bound in normal mode.
//!
//! The built-in configuration, `defaults.toml`, is read the same way.

use std::path::PathBuf;

use toml::{Table, Value};

use crate::bindings::Mode;
use crate::command::{self, Command};
use crate::keymap;

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

/// Reads a configuration file named by hand, which must be there.
pub fn read_from(path: &std::path::Path) -> Result<String, Unreadable> {
    std::fs::read_to_string(path).map_err(|error| Unreadable(path.to_owned(), error))
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
        for (name, value) in entries {
            let read = match (section.as_str(), value) {
                ("settings", _) => settings(name, value),
                (_, Value::Table(bindings)) => {
                    let Some(mode) = Mode::named(name) else {
                        problems.push(Problem(format!("keys.{name}: unknown mode")));
                        continue;
                    };
                    for (written, action) in bindings {
                        match keys(mode, written, action) {
                            Ok(command) => commands.push(command),
                            Err(message) => {
                                problems.push(Problem(format!("keys.{name}.{written}: {message}")));
                            }
                        }
                    }
                    continue;
                }
                _ => keys(Mode::Normal, name, value),
            };
            match read {
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

/// The command for one key binding, in a mode.
fn keys(mode: Mode, written: &str, value: &Value) -> Result<Command, String> {
    let keys = keymap::parse_keys(written).map_err(|error| error.to_string())?;
    let Value::String(action) = value else {
        return Err("not the name of an action".to_owned());
    };
    if action == "none" {
        return Ok(Command::Unmap(mode, keys));
    }
    if let Some(line) = action.strip_prefix(':') {
        if mode != Mode::Normal {
            return Err(command::Error::CommandOutsideNormal.to_string());
        }
        return Ok(Command::MapLine(keys, line.trim().to_owned()));
    }
    let bound = mode
        .bound(action)
        .ok_or_else(|| keymap::Error::UnknownAction(action.clone()).to_string())?;
    Ok(Command::Map(keys, bound))
}

/// Where the configuration file is looked for.
pub fn path() -> Option<PathBuf> {
    Some(crate::dirs::config()?.join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bindings::{Bound, DEFAULTS};
    use crate::command::{Recolor, Setting};
    use crate::input::{Binding, Key};
    use crate::recolor::Rgb;
    use crate::visual;

    #[test]
    fn settings_and_keys_are_read() {
        let text = r##"
            # A comment.
            [keys.normal]
            "ctrl+n" = "next-tab"
            "g g" = "none"

            [keys.visual]
            "Y" = "yank"

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
                Command::Set(Setting::ScrollStep(60.0)),
                Command::Set(Setting::Recolor(Recolor::On)),
                Command::Set(Setting::RecolorInk(Some(Rgb(0x10, 0x20, 0x30)))),
                Command::Map(vec![Key::Ctrl('n')], Bound::Normal(Binding::NextTab)),
                Command::Unmap(Mode::Normal, vec![Key::Char('g'), Key::Char('g')]),
                Command::Map(vec![Key::Char('Y')], Bound::Visual(visual::Binding::Yank)),
            ]
        );
    }

    /// Before modes, keys were bound straight in `[keys]`, which still
    /// binds them in normal mode.
    #[test]
    fn keys_outside_a_mode_are_bound_in_normal_mode() {
        let (commands, problems) = parse("[keys]\n\"Q\" = \"quit\"\n\"q\" = \"none\"\n");
        assert_eq!(problems, []);
        assert_eq!(
            commands,
            [
                Command::Map(vec![Key::Char('Q')], Bound::Normal(Binding::Quit)),
                Command::Unmap(Mode::Normal, vec![Key::Char('q')]),
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
            [keys.normal]
            "j" = "fly"
            "nope" = "quit"
            "k" = 3
            "q" = "quit"
            [keys.visual]
            "j" = "scroll-down"
            [keys.insert]
            "x" = "quit"
        "#;
        let (commands, problems) = parse(text);
        assert_eq!(
            commands,
            [
                Command::Set(Setting::ScrollStep(0.5)),
                Command::Map(vec![Key::Char('q')], Bound::Normal(Binding::Quit)),
            ]
        );
        let messages: Vec<_> = problems.iter().map(ToString::to_string).collect();
        assert_eq!(
            messages,
            [
                "unknown section: colours",
                "settings.colour: unknown setting: colour",
                "settings.trim: `maybe` is not true or false",
                "keys.normal.j: unknown action: fly",
                "keys.normal.nope: unknown key: nope",
                "keys.normal.k: not the name of an action",
                "keys.visual.j: unknown action: scroll-down",
                "keys.insert: unknown mode",
            ]
        );
    }

    /// The built-in configuration is where every setting gets its
    /// default, so it must set every one.
    #[test]
    fn the_built_in_configuration_sets_every_setting() {
        let (commands, problems) = parse(DEFAULTS);
        assert_eq!(problems, []);
        let set: Vec<_> = DEFAULTS
            .lines()
            .filter_map(|line| line.split_once(" = "))
            .map(|(name, _)| name.trim())
            .filter(|name| command::SETTINGS.contains(name))
            .collect();
        for name in command::SETTINGS {
            assert_eq!(
                set.iter().filter(|&&known| known == name).count(),
                1,
                "{name} is not set once in defaults.toml"
            );
        }
        for command in commands {
            assert!(
                matches!(command, Command::Set(_) | Command::Map(..)),
                "unexpected in the defaults: {command:?}"
            );
        }
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
