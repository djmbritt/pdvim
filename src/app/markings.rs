//! Highlights and the other markings in the window: making them, taking
//! them away, and taking either back; going from one to the next; and the
//! sidebar that lists them.

use iced::widget::canvas;
use iced::widget::{Column, column, container, row, text};
use iced::{Color, Element, Fill, Font, Task};

use super::{
    App, Document, HELP_ROW, LIST_TEXT_SIZE, Message, Notice, Prompt, SIDEBAR_WIDTH, Sidebar,
    colour, current_document, document_by_id, draw_label, label_size, rectangle,
};
use crate::backend::{self, Annotation, Marking, Markup, MarkupKind};
use crate::bindings::Keymaps;
use crate::hints::{Hints, Outcome as HintOutcome};
use crate::input::MarkingAction;
use crate::keymap::Pending;
use crate::layout::Rect;
use crate::markings::{self as found, Binding, List, Outcome};
use crate::reader::Reader;
use crate::theme::Colours;

/// The size of a note's icon, in points.
const NOTE_SIZE: f32 = 20.0;

/// How far from the page's left edge a note goes, and how far below the
/// top of the window, when not at visual mode's cursor, in points.
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
    /// The note on a marking was changed: the text of a note, or the
    /// comment on another kind of marking. None for no note.
    Renoted {
        annotation: Annotation,
        before: Option<String>,
        after: String,
    },
    /// A field of a form was filled in: what it held, and holds; and
    /// whether it is a box or a button, ticked rather than typed in.
    Filled {
        widget: Annotation,
        before: String,
        after: String,
        tick: bool,
    },
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
            Self::Renoted { .. } => "note".to_owned(),
            Self::Filled { .. } => "field".to_owned(),
        }
    }

    /// Takes markings that came back as other annotations to be those:
    /// putting a marking back, or making it again, gives it a new number.
    fn renumber(&mut self, moved: &[(Annotation, Annotation)]) {
        let new = |old: Annotation| {
            moved
                .iter()
                .find(|(from, _)| *from == old)
                .map_or(old, |&(_, to)| to)
        };
        match self {
            Self::Marked { annotations, .. } => {
                for annotation in annotations {
                    *annotation = new(*annotation);
                }
            }
            Self::Unmarked(markings) => {
                for marking in markings {
                    (marking.page, marking.xref) = new(marking.annotation());
                }
            }
            Self::Renoted { annotation, .. } => *annotation = new(*annotation),
            // A widget of a form is never taken away and put back.
            Self::Filled { .. } => {}
        }
    }

    /// The annotations that, made again or put back by this change, are now
    /// other annotations: each as it was, and as it is.
    fn moved(&self, step: Step, asked: &Self) -> Vec<(Annotation, Annotation)> {
        match (self, step, asked) {
            (Self::Unmarked(now), Step::Undo, Self::Unmarked(before)) => before
                .iter()
                .map(Marking::annotation)
                .zip(now.iter().map(Marking::annotation))
                .collect(),
            (
                Self::Marked {
                    annotations: now, ..
                },
                Step::Redo,
                Self::Marked {
                    annotations: before,
                    ..
                },
            ) => before.iter().copied().zip(now.iter().copied()).collect(),
            _ => Vec::new(),
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
    /// Change the note on the marking last gone to.
    ChangeCurrent,
    /// Copy the text of the marking last gone to.
    YankCurrent,
    /// Change the note on the marking at a character, from visual mode.
    ChangeAt(u32, Rect),
    /// Label the markings in view, to pick one.
    Label,
    /// Take away the markings over these areas, each on a page.
    DeleteIn(Vec<(u32, Rect)>),
    /// List them in the sidebar.
    List,
    /// Know them afresh after a change, for the sidebar and the frame
    /// around the one gone to.
    Refresh,
    /// Label them with the links in view, to follow or pick one.
    Follow,
}

impl App {
    pub(super) fn on_marking_action(&mut self, action: MarkingAction) -> Task<Message> {
        let wanted = match action {
            MarkingAction::Step { forward, times } => Wanted::Step { forward, times },
            MarkingAction::Delete => Wanted::DeleteCurrent,
            MarkingAction::ChangeNote => Wanted::ChangeCurrent,
            MarkingAction::Yank => Wanted::YankCurrent,
            MarkingAction::Label => Wanted::Label,
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
            Wanted::ChangeCurrent => {
                if let Some(marking) = self.picked() {
                    self.ask_to_change(&marking);
                }
                Task::none()
            }
            Wanted::YankCurrent => self
                .picked()
                .map_or_else(Task::none, |marking| self.yank_marking(&marking)),
            Wanted::ChangeAt(page, character) => {
                match self.marking_at(page, character) {
                    Some(marking) => self.ask_to_change(&marking),
                    None => self.notice = Notice::Info("no marking here".to_owned()),
                }
                Task::none()
            }
            Wanted::Label => {
                self.label_markings();
                Task::none()
            }
            Wanted::Follow => self.label_links(crate::input::LinkGoal::Follow),
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
        let Some(marking) = self.picked() else {
            return Task::none();
        };
        let Some(id) = self.document().map(|document| document.id) else {
            return Task::none();
        };
        self.start_edit(id, Edit::Unmarked(vec![marking]), Step::Do)
    }

    /// The marking last gone to, if it is in view; or else says how to
    /// pick one.
    fn picked(&mut self) -> Option<Marking> {
        let height = self.area.height;
        let document = self.document()?;
        let picked = document.marking.and_then(|current| {
            document
                .markings
                .as_ref()?
                .iter()
                .find(|marking| marking.annotation() == current)
                .filter(|marking| in_view(&document.reader, marking, height))
                .cloned()
        });
        if picked.is_none() {
            self.notice = Notice::Info("no marking picked here: ]h or [h picks one".to_owned());
        }
        picked
    }

    /// The note on the marking picked with `]h`, `[h` or `gh`, while it is
    /// in view.
    pub(super) fn picked_note(&self) -> Option<&str> {
        let document = self.document()?;
        let current = document.marking?;
        document
            .markings
            .as_ref()?
            .iter()
            .find(|marking| marking.annotation() == current)
            .filter(|marking| in_view(&document.reader, marking, self.area.height))?
            .kept
            .contents
            .as_deref()
            .filter(|note| !note.trim().is_empty())
    }

    /// The marking at a character of the document in view: over it, or a
    /// note beside its line.
    pub(super) fn marking_at(&self, page: u32, character: Rect) -> Option<Marking> {
        let markings = self.document()?.markings.as_ref()?;
        let index = found::at(markings, page, character)?;
        markings.get(index).cloned()
    }

    /// The marking at visual mode's cursor: over it, or a note beside its
    /// line.
    pub(super) fn marking_at_cursor(&self) -> Option<Marking> {
        let cursor = self.visual.as_ref()?.cursor();
        let character = crate::visual::character(&self.document()?.text, cursor)?;
        self.marking_at(cursor.page, character)
    }

    /// The note on the marking at visual mode's cursor, if it has one.
    pub(super) fn note_at_cursor(&self) -> Option<String> {
        self.marking_at_cursor()?
            .kept
            .contents
            .filter(|note| !note.trim().is_empty())
    }

    /// Whether there is a marking for visual mode to take away or change:
    /// under the selection, or at the cursor.
    pub(super) fn selection_marked(&self) -> bool {
        let (Some(visual), Some(document)) = (self.visual.as_ref(), self.document()) else {
            return false;
        };
        let markings = document.markings.as_deref().unwrap_or_default();
        !found::touching(markings, &visual.areas(&document.text)).is_empty()
            || self.marking_at_cursor().is_some()
    }

    /// The markings that show in the window.
    pub(super) fn markings_in_view(&self) -> Vec<Marking> {
        let height = self.area.height;
        let Some(document) = self.document() else {
            return Vec::new();
        };
        document
            .markings
            .iter()
            .flatten()
            .filter(|marking| in_view(&document.reader, marking, height))
            .cloned()
            .collect()
    }

    /// Labels the markings in view, to pick one by typing its label.
    fn label_markings(&mut self) {
        self.pending = Pending::new();
        let shown = self
            .markings_in_view()
            .into_iter()
            .map(|marking| (marking.page, marking))
            .collect();
        self.marking_hints = Hints::new(shown, &self.hint_chars);
        if let Some(hints) = &mut self.marking_hints {
            hints.set_texts(|_, marking| marking.text.clone());
        }
        if self.marking_hints.is_none() {
            self.notice = Notice::Info("no markings in view".to_owned());
        }
    }

    /// Picks the marking whose label was typed, as `]h` would go to it.
    pub(super) fn on_marking_hint(&mut self, outcome: HintOutcome<Marking>) {
        match outcome {
            HintOutcome::Wait => return,
            HintOutcome::Cancel => {}
            HintOutcome::NoMatch => self.notice = Notice::Error("no such marking".to_owned()),
            HintOutcome::Picked(hint) => self.pick_marking(&hint.item),
        }
        self.marking_hints = None;
    }

    /// Picks a marking, as `]h` would go to it, without moving.
    pub(super) fn pick_marking(&mut self, picked: &Marking) {
        if let Some(document) = current_document(&mut self.tabs, self.current) {
            let markings = document.markings.as_deref().unwrap_or_default();
            let count = markings.len();
            let index = markings
                .iter()
                .position(|marking| marking == picked)
                .unwrap_or_default();
            document.marking = Some(picked.annotation());
            self.notice = Notice::Info(describe(picked, index, count));
        }
    }

    /// Copies the text of a marking: what a highlight or other marking is
    /// over, or a note's text.
    fn yank_marking(&mut self, marking: &Marking) -> Task<Message> {
        let text = if marking.kind == MarkupKind::Note {
            marking.kept.contents.clone().unwrap_or_default()
        } else {
            marking.text.clone()
        };
        if text.is_empty() {
            self.notice = Notice::Info(format!("the {} has no text", marking.kind.noun()));
            return Task::none();
        }
        let count = text.chars().count();
        self.notice = Notice::Info(format!(
            "copied the {}: {count} character{}",
            marking.kind.noun(),
            if count == 1 { "" } else { "s" }
        ));
        iced::clipboard::write(text)
    }

    /// Opens the prompt to change the note on a marking, with the note it
    /// has now.
    fn ask_to_change(&mut self, marking: &Marking) {
        let Some(document) = self.document() else {
            return;
        };
        self.note_editing = Some((document.id, marking.annotation()));
        self.completing = None;
        self.prompt_pending = Pending::new();
        let now = marking.kept.contents.clone().unwrap_or_default();
        self.prompt = Some((Prompt::Command, format!("note {now}")));
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
            Outcome::Change(index) => {
                if let Some(marking) = list.get(index).cloned() {
                    self.sidebar = None;
                    self.ask_to_change(&marking);
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
        let editing = self.note_editing.take();
        let Some(document) = self.document() else {
            return Task::none();
        };
        if let Some((id, annotation)) = editing.filter(|(id, _)| *id == document.id) {
            let edit = Edit::Renoted {
                annotation,
                before: None,
                after: text,
            };
            return self.start_edit(id, edit, Step::Do);
        }
        let (page, left, top) = match wanted {
            // Over the sentence's first character, so that it stays inside
            // the text when the margins are trimmed. It lets the text
            // through, so what it covers can still be read.
            Some((id, page, area)) if id == document.id => {
                let middle = f32::midpoint(area.top, area.bottom);
                (page, area.left, (middle - NOTE_SIZE / 2.0).max(0.0))
            }
            _ => {
                let place = document.reader.place();
                let height = document
                    .reader
                    .page_size(place.page)
                    .map_or(0.0, |size| size.height);
                // At the text's left edge once the margins are measured, so
                // that trimming them keeps it in view.
                let left = document
                    .contents
                    .as_ref()
                    .and_then(|contents| contents.get(crate::num::index(place.page)))
                    .map_or(NOTE_MARGIN, |content| content.left + NOTE_MARGIN / 2.0);
                (place.page, left, place.down.mul_add(height, NOTE_MARGIN))
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
                    // Kept, so that what redo makes again can be matched to
                    // it.
                    backend.remove(annotations.clone()).await?;
                    Ok(Edit::Marked {
                        markup,
                        annotations,
                    })
                }
                (Edit::Unmarked(markings), Step::Do | Step::Redo) => {
                    let annotations = markings.iter().map(Marking::annotation).collect();
                    Ok(Edit::Unmarked(backend.remove(annotations).await?))
                }
                (
                    Edit::Renoted {
                        annotation, after, ..
                    },
                    Step::Do | Step::Redo,
                ) => {
                    let before = backend.set_note(annotation, after.clone()).await?;
                    Ok(Edit::Renoted {
                        annotation,
                        before,
                        after,
                    })
                }
                (
                    Edit::Renoted {
                        annotation,
                        before,
                        after,
                    },
                    Step::Undo,
                ) => {
                    let text = before.clone().unwrap_or_default();
                    backend.set_note(annotation, text).await?;
                    Ok(Edit::Renoted {
                        annotation,
                        before,
                        after,
                    })
                }
                (
                    Edit::Filled {
                        widget,
                        before,
                        after,
                        tick,
                    },
                    step,
                ) => {
                    let undoing = step == Step::Undo;
                    let value = if undoing { &before } else { &after };
                    let held = backend.fill(widget, value.clone()).await?;
                    // Done afresh, what it held is what undo goes back to.
                    let before = if undoing { before } else { held };
                    Ok(Edit::Filled {
                        widget,
                        before,
                        after,
                        tick,
                    })
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
            (Edit::Renoted { .. }, Step::Do | Step::Redo) => "note changed".to_owned(),
            (Edit::Renoted { .. }, Step::Undo) => "note changed back".to_owned(),
            (Edit::Filled { after, tick, .. }, Step::Do | Step::Redo) => {
                super::forms::filled(after, *tick)
            }
            (Edit::Filled { before, tick, .. }, Step::Undo) => {
                format!("{}, as it was", super::forms::filled(before, *tick))
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
        // What came back under another number is that number now, in every
        // change still to undo or redo, and as the marking gone to.
        let moved = edit.moved(step, &asked);
        for other in document.undo.iter_mut().chain(document.redo.iter_mut()) {
            other.renumber(&moved);
        }
        if let Some(current) = document.marking
            && let Some(&(_, to)) = moved.iter().find(|(from, _)| *from == current)
        {
            document.marking = Some(to);
        }
        let gone = edit.gone(step, &asked);
        if document
            .marking
            .is_some_and(|current| gone.contains(&current))
        {
            document.marking = None;
        }
        if let Edit::Filled {
            widget,
            before,
            after,
            tick,
        } = &edit
        {
            if *tick {
                // Picking a button changes the others of its group.
                document.fields = None;
            } else if let Some((_, field)) = document
                .fields
                .iter_mut()
                .flatten()
                .find(|(_, field)| field.widget == *widget)
            {
                field
                    .value
                    .clone_from(if step == Step::Undo { before } else { after });
            }
        }
        match step {
            Step::Do | Step::Redo => document.undo.push(edit),
            Step::Undo => document.redo.push(edit),
        }
        // Every page is rendered afresh to show the change.
        document.edition = document.edition.saturating_add(1);
        document.markings = None;
        // The fields found with the links hold what they held before.
        document.links.clear();
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
    describe(marking, index, count)
}

/// What the bottom line says of a marking, the one with this index of so
/// many: its kind, its place, and its note.
/// What the status bar says on going to a marking. Its note shows in a
/// band of its own, which has room for all of it.
fn describe(marking: &Marking, index: usize, count: usize) -> String {
    format!(
        "{} {} of {count}",
        marking.kind.noun(),
        index.saturating_add(1)
    )
}

/// The band over the bottom of the pages that shows the whole note on the
/// marking picked, wrapped to fit.
pub(super) fn note_band(note: String, chrome: Colours) -> Element<'static, Message> {
    let band = container(
        text(note)
            .size(LIST_TEXT_SIZE)
            .color(chrome.bar_text)
            .wrapping(text::Wrapping::WordOrGlyph),
    )
    .padding([6, 10])
    .width(Fill)
    .style(move |_| {
        container::background(chrome.list).border(iced::Border {
            color: chrome.list_border,
            width: 1.0,
            radius: 0.0.into(),
        })
    });
    container(band).align_bottom(Fill).clip(true).into()
}

/// Draws the label on each marking in view, just left of its first area,
/// so that it does not hide the marking's colour.
pub(super) fn draw_labels(frame: &mut canvas::Frame, document: &Document, hints: &Hints<Marking>) {
    for hint in hints.shown() {
        draw_marking_label(frame, document, hint.label, &hint.item);
    }
}

/// Draws a label just left of a marking's first line.
pub(super) fn draw_marking_label(
    frame: &mut canvas::Frame,
    document: &Document,
    label: [char; 2],
    marking: &Marking,
) {
    let Some(area) = marking
        .areas
        .first()
        .and_then(|&area| document.reader.area_on_screen(marking.page, area))
    else {
        return;
    };
    let width = label_size().width;
    draw_label(
        frame,
        label,
        iced::Point::new(area.left - width - 2.0, area.top),
    );
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
            "{}-{end} of {}   {}: go   {}: take away   {}: change note",
            top.saturating_add(1),
            list.len(),
            keymaps.markings.first(Binding::Go),
            keymaps.markings.first(Binding::Delete),
            keymaps.markings.first(Binding::Change),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Kept;
    use crate::recolor::Rgb;

    fn marking(xref: i32) -> Marking {
        Marking {
            page: 2,
            xref,
            kind: MarkupKind::Highlight,
            areas: Vec::new(),
            text: String::new(),
            kept: Kept::default(),
        }
    }

    fn marked(annotations: Vec<Annotation>) -> Edit {
        Edit::Marked {
            markup: Markup {
                kind: MarkupKind::Highlight,
                pages: Vec::new(),
                colour: Rgb(0, 0, 0),
                note: None,
            },
            annotations,
        }
    }

    #[test]
    fn a_marking_put_back_is_found_again_by_older_changes() {
        // A highlight made as 10, then taken away; undoing that puts it
        // back as 30.
        let asked = Edit::Unmarked(vec![marking(10)]);
        let now = Edit::Unmarked(vec![marking(30)]);
        let moved = now.moved(Step::Undo, &asked);
        assert_eq!(moved, [((2, 10), (2, 30))]);
        // So undoing the highlight takes away 30.
        let mut older = marked(vec![(2, 10), (2, 11)]);
        older.renumber(&moved);
        let Edit::Marked { annotations, .. } = older else {
            panic!()
        };
        assert_eq!(annotations, [(2, 30), (2, 11)]);
    }

    #[test]
    fn a_marking_made_again_is_found_again_by_newer_changes() {
        let moved = marked(vec![(2, 40)]).moved(Step::Redo, &marked(vec![(2, 10)]));
        assert_eq!(moved, [((2, 10), (2, 40))]);
        let mut newer = Edit::Renoted {
            annotation: (2, 10),
            before: None,
            after: "a note".into(),
        };
        newer.renumber(&moved);
        let Edit::Renoted { annotation, .. } = newer else {
            panic!()
        };
        assert_eq!(annotation, (2, 40));
        // Nothing moves when nothing came back.
        assert_eq!(
            marked(vec![(2, 40)]).moved(Step::Do, &marked(Vec::new())),
            []
        );
    }
}
