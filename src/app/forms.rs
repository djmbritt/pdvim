//! Filling in forms: a field picked by its label is ticked, if it is a
//! box, or else its value is typed at the prompt, after its name.

use iced::Task;

use super::{App, Message, Notice, Prompt};
use crate::backend::{Field, FieldKind, OFF};
use crate::complete::{Candidate, Completion};
use crate::keymap::Pending;

use super::markings::{Edit, Step};

impl App {
    /// Fills in a field picked by its label: a box is ticked or unticked,
    /// a button of a group picked, and for anything else the prompt opens
    /// with what the field holds.
    pub(super) fn fill_field(&mut self, field: Field) -> Task<Message> {
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
            FieldKind::Text | FieldKind::Choice(_) => {
                self.completing = None;
                self.prompt_pending = Pending::new();
                self.prompt = Some((Prompt::Field, field.value.clone()));
                self.field_editing = Some((id, field));
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

    /// Puts what was typed at the prompt in the field it was typed for.
    pub(super) fn submit_field(&mut self, value: &str) -> Task<Message> {
        let Some((id, field)) = self.field_editing.take() else {
            return Task::none();
        };
        if value == field.value {
            return Task::none();
        }
        let edit = Edit::Filled {
            widget: field.widget,
            before: field.value,
            after: value.to_owned(),
            tick: false,
        };
        self.start_edit(id, edit, Step::Do)
    }

    /// What the bottom line shows while typing at a prompt: what is typed,
    /// after the prompt's sigil, or a field's name.
    pub(super) fn prompt_line(&self, prompt: Prompt, line: &str) -> String {
        let lead = match (&self.field_editing, prompt) {
            (Some((_, field)), Prompt::Field) => format!("{}: ", field.name),
            _ => prompt.sigil().to_string(),
        };
        format!("{lead}{line}\u{2588}")
    }

    /// The values the field being filled in offers that fit what is typed,
    /// to complete it with.
    pub(super) fn field_options(&self, typed: &str) -> Completion {
        let options: &[String] = match self.field_editing.as_ref().map(|(_, field)| &field.kind) {
            Some(FieldKind::Choice(options)) => options,
            _ => &[],
        };
        Completion {
            start: 0,
            candidates: fitting(options, typed),
        }
    }
}

/// The options that start as what is typed, whatever the case; when none
/// do, those that have it anywhere.
fn fitting(options: &[String], typed: &str) -> Vec<Candidate> {
    let typed = typed.to_lowercase();
    let candidate = |option: &String| Candidate {
        text: option.clone(),
        label: option.clone(),
        // No space goes after a value.
        whole: false,
    };
    let starting: Vec<Candidate> = options
        .iter()
        .filter(|option| option.to_lowercase().starts_with(&typed))
        .map(candidate)
        .collect();
    if !starting.is_empty() {
        return starting;
    }
    options
        .iter()
        .filter(|option| option.to_lowercase().contains(&typed))
        .map(candidate)
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

    #[test]
    fn options_that_start_as_typed_come_before_those_that_have_it() {
        let options: Vec<String> = ["Netherlands", "Germany", "New Zealand", "Sweden"]
            .iter()
            .map(|option| (*option).to_owned())
            .collect();
        let labels = |typed| -> Vec<String> {
            fitting(&options, typed)
                .into_iter()
                .map(|candidate| candidate.label)
                .collect()
        };
        assert_eq!(labels("ne"), ["Netherlands", "New Zealand"]);
        assert_eq!(labels("land"), ["Netherlands", "New Zealand"]);
        assert_eq!(labels(""), options);
        assert!(labels("xyz").is_empty());
    }
}
