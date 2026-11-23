//! The text handling tried in many languages and scripts: finding
//! sentences, copying, searching, and moving the cursor, each over the
//! same sample in every language. Most of pdvim's tests are in English; a
//! rule that only holds for it, such as that a sentence starts with a
//! capital, shows up here as a language it fails in.
//!
//! The samples are Article 1 of the Universal Declaration of Human
//! Rights, the text most often translated, in a language of each kind of
//! script: alphabets with and without capitals, abjads written right to
//! left, abugidas whose vowels are marks on consonants, and Chinese,
//! Japanese and Thai, which put no spaces between words. Each test checks
//! every language and lists all that fail, so that one run shows where
//! pdvim stands.

use std::collections::HashMap;
use std::path::Path;

use mupdf::Document;

use crate::backend::{self, Needle, TextChar, TextLine};
use crate::bindings::Keymaps;
use crate::input::Key;
use crate::layout::Rect;
use crate::visual::{Kind, Spot, Text, Visual};

/// A language's sample: its sentences, and whether it puts spaces between
/// words, and so between sentences and at the ends of lines.
#[derive(Clone)]
struct Sample {
    code: String,
    language: String,
    sentences: Vec<String>,
    spaced: bool,
}

/// The samples, from the file the PDFs are made from too.
fn samples() -> Vec<Sample> {
    include_str!("../fuzz/languages/samples.tsv")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            Sample {
                code: fields[0].to_owned(),
                language: fields[1].to_owned(),
                spaced: fields[2] == "spaced",
                sentences: fields[5..].iter().map(|&sentence| sentence.to_owned()).collect(),
            }
        })
        .collect()
}

/// Where a page of text comes from: laid out here, or read by MuPDF from
/// the sample's PDF.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Laid,
    Pdf,
}

/// A page each check runs over: a sample, laid out here or as MuPDF reads
/// it from its PDF, named by its language and which.
struct Page {
    name: String,
    sample: Sample,
    source: Source,
    text: Text,
}

/// Every sample, laid out here and as read from its PDF.
fn pages() -> Vec<Page> {
    samples()
        .into_iter()
        .flat_map(|sample| {
            let laid = Page {
                name: sample.language.clone(),
                text: sample.page(),
                source: Source::Laid,
                sample: sample.clone(),
            };
            let read = Page {
                name: format!("{} (PDF)", sample.language),
                text: sample.pdf_page(),
                source: Source::Pdf,
                sample,
            };
            [laid, read]
        })
        .collect()
}

impl Sample {
    /// The whole text, as written.
    fn text(&self) -> String {
        self.sentences.join(if self.spaced { " " } else { "" })
    }

    /// The page of the sample's PDF, its text as MuPDF reads it.
    fn pdf_page(&self) -> Text {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fuzz/seeds/pdf")
            .join(format!("lang-{}.pdf", self.code));
        let bytes = std::fs::read(&path).unwrap();
        let document = Document::from_bytes(&bytes, "application/pdf").unwrap();
        HashMap::from([(0, backend::page_text(&document, 0).unwrap())])
    }

    /// The text broken into lines as a narrow column would break it:
    /// between words where there are spaces, anywhere where there are
    /// not.
    fn lines(&self) -> Vec<String> {
        let text = self.text();
        if !self.spaced {
            // Sixteen characters to a line, but never a mark apart from
            // the letter it is on.
            let mut lines: Vec<String> = Vec::new();
            for character in text.chars() {
                match lines.last_mut() {
                    Some(line) if line.chars().count() < 16 || is_mark(character) => {
                        line.push(character);
                    }
                    _ => lines.push(character.to_string()),
                }
            }
            return lines;
        }
        let mut lines: Vec<String> = Vec::new();
        for word in text.split(' ') {
            match lines.last_mut() {
                Some(line) if line.chars().count().saturating_add(word.chars().count()) < 28 => {
                    line.push(' ');
                    line.push_str(word);
                }
                _ => lines.push(word.to_owned()),
            }
        }
        lines
    }

    /// The page the text is on: its lines one under another, each spread
    /// over the same width, as justified text is, so that no line looks
    /// like the end of a paragraph.
    fn page(&self) -> Text {
        let lines = self
            .lines()
            .iter()
            .enumerate()
            .map(|(row, line)| {
                let count = line.chars().count();
                let width = 400.0 / f32::from(u16::try_from(count).unwrap());
                let top = f32::from(u16::try_from(row).unwrap()) * 14.0;
                TextLine {
                    chars: line
                        .chars()
                        .enumerate()
                        .map(|(column, character)| {
                            let left = f32::from(u16::try_from(column).unwrap()) * width;
                            TextChar {
                                character,
                                area: Rect {
                                    left,
                                    top,
                                    right: left + width,
                                    bottom: top + 12.0,
                                },
                            }
                        })
                        .collect(),
                }
            })
            .collect();
        HashMap::from([(0, lines)])
    }
}

/// Feeds keys to visual mode, as typed.
fn keys(visual: &mut Visual, text: &Text, typed: &str) {
    let keymap = Keymaps::default().visual;
    for key in typed.chars() {
        visual.feed(Key::Char(key), &keymap, text);
    }
}

/// The last character of the page.
fn end(text: &Text) -> Spot {
    let lines = &text[&0];
    let line = lines.len().saturating_sub(1);
    Spot {
        page: 0,
        line,
        index: lines[line].chars.len().saturating_sub(1),
    }
}

/// The character at a spot.
fn character(text: &Text, spot: Spot) -> char {
    text[&0][spot.line].chars[spot.index].character
}

/// Whether a character is a mark that belongs to the letter before it: an
/// accent, or a vowel sign of an abugida. The cursor stopping on one has
/// split a letter in two.
fn is_mark(character: char) -> bool {
    matches!(u32::from(character),
        0x0300..=0x036f // accents
        | 0x0591..=0x05c7 // Hebrew points
        | 0x064b..=0x065f | 0x0670 // Arabic vowel marks
        | 0x0900..=0x0903 | 0x093a..=0x094f | 0x0951..=0x0957 | 0x0962 | 0x0963 // Devanagari
        | 0x0bbe..=0x0bcd | 0x0bd7 // Tamil
        | 0x0e31 | 0x0e34..=0x0e3a | 0x0e47..=0x0e4e // Thai
    )
}

/// The checks known to fail on a page, and not for pdvim's want: each
/// the check's name and the page's.
///
/// In the Arabic PDF, Chromium gives the letter alef with the vowel mark
/// on it as one glyph, its text marked as both. Where that glyph ends a
/// line, MuPDF reads only the mark, and the alef is lost; other readers
/// keep it.
const KNOWN: &[(&str, &str)] = &[
    ("Reading a PDF", "Arabic (PDF)"),
    ("Copying", "Arabic (PDF)"),
    ("Searching", "Arabic (PDF)"),
    ("Selecting a sentence", "Arabic (PDF)"),
];

/// Fails, listing every page a check failed on and how, but those known
/// to fail; and if one known to fail no longer does, so that it is taken
/// off the list. Each failure starts with its page's name.
fn report(check: &str, failures: &[String]) {
    let known = |name: &str| KNOWN.contains(&(check, name));
    let page_of = |failure: &String| failure.split(": ").next().unwrap_or_default().to_owned();
    for (known_check, name) in KNOWN {
        assert!(
            *known_check != check || failures.iter().any(|failure| page_of(failure) == *name),
            "{check} now passes on {name}: take it off KNOWN"
        );
    }
    let failures: Vec<String> = failures
        .iter()
        .filter(|failure| !known(&page_of(failure)))
        .cloned()
        .collect();
    let mut names: Vec<String> = failures.iter().map(page_of).collect();
    names.dedup();
    assert!(
        failures.is_empty(),
        "{check} fails on {} of {} pages:\n  {}",
        names.len(),
        pages().len(),
        failures.join("\n  ")
    );
}

/// The text without its spaces, to compare where text is split apart from
/// how it is spaced.
fn unspaced(text: &str) -> String {
    text.chars().filter(|character| !character.is_whitespace()).collect()
}

#[test]
fn the_pdfs_read_back_as_written() {
    let mut failures = Vec::new();
    for page in pages().into_iter().filter(|page| page.source == Source::Pdf) {
        let read: String = page.text[&0]
            .iter()
            .flat_map(|line| line.chars.iter().map(|character| character.character))
            .collect();
        if read.contains('\u{fffd}') {
            failures.push(format!("{}: has characters MuPDF cannot name", page.name));
        }
        let written = unspaced(&page.sample.text());
        if unspaced(&read) != written {
            failures.push(format!("{}: reads as {:?}", page.name, unspaced(&read)));
        }
    }
    report("Reading a PDF", &failures);
}

#[test]
fn a_sentence_is_selected_whole() {
    let mut failures = Vec::new();
    for page in pages() {
        let text = &page.text;
        let sentences = &page.sample.sentences;
        let mut first = Visual::new(Kind::Characters, Spot::line_start(0, 0));
        keys(&mut first, text, "is");
        let got = unspaced(&first.text(text));
        if got != unspaced(&sentences[0]) {
            failures.push(format!("{}: `is` at the start selects {got:?}", page.name));
        }
        if let Some(second) = sentences.get(1) {
            let mut last = Visual::new(Kind::Characters, end(text));
            keys(&mut last, text, "is");
            let got = unspaced(&last.text(text));
            if got != unspaced(second) {
                failures.push(format!("{}: `is` at the end selects {got:?}", page.name));
            }
        }
    }
    report("Selecting a sentence", &failures);
}

#[test]
fn text_copied_is_the_text_written() {
    let mut failures = Vec::new();
    for page in pages() {
        let text = &page.text;
        let mut all = Visual::new(Kind::Characters, Spot::line_start(0, 0));
        all.move_to(end(text));
        let copied = all.text(text);
        if copied != page.sample.text() {
            failures.push(format!("{}: copied as {copied:?}", page.name));
        }
    }
    report("Copying", &failures);
}

#[test]
fn every_sentence_is_found_in_capitals_with_case_ignored() {
    let mut failures = Vec::new();
    for page in pages() {
        for sentence in &page.sample.sentences {
            let needle = Needle {
                text: sentence.to_uppercase(),
                match_case: false,
                whole_word: false,
            };
            if backend::text_hits(&page.text[&0], 0, &needle).is_empty() {
                failures.push(format!("{}: {:?} is not found", page.name, needle.text));
            }
        }
    }
    report("Searching", &failures);
}

#[test]
fn the_cursor_stops_at_every_word_and_never_inside_a_letter() {
    let mut failures = Vec::new();
    for page in pages() {
        let text = &page.text;
        let name = &page.name;
        let last = end(text);
        // Where `w` stops, from the start to the end.
        let mut cursor = Visual::cursor_at(Spot::line_start(0, 0));
        let mut stops = vec![cursor.cursor()];
        while cursor.cursor() != last && stops.len() < 500 {
            keys(&mut cursor, text, "w");
            if stops.last() == Some(&cursor.cursor()) {
                break;
            }
            stops.push(cursor.cursor());
        }
        if let Some(&inside) = stops.iter().find(|&&spot| is_mark(character(text, spot))) {
            failures.push(format!(
                "{name}: `w` stops on the mark {:?} at {inside:?}",
                character(text, inside)
            ));
        }
        if page.sample.spaced {
            // Every word after a space starts where `w` stops.
            for (line, row) in text[&0].iter().enumerate() {
                for index in 1..row.chars.len() {
                    let after_space = row.chars[index - 1].character == ' ';
                    let spot = Spot { page: 0, line, index };
                    if after_space && !stops.contains(&spot) {
                        failures.push(format!("{name}: `w` passes over the word at {spot:?}"));
                    }
                }
            }
        } else if stops.len() <= text[&0].len() {
            failures.push(format!(
                "{name}: `w` stops only {} times in {} lines",
                stops.len(),
                text[&0].len()
            ));
        }
        // `l` along the first line.
        let mut right = Visual::cursor_at(Spot::line_start(0, 0));
        for _ in 0..text[&0][0].chars.len() {
            keys(&mut right, text, "l");
            let spot = right.cursor();
            if is_mark(character(text, spot)) {
                failures.push(format!(
                    "{name}: `l` stops on the mark {:?} at {spot:?}",
                    character(text, spot)
                ));
                break;
            }
        }
    }
    report("Moving the cursor", &failures);
}
