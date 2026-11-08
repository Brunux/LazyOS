//! The design system's vocabulary: roles, spaces, text styles and density.
//!
//! A widget asks for `ColorRole::Accent` and `TextStyle::Dim`, never for a hex
//! value or a magic padding number. That is what makes the root menu, the
//! installer and the AI surface look like one session instead of thirty
//! decisions, and it is what lets the theme engine (spec 004) change every
//! surface by changing one palette.
//!
//! The rules the tests enforce:
//!
//! - a colour comes from the palette (`Color::Rgb` appears in this file only);
//! - the density is decided once, from the session, never per widget;
//! - a role is added here deliberately - a widget that "just needs a slightly
//!   different grey" is a token, not a local constant.

use lazyos_theme::{Palette, Rgb};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, BorderType, Borders, Padding};

/// What a surface is asking for, semantically.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorRole {
    /// The window behind everything.
    Bg,
    /// A card, a bar, the selected row's fill.
    Surface,
    /// A second surface on top of `Surface`.
    SurfaceAlt,
    /// Regular text.
    Text,
    /// Secondary text: sizes, dates, hints.
    Dim,
    /// The one colour that marks "this is LazyOS" and the current focus.
    Accent,
    /// A second accent for information that is not the focus.
    AccentAlt,
    Ok,
    Warn,
    Danger,
    /// The fill of the selected row.
    Selection,
}

/// The spacing scale. Four steps, no arbitrary numbers: on this 1366 px panel
/// one cell is ~8 px, so the scale is in cells and the density multiplies it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    Zero,
    One,
    Two,
    Three,
}

impl Space {
    pub const fn cells(self) -> u16 {
        match self {
            Space::Zero => 0,
            Space::One => 1,
            Space::Two => 2,
            Space::Three => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Border {
    /// The session's rounded corners (`╭╮╰╯`).
    Rounded,
    /// No frame at all, for a body that owns its whole area.
    None,
}

/// The text styles a widget can ask for, by job rather than by attribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextStyle {
    /// A window or card title.
    Title,
    Body,
    Dim,
    /// A command, a path, a number worth copying.
    Code,
    /// A key in the key bar.
    Key,
    /// The error card's summary.
    Danger,
}

/// How much room a row and the frame's bands get.
///
/// The tablet is folded over the keyboard: the touch target has to be a finger,
/// not a mouse pointer, so a row takes two lines and the bands grow. It is read
/// once from the session (`$XDG_RUNTIME_DIR/lazyos-tablet`, the directory
/// `tablet-mode` keeps) - never per widget, which is how a window ends up with
/// half its rows touch-sized.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Density {
    Normal,
    Tablet,
}

impl Density {
    /// The number of lines one row occupies.
    pub const fn row_stride(self) -> u16 {
        match self {
            Density::Normal => 1,
            Density::Tablet => 2,
        }
    }

    /// Height of the header band, in lines.
    pub const fn header_height(self) -> u16 {
        match self {
            Density::Normal => 2,
            Density::Tablet => 3,
        }
    }
}

/// The theme's colour in the terminal library's type.
pub fn color(rgb: Rgb) -> Color {
    Color::Rgb(rgb.r, rgb.g, rgb.b)
}

/// The palette plus the decisions derived from it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Design {
    palette: Palette,
    density: Density,
    reduced_motion: bool,
}

impl Design {
    pub const fn new(palette: Palette, density: Density, reduced_motion: bool) -> Self {
        Self {
            palette,
            density,
            reduced_motion,
        }
    }

    pub fn solarized() -> Self {
        Self::new(Palette::solarized_dark(), Density::Normal, false)
    }

    /// The session's design: the active theme's palette, the density the tablet state asks for,
    /// and `LAZYOS_REDUCED_MOTION=1` honoured.
    pub fn for_session(runtime_dir: Option<&str>) -> Self {
        let paths = lazyos_theme::Paths::from_env();
        let palette = Palette::for_session(&paths);
        Self::new(
            palette,
            Self::detect_density(runtime_dir),
            matches!(std::env::var("LAZYOS_REDUCED_MOTION"), Ok(v) if v == "1"),
        )
    }

    pub fn with_palette(mut self, palette: Palette) -> Self {
        self.palette = palette;
        self
    }

    /// Tablet mode keeps its state directory while it is on, so its presence is
    /// the signal; a session that is not rooted in an XDG runtime directory gets
    /// the normal density instead of guessing.
    pub fn detect_density(runtime_dir: Option<&str>) -> Density {
        match runtime_dir {
            Some(dir) if std::path::Path::new(dir).join("lazyos-tablet").is_dir() => {
                Density::Tablet
            }
            _ => Density::Normal,
        }
    }

    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    pub fn density(&self) -> Density {
        self.density
    }

    pub fn reduced_motion(&self) -> bool {
        self.reduced_motion
    }

    pub fn with_density(mut self, density: Density) -> Self {
        self.density = density;
        self
    }

    pub fn with_reduced_motion(mut self, reduced: bool) -> Self {
        self.reduced_motion = reduced;
        self
    }

    pub fn color(&self, role: ColorRole) -> Color {
        let p = &self.palette;
        color(match role {
            ColorRole::Bg => p.bg,
            ColorRole::Surface => p.surface,
            ColorRole::SurfaceAlt => p.surface_alt,
            ColorRole::Text => p.text,
            ColorRole::Dim => p.dim,
            ColorRole::Accent => p.accent,
            ColorRole::AccentAlt => p.accent_alt,
            ColorRole::Ok => p.ok,
            ColorRole::Warn => p.warn,
            ColorRole::Danger => p.danger,
            ColorRole::Selection => p.selection,
        })
    }

    pub fn style(&self, kind: TextStyle) -> Style {
        match kind {
            TextStyle::Title => Style::default()
                .fg(self.color(ColorRole::Accent))
                .add_modifier(Modifier::BOLD),
            TextStyle::Body => Style::default()
                .fg(self.color(ColorRole::Text))
                .bg(self.color(ColorRole::Bg)),
            TextStyle::Dim => Style::default()
                .fg(self.color(ColorRole::Dim))
                .bg(self.color(ColorRole::Bg)),
            TextStyle::Code => Style::default().fg(self.color(ColorRole::AccentAlt)),
            TextStyle::Key => Style::default()
                .fg(self.color(ColorRole::Bg))
                .bg(self.color(ColorRole::Accent))
                .add_modifier(Modifier::BOLD),
            TextStyle::Danger => Style::default()
                .fg(self.color(ColorRole::Danger))
                .add_modifier(Modifier::BOLD),
        }
    }

    /// The padding inside a card, in cells: the density widens it because a
    /// finger needs more room than a pointer.
    pub fn padding(&self, space: Space) -> u16 {
        let extra = match self.density {
            Density::Normal => 0,
            Density::Tablet => 1,
        };
        space.cells() + extra
    }

    pub fn header_height(&self) -> u16 {
        self.density.header_height()
    }

    pub fn row_stride(&self) -> u16 {
        self.density.row_stride()
    }

    /// A framed surface (a card, a list, the body).
    pub fn block(&self, border: Border) -> Block<'static> {
        let pad = self.padding(Space::One);
        let mut block = Block::default()
            .style(
                Style::default()
                    .bg(self.color(ColorRole::Bg))
                    .fg(self.color(ColorRole::Text)),
            )
            .padding(Padding::new(pad, pad, 0, 0));
        if border == Border::Rounded {
            block = block
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(self.color(ColorRole::SurfaceAlt)));
        }
        block
    }
}

/// The glyphs the widgets draw, as codepoints.
///
/// `lazyos --glyphs` prints these and `check-system.sh` asks fontconfig for each
/// one: a box-drawing character that no font in foot's chain has is a hole in
/// the frame that logs nothing, which is the same class of bug as a missing icon
/// in the launcher. Keeping the list here means the check cannot drift from what
/// the widgets actually draw.
pub const GLYPHS: &[char] = &[
    '\u{2500}', // ─ horizontal
    '\u{2502}', // │ vertical
    '\u{256d}', // ╭
    '\u{256e}', // ╮
    '\u{256f}', // ╯
    '\u{2570}', // ╰
    '\u{2588}', // █ gauge full
    '\u{258d}', // ▍ selection bar
    '\u{2591}', // ░ gauge empty
    '\u{2595}', // ▕ gauge end
    '\u{2026}', // … truncation
    '\u{00b7}', // · separator
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_role_maps_to_a_palette_colour() {
        let design = Design::solarized();
        let roles = [
            ColorRole::Bg,
            ColorRole::Surface,
            ColorRole::SurfaceAlt,
            ColorRole::Text,
            ColorRole::Dim,
            ColorRole::Accent,
            ColorRole::AccentAlt,
            ColorRole::Ok,
            ColorRole::Warn,
            ColorRole::Danger,
            ColorRole::Selection,
        ];
        // Solarized deliberately shares values between roles (selection is
        // base02, the same as a surface); what must hold is that nothing except
        // `Bg` maps to the background, and that the meaning-carrying roles are
        // distinguishable from each other.
        for role in roles {
            if role != ColorRole::Bg {
                assert_ne!(design.color(role), design.color(ColorRole::Bg), "{role:?}");
            }
        }
        let meaning = [
            ColorRole::Text,
            ColorRole::Dim,
            ColorRole::Accent,
            ColorRole::AccentAlt,
            ColorRole::Ok,
            ColorRole::Warn,
            ColorRole::Danger,
        ];
        for (i, a) in meaning.iter().enumerate() {
            for b in &meaning[i + 1..] {
                assert_ne!(design.color(*a), design.color(*b), "{a:?} vs {b:?}");
            }
        }
    }

    #[test]
    fn text_styles_are_readable_on_the_background() {
        let design = Design::solarized();
        // The three that carry meaning must not be the background colour: a
        // title or an error that disappears into the window is a blank row.
        for kind in [TextStyle::Title, TextStyle::Danger, TextStyle::Body] {
            assert_ne!(design.style(kind).fg, Some(design.color(ColorRole::Bg)));
        }
    }

    #[test]
    fn dense_mode_grows_the_row_and_the_bands() {
        let normal = Design::solarized();
        let tablet = Design::solarized().with_density(Density::Tablet);
        assert_eq!(normal.row_stride(), 1);
        assert_eq!(tablet.row_stride(), 2);
        assert_eq!(tablet.header_height(), normal.header_height() + 1);
        assert_eq!(tablet.padding(Space::Two), normal.padding(Space::Two) + 1);
    }

    #[test]
    fn the_spacing_scale_is_four_steps() {
        assert_eq!(Space::Zero.cells(), 0);
        assert_eq!(Space::Three.cells(), 3);
        assert!(Space::Zero.cells() < Space::One.cells());
        assert!(Space::One.cells() < Space::Two.cells());
        assert!(Space::Two.cells() < Space::Three.cells());
    }

    #[test]
    fn a_missing_tablet_state_means_normal_density() {
        assert_eq!(Design::detect_density(None), Density::Normal);
        assert_eq!(
            Design::detect_density(Some("/nonexistent/lazyos")),
            Density::Normal
        );
    }

    #[test]
    fn the_glyph_table_has_no_duplicates() {
        let mut seen = std::collections::BTreeSet::new();
        for glyph in GLYPHS {
            assert!(seen.insert(*glyph), "duplicate glyph {glyph:?}");
        }
    }
}
