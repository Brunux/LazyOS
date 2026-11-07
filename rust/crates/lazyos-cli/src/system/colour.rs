//! Colour: pick a pixel and show the palette (`rofi-color`).
//!
//! The old script used `slurp -p` to pick a 1x1 region, `grim` to capture it,
//! and `ffmpeg` to read the raw pixel value. It then showed the hex, RGB, and
//! the solarized palette as swatch rows.

#![allow(dead_code)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::vec_init_then_push)]
use std::cell::Cell;

use lazyos_ui::{
    BodyCtx, Design, Glyph, Header, KeyHint, ListAction, ListState, Outcome, Row, Screen,
    TextStyle, draw_card,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::Rect;

use crate::system::{missing, out, say};

const NEEDED: &[&str] = &["slurp", "grim", "ffmpeg"];

/// The colour screen: a list of swatches with the picked colour first.
pub struct ColourScreen {
    picked: Option<Colour>,
    list: ListState,
    header: Header,
    keybar: Vec<KeyHint>,
    rows_height: Cell<u16>,
}

#[derive(Clone, Debug)]
struct Colour {
    hex: String,
    rgb: (u8, u8, u8),
}

impl ColourScreen {
    pub fn new() -> Self {
        let mut list = ListState::new().with_empty("pick a pixel to see its colour");
        Self {
            picked: None,
            list,
            header: Header::new("LazyOS").subtitle("Colour"),
            keybar: vec![
                KeyHint::new("Enter", "copy / pick"),
                KeyHint::new("Esc", "close"),
            ],
            rows_height: Cell::new(20),
        }
    }

    fn pick_pixel() -> Option<Colour> {
        // slurp -p picks a point and outputs "x,y"
        let point = out(&["slurp", "-p"])?;
        let point = point.trim();
        if point.is_empty() {
            return None;
        }
        // grim -g "x,y 1x1" captures a 1x1 pixel at that point
        let tmp = std::env::temp_dir().join(format!("colour-{}.ppm", std::process::id()));
        let geom = format!("{} 1x1", point);
        if out(&["grim", "-g", &geom, "-t", "ppm", &tmp.to_string_lossy()]).is_none() {
            return None;
        }
        // ffmpeg reads the single pixel
        let output = std::process::Command::new("ffmpeg")
            .args([
                "-v",
                "quiet",
                "-f",
                "image2pipe",
                "-i",
                "-",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgb24",
                "-",
            ])
            .stdin(std::process::Stdio::from(std::fs::File::open(&tmp).ok()?))
            .stdout(std::process::Stdio::piped())
            .spawn()
            .ok()?
            .wait_with_output()
            .ok()?;
        let _ = std::fs::remove_file(&tmp);
        if output.stdout.len() < 3 {
            return None;
        }
        let (r, g, b) = (output.stdout[0], output.stdout[1], output.stdout[2]);
        let hex = format!("#{:02x}{:02x}{:02x}", r, g, b);
        Some(Colour {
            hex: hex.to_uppercase(),
            rgb: (r, g, b),
        })
    }

    fn pick_detached() {
        let script = r#"sleep 0.2
pos=$(slurp -p -f '%x,%y' 2>/dev/null) || exit 0
[ -n "$pos" ] || exit 0
rgb=$(grim -g "$pos 1x1" - 2>/dev/null | ffmpeg -v error -i pipe:0 -frames:v 1 -f rawvideo -pix_fmt rgb24 pipe:1 2>/dev/null | od -An -tu1) || rgb=
set -- ${rgb:-}
if [ $# -lt 3 ]; then
    notify-send -a LazyOS -u critical -i dialog-error "Colour" "That pixel could not be read"
    exit 1
fi
hex=$(printf '#%02x%02x%02x' "$1" "$2" "$3")
printf '%s' "$hex" | wl-copy
notify-send -a LazyOS -i color-picker "Colour" "$hex copied to the clipboard (${pos%%,*}, ${pos#*,})"
"#;
        crate::system::spawn_detached(script);
    }

    fn build_rows(&mut self) {
        let mut rows = Vec::new();
        rows.push(
            Row::new("Pick a pixel")
                .glyph(Glyph::Brush)
                .detail("the colour under the pointer, to the clipboard")
                .value("pick"),
        );
        if let Some(c) = &self.picked {
            rows.push(
                Row::new(format!("Picked: {}", c.hex))
                    .glyph(Glyph::Brush)
                    .detail(format!(
                        "RGB({}, {}, {}) · Enter copies",
                        c.rgb.0, c.rgb.1, c.rgb.2
                    ))
                    .value(format!("hex:{}", c.hex)),
            );
        }
        // Solarized palette swatches
        const SOLARIZED: &[(&str, &str, &str)] = &[
            ("base03", "#002b36", "background"),
            ("base02", "#073642", "background highlights"),
            ("base01", "#586e75", "comments / optional emphasis"),
            ("base00", "#657b83", "comments"),
            ("base0", "#839496", "body text / default code"),
            ("base1", "#93a1a1", "emphasized text"),
            ("base2", "#eee8d5", "background highlights"),
            ("base3", "#fdf6e3", "background"),
            ("yellow", "#b58900", "warning"),
            ("orange", "#cb4b16", "error / orange"),
            ("red", "#dc322f", "error"),
            ("magenta", "#d33682", "magenta"),
            ("violet", "#6c71c4", "violet"),
            ("blue", "#268bd2", "blue / links"),
            ("cyan", "#2aa198", "accent / cyan"),
            ("green", "#859900", "success"),
        ];
        for (name, hex, role) in SOLARIZED {
            rows.push(
                Row::new(format!("{}  {}", name, hex.to_uppercase()))
                    .glyph(Glyph::Brush)
                    .detail(format!("{role} · Enter copies"))
                    .value(format!("hex:{}", hex)),
            );
        }
        self.list.set_rows(rows);
    }
}

impl Screen for ColourScreen {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        draw_card(ctx.buf, ctx.area, ctx.design, Some("Colour"), true);
        let (filter, rows) = super::pick::list_layout(ctx.area);
        ctx.buf.set_string(
            filter.x,
            filter.y,
            "Enter copies or picks a pixel · Escape closes",
            ctx.design.style(TextStyle::Dim),
        );
        self.rows_height.set(rows.height);
        self.list.render(rows, ctx.buf, ctx.design, true);
    }

    fn key(&mut self, key: KeyEvent, design: &Design) -> Outcome {
        if key.kind != KeyEventKind::Press {
            return Outcome::Continue;
        }
        let rows_height = self.rows_height.get();
        match key.code {
            KeyCode::Esc => return Outcome::Done(lazyos_ui::app::exit::ESCAPE),
            KeyCode::Enter => {
                if let Some(index) = self.list.selected_index() {
                    return self.activate(index);
                }
            }
            _ => {}
        }
        match self.list.handle(key, rows_height, design) {
            ListAction::Activated(index) => self.activate(index),
            _ => Outcome::Continue,
        }
    }

    fn click(&mut self, row: Option<usize>) -> Outcome {
        if let Some(row) = row {
            self.list.set_cursor(row);
            if let Some(index) = self.list.selected_index() {
                return self.activate(index);
            }
        }
        Outcome::Continue
    }

    fn hit(&self, area: Rect, x: u16, y: u16, design: &Design) -> Option<usize> {
        let (_, rows) = super::pick::list_layout(area);
        self.list.hit_row(rows, x, y, design)
    }

    fn keybar(&self) -> Option<&[KeyHint]> {
        Some(&self.keybar)
    }

    fn header(&self) -> Option<&Header> {
        Some(&self.header)
    }
}

impl ColourScreen {
    fn activate(&mut self, index: usize) -> Outcome {
        let rows = self.list.rows();
        let Some(row) = rows.get(index) else {
            return Outcome::Continue;
        };
        let Some(key) = row.value.as_deref() else {
            return Outcome::Continue;
        };
        if key == "pick" {
            Self::pick_detached();
            Outcome::Done(lazyos_ui::app::exit::OK)
        } else if let Some(hex) = key.strip_prefix("hex:") {
            let _ = std::process::Command::new("wl-copy")
                .arg(hex)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            say(
                "Colour",
                format!("{hex} copied to the clipboard"),
                lazyos_ui::Urgency::Normal,
                Glyph::Brush,
            );
            Outcome::Done(lazyos_ui::app::exit::OK)
        } else {
            Outcome::Continue
        }
    }
}

/// `lazyos menu colour` (`rofi-color`): the window.
pub fn menu(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    if let Some(said) = missing(NEEDED) {
        say(
            "Colour",
            format!("{said} (slurp, grim, ffmpeg)"),
            lazyos_ui::Urgency::Critical,
            Glyph::Brush,
        );
        return Ok(3);
    }
    let mut screen = ColourScreen::new();
    screen.build_rows();
    let code = crate::run_screen(terminal, design, "Colour", &mut screen)?;
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_palette_is_complete() {
        let mut screen = ColourScreen::new();
        screen.build_rows();
        // 1 pick row + 16 solarized palette swatches = 17 rows
        assert_eq!(screen.list.rows().len(), 17);
        assert_eq!(screen.list.rows()[0].value.as_deref(), Some("pick"));
        assert_eq!(screen.list.rows()[1].value.as_deref(), Some("hex:#002b36"));
    }
}
