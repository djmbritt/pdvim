//! Tabs beyond going between them: closing all but one, moving one, and
//! opening again those closed, as Tridactyl's `:tabonly`, `:tabmove` and
//! `u`.

use std::path::PathBuf;

use crate::task::Task;

use super::{App, Message, Notice, Tab, info, tilde};
use crate::backend;
use crate::command::TabPlace;
use crate::picker::{Choice, Entry, Picker};
use crate::reader::Place;

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
    pub(super) fn open_new_tab(
        &mut self,
        path: PathBuf,
        index: usize,
        place: Place,
    ) -> Task<Message> {
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
            backend::open(path.clone(), None, self.flow_for(&path)),
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

/// Tabs in a running app: going between them, moving them, closing all
/// but one, opening a closed one again, and asking before closing.
#[cfg(test)]
mod tests {
    use crate::app::testing::Session;

    /// The file names of the tabs, in order, and which is in view.
    fn tabs(session: &Session) -> (Vec<String>, usize) {
        let names = session
            .app
            .tabs
            .iter()
            .map(|tab| {
                tab.document
                    .as_ref()
                    .and_then(|document| document.path.file_name())
                    .map_or_default(|name| name.to_string_lossy().into_owned())
            })
            .collect();
        (names, session.app.current)
    }

    /// The names, as [`tabs`] gives them.
    fn named(names: [&str; 3]) -> Vec<String> {
        names.map(str::to_owned).to_vec()
    }

    fn three() -> Session {
        let mut session = Session::open("text.pdf");
        session.open_fixture("links.pdf");
        session.open_fixture("outline.pdf");
        session
    }

    #[test]
    fn tabs_open_beside_each_other_and_are_gone_between() {
        let mut session = three();
        assert_eq!(
            tabs(&session),
            (named(["text.pdf", "links.pdf", "outline.pdf"]), 2)
        );
        session.keys("gt");
        assert_eq!(tabs(&session).1, 0);
        session.keys("gT");
        assert_eq!(tabs(&session).1, 2);
        session.keys(":tabnext 2<Enter>");
        assert_eq!(tabs(&session).1, 1);
    }

    #[test]
    fn a_tab_is_moved() {
        let mut session = three();
        session.keys(":tabmove 0<Enter>");
        assert_eq!(
            tabs(&session),
            (named(["outline.pdf", "text.pdf", "links.pdf"]), 0)
        );
        session.keys(":tabmove +1<Enter>");
        assert_eq!(
            tabs(&session),
            (named(["text.pdf", "outline.pdf", "links.pdf"]), 1)
        );
    }

    #[test]
    fn all_but_one_are_closed_and_a_closed_one_opens_again() {
        let mut session = three();
        session.keys(":tabonly<Enter>");
        assert_eq!(tabs(&session), (vec!["outline.pdf".into()], 0));
        session.keys("U");
        assert_eq!(tabs(&session).0.len(), 2);
    }

    #[test]
    fn closing_a_tab_is_asked_about_first() {
        let mut session = three();
        session.keys("q");
        assert!(session.app.asking.is_some());
        session.keys("n");
        assert_eq!(tabs(&session).0.len(), 3);
        session.keys("qy");
        assert_eq!(tabs(&session).0.len(), 2);
        assert!(!session.exited);
    }

    #[test]
    fn the_list_of_tabs_goes_to_one() {
        let mut session = three();
        session.keys("b");
        assert!(session.app.tab_list.is_some());
        session.keys("<Up><Up><Enter>");
        assert!(session.app.tab_list.is_none());
        assert_eq!(tabs(&session).1, 0);
    }
}
