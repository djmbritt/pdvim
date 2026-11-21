//! Entry points for fuzzing: each takes arbitrary bytes, drives a part of
//! the reader with them, and panics if that part panics or breaks a promise
//! the rest of the reader relies on. The targets in `fuzz/` call these, so
//! that the harness can reach what the crate keeps to itself.
//!
//! Bytes that make no sense to a part, such as a configuration that is not
//! UTF-8, are not rejected but read leniently, so that every input tests
//! something.

use std::path::PathBuf;

use crate::backend::Chapter;
use crate::backend::{self, TextChar, TextLine};
use crate::bindings::{Bound, Keymaps};
use crate::command::{self, Command};
use crate::complete;
use crate::config;
use crate::help::Help;
use crate::hints::{self, Hints};
use crate::history::{Fit, View};
use crate::input::Key;
use crate::keymap::{self, Counts, Pending};
use crate::layout::{PageSize, Rect, Rotation};
use crate::num;
use crate::outline::{self, Outline};
use crate::reader::Reader;
use crate::picker::{Choice, Entry, Picker};
use crate::visual::{self, Kind, Spot, Text, Visual};

/// The keys the reader tells apart that type no character.
const NAMED: [Key; 16] = [
    Key::Escape,
    Key::Enter,
    Key::Backspace,
    Key::ShiftSpace,
    Key::Tab,
    Key::ShiftTab,
    Key::CtrlTab,
    Key::CtrlShiftTab,
    Key::Up,
    Key::Down,
    Key::Left,
    Key::Right,
    Key::PageUp,
    Key::PageDown,
    Key::Home,
    Key::End,
];

/// Characters beyond ASCII, for keys and for text on pages.
const WIDE: [char; 8] = ['é', 'ß', 'Ω', '中', '—', '\u{a0}', '\u{301}', '😀'];

/// The letters held with control that the reader gives a meaning to, and a
/// few it does not.
const CONTROL: [char; 16] = [
    'a', 'b', 'c', 'd', 'e', 'f', 'h', 'j', 'k', 'n', 'p', 'r', 'u', 'y', 'z', '[',
];

/// Characters a line of text on a page is made of: letters, spaces and
/// punctuation, which the motions over words tell apart.
const PRINTED: [char; 12] = ['a', 'b', 'x', 'A', ' ', ' ', '.', ',', '-', '\'', 'é', '中'];

/// The most pages a document is read through, so that one input does not
/// take long.
const PAGES: u32 = 4;

/// Bytes read from the front, for building the parts of an input in turn.
/// Once they run out every read gives zero.
struct Bytes<'a>(&'a [u8]);

impl Bytes<'_> {
    const fn byte(&mut self) -> u8 {
        let Some((&first, rest)) = self.0.split_first() else {
            return 0;
        };
        self.0 = rest;
        first
    }

    /// A number below `bound`, or 0 when `bound` is 0.
    fn below(&mut self, bound: usize) -> usize {
        usize::from(self.byte()).checked_rem(bound).unwrap_or(0)
    }

    fn pick<T: Copy>(&mut self, from: &[T], default: T) -> T {
        from.get(self.below(from.len())).copied().unwrap_or(default)
    }

    /// The bytes not yet read, as keys.
    fn keys(&self) -> impl Iterator<Item = Key> + '_ {
        self.0.iter().map(|&byte| key(byte))
    }
}

/// A key for a byte: printable ASCII types itself, and the rest stand for
/// named keys, characters beyond ASCII and letters held with control.
fn key(byte: u8) -> Key {
    let rest = usize::from(byte);
    match byte {
        0x20..=0x7e => Key::Char(char::from(byte)),
        0x00..=0x0f => NAMED.get(rest).copied().unwrap_or(Key::Escape),
        0x10..=0x1f => Key::Char(
            WIDE.get(rest.checked_rem(WIDE.len()).unwrap_or(0))
                .copied()
                .unwrap_or('é'),
        ),
        _ => Key::Ctrl(
            CONTROL
                .get(rest.checked_rem(CONTROL.len()).unwrap_or(0))
                .copied()
                .unwrap_or('c'),
        ),
    }
}

/// Reads a configuration file.
pub fn config(data: &[u8]) {
    let text = String::from_utf8_lossy(data);
    let (commands, problems) = config::parse(&text);
    for problem in &problems {
        assert!(!problem.0.is_empty(), "a problem with no description");
    }
    // The commands are applied to the bindings the way the app applies
    // them, and the cheat sheet lists the result.
    let mut keymaps = Keymaps::default();
    for command in commands {
        match command {
            Command::Map(keys, bound) => {
                assert!(!keys.is_empty(), "a binding of no keys");
                keymaps.bind(keys.clone(), bound);
                assert!(
                    bound_to(&keymaps, &keys, bound),
                    "{keys:?} is not bound to {bound:?}"
                );
            }
            Command::Unmap(mode, keys) => {
                keymaps.unbind(mode, &keys);
            }
            Command::MapClear(mode) => keymaps.clear(mode),
            _ => {}
        }
    }
    let help = Help::new(&keymaps);
    let _ = help.shown(usize::MAX);
}

/// Whether keys are bound to something, in its mode.
fn bound_to(keymaps: &Keymaps, keys: &[Key], bound: Bound) -> bool {
    use keymap::Match;
    match bound {
        Bound::Normal(to) => keymaps.normal.lookup(keys) == Match::Bound(to),
        Bound::Visual(to) => keymaps.visual.lookup(keys) == Match::Bound(to),
        Bound::Hints(to) => keymaps.hints.lookup(keys) == Match::Bound(to),
        Bound::Outline(to) => keymaps.outline.lookup(keys) == Match::Bound(to),
        Bound::Markings(to) => keymaps.markings.lookup(keys) == Match::Bound(to),
        Bound::Recent(to) => keymaps.recent.lookup(keys) == Match::Bound(to),
        Bound::Help(to) => keymaps.help.lookup(keys) == Match::Bound(to),
        Bound::Prompt(to) => keymaps.prompt.lookup(keys) == Match::Bound(to),
        Bound::Confirm(to) => keymaps.confirm.lookup(keys) == Match::Bound(to),
    }
}

/// Parses a line typed after `:`, and completes it.
pub fn command(data: &[u8]) {
    let line = String::from_utf8_lossy(data);
    let _ = command::parse(&line);
    // Paths are completed from the files on disk, which would make the
    // outcome depend on the machine; the rest is completed from lists.
    let name = line.split_whitespace().next().unwrap_or_default();
    if line.trim_start().contains(char::is_whitespace)
        && matches!(
            command::canonical(name),
            Some("open" | "tabedit" | "tabnew")
        )
    {
        return;
    }
    let completion = complete::complete(&line);
    // The app replaces the line from `start` on with a candidate.
    assert!(
        line.is_char_boundary(completion.start),
        "completion starts inside a character: {:?} at {}",
        line,
        completion.start
    );
    let common = complete::common_start(&completion.candidates);
    for candidate in &completion.candidates {
        assert!(
            candidate.text.starts_with(common),
            "{common:?} does not start {:?}",
            candidate.text
        );
    }
}

/// Parses keys as the configuration writes them. Keys that parse are
/// written back as the cheat sheet shows them, which must parse to the same
/// keys.
pub fn keys(data: &[u8]) {
    let written = String::from_utf8_lossy(data);
    let Ok(keys) = keymap::parse_keys(&written) else {
        return;
    };
    assert!(!keys.is_empty(), "{written:?} parsed to no keys");
    let shown: Vec<String> = keys.iter().map(ToString::to_string).collect();
    let shown = shown.join(" ");
    assert_eq!(
        keymap::parse_keys(&shown).ok(),
        Some(keys),
        "{written:?} is shown as {shown:?}, which reads differently"
    );
}

/// Types keys at each of the modes that take them.
///
/// The modes are normal mode, the cheat sheet, the link labels, the
/// recent documents, the table of contents and visual mode, each with the
/// built-in bindings. The first bytes shape the lists and text the modes
/// work over; the rest are keys.
pub fn modes(data: &[u8]) {
    let mut bytes = Bytes(data);
    let rows = bytes.below(12);
    let labels = bytes.below(800);
    let recent = bytes.below(30);
    let chapters = chapters(&mut bytes, 0);
    let page = u32::from(bytes.byte());
    let text = text(&mut bytes);
    let lines: Vec<(u32, usize)> = text
        .iter()
        .flat_map(|(&page, lines)| (0..lines.len()).map(move |line| (page, line)))
        .collect();
    let start = bytes.below(lines.len());
    let kind = if bytes.byte() & 1 == 0 {
        Kind::Characters
    } else {
        Kind::Lines
    };

    let keymaps = Keymaps::default();
    let mut pending = Pending::new();
    let mut help = Help::new(&keymaps);
    let letters: Vec<char> = crate::hints::HOME_ROW.chars().collect();
    let mut hints = Hints::new(
        (0..num::len(labels)).map(|item| (item, item)).collect(),
        &letters,
    );
    if let Some(hints) = &mut hints {
        hints.set_texts(|page, _| page.to_string());
    }
    let mut picker = Picker::new(
        (0..recent)
            .map(|entry| Entry {
                text: format!("/{entry}.pdf"),
                value: PathBuf::from(format!("/{entry}.pdf")),
            })
            .collect(),
    );
    let mut sidebar = Outline::new(&chapters, page);
    let mut selecting = lines
        .get(start)
        .map(|&(page, line)| Visual::new(kind, Spot::line_start(page, line)));

    for key in bytes.keys() {
        let _ = pending.feed(key, &keymaps.normal, Counts::Taken);
        let _ = pending.display();
        help_key(&mut help, &keymaps, key, rows);
        if let Some(labelled) = &mut hints
            && !hints_key(labelled, &keymaps, key)
        {
            hints = None;
        }
        if let Some(list) = &mut picker
            && !picker_key(list, &keymaps, key)
        {
            picker = None;
        }
        if let Some(contents) = &mut sidebar
            && !outline_key(contents, &keymaps, key, rows)
        {
            sidebar = None;
        }
        if let Some(selection) = &mut selecting
            && !visual_key(selection, &keymaps, key, &text)
        {
            selecting = None;
        }
    }
}

fn help_key(help: &mut Help, keymaps: &Keymaps, key: Key, rows: usize) {
    if !help.feed(key, &keymaps.help, rows) {
        *help = Help::new(keymaps);
    }
    let (top, count) = help.position();
    let shown = help.shown(rows).len();
    assert!(top <= count, "the cheat sheet is scrolled past its end");
    assert_eq!(shown, rows.min(count.saturating_sub(top)));
    if count >= rows {
        assert_eq!(shown, rows, "the cheat sheet leaves rows empty");
    }
}

/// Returns whether the labels stay up.
fn hints_key(hints: &mut Hints<u32>, keymaps: &Keymaps, key: Key) -> bool {
    match hints.feed(key, &keymaps.hints) {
        hints::Outcome::Wait => {
            let typed = hints.typed();
            assert!(
                hints
                    .shown()
                    .all(|hint| typed.is_none_or(|typed| hint.label[0] == typed)),
                "a label shows that does not start with {typed:?}"
            );
            assert!(hints.shown().next().is_some(), "waiting with no labels");
            true
        }
        hints::Outcome::Picked(hint) => {
            // Each thing was labelled with its own number.
            assert_eq!(hint.page, hint.item);
            false
        }
        hints::Outcome::Cancel | hints::Outcome::NoMatch => false,
    }
}

/// Returns whether the list stays up.
fn picker_key(picker: &mut Picker<PathBuf>, keymaps: &Keymaps, key: Key) -> bool {
    let outcome = picker.feed(key, &keymaps.recent);
    let shown = picker.shown().count();
    assert!(picker.selected() < shown.max(1));
    match outcome {
        Some(Choice::Open(path)) => {
            assert!(picker.shown().any(|entry| entry.value == path));
            false
        }
        Some(Choice::Close) => false,
        None => true,
    }
}

/// Returns whether the sidebar stays up.
fn outline_key(sidebar: &mut Outline, keymaps: &Keymaps, key: Key, rows: usize) -> bool {
    let outcome = sidebar.feed(key, &keymaps.outline, rows);
    let shown = sidebar.shown(rows.max(1));
    assert_eq!(
        shown.iter().filter(|line| line.selected).count(),
        1,
        "the selected entry is not shown"
    );
    let (top, count) = sidebar.position(rows);
    assert!(top < count, "the sidebar is scrolled past its end");
    !matches!(outcome, outline::Outcome::Go(..) | outline::Outcome::Close)
}

/// Returns whether visual mode goes on.
fn visual_key(selection: &mut Visual, keymaps: &Keymaps, key: Key, text: &Text) -> bool {
    let outcome = selection.feed(key, &keymaps.visual, text);
    let cursor = selection.cursor();
    assert!(
        visual::character(text, cursor).is_some(),
        "the cursor is off the text: {cursor:?}"
    );
    let areas = selection.areas(text);
    assert!(!areas.is_empty(), "nothing is selected");
    let selected = selection.text(text);
    let _ = selection.neighbours(PAGES);
    match outcome {
        visual::Outcome::Stay | visual::Outcome::Flash => true,
        // Spaces at the ends of lines are left out, so a selection of
        // spaces copies nothing.
        visual::Outcome::Copy(copied) => {
            assert_eq!(copied, selected);
            false
        }
        visual::Outcome::Mark(_, areas) | visual::Outcome::Unmark(areas) => {
            assert!(!areas.is_empty(), "marked nothing");
            false
        }
        visual::Outcome::Note(page, _) | visual::Outcome::ChangeNote(page, _) => {
            assert_eq!(page, cursor.page, "a note away from the cursor");
            false
        }
        visual::Outcome::Leave => false,
    }
}

/// A table of contents a few levels deep.
fn chapters(bytes: &mut Bytes<'_>, depth: u8) -> Vec<Chapter> {
    let count = if depth >= 3 { 0 } else { bytes.below(4) };
    (0..count)
        .map(|index| {
            let leads = bytes.byte();
            Chapter {
                title: format!("{depth}.{index}"),
                // Some entries lead nowhere.
                page: (leads & 1 == 0).then(|| u32::from(leads >> 1)),
                top: (leads & 2 == 0).then(|| f32::from(leads)),
                sections: chapters(bytes, depth.saturating_add(1)),
            }
        })
        .collect()
}

/// The text of a few pages, as the backend reads it: lines with at least
/// one character each, on pages that may not have been read.
fn text(bytes: &mut Bytes<'_>) -> Text {
    let mut text = Text::new();
    for page in 0..PAGES {
        let lines = bytes.below(5);
        if lines == 0 {
            continue;
        }
        let lines = (0..lines)
            .map(|line| {
                let length = bytes.below(9).max(1);
                TextLine {
                    chars: (0..length)
                        .map(|index| TextChar {
                            character: bytes.pick(&PRINTED, 'a'),
                            area: cell(line, index),
                        })
                        .collect(),
                }
            })
            .collect();
        text.insert(page, lines);
    }
    text
}

/// Where a character sits: a grid of cells ten points across and twelve
/// down.
fn cell(line: usize, index: usize) -> Rect {
    let left = num::float(num::len(index)) * 10.0;
    let top = num::float(num::len(line)) * 12.0;
    Rect {
        left,
        top,
        right: left + 9.0,
        bottom: top + 11.0,
    }
}

/// Scrolls, zooms and turns a view over pages of any size, as the keys do,
/// and as a view saved in the history is put back. The first bytes give the
/// pages and the window.
pub fn reader(data: &[u8]) {
    let mut bytes = Bytes(data);
    let count = bytes.below(6);
    let sizes: Vec<PageSize> = (0..count)
        .map(|_| PageSize {
            width: f32::from(bytes.byte()) * 8.0,
            height: f32::from(bytes.byte()) * 8.0,
        })
        .collect();
    let mut window = (
        f32::from(bytes.byte()) * 16.0,
        f32::from(bytes.byte()) * 16.0,
    );
    let mut reader = Reader::new(sizes.clone(), window);
    while !bytes.0.is_empty() {
        let amount = f32::from(bytes.byte()) - 128.0;
        let whole = u32::from(bytes.byte());
        match bytes.byte().checked_rem(17).unwrap_or(0) {
            0 => reader.scroll_by(amount, 0.0),
            1 => reader.scroll_by(0.0, amount * 40.0),
            2 => reader.scroll_screens(amount / 16.0),
            3 => reader.page_by(i32::from(bytes.byte()).saturating_sub(128)),
            4 => reader.go_to_page(whole),
            5 => reader.go_to_spot(whole, (amount >= 0.0).then_some(amount * 8.0)),
            6 => reader.page_top(),
            7 => reader.page_bottom(),
            8 => reader.zoom_in(),
            9 => reader.zoom_out(),
            10 => reader.zoom_to(f32::from(bytes.byte()) / 50.0),
            11 => reader.fit(if amount >= 0.0 { Fit::Page } else { Fit::Width }),
            12 => reader.rotate(),
            13 => {
                window = (
                    f32::from(bytes.byte()) * 16.0,
                    f32::from(bytes.byte()) * 16.0,
                );
                reader.set_window(window);
            }
            14 => {
                // Trimming: the content of each page, inside the page.
                let crops = (amount >= 0.0).then(|| {
                    sizes
                        .iter()
                        .map(|size| Rect {
                            left: size.width / 4.0,
                            top: size.height / 8.0,
                            right: size.width / 2.0,
                            bottom: size.height,
                        })
                        .collect()
                });
                reader.set_crops(crops);
            }
            15 => reader.reveal(
                whole,
                Rect {
                    left: amount,
                    top: amount * 4.0,
                    right: amount + 30.0,
                    bottom: amount * 4.0 + 12.0,
                },
            ),
            _ => {
                // A view from the history, which is only as sound as the
                // database it was read from.
                let rotation = match bytes.byte() & 3 {
                    0 => Rotation::None,
                    1 => Rotation::Quarter,
                    2 => Rotation::Half,
                    _ => Rotation::ThreeQuarters,
                };
                reader.restore(View {
                    x: amount * 10.0,
                    y: f32::from(bytes.byte()) * 100.0,
                    zoom: f32::from(bytes.byte()) / 20.0,
                    fit: Fit::None,
                    rotation,
                    recolor: false,
                    trim: false,
                });
            }
        }
        check_reader(&reader, count);
    }
}

fn check_reader(reader: &Reader, count: usize) {
    let pages = reader.page_count();
    assert_eq!(num::index(pages), count);
    let zoom = reader.zoom();
    assert!(zoom.is_finite() && zoom > 0.0, "zoom is {zoom}");
    let view = reader.view(false);
    assert!(
        view.x.is_finite() && view.y.is_finite(),
        "the view is at {}, {}",
        view.x,
        view.y
    );
    if pages == 0 {
        return;
    }
    let current = reader.current_page();
    assert!(current < pages, "page {current} of {pages}");
    let visible = reader.visible_pages();
    assert!(visible.end <= pages, "pages {visible:?} of {pages}");
    for page in visible {
        let _ = reader.page_on_screen(page);
    }
}

/// Opens bytes as a PDF and reads the first pages the way the reader does:
/// their sizes, links, text, matches, margins and pixels, and the table of
/// contents.
pub fn pdf(data: &[u8]) {
    backend::examine(data, PAGES);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every target takes every byte, and nothing at all.
    #[test]
    fn the_targets_take_anything() {
        let all: Vec<u8> = (0..=u8::MAX).collect();
        let reversed: Vec<u8> = all.iter().rev().copied().collect();
        for data in [&[][..], &all, &reversed, b"jjjkkGgg\x04\x05"] {
            config(data);
            command(data);
            keys(data);
            modes(data);
            reader(data);
            pdf(data);
        }
    }

    #[test]
    fn the_example_configuration_reads() {
        config(include_bytes!("../defaults.toml"));
    }

    #[test]
    fn keys_go_round() {
        for written in [
            "g g",
            "ctrl+d",
            "shift+tab",
            "space",
            "Q",
            "shift+q",
            "+",
            "<",
        ] {
            keys(written.as_bytes());
        }
    }

    #[test]
    fn commands_complete() {
        for line in [
            "",
            "se",
            "set rec",
            "set recolor t",
            "map g g fi",
            "é",
            " é x",
        ] {
            command(line.as_bytes());
        }
    }

    #[test]
    fn modes_take_long_typing() {
        // Labels, a sidebar and text on every page, then every key twice.
        let mut data = vec![5, 200, 10, 3, 0, 2, 1, 0, 4];
        data.extend([4, 8, 1, 2, 3, 4, 5, 6, 7, 8].repeat(4));
        data.extend([0, 0]);
        data.extend((0..=u8::MAX).chain(0..=u8::MAX));
        modes(&data);
    }
}
