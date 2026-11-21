//! What a document says about itself: the name its tab and the window go
//! by, and the panel `:info` shows.

use iced::widget::{Column, column, container, row, text};
use iced::{Color, Element, Fill, Font};

use super::{
    App, Document, LIST_TEXT_SIZE, LIST_WIDTH, Message, Notice, current_document, leap, tilde,
};
use crate::num;
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

    /// The label printed on a page, counted from 0, if the document gives
    /// labels.
    fn label(&self, page: u32) -> Option<&str> {
        self.labels
            .as_ref()?
            .get(num::index(page))
            .map(String::as_str)
    }

    /// Where in the document the view is, as the status bar says it:
    /// `12/300`, or with the printed number, `xii (12/300)`.
    pub(super) fn page_position(&self) -> String {
        let count = self.reader.page_count();
        let page = self.reader.current_page().min(count.saturating_sub(1));
        let place = format!("{}/{count}", page.saturating_add(1));
        match self.label(page) {
            Some(label) if !label.is_empty() && *label != page.saturating_add(1).to_string() => {
                format!("{label} ({place})")
            }
            _ => place,
        }
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
                Some(format!("{pages}, on page {}", self.page_position())),
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
    /// Goes to the page with this number printed on it, or else at this
    /// place, counted from 1.
    pub(super) fn go_to_label(&mut self, label: &str) {
        let Some(document) = current_document(&mut self.tabs, self.current) else {
            return;
        };
        match labelled(document.labels.as_deref(), label) {
            Some(page) => leap(document, page),
            None => self.notice = Notice::Error(format!("no page is numbered {label}")),
        }
    }

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

/// The page, counted from 0, with this label printed on it: the first
/// that has it exactly, then the first that has it in other case. Without
/// such a label, a number is the page at that place, counted from 1.
fn labelled(labels: Option<&[String]>, label: &str) -> Option<u32> {
    let labels = labels.unwrap_or_default();
    let found = labels
        .iter()
        .position(|printed| printed == label)
        .or_else(|| {
            labels
                .iter()
                .position(|printed| printed.eq_ignore_ascii_case(label))
        });
    found.map_or_else(
        || {
            label
                .parse::<u32>()
                .ok()
                .filter(|&page| page > 0)
                .map(|page| page.saturating_sub(1))
        },
        |page| u32::try_from(page).ok(),
    )
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
    fn pages_are_found_by_their_printed_numbers() {
        let labels: Vec<String> = ["i", "ii", "1", "2", "A-1"].map(String::from).to_vec();
        let labels = Some(labels.as_slice());
        assert_eq!(labelled(labels, "ii"), Some(1));
        assert_eq!(labelled(labels, "II"), Some(1));
        // The printed 2, not the second page.
        assert_eq!(labelled(labels, "2"), Some(3));
        assert_eq!(labelled(labels, "a-1"), Some(4));
        // A number printed on no page is the page at that place.
        assert_eq!(labelled(labels, "5"), Some(4));
        assert_eq!(labelled(labels, "xiv"), None);
        assert_eq!(labelled(None, "7"), Some(6));
        assert_eq!(labelled(None, "0"), None);
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
