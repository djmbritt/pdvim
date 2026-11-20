//! The list of recently read documents, for picking one to open.

use std::path::PathBuf;

use crate::input::Key;

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
    /// Whether `g` was just pressed, waiting for a second `g`.
    g: bool,
}

impl Picker {
    /// A list of these documents, the first selected. There is no list of
    /// nothing.
    pub fn new(entries: Vec<PathBuf>) -> Option<Self> {
        (!entries.is_empty()).then_some(Self {
            entries,
            selected: 0,
            g: false,
        })
    }

    pub fn entries(&self) -> &[PathBuf] {
        &self.entries
    }

    pub const fn selected(&self) -> usize {
        self.selected
    }

    /// Takes a key. Moving the selection keeps the list open; a choice
    /// closes it.
    pub fn feed(&mut self, key: Key) -> Option<Choice> {
        let last = self.entries.len().saturating_sub(1);
        let g = std::mem::take(&mut self.g);
        match key {
            Key::Char('j') | Key::Down | Key::Ctrl('n' | 'j') => {
                self.selected = self.selected.saturating_add(1).min(last);
            }
            Key::Char('k') | Key::Up | Key::Ctrl('p' | 'k') => {
                self.selected = self.selected.saturating_sub(1);
            }
            Key::Char('g') if g => self.selected = 0,
            Key::Char('g') => self.g = true,
            Key::Char('G') | Key::End => self.selected = last,
            Key::Home => self.selected = 0,
            Key::Enter | Key::Char('l' | 'o') | Key::Right => return self.choose(self.selected),
            // A digit opens that entry, counted from 1.
            Key::Char(digit @ '1'..='9') => {
                let index = digit
                    .to_digit(10)
                    .and_then(|digit| usize::try_from(digit).ok())?
                    .saturating_sub(1);
                return self.choose(index);
            }
            Key::Escape | Key::Ctrl('c' | '[') | Key::Char('q') => return Some(Choice::Close),
            _ => {}
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
        assert_eq!(picker.feed(Key::Char('k')), None);
        assert_eq!(picker.selected(), 0);
        picker.feed(Key::Char('j'));
        picker.feed(Key::Down);
        picker.feed(Key::Ctrl('n'));
        assert_eq!(picker.selected(), 2);
        picker.feed(Key::Char('g'));
        picker.feed(Key::Char('g'));
        assert_eq!(picker.selected(), 0);
        picker.feed(Key::Char('G'));
        assert_eq!(picker.selected(), 2);
        picker.feed(Key::Up);
        assert_eq!(picker.feed(Key::Enter), Some(open("/b.pdf")));
    }

    #[test]
    fn a_digit_opens_that_entry() {
        let mut picker = picker();
        assert_eq!(picker.feed(Key::Char('3')), Some(open("/c.pdf")));
        assert_eq!(picker.feed(Key::Char('1')), Some(open("/a.pdf")));
        assert_eq!(picker.feed(Key::Char('4')), None);
    }

    #[test]
    fn escape_and_q_close_the_list() {
        let mut picker = picker();
        assert_eq!(picker.feed(Key::Escape), Some(Choice::Close));
        assert_eq!(picker.feed(Key::Char('q')), Some(Choice::Close));
        assert_eq!(picker.feed(Key::Ctrl('c')), Some(Choice::Close));
    }
}
