//! Scrolling without building the window again. The pages, and what is
//! drawn over them, are laid out from where the view was (the origin),
//! with a page more on either side of those in view, and slid by a live
//! offset to where the view is now: a key that only moves the view changes
//! the offset and nothing else, and Rune neither runs the view again nor
//! lays it out. Anything else, or a move that brings in a page not laid
//! out, or one that changes what the bars say, lays the pages out again
//! from where the view is then.
//!
//! What is drawn depends on the origin alone, never on the offset, so
//! that the view built again at any time builds the same window: Rune's
//! debug builds check that it does after each move that only slid.

use std::ops::Range;

use super::{App, Document, Message, RENDER_AHEAD};
use crate::num;
use rune::Live;

use crate::reader::Reader;

/// Where the page layers were laid out from, and how far they are slid
/// from there.
pub(super) struct Sliding {
    /// How far the view has moved since the page layers were laid out, in
    /// logical pixels.
    pub(super) offset: Live<[f32; 2]>,
    origin: Option<Origin>,
}

impl Default for Sliding {
    fn default() -> Self {
        Self {
            offset: Live::new([0.0, 0.0]),
            origin: None,
        }
    }
}

/// Where the page layers were last laid out from.
#[derive(Debug, Clone, PartialEq)]
struct Origin {
    /// The document it was laid out for.
    document: u64,
    /// The window's top-left corner in the document then, in points.
    corner: (f32, f32),
    zoom: f32,
    /// The pages laid out.
    pages: Range<u32>,
}

/// What the window shows apart from where the pages are: while this stays
/// the same, a move of the view needs only the offset changed.
#[derive(Debug, PartialEq)]
pub(super) struct Still {
    tab: usize,
    tabs: usize,
    document: Option<u64>,
    title: String,
    /// The page and zoom, as the status bar gives them.
    position: String,
    /// What has been typed of a command.
    typed: String,
    note: Option<String>,
    area: (f32, f32),
}

impl App {
    /// Whether a message may move the view and nothing more.
    pub(super) const fn may_slide(message: &Message) -> bool {
        matches!(message, Message::Keyboard(_) | Message::Wheel(_))
    }

    /// What the window shows apart from where the pages are, when nothing
    /// is shown over the pages that could move or change with them; none
    /// otherwise, and then the window is built again.
    pub(super) fn still(&self) -> Option<Still> {
        let plain = self.prompt.is_none()
            && self.completing.is_none()
            && self.picker.is_none()
            && self.tab_list.is_none()
            && self.match_list.is_none()
            && self.help.is_none()
            && self.overview.is_none()
            && self.info.is_none()
            && self.sidebar.is_none()
            && self.hints.is_none()
            && self.marking_hints.is_none()
            && self.line_hints.is_none()
            && self.flash.is_none()
            && self.visual.is_none()
            && self.asking.is_none()
            && self.field_edit.is_none()
            && self.awaiting_mark.is_none()
            && self.preview.is_none()
            && matches!(self.notice, super::Notice::None)
            && !self.mouse.panning();
        if !plain {
            return None;
        }
        let document = self.document();
        Some(Still {
            tab: self.current,
            tabs: self.tabs.len(),
            document: document.map(|document| document.id),
            title: self.title(),
            position: document.map_or_else(String::new, |document| {
                format!(
                    "{} {}",
                    document.page_position(),
                    num::round(document.reader.zoom() * 100.0)
                )
            }),
            typed: self.pending.display(),
            note: self.picked_note().map(str::to_owned),
            area: (self.area.width, self.area.height),
        })
    }

    /// Slides the page layer to where the view is now, if it was laid out
    /// for this document and zoom and holds every page in view: whether it
    /// could.
    pub(super) fn slide(&self) -> bool {
        let (Some(origin), Some(document)) = (&self.sliding.origin, self.document()) else {
            return false;
        };
        let reader = &document.reader;
        let visible = reader.visible_pages();
        let zoom = reader.zoom();
        if origin.document != document.id
            || (origin.zoom - zoom).abs() > f32::EPSILON
            || visible.start < origin.pages.start
            || visible.end > origin.pages.end
        {
            return false;
        }
        let (x, y) = reader.corner();
        self.sliding
            .offset
            .set([(origin.corner.0 - x) * zoom, (origin.corner.1 - y) * zoom]);
        true
    }

    /// Lays the page layer out again from where the view is now.
    pub(super) fn settle(&mut self) {
        self.sliding.origin = self.document().map(|document| Origin {
            document: document.id,
            corner: document.reader.corner(),
            zoom: document.reader.zoom(),
            pages: laid_out(document),
        });
        self.sliding.offset.set([0.0, 0.0]);
    }

    /// Has the reader give screen positions from the origin while the
    /// page layers are drawn, until the guard returned is dropped.
    pub(super) fn drawing_from_origin<'a>(&self, document: &'a Document) -> DrawingFrom<'a> {
        let corner = self
            .sliding
            .origin
            .as_ref()
            .filter(|origin| {
                origin.document == document.id
                    && (origin.zoom - document.reader.zoom()).abs() <= f32::EPSILON
            })
            .map(|origin| origin.corner);
        document.reader.draw_from(corner);
        DrawingFrom(&document.reader)
    }

    /// The pages the page layer lays out.
    pub(super) fn drawn_pages(&self, document: &Document) -> Range<u32> {
        self.sliding
            .origin
            .as_ref()
            .filter(|origin| origin.document == document.id)
            .map_or_else(|| laid_out(document), |origin| origin.pages.clone())
    }
}

/// Keeps the reader giving screen positions from the origin while it
/// lives.
pub(super) struct DrawingFrom<'a>(&'a Reader);

impl Drop for DrawingFrom<'_> {
    fn drop(&mut self) {
        self.0.draw_from(None);
    }
}

/// The pages in view, and one more on either side, as many as are
/// rendered ahead.
fn laid_out(document: &Document) -> Range<u32> {
    let visible = document.reader.visible_pages();
    let last = document.reader.page_count();
    visible.start.saturating_sub(RENDER_AHEAD)..visible.end.saturating_add(RENDER_AHEAD).min(last)
}

#[cfg(test)]
mod tests {
    use rune::Tx;

    use super::super::testing::showing;
    use super::super::{App, Message};
    use crate::input::Key;
    use crate::layout::Rect;

    /// Where the first page is on screen now.
    fn first_page(app: &App) -> Rect {
        app.document().unwrap().reader.page_on_screen(0).unwrap()
    }

    fn press(app: &mut App, key: Key) {
        app.dispatch(Message::Keyboard(key), &mut Tx::default());
    }

    #[test]
    fn scrolling_slides_the_pages_laid_out_before_to_where_they_are_now() {
        let (mut app, _) = showing("outline.pdf");
        press(&mut app, Key::Char('j'));
        // Where the page was laid out: the view draws it there again, the
        // same to the bit, until it is laid out again.
        let mut laid_out = first_page(&app);
        let mut slid = 0;
        for _ in 0..12 {
            press(&mut app, Key::Char('j'));
            let [x, y] = app.sliding.offset.get();
            let now = first_page(&app);
            let drawn = {
                let document = app.document().unwrap();
                let _from = app.drawing_from_origin(document);
                first_page(&app)
            };
            if x == 0.0 && y == 0.0 {
                assert_eq!(drawn, now, "laid out again where the view is");
                laid_out = now;
                continue;
            }
            slid += 1;
            assert_eq!(drawn, laid_out, "drawn where it was laid out");
            assert!(
                (laid_out.left + x - now.left).abs() < 0.01,
                "{laid_out:?} {x} {now:?}"
            );
            assert!(
                (laid_out.top + y - now.top).abs() < 0.01,
                "{laid_out:?} {y} {now:?}"
            );
        }
        assert!(slid > 0, "some presses of j only slid the pages");
    }

    #[test]
    fn anything_shown_over_the_pages_builds_the_window_again() {
        let (mut app, _) = showing("outline.pdf");
        press(&mut app, Key::Char('j'));
        press(&mut app, Key::Char('j'));
        press(&mut app, Key::Char(':'));
        assert_eq!(app.sliding.offset.get(), [0.0, 0.0], "the prompt opening");
        press(&mut app, Key::Escape);
        press(&mut app, Key::Char('j'));
        press(&mut app, Key::Char('2'));
        assert_eq!(app.sliding.offset.get(), [0.0, 0.0], "a count typed");
    }

    #[test]
    fn going_to_another_page_builds_the_window_again() {
        let (mut app, _) = showing("outline.pdf");
        press(&mut app, Key::Char('j'));
        press(&mut app, Key::Char('G'));
        assert_eq!(app.sliding.offset.get(), [0.0, 0.0]);
        let origin = app.sliding.origin.clone().unwrap();
        let reader = &app.document().unwrap().reader;
        assert_eq!(origin.corner, reader.corner());
        assert!(origin.pages.contains(&(reader.page_count() - 1)));
    }
}
