//! Entry points for the benchmarks in `benches/`: the work done on every
//! frame or key press that a long document or a large window could make
//! slow, set up once and run again and again, so that the harness can reach
//! what the crate keeps to itself.
//!
//! The document timed is written here: pages of prose set close, as a
//! paper's are, so that each page has many lines, sentences and
//! paragraphs, and none of it depends on a file outside the repository.

use std::fmt::Write as _;

use mupdf::Document;

use crate::backend::{self, Needle, Recording, RenderJob, TextLine};
use crate::config;
use crate::flash::{Flash, Press};
use crate::layout::{Layout, PageSize, Rotation};
use crate::recolor::Palette;
use crate::visual::{self, Spot};

/// The sentences a page is made of, over and over: words of every length,
/// commas, and full stops, as the motions and searches meet in prose.
const PROSE: [&str; 6] = [
    "The reader keeps each page it has drawn, and draws it again only when",
    "the zoom or the colours change. A page of a paper holds sixty lines or",
    "more, set close, with a sentence ending partway along most of them, and",
    "a paragraph every dozen lines or so. Searching walks every character of",
    "every line, and a jump with flash measures how far each match is from",
    "the cursor. None of this may keep a key press waiting.",
];

/// Lines of text on each page, and how far apart, in points.
const LINES: u32 = 64;
const LEADING: u32 = 11;

/// A PDF of `pages` pages of prose, each a US letter page of
/// [`LINES`] lines in Times, a paragraph every eight lines.
#[must_use]
pub fn prose_pdf(pages: u32) -> Vec<u8> {
    let mut text = String::new();
    for line in 0..LINES {
        let words = PROSE
            .get(usize::try_from(line).unwrap_or(0).checked_rem(PROSE.len()).unwrap_or(0))
            .copied()
            .unwrap_or_default();
        // A paragraph starts after a line left blank.
        if line.checked_rem(8) == Some(0) && line != 0 {
            text.push_str("T* ");
        }
        let _ = writeln!(text, "({words}) '");
    }
    let content = format!("BT /F1 9 Tf {LEADING} TL 54 760 Td\n{text}ET");

    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        String::new(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>".to_owned(),
    ];
    let mut kids = Vec::new();
    for _ in 0..pages {
        let page = objects.len().saturating_add(1);
        let stream = page.saturating_add(1);
        kids.push(format!("{page} 0 R"));
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
             /Resources << /Font << /F1 3 0 R >> >> /Contents {stream} 0 R >>"
        ));
        objects.push(format!(
            "<< /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        ));
    }
    if let Some(tree) = objects.get_mut(1) {
        *tree = format!(
            "<< /Type /Pages /Kids [{}] /Count {pages} >>",
            kids.join(" ")
        );
    }

    let mut pdf = String::from("%PDF-1.4\n");
    let mut offsets = Vec::with_capacity(objects.len());
    for (index, body) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        let _ = write!(pdf, "{} 0 obj\n{body}\nendobj\n", index.saturating_add(1));
    }
    let table = pdf.len();
    let count = objects.len().saturating_add(1);
    let _ = write!(pdf, "xref\n0 {count}\n0000000000 65535 f \n");
    for offset in offsets {
        let _ = writeln!(pdf, "{offset:010} 00000 n ");
    }
    let _ = write!(
        pdf,
        "trailer\n<< /Size {count} /Root 1 0 R >>\nstartxref\n{table}\n%%EOF\n"
    );
    pdf.into_bytes()
}

/// The first page of a document, opened, recorded and read once, to time
/// what is done with it again and again.
pub struct Page {
    document: Document,
    recording: Recording,
    lines: Vec<TextLine>,
}

impl Page {
    /// Opens a PDF's first page, or `None` if MuPDF cannot.
    #[must_use]
    pub fn open(pdf: &[u8]) -> Option<Self> {
        let document = Document::from_bytes(pdf, "application/pdf").ok()?;
        let recording = backend::record(&document, 0).ok()?;
        let lines = backend::page_text(&document, 0).ok()?;
        Some(Self {
            document,
            recording,
            lines,
        })
    }

    /// The lines of text the page was read as.
    #[must_use]
    pub const fn lines(&self) -> usize {
        self.lines.len()
    }

    /// Records the page afresh, as each page is when first shown.
    #[must_use]
    pub fn record(&self) -> bool {
        backend::record(&self.document, 0).is_ok()
    }

    /// Draws the page from its recording at `scale` pixels per point,
    /// recoloured as on a dark background or not, and gives the bitmap's
    /// size in bytes.
    #[must_use]
    pub fn render(&self, scale: f32, recolour: bool) -> usize {
        let job = RenderJob {
            page: 0,
            scale,
            rotation: Rotation::None,
            clip: None,
            palette: recolour.then(Palette::default),
        };
        backend::render(&self.recording, job, 16_384.0).map_or(0, |bitmap| bitmap.pixels.len())
    }

    /// Reads the page's text, as visual mode, flash and copying need it.
    #[must_use]
    pub fn read_text(&self) -> usize {
        backend::page_text(&self.document, 0).map_or(0, |lines| lines.len())
    }

    /// Searches the page's text, as each page is searched once read, and
    /// gives the number of matches.
    #[must_use]
    pub fn search(&self, text: &str) -> usize {
        let needle = Needle {
            text: text.to_owned(),
            match_case: false,
            whole_word: false,
        };
        backend::text_hits(&self.lines, 0, &needle).len()
    }

    /// Types text to jump to with flash, a character at a time as the
    /// labels are worked out afresh after each, and gives the number of
    /// matches left.
    #[must_use]
    pub fn flash(&self, typed: &str) -> usize {
        let lines: Vec<_> = self
            .lines
            .iter()
            .enumerate()
            .map(|(line, text)| (0, line, text))
            .collect();
        let mut flash = Flash::new();
        for character in typed.chars() {
            flash.feed(Press::Char(character), &lines, Spot::line_start(0, 0));
        }
        flash.targets().len()
    }

    /// Finds where the page's sentences start, as `is`, `as` and the
    /// labels of `v` do.
    #[must_use]
    pub fn sentences(&self) -> usize {
        visual::sentence_starts(&self.lines).len()
    }

    /// Finds where the page's paragraphs start, as `ip`, `{` and `}` do.
    #[must_use]
    pub fn paragraphs(&self) -> usize {
        visual::paragraph_starts(&self.lines).len()
    }
}

/// Reads a configuration file, as pdvim does at every start and at
/// `:source`, and gives the number of commands in it.
#[must_use]
pub fn config(text: &str) -> usize {
    config::parse(text).0.len()
}

/// A column of `pages` US letter pages, as a long document is laid out.
#[must_use]
pub fn column(pages: u32) -> Layout {
    let sizes = vec![
        PageSize {
            width: 612.0,
            height: 792.0,
        };
        usize::try_from(pages).unwrap_or(0)
    ];
    Layout::new(&sizes, Rotation::None)
}

/// Finds the pages a view at each of `views` heights down a column shows,
/// as every frame of scrolling does, and gives their total, so that none
/// of the work is skipped.
#[must_use]
pub fn pages_in_view(layout: &Layout, views: &[f32], height: f32) -> u32 {
    views
        .iter()
        .map(|&top| {
            let shown = layout.pages_between(top, top + height);
            shown.end.saturating_sub(shown.start)
        })
        .fold(layout.page_at(0.0), u32::saturating_add)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the benchmarks time is all there: a page whose text reads back
    /// as written, with sentences and paragraphs in it to find.
    #[test]
    fn the_page_timed_reads_back_as_written() {
        let Some(page) = Page::open(&prose_pdf(3)) else {
            panic!("MuPDF could not open the PDF written to time");
        };
        assert_eq!(page.lines(), 64);
        assert!(page.record());
        assert!(page.render(1.0, true) > 0);
        assert!(page.search("the") > 10);
        assert!(page.flash("the") > 0);
        assert!(page.sentences() > 10);
        assert!(page.paragraphs() > 1);
        assert!(config(include_str!("../defaults.toml")) > 100);
        let layout = column(10_000);
        assert_eq!(layout.page_count(), 10_000);
        assert!(pages_in_view(&layout, &[792.0 * 5_000.0], 1080.0) >= 2);
    }
}
