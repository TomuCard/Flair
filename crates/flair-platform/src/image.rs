//! Small image helpers for icons: decode PNG and SVG, scale, encode PNG
//! (icon cache). Pixels are always straight-alpha RGBA.

use std::io::Cursor;

use resvg::tiny_skia::{FilterQuality, Pixmap, PixmapPaint, PremultipliedColorU8, Transform};
use resvg::usvg;

/// Straight-alpha RGBA pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconRgba {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Largest image accepted when decoding (icons are small; anything bigger is
/// suspicious or wasteful).
const MAX_SIDE: u32 = 1024;

/// Renders an SVG to a `size`×`size` square, centered, aspect ratio kept.
pub fn render_svg(data: &[u8], size: u32) -> Option<IconRgba> {
    let tree = usvg::Tree::from_data(data, &usvg::Options::default()).ok()?;
    let mut pixmap = Pixmap::new(size, size)?;
    let (w, h) = (tree.size().width(), tree.size().height());
    #[allow(clippy::cast_precision_loss)]
    let scale = size as f32 / w.max(h);
    #[allow(clippy::cast_precision_loss)]
    let (dx, dy) = (
        (size as f32 - w * scale) / 2.0,
        (size as f32 - h * scale) / 2.0,
    );
    resvg::render(
        &tree,
        Transform::from_row(scale, 0.0, 0.0, scale, dx, dy),
        &mut pixmap.as_mut(),
    );
    Some(from_pixmap(&pixmap))
}

/// Decodes a PNG (any color type) to RGBA.
pub fn decode_png(data: &[u8]) -> Option<IconRgba> {
    let mut decoder = png::Decoder::new(Cursor::new(data));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let (width, height) = (reader.info().width, reader.info().height);
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return None;
    }
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let pixels = &buf[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => pixels.to_vec(),
        png::ColorType::Rgb => pixels
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => pixels
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => pixels.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    Some(IconRgba {
        width,
        height,
        rgba,
    })
}

/// Encodes RGBA pixels as a PNG.
pub fn encode_png(icon: &IconRgba) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, icon.width, icon.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(&icon.rgba).ok()?;
    writer.finish().ok()?;
    Some(out)
}

/// Scales an image to fit a `size`×`size` square (centered). Images already
/// at that size are returned as is.
pub fn fit(icon: IconRgba, size: u32) -> Option<IconRgba> {
    if icon.width == size && icon.height == size {
        return Some(icon);
    }
    let mut src = Pixmap::new(icon.width, icon.height)?;
    for (dst, p) in src
        .pixels_mut()
        .iter_mut()
        .zip(icon.rgba.as_chunks::<4>().0.iter())
    {
        *dst = resvg::tiny_skia::ColorU8::from_rgba(p[0], p[1], p[2], p[3]).premultiply();
    }
    let mut out = Pixmap::new(size, size)?;
    #[allow(clippy::cast_precision_loss)]
    let scale = size as f32 / icon.width.max(icon.height) as f32;
    #[allow(clippy::cast_precision_loss)]
    let (dx, dy) = (
        (size as f32 - icon.width as f32 * scale) / 2.0,
        (size as f32 - icon.height as f32 * scale) / 2.0,
    );
    let paint = PixmapPaint {
        quality: FilterQuality::Bicubic,
        ..PixmapPaint::default()
    };
    out.draw_pixmap(
        0,
        0,
        src.as_ref(),
        &paint,
        Transform::from_row(scale, 0.0, 0.0, scale, dx, dy),
        None,
    );
    Some(from_pixmap(&out))
}

/// tiny-skia stores premultiplied alpha; Flair works with straight alpha.
fn from_pixmap(pixmap: &Pixmap) -> IconRgba {
    let rgba = pixmap
        .pixels()
        .iter()
        .flat_map(|p: &PremultipliedColorU8| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    IconRgba {
        width: pixmap.width(),
        height: pixmap.height(),
        rgba,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SQUARE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 20"><rect width="10" height="20" fill="#ff0000"/></svg>"##;

    #[test]
    fn svg_is_centered_in_a_square() {
        let icon = render_svg(SQUARE.as_bytes(), 20).expect("svg");
        assert_eq!((icon.width, icon.height), (20, 20));
        let px = |x: usize, y: usize| &icon.rgba[(y * 20 + x) * 4..][..4];
        assert_eq!(px(10, 10), [255, 0, 0, 255]);
        // Narrow drawing: transparent margins left and right.
        assert_eq!(px(1, 10)[3], 0);
        assert!(render_svg(b"not svg", 20).is_none());
    }

    #[test]
    fn png_round_trip_and_fit() {
        let icon = IconRgba {
            width: 4,
            height: 2,
            rgba: [[0, 0, 255, 255]; 8].concat(),
        };
        let bytes = encode_png(&icon).expect("encode");
        assert_eq!(decode_png(&bytes), Some(icon.clone()));
        let fitted = fit(icon, 8).expect("fit");
        assert_eq!((fitted.width, fitted.height), (8, 8));
        // Centered vertically: top row transparent, middle blue.
        assert_eq!(fitted.rgba[3], 0);
        assert_eq!(&fitted.rgba[(4 * 8 + 4) * 4..][..4], [0, 0, 255, 255]);
        assert!(decode_png(b"nope").is_none());
    }
}
