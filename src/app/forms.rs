//! Filling in forms: a field picked by its label is ticked, if it is a
//! box; a text field is typed into where it is on the page, and a list
//! opens under its field to pick from.

use iced::widget::canvas;
use iced::{Font, Point, Size, Task};

use super::{App, Document, Message, Notice, rectangle};
use crate::backend::{Annotation, Field, FieldKind, OFF};
use crate::input::{FieldPick, Key, PromptBinding};
use crate::keymap::{Counts, Fed};
use crate::layout::Rect;
use crate::theme::Colours;

use super::markings::{Edit, Step};

/// Which field to go to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FieldGoal {
    /// The first in view, or the next below it.
    First,
    /// The last in view, or the one before it above.
    Last,
    /// The one last gone to in the document.
    Again,
    /// The one after, or before, this one, coming round at the ends.
    Step { from: Annotation, forward: bool },
}

impl From<FieldPick> for FieldGoal {
    fn from(pick: FieldPick) -> Self {
        match pick {
            FieldPick::First => Self::First,
            FieldPick::Last => Self::Last,
            FieldPick::Again => Self::Again,
        }
    }
}

/// The most options a list shows at once.
const LIST_ROWS: usize = 8;

/// A field being filled in, where it is.
pub(super) struct FieldEdit {
    /// The document it is in.
    id: u64,
    field: Field,
    /// Its area on the unturned page, in points.
    area: Rect,
    editor: Editor,
}

/// What is being typed into a field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Editor {
    /// Text, and the cursor: how many characters come before it.
    Text { text: Vec<char>, cursor: usize },
    /// A list to pick from: its options, what is typed to narrow them,
    /// and which of those left is picked.
    Choice {
        options: Vec<String>,
        typed: String,
        selected: usize,
    },
    /// A box or a button, gone to with tab, to tick with a key.
    Toggle,
}

/// What a key comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Typed {
    Stay,
    /// The field is to hold this.
    Done(String),
    Cancel,
    /// The clipboard is to be pasted.
    Paste,
    /// The box is to be ticked or unticked.
    Toggle,
    /// On to the next field, or back to the one before; the field is to
    /// hold this first, if anything.
    Move {
        forward: bool,
        value: Option<String>,
    },
}

impl Editor {
    /// Typing into text, the cursor at its end.
    pub fn text(value: &str) -> Self {
        let text: Vec<char> = value.chars().collect();
        let cursor = text.len();
        Self::Text { text, cursor }
    }

    /// Picking from a list, what the field holds picked.
    pub fn choice(options: Vec<String>, value: &str) -> Self {
        let selected = options
            .iter()
            .position(|option| option == value)
            .unwrap_or(0);
        Self::Choice {
            options,
            typed: String::new(),
            selected,
        }
    }

    /// The options left once narrowed by what is typed.
    pub fn shown(&self) -> Vec<&String> {
        match self {
            Self::Text { .. } | Self::Toggle => Vec::new(),
            Self::Choice { options, typed, .. } => fitting(options, typed),
        }
    }

    /// Takes a key, as the prompt's bindings read it.
    pub fn feed(&mut self, fed: Fed<PromptBinding>) -> Typed {
        use PromptBinding as B;
        let shown = self.shown().len();
        // Tab goes on to the next field, and escape leaves the field, each
        // keeping what is in it.
        if let Fed::Bound(binding @ (B::Complete | B::CompletePrevious), _) = fed {
            return Typed::Move {
                forward: binding == B::Complete,
                value: self.value(),
            };
        }
        if let Fed::Bound(B::Cancel, _) = fed {
            return self.value().map_or(Typed::Cancel, Typed::Done);
        }
        match (self, fed) {
            (Self::Toggle, Fed::Bound(B::Submit, _)) => Typed::Toggle,
            (Self::Toggle, Fed::Unbound(keys))
                if keys
                    .iter()
                    .any(|key| matches!(key, Key::Char(' ') | Key::ShiftSpace)) =>
            {
                Typed::Toggle
            }
            (Self::Toggle, _) => Typed::Stay,
            (_, Fed::Bound(B::Paste, _)) => Typed::Paste,
            (Self::Text { text, cursor }, fed) => edit_text(text, cursor, fed),
            (
                Self::Choice {
                    options,
                    typed,
                    selected,
                },
                fed,
            ) => pick(options, typed, selected, shown, fed),
        }
    }

    /// What the field is to hold: what is typed, or the option picked;
    /// none for a box, or a list narrowed to nothing.
    fn value(&self) -> Option<String> {
        match self {
            Self::Text { text, .. } => Some(text.iter().collect()),
            Self::Choice { .. } => self
                .shown()
                .get(self.selected())
                .map(|option| (*option).clone()),
            Self::Toggle => None,
        }
    }

    /// Which of the options shown is picked.
    const fn selected(&self) -> usize {
        match self {
            Self::Choice { selected, .. } => *selected,
            Self::Text { .. } | Self::Toggle => 0,
        }
    }

    /// Puts pasted text at the cursor, on one line.
    pub fn paste(&mut self, pasted: &str) {
        let mut line = String::new();
        super::append_pasted(&mut line, pasted);
        match self {
            Self::Text { text, cursor } => {
                for character in line.chars() {
                    text.insert(*cursor, character);
                    *cursor = cursor.saturating_add(1);
                }
            }
            Self::Choice {
                typed, selected, ..
            } => {
                typed.push_str(&line);
                *selected = 0;
            }
            Self::Toggle => {}
        }
    }
}

/// Takes a key while typing text.
#[expect(clippy::wildcard_enum_match_arm, reason = "the other keys do nothing here")]
fn edit_text(text: &mut Vec<char>, cursor: &mut usize, fed: Fed<PromptBinding>) -> Typed {
    use PromptBinding as B;
    match fed {
        Fed::Bound(B::Submit, _) => return Typed::Done(text.iter().collect()),
        Fed::Bound(B::RubOut, _) => {
            if let Some(before) = cursor.checked_sub(1) {
                text.remove(before);
                *cursor = before;
            }
        }
        Fed::Bound(B::RubOutWord, _) => {
            let space = |at: usize| text.get(at.saturating_sub(1)).map(|c| c.is_whitespace());
            let mut start = *cursor;
            while start > 0 && space(start) == Some(true) {
                start = start.saturating_sub(1);
            }
            while start > 0 && space(start) == Some(false) {
                start = start.saturating_sub(1);
            }
            text.drain(start..*cursor);
            *cursor = start;
        }
        Fed::Bound(B::ClearLine, _) => {
            text.clear();
            *cursor = 0;
        }
        Fed::Unbound(keys) => {
            for key in keys {
                match key {
                    Key::Char(character) => {
                        text.insert(*cursor, character);
                        *cursor = cursor.saturating_add(1);
                    }
                    Key::ShiftSpace => {
                        text.insert(*cursor, ' ');
                        *cursor = cursor.saturating_add(1);
                    }
                    Key::Left => *cursor = cursor.saturating_sub(1),
                    Key::Right => *cursor = cursor.saturating_add(1).min(text.len()),
                    Key::Home => *cursor = 0,
                    Key::End => *cursor = text.len(),
                    _ => {}
                }
            }
        }
        Fed::Bound(_, _) | Fed::Command(..) | Fed::Waiting => {}
    }
    Typed::Stay
}

/// Takes a key while picking from a list, of which so many are shown.
#[expect(clippy::wildcard_enum_match_arm, reason = "the other keys do nothing here")]
fn pick(
    options: &[String],
    typed: &mut String,
    selected: &mut usize,
    shown: usize,
    fed: Fed<PromptBinding>,
) -> Typed {
    use PromptBinding as B;
    match fed {
        Fed::Bound(B::Submit, _) => {
            return fitting(options, typed)
                .get(*selected)
                .map_or(Typed::Stay, |option| Typed::Done((*option).clone()));
        }
        Fed::Bound(B::RubOut, _) => {
            typed.pop();
            *selected = 0;
        }
        Fed::Bound(B::ClearLine | B::RubOutWord, _) => {
            typed.clear();
            *selected = 0;
        }
        Fed::Bound(B::Newer, _) => {
            *selected = selected.saturating_add(1).checked_rem(shown).unwrap_or(0);
        }
        Fed::Bound(B::Older, _) => {
            *selected = selected
                .checked_sub(1)
                .unwrap_or_else(|| shown.saturating_sub(1));
        }
        Fed::Unbound(keys) => {
            for key in keys {
                match key {
                    Key::Char(character) => typed.push(character),
                    Key::ShiftSpace => typed.push(' '),
                    _ => continue,
                }
                *selected = 0;
            }
        }
        Fed::Bound(_, _) | Fed::Command(..) | Fed::Waiting => {}
    }
    Typed::Stay
}

impl App {
    /// Fills in a field picked by its label, which covers this area of
    /// its page: a box is ticked or unticked, a button of a group picked,
    /// and text or a list is typed into or picked from where it is.
    pub(super) fn fill_field(&mut self, field: Field, area: Rect) -> Task<Message> {
        let Some(id) = self.document().map(|document| document.id) else {
            return Task::none();
        };
        let after = match &field.kind {
            FieldKind::Checkbox { on } if field.value == *on => OFF.to_owned(),
            FieldKind::Radio { on } if field.value == *on => {
                self.notice = Notice::Info(format!("{} is picked already", field.name));
                return Task::none();
            }
            FieldKind::Checkbox { on } | FieldKind::Radio { on } => on.clone(),
            FieldKind::Text => {
                self.start_field(id, Editor::text(&field.value), field, area);
                return Task::none();
            }
            FieldKind::Choice(options) => {
                let editor = Editor::choice(options.clone(), &field.value);
                self.start_field(id, editor, field, area);
                return Task::none();
            }
        };
        if let Some(document) = self.document_mut() {
            document.last_field = Some(field.widget);
        }
        let edit = Edit::Filled {
            widget: field.widget,
            before: field.value,
            after,
            tick: true,
        };
        self.start_edit(id, edit, Step::Do)
    }

    fn start_field(&mut self, id: u64, editor: Editor, field: Field, area: Rect) {
        self.prompt_pending = crate::keymap::Pending::new();
        // The field is shown, so that what is typed shows where it goes.
        if let Some(document) = self.document_mut() {
            document.reader.reveal(field.widget.0, area);
            document.last_field = Some(field.widget);
        }
        self.field_edit = Some(FieldEdit {
            id,
            field,
            area,
            editor,
        });
    }

    /// Takes a key while a field is filled in.
    pub(super) fn on_field_key(&mut self, key: Key) -> Task<Message> {
        let fed = self
            .prompt_pending
            .feed(key, &self.keymaps.prompt, Counts::NotTaken);
        let Some(edit) = &mut self.field_edit else {
            return Task::none();
        };
        match edit.editor.feed(fed) {
            Typed::Stay => Task::none(),
            Typed::Paste => iced::clipboard::read().map(Message::Pasted),
            Typed::Cancel => {
                self.field_edit = None;
                Task::none()
            }
            Typed::Toggle => self.toggle_focused(),
            Typed::Done(value) => {
                let Some(FieldEdit { id, field, .. }) = self.field_edit.take() else {
                    return Task::none();
                };
                self.put_in(id, field, value)
            }
            Typed::Move { forward, value } => {
                let Some(FieldEdit { id, field, .. }) = self.field_edit.take() else {
                    return Task::none();
                };
                let from = field.widget;
                let kept = value.map_or_else(Task::none, |value| self.put_in(id, field, value));
                Task::batch([kept, self.go_to_field(FieldGoal::Step { from, forward })])
            }
        }
    }

    /// Fills in a field with what was typed or picked, if that is new.
    fn put_in(&mut self, id: u64, field: Field, value: String) -> Task<Message> {
        if value == field.value {
            return Task::none();
        }
        let edit = Edit::Filled {
            widget: field.widget,
            before: field.value,
            after: value,
            tick: false,
        };
        self.start_edit(id, edit, Step::Do)
    }

    /// Ticks or unticks the box gone to, which stays gone to.
    fn toggle_focused(&mut self) -> Task<Message> {
        let Some(edit) = &mut self.field_edit else {
            return Task::none();
        };
        let field = &mut edit.field;
        let after = match &field.kind {
            FieldKind::Checkbox { on } if field.value == *on => OFF.to_owned(),
            FieldKind::Checkbox { on } | FieldKind::Radio { on } if field.value != *on => {
                on.clone()
            }
            FieldKind::Text
            | FieldKind::Choice(_)
            | FieldKind::Checkbox { .. }
            | FieldKind::Radio { .. } => return Task::none(),
        };
        let filled = Edit::Filled {
            widget: field.widget,
            before: std::mem::replace(&mut field.value, after.clone()),
            after,
            tick: true,
        };
        let id = edit.id;
        self.start_edit(id, filled, Step::Do)
    }

    /// Goes to a field of a form, to fill it in: the first in view, or
    /// the one after or before another. The fields are found first, when
    /// they have not been.
    pub(super) fn go_to_field(&mut self, goal: FieldGoal) -> Task<Message> {
        self.pending = crate::keymap::Pending::new();
        let Some(document) = self.document() else {
            return Task::none();
        };
        let id = document.id;
        let Some(fields) = &document.fields else {
            let found = document.backend.fields();
            self.fields_wanted = Some((id, goal));
            return Task::perform(found, move |result| Message::FieldsFound(id, result));
        };
        if fields.is_empty() {
            self.notice = Notice::Info("no fields to fill in".to_owned());
            return Task::none();
        }
        let Some(index) = field_index(
            fields,
            &document.reader,
            self.area.height,
            goal,
            document.last_field,
        ) else {
            self.notice = Notice::Info("no field gone to yet in this document".to_owned());
            return Task::none();
        };
        let Some((area, field)) = fields.get(index).cloned() else {
            return Task::none();
        };
        let editor = match &field.kind {
            FieldKind::Text => Editor::text(&field.value),
            FieldKind::Choice(options) => Editor::choice(options.clone(), &field.value),
            FieldKind::Checkbox { .. } | FieldKind::Radio { .. } => Editor::Toggle,
        };
        self.start_field(id, editor, field, area);
        Task::none()
    }

    /// Keeps the fields found in a document, and goes to the one wanted.
    pub(super) fn on_fields(
        &mut self,
        id: u64,
        result: Result<Vec<(Rect, Field)>, crate::backend::Error>,
    ) -> Task<Message> {
        let wanted = self
            .fields_wanted
            .take()
            .filter(|(wanted, _)| *wanted == id);
        let Some(document) = super::document_by_id(&mut self.tabs, id) else {
            return Task::none();
        };
        match result {
            Ok(fields) => document.fields = Some(fields),
            Err(error) => {
                self.notice = Notice::Error(error.to_string());
                return Task::none();
            }
        }
        match wanted {
            Some((_, goal)) if self.document().is_some_and(|document| document.id == id) => {
                self.go_to_field(goal)
            }
            _ => Task::none(),
        }
    }

    /// Pastes into the field being filled in; whether one was.
    pub(super) fn paste_into_field(&mut self, text: &str) -> bool {
        let Some(edit) = &mut self.field_edit else {
            return false;
        };
        edit.editor.paste(text);
        true
    }

    /// What the bottom line says while a field is filled in.
    pub(super) fn field_status(&self) -> Option<String> {
        let edit = self.field_edit.as_ref()?;
        let key = |binding| self.keymaps.prompt.first(binding);
        let how = match edit.editor {
            Editor::Text { .. } => format!("{} fills it in", key(PromptBinding::Submit)),
            Editor::Choice { .. } => format!(
                "type to narrow, {}/{} to move, {} picks",
                key(PromptBinding::Newer),
                key(PromptBinding::Older),
                key(PromptBinding::Submit),
            ),
            Editor::Toggle => format!("space or {} ticks it", key(PromptBinding::Submit)),
        };
        let how = format!(
            "{how}, {}/{} to the next/previous field, {} leaves it as it is",
            key(PromptBinding::Complete),
            key(PromptBinding::CompletePrevious),
            key(PromptBinding::Cancel),
        );
        Some(format!("{}    {how}", edit.field.name))
    }
}

/// Which of the fields, in reading order, a goal comes to, with the view
/// in a window this high and the field last gone to; none when there is
/// none to go back to.
fn field_index(
    fields: &[(Rect, Field)],
    reader: &crate::reader::Reader,
    height: f32,
    goal: FieldGoal,
    last: Option<Annotation>,
) -> Option<usize> {
    let on_screen = |(area, field): &(Rect, Field)| reader.area_on_screen(field.widget.0, *area);
    let at = |widget: Annotation| fields.iter().position(|(_, field)| field.widget == widget);
    let count = fields.len();
    match goal {
        // Not above the view: in it, or else the next below it.
        FieldGoal::First => Some(
            fields
                .iter()
                .position(|field| on_screen(field).is_some_and(|area| area.bottom > 0.0))
                .unwrap_or(0),
        ),
        // Not below the view: in it, or else the one before above it.
        FieldGoal::Last => Some(
            fields
                .iter()
                .rposition(|field| on_screen(field).is_some_and(|area| area.top < height))
                .unwrap_or_else(|| count.saturating_sub(1)),
        ),
        FieldGoal::Again => last.and_then(at),
        FieldGoal::Step { from, forward } => Some(at(from).map_or(0, |index| {
            if forward {
                index.saturating_add(1).checked_rem(count).unwrap_or(0)
            } else {
                index
                    .checked_sub(1)
                    .unwrap_or_else(|| count.saturating_sub(1))
            }
        })),
    }
}

/// Draws a field being filled in over the field on the page: its text and
/// the cursor, or what is typed and, under it, the list to pick from.
pub(super) fn draw(
    frame: &mut canvas::Frame,
    document: &Document,
    edit: &FieldEdit,
    chrome: Colours,
) {
    if document.id != edit.id {
        return;
    }
    let Some(area) = document
        .reader
        .area_on_screen(edit.field.widget.0, edit.area)
    else {
        return;
    };
    let field = rectangle(area);
    let stroke = canvas::Stroke::default()
        .with_color(super::CURSOR)
        .with_width(2.0);
    // A box keeps its look, framed.
    if edit.editor == Editor::Toggle {
        frame.stroke(
            &canvas::Path::rectangle(field.position(), field.size()),
            stroke,
        );
        return;
    }
    let size = (field.height * 0.62).clamp(8.0, 28.0);
    // A monospaced font, so that the cursor goes where the letters are.
    let width = size * 0.6;
    let pad = (size * 0.25).max(2.0);
    frame.fill_rectangle(field.position(), field.size(), chrome.list);
    let fits = crate::num::index(crate::num::round(
        ((field.width - pad * 2.0) / width).floor().max(1.0),
    ));
    let (text, cursor): (Vec<char>, usize) = match &edit.editor {
        Editor::Text { text, cursor } => (text.clone(), *cursor),
        Editor::Choice { typed, .. } => {
            let text: Vec<char> = typed.chars().collect();
            let cursor = text.len();
            (text, cursor)
        }
        Editor::Toggle => (Vec::new(), 0),
    };
    // The text scrolls along so that the cursor stays in the field.
    let first = cursor.saturating_sub(fits);
    let shown: String = text.iter().skip(first).take(fits).collect();
    let middle = field.y + field.height / 2.0;
    write(
        frame,
        shown,
        Point::new(field.x + pad, middle),
        size,
        chrome.bar_text,
    );
    let before = crate::num::float(crate::num::len(cursor.saturating_sub(first)));
    frame.fill_rectangle(
        Point::new(field.x + pad + width * before, middle - size * 0.6),
        Size::new(2.0, size * 1.2),
        chrome.bar_text,
    );
    frame.stroke(
        &canvas::Path::rectangle(field.position(), field.size()),
        stroke,
    );
    if let Editor::Choice { selected, .. } = &edit.editor {
        draw_list(frame, field, &edit.editor.shown(), *selected, size, chrome);
    }
}

/// Draws the options left under the field, or over it when there is no
/// room under it, as many as fit around the one picked.
fn draw_list(
    frame: &mut canvas::Frame,
    field: iced::Rectangle,
    shown: &[&String],
    selected: usize,
    size: f32,
    chrome: Colours,
) {
    let row = (size * 1.5).max(16.0);
    let rows = shown.len().clamp(1, LIST_ROWS);
    let height = row * crate::num::float(crate::num::len(rows));
    let top = if field.y + field.height + height <= frame.height() {
        field.y + field.height
    } else {
        (field.y - height).max(0.0)
    };
    let list = iced::Rectangle {
        x: field.x,
        y: top,
        width: field.width.max(160.0),
        height,
    };
    frame.fill_rectangle(list.position(), list.size(), chrome.list);
    let pad = (size * 0.25).max(2.0);
    if shown.is_empty() {
        write(
            frame,
            "no option fits".to_owned(),
            Point::new(list.x + pad, top + row / 2.0),
            size,
            chrome.list_number,
        );
    }
    let first = selected.saturating_add(1).saturating_sub(rows);
    for (offset, option) in shown.iter().enumerate().skip(first).take(rows) {
        let y = top + row * crate::num::float(crate::num::len(offset.saturating_sub(first)));
        if offset == selected {
            frame.fill_rectangle(
                Point::new(list.x, y),
                Size::new(list.width, row),
                chrome.tab_current,
            );
        }
        write(
            frame,
            (*option).clone(),
            Point::new(list.x + pad, y + row / 2.0),
            size,
            chrome.bar_text,
        );
    }
    let stroke = canvas::Stroke::default()
        .with_color(chrome.list_border)
        .with_width(1.0);
    frame.stroke(
        &canvas::Path::rectangle(list.position(), list.size()),
        stroke,
    );
}

/// Writes a line of text, starting at a point on its left and centred on
/// it from top to bottom.
fn write(frame: &mut canvas::Frame, content: String, at: Point, size: f32, colour: iced::Color) {
    frame.fill_text(canvas::Text {
        content,
        position: at,
        color: colour,
        size: size.into(),
        font: Font::MONOSPACE,
        align_x: iced::widget::text::Alignment::Left,
        align_y: iced::alignment::Vertical::Center,
        ..canvas::Text::default()
    });
}

/// The options that start as what is typed, whatever the case; when none
/// do, those that have it anywhere.
fn fitting<'a>(options: &'a [String], typed: &str) -> Vec<&'a String> {
    let typed = typed.to_lowercase();
    let starting: Vec<&String> = options
        .iter()
        .filter(|option| option.to_lowercase().starts_with(&typed))
        .collect();
    if !starting.is_empty() {
        return starting;
    }
    options
        .iter()
        .filter(|option| option.to_lowercase().contains(&typed))
        .collect()
}

/// What the bottom line says once a field holds a value: a box or a
/// button is ticked or not, and anything else holds what was typed.
pub(super) fn filled(value: &str, tick: bool) -> String {
    match (value, tick) {
        (OFF, true) => "unticked".to_owned(),
        (_, true) => "ticked".to_owned(),
        ("", false) => "field emptied".to_owned(),
        (_, false) => format!("filled in: {value}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(editor: &mut Editor, typed: &[Key]) {
        for key in typed {
            editor.feed(Fed::Unbound(vec![*key]));
        }
    }

    fn bound(editor: &mut Editor, binding: PromptBinding) -> Typed {
        editor.feed(Fed::Bound(binding, None))
    }

    fn done(value: &str) -> Typed {
        Typed::Done(value.to_owned())
    }

    #[test]
    fn text_is_typed_where_the_cursor_is() {
        let mut editor = Editor::text("Ann");
        keys(
            &mut editor,
            &[Key::Home, Key::Char('J'), Key::End, Key::Char('e')],
        );
        assert_eq!(bound(&mut editor, PromptBinding::Submit), done("JAnne"));
        keys(&mut editor, &[Key::Left, Key::Left]);
        bound(&mut editor, PromptBinding::RubOut);
        assert_eq!(bound(&mut editor, PromptBinding::Submit), done("JAne"));
        // Pasted lines become one, as at the prompt.
        editor.paste("Mary\nLou");
        assert_eq!(
            bound(&mut editor, PromptBinding::Submit),
            done("JAMary Loune")
        );
        bound(&mut editor, PromptBinding::RubOutWord);
        assert_eq!(bound(&mut editor, PromptBinding::Submit), done("JAMary ne"));
        // Escape keeps what is typed, as enter does.
        assert_eq!(bound(&mut editor, PromptBinding::Cancel), done("JAMary ne"));
        assert_eq!(
            bound(&mut Editor::Toggle, PromptBinding::Cancel),
            Typed::Cancel
        );
    }

    fn countries() -> Vec<String> {
        ["Netherlands", "Germany", "New Zealand", "Sweden"]
            .iter()
            .map(|option| (*option).to_owned())
            .collect()
    }

    #[test]
    fn a_list_starts_at_what_the_field_holds_and_typing_narrows_it() {
        let mut editor = Editor::choice(countries(), "New Zealand");
        assert_eq!(
            bound(&mut editor, PromptBinding::Submit),
            done("New Zealand")
        );
        bound(&mut editor, PromptBinding::Newer);
        assert_eq!(bound(&mut editor, PromptBinding::Submit), done("Sweden"));
        // Going on from the last comes round to the first.
        bound(&mut editor, PromptBinding::Newer);
        assert_eq!(
            bound(&mut editor, PromptBinding::Submit),
            done("Netherlands")
        );
        keys(&mut editor, &[Key::Char('n'), Key::Char('e')]);
        assert_eq!(editor.shown(), ["Netherlands", "New Zealand"]);
        bound(&mut editor, PromptBinding::Older);
        assert_eq!(
            bound(&mut editor, PromptBinding::Submit),
            done("New Zealand")
        );
        keys(&mut editor, &[Key::Char('x')]);
        assert!(editor.shown().is_empty());
        assert_eq!(bound(&mut editor, PromptBinding::Submit), Typed::Stay);
        bound(&mut editor, PromptBinding::RubOut);
        assert_eq!(editor.shown().len(), 2);
    }

    #[test]
    fn tab_goes_on_keeping_what_is_in_the_field() {
        let mut text = Editor::text("Ann");
        keys(&mut text, &[Key::Char('e')]);
        assert_eq!(
            bound(&mut text, PromptBinding::Complete),
            Typed::Move {
                forward: true,
                value: Some("Anne".to_owned())
            }
        );
        let mut list = Editor::choice(countries(), "Germany");
        bound(&mut list, PromptBinding::Newer);
        assert_eq!(
            bound(&mut list, PromptBinding::CompletePrevious),
            Typed::Move {
                forward: false,
                value: Some("New Zealand".to_owned())
            }
        );
        let mut tick = Editor::Toggle;
        assert_eq!(keys_last(&mut tick, Key::Char(' ')), Typed::Toggle);
        assert_eq!(bound(&mut tick, PromptBinding::Submit), Typed::Toggle);
        assert_eq!(keys_last(&mut tick, Key::Char('x')), Typed::Stay);
        assert_eq!(
            bound(&mut tick, PromptBinding::Complete),
            Typed::Move {
                forward: true,
                value: None
            }
        );
    }

    fn keys_last(editor: &mut Editor, key: Key) -> Typed {
        editor.feed(Fed::Unbound(vec![key]))
    }

    #[test]
    fn options_that_start_as_typed_come_before_those_that_have_it() {
        let options = countries();
        assert_eq!(fitting(&options, "ne"), ["Netherlands", "New Zealand"]);
        assert_eq!(fitting(&options, "land"), ["Netherlands", "New Zealand"]);
        assert_eq!(fitting(&options, "").len(), 4);
        assert!(fitting(&options, "xyz").is_empty());
    }
}

/// Filling in a form in a running app: picking a field by key, typing,
/// ticking and picking from a list, and leaving a field as it was.
#[cfg(test)]
mod sessions {
    use iced::futures::executor::block_on;

    use crate::app::testing::Session;

    /// What each field of the form holds, by name.
    fn values(session: &Session) -> Vec<(String, String)> {
        let document = session.app.document().unwrap();
        block_on(document.backend.fields())
            .unwrap()
            .into_iter()
            .map(|(_, field)| (field.name, field.value))
            .collect()
    }

    fn value(session: &Session, name: &str) -> String {
        values(session)
            .into_iter()
            .find(|(field, _)| field == name)
            .map(|(_, value)| value)
            .unwrap()
    }

    #[test]
    fn a_text_field_is_typed_into_and_kept() {
        let mut session = Session::open("form.pdf");
        session.keys("gi");
        assert!(session.app.field_edit.is_some());
        session.keys("<BS><BS><BS>Bob<Enter>");
        assert!(session.app.field_edit.is_none());
        assert_eq!(value(&session, "Your name"), "Bob");
        assert!(session.app.document().unwrap().modified());
    }

    #[test]
    fn tab_goes_on_to_the_box_and_space_ticks_it() {
        let mut session = Session::open("form.pdf");
        let before = value(&session, "agree");
        session.keys("gi<Tab><Space>");
        assert_ne!(value(&session, "agree"), before);
        session.keys("<Space>");
        assert_eq!(value(&session, "agree"), before);
    }

    #[test]
    fn a_list_is_narrowed_by_typing_and_picked_from() {
        let mut session = Session::open("form.pdf");
        assert_eq!(value(&session, "country"), "Netherlands");
        // Narrowed to nothing, enter picks nothing.
        session.keys("gIxyz<Enter>");
        assert!(session.app.field_edit.is_some());
        // The options are the values the form keeps, not what it shows.
        session.keys("<C-u>de<Enter>");
        assert!(session.app.field_edit.is_none());
        assert_eq!(value(&session, "country"), "de");
    }

    #[test]
    fn escape_keeps_what_is_typed_and_undo_takes_it_back() {
        let mut session = Session::open("form.pdf");
        session.keys("giZoe<Esc>");
        assert!(session.app.field_edit.is_none());
        assert_eq!(value(&session, "Your name"), "AnnZoe");
        session.keys("u");
        assert_eq!(value(&session, "Your name"), "Ann");
    }
}
