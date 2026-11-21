//! The sidebar listing a document's table of contents: a tree of chapters
//! and their sections, folded and unfolded from the keyboard.

use crate::backend::Chapter;
use crate::input::Key;
use crate::keymap::{Counts, Fed, Keymap, Pending, bindable};

bindable! {
    /// What keys can be bound to in the table of contents.
    pub enum Binding {
        Down = "down", "Select the next entry";
        Up = "up", "Select the previous entry";
        HalfDown = "half-down", "Select the entry half a screen down";
        HalfUp = "half-up", "Select the entry half a screen up";
        Top = "top", "Select the first entry";
        Bottom = "bottom", "Select the last entry";
        /// An entry that leads nowhere opens or closes instead.
        Go = "go", "Go to the entry selected";
        /// Already open: step into the first section.
        Open = "open", "Show the sections under the entry, or go into them";
        Toggle = "toggle", "Show or hide the sections under the entry";
        /// Already closed: go out to the entry above, and close that.
        Fold = "fold", "Hide the sections under the entry, or go out of them";
        Close = "close", "Close the table of contents";
    }
}

/// An entry of the table of contents, laid out flat in reading order.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub title: String,
    /// The page the entry leads to, counted from 0, if any.
    pub page: Option<u32>,
    /// How far down that page, in points, when the entry says.
    pub top: Option<f32>,
    /// How deep in the tree: 0 for a chapter, 1 for its sections, and so on.
    pub depth: usize,
    /// The entry this one is under, if any.
    parent: Option<usize>,
    /// Whether any entries are under this one.
    pub has_sections: bool,
}

/// What a key typed in the sidebar comes to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Outcome {
    Stay,
    /// Go to this page, and this far down it if given, and close.
    Go(u32, Option<f32>),
    Close,
}

/// A line of the sidebar as shown.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shown<'a> {
    pub entry: &'a Entry,
    pub unfolded: bool,
    pub selected: bool,
}

/// The sidebar: the entries, which are unfolded, which is selected, and how
/// far down the list is scrolled.
#[derive(Debug, Clone, PartialEq)]
pub struct Outline {
    entries: Vec<Entry>,
    unfolded: Vec<bool>,
    selected: usize,
    /// The first of the visible entries shown.
    top: usize,
    pending: Pending<Binding>,
}

impl Outline {
    /// The sidebar for a table of contents, with the entry for the page in
    /// view selected and the entries above it unfolded. None for an empty
    /// table of contents.
    pub fn new(chapters: &[Chapter], page: u32) -> Option<Self> {
        let mut entries = Vec::new();
        flatten(chapters, 0, None, &mut entries);
        if entries.is_empty() {
            return None;
        }
        // The last entry that starts on or before the page in view.
        let selected = entries
            .iter()
            .rposition(|entry| entry.page.is_some_and(|start| start <= page))
            .unwrap_or(0);
        let mut outline = Self {
            unfolded: vec![false; entries.len()],
            entries,
            selected,
            top: 0,
            pending: Pending::new(),
        };
        let mut above = outline.parent(selected);
        while let Some(index) = above {
            outline.set_unfolded(index, true);
            above = outline.parent(index);
        }
        Some(outline)
    }

    fn parent(&self, index: usize) -> Option<usize> {
        self.entries.get(index)?.parent
    }

    fn set_unfolded(&mut self, index: usize, unfolded: bool) {
        if let Some(flag) = self.unfolded.get_mut(index) {
            *flag = unfolded;
        }
    }

    fn is_unfolded(&self, index: usize) -> bool {
        self.unfolded.get(index).copied().unwrap_or(false)
    }

    /// The entries showing: those with nothing folded above them.
    fn visible(&self) -> Vec<usize> {
        (0..self.entries.len())
            .filter(|&index| {
                let mut above = self.parent(index);
                while let Some(parent) = above {
                    if !self.is_unfolded(parent) {
                        return false;
                    }
                    above = self.parent(parent);
                }
                true
            })
            .collect()
    }

    /// The entries that fit in this many rows, scrolled so the selected one
    /// shows.
    pub fn shown(&self, rows: usize) -> Vec<Shown<'_>> {
        let visible = self.visible();
        let top = self.first(&visible, rows);
        visible
            .into_iter()
            .skip(top)
            .take(rows)
            .filter_map(|index| {
                Some(Shown {
                    entry: self.entries.get(index)?,
                    unfolded: self.is_unfolded(index),
                    selected: index == self.selected,
                })
            })
            .collect()
    }

    /// Takes a key, bound as in the keymap, given how many rows are shown.
    pub fn feed(&mut self, key: Key, keymap: &Keymap<Binding>, rows: usize) -> Outcome {
        match self.pending.feed(key, keymap, Counts::Taken) {
            Fed::Bound(binding, count) => self.act(binding, count.unwrap_or(1), rows),
            Fed::Waiting | Fed::Unbound(_) => Outcome::Stay,
        }
    }

    /// Does what a binding says, moving a number of times.
    fn act(&mut self, binding: Binding, times: u32, rows: usize) -> Outcome {
        let rows = rows.max(1);
        let times = usize::try_from(times).unwrap_or(usize::MAX);
        let visible = self.visible();
        let here = visible
            .iter()
            .position(|&index| index == self.selected)
            .unwrap_or(0);
        let last = visible.len().saturating_sub(1);
        let half = rows.div_ceil(2);
        let to = match binding {
            Binding::Close => return Outcome::Close,
            Binding::Go => {
                let Some(entry) = self.entries.get(self.selected) else {
                    return Outcome::Close;
                };
                if let Some(page) = entry.page {
                    return Outcome::Go(page, entry.top);
                }
                // An entry that leads nowhere opens or closes instead.
                let unfolded = self.is_unfolded(self.selected);
                self.set_unfolded(self.selected, !unfolded);
                here
            }
            Binding::Down => here.saturating_add(times),
            Binding::Up => here.saturating_sub(times),
            Binding::HalfDown => here.saturating_add(half),
            Binding::HalfUp => here.saturating_sub(half),
            Binding::Top => 0,
            Binding::Bottom => last,
            Binding::Open | Binding::Toggle => {
                let has_sections = self
                    .entries
                    .get(self.selected)
                    .is_some_and(|entry| entry.has_sections);
                if has_sections && !self.is_unfolded(self.selected) {
                    self.set_unfolded(self.selected, true);
                    here
                } else if has_sections && binding == Binding::Open {
                    // Already open: step into the first section.
                    here.saturating_add(1)
                } else {
                    self.set_unfolded(self.selected, false);
                    here
                }
            }
            Binding::Fold => {
                if self.is_unfolded(self.selected) {
                    self.set_unfolded(self.selected, false);
                } else if let Some(parent) = self.parent(self.selected) {
                    self.selected = parent;
                    self.set_unfolded(parent, false);
                }
                return self.scrolled(rows);
            }
        };
        // Folding may have changed what shows.
        let visible = self.visible();
        if let Some(&index) = visible.get(to.min(visible.len().saturating_sub(1))) {
            self.selected = index;
        }
        self.scrolled(rows)
    }

    /// Scrolls so that the selected entry shows.
    fn scrolled(&mut self, rows: usize) -> Outcome {
        self.top = self.first(&self.visible(), rows);
        Outcome::Stay
    }

    /// The first of the visible entries to show in this many rows: where
    /// the list was scrolled to, moved as little as keeps the selected
    /// entry in view. The list is not scrolled to it until a key is typed,
    /// and the window may have changed size since.
    fn first(&self, visible: &[usize], rows: usize) -> usize {
        let rows = rows.max(1);
        let here = visible
            .iter()
            .position(|&index| index == self.selected)
            .unwrap_or(0);
        if here < self.top {
            here
        } else if here >= self.top.saturating_add(rows) {
            here.saturating_add(1).saturating_sub(rows)
        } else {
            self.top
        }
    }

    /// The first entry shown in this many rows, and how many show when
    /// everything open is counted.
    pub fn position(&self, rows: usize) -> (usize, usize) {
        let visible = self.visible();
        (self.first(&visible, rows), visible.len())
    }
}

/// Lays chapters out flat in reading order, each after the one it is under.
fn flatten(chapters: &[Chapter], depth: usize, parent: Option<usize>, entries: &mut Vec<Entry>) {
    for chapter in chapters {
        let index = entries.len();
        entries.push(Entry {
            title: chapter.title.clone(),
            page: chapter.page,
            top: chapter.top,
            depth,
            parent,
            has_sections: !chapter.sections.is_empty(),
        });
        flatten(
            &chapter.sections,
            depth.saturating_add(1),
            Some(index),
            entries,
        );
    }
}

/// Where a chapter or section starts, for going from one to the next.
#[derive(Debug, Clone, PartialEq)]
pub struct Start {
    pub title: String,
    /// The page, counted from 0.
    pub page: u32,
    /// How far down that page, in points, when the entry says.
    pub top: Option<f32>,
}

/// Where every chapter and section that leads somewhere starts, in reading
/// order.
pub fn starts(chapters: &[Chapter]) -> Vec<Start> {
    let mut entries = Vec::new();
    flatten(chapters, 0, None, &mut entries);
    let mut starts: Vec<Start> = entries
        .into_iter()
        .filter_map(|entry| {
            Some(Start {
                page: entry.page?,
                top: entry.top,
                title: entry.title,
            })
        })
        .collect();
    starts.sort_by(|one, other| {
        (one.page, one.top.unwrap_or(0.0))
            .partial_cmp(&(other.page, other.top.unwrap_or(0.0)))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    starts
}

/// How far, in pixels, a start must be from the top of the window to count
/// as after it or before it, rather than where the window is.
const HERE: f32 = 2.0;

/// The start a number of chapters on from the top of the window, or back,
/// given how far below the top of the window each start is, in reading
/// order; starts above it are less than 0. Starts at the same place count
/// once, as the first of them. None past either end.
pub fn step(offsets: &[f32], forward: bool, times: u32) -> Option<usize> {
    let wanted = |offset: f32| {
        if forward {
            offset > HERE
        } else {
            offset < -HERE
        }
    };
    let mut candidates: Vec<(usize, f32)> = Vec::new();
    for (index, &offset) in offsets.iter().enumerate() {
        if !wanted(offset) {
            continue;
        }
        if candidates
            .last()
            .is_some_and(|&(_, last)| (offset - last).abs() <= HERE)
        {
            continue;
        }
        candidates.push((index, offset));
    }
    let times = usize::try_from(times.max(1)).unwrap_or(usize::MAX);
    let picked = if forward {
        candidates.get(times.saturating_sub(1))
    } else {
        candidates
            .len()
            .checked_sub(times)
            .and_then(|at| candidates.get(at))
    };
    picked.map(|&(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chapters_start_in_reading_order() {
        let chapters = vec![
            Chapter {
                title: "Two".into(),
                page: Some(5),
                top: Some(100.0),
                sections: vec![Chapter {
                    title: "Two, first".into(),
                    page: Some(5),
                    top: Some(400.0),
                    sections: Vec::new(),
                }],
            },
            Chapter {
                title: "Nowhere".into(),
                page: None,
                top: None,
                sections: Vec::new(),
            },
            Chapter {
                title: "One".into(),
                page: Some(1),
                top: None,
                sections: Vec::new(),
            },
        ];
        let titles: Vec<_> = starts(&chapters)
            .into_iter()
            .map(|start| start.title)
            .collect();
        assert_eq!(titles, ["One", "Two", "Two, first"]);
    }

    #[test]
    fn steps_go_from_the_top_of_the_window() {
        // Two chapters above the window, one at its top, and two below.
        let offsets = [-900.0, -300.0, 0.5, 200.0, 1500.0];
        assert_eq!(step(&offsets, true, 1), Some(3));
        assert_eq!(step(&offsets, true, 2), Some(4));
        assert_eq!(step(&offsets, true, 3), None);
        assert_eq!(step(&offsets, false, 1), Some(1));
        assert_eq!(step(&offsets, false, 2), Some(0));
        assert_eq!(step(&offsets, false, 3), None);
        assert_eq!(step(&[], true, 1), None);
    }

    #[test]
    fn starts_at_the_same_place_count_once() {
        // A chapter and its first section, on the same spot.
        let offsets = [100.0, 100.0, 500.0];
        assert_eq!(step(&offsets, true, 1), Some(0));
        assert_eq!(step(&offsets, true, 2), Some(2));
        let offsets = [-500.0, -100.0, -100.0];
        assert_eq!(step(&offsets, false, 1), Some(1));
        assert_eq!(step(&offsets, false, 2), Some(0));
    }

    fn keymap() -> Keymap<Binding> {
        crate::bindings::Keymaps::default().outline
    }

    fn chapter(title: &str, page: u32, sections: Vec<Chapter>) -> Chapter {
        Chapter {
            title: title.to_owned(),
            page: Some(page),
            top: None,
            sections,
        }
    }

    /// One: 1.1, 1.2 (1.2.1); Two; Three: 3.1.
    fn book() -> Vec<Chapter> {
        vec![
            chapter(
                "One",
                0,
                vec![
                    chapter("1.1", 1, Vec::new()),
                    chapter("1.2", 3, vec![chapter("1.2.1", 4, Vec::new())]),
                ],
            ),
            chapter("Two", 10, Vec::new()),
            chapter("Three", 20, vec![chapter("3.1", 22, Vec::new())]),
        ]
    }

    fn titles(outline: &Outline) -> Vec<&str> {
        outline
            .shown(100)
            .iter()
            .map(|shown| shown.entry.title.as_str())
            .collect()
    }

    fn selected(outline: &Outline) -> &str {
        outline
            .shown(100)
            .into_iter()
            .find(|shown| shown.selected)
            .map_or("", |shown| shown.entry.title.as_str())
    }

    fn keys(outline: &mut Outline, keys: &str) -> Outcome {
        let mut outcome = Outcome::Stay;
        for key in keys.chars() {
            outcome = outline.feed(Key::Char(key), &keymap(), 100);
        }
        outcome
    }

    #[test]
    fn nothing_listed_no_sidebar() {
        assert_eq!(Outline::new(&[], 0), None);
    }

    #[test]
    fn it_opens_on_the_chapter_in_view() {
        let outline = Outline::new(&book(), 5).unwrap();
        assert_eq!(selected(&outline), "1.2.1");
        assert_eq!(
            titles(&outline),
            ["One", "1.1", "1.2", "1.2.1", "Two", "Three"]
        );
        let outline = Outline::new(&book(), 12).unwrap();
        assert_eq!(selected(&outline), "Two");
        assert_eq!(titles(&outline), ["One", "Two", "Three"]);
    }

    #[test]
    fn folding_and_unfolding() {
        let mut outline = Outline::new(&book(), 0).unwrap();
        assert_eq!(titles(&outline), ["One", "Two", "Three"]);
        keys(&mut outline, "l");
        assert_eq!(titles(&outline), ["One", "1.1", "1.2", "Two", "Three"]);
        // Open already: l steps in.
        keys(&mut outline, "l");
        assert_eq!(selected(&outline), "1.1");
        // h on a closed entry goes up to the one it is under, and closes it.
        keys(&mut outline, "h");
        assert_eq!(selected(&outline), "One");
        assert_eq!(titles(&outline), ["One", "Two", "Three"]);
        // Space opens and closes.
        keys(&mut outline, "G ");
        assert_eq!(titles(&outline), ["One", "Two", "Three", "3.1"]);
        keys(&mut outline, " ");
        assert_eq!(titles(&outline), ["One", "Two", "Three"]);
    }

    #[test]
    fn moving_and_going() {
        let mut outline = Outline::new(&book(), 0).unwrap();
        keys(&mut outline, "jj");
        assert_eq!(selected(&outline), "Three");
        keys(&mut outline, "j");
        assert_eq!(selected(&outline), "Three");
        keys(&mut outline, "gg");
        assert_eq!(selected(&outline), "One");
        keys(&mut outline, "j");
        assert_eq!(
            outline.feed(Key::Enter, &keymap(), 100),
            Outcome::Go(10, None)
        );
        assert_eq!(outline.feed(Key::Escape, &keymap(), 100), Outcome::Close);
        assert_eq!(outline.feed(Key::Tab, &keymap(), 100), Outcome::Close);
    }

    /// Found by fuzzing: the entry for the page in view was selected but
    /// scrolled out of sight until a key was typed.
    #[test]
    fn it_opens_scrolled_to_the_chapter_in_view() {
        let outline = Outline::new(&book(), 21).unwrap();
        let shown: Vec<_> = outline
            .shown(2)
            .iter()
            .map(|shown| (shown.entry.title.as_str(), shown.selected))
            .collect();
        assert_eq!(shown, [("Two", false), ("Three", true)]);
        assert_eq!(outline.position(2), (1, 3));
    }

    #[test]
    fn the_list_scrolls_to_keep_the_selection_in_view() {
        let mut outline = Outline::new(&book(), 5).unwrap();
        // Six showing, two rows: 1.2.1 is fourth, so the list starts at 1.2.
        outline.feed(Key::Char('k'), &keymap(), 2);
        outline.feed(Key::Char('j'), &keymap(), 2);
        assert_eq!(outline.position(2), (2, 6));
        let shown: Vec<_> = outline
            .shown(2)
            .iter()
            .map(|shown| shown.entry.title.as_str())
            .collect();
        assert_eq!(shown, ["1.2", "1.2.1"]);
        outline.feed(Key::Char('g'), &keymap(), 2);
        outline.feed(Key::Char('g'), &keymap(), 2);
        assert_eq!(outline.position(2), (0, 6));
    }
}
