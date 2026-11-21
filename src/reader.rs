//! The view on an open document: which part shows, and how large.
//!
//! A [`Reader`] knows the page layout, the zoom and the scroll position, and
//! moves them in response to commands. It knows nothing about rendering or
//! the window toolkit, so it can be tested by itself.

use std::ops::Range;

use crate::history::{Fit, View};
use crate::layout::{Layout, PageSize, Rect, Rotation};
use crate::num;

/// The smallest and largest zoom, in screen pixels per point.
const ZOOM_RANGE: (f32, f32) = (0.1, 10.0);

/// How much one zoom step enlarges the page.
const ZOOM_STEP: f32 = 1.1;

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
    /// How many pages sit side by side.
    columns: usize,
}

impl Reader {
    /// A view of the start of a document, fitted to the window's width.
    pub fn new(sizes: Vec<PageSize>, window: (f32, f32)) -> Self {
        let layout = Layout::new(&sizes, Rotation::None);
        let mut reader = Self {
            sizes,
            crops: None,
            layout,
            rotation: Rotation::None,
            zoom: 1.0,
            fit: Fit::Width,
            x: 0.0,
            y: 0.0,
            window,
            columns: 1,
        };
        reader.refit(Anchor::Top);
        reader
    }

    /// Sets how many pages sit side by side, staying at the same place.
    pub fn set_columns(&mut self, columns: usize) {
        let columns = columns.max(1);
        if columns == self.columns {
            return;
        }
        let place = self.place();
        self.columns = columns;
        self.lay_out();
        self.refit(Anchor::Top);
        self.go_to_place(place);
    }

    /// Puts the view back the way it was left.
    pub fn restore(&mut self, view: View) {
        self.rotation = view.rotation;
        self.lay_out();
        self.zoom = clamp_zoom(view.zoom);
        self.fit = view.fit;
        self.x = view.x;
        self.y = view.y;
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

    pub const fn rotation(&self) -> Rotation {
        self.rotation
    }

    pub fn page_count(&self) -> u32 {
        self.layout.page_count()
    }

    /// The page at the top of the window, counted from 0.
    pub fn current_page(&self) -> u32 {
        self.layout.page_at(self.y)
    }

    /// The pages at least partly inside the window.
    pub fn visible_pages(&self) -> Range<u32> {
        self.layout
            .pages_between(self.y, self.y + self.window.1 / self.zoom)
    }

    /// Tells the reader the window's size, in screen pixels.
    pub fn set_window(&mut self, window: (f32, f32)) {
        self.window = window;
        self.refit(Anchor::Top);
    }

    /// Scrolls by a distance in screen pixels: right and down are positive.
    pub fn scroll_by(&mut self, right: f32, down: f32) {
        self.x += right / self.zoom;
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
            self.y = area.top;
            self.clamp();
        }
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
        Some(self.to_screen(Rect {
            left: page_area.left + turned.left,
            top: page_area.top + turned.top,
            right: page_area.left + turned.right,
            bottom: page_area.top + turned.bottom,
        }))
    }

    /// Scrolls just enough to bring an area of an unturned page into the
    /// window. An area that is already in view is left where it is; one
    /// that is not is centred.
    pub fn reveal(&mut self, page: u32, area: Rect) {
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
        let shown: Vec<PageSize> = self.crops.as_ref().map_or_else(
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
        self.layout = Layout::in_columns(&shown, self.rotation, self.columns);
    }

    /// Converts a rectangle in the document to screen pixels.
    fn to_screen(&self, area: Rect) -> Rect {
        Rect {
            left: (area.left - self.x) * self.zoom,
            top: (area.top - self.y) * self.zoom,
            right: (area.right - self.x) * self.zoom,
            bottom: (area.bottom - self.y) * self.zoom,
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
        self.y = self.y.clamp(0.0, (self.layout.height() - height).max(0.0));
        if !self.x.is_finite() {
            self.x = 0.0;
        }
        if !self.y.is_finite() {
            self.y = 0.0;
        }
    }
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
