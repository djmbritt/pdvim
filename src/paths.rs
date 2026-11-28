//! Settings for some documents only, picked by their paths, as the
//! configuration's `[[paths]]` tables give them:
//!
//! ```toml
//! [[paths]]
//! match = "~/scans/**"
//! recolor = false
//! trim = true
//! ```
//!
//! A rule sets how a document whose path it matches is shown each time it
//! opens: whether it is recolored, whether its margins are trimmed, and
//! how many pages sit side by side. Where several match, the last wins.

use std::path::Path;

use crate::command::{self, Recolor, Setting};

/// The settings a rule can give: those for how one document is shown.
pub const SETTINGS: [&str; 3] = ["recolor", "trim", "columns"];

/// The settings some documents have, set by a rule or several.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Settings {
    pub recolor: Option<Recolor>,
    pub trim: Option<bool>,
    pub columns: Option<usize>,
}

impl Settings {
    /// Takes a setting, if it is one a rule can give.
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "the other settings are not set by path"
    )]
    pub fn set(&mut self, setting: &Setting) -> Result<(), String> {
        match *setting {
            Setting::Recolor(recolor) => self.recolor = Some(recolor),
            Setting::Trim(trim) => self.trim = Some(trim),
            Setting::Columns(columns) => self.columns = Some(columns),
            _ => return Err(format!("only {} are set by path", SETTINGS.join(", "))),
        }
        Ok(())
    }

    /// These settings, with those of `later` over them.
    const fn under(self, later: Self) -> Self {
        Self {
            recolor: match later.recolor {
                Some(recolor) => Some(recolor),
                None => self.recolor,
            },
            trim: match later.trim {
                Some(trim) => Some(trim),
                None => self.trim,
            },
            columns: match later.columns {
                Some(columns) => Some(columns),
                None => self.columns,
            },
        }
    }
}

/// A pattern of paths and the settings of the documents it matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub pattern: Pattern,
    pub settings: Settings,
}

impl Rule {
    /// A rule from a pattern and settings by name and value, as written in
    /// the configuration.
    pub fn new<'a>(
        pattern: &str,
        settings: impl IntoIterator<Item = (&'a str, String)>,
    ) -> Result<Self, String> {
        let mut rule = Self {
            pattern: Pattern::new(pattern)?,
            settings: Settings::default(),
        };
        for (name, value) in settings {
            let setting = command::setting(name, &value).map_err(|error| error.to_string())?;
            rule.settings
                .set(&setting)
                .map_err(|reason| format!("{name}: {reason}"))?;
        }
        Ok(rule)
    }
}

impl Settings {
    /// Changes whether a document opening is recolored and trimmed as
    /// these settings say, recoloring it under `auto` as the system's
    /// theme, when known, asks; and answers with the columns they give.
    pub fn apply(self, recolor: &mut bool, trim: &mut bool, dark: Option<bool>) -> Option<usize> {
        match self.recolor {
            Some(Recolor::On) => *recolor = true,
            Some(Recolor::Off) => *recolor = false,
            Some(Recolor::Auto) => *recolor = dark.unwrap_or(*recolor),
            None => {}
        }
        if let Some(ruled) = self.trim {
            *trim = ruled;
        }
        self.columns
    }
}

/// The settings the rules give a document, the later rules over the
/// earlier.
pub fn settings_for(rules: &[Rule], path: &Path) -> Settings {
    rules
        .iter()
        .filter(|rule| rule.pattern.matches(path))
        .fold(Settings::default(), |settings, rule| {
            settings.under(rule.settings)
        })
}

/// A pattern of paths, as in a shell: `*` is any characters but `/`, `?`
/// is one, and `**` is any characters at all, so that `~/scans/**` is
/// everything under `~/scans`, however deep. A pattern ending in `/` is
/// everything under that folder. One without a `/` is matched against
/// the file's name alone, so that `*.cbz` is every comic. `~` at the
/// start is the home folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    parts: Vec<Part>,
    /// Whether only the file's name is matched.
    name_only: bool,
}

/// A piece of a pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Character(char),
    /// `?`: any one character but `/`.
    One,
    /// `*`: any characters but `/`.
    Within,
    /// `**`: any characters.
    Across,
    /// `**/`: nothing, or any characters ending in `/`.
    Folders,
}

impl Pattern {
    pub fn new(written: &str) -> Result<Self, String> {
        let written = written.trim();
        if written.is_empty() {
            return Err("an empty pattern".to_owned());
        }
        let name_only = !written.contains('/');
        let mut text = if name_only {
            written.to_owned()
        } else {
            slashed(&command::expand_home(written).to_string_lossy())
        };
        if text.ends_with('/') {
            text.push_str("**");
        }
        let mut parts = Vec::new();
        let mut characters = text.chars().peekable();
        while let Some(character) = characters.next() {
            parts.push(match character {
                '?' => Part::One,
                '*' if characters.peek() == Some(&'*') => {
                    characters.next();
                    if characters.peek() == Some(&'/') {
                        characters.next();
                        Part::Folders
                    } else {
                        Part::Across
                    }
                }
                '*' => Part::Within,
                character => Part::Character(character),
            });
        }
        Ok(Self { parts, name_only })
    }

    /// Whether a path matches: the whole of it, or its name alone.
    pub fn matches(&self, path: &Path) -> bool {
        let text = if self.name_only {
            path.file_name()
                .map_or_default(|name| name.to_string_lossy().into_owned())
        } else {
            slashed(&path.to_string_lossy())
        };
        let characters: Vec<char> = text.chars().collect();
        matches(&self.parts, &characters)
    }
}

/// A path with `/` between its folders, as on Windows too.
fn slashed(path: &str) -> String {
    path.replace('\\', "/")
}

/// Whether the parts of a pattern match the characters, all of each.
/// `reached[i]` says whether the parts so far can match the first `i`
/// characters.
fn matches(parts: &[Part], characters: &[char]) -> bool {
    let mut reached = vec![false; characters.len().saturating_add(1)];
    if let Some(first) = reached.first_mut() {
        *first = true;
    }
    for part in parts {
        let mut next = vec![false; reached.len()];
        for (start, _) in reached.iter().enumerate().filter(|(_, reached)| **reached) {
            let rest = characters.get(start..).unwrap_or_default();
            let mut mark = |taken: usize| {
                if let Some(slot) = next.get_mut(start.saturating_add(taken)) {
                    *slot = true;
                }
            };
            match *part {
                Part::Character(wanted) => {
                    if rest.first() == Some(&wanted) {
                        mark(1);
                    }
                }
                Part::One => {
                    if rest.first().is_some_and(|character| *character != '/') {
                        mark(1);
                    }
                }
                Part::Within => {
                    mark(0);
                    for (taken, _) in rest.iter().take_while(|c| **c != '/').enumerate() {
                        mark(taken.saturating_add(1));
                    }
                }
                Part::Across => {
                    for taken in 0..=rest.len() {
                        mark(taken);
                    }
                }
                Part::Folders => {
                    mark(0);
                    for (taken, _) in rest.iter().enumerate().filter(|(_, c)| **c == '/') {
                        mark(taken.saturating_add(1));
                    }
                }
            }
        }
        reached = next;
    }
    reached.last().copied().unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matching(pattern: &str, path: &str) -> bool {
        Pattern::new(pattern).unwrap().matches(Path::new(path))
    }

    #[test]
    fn stars_stay_in_a_folder_and_double_stars_do_not() {
        assert!(matching("/books/*.pdf", "/books/a.pdf"));
        assert!(!matching("/books/*.pdf", "/books/old/a.pdf"));
        assert!(matching("/books/**", "/books/old/a.pdf"));
        assert!(matching("/books/**/a.pdf", "/books/a.pdf"));
        assert!(matching("/books/**/a.pdf", "/books/x/y/a.pdf"));
        assert!(!matching("/books/**/a.pdf", "/books/x/ba.pdf"));
        assert!(matching("/books/?.pdf", "/books/a.pdf"));
        assert!(!matching("/books/?.pdf", "/books/ab.pdf"));
        assert!(!matching("/books/*.pdf", "/bookshelf/a.pdf"));
    }

    #[test]
    fn a_folder_is_everything_under_it_and_a_name_is_any_file_s() {
        assert!(matching("/scans/", "/scans/2024/receipt.pdf"));
        assert!(!matching("/scans/", "/scansion.pdf"));
        assert!(matching("*.cbz", "/anywhere/at/all/comic.cbz"));
        assert!(!matching("*.cbz", "/comics.cbz/inside.pdf"));
    }

    #[test]
    fn a_tilde_is_the_home_folder() {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let home = home.to_string_lossy().into_owned();
        assert!(matching("~/scans/**", &format!("{home}/scans/a.pdf")));
    }

    #[test]
    fn later_rules_win_and_only_document_settings_are_taken() {
        let rule = |pattern, settings: &[(&'static str, &str)]| {
            Rule::new(
                pattern,
                settings
                    .iter()
                    .map(|(name, value)| (*name, (*value).to_owned())),
            )
        };
        let rules = [
            rule("/scans/**", &[("recolor", "false"), ("trim", "true")]).unwrap(),
            rule("/scans/colour/**", &[("recolor", "auto"), ("columns", "2")]).unwrap(),
        ];
        assert_eq!(
            settings_for(&rules, Path::new("/scans/colour/a.pdf")),
            Settings {
                recolor: Some(Recolor::Auto),
                trim: Some(true),
                columns: Some(2),
            }
        );
        assert_eq!(
            settings_for(&rules, Path::new("/books/a.pdf")),
            Settings::default()
        );
        rule("/x/**", &[("theme", "pdvim-dark")]).unwrap_err();
        rule("/x/**", &[("trim", "maybe")]).unwrap_err();
        rule(" ", &[]).unwrap_err();
    }
}
