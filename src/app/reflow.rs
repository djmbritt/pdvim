//! Books without pages of their own, as EPUB and FB2, laid out on pages the
//! size of the window, or of a column of it: again when the window, the
//! columns or the type change, keeping the place read. A book always fits
//! the window's width; the zoom keys change the size of its type instead.

use std::path::Path;

use iced::Task;

use super::{App, Message, Notice};
use crate::backend::{self, Flow};
use crate::history::{self, Fit, View};
use crate::input::Action;
use crate::layout::GAP;
use crate::num;
use crate::reader::{PageMode, Reader};

/// How much one press of a zoom key changes the type of a book, in points.
const FONT_STEP: f32 = 1.0;

impl App {
    /// The page a book is laid out on: the window, or one column of it,
    /// in the type set. A row of pages and the gaps between them fill the
    /// window's width exactly when the book fits it.
    pub(super) fn flow(&self, columns: usize) -> Flow {
        let columns = num::float(u32::try_from(columns.max(1)).unwrap_or(1));
        let gaps = GAP * (columns - 1.0);
        let width = (self.area.width - gaps) / columns;
        Flow::new(width, self.area.height, self.font_size)
    }

    /// Whether the document in view is a book, laid out on the window.
    pub(super) fn in_book(&self) -> bool {
        self.document()
            .is_some_and(|document| document.flow.is_some())
    }

    /// The page a document about to open would be laid out on, if it is a
    /// book: in as many columns as it was left in, or as a document not
    /// seen before starts with.
    pub(super) fn flow_for(&self, path: &Path) -> Flow {
        let left = self
            .history
            .as_ref()
            .and_then(|history| history.load(path).ok().flatten());
        let columns = crate::paths::settings_for(&self.path_rules, path)
            .columns
            .unwrap_or_else(|| left.map_or(self.fresh.columns, |view| view.columns));
        self.flow(match self.paging.mode {
            PageMode::Continuous => columns,
            PageMode::Single => 1,
            PageMode::Double => 2,
        })
    }

    /// Sets how many pages sit side by side in the tab in view, laying a
    /// book out again to match.
    pub(super) fn set_columns(&mut self, columns: usize) -> Task<Message> {
        let Some(document) = self.document_mut() else {
            return Task::none();
        };
        document.reader.set_columns(columns);
        self.notice = Notice::Info(if document.reader.page_mode().paged() {
            format!("{columns} to a row once pages are continuous")
        } else if columns == 1 {
            "one page to a row".to_owned()
        } else {
            format!("{columns} pages side by side")
        });
        self.reflow()
    }

    /// A view of a document's pages, of these sizes, fitted to the window
    /// and shown as every tab shows its pages.
    pub(super) fn new_reader(&self, sizes: Vec<crate::layout::PageSize>) -> Reader {
        let mut reader = Reader::new(sizes, (self.area.width, self.area.height));
        reader.set_page_mode(self.paging.mode);
        reader
    }

    /// Shows the pages of every tab in a column, a page at a time, or two
    /// at a time, and lays books out again for the width a page has.
    pub(super) fn set_page_mode(&mut self, mode: PageMode) -> Task<Message> {
        self.paging.mode = mode;
        for tab in &mut self.tabs {
            if let Some(document) = &mut tab.document {
                document.reader.set_page_mode(mode);
            }
        }
        if !self.sourcing {
            self.notice = Notice::Info(
                match mode {
                    PageMode::Continuous => "pages continuous",
                    PageMode::Single => "one page at a time",
                    PageMode::Double => "two pages at a time",
                }
                .to_owned(),
            );
        }
        self.reflow()
    }

    /// Sets the size of type books are laid out in, and lays them out again.
    pub(super) fn set_font_size(&mut self, size: f32) -> Task<Message> {
        let size = size.clamp(Flow::EM.0, Flow::EM.1);
        self.font_size = size;
        self.notice = Notice::Info(format!("font size {size}"));
        self.reflow()
    }

    /// Carries out a zoom key in a book: bigger or smaller type, the type
    /// set again, or a share of it. A book always fits the window's width,
    /// so fitting it otherwise does nothing.
    pub(super) fn zoom_book(&mut self, action: Action) -> Task<Message> {
        let size = match action {
            Action::ZoomIn => self.font_size + FONT_STEP,
            Action::ZoomOut => self.font_size - FONT_STEP,
            Action::ZoomTo(percent) => self.font_base * num::float(percent) / 100.0,
            _ => {
                self.notice = Notice::Info("a book always fits the window's width".to_owned());
                return Task::none();
            }
        };
        self.set_font_size(size)
    }

    /// Lays out again, on the page the window now makes, every book laid
    /// out on another, by opening it again in its tab. The book stays in
    /// view until the new layout is ready, and then shows the same place.
    pub(super) fn reflow(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        for index in 0..self.tabs.len() {
            let Some(tab) = self.tabs.get(index) else {
                continue;
            };
            let Some(document) = &tab.document else {
                continue;
            };
            let flow = self.flow(document.reader.pages_per_row());
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

/// Puts a reader's view back as it was left, moved to the same place if
/// the book has been laid out anew, and says what view it took.
pub(super) fn restore(reader: &mut Reader, view: View, laid_out: bool) -> View {
    // Laid out as it was, to compare its height with the view's.
    reader.set_columns(view.columns);
    let view = refit(view, reader, laid_out);
    reader.restore(view);
    view
}

/// A view moved to the same place in a book laid out anew, which may have
/// come out taller or shorter: as far down, as a share of the whole, and
/// fitting the window's width, as a book always does.
pub(super) fn refit(view: View, reader: &Reader, laid_out: bool) -> View {
    if !laid_out {
        return view;
    }
    let view = View {
        fit: Fit::Width,
        ..view
    };
    let extent = reader.extent();
    if view.extent.is_nan() || view.extent <= 0.0 || !extent.is_finite() {
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
    use crate::layout::{PageSize, Rotation};

    fn view(y: f32, extent: f32) -> View {
        View {
            x: 0.0,
            y,
            zoom: 1.0,
            fit: Fit::None,
            rotation: Rotation::None,
            recolor: false,
            trim: false,
            extent,
            columns: 1,
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
        assert_eq!(moved.fit, Fit::Width);
        // A view from before the height was kept, or of a document with
        // pages of its own, stays as it was.
        assert!((refit(view(300.0, 0.0), &reader, true).y - 300.0).abs() < 0.01);
        assert!((refit(view(300.0, 100.0), &reader, false).y - 300.0).abs() < 0.01);
    }
}
