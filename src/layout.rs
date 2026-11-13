//! Where pages sit in the document.
//!
//! Pages are stacked in one column, each centred on the widest page, with a
//! gap between them. Everything here is measured in PDF points at zoom 1;
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

/// The position of every page in the column.
#[derive(Debug, Clone, Default)]
pub struct Layout {
    /// The size of each page, already turned.
    sizes: Vec<PageSize>,
    /// The top edge of each page.
    tops: Vec<f32>,
    /// The width of the widest page.
    width: f32,
    /// The bottom edge of the last page.
    height: f32,
}

impl Layout {
    /// Lays out pages of the given unturned sizes.
    pub fn new(pages: &[PageSize], rotation: Rotation) -> Self {
        let sizes: Vec<PageSize> = pages.iter().map(|size| rotation.turn_size(*size)).collect();
        let mut tops = Vec::with_capacity(sizes.len());
        let mut cursor = 0.0_f32;
        let mut width = 0.0_f32;
        let mut height = 0.0_f32;
        for size in &sizes {
            tops.push(cursor);
            height = cursor + size.height;
            cursor = height + GAP;
            width = width.max(size.width);
        }
        Self {
            sizes,
            tops,
            width,
            height,
        }
    }

    /// The number of pages.
    pub fn page_count(&self) -> u32 {
        num::len(self.sizes.len())
    }

    /// The width of the widest page.
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

    /// The rectangle a page occupies in the column.
    pub fn page_rect(&self, page: u32) -> Option<Rect> {
        let size = self.size(page)?;
        let top = *self.tops.get(num::index(page))?;
        let left = (self.width - size.width) / 2.0;
        Some(Rect {
            left,
            top,
            right: left + size.width,
            bottom: top + size.height,
        })
    }

    /// The page at a height in the column. A height in the gap below a page
    /// counts as that page, and heights outside the column count as the
    /// first or last page.
    pub fn page_at(&self, y: f32) -> u32 {
        let after = self.tops.partition_point(|top| *top <= y);
        num::len(after.saturating_sub(1))
    }

    /// The pages that overlap the band between two heights.
    pub fn pages_between(&self, top: f32, bottom: f32) -> Range<u32> {
        if self.sizes.is_empty() {
            return 0..0;
        }
        let first = self.page_at(top);
        let last = self.page_at(bottom);
        first..last.saturating_add(1)
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
