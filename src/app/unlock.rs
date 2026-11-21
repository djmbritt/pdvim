//! Asking for the password of a locked document, and opening it with it.

use std::path::{Path, PathBuf};

use iced::Task;

use super::{App, Message, Notice, Pending, Prompt, Tab, tilde};
use crate::backend;

impl App {
    /// Asks for the password of a document that a request is opening, once
    /// the documents asked about before it are done with.
    pub(super) fn ask_password(&mut self, request: u64, path: PathBuf, wrong: bool) {
        self.locked.push_back((request, path, wrong));
        if self.locked.len() == 1 {
            self.prompt_for_password();
        }
    }

    /// Opens the prompt for the first document waiting for a password.
    fn prompt_for_password(&mut self) {
        if self.locked.is_empty() {
            return;
        }
        self.completing = None;
        self.prompt_pending = Pending::new();
        self.prompt = Some((Prompt::Password, String::new()));
    }

    /// What the bottom line says while a password is typed: whose it is,
    /// and a dot for each character typed, which are not shown.
    pub(super) fn password_prompt(&self, typed: &str) -> String {
        let (name, wrong) = self
            .locked
            .front()
            .map(|(_, path, wrong)| (file_name(path), *wrong))
            .unwrap_or_default();
        let dots = "\u{2022}".repeat(typed.chars().count());
        let ask = if wrong {
            "wrong password; try again for"
        } else {
            "password for"
        };
        format!("{ask} {name}: {dots}\u{2588}")
    }

    /// Opens the first document waiting for a password with the one typed.
    pub(super) fn unlock(&mut self, password: String) -> Task<Message> {
        let Some((request, path, _)) = self.locked.pop_front() else {
            return Task::none();
        };
        self.notice = Notice::Info(format!("opening {}", tilde(&path)));
        let opening = backend::open(path.clone(), Some(password), self.flow_for(&path));
        // The next locked document is asked about meanwhile; this one comes
        // back to the end of the line if the password is wrong.
        self.prompt_for_password();
        Task::perform(opening, move |result| {
            Message::Opened(request, path, result)
        })
    }

    /// Leaves a prompt without running it. Leaving the password prompt
    /// gives up opening that document.
    pub(super) fn left_prompt(&mut self, prompt: Prompt) -> Task<Message> {
        // A note asked for from visual mode is given up with its prompt.
        self.note_at = None;
        self.note_editing = None;
        if prompt != Prompt::Password {
            return Task::none();
        }
        let Some((request, path, _)) = self.locked.pop_front() else {
            return Task::none();
        };
        self.abandon_opening(request);
        self.notice = Notice::Info(format!("{} not opened: it is locked", file_name(&path)));
        self.prompt_for_password();
        Task::none()
    }
}

impl App {
    /// Deals with a document that did not open: asks for its password if
    /// it is locked, and otherwise says why.
    pub(super) fn not_opened(&mut self, request: u64, path: PathBuf, error: backend::Error) {
        match error {
            backend::Error::Encrypted(_) => self.ask_password(request, path, false),
            backend::Error::WrongPassword(_) => self.ask_password(request, path, true),
            error => {
                self.notice = Notice::Error(error.to_string());
                self.abandon_opening(request);
            }
        }
    }

    /// Stops waiting for the document a request was opening, and closes the
    /// tab opened for it alone.
    fn abandon_opening(&mut self, request: u64) {
        let Some(index) = self
            .tabs
            .iter()
            .position(|tab| tab.opening.as_ref().is_some_and(|(id, _)| *id == request))
        else {
            return;
        };
        if let Some(tab) = self.tabs.get_mut(index) {
            tab.opening = None;
        }
        if self.tabs.len() > 1 && self.tabs.get(index).is_some_and(Tab::is_empty) {
            self.remove_tab(index);
        }
    }
}

/// The name of a file, for saying which document is meant.
fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(|| tilde(path), |name| name.to_string_lossy().into_owned())
}
