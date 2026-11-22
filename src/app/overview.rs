//! The overview of the document in view: every page small, side by side
//! in rows, over the pages. `g o` opens it at the page being read; the
//! keys move through the pages, and enter reads from the one selected.
//!
//! The small pages are rendered as they come into view, and kept with the
//! document, so that opening the overview again shows them at once.

use iced::widget::{Column, Row, column, container, image, text};
use iced::{Element, Fill, Font, Length, Task};

use super::{App, Document, Message, Notice, RenderKey, current_document, document_by_id, leap};
use crate::backend::{self, RenderJob};
use crate::input::Key;
use crate::layout::Rotation;
use crate::num;
use crate::overview::{Outcome, Overview};
use crate::theme::Colours;

/// How wide a small page is, in pixels before the screen's scale.
const WIDTH: f32 = 160.0;
/// The room between small pages, and around them all.
const GAP: f32 = 20.0;
/// How high the line under each, with its number, is.
const CAPTION: f32 = 18.0;
/// The frame around each, which shows when it is selected.
const FRAME: f32 = 3.0;
/// The rows rendered ahead of those in view, either way.
const AHEAD: u32 = 2;

impl App {
    /// Shows every page of the document in view small, at the page being
    /// read.
    pub(super) fn show_overview(&mut self) -> Task<Message> {
        let Some(document) = self.document() else {
            return Task::none();
        };
        let reader = &document.reader;
        self.overview = Some((
            document.id,
            Overview::new(reader.page_count(), reader.current_page()),
        ));
        self.render_small()
    }

    /// The overview, if it is of the document in view.
    pub(super) fn overview(&self) -> Option<&Overview> {
        let id = self.document()?.id;
        self.overview
            .as_ref()
            .filter(|(of, _)| *of == id)
            .map(|(_, overview)| overview)
    }

    /// What the status bar says in the overview: the page selected, what
    /// has been typed, and the keys to read from it or go back.
    pub(super) fn overview_status(&self, overview: &Overview) -> String {
        let count = self
            .document()
            .map_or(0, |document| document.reader.page_count());
        let typed = overview.typed();
        let typed = if typed.is_empty() {
            typed
        } else {
            format!("{typed}    ")
        };
        format!(
            "{typed}page {} of {count}    {}: read    {}: back",
            overview.selected().saturating_add(1),
            self.keymaps.overview.first(crate::overview::Binding::Go),
            self.keymaps.overview.first(crate::overview::Binding::Close),
        )
    }

    /// How many small pages fit across the window, and how many rows of
    /// them down it.
    fn overview_shape(&self) -> (u32, u32) {
        let (width, height) = self.small_size();
        // Each takes its size and its frame, with a gap after it, and the
        // gap around them all takes one more.
        let fit = |room: f32, each: f32| {
            num::round(((room - GAP) / (each + FRAME + FRAME + GAP)).floor()).max(1)
        };
        (
            fit(self.area.width, width),
            fit(self.area.height, height + CAPTION),
        )
    }

    /// The size each small page is given: as wide as [`WIDTH`], and as
    /// high as the page selected is for that width, turned as the pages are.
    fn small_size(&self) -> (f32, f32) {
        let shape = self.document().and_then(|document| {
            let page = self
                .overview()
                .map_or_else(|| document.reader.current_page(), Overview::selected);
            let size = document.reader.page_size(page)?;
            let turned = matches!(
                document.reader.rotation(),
                Rotation::Quarter | Rotation::ThreeQuarters
            );
            let (across, down) = if turned {
                (size.height, size.width)
            } else {
                (size.width, size.height)
            };
            (across > 0.0).then(|| down / across)
        });
        (WIDTH, WIDTH * shape.unwrap_or(1.4).clamp(0.4, 2.0))
    }

    /// Takes a key while the overview is shown.
    pub(super) fn on_overview_key(&mut self, key: Key) -> Task<Message> {
        let (columns, rows) = self.overview_shape();
        let Some((_, overview)) = &mut self.overview else {
            return Task::none();
        };
        match overview.feed(key, &self.keymaps.overview, columns, rows) {
            // The small pages now in view are asked for after each key.
            Outcome::Stay => Task::none(),
            Outcome::Close => {
                self.overview = None;
                Task::none()
            }
            Outcome::Go(page) => self.go_from_overview(page),
        }
    }

    /// Leaves the overview for a page, picked by key or by a click.
    pub(super) fn go_from_overview(&mut self, page: u32) -> Task<Message> {
        self.overview = None;
        if let Some(document) = self.document_mut() {
            leap(document, page);
        }
        Task::none()
    }

    /// Asks for the small pages in view, and the rows around them, that
    /// are not yet rendered as they are wanted now.
    pub(super) fn render_small(&mut self) -> Task<Message> {
        let (columns, rows) = self.overview_shape();
        let Some(shown) = self.overview().map(|overview| {
            overview.shown(columns, rows.saturating_add(AHEAD.saturating_mul(2)))
        }) else {
            return Task::none();
        };
        let palette = self.palette();
        let scale_factor = self.scale_factor;
        let Some(document) = current_document(&mut self.tabs, self.current) else {
            return Task::none();
        };
        let id = document.id;
        let rotation = document.reader.rotation();
        let recolor = document.recolor.then_some(palette);
        let mut tasks = Vec::new();
        for page in shown {
            let Some(size) = document.reader.page_size(page) else {
                continue;
            };
            let across = match rotation {
                Rotation::Quarter | Rotation::ThreeQuarters => size.height,
                Rotation::None | Rotation::Half => size.width,
            };
            let scale = WIDTH * scale_factor / across.max(1.0);
            let wanted = RenderKey {
                scale: scale.to_bits(),
                rotation,
                palette: recolor,
                trimmed: false,
                edition: document.edition,
            };
            let current = document
                .small
                .get(&page)
                .is_some_and(|(_, key)| *key == wanted);
            if current || document.rendering_small.contains_key(&page) {
                continue;
            }
            document.rendering_small.insert(page, wanted);
            let render = document.backend.render(RenderJob {
                page,
                scale,
                rotation,
                clip: None,
                palette: recolor,
            });
            tasks.push(Task::perform(render, move |result| {
                let handle = result.map(|bitmap| {
                    image::Handle::from_rgba(bitmap.width, bitmap.height, bitmap.pixels)
                });
                Message::RenderedSmall(id, page, wanted, handle)
            }));
        }
        Task::batch(tasks)
    }

    /// Keeps a small page that was rendered. One still wanted, as when it
    /// was out of date by the time it came back, is asked for again after.
    pub(super) fn on_rendered_small(
        &mut self,
        id: u64,
        page: u32,
        key: RenderKey,
        result: Result<image::Handle, backend::Error>,
    ) -> Task<Message> {
        let Some(document) = document_by_id(&mut self.tabs, id) else {
            return Task::none();
        };
        document.rendering_small.remove(&page);
        match result {
            Ok(handle) => {
                document.small.insert(page, (handle, key));
            }
            Err(error) => self.notice = Notice::Error(error.to_string()),
        }
        Task::none()
    }

    /// The overview, drawn over the pages.
    pub(super) fn overview_grid(&self, chrome: Colours) -> Option<Element<'_, Message>> {
        let overview = self.overview()?;
        let document = self.document()?;
        let (columns, rows) = self.overview_shape();
        let (width, height) = self.small_size();
        let shown: Vec<u32> = overview.shown(columns, rows).collect();
        let lines = shown.chunks(num::index(columns)).map(|line| {
            Row::with_children(
                line.iter()
                    .map(|&page| small_page(document, page, overview.selected() == page, (width, height), chrome)),
            )
            .spacing(GAP)
            .into()
        });
        let grid = Column::with_children(lines).spacing(GAP);
        Some(
            container(grid)
                .padding(GAP)
                .width(Fill)
                .height(Fill)
                .center_x(Fill)
                .center_y(Fill)
                .style(move |_| container::background(chrome.backdrop))
                .into(),
        )
    }
}

/// One small page, framed in the theme's colour when selected, with its
/// number under it: the number printed on it, when the document gives one.
fn small_page(
    document: &Document,
    page: u32,
    selected: bool,
    (width, height): (f32, f32),
    chrome: Colours,
) -> Element<'_, Message> {
    let picture: Element<'_, Message> = match document.small.get(&page) {
        Some((handle, _)) => image(handle.clone())
            .width(Length::Fixed(width))
            .height(Length::Fixed(height))
            .content_fit(iced::ContentFit::Contain)
            .into(),
        None => iced::widget::Space::new()
            .width(Length::Fixed(width))
            .height(Length::Fixed(height))
            .into(),
    };
    let number = page.saturating_add(1).to_string();
    let caption = document
        .label(page)
        .filter(|label| *label != number)
        .map_or_else(|| number.clone(), |label| format!("{label} ({number})"));
    let frame = if selected {
        chrome.tab_current
    } else {
        chrome.backdrop
    };
    let colour = if selected {
        chrome.bar_text
    } else {
        chrome.list_number
    };
    let small = column![
        container(picture)
            .padding(FRAME)
            .style(move |_| container::background(frame)),
        container(
            text(caption)
                .size(12)
                .font(Font::MONOSPACE)
                .color(colour)
                .wrapping(text::Wrapping::None)
        )
        .height(CAPTION)
        .center_x(Length::Fixed(width))
        .clip(true),
    ]
    .width(Length::Shrink);
    super::clickable(small, super::pointer::Click::Overview(page))
}
