//! Picking from the keyboard: each thing in view, such as a link to follow
//! or a line to start selecting at, gets a label of two letters, and typing
//! a label picks it.

use crate::input::Key;
use crate::keymap::{Counts, Fed, Keymap, Pending, bindable};

bindable! {
    /// What keys can be bound to while labels are shown. Letters bound to
    /// nothing type a label.
    pub enum Binding {
        Cancel = "cancel", "Put the labels away";
        Back = "back", "Take back the first letter typed, or the last of the text; with none, cancel";
        /// Typed after it, text narrows the labels to the things that hold
        /// it, such as the words of a link.
        Filter = "filter", "Narrow the labels by text typed next; enter picks the only one left";
        /// From the labels on lines after `v` or `V`, say.
        Figures = "figures", "Label the pictures and figures instead, to select one";
    }
}

/// The letters labels are made of, unless set otherwise: the home row.
pub const HOME_ROW: &str = "asdfghjkl";
/// The letters added to the ones set when there are too many things for
/// two of them to label.
const ALL: [char; 26] = [
    'a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l', 'q', 'w', 'e', 'r', 't', 'y', 'u', 'i', 'o', 'p',
    'z', 'x', 'c', 'v', 'b', 'n', 'm',
];

/// A thing in view and its label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hint<T> {
    pub label: [char; 2],
    /// The page the thing is on, counted from 0.
    pub page: u32,
    pub item: T,
}

/// What a key typed while the labels are shown comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome<T> {
    /// Half a label: wait for the other half.
    Wait,
    /// A whole label: this was picked.
    Picked(Hint<T>),
    /// Put the labels away.
    Cancel,
    /// No label starts like that: put the labels away and say so.
    NoMatch,
    /// Label the pictures and figures in view instead, to select one.
    Figures,
}

/// The labels shown, and the first letter of one if it has been typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hints<T> {
    /// Everything labelled, each with its page and its text.
    items: Vec<(u32, T, String)>,
    /// The letters labels are made of.
    letters: Vec<char>,
    labelled: Vec<Hint<T>>,
    typed: Option<char>,
    /// The text typed to narrow the labels, and whether more is being
    /// typed.
    filter: String,
    filtering: bool,
    pending: Pending<Binding>,
}

impl<T: Clone> Hints<T> {
    /// Labels the things given, in the order given, with labels made of
    /// these letters, and more when they are too few. There are no labels
    /// for nothing. Things beyond what two letters can label are left out.
    pub fn new(items: Vec<(u32, T)>, letters: &[char]) -> Option<Self> {
        if items.is_empty() {
            return None;
        }
        let mut letters: Vec<char> = letters.to_vec();
        letters.dedup();
        for extra in ALL {
            if letters.len().saturating_mul(letters.len()) >= items.len() {
                break;
            }
            if !letters.contains(&extra) {
                letters.push(extra);
            }
        }
        let mut hints = Self {
            items: items
                .into_iter()
                .map(|(page, item)| (page, item, String::new()))
                .collect(),
            letters,
            labelled: Vec::new(),
            typed: None,
            filter: String::new(),
            filtering: false,
            pending: Pending::new(),
        };
        hints.label();
        Some(hints)
    }

    /// Gives each thing the text that narrowing matches it by.
    pub fn set_texts(&mut self, text: impl Fn(u32, &T) -> String) {
        for (page, item, kept) in &mut self.items {
            *kept = text(*page, item);
        }
    }

    /// Labels the things that hold the text typed, from the first label
    /// on.
    fn label(&mut self) {
        let count = self.letters.len();
        let filter = self.filter.to_lowercase();
        self.labelled = self
            .items
            .iter()
            .filter(|(_, _, text)| filter.is_empty() || text.to_lowercase().contains(&filter))
            .take(count.saturating_mul(count))
            .enumerate()
            .filter_map(|(index, (page, item, _))| {
                // The first letter changes fastest, so that few links share
                // one and the first key already narrows them down.
                let first = *self.letters.get(index.checked_rem(count)?)?;
                let second = *self.letters.get(index.checked_div(count)?)?;
                Some(Hint {
                    label: [first, second],
                    page: *page,
                    item: item.clone(),
                })
            })
            .collect();
        self.typed = None;
    }

    /// The labels still in play: all of them, or those that start with the
    /// letter typed.
    pub fn shown(&self) -> impl Iterator<Item = &Hint<T>> {
        self.labelled
            .iter()
            .filter(|hint| self.typed.is_none_or(|typed| hint.label[0] == typed))
    }

    /// The first letter typed, if any.
    pub const fn typed(&self) -> Option<char> {
        self.typed
    }

    /// The text typed to narrow the labels, while there is some or more is
    /// being typed.
    pub fn filter(&self) -> Option<&str> {
        (self.filtering || !self.filter.is_empty()).then_some(self.filter.as_str())
    }

    /// Takes a key, bound as in the keymap.
    pub fn feed(&mut self, key: Key, keymap: &Keymap<Binding>) -> Outcome<T> {
        let fed = self.pending.feed(key, keymap, Counts::NotTaken);
        if self.filtering {
            return self.narrow(fed, key);
        }
        let letter = match fed {
            Fed::Bound(Binding::Cancel, _) => return Outcome::Cancel,
            Fed::Bound(Binding::Back, _) => {
                if self.typed.take().is_none() {
                    return Outcome::Cancel;
                }
                return Outcome::Wait;
            }
            Fed::Bound(Binding::Filter, _) => {
                self.typed = None;
                self.filtering = true;
                return Outcome::Wait;
            }
            Fed::Bound(Binding::Figures, _) => return Outcome::Figures,
            // The last key of a sequence bound to nothing is a letter of a
            // label, if it is a letter at all.
            Fed::Unbound(keys) => match keys.last() {
                Some(Key::Char(character)) => character.to_ascii_lowercase(),
                _ => return Outcome::Wait,
            },
            Fed::Waiting | Fed::Command(..) => return Outcome::Wait,
        };
        let Some(first) = self.typed else {
            if self.labelled.iter().any(|hint| hint.label[0] == letter) {
                self.typed = Some(letter);
                return Outcome::Wait;
            }
            return Outcome::NoMatch;
        };
        self.labelled
            .iter()
            .find(|hint| hint.label == [first, letter])
            .map_or(Outcome::NoMatch, |hint| Outcome::Picked(hint.clone()))
    }

    /// Takes a key while text to narrow the labels is typed. Enter picks
    /// the only label left, or stops typing text so that a label can be
    /// typed.
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "the other keys do nothing here"
    )]
    fn narrow(&mut self, fed: Fed<Binding>, key: Key) -> Outcome<T> {
        match fed {
            Fed::Bound(Binding::Cancel, _) => return Outcome::Cancel,
            Fed::Bound(Binding::Back, _) => {
                if self.filter.pop().is_none() {
                    self.filtering = false;
                }
            }
            Fed::Unbound(keys) if keys.last() == Some(&Key::Enter) => {
                self.filtering = false;
                return match self.labelled.as_slice() {
                    [only] => Outcome::Picked(only.clone()),
                    [] => Outcome::NoMatch,
                    _ => Outcome::Wait,
                };
            }
            Fed::Unbound(keys) => self.filter.extend(keys.iter().filter_map(|key| match key {
                Key::Char(character) => Some(*character),
                Key::ShiftSpace => Some(' '),
                _ => None,
            })),
            Fed::Bound(Binding::Filter, _) => self.filter.push('/'),
            // Text being typed, the key is a letter of it.
            Fed::Bound(Binding::Figures, _) => {
                if let Key::Char(character) = key {
                    self.filter.push(character);
                }
            }
            Fed::Waiting | Fed::Command(..) => return Outcome::Wait,
        }
        self.label();
        Outcome::Wait
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{Link, Target};
    use crate::layout::Rect;

    fn keymap() -> Keymap<Binding> {
        crate::bindings::Keymaps::default().hints
    }

    fn home_row() -> Vec<char> {
        HOME_ROW.chars().collect()
    }

    fn links(count: u32) -> Vec<(u32, Link)> {
        (0..count)
            .map(|index| {
                (
                    index,
                    Link {
                        area: Rect {
                            left: 0.0,
                            top: 0.0,
                            right: 1.0,
                            bottom: 1.0,
                        },
                        target: Target::Page {
                            page: index,
                            top: None,
                        },
                    },
                )
            })
            .collect()
    }

    fn labels(hints: &Hints<Link>) -> Vec<String> {
        hints
            .shown()
            .map(|hint| hint.label.iter().collect())
            .collect()
    }

    #[test]
    fn capital_i_asks_for_the_figures_but_is_typed_into_text() {
        let keymap = crate::bindings::Keymaps::default().hints;
        let mut hints = Hints::new(links(3), &home_row()).unwrap();
        assert_eq!(hints.feed(Key::Char('I'), &keymap), Outcome::Figures);
        assert_eq!(hints.feed(Key::Char('/'), &keymap), Outcome::Wait);
        assert_eq!(hints.feed(Key::Char('I'), &keymap), Outcome::Wait);
        assert_eq!(hints.filter(), Some("I"));
    }

    #[test]
    fn no_links_no_labels() {
        assert_eq!(Hints::<Link>::new(Vec::new(), &home_row()), None);
    }

    #[test]
    fn labels_come_from_the_home_row_first_letter_first() {
        let hints = Hints::new(links(11), &home_row()).unwrap();
        assert_eq!(
            labels(&hints),
            [
                "aa", "sa", "da", "fa", "ga", "ha", "ja", "ka", "la", "as", "ss"
            ]
        );
    }

    #[test]
    fn many_links_use_the_whole_alphabet_and_every_label_differs() {
        let hints = Hints::new(links(100), &home_row()).unwrap();
        let mut seen = labels(&hints);
        assert_eq!(seen.len(), 100);
        assert_eq!(seen[9], "qa");
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), 100);
        // Beyond 26 × 26, the rest are left out.
        assert_eq!(
            Hints::new(links(700), &home_row()).unwrap().shown().count(),
            676
        );
    }

    #[test]
    fn two_letters_follow_a_link() {
        let mut hints = Hints::new(links(11), &home_row()).unwrap();
        assert_eq!(hints.feed(Key::Char('s'), &keymap()), Outcome::Wait);
        assert_eq!(hints.typed(), Some('s'));
        assert_eq!(labels(&hints), ["sa", "ss"]);
        let Outcome::Picked(hint) = hints.feed(Key::Char('S'), &keymap()) else {
            panic!("expected to follow a link");
        };
        assert_eq!(hint.label, ['s', 's']);
        assert_eq!(hint.page, 10);
    }

    #[test]
    fn backspace_escape_and_strangers() {
        let mut hints = Hints::new(links(3), &home_row()).unwrap();
        assert_eq!(hints.feed(Key::Char('a'), &keymap()), Outcome::Wait);
        assert_eq!(hints.feed(Key::Backspace, &keymap()), Outcome::Wait);
        assert_eq!(hints.typed(), None);
        assert_eq!(hints.feed(Key::Backspace, &keymap()), Outcome::Cancel);
        assert_eq!(hints.feed(Key::Char('z'), &keymap()), Outcome::NoMatch);
        hints.feed(Key::Char('a'), &keymap());
        assert_eq!(hints.feed(Key::Char('z'), &keymap()), Outcome::NoMatch);
        assert_eq!(hints.feed(Key::Escape, &keymap()), Outcome::Cancel);
    }

    #[test]
    fn labels_are_made_of_the_letters_set() {
        let hints = Hints::new(links(5), &['x', 'y']).unwrap();
        // Two letters label four; a third is taken from the alphabet.
        assert_eq!(labels(&hints), ["xx", "yx", "ax", "xy", "yy"]);
    }

    #[test]
    fn text_typed_after_the_filter_key_narrows_the_labels() {
        let mut hints = Hints::new(links(4), &home_row()).unwrap();
        let texts = ["Intro", "Methods", "Results", "Introspection"];
        hints.set_texts(|page, _| {
            let index = usize::try_from(page).unwrap();
            texts[index].to_owned()
        });
        assert_eq!(hints.feed(Key::Char('/'), &keymap()), Outcome::Wait);
        assert_eq!(hints.filter(), Some(""));
        for character in "INTRO".chars() {
            assert_eq!(hints.feed(Key::Char(character), &keymap()), Outcome::Wait);
        }
        // Labelled afresh, from the first label.
        assert_eq!(labels(&hints), ["aa", "sa"]);
        let pages: Vec<u32> = hints.shown().map(|hint| hint.page).collect();
        assert_eq!(pages, [0, 3]);
        // Enter stops typing text: letters type a label again.
        assert_eq!(hints.feed(Key::Enter, &keymap()), Outcome::Wait);
        assert_eq!(hints.filter(), Some("INTRO"));
        assert_eq!(hints.feed(Key::Char('s'), &keymap()), Outcome::Wait);
        let Outcome::Picked(hint) = hints.feed(Key::Char('a'), &keymap()) else {
            panic!("expected a pick");
        };
        assert_eq!(hint.page, 3);
    }

    #[test]
    fn enter_picks_the_only_label_left_and_backspace_widens() {
        let mut hints = Hints::new(links(3), &home_row()).unwrap();
        hints.set_texts(|page, _| format!("link {page}"));
        hints.feed(Key::Char('/'), &keymap());
        hints.feed(Key::Char('2'), &keymap());
        hints.feed(Key::Char('9'), &keymap());
        assert_eq!(hints.shown().count(), 0);
        hints.feed(Key::Backspace, &keymap());
        assert_eq!(hints.shown().count(), 1);
        let Outcome::Picked(hint) = hints.feed(Key::Enter, &keymap()) else {
            panic!("expected a pick");
        };
        assert_eq!(hint.page, 2);
        // With nothing typed, backspace leaves the text, then the labels.
        let mut hints = Hints::new(links(3), &home_row()).unwrap();
        hints.feed(Key::Char('/'), &keymap());
        assert_eq!(hints.feed(Key::Backspace, &keymap()), Outcome::Wait);
        assert_eq!(hints.filter(), None);
        assert_eq!(hints.feed(Key::Backspace, &keymap()), Outcome::Cancel);
    }
}
