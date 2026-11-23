//! Every match of the last search in a list, as vim's quickfix list: each
//! with its page and its line, to go to by moving through the list,
//! narrowing it by typing, or clicking.

use iced::Task;

use super::{App, Message, Notice, TextFor, current_document, jump_to};
use crate::backend::TextLine;
use crate::input::Key;
use crate::layout::Rect;
use crate::picker::{Choice, Entry, Picker};

/// The most characters of a line shown for a match.
const LINE_LENGTH: usize = 100;

impl App {
    /// Lists every match of the last search, once the text of the pages
    /// they are on is read.
    pub(super) fn show_matches(&mut self) -> Task<Message> {
        self.pending = crate::keymap::Pending::new();
        let Some(document) = self.document() else {
            return Task::none();
        };
        let id = document.id;
        if document.results.search.is_none() || document.results.hits.is_empty() {
            self.notice = Notice::Error("no matches to list: search first".to_owned());
            return Task::none();
        }
        let mut pages: Vec<u32> = document.results.hits.iter().map(|hit| hit.page).collect();
        pages.dedup();
        if pages.iter().all(|page| document.text.contains_key(page)) {
            self.list_matches();
            return Task::none();
        }
        self.notice = Notice::Info("reading the pages the matches are on".to_owned());
        self.lines_wanted = Some((id, TextFor::Matches));
        self.read_text(pages)
    }

    /// Whether the text of every page with a match is read.
    pub(super) fn matches_read(&self) -> bool {
        self.document().is_some_and(|document| {
            document
                .results
                .hits
                .iter()
                .all(|hit| document.text.contains_key(&hit.page))
        })
    }

    /// Shows the list of matches, the one gone to last selected.
    pub(super) fn list_matches(&mut self) {
        let Some(document) = self.document() else {
            return;
        };
        let entries: Vec<Entry<usize>> = document
            .results
            .hits
            .iter()
            .enumerate()
            .map(|(index, hit)| {
                let number = hit.page.saturating_add(1).to_string();
                let page = document.label(hit.page).unwrap_or(&number).to_owned();
                let line = document
                    .text
                    .get(&hit.page)
                    .map(|lines| line_around(lines, hit.area))
                    .unwrap_or_default();
                Entry {
                    text: format!("{page:>5}  {line}"),
                    value: index,
                }
            })
            .collect();
        let current = document.results.current;
        let needle = document
            .results
            .search
            .as_ref()
            .map(|search| search.needle.clone())
            .unwrap_or_default();
        self.match_list = Picker::new(entries).map(|mut list| {
            if let Some(current) = current {
                list.select(current);
            }
            (needle, list)
        });
    }

    /// Moves through the list of matches, narrows it, or goes to one.
    pub(super) fn on_match_list_key(&mut self, key: Key) {
        let Some((_, list)) = &mut self.match_list else {
            return;
        };
        match list.feed(key, &self.keymaps.recent) {
            Some(Choice::Open(index)) => self.go_to_match(index),
            Some(Choice::Close) => self.match_list = None,
            None => {}
        }
    }

    /// Goes to a match, by its place among all the matches, and closes the
    /// list.
    pub(super) fn go_to_match(&mut self, index: usize) {
        self.match_list = None;
        let notice = &mut self.notice;
        if let Some(document) = current_document(&mut self.tabs, self.current) {
            document.results.shown = true;
            jump_to(document, Some(index), notice);
        }
    }
}

/// The text of the line a match is on, its spaces made single, cut short
/// if long: the line the match's area overlaps most.
fn line_around(lines: &[TextLine], area: Rect) -> String {
    let overlap = |line: &TextLine| {
        line.chars
            .iter()
            .map(|c| {
                let across = (c.area.right.min(area.right) - c.area.left.max(area.left)).max(0.0);
                let down = (c.area.bottom.min(area.bottom) - c.area.top.max(area.top)).max(0.0);
                across * down
            })
            .sum::<f32>()
    };
    let Some(line) = lines
        .iter()
        .max_by(|a, b| overlap(a).total_cmp(&overlap(b)))
        .filter(|line| overlap(line) > 0.0)
    else {
        return String::new();
    };
    let text: String = line.chars.iter().map(|c| c.character).collect();
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() > LINE_LENGTH {
        let short: String = text.chars().take(LINE_LENGTH).collect();
        format!("{short}\u{2026}")
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::TextChar;

    fn line(content: &str, top: f32) -> TextLine {
        TextLine {
            chars: content
                .chars()
                .enumerate()
                .map(|(column, character)| {
                    let left = f32::from(u16::try_from(column).unwrap()) * 10.0;
                    TextChar {
                        character,
                        area: Rect {
                            left,
                            top,
                            right: left + 10.0,
                            bottom: top + 12.0,
                        },
                    }
                })
                .collect(),
        }
    }

    #[test]
    fn a_match_shows_with_the_line_it_is_on() {
        let lines = [line("first   line", 0.0), line("the second line", 20.0)];
        let hit = Rect {
            left: 40.0,
            top: 20.0,
            right: 100.0,
            bottom: 32.0,
        };
        assert_eq!(line_around(&lines, hit), "the second line");
        let off = Rect {
            left: 0.0,
            top: 100.0,
            right: 10.0,
            bottom: 110.0,
        };
        assert_eq!(line_around(&lines, off), "");
        let long = [line(&"word ".repeat(40), 0.0)];
        let shown = line_around(&long, hit.union(Rect { top: 0.0, ..hit }));
        assert_eq!(shown.chars().count(), LINE_LENGTH + 1);
        assert!(shown.ends_with('\u{2026}'));
    }
}

#[cfg(test)]
mod app_tests {
    use iced::futures::executor::block_on;

    use crate::app::pointer::{Click, Pointer};
    use crate::app::testing::{on_screen, read, selected, showing};
    use crate::app::{Action, App, Message, Search, move_view};
    use iced::Task;

    /// Searches the document in view at once, as `/` does once the
    /// backend answers.
    fn search(app: &mut App, id: u64, text: &str, whole_word: bool) -> usize {
        let needle = app.finding.needle(text, whole_word);
        let found = block_on(app.document().unwrap().backend.search(needle));
        let search = Search {
            needle: text.to_owned(),
            forward: true,
            whole_word,
        };
        let _: Task<Message> = app.update(Message::Found(id, search, found));
        app.document().unwrap().results.hits.len()
    }

    #[test]
    fn a_capital_makes_a_search_match_case() {
        let (mut app, id) = showing("text.pdf");
        // "Hello, world. A line of text" and "second line -- with words".
        assert_eq!(search(&mut app, id, "line", false), 2);
        assert_eq!(search(&mut app, id, "Line", false), 0);
        assert_eq!(search(&mut app, id, "Hello", false), 1);
        app.finding.smartcase = false;
        assert_eq!(search(&mut app, id, "Line", false), 2);
        // A word is found only whole, in any case.
        assert_eq!(search(&mut app, id, "wor", true), 0);
        assert_eq!(search(&mut app, id, "WORDS", true), 1);
    }

    #[test]
    fn every_match_is_listed_with_its_line_once_the_text_is_read() {
        let (mut app, id) = showing("text.pdf");
        assert_eq!(search(&mut app, id, "line", false), 2);
        let _: Task<Message> = app.show_matches();
        assert!(app.match_list.is_none(), "listed before the text is read");
        read(&mut app, id, 0);
        let (needle, list) = app.match_list.as_ref().unwrap();
        assert_eq!(needle, "line");
        let lines: Vec<&str> = list.shown().map(|entry| entry.text.as_str()).collect();
        assert_eq!(
            lines,
            [
                "    1  Hello, world. A line of text",
                "    1  second line -- with words"
            ]
        );
        let _: Task<Message> = app.update(Message::Clicked(Click::Match(1)));
        assert!(app.match_list.is_none());
        assert_eq!(app.document().unwrap().results.current, Some(1));
    }

    #[test]
    fn the_last_selection_is_selected_again() {
        let (mut app, id) = showing("text.pdf");
        read(&mut app, id, 0);
        let _: Task<Message> = app.select_again();
        assert!(app.visual.is_none());
        // A double click on "world" selects it; a click leaves.
        let word = on_screen(&app, 0, (130.0, 66.0));
        for _ in 0..2 {
            let _: Task<Message> = app.update(Message::Pointer(Pointer::Press(word, true)));
            let _: Task<Message> = app.update(Message::Pointer(Pointer::Release(word)));
        }
        app.mouse = crate::app::pointer::Mouse::default();
        let _: Task<Message> = app.update(Message::Pointer(Pointer::Press(word, true)));
        let _: Task<Message> = app.update(Message::Pointer(Pointer::Release(word)));
        assert!(app.visual.is_none());
        let _: Task<Message> = app.select_again();
        assert_eq!(selected(&app), "world");
    }

    #[test]
    fn a_count_and_percent_goes_that_far_through() {
        let (mut app, _) = showing("links.pdf");
        let document = app.document_mut().unwrap();
        for (percent, page) in [(50, 0), (51, 1), (100, 1), (0, 0), (400, 1)] {
            move_view(document, Action::GotoPercent(Some(percent)), 40.0);
            assert_eq!(document.reader.current_page(), page, "{percent}%");
        }
    }
}
