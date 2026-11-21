//! Books without pages of their own, as EPUB and FB2, laid out on pages the
//! size of the window: again when the window changes size or the type does,
//! keeping the place read.

use std::path::Path;

use iced::Task;

use super::{App, Message};
use crate::backend::{self, Flow};
use crate::history::{self, View};
use crate::reader::Reader;

impl App {
    /// The page books are laid out on: the window, in the type set.
    pub(super) fn flow(&self) -> Flow {
        Flow::new(self.area.width, self.area.height, self.font_size)
    }

    /// Lays out again, on the page the window now makes, every book laid
    /// out on another, by opening it again in its tab. The book stays in
    /// view until the new layout is ready, and then shows the same place.
    pub(super) fn reflow(&mut self) -> Task<Message> {
        let flow = self.flow();
        let mut tasks = Vec::new();
        for index in 0..self.tabs.len() {
            let Some(tab) = self.tabs.get(index) else {
                continue;
            };
            let Some(document) = &tab.document else {
                continue;
            };
            // Each layout waits for the one before it: a window dragged to
            // a new size is laid out for once the last is done.
            if tab.opening.is_some() || document.flow.is_none_or(|laid| laid == flow) {
                continue;
            }
            let path = document.path.clone();
            let request = self.next_id();
            if let Some(tab) = self.tabs.get_mut(index) {
                tab.opening = Some((request, path.clone()));
            }
            tasks.push(Task::perform(
                backend::open(path.clone(), None, flow),
                move |result| Message::Opened(request, path, result),
            ));
        }
        Task::batch(tasks)
    }

    /// The view a document opening in a tab starts in: for a book laid out
    /// again, the view of the layout it replaces; for any other, the one it
    /// was left in, from the history.
    pub(super) fn starting_view(
        &self,
        index: usize,
        path: &Path,
    ) -> Option<Result<Option<View>, history::Error>> {
        let left = self
            .tabs
            .get(index)
            .and_then(|tab| tab.document.as_ref())
            .filter(|document| document.path == path && document.flow.is_some());
        if let Some(left) = left {
            let view = left
                .returning
                .unwrap_or_else(|| left.reader.view(left.recolor));
            return Some(Ok(Some(view)));
        }
        self.history.as_ref().map(|history| history.load(path))
    }
}

/// A view moved to the same place in a book laid out anew, which may have
/// come out taller or shorter: as far down, as a share of the whole.
pub(super) fn refit(view: View, reader: &Reader, laid_out: bool) -> View {
    let extent = reader.extent();
    if !laid_out || view.extent.is_nan() || view.extent <= 0.0 || !extent.is_finite() {
        return view;
    }
    let y = view.y / view.extent * extent;
    View {
        y: if y.is_finite() { y } else { view.y },
        extent,
        ..view
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::Fit;
    use crate::layout::{PageSize, Rotation};

    fn view(y: f32, extent: f32) -> View {
        View {
            x: 0.0,
            y,
            zoom: 1.0,
            fit: Fit::Width,
            rotation: Rotation::None,
            recolor: false,
            trim: false,
            extent,
        }
    }

    #[test]
    fn a_book_laid_out_anew_keeps_its_place() {
        let page = PageSize {
            width: 400.0,
            height: 600.0,
        };
        let reader = Reader::new(vec![page; 10], (400.0, 600.0));
        let extent = reader.extent();
        // Halfway through a book half as tall as it is now.
        let moved = refit(view(extent / 4.0, extent / 2.0), &reader, true);
        assert!((moved.y - extent / 2.0).abs() < 0.01);
        assert!((moved.extent - extent).abs() < 0.01);
        // A view from before the height was kept, or of a document with
        // pages of its own, stays as it was.
        assert!((refit(view(300.0, 0.0), &reader, true).y - 300.0).abs() < 0.01);
        assert!((refit(view(300.0, 100.0), &reader, false).y - 300.0).abs() < 0.01);
    }
}
