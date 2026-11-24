//! Reading and rendering documents with MuPDF.
//!
//! A MuPDF document must stay on the thread that opened it, and rendering a
//! page can take a while. So each open document gets a worker thread that
//! owns it. The interface sends the worker requests through a [`Backend`]
//! and awaits each answer, which keeps the window responsive. The worker
//! records each page it is asked to draw, and a few painter threads draw
//! from the recordings, so that a page slow to draw holds up nothing else.

use std::collections::VecDeque;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::thread;

use iced::futures::channel::oneshot;
use mupdf::{
    Colorspace, DestinationKind, Device, DisplayList, Document, ImageFormat, Matrix, Pixmap,
    TextCharFlags, TextPageFlags,
};

use crate::fonts;
use crate::layout::{PageSize, Rect, Rotation};
use crate::num;
use crate::prose;
use crate::recolor::{Palette, Rgb};

/// The longest side a rendered page may have, in pixels. Graphics cards
/// refuse textures much larger than this, so a page zoomed in further is
/// rendered at this size and stretched.
const LONGEST_SIDE: f32 = 8000.0;

/// How many pages the worker keeps the recordings of, to draw again at
/// another zoom or in other colours without reading them again.
const RECORDINGS_KEPT: usize = 8;

/// How many pages of a document are drawn at once, each on a thread of
/// its own: a page that takes long to draw holds up only its own picture.
const PAINTERS: usize = 3;

/// The scale pages are rendered at to find their margins, in pixels per
/// point. Coarse is enough: only the outline of the content matters.
const MEASURE_SCALE: f32 = 0.5;

/// The longest side a page rendered to find its margins may have, in
/// pixels. A page larger than this at [`MEASURE_SCALE`] is rendered
/// smaller still, so that a page claiming to be absurdly large does not
/// take all the memory and time there is.
const MEASURE_SIDE: f32 = 2000.0;

/// How dark a grey pixel must be to count as content rather than paper.
const PAPER: u8 = 245;

/// The margin left around the content of a trimmed page, in points.
const TRIM_MARGIN: f32 = 6.0;

/// The most pages a document may have. A few kilobytes of HTML can lay
/// out to hundreds of millions of pages, which would take minutes and
/// gigabytes to measure; no real document comes near this.
const MOST_PAGES: i32 = 100_000;

/// The most search results reported for one page.
const HITS_PER_PAGE: u32 = 500;

/// Why the backend could not do what was asked.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("cannot open {path}: {reason}")]
    Open { path: String, reason: String },
    #[error("{0} is password-protected")]
    Encrypted(String),
    #[error("wrong password for {0}")]
    WrongPassword(String),
    #[error("cannot render page {page}: {reason}")]
    Render { page: u32, reason: String },
    #[error("cannot measure the margins: {0}")]
    Measure(String),
    #[error("search failed: {0}")]
    Search(String),
    #[error("cannot read the links: {0}")]
    Links(String),
    #[error("cannot read the text: {0}")]
    Text(String),
    #[error("cannot mark the text: {0}")]
    Markup(String),
    #[error("cannot take the marking away: {0}")]
    Unmark(String),
    #[error("cannot read the markings: {0}")]
    Markings(String),
    #[error("cannot save: {0}")]
    Save(String),
    #[error("cannot read the table of contents: {0}")]
    Outline(String),
    #[error("cannot fill in the form: {0}")]
    Form(String),
    #[error("cannot export the page: {0}")]
    Export(String),
    #[error("the document was closed")]
    Closed,
}

/// A page to render and how.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderJob {
    /// The page, counted from 0.
    pub page: u32,
    /// Pixels per point.
    pub scale: f32,
    pub rotation: Rotation,
    /// The part of the unturned page to render, in points from its top-left
    /// corner. The whole page when absent.
    pub clip: Option<Rect>,
    /// The colours to recolour the page in, if any.
    pub palette: Option<Palette>,
}

/// A page to write to a file, as `:export` asks.
#[derive(Debug, Clone, PartialEq)]
pub struct Export {
    /// The page, counted from 0.
    pub page: u32,
    /// Pixels per point, for an image.
    pub scale: f32,
    pub rotation: Rotation,
    /// The file to write: an image if it ends in `.png`, a drawing if in
    /// `.svg`.
    pub target: PathBuf,
    /// Whether to write over a file that is there already.
    pub force: bool,
}

/// What a page can be exported as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Png,
    Svg,
}

impl ExportFormat {
    /// The format a file's extension asks for, if it is one of them.
    pub fn of(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "png" => Some(Self::Png),
            "svg" => Some(Self::Svg),
            _ => None,
        }
    }
}

/// The most pixels along either side of an exported image.
const EXPORT_LONGEST_SIDE: f32 = 10_000.0;

/// A rendered page as rows of red, green, blue and alpha bytes.
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Text to search for, and how.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Needle {
    pub text: String,
    /// Whether upper and lower case must be as typed.
    pub match_case: bool,
    /// Whether it must be a word of its own, not part of a longer one.
    pub whole_word: bool,
}

/// A place where the searched text occurs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    /// The page, counted from 0.
    pub page: u32,
    /// The area the text covers on the unturned page, in points.
    pub area: Rect,
}

/// A link on a page.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    /// The area that is the link on the unturned page, in points.
    pub area: Rect,
    pub target: Target,
}

/// Where a link leads.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// A place in the same document: a page counted from 0, and how far
    /// down the unturned page, in points, when the link says.
    Page { page: u32, top: Option<f32> },
    /// Anything else, such as a web page, for another program to open.
    Uri(String),
    /// A field of a form, to fill in.
    Field(Field),
}

/// A field of a form that can be filled in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    /// The page and the widget's object number, which find it again.
    pub widget: Annotation,
    pub kind: FieldKind,
    /// What it holds: its text, or the state of a box.
    pub value: String,
    /// What the form calls it.
    pub name: String,
}

/// What can be put in a field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldKind {
    /// Text, typed.
    Text,
    /// One of these.
    Choice(Vec<String>),
    /// A box that is ticked by setting it to the state named, and
    /// unticked by setting it to `Off`.
    Checkbox { on: String },
    /// One of a group, picked by setting it to the state named.
    Radio { on: String },
}

/// The state of a box that is not ticked.
pub const OFF: &str = "Off";

/// A line of text on a page, in the order MuPDF reads the page.
#[derive(Debug, Clone, PartialEq)]
pub struct TextLine {
    pub chars: Vec<TextChar>,
}

/// A character on a page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextChar {
    pub character: char,
    /// The area the character takes on the unturned page, in points.
    pub area: Rect,
}

/// The lines of text on some pages, each page, counted from 0, with its
/// lines.
pub type PageText = Vec<(u32, Vec<TextLine>)>;

/// The links on some pages, each page, counted from 0, with its links.
pub type PageLinks = Vec<(u32, Vec<Link>)>;

/// The pictures and figures on some pages, each page, counted from 0, with
/// the area of each on the unturned page, in points.
pub type PageFigures = Vec<(u32, Vec<Rect>)>;

/// What a document says about itself. Fields it leaves empty are none.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Info {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    /// The program the document was written in.
    pub creator: Option<String>,
    /// The program that made the PDF.
    pub producer: Option<String>,
    /// When it was made and last changed, as `2013-04-15 12:00 +01:00`.
    pub created: Option<String>,
    pub modified: Option<String>,
    /// The kind of file, as `PDF 1.7`.
    pub format: Option<String>,
    /// How it is encrypted, or `None`.
    pub encryption: Option<String>,
}

impl Info {
    /// The title, if it is one worth naming the document by: not left
    /// empty, and not just the name of the file it was made from, as in
    /// `Microsoft Word - report.docx` or `untitled`.
    pub fn name(&self) -> Option<&str> {
        let title = self.title.as_deref()?;
        let lower = title.to_lowercase();
        let file_name = [
            ".pdf", ".doc", ".docx", ".dvi", ".tex", ".ps", ".indd", ".odt", ".rtf", ".txt",
            ".qxd", ".pages", ".ppt", ".pptx", ".xls", ".xlsx",
        ]
        .iter()
        .any(|extension| lower.ends_with(extension));
        let placeholder = matches!(lower.as_str(), "untitled" | "title" | "untitled document");
        (!file_name && !placeholder && !lower.starts_with("microsoft ")).then_some(title)
    }
}

/// The label of every page, as the document's `/PageLabels` give them, if
/// it has any and they are not all just the page's place counted from 1.
fn page_labels(document: &Document) -> Option<Vec<String>> {
    let pdf = mupdf::pdf::PdfDocument::try_from(document.clone()).ok()?;
    pdf.catalog().ok()?.get_dict("PageLabels").ok()??;
    let count = usize::try_from(pdf.page_count().ok()?).ok()?;
    let labels: Vec<String> = (0..count)
        .map(|page| pdf.page_label(page).unwrap_or_default())
        .collect();
    let plain = labels
        .iter()
        .enumerate()
        .all(|(page, label)| *label == page.saturating_add(1).to_string());
    (!plain).then_some(labels)
}

/// Reads what a document says about itself.
fn read_info(document: &Document) -> Info {
    use mupdf::document::MetadataName;
    let read = |name: MetadataName| {
        document
            .metadata(name)
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    };
    let date = |name: MetadataName| read(name).map(|value| pdf_date(&value));
    Info {
        title: read(MetadataName::Title),
        author: read(MetadataName::Author),
        subject: read(MetadataName::Subject),
        keywords: read(MetadataName::Keywords),
        creator: read(MetadataName::Creator),
        producer: read(MetadataName::Producer),
        created: date(MetadataName::CreationDate),
        modified: date(MetadataName::ModDate),
        format: read(MetadataName::Format),
        encryption: read(MetadataName::Encryption),
    }
}

/// A date as a PDF writes it, `D:20130415120000+01'00'`, as people read
/// it: `2013-04-15 12:00 +01:00`. A date not written that way is left as
/// it is.
fn pdf_date(written: &str) -> String {
    let rest = written.strip_prefix("D:").unwrap_or(written);
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let zone = rest.get(digits.len()..).unwrap_or_default();
    let part = |from: usize, to: usize| digits.get(from..to);
    let Some(year) = part(0, 4) else {
        return written.to_owned();
    };
    let mut date = year.to_owned();
    for (from, to, before) in [(4, 6, "-"), (6, 8, "-"), (8, 10, " "), (10, 12, ":")] {
        match part(from, to) {
            Some(value) => {
                date.push_str(before);
                date.push_str(value);
            }
            None => break,
        }
    }
    // A time with hours but no minutes reads as on the hour.
    if digits.len() == 10 {
        date.push_str(":00");
    }
    let zone = zone.replace('\'', "");
    match zone.as_bytes().first() {
        Some(b'Z') => date.push_str(" UTC"),
        Some(sign @ (b'+' | b'-')) => {
            let numbers: String = zone.chars().skip(1).filter(char::is_ascii_digit).collect();
            if let (Some(hours), minutes) = (numbers.get(0..2), numbers.get(2..4).unwrap_or("00")) {
                date.push(' ');
                date.push(char::from(*sign));
                date.push_str(hours);
                date.push(':');
                date.push_str(minutes);
            }
        }
        _ => {}
    }
    date
}

/// The page a document without pages of its own, as an EPUB book, is laid
/// out on: its size, and the size of its type, all in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flow {
    pub width: f32,
    pub height: f32,
    pub em: f32,
}

impl Flow {
    /// The smallest page laid out on, in points either way.
    const SMALLEST: f32 = 100.0;
    /// The largest page laid out on, in points either way.
    const LARGEST: f32 = 10_000.0;
    /// The smallest and largest type, in points.
    pub const EM: (f32, f32) = (4.0, 72.0);

    /// A page of this size, kept within reason: a window not yet shown,
    /// with no size, gets a page of a book's shape.
    pub fn new(width: f32, height: f32, em: f32) -> Self {
        let fit = |side: f32, otherwise: f32| {
            if side.is_finite() && side >= Self::SMALLEST {
                side.min(Self::LARGEST)
            } else {
                otherwise
            }
        };
        let em = if em.is_finite() { em } else { 12.0 };
        Self {
            width: fit(width, 600.0),
            height: fit(height, 800.0),
            em: em.clamp(Self::EM.0, Self::EM.1),
        }
    }
}

/// A document that has just been opened.
#[derive(Debug, Clone)]
pub struct Opened {
    pub backend: Backend,
    /// The unturned size of every page, in points.
    pub sizes: Vec<PageSize>,
    pub info: Box<Info>,
    /// The number printed on each page, as `xii` or `A-3`, when the
    /// document gives any that are not just the page's place.
    pub labels: Option<Vec<String>>,
    /// The page it was laid out on, for a document without pages of its
    /// own.
    pub flow: Option<Flow>,
}

/// A request to the worker, with the channel to answer on.
enum Request {
    Render(RenderJob, oneshot::Sender<Result<Bitmap, Error>>),
    /// Every page searched, or only these.
    Search(
        Needle,
        Option<Vec<u32>>,
        oneshot::Sender<Result<Vec<Hit>, Error>>,
    ),
    Measure(oneshot::Sender<Result<Vec<Rect>, Error>>),
    Links(Vec<u32>, oneshot::Sender<Result<PageLinks, Error>>),
    Figures(Vec<u32>, oneshot::Sender<Result<PageFigures, Error>>),
    Text(Vec<u32>, oneshot::Sender<Result<PageText, Error>>),
    Markup(Markup, oneshot::Sender<Result<Vec<Annotation>, Error>>),
    Remove(
        Vec<Annotation>,
        oneshot::Sender<Result<Vec<Marking>, Error>>,
    ),
    Restore(
        Vec<Marking>,
        oneshot::Sender<Result<Vec<Annotation>, Error>>,
    ),
    SetNote(
        Annotation,
        String,
        oneshot::Sender<Result<Option<String>, Error>>,
    ),
    Fill(Annotation, String, oneshot::Sender<Result<String, Error>>),
    Fields(oneshot::Sender<Result<Vec<(Rect, Field)>, Error>>),
    Markings(oneshot::Sender<Result<Vec<Marking>, Error>>),
    Save(Option<PathBuf>, oneshot::Sender<Result<(), Error>>),
    Export(Export, oneshot::Sender<Result<(), Error>>),
    Outline(oneshot::Sender<Result<Vec<Chapter>, Error>>),
    GuessOutline(oneshot::Sender<Result<Vec<Chapter>, Error>>),
}

/// An entry in a document's table of contents, with the entries under it.
#[derive(Debug, Clone, PartialEq)]
pub struct Chapter {
    pub title: String,
    /// The page the entry leads to, counted from 0, if it leads anywhere in
    /// the document.
    pub page: Option<u32>,
    /// How far down that page, in points, when the entry says.
    pub top: Option<f32>,
    pub sections: Vec<Self>,
}

/// An annotation pdvim added to a page: the page, counted from 0, and the
/// number of the annotation's object in the file.
pub type Annotation = (u32, i32);

/// The ways text can be marked, each a kind of text markup annotation in
/// the PDF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkupKind {
    Highlight,
    Underline,
    StrikeOut,
    /// A wavy underline.
    Squiggly,
    /// A sticky note, shown as an icon, with text to read.
    Note,
}

impl MarkupKind {
    /// The kind of marking an annotation is, if it is one.
    #[expect(clippy::wildcard_enum_match_arm, reason = "the other annotations are not markings")]
    const fn of(kind: mupdf::pdf::PdfAnnotationType) -> Option<Self> {
        match kind {
            mupdf::pdf::PdfAnnotationType::Highlight => Some(Self::Highlight),
            mupdf::pdf::PdfAnnotationType::Underline => Some(Self::Underline),
            mupdf::pdf::PdfAnnotationType::StrikeOut => Some(Self::StrikeOut),
            mupdf::pdf::PdfAnnotationType::Squiggly => Some(Self::Squiggly),
            mupdf::pdf::PdfAnnotationType::Text => Some(Self::Note),
            _ => None,
        }
    }

    /// The kind of annotation in the PDF.
    const fn annotation(self) -> mupdf::pdf::PdfAnnotationType {
        match self {
            Self::Highlight => mupdf::pdf::PdfAnnotationType::Highlight,
            Self::Underline => mupdf::pdf::PdfAnnotationType::Underline,
            Self::StrikeOut => mupdf::pdf::PdfAnnotationType::StrikeOut,
            Self::Squiggly => mupdf::pdf::PdfAnnotationType::Squiggly,
            Self::Note => mupdf::pdf::PdfAnnotationType::Text,
        }
    }

    /// What a marking of this kind is called.
    pub const fn noun(self) -> &'static str {
        match self {
            Self::Highlight => "highlight",
            Self::Underline => "underline",
            Self::StrikeOut => "strike-out",
            Self::Squiggly => "squiggly underline",
            Self::Note => "note",
        }
    }

    /// What making one did, said of the text.
    pub const fn done(self) -> &'static str {
        match self {
            Self::Highlight => "highlighted",
            Self::Underline => "underlined",
            Self::StrikeOut => "struck out",
            Self::Squiggly => "underlined with a squiggle",
            Self::Note => "note added",
        }
    }
}

/// Text marked in the file, by pdvim or by another program: a highlight,
/// underline, strike-out or squiggly underline.
#[derive(Debug, Clone, PartialEq)]
pub struct Marking {
    /// The page, counted from 0.
    pub page: u32,
    /// The number of the annotation's object in the file.
    pub xref: i32,
    pub kind: MarkupKind,
    /// The areas it covers, one to a line, on the unturned page in points.
    pub areas: Vec<Rect>,
    /// The text it covers.
    pub text: String,
    /// The rest of what is needed to put it back once taken away.
    pub kept: Kept,
}

impl Marking {
    /// The annotation it is.
    pub const fn annotation(&self) -> Annotation {
        (self.page, self.xref)
    }

    /// Its colour, as near as red, green and blue come to it.
    pub fn colour(&self) -> Option<Rgb> {
        let channel = |value: f32| u8::try_from(num::round(value * 255.0)).unwrap_or(u8::MAX);
        self.kept.colour.map(|colour| match colour {
            mupdf::color::AnnotationColor::Gray(grey) => {
                Rgb(channel(grey), channel(grey), channel(grey))
            }
            mupdf::color::AnnotationColor::Rgb { red, green, blue } => {
                Rgb(channel(red), channel(green), channel(blue))
            }
            mupdf::color::AnnotationColor::Cmyk {
                cyan,
                magenta,
                yellow,
                key,
            } => Rgb(
                channel((1.0 - cyan) * (1.0 - key)),
                channel((1.0 - magenta) * (1.0 - key)),
                channel((1.0 - yellow) * (1.0 - key)),
            ),
        })
    }
}

/// What a marking has besides its kind and areas, kept to put it back as
/// it was.
#[derive(Debug, Clone, PartialEq)]
pub struct Kept {
    pub colour: Option<mupdf::color::AnnotationColor>,
    pub opacity: f32,
    /// The note written on it, if any.
    pub contents: Option<String>,
    pub author: Option<String>,
}

impl Default for Kept {
    fn default() -> Self {
        Self {
            colour: None,
            opacity: 1.0,
            contents: None,
            author: None,
        }
    }
}

/// Text to mark in the file: how, and in what colour.
#[derive(Debug, Clone, PartialEq)]
pub struct Markup {
    pub kind: MarkupKind,
    /// The pages, counted from 0, each with the areas to cover on it, one
    /// to a line of text, on the unturned page in points.
    pub pages: Vec<(u32, Vec<Rect>)>,
    pub colour: Rgb,
    /// The text of a note; none for the other kinds.
    pub note: Option<String>,
}

/// A handle to the worker that owns an open document. The worker stops
/// once every handle is dropped.
#[derive(Debug, Clone)]
pub struct Backend {
    requests: mpsc::Sender<Request>,
}

impl Backend {
    /// Hands a request to the worker. A send only fails when the worker is
    /// gone; the reply channel is then dropped too, which the asker's await
    /// reports as closed.
    fn ask(&self, request: Request) {
        let _: Result<(), mpsc::SendError<Request>> = self.requests.send(request);
    }

    /// Renders a page.
    pub fn render(&self, job: RenderJob) -> impl Future<Output = Result<Bitmap, Error>> + use<> {
        let (reply, answer) = oneshot::channel();
        self.ask(Request::Render(job, reply));
        async move { answer.await.unwrap_or(Err(Error::Closed)) }
    }

    /// Finds every place the text occurs, in page order.
    pub fn search(&self, needle: Needle) -> impl Future<Output = Result<Vec<Hit>, Error>> + use<> {
        let (reply, answer) = oneshot::channel();
        self.ask(Request::Search(needle, None, reply));
        async move { answer.await.unwrap_or(Err(Error::Closed)) }
    }

    /// Finds every place the text occurs on some pages, in page order.
    pub fn search_pages(
        &self,
        needle: Needle,
        pages: Vec<u32>,
    ) -> impl Future<Output = Result<Vec<Hit>, Error>> + use<> {
        let (reply, answer) = oneshot::channel();
        self.ask(Request::Search(needle, Some(pages), reply));
        async move { answer.await.unwrap_or(Err(Error::Closed)) }
    }

    /// Finds the links on some pages, each page with its links.
    /// Finds the pictures and figures on some pages.
    pub fn figures(
        &self,
        pages: Vec<u32>,
    ) -> impl Future<Output = Result<PageFigures, Error>> + use<> {
        let (reply, answer) = oneshot::channel();
        self.ask(Request::Figures(pages, reply));
        async move { answer.await.unwrap_or(Err(Error::Closed)) }
    }

    pub fn links(&self, pages: Vec<u32>) -> impl Future<Output = Result<PageLinks, Error>> + use<> {
        let (reply, answer) = oneshot::channel();
        self.ask(Request::Links(pages, reply));
        async move { answer.await.unwrap_or(Err(Error::Closed)) }
    }

    /// Reads the lines of text on some pages.
    /// Writes a page to a file, as an image or a drawing.
    pub fn export(&self, export: Export) -> impl Future<Output = Result<(), Error>> + use<> {
        let (reply, answer) = oneshot::channel();
        self.ask(Request::Export(export, reply));
        async move { answer.await.unwrap_or(Err(Error::Closed)) }
    }

    pub fn text(&self, pages: Vec<u32>) -> impl Future<Output = Result<PageText, Error>> + use<> {
        let (reply, answer) = oneshot::channel();
        self.ask(Request::Text(pages, reply));
        async move { answer.await.unwrap_or(Err(Error::Closed)) }
    }

    /// Highlights text by adding highlight annotations to the PDF in
    /// memory; [`Self::save`] writes them to a file. Answers with the
    /// annotations added, to take away again on undo.
    pub fn highlight(
        &self,
        highlight: Markup,
    ) -> impl Future<Output = Result<Vec<Annotation>, Error>> + use<> {
        let (reply, answer) = oneshot::channel();
        self.ask(Request::Markup(highlight, reply));
        async move { answer.await.unwrap_or(Err(Error::Closed)) }
    }

    /// Takes markings away, in memory. Answers with what they were, to put
    /// them back with [`Self::restore`].
    pub fn remove(
        &self,
        annotations: Vec<Annotation>,
    ) -> impl Future<Output = Result<Vec<Marking>, Error>> + use<> {
        let (reply, answer) = oneshot::channel();
        self.ask(Request::Remove(annotations, reply));
        async move { answer.await.unwrap_or(Err(Error::Closed)) }
    }

    /// Puts back markings that were taken away, in memory. Answers with
    /// the annotations they now are, in the same order.
    pub fn restore(
        &self,
        markings: Vec<Marking>,
    ) -> impl Future<Output = Result<Vec<Annotation>, Error>> + use<> {
        let (reply, answer) = oneshot::channel();
        self.ask(Request::Restore(markings, reply));
        async move { answer.await.unwrap_or(Err(Error::Closed)) }
    }

    /// Sets the note on a marking: the text of a note, or the comment on
    /// another kind; empty for none. Answers with what it was.
    /// Every field of a form in the document that can be filled in, page by
    /// page, in reading order, each with its area.
    pub fn fields(&self) -> impl Future<Output = Result<Vec<(Rect, Field)>, Error>> + use<> {
        let (reply, answer) = oneshot::channel();
        self.ask(Request::Fields(reply));
        async move { answer.await.unwrap_or(Err(Error::Closed)) }
    }

    /// Sets the value of a field of a form. Answers with the value it had.
    pub fn fill(
        &self,
        widget: Annotation,
        value: String,
    ) -> impl Future<Output = Result<String, Error>> + use<> {
        let (reply, answer) = oneshot::channel();
        self.ask(Request::Fill(widget, value, reply));
        async move { answer.await.unwrap_or(Err(Error::Closed)) }
    }

    pub fn set_note(
        &self,
        annotation: Annotation,
        text: String,
    ) -> impl Future<Output = Result<Option<String>, Error>> + use<> {
        let (reply, answer) = oneshot::channel();
        self.ask(Request::SetNote(annotation, text, reply));
        async move { answer.await.unwrap_or(Err(Error::Closed)) }
    }

    /// Finds every marking in the document, in reading order.
    pub fn markings(&self) -> impl Future<Output = Result<Vec<Marking>, Error>> + use<> {
        let (reply, answer) = oneshot::channel();
        self.ask(Request::Markings(reply));
        async move { answer.await.unwrap_or(Err(Error::Closed)) }
    }

    /// Saves the document's changes: into its own file without a target,
    /// where what the file had is left as it was and the changes are
    /// written after it; or as a whole new file at the target, leaving the
    /// document's own file alone.
    pub fn save(&self, target: Option<PathBuf>) -> impl Future<Output = Result<(), Error>> + use<> {
        let (reply, answer) = oneshot::channel();
        self.ask(Request::Save(target, reply));
        async move { answer.await.unwrap_or(Err(Error::Closed)) }
    }

    /// Reads the table of contents, which may be empty.
    pub fn outline(&self) -> impl Future<Output = Result<Vec<Chapter>, Error>> + use<> {
        let (reply, answer) = oneshot::channel();
        self.ask(Request::Outline(reply));
        async move { answer.await.unwrap_or(Err(Error::Closed)) }
    }

    /// Guesses a table of contents from the text of the document, for one
    /// that has none: empty when none can be told. It is read from a copy
    /// of the document opened again, so that the pages go on being drawn
    /// meanwhile.
    pub fn guess_outline(&self) -> impl Future<Output = Result<Vec<Chapter>, Error>> + use<> {
        let (reply, answer) = oneshot::channel();
        self.ask(Request::GuessOutline(reply));
        async move { answer.await.unwrap_or(Err(Error::Closed)) }
    }

    /// Finds the part of every page that is not blank margin, in points
    /// from the top-left corner of the unturned page. A blank page keeps
    /// its whole area.
    pub fn measure(&self) -> impl Future<Output = Result<Vec<Rect>, Error>> + use<> {
        let (reply, answer) = oneshot::channel();
        self.ask(Request::Measure(reply));
        async move { answer.await.unwrap_or(Err(Error::Closed)) }
    }
}

/// Opens a document on a new worker thread, unlocking it with the
/// password if it is locked, and laying it out on the page given if it has
/// no pages of its own.
pub fn open(
    path: PathBuf,
    password: Option<String>,
    flow: Flow,
) -> impl Future<Output = Result<Opened, Error>> {
    fonts::install();
    let (reply, answer) = oneshot::channel();
    let (requests, inbox) = mpsc::channel();
    let name = path.display().to_string();
    let spawned = thread::Builder::new()
        .name("pdvim-document".to_owned())
        .spawn(move || {
            let document = match load(&path, password.as_deref(), flow) {
                Ok((document, sizes, flow)) => {
                    let backend = Backend { requests };
                    let info = Box::new(read_info(&document));
                    let labels = page_labels(&document);
                    if reply
                        .send(Ok(Opened {
                            backend,
                            sizes,
                            info,
                            labels,
                            flow,
                        }))
                        .is_err()
                    {
                        return;
                    }
                    document
                }
                Err(error) => {
                    reply.answer(Err(error));
                    return;
                }
            };
            Worker {
                document,
                path,
                password,
                inbox,
                deferred: VecDeque::new(),
                recordings: VecDeque::new(),
                painters: Painters::new(),
            }
            .serve();
        });
    async move {
        if let Err(error) = spawned {
            return Err(Error::Open {
                path: name,
                reason: error.to_string(),
            });
        }
        answer.await.unwrap_or(Err(Error::Closed))
    }
}

/// Answering a request.
trait Answer<T> {
    /// Sends the answer. The asker may have stopped waiting, as when a page
    /// is scrolled away before it is drawn: then no one wants it.
    fn answer(self, value: T);
}

impl<T> Answer<T> for oneshot::Sender<T> {
    fn answer(self, value: T) {
        let _: Result<(), T> = self.send(value);
    }
}

/// A path as MuPDF takes it: as it is on Unix, and as text on Windows,
/// where it must be valid UTF-8.
#[cfg(unix)]
#[expect(clippy::unnecessary_wraps, reason = "the same result as on Windows, where it can fail")]
const fn file_path(path: &Path) -> Result<&Path, mupdf::Error> {
    Ok(path)
}

#[cfg(not(unix))]
fn file_path(path: &Path) -> Result<&str, mupdf::Error> {
    path.to_str().ok_or(mupdf::Error::InvalidUtf8)
}

/// Opens the file, unlocking it with the password if it is locked, lays it
/// out on the page given if it has no pages of its own, and measures every
/// page.
fn load(
    path: &Path,
    password: Option<&str>,
    flow: Flow,
) -> Result<(Document, Vec<PageSize>, Option<Flow>), Error> {
    let name = path.display().to_string();
    let failed = |error: mupdf::Error| Error::Open {
        path: name.clone(),
        reason: reason(&error, &name),
    };
    let mut document = match unlocked(path, password).map_err(failed)? {
        Unlocked::Open(document) => document,
        Unlocked::Locked => return Err(Error::Encrypted(name)),
        Unlocked::Wrong => return Err(Error::WrongPassword(name)),
    };
    let flow = lay_out(&mut document, flow).map_err(failed)?;
    let sizes = page_sizes(&document, flow).map_err(failed)?;
    Ok((document, sizes, flow))
}

/// Lays a document without pages of its own out on the page given, and
/// says so; leaves any other as it is.
fn lay_out(document: &mut Document, flow: Flow) -> Result<Option<Flow>, mupdf::Error> {
    if !document.is_reflowable()? {
        return Ok(None);
    }
    document.layout(flow.width, flow.height, flow.em)?;
    Ok(Some(flow))
}

/// A document opened, or still locked.
enum Unlocked {
    Open(Document),
    /// It needs a password, and none was given.
    Locked,
    /// The password given does not unlock it.
    Wrong,
}

/// Opens a file, unlocking it with the password if it needs one.
fn unlocked(path: &Path, password: Option<&str>) -> Result<Unlocked, mupdf::Error> {
    let mut document = opened(path, cfg!(windows))?;
    if !document.needs_password()? {
        return Ok(Unlocked::Open(document));
    }
    let Some(password) = password else {
        return Ok(Unlocked::Locked);
    };
    Ok(if document.authenticate(password)? {
        Unlocked::Open(document)
    } else {
        Unlocked::Wrong
    })
}

/// Opens a file. A PDF that MuPDF had to repair is saved by writing it
/// whole beside the file, then putting that in the file's place, which
/// Windows refuses while the file is open. So where `release` is set, such
/// a PDF is read into memory instead, and the file let go.
fn opened(path: &Path, release: bool) -> Result<Document, mupdf::Error> {
    let document = Document::open(file_path(path)?)?;
    let repaired = mupdf::pdf::PdfDocument::try_from(document.clone())
        .is_ok_and(|pdf| !pdf.can_be_saved_incrementally());
    if !(release && repaired) {
        return Ok(document);
    }
    let bytes = std::fs::read(path)?;
    drop(document);
    Document::from_bytes(&bytes, "application/pdf")
}

/// Opens a file again, unlocked as it was the first time.
fn reopen(path: &Path, password: Option<&str>) -> Result<Document, mupdf::Error> {
    match unlocked(path, password)? {
        Unlocked::Open(document) => Ok(document),
        Unlocked::Locked | Unlocked::Wrong => Err(mupdf::Error::InvalidArgument(
            "the document is locked".to_owned(),
        )),
    }
}

/// The unturned size of every page, in points: for a book laid out on a
/// page, that page's, and for any other, each measured. A document of more
/// pages than [`MOST_PAGES`] is refused.
fn page_sizes(document: &Document, flow: Option<Flow>) -> Result<Vec<PageSize>, mupdf::Error> {
    let count = document.page_count()?;
    if count > MOST_PAGES {
        return Err(mupdf::Error::InvalidArgument(format!(
            "it has {count} pages, more than the {MOST_PAGES} pdvim opens"
        )));
    }
    if let Some(flow) = flow {
        let size = PageSize {
            width: flow.width,
            height: flow.height,
        };
        return Ok(vec![size; usize::try_from(count).unwrap_or(0)]);
    }
    let mut sizes = Vec::new();
    for number in 0..count {
        let bounds = document.load_page(number)?.bounds()?;
        sizes.push(PageSize {
            width: bounds.width(),
            height: bounds.height(),
        });
    }
    Ok(sizes)
}

/// Opens bytes as a document of the kind `magic` names, as
/// `application/pdf` or `epub`, and reads up to `pages` pages of it the way
/// the worker reads an open document, for fuzzing. What fails is not
/// reported: only a panic or a crash is a finding.
pub fn examine(bytes: &[u8], magic: &str, pages: u32) {
    fonts::install();
    let Ok(mut document) = Document::from_bytes(bytes, magic) else {
        return;
    };
    if document.needs_password().unwrap_or(true) {
        return;
    }
    let Ok(flow) = lay_out(&mut document, Flow::new(400.0, 600.0, 12.0)) else {
        return;
    };
    let Ok(sizes) = page_sizes(&document, flow) else {
        return;
    };
    let _: Result<Vec<Chapter>, _> = outline(&document, || Document::from_bytes(bytes, magic));
    for number in (0..num::len(sizes.len())).take(num::index(pages)) {
        // The fuzzer stops at the first panic, before an unwind could be
        // caught, so the panic the worker already guards against is let
        // through quietly here.
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let _: Result<Vec<Link>, _> = guarded_links(&document, number);
        std::panic::set_hook(hook);
        let _: Result<Vec<TextLine>, _> = page_text(&document, number);
        let _: Result<Vec<Hit>, _> = page_hits(&document, number, "e");
        let _: Result<Rect, _> = page_content(&document, number);
        // The cap on the size of a rendering is lower than the reader's,
        // as the fuzzer slows every pixel, but it is met the same way: from
        // a recording, played twice as the worker plays it again.
        let Ok(recording) = record(&document, number) else {
            continue;
        };
        for rotation in [Rotation::None, Rotation::Quarter] {
            let _: Result<Bitmap, _> = render(
                &recording,
                RenderJob {
                    page: number,
                    scale: 0.25,
                    rotation,
                    clip: None,
                    palette: Some(Palette::default()),
                },
                1000.0,
            );
        }
    }
}

/// The links on a page, as [`page_links`] finds them, but with a panic in
/// the mupdf crate turned into an error.
fn guarded_links(document: &Document, number: u32) -> Result<Vec<Link>, Error> {
    // The mupdf crate panics on a link whose destination MuPDF cannot
    // resolve. That must not take the worker, and with it the document,
    // down.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        page_links(document, number)
    }))
    .map_err(|_panic| {
        Error::Links(format!(
            "a link on page {} is broken",
            number.saturating_add(1)
        ))
    })?
    .map_err(|error| Error::Links(error.to_string()))
}

/// The matches for a needle on a page.
fn page_hits(document: &Document, number: u32, needle: &str) -> Result<Vec<Hit>, mupdf::Error> {
    let page = document.load_page(page_number(number))?;
    let bounds = page.bounds()?;
    Ok(page
        .search(needle, HITS_PER_PAGE)?
        .iter()
        .map(|quad| Hit {
            page: number,
            area: quad_area(quad, &bounds),
        })
        .collect())
}

/// The places on a page, given its lines of text, where the text searched
/// for occurs, as it says: in its case, or as a word of its own. Spaces in
/// it match any run of space, and the end of a line counts as a space, so
/// that text running on to the next line is found. A place over several
/// lines is a hit on each.
pub fn text_hits(lines: &[TextLine], page: u32, needle: &Needle) -> Vec<Hit> {
    // The page's characters in reading order, each with its line and
    // area; the end of each line a space with neither.
    let mut chars: Vec<(char, Option<(usize, Rect)>)> = Vec::new();
    for (row, line) in lines.iter().enumerate() {
        chars.extend(
            line.chars
                .iter()
                .map(|text_char| (text_char.character, Some((row, text_char.area)))),
        );
        chars.push((' ', None));
    }
    let wanted: Vec<char> = needle
        .text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .collect();
    let same = |a: char, b: char| {
        if needle.match_case {
            a == b
        } else {
            folded(a) == folded(b)
        }
    };
    let is_word = |c: Option<&(char, Option<(usize, Rect)>)>| {
        c.is_some_and(|(c, _)| c.is_alphanumeric() || *c == '_')
    };
    let mut hits = Vec::new();
    let mut start = 0;
    while start < chars.len() && hits.len() < num::index(HITS_PER_PAGE) {
        let Some(end) = match_at(&chars, start, &wanted, same) else {
            start = start.saturating_add(1);
            continue;
        };
        let before = start.checked_sub(1).and_then(|before| chars.get(before));
        if needle.whole_word && (is_word(before) || is_word(chars.get(end))) {
            start = start.saturating_add(1);
            continue;
        }
        // An area for each line the place is on.
        let mut areas: Vec<(usize, Rect)> = Vec::new();
        for (_, place) in chars.get(start..end).unwrap_or_default() {
            let Some((row, area)) = place else {
                continue;
            };
            match areas.last_mut() {
                Some((last, joined)) if last == row => *joined = joined.union(*area),
                _ => areas.push((*row, *area)),
            }
        }
        hits.extend(areas.into_iter().map(|(_, area)| Hit { page, area }));
        start = end.max(start.saturating_add(1));
    }
    hits
}

/// Where the text wanted ends if it starts at this character, past its
/// last character; a space in it takes in a run of spaces.
fn match_at(
    chars: &[(char, Option<(usize, Rect)>)],
    start: usize,
    wanted: &[char],
    same: impl Fn(char, char) -> bool,
) -> Option<usize> {
    let mut at = start;
    for &want in wanted {
        // The end of a line in a script without spaces is no space: the
        // text runs on to the next line.
        if want != ' '
            && at > start
            && let Some((_, None)) = chars.get(at)
            && prose::unspaced(want).is_some()
        {
            at = at.saturating_add(1);
        }
        let (have, _) = *chars.get(at)?;
        if want == ' ' {
            if !have.is_whitespace() {
                return None;
            }
            while chars.get(at).is_some_and(|(c, _)| c.is_whitespace()) {
                at = at.saturating_add(1);
            }
            continue;
        }
        if !same(want, have) {
            return None;
        }
        at = at.saturating_add(1);
    }
    Some(at)
}

/// The area of a page that holds its content, found by rendering it small
/// and looking for what is not paper, with a margin around it. The whole
/// page when it is blank. Annotations are left out of the rendering, so
/// that notes and highlights never change the margins.
fn page_content(document: &Document, number: u32) -> Result<Rect, mupdf::Error> {
    let page = document.load_page(page_number(number))?;
    let bounds = page.bounds()?;
    let whole = Rect {
        left: 0.0,
        top: 0.0,
        right: bounds.width(),
        bottom: bounds.height(),
    };
    let longest = bounds.width().max(bounds.height()).max(1.0);
    let scale = MEASURE_SCALE.min(MEASURE_SIDE / longest);
    let pixmap = page.to_pixmap(
        &Matrix::new_scale(scale, scale),
        &Colorspace::device_gray(),
        false,
        false,
    )?;
    let stride = usize::try_from(pixmap.stride()).unwrap_or(0);
    let content = content_box(pixmap.samples(), stride, num::index(pixmap.width()));
    Ok(content.map_or(whole, |(left, top, right, bottom)| {
        let points = |pixels: usize| num::float(num::len(pixels)) / scale;
        Rect {
            left: (points(left) - TRIM_MARGIN).max(0.0),
            top: (points(top) - TRIM_MARGIN).max(0.0),
            right: (points(right) + TRIM_MARGIN).min(whole.right),
            bottom: (points(bottom) + TRIM_MARGIN).min(whole.bottom),
        }
    }))
}

/// The most outline entries looked at for a missing title, in case the
/// entries run in a circle that the record of those seen does not catch.
const OUTLINE_ENTRIES: usize = 100_000;

/// The table of contents. `reopen` opens the document again, apart from
/// the one given, for when its outline must be mended to be read.
fn outline(
    document: &Document,
    reopen: impl FnOnce() -> Result<Document, mupdf::Error>,
) -> Result<Vec<Chapter>, mupdf::Error> {
    // The mupdf crate reads the title of every entry without checking that
    // there is one, and MuPDF has none for an entry without a /Title, so
    // reading such an outline would crash. Those entries are given empty
    // titles first, in a copy of the document, so that the copy the
    // highlights are saved from is left as the file has it.
    let outlines = match mupdf::pdf::PdfDocument::try_from(document.clone()) {
        Ok(pdf) if untitled(&pdf, false)? => {
            let copy = mupdf::pdf::PdfDocument::try_from(reopen()?)?;
            settle(&copy);
            untitled(&copy, true)?;
            copy.outlines()?
        }
        _ => document.outlines()?,
    };
    Ok(outlines
        .into_iter()
        .map(|outline| chapter(outline, 0))
        .collect())
}

/// The longest a table of contents is guessed for: the pages read by then
/// are guessed from.
const GUESS_TIME: std::time::Duration = std::time::Duration::from_secs(20);

/// Guesses a table of contents from the text of the file, opened again.
/// Books laid out by pdvim have their own, and are left alone.
fn guess_outline(path: &Path, password: Option<&str>) -> Result<Vec<Chapter>, mupdf::Error> {
    let document = reopen(path, password)?;
    if document.is_reflowable()? {
        return Ok(Vec::new());
    }
    let count = u32::try_from(document.page_count()?).unwrap_or(0);
    let start = std::time::Instant::now();
    let mut pages = Vec::new();
    for number in 0..count {
        if start.elapsed() > GUESS_TIME {
            break;
        }
        // A page that cannot be read is left blank, keeping the others in
        // their places.
        pages.push(styled_text(&document, number).unwrap_or_default());
    }
    let labels = page_labels(&document);
    Ok(crate::contents::guess(&pages, labels.as_deref()))
}

/// The lines of a page with the size and weight of their type, for telling
/// headings apart.
fn styled_text(
    document: &Document,
    number: u32,
) -> Result<crate::contents::PageText, mupdf::Error> {
    let page = document.load_page(page_number(number))?;
    let bounds = page.bounds()?;
    let text = page.to_text_page(TextPageFlags::empty())?;
    let mut lines = Vec::new();
    for block in text.blocks() {
        for line in block.lines() {
            let mut words = String::new();
            let mut sizes = 0.0_f32;
            let mut bold = 0_usize;
            let mut count = 0_usize;
            for text_char in line.chars() {
                let Some(character) = text_char.char() else {
                    continue;
                };
                words.push(character);
                if character.is_whitespace() {
                    continue;
                }
                count = count.saturating_add(1);
                sizes += text_char.size();
                if text_char.flags().contains(TextCharFlags::BOLD)
                    || text_char.font().is_some_and(|font| font.is_bold())
                {
                    bold = bold.saturating_add(1);
                }
            }
            let text = words.trim();
            if count == 0 || text.is_empty() {
                continue;
            }
            let area = line.bounds();
            lines.push(crate::contents::Line {
                text: text.to_owned(),
                size: sizes / num::float(num::len(count)),
                bold: bold.saturating_mul(2) > count,
                left: area.x0 - bounds.x0,
                top: area.y0 - bounds.y0,
                bottom: area.y1 - bounds.y0,
            });
        }
    }
    Ok(crate::contents::PageText {
        lines,
        height: bounds.height(),
    })
}

/// Reads every object of a PDF. MuPDF rebuilds its record of where the
/// objects of a damaged file are once it first comes upon a damaged one,
/// and forgets every change made until then. It does so only once, so
/// after this the changes made stay.
fn settle(pdf: &mupdf::pdf::PdfDocument) {
    let mut number = 1;
    // The count is read again each time, since rebuilding can change it.
    while pdf
        .count_objects()
        .is_ok_and(|count| u32::try_from(number).is_ok_and(|number| number < count))
    {
        let _: Result<Option<_>, _> = pdf.xref_object(number);
        number = number.saturating_add(1);
    }
}

/// Whether any entry of a PDF's outline has no title, mending each such
/// entry if `mend` is set. MuPDF has no title for an entry without a
/// /Title, nor for an entry that is not a dictionary at all, as in a
/// damaged file: the first is given an empty title, and the second is
/// left out.
fn untitled(pdf: &mupdf::pdf::PdfDocument, mend: bool) -> Result<bool, mupdf::Error> {
    let Some(outlines) = pdf.catalog()?.get_dict("Outlines")? else {
        return Ok(false);
    };
    // Each entry still to look at, as the entry that leads to it and the
    // key it does so by.
    let mut waiting = vec![(outlines, "First")];
    let mut seen = std::collections::HashSet::new();
    let mut found = false;
    let mut looked = 0_usize;
    while let Some((mut before, key)) = waiting.pop() {
        looked = looked.saturating_add(1);
        if looked > OUTLINE_ENTRIES {
            break;
        }
        let Some(mut entry) = before.get_dict(key)? else {
            continue;
        };
        // Every way to an entry that is not a dictionary is cut, so it is
        // only the dictionaries that are looked at once.
        if !entry.is_dict()? {
            found = true;
            if !mend {
                return Ok(true);
            }
            before.dict_delete(key)?;
            continue;
        }
        if entry.is_indirect()? && !seen.insert(entry.as_indirect()?) {
            continue;
        }
        if entry.get_dict("Title")?.is_none() {
            found = true;
            if !mend {
                return Ok(true);
            }
            entry.dict_put("Title", mupdf::pdf::PdfObject::new_string("")?)?;
        }
        waiting.push((entry.try_clone()?, "Next"));
        waiting.push((entry, "First"));
    }
    Ok(found)
}

/// The links on a page, with their areas measured from the corner of the
/// page.
fn page_links(document: &Document, number: u32) -> Result<Vec<Link>, mupdf::Error> {
    let page = document.load_page(page_number(number))?;
    let bounds = page.bounds()?;
    let links = page
        .links()?
        .map(|link| {
            let target = match link.dest {
                Some(destination) => Target::Page {
                    page: destination.loc.page_number,
                    top: destination_top(destination.kind),
                },
                None => Target::Uri(link.uri),
            };
            Link {
                area: relative(link.bounds, bounds),
                target,
            }
        })
        .collect::<Vec<_>>();
    if !document.is_pdf() {
        return Ok(links);
    }
    let mut links = links;
    links.extend(
        page_fields(page, bounds, number)?
            .into_iter()
            .map(|(area, field)| Link {
                area,
                target: Target::Field(field),
            }),
    );
    Ok(links)
}

/// The fields of a form on a page that can be filled in, each with its
/// area, in the order the page lists them.
fn page_fields(
    page: mupdf::Page,
    bounds: mupdf::Rect,
    number: u32,
) -> Result<Vec<(Rect, Field)>, mupdf::Error> {
    let page = mupdf::pdf::PdfPage::try_from(page)?;
    let mut fields = Vec::new();
    for widget in page.widgets() {
        let Some(kind) = field_kind(&widget)? else {
            continue;
        };
        let name = widget
            .label()?
            .or(widget.name()?)
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "field".to_owned());
        fields.push((
            relative(widget.annotation().bounds()?, bounds),
            Field {
                widget: (number, widget.xref()?),
                kind,
                value: widget.value()?.unwrap_or_default(),
                name,
            },
        ));
    }
    Ok(fields)
}

/// The fields on a page, in reading order: down the page, and along each
/// line.
fn fields_in_order(document: &Document, number: u32) -> Result<Vec<(Rect, Field)>, mupdf::Error> {
    if !document.is_pdf() {
        return Ok(Vec::new());
    }
    let page = document.load_page(page_number(number))?;
    let bounds = page.bounds()?;
    let mut fields = page_fields(page, bounds, number)?;
    fields.sort_by(|(one, _), (other, _)| {
        one.top
            .total_cmp(&other.top)
            .then(one.left.total_cmp(&other.left))
    });
    Ok(fields)
}

/// An area of a page as measured from its top-left corner.
fn relative(area: mupdf::Rect, page: mupdf::Rect) -> Rect {
    Rect {
        left: area.x0 - page.x0,
        top: area.y0 - page.y0,
        right: area.x1 - page.x0,
        bottom: area.y1 - page.y0,
    }
}

/// What can be put in a widget of a form; none for one that cannot be
/// filled in, as a button, a signature, or a field made read-only.
fn field_kind(widget: &mupdf::pdf::PdfWidget) -> Result<Option<FieldKind>, mupdf::Error> {
    use mupdf::pdf::WidgetType;
    if widget.is_readonly()? {
        return Ok(None);
    }
    Ok(match widget.r#type()? {
        WidgetType::Text => Some(FieldKind::Text),
        WidgetType::Combobox | WidgetType::Listbox => Some(FieldKind::Choice(options(widget)?)),
        WidgetType::Checkbox => Some(FieldKind::Checkbox {
            on: on_state(widget)?,
        }),
        WidgetType::RadioButton => Some(FieldKind::Radio {
            on: on_state(widget)?,
        }),
        WidgetType::Button | WidgetType::Signature | WidgetType::Unknown => None,
    })
}

/// The name of the state a box is in when ticked: its appearance's other
/// state than `Off`, or `Yes` when it gives none.
fn on_state(widget: &mupdf::pdf::PdfWidget) -> Result<String, mupdf::Error> {
    let Some(states) = widget
        .annotation()
        .object()
        .get_dict("AP")?
        .and_then(|appearance| appearance.get_dict("N").transpose())
        .transpose()?
    else {
        return Ok("Yes".to_owned());
    };
    for index in 0..i32::try_from(states.dict_len()?).unwrap_or(i32::MAX) {
        if let Some(key) = states.get_dict_key(index)? {
            let name = String::from_utf8_lossy(&key.as_name()?).into_owned();
            if name != OFF {
                return Ok(name);
            }
        }
    }
    Ok("Yes".to_owned())
}

/// The values a choice field offers, as the form stores them.
fn options(widget: &mupdf::pdf::PdfWidget) -> Result<Vec<String>, mupdf::Error> {
    let Some(list) = widget.annotation().object().get_dict_inheritable("Opt")? else {
        return Ok(Vec::new());
    };
    let mut found = Vec::new();
    for index in 0..i32::try_from(list.len()?).unwrap_or(i32::MAX) {
        let Some(option) = list.get_array(index)? else {
            continue;
        };
        // An option is its value, or a pair of its value and what to show.
        let value = match option.get_array(0)? {
            Some(first) if option.is_array()? => first,
            _ => option,
        };
        found.push(value.as_string()?);
    }
    Ok(found)
}

/// Sets the value of a field of a form, and answers with the value it had.
fn fill_in(document: &Document, (number, xref): Annotation, value: &str) -> Result<String, Error> {
    let failed = |error: mupdf::Error| Error::Form(error.to_string());
    let mut pdf = mupdf::pdf::PdfDocument::try_from(document.clone())
        .map_err(|_not_pdf| Error::Form("only PDF files have forms".to_owned()))?;
    if !pdf.can_be_saved_incrementally() {
        settle(&pdf);
    }
    let page = pdf.load_page(page_number(number)).map_err(failed)?;
    let page = mupdf::pdf::PdfPage::try_from(page).map_err(failed)?;
    let Some(mut widget) = page
        .widgets()
        .find(|widget| widget.xref().is_ok_and(|found| found == xref))
    else {
        return Err(Error::Form("the field is gone".to_owned()));
    };
    let before = widget.value().map_err(failed)?.unwrap_or_default();
    // The form's scripts are not run: a document may not run code here.
    if !widget.set_value(&mut pdf, value, true).map_err(failed)? {
        return Err(Error::Form(format!("the field does not take {value:?}")));
    }
    widget.update().map_err(failed)?;
    Ok(before)
}

/// Adds a highlight annotation to each page, over the areas given, and
/// saves the file incrementally. Answers with the
/// annotations added.
fn highlight_in(document: &Document, highlight: &Markup) -> Result<Vec<Annotation>, Error> {
    let failed = |error: mupdf::Error| Error::Markup(error.to_string());
    let pdf = editable(document)?;
    let Rgb(red, green, blue) = highlight.colour;
    let colour = mupdf::color::AnnotationColor::Rgb {
        red: f32::from(red) / 255.0,
        green: f32::from(green) / 255.0,
        blue: f32::from(blue) / 255.0,
    };
    let mut added = Vec::new();
    for (number, areas) in &highlight.pages {
        let page = pdf.load_page(page_number(*number)).map_err(failed)?;
        let bounds = page.bounds().map_err(failed)?;
        let mut page = mupdf::pdf::PdfPage::try_from(page).map_err(failed)?;
        let mut annotation = page
            .create_annotation(highlight.kind.annotation())
            .map_err(failed)?;
        annotation.set_color(colour).map_err(failed)?;
        if highlight.kind == MarkupKind::Note {
            annotation.set_opacity(NOTE_OPACITY).map_err(failed)?;
        }
        place(&mut annotation, highlight.kind, areas, &bounds).map_err(failed)?;
        if let Some(note) = &highlight.note {
            annotation.set_contents(note).map_err(failed)?;
        }
        annotation.update().map_err(failed)?;
        added.push((*number, annotation.xref().map_err(failed)?));
    }
    Ok(added)
}

/// How much of the text under a note's icon it hides: the rest shows
/// through.
const NOTE_OPACITY: f32 = 0.6;

/// Takes markings away. Annotations no longer there are passed over.
/// Answers with what was taken away.
fn remove_in(document: &Document, annotations: &[Annotation]) -> Result<Vec<Marking>, Error> {
    let failed = |error: mupdf::Error| Error::Unmark(error.to_string());
    let pdf = editable(document)?;
    let mut pages: Vec<u32> = annotations.iter().map(|(page, _)| *page).collect();
    pages.sort_unstable();
    pages.dedup();
    let mut removed = Vec::new();
    for number in pages {
        let page = pdf.load_page(page_number(number)).map_err(failed)?;
        let bounds = page.bounds().map_err(failed)?;
        let mut page = mupdf::pdf::PdfPage::try_from(page).map_err(failed)?;
        let doomed: Vec<_> = page
            .annotations()
            .filter(|annotation| {
                annotation
                    .xref()
                    .is_ok_and(|xref| annotations.contains(&(number, xref)))
            })
            .collect();
        if doomed.is_empty() {
            continue;
        }
        let lines = page_text(document, number).map_err(failed)?;
        for annotation in doomed {
            if let Some(marking) = read_marking(number, &annotation, &bounds, &lines) {
                removed.push(marking);
            }
            page.delete_annotation(annotation).map_err(failed)?;
        }
    }
    Ok(removed)
}

/// Puts back markings that were taken away, as they were. Answers with
/// the annotations they now are, in the same order.
fn restore_in(document: &Document, markings: &[Marking]) -> Result<Vec<Annotation>, Error> {
    let failed = |error: mupdf::Error| Error::Markup(error.to_string());
    let pdf = editable(document)?;
    let mut restored = Vec::new();
    for marking in markings {
        let page = pdf.load_page(page_number(marking.page)).map_err(failed)?;
        let bounds = page.bounds().map_err(failed)?;
        let mut page = mupdf::pdf::PdfPage::try_from(page).map_err(failed)?;
        let mut annotation = page
            .create_annotation(marking.kind.annotation())
            .map_err(failed)?;
        if let Some(colour) = marking.kept.colour {
            annotation.set_color(colour).map_err(failed)?;
        }
        place(&mut annotation, marking.kind, &marking.areas, &bounds).map_err(failed)?;
        if marking.kept.opacity < 1.0 {
            annotation
                .set_opacity(marking.kept.opacity)
                .map_err(failed)?;
        }
        if let Some(contents) = &marking.kept.contents {
            annotation.set_contents(contents).map_err(failed)?;
        }
        if let Some(author) = &marking.kept.author {
            annotation.set_author(author).map_err(failed)?;
        }
        annotation.update().map_err(failed)?;
        restored.push((marking.page, annotation.xref().map_err(failed)?));
    }
    Ok(restored)
}

/// Sets the note on a marking, empty for none. Answers with what it was.
fn set_note_in(
    document: &Document,
    (number, xref): Annotation,
    text: &str,
) -> Result<Option<String>, Error> {
    let failed = |error: mupdf::Error| Error::Markup(error.to_string());
    let pdf = editable(document)?;
    let page = pdf.load_page(page_number(number)).map_err(failed)?;
    let page = mupdf::pdf::PdfPage::try_from(page).map_err(failed)?;
    let Some(mut annotation) = page
        .annotations()
        .find(|annotation| annotation.xref().is_ok_and(|found| found == xref))
    else {
        return Err(Error::Markup("the marking is gone".to_owned()));
    };
    let before = annotation
        .contents()
        .map_err(failed)?
        .filter(|contents| !contents.is_empty())
        .map(str::to_owned);
    annotation.set_contents(text).map_err(failed)?;
    annotation.update().map_err(failed)?;
    Ok(before)
}

/// The markings on a page, in the order the page lists them.
fn page_markings(document: &Document, number: u32) -> Result<Vec<Marking>, mupdf::Error> {
    if !document.is_pdf() {
        return Ok(Vec::new());
    }
    let page = document.load_page(page_number(number))?;
    let bounds = page.bounds()?;
    let page = mupdf::pdf::PdfPage::try_from(page)?;
    let annotations: Vec<_> = page
        .annotations()
        .filter(|annotation| {
            annotation
                .r#type()
                .is_ok_and(|kind| MarkupKind::of(kind).is_some())
        })
        .collect();
    if annotations.is_empty() {
        return Ok(Vec::new());
    }
    let lines = page_text(document, number)?;
    Ok(annotations
        .iter()
        .filter_map(|annotation| read_marking(number, annotation, &bounds, &lines))
        .collect())
}

/// What a marking is, from its annotation, with the text under it from the
/// lines of its page. None for an annotation that is not a marking, or
/// cannot be read.
fn read_marking(
    page: u32,
    annotation: &mupdf::pdf::PdfAnnotation,
    bounds: &mupdf::Rect,
    lines: &[TextLine],
) -> Option<Marking> {
    let kind = MarkupKind::of(annotation.r#type().ok()?)?;
    let xref = annotation.xref().ok().filter(|&xref| xref > 0)?;
    let areas: Vec<Rect> = if kind == MarkupKind::Note {
        let rect = annotation.rect().ok()?;
        vec![Rect {
            left: rect.x0 - bounds.x0,
            top: rect.y0 - bounds.y0,
            right: rect.x1 - bounds.x0,
            bottom: rect.y1 - bounds.y0,
        }]
    } else {
        annotation
            .quad_points()
            .ok()?
            .iter()
            .map(|quad| quad_area(quad, bounds))
            .collect()
    };
    let owned = |text: Option<&str>| text.filter(|text| !text.is_empty()).map(str::to_owned);
    let text = if kind == MarkupKind::Note {
        owned(annotation.contents().ok().flatten()).unwrap_or_default()
    } else {
        text_within(lines, &areas)
    };
    Some(Marking {
        page,
        xref,
        kind,
        text,
        areas,
        kept: Kept {
            colour: annotation.color().ok().flatten(),
            opacity: annotation.opacity().unwrap_or(1.0),
            contents: owned(annotation.contents().ok().flatten()),
            author: owned(annotation.author().ok().flatten()),
        },
    })
}

/// The text of the characters whose middles are inside any of the areas,
/// a line's worth apart by a space.
fn text_within(lines: &[TextLine], areas: &[Rect]) -> String {
    let inside = |area: &Rect| {
        let x = f32::midpoint(area.left, area.right);
        let y = f32::midpoint(area.top, area.bottom);
        areas.iter().any(|around| {
            (around.left..=around.right).contains(&x) && (around.top..=around.bottom).contains(&y)
        })
    };
    // The part of each line under the marking, joined as running text,
    // words broken over lines made whole.
    let parts: Vec<String> = lines
        .iter()
        .map(|line| {
            line.chars
                .iter()
                .filter(|character| inside(&character.area))
                .map(|character| character.character)
                .collect()
        })
        .collect();
    prose::join(parts.iter().map(|part| (part.as_str(), false)))
}

/// Puts an annotation over areas of a page, measured from its corner: a
/// note at the first area, which is where its icon goes, and any other
/// marking over every area.
fn place(
    annotation: &mut mupdf::pdf::PdfAnnotation,
    kind: MarkupKind,
    areas: &[Rect],
    bounds: &mupdf::Rect,
) -> Result<(), mupdf::Error> {
    if kind != MarkupKind::Note {
        return annotation.set_quad_points(quads(areas, bounds));
    }
    let Some(area) = areas.first() else {
        return Ok(());
    };
    annotation.set_rect(mupdf::Rect {
        x0: bounds.x0 + area.left,
        y0: bounds.y0 + area.top,
        x1: bounds.x0 + area.right,
        y1: bounds.y0 + area.bottom,
    })
}

/// Areas of a page, measured from its corner, as quads on the page.
fn quads(areas: &[Rect], bounds: &mupdf::Rect) -> Vec<mupdf::Quad> {
    areas
        .iter()
        .map(|area| {
            mupdf::Quad::from(mupdf::Rect {
                x0: bounds.x0 + area.left,
                y0: bounds.y0 + area.top,
                x1: bounds.x0 + area.right,
                y1: bounds.y0 + area.bottom,
            })
        })
        .collect()
}

/// The document as a PDF that can be changed. A file MuPDF had to repair
/// is read through first, so that a repair it does later cannot forget the
/// changes; saving such a file rewrites it whole.
fn editable(document: &Document) -> Result<mupdf::pdf::PdfDocument, Error> {
    let pdf = mupdf::pdf::PdfDocument::try_from(document.clone())
        .map_err(|_not_pdf| Error::Markup("only PDF files can be marked".to_owned()))?;
    if !pdf.can_be_saved_incrementally() {
        settle(&pdf);
    }
    Ok(pdf)
}

/// Saves the changes to a PDF. Into its own file, the save is incremental:
/// what was in the file when it was opened is left as it was, and every
/// change since is written after it. MuPDF rewrites that tail on each
/// save, so the file can shrink when a change is taken back. A file MuPDF
/// had to repair cannot be added to that way, so it is written whole to a
/// file beside it, which then takes its place: the file is never left half
/// written. To a target, the whole document is written as a new file.
fn save_to(document: &Document, path: &Path, target: Option<&Path>) -> Result<(), Error> {
    let failed = |error: mupdf::Error| Error::Save(error.to_string());
    let pdf = mupdf::pdf::PdfDocument::try_from(document.clone())
        .map_err(|_not_pdf| Error::Save("only PDF files can be saved".to_owned()))?;
    if let Some(target) = target {
        return pdf
            .save_with_options(writable(target)?, mupdf::pdf::PdfWriteOptions::default())
            .map_err(failed);
    }
    if pdf.can_be_saved_incrementally() {
        let mut options = mupdf::pdf::PdfWriteOptions::default();
        options.set_incremental(true);
        return pdf
            .save_with_options(writable(path)?, options)
            .map_err(failed);
    }
    let (beside, file) = rewrite_file(path).map_err(|error| Error::Save(error.to_string()))?;
    let written = write_whole(&pdf, file).and_then(|()| {
        std::fs::rename(&beside, path).map_err(|error| Error::Save(error.to_string()))
    });
    if written.is_err() {
        let _: std::io::Result<()> = std::fs::remove_file(&beside);
    }
    written
}

/// Writes a whole PDF into a file pdvim has just made, and makes sure it
/// reached the disk. MuPDF is handed the open file, not its name, so it
/// cannot be led by a link put in its place to write somewhere else.
fn write_whole(pdf: &mupdf::pdf::PdfDocument, file: std::fs::File) -> Result<(), Error> {
    let mut writer = std::io::BufWriter::new(file);
    pdf.write_to_with_options(&mut writer, mupdf::pdf::PdfWriteOptions::default())
        .map_err(|error| Error::Save(error.to_string()))?;
    let file = writer
        .into_inner()
        .map_err(|error| Error::Save(error.error().to_string()))?;
    file.sync_all()
        .map_err(|error| Error::Save(error.to_string()))
}

/// A path as MuPDF takes it, to write to.
fn writable(path: &Path) -> Result<&str, Error> {
    path.to_str().ok_or_else(|| {
        Error::Save(format!(
            "{} is not a path MuPDF can write to",
            path.display()
        ))
    })
}

/// Where a file being rewritten whole is written first: beside it, hidden,
/// so that putting it in place is a rename on the same disk.
fn rewrite_path(path: &Path, attempt: u32) -> PathBuf {
    let mut name = std::ffi::OsString::from(".");
    name.push(path.file_name().unwrap_or_default());
    name.push(format!(".{}-{attempt}.pdvim-save", std::process::id()));
    path.with_file_name(name)
}

/// A new file beside a document, to write it whole into, made by pdvim
/// alone: where a file or link of that name is there already, another name
/// is tried. It takes the permissions of the document, which it replaces.
fn rewrite_file(path: &Path) -> std::io::Result<(PathBuf, std::fs::File)> {
    let permissions = std::fs::metadata(path)?.permissions();
    let mut attempt = 0_u32;
    loop {
        let beside = rewrite_path(path, attempt);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&beside)
        {
            Ok(file) => {
                if let Err(error) = file.set_permissions(permissions) {
                    let _: std::io::Result<()> = std::fs::remove_file(&beside);
                    return Err(error);
                }
                return Ok((beside, file));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && attempt < 100 => {
                attempt = attempt.saturating_add(1);
            }
            Err(error) => return Err(error),
        }
    }
}

/// The box around a quad, measured from the corner of the page.
fn quad_area(quad: &mupdf::Quad, bounds: &mupdf::Rect) -> Rect {
    let corners = [quad.ul, quad.ur, quad.ll, quad.lr];
    let xs = corners.iter().map(|corner| corner.x - bounds.x0);
    let ys = corners.iter().map(|corner| corner.y - bounds.y0);
    Rect {
        left: xs.clone().fold(f32::INFINITY, f32::min),
        top: ys.clone().fold(f32::INFINITY, f32::min),
        right: xs.fold(f32::NEG_INFINITY, f32::max),
        bottom: ys.fold(f32::NEG_INFINITY, f32::max),
    }
}

/// The lines of text on a page, with each character's area measured from
/// the corner of the page. Lines with no characters are left out.
pub fn page_text(document: &Document, number: u32) -> Result<Vec<TextLine>, mupdf::Error> {
    let page = document.load_page(page_number(number))?;
    let bounds = page.bounds()?;
    let text = page.to_text_page(TextPageFlags::empty())?;
    let mut lines = Vec::new();
    for block in text.blocks() {
        for line in block.lines() {
            // Each character, and whether MuPDF made it up: a space where
            // it saw a gap between letters.
            let chars: Vec<(TextChar, bool)> = line
                .chars()
                .filter_map(|text_char| {
                    let made_up = text_char.flags().contains(TextCharFlags::SYNTHETIC);
                    let character = TextChar {
                        character: text_char.char()?,
                        area: quad_area(&text_char.quad(), &bounds),
                    };
                    Some((character, made_up))
                })
                .collect();
            let chars = if right_to_left(chars.iter().map(|(text_char, _)| text_char.character)) {
                in_reading_order(chars)
            } else {
                chars.into_iter().map(|(text_char, _)| text_char).collect()
            };
            if !chars.is_empty() {
                lines.push(TextLine { chars });
            }
        }
    }
    Ok(lines)
}

/// Whether a character is a letter of a script written right to left:
/// Hebrew, Arabic, Syriac, Thaana, N'Ko and those near them, and their
/// presentation forms.
fn is_right_to_left(character: char) -> bool {
    matches!(u32::from(character),
        0x0590..=0x08ff | 0xfb1d..=0xfdff | 0xfe70..=0xfefe | 0x10800..=0x10fff | 0x1e800..=0x1efff
    )
}

/// Whether a line is written right to left: more of its letters are of a
/// script written so than of any other.
fn right_to_left(characters: impl Iterator<Item = char>) -> bool {
    let (right, left) = characters
        .filter(|character| character.is_alphabetic())
        .fold((0_usize, 0_usize), |(right, left), character| {
            if is_right_to_left(character) {
                (right.saturating_add(1), left)
            } else {
                (right, left.saturating_add(1))
            }
        });
    right > left
}

/// The characters of a line written right to left in the order they are
/// read. MuPDF gives each run of text in order, but the runs of a line
/// left to right, so a sentence that ends partway along a line comes out
/// in pieces: read from the right edge, they come together. Numbers and
/// words of left-to-right scripts within the line are then turned back to
/// read left to right, and each vowel mark put after the letter it is on.
/// The spaces MuPDF made up from gaps between the joined letters of Arabic
/// are left out where the line has spaces of its own.
fn in_reading_order(chars: Vec<(TextChar, bool)>) -> Vec<TextChar> {
    let spaced = chars
        .iter()
        .any(|(text_char, made_up)| text_char.character == ' ' && !made_up);
    let mut chars: Vec<TextChar> = chars
        .into_iter()
        .filter(|(text_char, made_up)| !(spaced && *made_up && text_char.character == ' '))
        .map(|(text_char, _)| text_char)
        .collect();
    let middle = |text_char: &TextChar| text_char.area.left + text_char.area.right;
    chars.sort_by(|one, other| middle(other).total_cmp(&middle(one)));
    // A run of left-to-right text, read from its left.
    let left_to_right = |text_char: &TextChar| {
        text_char.character.is_alphanumeric() && !is_right_to_left(text_char.character)
    };
    let mut start = 0;
    while start < chars.len() {
        if !chars.get(start).is_some_and(left_to_right) {
            start = start.saturating_add(1);
            continue;
        }
        let mut end = start;
        while chars.get(end).is_some_and(|text_char| {
            left_to_right(text_char) || prose::is_mark(text_char.character)
        }) {
            end = end.saturating_add(1);
        }
        if let Some(run) = chars.get_mut(start..end) {
            run.reverse();
        }
        start = end;
    }
    // A mark read before the letter it sits over goes after it. Where a
    // PDF gives a letter and its mark as one glyph, MuPDF gives the mark
    // the glyph and the letter no width at the glyph's left edge.
    for index in 1..chars.len() {
        let before = index.saturating_sub(1);
        let (Some(mark), Some(letter)) = (chars.get(before), chars.get(index)) else {
            continue;
        };
        let over = |text_char: &TextChar| {
            let centre = f32::midpoint(mark.area.left, mark.area.right);
            let within = text_char.area.left <= centre && centre <= text_char.area.right;
            let sharing = text_char.area.right <= text_char.area.left
                && (text_char.area.left - mark.area.left).abs() < 1.0;
            within || sharing
        };
        if prose::is_mark(mark.character) && !prose::is_mark(letter.character) && over(letter) {
            chars.swap(before, index);
        }
    }
    chars
}

/// Writes a page to a file: as a PNG image at the scale asked for, or an
/// SVG drawing, turned as asked, with its markings, in its own colours. A
/// file there already is left alone unless the export is forced.
fn export_page(document: &Document, export: &Export) -> Result<(), Error> {
    let failed = |error: &dyn std::fmt::Display| Error::Export(error.to_string());
    let format = ExportFormat::of(&export.target)
        .ok_or_else(|| Error::Export("name a .png or .svg file".to_owned()))?;
    let page = document
        .load_page(page_number(export.page))
        .map_err(|error| failed(&error))?;
    let bytes = match format {
        ExportFormat::Svg => {
            // Turned about its corner, the page lands off the drawing's
            // frame, which starts at 0, 0, so it is moved back onto it.
            let bounds = page.bounds().map_err(|error| failed(&error))?;
            let mut transform = Matrix::IDENTITY;
            transform.rotate(export.rotation.degrees());
            let turned = bounds.transform(&transform);
            transform.e -= turned.x0;
            transform.f -= turned.y0;
            page.to_svg(&transform)
                .map_err(|error| failed(&error))?
                .into_bytes()
        }
        ExportFormat::Png => {
            let bounds = page.bounds().map_err(|error| failed(&error))?;
            let longest = bounds.width().max(bounds.height()).max(1.0);
            let scale = export.scale.max(0.01).min(EXPORT_LONGEST_SIDE / longest);
            let mut transform = Matrix::new_scale(scale, scale);
            transform.rotate(export.rotation.degrees());
            let pixmap = page
                .to_pixmap(&transform, &Colorspace::device_rgb(), false, true)
                .map_err(|error| failed(&error))?;
            let mut bytes = Vec::new();
            pixmap
                .write_to(&mut bytes, ImageFormat::PNG)
                .map_err(|error| failed(&error))?;
            bytes
        }
    };
    let mut options = std::fs::OpenOptions::new();
    options.write(true);
    if export.force {
        options.create(true).truncate(true);
    } else {
        options.create_new(true);
    }
    let mut file = options.open(&export.target).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            Error::Export(format!(
                "{} exists; :export! writes over it",
                export.target.display()
            ))
        } else {
            failed(&error)
        }
    })?;
    std::io::Write::write_all(&mut file, &bytes).map_err(|error| failed(&error))
}

/// The pictures and figures on a page: each picture, and each drawing
/// made of lines and shapes, such as a chart or a table, with any picture
/// in it, measured from the corner of the page. The page is drawn to a
/// device that only notes where each picture and shape lands; MuPDF's own
/// reading of the page leaves out the shapes that are only outlined.
fn page_figures(document: &Document, number: u32) -> Result<Vec<Rect>, mupdf::Error> {
    let page = document.load_page(page_number(number))?;
    let bounds = page.bounds()?;
    let found = std::rc::Rc::new(std::cell::RefCell::new(Drawn::default()));
    {
        let device = Device::from_native(Noting(std::rc::Rc::clone(&found)))?;
        page.run(&device, &Matrix::IDENTITY)?;
    }
    let drawn = found.take();
    let area = |rect: &mupdf::Rect| Rect {
        left: rect.x0 - bounds.x0,
        top: rect.y0 - bounds.y0,
        right: rect.x1 - bounds.x0,
        bottom: rect.y1 - bounds.y0,
    };
    let pictures: Vec<Rect> = drawn.pictures.iter().map(area).collect();
    let shapes: Vec<Rect> = drawn.shapes.iter().map(area).collect();
    let page_area = Rect {
        left: 0.0,
        top: 0.0,
        right: bounds.width(),
        bottom: bounds.height(),
    };
    Ok(figures(&pictures, &shapes, page_area))
}

/// Where the pictures and shapes on a page land.
#[derive(Default)]
struct Drawn {
    pictures: Vec<mupdf::Rect>,
    shapes: Vec<mupdf::Rect>,
}

/// A device that draws nothing, and notes where pictures and shapes land.
struct Noting(std::rc::Rc<std::cell::RefCell<Drawn>>);

impl Noting {
    fn shape(&self, path: &mupdf::Path, stroke: &mupdf::StrokeState, transform: &Matrix) {
        let mut drawn = self.0.borrow_mut();
        if drawn.shapes.len() < FIGURE_SHAPES
            && let Ok(area) = path.bounds(stroke, transform)
        {
            drawn.shapes.push(area);
        }
    }
}

impl mupdf::NativeDevice for Noting {
    fn fill_path(
        &mut self,
        path: &mupdf::Path,
        _even_odd: bool,
        transform: Matrix,
        _colour_space: &Colorspace,
        _colour: &[f32],
        _alpha: f32,
        _params: mupdf::ColorParams,
    ) {
        self.shape(path, &mupdf::StrokeState::default(), &transform);
    }

    fn stroke_path(
        &mut self,
        path: &mupdf::Path,
        stroke: &mupdf::StrokeState,
        transform: Matrix,
        _colour_space: &Colorspace,
        _colour: &[f32],
        _alpha: f32,
        _params: mupdf::ColorParams,
    ) {
        self.shape(path, stroke, &transform);
    }

    fn fill_image(
        &mut self,
        _image: &mupdf::Image,
        transform: Matrix,
        _alpha: f32,
        _params: mupdf::ColorParams,
    ) {
        // A picture fills the square from 0 to 1, moved into place.
        let unit = mupdf::Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 1.0,
            y1: 1.0,
        };
        self.0.borrow_mut().pictures.push(unit.transform(&transform));
    }
}

/// How near, in points, shapes are to count as one drawing.
const FIGURE_GAP: f32 = 8.0;
/// How small, in points along either side, a figure may be.
const FIGURE_SMALLEST: f32 = 24.0;
/// The most shapes on a page drawn into figures; past these, the rest are
/// left out, as finding which belong together would take too long.
const FIGURE_SHAPES: usize = 4000;

/// Gathers pictures and shapes into figures: those that touch or nearly
/// touch are one. Shapes that fill most of the page, such as its
/// background, are left out, and so are figures too small to be anything
/// but a rule or a dot.
fn figures(pictures: &[Rect], shapes: &[Rect], page: Rect) -> Vec<Rect> {
    let size = |area: &Rect| (area.right - area.left) * (area.bottom - area.top);
    let page_size = size(&page);
    let parts: Vec<Rect> = pictures
        .iter()
        .copied()
        .chain(
            shapes
                .iter()
                .take(FIGURE_SHAPES)
                .filter(|shape| size(shape) < page_size * 0.9)
                .copied(),
        )
        .collect();
    let near = |area: &Rect, other: &Rect| {
        other.left <= area.right + FIGURE_GAP
            && area.left <= other.right + FIGURE_GAP
            && other.top <= area.bottom + FIGURE_GAP
            && area.top <= other.bottom + FIGURE_GAP
    };
    let join = |area: Rect, other: Rect| Rect {
        left: area.left.min(other.left),
        top: area.top.min(other.top),
        right: area.right.max(other.right),
        bottom: area.bottom.max(other.bottom),
    };
    // First the parts near one another, each pair looked at once: each
    // part points to one in its group, and the group's first points to
    // itself.
    let mut group: Vec<usize> = (0..parts.len()).collect();
    let first = |group: &mut Vec<usize>, part: usize| {
        let mut leader = part;
        while let Some(&up) = group.get(leader).filter(|up| **up != leader) {
            leader = up;
        }
        // Each part on the way points straight to it from now on, so
        // that the way stays short.
        let mut on = part;
        while let Some(slot) = group.get_mut(on).filter(|slot| **slot != leader) {
            on = std::mem::replace(slot, leader);
        }
        leader
    };
    for (index, area) in parts.iter().enumerate() {
        for (other, other_area) in parts.iter().enumerate().skip(index.saturating_add(1)) {
            if near(area, other_area) {
                let (one, two) = (first(&mut group, index), first(&mut group, other));
                if let Some(slot) = group.get_mut(two) {
                    *slot = one;
                }
            }
        }
    }
    let mut groups: Vec<(usize, Rect)> = Vec::new();
    for (index, area) in parts.iter().enumerate() {
        let leader = first(&mut group, index);
        match groups.iter_mut().find(|(other, _)| *other == leader) {
            Some((_, joined)) => *joined = join(*joined, *area),
            None => groups.push((leader, *area)),
        }
    }
    // Then the groups that have grown near one another, few as they are,
    // until none are.
    let mut parts: Vec<Rect> = groups.into_iter().map(|(_, area)| area).collect();
    let mut index = 0;
    while let Some(&area) = parts.get(index) {
        let found = parts
            .iter()
            .enumerate()
            .skip(index.saturating_add(1))
            .find(|(_, other)| near(&area, other))
            .map(|(other, _)| other);
        if let Some(other) = found {
            let other = parts.swap_remove(other);
            if let Some(part) = parts.get_mut(index) {
                *part = join(area, other);
            }
            index = 0;
        } else {
            index = index.saturating_add(1);
        }
    }
    parts
        .into_iter()
        .filter(|area| {
            area.right - area.left >= FIGURE_SMALLEST && area.bottom - area.top >= FIGURE_SMALLEST
        })
        .map(|area| Rect {
            left: area.left.max(page.left),
            top: area.top.max(page.top),
            right: area.right.min(page.right),
            bottom: area.bottom.min(page.bottom),
        })
        .collect()
}

/// How deep a table of contents goes, chapter in chapter, before what is
/// deeper still is left out. Real documents go a handful deep; a document
/// made to go far deeper would otherwise overflow the stack of whatever
/// walks it, as each level is a call.
const CHAPTER_DEPTH: usize = 64;

/// An entry of MuPDF's outline as pdvim's own, at a depth in it, counted
/// from 0.
fn chapter(outline: mupdf::Outline, depth: usize) -> Chapter {
    let sections = if depth < CHAPTER_DEPTH {
        let deeper = depth.saturating_add(1);
        outline
            .down
            .into_iter()
            .map(|section| chapter(section, deeper))
            .collect()
    } else {
        discard(outline.down);
        Vec::new()
    };
    Chapter {
        title: outline.title.trim().to_owned(),
        page: outline.dest.map(|destination| destination.loc.page_number),
        top: outline
            .dest
            .and_then(|destination| destination_top(destination.kind)),
        sections,
    }
}

/// Lets go of entries of an outline one at a time, rather than each in the
/// one above it, so that however deep they go they take no more stack.
fn discard(mut outlines: Vec<mupdf::Outline>) {
    while let Some(mut outline) = outlines.pop() {
        outlines.append(&mut outline.down);
    }
}

/// How far down its page a destination is, when it says. MuPDF gives it
/// measured downwards from the top of the page, though the mupdf crate names
/// the fields of a rectangle the way PDF, which measures upwards, does: its
/// `bottom` is the top edge.
const fn destination_top(kind: DestinationKind) -> Option<f32> {
    match kind {
        DestinationKind::XYZ { top, .. }
        | DestinationKind::FitH { top }
        | DestinationKind::FitBH { top } => top,
        DestinationKind::FitR { bottom, .. } => Some(bottom),
        DestinationKind::Fit
        | DestinationKind::FitB
        | DestinationKind::FitV { .. }
        | DestinationKind::FitBV { .. } => None,
    }
}

/// What went wrong, without the wrapping MuPDF puts around its messages or
/// a repeat of the path, which the caller states already.
fn reason(error: &mupdf::Error, path: &str) -> String {
    let text = error.to_string();
    let message = text
        .split_once("message: ")
        .map_or(text.as_str(), |(_, message)| message);
    let repeated = format!("cannot open {path}: ");
    message
        .strip_prefix(repeated.as_str())
        .unwrap_or(message)
        .to_owned()
}

/// The thread that owns a document and answers requests about it.
struct Worker {
    document: Document,
    /// The file the document was opened from, which highlights are saved
    /// to.
    path: PathBuf,
    /// The password that unlocked it, to open it again with.
    password: Option<String>,
    inbox: mpsc::Receiver<Request>,
    /// Long requests that arrived while another was under way.
    deferred: VecDeque<Request>,
    /// The pages drawn lately, recorded, the latest last.
    recordings: VecDeque<Arc<Recording>>,
    /// The threads pages are drawn on.
    painters: Painters,
}

/// A page to draw from its recording, and where to send the picture.
type Painting = (Arc<Recording>, RenderJob, oneshot::Sender<Result<Bitmap, Error>>);

/// Threads that draw pages from their recordings, apart from the worker,
/// so that a page that takes long to draw, as a crafted font can make it,
/// holds up neither the document's text, searches and saving nor the
/// other pages. They end once the worker does.
struct Painters {
    paintings: Option<mpsc::Sender<Painting>>,
}

impl Painters {
    /// Starts the threads. Without them, pages are drawn on the worker.
    fn new() -> Self {
        let (paintings, waiting) = mpsc::channel::<Painting>();
        let waiting = Arc::new(Mutex::new(waiting));
        let started = (0..PAINTERS)
            .filter(|_| {
                let waiting = Arc::clone(&waiting);
                thread::Builder::new()
                    .name("pdvim-painter".to_owned())
                    .spawn(move || paint(&waiting))
                    .is_ok()
            })
            .count();
        Self {
            paintings: (started > 0).then_some(paintings),
        }
    }

    /// Hands a painting to the threads, or gives it back if there are none.
    fn hand(&self, painting: Painting) -> Option<Painting> {
        match &self.paintings {
            Some(paintings) => paintings.send(painting).err().map(|failed| failed.0),
            None => Some(painting),
        }
    }
}

/// Draws the pages handed to the painters, one at a time, until the worker
/// is gone.
fn paint(waiting: &Mutex<mpsc::Receiver<Painting>>) {
    loop {
        // Held only while waiting, so that the others take the next one.
        let next = waiting
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .recv();
        let Ok((recording, job, reply)) = next else {
            return;
        };
        reply.answer(drawn(&recording, job));
    }
}

/// A page drawn from its recording, as the reader shows it.
fn drawn(recording: &Recording, job: RenderJob) -> Result<Bitmap, Error> {
    render(recording, job, LONGEST_SIDE).map_err(|error| Error::Render {
        page: job.page.saturating_add(1),
        reason: error.to_string(),
    })
}

/// What it takes to draw a page, recorded once: MuPDF reads the page and
/// its annotations into a list of drawing operations, which can be played
/// at any scale far faster than the page can be read again.
pub struct Recording {
    page: u32,
    list: DisplayList,
    bounds: mupdf::Rect,
}

/// Records a page, with its annotations.
pub fn record(document: &Document, page: u32) -> Result<Recording, mupdf::Error> {
    let loaded = document.load_page(page_number(page))?;
    Ok(Recording {
        page,
        bounds: loaded.bounds()?,
        list: loaded.to_display_list(true)?,
    })
}

impl Worker {
    /// Answers requests until every [`Backend`] handle is gone.
    fn serve(mut self) {
        loop {
            let request = match self.deferred.pop_front() {
                Some(request) => request,
                None => match self.inbox.recv() {
                    Ok(request) => request,
                    Err(_) => return,
                },
            };
            self.answer(request);
        }
    }

    /// Carries out a request and replies to it.
    fn answer(&mut self, request: Request) {
        // The asker may have gone away meanwhile; a failed reply is fine.
        match request {
            Request::Render(job, reply) => self.render_apart(job, reply),
            Request::Search(needle, pages, reply) => {
                reply.answer(
                    self.search(&needle, pages)
                        .map_err(|error| Error::Search(error.to_string())),
                );
            }
            Request::Measure(reply) => {
                reply.answer(
                    self.measure()
                        .map_err(|error| Error::Measure(error.to_string())),
                );
            }
            Request::Links(pages, reply) => {
                reply.answer(self.links(&pages));
            }
            Request::Figures(pages, reply) => {
                reply.answer(
                    pages
                        .iter()
                        .map(|&page| Ok((page, page_figures(&self.document, page)?)))
                        .collect::<Result<_, mupdf::Error>>()
                        .map_err(|error| Error::Text(error.to_string())),
                );
            }
            Request::Markup(highlight, reply) => {
                self.recordings.clear();
                reply.answer(highlight_in(&self.document, &highlight));
            }
            Request::Outline(reply) => {
                reply.answer(
                    outline(&self.document, || {
                        reopen(&self.path, self.password.as_deref())
                    })
                    .map_err(|error| Error::Outline(error.to_string())),
                );
            }
            Request::GuessOutline(reply) => {
                let path = self.path.clone();
                let password = self.password.clone();
                let spawned = thread::Builder::new()
                    .name("pdvim-contents".to_owned())
                    .spawn(move || {
                        reply.answer(
                            guess_outline(&path, password.as_deref())
                                .map_err(|error| Error::Outline(error.to_string())),
                        );
                    });
                // Without a thread, the reply is dropped, which says the
                // document is closed; no table of contents is guessed.
                drop(spawned);
            }
            // A change to the markings changes how pages look, so the
            // recordings are made again.
            Request::Remove(annotations, reply) => {
                self.recordings.clear();
                reply.answer(remove_in(&self.document, &annotations));
            }
            Request::Restore(markings, reply) => {
                self.recordings.clear();
                reply.answer(restore_in(&self.document, &markings));
            }
            Request::Fields(reply) => {
                reply.answer(
                    self.fields()
                        .map_err(|error| Error::Form(error.to_string())),
                );
            }
            Request::Fill(widget, value, reply) => {
                self.recordings.clear();
                reply.answer(fill_in(&self.document, widget, &value));
            }
            Request::SetNote(annotation, text, reply) => {
                self.recordings.clear();
                reply.answer(set_note_in(&self.document, annotation, &text));
            }
            Request::Markings(reply) => {
                reply.answer(
                    self.markings()
                        .map_err(|error| Error::Markings(error.to_string())),
                );
            }
            Request::Save(target, reply) => {
                reply.answer(save_to(&self.document, &self.path, target.as_deref()));
            }
            Request::Export(export, reply) => {
                reply.answer(export_page(&self.document, &export));
            }
            Request::Text(pages, reply) => {
                reply.answer(
                    pages
                        .iter()
                        .map(|&page| Ok((page, page_text(&self.document, page)?)))
                        .collect::<Result<_, mupdf::Error>>()
                        .map_err(|error| Error::Text(error.to_string())),
                );
            }
        }
    }

    /// Renders the pages that were asked for while a long request is under
    /// way, so that scrolling does not wait for it. Other requests are put
    /// aside until the long one is done.
    fn render_waiting(&mut self) {
        while let Ok(request) = self.inbox.try_recv() {
            if let Request::Render(job, reply) = request {
                self.render_apart(job, reply);
            } else {
                self.deferred.push_back(request);
            }
        }
    }

    /// A page's recording, made when it is not among those kept, and kept
    /// as the latest.
    fn recording(&mut self, page: u32) -> Result<Arc<Recording>, Error> {
        let kept = self
            .recordings
            .iter()
            .position(|recording| recording.page == page)
            .and_then(|index| self.recordings.remove(index));
        let recording = match kept {
            Some(recording) => recording,
            None => Arc::new(record(&self.document, page).map_err(|error| Error::Render {
                page: page.saturating_add(1),
                reason: error.to_string(),
            })?),
        };
        self.recordings.push_back(Arc::clone(&recording));
        if self.recordings.len() > RECORDINGS_KEPT {
            self.recordings.pop_front();
        }
        Ok(recording)
    }

    /// Draws a page on the worker, from its recording.
    #[cfg(test)]
    fn render(&mut self, job: RenderJob) -> Result<Bitmap, Error> {
        let recording = self.recording(job.page)?;
        drawn(&recording, job)
    }

    /// Records a page here, where the document is, and has it drawn on a
    /// painter, which replies; on the worker if no painter can take it.
    fn render_apart(&mut self, job: RenderJob, reply: oneshot::Sender<Result<Bitmap, Error>>) {
        let recording = match self.recording(job.page) {
            Ok(recording) => recording,
            Err(error) => {
                reply.answer(Err(error));
                return;
            }
        };
        if let Some((recording, job, reply)) = self.painters.hand((recording, job, reply)) {
            reply.answer(drawn(&recording, job));
        }
    }

    /// Finds the links on some pages.
    fn links(&self, pages: &[u32]) -> Result<PageLinks, Error> {
        pages
            .iter()
            .map(|&number| Ok((number, guarded_links(&self.document, number)?)))
            .collect()
    }

    /// Searches every page in order, or only some.
    fn search(
        &mut self,
        needle: &Needle,
        pages: Option<Vec<u32>>,
    ) -> Result<Vec<Hit>, mupdf::Error> {
        let mut hits = Vec::new();
        if needle.text.trim().is_empty() {
            return Ok(hits);
        }
        let count = u32::try_from(self.document.page_count()?).unwrap_or(0);
        let pages = pages.unwrap_or_else(|| (0..count).collect());
        for number in pages.into_iter().filter(|page| *page < count) {
            self.render_waiting();
            // MuPDF's own search ignores case and finds parts of words, so
            // a search that cares about either is made on the page's text.
            if needle.match_case || needle.whole_word {
                let lines = page_text(&self.document, number)?;
                hits.extend(text_hits(&lines, number, needle));
            } else {
                hits.extend(page_hits(&self.document, number, &needle.text)?);
            }
        }
        Ok(hits)
    }

    /// Finds the markings on every page, in reading order: by page, then
    /// down each page.
    fn markings(&mut self) -> Result<Vec<Marking>, mupdf::Error> {
        let count = u32::try_from(self.document.page_count()?).unwrap_or(0);
        let mut markings = Vec::new();
        for number in 0..count {
            self.render_waiting();
            let mut on_page = page_markings(&self.document, number)?;
            on_page.sort_by(|one, other| {
                let top =
                    |marking: &Marking| marking.areas.first().map(|area| (area.top, area.left));
                top(one)
                    .partial_cmp(&top(other))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            markings.extend(on_page);
        }
        Ok(markings)
    }

    /// Finds the fields of every page, in reading order.
    fn fields(&mut self) -> Result<Vec<(Rect, Field)>, mupdf::Error> {
        let count = u32::try_from(self.document.page_count()?).unwrap_or(0);
        let mut fields = Vec::new();
        for number in 0..count {
            self.render_waiting();
            fields.extend(fields_in_order(&self.document, number)?);
        }
        Ok(fields)
    }

    /// Finds the content of every page.
    fn measure(&mut self) -> Result<Vec<Rect>, mupdf::Error> {
        let count = u32::try_from(self.document.page_count()?).unwrap_or(0);
        let mut contents = Vec::new();
        for number in 0..count {
            self.render_waiting();
            contents.push(page_content(&self.document, number)?);
        }
        Ok(contents)
    }
}

/// The smallest box around the pixels of a grey image that are not paper,
/// as its left, top, right and bottom edges, the last two exclusive. `None`
/// when the image is blank. Each row is `stride` bytes, of which the first
/// `width` are pixels.
fn content_box(
    samples: &[u8],
    stride: usize,
    width: usize,
) -> Option<(usize, usize, usize, usize)> {
    let mut found: Option<(usize, usize, usize, usize)> = None;
    for (y, row) in samples.chunks(stride.max(1)).enumerate() {
        let row = row.get(..width).unwrap_or(row);
        let Some(first) = row.iter().position(|grey| *grey < PAPER) else {
            continue;
        };
        let last = row.iter().rposition(|grey| *grey < PAPER).unwrap_or(first);
        let below = y.saturating_add(1);
        let after = last.saturating_add(1);
        found = Some(
            found.map_or((first, y, after, below), |(left, top, right, _)| {
                (left.min(first), top, right.max(after), below)
            }),
        );
    }
    found
}

/// Draws a recorded page, or the part of it the job asks for, to a bitmap
/// whose longest side is at most `longest_side` pixels.
pub fn render(
    recording: &Recording,
    job: RenderJob,
    longest_side: f32,
) -> Result<Bitmap, mupdf::Error> {
    let bounds = recording.bounds;
    let area = job.clip.map_or(bounds, |clip| mupdf::Rect {
        x0: bounds.x0 + clip.left,
        y0: bounds.y0 + clip.top,
        x1: bounds.x0 + clip.right,
        y1: bounds.y0 + clip.bottom,
    });
    let longest = area.width().max(area.height()).max(1.0);
    // The cap comes last: nothing may undo it.
    let scale = job.scale.max(0.01).min(longest_side / longest);
    let mut transform = Matrix::new_scale(scale, scale);
    transform.rotate(job.rotation.degrees());

    let mut pixels = area.transform(&transform).round();
    // A page so long and thin that its short side comes to nothing still
    // gets a pixel, as an image of no pixels cannot be shown.
    pixels.x1 = pixels.x1.max(pixels.x0.saturating_add(1));
    pixels.y1 = pixels.y1.max(pixels.y0.saturating_add(1));
    let mut pixmap = Pixmap::new_with_rect(&Colorspace::device_rgb(), pixels, false)?;
    pixmap.clear_with(i32::from(u8::MAX))?;
    {
        // The device must be closed, which dropping it does, before the
        // pixels are complete.
        let device = Device::from_pixmap(&pixmap)?;
        // Only what lands in the bitmap is drawn.
        recording
            .list
            .run(&device, &transform, area.transform(&transform))?;
    }
    Ok(to_bitmap(&pixmap, job.palette))
}

/// Copies a MuPDF pixmap without alpha into an RGBA bitmap, recolouring it
/// on the way if a palette is given.
fn to_bitmap(pixmap: &Pixmap, palette: Option<Palette>) -> Bitmap {
    let width = pixmap.width();
    let height = pixmap.height();
    let channels = usize::from(pixmap.n()).max(1);
    let stride = usize::try_from(pixmap.stride()).unwrap_or(0).max(1);
    let columns = num::index(width);
    let area = columns.saturating_mul(num::index(height));
    // Opaque white, each pixel then written over in place: much faster
    // than growing the bitmap four bytes at a time.
    let mut pixels = vec![u8::MAX; area.saturating_mul(4)];
    let rows = pixmap
        .samples()
        .chunks(stride)
        .zip(pixels.chunks_exact_mut(columns.saturating_mul(4).max(1)));
    // The lookup is worked out once for the page, and the loops kept
    // apart, so that the one run for each pixel does no more than it must.
    let table = palette.map(Palette::table);
    for (row, out) in rows {
        let row = row.chunks_exact(channels).take(columns);
        let out = out.chunks_exact_mut(4);
        match &table {
            Some(table) => {
                for (pixel, into) in row.zip(out) {
                    write_pixel(into, table.apply(colour_of(pixel)));
                }
            }
            None => {
                for (pixel, into) in row.zip(out) {
                    write_pixel(into, colour_of(pixel));
                }
            }
        }
    }
    Bitmap {
        width,
        height,
        pixels,
    }
}

/// The colour of a pixel of a pixmap, of three channels or of one.
const fn colour_of(pixel: &[u8]) -> Rgb {
    match *pixel {
        [red, green, blue, ..] => Rgb(red, green, blue),
        [grey, ..] => Rgb(grey, grey, grey),
        [] => Rgb(0, 0, 0),
    }
}

/// Writes a colour over the first three bytes of a bitmap's pixel, leaving
/// it opaque.
const fn write_pixel(into: &mut [u8], Rgb(red, green, blue): Rgb) {
    if let [r, g, b, _] = into {
        *r = red;
        *g = green;
        *b = blue;
    }
}

/// A letter as it is compared when case does not matter: in small letters,
/// with the Greek final sigma as any other, and the Turkish dotless and
/// dotted i as plain i, so that a search typed in capitals finds them.
fn folded(character: char) -> char {
    match character {
        '\u{3c2}' => '\u{3c3}',
        '\u{131}' | '\u{130}' => 'i',
        _ => character.to_lowercase().next().unwrap_or(character),
    }
}

/// A page number in the form MuPDF takes.
fn page_number(page: u32) -> i32 {
    i32::try_from(page).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    /// The page the tests lay books out on.
    const FLOW: Flow = Flow {
        width: 400.0,
        height: 600.0,
        em: 12.0,
    };

    use super::*;

    /// A file for a test, removed when the test is done with it. Declared
    /// before the documents opened from it, it outlives them: Windows does
    /// not remove a file that is open.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str, content: impl AsRef<[u8]>) -> Self {
            Self::of_kind(name, "pdf", content)
        }

        /// A file with the extension given, as `epub`.
        fn of_kind(name: &str, extension: &str, content: impl AsRef<[u8]>) -> Self {
            let path = std::env::temp_dir()
                .join(format!("pdvim-{name}-{}.{extension}", std::process::id()));
            std::fs::write(&path, content).unwrap();
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _: std::io::Result<()> = std::fs::remove_file(&self.0);
        }
    }

    /// A PDF of three letter-size pages. The first has a link to a spot
    /// on the third, 500 points up from its foot, and a link to a web page.
    fn linked_pdf() -> Vec<u8> {
        pdf(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Annots [6 0 R 7 0 R] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
            "<< /Type /Annot /Subtype /Link /Rect [72 700 200 720] \
             /A << /S /GoTo /D [5 0 R /XYZ 0 500 null] >> >>",
            "<< /Type /Annot /Subtype /Link /Rect [72 600 200 620] \
             /A << /S /URI /URI (https://example.com/) >> >>",
        ])
    }

    /// A PDF of one letter-size page with "Hello world" in Helvetica, its
    /// baseline 92 points from the top.
    fn text_pdf() -> Vec<u8> {
        let content = "BT /F1 12 Tf 72 700 Td (Hello world) Tj ET";
        pdf(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
             /Resources << /Font << /F1 5 0 R >> >> >>",
            &format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            ),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        ])
    }

    /// A page with a form: a text field, a box to tick, and a list to pick
    /// from.
    fn form_pdf() -> Vec<u8> {
        let style = "/DA (/Helv 10 Tf 0 g) /F 4 /P 3 0 R";
        pdf(&[
            "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R 5 0 R 6 0 R] \
             /DA (/Helv 10 Tf 0 g) /DR << /Font << /Helv 8 0 R >> >> >> >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
             /Annots [4 0 R 5 0 R 6 0 R] >>",
            &format!(
                "<< /Type /Annot /Subtype /Widget /FT /Tx /T (name) /TU (Your name) \
                 /Rect [72 700 300 720] /V (Ann) {style} >>"
            ),
            &format!(
                "<< /Type /Annot /Subtype /Widget /FT /Btn /T (agree) /Rect [72 650 90 668] \
                 /V /Off /AS /Off /AP << /N << /Yes 7 0 R /Off 7 0 R >> >> {style} >>"
            ),
            &format!(
                "<< /Type /Annot /Subtype /Widget /FT /Ch /Ff 131072 /T (country) \
                 /Rect [72 600 300 620] /Opt [(Netherlands) [(de) (Germany)]] \
                 /V (Netherlands) {style} >>"
            ),
            "<< /Type /XObject /Subtype /Form /BBox [0 0 18 18] /Length 0 >>\nstream\n\nendstream",
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        ])
    }

    /// The fields among the links on the first page.
    fn fields(document: &Document) -> Vec<(Rect, Field)> {
        page_links(document, 0)
            .unwrap()
            .into_iter()
            .filter_map(|link| match link.target {
                Target::Field(field) => Some((link.area, field)),
                Target::Page { .. } | Target::Uri(_) => None,
            })
            .collect()
    }

    #[test]
    fn the_fields_of_a_form_are_found_with_the_links() {
        let file = Scratch::new("form", form_pdf());
        let document = Document::open(file.0.to_str().unwrap()).unwrap();
        let found = fields(&document);
        let kinds: Vec<(&str, &FieldKind, &str)> = found
            .iter()
            .map(|(_, field)| (field.name.as_str(), &field.kind, field.value.as_str()))
            .collect();
        assert_eq!(
            kinds,
            [
                ("Your name", &FieldKind::Text, "Ann"),
                (
                    "agree",
                    &FieldKind::Checkbox {
                        on: "Yes".to_owned()
                    },
                    OFF
                ),
                (
                    "country",
                    &FieldKind::Choice(vec!["Netherlands".to_owned(), "de".to_owned()]),
                    "Netherlands"
                ),
            ]
        );
        // Measured down from the top of the page, as links are.
        let (area, _) = &found[0];
        assert!((area.top - 72.0).abs() < 0.5 && (area.left - 72.0).abs() < 0.5, "{area:?}");
    }

    #[test]
    fn fields_filled_in_are_saved_with_the_file() {
        let file = Scratch::new("filled", form_pdf());
        let document = Document::open(file.0.to_str().unwrap()).unwrap();
        let widgets: Vec<Annotation> = fields(&document)
            .into_iter()
            .map(|(_, field)| field.widget)
            .collect();
        assert_eq!(fill_in(&document, widgets[0], "Bob").unwrap(), "Ann");
        assert_eq!(fill_in(&document, widgets[1], "Yes").unwrap(), OFF);
        assert_eq!(fill_in(&document, widgets[2], "de").unwrap(), "Netherlands");
        save_to(&document, &file.0, None).unwrap();
        drop(document);

        let reopened = Document::open(file.0.to_str().unwrap()).unwrap();
        let values: Vec<String> = fields(&reopened)
            .into_iter()
            .map(|(_, field)| field.value)
            .collect();
        assert_eq!(values, ["Bob", "Yes", "de"]);
        // And taken back, as undo does.
        assert_eq!(fill_in(&reopened, widgets[1], OFF).unwrap(), "Yes");
    }

    /// A PDF of these objects, numbered from 1, the first the catalog.
    fn pdf(objects: &[&str]) -> Vec<u8> {
        let size = objects.len().saturating_add(1);
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend(format!("{} 0 obj\n{object}\nendobj\n", index.saturating_add(1)).bytes());
        }
        let xref = pdf.len();
        pdf.extend(format!("xref\n0 {size}\n0000000000 65535 f \n").bytes());
        for offset in offsets {
            pdf.extend(format!("{offset:010} 00000 n \n").bytes());
        }
        pdf.extend(
            format!("trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").bytes(),
        );
        pdf
    }

    #[test]
    fn links_are_found_with_their_areas_and_targets() {
        let file = Scratch::new("links", linked_pdf());
        let path = file.0.clone();
        let document = Document::open(path.to_str().unwrap()).unwrap();
        let links = page_links(&document, 0).unwrap();

        assert_eq!(links.len(), 2, "{links:?}");
        let close = |left: f32, right: f32| (left - right).abs() < 0.5;
        // Measured down from the top of the page: 792 - 720 = 72.
        let area = links[0].area;
        assert!(
            close(area.left, 72.0)
                && close(area.top, 72.0)
                && close(area.right, 200.0)
                && close(area.bottom, 92.0),
            "{area:?}"
        );
        let Target::Page { page, top } = links[0].target else {
            panic!("expected a link into the document: {:?}", links[0].target);
        };
        assert_eq!(page, 2);
        // 500 up from the foot is 292 down from the top.
        assert!(top.is_some_and(|top| close(top, 292.0)), "{top:?}");
        assert_eq!(
            links[1].target,
            Target::Uri("https://example.com/".to_owned())
        );
        assert!(page_links(&document, 1).unwrap().is_empty());
    }

    #[test]
    fn text_is_read_a_character_at_a_time() {
        let file = Scratch::new("text", text_pdf());
        let path = file.0.clone();
        let document = Document::open(path.to_str().unwrap()).unwrap();
        let lines = page_text(&document, 0).unwrap();

        assert_eq!(lines.len(), 1, "{lines:?}");
        let content: String = lines[0]
            .chars
            .iter()
            .map(|character| character.character)
            .collect();
        assert_eq!(content, "Hello world");
        let first = lines[0].chars[0].area;
        assert!((first.left - 72.0).abs() < 0.5, "{first:?}");
        // Measured down from the top: the baseline is 92 points down, so
        // the letter spans it from above.
        assert!(first.top < 92.0 && first.bottom > 85.0, "{first:?}");
        let lefts: Vec<f32> = lines[0]
            .chars
            .iter()
            .map(|character| character.area.left)
            .collect();
        assert!(lefts.is_sorted(), "{lefts:?}");
    }

    /// A worker for a document, answering requests put to it directly.
    fn worker(path: &Path) -> Worker {
        let (_, inbox) = mpsc::channel();
        Worker {
            document: Document::open(path.to_str().unwrap()).unwrap(),
            path: path.to_owned(),
            password: None,
            inbox,
            deferred: VecDeque::new(),
            recordings: VecDeque::new(),
            painters: Painters::new(),
        }
    }

    fn job(scale: f32) -> RenderJob {
        RenderJob {
            page: 0,
            scale,
            rotation: Rotation::None,
            clip: None,
            palette: None,
        }
    }

    #[test]
    fn a_page_drawn_again_from_its_recording_comes_out_the_same() {
        let file = Scratch::new("recorded", text_pdf());
        let mut worker = worker(&file.0);
        let first = worker.render(job(1.5)).unwrap();
        assert_eq!(worker.recordings.len(), 1);
        // Another zoom, then back: the same recording, played twice.
        worker.render(job(2.0)).unwrap();
        let again = worker.render(job(1.5)).unwrap();
        assert_eq!(worker.recordings.len(), 1);
        assert_eq!(again.pixels, first.pixels);
        // And as a page read afresh comes out.
        let fresh = render(
            &record(&worker.document, 0).unwrap(),
            job(1.5),
            LONGEST_SIDE,
        )
        .unwrap();
        assert_eq!(fresh.pixels, first.pixels);
    }

    #[test]
    fn a_page_asked_for_is_drawn_on_a_painter_as_on_the_worker() {
        let file = Scratch::new("painted", text_pdf());
        let mut worker = worker(&file.0);
        let here = worker.render(job(1.5)).unwrap();
        let (reply, answer) = oneshot::channel();
        worker.answer(Request::Render(job(1.5), reply));
        let painted = iced::futures::executor::block_on(answer).unwrap().unwrap();
        assert_eq!(painted.pixels, here.pixels, "the painter draws it as the worker does");
        // The recording made for the worker is the one the painter played.
        assert_eq!(worker.recordings.len(), 1, "one recording");
    }

    #[test]
    fn a_new_highlight_shows_on_the_next_drawing() {
        let file = Scratch::new("rerecorded", text_pdf());
        let mut worker = worker(&file.0);
        let before = worker.render(job(1.0)).unwrap();
        let line = page_text(&worker.document, 0).unwrap()[0].chars[0].area;
        let (reply, _answer) = oneshot::channel();
        worker.answer(Request::Markup(
            Markup {
                kind: MarkupKind::Highlight,
                pages: vec![(0, vec![line])],
                colour: Rgb(0xff, 0x00, 0x00),
                note: None,
            },
            reply,
        ));
        let after = worker.render(job(1.0)).unwrap();
        assert_ne!(after.pixels, before.pixels);
    }

    #[test]
    fn only_so_many_recordings_are_kept() {
        let objects: Vec<String> = (0..12)
            .map(|_| "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>".to_owned())
            .collect();
        let kids: Vec<String> = (0..12).map(|index| format!("{} 0 R", index + 3)).collect();
        let pages = format!("<< /Type /Pages /Kids [{}] /Count 12 >>", kids.join(" "));
        let mut all = vec!["<< /Type /Catalog /Pages 2 0 R >>", pages.as_str()];
        all.extend(objects.iter().map(String::as_str));
        let file = Scratch::new("many", pdf(&all));
        let mut worker = worker(&file.0);
        for page in 0..12 {
            worker.render(RenderJob { page, ..job(0.5) }).unwrap();
        }
        let kept: Vec<u32> = worker
            .recordings
            .iter()
            .map(|recording| recording.page)
            .collect();
        assert_eq!(kept, (4..12).collect::<Vec<_>>());
    }

    #[test]
    fn highlights_are_saved_after_what_the_file_had() {
        let original = text_pdf();
        let file = Scratch::new("highlight", &original);
        let path = file.0.clone();
        let document = Document::open(path.to_str().unwrap()).unwrap();
        let line = page_text(&document, 0).unwrap()[0]
            .chars
            .iter()
            .map(|character| character.area)
            .reduce(|area, other| Rect {
                left: area.left.min(other.left),
                top: area.top.min(other.top),
                right: area.right.max(other.right),
                bottom: area.bottom.max(other.bottom),
            })
            .unwrap();
        let highlight = Markup {
            kind: MarkupKind::Highlight,
            pages: vec![(0, vec![line])],
            colour: Rgb(0xff, 0xff, 0x00),
            note: None,
        };
        let first = highlight_in(&document, &highlight).unwrap();
        highlight_in(&document, &highlight).unwrap();
        assert_eq!(first.len(), 1);

        // Nothing is written until the document is saved.
        assert_eq!(std::fs::read(&path).unwrap(), original);
        save_to(&document, &path, None).unwrap();

        // The original bytes are untouched, with the change after them.
        let saved = std::fs::read(&path).unwrap();
        assert!(saved.len() > original.len());
        assert_eq!(&saved[..original.len()], original.as_slice());

        // Opened afresh, the page has both highlights, over the line.
        let reopened = Document::open(path.to_str().unwrap()).unwrap();
        let page = mupdf::pdf::PdfPage::try_from(reopened.load_page(0).unwrap()).unwrap();
        let highlights: Vec<_> = page
            .annotations()
            .filter(|annotation| {
                annotation.r#type().ok() == Some(mupdf::pdf::PdfAnnotationType::Highlight)
            })
            .collect();
        assert_eq!(highlights.len(), 2);
        let quads = highlights[0].quad_points().unwrap();
        assert_eq!(quads.len(), 1);
        assert!(
            (quads[0].ul.x - line.left).abs() < 0.5,
            "{:?} {line:?}",
            quads[0]
        );
        assert!(
            (quads[0].ll.y - line.bottom).abs() < 0.5,
            "{:?} {line:?}",
            quads[0]
        );
        drop(highlights);
        drop(page);

        // Taking the first away leaves the second. MuPDF rewrites what it
        // added since the file was opened, so the file may shrink, but what
        // was there when it was opened stays as it was.
        remove_in(&document, &first).unwrap();
        save_to(&document, &path, None).unwrap();
        let after = std::fs::read(&path).unwrap();
        assert!(after.len() > original.len());
        assert_eq!(&after[..original.len()], original.as_slice());
        let reopened = Document::open(path.to_str().unwrap()).unwrap();
        let page = mupdf::pdf::PdfPage::try_from(reopened.load_page(0).unwrap()).unwrap();
        let left: Vec<i32> = page
            .annotations()
            .filter_map(|annotation| annotation.xref().ok())
            .collect();
        assert_eq!(left.len(), 1);
        assert!(!left.contains(&first[0].1));
    }

    #[test]
    fn saving_to_another_file_leaves_the_document_s_own_alone() {
        let original = text_pdf();
        let file = Scratch::new("save-as", &original);
        let copy = Scratch::new("save-as-copy", b"");
        let document = Document::open(file.0.to_str().unwrap()).unwrap();
        let highlight = Markup {
            kind: MarkupKind::Highlight,
            pages: vec![(
                0,
                vec![Rect {
                    left: 10.0,
                    top: 10.0,
                    right: 100.0,
                    bottom: 30.0,
                }],
            )],
            colour: Rgb(0xff, 0xff, 0x00),
            note: None,
        };
        highlight_in(&document, &highlight).unwrap();
        save_to(&document, &file.0, Some(&copy.0)).unwrap();
        assert_eq!(std::fs::read(&file.0).unwrap(), original);

        let saved = Document::open(copy.0.to_str().unwrap()).unwrap();
        let page = mupdf::pdf::PdfPage::try_from(saved.load_page(0).unwrap()).unwrap();
        assert_eq!(page.annotations().count(), 1);
    }

    #[test]
    fn shapes_near_one_another_make_one_figure_and_rules_none() {
        let rect = |left, top, right, bottom| Rect {
            left,
            top,
            right,
            bottom,
        };
        let page = rect(0.0, 0.0, 600.0, 800.0);
        let shapes = [
            // A chart: an axis, bars a little apart, and a frame around a
            // picture in it.
            rect(100.0, 100.0, 102.0, 300.0),
            rect(110.0, 200.0, 130.0, 300.0),
            rect(135.0, 150.0, 155.0, 300.0),
            rect(158.0, 250.0, 178.0, 300.0),
            // A rule under a heading, too thin to be a figure.
            rect(50.0, 400.0, 550.0, 401.0),
            // The page's background.
            rect(0.0, 0.0, 600.0, 800.0),
            // A table's lines, far below.
            rect(100.0, 600.0, 500.0, 601.0),
            rect(100.0, 640.0, 500.0, 641.0),
            rect(100.0, 600.0, 101.0, 641.0),
            rect(499.0, 600.0, 500.0, 641.0),
        ];
        let pictures = [rect(170.0, 120.0, 260.0, 200.0), rect(400.0, 700.0, 410.0, 710.0)];
        assert_eq!(
            figures(&pictures, &shapes, page),
            [rect(100.0, 100.0, 260.0, 300.0), rect(100.0, 600.0, 500.0, 641.0)]
        );
    }

    #[test]
    fn the_figures_of_a_page_are_found() {
        // A page with a filled square, 100 points a side, a square only
        // outlined, far from it, and a line of text that is no figure.
        let content = "0 0 1 rg 100 500 100 100 re f \
                       0 0 0 RG 1 w 350 100 m 450 100 l 450 200 l 350 200 l h S \
                       BT /F1 12 Tf 72 700 Td (Hello) Tj ET";
        let file = Scratch::new(
            "figures",
            pdf(&[
                "<< /Type /Catalog /Pages 2 0 R >>",
                "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
                 /Resources << /Font << /F1 5 0 R >> >> >>",
                &format!(
                    "<< /Length {} >>\nstream\n{content}\nendstream",
                    content.len()
                ),
                "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
            ]),
        );
        let document = Document::open(file.0.to_str().unwrap()).unwrap();
        let mut found = page_figures(&document, 0).unwrap();
        found.sort_by(|one, other| one.top.total_cmp(&other.top));
        // Measured from the top: 792 - 600 = 192 down, and 792 - 200 =
        // 592, give or take half the outline's width.
        let near = |area: &Rect, (left, top, right, bottom): (f32, f32, f32, f32)| {
            (area.left - left).abs() <= 1.0
                && (area.top - top).abs() <= 1.0
                && (area.right - right).abs() <= 1.0
                && (area.bottom - bottom).abs() <= 1.0
        };
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(near(&found[0], (100.0, 192.0, 200.0, 292.0)), "{found:?}");
        assert!(near(&found[1], (350.0, 592.0, 450.0, 692.0)), "{found:?}");
    }

    #[test]
    fn a_page_is_exported_as_an_image_or_a_drawing_but_not_over_a_file() {
        let file = Scratch::new("export", text_pdf());
        let image = Scratch::of_kind("export-image", "png", b"there");
        let drawing = Scratch::of_kind("export-drawing", "svg", b"");
        let document = Document::open(file.0.to_str().unwrap()).unwrap();
        let export = |target: &Path, force| Export {
            page: 0,
            scale: 2.0,
            rotation: Rotation::Quarter,
            target: target.to_owned(),
            force,
        };
        // A file there already is left as it was, unless forced.
        assert!(matches!(
            export_page(&document, &export(&image.0, false)),
            Err(Error::Export(reason)) if reason.contains("exists")
        ));
        assert_eq!(std::fs::read(&image.0).unwrap(), b"there");
        export_page(&document, &export(&image.0, true)).unwrap();
        let png = std::fs::read(&image.0).unwrap();
        assert!(png.starts_with(b"\x89PNG"));
        // Letter size at two pixels a point, turned a quarter: 1584 wide
        // and 1224 high, as the PNG's header says.
        assert_eq!(png.get(16..24), Some(&[0, 0, 6, 48, 0, 0, 4, 200][..]));

        export_page(&document, &export(&drawing.0, true)).unwrap();
        let svg = std::fs::read_to_string(&drawing.0).unwrap();
        // Turned, 792 wide, with the text on the frame: its glyphs are put
        // where x and y are both at least 0.
        assert!(svg.contains(r#"viewBox="0 0 792 612""#), "{svg}");
        let placed: Vec<(f32, f32)> = svg
            .split(r#"transform="matrix("#)
            .skip(1)
            .filter_map(|rest| {
                let numbers: Vec<f32> = rest
                    .split(')')
                    .next()?
                    .split(',')
                    .filter_map(|number| number.trim().parse().ok())
                    .collect();
                Some((*numbers.get(4)?, *numbers.get(5)?))
            })
            .collect();
        assert!(!placed.is_empty(), "{svg}");
        assert!(placed.iter().all(|&(x, y)| x >= 0.0 && y >= 0.0), "{placed:?}");

        let other = Path::new("page.jpg");
        assert_eq!(ExportFormat::of(other), None);
        assert_eq!(ExportFormat::of(Path::new("Page.PNG")), Some(ExportFormat::Png));
    }

    /// A one-page PDF saying "Hello world", locked with the password
    /// `secret` by AES-256.
    const LOCKED: &[u8] = include_bytes!("../fuzz/seeds/pdf/encrypted.pdf");

    /// A file of the formats seeds, which start with a byte that picks the
    /// format for the fuzzer, without that byte.
    fn seed(bytes: &'static [u8]) -> &'static [u8] {
        bytes.split_first().unwrap().1
    }

    const BOOK: &[u8] = include_bytes!("../fuzz/seeds/formats/book.epub");
    const STORY: &[u8] = include_bytes!("../fuzz/seeds/formats/story.fb2");
    const COMIC: &[u8] = include_bytes!("../fuzz/seeds/formats/comic.cbz");
    const PAPER: &[u8] = include_bytes!("../fuzz/seeds/formats/paper.xps");
    const DRAWING: &[u8] = include_bytes!("../fuzz/seeds/formats/drawing.svg");
    const PNG: &[u8] = include_bytes!("../fuzz/seeds/formats/picture.png");
    const JPEG: &[u8] = include_bytes!("../fuzz/seeds/formats/picture.jpg");

    #[test]
    fn books_comics_drawings_and_images_open() {
        let kinds: [(&str, &[u8], usize, bool); 7] = [
            ("epub", BOOK, 0, true),
            ("fb2", STORY, 0, true),
            ("cbz", COMIC, 3, false),
            ("xps", PAPER, 2, false),
            ("svg", DRAWING, 1, false),
            ("png", PNG, 1, false),
            ("jpg", JPEG, 1, false),
        ];
        for (extension, bytes, pages, reflowed) in kinds {
            let file = Scratch::of_kind("format", extension, seed(bytes));
            let (document, sizes, flow) = load(&file.0, None, FLOW).unwrap();
            assert_eq!(flow.is_some(), reflowed, "{extension}");
            if reflowed {
                // Laid out on pages the size asked for, as many as it takes.
                assert!(sizes.len() > 1, "{extension}");
                assert!(
                    sizes.iter().all(|size| size.width == FLOW.width),
                    "{extension}"
                );
            } else {
                assert_eq!(sizes.len(), pages, "{extension}");
            }
            assert!(record(&document, 0).is_ok(), "{extension}");
            // Only a PDF takes markings, or can be saved.
            assert!(editable(&document).is_err(), "{extension}");
        }
    }

    #[test]
    fn a_book_in_bigger_type_takes_more_pages() {
        let file = Scratch::of_kind("book", "epub", seed(BOOK));
        let (_, small, _) = load(&file.0, None, FLOW).unwrap();
        let big = Flow { em: 24.0, ..FLOW };
        let (document, large, flow) = load(&file.0, None, big).unwrap();
        assert_eq!(flow, Some(big));
        assert!(large.len() > small.len());
        // Its table of contents and text are read as a PDF's are.
        let chapters = outline(&document, || reopen(&file.0, None)).unwrap();
        let titles: Vec<&str> = chapters
            .iter()
            .map(|chapter| chapter.title.as_str())
            .collect();
        assert_eq!(titles, ["Chapter 1", "Chapter 2"]);
        // The heading may wrap, as the type of the fonts on the system is
        // wider or narrower, so the page's words are read across lines.
        let words: Vec<String> = page_text(&document, 0)
            .unwrap()
            .iter()
            .map(|line| line.chars.iter().map(|glyph| glyph.character).collect())
            .collect();
        let words = words.join(" ");
        let words: Vec<&str> = words.split_whitespace().take(2).collect();
        assert_eq!(words, ["Chapter", "1"]);
    }

    #[test]
    fn a_page_of_html_laid_out_to_millions_of_pages_is_refused() {
        let bytes = seed(include_bytes!("../fuzz/seeds/formats/many-pages.html"));
        let file = Scratch::of_kind("many", "html", bytes);
        let start = std::time::Instant::now();
        let refused = load(&file.0, None, FLOW);
        assert!(
            matches!(&refused, Err(Error::Open { reason, .. }) if reason.contains("pages")),
            "{:?}",
            refused.map(|(_, sizes, _)| sizes.len())
        );
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
    }

    #[test]
    fn a_page_for_a_book_is_kept_within_reason() {
        let unshown = Flow::new(0.0, f32::NAN, 200.0);
        assert!(unshown.width >= 100.0 && unshown.height >= 100.0);
        assert!((unshown.em - Flow::EM.1).abs() < f32::EPSILON);
        let window = Flow::new(800.0, 50_000.0, 12.0);
        assert!((window.width - 800.0).abs() < f32::EPSILON);
        assert!(window.height <= 10_000.0);
    }

    #[test]
    fn a_locked_document_opens_only_with_its_password() {
        let file = Scratch::new("locked", LOCKED);
        assert!(matches!(
            load(&file.0, None, FLOW),
            Err(Error::Encrypted(_))
        ));
        assert!(matches!(
            load(&file.0, Some("guess"), FLOW),
            Err(Error::WrongPassword(_))
        ));
        let (document, sizes, _) = load(&file.0, Some("secret"), FLOW).unwrap();
        assert_eq!(sizes.len(), 1);
        let text: String = page_text(&document, 0).unwrap()[0]
            .chars
            .iter()
            .map(|character| character.character)
            .collect();
        assert_eq!(text, "Hello world");
        reopen(&file.0, Some("secret")).unwrap();
        reopen(&file.0, None).unwrap_err();
    }

    #[test]
    fn a_locked_document_stays_locked_when_marked_and_saved() {
        let file = Scratch::new("locked-saved", LOCKED);
        let copy = Scratch::new("locked-copy", b"");
        let (document, _, _) = load(&file.0, Some("secret"), FLOW).unwrap();
        let markup = Markup {
            kind: MarkupKind::Highlight,
            pages: vec![(
                0,
                vec![Rect {
                    left: 70.0,
                    top: 80.0,
                    right: 140.0,
                    bottom: 96.0,
                }],
            )],
            colour: Rgb(0xff, 0xff, 0x00),
            note: None,
        };
        highlight_in(&document, &markup).unwrap();
        save_to(&document, &file.0, None).unwrap();
        save_to(&document, &file.0, Some(&copy.0)).unwrap();
        for saved in [&file.0, &copy.0] {
            assert!(matches!(load(saved, None, FLOW), Err(Error::Encrypted(_))));
            let (document, _, _) = load(saved, Some("secret"), FLOW).unwrap();
            assert_eq!(page_markings(&document, 0).unwrap().len(), 1);
        }
    }

    #[test]
    fn printed_page_numbers_are_read() {
        // Two pages numbered i and ii, then two numbered from 1.
        let labelled = pdf(&[
            "<< /Type /Catalog /Pages 2 0 R /PageLabels << /Nums [0 << /S /r >> 2 << /S /D >>] >> >>",
            "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R 6 0 R] /Count 4 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
        ]);
        let document = Document::from_bytes(&labelled, "application/pdf").unwrap();
        assert_eq!(
            page_labels(&document),
            Some(vec!["i".into(), "ii".into(), "1".into(), "2".into()])
        );
        // No labels, and labels that are just the page's place, are none.
        let document = Document::from_bytes(&text_pdf(), "application/pdf").unwrap();
        assert_eq!(page_labels(&document), None);
        let plain = pdf(&[
            "<< /Type /Catalog /Pages 2 0 R /PageLabels << /Nums [0 << /S /D >>] >> >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
        ]);
        let document = Document::from_bytes(&plain, "application/pdf").unwrap();
        assert_eq!(page_labels(&document), None);
    }

    #[test]
    fn dates_read_as_people_write_them() {
        assert_eq!(
            pdf_date("D:20130415120000+01'00'"),
            "2013-04-15 12:00 +01:00"
        );
        assert_eq!(pdf_date("D:20130415120000Z"), "2013-04-15 12:00 UTC");
        assert_eq!(pdf_date("D:199912311259-05'30"), "1999-12-31 12:59 -05:30");
        assert_eq!(pdf_date("D:2013"), "2013");
        assert_eq!(pdf_date("D:201304"), "2013-04");
        assert_eq!(pdf_date("D:2013041509"), "2013-04-15 09:00");
        assert_eq!(pdf_date("20130415"), "2013-04-15");
        assert_eq!(pdf_date("last Tuesday"), "last Tuesday");
        assert_eq!(pdf_date(""), "");
    }

    #[test]
    fn only_a_real_title_names_a_document() {
        let titled = |title: &str| Info {
            title: Some(title.to_owned()),
            ..Info::default()
        };
        assert_eq!(titled("A Complete Guide").name(), Some("A Complete Guide"));
        assert_eq!(titled("report.docx").name(), None);
        assert_eq!(titled("Microsoft Word - report").name(), None);
        assert_eq!(titled("Untitled").name(), None);
        assert_eq!(Info::default().name(), None);
    }

    #[test]
    fn what_a_document_says_about_itself_is_read() {
        // An /Info dictionary, the sixth object, given in the trailer.
        let bytes = pdf(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
            "<< >>",
            "<< >>",
            "<< /Title (Hello) /Author (Someone) /CreationDate (D:20130415120000Z) >>",
        ]);
        let text = String::from_utf8(bytes)
            .unwrap()
            .replace("/Root 1 0 R", "/Root 1 0 R /Info 6 0 R");
        let document = Document::from_bytes(text.as_bytes(), "application/pdf").unwrap();
        let info = read_info(&document);
        assert_eq!(info.title.as_deref(), Some("Hello"));
        assert_eq!(info.author.as_deref(), Some("Someone"));
        assert_eq!(info.created.as_deref(), Some("2013-04-15 12:00 UTC"));
        assert_eq!(info.subject, None);
        assert!(info.format.is_some_and(|format| format.starts_with("PDF")));
    }

    #[test]
    fn a_damaged_file_read_into_memory_lets_its_file_be_replaced() {
        let mut damaged = text_pdf();
        let at = damaged
            .windows(9)
            .rposition(|bytes| bytes == b"startxref")
            .unwrap();
        damaged.truncate(at);
        damaged.extend_from_slice(b"startxref\n9\n%%EOF\n");
        let file = Scratch::new("damaged-in-memory", &damaged);
        let document = opened(&file.0, true).unwrap();
        let markup = Markup {
            kind: MarkupKind::Highlight,
            pages: vec![(
                0,
                vec![Rect {
                    left: 10.0,
                    top: 10.0,
                    right: 100.0,
                    bottom: 30.0,
                }],
            )],
            colour: Rgb(0xff, 0xff, 0x00),
            note: None,
        };
        highlight_in(&document, &markup).unwrap();
        save_to(&document, &file.0, None).unwrap();
        // Once more, now that the file has been replaced.
        highlight_in(&document, &markup).unwrap();
        save_to(&document, &file.0, None).unwrap();
        let saved = Document::open(file.0.to_str().unwrap()).unwrap();
        let page = mupdf::pdf::PdfPage::try_from(saved.load_page(0).unwrap()).unwrap();
        assert_eq!(page.annotations().count(), 2);
    }

    #[test]
    fn a_damaged_file_is_marked_and_saved_whole() {
        // A file whose cross-reference table is not where it says, as in
        // many downloaded books, so that MuPDF repairs it on opening.
        let mut damaged = text_pdf();
        let at = damaged
            .windows(9)
            .rposition(|bytes| bytes == b"startxref")
            .unwrap();
        damaged.truncate(at);
        damaged.extend_from_slice(b"startxref\n9\n%%EOF\n");
        let file = Scratch::new("damaged", &damaged);
        let (document, _, _) = load(&file.0, None, FLOW).unwrap();
        let pdf = mupdf::pdf::PdfDocument::try_from(document.clone()).unwrap();
        assert!(!pdf.can_be_saved_incrementally());
        let markup = Markup {
            kind: MarkupKind::Underline,
            pages: vec![(
                0,
                vec![Rect {
                    left: 10.0,
                    top: 10.0,
                    right: 100.0,
                    bottom: 30.0,
                }],
            )],
            colour: Rgb(0x1e, 0x88, 0xe5),
            note: None,
        };
        highlight_in(&document, &markup).unwrap();
        save_to(&document, &file.0, None).unwrap();
        assert!(!rewrite_path(&file.0, 0).exists());

        let saved = mupdf::pdf::PdfDocument::open(file.0.to_str().unwrap()).unwrap();
        assert!(saved.can_be_saved_incrementally());
        let page = mupdf::pdf::PdfPage::try_from(saved.load_page(0).unwrap()).unwrap();
        assert_eq!(page.annotations().count(), 1);
    }

    #[test]
    fn markings_are_found_taken_away_and_put_back_as_they_were() {
        let file = Scratch::new("markings", text_pdf());
        let document = Document::open(file.0.to_str().unwrap()).unwrap();
        // Over "Hello", and not "world".
        let hello = Rect {
            left: 70.0,
            top: 80.0,
            right: 101.0,
            bottom: 96.0,
        };
        let markup = Markup {
            kind: MarkupKind::Underline,
            pages: vec![(0, vec![hello])],
            colour: Rgb(0x1e, 0x88, 0xe5),
            note: None,
        };
        let added = highlight_in(&document, &markup).unwrap();
        // A note, as another program may have written on it.
        {
            let pdf = mupdf::pdf::PdfDocument::try_from(document.clone()).unwrap();
            let page = mupdf::pdf::PdfPage::try_from(pdf.load_page(0).unwrap()).unwrap();
            let mut annotation = page.annotations().next().unwrap();
            annotation.set_contents("a note").unwrap();
        }

        let found = page_markings(&document, 0).unwrap();
        assert_eq!(found.len(), 1);
        let marking = &found[0];
        assert_eq!(marking.annotation(), added[0]);
        assert_eq!(marking.kind, MarkupKind::Underline);
        assert_eq!(marking.text, "Hello");
        assert_eq!(marking.colour(), Some(Rgb(0x1e, 0x88, 0xe5)));
        assert_eq!(marking.kept.contents.as_deref(), Some("a note"));

        let removed = remove_in(&document, &added).unwrap();
        assert_eq!(removed, found);
        assert_eq!(page_markings(&document, 0).unwrap(), []);
        // Gone already: nothing more to take away.
        assert_eq!(remove_in(&document, &added).unwrap(), []);

        let restored = restore_in(&document, &removed).unwrap();
        let again = page_markings(&document, 0).unwrap();
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].annotation(), restored[0]);
        assert_eq!(again[0].kind, marking.kind);
        assert_eq!(again[0].text, marking.text);
        assert_eq!(again[0].kept, marking.kept);
        for (one, other) in again[0].areas.iter().zip(&marking.areas) {
            assert!((one.top - other.top).abs() < 0.01 && (one.left - other.left).abs() < 0.01);
        }
    }

    #[test]
    fn notes_in_the_margin_leave_the_margins_as_they_were() {
        let file = Scratch::new("trim", text_pdf());
        let document = Document::open(file.0.to_str().unwrap()).unwrap();
        let before = page_content(&document, 0).unwrap();
        let markup = Markup {
            kind: MarkupKind::Note,
            pages: vec![(
                0,
                vec![Rect {
                    left: 4.0,
                    top: 4.0,
                    right: 24.0,
                    bottom: 24.0,
                }],
            )],
            colour: Rgb(0, 0, 0),
            note: Some("In the corner".to_owned()),
        };
        highlight_in(&document, &markup).unwrap();
        assert_eq!(page_content(&document, 0).unwrap(), before);
    }

    #[test]
    fn a_note_is_found_taken_away_and_put_back_with_its_text() {
        let file = Scratch::new("note", text_pdf());
        let document = Document::open(file.0.to_str().unwrap()).unwrap();
        let spot = Rect {
            left: 72.0,
            top: 80.0,
            right: 92.0,
            bottom: 100.0,
        };
        let markup = Markup {
            kind: MarkupKind::Note,
            pages: vec![(0, vec![spot])],
            colour: Rgb(0xfd, 0xd8, 0x35),
            note: Some("Check this greeting".to_owned()),
        };
        let added = highlight_in(&document, &markup).unwrap();
        let found = page_markings(&document, 0).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, MarkupKind::Note);
        assert_eq!(found[0].text, "Check this greeting");
        assert_eq!(found[0].annotation(), added[0]);
        let area = found[0].areas[0];
        assert!((area.left - spot.left).abs() < 0.5 && (area.top - spot.top).abs() < 0.5);

        let removed = remove_in(&document, &added).unwrap();
        assert_eq!(page_markings(&document, 0).unwrap(), []);
        restore_in(&document, &removed).unwrap();
        let again = page_markings(&document, 0).unwrap();
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].kind, MarkupKind::Note);
        assert_eq!(again[0].text, "Check this greeting");

        save_to(&document, &file.0, None).unwrap();
        let saved = Document::open(file.0.to_str().unwrap()).unwrap();
        assert_eq!(
            page_markings(&saved, 0).unwrap()[0].text,
            "Check this greeting"
        );

        let note = again[0].annotation();
        let before = set_note_in(&document, note, "Check it twice").unwrap();
        assert_eq!(before.as_deref(), Some("Check this greeting"));
        assert_eq!(
            page_markings(&document, 0).unwrap()[0].text,
            "Check it twice"
        );
        set_note_in(&document, (0, 9999), "nowhere").unwrap_err();
    }

    #[test]
    fn each_kind_of_marking_is_its_own_kind_of_annotation() {
        use mupdf::pdf::PdfAnnotationType as Type;
        let file = Scratch::new("kinds", text_pdf());
        let document = Document::open(file.0.to_str().unwrap()).unwrap();
        let area = Rect {
            left: 10.0,
            top: 10.0,
            right: 100.0,
            bottom: 30.0,
        };
        for kind in [
            MarkupKind::Highlight,
            MarkupKind::Underline,
            MarkupKind::StrikeOut,
            MarkupKind::Squiggly,
        ] {
            let markup = Markup {
                kind,
                pages: vec![(0, vec![area])],
                colour: Rgb(0x1e, 0x88, 0xe5),
                note: None,
            };
            highlight_in(&document, &markup).unwrap();
        }
        save_to(&document, &file.0, None).unwrap();

        let saved = Document::open(file.0.to_str().unwrap()).unwrap();
        let page = mupdf::pdf::PdfPage::try_from(saved.load_page(0).unwrap()).unwrap();
        let kinds: Vec<_> = page
            .annotations()
            .map(|annotation| annotation.r#type().unwrap())
            .collect();
        assert_eq!(
            kinds,
            [
                Type::Highlight,
                Type::Underline,
                Type::StrikeOut,
                Type::Squiggly
            ]
        );
    }

    /// A PDF of three pages with a table of contents: Chapter 1 on the
    /// first, with Section 1.1 on the second, 500 points up from its foot,
    /// then Chapter 2 on the third.
    fn outlined_pdf() -> Vec<u8> {
        pdf(&[
            "<< /Type /Catalog /Pages 2 0 R /Outlines 6 0 R >>",
            "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
            "<< /Type /Outlines /First 7 0 R /Last 9 0 R /Count 3 >>",
            "<< /Title (Chapter 1) /Parent 6 0 R /Next 9 0 R /First 8 0 R /Last 8 0 R \
             /Count 1 /Dest [3 0 R /Fit] >>",
            "<< /Title (Section 1.1) /Parent 7 0 R /Dest [4 0 R /XYZ 0 500 null] >>",
            "<< /Title (Chapter 2) /Parent 6 0 R /Prev 7 0 R /Dest [5 0 R /Fit] >>",
        ])
    }

    #[test]
    fn the_table_of_contents_is_read_as_a_tree() {
        let file = Scratch::new("outline", outlined_pdf());
        let path = file.0.clone();
        let document = Document::open(path.to_str().unwrap()).unwrap();
        let chapters = outline(&document, || panic!("nothing to mend")).unwrap();

        assert_eq!(chapters.len(), 2, "{chapters:?}");
        assert_eq!(chapters[0].title, "Chapter 1");
        assert_eq!((chapters[0].page, chapters[0].top), (Some(0), None));
        let section = &chapters[0].sections[0];
        assert_eq!(section.title, "Section 1.1");
        assert_eq!(section.page, Some(1));
        assert!(
            section.top.is_some_and(|top| (top - 292.0).abs() < 0.5),
            "{section:?}"
        );
        assert_eq!(chapters[1].title, "Chapter 2");
        assert_eq!(chapters[1].page, Some(2));
        assert!(chapters[1].sections.is_empty());
    }

    #[test]
    fn a_table_of_contents_is_read_only_so_deep() {
        let depth = CHAPTER_DEPTH.saturating_add(20);
        let mut objects = vec![
            "<< /Type /Catalog /Pages 2 0 R /Outlines 4 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>".to_owned(),
            "<< /Type /Outlines /First 5 0 R /Last 5 0 R >>".to_owned(),
        ];
        for level in 0..depth {
            let number = level.saturating_add(5);
            let under = if level.saturating_add(1) < depth {
                format!("/First {0} 0 R /Last {0} 0 R", number.saturating_add(1))
            } else {
                String::new()
            };
            objects.push(format!(
                "<< /Title (Level {level}) /Parent {} 0 R {under} /Dest [3 0 R /Fit] >>",
                number.saturating_sub(1)
            ));
        }
        let objects: Vec<&str> = objects.iter().map(String::as_str).collect();
        let file = Scratch::new("deep-outline", pdf(&objects));
        let document = Document::open(file.0.to_str().unwrap()).unwrap();
        let chapters = outline(&document, || panic!("nothing to mend")).unwrap();

        let mut levels = 0;
        let mut here = chapters.as_slice();
        while let [chapter] = here {
            levels += 1;
            here = &chapter.sections;
        }
        assert_eq!(levels, CHAPTER_DEPTH + 1);
    }

    /// Found by fuzzing: the mupdf crate crashes on an outline entry
    /// without a title.
    #[test]
    fn an_entry_without_a_title_is_read_from_a_mended_copy() {
        let bytes = pdf(&[
            "<< /Type /Catalog /Pages 2 0 R /Outlines 4 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
            "<< /Type /Outlines /First 5 0 R /Last 6 0 R /Count 2 >>",
            "<< /Title (Named) /Parent 4 0 R /Next 6 0 R /Dest [3 0 R /Fit] >>",
            "<< /Parent 4 0 R /Prev 5 0 R /Dest [3 0 R /Fit] >>",
        ]);
        let document = Document::from_bytes(&bytes, "application/pdf").unwrap();
        let chapters = outline(&document, || {
            Document::from_bytes(&bytes, "application/pdf")
        })
        .unwrap();
        assert_eq!(chapters[0].title, "Named");
        assert_eq!(chapters[1].title, "");
        assert_eq!(chapters[1].page, Some(0));
        // The document itself, which highlights are saved from, is not
        // mended.
        let pdf = mupdf::pdf::PdfDocument::try_from(document).unwrap();
        assert!(untitled(&pdf, false).unwrap());
    }

    /// Found by fuzzing: an entry that is not a dictionary has no title
    /// either.
    #[test]
    fn an_entry_that_is_not_a_dictionary_is_left_out() {
        let bytes = pdf(&[
            "<< /Type /Catalog /Pages 2 0 R /Outlines 4 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
            "<< /Type /Outlines /First 5 0 R /Count 2 >>",
            "<< /Title (Named) /First 6 0 R /Next 7 0 R >>",
            "(not a dictionary)",
            // Object 6 is reached twice: as Named's first section and as
            // what follows After. 9 is not in the file at all.
            "<< /Title (After) /Prev 5 0 R /First 9 0 R /Next 6 0 R >>",
        ]);
        let document = Document::from_bytes(&bytes, "application/pdf").unwrap();
        let chapters = outline(&document, || {
            Document::from_bytes(&bytes, "application/pdf")
        })
        .unwrap();
        let titles: Vec<_> = chapters
            .iter()
            .map(|chapter| chapter.title.as_str())
            .collect();
        assert_eq!(titles, ["Named", "After"]);
        assert!(chapters.iter().all(|chapter| chapter.sections.is_empty()));
    }

    /// Found by fuzzing: in a damaged file, MuPDF rebuilt its record of the
    /// objects while reading the outline, and forgot the titles given. The
    /// file is the one the fuzzer found, as no simpler one did the same.
    #[test]
    fn titles_given_in_a_damaged_file_are_not_forgotten() {
        let bytes = include_bytes!("../fuzz/seeds/pdf/outline-damaged.pdf");
        let document = Document::from_bytes(bytes, "application/pdf").unwrap();
        let chapters =
            outline(&document, || Document::from_bytes(bytes, "application/pdf")).unwrap();
        let titles: Vec<_> = chapters
            .iter()




            .map(|chapter| chapter.title.as_str())
            .collect();
        assert_eq!(titles, ["Chapter 1", ""]);
    }

    #[test]
    fn an_outline_that_runs_in_a_circle_is_refused() {
        let bytes = pdf(&[
            "<< /Type /Catalog /Pages 2 0 R /Outlines 4 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
            "<< /Type /Outlines /First 5 0 R /Count 2 >>",
            "<< /Title (Named) /Next 6 0 R >>",
            "<< /Prev 5 0 R /Next 5 0 R /First 6 0 R >>",
        ]);
        let document = Document::from_bytes(&bytes, "application/pdf").unwrap();
        let reopen = || Document::from_bytes(&bytes, "application/pdf");
        outline(&document, reopen).unwrap_err();
    }

    /// Found by fuzzing: a page claiming to be billions of miles wide was
    /// measured at full size.
    #[test]
    fn a_huge_page_is_measured_small() {
        let content = "0 0 0 rg 0 0 1000000000 100 re f";
        let bytes = pdf(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 3333333333333333200 200] \
             /Contents 4 0 R >>",
            &format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            ),
        ]);
        let document = Document::from_bytes(&bytes, "application/pdf").unwrap();
        let started = std::time::Instant::now();
        let area = page_content(&document, 0).unwrap();
        assert!(started.elapsed().as_secs() < 2, "{:?}", started.elapsed());
        assert!(area.right > area.left && area.bottom <= 200.0, "{area:?}");
    }

    /// Found by fuzzing: a lower bound on the scale undid the cap on the
    /// size of a rendered page, so a huge page came out 16777216 pixels
    /// long, far beyond what a graphics card takes.
    #[test]
    fn a_huge_page_is_rendered_no_larger_than_the_cap() {
        let bytes = pdf(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 3333333333333333200 200] >>",
        ]);
        let document = Document::from_bytes(&bytes, "application/pdf").unwrap();
        let recording = record(&document, 0).unwrap();
        for rotation in [Rotation::None, Rotation::Quarter] {
            let bitmap = render(
                &recording,
                RenderJob {
                    page: 0,
                    scale: 0.25,
                    rotation,
                    clip: None,
                    palette: None,
                },
                LONGEST_SIDE,
            )
            .unwrap();
            let longest = bitmap.width.max(bitmap.height);
            let shortest = bitmap.width.min(bitmap.height);
            assert!(
                longest <= 8001 && shortest >= 1,
                "{}x{}",
                bitmap.width,
                bitmap.height
            );
        }
    }

    #[test]
    fn a_blank_image_has_no_content() {
        assert_eq!(content_box(&[255; 12], 4, 4), None);
        assert_eq!(content_box(&[], 4, 4), None);
    }

    #[test]
    fn content_is_boxed_and_row_padding_is_ignored() {
        // Four pixels to a row, then one byte of padding that looks dark.
        #[rustfmt::skip]
        let image = [
            255, 255, 255, 255, 0,
            255,   0, 255, 255, 0,
            255, 255, 200, 255, 0,
            255, 255, 255, 255, 0,
        ];
        assert_eq!(content_box(&image, 5, 4), Some((1, 1, 3, 3)));
    }

    #[test]
    fn faint_marks_count_as_paper() {
        assert_eq!(content_box(&[255, 250, 246, 255], 4, 4), None);
        assert_eq!(content_box(&[255, 250, 244, 255], 4, 4), Some((2, 0, 3, 1)));
    }

    /// Lines of text, each character 10 points wide and each line 20
    /// points under the one before.
    fn lines(rows: &[&str]) -> Vec<TextLine> {
        rows.iter()
            .enumerate()
            .map(|(row, content)| TextLine {
                chars: content
                    .chars()
                    .enumerate()
                    .map(|(column, character)| {
                        let left = f32::from(u16::try_from(column).unwrap()) * 10.0;
                        let top = f32::from(u16::try_from(row).unwrap()) * 20.0;
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
            })
            .collect()
    }

    fn found(rows: &[&str], text: &str, match_case: bool, whole_word: bool) -> Vec<(f32, f32)> {
        let needle = Needle {
            text: text.to_owned(),
            match_case,
            whole_word,
        };
        text_hits(&lines(rows), 3, &needle)
            .into_iter()
            .map(|hit| {
                assert_eq!(hit.page, 3);
                (hit.area.left, hit.area.top)
            })
            .collect()
    }

    #[test]
    fn a_search_in_case_finds_only_that_case() {
        let rows = ["Fox and fox", "FOX"];
        assert_eq!(found(&rows, "Fox", true, false), [(0.0, 0.0)]);
        assert_eq!(found(&rows, "fox", false, false).len(), 3);
    }

    #[test]
    fn a_search_for_a_word_skips_it_inside_others() {
        let rows = ["foxes, fox. fox_y outfox", "fox"];
        assert_eq!(
            found(&rows, "fox", false, true),
            [(70.0, 0.0), (0.0, 20.0)]
        );
    }

    #[test]
    fn a_search_runs_on_over_the_end_of_a_line() {
        let rows = ["the quick", "brown  fox"];
        let hits = found(&rows, "quick brown fox", true, false);
        assert_eq!(hits, [(40.0, 0.0), (0.0, 20.0)]);
        assert!(found(&rows, "quickbrown", true, false).is_empty());
    }
}
