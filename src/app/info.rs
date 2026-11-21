//! What a document says about itself: the name its tab and the window go
//! by, and the panel `:info` shows.

use iced::widget::{Column, column, container, row, text};
use iced::{Color, Element, Fill, Font};

use super::{App, Document, LIST_TEXT_SIZE, LIST_WIDTH, Message, Notice, tilde};
use crate::theme::Colours;

/// The most characters of a title a tab shows.
const TAB_NAME: usize = 40;

/// How wide the names of the facts are, in characters.
const LABEL_WIDTH: f32 = 110.0;

impl Document {
    /// What the document goes by: its title, when it has one worth the
    /// name, or else its file name.
    pub(super) fn name(&self) -> String {
        self.info.name().map_or_else(
            || {
                self.path.file_name().map_or_else(
                    || tilde(&self.path),
                    |name| name.to_string_lossy().into_owned(),
                )
            },
            str::to_owned,
        )
    }

    /// Each fact about the document, named, in the order shown.
    fn facts(&self) -> Vec<(&'static str, String)> {
        let info = &self.info;
        let pages = self.reader.page_count();
        let size = std::fs::metadata(&self.path)
            .ok()
            .map(|metadata| bytes(metadata.len()));
        [
            ("Title", info.title.clone()),
            ("Author", info.author.clone()),
            ("Subject", info.subject.clone()),
            ("Keywords", info.keywords.clone()),
            ("File", Some(tilde(&self.path))),
            ("Size", size),
            (
                "Pages",
                Some(format!(
                    "{pages}, on page {}",
                    self.reader.current_page().saturating_add(1)
                )),
            ),
            ("Created", info.created.clone()),
            ("Modified", info.modified.clone()),
            ("Creator", info.creator.clone()),
            ("Producer", info.producer.clone()),
            ("Format", info.format.clone()),
            ("Encryption", info.encryption.clone()),
        ]
        .into_iter()
        .filter_map(|(name, value)| Some((name, value?)))
        .collect()
    }
}

impl App {
    /// Shows what the document in view says about itself.
    pub(super) fn show_info(&mut self) {
        match self.document() {
            Some(document) => {
                self.info = Some((document.name(), document.facts()));
            }
            None => self.notice = Notice::Info("no document is open".to_owned()),
        }
    }
}

/// A name cut short to fit a tab.
pub(super) fn tab_name(name: &str) -> String {
    if name.chars().count() <= TAB_NAME {
        return name.to_owned();
    }
    let mut short: String = name.chars().take(TAB_NAME.saturating_sub(1)).collect();
    short.push('\u{2026}');
    short
}

/// A size in bytes as people read it: 812 KB, 4.9 MB.
fn bytes(size: u64) -> String {
    const UNITS: [&str; 4] = ["bytes", "KB", "MB", "GB"];
    let mut value = size;
    let mut tenths = 0;
    let mut unit = 0;
    while value >= 1000 && unit < UNITS.len().saturating_sub(1) {
        tenths = value % 1000 / 100;
        value /= 1000;
        unit = unit.saturating_add(1);
    }
    let name = UNITS.get(unit).copied().unwrap_or_default();
    if unit == 0 || value >= 10 {
        format!("{value} {name}")
    } else {
        format!("{value}.{tenths} {name}")
    }
}

/// The panel of facts about a document, over the pages; any key closes it.
pub(super) fn panel<'a>(
    name: &'a str,
    facts: &'a [(&'static str, String)],
    chrome: Colours,
) -> Element<'a, Message> {
    let label = |content: String, colour: Color| {
        text(content)
            .size(LIST_TEXT_SIZE)
            .font(Font::MONOSPACE)
            .color(colour)
    };
    let heading = container(label(name.to_owned(), chrome.bar_text).font(Font {
        weight: iced::font::Weight::Bold,
        ..Font::MONOSPACE
    }))
    .padding([4, 10]);
    let lines = facts.iter().map(|(fact, value)| {
        row![
            container(label((*fact).to_owned(), chrome.list_number)).width(LABEL_WIDTH),
            container(label(value.clone(), chrome.bar_text)).width(Fill),
        ]
        .spacing(12)
        .padding([3, 10])
        .into()
    });
    let panel = container(column![heading, Column::with_children(lines).spacing(1)].spacing(4))
        .padding(8)
        .max_width(LIST_WIDTH)
        .style(move |_| {
            container::background(chrome.list).border(iced::Border {
                color: chrome.list_border,
                width: 1.0,
                radius: 4.0.into(),
            })
        });
    container(panel)
        .center_x(Fill)
        .center_y(Fill)
        .padding(24)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_as_people_write_them() {
        assert_eq!(bytes(0), "0 bytes");
        assert_eq!(bytes(999), "999 bytes");
        assert_eq!(bytes(1000), "1.0 KB");
        assert_eq!(bytes(812_345), "812 KB");
        assert_eq!(bytes(5_094_348), "5.0 MB");
        assert_eq!(bytes(4_950_000), "4.9 MB");
        assert_eq!(bytes(12_000_000_000_000), "12000 GB");
    }

    #[test]
    fn long_names_are_cut_short_for_a_tab() {
        assert_eq!(tab_name("Short"), "Short");
        let long = "A Complete Guide To Volume Price Analysis, Read the Market";
        let short = tab_name(long);
        assert_eq!(short.chars().count(), TAB_NAME);
        assert!(short.ends_with('\u{2026}'));
    }
}
