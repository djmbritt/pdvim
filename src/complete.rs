//! Completing what is typed after `:`.
//!
//! [`complete`] works out which word is being typed and what it could
//! become: a command name, a setting, a value, an action, or a path.

use std::path::{Path, PathBuf};

use crate::bindings::Mode;
use crate::command;
use crate::input::Binding;
use crate::keymap::Bindable;
use crate::theme::Theme;

/// The kinds of file offered after `open`: the documents pdvim reads. MuPDF
/// is built with PDF only, so other kinds would not open.
const DOCUMENTS: [&str; 1] = ["pdf"];

/// One way the word being typed could end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The whole word, to put in place of what was typed.
    pub text: String,
    /// What the list shows: the word, or the last part of a path.
    pub label: String,
    /// Whether the word is finished, so that a space can follow it. A
    /// directory is not: a name inside it may follow.
    pub whole: bool,
}

/// The candidates for the word that ends a line, and where that word
/// starts, in bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    pub start: usize,
    pub candidates: Vec<Candidate>,
}

/// What the word at the end of a command line could become.
pub fn complete(line: &str) -> Completion {
    let start = line.len().saturating_sub(line.trim_start().len());
    let Some((name, rest)) = line
        .get(start..)
        .and_then(|command| command.split_once(char::is_whitespace))
    else {
        // Still on the name: a command's, or an action's.
        let typed = line.get(start..).unwrap_or_default();
        let mut names: Vec<&str> = command::names().chain(Binding::names()).collect();
        names.sort_unstable();
        names.dedup();
        return Completion {
            start,
            candidates: words(typed, names),
        };
    };
    // Where the text after the name starts.
    let after = line.len().saturating_sub(rest.trim_start().len());
    let rest = rest.trim_start();
    // The last word, and where it starts.
    let last = line
        .len()
        .saturating_sub(rest.rsplit(char::is_whitespace).next().map_or(0, str::len));
    let last_word = line.get(last..).unwrap_or_default();
    let words_before = rest
        .split_whitespace()
        .count()
        .saturating_sub(usize::from(!last_word.is_empty()));
    let candidates = match command::canonical(name) {
        // A path may have spaces in it, so it is everything after the name.
        Some("theme") if words_before == 0 => {
            words(last_word, std::iter::once("auto").chain(Theme::names()))
        }
        Some("open" | "tabedit" | "tabnew") => {
            return Completion {
                start: after,
                candidates: paths(rest),
            };
        }
        Some("set") if words_before == 0 => words(last_word, command::SETTINGS),
        Some("set") if words_before == 1 => match rest.split_whitespace().next() {
            Some("autosave" | "confirm-quit" | "confirm-tab-close" | "statusbar" | "tabbar" | "trim") => {
                words(last_word, ["true", "false"])
            }
            Some("recolor") => words(last_word, ["true", "false", "auto"]),
            Some("theme") => words(last_word, std::iter::once("auto").chain(Theme::names())),
            Some("theme-dark" | "theme-light") => words(last_word, Theme::names()),
            Some("recolor-darkcolor" | "recolor-lightcolor") => words(last_word, ["theme"]),
            _ => Vec::new(),
        },
        // A mode may come first.
        Some("map" | "unmap" | "mapclear") if words_before == 0 => {
            words(last_word, Mode::ALL.iter().map(|mode| mode.name()))
        }
        // The action comes after at least one key, in the mode named first
        // or in normal mode.
        Some("map") => {
            let named = rest.split_whitespace().next().and_then(Mode::named);
            let keys = words_before.saturating_sub(usize::from(named.is_some()));
            if keys >= 1 {
                words(last_word, named.unwrap_or(Mode::Normal).names())
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    };
    Completion {
        start: last,
        candidates,
    }
}

/// The words that start with what was typed, as finished words.
fn words<'a>(typed: &str, known: impl IntoIterator<Item = &'a str>) -> Vec<Candidate> {
    known
        .into_iter()
        .filter(|word| word.starts_with(typed))
        .map(|word| Candidate {
            text: word.to_owned(),
            label: word.to_owned(),
            whole: true,
        })
        .collect()
}

/// The directories and documents whose paths start with what was typed. A
/// leading `~` stands for the home directory, and stays in the result.
fn paths(typed: &str) -> Vec<Candidate> {
    // The directory part, as typed, and the start of a name in it.
    let (directory, prefix) = typed
        .rfind('/')
        .map_or(("", typed), |slash| typed.split_at(slash.saturating_add(1)));
    let on_disk = expand_home(directory);
    let Ok(entries) = std::fs::read_dir(if directory.is_empty() {
        Path::new(".")
    } else {
        &on_disk
    }) else {
        return Vec::new();
    };
    let mut candidates: Vec<Candidate> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            // Hidden files only when a name starting with `.` is typed.
            if !name.starts_with(prefix) || (name.starts_with('.') && !prefix.starts_with('.')) {
                return None;
            }
            // Following links, so that a link to a directory is one.
            let is_directory = entry.path().is_dir();
            if !is_directory && !is_document(&name) {
                return None;
            }
            let label = if is_directory {
                format!("{name}/")
            } else {
                name
            };
            Some(Candidate {
                text: format!("{directory}{label}"),
                label,
                whole: false,
            })
        })
        .collect();
    candidates.sort_by_key(|candidate| candidate.label.to_lowercase());
    candidates
}

/// Whether a file name is that of a document pdvim can open.
fn is_document(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            DOCUMENTS
                .iter()
                .any(|known| known.eq_ignore_ascii_case(extension))
        })
}

/// Replaces a leading `~` with the home directory.
fn expand_home(path: &str) -> PathBuf {
    match (path.strip_prefix('~'), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) if rest.is_empty() || rest.starts_with('/') => {
            PathBuf::from(home).join(rest.trim_start_matches('/'))
        }
        _ => PathBuf::from(path),
    }
}

/// The longest start that all the candidates share.
pub fn common_start(candidates: &[Candidate]) -> &str {
    let Some((first, others)) = candidates.split_first() else {
        return "";
    };
    let mut end = first.text.len();
    for other in others {
        end = first
            .text
            .char_indices()
            .zip(other.text.chars())
            .find(|((_, mine), theirs)| mine != theirs)
            .map_or_else(
                || end.min(other.text.len()),
                |((index, _), _)| index.min(end),
            );
    }
    first.text.get(..end).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(completion: &Completion) -> Vec<&str> {
        completion
            .candidates
            .iter()
            .map(|candidate| candidate.text.as_str())
            .collect()
    }

    #[test]
    fn command_names_complete() {
        let completion = complete("tab");
        assert_eq!(completion.start, 0);
        assert_eq!(
            texts(&completion),
            [
                "tabclose",
                "tabedit",
                "tabmove",
                "tabnew",
                "tabnext",
                "tabonly",
                "tabprevious"
            ]
        );
        // Actions are offered too, once each when a command shares a name.
        assert_eq!(
            texts(&complete("scroll-")),
            [
                "scroll-down",
                "scroll-full-down",
                "scroll-full-up",
                "scroll-half-down",
                "scroll-half-up",
                "scroll-left",
                "scroll-right",
                "scroll-up"
            ]
        );
        assert_eq!(texts(&complete("qu")), ["quit"]);
        assert_eq!(texts(&complete("r")), ["recent", "redo", "reopen-tab", "rotate"]);
        assert_eq!(texts(&complete("  rec")), ["recent"]);
        assert_eq!(complete("  rec").start, 2);
        assert_eq!(texts(&complete("map")), ["map", "mapclear"]);
        assert_eq!(texts(&complete("zzz")), Vec::<&str>::new());
    }

    #[test]
    fn settings_and_their_values_complete() {
        let completion = complete("set recolor-");
        assert_eq!(completion.start, 4);
        assert_eq!(
            texts(&completion),
            ["recolor-darkcolor", "recolor-lightcolor"]
        );
        assert_eq!(texts(&complete("set ")).len(), command::SETTINGS.len());
        let completion = complete("set trim f");
        assert_eq!(completion.start, 9);
        assert_eq!(texts(&completion), ["false"]);
        assert_eq!(texts(&complete("set scroll-step ")), Vec::<&str>::new());
    }

    #[test]
    fn actions_complete_after_the_keys() {
        assert_eq!(texts(&complete("map j")), Vec::<&str>::new());
        let completion = complete("map g x next-");
        assert_eq!(completion.start, 8);
        assert_eq!(
            texts(&completion),
            ["next-page", "next-chapter", "next-marking", "next-tab"]
        );
        assert_eq!(
            texts(&complete("map ctrl+n ")).len(),
            Binding::names().count()
        );
    }

    #[test]
    fn themes_complete_after_theme() {
        assert_eq!(
            texts(&complete("colo catppuccin-m")),
            ["catppuccin-macchiato", "catppuccin-mocha"]
        );
        assert_eq!(texts(&complete("theme au")), ["auto"]);
        assert_eq!(
            texts(&complete("set theme pdvim-")),
            ["pdvim-dark", "pdvim-light"]
        );
    }

    #[test]
    fn a_mode_completes_first_then_its_actions() {
        assert_eq!(texts(&complete("map vi")), ["visual"]);
        assert_eq!(texts(&complete("unmap o")), ["outline"]);
        assert_eq!(texts(&complete("map visual ")), Vec::<&str>::new());
        assert_eq!(texts(&complete("map visual Y ya")), ["yank"]);
        assert_eq!(texts(&complete("map v vis")), ["visual", "visual-line"]);
    }

    #[test]
    fn paths_complete_to_directories_and_documents() {
        let root = std::env::temp_dir().join(format!("pdvim-complete-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Papers")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        for file in [
            "paper one.pdf",
            "Book.PDF",
            "story.epub",
            "notes.txt",
            ".secret.pdf",
        ] {
            std::fs::write(root.join(file), "").unwrap();
        }
        let base = format!("{}/", root.display());

        let completion = complete(&format!("open {base}"));
        assert_eq!(completion.start, 5);
        let labels: Vec<_> = completion
            .candidates
            .iter()
            .map(|candidate| candidate.label.as_str())
            .collect();
        assert_eq!(labels, ["Book.PDF", "paper one.pdf", "Papers/"]);
        assert!(
            completion
                .candidates
                .iter()
                .all(|candidate| !candidate.whole)
        );

        // Names are matched with case, as in vim.
        let completion = complete(&format!("tabe {base}pap"));
        assert_eq!(texts(&completion), [format!("{base}paper one.pdf")]);
        let completion = complete(&format!("tabe {base}Pap"));
        assert_eq!(texts(&completion), [format!("{base}Papers/")]);
        assert_eq!(
            texts(&complete(&format!("o {base}.")))
                .iter()
                .map(|text| text.trim_start_matches(&base))
                .collect::<Vec<_>>(),
            [".hidden/", ".secret.pdf"]
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_tilde_stays_in_the_completed_path() {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        if !Path::new(&home).is_dir() {
            return;
        }
        let completion = complete("open ~/");
        assert!(
            completion
                .candidates
                .iter()
                .all(|candidate| candidate.text.starts_with("~/"))
        );
    }

    #[test]
    fn the_common_start_is_found() {
        let candidate = |text: &str| Candidate {
            text: text.to_owned(),
            label: text.to_owned(),
            whole: true,
        };
        assert_eq!(common_start(&[]), "");
        assert_eq!(
            common_start(&[candidate("tabnew"), candidate("tabnext")]),
            "tabne"
        );
        assert_eq!(
            common_start(&[candidate("map"), candidate("mapclear")]),
            "map"
        );
        assert_eq!(common_start(&[candidate("été"), candidate("étage")]), "ét");
    }
}
