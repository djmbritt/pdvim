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
use crate::prose;

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
        /// As flash.nvim's `s`: type some text, then a match's label.
        Flash = "flash", "Move the cursor to text typed next, by its label";
        /// Then `y` copies it as a picture, and `H` and the rest mark it.
        Figure = "figure", "Label the pictures and figures in view, to select one";
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
        /// As vim's `ip`; again, or with a count, takes in the next ones.
        InnerParagraph = "inner-paragraph", "Select the paragraph, or the next too";
        /// As vim's `ap`. A PDF has no blank lines after paragraphs, so the
        /// text copied ends with one instead.
        AroundParagraph = "around-paragraph", "Select the paragraph and the gap after it";
        /// As vim's `iw`; again, or with a count, takes in the next words.
        InnerWord = "inner-word", "Select the word, or the next too";
        /// As vim's `aw`: the spaces after the word, or else before it.
        AroundWord = "around-word", "Select the word and the space after it";
        /// As vim's `is`; again, or with a count, takes in the next ones.
        InnerSentence = "inner-sentence", "Select the sentence, or the next too";
        /// As vim's `as`: the spaces after the sentence, or else before it.
        AroundSentence = "around-sentence", "Select the sentence and the space after it";
        /// As vim's `*`, for the selection: a word is found only whole.
        SearchSelection = "search-selection", "Search forward for the selection, and leave";
        SearchSelectionBack = "search-selection-back", "Search backward for the selection, and leave";
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
    /// Ask for text to jump the cursor to.
    Flash,
    /// Label the pictures and figures in view, to select one.
    Figure,
    /// Copy this area of this page as a picture, and leave.
    CopyPicture(u32, Rect),
    /// Search for this text, forward or backward, as a whole word if it
    /// is one, and leave.
    Search {
        text: String,
        forward: bool,
        whole_word: bool,
    },
}

/// Where across the page `j` and `k` take the cursor.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Goal {
    /// The character that covers this far across, or the nearest.
    Across(f32),
    /// The end of each line, after `$`, as in vim.
    End,
}

/// The selection: where it started, where the cursor is, and how.
#[derive(Debug, Clone, PartialEq)]
pub struct Visual {
    pub kind: Kind,
    anchor: Spot,
    cursor: Spot,
    /// Where across the page `j` and `k` aim for, kept from one to the
    /// next so that short lines do not pull the cursor left for good.
    goal: Option<Goal>,
    /// The count and keys typed so far.
    pending: Pending<Binding>,
    /// A search whose character is still to be typed, and its count.
    finding: Option<(bool, bool, u32)>,
    /// The last search for a character, for `;` and `,` to repeat.
    last_find: Option<Find>,
    /// Whether `ap` selected the paragraphs, so that the text copied ends
    /// with a blank line.
    around: bool,
    /// Whether the selection runs from where it started to the cursor, or
    /// there is only the cursor, after a jump with flash, until `v` or `V`
    /// starts selecting from it.
    selecting: bool,
    /// A picture or figure selected instead of text: its page and area.
    figure: Option<(u32, Rect)>,
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
            around: false,
            selecting: true,
            figure: None,
        }
    }

    pub const fn cursor(&self) -> Spot {
        self.cursor
    }

    /// The same selection afresh, as `gv` selects it again: nothing typed
    /// towards a key is kept.
    pub const fn again(&self) -> Self {
        let mut again = Self::new(self.kind, self.anchor);
        again.cursor = self.cursor;
        again
    }

    /// Selects a text object, as vim's `iw`, `as` or `ip` do. `around` says
    /// whether the selection was whole paragraphs and their gaps before.
    fn select_object(&mut self, binding: Binding, times: u32, text: &Text, around: bool) {
        match binding {
            Binding::InnerWord | Binding::AroundWord => {
                self.words(text, times, binding == Binding::AroundWord);
            }
            Binding::InnerSentence | Binding::AroundSentence => {
                self.sentences(text, times, binding == Binding::AroundSentence);
            }
            _ => {
                self.paragraphs(text, times);
                self.around = binding == Binding::AroundParagraph
                    || (around && self.anchor != self.cursor);
            }
        }
    }

    /// A search for the selection, its spaces made single, for a whole
    /// word if it is one; leaving, when there is nothing to search for.
    fn search(&self, text: &Text, forward: bool) -> Outcome {
        let selected = self.text(text);
        let selected = selected.split_whitespace().collect::<Vec<_>>().join(" ");
        if selected.is_empty() {
            return Outcome::Leave;
        }
        let whole_word = selected.chars().all(|c| c.is_alphanumeric() || c == '_');
        Outcome::Search {
            text: selected,
            forward,
            whole_word,
        }
    }

    /// Selects whole sentences, as vim's `is` and `as` do: the sentence at
    /// the cursor, or, with a selection already made or a count, the
    /// sentences after it too; around them, the spaces after the last, or
    /// else those before the first.
    fn sentences(&mut self, text: &Text, times: u32, around: bool) {
        let mut more = times;
        if self.anchor == self.cursor {
            self.anchor = sentence_start(text, self.cursor);
            self.cursor = sentence_end(text, self.anchor);
            more = more.saturating_sub(1);
        }
        for _ in 0..more {
            let Some(next) = next_sentence_start(text, self.cursor) else {
                break;
            };
            self.cursor = sentence_end(text, next);
        }
        if !around {
            return;
        }
        // The spaces up to the next sentence, which may run over the end of
        // the line; with none, those before the first.
        match next_sentence_start(text, self.cursor).and_then(|next| before(text, next)) {
            Some(last) if last > self.cursor => self.cursor = last,
            _ => {
                let space = |spot: Spot| {
                    line(text, spot.page, spot.line)
                        .and_then(|line| line.chars.get(spot.index))
                        .is_some_and(|c| c.character.is_whitespace())
                };
                while let Some(index) = self.anchor.index.checked_sub(1)
                    && space(Spot {
                        index,
                        ..self.anchor
                    })
                {
                    self.anchor.index = index;
                }
            }
        }
    }

    /// Selects whole words, as vim's `iw` and `aw` do: the word at the
    /// cursor, or, with a selection already made or a count, the words
    /// after it too; around them, the spaces after the last, or else those
    /// before the first.
    fn words(&mut self, text: &Text, times: u32, around: bool) {
        let mut more = times;
        if self.anchor == self.cursor {
            let (first, last) = word_at(text, self.cursor);
            self.anchor = first;
            self.cursor = last;
            more = more.saturating_sub(1);
        }
        for _ in 0..more {
            match self.horizontal(Binding::WordEnd, text) {
                Some(moved) if moved != self.cursor => self.cursor = moved,
                _ => break,
            }
        }
        if !around {
            return;
        }
        let space = |spot: Spot| {
            line(text, spot.page, spot.line)
                .and_then(|line| line.chars.get(spot.index))
                .is_some_and(|c| c.character.is_whitespace())
        };
        let after = |spot: Spot| Spot {
            index: spot.index.saturating_add(1),
            ..spot
        };
        if space(after(self.cursor)) {
            while space(after(self.cursor)) {
                self.cursor = after(self.cursor);
            }
        } else {
            while let Some(index) = self.anchor.index.checked_sub(1)
                && space(Spot {
                    index,
                    ..self.anchor
                })
            {
                self.anchor.index = index;
            }
        }
    }

    /// Puts the cursor at the end of its line, and keeps it at the end of
    /// each line `j` and `k` go to, as `V` starts.
    pub fn stick_to_line_end(&mut self, text: &Text) {
        if let Some(current) = line(text, self.cursor.page, self.cursor.line) {
            self.cursor.index = current.chars.len().saturating_sub(1);
        }
        self.goal = Some(Goal::End);
    }

    /// Puts the cursor at a spot; the selection runs from where it started
    /// to there.
    pub const fn move_to(&mut self, spot: Spot) {
        self.cursor = spot;
        self.goal = None;
    }

    /// Just a cursor at a spot, as a jump with flash leaves: it moves
    /// without selecting until `v` or `V` starts a selection from it.
    pub const fn cursor_at(at: Spot) -> Self {
        let mut visual = Self::new(Kind::Characters, at);
        visual.selecting = false;
        visual
    }

    /// Puts just the cursor at a spot, letting go of the selection, as a
    /// jump with flash does.
    pub const fn place_at(&mut self, spot: Spot) {
        self.kind = Kind::Characters;
        self.anchor = spot;
        self.cursor = spot;
        self.goal = None;
        self.around = false;
        self.selecting = false;
    }

    /// Selects a picture or figure, the area of a page, instead of text.
    pub const fn select_figure(&mut self, page: u32, area: Rect) {
        self.figure = Some((page, area));
    }

    /// The picture or figure selected, if one is: its page and area.
    pub const fn figure(&self) -> Option<(u32, Rect)> {
        self.figure
    }

    /// Whether text is selected, or there is only the cursor.
    pub const fn selecting(&self) -> bool {
        self.selecting
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
            Fed::Waiting | Fed::Unbound(_) | Fed::Command(..) => Outcome::Stay,
        }
    }

    /// Does what a binding says, a number of times where that means
    /// anything.
    fn act(&mut self, binding: Binding, times: u32, text: &Text) -> Outcome {
        if let Some(outcome) = self.act_on_figure(binding) {
            return outcome;
        }
        if !self.selecting {
            match binding {
                // `v` and `V` start selecting from the cursor.
                Binding::Characters | Binding::Lines => {
                    self.selecting = true;
                    self.anchor = self.cursor;
                    self.kind = if binding == Binding::Characters {
                        Kind::Characters
                    } else {
                        Kind::Lines
                    };
                    // As `V` starts it from a label: at the line's end.
                    if self.kind == Kind::Lines {
                        self.stick_to_line_end(text);
                    }
                    return Outcome::Stay;
                }
                // A text object is selected, from the cursor.
                Binding::InnerWord
                | Binding::AroundWord
                | Binding::InnerSentence
                | Binding::AroundSentence
                | Binding::InnerParagraph
                | Binding::AroundParagraph => self.selecting = true,
                _ => {}
            }
        }
        let outcome = self.act_on_selection(binding, times, text);
        // With only a cursor, the selection is wherever it goes.
        if !self.selecting {
            self.anchor = self.cursor;
            self.kind = Kind::Characters;
        }
        outcome
    }

    /// Does what a binding says to a picture or figure selected: copying
    /// or marking it, a note beside it, or leaving. Anything else lets it
    /// go, and is done to the text from the cursor, so `None`.
    fn act_on_figure(&mut self, binding: Binding) -> Option<Outcome> {
        let (page, area) = self.figure?;
        let mark = |kind| Some(Outcome::Mark(kind, vec![(page, area)]));
        match binding {
            Binding::Yank => Some(Outcome::CopyPicture(page, area)),
            Binding::Highlight => mark(MarkupKind::Highlight),
            Binding::Underline => mark(MarkupKind::Underline),
            Binding::StrikeOut => mark(MarkupKind::StrikeOut),
            Binding::Squiggly => mark(MarkupKind::Squiggly),
            Binding::Unmark => Some(Outcome::Unmark(vec![(page, area)])),
            Binding::Note => Some(Outcome::Note(page, area)),
            Binding::Leave => Some(Outcome::Leave),
            Binding::Figure => Some(Outcome::Figure),
            // Neither has a meaning for a picture.
            Binding::OtherEnd | Binding::SearchSelection | Binding::SearchSelectionBack => {
                Some(Outcome::Stay)
            }
            _ => {
                self.figure = None;
                None
            }
        }
    }

    /// Does what a binding says to the selection.
    fn act_on_selection(&mut self, binding: Binding, times: u32, text: &Text) -> Outcome {
        // Anything but copying, or taking in more paragraphs, makes the
        // selection something other than whole paragraphs and their gaps.
        let around = std::mem::take(&mut self.around);
        match binding {
            Binding::Leave => return Outcome::Leave,
            Binding::Flash => return Outcome::Flash,
            Binding::Figure => return Outcome::Figure,
            Binding::Yank => {
                let mut copied = self.text(text);
                if around {
                    copied.push_str("\n\n");
                }
                return Outcome::Copy(copied);
            }
            Binding::InnerWord
            | Binding::AroundWord
            | Binding::InnerSentence
            | Binding::AroundSentence
            | Binding::InnerParagraph
            | Binding::AroundParagraph => self.select_object(binding, times, text, around),
            Binding::SearchSelection => return self.search(text, true),
            Binding::SearchSelectionBack => return self.search(text, false),
            Binding::Highlight => return Outcome::Mark(MarkupKind::Highlight, self.areas(text)),
            Binding::Underline => return Outcome::Mark(MarkupKind::Underline, self.areas(text)),
            Binding::StrikeOut => return Outcome::Mark(MarkupKind::StrikeOut, self.areas(text)),
            Binding::Squiggly => return Outcome::Mark(MarkupKind::Squiggly, self.areas(text)),
            Binding::Unmark => return Outcome::Unmark(self.areas(text)),
            Binding::Note => {
                // The note goes at the start of the cursor's sentence.
                let start = sentence_start(text, self.cursor);
                return character(text, start)
                    .map_or(Outcome::Leave, |area| Outcome::Note(start.page, area));
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
                // Lines are selected with the cursor at the end of its
                // line, where it stays as it goes up and down; characters
                // let it go.
                if kind == Kind::Lines {
                    self.stick_to_line_end(text);
                } else {
                    self.goal = None;
                }
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
                self.goal = matches!(binding, Binding::LineEnd).then_some(Goal::End);
            }
        }
        Outcome::Stay
    }

    /// Selects whole paragraphs, as vim's `ip` and `ap` do: the one the
    /// cursor is in and those after it, `times` in all, from the start of
    /// the first to the end of the last. With more than a character
    /// selected already, the selection goes on to the ends of the next
    /// paragraphs instead, the cursor's own first if it is partway along.
    fn paragraphs(&mut self, text: &Text, times: u32) {
        let at = (self.cursor.page, self.cursor.line);
        let Some(mut end) = paragraph_end(text, at) else {
            return;
        };
        let mut left = times.saturating_sub(1);
        if self.anchor == self.cursor {
            let (page, line) = paragraph_start(text, at);
            self.anchor = Spot::line_start(page, line);
        } else if end == at && self.kind == Kind::Lines {
            // The cursor ends a paragraph already: the next one counts.
            left = times;
        }
        for _ in 0..left {
            let Some(further) = next_line(text, end).and_then(|next| paragraph_end(text, next))
            else {
                break;
            };
            end = further;
        }
        self.kind = Kind::Lines;
        self.goal = None;
        self.cursor = Spot {
            page: end.0,
            line: end.1,
            index: line(text, end.0, end.1).map_or(0, |line| line.chars.len().saturating_sub(1)),
        };
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
    /// under the goal across the page: the one that covers a point a
    /// quarter of the way into the cursor's character, so that the cursor
    /// goes straight down from where the character starts, rather than
    /// being pulled right by a wide one; or the nearest to it; or the last.
    fn vertical(&mut self, text: &Text, times: u32, down: bool) {
        let goal = *self.goal.get_or_insert_with(|| {
            Goal::Across(character(text, self.cursor).map_or(0.0, |area| {
                (area.right - area.left).mul_add(0.25, area.left)
            }))
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
        let index = match goal {
            Goal::End => target.chars.len().saturating_sub(1),
            Goal::Across(goal) => {
                // Nothing for a character that covers the goal.
                let distance = |area: &Rect| (area.left - goal).max(goal - area.right).max(0.0);
                target
                    .chars
                    .iter()
                    .enumerate()
                    .min_by(|(_, one), (_, other)| {
                        distance(&one.area).total_cmp(&distance(&other.area))
                    })
                    .map_or(0, |(index, _)| index)
            }
        };
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
        // Each line's part, and whether a paragraph starts there.
        let mut lines: Vec<(String, bool)> = Vec::new();
        let mut starts: HashMap<u32, Vec<usize>> = HashMap::new();
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
                let paragraph = if line_index == 0 {
                    !lines.is_empty() && page_starts_paragraph(text, page)
                } else {
                    starts
                        .entry(page)
                        .or_insert_with(|| {
                            text.get(&page)
                                .map(|lines| paragraph_starts(lines))
                                .unwrap_or_default()
                        })
                        .contains(&line_index)
                };
                lines.push((content, paragraph));
            }
            at = next_line(text, (page, line_index));
        }
        prose::join(lines.iter().map(|(line, starts)| (line.as_str(), *starts)))
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

/// Where sentences start partway along the lines of a page, as each line
/// and the place in it: after `.`, `!` or `?`, perhaps closed by a quote
/// or bracket, then a space or a line break, at a capital, a digit or an
/// opening quote. A full stop after a common abbreviation, such as `Mr.`,
/// or after an initial does not end a sentence. Sentences that start a
/// line are left out, as the line's start is labelled already.
pub fn sentence_starts(lines: &[TextLine]) -> Vec<(usize, usize)> {
    every_sentence_start(lines)
        .into_iter()
        .filter(|&(_, index)| index > 0)
        .collect()
}

/// The start of the sentence a spot is in: the last sentence to start on
/// its page at or before it, or else the page's first character.
pub fn sentence_start(text: &Text, at: Spot) -> Spot {
    let Some(lines) = text.get(&at.page) else {
        return at;
    };
    let spot = |(line, index)| Spot {
        page: at.page,
        line,
        index,
    };
    every_sentence_start(lines)
        .into_iter()
        .map(spot)
        .take_while(|start| *start <= at)
        .last()
        .or_else(|| {
            lines
                .iter()
                .enumerate()
                .find_map(|(line, text)| {
                    let index = text
                        .chars
                        .iter()
                        .position(|character| !character.character.is_whitespace())?;
                    Some(spot((line, index)))
                })
                .filter(|start| *start <= at)
        })
        .unwrap_or(at)
}

/// Where sentences start on a page, those that start a line among them,
/// and the page's first if it starts one. A line after a short one, such
/// as a heading or a paragraph's last line, starts one too, if it starts
/// with a capital.
fn every_sentence_start(lines: &[TextLine]) -> Vec<(usize, usize)> {
    const ENDS: &[char] = &['.', '!', '?', '\u{2026}'];
    const CLOSERS: &[char] = &['"', '\'', ')', ']', '\u{201d}', '\u{2019}', '\u{bb}'];
    const OPENERS: &[char] = &['"', '\'', '(', '[', '\u{201c}', '\u{2018}', '\u{ab}'];
    const ABBREVIATIONS: &[&str] = &[
        "mr", "mrs", "ms", "dr", "st", "prof", "sr", "jr", "vs", "cf", "fig", "no", "vol", "pp",
        "e.g", "i.e", "etc", "al", "approx", "ch", "sec", "eq",
    ];
    let mut starts = Vec::new();
    // The word so far, to tell an abbreviation's full stop from a
    // sentence's.
    let mut word = String::new();
    // Whether the text so far ends a sentence, and whether a space or a
    // line break has come since. A page starts as if after one.
    let mut ended = true;
    let mut gap = true;
    let (left, right) = lines
        .iter()
        .flat_map(|line| &line.chars)
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(left, right), character| {
            (left.min(character.area.left), right.max(character.area.right))
        });
    // A line that stops this far short of the text's right edge ends what
    // it is in.
    let short = (right - left).mul_add(-0.15, right);
    for (row, line) in lines.iter().enumerate() {
        let after_short = row
            .checked_sub(1)
            .and_then(|above| lines.get(above))
            .and_then(|above| above.chars.last())
            .is_some_and(|last| last.area.right < short);
        if ended || after_short {
            ended = true;
            gap = true;
        }
        for (index, text) in line.chars.iter().enumerate() {
            let character = text.character;
            if character.is_whitespace() {
                gap = gap || ended;
                word.clear();
                continue;
            }
            let opens = character.is_uppercase()
                || character.is_ascii_digit()
                || OPENERS.contains(&character);
            if ended && gap && opens {
                starts.push((row, index));
            }
            let shortened = character == '.'
                && (word.chars().count() == 1
                    || ABBREVIATIONS.contains(&word.to_lowercase().as_str()));
            if ENDS.contains(&character) && !shortened {
                ended = true;
                gap = false;
            } else if !(ended && !gap && CLOSERS.contains(&character)) {
                ended = false;
                gap = false;
            }
            word.push(character);
        }
        word.clear();
    }
    starts
}

/// The lines that start paragraphs on a page, the first among them. PDF
/// text has no blank lines between paragraphs, so a row of text starts one
/// when the row before it stops short of a right edge that full rows
/// share, as in justified text; when it is further below that row than
/// rows usually are; when it is indented past the rows on either side; or
/// when it is above the row before, as at the top of a column. Lines side
/// by side, such as a listing's line numbers and its code, make one row.
pub fn paragraph_starts(lines: &[TextLine]) -> Vec<usize> {
    let rows = rows(lines);
    // The usual distance from one row's top to the next one's.
    let mut steps: Vec<f32> = rows
        .windows(2)
        .filter_map(|pair| match pair {
            [(_, Some(above)), (_, Some(below))] => Some(below.top - above.top),
            _ => None,
        })
        .filter(|step| *step > 0.0)
        .collect();
    steps.sort_by(f32::total_cmp);
    let usual = steps.get(steps.len() / 2).copied();
    // Whether a right edge is one that several rows end at.
    let shared = |right: f32| {
        rows.iter()
            .filter_map(|(_, shape)| *shape)
            .filter(|shape| (shape.right - right).abs() <= EDGE)
            .count()
            >= 3
    };
    let shape = |row: Option<usize>| rows.get(row?).and_then(|(_, shape)| *shape);
    let mut starts = Vec::new();
    for (row, (first, here)) in rows.iter().enumerate() {
        let (Some(here), Some(above)) = (*here, shape(row.checked_sub(1))) else {
            starts.push(*first);
            continue;
        };
        let before = shape(row.checked_sub(2));
        let below = shape(row.checked_add(1));
        // The row above stops 15% of its column's width short of the
        // right edge the row before it ends at, which other rows share.
        let short = before.is_some_and(|before| {
            let width = before.right - before.left;
            shared(before.right) && above.right < width.mul_add(-0.15, before.right)
        });
        let far = usual.is_some_and(|usual| here.top - above.top > usual * 1.5);
        let indent = above.bottom - above.top;
        let indented = here.left > above.left + indent
            && below.is_none_or(|below| here.left > below.left + indent);
        let higher = here.bottom <= above.top;
        if short || far || indented || higher {
            starts.push(*first);
        }
    }
    starts
}

/// How near, in points, right edges are to count as one.
const EDGE: f32 = 1.5;

/// The rows of text on a page: each with the first of its lines, and
/// where they lie together. A line that sits beside the one before it,
/// overlapping it from top to bottom by more than half, is in its row.
fn rows(lines: &[TextLine]) -> Vec<(usize, Option<Shape>)> {
    let mut rows: Vec<(usize, Option<Shape>)> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let shape = Shape::of(line);
        if let (Some((_, Some(row))), Some(shape)) = (rows.last_mut(), shape) {
            let overlap = row.bottom.min(shape.bottom) - row.top.max(shape.top);
            let height = (row.bottom - row.top).min(shape.bottom - shape.top);
            if overlap > height * 0.5 {
                *row = Shape {
                    left: row.left.min(shape.left),
                    right: row.right.max(shape.right),
                    top: row.top.min(shape.top),
                    bottom: row.bottom.max(shape.bottom),
                };
                continue;
            }
        }
        rows.push((index, shape));
    }
    rows
}

/// Where a line of text lies on its page: the edges of its characters,
/// its left one at the first that is not a space.
#[derive(Debug, Clone, Copy)]
struct Shape {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}

impl Shape {
    fn of(line: &TextLine) -> Option<Self> {
        let first = line
            .chars
            .iter()
            .find(|character| !character.character.is_whitespace())?;
        let last = line
            .chars
            .iter()
            .rfind(|character| !character.character.is_whitespace())?;
        let (top, bottom) = line
            .chars
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(top, bottom), character| {
                (top.min(character.area.top), bottom.max(character.area.bottom))
            });
        Some(Self {
            left: first.area.left,
            right: last.area.right,
            top,
            bottom,
        })
    }
}

/// The first line of the paragraph a line is in. Paragraphs stop at the
/// edges of pages.
fn paragraph_start(text: &Text, (page, row): (u32, usize)) -> (u32, usize) {
    let start = text.get(&page).map_or(row, |lines| {
        paragraph_starts(lines)
            .into_iter()
            .take_while(|start| *start <= row)
            .last()
            .unwrap_or(row)
    });
    (page, start)
}

/// The last line of the paragraph a line is in, if the line is there.
fn paragraph_end(text: &Text, (page, row): (u32, usize)) -> Option<(u32, usize)> {
    let lines = text.get(&page)?;
    lines.get(row)?;
    let end = paragraph_starts(lines)
        .into_iter()
        .find(|start| *start > row)
        .unwrap_or(lines.len())
        .saturating_sub(1);
    Some((page, end))
}

/// A line of a page.
fn line(text: &Text, page: u32, line: usize) -> Option<&TextLine> {
    text.get(&page)?.get(line)
}

/// The character nearest a point of a page, in points from its top-left
/// corner: on the line the point is on, or else the nearest line, the
/// character it is over, or else the nearest along the line. None on a page
/// without text, or whose text is not read yet.
pub fn spot_at(text: &Text, page: u32, (across, down): (f32, f32)) -> Option<Spot> {
    let lines = text.get(&page)?;
    let distance = |line: &TextLine| {
        let top = line
            .chars
            .iter()
            .map(|c| c.area.top)
            .fold(f32::INFINITY, f32::min);
        let bottom = line
            .chars
            .iter()
            .map(|c| c.area.bottom)
            .fold(f32::NEG_INFINITY, f32::max);
        if down < top {
            top - down
        } else if down > bottom {
            down - bottom
        } else {
            0.0
        }
    };
    let (line_index, line) = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| !line.chars.is_empty())
        .min_by(|(_, a), (_, b)| distance(a).total_cmp(&distance(b)))?;
    // The character the point is over, or before which it falls.
    let index = line
        .chars
        .iter()
        .position(|c| across < c.area.right)
        .unwrap_or_else(|| line.chars.len().saturating_sub(1));
    Some(Spot {
        page,
        line: line_index,
        index,
    })
}

/// The start of the line of a page nearest the top of an area, as the
/// cursor goes to with a picture or figure selected; the page's first line
/// if its text is not read, or it has none.
pub fn spot_near(text: &Text, page: u32, area: Rect) -> Spot {
    let nearest = text.get(&page).and_then(|lines| {
        lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| Some((index, line.chars.first()?.area.top)))
            .min_by(|(_, one), (_, other)| {
                (one - area.top).abs().total_cmp(&(other - area.top).abs())
            })
            .map(|(index, _)| index)
    });
    Spot::line_start(page, nearest.unwrap_or(0))
}

/// The first and last characters of the word at a spot: letters and
/// digits, or a run of other characters that are not spaces.
pub fn word_at(text: &Text, spot: Spot) -> (Spot, Spot) {
    let Some(chars) = line(text, spot.page, spot.line).map(|line| &line.chars) else {
        return (spot, spot);
    };
    let kind = |index: usize| {
        chars.get(index).map(|c| {
            if c.character.is_alphanumeric() || c.character == '_' {
                1
            } else if c.character.is_whitespace() {
                0
            } else {
                2
            }
        })
    };
    let Some(wanted) = kind(spot.index) else {
        return (spot, spot);
    };
    let mut first = spot.index;
    while let Some(before) = first.checked_sub(1)
        && kind(before) == Some(wanted)
    {
        first = before;
    }
    let mut last = spot.index;
    while kind(last.saturating_add(1)) == Some(wanted) {
        last = last.saturating_add(1);
    }
    (
        Spot {
            index: first,
            ..spot
        },
        Spot {
            index: last,
            ..spot
        },
    )
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

/// Whether a page's first line starts a paragraph, rather than going on
/// with the one the page before ends with: only when that ended a
/// sentence on a short last line, and this starts one with a capital. A
/// page with no page of text before it, as far as is read, starts one.
pub fn page_starts_paragraph(text: &Text, page: u32) -> bool {
    let Some(first) = line(text, page, 0) else {
        return true;
    };
    let Some((before, row)) = previous_line(text, (page, 0)) else {
        return true;
    };
    let Some(lines) = text.get(&before) else {
        return true;
    };
    let last: String = lines
        .get(row)
        .map(|line| line.chars.iter().map(|c| c.character).collect())
        .unwrap_or_default();
    let opening: String = first.chars.iter().map(|c| c.character).collect();
    prose::ends_sentence(&last)
        && ends_short(lines)
        && opening.trim_start().starts_with(char::is_uppercase)
}

/// Whether the last line of a page stops well short of the right edge the
/// page's lines usually end at, as a paragraph's last line does: by more
/// than 15% of their width.
fn ends_short(lines: &[TextLine]) -> bool {
    let edges = |line: &TextLine| {
        let left = line
            .chars
            .iter()
            .map(|c| c.area.left)
            .fold(f32::INFINITY, f32::min);
        let right = line
            .chars
            .iter()
            .map(|c| c.area.right)
            .fold(f32::NEG_INFINITY, f32::max);
        (left, right)
    };
    let full: Vec<(f32, f32)> = lines
        .iter()
        .filter(|line| !line.chars.is_empty())
        .map(edges)
        .collect();
    let Some(&(_, last)) = full.last() else {
        return false;
    };
    let middle = |mut edges: Vec<f32>| {
        edges.sort_by(f32::total_cmp);
        edges.get(edges.len() / 2).copied()
    };
    let (Some(left), Some(right)) = (
        middle(full.iter().map(|(left, _)| *left).collect()),
        middle(full.iter().map(|(_, right)| *right).collect()),
    ) else {
        return false;
    };
    last < (right - left).mul_add(-0.15, right)
}

/// Where the first sentence after a spot starts: later on its page, or on
/// a page after, of those whose text is read.
fn next_sentence_start(text: &Text, at: Spot) -> Option<Spot> {
    let mut page = at.page;
    loop {
        let lines = text.get(&page)?;
        let found = every_sentence_start(lines)
            .into_iter()
            .map(|(line, index)| Spot { page, line, index })
            .find(|start| *start > at);
        if found.is_some() {
            return found;
        }
        page = page.checked_add(1)?;
    }
}

/// The last character of the sentence starting at a spot, spaces left
/// out: just before the next sentence, or else the end of the page.
fn sentence_end(text: &Text, start: Spot) -> Spot {
    let mut end = next_sentence_start(text, start)
        .and_then(|next| before(text, next))
        .or_else(|| {
            let lines = text.get(&start.page)?;
            let (row, last) = lines
                .iter()
                .enumerate()
                .rev()
                .find(|(_, line)| !line.chars.is_empty())?;
            Some(Spot {
                page: start.page,
                line: row,
                index: last.chars.len().saturating_sub(1),
            })
        })
        .unwrap_or(start);
    while end > start
        && line(text, end.page, end.line)
            .and_then(|line| line.chars.get(end.index))
            .is_none_or(|c| c.character.is_whitespace())
    {
        let Some(earlier) = before(text, end) else {
            break;
        };
        end = earlier;
    }
    end
}

/// The character before a spot: earlier on its line, or the last of the
/// line before, skipping empty lines.
fn before(text: &Text, at: Spot) -> Option<Spot> {
    if let Some(index) = at.index.checked_sub(1) {
        return Some(Spot { index, ..at });
    }
    let mut here = (at.page, at.line);
    loop {
        here = previous_line(text, here)?;
        let (page, row) = here;
        if let Some(index) = line(text, page, row)?.chars.len().checked_sub(1) {
            return Some(Spot {
                page,
                line: row,
                index,
            });
        }
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

    #[test]
    fn sentences_are_found_where_they_start() {
        let lines = page(&[
            "Down the hole. She fell! Was it",
            "far? \u{201c}Yes,\u{201d} said Mr. Rabbit. e.g. no J. Smith",
            "More. (Aside.) Then",
            "end.",
            "Next line",
        ]);
        assert_eq!(
            sentence_starts(&lines),
            vec![(0, 15), (0, 25), (1, 5), (2, 6), (2, 15)]
        );
    }

    #[test]
    fn a_spot_is_in_the_sentence_started_last() {
        let mut text = Text::new();
        text.insert(
            0,
            page(&[
                "the end. First one goes",
                "on here. Second goes",
                "on and on",
                "Heading",
                "Para starts here and",
                "goes on.",
            ]),
        );
        let start = |line, index| sentence_start(&text, Spot { page: 0, line, index });
        // Before any sentence starts, the page's first character.
        assert_eq!(start(0, 4), Spot { page: 0, line: 0, index: 0 });
        // "First" starts at 9; "on here" is still in that sentence.
        assert_eq!(start(0, 9), Spot { page: 0, line: 0, index: 9 });
        assert_eq!(start(1, 3), Spot { page: 0, line: 0, index: 9 });
        // "Second" starts at 9 of the second line.
        assert_eq!(start(2, 4), Spot { page: 0, line: 1, index: 9 });
        // A line after a short one starts a sentence: the heading, and
        // the paragraph after it.
        assert_eq!(start(3, 2), Spot { page: 0, line: 3, index: 0 });
        assert_eq!(start(5, 2), Spot { page: 0, line: 4, index: 0 });
    }

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

    #[test]
    fn a_point_finds_the_character_under_it_or_the_nearest() {
        let text = text();
        // Over the "w" of "two".
        assert_eq!(spot_at(&text, 0, (55.0, 5.0)), Some(spot(0, 0, 5)));
        // Between the lines, nearer the second; past the end of it.
        assert_eq!(spot_at(&text, 0, (500.0, 17.0)), Some(spot(0, 1, 8)));
        // Above the text.
        assert_eq!(spot_at(&text, 2, (5.0, -40.0)), Some(spot(2, 0, 0)));
        assert_eq!(spot_at(&text, 1, (5.0, 5.0)), None);
        assert_eq!(spot_at(&text, 7, (5.0, 5.0)), None);
    }

    #[test]
    fn inner_and_around_sentence_select_sentences_and_their_spaces() {
        let text = HashMap::from([
            (0, page(&["One two. Three four", "five. Six seven."])),
            (1, page(&["Eight nine."])),
        ]);
        let keymap = crate::bindings::Keymaps::default().visual;
        // The cursor on the "u" of "four".
        let typed = |keys: &str| {
            let mut visual = Visual::new(Kind::Characters, spot(0, 0, 16));
            for key in keys.chars() {
                let _ = visual.feed(Key::Char(key), &keymap, &text);
            }
            (visual.text(&text), visual.cursor())
        };
        assert_eq!(typed("is").0, "Three four five.");
        // The space after "five." is taken in.
        assert_eq!(typed("as").1, spot(0, 1, 5));
        assert_eq!(typed("isis").0, "Three four five. Six seven.");
        // On to the next page, and no further than the text read. The
        // first page's last line is short, and ends a sentence, so the next
        // page starts a paragraph.
        assert_eq!(
            typed("isisis").0,
            "Three four five. Six seven.\n\nEight nine."
        );
        assert_eq!(typed("isisisis").1, spot(1, 0, 10));
        // At the end, with no space after, the space before is taken in.
        let mut last = Visual::new(Kind::Characters, spot(1, 0, 2));
        for key in "as".chars() {
            let _ = last.feed(Key::Char(key), &keymap, &text);
        }
        assert_eq!(last.text(&text), "Eight nine.");
    }

    #[test]
    fn inner_and_around_word_select_words_and_their_spaces() {
        let text = text();
        let keymap = crate::bindings::Keymaps::default().visual;
        let typed = |keys: &str| {
            let mut visual = Visual::new(Kind::Characters, spot(0, 0, 5));
            let mut outcome = Outcome::Stay;
            for key in keys.chars() {
                outcome = visual.feed(Key::Char(key), &keymap, &text);
            }
            (visual.text(&text), outcome, visual.cursor())
        };
        assert_eq!(typed("iw").0, "two");
        // The space after it is selected too, though copying leaves out
        // spaces at the end.
        assert_eq!(typed("aw").2, spot(0, 0, 7));
        assert_eq!(typed("iwiw").0, "two three");
        assert_eq!(
            typed("iw*").1,
            Outcome::Search {
                text: "two".to_owned(),
                forward: true,
                whole_word: true
            }
        );
        assert_eq!(
            typed("aw#").1,
            Outcome::Search {
                text: "two".to_owned(),
                forward: false,
                whole_word: true
            }
        );
    }

    #[test]
    fn a_word_is_found_around_a_spot() {
        let text = text();
        assert_eq!(
            word_at(&text, spot(0, 0, 5)),
            (spot(0, 0, 4), spot(0, 0, 6))
        );
        assert_eq!(
            word_at(&text, spot(0, 0, 0)),
            (spot(0, 0, 0), spot(0, 0, 2))
        );
        assert_eq!(
            word_at(&text, spot(0, 1, 8)),
            (spot(0, 1, 5), spot(0, 1, 8))
        );
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
    fn after_a_dollar_lines_are_gone_through_at_their_ends() {
        let text = text();
        let mut visual = Visual::new(Kind::Characters, spot(0, 0, 0));
        keys(&mut visual, &text, "$");
        let first = line(&text, 0, 0).unwrap().chars.len();
        assert_eq!(visual.cursor(), spot(0, 0, first - 1));
        keys(&mut visual, &text, "j");
        let second = line(&text, 0, 1).unwrap().chars.len();
        assert_eq!(visual.cursor(), spot(0, 1, second - 1));
        // A motion across the line ends it.
        keys(&mut visual, &text, "0j");
        assert_eq!(visual.cursor(), spot(2, 0, 0));
    }

    #[test]
    fn a_selection_of_lines_starts_at_the_end_of_its_line() {
        let text = text();
        let mut visual = Visual::new(Kind::Lines, spot(0, 0, 0));
        visual.stick_to_line_end(&text);
        let first = line(&text, 0, 0).unwrap().chars.len();
        assert_eq!(visual.cursor(), spot(0, 0, first - 1));
        keys(&mut visual, &text, "j");
        let second = line(&text, 0, 1).unwrap().chars.len();
        assert_eq!(visual.cursor(), spot(0, 1, second - 1));
    }

    #[test]
    fn a_sentence_ending_a_full_last_line_goes_on_over_the_page() {
        let text = HashMap::from([
            (0, page(&["a line as long as the next", "ends a sentence right there."])),
            (1, page(&["Then the page goes on.", "with more"])),
        ]);
        let mut visual = Visual::new(Kind::Characters, spot(0, 1, 0));
        assert_eq!(
            keys(&mut visual, &text, "j$y"),
            Outcome::Copy("ends a sentence right there. Then the page goes on.".to_owned())
        );
    }

    #[test]
    fn the_selection_is_copied_as_running_text() {
        let text = text();
        let mut visual = Visual::new(Kind::Characters, spot(0, 0, 4));
        assert_eq!(
            keys(&mut visual, &text, "j$y"),
            Outcome::Copy("two three four five".to_owned())
        );
        // Backwards selections read forwards, and the page between, with no
        // text, joins the lines on either side.
        let mut visual = Visual::new(Kind::Characters, spot(2, 0, 2));
        assert_eq!(
            keys(&mut visual, &text, "kby"),
            Outcome::Copy("four five six".to_owned())
        );
        // Whole lines.
        let mut visual = Visual::new(Kind::Lines, spot(0, 1, 6));
        assert_eq!(
            keys(&mut visual, &text, "jy"),
            Outcome::Copy("four five six".to_owned())
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
    fn placing_the_cursor_lets_go_of_the_selection() {
        let text = text();
        let mut visual = Visual::new(Kind::Lines, spot(0, 0, 0));
        keys(&mut visual, &text, "j");
        visual.place_at(spot(0, 0, 4));
        assert!(!visual.selecting());
        assert_eq!(visual.kind, Kind::Characters);
        assert_eq!(visual.cursor(), spot(0, 0, 4));
        assert_eq!((visual.anchor, visual.cursor), (spot(0, 0, 4), spot(0, 0, 4)));
    }

    #[test]
    fn selecting_lines_puts_the_cursor_at_the_end_of_its_line() {
        let text = text();
        let end = |line: usize| text[&0][line].chars.len() - 1;
        let mut from_cursor = Visual::cursor_at(spot(0, 0, 2));
        keys(&mut from_cursor, &text, "V");
        assert_eq!(from_cursor.cursor(), spot(0, 0, end(0)));
        keys(&mut from_cursor, &text, "j");
        assert_eq!(from_cursor.cursor(), spot(0, 1, end(1)));
        let mut switched = Visual::new(Kind::Characters, spot(0, 0, 2));
        keys(&mut switched, &text, "V");
        assert_eq!(switched.cursor(), spot(0, 0, end(0)));
        assert_eq!(switched.anchor, spot(0, 0, 2));
    }

    #[test]
    fn a_figure_selected_is_copied_or_marked_whole_and_a_motion_lets_it_go() {
        let text = text();
        let area = Rect {
            left: 10.0,
            top: 20.0,
            right: 110.0,
            bottom: 70.0,
        };
        let with_figure = || {
            let mut visual = Visual::new(Kind::Characters, spot(0, 0, 2));
            visual.select_figure(1, area);
            visual
        };
        assert_eq!(keys(&mut with_figure(), &text, "y"), Outcome::CopyPicture(1, area));
        assert_eq!(
            keys(&mut with_figure(), &text, "H"),
            Outcome::Mark(MarkupKind::Highlight, vec![(1, area)])
        );
        assert_eq!(keys(&mut with_figure(), &text, "d"), Outcome::Unmark(vec![(1, area)]));
        assert_eq!(keys(&mut with_figure(), &text, "n"), Outcome::Note(1, area));
        assert_eq!(keys(&mut with_figure(), &text, "I"), Outcome::Figure);
        assert_eq!(keys(&mut with_figure(), &text, "o"), Outcome::Stay);
        let mut moved = with_figure();
        assert_eq!(keys(&mut moved, &text, "l"), Outcome::Stay);
        assert_eq!(moved.figure(), None);
        assert_eq!(moved.cursor(), spot(0, 0, 3));
        // Without a figure, the same key asks for the labels.
        let mut plain = Visual::new(Kind::Characters, spot(0, 0, 2));
        assert_eq!(keys(&mut plain, &text, "I"), Outcome::Figure);
    }

    #[test]
    fn moving_the_cursor_while_selecting_keeps_where_it_started() {
        let mut visual = Visual::new(Kind::Characters, spot(0, 0, 2));
        visual.move_to(spot(0, 1, 5));
        assert!(visual.selecting());
        assert_eq!((visual.anchor, visual.cursor), (spot(0, 0, 2), spot(0, 1, 5)));
    }

    #[test]
    fn just_a_cursor_moves_without_selecting_until_v() {
        let text = text();
        let mut visual = Visual::cursor_at(spot(0, 0, 0));
        keys(&mut visual, &text, "ll");
        assert_eq!((visual.anchor, visual.cursor), (spot(0, 0, 2), spot(0, 0, 2)));
        assert_eq!(keys(&mut visual, &text, "v"), Outcome::Stay);
        assert!(visual.selecting());
        keys(&mut visual, &text, "l");
        assert_eq!((visual.anchor, visual.cursor), (spot(0, 0, 2), spot(0, 0, 3)));
        let mut lines = Visual::cursor_at(spot(0, 0, 2));
        keys(&mut lines, &text, "V");
        assert!(lines.selecting());
        assert_eq!(lines.kind, Kind::Lines);
        let mut word = Visual::cursor_at(spot(0, 0, 0));
        keys(&mut word, &text, "iw");
        assert!(word.selecting());
        assert_ne!(word.anchor, word.cursor);
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

    /// The same lines, those from `row` on lowered by `by` points.
    fn lowered(mut lines: Vec<TextLine>, row: usize, by: f32) -> Vec<TextLine> {
        for line in lines.iter_mut().skip(row) {
            for character in &mut line.chars {
                character.area.top += by;
                character.area.bottom += by;
            }
        }
        lines
    }

    /// Three paragraphs of justified text, each full line 26 characters
    /// long: one ended by a short line, one by an indented line after it,
    /// and one by itself.
    fn paragraphs() -> Vec<TextLine> {
        page(&[
            "The first paragraph runs o",
            "to here.",
            "The second one runs on and",
            "on and fills its last line",
            "    The third is indented,",
            "goes on and on to the end ",
            "here.",
        ])
    }

    #[test]
    fn paragraphs_start_after_short_lines_and_at_indents() {
        assert_eq!(paragraph_starts(&paragraphs()), vec![0, 2, 4]);
    }

    #[test]
    fn paragraphs_start_after_a_gap_and_at_the_top_of_a_column() {
        let lines = page(&[
            "A heading as wide as text",
            "The text under it runs on",
            "and on, and it carries on",
            "to the foot of the column",
            "and carries on up in the ",
        ]);
        // Further below the heading than lines usually are.
        let lines = lowered(lines, 1, 15.0);
        assert_eq!(paragraph_starts(&lines), vec![0, 1]);
        // The last line moved up beside the first, as a second column.
        let lines = lowered(lines, 4, -90.0);
        assert_eq!(paragraph_starts(&lines), vec![0, 1, 4]);
    }

    #[test]
    fn a_listing_is_one_paragraph_however_its_lines_end() {
        // Each line number and its code are lines of their own, side by
        // side, and the code's lines end where they end.
        let mut lines = Vec::new();
        for (row, code) in ["digraph G {", "  main -> parse;", "  main -> init;", "}"]
            .iter()
            .enumerate()
        {
            let mut number = page(&[&format!("{}:", row + 1)]);
            let mut code = page(&[&format!("    {code}")]);
            let lower = 20.0 * f32::from(u8::try_from(row).unwrap());
            number = lowered(number, 0, lower);
            code = lowered(code, 0, lower);
            lines.extend(number);
            lines.extend(code);
        }
        assert_eq!(paragraph_starts(&lines), vec![0]);
    }

    #[test]
    fn i_p_selects_the_paragraph_the_cursor_is_in() {
        let text = Text::from([(0, paragraphs())]);
        let mut visual = Visual::new(Kind::Characters, spot(0, 3, 5));
        keys(&mut visual, &text, "ip");
        assert_eq!(visual.kind, Kind::Lines);
        assert_eq!(visual.anchor, spot(0, 2, 0));
        assert_eq!(visual.cursor, spot(0, 3, 25));
        assert_eq!(
            keys(&mut visual, &text, "y"),
            Outcome::Copy("The second one runs on and on and fills its last line".into())
        );
    }

    #[test]
    fn i_p_again_or_with_a_count_takes_in_the_next_paragraphs() {
        let text = Text::from([(0, paragraphs())]);
        let mut visual = Visual::new(Kind::Characters, spot(0, 0, 3));
        keys(&mut visual, &text, "ipip");
        assert_eq!((visual.anchor, visual.cursor.line), (spot(0, 0, 0), 3));
        let mut counted = Visual::new(Kind::Characters, spot(0, 0, 3));
        keys(&mut counted, &text, "3ip");
        assert_eq!((counted.anchor, counted.cursor.line), (spot(0, 0, 0), 6));
        // There is no fourth: the selection stays at the last.
        keys(&mut counted, &text, "ip");
        assert_eq!(counted.cursor.line, 6);
    }

    #[test]
    fn a_selection_partway_along_goes_on_to_its_paragraph_end_first() {
        let text = Text::from([(0, paragraphs())]);
        let mut visual = Visual::new(Kind::Characters, spot(0, 2, 0));
        keys(&mut visual, &text, "l");
        keys(&mut visual, &text, "ip");
        assert_eq!((visual.anchor, visual.cursor.line), (spot(0, 2, 0), 3));
    }

    #[test]
    fn paragraphs_run_on_to_the_next_page_when_taken_in() {
        let text = Text::from([
            (0, page(&["Only paragraph of the page"])),
            (1, Vec::new()),
            (2, page(&["The next one, two pages on"])),
        ]);
        let mut visual = Visual::new(Kind::Characters, spot(0, 0, 2));
        keys(&mut visual, &text, "2ip");
        assert_eq!(visual.cursor, spot(2, 0, 25));
    }

    #[test]
    fn a_p_copies_the_paragraph_with_a_blank_line_after_it() {
        let text = Text::from([(0, paragraphs())]);
        let mut visual = Visual::new(Kind::Characters, spot(0, 1, 2));
        assert_eq!(
            keys(&mut visual, &text, "apy"),
            Outcome::Copy("The first paragraph runs o to here.\n\n".into())
        );
        // Moving afterwards makes it a selection like any other.
        let mut moved = Visual::new(Kind::Characters, spot(0, 1, 2));
        assert_eq!(
            keys(&mut moved, &text, "apjy"),
            Outcome::Copy(
                "The first paragraph runs o to here.\n\nThe second one runs on and".into()
            )
        );
    }
}
