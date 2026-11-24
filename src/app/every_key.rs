//! Every key bound by default, in every mode, pressed in a session of its
//! own, and every command run: none may panic, and each must leave the
//! reader sound, its page one there is and its zoom one it can draw at.
//! Each failure is listed with the binding or command that caused it, all
//! of them in one run. Then the keys whose work can be seen in the app are
//! pressed again, each checked for doing what it is for.

use std::panic::{AssertUnwindSafe, catch_unwind};

use iced::Element;

use super::Message;
use super::testing::Session;
use crate::bindings::Keymaps;
use crate::command;
use crate::input::{self, Key};
use crate::keymap::{Bindable, Keymap};
use crate::reader::PageMode;

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
    let _: Element<'_, Message> = session.app.view();
    let _: super::mode::Mode = session.app.mode();
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
    ("export", &["$DIR/page.png", "$DIR/page.svg", "$DIR/notes.md"]),
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

/// The first key sequence bound by default in normal mode to the binding
/// of this name.
fn keys_of(name: &str) -> Vec<Key> {
    let keymaps = Keymaps::default();
    let keys = bound(&keymaps.normal)
        .into_iter()
        .find(|(_, bound_name)| *bound_name == name)
        .map(|(keys, _)| keys)
        .unwrap_or_default();
    assert!(!keys.is_empty(), "no key is bound to {name} by default");
    keys
}

/// Presses the keys bound to the binding of this name.
fn press_bound(session: &mut Session, name: &str) {
    for key in keys_of(name) {
        session.press(key);
    }
}

/// The settings each toggle turns, as words to compare.
fn toggled(session: &Session, name: &str) -> String {
    let app = &session.app;
    let document = app.document();
    match name {
        "toggle-trim" => document.is_some_and(|document| document.trim).to_string(),
        "toggle-bars" => format!("{} {}", app.bars.tabs, app.bars.status),
        "toggle-columns" => document
            .map_or(0, |document| document.reader.columns())
            .to_string(),
        "cycle-page-mode" => match app.paging.mode {
            PageMode::Continuous => "continuous",
            PageMode::Single => "single",
            PageMode::Double => "double",
        }
        .to_owned(),
        "toggle-borders" => app.paging.borders.to_string(),
        "toggle-fullscreen" => app.bars.fullscreen.to_string(),
        _ => String::new(),
    }
}

#[test]
fn each_toggle_changes_its_setting_and_comes_back_to_where_it_began() {
    let toggles = [
        "toggle-trim",
        "toggle-bars",
        "toggle-columns",
        "cycle-page-mode",
        "toggle-borders",
        "toggle-fullscreen",
    ];
    let mut failures = Vec::new();
    for name in toggles {
        let mut session = Session::open("links.pdf");
        let start = toggled(&session, name);
        let mut seen = vec![start.clone()];
        // A toggle comes back after two presses, the page modes after three.
        for _ in 0..3 {
            press_bound(&mut session, name);
            let now = toggled(&session, name);
            if now == start {
                break;
            }
            if seen.contains(&now) {
                failures.push(format!("{name} came back to {now}, not to {start}"));
                break;
            }
            seen.push(now);
        }
        if seen.len() < 2 {
            failures.push(format!("{name} left its setting at {start}"));
        } else if toggled(&session, name) != start {
            failures.push(format!("{name} never came back to {start}"));
        }
    }
    report(&failures);
}

/// The match the search is at, if any.
fn current_match(session: &Session) -> Option<usize> {
    session.app.document().and_then(|document| document.results.current)
}

#[test]
fn n_and_capital_n_go_through_the_matches_and_g_slash_lists_them() {
    let mut session = Session::open("text.pdf");
    session.keys("/o<Enter>");
    let first = current_match(&session);
    assert!(first.is_some(), "the search finds no match to start from");
    press_bound(&mut session, "search-next");
    let next = current_match(&session);
    assert_ne!(next, first, "search-next stays on the match");
    press_bound(&mut session, "search-previous");
    assert_eq!(current_match(&session), first, "search-previous does not go back");
    assert!(session.app.match_list.is_none(), "the matches are listed before asked");
    press_bound(&mut session, "matches");
    assert!(session.app.match_list.is_some(), "matches does not list them");
}

#[test]
fn the_document_is_saved_from_its_key() {
    let mut session = Session::open("text.pdf");
    // A line struck out, to have something to save.
    session.keys("vaaeS");
    let modified =
        |session: &Session| session.app.document().is_some_and(super::Document::modified);
    assert!(modified(&session), "striking out a line leaves nothing to save");
    press_bound(&mut session, "write");
    assert!(!modified(&session), "write leaves the change unsaved");
}

#[test]
fn the_last_selection_is_selected_again_from_its_key() {
    let mut session = Session::open("text.pdf");
    session.keys("vaae");
    let selected = session.selected();
    assert!(!selected.is_empty(), "nothing is selected to start with");
    session.keys("<Esc><Esc>");
    assert!(session.app.visual.is_none(), "escape leaves the selection");
    press_bound(&mut session, "select-again");
    assert_eq!(session.selected(), selected, "select-again selects something else, or nothing");
}

#[test]
fn the_table_of_contents_and_the_pages_small_show_from_their_keys() {
    let mut session = Session::open("outline.pdf");
    press_bound(&mut session, "outline");
    assert!(session.app.sidebar.is_some(), "outline shows no table of contents");
    let mut session = Session::open("outline.pdf");
    press_bound(&mut session, "overview");
    assert!(session.app.overview.is_some(), "overview shows no pages");
}

#[test]
fn a_mark_set_on_a_page_is_gone_back_to() {
    let mut session = Session::open("links.pdf");
    press_bound(&mut session, "set-mark");
    session.keys("a");
    session.keys("G");
    let page = |session: &Session| {
        session
            .app
            .document()
            .map(|document| document.reader.current_page())
    };
    assert_eq!(page(&session), Some(1), "G does not go to the last page");
    press_bound(&mut session, "go-to-mark");
    session.keys("a");
    assert_eq!(page(&session), Some(0), "the mark does not go back to the first page");
}

#[test]
fn the_path_the_title_and_the_page_are_copied_from_their_keys() {
    let mut session = Session::open("text.pdf");
    press_bound(&mut session, "yank-path");
    let path = session.copied.last().cloned().unwrap_or_default();
    assert!(path.ends_with("text.pdf"), "yank-path copied {path:?}");
    for name in ["yank-title", "yank-page"] {
        let before = session.copied.len();
        press_bound(&mut session, name);
        assert!(session.copied.len() > before, "{name} copies nothing");
    }
}

#[test]
fn f_labels_the_links_in_view() {
    let mut session = Session::open("links.pdf");
    press_bound(&mut session, "follow-link");
    assert!(
        session.app.hints.is_some() || session.app.marking_hints.is_some(),
        "follow-link labels nothing"
    );
}
