//! A table of contents guessed for a document that has none, from its
//! text: from a printed contents page when it has one, and otherwise from
//! the headings, told from the rest of the text by their type.
//!
//! Neither way is sure. A printed contents page gives the author's own
//! titles, but its page numbers are the printed ones, which are found in
//! the document's page labels or by looking for the titles on the pages.
//! Headings are lines in larger or bold type that stand alone, ranked by
//! their size into chapters and sections.

use std::collections::HashMap;

use crate::backend::Chapter;
use crate::num;

/// A line of text on a page, as it is set.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub text: String,
    /// The size of its type, in points.
    pub size: f32,
    /// Whether most of it is bold.
    pub bold: bool,
    /// Where it starts and ends across the page, and its top and bottom,
    /// in points from the top-left corner.
    pub left: f32,
    pub top: f32,
    pub bottom: f32,
}

/// The text of a page.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PageText {
    pub lines: Vec<Line>,
    /// The height of the page, in points.
    pub height: f32,
}

/// How many pages from the start a printed contents page is looked for in.
const CONTENTS_WITHIN: usize = 40;

/// The fewest entries a printed contents page has.
const FEWEST_ENTRIES: usize = 4;

/// How much larger than the text a heading's type is, at least, unless it
/// is bold and numbered.
const HEADING_SIZE: f32 = 1.15;

/// The most levels of headings told apart.
const MOST_LEVELS: usize = 3;

/// The share of a page's height, at its top and at its bottom, where
/// running heads and page numbers are set.
const MARGIN: f32 = 0.08;

/// A table of contents for the pages, given the label printed on each page
/// if the document says. Empty when none can be told.
pub fn guess(pages: &[PageText], labels: Option<&[String]>) -> Vec<Chapter> {
    let printed = from_contents_page(pages, labels);
    if printed.len() >= FEWEST_ENTRIES.saturating_sub(1) {
        return tree(printed);
    }
    tree(from_headings(pages))
}

/// An entry before the entries are put in a tree.
#[derive(Debug, Clone, PartialEq)]
struct Entry {
    /// How deep it is: 0 for a chapter.
    level: usize,
    title: String,
    page: u32,
    top: Option<f32>,
}

/// The entries in a tree of chapters and their sections.
fn tree(entries: Vec<Entry>) -> Vec<Chapter> {
    let shallowest = entries.iter().map(|entry| entry.level).min().unwrap_or(0);
    // The chapters, and the open sections under the last of them, deepest
    // last, each with its level.
    let mut done: Vec<Chapter> = Vec::new();
    let mut open: Vec<(usize, Chapter)> = Vec::new();
    for entry in entries {
        let level = entry.level.saturating_sub(shallowest);
        while open
            .last()
            .is_some_and(|(open_level, _)| *open_level >= level)
        {
            fold(&mut open, &mut done);
        }
        open.push((
            level,
            Chapter {
                title: entry.title,
                page: Some(entry.page),
                top: entry.top,
                sections: Vec::new(),
            },
        ));
    }
    while !open.is_empty() {
        fold(&mut open, &mut done);
    }
    done
}

/// Closes the deepest open entry, putting it under the one above it, or
/// among the chapters.
fn fold(open: &mut Vec<(usize, Chapter)>, done: &mut Vec<Chapter>) {
    let Some((_, chapter)) = open.pop() else {
        return;
    };
    match open.last_mut() {
        Some((_, above)) => above.sections.push(chapter),
        None => done.push(chapter),
    }
}

/// Text made plain for comparing: lower case letters and digits only.
fn key(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The start of a title's key, by which it is looked for on a page.
fn title_key(title: &str) -> String {
    key(title).chars().take(24).collect()
}

/// Lines on the same baseline, as a contents page sets a title and its
/// page number apart, joined into rows, top to bottom.
fn rows(page: &PageText) -> Vec<Line> {
    let mut lines = page.lines.clone();
    lines.sort_by(|a, b| a.top.total_cmp(&b.top).then(a.left.total_cmp(&b.left)));
    let mut rows: Vec<Line> = Vec::new();
    for line in lines {
        match rows.last_mut() {
            Some(row) if (row.top - line.top).abs() < row.size.min(line.size) * 0.4 => {
                row.text.push(' ');
                row.text.push_str(&line.text);
                row.left = row.left.min(line.left);
                row.bottom = row.bottom.max(line.bottom);
            }
            _ => rows.push(line),
        }
    }
    rows
}

/// A row of a contents page as a title and the page number printed after
/// it, with dot leaders or space between.
fn contents_entry(row: &str) -> Option<(String, String)> {
    let row = row.trim_end();
    let number: String = {
        let digits: String = row.chars().rev().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() {
            // A Roman number, as front matter has, set apart by a space or
            // a dot.
            let roman: String = row
                .chars()
                .rev()
                .take_while(|c| "ivxlc".contains(*c))
                .collect();
            let before = row.chars().rev().nth(roman.chars().count());
            if roman.is_empty() || !matches!(before, Some(' ' | '.')) {
                return None;
            }
            roman
        } else {
            digits
        }
        .chars()
        .rev()
        .collect()
    };
    if number.len() > 5 {
        return None;
    }
    let title = row
        .strip_suffix(number.as_str())?
        .trim_end_matches(|c: char| c.is_whitespace() || ".·…_".contains(c));
    let letters = title.chars().filter(|c| c.is_alphabetic()).count();
    (letters >= 2 && title.chars().count() <= 200).then(|| (title.to_owned(), number))
}

/// The entries of a printed contents page, on the pages they lead to.
/// Empty when there is no contents page, or its page numbers cannot be
/// found in the document.
fn from_contents_page(pages: &[PageText], labels: Option<&[String]>) -> Vec<Entry> {
    // The first run of pages that are mostly entries.
    let mut found: Vec<(usize, Line, String, String)> = Vec::new();
    let mut contents_pages = Vec::new();
    for (index, page) in pages.iter().enumerate().take(CONTENTS_WITHIN) {
        let rows = rows(page);
        let entries: Vec<(Line, String, String)> = rows
            .iter()
            .filter_map(|row| {
                contents_entry(&row.text).map(|(title, number)| (row.clone(), title, number))
            })
            .collect();
        let mostly =
            entries.len() >= FEWEST_ENTRIES && entries.len().saturating_mul(2) >= rows.len();
        if mostly {
            contents_pages.push(index);
            found.extend(
                entries
                    .into_iter()
                    .map(|(row, title, number)| (index, row, title, number)),
            );
        } else if !contents_pages.is_empty() {
            break;
        }
    }
    if found.is_empty() {
        return Vec::new();
    }
    let keys: Vec<String> = pages
        .iter()
        .map(|page| page.lines.iter().map(|line| key(&line.text)).collect())
        .collect();
    let offset = offset(&found, &keys, &contents_pages);
    let by_label: HashMap<&str, usize> = labels
        .unwrap_or_default()
        .iter()
        .enumerate()
        .rev()
        .map(|(index, label)| (label.as_str(), index))
        .collect();
    let levels = indent_levels(found.iter().map(|(_, row, _, _)| row.left).collect());
    found
        .into_iter()
        .filter_map(|(_, row, title, number)| {
            let index = by_label.get(number.as_str()).copied().or_else(|| {
                let printed: usize = number.parse().ok()?;
                printed.checked_sub(1)?.checked_add_signed(offset?)
            })?;
            let page = pages.get(index)?;
            if contents_pages.contains(&index) {
                return None;
            }
            let wanted = title_key(&title);
            let top = page
                .lines
                .iter()
                .find(|line| !wanted.is_empty() && key(&line.text).starts_with(&wanted))
                .map(|line| line.top);
            Some(Entry {
                level: levels(row.left),
                title,
                page: num::len(index),
                top,
            })
        })
        .collect()
}

/// How many pages on from its printed number each page is, found by
/// looking for the titles of the entries near the pages they give: the
/// difference most of those found agree on. None when too few are found.
fn offset(
    found: &[(usize, Line, String, String)],
    keys: &[String],
    contents_pages: &[usize],
) -> Option<isize> {
    let mut votes: HashMap<isize, usize> = HashMap::new();
    let mut looked = 0_usize;
    for (_, _, title, number) in found.iter().take(40) {
        let Ok(printed) = number.parse::<usize>() else {
            continue;
        };
        let wanted = title_key(title);
        if wanted.chars().count() < 4 {
            continue;
        }
        looked = looked.saturating_add(1);
        let index = printed.saturating_sub(1);
        let near = index.saturating_sub(20)..index.saturating_add(80).min(keys.len());
        for page in near {
            if contents_pages.contains(&page) {
                continue;
            }
            if keys.get(page).is_some_and(|text| text.contains(&wanted)) {
                let (Ok(page), Ok(index)) = (isize::try_from(page), isize::try_from(index)) else {
                    continue;
                };
                let vote = votes.entry(page.saturating_sub(index)).or_default();
                *vote = vote.saturating_add(1);
            }
        }
    }
    let (best, count) = votes
        .into_iter()
        .max_by_key(|(offset, count)| (*count, std::cmp::Reverse(offset.unsigned_abs())))?;
    (count >= 2 && count.saturating_mul(5) >= looked).then_some(best)
}

/// The level of each entry from how far it is indented: the least
/// indented are chapters.
fn indent_levels(mut lefts: Vec<f32>) -> impl Fn(f32) -> usize {
    // Indents closer than this are one.
    let step = 6.0_f32;
    let mut indents: Vec<f32> = Vec::new();
    lefts.sort_by(f32::total_cmp);
    for left in lefts {
        if indents.last().is_none_or(|last| left - last >= step) {
            indents.push(left);
        }
    }
    move |left| {
        indents
            .iter()
            .take_while(|indent| left - **indent >= -step / 2.0)
            .count()
            .saturating_sub(1)
            .min(MOST_LEVELS.saturating_sub(1))
    }
}

/// The type of a line, to tell headings apart by: its size, to half a
/// point, and whether it is bold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Style {
    half_points: u32,
    bold: bool,
}

impl Style {
    fn of(line: &Line) -> Self {
        Self {
            half_points: num::round(line.size * 2.0),
            bold: line.bold,
        }
    }
}

/// Whether a line starts with a number such as `3`, `2.1` or `IV`, or a
/// word such as `Chapter`, as headings do; and how many parts its number
/// has.
fn numbered(text: &str) -> Option<usize> {
    let first = text.split_whitespace().next()?;
    let lower = first.to_lowercase();
    if ["chapter", "part", "appendix", "section", "book", "lecture"].contains(&lower.as_str()) {
        return Some(1);
    }
    let number = first.trim_end_matches(['.', ':', ')']);
    let parts: Vec<&str> = number.split('.').collect();
    let all_numbers = !number.is_empty()
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()));
    let roman =
        number.len() <= 6 && !number.is_empty() && number.chars().all(|c| "IVXL".contains(c));
    (all_numbers || roman).then_some(parts.len())
}

/// Whether a line is a page number or the like, with no words.
fn wordless(text: &str) -> bool {
    text.chars().filter(|c| c.is_alphabetic()).count() < 2
        || numbered(text).is_some() && text.split_whitespace().count() == 1
}

/// The headings of the pages, each with its level.
fn from_headings(pages: &[PageText]) -> Vec<Entry> {
    let body = body_size(pages);
    if body <= 0.0 {
        return Vec::new();
    }
    let running = running_heads(pages);
    // Every heading, with its style.
    let mut headings: Vec<(Style, Entry)> = Vec::new();
    for (index, page) in pages.iter().enumerate() {
        let mut lines: Vec<&Line> = page.lines.iter().collect();
        lines.sort_by(|a, b| a.top.total_cmp(&b.top));
        let mut previous: Option<&Line> = None;
        for line in lines {
            let text = line.text.trim();
            let in_margin =
                line.top < page.height * MARGIN || line.bottom > page.height * (1.0 - MARGIN);
            let larger = line.size >= body * HEADING_SIZE;
            let bold_numbered = line.bold
                && line.size >= body * 0.95
                && numbered(text).is_some_and(|_| !wordless(text));
            let heading = (larger || bold_numbered)
                && text.chars().count() <= 120
                && !(in_margin && running.contains(&key_without_digits(text)))
                && !text.ends_with([',', ';']);
            if !heading {
                previous = None;
                continue;
            }
            // A heading set on two lines, or a number set above its title,
            // is one heading.
            if let (Some(last), Some((style, entry))) = (previous, headings.last_mut())
                && entry.page == num::len(index)
                && line.top - last.bottom < line.size.max(last.size) * 1.2
                && (Style::of(line) == *style || wordless(&entry.title))
            {
                if wordless(&entry.title) {
                    *style = Style::of(line);
                }
                entry.title.push(' ');
                entry.title.push_str(text);
                previous = Some(line);
                continue;
            }
            previous = Some(line);
            headings.push((
                Style::of(line),
                Entry {
                    level: 0,
                    title: text.to_owned(),
                    page: num::len(index),
                    top: Some(line.top),
                },
            ));
        }
    }
    headings.retain(|(_, entry)| !wordless(&entry.title));
    rank(headings, pages.len())
}

/// The headings in the styles that look like those of headings, each
/// ranked by its style: the largest is a chapter.
fn rank(mut headings: Vec<(Style, Entry)>, pages: usize) -> Vec<Entry> {
    let mut counts: HashMap<Style, usize> = HashMap::new();
    for (style, _) in &headings {
        let count = counts.entry(*style).or_default();
        *count = count.saturating_add(1);
    }
    // A style on most lines of most pages is the text of a page set large,
    // not a heading.
    let most = pages.saturating_mul(3).max(20);
    let mut styles: Vec<Style> = counts
        .iter()
        .filter(|(_, count)| **count <= most)
        .map(|(style, _)| *style)
        .collect();
    styles.sort_by(|a, b| b.half_points.cmp(&a.half_points).then(b.bold.cmp(&a.bold)));
    // A style used once, as for the title of the document, would hold
    // every other heading under it.
    while styles.len() > 1
        && styles
            .first()
            .is_some_and(|style| counts.get(style).copied().unwrap_or(0) == 1)
    {
        styles.remove(0);
    }
    styles.truncate(MOST_LEVELS);
    headings.retain(|(style, _)| styles.contains(style));
    if headings.len() < 2 {
        return Vec::new();
    }
    headings
        .into_iter()
        .map(|(style, mut entry)| {
            entry.level = styles.iter().position(|kept| *kept == style).unwrap_or(0);
            entry
        })
        .collect()
}

/// The size of the type most of the text is set in.
fn body_size(pages: &[PageText]) -> f32 {
    let mut counts: HashMap<u32, usize> = HashMap::new();
    for line in pages.iter().flat_map(|page| &page.lines) {
        let count = counts.entry(num::round(line.size * 2.0)).or_default();
        *count = count.saturating_add(line.text.chars().count());
    }
    counts
        .into_iter()
        .max_by_key(|(size, count)| (*count, *size))
        .map_or(0.0, |(half_points, _)| num::float(half_points) / 2.0)
}

/// A line's text without its numbers, so that a running head is the same
/// on every page whatever page number it carries.
fn key_without_digits(text: &str) -> String {
    key(text).chars().filter(|c| !c.is_ascii_digit()).collect()
}

/// The lines set in the top or bottom margin of three or more pages: the
/// running heads and feet.
fn running_heads(pages: &[PageText]) -> std::collections::HashSet<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for page in pages {
        let mut seen = std::collections::HashSet::new();
        for line in &page.lines {
            let in_margin =
                line.top < page.height * MARGIN || line.bottom > page.height * (1.0 - MARGIN);
            if in_margin && seen.insert(key_without_digits(&line.text)) {
                let count = counts.entry(key_without_digits(&line.text)).or_default();
                *count = count.saturating_add(1);
            }
        }
    }
    counts
        .into_iter()
        .filter(|(_, count)| *count >= 3)
        .map(|(text, _)| text)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, size: f32, bold: bool, left: f32, top: f32) -> Line {
        Line {
            text: text.to_owned(),
            size,
            bold,
            left,
            top,
            bottom: top + size,
        }
    }

    /// A page of body text with these lines set above it.
    fn page(above: Vec<Line>) -> PageText {
        let mut lines = above;
        for row in 0..30 {
            let top = num::float(row).mul_add(14.0, 300.0);
            lines.push(line(
                "The quick brown fox jumps over the lazy dog, again and again",
                11.0,
                false,
                72.0,
                top,
            ));
        }
        lines.push(line("Running Head of the Book", 9.0, false, 72.0, 20.0));
        PageText {
            lines,
            height: 792.0,
        }
    }

    fn titles(chapters: &[Chapter], depth: usize, out: &mut Vec<String>) {
        for chapter in chapters {
            out.push(format!(
                "{}{} @{}",
                "  ".repeat(depth),
                chapter.title,
                chapter.page.unwrap_or(u32::MAX)
            ));
            titles(&chapter.sections, depth.saturating_add(1), out);
        }
    }

    fn outline(chapters: &[Chapter]) -> Vec<String> {
        let mut out = Vec::new();
        titles(chapters, 0, &mut out);
        out
    }

    #[test]
    fn headings_are_ranked_by_their_type() {
        let pages = vec![
            page(vec![line("A Book About Foxes", 30.0, true, 100.0, 100.0)]),
            page(vec![line("1 Beginnings", 20.0, true, 72.0, 100.0)]),
            page(vec![line("1.1 The first fox", 14.0, true, 72.0, 100.0)]),
            page(vec![line("1.2 The second fox", 14.0, true, 72.0, 100.0)]),
            page(vec![
                line("2 Endings", 20.0, true, 72.0, 100.0),
                line("2.1 The last fox", 14.0, true, 72.0, 200.0),
            ]),
        ];
        assert_eq!(
            outline(&guess(&pages, None)),
            [
                "1 Beginnings @1",
                "  1.1 The first fox @2",
                "  1.2 The second fox @3",
                "2 Endings @4",
                "  2.1 The last fox @4",
            ]
        );
    }

    #[test]
    fn a_number_set_above_its_title_and_a_title_on_two_lines_are_one_heading() {
        let pages = vec![
            page(vec![
                line("1", 24.0, true, 72.0, 100.0),
                line("Where It All", 20.0, true, 72.0, 130.0),
                line("Began", 20.0, true, 72.0, 154.0),
            ]),
            page(vec![
                line("2", 24.0, true, 72.0, 100.0),
                line("After", 20.0, true, 72.0, 130.0),
            ]),
        ];
        assert_eq!(
            outline(&guess(&pages, None)),
            ["1 Where It All Began @0", "2 After @1"]
        );
    }

    #[test]
    fn text_without_headings_has_no_contents() {
        let pages = vec![page(Vec::new()), page(Vec::new()), page(Vec::new())];
        assert!(guess(&pages, None).is_empty());
    }

    /// A contents page, and the pages it leads to, numbered from 1 on the
    /// third page.
    fn book_with_contents() -> Vec<PageText> {
        let entries = [
            ("Introduction", 72.0, "1"),
            ("Foxes", 72.0, "2"),
            ("Red foxes", 90.0, "2"),
            ("Arctic foxes . . . . . . . .", 90.0, "3"),
            ("Dogs", 72.0, "4"),
        ];
        let mut contents = PageText {
            lines: vec![line("Contents", 20.0, true, 72.0, 80.0)],
            height: 792.0,
        };
        for (row, (title, left, number)) in entries.into_iter().enumerate() {
            let top = num::float(num::len(row)).mul_add(20.0, 150.0);
            contents.lines.push(line(title, 11.0, false, left, top));
            contents.lines.push(line(number, 11.0, false, 500.0, top));
        }
        let titled = |title: &str| page(vec![line(title, 11.0, false, 72.0, 120.0)]);
        vec![
            page(Vec::new()),
            contents,
            titled("Introduction"),
            titled("Foxes"),
            titled("Arctic foxes"),
            titled("Dogs"),
        ]
    }

    #[test]
    fn a_printed_contents_page_is_read_and_its_numbers_found() {
        assert_eq!(
            outline(&guess(&book_with_contents(), None)),
            [
                "Introduction @2",
                "Foxes @3",
                "  Red foxes @3",
                "  Arctic foxes @4",
                "Dogs @5",
            ]
        );
    }

    #[test]
    fn page_labels_say_where_printed_numbers_are() {
        let labels: Vec<String> = ["i", "ii", "1", "2", "3", "4"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        let chapters = guess(&book_with_contents(), Some(&labels));
        assert_eq!(chapters.first().and_then(|chapter| chapter.page), Some(2));
        assert_eq!(chapters.last().and_then(|chapter| chapter.page), Some(5));
    }

    #[test]
    fn contents_entries_are_told_from_text() {
        assert_eq!(
            contents_entry("Arctic foxes . . . . 12"),
            Some(("Arctic foxes".to_owned(), "12".to_owned()))
        );
        assert_eq!(
            contents_entry("Preface.......xiv"),
            Some(("Preface".to_owned(), "xiv".to_owned()))
        );
        assert_eq!(
            contents_entry("in 1999"),
            Some(("in".to_owned(), "1999".to_owned()))
        );
        assert_eq!(contents_entry("Just some words"), None);
        assert_eq!(contents_entry("42"), None);
        assert_eq!(contents_entry("a lexicon"), None);
    }

    #[test]
    fn headings_are_known_by_their_numbers() {
        assert_eq!(numbered("2.3.1 Results"), Some(3));
        assert_eq!(numbered("Chapter Seven"), Some(1));
        assert_eq!(numbered("IV. Endings"), Some(1));
        assert_eq!(numbered("Results"), None);
    }
}
