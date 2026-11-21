//! Search as you type: while a search is typed after `/` or `?`, the
//! matches on the pages in view are highlighted, as vim's `incsearch`
//! does.

use iced::Task;
use iced::widget::canvas;

use super::{App, Document, HIT, Message, rectangle};
use crate::backend::{self, Hit};
use crate::input::Prompt;

/// The matches in view for what is being typed.
pub(super) struct Preview {
    /// The document searched, by its number.
    id: u64,
    needle: String,
    hits: Vec<Hit>,
}

impl App {
    /// Searches the pages in view for what is being typed, when the
    /// prompt is a search and searching as you type is on.
    pub(super) fn preview_search(&mut self) -> Task<Message> {
        let Some((Prompt::SearchForward | Prompt::SearchBackward, line)) = &self.prompt else {
            return Task::none();
        };
        if !self.incsearch || line.is_empty() {
            self.preview = None;
            return Task::none();
        }
        let Some(document) = self.document() else {
            return Task::none();
        };
        let id = document.id;
        let needle = line.clone();
        let pages = document.reader.visible_pages().collect();
        let finding = document.backend.search_pages(needle.clone(), pages);
        Task::perform(finding, move |result| {
            Message::Previewed(id, needle, result)
        })
    }

    /// Keeps the matches found, if they are still for what is typed.
    pub(super) fn on_previewed(
        &mut self,
        id: u64,
        needle: String,
        result: Result<Vec<Hit>, backend::Error>,
    ) {
        // A failed search says so when it is run; typing goes on.
        let Ok(hits) = result else {
            return;
        };
        if self.typed_search() == Some(needle.as_str()) {
            self.preview = Some(Preview { id, needle, hits });
        }
    }

    /// The text of the search being typed, if one is.
    fn typed_search(&self) -> Option<&str> {
        match &self.prompt {
            Some((Prompt::SearchForward | Prompt::SearchBackward, line)) => Some(line),
            _ => None,
        }
    }

    /// The matches to show for the search being typed in a document, if
    /// they are for what is typed now.
    pub(super) fn preview_for(&self, document: &Document) -> Option<&[Hit]> {
        let preview = self.preview.as_ref()?;
        (preview.id == document.id && self.typed_search() == Some(preview.needle.as_str()))
            .then_some(preview.hits.as_slice())
    }
}

/// Highlights the matches found so far for the search being typed.
pub(super) fn draw(frame: &mut canvas::Frame, document: &Document, hits: &[Hit]) {
    for hit in hits {
        if let Some(area) = document.reader.area_on_screen(hit.page, hit.area) {
            let area = rectangle(area);
            frame.fill_rectangle(area.position(), area.size(), HIT);
        }
    }
}
