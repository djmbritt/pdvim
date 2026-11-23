//! Lists to pick from by typing: the recently read documents, to open
//! one, and the open tabs, to go to one. Typing narrows the list to the
//! entries that hold every word typed.

use crate::input::Key;
use crate::keymap::{Counts, Fed, Keymap, Pending, bindable};

bindable! {
    /// What keys can be bound to in a list to pick from. Keys bound to
    /// nothing type, to narrow the list.
    pub enum Binding {
        Down = "down", "Select the next entry";
        Up = "up", "Select the previous entry";
        Top = "top", "Select the first entry";
        Bottom = "bottom", "Select the last entry";
        Open = "open", "Open the document or go to the tab selected";
        RubOut = "rub-out", "Rub out a character typed; with none, close";
        ClearLine = "clear-line", "Rub out everything typed";
        Close = "close", "Close the list";
    }
}

/// How many recent documents the list holds at most.
pub const SHOWN: usize = 20;

/// What a key pressed in the list asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice<T> {
    /// Open this, or go to it.
    Open(T),
    /// Close the list without picking anything.
    Close,
}

/// An entry: what the list shows and what typing is matched against, and
/// what picking it gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry<T> {
    pub text: String,
    pub value: T,
}

/// The list on screen, what has been typed, and which entry is selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picker<T> {
    entries: Vec<Entry<T>>,
    typed: String,
    /// The entries that hold every word typed, by their place.
    shown: Vec<usize>,
    /// The entry selected, by its place among those shown.
    selected: usize,
    pending: Pending<Binding>,
}

impl<T: Clone> Picker<T> {
    /// A list of these entries, the first selected. There is no list of
    /// nothing.
    pub fn new(entries: Vec<Entry<T>>) -> Option<Self> {
        (!entries.is_empty()).then(|| Self {
            shown: (0..entries.len()).collect(),
            entries,
            typed: String::new(),
            selected: 0,
            pending: Pending::new(),
        })
    }

    /// The entries that hold every word typed, in order.
    pub fn shown(&self) -> impl Iterator<Item = &Entry<T>> {
        self.shown
            .iter()
            .filter_map(|&index| self.entries.get(index))
    }

    /// The place of the entry selected among those shown.
    pub const fn selected(&self) -> usize {
        self.selected
    }

    /// What has been typed to narrow the list.
    pub fn typed(&self) -> &str {
        &self.typed
    }

    /// What picking the entry shown in this place gives, as a click on it
    /// does.
    pub fn choose(&mut self, index: usize) -> Option<T> {
        let entry = self
            .shown
            .get(index)
            .and_then(|&entry| self.entries.get(entry))?;
        let value = entry.value.clone();
        self.selected = index;
        Some(value)
    }

    /// Selects one of the entries shown, by its place among them.
    pub const fn select(&mut self, index: usize) {
        if index < self.shown.len() {
            self.selected = index;
        }
    }

    /// Takes a key, bound as in the keymap. Moving the selection and typing
    /// keep the list open; a choice closes it.
    #[expect(clippy::wildcard_enum_match_arm, reason = "the other keys do nothing here")]
    pub fn feed(&mut self, key: Key, keymap: &Keymap<Binding>) -> Option<Choice<T>> {
        let last = self.shown.len().saturating_sub(1);
        match self.pending.feed(key, keymap, Counts::NotTaken) {
            Fed::Bound(Binding::Down, _) => {
                self.selected = self.selected.saturating_add(1).min(last);
            }
            Fed::Bound(Binding::Up, _) => self.selected = self.selected.saturating_sub(1),
            Fed::Bound(Binding::Top, _) => self.selected = 0,
            Fed::Bound(Binding::Bottom, _) => self.selected = last,
            Fed::Bound(Binding::Open, _) => {
                return self
                    .shown
                    .get(self.selected)
                    .and_then(|&index| self.entries.get(index))
                    .map(|entry| Choice::Open(entry.value.clone()));
            }
            Fed::Bound(Binding::Close, _) => return Some(Choice::Close),
            Fed::Bound(Binding::RubOut, _) => {
                if self.typed.pop().is_none() {
                    return Some(Choice::Close);
                }
                self.narrow();
            }
            Fed::Bound(Binding::ClearLine, _) => {
                self.typed.clear();
                self.narrow();
            }
            Fed::Unbound(keys) => {
                let before = self.typed.len();
                self.typed.extend(keys.iter().filter_map(|key| match key {
                    Key::Char(character) => Some(*character),
                    Key::ShiftSpace => Some(' '),
                    _ => None,
                }));
                if self.typed.len() != before {
                    self.narrow();
                }
            }
            Fed::Waiting | Fed::Command(..) => {}
        }
        None
    }

    /// Shows only the entries that hold every word typed, ignoring case,
    /// and selects the first.
    fn narrow(&mut self) {
        let words: Vec<String> = self
            .typed
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        self.shown = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                let text = entry.text.to_lowercase();
                words.iter().all(|word| text.contains(word.as_str()))
            })
            .map(|(index, _)| index)
            .collect();
        self.selected = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keymap() -> Keymap<Binding> {
        crate::bindings::Keymaps::default().recent
    }

    fn picker() -> Picker<u32> {
        Picker::new(
            ["/books/rust.pdf", "/papers/raft.pdf", "/books/c.pdf"]
                .iter()
                .zip(1..)
                .map(|(text, value)| Entry {
                    text: (*text).to_owned(),
                    value,
                })
                .collect(),
        )
        .unwrap()
    }

    fn typing(picker: &mut Picker<u32>, text: &str) {
        for character in text.chars() {
            assert_eq!(picker.feed(Key::Char(character), &keymap()), None);
        }
    }

    fn shown(picker: &Picker<u32>) -> Vec<u32> {
        picker.shown().map(|entry| entry.value).collect()
    }

    #[test]
    fn an_empty_list_is_not_shown() {
        assert_eq!(Picker::<u32>::new(Vec::new()), None);
    }

    #[test]
    fn the_selection_moves_and_stops_at_the_ends() {
        let mut picker = picker();
        assert_eq!(picker.feed(Key::Up, &keymap()), None);
        assert_eq!(picker.selected(), 0);
        picker.feed(Key::Down, &keymap());
        picker.feed(Key::Ctrl('n'), &keymap());
        picker.feed(Key::Ctrl('j'), &keymap());
        assert_eq!(picker.selected(), 2);
        picker.feed(Key::Home, &keymap());
        assert_eq!(picker.selected(), 0);
        picker.feed(Key::End, &keymap());
        assert_eq!(picker.selected(), 2);
        picker.feed(Key::Ctrl('p'), &keymap());
        assert_eq!(picker.feed(Key::Enter, &keymap()), Some(Choice::Open(2)));
    }

    #[test]
    fn typing_narrows_to_the_entries_holding_every_word() {
        let mut picker = picker();
        typing(&mut picker, "BOOKS");
        assert_eq!(shown(&picker), [1, 3]);
        picker.feed(Key::Down, &keymap());
        typing(&mut picker, " c");
        // Narrowing selects the first again.
        assert_eq!(picker.selected(), 0);
        assert_eq!(shown(&picker), [3]);
        assert_eq!(picker.typed(), "BOOKS c");
        typing(&mut picker, "x");
        assert_eq!(shown(&picker), Vec::<u32>::new());
        assert_eq!(picker.feed(Key::Enter, &keymap()), None);
        picker.feed(Key::Backspace, &keymap());
        assert_eq!(shown(&picker), [3]);
        picker.feed(Key::Ctrl('u'), &keymap());
        assert_eq!(shown(&picker), [1, 2, 3]);
        // Rubbing out with nothing typed closes the list.
        assert_eq!(picker.feed(Key::Backspace, &keymap()), Some(Choice::Close));
    }

    #[test]
    fn letters_and_digits_type() {
        let mut picker = picker();
        typing(&mut picker, "jkq3");
        assert_eq!(picker.typed(), "jkq3");
    }

    #[test]
    fn escape_closes_the_list() {
        let mut picker = picker();
        assert_eq!(picker.feed(Key::Escape, &keymap()), Some(Choice::Close));
        assert_eq!(picker.feed(Key::Ctrl('c'), &keymap()), Some(Choice::Close));
    }
}
