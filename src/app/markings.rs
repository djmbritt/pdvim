//! Highlights and the other markings in the window: making them, taking
//! them away, and taking either back; going from one to the next; and the
//! sidebar that lists them.

use iced::widget::canvas;
use iced::widget::{Column, column, container, row, text};
use iced::{Color, Element, Fill, Font, Task};

use super::{
    App, Document, HELP_ROW, LIST_TEXT_SIZE, Message, Notice, Prompt, SIDEBAR_WIDTH, Sidebar,
    colour, current_document, document_by_id, rectangle,
};
use crate::backend::{self, Annotation, Marking, Markup, MarkupKind};
use crate::bindings::Keymaps;
use crate::input::MarkingAction;
use crate::keymap::Pending;
use crate::layout::Rect;
use crate::markings::{self as found, Binding, List, Outcome};
use crate::reader::Reader;
use crate::theme::Colours;

/// The size of a note's icon, in points.
const NOTE_SIZE: f32 = 20.0;

/// How far from the page's left edge a note goes, and how far below the
/// top of the window when not beside visual mode's cursor, in points.
const NOTE_MARGIN: f32 = 12.0;

/// The colour of the frame around the marking last gone to.
const CURRENT: Color = Color::from_rgba8(0xff, 0x70, 0x00, 0.9);

/// A change made to a document's file, to take back and put back.
#[derive(Debug, Clone)]
pub enum Edit {
    /// Text was marked, which added these annotations.
    Marked {
        markup: Markup,
        annotations: Vec<Annotation>,
    },
    /// These markings were taken away; or, while that is taken back, put
    /// back as they now are.
    Unmarked(Vec<Marking>),
}

impl Edit {
    /// What the change was to, as the bottom line says it.
    fn noun(&self) -> String {
        match self {
            Self::Marked { markup, .. } => markup.kind.noun().to_owned(),
            Self::Unmarked(markings) => match markings.as_slice() {
                [one] => one.kind.noun().to_owned(),
                many => format!("{} markings", many.len()),
            },
        }
    }

    /// The annotations the change, once made, took away.
    fn gone(&self, step: Step, asked: &Self) -> Vec<Annotation> {
        match (self, step, asked) {
            (Self::Unmarked(markings), Step::Do | Step::Redo, _) => {
                markings.iter().map(Marking::annotation).collect()
            }
            (_, Step::Undo, Self::Marked { annotations, .. }) => annotations.clone(),
            _ => Vec::new(),
        }
    }
}

/// Which way a change goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Do,
    Undo,
    Redo,
}

/// What the markings of a document were asked for, to do once found.
#[derive(Debug, Clone, PartialEq)]
pub enum Wanted {
    /// Go a number of markings on, or back.
    Step { forward: bool, times: u32 },
    /// Take away the marking last gone to.
    DeleteCurrent,
    /// Take away the markings over these areas, each on a page.
    DeleteIn(Vec<(u32, Rect)>),
    /// List them in the sidebar.
    List,
    /// Know them afresh after a change, for the sidebar and the frame
    /// around the one gone to.
    Refresh,
}

impl App {
    pub(super) fn on_marking_action(&mut self, action: MarkingAction) -> Task<Message> {
        let wanted = match action {
            MarkingAction::Step { forward, times } => Wanted::Step { forward, times },
            MarkingAction::Delete => Wanted::DeleteCurrent,
            MarkingAction::List => Wanted::List,
        };
        self.with_markings(wanted)
    }

    /// Does what the markings of the document in view are wanted for,
    /// finding them first if they are not known.
    pub(super) fn with_markings(&mut self, wanted: Wanted) -> Task<Message> {
        let Some(document) = self.document() else {
            return Task::none();
        };
        if document.markings.is_some() {
            return self.use_markings(wanted);
        }
        let id = document.id;
        let edition = document.edition;
        let finding = document.backend.markings();
        Task::perform(finding, move |result| {
            Message::MarkingsFound(id, edition, wanted, result)
        })
    }

    pub(super) fn on_markings(
        &mut self,
        id: u64,
        edition: u32,
        wanted: Wanted,
        result: Result<Vec<Marking>, backend::Error>,
    ) -> Task<Message> {
        let Some(document) = document_by_id(&mut self.tabs, id) else {
            return Task::none();
        };
        match result {
            Ok(markings) if edition == document.edition => document.markings = Some(markings),
            // Changed since: they are found again below.
            Ok(_) => {}
            Err(error) => {
                self.notice = Notice::Error(error.to_string());
                return Task::none();
            }
        }
        // Only for the document still in view.
        if self.document().is_some_and(|document| document.id == id) {
            return self.with_markings(wanted);
        }
        Task::none()
    }

    /// Does what the markings were wanted for, now that they are known.
    fn use_markings(&mut self, wanted: Wanted) -> Task<Message> {
        match wanted {
            Wanted::Step { forward, times } => {
                self.step_marking(forward, times);
                Task::none()
            }
            Wanted::DeleteCurrent => self.delete_current(),
            Wanted::DeleteIn(areas) => self.delete_in(&areas),
            Wanted::List => {
                self.list_markings();
                Task::none()
            }
            Wanted::Refresh => {
                let markings = self
                    .document()
                    .and_then(|document| document.markings.clone())
                    .unwrap_or_default();
                if let Some(Sidebar::Markings(list)) = self.sidebar.take() {
                    self.sidebar = list.refreshed(markings).map(Sidebar::Markings);
                }
                Task::none()
            }
        }
    }

    /// Goes a number of markings on from the one last gone to, if it is in
    /// view, or from the top of the view; or back.
    fn step_marking(&mut self, forward: bool, times: u32) {
        let height = self.area.height;
        let Some(document) = current_document(&mut self.tabs, self.current) else {
            return;
        };
        let Some(markings) = &document.markings else {
            return;
        };
        if markings.is_empty() {
            self.notice = Notice::Info("this document has no highlights or other markings".into());
            return;
        }
        let reader = &document.reader;
        let from = document
            .marking
            .and_then(|current| {
                markings
                    .iter()
                    .position(|marking| marking.annotation() == current)
            })
            .filter(|&index| {
                markings
                    .get(index)
                    .is_some_and(|marking| in_view(reader, marking, height))
            });
        let top = reader.current_page();
        let below = |marking: &Marking| {
            marking.page > top
                || (marking.page == top
                    && on_screen(reader, marking).is_some_and(|area| area.top >= 0.0))
        };
        let Some(index) = found::step(markings, from, below, forward, times) else {
            self.notice = Notice::Info(
                if forward {
                    "no marking after this one"
                } else {
                    "no marking before this one"
                }
                .to_owned(),
            );
            return;
        };
        let count = markings.len();
        if let Some(marking) = markings.get(index).cloned() {
            self.notice = Notice::Info(go_to(document, &marking, index, count));
        }
    }

    /// Takes away the marking last gone to, if it is in view.
    fn delete_current(&mut self) -> Task<Message> {
        let height = self.area.height;
        let Some(document) = self.document() else {
            return Task::none();
        };
        let picked = document.marking.and_then(|current| {
            document
                .markings
                .as_ref()?
                .iter()
                .find(|marking| marking.annotation() == current)
                .filter(|marking| in_view(&document.reader, marking, height))
                .cloned()
        });
        let Some(marking) = picked else {
            self.notice = Notice::Info("no marking picked here: ]h or [h picks one".to_owned());
            return Task::none();
        };
        let id = document.id;
        self.start_edit(id, Edit::Unmarked(vec![marking]), Step::Do)
    }

    /// Takes away the markings over some areas, as selected in visual mode.
    fn delete_in(&mut self, areas: &[(u32, Rect)]) -> Task<Message> {
        let Some(document) = self.document() else {
            return Task::none();
        };
        let markings = document.markings.as_deref().unwrap_or_default();
        let doomed: Vec<Marking> = found::touching(markings, areas)
            .into_iter()
            .filter_map(|index| markings.get(index).cloned())
            .collect();
        if doomed.is_empty() {
            self.notice = Notice::Info("no marking there".to_owned());
            return Task::none();
        }
        let id = document.id;
        self.start_edit(id, Edit::Unmarked(doomed), Step::Do)
    }

    /// Lists the markings in the sidebar, or says there are none.
    fn list_markings(&mut self) {
        let Some(document) = self.document() else {
            return;
        };
        let markings = document.markings.clone().unwrap_or_default();
        let page = document.reader.current_page();
        match List::new(markings, page) {
            Some(list) => {
                self.pending = Pending::new();
                self.sidebar = Some(Sidebar::Markings(list));
            }
            None => {
                self.notice =
                    Notice::Info("this document has no highlights or other markings".to_owned());
            }
        }
    }

    /// Does what a key in the list of markings came to.
    pub(super) fn on_marking_list(&mut self, outcome: Outcome) -> Task<Message> {
        let Some(Sidebar::Markings(list)) = &self.sidebar else {
            return Task::none();
        };
        match outcome {
            Outcome::Stay => Task::none(),
            Outcome::Close => {
                self.sidebar = None;
                Task::none()
            }
            Outcome::Go(index) => {
                let count = list.len();
                let marking = list.get(index).cloned();
                self.sidebar = None;
                if let (Some(marking), Some(document)) =
                    (marking, current_document(&mut self.tabs, self.current))
                {
                    self.notice = Notice::Info(go_to(document, &marking, index, count));
                }
                Task::none()
            }
            Outcome::Delete(index) => {
                let marking = list.get(index).cloned();
                match (marking, self.document()) {
                    (Some(marking), Some(document)) => {
                        let id = document.id;
                        self.start_edit(id, Edit::Unmarked(vec![marking]), Step::Do)
                    }
                    _ => Task::none(),
                }
            }
        }
    }

    /// Opens the prompt for the text of a note to put at an area of a page
    /// of the document in view.
    pub(super) fn ask_for_note(&mut self, page: u32, area: Rect) {
        let Some(document) = self.document() else {
            return;
        };
        self.note_at = Some((document.id, page, area));
        self.completing = None;
        self.prompt_pending = Pending::new();
        self.prompt = Some((Prompt::Command, "note ".to_owned()));
    }

    /// Puts a note with this text in the left margin: beside the line of
    /// visual mode's cursor, or else at the top of the window.
    pub(super) fn add_note(&mut self, text: String) -> Task<Message> {
        let colour = self.markup_colours.of(MarkupKind::Note);
        let wanted = self.note_at.take();
        let Some(document) = self.document() else {
            return Task::none();
        };
        let (page, left, top) = match wanted {
            // In the margin beside the cursor's line, so as not to cover
            // the text.
            Some((id, page, area)) if id == document.id => {
                let middle = f32::midpoint(area.top, area.bottom);
                (page, NOTE_MARGIN, (middle - NOTE_SIZE / 2.0).max(0.0))
            }
            _ => {
                let place = document.reader.place();
                let height = document
                    .reader
                    .page_size(place.page)
                    .map_or(0.0, |size| size.height);
                (
                    place.page,
                    NOTE_MARGIN,
                    place.down.mul_add(height, NOTE_MARGIN),
                )
            }
        };
        let markup = Markup {
            kind: MarkupKind::Note,
            pages: vec![(
                page,
                vec![Rect {
                    left,
                    top,
                    right: left + NOTE_SIZE,
                    bottom: top + NOTE_SIZE,
                }],
            )],
            colour,
            note: Some(text),
        };
        let id = document.id;
        self.start_edit(
            id,
            Edit::Marked {
                markup,
                annotations: Vec::new(),
            },
            Step::Do,
        )
    }

    /// Takes back the last change to the markings of the document in view.
    pub(super) fn undo(&mut self) -> Task<Message> {
        let Some(document) = self.document_mut() else {
            return Task::none();
        };
        let Some(edit) = document.undo.pop() else {
            self.notice = Notice::Info("nothing to undo".to_owned());
            return Task::none();
        };
        let id = document.id;
        self.notice = Notice::Info("undoing".to_owned());
        self.start_edit(id, edit, Step::Undo)
    }

    /// Puts back the last change taken back.
    pub(super) fn redo(&mut self) -> Task<Message> {
        let Some(document) = self.document_mut() else {
            return Task::none();
        };
        let Some(edit) = document.redo.pop() else {
            self.notice = Notice::Info("nothing to redo".to_owned());
            return Task::none();
        };
        let id = document.id;
        self.notice = Notice::Info("redoing".to_owned());
        self.start_edit(id, edit, Step::Redo)
    }

    /// Makes a change to a document's markings, takes it back, or puts it
    /// back.
    pub(super) fn start_edit(&mut self, id: u64, edit: Edit, step: Step) -> Task<Message> {
        let Some(document) = document_by_id(&mut self.tabs, id) else {
            return Task::none();
        };
        let backend = document.backend.clone();
        let asked = edit.clone();
        let making = async move {
            match (edit, step) {
                (Edit::Marked { markup, .. }, Step::Do | Step::Redo) => {
                    let annotations = backend.highlight(markup.clone()).await?;
                    Ok(Edit::Marked {
                        markup,
                        annotations,
                    })
                }
                (
                    Edit::Marked {
                        markup,
                        annotations,
                    },
                    Step::Undo,
                ) => {
                    backend.remove(annotations).await?;
                    Ok(Edit::Marked {
                        markup,
                        annotations: Vec::new(),
                    })
                }
                (Edit::Unmarked(markings), Step::Do | Step::Redo) => {
                    let annotations = markings.iter().map(Marking::annotation).collect();
                    Ok(Edit::Unmarked(backend.remove(annotations).await?))
                }
                (Edit::Unmarked(mut markings), Step::Undo) => {
                    let annotations = backend.restore(markings.clone()).await?;
                    for (marking, (_, xref)) in markings.iter_mut().zip(annotations) {
                        marking.xref = xref;
                    }
                    Ok(Edit::Unmarked(markings))
                }
            }
        };
        Task::perform(making, move |result| {
            Message::Edited(id, asked, step, result)
        })
    }

    pub(super) fn on_edited(
        &mut self,
        id: u64,
        asked: Edit,
        step: Step,
        result: Result<Edit, backend::Error>,
    ) -> Task<Message> {
        let Some(document) = document_by_id(&mut self.tabs, id) else {
            return Task::none();
        };
        let edit = match result {
            Ok(edit) => edit,
            Err(error) => {
                // What failed can be tried again.
                match step {
                    Step::Do => {}
                    Step::Undo => document.undo.push(asked),
                    Step::Redo => document.redo.push(asked),
                }
                self.notice = Notice::Error(error.to_string());
                return Task::none();
            }
        };
        if step == Step::Do && matches!(&edit, Edit::Unmarked(markings) if markings.is_empty()) {
            self.notice = Notice::Info("that marking is gone already".to_owned());
            return Task::none();
        }
        let noun = edit.noun();
        let done = match (&edit, step) {
            (Edit::Marked { markup, .. }, Step::Do) => markup.kind.done().to_owned(),
            (Edit::Marked { .. }, Step::Undo) => format!("{noun} taken back"),
            (Edit::Unmarked(_), Step::Do | Step::Redo) => format!("{noun} taken away"),
            (Edit::Marked { .. }, Step::Redo) | (Edit::Unmarked(_), Step::Undo) => {
                format!("{noun} put back")
            }
        };
        if step == Step::Do {
            // A state that was saved and then undone is gone for good once
            // something new is done in its place.
            if document
                .saved
                .is_some_and(|saved| saved > document.undo.len())
            {
                document.saved = None;
            }
            // A new change ends what could be redone.
            document.redo.clear();
        }
        let gone = edit.gone(step, &asked);
        if document
            .marking
            .is_some_and(|current| gone.contains(&current))
        {
            document.marking = None;
        }
        match step {
            Step::Do | Step::Redo => document.undo.push(edit),
            Step::Undo => document.redo.push(edit),
        }
        // Every page is rendered afresh to show the change.
        document.edition = document.edition.saturating_add(1);
        document.markings = None;
        let wanted_again = document.marking.is_some();
        let in_view = self.document().is_some_and(|document| document.id == id);
        let refresh =
            if in_view && (wanted_again || matches!(self.sidebar, Some(Sidebar::Markings(_)))) {
                self.with_markings(Wanted::Refresh)
            } else {
                Task::none()
            };
        Task::batch([self.changed(id, &done), refresh])
    }
}

/// Where a marking is in the window, if it can be placed.
fn on_screen(reader: &Reader, marking: &Marking) -> Option<Rect> {
    reader.area_on_screen(marking.page, found::extent(marking)?)
}

/// Whether any of a marking shows in a window this high.
fn in_view(reader: &Reader, marking: &Marking, height: f32) -> bool {
    on_screen(reader, marking).is_some_and(|area| area.bottom > 0.0 && area.top < height)
}

/// Goes to a marking, the one with this index of so many, noting where
/// it left from in the jump list. Answers with what to say about it.
fn go_to(document: &mut Document, marking: &Marking, index: usize, count: usize) -> String {
    document.marking = Some(marking.annotation());
    document.jumps.record(document.reader.place());
    if let Some(area) = found::extent(marking) {
        document.reader.reveal(marking.page, area);
    }
    let said = format!(
        "{} {} of {count}",
        marking.kind.noun(),
        index.saturating_add(1)
    );
    match &marking.kept.contents {
        Some(note) => format!("{said}: {note}"),
        None => said,
    }
}

/// Draws a frame around each line of the marking last gone to.
pub(super) fn draw_current(frame: &mut canvas::Frame, document: &Document) {
    let (Some(current), Some(markings)) = (document.marking, &document.markings) else {
        return;
    };
    let Some(marking) = markings
        .iter()
        .find(|marking| marking.annotation() == current)
    else {
        return;
    };
    let stroke = canvas::Stroke::default()
        .with_color(CURRENT)
        .with_width(2.0);
    for &area in &marking.areas {
        if let Some(area) = document.reader.area_on_screen(marking.page, area) {
            let area = rectangle(area);
            frame.stroke(
                &canvas::Path::rectangle(area.position(), area.size()),
                stroke,
            );
        }
    }
}

/// The markings, in a sidebar over the left of the pages: as many as fit,
/// each with its colour, its text and its page, the selected one marked.
pub(super) fn list<'a>(
    list: &'a List,
    keymaps: &Keymaps,
    chrome: Colours,
    rows: usize,
) -> Element<'a, Message> {
    let label = |content: String, colour: Color| {
        text(content)
            .size(LIST_TEXT_SIZE)
            .font(Font::MONOSPACE)
            .color(colour)
            .wrapping(text::Wrapping::None)
    };
    let (top, shown) = list.shown(rows);
    let entries = shown.iter().enumerate().map(|(offset, marking)| {
        let selected = top.saturating_add(offset) == list.selected();
        let swatch = marking.colour().map_or(chrome.list_number, colour);
        let words = if marking.text.is_empty() {
            format!("({})", marking.kind.noun())
        } else {
            marking.text.clone()
        };
        let line = row![
            label("\u{25a0}".to_owned(), swatch),
            container(label(
                words,
                if selected {
                    chrome.bar_text
                } else {
                    chrome.tab_text
                }
            ))
            .width(Fill)
            .clip(true),
            label(
                marking.page.saturating_add(1).to_string(),
                chrome.list_number
            ),
        ]
        .spacing(6);
        container(line)
            .height(HELP_ROW)
            .width(Fill)
            .padding([0, 6])
            .align_y(iced::alignment::Vertical::Center)
            .style(move |_| {
                if selected {
                    container::background(chrome.tab_current)
                } else {
                    container::Style::default()
                }
            })
            .into()
    });
    let heading = container(label("Markings".to_owned(), chrome.bar_text).font(Font {
        weight: iced::font::Weight::Bold,
        ..Font::MONOSPACE
    }))
    .height(HELP_ROW)
    .padding([0, 6]);
    let end = top.saturating_add(shown.len());
    let foot = container(label(
        format!(
            "{}-{end} of {}   {}: go   {}: take away",
            top.saturating_add(1),
            list.len(),
            keymaps.markings.first(Binding::Go),
            keymaps.markings.first(Binding::Delete),
        ),
        chrome.list_number,
    ))
    .height(HELP_ROW)
    .padding([0, 6])
    .clip(true);
    let panel = container(column![
        heading,
        Column::with_children(entries).height(Fill),
        foot
    ])
    .padding(8)
    .width(SIDEBAR_WIDTH)
    .height(Fill)
    .style(move |_| {
        container::background(chrome.list).border(iced::Border {
            color: chrome.list_border,
            width: 1.0,
            radius: 0.0.into(),
        })
    });
    container(panel).width(Fill).height(Fill).into()
}
