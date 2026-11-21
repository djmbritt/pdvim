//! Visual mode: a cursor over the text of the pages, and the text selected
//! between where it started and where it is, to copy.
//!
//! The text is read a page at a time. Motions run over the pages whose text
//! is known and stop where it runs out; the app reads the pages around the
//! cursor as it moves, so the next press goes on.

use std::collections::HashMap;

use crate::backend::{MarkupKind, TextLine};
use crate::input::Key;
use crate::keymap::{Counts, Fed, Keymap, Pending, bindable};
use crate::layout::Rect;

bindable! {
    /// What keys can be bound to in visual mode.
    pub enum Binding {
        Yank = "yank", "Copy the selection, and leave";
        Highlight = "highlight", "Highlight the selection in the PDF, and leave";
        Underline = "underline", "Underline the selection in the PDF, and leave";
        StrikeOut = "strike-out", "Strike out the selection in the PDF, and leave";
        Squiggly = "squiggly", "Underline the selection with a squiggle, and leave";
        Unmark = "unmark", "Take away the markings under the selection, and leave";
        /// The text is typed at the prompt that opens.
        Note = "note", "Put a note at the cursor, and leave";
        /// The marking under the cursor, or the note beside its line.
        ChangeNote = "change-note", "Change the note on the marking at the cursor, and leave";
        Leave = "leave", "Leave without copying";
        OtherEnd = "other-end", "Go to the other end of the selection";
        /// The kind in force leaves; the other kind switches to it.
        Characters = "characters", "Select characters; leave if already";
        Lines = "lines", "Select whole lines; leave if already";
        Down = "down", "Go down a line";
        Up = "up", "Go up a line";
        Left = "left", "Go left a character";
        Right = "right", "Go right a character";
        LineStart = "line-start", "Go to the start of the line";
        LineText = "line-text", "Go to the first character that is not a space";
        LineEnd = "line-end", "Go to the end of the line";
        NextWord = "next-word", "Go to the start of the next word";
        PreviousWord = "previous-word", "Go to the start of the word, or the one before";
        WordEnd = "word-end", "Go to the end of the word, or the next one";
        /// The character searched for is the next key typed.
        FindForward = "find-forward", "Go to the next x on the line, typed next";
        FindBackward = "find-backward", "Go to the previous x on the line";
        TillForward = "till-forward", "Go to just before the next x on the line";
        TillBackward = "till-backward", "Go to just after the previous x on the line";
        RepeatFind = "repeat-find", "Repeat the last find";
        RepeatFindReversed = "repeat-find-reversed", "Repeat the last find the other way";
    }
}

/// The lines of text of each page read so far.
pub type Text = HashMap<u32, Vec<TextLine>>;

/// A character in the text: its page, its line on that page, and its place
/// in that line. Spots sort in reading order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Spot {
    pub page: u32,
    pub line: usize,
    pub index: usize,
}

impl Spot {
    /// The first character of a line.
    pub const fn line_start(page: u32, line: usize) -> Self {
        Self {
            page,
            line,
            index: 0,
        }
    }
}

/// Whether characters or whole lines are selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Characters,
    Lines,
}

/// A search along the cursor's line for a character, as with `f`, `F`,
/// `t` and `T`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Find {
    /// Towards the end of the line, or the start.
    forward: bool,
    /// Whether the cursor stops just short of the character, as with `t`.
    till: bool,
    target: char,
}

/// What a key typed in visual mode comes to.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Stay in visual mode.
    Stay,
    /// Copy this text, and leave.
    Copy(String),
    /// Mark these areas this way, each a line's on a page, and leave.
    Mark(MarkupKind, Vec<(u32, Rect)>),
    /// Take away the markings over these areas, each a line's on a page,
    /// and leave.
    Unmark(Vec<(u32, Rect)>),
    /// Put a note at the cursor, which is this area of this page, and
    /// leave.
    Note(u32, Rect),
    /// Change the note on the marking at the cursor, which is this area
    /// of this page, and leave.
    ChangeNote(u32, Rect),
    /// Leave without copying.
    Leave,
}

/// The selection: where it started, where the cursor is, and how.
#[derive(Debug, Clone, PartialEq)]
pub struct Visual {
    pub kind: Kind,
    anchor: Spot,
    cursor: Spot,
    /// How far across the page `j` and `k` aim for, kept from one to the
    /// next so that short lines do not pull the cursor left for good.
    goal: Option<f32>,
    /// The count and keys typed so far.
    pending: Pending<Binding>,
    /// A search whose character is still to be typed, and its count.
    finding: Option<(bool, bool, u32)>,
    /// The last search for a character, for `;` and `,` to repeat.
    last_find: Option<Find>,
}

impl Visual {
    /// A selection of one character, or one line, at a spot.
    pub const fn new(kind: Kind, at: Spot) -> Self {
        Self {
            kind,
            anchor: at,
            cursor: at,
            goal: None,
            pending: Pending::new(),
            finding: None,
            last_find: None,
        }
    }

    pub const fn cursor(&self) -> Spot {
        self.cursor
    }

    /// What has been typed so far, for the status bar.
    pub fn typed(&self) -> String {
        self.pending.display()
    }

    /// The first and last characters selected, in reading order.
    fn range(&self, text: &Text) -> (Spot, Spot) {
        let (first, last) = if self.anchor <= self.cursor {
            (self.anchor, self.cursor)
        } else {
            (self.cursor, self.anchor)
        };
        match self.kind {
            Kind::Characters => (first, last),
            Kind::Lines => (
                Spot::line_start(first.page, first.line),
                Spot {
                    index: line(text, last.page, last.line)
                        .map_or(0, |line| line.chars.len().saturating_sub(1)),
                    ..last
                },
            ),
        }
    }

    /// Takes a key, bound as in the keymap.
    pub fn feed(&mut self, key: Key, keymap: &Keymap<Binding>, text: &Text) -> Outcome {
        // The character after `f`, `F`, `t` or `T`; anything else, such as
        // escape, gives up the search.
        if let Some((forward, till, times)) = self.finding.take() {
            if let Key::Char(target) = key {
                let find = Find {
                    forward,
                    till,
                    target,
                };
                self.last_find = Some(find);
                self.find(text, find, times, false);
            }
            return Outcome::Stay;
        }
        match self.pending.feed(key, keymap, Counts::Taken) {
            Fed::Bound(binding, count) => self.act(binding, count.unwrap_or(1), text),
            Fed::Waiting | Fed::Unbound(_) => Outcome::Stay,
        }
    }

    /// Does what a binding says, a number of times where that means
    /// anything.
    fn act(&mut self, binding: Binding, times: u32, text: &Text) -> Outcome {
        match binding {
            Binding::Leave => return Outcome::Leave,
            Binding::Yank => return Outcome::Copy(self.text(text)),
            Binding::Highlight => return Outcome::Mark(MarkupKind::Highlight, self.areas(text)),
            Binding::Underline => return Outcome::Mark(MarkupKind::Underline, self.areas(text)),
            Binding::StrikeOut => return Outcome::Mark(MarkupKind::StrikeOut, self.areas(text)),
            Binding::Squiggly => return Outcome::Mark(MarkupKind::Squiggly, self.areas(text)),
            Binding::Unmark => return Outcome::Unmark(self.areas(text)),
            Binding::Note => {
                return character(text, self.cursor)
                    .map_or(Outcome::Leave, |area| Outcome::Note(self.cursor.page, area));
            }
            Binding::ChangeNote => {
                return character(text, self.cursor).map_or(Outcome::Leave, |area| {
                    Outcome::ChangeNote(self.cursor.page, area)
                });
            }
            Binding::OtherEnd => std::mem::swap(&mut self.anchor, &mut self.cursor),
            Binding::Characters | Binding::Lines => {
                let kind = if binding == Binding::Characters {
                    Kind::Characters
                } else {
                    Kind::Lines
                };
                if kind == self.kind {
                    return Outcome::Leave;
                }
                self.kind = kind;
            }
            Binding::FindForward => self.finding = Some((true, false, times)),
            Binding::FindBackward => self.finding = Some((false, false, times)),
            Binding::TillForward => self.finding = Some((true, true, times)),
            Binding::TillBackward => self.finding = Some((false, true, times)),
            Binding::RepeatFind => {
                if let Some(find) = self.last_find {
                    self.find(text, find, times, true);
                }
            }
            Binding::RepeatFindReversed => {
                if let Some(find) = self.last_find {
                    let reversed = Find {
                        forward: !find.forward,
                        ..find
                    };
                    self.find(text, reversed, times, true);
                }
            }
            Binding::Down => self.vertical(text, times, true),
            Binding::Up => self.vertical(text, times, false),
            Binding::Left
            | Binding::Right
            | Binding::LineStart
            | Binding::LineText
            | Binding::LineEnd
            | Binding::NextWord
            | Binding::PreviousWord
            | Binding::WordEnd => {
                // A motion that goes nowhere more, such as `$` once at the
                // end, stops the count, however large.
                for _ in 0..times {
                    let Some(moved) = self.horizontal(binding, text) else {
                        break;
                    };
                    if moved == self.cursor {
                        break;
                    }
                    self.cursor = moved;
                }
                self.goal = None;
            }
        }
        Outcome::Stay
    }

    /// Where a motion along the text takes the cursor, if anywhere.
    fn horizontal(&self, motion: Binding, text: &Text) -> Option<Spot> {
        let cursor = self.cursor;
        let length = line(text, cursor.page, cursor.line)?.chars.len();
        match motion {
            Binding::Right => (cursor.index.saturating_add(1) < length).then_some(Spot {
                index: cursor.index.saturating_add(1),
                ..cursor
            }),
            Binding::Left => cursor
                .index
                .checked_sub(1)
                .map(|index| Spot { index, ..cursor }),
            Binding::LineStart => Some(Spot { index: 0, ..cursor }),
            Binding::LineText => line(text, cursor.page, cursor.line)?
                .chars
                .iter()
                .position(|character| !character.character.is_whitespace())
                .map(|index| Spot { index, ..cursor }),
            Binding::LineEnd => Some(Spot {
                index: length.saturating_sub(1),
                ..cursor
            }),
            Binding::NextWord => find(text, cursor, next, |spot| starts_word(text, spot)),
            Binding::PreviousWord => find(text, cursor, previous, |spot| starts_word(text, spot)),
            Binding::WordEnd => find(text, cursor, next, |spot| ends_word(text, spot)),
            _ => None,
        }
    }

    /// Moves the cursor to the character searched for, the given number of
    /// times along its line, or leaves it if there are not that many. A
    /// repeated `t` or `T`, and each `t` or `T` after the first of a count,
    /// looks past the character right next to the cursor, so that it moves
    /// on rather than stay put.
    fn find(&mut self, text: &Text, find: Find, times: u32, repeated: bool) {
        let Some(current) = line(text, self.cursor.page, self.cursor.line) else {
            return;
        };
        let matches = |index: &usize| {
            current
                .chars
                .get(*index)
                .is_some_and(|character| character.character == find.target)
        };
        let mut at = self.cursor.index;
        for time in 0..times {
            let skip = usize::from(find.till && (repeated || time > 0));
            let found = if find.forward {
                (at.saturating_add(1).saturating_add(skip)..current.chars.len()).find(matches)
            } else {
                at.checked_sub(skip)
                    .and_then(|end| (0..end).rev().find(matches))
            };
            let Some(found) = found else {
                return;
            };
            at = match (find.till, find.forward) {
                (false, _) => found,
                (true, true) => found.saturating_sub(1),
                (true, false) => found.saturating_add(1),
            };
        }
        self.cursor.index = at;
        self.goal = None;
    }

    /// Moves the cursor a number of lines down, or up, to the character
    /// nearest the goal across the page.
    fn vertical(&mut self, text: &Text, times: u32, down: bool) {
        let goal = *self.goal.get_or_insert_with(|| {
            character(text, self.cursor).map_or(0.0, |area| f32::midpoint(area.left, area.right))
        });
        let mut at = (self.cursor.page, self.cursor.line);
        for _ in 0..times {
            let step = if down {
                next_line(text, at)
            } else {
                previous_line(text, at)
            };
            let Some(moved) = step else {
                break;
            };
            at = moved;
        }
        let (page, line_index) = at;
        let Some(target) = line(text, page, line_index) else {
            return;
        };
        let distance = |area: &Rect| (f32::midpoint(area.left, area.right) - goal).abs();
        let index = target
            .chars
            .iter()
            .enumerate()
            .min_by(|(_, one), (_, other)| distance(&one.area).total_cmp(&distance(&other.area)))
            .map_or(0, |(index, _)| index);
        self.cursor = Spot {
            page,
            line: line_index,
            index,
        };
    }

    /// The text selected, a line of text to a line, without trailing
    /// spaces.
    pub fn text(&self, text: &Text) -> String {
        let (first, last) = self.range(text);
        let mut lines = Vec::new();
        let mut at = Some((first.page, first.line));
        while let Some((page, line_index)) = at {
            if (page, line_index) > (last.page, last.line) {
                break;
            }
            if let Some(current) = line(text, page, line_index) {
                let from = if (page, line_index) == (first.page, first.line) {
                    first.index
                } else {
                    0
                };
                let to = if (page, line_index) == (last.page, last.line) {
                    last.index.saturating_add(1)
                } else {
                    current.chars.len()
                };
                let content: String = current
                    .chars
                    .iter()
                    .take(to)
                    .skip(from)
                    .map(|character| character.character)
                    .collect();
                lines.push(content.trim_end().to_owned());
            }
            at = next_line(text, (page, line_index));
        }
        lines.join("\n")
    }

    /// The area selected on each line, as a page and an area on it.
    pub fn areas(&self, text: &Text) -> Vec<(u32, Rect)> {
        let (first, last) = self.range(text);
        let mut areas = Vec::new();
        let mut at = Some((first.page, first.line));
        while let Some((page, line_index)) = at {
            if (page, line_index) > (last.page, last.line) {
                break;
            }
            if let Some(current) = line(text, page, line_index) {
                let from = if (page, line_index) == (first.page, first.line) {
                    first.index
                } else {
                    0
                };
                let to = if (page, line_index) == (last.page, last.line) {
                    last.index.saturating_add(1)
                } else {
                    current.chars.len()
                };
                let mut chars = current.chars.iter().take(to).skip(from);
                if let Some(first_char) = chars.next() {
                    let area = chars.fold(first_char.area, |area, character| Rect {
                        left: area.left.min(character.area.left),
                        top: area.top.min(character.area.top),
                        right: area.right.max(character.area.right),
                        bottom: area.bottom.max(character.area.bottom),
                    });
                    areas.push((page, area));
                }
            }
            at = next_line(text, (page, line_index));
        }
        areas
    }

    /// The pages whose text the cursor may move into next: the ones before
    /// and after its own.
    pub fn neighbours(&self, page_count: u32) -> Vec<u32> {
        let page = self.cursor.page;
        [page.checked_sub(1), page.checked_add(1)]
            .into_iter()
            .flatten()
            .filter(|page| *page < page_count)
            .collect()
    }
}

/// A line of a page.
fn line(text: &Text, page: u32, line: usize) -> Option<&TextLine> {
    text.get(&page)?.get(line)
}

/// The area of the character at a spot.
pub fn character(text: &Text, spot: Spot) -> Option<Rect> {
    line(text, spot.page, spot.line)?
        .chars
        .get(spot.index)
        .map(|character| character.area)
}

/// Whether the character at a spot is a space, or there is none.
fn is_space(text: &Text, spot: Spot) -> bool {
    line(text, spot.page, spot.line)
        .and_then(|line| line.chars.get(spot.index))
        .is_none_or(|character| character.character.is_whitespace())
}

/// Whether a word starts at a spot: a character that is not a space, first
/// in its line or after a space.
fn starts_word(text: &Text, spot: Spot) -> bool {
    !is_space(text, spot)
        && spot
            .index
            .checked_sub(1)
            .is_none_or(|index| is_space(text, Spot { index, ..spot }))
}

/// Whether a word ends at a spot: a character that is not a space, last in
/// its line or before a space.
fn ends_word(text: &Text, spot: Spot) -> bool {
    !is_space(text, spot)
        && is_space(
            text,
            Spot {
                index: spot.index.saturating_add(1),
                ..spot
            },
        )
}

/// The first spot past `from`, stepping with `step`, that `wanted` accepts.
fn find(
    text: &Text,
    from: Spot,
    step: fn(&Text, Spot) -> Option<Spot>,
    wanted: impl Fn(Spot) -> bool,
) -> Option<Spot> {
    let mut at = step(text, from)?;
    loop {
        if wanted(at) {
            return Some(at);
        }
        at = step(text, at)?;
    }
}

/// The character after a spot, on the next line or page if need be.
fn next(text: &Text, spot: Spot) -> Option<Spot> {
    let length = line(text, spot.page, spot.line)?.chars.len();
    if spot.index.saturating_add(1) < length {
        return Some(Spot {
            index: spot.index.saturating_add(1),
            ..spot
        });
    }
    let (page, line) = next_line(text, (spot.page, spot.line))?;
    Some(Spot::line_start(page, line))
}

/// The character before a spot, on the line or page before if need be.
fn previous(text: &Text, spot: Spot) -> Option<Spot> {
    if let Some(index) = spot.index.checked_sub(1) {
        return Some(Spot { index, ..spot });
    }
    let (page, line_index) = previous_line(text, (spot.page, spot.line))?;
    let length = line(text, page, line_index)?.chars.len();
    Some(Spot {
        page,
        line: line_index,
        index: length.saturating_sub(1),
    })
}

/// The line after a line, on a later page if need be, skipping pages with
/// no text. None past the last page whose text is known.
fn next_line(text: &Text, (page, line): (u32, usize)) -> Option<(u32, usize)> {
    if line.saturating_add(1) < text.get(&page)?.len() {
        return Some((page, line.saturating_add(1)));
    }
    let mut page = page.checked_add(1)?;
    loop {
        if !text.get(&page)?.is_empty() {
            return Some((page, 0));
        }
        page = page.checked_add(1)?;
    }
}

/// The line before a line, on an earlier page if need be, skipping pages
/// with no text. None before the first page whose text is known.
fn previous_line(text: &Text, (page, line): (u32, usize)) -> Option<(u32, usize)> {
    if let Some(line) = line.checked_sub(1) {
        return Some((page, line));
    }
    let mut page = page.checked_sub(1)?;
    loop {
        let count = text.get(&page)?.len();
        if let Some(last) = count.checked_sub(1) {
            return Some((page, last));
        }
        page = page.checked_sub(1)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::TextChar;

    /// Lines of text, each character ten points wide and each line twenty
    /// points below the last.
    fn page(lines: &[&str]) -> Vec<TextLine> {
        lines
            .iter()
            .enumerate()
            .map(|(row, content)| TextLine {
                chars: content
                    .chars()
                    .enumerate()
                    .map(|(column, character)| {
                        let left = f32::from(u16::try_from(column).unwrap()) * 10.0;
                        let top = f32::from(u16::try_from(row).unwrap()) * 20.0;
                        TextChar {
                            character,
                            area: Rect {
                                left,
                                top,
                                right: left + 10.0,
                                bottom: top + 12.0,
                            },
                        }
                    })
                    .collect(),
            })
            .collect()
    }

    fn text() -> Text {
        HashMap::from([
            (0, page(&["one two three", "four five"])),
            (1, Vec::new()),
            (2, page(&["six", "seven eight nine"])),
        ])
    }

    fn spot(page: u32, line: usize, index: usize) -> Spot {
        Spot { page, line, index }
    }

    fn keymap() -> Keymap<Binding> {
        crate::bindings::Keymaps::default().visual
    }

    fn keys(visual: &mut Visual, text: &Text, keys: &str) -> Outcome {
        let keymap = keymap();
        let mut outcome = Outcome::Stay;
        for key in keys.chars() {
            outcome = visual.feed(Key::Char(key), &keymap, text);
        }
        outcome
    }

    #[test]
    fn characters_and_line_ends() {
        let text = text();
        let mut visual = Visual::new(Kind::Characters, spot(0, 0, 0));
        keys(&mut visual, &text, "lll");
        assert_eq!(visual.cursor(), spot(0, 0, 3));
        keys(&mut visual, &text, "$");
        assert_eq!(visual.cursor(), spot(0, 0, 12));
        keys(&mut visual, &text, "l");
        assert_eq!(visual.cursor(), spot(0, 0, 12));
        keys(&mut visual, &text, "0h");
        assert_eq!(visual.cursor(), spot(0, 0, 0));
        keys(&mut visual, &text, "5l");
        assert_eq!(visual.cursor(), spot(0, 0, 5));
    }

    #[test]
    fn words_run_across_lines_and_pages() {
        let text = text();
        let mut visual = Visual::new(Kind::Characters, spot(0, 0, 0));
        keys(&mut visual, &text, "w");
        assert_eq!(visual.cursor(), spot(0, 0, 4));
        keys(&mut visual, &text, "ww");
        assert_eq!(visual.cursor(), spot(0, 1, 0));
        keys(&mut visual, &text, "e");
        assert_eq!(visual.cursor(), spot(0, 1, 3));
        // The empty page is skipped.
        keys(&mut visual, &text, "ww");
        assert_eq!(visual.cursor(), spot(2, 0, 0));
        keys(&mut visual, &text, "b");
        assert_eq!(visual.cursor(), spot(0, 1, 5));
        // Nothing past the last word: the cursor stays.
        let mut visual = Visual::new(Kind::Characters, spot(2, 1, 12));
        keys(&mut visual, &text, "w");
        assert_eq!(visual.cursor(), spot(2, 1, 12));
    }

    #[test]
    fn lines_keep_their_aim_across_short_ones() {
        let text = text();
        let mut visual = Visual::new(Kind::Characters, spot(0, 0, 8));
        keys(&mut visual, &text, "j");
        assert_eq!(visual.cursor(), spot(0, 1, 8));
        // "six" is short: the cursor goes to its end...
        keys(&mut visual, &text, "j");
        assert_eq!(visual.cursor(), spot(2, 0, 2));
        // ...and back out to where it aimed on the next line.
        keys(&mut visual, &text, "j");
        assert_eq!(visual.cursor(), spot(2, 1, 8));
        keys(&mut visual, &text, "3k");
        assert_eq!(visual.cursor(), spot(0, 0, 8));
    }

    #[test]
    fn the_selection_is_copied_line_by_line() {
        let text = text();
        let mut visual = Visual::new(Kind::Characters, spot(0, 0, 4));
        assert_eq!(
            keys(&mut visual, &text, "j$y"),
            Outcome::Copy("two three\nfour five".to_owned())
        );
        // Backwards selections read forwards.
        let mut visual = Visual::new(Kind::Characters, spot(2, 0, 2));
        assert_eq!(
            keys(&mut visual, &text, "kby"),
            Outcome::Copy("four five\nsix".to_owned())
        );
        // Whole lines.
        let mut visual = Visual::new(Kind::Lines, spot(0, 1, 6));
        assert_eq!(
            keys(&mut visual, &text, "jy"),
            Outcome::Copy("four five\nsix".to_owned())
        );
    }

    #[test]
    fn f_and_t_find_a_character_on_the_line() {
        let text = text();
        // "seven eight nine"
        let mut visual = Visual::new(Kind::Characters, spot(2, 1, 0));
        keys(&mut visual, &text, "fe");
        assert_eq!(visual.cursor(), spot(2, 1, 1));
        keys(&mut visual, &text, ";");
        assert_eq!(visual.cursor(), spot(2, 1, 3));
        keys(&mut visual, &text, "2;");
        assert_eq!(visual.cursor(), spot(2, 1, 15));
        keys(&mut visual, &text, ",");
        assert_eq!(visual.cursor(), spot(2, 1, 6));
        keys(&mut visual, &text, "Fs");
        assert_eq!(visual.cursor(), spot(2, 1, 0));
        // `t` stops short; repeated, it moves on past the next one.
        keys(&mut visual, &text, "te");
        assert_eq!(visual.cursor(), spot(2, 1, 0));
        keys(&mut visual, &text, ";");
        assert_eq!(visual.cursor(), spot(2, 1, 2));
        keys(&mut visual, &text, "$Tg");
        assert_eq!(visual.cursor(), spot(2, 1, 9));
        // Not there, or not that many: the cursor stays.
        keys(&mut visual, &text, "0fz");
        assert_eq!(visual.cursor(), spot(2, 1, 0));
        keys(&mut visual, &text, "9fe");
        assert_eq!(visual.cursor(), spot(2, 1, 0));
        // Escape gives up the search but stays in visual mode.
        assert_eq!(visual.feed(Key::Char('f'), &keymap(), &text), Outcome::Stay);
        assert_eq!(visual.feed(Key::Escape, &keymap(), &text), Outcome::Stay);
        assert_eq!(visual.feed(Key::Escape, &keymap(), &text), Outcome::Leave);
    }

    #[test]
    fn a_count_of_t_stops_short_of_each_in_turn() {
        let text = text();
        // "seven eight nine": the first e is right next to the cursor.
        let mut visual = Visual::new(Kind::Characters, spot(2, 1, 0));
        keys(&mut visual, &text, "2te");
        assert_eq!(visual.cursor(), spot(2, 1, 2));
        keys(&mut visual, &text, "$2Te");
        assert_eq!(visual.cursor(), spot(2, 1, 4));
    }

    /// Found by fuzzing: a motion that went nowhere was still repeated the
    /// whole count, which for a count of billions froze the window.
    #[test]
    fn huge_counts_stop_once_the_cursor_does() {
        let text = text();
        let mut visual = Visual::new(Kind::Characters, spot(2, 1, 0));
        let started = std::time::Instant::now();
        for motion in [
            "$", "0", "^", "l", "h", "w", "b", "e", "te", "Te", "fe", ";", ",",
        ] {
            keys(&mut visual, &text, &format!("4000000000{motion}"));
        }
        keys(&mut visual, &text, "4000000000j4000000000k");
        assert!(started.elapsed().as_secs() < 1, "{:?}", started.elapsed());
    }

    #[test]
    fn caret_goes_to_the_first_character_that_is_not_a_space() {
        let text = HashMap::from([(0, page(&["   indented"]))]);
        let mut visual = Visual::new(Kind::Characters, spot(0, 0, 8));
        keys(&mut visual, &text, "^");
        assert_eq!(visual.cursor(), spot(0, 0, 3));
    }

    #[test]
    fn capital_h_highlights_what_is_selected() {
        let text = text();
        let mut visual = Visual::new(Kind::Characters, spot(0, 0, 4));
        let expected = visual.areas(&text);
        assert_eq!(
            keys(&mut visual, &text, "H"),
            Outcome::Mark(MarkupKind::Highlight, expected.clone())
        );
        for (key, kind) in [
            ("U", MarkupKind::Underline),
            ("S", MarkupKind::StrikeOut),
            ("W", MarkupKind::Squiggly),
        ] {
            let mut visual = Visual::new(Kind::Characters, spot(0, 0, 4));
            assert_eq!(
                keys(&mut visual, &text, key),
                Outcome::Mark(kind, expected.clone())
            );
        }
    }

    #[test]
    fn o_swaps_the_ends_and_v_switches_or_leaves() {
        let text = text();
        let mut visual = Visual::new(Kind::Characters, spot(0, 0, 0));
        keys(&mut visual, &text, "llo");
        assert_eq!(visual.cursor(), spot(0, 0, 0));
        keys(&mut visual, &text, "V");
        assert_eq!(visual.kind, Kind::Lines);
        assert_eq!(keys(&mut visual, &text, "v"), Outcome::Stay);
        assert_eq!(visual.kind, Kind::Characters);
        assert_eq!(keys(&mut visual, &text, "v"), Outcome::Leave);
        assert_eq!(visual.feed(Key::Escape, &keymap(), &text), Outcome::Leave);
    }

    #[test]
    fn the_areas_selected_cover_each_line() {
        let text = text();
        let mut visual = Visual::new(Kind::Characters, spot(0, 0, 4));
        keys(&mut visual, &text, "j");
        let areas = visual.areas(&text);
        let close = |left: f32, right: f32| (left - right).abs() < 1e-3;
        assert_eq!(areas.len(), 2);
        assert!(close(areas[0].1.left, 40.0) && close(areas[0].1.right, 130.0));
        // Down from "t" lands on the space after "four".
        assert!(close(areas[1].1.left, 0.0) && close(areas[1].1.right, 50.0));
    }

    #[test]
    fn neighbouring_pages_are_wanted() {
        let visual = Visual::new(Kind::Characters, spot(0, 0, 0));
        assert_eq!(visual.neighbours(3), [1]);
        let visual = Visual::new(Kind::Characters, spot(2, 0, 0));
        assert_eq!(visual.neighbours(3), [1]);
        let visual = Visual::new(Kind::Characters, spot(1, 0, 0));
        assert_eq!(visual.neighbours(3), [0, 2]);
    }
}
