//! Where pages sit in the document.
//!
//! Pages are stacked in rows of one or more, each row centred on the widest,
//! with a gap between pages and between rows. Everything here is measured in PDF points at zoom 1;
//! the view multiplies by the zoom to get screen positions.

use std::ops::Range;

use crate::num;

/// The space between two pages, in points.
pub const GAP: f32 = 8.0;

/// The width and height of a page, in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSize {
    pub width: f32,
    pub height: f32,
}

/// A rectangle, in points, by its left, top, right and bottom edges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

/// How far every page is turned clockwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Rotation {
    #[default]
    None,
    Quarter,
    Half,
    ThreeQuarters,
}

impl Rotation {
    /// The rotation a quarter turn further clockwise.
    pub const fn next(self) -> Self {
        match self {
            Self::None => Self::Quarter,
            Self::Quarter => Self::Half,
            Self::Half => Self::ThreeQuarters,
            Self::ThreeQuarters => Self::None,
        }
    }

    /// The rotation in degrees.
    pub const fn degrees(self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::Quarter => 90.0,
            Self::Half => 180.0,
            Self::ThreeQuarters => 270.0,
        }
    }

    /// The number of quarter turns, as stored in the history database.
    pub const fn quarters(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Quarter => 1,
            Self::Half => 2,
            Self::ThreeQuarters => 3,
        }
    }

    /// The rotation for a number of quarter turns. Whole turns are dropped.
    pub const fn from_quarters(quarters: u8) -> Self {
        match quarters % 4 {
            1 => Self::Quarter,
            2 => Self::Half,
            3 => Self::ThreeQuarters,
            _ => Self::None,
        }
    }

    /// The size a page of the given size has once turned.
    pub const fn turn_size(self, size: PageSize) -> PageSize {
        match self {
            Self::None | Self::Half => size,
            Self::Quarter | Self::ThreeQuarters => PageSize {
                width: size.height,
                height: size.width,
            },
        }
    }

    /// Where a rectangle on an unturned page of the given size ends up once
    /// the page is turned.
    /// Where a point of the turned page, of this unturned size, is on the
    /// page unturned: the reverse of [`Self::turn_rect`] for a point.
    pub fn unturn_point(self, (across, down): (f32, f32), size: PageSize) -> (f32, f32) {
        match self {
            Self::None => (across, down),
            Self::Quarter => (down, size.height - across),
            Self::Half => (size.width - across, size.height - down),
            Self::ThreeQuarters => (size.width - down, across),
        }
    }

    pub fn turn_rect(self, rect: Rect, size: PageSize) -> Rect {
        match self {
            Self::None => rect,
            Self::Quarter => Rect {
                left: size.height - rect.bottom,
                top: rect.left,
                right: size.height - rect.top,
                bottom: rect.right,
            },
            Self::Half => Rect {
                left: size.width - rect.right,
                top: size.height - rect.bottom,
                right: size.width - rect.left,
                bottom: size.height - rect.top,
            },
            Self::ThreeQuarters => Rect {
                left: rect.top,
                top: size.width - rect.right,
                right: rect.bottom,
                bottom: size.width - rect.left,
            },
        }
    }
}

/// How pages are put into rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrangement {
    /// So many pages to a row, 1 or more.
    Columns(usize),
    /// The first page alone, as a book's cover, then two pages to a row,
    /// so that facing pages sit side by side as in the printed book.
    Spreads,
}

impl Arrangement {
    /// How many pages each row holds, in turn, for a number of pages.
    fn row_lengths(self, pages: usize) -> Vec<usize> {
        let (first, rest) = match self {
            Self::Columns(columns) => (columns.max(1), columns.max(1)),
            Self::Spreads => (1, 2),
        };
        let mut lengths = Vec::new();
        let mut left = pages;
        let mut length = first;
        while left > 0 {
            let taken = length.min(left);
            lengths.push(taken);
            left = left.saturating_sub(taken);
            length = rest;
        }
        lengths
    }
}

/// The position of every page in the column.
#[derive(Debug, Clone, Default)]
pub struct Layout {
    /// The size of each page, already turned.
    sizes: Vec<PageSize>,
    /// The top edge of each page: that of its row.
    tops: Vec<f32>,
    /// How far from the left of its row each page starts.
    offsets: Vec<f32>,
    /// The width and height of each row.
    rows: Vec<PageSize>,
    /// The row each page is in.
    row_of: Vec<usize>,
    /// The first page of each row.
    firsts: Vec<usize>,
    /// The width of the widest row.
    width: f32,
    /// The bottom edge of the last row.
    height: f32,
}

impl Layout {
    /// Lays out pages of the given unturned sizes, one to a row.
    pub fn new(pages: &[PageSize], rotation: Rotation) -> Self {
        Self::in_columns(pages, rotation, 1)
    }

    /// Lays out pages of the given unturned sizes, so many to a row, side
    /// by side and aligned at the top. Fewer than one is one.
    pub fn in_columns(pages: &[PageSize], rotation: Rotation, columns: usize) -> Self {
        Self::arranged(pages, rotation, Arrangement::Columns(columns))
    }

    /// Lays out pages of the given unturned sizes in rows as arranged, side
    /// by side and aligned at the top.
    pub fn arranged(pages: &[PageSize], rotation: Rotation, arrangement: Arrangement) -> Self {
        let sizes: Vec<PageSize> = pages.iter().map(|size| rotation.turn_size(*size)).collect();
        let mut tops = Vec::with_capacity(sizes.len());
        let mut offsets = Vec::with_capacity(sizes.len());
        let mut row_of = Vec::with_capacity(sizes.len());
        let mut firsts = Vec::new();
        let mut rows = Vec::new();
        let mut cursor = 0.0_f32;
        let mut width = 0.0_f32;
        let mut height = 0.0_f32;
        let mut rest = sizes.as_slice();
        for length in arrangement.row_lengths(sizes.len()) {
            let (row, after) = rest.split_at(length.min(rest.len()));
            rest = after;
            firsts.push(tops.len());
            let mut left = 0.0_f32;
            let mut tallest = 0.0_f32;
            for size in row {
                tops.push(cursor);
                offsets.push(left);
                row_of.push(rows.len());
                left += size.width + GAP;
                tallest = tallest.max(size.height);
            }
            let row_width = (left - GAP).max(0.0);
            rows.push(PageSize {
                width: row_width,
                height: tallest,
            });
            height = cursor + tallest;
            cursor = height + GAP;
            width = width.max(row_width);
        }
        Self {
            sizes,
            tops,
            offsets,
            rows,
            row_of,
            firsts,
            width,
            height,
        }
    }

    /// The number of pages.
    pub fn page_count(&self) -> u32 {
        num::len(self.sizes.len())
    }

    /// The width of the widest row.
    pub const fn width(&self) -> f32 {
        self.width
    }

    /// The height of the whole column.
    pub const fn height(&self) -> f32 {
        self.height
    }

    /// The turned size of a page.
    pub fn size(&self, page: u32) -> Option<PageSize> {
        self.sizes.get(num::index(page)).copied()
    }

    /// The size of the row a page is in.
    pub fn row_size(&self, page: u32) -> Option<PageSize> {
        self.rows.get(*self.row_of.get(num::index(page))?).copied()
    }

    /// The top and bottom edges of the row a page is in.
    pub fn row_band(&self, page: u32) -> Option<(f32, f32)> {
        let top = *self.tops.get(num::index(page))?;
        Some((top, top + self.row_size(page)?.height))
    }

    /// The pages of the row a page is in.
    pub fn row_pages(&self, page: u32) -> Range<u32> {
        let Some(&row) = self.row_of.get(num::index(page)) else {
            return 0..0;
        };
        self.pages_of_row(row)
    }

    /// The first page of the row so many rows after, or before, the row a
    /// page is in, if there is such a row.
    pub fn row_page_by(&self, page: u32, rows: i64) -> Option<u32> {
        let row = i64::try_from(*self.row_of.get(num::index(page))?).ok()?;
        let target = usize::try_from(row.checked_add(rows)?).ok()?;
        self.firsts.get(target).map(|first| num::len(*first))
    }

    /// The pages of a row, counted from 0.
    fn pages_of_row(&self, row: usize) -> Range<u32> {
        let first = self.firsts.get(row).copied().unwrap_or(0);
        let end = self
            .firsts
            .get(row.saturating_add(1))
            .copied()
            .unwrap_or(self.sizes.len());
        num::len(first)..num::len(end)
    }

    /// The rectangle a page occupies in the column.
    pub fn page_rect(&self, page: u32) -> Option<Rect> {
        let size = self.size(page)?;
        let top = *self.tops.get(num::index(page))?;
        let offset = *self.offsets.get(num::index(page))?;
        let row = self.row_size(page)?;
        let left = (self.width - row.width) / 2.0 + offset;
        Some(Rect {
            left,
            top,
            right: left + size.width,
            bottom: top + size.height,
        })
    }

    /// The first page of the row a page is in.
    fn row_start(&self, page: usize) -> usize {
        self.row_of
            .get(page)
            .and_then(|row| self.firsts.get(*row))
            .copied()
            .unwrap_or(0)
    }

    /// The first page of the row at a height in the column. A height in
    /// the gap below a row counts as that row, and heights outside the
    /// column count as the first or last row.
    pub fn page_at(&self, y: f32) -> u32 {
        let after = self.tops.partition_point(|top| *top <= y);
        num::len(self.row_start(after.saturating_sub(1)))
    }

    /// The pages in the rows that overlap the band between two heights.
    pub fn pages_between(&self, top: f32, bottom: f32) -> Range<u32> {
        if self.sizes.is_empty() {
            return 0..0;
        }
        let first = self.page_at(top);
        let end = self.row_pages(self.page_at(bottom)).end;
        first..end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A4: PageSize = PageSize {
        width: 595.0,
        height: 842.0,
    };
    const WIDE: PageSize = PageSize {
        width: 842.0,
        height: 595.0,
    };

    fn close(left: f32, right: f32) -> bool {
        (left - right).abs() < 1e-3
    }

    #[test]
    fn pages_stack_with_gaps_and_centre_on_the_widest() {
        let layout = Layout::new(&[A4, WIDE, A4], Rotation::None);
        assert_eq!(layout.page_count(), 3);
        assert!(close(layout.width(), 842.0));
        assert!(close(layout.height(), 842.0 + GAP + 595.0 + GAP + 842.0));

        let first = layout.page_rect(0).unwrap();
        assert!(close(first.left, (842.0 - 595.0) / 2.0));
        assert!(close(first.top, 0.0));
        let second = layout.page_rect(1).unwrap();
        assert!(close(second.left, 0.0));
        assert!(close(second.top, 842.0 + GAP));
        assert!(layout.page_rect(3).is_none());
    }

    #[test]
    fn heights_map_to_pages() {
        let layout = Layout::new(&[A4, A4, A4], Rotation::None);
        assert_eq!(layout.page_at(-5.0), 0);
        assert_eq!(layout.page_at(0.0), 0);
        assert_eq!(layout.page_at(841.0), 0);
        assert_eq!(
            layout.page_at(845.0),
            0,
            "the gap belongs to the page above"
        );
        assert_eq!(layout.page_at(850.0), 1);
        assert_eq!(layout.page_at(1e9), 2);
        assert_eq!(layout.pages_between(800.0, 900.0), 0..2);
        assert_eq!(layout.pages_between(0.0, 100.0), 0..1);
    }

    #[test]
    fn pages_sit_side_by_side_in_columns() {
        let layout = Layout::in_columns(&[A4, WIDE, A4], Rotation::None, 2);
        // The first row holds A4 and WIDE, as tall as A4; the second row
        // holds the last A4 alone, centred.
        assert!(close(layout.width(), 595.0 + GAP + 842.0));
        assert!(close(layout.height(), 842.0 + GAP + 842.0));
        let first = layout.page_rect(0).unwrap();
        let second = layout.page_rect(1).unwrap();
        let third = layout.page_rect(2).unwrap();
        assert!(close(first.left, 0.0) && close(first.top, 0.0));
        assert!(close(second.left, 595.0 + GAP) && close(second.top, 0.0));
        assert!(close(third.top, 842.0 + GAP));
        assert!(close(third.left, (layout.width() - 595.0) / 2.0));
        // A height names the first page of its row, and a band takes in
        // every page of the rows it meets.
        assert_eq!(layout.page_at(100.0), 0);
        assert_eq!(layout.page_at(900.0), 2);
        assert_eq!(layout.pages_between(0.0, 100.0), 0..2);
        assert_eq!(layout.pages_between(0.0, 900.0), 0..3);
        let row = layout.row_size(1).unwrap();
        assert!(close(row.height, 842.0));
        let empty = Layout::in_columns(&[], Rotation::None, 0);
        assert!(empty.row_size(0).is_none());
    }

    #[test]
    fn spreads_put_the_cover_alone_and_then_pages_in_pairs() {
        let layout = Layout::arranged(&[A4; 6], Rotation::None, Arrangement::Spreads);
        // Rows: 0 | 1 2 | 3 4 | 5.
        assert_eq!(layout.row_pages(0), 0..1);
        assert_eq!(layout.row_pages(1), 1..3);
        assert_eq!(layout.row_pages(2), 1..3);
        assert_eq!(layout.row_pages(4), 3..5);
        assert_eq!(layout.row_pages(5), 5..6);
        assert!(close(layout.width(), 595.0 + GAP + 595.0));
        // The cover and the last page sit alone, centred.
        let cover = layout.page_rect(0).unwrap();
        assert!(close(cover.left, (layout.width() - 595.0) / 2.0));
        let right = layout.page_rect(2).unwrap();
        assert!(close(right.left, 595.0 + GAP) && close(right.top, 842.0 + GAP));
        assert_eq!(layout.page_at(842.0 + GAP + 1.0), 1);
        assert_eq!(layout.pages_between(0.0, 842.0 + GAP + 1.0), 0..3);
        let (top, bottom) = layout.row_band(2).unwrap();
        assert!(close(top, 842.0 + GAP) && close(bottom, 2.0_f32.mul_add(842.0, GAP)));
        // Stepping by rows, and off either end.
        assert_eq!(layout.row_page_by(0, 1), Some(1));
        assert_eq!(layout.row_page_by(2, 1), Some(3));
        assert_eq!(layout.row_page_by(2, -1), Some(0));
        assert_eq!(layout.row_page_by(5, 1), None);
        assert_eq!(layout.row_page_by(0, -1), None);
        // A document of one page is the cover alone.
        let one = Layout::arranged(&[A4], Rotation::None, Arrangement::Spreads);
        assert_eq!(one.row_pages(0), 0..1);
    }

    #[test]
    fn an_empty_document_has_no_pages() {
        let layout = Layout::new(&[], Rotation::None);
        assert_eq!(layout.page_count(), 0);
        assert_eq!(layout.pages_between(0.0, 100.0), 0..0);
        assert_eq!(layout.page_at(10.0), 0);
    }

    #[test]
    fn turning_swaps_sides_and_moves_rectangles() {
        assert_eq!(Rotation::Quarter.turn_size(A4), WIDE);
        assert_eq!(Rotation::Half.turn_size(A4), A4);

        // A mark in the top-left corner of an upright page.
        let mark = Rect {
            left: 10.0,
            top: 20.0,
            right: 110.0,
            bottom: 40.0,
        };
        // A quarter turn clockwise moves the top-left to the top-right.
        let turned = Rotation::Quarter.turn_rect(mark, A4);
        assert!(close(turned.left, 842.0 - 40.0));
        assert!(close(turned.right, 842.0 - 20.0));
        assert!(close(turned.top, 10.0));
        assert!(close(turned.bottom, 110.0));
        // A half turn moves it to the bottom-right.
        let turned = Rotation::Half.turn_rect(mark, A4);
        assert!(close(turned.left, 595.0 - 110.0));
        assert!(close(turned.bottom, 842.0 - 20.0));
        // Three quarters moves it to the bottom-left.
        let turned = Rotation::ThreeQuarters.turn_rect(mark, A4);
        assert!(close(turned.left, 20.0));
        assert!(close(turned.bottom, 595.0 - 10.0));
    }

    #[test]
    fn four_quarter_turns_come_back_round() {
        let mut rotation = Rotation::None;
        for _ in 0..4 {
            rotation = rotation.next();
        }
        assert_eq!(rotation, Rotation::None);
        assert_eq!(
            Rotation::from_quarters(Rotation::Half.quarters()),
            Rotation::Half
        );
        assert_eq!(Rotation::from_quarters(7), Rotation::ThreeQuarters);
    }
}
