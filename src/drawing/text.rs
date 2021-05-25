use crate::definitions::{Clamp, Image};
use crate::drawing::Canvas;
use conv::ValueInto;
use image::{GenericImage, ImageBuffer, Pixel};
use std::f32;
use std::i32;

use crate::pixelops::weighted_sum;
use rusttype::{point, Font, PositionedGlyph, Rect, Scale, VMetrics};
use std::cmp::max;

use crate::rect::Rect as IpRect;

fn layout_glyphs(
    scale: Scale,
    font: &Font,
    text: &str,
    mut f: impl FnMut(PositionedGlyph, Rect<i32>),
) -> (i32, i32) {
    let v_metrics = font.v_metrics(scale);

    let (mut w, mut h) = (0, 0);

    for g in font.layout(text, scale, point(0.0, v_metrics.ascent)) {
        if let Some(bb) = g.pixel_bounding_box() {
            w = max(w, bb.max.x);
            h = max(h, bb.max.y);
            f(g, bb);
        }
    }

    (w, h)
}

/// Get the width and height of the given text, rendered with the given font and scale. Note that this function *does not* support newlines, you must do this manually.
pub fn text_size(scale: Scale, font: &Font, text: &str) -> (i32, i32) {
    layout_glyphs(scale, font, text, |_, _| {})
}

/// Draws colored text on an image in place. `scale` is augmented font scaling on both the x and y axis (in pixels). Note that this function *does not* support newlines, you must do this manually.
pub fn draw_text_mut<'a, C>(
    canvas: &'a mut C,
    color: C::Pixel,
    x: i32,
    y: i32,
    scale: Scale,
    font: &'a Font<'a>,
    text: &'a str,
) where
    C: Canvas,
    <C::Pixel as Pixel>::Subpixel: ValueInto<f32> + Clamp<f32>,
{
    let image_width = canvas.width() as i32;
    let image_height = canvas.height() as i32;

    layout_glyphs(scale, font, text, |g, bb| {
        g.draw(|gx, gy, gv| {
            let gx = gx as i32 + bb.min.x;
            let gy = gy as i32 + bb.min.y;

            let image_x = gx + x;
            let image_y = gy + y;

            if (0..image_width).contains(&image_x) && (0..image_height).contains(&image_y) {
                let pixel = canvas.get_pixel(image_x as u32, image_y as u32);
                let weighted_color = weighted_sum(pixel, color, 1.0 - gv, gv);
                canvas.draw_pixel(image_x as u32, image_y as u32, weighted_color);
            }
        })
    });
}

/// Draws colored text on an image in place. `scale` is augmented font scaling on both the x and y axis (in pixels). Note that this function *does not* support newlines, you must do this manually.
pub fn draw_text<'a, I>(
    image: &'a mut I,
    color: I::Pixel,
    x: i32,
    y: i32,
    scale: Scale,
    font: &'a Font<'a>,
    text: &'a str,
) -> Image<I::Pixel>
where
    I: GenericImage,
    <I::Pixel as Pixel>::Subpixel: ValueInto<f32> + Clamp<f32>,
    I::Pixel: 'static,
{
    let mut out = ImageBuffer::new(image.width(), image.height());
    out.copy_from(image, 0, 0).unwrap();
    draw_text_mut(&mut out, color, x, y, scale, font, text);
    out
}

/// This helper function is used to find the top (or) left corner of a text.
/// It takes handles only one dimension per call to make it more reusable.
/// It takes a `rectangle_size` which is the length (width or height) of the surrounding rectangle
/// and a `content_size` which is the length (width or height) of the text inside.
/// The `relative_position` is then used to divide the space
/// which is not used up by the content (=`content_size`) to determine a padding
/// inside the rectangle `rectangle_size`.
fn calculate_center(
    rectangle_size: u32,
    content_size: u32,
    relative_position: &EdgePosition,
) -> u32 {
    ((rectangle_size - content_size) as f32 * relative_position.0 / 100.0) as u32
}

fn find_text_area_coordinates(
    position: &Position,
    rectangle: &IpRect,
    width: u32,
    height: u32,
) -> (u32, u32) {
    match position {
        Position::HorizontalCenter(edge_position) => (
            rectangle.left() as u32 + calculate_center(rectangle.width(), width, edge_position),
            rectangle.top() as u32 + ((rectangle.height() - height) / 2) as u32,
        ),
        Position::HorizontalBottom(edge_position) => (
            rectangle.left() as u32 + calculate_center(rectangle.width(), width, edge_position),
            rectangle.bottom() as u32 - height,
        ),
        Position::HorizontalTop(edge_position) => (
            rectangle.left() as u32 + calculate_center(rectangle.width(), width, edge_position),
            rectangle.top() as u32,
        ),
        Position::VerticalCenter(edge_position) => (
            rectangle.left() as u32 + ((rectangle.width() - width) / 2) as u32,
            rectangle.top() as u32 + calculate_center(rectangle.height(), height, edge_position),
        ),
        Position::VerticalRight(edge_position) => (
            rectangle.right() as u32 - width,
            rectangle.top() as u32 + calculate_center(rectangle.height(), height, edge_position),
        ),
        Position::VerticalLeft(edge_position) => (
            rectangle.left() as u32,
            rectangle.top() as u32 + calculate_center(rectangle.height(), height, edge_position),
        ),
        Position::Any(horizontal_edge, vertical_edge) => (
            rectangle.left() as u32 + calculate_center(rectangle.width(), width, horizontal_edge),
            rectangle.top() as u32 + calculate_center(rectangle.height(), height, vertical_edge),
        ),
    }
}

/// An arrangement of glyphs which can be drawn onto an image.
/// This string also knows about its size, according to scaling and font properties.
pub struct GlyphString<'a> {
    glyphs: Vec<PositionedGlyph<'a>>,
}

impl<'a> GlyphString<'a> {
    /// Construct a `GlyphString` from `text` scaled by `scale` using the Font `font`.
    pub fn new(scale: Scale, font: &'a Font<'a>, text: &'a str) -> Self {
        let v_metrics = font.v_metrics(scale);
        let offset = point(0.0, v_metrics.ascent);

        let glyphs = font.layout(text, scale, offset).collect();

        Self { glyphs }
    }

    /// Find out how much horizontal space this `GlyphString` needs when drawn.
    // https://docs.rs/artano/0.2.8/src/artano/annotation.rs.html#270-277
    pub fn width(&self) -> u32 {
        2 + self
            .glyphs
            .iter()
            .map(|glyph| glyph.unpositioned().h_metrics().advance_width)
            .sum::<f32>() as u32
    }

    /// Find out how much vertical space this `GlyphString` needs when drawn.
    pub fn height(&self) -> u32 {
        self.glyphs
            .first()
            .map(|glyph| {
                let scale = glyph.scale();
                let font = glyph.font();

                let VMetrics {
                    ascent, descent, ..
                } = font.v_metrics(scale);
                ((ascent - descent) as f32 * 1.1) as u32
            })
            .unwrap_or(0)
    }

    /// Draws this `GlyphString` onto the `image` at the given coordinates `x` and `y`.
    /// For an out-of-place version use [`GlyphString::draw`](#method.draw).
    /// Behaves identical to [`draw_text_mut`](fn.draw_text_mut)
    pub fn draw_mut<I>(&self, image: &mut I, color: I::Pixel, x: u32, y: u32)
    where
        I: GenericImage,
        <I::Pixel as Pixel>::Subpixel: ValueInto<f32> + Clamp<f32>,
    {
        for g in self.glyphs.iter() {
            if let Some(bb) = g.pixel_bounding_box() {
                g.draw(|gx, gy, gv| {
                    let gx = gx as i32 + bb.min.x;
                    let gy = gy as i32 + bb.min.y;

                    let image_x = gx + x as i32;
                    let image_y = gy + y as i32;

                    let image_width = image.width() as i32;
                    let image_height = image.height() as i32;

                    if image_x >= 0
                        && image_x < image_width
                        && image_y >= 0
                        && image_y < image_height
                    {
                        let pixel = image.get_pixel(image_x as u32, image_y as u32);
                        let weighted_color = weighted_sum(pixel, color, 1.0 - gv, gv);
                        image.put_pixel(image_x as u32, image_y as u32, weighted_color);
                    }
                })
            }
        }
    }

    /// Draws this `GlyphString` onto a copy of `image` at the given coordinates `x` and `y` and return the copy.
    /// For an in-place version use [`GlyphString::draw_mut`](#method.draw_mut).
    /// Behaves identical to [`draw_text`](fn.draw_text.html).
    pub fn draw<I>(&self, image: &I, color: I::Pixel, x: u32, y: u32) -> Image<I::Pixel>
    where
        I: GenericImage,
        <I::Pixel as Pixel>::Subpixel: ValueInto<f32> + Clamp<f32>,
        I::Pixel: 'static,
    {
        let mut out = ImageBuffer::new(image.width(), image.height());
        out.copy_from(image, 0, 0);
        self.draw_mut(&mut out, color, x, y);
        out
    }

    /// Draws this `GlyphString` onto the `image` inside a `rectangle` at a `position`.
    /// For an out-of-place version use [`GlyphString::draw_positioned`](#method.draw_positioned).
    ///
    /// ##Example: drawing some text to the center and top-left corner of an image
    /// ```no_run
    /// use imageproc::drawing::{EdgePosition, GlyphString, Position};
    /// use imageproc::rect::Rect;
    /// use image::{ImageBuffer, Rgb};
    /// use rusttype::Scale;
    ///
    /// let text = "Hello World";
    /// let scale = Scale::uniform(12.0);
    /// let font = unimplemented!(); // load your font here
    /// let mut image = ImageBuffer::from_pixel(100, 100, Rgb([0u8, 0u8, 0u8]));
    /// let rect = Rect::at(0, 0).of_size(image.width(), image.height());
    ///
    /// let position = Position::HorizontalCenter(50.0.into());
    /// GlyphString::new(scale, &font, &text)
    ///     .draw_positioned_mut(&mut image, Rgb([0u8, 0u8, 255u8]), &position, &rect);
    ///
    /// let position = Position::HorizontalTop(0.0.into());
    /// GlyphString::new(scale, &font, &text)
    ///     .draw_positioned_mut(&mut image, Rgb([0u8, 255u8, 0u8]), &position, &rect);
    /// ```
    ///
    /// What we are doing here:
    ///
    /// 1. Find an x, y such that
    ///     1.1 (x, y) lies on the edge of `position` and inside of `rectangle`
    ///     1.2 (x, y) should be the top-left corner of the text area (the space consumed by the glyphs)
    ///     1.3 the text area should divide the rectangle according to the given `position`
    ///     1.4 there should be an equal padding in all directions (from the edges of the text area to the edges of rectangle)
    /// 2. Draw the text to x, y
    ///
    /// <pre>
    ///     +-----------------------------------------+  = `image` bounds
    ///     |                                         |
    ///     |     +-----------------------------+     |  = `rectangle` bounds
    ///     |     |                             |     |
    ///     |     |     +-----------------+     |     |  = text area <- y
    ///     |     |     |Some example text|     |     |
    ///     |     |     +-----------------+     |     |
    ///     |     |                             |     |
    ///     |     +-----------------------------+     |
    ///     |                                         |
    ///     +-----------------------------------------+
    ///
    ///                 ^
    ///                 |
    ///                 x
    /// </pre>
    pub fn draw_positioned_mut<'b, I>(
        &self,
        image: &'b mut I,
        color: I::Pixel,
        position: &Position,
        rectangle: &IpRect,
    ) where
        I: GenericImage,
        <I::Pixel as Pixel>::Subpixel: ValueInto<f32> + Clamp<f32>,
    {
        let width = self.width();
        let height = self.height();
        let (x, y) = find_text_area_coordinates(position, rectangle, width, height);

        self.draw_mut(image, color, x, y)
    }

    /// Draws this `GlyphString` onto a copy of `image` at the given coordinates `x` and `y` and return the copy.
    /// For an in-place version use [`GlyphString::draw_positioned_mut`](#method.draw_positioned_mut).
    pub fn draw_positioned<I>(
        &self,
        image: &I,
        color: I::Pixel,
        position: &Position,
        rectangle: &IpRect,
    ) -> Image<I::Pixel>
    where
        I: GenericImage,
        <I::Pixel as Pixel>::Subpixel: ValueInto<f32> + Clamp<f32>,
        I::Pixel: 'static,
    {
        let mut out = ImageBuffer::new(image.width(), image.height());
        out.copy_from(image, 0, 0);
        self.draw_positioned_mut(&mut out, color, position, rectangle);
        out
    }
}

/// The relative position of a point on an edge.
///
/// +--------------------------------+
/// 0%       something else          100%
///
/// Edge in this context does not exclusively refer to the edges
/// which are boundaries of a rectangle but rather all lines which are parallel to two opposing boundaries.
///
/// <pre>
///    +---------------+  <- "typical" horizontal edge
///    |               |
///    +---------------+  <- "typical" horizontal edge
///    ^               ^
///    |               |
/// "typical" vertical edges
///
///   +---------------+
///   |               |
///   |---------------| <- parallel to horizontal edges (= also an edge)
///   |               |
///   +---------------+
/// </pre>
pub struct EdgePosition(pub f32);

impl EdgePosition {
    /// The left-most point of a horizontal edge.
    /// Equivalent to `EdgePosition(0.into())`.
    pub fn left() -> Self {
        0.into()
    }
    /// The top-most point of a vertical edge.
    /// Equivalent to `EdgePosition(0.into())`.
    pub fn top() -> Self {
        0.into()
    }

    /// The center point of a horizontal or vertical edge.
    /// Equivalent to `EdgePosition(50.into())`.
    pub fn center() -> Self {
        50.into()
    }

    /// The right-most point of a horizontal edge.
    /// Equivalent to `EdgePosition(100.into())`.
    pub fn right() -> Self {
        100.into()
    }
    /// The bottom-most point of a vertical edge.
    /// Equivalent to `EdgePosition(100.into())`.
    pub fn bottom() -> Self {
        100.into()
    }
}

impl From<f32> for EdgePosition {
    fn from(from: f32) -> Self {
        Self(from)
    }
}

impl From<u32> for EdgePosition {
    fn from(from: u32) -> Self {
        Self(from as _)
    }
}

/// A position inside a rectangle
pub enum Position {
    /// top edge
    /// <pre>
    /// +---------------+  <- this
    /// |               |
    /// +---------------+
    /// </pre>
    HorizontalTop(EdgePosition),
    /// horizontal center edge (horizontally centered between top and bottom)
    /// <pre>
    /// +---------------+
    /// |               |
    /// |---------------| <- this
    /// |               |
    /// +---------------+
    /// </pre>
    HorizontalCenter(EdgePosition),
    /// bottom edge
    /// <pre>
    /// +---------------+
    /// |               |
    /// +---------------+  <- this
    /// </pre>
    HorizontalBottom(EdgePosition),
    /// left edge
    /// <pre>
    /// +---------------+
    /// |               |
    /// +---------------+
    /// ^
    /// |
    /// this
    /// </pre>
    VerticalLeft(EdgePosition),
    /// Vertical center edge (vertically centered between left and right)
    /// <pre>
    /// +---------------+
    /// |       |       |
    /// |       |       |
    /// |       |       |
    /// +---------------+
    ///         ^
    ///         |
    ///         this
    /// </pre>
    VerticalCenter(EdgePosition),
    /// left edge
    /// <pre>
    /// +---------------+
    /// |               |
    /// +---------------+
    ///                 ^
    ///                 |
    ///                 this
    /// </pre>
    VerticalRight(EdgePosition),
    /// fine-grained control over horizontal and vertical edges
    Any(EdgePosition, EdgePosition),
}

/// An arrangement of PositionedGlyphStrings
pub struct GlyphStrings<'a>(pub &'a [&'a GlyphString<'a>]);

impl<'a> GlyphStrings<'a> {
    /// create
    #[inline]
    pub fn new(glyph_strings: &'a [&GlyphString<'a>]) -> Self {
        Self(glyph_strings)
    }

    /// draw text
    #[inline]
    pub fn draw_positioned_mut<'b, I>(
        &self,
        image: &'b mut I,
        colors: &[I::Pixel],
        position: &Position,
        rectangle: &IpRect,
    ) where
        I: GenericImage,
        <I::Pixel as Pixel>::Subpixel: ValueInto<f32> + Clamp<f32>,
    {
        let width = self.width();
        let height = self.height();
        let (mut x, y) = find_text_area_coordinates(position, rectangle, width, height);

        for (string, &color) in self.0.iter().zip(colors.iter()) {
            string.draw_mut(image, color, x as _, y as _);
            x += string.width()
        }
    }

    /// Find out how much vertical space this `GlyphStrings` needs when drawn.
    pub fn height(&self) -> u32 {
        self.0
            .iter()
            .map(|string| string.height())
            .max()
            .unwrap_or(0)
    }

    /// Find out how much horizontal space this `GlyphStrings` needs when drawn.
    pub fn width(&self) -> u32 {
        self.0.iter().map(|string| string.width()).sum::<u32>()
    }
}
