//! Tabs beyond going between them: closing all but one, moving one, and
//! opening again those closed, as Tridactyl's `:tabonly`, `:tabmove` and
//! `u`.

use std::path::PathBuf;

use iced::Task;

use super::{App, Message, Notice};
use crate::command::TabPlace;

/// How many closed tabs are kept to open again.
const CLOSED_KEPT: usize = 20;

impl App {
    /// Remembers a tab being closed, by its place and its document's file,
    /// to open again.
    pub(super) fn closing(&mut self, index: usize, path: PathBuf) {
        self.closed.push((index, path));
        let over = self.closed.len().saturating_sub(CLOSED_KEPT);
        self.closed.drain(..over);
    }

    /// Opens the tabs closed last again, the last first, each in the place
    /// it had. Each comes back where it was left, as the history says.
    pub(super) fn reopen_tabs(&mut self, times: u32) -> Task<Message> {
        let mut tasks = Vec::new();
        for _ in 0..times {
            let Some((index, path)) = self.closed.pop() else {
                if tasks.is_empty() {
                    self.notice = Notice::Error("no closed tabs to open again".to_owned());
                }
                break;
            };
            if !path.is_file() {
                continue;
            }
            // A document open again since only comes into view.
            let open = self.tabs.iter().any(|tab| tab.holds(&path));
            tasks.push(self.open_in_tab(path));
            if !open {
                self.move_tab(TabPlace::At(index));
            }
        }
        Task::batch(tasks)
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
                self.closing(index, document.path);
            }
        }
        self.current = 0;
    }
}
