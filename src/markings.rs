//! The markings of a document: highlights, underlines, strike-outs and
//! squiggly underlines, made by pdvim or by another program. Going from one
//! to the next, finding those under a selection, and the sidebar that
//! lists them.

use crate::backend::Marking;
use crate::input::Key;
use crate::keymap::{Counts, Fed, Keymap, Pending, bindable};
use crate::layout::Rect;

bindable! {
    /// What keys can be bound to in the list of markings.
    pub enum Binding {
        Down = "down", "Select the next marking";
        Up = "up", "Select the previous marking";
        HalfDown = "half-down", "Select the marking half a screen down";
        HalfUp = "half-up", "Select the marking half a screen up";
        Top = "top", "Select the first marking";
        Bottom = "bottom", "Select the last marking";
        Go = "go", "Go to the marking selected";
        Delete = "delete", "Take the marking selected away";
        Change = "change", "Change the note on the marking selected";
        Close = "close", "Close the list";
    }
}

/// The marking a number of steps on from another, or from the view when
/// none is picked: the first that `below` says starts at or below the top
/// of the view, going forward, or the last that starts above it, going
/// back. The markings are in reading order. None past either end.
pub fn step(
    markings: &[Marking],
    from: Option<usize>,
    below: impl Fn(&Marking) -> bool,
    forward: bool,
    times: u32,
) -> Option<usize> {
    let times = usize::try_from(times.max(1)).unwrap_or(usize::MAX);
    let (start, further) = match from {
        Some(index) => (Some(index), times),
        None if forward => (markings.iter().position(&below), times.saturating_sub(1)),
        None => (
            markings.iter().rposition(|marking| !below(marking)),
            times.saturating_sub(1),
        ),
    };
    let start = start?;
    let to = if forward {
        start.checked_add(further)?
    } else {
        start.checked_sub(further)?
    };
    (to < markings.len()).then_some(to)
}

/// The markings that overlap any of the areas, each on its page.
pub fn touching(markings: &[Marking], areas: &[(u32, Rect)]) -> Vec<usize> {
    let overlap = |one: &Rect, other: &Rect| {
        one.left < other.right
            && other.left < one.right
            && one.top < other.bottom
            && other.top < one.bottom
    };
    markings
        .iter()
        .enumerate()
        .filter(|(_, marking)| {
            areas.iter().any(|(page, area)| {
                *page == marking.page && marking.areas.iter().any(|covered| overlap(covered, area))
            })
        })
        .map(|(index, _)| index)
        .collect()
}

/// The marking at a character: one over it, or else a note beside its
/// line, which sits in the margin.
pub fn at(markings: &[Marking], page: u32, character: Rect) -> Option<usize> {
    let over = touching(markings, &[(page, character)]);
    over.first().copied().or_else(|| {
        markings.iter().position(|marking| {
            marking.page == page
                && marking.kind == crate::backend::MarkupKind::Note
                && marking
                    .areas
                    .first()
                    .is_some_and(|area| area.top < character.bottom && character.top < area.bottom)
        })
    })
}

/// The box around every area of a marking.
pub fn extent(marking: &Marking) -> Option<Rect> {
    marking.areas.iter().copied().reduce(|one, other| Rect {
        left: one.left.min(other.left),
        top: one.top.min(other.top),
        right: one.right.max(other.right),
        bottom: one.bottom.max(other.bottom),
    })
}

/// What a key typed in the list comes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Stay,
    /// Go to the marking with this index, and close.
    Go(usize),
    /// Take the marking with this index away, and stay.
    Delete(usize),
    /// Change the note on the marking with this index.
    Change(usize),
    Close,
}

/// The list of markings in the sidebar, and which is selected.
#[derive(Debug, Clone, PartialEq)]
pub struct List {
    entries: Vec<Marking>,
    selected: usize,
    pending: Pending<Binding>,
}

impl List {
    /// The list of these markings, with the first on or after a page
    /// selected. There is no list of nothing.
    pub fn new(entries: Vec<Marking>, page: u32) -> Option<Self> {
        if entries.is_empty() {
            return None;
        }
        let selected = entries
            .iter()
            .position(|marking| marking.page >= page)
            .unwrap_or_else(|| entries.len().saturating_sub(1));
        Some(Self {
            entries,
            selected,
            pending: Pending::new(),
        })
    }

    /// The same list with the markings as they are now, the selection
    /// staying where it was as near as it can. None once none are left.
    pub fn refreshed(self, entries: Vec<Marking>) -> Option<Self> {
        if entries.is_empty() {
            return None;
        }
        let selected = self.selected.min(entries.len().saturating_sub(1));
        Some(Self {
            entries,
            selected,
            pending: self.pending,
        })
    }

    pub const fn selected(&self) -> usize {
        self.selected
    }

    /// The marking with this index.
    pub fn get(&self, index: usize) -> Option<&Marking> {
        self.entries.get(index)
    }

    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    /// The first entry shown in this many rows, so that the selected one
    /// shows, and the entries from there.
    pub fn shown(&self, rows: usize) -> (usize, &[Marking]) {
        let rows = rows.max(1);
        let top = self
            .selected
            .checked_div(rows)
            .unwrap_or(0)
            .saturating_mul(rows);
        let end = top.saturating_add(rows).min(self.entries.len());
        (top, self.entries.get(top..end).unwrap_or_default())
    }

    /// Takes a key, bound as in the keymap, given how many rows are shown.
    pub fn feed(&mut self, key: Key, keymap: &Keymap<Binding>, rows: usize) -> Outcome {
        let (binding, count) = match self.pending.feed(key, keymap, Counts::Taken) {
            Fed::Bound(binding, count) => (binding, count.unwrap_or(1)),
            Fed::Waiting | Fed::Unbound(_) | Fed::Command(..) => return Outcome::Stay,
        };
        let times = usize::try_from(count).unwrap_or(usize::MAX);
        let half = rows.max(1).div_ceil(2);
        let last = self.entries.len().saturating_sub(1);
        self.selected = match binding {
            Binding::Go => return Outcome::Go(self.selected),
            Binding::Delete => return Outcome::Delete(self.selected),
            Binding::Change => return Outcome::Change(self.selected),
            Binding::Close => return Outcome::Close,
            Binding::Down => self.selected.saturating_add(times),
            Binding::Up => self.selected.saturating_sub(times),
            Binding::HalfDown => self.selected.saturating_add(half),
            Binding::HalfUp => self.selected.saturating_sub(half),
            Binding::Top => 0,
            Binding::Bottom => last,
        }
        .min(last);
        Outcome::Stay
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{Kept, MarkupKind};

    fn marking(page: u32, top: f32) -> Marking {
        Marking {
            page,
            xref: i32::try_from(page).unwrap().saturating_add(1),
            kind: MarkupKind::Highlight,
            areas: vec![Rect {
                left: 10.0,
                top,
                right: 200.0,
                bottom: top + 12.0,
            }],
            text: String::new(),
            kept: Kept::default(),
        }
    }

    /// Markings on pages 1, 3 (two) and 6, with the view's top on page 3,
    /// between the two there.
    fn markings() -> Vec<Marking> {
        vec![
            marking(1, 100.0),
            marking(3, 50.0),
            marking(3, 400.0),
            marking(6, 20.0),
        ]
    }

    fn below(marking: &Marking) -> bool {
        marking.page > 3 || (marking.page == 3 && marking.areas[0].top >= 200.0)
    }

    #[test]
    fn stepping_from_the_view_starts_at_its_top() {
        let markings = markings();
        assert_eq!(step(&markings, None, below, true, 1), Some(2));
        assert_eq!(step(&markings, None, below, false, 1), Some(1));
        assert_eq!(step(&markings, None, below, true, 2), Some(3));
        assert_eq!(step(&markings, None, below, false, 2), Some(0));
        assert_eq!(step(&markings, None, below, true, 3), None);
        assert_eq!(step(&markings, None, below, false, 3), None);
    }

    #[test]
    fn stepping_from_a_marking_goes_on_from_it() {
        let markings = markings();
        assert_eq!(step(&markings, Some(1), below, true, 1), Some(2));
        assert_eq!(step(&markings, Some(1), below, false, 1), Some(0));
        assert_eq!(step(&markings, Some(0), below, false, 1), None);
        assert_eq!(step(&markings, Some(3), below, true, 1), None);
        assert_eq!(step(&markings, Some(0), below, true, 3), Some(3));
    }

    #[test]
    fn nothing_to_step_to_without_markings() {
        assert_eq!(step(&[], None, |_| true, true, 1), None);
        assert_eq!(step(&[], None, |_| false, false, 1), None);
    }

    #[test]
    fn the_marking_at_a_character_is_over_it_or_a_note_beside_its_line() {
        let mut markings = markings();
        let mut note = marking(3, 395.0);
        note.kind = MarkupKind::Note;
        note.areas = vec![Rect {
            left: 12.0,
            top: 395.0,
            right: 32.0,
            bottom: 415.0,
        }];
        markings.push(note);
        let character = |left: f32, top: f32| Rect {
            left,
            top,
            right: left + 6.0,
            bottom: top + 10.0,
        };
        // Over the highlight on page 3 at 400.
        assert_eq!(at(&markings, 3, character(50.0, 402.0)), Some(2));
        // Past its end, on the same line: the note beside the line.
        assert_eq!(at(&markings, 3, character(300.0, 402.0)), Some(4));
        // A line with neither.
        assert_eq!(at(&markings, 3, character(50.0, 250.0)), None);
        assert_eq!(at(&markings, 5, character(50.0, 402.0)), None);
    }

    #[test]
    fn markings_under_a_selection_are_found() {
        let markings = markings();
        let on = |page, top| {
            (
                page,
                Rect {
                    left: 50.0,
                    top,
                    right: 60.0,
                    bottom: top + 5.0,
                },
            )
        };
        assert_eq!(touching(&markings, &[on(3, 403.0)]), [2]);
        assert_eq!(touching(&markings, &[on(3, 300.0)]), [0_usize; 0]);
        assert_eq!(touching(&markings, &[on(1, 103.0), on(6, 22.0)]), [0, 3]);
        // The same place on another page is not under it.
        assert_eq!(touching(&markings, &[on(2, 103.0)]), [0_usize; 0]);
    }

    fn keymap() -> Keymap<Binding> {
        crate::bindings::Keymaps::default().markings
    }

    #[test]
    fn the_list_starts_at_the_page_in_view() {
        assert_eq!(List::new(markings(), 2).unwrap().selected(), 1);
        assert_eq!(List::new(markings(), 9).unwrap().selected(), 3);
        assert_eq!(List::new(Vec::new(), 0), None);
    }

    #[test]
    fn the_list_moves_goes_and_deletes() {
        let mut list = List::new(markings(), 0).unwrap();
        assert_eq!(list.feed(Key::Char('j'), &keymap(), 10), Outcome::Stay);
        assert_eq!(list.selected(), 1);
        list.feed(Key::Char('G'), &keymap(), 10);
        assert_eq!(list.selected(), 3);
        list.feed(Key::Char('j'), &keymap(), 10);
        assert_eq!(list.selected(), 3);
        list.feed(Key::Char('2'), &keymap(), 10);
        list.feed(Key::Char('k'), &keymap(), 10);
        assert_eq!(list.feed(Key::Enter, &keymap(), 10), Outcome::Go(1));
        assert_eq!(list.feed(Key::Char('d'), &keymap(), 10), Outcome::Delete(1));
        assert_eq!(list.feed(Key::Char('c'), &keymap(), 10), Outcome::Change(1));
        assert_eq!(list.feed(Key::Escape, &keymap(), 10), Outcome::Close);
    }

    #[test]
    fn a_refreshed_list_keeps_its_place() {
        let mut list = List::new(markings(), 0).unwrap();
        list.feed(Key::Char('G'), &keymap(), 10);
        let mut fewer = markings();
        fewer.pop();
        let list = list.refreshed(fewer).unwrap();
        assert_eq!(list.selected(), 2);
        assert_eq!(list.refreshed(Vec::new()), None);
    }

    #[test]
    fn the_list_shows_the_rows_around_the_selection() {
        let mut list = List::new(markings(), 0).unwrap();
        assert_eq!(list.shown(3).0, 0);
        assert_eq!(list.shown(3).1.len(), 3);
        list.feed(Key::Char('G'), &keymap(), 3);
        assert_eq!(list.shown(3).0, 3);
        assert_eq!(list.shown(3).1.len(), 1);
    }
}
