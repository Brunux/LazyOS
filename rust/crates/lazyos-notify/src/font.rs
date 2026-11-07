use fontdue::{Font, FontSettings};
use tiny_skia::{Color, PixmapMut};

pub struct FontRenderer {
    regular: Option<Font>,
    bold: Option<Font>,
    symbols: Option<Font>,
}

impl FontRenderer {
    pub fn new() -> Self {
        let regular = Self::load_font(&[
            "/usr/share/fonts/truetype/jetbrains-mono/JetBrainsMono-Regular.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            "/usr/share/fonts/truetype/adwaita/AdwaitaSans-Regular.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        ]);

        let bold = Self::load_font(&[
            "/usr/share/fonts/truetype/jetbrains-mono/JetBrainsMono-Bold.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
            "/usr/share/fonts/truetype/adwaita/AdwaitaSans-Regular.ttf",
            "/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
        ]);

        let symbols = Self::load_font(&[
            "/usr/share/fonts/truetype/nerd-fonts-symbols/SymbolsNerdFontMono-Regular.ttf",
            "/usr/share/fonts/truetype/nerd-fonts-symbols/SymbolsNerdFont-Regular.ttf",
        ]);

        Self {
            regular,
            bold,
            symbols,
        }
    }

    fn load_font(paths: &[&str]) -> Option<Font> {
        for path in paths {
            if let Ok(bytes) = std::fs::read(path) {
                if let Ok(font) = Font::from_bytes(bytes, FontSettings::default()) {
                    return Some(font);
                }
            }
        }
        None
    }

    pub fn pick_font(&self, ch: char, bold: bool) -> Option<&Font> {
        if self
            .symbols
            .as_ref()
            .is_some_and(|f| f.lookup_glyph_index(ch) != 0)
        {
            return self.symbols.as_ref();
        }
        if bold {
            if let Some(ref f) = self.bold {
                if f.lookup_glyph_index(ch) != 0 {
                    return Some(f);
                }
            }
        }
        if let Some(ref f) = self.regular {
            if f.lookup_glyph_index(ch) != 0 {
                return Some(f);
            }
        }
        self.bold
            .as_ref()
            .or(self.regular.as_ref())
            .or(self.symbols.as_ref())
    }

    pub fn measure_width(&self, text: &str, size: f32, bold: bool) -> f32 {
        let mut width = 0.0f32;
        for ch in text.chars() {
            if let Some(font) = self.pick_font(ch, bold) {
                let metrics = font.metrics(ch, size);
                width += metrics.advance_width;
            } else {
                width += size * 0.5;
            }
        }
        width
    }

    pub fn wrap_text(
        &self,
        text: &str,
        size: f32,
        max_width: f32,
        max_lines: usize,
        bold: bool,
    ) -> Vec<String> {
        let mut lines = Vec::new();
        for paragraph in text.lines() {
            if lines.len() >= max_lines {
                break;
            }
            let mut cur_line = String::new();
            let mut cur_width = 0.0f32;

            for word in paragraph.split_whitespace() {
                let word_width = self.measure_width(word, size, bold);
                let space_width = self.measure_width(" ", size, bold);

                if cur_line.is_empty() {
                    cur_line.push_str(word);
                    cur_width = word_width;
                } else if cur_width + space_width + word_width <= max_width {
                    cur_line.push(' ');
                    cur_line.push_str(word);
                    cur_width += space_width + word_width;
                } else {
                    lines.push(std::mem::take(&mut cur_line));
                    if lines.len() >= max_lines {
                        break;
                    }
                    cur_line.push_str(word);
                    cur_width = word_width;
                }
            }

            if !cur_line.is_empty() && lines.len() < max_lines {
                lines.push(cur_line);
            }
        }

        if lines.len() == max_lines && !text.is_empty() {
            if let Some(last) = lines.last_mut() {
                let ellipsis = "…";
                let el_width = self.measure_width(ellipsis, size, bold);
                while !last.is_empty()
                    && self.measure_width(last, size, bold) + el_width > max_width
                {
                    last.pop();
                }
                last.push('…');
            }
        }

        lines
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_text(
        &self,
        pixmap: &mut PixmapMut,
        text: &str,
        x: f32,
        y: f32,
        size: f32,
        color: Color,
        bold: bool,
    ) -> f32 {
        let mut cur_x = x;
        let p_width = pixmap.width() as i32;
        let p_height = pixmap.height() as i32;

        for ch in text.chars() {
            let Some(font) = self.pick_font(ch, bold) else {
                cur_x += size * 0.5;
                continue;
            };

            let (metrics, bitmap) = font.rasterize(ch, size);
            let gx = (cur_x + metrics.xmin as f32).round() as i32;
            let gy = (y - metrics.height as f32 - metrics.ymin as f32).round() as i32;

            for row in 0..metrics.height {
                for col in 0..metrics.width {
                    let px = gx + col as i32;
                    let py = gy + row as i32;
                    if px < 0 || px >= p_width || py < 0 || py >= p_height {
                        continue;
                    }

                    let alpha_coverage = bitmap[row * metrics.width + col] as f32 / 255.0;
                    if alpha_coverage <= 0.001 {
                        continue;
                    }

                    let pixel_index = (py as usize * p_width as usize + px as usize) * 4;
                    let slice = &mut pixmap.data_mut()[pixel_index..pixel_index + 4];

                    // Premultiplied alpha blending:
                    let src_a = color.alpha() * alpha_coverage;
                    let src_r = color.red() * src_a;
                    let src_g = color.green() * src_a;
                    let src_b = color.blue() * src_a;

                    let dst_r = slice[0] as f32 / 255.0;
                    let dst_g = slice[1] as f32 / 255.0;
                    let dst_b = slice[2] as f32 / 255.0;
                    let dst_a = slice[3] as f32 / 255.0;

                    let out_a = src_a + dst_a * (1.0 - src_a);
                    if out_a > 0.0 {
                        let out_r = (src_r + dst_r * (1.0 - src_a)).min(1.0);
                        let out_g = (src_g + dst_g * (1.0 - src_a)).min(1.0);
                        let out_b = (src_b + dst_b * (1.0 - src_a)).min(1.0);

                        slice[0] = (out_r * 255.0).round() as u8;
                        slice[1] = (out_g * 255.0).round() as u8;
                        slice[2] = (out_b * 255.0).round() as u8;
                        slice[3] = (out_a * 255.0).round() as u8;
                    }
                }
            }

            cur_x += metrics.advance_width;
        }

        cur_x
    }
}
