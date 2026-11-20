//! The jump list: the places a document was left by a big move, such as
//! `gg`, `G`, a search, a chapter or a link, so that `ctrl+o` can go back
//! to them and `ctrl+i` forward again, as in vim.

use crate::reader::Place;

/// How many places the list keeps.
const KEPT: usize = 100;

/// The places jumped from, oldest first, and where in them going back and
/// forward has got to.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Jumps {
    places: Vec<Place>,
    /// The place gone back or forward to, or the length when at the newest,
    /// which is not in the list until going back leaves it.
    at: usize,
    /// Where the last jump left from, for `` ` ` `` to go back to.
    before: Option<Place>,
}

impl Jumps {
    /// Notes a place about to be jumped from. A place already in the list
    /// moves to the end, so that each shows once.
    pub fn record(&mut self, place: Place) {
        self.add(place);
        self.at = self.places.len();
        self.before = Some(place);
    }

    /// The place before the one gone back or forward to, a number of times
    /// over, given where the document is now. None at the oldest.
    pub fn back(&mut self, here: Place, times: u32) -> Option<Place> {
        if self.at >= self.places.len() {
            // Leaving the newest place: keep it, to come forward to.
            self.add(here);
            self.at = self.places.len().saturating_sub(1);
        }
        let times = usize::try_from(times).unwrap_or(usize::MAX);
        let to = self.at.checked_sub(times.max(1))?;
        self.at = to;
        self.places.get(to).copied()
    }

    /// The place after the one gone back to, a number of times over. None
    /// at the newest.
    pub fn forward(&mut self, times: u32) -> Option<Place> {
        let times = usize::try_from(times).unwrap_or(usize::MAX);
        let to = self.at.saturating_add(times.max(1));
        let place = self.places.get(to).copied()?;
        self.at = to;
        Some(place)
    }

    /// Where the last jump left from, if there was one.
    pub const fn before(&self) -> Option<Place> {
        self.before
    }

    /// Puts a place at the end, taking it out from wherever else it is,
    /// and drops the oldest past the most kept.
    fn add(&mut self, place: Place) {
        self.places.retain(|kept| !kept.near(place));
        self.places.push(place);
        if self.places.len() > KEPT {
            self.places.remove(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(page: u32) -> Place {
        Place { page, down: 0.0 }
    }

    #[test]
    fn back_and_forward_go_through_the_places_jumped_from() {
        let mut jumps = Jumps::default();
        // From page 1 to 5, from 5 to 9.
        jumps.record(page(1));
        jumps.record(page(5));
        assert_eq!(jumps.back(page(9), 1), Some(page(5)));
        assert_eq!(jumps.back(page(5), 1), Some(page(1)));
        assert_eq!(jumps.back(page(1), 1), None);
        assert_eq!(jumps.forward(1), Some(page(5)));
        assert_eq!(jumps.forward(1), Some(page(9)));
        assert_eq!(jumps.forward(1), None);
    }

    #[test]
    fn a_count_goes_that_many_places_at_once() {
        let mut jumps = Jumps::default();
        for number in 1..=4 {
            jumps.record(page(number));
        }
        assert_eq!(jumps.back(page(7), 3), Some(page(2)));
        assert_eq!(jumps.forward(2), Some(page(4)));
        assert_eq!(jumps.back(page(4), 99), None);
    }

    #[test]
    fn a_place_shows_once_however_often_it_is_left() {
        let mut jumps = Jumps::default();
        jumps.record(page(1));
        jumps.record(page(2));
        jumps.record(page(1));
        assert_eq!(jumps.back(page(3), 1), Some(page(1)));
        assert_eq!(jumps.back(page(1), 1), Some(page(2)));
        assert_eq!(jumps.back(page(2), 1), None);
    }

    #[test]
    fn a_jump_after_going_back_keeps_the_list() {
        let mut jumps = Jumps::default();
        jumps.record(page(1));
        jumps.record(page(2));
        assert_eq!(jumps.back(page(3), 1), Some(page(2)));
        // From page 2, a new jump.
        jumps.record(page(2));
        assert_eq!(jumps.back(page(8), 1), Some(page(2)));
        assert_eq!(jumps.back(page(2), 1), Some(page(3)));
        assert_eq!(jumps.back(page(3), 1), Some(page(1)));
    }

    #[test]
    fn the_place_before_the_last_jump_is_kept() {
        let mut jumps = Jumps::default();
        assert_eq!(jumps.before(), None);
        jumps.record(page(4));
        assert_eq!(jumps.before(), Some(page(4)));
    }

    #[test]
    fn the_list_keeps_the_newest_hundred() {
        let mut jumps = Jumps::default();
        for number in 0..150 {
            jumps.record(page(number));
        }
        // Going back keeps where it left, which pushes out one more.
        assert_eq!(jumps.back(page(500), 99), Some(page(51)));
        assert_eq!(jumps.back(page(51), 1), None);
    }
}
