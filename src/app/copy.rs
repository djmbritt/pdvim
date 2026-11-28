//! Copying what the document is: its file's path, its title, or the text
//! of the page in view, as Tridactyl's `yy` copies a page's address; and
//! exporting the page in view to a file, or the markings to Markdown.

use crate::task::Task;

use super::{App, Message, Notice, TextFor, tilde};
use std::path::PathBuf;

use crate::backend::{Export, ExportFormat, TextLine};
use crate::input::CopyWhat;

impl App {
    /// Writes the page in view to a file, as an image the size it is shown
    /// at or as a drawing, turned as it is shown.
    pub(super) fn export(&mut self, target: PathBuf, force: bool) -> Task<Message> {
        let scale_factor = self.scale_factor;
        let Some(document) = self.document() else {
            self.notice = Notice::Error("no document to export".to_owned());
            return Task::none();
        };
        if crate::notes::is_markdown(&target) {
            return self.export_notes(target, force);
        }
        if ExportFormat::of(&target).is_none() {
            self.notice = Notice::Error("export to a .png, .svg or .md file".to_owned());
            return Task::none();
        }
        let export = Export {
            page: document.reader.current_page(),
            scale: document.reader.zoom() * scale_factor,
            rotation: document.reader.rotation(),
            target: target.clone(),
            force,
        };
        Task::perform(document.backend.export(export), move |result| {
            Message::Exported(target, result)
        })
    }

    /// Writes the highlights, underlines, strike-outs and notes of the
    /// document in view, its changes not yet saved too, to a Markdown file.
    fn export_notes(&self, target: PathBuf, force: bool) -> Task<Message> {
        let Some(document) = self.document() else {
            return Task::none();
        };
        let title = document.name();
        let file = document
            .path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let labels = document.labels.clone();
        let markings = document.backend.markings();
        let written = target.clone();
        let exporting = async move {
            let markings = markings.await.map_err(|error| error.to_string())?;
            if markings.is_empty() {
                return Err("no highlights or notes to export".to_owned());
            }
            let text = crate::notes::markdown(&title, &file, &markings, labels.as_deref());
            write_new(&written, text.as_bytes(), force)?;
            Ok(markings.len())
        };
        Task::perform(exporting, move |result| {
            Message::NotesExported(target, result)
        })
    }

    /// Says how many markings were exported, or why none were.
    pub(super) fn on_notes_exported(
        &mut self,
        target: &std::path::Path,
        result: Result<usize, String>,
    ) -> Task<Message> {
        self.notice = match result {
            Ok(count) => Notice::Info(format!(
                "exported {count} {} to {}",
                if count == 1 { "marking" } else { "markings" },
                tilde(target)
            )),
            Err(error) => Notice::Error(error),
        };
        Task::none()
    }

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
                crate::task::copy(path)
            }
            CopyWhat::Title => {
                let title = document.name();
                self.notice = Notice::Info(format!("copied {title}"));
                crate::task::copy(title)
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
        crate::task::copy(text)
    }
}

/// The text of a page as running text: the lines of each paragraph joined,
/// and the paragraphs kept apart by a blank line.
fn page_text(lines: &[TextLine]) -> String {
    let starts = crate::visual::paragraph_starts(lines);
    let texts: Vec<String> = lines
        .iter()
        .map(|line| line.chars.iter().map(|glyph| glyph.character).collect())
        .collect();
    crate::prose::join(
        texts
            .iter()
            .enumerate()
            .map(|(index, text)| (text.as_str(), starts.contains(&index))),
    )
}

/// Writes a file, writing over one already there only when `force` says
/// to, as `:export!` does.
fn write_new(target: &std::path::Path, bytes: &[u8], force: bool) -> Result<(), String> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true);
    if force {
        options.create(true).truncate(true);
    } else {
        options.create_new(true);
    }
    let mut file = options.open(target).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            format!("{} exists; :export! writes over it", target.display())
        } else {
            format!("cannot write {}: {error}", target.display())
        }
    })?;
    std::io::Write::write_all(&mut file, bytes)
        .map_err(|error| format!("cannot write {}: {error}", target.display()))
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
    fn a_page_is_copied_as_running_text() {
        let lines = [line("  "), line("First line  "), line("second"), line(" ")];
        assert_eq!(page_text(&lines), "First line second");
        assert_eq!(page_text(&[]), "");
    }

    /// Runs a line at the prompt, as typed after `:`.
    fn command(session: &mut super::super::testing::Session, line: &str) {
        session.keys(":");
        for character in line.chars() {
            session.press(crate::input::Key::Char(character));
        }
        session.keys("<Enter>");
    }

    /// What the reader says now.
    fn said(session: &super::super::testing::Session) -> &str {
        match &session.app.notice {
            Notice::Info(said) | Notice::Error(said) => said,
            Notice::None => "",
        }
    }

    #[test]
    fn the_markings_are_exported_to_markdown_their_changes_not_yet_saved_too() {
        let mut session = super::super::testing::Session::open("text.pdf");
        // The first line struck out, and not saved.
        session.keys("vaaeS");
        let struck = session.app.document().map(super::super::Document::modified);
        assert_eq!(struck, Some(true), "the strike-out is not yet saved");
        let target = session.dir.join("notes.md");
        command(&mut session, &format!("export {}", target.display()));
        assert!(
            said(&session).starts_with("exported 1 marking to"),
            "said {:?}",
            said(&session)
        );
        let written = std::fs::read_to_string(&target).unwrap_or_default();
        assert!(
            written.starts_with("# "),
            "a heading with the title: {written}"
        );
        assert!(
            written.contains("1 marking from text.pdf."),
            "where it is from: {written}"
        );
        assert!(
            written.contains("## Page 1\n\n*Struck out:*\n\n> "),
            "the strike-out: {written}"
        );
    }

    #[test]
    fn a_markdown_file_there_already_is_written_over_only_when_told() {
        let mut session = super::super::testing::Session::open("text.pdf");
        session.keys("vaaeS");
        let target = session.dir.join("notes.md");
        let kept = std::fs::write(&target, "mine");
        assert!(kept.is_ok(), "a file to keep");
        command(&mut session, &format!("export {}", target.display()));
        assert!(
            said(&session).ends_with(":export! writes over it"),
            "said {:?}",
            said(&session)
        );
        let kept = std::fs::read_to_string(&target).unwrap_or_default();
        assert_eq!(kept, "mine", "the file is kept");
        command(&mut session, &format!("export! {}", target.display()));
        let written = std::fs::read_to_string(&target).unwrap_or_default();
        assert!(
            written.contains("Struck out"),
            "the file is written over: {written}"
        );
    }

    #[test]
    fn a_document_without_markings_exports_none() {
        let mut session = super::super::testing::Session::open("text.pdf");
        let target = session.dir.join("notes.md");
        command(&mut session, &format!("export {}", target.display()));
        assert_eq!(
            said(&session),
            "no highlights or notes to export",
            "what is said"
        );
        assert!(!target.exists(), "no file is written");
    }
}
