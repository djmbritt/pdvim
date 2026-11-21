//! Filling in forms: a field picked by its label is ticked, if it is a
//! box; a text field is typed into where it is on the page, and a list
//! opens under its field to pick from.

use iced::widget::canvas;
use iced::{Font, Point, Size, Task};

use super::{App, Document, Message, Notice, rectangle};
use crate::backend::{Field, FieldKind, OFF};
use crate::input::{Key, PromptBinding};
use crate::keymap::{Counts, Fed};
use crate::layout::Rect;
use crate::theme::Colours;

use super::markings::{Edit, Step};

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
            Self::Text { .. } => Vec::new(),
            Self::Choice { options, typed, .. } => fitting(options, typed),
        }
    }

    /// Takes a key, as the prompt's bindings read it.
    pub fn feed(&mut self, fed: Fed<PromptBinding>) -> Typed {
        use PromptBinding as B;
        let shown = self.shown().len();
        match (self, fed) {
            (_, Fed::Bound(B::Cancel, _)) => Typed::Cancel,
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
        }
    }
}

/// Takes a key while typing text.
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
        Fed::Bound(_, _) | Fed::Waiting => {}
    }
    Typed::Stay
}

/// Takes a key while picking from a list, of which so many are shown.
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
        Fed::Bound(B::Newer | B::Complete, _) => {
            *selected = selected.saturating_add(1).checked_rem(shown).unwrap_or(0);
        }
        Fed::Bound(B::Older | B::CompletePrevious, _) => {
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
        Fed::Bound(_, _) | Fed::Waiting => {}
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
            Typed::Done(value) => {
                let Some(FieldEdit { id, field, .. }) = self.field_edit.take() else {
                    return Task::none();
                };
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
            Editor::Text { .. } => format!(
                "{} fills it in, {} leaves it as it was",
                key(PromptBinding::Submit),
                key(PromptBinding::Cancel),
            ),
            Editor::Choice { .. } => format!(
                "type to narrow, {}/{} to move, {} picks, {} leaves it",
                key(PromptBinding::Newer),
                key(PromptBinding::Older),
                key(PromptBinding::Submit),
                key(PromptBinding::Cancel),
            ),
        };
        Some(format!("-- FIELD -- {}    {how}", edit.field.name))
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
    let stroke = canvas::Stroke::default()
        .with_color(super::CURSOR)
        .with_width(2.0);
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
        assert_eq!(bound(&mut editor, PromptBinding::Cancel), Typed::Cancel);
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
    fn options_that_start_as_typed_come_before_those_that_have_it() {
        let options = countries();
        assert_eq!(fitting(&options, "ne"), ["Netherlands", "New Zealand"]);
        assert_eq!(fitting(&options, "land"), ["Netherlands", "New Zealand"]);
        assert_eq!(fitting(&options, "").len(), 4);
        assert!(fitting(&options, "xyz").is_empty());
    }
}
