//! Turn an RGBA image into truecolor ANSI art using Unicode half blocks.
//!
//! Every terminal cell shows two vertically stacked pixels: `▀` coloured with
//! the top pixel as foreground and the bottom pixel as background.

use anyhow::{Context, Result};
use image::RgbaImage;

/// Pixels with alpha below this are treated as transparent.
const ALPHA_CUTOFF: u8 = 128;
const UPPER: char = '▀';
const LOWER: char = '▄';
const RESET: &str = "\x1b[0m";

/// Rendered sprite: one string per terminal row, plus the width in cells.
#[derive(Debug)]
pub struct Art {
    pub lines: Vec<String>,
    pub width: usize,
}

pub fn decode(png: &[u8]) -> Result<RgbaImage> {
    let img = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .context("failed to decode embedded sprite PNG")?;
    Ok(img.to_rgba8())
}

type Rgb = [u8; 3];

fn pixel(img: &RgbaImage, x: u32, y: u32) -> Option<Rgb> {
    if y >= img.height() {
        return None;
    }
    let [r, g, b, a] = img.get_pixel(x, y).0;
    (a >= ALPHA_CUTOFF).then_some([r, g, b])
}

/// SGR sequence + glyph for one cell, or `None` for a fully transparent cell.
fn cell(top: Option<Rgb>, bottom: Option<Rgb>) -> Option<(String, char)> {
    let fg = |[r, g, b]: Rgb| format!("38;2;{r};{g};{b}");
    let bg = |[r, g, b]: Rgb| format!("48;2;{r};{g};{b}");
    match (top, bottom) {
        (Some(t), Some(b)) => Some((format!("\x1b[{};{}m", fg(t), bg(b)), UPPER)),
        (Some(t), None) => Some((format!("\x1b[49;{}m", fg(t)), UPPER)),
        (None, Some(b)) => Some((format!("\x1b[49;{}m", fg(b)), LOWER)),
        (None, None) => None,
    }
}

pub fn render(img: &RgbaImage) -> Art {
    let mut lines = Vec::with_capacity(img.height().div_ceil(2) as usize);
    for y in (0..img.height()).step_by(2) {
        let mut line = String::new();
        let mut sgr = String::new(); // currently active SGR, "" = reset
        let mut pending_spaces = 0usize;
        for x in 0..img.width() {
            match cell(pixel(img, x, y), pixel(img, x, y + 1)) {
                None => pending_spaces += 1,
                Some((code, glyph)) => {
                    if pending_spaces > 0 {
                        if !sgr.is_empty() {
                            line.push_str(RESET);
                            sgr.clear();
                        }
                        line.extend(std::iter::repeat_n(' ', pending_spaces));
                        pending_spaces = 0;
                    }
                    if code != sgr {
                        line.push_str(&code);
                        sgr = code;
                    }
                    line.push(glyph);
                }
            }
        }
        // Trailing transparent cells are dropped entirely.
        if !sgr.is_empty() {
            line.push_str(RESET);
        }
        lines.push(line);
    }
    Art {
        lines,
        width: img.width() as usize,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    const R: Rgba<u8> = Rgba([255, 0, 0, 255]);
    const B: Rgba<u8> = Rgba([0, 0, 255, 255]);
    const T: Rgba<u8> = Rgba([9, 9, 9, 0]);

    /// Build an image from rows of pixels.
    fn img(rows: &[&[Rgba<u8>]]) -> RgbaImage {
        let mut im = RgbaImage::new(rows[0].len() as u32, rows.len() as u32);
        for (y, row) in rows.iter().enumerate() {
            for (x, p) in row.iter().enumerate() {
                im.put_pixel(x as u32, y as u32, *p);
            }
        }
        im
    }

    #[test]
    fn both_opaque_uses_fg_and_bg() {
        let art = render(&img(&[&[R], &[B]]));
        assert_eq!(art.lines, vec!["\x1b[38;2;255;0;0;48;2;0;0;255m▀\x1b[0m"]);
        assert_eq!(art.width, 1);
    }

    #[test]
    fn only_top_opaque() {
        let art = render(&img(&[&[R], &[T]]));
        assert_eq!(art.lines, vec!["\x1b[49;38;2;255;0;0m▀\x1b[0m"]);
    }

    #[test]
    fn only_bottom_opaque() {
        let art = render(&img(&[&[T], &[B]]));
        assert_eq!(art.lines, vec!["\x1b[49;38;2;0;0;255m▄\x1b[0m"]);
    }

    #[test]
    fn transparent_cells_are_spaces_and_trailing_ones_trimmed() {
        let art = render(&img(&[&[T, R, T, T], &[T, R, T, T]]));
        assert_eq!(art.lines, vec![" \x1b[38;2;255;0;0;48;2;255;0;0m▀\x1b[0m"]);
        assert_eq!(art.width, 4);
    }

    #[test]
    fn gap_between_colours_resets_before_spaces() {
        let art = render(&img(&[&[R, T, R]]));
        let red = "\x1b[49;38;2;255;0;0m▀";
        assert_eq!(art.lines, vec![format!("{red}\x1b[0m {red}\x1b[0m")]);
    }

    #[test]
    fn repeated_colour_is_not_re_emitted() {
        let art = render(&img(&[&[R, R]]));
        assert_eq!(art.lines, vec!["\x1b[49;38;2;255;0;0m▀▀\x1b[0m"]);
    }

    #[test]
    fn odd_height_last_row_has_transparent_bottom() {
        let art = render(&img(&[&[R], &[R], &[B]]));
        assert_eq!(art.lines.len(), 2);
        assert_eq!(art.lines[1], "\x1b[49;38;2;0;0;255m▀\x1b[0m");
    }
}
