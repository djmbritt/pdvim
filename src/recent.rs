//! The list of recently read documents, for picking one to open.

use std::path::PathBuf;

use crate::input::Key;
use crate::keymap::{Counts, Fed, Keymap, Pending, bindable};

bindable! {
    /// What keys can be bound to in the list of recent documents. A digit
    /// bound to nothing opens that entry, counted from 1.
    pub enum Binding {
        Down = "down", "Select the next document";
        Up = "up", "Select the previous document";
        Top = "top", "Select the first document";
        Bottom = "bottom", "Select the last document";
        Open = "open", "Open the document selected";
        Close = "close", "Close the list";
    }
}

/// How many documents the list shows at most.
pub const SHOWN: usize = 20;

/// What a key pressed in the list asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// Open this document.
    Open(PathBuf),
    /// Close the list without opening anything.
    Close,
}

/// The list on screen, and which entry is selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picker {
    entries: Vec<PathBuf>,
    selected: usize,
    pending: Pending<Binding>,
}

impl Picker {
    /// A list of these documents, the first selected. There is no list of
    /// nothing.
    pub fn new(entries: Vec<PathBuf>) -> Option<Self> {
        (!entries.is_empty()).then_some(Self {
            entries,
            selected: 0,
            pending: Pending::new(),
        })
    }

    pub fn entries(&self) -> &[PathBuf] {
        &self.entries
    }

    pub const fn selected(&self) -> usize {
        self.selected
    }

    /// Takes a key, bound as in the keymap. Moving the selection keeps the
    /// list open; a choice closes it.
    pub fn feed(&mut self, key: Key, keymap: &Keymap<Binding>) -> Option<Choice> {
        let last = self.entries.len().saturating_sub(1);
        match self.pending.feed(key, keymap, Counts::NotTaken) {
            Fed::Bound(Binding::Down, _) => {
                self.selected = self.selected.saturating_add(1).min(last);
            }
            Fed::Bound(Binding::Up, _) => self.selected = self.selected.saturating_sub(1),
            Fed::Bound(Binding::Top, _) => self.selected = 0,
            Fed::Bound(Binding::Bottom, _) => self.selected = last,
            Fed::Bound(Binding::Open, _) => return self.choose(self.selected),
            Fed::Bound(Binding::Close, _) => return Some(Choice::Close),
            Fed::Unbound(keys) => {
                if let [Key::Char(digit @ '1'..='9')] = keys.as_slice() {
                    let index = digit
                        .to_digit(10)
                        .and_then(|digit| usize::try_from(digit).ok())?
                        .saturating_sub(1);
                    return self.choose(index);
                }
            }
            Fed::Waiting => {}
        }
        None
    }

    fn choose(&self, index: usize) -> Option<Choice> {
        self.entries.get(index).cloned().map(Choice::Open)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keymap() -> Keymap<Binding> {
        crate::bindings::Keymaps::default().recent
    }

    fn picker() -> Picker {
        Picker::new(["/a.pdf", "/b.pdf", "/c.pdf"].map(PathBuf::from).to_vec()).unwrap()
    }

    fn open(path: &str) -> Choice {
        Choice::Open(PathBuf::from(path))
    }

    #[test]
    fn an_empty_list_is_not_shown() {
        assert_eq!(Picker::new(Vec::new()), None);
    }

    #[test]
    fn the_selection_moves_and_stops_at_the_ends() {
        let mut picker = picker();
        assert_eq!(picker.feed(Key::Char('k'), &keymap()), None);
        assert_eq!(picker.selected(), 0);
        picker.feed(Key::Char('j'), &keymap());
        picker.feed(Key::Down, &keymap());
        picker.feed(Key::Ctrl('n'), &keymap());
        assert_eq!(picker.selected(), 2);
        picker.feed(Key::Char('g'), &keymap());
        picker.feed(Key::Char('g'), &keymap());
        assert_eq!(picker.selected(), 0);
        picker.feed(Key::Char('G'), &keymap());
        assert_eq!(picker.selected(), 2);
        picker.feed(Key::Up, &keymap());
        assert_eq!(picker.feed(Key::Enter, &keymap()), Some(open("/b.pdf")));
    }

    #[test]
    fn a_digit_opens_that_entry() {
        let mut picker = picker();
        assert_eq!(picker.feed(Key::Char('3'), &keymap()), Some(open("/c.pdf")));
        assert_eq!(picker.feed(Key::Char('1'), &keymap()), Some(open("/a.pdf")));
        assert_eq!(picker.feed(Key::Char('4'), &keymap()), None);
    }

    #[test]
    fn escape_and_q_close_the_list() {
        let mut picker = picker();
        assert_eq!(picker.feed(Key::Escape, &keymap()), Some(Choice::Close));
        assert_eq!(picker.feed(Key::Char('q'), &keymap()), Some(Choice::Close));
        assert_eq!(picker.feed(Key::Ctrl('c'), &keymap()), Some(Choice::Close));
    }
}
