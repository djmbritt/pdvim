//! What the window is drawn with, over Rune: colours and the plain shapes
//! of geometry, the type the window's text is set in, and a [`Frame`] that
//! rectangles, text and pictures are drawn into at given places, for what
//! is laid over the pages.

use rune::{El, ImageFilter, Pixels, TextStyle, Transition, el, text};

/// A colour, in sRGB with straight alpha, each part from 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const WHITE: Self = Self::from_rgb(1.0, 1.0, 1.0);

    pub const fn from_rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    pub const fn from_rgb8(red: u8, green: u8, blue: u8) -> Self {
        Self::from_rgba8(red, green, blue, 1.0)
    }

    pub const fn from_rgba8(red: u8, green: u8, blue: u8, a: f32) -> Self {
        Self {
            r: byte(red),
            g: byte(green),
            b: byte(blue),
            a,
        }
    }

    /// The colour as Rune takes it, in linear light.
    pub fn rune(self) -> rune::Color {
        rune::Color {
            r: rune::color::srgb_decode(self.r),
            g: rune::color::srgb_decode(self.g),
            b: rune::color::srgb_decode(self.b),
            a: self.a,
        }
    }
}

/// A byte of a colour as a part from 0 to 1.
const fn byte(value: u8) -> f32 {
    // Exact: a byte fits in an f32's mantissa.
    #[expect(clippy::as_conversions, reason = "From is not const")]
    let value = value as f32;
    value / 255.0
}

/// A place in the window, in logical pixels from its top-left.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// How far it is to another point.
    pub fn distance(self, other: Self) -> f32 {
        (self.x - other.x).hypot(self.y - other.y)
    }
}

/// A width and a height, in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Self = Self {
        width: 0.0,
        height: 0.0,
    };

    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

/// A rectangle: its top-left corner and its size.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rectangle {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rectangle {
    pub const fn position(self) -> Point {
        Point::new(self.x, self.y)
    }

    pub const fn size(self) -> Size {
        Size::new(self.width, self.height)
    }
}

/// The type text is set in: monospaced, at a size, bold or not.
pub const fn mono(size: f32, bold: bool) -> TextStyle {
    TextStyle::new(size, if bold { 700.0 } else { 400.0 }).mono()
}

/// A line of text in the monospaced type, in a colour, kept on one line.
pub fn label(content: impl Into<String>, size: f32, colour: Color) -> El {
    text(content)
        .font(mono(size, false))
        .color(colour.rune())
        .nowrap()
}

/// The same in bold.
pub fn bold(content: impl Into<String>, size: f32, colour: Color) -> El {
    text(content)
        .font(mono(size, true))
        .color(colour.rune())
        .nowrap()
}

/// How text drawn in a [`Frame`] lines up with the point it is drawn at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    /// The point is the middle of its left edge.
    Left,
    /// The point is its centre.
    Center,
}

/// A line of text to draw in a [`Frame`].
#[derive(Debug, Clone)]
pub struct Text {
    pub content: String,
    pub position: Point,
    pub color: Color,
    pub size: f32,
    pub bold: bool,
    pub align: Align,
}

/// Shapes, text and pictures drawn at places of their own over an area,
/// the last drawn on top: what an iced canvas drew, as Rune elements.
pub struct Frame {
    drawn: Vec<El>,
    size: Size,
}

impl Frame {
    /// An empty frame over an area of this size.
    pub const fn new(size: Size) -> Self {
        Self {
            drawn: Vec::new(),
            size,
        }
    }

    /// The height of the area.
    pub const fn height(&self) -> f32 {
        self.size.height
    }

    /// Fills a rectangle with a colour.
    pub fn fill_rectangle(&mut self, at: Point, size: Size, colour: Color) {
        if size.width <= 0.0 || size.height <= 0.0 || colour.a <= 0.0 {
            return;
        }
        self.push(
            el().absolute(at.x, at.y)
                .size(size.width, size.height)
                .fill(colour.rune()),
        );
    }

    /// Draws a line round the inside of a rectangle.
    pub fn stroke_rectangle(&mut self, at: Point, size: Size, width: f32, colour: Color) {
        if size.width <= 0.0 || size.height <= 0.0 {
            return;
        }
        self.push(
            el().absolute(at.x, at.y)
                .size(size.width, size.height)
                .border(width, colour.rune()),
        );
    }

    /// Draws a picture to fill a rectangle, pixel for pixel when it is
    /// the size of the rectangle on screen.
    pub fn draw_image(&mut self, area: Rectangle, pixels: &Pixels) {
        if area.width <= 0.0 || area.height <= 0.0 {
            return;
        }
        self.push(
            el().absolute(area.x, area.y)
                .size(area.width, area.height)
                .pixels(pixels)
                .image_filter(ImageFilter::Linear),
        );
    }

    /// Draws a line of text.
    pub fn fill_text(&mut self, drawn: Text) {
        let line = text(drawn.content)
            .font(mono(drawn.size, drawn.bold))
            .color(drawn.color.rune())
            .nowrap();
        // A box as high as a line, its middle on the point, holds it; a
        // centred one is as wide again on the other side.
        let height = drawn.size * 1.25;
        let top = drawn.position.y - height / 2.0;
        let placed = match drawn.align {
            Align::Left => el()
                .absolute(drawn.position.x, top)
                .h(height)
                .items_center()
                .child(line),
            Align::Center => {
                // Wide enough for any label drawn centred.
                let width = drawn.size * 16.0;
                el().absolute(drawn.position.x - width / 2.0, top)
                    .size(width, height)
                    .center()
                    .child(line)
            }
        };
        self.push(placed);
    }

    /// Adds what was drawn, there at once: nothing fades in or out.
    fn push(&mut self, drawn: El) {
        self.drawn.push(drawn.transition(Transition::none()));
    }

    /// What was drawn, in a layer filling its parent. It is not clipped:
    /// the page layers are slid, and the area for pages clips them.
    pub fn into_layer(self) -> El {
        el().w_full()
            .h_full()
            .absolute(0.0, 0.0)
            .children(self.drawn)
    }
}
