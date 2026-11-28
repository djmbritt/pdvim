//! The view on an open document: which part shows, and how large.
//!
//! A [`Reader`] knows the page layout, the zoom and the scroll position, and
//! moves them in response to commands. It knows nothing about rendering or
//! the window toolkit, so it can be tested by itself.

use std::cell::Cell;
use std::ops::Range;

use crate::history::{Fit, View};
use crate::layout::{Arrangement, Layout, PageSize, Rect, Rotation};
use crate::num;

/// The smallest and largest zoom, in screen pixels per point.
const ZOOM_RANGE: (f32, f32) = (0.1, 10.0);

/// The most pages that sit side by side.
pub const MOST_COLUMNS: usize = 4;

/// How much one zoom step enlarges the page.
const ZOOM_STEP: f32 = 1.1;

/// The most a trimmed page is enlarged to be as wide as the widest: more
/// would blow up a page with little on it, such as a title, out of all
/// proportion to the pages around it.
const MOST_STRETCH: f32 = 2.0;

/// How near the edge of a page, in points, the window must be for scrolling
/// on to turn to the next page or spread.
const EDGE: f32 = 0.5;

/// The part of a full screen that stays visible after scrolling by one, so
/// the reader keeps their place.
const SCREEN_OVERLAP: f32 = 0.1;

/// The point that stays put while the zoom changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Anchor {
    /// The middle of the window, for zooming in and out.
    Centre,
    /// The top of the window, for when the window changes size.
    Top,
}

/// How pages are shown: one after another in a column to scroll through, or
/// a page or a spread at a time, to turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageMode {
    /// Every page in a column, in rows as wide as the columns say.
    #[default]
    Continuous,
    /// One page at a time.
    Single,
    /// Two facing pages at a time, after the cover alone, as in a book.
    Double,
}

impl PageMode {
    /// The names `page-mode` takes.
    pub const NAMES: [&str; 3] = ["continuous", "single", "double"];

    /// The mode a name says, if it says one.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "continuous" => Some(Self::Continuous),
            "single" => Some(Self::Single),
            "double" => Some(Self::Double),
            _ => None,
        }
    }

    /// The mode after this one, going round.
    pub const fn next(self) -> Self {
        match self {
            Self::Continuous => Self::Single,
            Self::Single => Self::Double,
            Self::Double => Self::Continuous,
        }
    }

    /// Whether a page or a spread shows at a time, rather than a column.
    pub const fn paged(self) -> bool {
        !matches!(self, Self::Continuous)
    }
}

/// A place in a document, whatever the zoom: a page, and how far down it
/// the top of the window is, as a share of the page's height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Place {
    /// The page, counted from 0.
    pub page: u32,
    /// From 0 at the top of the page to 1 at its bottom.
    pub down: f32,
}

impl Place {
    /// Whether two places are so near that one stands for the other.
    pub fn near(self, other: Self) -> bool {
        self.page == other.page && (self.down - other.down).abs() < 0.02
    }
}

/// The view on an open document.
#[derive(Debug, Clone)]
pub struct Reader {
    /// The unturned size of every page.
    sizes: Vec<PageSize>,
    /// The part of each unturned page that shows, when margins are trimmed.
    crops: Option<Vec<Rect>>,
    /// How much larger than its size each page shows: trimmed, a narrower
    /// page is enlarged to be as wide as the widest. Empty when none is.
    stretches: Vec<f32>,
    layout: Layout,
    rotation: Rotation,
    zoom: f32,
    fit: Fit,
    /// The left edge of the window in the document, in points.
    x: f32,
    /// The top edge of the window in the document, in points.
    y: f32,
    /// The width and height of the window, in screen pixels.
    window: (f32, f32),
    /// How many pages sit side by side, when continuous.
    columns: usize,
    mode: PageMode,
    /// The page shown, when a page or a spread shows at a time: the window
    /// stays on its row.
    shown: u32,
    /// While the window draws its pages from where the view was when they
    /// were laid out, which it then slides to where the view is now, that
    /// corner: screen positions are given from it. See [`Self::draw_from`].
    drawn_from: Cell<Option<(f32, f32)>>,
}

impl Reader {
    /// A view of the start of a document, fitted to the window's width.
    pub fn new(sizes: Vec<PageSize>, window: (f32, f32)) -> Self {
        let layout = Layout::new(&sizes, Rotation::None);
        let mut reader = Self {
            sizes,
            crops: None,
            stretches: Vec::new(),
            layout,
            rotation: Rotation::None,
            zoom: 1.0,
            fit: Fit::Width,
            x: 0.0,
            y: 0.0,
            window,
            columns: 1,
            mode: PageMode::Continuous,
            shown: 0,
            drawn_from: Cell::new(None),
        };
        reader.refit(Anchor::Top);
        reader
    }

    /// Sets how many pages sit side by side, from 1 to [`MOST_COLUMNS`],
    /// staying at the same place.
    pub fn set_columns(&mut self, columns: usize) {
        let columns = columns.clamp(1, MOST_COLUMNS);
        if columns == self.columns {
            return;
        }
        let place = self.place();
        self.columns = columns;
        self.lay_out();
        self.refit(Anchor::Top);
        self.go_to_place(place);
    }

    /// How many pages sit side by side when continuous.
    pub const fn columns(&self) -> usize {
        self.columns
    }

    /// How many pages a row holds at most, as shown now: the columns when
    /// continuous, one page or two facing pages when not.
    pub const fn pages_per_row(&self) -> usize {
        match self.mode {
            PageMode::Continuous => self.columns,
            PageMode::Single => 1,
            PageMode::Double => 2,
        }
    }

    pub const fn page_mode(&self) -> PageMode {
        self.mode
    }

    /// Shows pages in a column, or a page or a spread at a time, staying at
    /// the same place.
    pub fn set_page_mode(&mut self, mode: PageMode) {
        if mode == self.mode {
            return;
        }
        let place = self.place();
        self.mode = mode;
        self.lay_out();
        self.refit(Anchor::Top);
        self.go_to_place(place);
    }

    /// Puts the view back the way it was left.
    pub fn restore(&mut self, view: View) {
        self.rotation = view.rotation;
        self.columns = view.columns.clamp(1, MOST_COLUMNS);
        self.lay_out();
        self.zoom = clamp_zoom(view.zoom);
        self.fit = view.fit;
        self.x = view.x;
        self.y = view.y;
        self.shown = self.layout.page_at(self.y);
        self.refit(Anchor::Top);
    }

    /// How tall the whole document is, turned as it is, in points, with
    /// its margins left on.
    pub fn extent(&self) -> f32 {
        Layout::in_columns(&self.sizes, self.rotation, self.columns).height()
    }

    /// The view as it is now, for the history. `recolor` is not the
    /// reader's to know, so the caller passes it through.
    pub fn view(&self, recolor: bool) -> View {
        let trim = self.crops.is_some();
        View {
            extent: self.extent(),
            columns: self.columns,
            x: self.x,
            y: self.y,
            zoom: self.zoom,
            fit: self.fit,
            rotation: self.rotation,
            recolor,
            trim,
        }
    }

    /// Shows only the given part of each unturned page, or the whole of
    /// every page again. The view stays on the same page, as far down it
    /// as before. Parts that do not match the pages one to one are ignored.
    pub fn set_crops(&mut self, crops: Option<Vec<Rect>>) {
        let page = self.current_page();
        let down = self.layout.page_rect(page).map_or(0.0, |area| {
            (self.y - area.top) / (area.bottom - area.top).max(1.0)
        });
        self.crops = crops.filter(|crops| crops.len() == self.sizes.len());
        self.lay_out();
        self.refit(Anchor::Top);
        if let Some(area) = self.layout.page_rect(page) {
            self.y = down.mul_add(area.bottom - area.top, area.top);
            self.clamp();
        }
    }

    /// The part of an unturned page that shows, when margins are trimmed.
    pub fn crop(&self, page: u32) -> Option<Rect> {
        self.crops.as_ref()?.get(num::index(page)).copied()
    }

    /// Screen pixels per point.
    pub const fn zoom(&self) -> f32 {
        self.zoom
    }

    /// How much larger than its size a page shows, besides the zoom: when
    /// margins are trimmed, a narrower page is enlarged to be as wide as
    /// the widest, up to twice its size.
    pub fn stretch(&self, page: u32) -> f32 {
        self.stretches.get(num::index(page)).copied().unwrap_or(1.0)
    }

    pub const fn rotation(&self) -> Rotation {
        self.rotation
    }

    pub fn page_count(&self) -> u32 {
        self.layout.page_count()
    }

    /// The page at the top of the window, counted from 0; a page or a
    /// spread at a time, the one shown.
    pub fn current_page(&self) -> u32 {
        if self.mode.paged() {
            return self.shown;
        }
        self.layout.page_at(self.y)
    }

    /// The pages at least partly inside the window.
    pub fn visible_pages(&self) -> Range<u32> {
        if self.mode.paged() {
            return self.layout.row_pages(self.shown);
        }
        self.layout
            .pages_between(self.y, self.y + self.window.1 / self.zoom)
    }

    /// Where the window's top-left corner is in the document, in points.
    pub const fn corner(&self) -> (f32, f32) {
        (self.x, self.y)
    }

    /// Gives screen positions as if the window's top-left corner were at
    /// this corner of the document, until called again with none: for
    /// drawing the pages where they were laid out, to be slid from there.
    pub fn draw_from(&self, corner: Option<(f32, f32)>) {
        self.drawn_from.set(corner);
    }

    /// Tells the reader the window's size, in screen pixels.
    pub fn set_window(&mut self, window: (f32, f32)) {
        self.window = window;
        self.refit(Anchor::Top);
    }

    /// Scrolls by a distance in screen pixels: right and down are positive.
    /// A page or a spread at a time, scrolling on from the bottom of it
    /// turns to the next, and from the top to the one before.
    pub fn scroll_by(&mut self, right: f32, down: f32) {
        self.x += right / self.zoom;
        if self.mode.paged()
            && let Some((top, bottom)) = self.shown_band()
        {
            let rows = if down > 0.0 && self.y >= bottom - EDGE {
                1
            } else if down < 0.0 && self.y <= top + EDGE {
                -1
            } else {
                0
            };
            if rows != 0 {
                self.turn(rows, rows < 0);
                return;
            }
        }
        self.y += down / self.zoom;
        self.clamp();
    }

    /// Scrolls by a number of screens, which may be a fraction: down is
    /// positive. A whole screen leaves a little overlap.
    pub fn scroll_screens(&mut self, screens: f32) {
        let whole = screens.trunc();
        let overlap = whole * SCREEN_OVERLAP;
        self.scroll_by(0.0, (screens - overlap) * self.window.1);
    }

    /// Moves to the top of the page a number of pages away; with pages
    /// side by side, a number of rows away.
    pub fn page_by(&mut self, pages: i32) {
        if self.mode.paged() {
            // Past the first or last row, go as far as there is.
            let mut rows = i64::from(pages);
            while rows != 0 && self.layout.row_page_by(self.shown, rows).is_none() {
                rows = rows.saturating_sub(rows.signum());
            }
            self.turn(rows, false);
            return;
        }
        let current = i64::from(self.current_page());
        let last = i64::from(self.page_count().saturating_sub(1));
        let columns = i64::try_from(self.columns).unwrap_or(1);
        let target = current
            .saturating_add(i64::from(pages).saturating_mul(columns))
            .clamp(0, last);
        self.go_to_page(u32::try_from(target).unwrap_or(0));
    }

    /// Moves to the top of a page, counted from 0. A page past the end is
    /// taken as the last.
    pub fn go_to_page(&mut self, page: u32) {
        let page = page.min(self.page_count().saturating_sub(1));
        if let Some(area) = self.layout.page_rect(page) {
            self.shown = page;
            self.y = area.top;
            self.clamp();
        }
    }

    /// Turns so many rows on, or back, a page or a spread at a time: to the
    /// top of the row, or with `at_bottom` to its bottom. Past either end,
    /// the view stays where it is.
    fn turn(&mut self, rows: i64, at_bottom: bool) {
        let Some(page) = self.layout.row_page_by(self.shown, rows) else {
            return;
        };
        self.shown = page;
        self.y = if at_bottom {
            f32::INFINITY
        } else {
            f32::NEG_INFINITY
        };
        self.clamp();
    }

    /// The highest and lowest the top of the window goes on the row shown,
    /// a page or a spread at a time: the same, centring the row, when the
    /// whole row fits.
    fn shown_band(&self) -> Option<(f32, f32)> {
        let (top, bottom) = self.layout.row_band(self.shown)?;
        let spare = (bottom - top) - self.window.1 / self.zoom;
        Some(if spare <= 0.0 {
            let centred = top + spare / 2.0;
            (centred, centred)
        } else {
            (top, top + spare)
        })
    }

    /// Goes to a spot on a page, given how far down the unturned page it
    /// is in points, and puts it at the top of the window. Without a spot,
    /// goes to the top of the page.
    pub fn go_to_spot(&mut self, page: u32, top: Option<f32>) {
        self.go_to_page(page);
        let Some(top) = top else {
            return;
        };
        let spot = Rect {
            left: 0.0,
            top,
            right: 0.0,
            bottom: top,
        };
        // On a turned page the spot need not be near the top; it is enough
        // that it shows.
        if self.rotation != Rotation::None {
            self.reveal(page, spot);
            return;
        }
        if let Some(on_screen) = self.area_on_screen(page, spot) {
            self.y += on_screen.top / self.zoom;
            self.clamp();
        }
    }

    /// The unturned size of a page, in points.
    pub fn page_size(&self, page: u32) -> Option<PageSize> {
        self.sizes.get(num::index(page)).copied()
    }

    /// Where the top of the window is.
    pub fn place(&self) -> Place {
        let page = self.current_page();
        let down = self.layout.page_rect(page).map_or(0.0, |area| {
            let height = area.bottom - area.top;
            if height > 0.0 {
                ((self.y - area.top) / height).clamp(0.0, 1.0)
            } else {
                0.0
            }
        });
        Place { page, down }
    }

    /// Moves the top of the window to a place. A page past the end is taken
    /// as the last.
    pub fn go_to_place(&mut self, place: Place) {
        let page = place.page.min(self.page_count().saturating_sub(1));
        if let Some(area) = self.layout.page_rect(page) {
            let down = if place.down.is_finite() {
                place.down.clamp(0.0, 1.0)
            } else {
                0.0
            };
            self.y = area.top + down * (area.bottom - area.top);
            self.clamp();
        }
    }

    /// Moves to the top of the current page.
    pub fn page_top(&mut self) {
        self.go_to_page(self.current_page());
    }

    /// Moves so the bottom of the current page is at the bottom of the
    /// window.
    pub fn page_bottom(&mut self) {
        if let Some(area) = self.layout.page_rect(self.current_page()) {
            self.y = (area.bottom - self.window.1 / self.zoom).max(area.top);
            self.clamp();
        }
    }

    pub fn zoom_in(&mut self) {
        self.zoom_to(self.zoom * ZOOM_STEP);
    }

    pub fn zoom_out(&mut self) {
        self.zoom_to(self.zoom / ZOOM_STEP);
    }

    /// Sets the zoom, in screen pixels per point, and stops fitting the
    /// page to the window.
    pub fn zoom_to(&mut self, zoom: f32) {
        self.fit = Fit::None;
        self.set_zoom(zoom, Anchor::Centre);
    }

    /// Fits the page to the window, and keeps it fitted as the window
    /// changes size.
    pub fn fit(&mut self, fit: Fit) {
        self.fit = fit;
        self.refit(Anchor::Centre);
    }

    /// Turns every page a quarter turn clockwise, staying on the same page.
    pub fn rotate(&mut self) {
        let page = self.current_page();
        self.rotation = self.rotation.next();
        self.lay_out();
        self.refit(Anchor::Top);
        self.go_to_page(page);
    }

    /// Where a page is in the window, in screen pixels.
    pub fn page_on_screen(&self, page: u32) -> Option<Rect> {
        self.layout.page_rect(page).map(|area| self.to_screen(area))
    }

    /// Where an area of an unturned page is in the window, in screen
    /// pixels.
    pub fn area_on_screen(&self, page: u32, area: Rect) -> Option<Rect> {
        let page_area = self.layout.page_rect(page)?;
        let mut size = *self.sizes.get(num::index(page))?;
        let mut area = area;
        if let Some(crop) = self.crop(page) {
            // Measure the area from the corner of the part that shows.
            area = Rect {
                left: area.left - crop.left,
                top: area.top - crop.top,
                right: area.right - crop.left,
                bottom: area.bottom - crop.top,
            };
            size = PageSize {
                width: crop.right - crop.left,
                height: crop.bottom - crop.top,
            };
        }
        let turned = self.rotation.turn_rect(area, size);
        let stretch = self.stretch(page);
        Some(self.to_screen(Rect {
            left: turned.left.mul_add(stretch, page_area.left),
            top: turned.top.mul_add(stretch, page_area.top),
            right: turned.right.mul_add(stretch, page_area.left),
            bottom: turned.bottom.mul_add(stretch, page_area.top),
        }))
    }

    /// The page under a point of the window, in screen pixels, and where on
    /// the unturned page the point is, in points from its top-left corner,
    /// as [`Self::area_on_screen`] has it. None off the pages.
    pub fn spot_on_page(&self, x: f32, y: f32) -> Option<(u32, f32, f32)> {
        let page = self.visible_pages().find(|&page| {
            self.page_on_screen(page).is_some_and(|area| {
                (area.left..=area.right).contains(&x) && (area.top..=area.bottom).contains(&y)
            })
        })?;
        let page_area = self.layout.page_rect(page)?;
        let stretch = self.stretch(page);
        // Where on the turned page, as it is laid out, before enlarging.
        let across = (x / self.zoom + self.x - page_area.left) / stretch;
        let down = (y / self.zoom + self.y - page_area.top) / stretch;
        let crop = self.crop(page);
        let size = match crop {
            Some(crop) => PageSize {
                width: crop.right - crop.left,
                height: crop.bottom - crop.top,
            },
            None => *self.sizes.get(num::index(page))?,
        };
        let (left, top) = self.rotation.unturn_point((across, down), size);
        let (left, top) = crop.map_or((left, top), |crop| (left + crop.left, top + crop.top));
        Some((page, left, top))
    }

    /// Scrolls just enough to bring an area of an unturned page into the
    /// window. An area that is already in view is left where it is; one
    /// that is not is centred.
    pub fn reveal(&mut self, page: u32, area: Rect) {
        // A page or a spread at a time, the page must be the one shown.
        if self.mode.paged() && !self.layout.row_pages(self.shown).contains(&page) {
            self.go_to_page(page);
        }
        let Some(on_screen) = self.area_on_screen(page, area) else {
            return;
        };
        let (width, height) = self.window;
        if on_screen.top < 0.0 || on_screen.bottom > height {
            let middle = f32::midpoint(on_screen.top, on_screen.bottom);
            self.y += (middle - height / 2.0) / self.zoom;
        }
        if on_screen.left < 0.0 || on_screen.right > width {
            let middle = f32::midpoint(on_screen.left, on_screen.right);
            self.x += (middle - width / 2.0) / self.zoom;
        }
        self.clamp();
    }

    /// Places the pages afresh, after their sizes or the rotation changed.
    fn lay_out(&mut self) {
        let mut shown: Vec<PageSize> = self.crops.as_ref().map_or_else(
            || self.sizes.clone(),
            |crops| {
                crops
                    .iter()
                    .map(|crop| PageSize {
                        width: crop.right - crop.left,
                        height: crop.bottom - crop.top,
                    })
                    .collect()
            },
        );
        self.stretches = if self.crops.is_some() {
            stretches(&shown, self.rotation)
        } else {
            Vec::new()
        };
        for (size, stretch) in shown.iter_mut().zip(&self.stretches) {
            size.width *= stretch;
            size.height *= stretch;
        }
        let arrangement = match self.mode {
            PageMode::Continuous => Arrangement::Columns(self.columns),
            PageMode::Single => Arrangement::Columns(1),
            PageMode::Double => Arrangement::Spreads,
        };
        self.layout = Layout::arranged(&shown, self.rotation, arrangement);
        self.shown = self.shown.min(self.page_count().saturating_sub(1));
    }

    /// Converts a rectangle in the document to screen pixels.
    fn to_screen(&self, area: Rect) -> Rect {
        let (x, y) = self.drawn_from.get().unwrap_or((self.x, self.y));
        Rect {
            left: (area.left - x) * self.zoom,
            top: (area.top - y) * self.zoom,
            right: (area.right - x) * self.zoom,
            bottom: (area.bottom - y) * self.zoom,
        }
    }

    /// Applies the fit, if there is one, and keeps the view within the
    /// document.
    fn refit(&mut self, anchor: Anchor) {
        let (width, height) = self.window;
        if width >= 1.0 && height >= 1.0 {
            let fitted = match self.fit {
                Fit::None => None,
                Fit::Width => Some(width / self.layout.width()),
                Fit::Page => self
                    .layout
                    .row_size(self.current_page())
                    .map(|page| (width / page.width).min(height / page.height)),
            };
            if let Some(zoom) = fitted.filter(|zoom| zoom.is_finite() && *zoom > 0.0) {
                self.set_zoom(zoom, anchor);
            }
        }
        self.clamp();
    }

    /// Changes the zoom, keeping the anchor point where it is on screen.
    fn set_zoom(&mut self, zoom: f32, anchor: Anchor) {
        let zoom = clamp_zoom(zoom);
        let (width, height) = self.window;
        let centre_x = self.x + width / (2.0 * self.zoom);
        self.x = centre_x - width / (2.0 * zoom);
        if anchor == Anchor::Centre {
            let centre_y = self.y + height / (2.0 * self.zoom);
            self.y = centre_y - height / (2.0 * zoom);
        }
        self.zoom = zoom;
        self.clamp();
    }

    /// Keeps the window inside the document. Where the document is narrower
    /// than the window it is centred.
    fn clamp(&mut self) {
        let width = self.window.0 / self.zoom;
        let height = self.window.1 / self.zoom;
        let spare = self.layout.width() - width;
        self.x = if spare <= 0.0 {
            spare / 2.0
        } else {
            self.x.clamp(0.0, spare)
        };
        self.y = match self.shown_band().filter(|_| self.mode.paged()) {
            Some((top, bottom)) => self.y.clamp(top, bottom),
            None => self.y.clamp(0.0, (self.layout.height() - height).max(0.0)),
        };
        if !self.x.is_finite() {
            self.x = 0.0;
        }
        if !self.y.is_finite() {
            self.y = 0.0;
        }
    }
}

/// How much to enlarge each page, of these unturned sizes, for it to be as
/// wide as the widest once turned, up to [`MOST_STRETCH`].
fn stretches(sizes: &[PageSize], rotation: Rotation) -> Vec<f32> {
    let width = |size: &PageSize| rotation.turn_size(*size).width;
    let widest = sizes.iter().map(width).fold(0.0_f32, f32::max);
    sizes
        .iter()
        .map(|size| {
            let stretch = widest / width(size);
            if stretch.is_finite() {
                stretch.clamp(1.0, MOST_STRETCH)
            } else {
                1.0
            }
        })
        .collect()
}

/// Keeps a zoom within the allowed range. Nonsense becomes real size.
const fn clamp_zoom(zoom: f32) -> f32 {
    if zoom.is_finite() {
        zoom.clamp(ZOOM_RANGE.0, ZOOM_RANGE.1)
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::GAP;

    const PAGE: PageSize = PageSize {
        width: 600.0,
        height: 800.0,
    };
    const WINDOW: (f32, f32) = (1200.0, 1000.0);

    fn close(left: f32, right: f32) -> bool {
        (left - right).abs() < 1e-2
    }

    fn reader() -> Reader {
        Reader::new(vec![PAGE; 10], WINDOW)
    }

    #[test]
    fn a_spot_on_a_page_goes_to_the_top_of_the_window() {
        let mut reader = reader();
        let spot = Rect {
            left: 0.0,
            top: 300.0,
            right: 0.0,
            bottom: 300.0,
        };
        reader.go_to_spot(3, Some(300.0));
        assert_eq!(reader.current_page(), 3);
        let on_screen = reader.area_on_screen(3, spot).unwrap();
        assert!(close(on_screen.top, 0.0), "{on_screen:?}");

        reader.go_to_spot(5, None);
        let page = reader.page_on_screen(5).unwrap();
        assert!(close(page.top, 0.0), "{page:?}");
    }

    #[test]
    fn a_page_at_a_time_scrolls_within_it_and_turns_at_its_edges() {
        let mut reader = reader();
        reader.set_page_mode(PageMode::Single);
        // Fitted to the width, a page is taller than the window: 1600
        // pixels against 1000.
        assert_eq!(reader.visible_pages(), 0..1);
        reader.scroll_by(0.0, 400.0);
        assert_eq!(reader.current_page(), 0);
        assert_eq!(reader.visible_pages(), 0..1, "only the page shown");
        // Scrolling on stops at the bottom of the page, then turns.
        reader.scroll_by(0.0, 400.0);
        let page = reader.page_on_screen(0).unwrap();
        assert!(close(page.bottom, 1000.0), "{page:?}");
        reader.scroll_by(0.0, 1.0);
        assert_eq!(reader.current_page(), 1);
        assert!(close(reader.page_on_screen(1).unwrap().top, 0.0));
        // Back from the top of a page turns to the bottom of the one before.
        reader.scroll_by(0.0, -1.0);
        assert_eq!(reader.current_page(), 0);
        assert!(close(reader.page_on_screen(0).unwrap().bottom, 1000.0));
        // Not past either end.
        reader.go_to_page(9);
        reader.scroll_by(0.0, 5000.0);
        reader.scroll_by(0.0, 5000.0);
        assert_eq!(reader.current_page(), 9);
        reader.page_by(-100);
        assert_eq!(reader.current_page(), 0);
        reader.scroll_by(0.0, -1.0);
        assert_eq!(reader.current_page(), 0);
    }

    #[test]
    fn a_page_that_fits_is_centred_and_any_scroll_turns_it() {
        let mut reader = reader();
        reader.set_page_mode(PageMode::Single);
        reader.fit(Fit::Page);
        let page = reader.page_on_screen(0).unwrap();
        assert!(
            close(page.top, 0.0) && close(page.bottom, 1000.0),
            "{page:?}"
        );
        reader.zoom_to(1.0);
        let page = reader.page_on_screen(0).unwrap();
        assert!(
            close(page.top, 100.0) && close(page.bottom, 900.0),
            "{page:?}"
        );
        reader.scroll_by(0.0, 40.0);
        assert_eq!(reader.current_page(), 1);
        assert_eq!(reader.visible_pages(), 1..2);
    }

    #[test]
    fn two_pages_at_a_time_face_each_other_after_the_cover() {
        let mut reader = reader();
        reader.set_page_mode(PageMode::Double);
        assert_eq!(reader.pages_per_row(), 2);
        assert_eq!(reader.visible_pages(), 0..1, "the cover alone");
        reader.page_by(1);
        assert_eq!(reader.visible_pages(), 1..3);
        reader.page_by(2);
        assert_eq!(reader.visible_pages(), 5..7);
        // Going to a page shows its spread.
        reader.go_to_page(4);
        assert_eq!(reader.visible_pages(), 3..5);
        assert_eq!(reader.current_page(), 4);
        // A search hit on another page turns to it.
        let hit = Rect {
            left: 10.0,
            top: 10.0,
            right: 20.0,
            bottom: 20.0,
        };
        reader.reveal(8, hit);
        assert_eq!(reader.visible_pages(), 7..9);
        reader.page_by(5);
        assert_eq!(reader.visible_pages(), 9..10, "the last page alone");
    }

    #[test]
    fn changing_how_pages_show_keeps_the_place() {
        let mut reader = reader();
        reader.go_to_page(6);
        reader.set_page_mode(PageMode::Single);
        assert_eq!(reader.current_page(), 6);
        reader.set_page_mode(PageMode::Double);
        assert_eq!(reader.current_page(), 6);
        assert_eq!(reader.visible_pages(), 5..7);
        reader.set_page_mode(PageMode::Continuous);
        assert_eq!(reader.current_page(), 6);
        assert_eq!(reader.page_mode().next(), PageMode::Single);
        assert_eq!(PageMode::from_name("double"), Some(PageMode::Double));
        assert_eq!(PageMode::from_name("book"), None);
    }

    #[test]
    fn a_point_on_screen_is_found_on_its_page_however_the_page_is_turned() {
        for turns in 0..4 {
            let mut reader = reader();
            for _ in 0..turns {
                reader.rotate();
            }
            reader.go_to_page(2);
            reader.scroll_by(30.0, 50.0);
            let area = Rect {
                left: 100.0,
                top: 200.0,
                right: 130.0,
                bottom: 260.0,
            };
            let on_screen = reader.area_on_screen(2, area).unwrap();
            let middle = (
                f32::midpoint(on_screen.left, on_screen.right),
                f32::midpoint(on_screen.top, on_screen.bottom),
            );
            let (page, left, top) = reader.spot_on_page(middle.0, middle.1).unwrap();
            assert_eq!(page, 2, "turned {turns} times");
            assert!(
                close(left, 115.0) && close(top, 230.0),
                "{left}, {top} turned {turns} times"
            );
        }
    }

    #[test]
    fn trimmed_pages_are_enlarged_to_the_width_of_the_widest() {
        let mut reader = reader();
        let crop = |left: f32, right: f32| Rect {
            left,
            top: 100.0,
            right,
            bottom: 700.0,
        };
        // Page 0 keeps 500 points of its width, page 1 only 400, and page
        // 2, a title, 100: enlarged, page 1 matches page 0, and page 2 is
        // held at twice its size.
        let mut crops = vec![crop(50.0, 550.0); 10];
        crops[1] = crop(100.0, 500.0);
        crops[2] = crop(250.0, 350.0);
        reader.set_crops(Some(crops));
        assert!(close(reader.stretch(0), 1.0));
        assert!(close(reader.stretch(1), 1.25));
        assert!(close(reader.stretch(2), 2.0));
        reader.go_to_page(1);
        let wide = reader.page_on_screen(0).unwrap();
        let narrow = reader.page_on_screen(1).unwrap();
        let title = reader.page_on_screen(2).unwrap();
        assert!(close(wide.right - wide.left, 1200.0), "{wide:?}");
        assert!(close(narrow.right - narrow.left, 1200.0), "{narrow:?}");
        assert!(close(title.right - title.left, 480.0), "{title:?}");
        // An area of page 1 is drawn where the enlarged page has it: its
        // right edge, 500 points across, at the right of the page.
        let edge = Rect {
            left: 450.0,
            top: 200.0,
            right: 500.0,
            bottom: 300.0,
        };
        let on_screen = reader.area_on_screen(1, edge).unwrap();
        assert!(close(on_screen.right, narrow.right), "{on_screen:?}");
        assert!(
            close(
                on_screen.bottom - on_screen.top,
                100.0 * 1.25 * reader.zoom()
            ),
            "{on_screen:?}"
        );
        // Untrimmed, nothing is enlarged.
        reader.set_crops(None);
        assert!(close(reader.stretch(1), 1.0));
    }

    #[test]
    fn a_new_view_fits_the_width_at_the_top() {
        let reader = reader();
        assert!(close(reader.zoom(), 2.0));
        assert_eq!(reader.current_page(), 0);
        assert_eq!(reader.visible_pages(), 0..1);
        let page = reader.page_on_screen(0).unwrap();
        assert!(close(page.left, 0.0));
        assert!(close(page.right, 1200.0));
        assert!(close(page.top, 0.0));
    }

    #[test]
    fn scrolling_stops_at_the_ends() {
        let mut reader = reader();
        reader.scroll_by(0.0, -500.0);
        assert!(close(reader.view(false).y, 0.0));
        reader.scroll_by(0.0, 100.0);
        assert!(close(reader.view(false).y, 50.0));
        reader.scroll_by(0.0, 1e9);
        let end = GAP.mul_add(9.0, 10.0 * 800.0) - 1000.0 / 2.0;
        assert!(close(reader.view(false).y, end));
        assert_eq!(reader.current_page(), 9);
    }

    #[test]
    fn pages_are_addressed_and_stepped() {
        let mut reader = reader();
        reader.go_to_page(3);
        assert_eq!(reader.current_page(), 3);
        assert!(close(reader.page_on_screen(3).unwrap().top, 0.0));
        reader.page_by(2);
        assert_eq!(reader.current_page(), 5);
        reader.page_by(-100);
        assert_eq!(reader.current_page(), 0);
        reader.go_to_page(500);
        assert_eq!(reader.current_page(), 9);
    }

    #[test]
    fn the_bottom_of_a_page_can_be_reached() {
        let mut reader = reader();
        reader.go_to_page(2);
        reader.page_bottom();
        assert_eq!(reader.current_page(), 2);
        assert!(close(reader.page_on_screen(2).unwrap().bottom, 1000.0));
        reader.page_top();
        assert!(close(reader.page_on_screen(2).unwrap().top, 0.0));
    }

    #[test]
    fn zooming_keeps_the_middle_and_stops_fitting() {
        let mut reader = reader();
        reader.go_to_page(4);
        let middle_before = reader.view(false).y + 1000.0 / (2.0 * reader.zoom());
        reader.zoom_to(1.0);
        let middle_after = reader.view(false).y + 1000.0 / 2.0;
        assert!(close(middle_before, middle_after));
        assert_eq!(reader.view(false).fit, Fit::None);
        // The page is now narrower than the window, so it is centred.
        let page = reader.page_on_screen(4).unwrap();
        assert!(close(page.left, 300.0));
        assert!(close(page.right, 900.0));

        reader.zoom_to(1000.0);
        assert!(close(reader.zoom(), 10.0));
        reader.zoom_to(f32::NAN);
        assert!(close(reader.zoom(), 1.0));
    }

    #[test]
    fn a_fit_follows_the_window() {
        let mut reader = reader();
        reader.go_to_page(1);
        let top = reader.view(false).y;
        reader.set_window((600.0, 1000.0));
        assert!(close(reader.zoom(), 1.0));
        assert!(close(reader.view(false).y, top), "the top line stays put");

        reader.fit(Fit::Page);
        assert!(close(reader.zoom(), 1.0));
        reader.set_window((600.0, 400.0));
        assert!(close(reader.zoom(), 0.5));
    }

    #[test]
    fn a_window_without_size_is_tolerated() {
        let mut reader = Reader::new(vec![PAGE; 3], (0.0, 0.0));
        assert!(close(reader.zoom(), 1.0));
        reader.go_to_page(2);
        reader.set_window(WINDOW);
        assert!(close(reader.zoom(), 2.0));
        assert_eq!(reader.current_page(), 2);
    }

    #[test]
    fn rotating_stays_on_the_page_and_refits() {
        let mut reader = reader();
        reader.go_to_page(6);
        reader.rotate();
        assert_eq!(reader.rotation(), Rotation::Quarter);
        assert_eq!(reader.current_page(), 6);
        assert!(close(reader.zoom(), 1200.0 / 800.0));
    }

    #[test]
    fn a_view_survives_a_round_trip() {
        let mut reader = reader();
        reader.rotate();
        reader.zoom_to(1.25);
        reader.go_to_page(7);
        let view = reader.view(true);

        let mut again = Reader::new(vec![PAGE; 10], WINDOW);
        again.restore(view);
        assert_eq!(again.view(true), view);
    }

    #[test]
    fn revealing_centres_what_is_out_of_view() {
        let mut reader = reader();
        let area = Rect {
            left: 100.0,
            top: 700.0,
            right: 200.0,
            bottom: 720.0,
        };
        // In view already: nothing moves.
        let inside = Rect {
            top: 100.0,
            bottom: 120.0,
            ..area
        };
        reader.reveal(0, inside);
        assert!(close(reader.view(false).y, 0.0));

        reader.reveal(0, area);
        let shown = reader.area_on_screen(0, area).unwrap();
        assert!(close(f32::midpoint(shown.top, shown.bottom), 500.0));

        reader.reveal(8, area);
        assert_eq!(reader.visible_pages().start, 8);
    }

    #[test]
    fn trimming_shows_only_the_content() {
        let mut reader = reader();
        reader.go_to_page(3);
        reader.scroll_by(0.0, 800.0);
        // Content with 100 points of margin on every side.
        let content = Rect {
            left: 100.0,
            top: 100.0,
            right: 500.0,
            bottom: 700.0,
        };
        reader.set_crops(Some(vec![content; 10]));
        assert!(reader.view(false).trim);
        assert_eq!(reader.crop(3), Some(content));
        // The width still fits the window, now with larger text.
        assert!(close(reader.zoom(), 3.0));
        // Still on the same page, half way down it.
        assert_eq!(reader.current_page(), 3);
        let page = reader.page_on_screen(3).unwrap();
        assert!(close(page.left, 0.0));
        assert!(close(page.right, 1200.0));
        assert!(close(page.top, -900.0));
        // An area is placed from the corner of the content.
        reader.go_to_page(3);
        let corner = Rect {
            left: 100.0,
            top: 100.0,
            right: 150.0,
            bottom: 120.0,
        };
        let shown = reader.area_on_screen(3, corner).unwrap();
        assert!(close(shown.left, 0.0));
        assert!(close(shown.top, 0.0));
        assert!(close(shown.right, 150.0));

        reader.set_crops(None);
        assert!(!reader.view(false).trim);
        assert!(close(reader.zoom(), 2.0));
        assert_eq!(reader.current_page(), 3);
    }

    #[test]
    fn crops_for_another_document_are_ignored() {
        let mut reader = reader();
        let crop = Rect {
            left: 0.0,
            top: 0.0,
            right: 10.0,
            bottom: 10.0,
        };
        reader.set_crops(Some(vec![crop; 3]));
        assert!(!reader.view(false).trim);
        assert!(close(reader.zoom(), 2.0));
    }

    #[test]
    fn an_empty_document_does_not_break() {
        let mut reader = Reader::new(Vec::new(), WINDOW);
        reader.scroll_by(10.0, 10.0);
        reader.page_by(1);
        reader.rotate();
        reader.fit(Fit::Page);
        assert_eq!(reader.page_count(), 0);
        assert_eq!(reader.visible_pages(), 0..0);
        assert!(reader.view(false).y.is_finite());
    }

    #[test]
    fn a_place_is_kept_whatever_the_zoom() {
        let mut reader = reader();
        reader.go_to_page(3);
        reader.scroll_by(0.0, 200.0);
        let place = reader.place();
        assert_eq!(place.page, 3);
        assert!(place.down > 0.0 && place.down < 1.0, "{place:?}");

        reader.zoom_to(2.5);
        reader.go_to_page(8);
        reader.go_to_place(place);
        assert!(reader.place().near(place), "{:?} {place:?}", reader.place());
    }

    #[test]
    fn two_columns_fit_side_by_side_and_step_by_rows() {
        let mut reader = reader();
        reader.go_to_page(4);
        reader.set_columns(2);
        // Two pages and the gap between fill the width.
        assert!(close(reader.zoom(), 1200.0 / GAP.mul_add(1.0, 1200.0)));
        assert_eq!(reader.current_page(), 4);
        assert_eq!(reader.visible_pages().start, 4);
        assert!(reader.visible_pages().end >= 6);
        let left = reader.page_on_screen(4).unwrap();
        let right = reader.page_on_screen(5).unwrap();
        assert!(close(left.top, right.top) && right.left > left.right);
        reader.page_by(1);
        assert_eq!(reader.current_page(), 6);
        reader.page_by(-1);
        assert_eq!(reader.current_page(), 4);
        reader.fit(Fit::Page);
        let width = 1200.0 / GAP.mul_add(1.0, 1200.0);
        assert!(close(reader.zoom(), width.min(1000.0 / PAGE.height)));
        // Kept with the view, for the next time.
        let view = reader.view(false);
        assert_eq!(view.columns, 2);
        let mut again = Reader::new(vec![PAGE; 10], WINDOW);
        again.restore(view);
        assert_eq!(again.view(false), view);
        reader.set_columns(1);
        assert_eq!(reader.current_page(), 4);
    }

    #[test]
    fn a_place_past_the_end_is_the_last_page() {
        let mut reader = reader();
        reader.go_to_place(Place {
            page: 99,
            down: f32::NAN,
        });
        assert_eq!(reader.current_page(), 9);
    }
}
