//! Wallpaper generation: gradient/geometric compositions with the wordmark (spec 013).
//!
//! Each theme gets a deterministic wallpaper generated from its palette. The
//! generator is pure: the same theme always produces the same bytes, so the
//! cache is content-addressed by the theme's hash.
//!
//! The composition is a gradient background with the LazyOS wordmark centered.
//! The wordmark uses the theme's accent colour for the 'z' and the orange
//! accent for the cursor (following the brand rules).

use crate::Rgb;
use image::{ImageBuffer, Rgba, RgbaImage};
use std::fs;

/// The wallpaper cache directory name.
pub const WALLPAPER_DIR: &str = ".cache/lazyos/wallpapers";

/// The wordmark text (the LazyOS mark).
const WORDMARK: &str = "lazyOS_";

/// Default wallpaper dimensions (panel resolution).
pub const DEFAULT_WIDTH: u32 = 1366;
pub const DEFAULT_HEIGHT: u32 = 912;

/// A generated wallpaper.
#[derive(Clone, Debug)]
pub struct Wallpaper {
    /// The theme slug this was generated for.
    pub slug: String,
    /// The image data (PNG).
    pub data: Vec<u8>,
    /// The width in pixels.
    pub width: u32,
    /// The height in pixels.
    pub height: u32,
}

/// Generates a wallpaper for a theme.
///
/// The wallpaper is a gradient from the theme's background to a slightly
/// lighter version, with the wordmark centered in the accent colour.
/// The 'z' of the mark uses the accent colour, the '_' cursor uses the
/// orange accent.
pub fn generate_wallpaper(
    theme: &crate::Theme,
    slug: &str,
    width: u32,
    height: u32,
) -> Result<Wallpaper, String> {
    let palette = theme.palette();
    let roles = theme.roles();

    // Background gradient: from bg to a slightly lighter version
    let bg = palette.bg;
    let bg_lighter = mix(bg, Rgb::new(255, 255, 255), 0.08);

    // Wordmark colours
    let z_colour = roles.accent.0; // accent for the 'z'
    let cursor_colour = roles.warn.0; // warn (orange) for the '_'

    let mut img: RgbaImage = ImageBuffer::new(width, height);

    // Draw gradient background
    for y in 0..height {
        let colour = mix_rgb(bg, bg_lighter, y as f32 / (height - 1) as f32);
        for x in 0..width {
            img.put_pixel(x, y, Rgba([colour.r, colour.g, colour.b, 255]));
        }
    }

    // Draw the wordmark centered
    draw_wordmark(&mut img, width, height, &roles, z_colour, cursor_colour)?;

    // Encode as PNG
    let mut png_data = Vec::new();
    {
        let mut cursor = std::io::Cursor::new(&mut png_data);
        img.write_to(&mut cursor, image::ImageFormat::Png)
            .map_err(|e| format!("PNG encoding failed: {e}"))?;
    }

    Ok(Wallpaper {
        slug: slug.to_string(),
        data: png_data,
        width,
        height,
    })
}

/// Generates a 128px thumbnail for the theme preview.
pub fn generate_thumbnail(theme: &crate::Theme, slug: &str) -> Result<Wallpaper, String> {
    generate_wallpaper(theme, slug, 128, 128)
}

/// Draws the wordmark centered on the image.
fn draw_wordmark(
    img: &mut RgbaImage,
    width: u32,
    height: u32,
    roles: &crate::Roles,
    z_colour: Rgb,
    cursor_colour: Rgb,
) -> Result<(), String> {
    // Use a simple bitmap font approach for the wordmark
    // The wordmark is "lazyOS_" with 'z' in accent and '_' in orange
    // We'll draw it as large block characters

    let font_size = (width.min(height) / 16).max(8);
    let char_width = font_size * 4 / 5;
    let char_height = font_size;
    let wordmark_width = WORDMARK.len() as u32 * char_width;
    let start_x = (width - wordmark_width) / 2;
    let start_y = (height - char_height) / 2;

    // Draw each character
    for (i, ch) in WORDMARK.chars().enumerate() {
        let x = start_x + i as u32 * char_width;
        let colour = if ch == 'z' {
            z_colour
        } else if ch == '_' {
            cursor_colour
        } else {
            roles.text.0 // fallback to text colour
        };

        draw_char(img, x, start_y, ch, char_width, char_height, colour)?;
    }

    Ok(())
}

/// Draws a single character as a large block.
fn draw_char(
    img: &mut RgbaImage,
    x: u32,
    y: u32,
    ch: char,
    width: u32,
    height: u32,
    colour: Rgb,
) -> Result<(), String> {
    // Simple 5x7 bitmap font for basic characters
    let bitmap = char_bitmap(ch).ok_or_else(|| format!("no bitmap for character: {ch}"))?;

    for (by, row) in bitmap.iter().enumerate() {
        for (bx, &pixel) in row.iter().enumerate() {
            if pixel {
                let px = x + (bx as u32 * width / 5);
                let py = y + (by as u32 * height / 7);
                if px < img.width() && py < img.height() {
                    let scale_x = width / 5;
                    let scale_y = height / 7;
                    for sx in 0..scale_x {
                        for sy in 0..scale_y {
                            let px2 = px + sx;
                            let py2 = py + sy;
                            if px2 < img.width() && py2 < img.height() {
                                img.put_pixel(px2, py2, Rgba([colour.r, colour.g, colour.b, 255]));
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// 5x7 bitmap fonts for the wordmark characters.
fn char_bitmap(ch: char) -> Option<[[bool; 5]; 7]> {
    match ch {
        'l' => Some([
            [false, false, false, false, false],
            [false, false, false, false, false],
            [false, false, false, false, false],
            [true, false, false, false, false],
            [true, false, false, false, false],
            [true, false, false, false, false],
            [true, true, true, true, true],
        ]),
        'a' => Some([
            [false, false, false, false, false],
            [false, false, false, false, false],
            [true, true, true, true, true],
            [true, false, false, true, true],
            [true, true, true, true, true],
            [true, false, false, true, true],
            [true, false, false, true, true],
        ]),
        'z' => Some([
            [true, true, true, true, true],
            [false, false, false, true, false],
            [false, false, true, false, false],
            [false, true, false, false, false],
            [true, false, false, false, false],
            [false, false, false, false, false],
            [true, true, true, true, true],
        ]),
        'y' => Some([
            [true, false, false, true, false],
            [true, false, false, true, false],
            [true, false, false, true, false],
            [false, true, true, false, false],
            [false, false, true, false, false],
            [false, false, true, false, false],
            [false, false, true, false, false],
        ]),
        'O' => Some([
            [true, true, true, true, true],
            [true, false, false, true, true],
            [true, false, false, true, true],
            [true, false, false, true, true],
            [true, false, false, true, true],
            [true, false, false, true, true],
            [true, true, true, true, true],
        ]),
        'S' => Some([
            [true, true, true, true, true],
            [true, false, false, false, false],
            [true, false, false, false, false],
            [true, true, true, true, true],
            [false, false, false, false, true],
            [false, false, false, false, true],
            [true, true, true, true, true],
        ]),
        '_' => Some([
            [false, false, false, false, false],
            [false, false, false, false, false],
            [false, false, false, false, false],
            [false, false, false, false, false],
            [false, false, false, false, false],
            [false, false, false, false, false],
            [true, true, true, true, true],
        ]),
        _ => None,
    }
}

/// Mixes two RGB colours.
fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    Rgb::new(
        (a.r as f32 + (b.r as f32 - a.r as f32) * t).round() as u8,
        (a.g as f32 + (b.g as f32 - a.g as f32) * t).round() as u8,
        (a.b as f32 + (b.b as f32 - a.b as f32) * t).round() as u8,
    )
}

/// Mixes two RGB colours (alternative signature).
fn mix_rgb(a: Rgb, b: Rgb, t: f32) -> Rgb {
    mix(a, b, t)
}

/// Gets the cache directory for wallpapers.
pub fn cache_dir() -> Result<std::path::PathBuf, String> {
    let home = dirs::home_dir().ok_or("no home directory")?;
    let dir = home.join(WALLPAPER_DIR);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Gets the cached wallpaper path for a theme.
pub fn cached_path(slug: &str, width: u32, height: u32) -> Result<std::path::PathBuf, String> {
    let dir = cache_dir()?;
    let filename = format!("{}-{}x{}.png", slug, width, height);
    Ok(dir.join(filename))
}

/// Gets or generates the wallpaper for a theme.
pub fn get_or_generate(
    theme: &crate::Theme,
    slug: &str,
    width: u32,
    height: u32,
) -> Result<Wallpaper, String> {
    let path = cached_path(slug, width, height)?;
    if path.exists() {
        let data = fs::read(&path).map_err(|e| e.to_string())?;
        return Ok(Wallpaper {
            slug: slug.to_string(),
            data,
            width,
            height,
        });
    }
    let wallpaper = generate_wallpaper(theme, slug, width, height)?;
    fs::write(&path, &wallpaper.data).map_err(|e| e.to_string())?;
    Ok(wallpaper)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solarized_wallpaper_generates() {
        let theme = crate::Theme::parse(crate::builtin::SOLARIZED).unwrap();
        let wallpaper = generate_wallpaper(&theme, "solarized", 100, 100).unwrap();
        assert_eq!(wallpaper.width, 100);
        assert_eq!(wallpaper.height, 100);
        assert!(!wallpaper.data.is_empty());
    }

    #[test]
    fn all_builtins_generate_wallpapers() {
        for (slug, text) in crate::builtin::ALL {
            let theme = crate::Theme::parse(text).unwrap();
            let wallpaper = generate_wallpaper(&theme, slug, 50, 50).unwrap();
            assert_eq!(wallpaper.width, 50);
            assert_eq!(wallpaper.height, 50);
            assert!(!wallpaper.data.is_empty());
        }
    }

    #[test]
    fn thumbnail_is_square() {
        let thumb = generate_thumbnail(
            &crate::Theme::parse(crate::builtin::SOLARIZED).unwrap(),
            "solarized",
        )
        .unwrap();
        assert_eq!(thumb.width, 128);
        assert_eq!(thumb.height, 128);
    }
}
