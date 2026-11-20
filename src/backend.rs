//! Reading and rendering documents with MuPDF.
//!
//! A MuPDF document must stay on the thread that opened it, and rendering a
//! page can take a while. So each open document gets a worker thread that
//! owns it. The interface sends the worker requests through a [`Backend`]
//! and awaits each answer, which keeps the window responsive.

use std::collections::VecDeque;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;

use iced::futures::channel::oneshot;
use mupdf::{Colorspace, DestinationKind, Device, Document, Matrix, Pixmap, TextPageFlags};

use crate::layout::{PageSize, Rect, Rotation};
use crate::num;
use crate::recolor::{Palette, Rgb};

/// The longest side a rendered page may have, in pixels. Graphics cards
/// refuse textures much larger than this, so a page zoomed in further is
/// rendered at this size and stretched.
const LONGEST_SIDE: f32 = 8000.0;

/// The scale pages are rendered at to find their margins, in pixels per
/// point. Coarse is enough: only the outline of the content matters.
const MEASURE_SCALE: f32 = 0.5;

/// How dark a grey pixel must be to count as content rather than paper.
const PAPER: u8 = 245;

/// The margin left around the content of a trimmed page, in points.
const TRIM_MARGIN: f32 = 6.0;

/// The most search results reported for one page.
const HITS_PER_PAGE: u32 = 500;

/// Why the backend could not do what was asked.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("cannot open {path}: {reason}")]
    Open { path: String, reason: String },
    #[error("{0} is password-protected, which is not supported yet")]
    Encrypted(String),
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

/// A rendered page as rows of red, green, blue and alpha bytes.
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
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
}

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

/// A document that has just been opened.
#[derive(Debug, Clone)]
pub struct Opened {
    pub backend: Backend,
    /// The unturned size of every page, in points.
    pub sizes: Vec<PageSize>,
}

/// A request to the worker, with the channel to answer on.
enum Request {
    Render(RenderJob, oneshot::Sender<Result<Bitmap, Error>>),
    Search(String, oneshot::Sender<Result<Vec<Hit>, Error>>),
    Measure(oneshot::Sender<Result<Vec<Rect>, Error>>),
    Links(Vec<u32>, oneshot::Sender<Result<PageLinks, Error>>),
    Text(Vec<u32>, oneshot::Sender<Result<PageText, Error>>),
}

/// A handle to the worker that owns an open document. The worker stops
/// once every handle is dropped.
#[derive(Debug, Clone)]
pub struct Backend {
    requests: mpsc::Sender<Request>,
}

impl Backend {
    /// Renders a page.
    pub fn render(&self, job: RenderJob) -> impl Future<Output = Result<Bitmap, Error>> + use<> {
        let (reply, answer) = oneshot::channel();
        // A send only fails when the worker is gone. The reply channel is
        // then dropped too, which the await below reports as closed.
        let _ = self.requests.send(Request::Render(job, reply));
        async move { answer.await.unwrap_or(Err(Error::Closed)) }
    }

    /// Finds every place the text occurs, ignoring case, in page order.
    pub fn search(&self, needle: String) -> impl Future<Output = Result<Vec<Hit>, Error>> + use<> {
        let (reply, answer) = oneshot::channel();
        let _ = self.requests.send(Request::Search(needle, reply));
        async move { answer.await.unwrap_or(Err(Error::Closed)) }
    }

    /// Finds the links on some pages, each page with its links.
    pub fn links(&self, pages: Vec<u32>) -> impl Future<Output = Result<PageLinks, Error>> + use<> {
        let (reply, answer) = oneshot::channel();
        let _ = self.requests.send(Request::Links(pages, reply));
        async move { answer.await.unwrap_or(Err(Error::Closed)) }
    }

    /// Reads the lines of text on some pages.
    pub fn text(&self, pages: Vec<u32>) -> impl Future<Output = Result<PageText, Error>> + use<> {
        let (reply, answer) = oneshot::channel();
        let _ = self.requests.send(Request::Text(pages, reply));
        async move { answer.await.unwrap_or(Err(Error::Closed)) }
    }

    /// Finds the part of every page that is not blank margin, in points
    /// from the top-left corner of the unturned page. A blank page keeps
    /// its whole area.
    pub fn measure(&self) -> impl Future<Output = Result<Vec<Rect>, Error>> + use<> {
        let (reply, answer) = oneshot::channel();
        let _ = self.requests.send(Request::Measure(reply));
        async move { answer.await.unwrap_or(Err(Error::Closed)) }
    }
}

/// Opens a document on a new worker thread.
pub fn open(path: PathBuf) -> impl Future<Output = Result<Opened, Error>> {
    let (reply, answer) = oneshot::channel();
    let (requests, inbox) = mpsc::channel();
    let name = path.display().to_string();
    let spawned = thread::Builder::new()
        .name("pdvim-document".to_owned())
        .spawn(move || {
            let document = match load(&path) {
                Ok((document, sizes)) => {
                    let backend = Backend { requests };
                    if reply.send(Ok(Opened { backend, sizes })).is_err() {
                        return;
                    }
                    document
                }
                Err(error) => {
                    let _ = reply.send(Err(error));
                    return;
                }
            };
            Worker {
                document,
                inbox,
                deferred: VecDeque::new(),
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

/// Opens the file and measures every page.
fn load(path: &Path) -> Result<(Document, Vec<PageSize>), Error> {
    let name = path.display().to_string();
    let failed = |error: mupdf::Error| Error::Open {
        path: name.clone(),
        reason: reason(&error, &name),
    };
    let document = Document::open(path).map_err(failed)?;
    if document.needs_password().map_err(failed)? {
        return Err(Error::Encrypted(name));
    }
    let count = document.page_count().map_err(failed)?;
    let mut sizes = Vec::new();
    for number in 0..count {
        let bounds = document
            .load_page(number)
            .and_then(|page| page.bounds())
            .map_err(failed)?;
        sizes.push(PageSize {
            width: bounds.width(),
            height: bounds.height(),
        });
    }
    Ok((document, sizes))
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
                area: Rect {
                    left: link.bounds.x0 - bounds.x0,
                    top: link.bounds.y0 - bounds.y0,
                    right: link.bounds.x1 - bounds.x0,
                    bottom: link.bounds.y1 - bounds.y0,
                },
                target,
            }
        })
        .collect();
    Ok(links)
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
fn page_text(document: &Document, number: u32) -> Result<Vec<TextLine>, mupdf::Error> {
    let page = document.load_page(page_number(number))?;
    let bounds = page.bounds()?;
    let text = page.to_text_page(TextPageFlags::empty())?;
    let mut lines = Vec::new();
    for block in text.blocks() {
        for line in block.lines() {
            let chars: Vec<TextChar> = line
                .chars()
                .filter_map(|text_char| {
                    Some(TextChar {
                        character: text_char.char()?,
                        area: quad_area(&text_char.quad(), &bounds),
                    })
                })
                .collect();
            if !chars.is_empty() {
                lines.push(TextLine { chars });
            }
        }
    }
    Ok(lines)
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
    inbox: mpsc::Receiver<Request>,
    /// Long requests that arrived while another was under way.
    deferred: VecDeque<Request>,
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
            // The asker may have gone away meanwhile; a failed reply is fine.
            match request {
                Request::Render(job, reply) => {
                    let _ = reply.send(self.render(job));
                }
                Request::Search(needle, reply) => {
                    let _ = reply.send(
                        self.search(&needle)
                            .map_err(|error| Error::Search(error.to_string())),
                    );
                }
                Request::Measure(reply) => {
                    let _ = reply.send(
                        self.measure()
                            .map_err(|error| Error::Measure(error.to_string())),
                    );
                }
                Request::Links(pages, reply) => {
                    let _ = reply.send(self.links(&pages));
                }
                Request::Text(pages, reply) => {
                    let _ = reply.send(
                        pages
                            .iter()
                            .map(|&page| Ok((page, page_text(&self.document, page)?)))
                            .collect::<Result<_, mupdf::Error>>()
                            .map_err(|error| Error::Text(error.to_string())),
                    );
                }
            }
        }
    }

    /// Renders the pages that were asked for while a long request is under
    /// way, so that scrolling does not wait for it. Other requests are put
    /// aside until the long one is done.
    fn render_waiting(&mut self) {
        while let Ok(request) = self.inbox.try_recv() {
            if let Request::Render(job, reply) = request {
                let _ = reply.send(self.render(job));
            } else {
                self.deferred.push_back(request);
            }
        }
    }

    fn render(&self, job: RenderJob) -> Result<Bitmap, Error> {
        render(&self.document, job).map_err(|error| Error::Render {
            page: job.page.saturating_add(1),
            reason: error.to_string(),
        })
    }

    /// Finds the links on some pages.
    fn links(&self, pages: &[u32]) -> Result<PageLinks, Error> {
        pages
            .iter()
            .map(|&number| {
                // The mupdf crate panics on a link whose destination MuPDF
                // cannot resolve. That must not take the worker, and with
                // it the document, down.
                let links = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    page_links(&self.document, number)
                }))
                .map_err(|_| {
                    Error::Links(format!(
                        "a link on page {} is broken",
                        number.saturating_add(1)
                    ))
                })?
                .map_err(|error| Error::Links(error.to_string()))?;
                Ok((number, links))
            })
            .collect()
    }

    /// Searches every page in order.
    fn search(&mut self, needle: &str) -> Result<Vec<Hit>, mupdf::Error> {
        let mut hits = Vec::new();
        if needle.is_empty() {
            return Ok(hits);
        }
        let count = u32::try_from(self.document.page_count()?).unwrap_or(0);
        for number in 0..count {
            self.render_waiting();
            let page = self.document.load_page(page_number(number))?;
            let bounds = page.bounds()?;
            for quad in page.search(needle, HITS_PER_PAGE)? {
                hits.push(Hit {
                    page: number,
                    area: quad_area(&quad, &bounds),
                });
            }
        }
        Ok(hits)
    }

    /// Finds the content of every page by rendering it small and looking
    /// for what is not paper.
    fn measure(&mut self) -> Result<Vec<Rect>, mupdf::Error> {
        let count = self.document.page_count()?;
        let mut contents = Vec::new();
        let scale = Matrix::new_scale(MEASURE_SCALE, MEASURE_SCALE);
        for number in 0..count {
            self.render_waiting();
            let page = self.document.load_page(number)?;
            let bounds = page.bounds()?;
            let whole = Rect {
                left: 0.0,
                top: 0.0,
                right: bounds.width(),
                bottom: bounds.height(),
            };
            let pixmap = page.to_pixmap(&scale, &Colorspace::device_gray(), false, true)?;
            let stride = usize::try_from(pixmap.stride()).unwrap_or(0);
            let content = content_box(pixmap.samples(), stride, num::index(pixmap.width()));
            contents.push(content.map_or(whole, |(left, top, right, bottom)| {
                let points = |pixels: usize| num::float(num::len(pixels)) / MEASURE_SCALE;
                Rect {
                    left: (points(left) - TRIM_MARGIN).max(0.0),
                    top: (points(top) - TRIM_MARGIN).max(0.0),
                    right: (points(right) + TRIM_MARGIN).min(whole.right),
                    bottom: (points(bottom) + TRIM_MARGIN).min(whole.bottom),
                }
            }));
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

/// Renders one page, or the part of it the job asks for, to a bitmap.
fn render(document: &Document, job: RenderJob) -> Result<Bitmap, mupdf::Error> {
    let page = document.load_page(page_number(job.page))?;
    let bounds = page.bounds()?;
    let area = job.clip.map_or(bounds, |clip| mupdf::Rect {
        x0: bounds.x0 + clip.left,
        y0: bounds.y0 + clip.top,
        x1: bounds.x0 + clip.right,
        y1: bounds.y0 + clip.bottom,
    });
    let longest = area.width().max(area.height()).max(1.0);
    let scale = job.scale.min(LONGEST_SIDE / longest).max(0.01);
    let mut transform = Matrix::new_scale(scale, scale);
    transform.rotate(job.rotation.degrees());

    let mut pixmap = Pixmap::new_with_rect(
        &Colorspace::device_rgb(),
        area.transform(&transform).round(),
        false,
    )?;
    pixmap.clear_with(i32::from(u8::MAX))?;
    {
        // The device must be closed, which dropping it does, before the
        // pixels are complete.
        let device = Device::from_pixmap(&pixmap)?;
        page.run(&device, &transform)?;
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
    let area = num::index(width).saturating_mul(num::index(height));
    let mut pixels = Vec::with_capacity(area.saturating_mul(4));
    for row in pixmap.samples().chunks(stride).take(num::index(height)) {
        for pixel in row.chunks_exact(channels).take(num::index(width)) {
            let colour = match *pixel {
                [red, green, blue, ..] => Rgb(red, green, blue),
                [grey, ..] => Rgb(grey, grey, grey),
                [] => Rgb(0, 0, 0),
            };
            let Rgb(red, green, blue) = palette.map_or(colour, |palette| palette.apply(colour));
            pixels.extend_from_slice(&[red, green, blue, u8::MAX]);
        }
    }
    Bitmap {
        width,
        height,
        pixels,
    }
}

/// A page number in the form MuPDF takes.
fn page_number(page: u32) -> i32 {
    i32::try_from(page).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let path = std::env::temp_dir().join(format!("pdvim-links-{}.pdf", std::process::id()));
        std::fs::write(&path, linked_pdf()).unwrap();
        let document = Document::open(path.to_str().unwrap()).unwrap();
        let links = page_links(&document, 0).unwrap();
        std::fs::remove_file(&path).unwrap();

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
        let path = std::env::temp_dir().join(format!("pdvim-text-{}.pdf", std::process::id()));
        std::fs::write(&path, text_pdf()).unwrap();
        let document = Document::open(path.to_str().unwrap()).unwrap();
        let lines = page_text(&document, 0).unwrap();
        std::fs::remove_file(&path).unwrap();

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
}
