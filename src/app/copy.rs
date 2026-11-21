//! Copying what the document is: its file's path, its title, or the text
//! of the page in view, as Tridactyl's `yy` copies a page's address.

use iced::Task;

use super::{App, Message, Notice, TextFor, tilde};
use crate::backend::TextLine;
use crate::input::CopyWhat;

impl App {
    /// Copies the path, the title or the text of the page in view. The
    /// page's text is read first if it has not been.
    pub(super) fn copy(&mut self, what: CopyWhat) -> Task<Message> {
        let Some(document) = self.document() else {
            return Task::none();
        };
        match what {
            CopyWhat::Path => {
                let path = document.path.to_string_lossy().into_owned();
                self.notice = Notice::Info(format!("copied {}", tilde(&document.path)));
                iced::clipboard::write(path)
            }
            CopyWhat::Title => {
                let title = document.name();
                self.notice = Notice::Info(format!("copied {title}"));
                iced::clipboard::write(title)
            }
            CopyWhat::Page => {
                let page = document.reader.current_page();
                if document.text.contains_key(&page) {
                    return self.copy_page(page);
                }
                self.lines_wanted = Some((document.id, TextFor::Copy(page)));
                self.read_text(vec![page])
            }
        }
    }

    /// Copies the text of a page already read.
    pub(super) fn copy_page(&mut self, page: u32) -> Task<Message> {
        let Some(lines) = self
            .document()
            .and_then(|document| document.text.get(&page))
        else {
            return Task::none();
        };
        let text = page_text(lines);
        let number = page.saturating_add(1);
        if text.is_empty() {
            self.notice = Notice::Info(format!("page {number} has no text"));
            return Task::none();
        }
        let count = text.chars().count();
        self.notice = Notice::Info(format!(
            "copied page {number}: {count} character{}",
            if count == 1 { "" } else { "s" }
        ));
        iced::clipboard::write(text)
    }
}

/// The text of a page, a line of text to a line, without trailing spaces
/// or blank lines at either end.
fn page_text(lines: &[TextLine]) -> String {
    let lines: Vec<String> = lines
        .iter()
        .map(|line| {
            let text: String = line.chars.iter().map(|glyph| glyph.character).collect();
            text.trim_end().to_owned()
        })
        .collect();
    lines.join("\n").trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::TextChar;
    use crate::layout::Rect;

    fn line(text: &str) -> TextLine {
        TextLine {
            chars: text
                .chars()
                .map(|character| TextChar {
                    character,
                    area: Rect {
                        left: 0.0,
                        top: 0.0,
                        right: 1.0,
                        bottom: 1.0,
                    },
                })
                .collect(),
        }
    }

    #[test]
    fn a_page_is_its_lines_trimmed() {
        let lines = [line("  "), line("First line  "), line("second"), line(" ")];
        assert_eq!(page_text(&lines), "First line\nsecond");
        assert_eq!(page_text(&[]), "");
    }
}
