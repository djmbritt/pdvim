//! Tabs beyond going between them: closing all but one, moving one, and
//! opening again those closed, as Tridactyl's `:tabonly`, `:tabmove` and
//! `u`.

use std::path::PathBuf;

use iced::Task;

use super::{App, Message, Notice, Tab, info, tilde};
use crate::backend;
use crate::reader::Place;
use crate::command::TabPlace;
use crate::picker::{Choice, Entry, Picker};

/// How many closed tabs are kept to open again.
const CLOSED_KEPT: usize = 20;

impl App {
    /// Remembers a tab being closed, by its place, its document's file
    /// and where it was in the document, to open again.
    pub(super) fn closing(&mut self, index: usize, path: PathBuf, place: Place) {
        self.closed.push((index, path, place));
        let over = self.closed.len().saturating_sub(CLOSED_KEPT);
        self.closed.drain(..over);
    }

    /// Opens the tabs closed last again, the last first, each in the place
    /// it had and where it was left in its document; in a tab of its own
    /// even when the document is open in another.
    pub(super) fn reopen_tabs(&mut self, times: u32) -> Task<Message> {
        let mut tasks = Vec::new();
        for _ in 0..times {
            let Some((index, path, place)) = self.closed.pop() else {
                if tasks.is_empty() {
                    self.notice = Notice::Error("no closed tabs to open again".to_owned());
                }
                break;
            };
            if path.is_file() {
                tasks.push(self.open_new_tab(path, index, place));
            }
        }
        Task::batch(tasks)
    }

    /// Opens a document in a new tab at a place among the tabs, and goes
    /// to a place in it, whether or not it is open in another tab already.
    pub(super) fn open_new_tab(&mut self, path: PathBuf, index: usize, place: Place) -> Task<Message> {
        let request = self.next_id();
        let index = index.min(self.tabs.len());
        self.tabs.insert(
            index,
            Tab {
                document: None,
                opening: Some((request, path.clone())),
            },
        );
        self.current = index;
        self.arriving = Some((path.clone(), place));
        self.notice = Notice::Info(format!("opening {}", tilde(&path)));
        Task::perform(
            backend::open(path.clone(), None, self.flow()),
            move |result| Message::Opened(request, path, result),
        )
    }

    /// Lists the open tabs, each with its number, name and file, the one in
    /// view selected; typing narrows the list.
    pub(super) fn show_tab_list(&mut self) {
        self.pending = crate::keymap::Pending::new();
        let entries = self
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                let path = tab
                    .document
                    .as_ref()
                    .map(|document| &document.path)
                    .or_else(|| tab.opening.as_ref().map(|(_, path)| path));
                let name = info::tab_name(&tab.name());
                let number = index.saturating_add(1);
                let file = path.map(|path| format!("  {}", tilde(path)));
                let text = format!("{number} {name}{}", file.unwrap_or_default());
                Entry { text, value: index }
            })
            .collect();
        self.tab_list = Picker::new(entries);
        if let Some(list) = &mut self.tab_list {
            list.select(self.current);
        }
    }

    /// Moves through the list of tabs, narrows it, or goes to a tab.
    pub(super) fn on_tab_list_key(&mut self, key: crate::input::Key) {
        let Some(list) = &mut self.tab_list else {
            return;
        };
        match list.feed(key, &self.keymaps.recent) {
            Some(Choice::Open(index)) => {
                self.tab_list = None;
                self.go_to_tab(index);
            }
            Some(Choice::Close) => self.tab_list = None,
            None => {}
        }
    }

    /// Moves the tab in view, keeping it in view.
    pub(super) fn move_tab(&mut self, place: TabPlace) {
        let last = self.tabs.len().saturating_sub(1);
        let to = match place {
            TabPlace::At(index) => index.min(last),
            TabPlace::Last => last,
            TabPlace::By(by) => {
                let current = i64::try_from(self.current).unwrap_or(0);
                let to = current.saturating_add(by).max(0);
                usize::try_from(to).unwrap_or(last).min(last)
            }
        };
        if to == self.current || self.current > last {
            return;
        }
        let tab = self.tabs.remove(self.current);
        self.tabs.insert(to, tab);
        self.current = to;
    }

    /// Closes every tab but the one in view. Tabs whose documents have
    /// changes not saved stay, unless forced.
    pub(super) fn only_tab(&mut self, force: bool) {
        let unsaved = self
            .tabs
            .iter()
            .enumerate()
            .filter(|(index, tab)| {
                *index != self.current
                    && tab.document.as_ref().is_some_and(super::Document::modified)
            })
            .count();
        if unsaved > 0 && !force {
            let them = if unsaved == 1 {
                "a tab has".to_owned()
            } else {
                format!("{unsaved} tabs have")
            };
            self.notice = Notice::Error(format!(
                "{them} unsaved highlights: :w them, or :tabonly! to close anyway"
            ));
            return;
        }
        let tabs = std::mem::take(&mut self.tabs);
        let current = self.current;
        for (index, tab) in tabs.into_iter().enumerate() {
            if index == current {
                self.tabs.push(tab);
            } else if let Some(document) = tab.document {
                self.remember(&document);
                let place = document.reader.place();
                self.closing(index, document.path, place);
            }
        }
        self.current = 0;
    }
}
