//! The highlights, underlines, strike-outs and notes of a document written
//! out as Markdown, for `:export notes.md`: a section for each page that
//! has any, each marking's text quoted and its note after it.

use crate::backend::{Marking, MarkupKind};

/// Whether a file is one the markings are exported to, by its extension.
pub fn is_markdown(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            ["md", "markdown"].contains(&extension.to_ascii_lowercase().as_str())
        })
}

/// The markings of a document as Markdown: its title, where they come from,
/// and the markings page by page in the order given, which is the order
/// they are on each page. `labels` are the pages' printed numbers, if the
/// document has them.
pub fn markdown(
    title: &str,
    file: &str,
    markings: &[Marking],
    labels: Option<&[String]>,
) -> String {
    let count = markings.len();
    let mut blocks = vec![
        format!("# {}", escaped(title)),
        format!(
            "{count} {} from {}.",
            if count == 1 { "marking" } else { "markings" },
            escaped(file)
        ),
    ];
    let mut page = None;
    for marking in markings {
        if page != Some(marking.page) {
            page = Some(marking.page);
            blocks.push(heading(marking.page, labels));
        }
        blocks.extend(block(marking));
    }
    let mut text = blocks.join("\n\n");
    text.push('\n');
    text
}

/// The heading of a page's section: its number, and its printed number
/// first when that is another.
fn heading(page: u32, labels: Option<&[String]>) -> String {
    let number = page.saturating_add(1).to_string();
    let label = labels
        .and_then(|labels| labels.get(usize::try_from(page).unwrap_or(usize::MAX)))
        .map(|label| label.trim())
        .filter(|label| !label.is_empty() && *label != number);
    label.map_or_else(
        || format!("## Page {number}"),
        |label| format!("## Page {} ({number})", escaped(label)),
    )
}

/// A marking as Markdown: what kind it is unless a highlight, the text it
/// covers quoted, then the note written on it.
fn block(marking: &Marking) -> Vec<String> {
    let mut blocks = Vec::new();
    let note = marking
        .kept
        .contents
        .as_deref()
        .map(str::trim)
        .filter(|note| !note.is_empty());
    if marking.kind == MarkupKind::Note {
        blocks.push(format!(
            "**Note:** {}",
            paragraphs(note.unwrap_or_default())
        ));
        return blocks;
    }
    let kind = match marking.kind {
        MarkupKind::Highlight | MarkupKind::Note => None,
        MarkupKind::Underline => Some("Underlined"),
        MarkupKind::StrikeOut => Some("Struck out"),
        MarkupKind::Squiggly => Some("Underlined with a wavy line"),
    };
    if let Some(kind) = kind {
        blocks.push(format!("*{kind}:*"));
    }
    let text = marking
        .text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if !text.is_empty() {
        blocks.push(format!("> {}", escaped(&text)));
    }
    if let Some(note) = note {
        blocks.push(paragraphs(note));
    }
    blocks
}

/// A note's text, its paragraphs kept and each line's breaks made spaces.
fn paragraphs(note: &str) -> String {
    note.split("\n\n")
        .map(|paragraph| escaped(&paragraph.split_whitespace().collect::<Vec<_>>().join(" ")))
        .filter(|paragraph| !paragraph.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Text with what Markdown would read as markup escaped, so that it shows
/// as written: emphasis, links, code, tags and tables anywhere, and
/// headings, quotes and lists at its start.
fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        if "\\`*_[]<>|~".contains(character) {
            out.push('\\');
        }
        out.push(character);
    }
    if out.starts_with(['#', '-', '+', '=']) {
        out.insert(0, '\\');
    }
    // A number and a full stop or bracket starts a numbered list: the stop
    // is escaped, as a digit cannot be.
    let digits = out.chars().take_while(char::is_ascii_digit).count();
    if digits > 0
        && out
            .get(digits..)
            .is_some_and(|rest| rest.starts_with(['.', ')']))
    {
        out.insert(digits, '\\');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Kept;

    fn marking(page: u32, kind: MarkupKind, text: &str, note: Option<&str>) -> Marking {
        Marking {
            page,
            xref: 0,
            kind,
            areas: Vec::new(),
            text: text.to_owned(),
            kept: Kept {
                contents: note.map(str::to_owned),
                ..Kept::default()
            },
        }
    }

    #[test]
    fn markings_are_written_page_by_page_with_their_notes() {
        let markings = [
            marking(0, MarkupKind::Highlight, "The quick brown fox", None),
            marking(
                0,
                MarkupKind::Underline,
                "jumps over\nthe lazy dog",
                Some("Why?"),
            ),
            marking(2, MarkupKind::StrikeOut, "again", None),
            marking(2, MarkupKind::Note, "", Some("Look this up.\n\nAnd this.")),
            marking(2, MarkupKind::Squiggly, "agian", None),
        ];
        let written = markdown("Foxes", "foxes.pdf", &markings, None);
        assert_eq!(
            written,
            "# Foxes\n\n\
             5 markings from foxes.pdf.\n\n\
             ## Page 1\n\n\
             > The quick brown fox\n\n\
             *Underlined:*\n\n\
             > jumps over the lazy dog\n\n\
             Why?\n\n\
             ## Page 3\n\n\
             *Struck out:*\n\n\
             > again\n\n\
             **Note:** Look this up.\n\n\
             And this.\n\n\
             *Underlined with a wavy line:*\n\n\
             > agian\n",
            "the Markdown written"
        );
    }

    #[test]
    fn a_page_is_named_by_its_printed_number_too_when_that_is_another() {
        let labels = ["i".to_owned(), "2".to_owned(), String::new()];
        let heading_of = |page| heading(page, Some(&labels));
        assert_eq!(
            heading_of(0),
            "## Page i (1)",
            "a printed number of its own"
        );
        assert_eq!(heading_of(1), "## Page 2", "the same number");
        assert_eq!(heading_of(2), "## Page 3", "no printed number");
        assert_eq!(heading_of(7), "## Page 8", "past the labels");
        assert_eq!(heading(0, None), "## Page 1", "a document without labels");
    }

    #[test]
    fn one_marking_is_one_marking() {
        let markings = [marking(0, MarkupKind::Highlight, "fox", None)];
        assert!(
            markdown("Foxes", "foxes.pdf", &markings, None).contains("1 marking from"),
            "one marking is counted as one"
        );
    }

    #[test]
    fn what_markdown_would_read_as_markup_is_written_as_it_is() {
        let inline = escaped("a *b* [c](d) <e> `f`");
        assert_eq!(inline, "a \\*b\\* \\[c\\](d) \\<e\\> \\`f\\`", "inline");
        assert_eq!(escaped("# not a heading"), "\\# not a heading", "a heading");
        assert_eq!(escaped("- not a list"), "\\- not a list", "a list");
        assert_eq!(
            escaped("12. not a list"),
            "12\\. not a list",
            "a numbered list"
        );
        assert_eq!(
            escaped("12 foxes. And more"),
            "12 foxes. And more",
            "a number in a sentence"
        );
        assert_eq!(escaped("> not a quote"), "\\> not a quote", "a quote");
    }

    #[test]
    fn markdown_files_are_known_by_their_extension() {
        let path = std::path::Path::new;
        assert!(is_markdown(path("notes.md")), ".md");
        assert!(is_markdown(path("notes.Markdown")), ".markdown in capitals");
        assert!(!is_markdown(path("page.png")), ".png");
        assert!(!is_markdown(path("notes")), "no extension");
    }
}
