//! Every key bound by default, in every mode, pressed in a session of its
//! own, and every command run: none may panic, and each must leave the
//! reader sound, its page one there is and its zoom one it can draw at.
//! Each failure is listed with the binding or command that caused it, all
//! of them in one run.

use std::panic::{AssertUnwindSafe, catch_unwind};

use super::testing::Session;
use crate::bindings::Keymaps;
use crate::command;
use crate::input::{self, Key};
use crate::keymap::{Bindable, Keymap};

/// The documents pressed at: one with links over two pages, and one with
/// text to select.
const FIXTURES: [&str; 2] = ["links.pdf", "text.pdf"];

/// What is wrong with the reader after something was done, if anything.
fn unsound(session: &Session) -> Option<String> {
    if let Some(document) = session.app.document() {
        let reader = &document.reader;
        let pages = reader.page_count();
        if reader.current_page() >= pages {
            return Some(format!("page {} of {pages}", reader.current_page()));
        }
        let zoom = reader.zoom();
        if !zoom.is_finite() || zoom <= 0.0 {
            return Some(format!("zoom {zoom}"));
        }
    }
    // The window can still be made of it.
    let _ = session.app.view();
    let _ = session.app.mode();
    None
}

/// Runs a session through `steps`, and says what went wrong: a panic, or
/// the reader left unsound.
fn tried(fixture: &str, steps: impl FnOnce(&mut Session)) -> Option<String> {
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let mut session = Session::open(fixture);
        steps(&mut session);
        unsound(&session)
    }));
    match outcome {
        Ok(problem) => problem,
        Err(panic) => Some(format!(
            "panics: {}",
            panic
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("?")
        )),
    }
}

/// Every key sequence bound in a keymap, with the name of what it is bound
/// to.
fn bound<B: Bindable + 'static>(keymap: &Keymap<B>) -> Vec<(Vec<Key>, &'static str)> {
    B::ALL
        .iter()
        .flat_map(|&(binding, name, _)| {
            keymap
                .keys_for(binding)
                .map(move |keys| (keys.to_vec(), name))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Presses every key sequence bound in a mode, once in each fixture and
/// again at once after, as a toggle is pressed twice: `enter` brings the
/// session into the mode first, and escape leaves after. Lists what went
/// wrong.
fn press_all(mode: &str, keys: &[(Vec<Key>, &'static str)], enter: &str) -> Vec<String> {
    let mut failures = Vec::new();
    for fixture in FIXTURES {
        for (sequence, name) in keys {
            let problem = tried(fixture, |session| {
                session.keys(enter);
                for _ in 0..2 {
                    for &key in sequence {
                        session.press(key);
                    }
                }
                session.keys("<Esc><Esc>");
            });
            if let Some(problem) = problem {
                failures.push(format!("{mode} {name} in {fixture}: {problem}"));
            }
        }
    }
    failures
}

/// Fails listing every failure, if there are any.
fn report(failures: &[String]) {
    assert!(
        failures.is_empty(),
        "{} failures:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

#[test]
fn every_key_of_normal_mode() {
    let keymaps = Keymaps::default();
    report(&press_all("normal", &bound(&keymaps.normal), ""));
}

#[test]
fn every_key_of_visual_mode() {
    let keymaps = Keymaps::default();
    // The first line's label starts selecting there.
    report(&press_all("visual", &bound(&keymaps.visual), "vaa"));
}

#[test]
fn every_key_of_the_lists_and_labels() {
    let keymaps = Keymaps::default();
    let mut failures = Vec::new();
    failures.extend(press_all("hints", &bound(&keymaps.hints), "f"));
    failures.extend(press_all("outline", &bound(&keymaps.outline), "<Tab>"));
    failures.extend(press_all("overview", &bound(&keymaps.overview), "go"));
    failures.extend(press_all("markings", &bound(&keymaps.markings), "<S-Tab>"));
    failures.extend(press_all("recent", &bound(&keymaps.recent), "o"));
    failures.extend(press_all("help", &bound(&keymaps.help), ":help<Enter>"));
    failures.extend(press_all("prompt", &bound(&keymaps.prompt), ":tab"));
    report(&failures);
}

/// What each command is run with, beyond running it bare: an argument of
/// the kind it takes. `$DIR` is the session's own directory. The shell
/// command `!` is left out: it would run on the machine.
const ARGUMENTS: &[(&str, &[&str])] = &[
    ("apropos", &["zoom"]),
    ("delmarks", &["a", "!"]),
    ("export", &["$DIR/page.png", "$DIR/page.svg"]),
    ("goto", &["2", "xii"]),
    ("help", &["visual"]),
    ("man", &["5"]),
    ("map", &["normal Q zoom-in", "x :zoom-to 150"]),
    ("mapclear", &["visual"]),
    ("note", &["a note"]),
    ("open", &["$DIR/text.pdf", "$DIR/missing.pdf"]),
    ("set", &["columns 2", "page-mode double", "backdrop paper", "recolor true"]),
    ("source", &["$DIR/config.toml"]),
    ("tabedit", &["$DIR/text.pdf"]),
    ("tabmove", &["0", "+1", "-1"]),
    ("tabnext", &["2"]),
    ("theme", &["catppuccin-latte", "nonsense"]),
    ("unmap", &["normal j"]),
    ("write", &["$DIR/copy.pdf"]),
];

#[test]
fn every_command_and_every_action_by_name() {
    let mut lines: Vec<String> = Vec::new();
    for name in command::names() {
        lines.push(name.to_owned());
        let arguments = ARGUMENTS
            .iter()
            .find(|(command, _)| *command == name)
            .map_or(&[][..], |(_, arguments)| arguments);
        lines.extend(arguments.iter().map(|argument| format!("{name} {argument}")));
    }
    lines.extend(input::Binding::ALL.iter().map(|(_, name, _)| (*name).to_owned()));
    lines.extend(["3", "40%", "0%", "zoom-to 150", "next-page 2"].map(str::to_owned));
    let mut failures = Vec::new();
    for fixture in FIXTURES {
        for line in &lines {
            let problem = tried(fixture, |session| {
                let dir = session.dir.display().to_string();
                let typed = line.replace("$DIR", &dir);
                session.keys(":");
                for character in typed.chars() {
                    session.press(Key::Char(character));
                }
                session.keys("<Enter><Esc><Esc>");
            });
            if let Some(problem) = problem {
                failures.push(format!(":{line} in {fixture}: {problem}"));
            }
        }
    }
    report(&failures);
}
