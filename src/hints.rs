//! Following links from the keyboard: each link in view gets a label of two
//! letters, and typing a label follows its link.

use crate::backend::Link;
use crate::input::Key;

/// The letters labels are made of when few are needed: the home row.
const HOME_ROW: [char; 9] = ['a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l'];
/// The letters labels are made of when the home row is not enough.
const ALL: [char; 26] = [
    'a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l', 'q', 'w', 'e', 'r', 't', 'y', 'u', 'i', 'o', 'p',
    'z', 'x', 'c', 'v', 'b', 'n', 'm',
];

/// A link in view and its label.
#[derive(Debug, Clone, PartialEq)]
pub struct Hint {
    pub label: [char; 2],
    /// The page the link is on, counted from 0.
    pub page: u32,
    pub link: Link,
}

/// What a key typed while the labels are shown comes to.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Half a label: wait for the other half.
    Wait,
    /// A whole label: follow this link.
    Follow(Hint),
    /// Put the labels away.
    Cancel,
    /// No label starts like that: put the labels away and say so.
    NoMatch,
}

/// The labels shown, and the first letter of one if it has been typed.
#[derive(Debug, Clone, PartialEq)]
pub struct Hints {
    hints: Vec<Hint>,
    typed: Option<char>,
}

impl Hints {
    /// Labels the links given, in the order given. There are no labels for
    /// no links. Links beyond what two letters can label are left out.
    pub fn new(links: Vec<(u32, Link)>) -> Option<Self> {
        let letters: &[char] = if links.len() <= HOME_ROW.len().saturating_mul(HOME_ROW.len()) {
            &HOME_ROW
        } else {
            &ALL
        };
        let count = letters.len();
        let hints: Vec<Hint> = links
            .into_iter()
            .take(count.saturating_mul(count))
            .enumerate()
            .filter_map(|(index, (page, link))| {
                // The first letter changes fastest, so that few links share
                // one and the first key already narrows them down.
                let first = *letters.get(index.checked_rem(count)?)?;
                let second = *letters.get(index.checked_div(count)?)?;
                Some(Hint {
                    label: [first, second],
                    page,
                    link,
                })
            })
            .collect();
        (!hints.is_empty()).then_some(Self { hints, typed: None })
    }

    /// The labels still in play: all of them, or those that start with the
    /// letter typed.
    pub fn shown(&self) -> impl Iterator<Item = &Hint> {
        self.hints
            .iter()
            .filter(|hint| self.typed.is_none_or(|typed| hint.label[0] == typed))
    }

    /// The first letter typed, if any.
    pub const fn typed(&self) -> Option<char> {
        self.typed
    }

    /// Takes a key.
    pub fn feed(&mut self, key: Key) -> Outcome {
        let letter = match key {
            Key::Escape | Key::Ctrl('c' | '[') => return Outcome::Cancel,
            Key::Backspace | Key::Ctrl('h') => {
                if self.typed.take().is_none() {
                    return Outcome::Cancel;
                }
                return Outcome::Wait;
            }
            Key::Char(character) => character.to_ascii_lowercase(),
            _ => return Outcome::Wait,
        };
        let Some(first) = self.typed else {
            if self.hints.iter().any(|hint| hint.label[0] == letter) {
                self.typed = Some(letter);
                return Outcome::Wait;
            }
            return Outcome::NoMatch;
        };
        self.hints
            .iter()
            .find(|hint| hint.label == [first, letter])
            .map_or(Outcome::NoMatch, |hint| Outcome::Follow(hint.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Target;
    use crate::layout::Rect;

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

    fn labels(hints: &Hints) -> Vec<String> {
        hints
            .shown()
            .map(|hint| hint.label.iter().collect())
            .collect()
    }

    #[test]
    fn no_links_no_labels() {
        assert_eq!(Hints::new(Vec::new()), None);
    }

    #[test]
    fn labels_come_from_the_home_row_first_letter_first() {
        let hints = Hints::new(links(11)).unwrap();
        assert_eq!(
            labels(&hints),
            [
                "aa", "sa", "da", "fa", "ga", "ha", "ja", "ka", "la", "as", "ss"
            ]
        );
    }

    #[test]
    fn many_links_use_the_whole_alphabet_and_every_label_differs() {
        let hints = Hints::new(links(100)).unwrap();
        let mut seen = labels(&hints);
        assert_eq!(seen.len(), 100);
        assert_eq!(seen[9], "qa");
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), 100);
        // Beyond 26 × 26, the rest are left out.
        assert_eq!(Hints::new(links(700)).unwrap().shown().count(), 676);
    }

    #[test]
    fn two_letters_follow_a_link() {
        let mut hints = Hints::new(links(11)).unwrap();
        assert_eq!(hints.feed(Key::Char('s')), Outcome::Wait);
        assert_eq!(hints.typed(), Some('s'));
        assert_eq!(labels(&hints), ["sa", "ss"]);
        let Outcome::Follow(hint) = hints.feed(Key::Char('S')) else {
            panic!("expected to follow a link");
        };
        assert_eq!(hint.label, ['s', 's']);
        assert_eq!(hint.page, 10);
    }

    #[test]
    fn backspace_escape_and_strangers() {
        let mut hints = Hints::new(links(3)).unwrap();
        assert_eq!(hints.feed(Key::Char('a')), Outcome::Wait);
        assert_eq!(hints.feed(Key::Backspace), Outcome::Wait);
        assert_eq!(hints.typed(), None);
        assert_eq!(hints.feed(Key::Backspace), Outcome::Cancel);
        assert_eq!(hints.feed(Key::Char('z')), Outcome::NoMatch);
        hints.feed(Key::Char('a'));
        assert_eq!(hints.feed(Key::Char('z')), Outcome::NoMatch);
        assert_eq!(hints.feed(Key::Escape), Outcome::Cancel);
    }
}
