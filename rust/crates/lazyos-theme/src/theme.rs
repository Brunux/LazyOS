//! `theme.toml`: the palette as data, and the semantic roles derived from it.
//!
//! Omarchy's model, copied concretely (`docs/plan-ui.md` §10): a theme is a
//! directory whose `theme.toml` is the single source of truth, and every surface
//! is *generated* from it. The base is the 16 ANSI colours every terminal
//! already understands (so `foot.ini` can be produced from the same data as
//! `style.css`); the semantic roles are either stated in `[roles]` or derived
//! from those 16, which is what lets a community theme be a handful of lines.
//!
//! The values here are the session's, and **solarized is the one that must
//! reproduce it byte for byte**: `theme set solarized` on a healthy card is an
//! empty diff, which is the fastest way to spot a generator bug.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::Rgb;

/// A colour as it is written in a theme file: `#rrggbb`, or `rrggbb` (the
/// form `foot.ini` uses).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hex(pub Rgb);

impl Hex {
    /// Parses `#rrggbb` / `rrggbb`, the only two forms a theme file may use.
    /// Anything else names what was written, because the error is read by the
    /// person who edited the file.
    pub fn parse(text: &str) -> Result<Self, String> {
        let digits = text.strip_prefix('#').unwrap_or(text);
        if digits.len() != 6 {
            return Err(format!(
                "{text:?} is not a colour: expected #rrggbb or rrggbb"
            ));
        }
        let byte = |at: usize| {
            u8::from_str_radix(&digits[at..at + 2], 16)
                .map_err(|_| format!("{text:?} is not a colour: {digits:?} is not hex"))
        };
        Ok(Self(Rgb::new(byte(0)?, byte(2)?, byte(4)?)))
    }

    /// `#rrggbb`, the form every generated surface except `foot.ini` wants.
    pub fn hashed(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0.r, self.0.g, self.0.b)
    }

    /// `rrggbb`, the form `foot.ini` wants.
    pub fn bare(self) -> String {
        format!("{:02x}{:02x}{:02x}", self.0.r, self.0.g, self.0.b)
    }

    pub fn mix(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let blend = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
        Self(Rgb::new(
            blend(self.0.r, other.0.r),
            blend(self.0.g, other.0.g),
            blend(self.0.b, other.0.b),
        ))
    }
}

impl<'de> Deserialize<'de> for Hex {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}

/// The 16 ANSI colours plus the two a terminal asks for by name. Every theme
/// states all eighteen: a missing one is a surface that would have to guess.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Colours {
    pub background: Hex,
    pub foreground: Hex,
    pub regular0: Hex,
    pub regular1: Hex,
    pub regular2: Hex,
    pub regular3: Hex,
    pub regular4: Hex,
    pub regular5: Hex,
    pub regular6: Hex,
    pub regular7: Hex,
    pub bright0: Hex,
    pub bright1: Hex,
    pub bright2: Hex,
    pub bright3: Hex,
    pub bright4: Hex,
    pub bright5: Hex,
    pub bright6: Hex,
    pub bright7: Hex,
}

/// The roles a surface asks for, before derivation: everything is optional
/// because the 16 colours already say most of it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleOverrides {
    pub surface: Option<Hex>,
    pub surface_alt: Option<Hex>,
    /// A row under the pointer: the surface lifted off the background.
    pub hover: Option<Hex>,
    /// Metadata that is present but deliberately quiet (`base0` in solarized).
    pub faint: Option<Hex>,
    /// A danger-tinted fill behind a danger text, for a warning card.
    pub danger_bg: Option<Hex>,
    pub border: Option<Hex>,
    pub border_focus: Option<Hex>,
    pub text: Option<Hex>,
    pub strong: Option<Hex>,
    pub dim: Option<Hex>,
    pub accent: Option<Hex>,
    pub accent_alt: Option<Hex>,
    pub ok: Option<Hex>,
    pub warn: Option<Hex>,
    pub danger: Option<Hex>,
    pub info: Option<Hex>,
    pub selection: Option<Hex>,
}

/// The roles after derivation: every surface can ask for any of these and get
/// a colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Roles {
    pub surface: Hex,
    pub surface_alt: Hex,
    pub hover: Hex,
    pub faint: Hex,
    pub danger_bg: Hex,
    pub border: Hex,
    pub border_focus: Hex,
    pub text: Hex,
    pub strong: Hex,
    pub dim: Hex,
    pub accent: Hex,
    pub accent_alt: Hex,
    pub ok: Hex,
    pub warn: Hex,
    pub danger: Hex,
    pub info: Hex,
    pub selection: Hex,
}

impl Roles {
    /// Every role, by the name a template writes (`{{ accent }}`). The list is
    /// the vocabulary: a template that asks for a name not here is a build
    /// failure, not a silent empty string.
    pub fn table(&self) -> BTreeMap<&'static str, Hex> {
        BTreeMap::from([
            ("surface", self.surface),
            ("surface_alt", self.surface_alt),
            ("hover", self.hover),
            ("faint", self.faint),
            ("danger_bg", self.danger_bg),
            ("border", self.border),
            ("border_focus", self.border_focus),
            ("text", self.text),
            ("strong", self.strong),
            ("dim", self.dim),
            ("accent", self.accent),
            ("accent_alt", self.accent_alt),
            ("ok", self.ok),
            ("warn", self.warn),
            ("danger", self.danger),
            ("info", self.info),
            ("selection", self.selection),
        ])
    }
}

/// How much motion the session may use. `full` is the 300 ms sweep; `reduced`
/// is the same screens without the sweep; `off` is `LAZYOS_REDUCED_MOTION=1`
/// applied to a whole theme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Motion {
    #[default]
    Full,
    Reduced,
    Off,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Theme {
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub source: String,
    pub colors: Colours,
    #[serde(default)]
    pub roles: RoleOverrides,
    /// Wallpaper file names inside the theme directory, cycled by
    /// `Alt+Shift+W`. The first is what `theme set` installs.
    #[serde(default)]
    pub wallpapers: Vec<String>,
    #[serde(default)]
    pub motion: Motion,
}

impl Theme {
    /// Parses and validates. The error carries toml's own span for a syntax or
    /// unknown-key error (`unknown field 'acccent', expected one of ...` with
    /// the line), and the role name for a palette that cannot be read.
    pub fn parse(text: &str) -> Result<Self, String> {
        let theme: Self = toml::from_str(text).map_err(|error| error.to_string())?;
        theme.validate()?;
        Ok(theme)
    }

    /// The semantic roles: stated overrides first, derivation second.
    ///
    /// The derivations are the ones a theme author would make by hand - the
    /// accent for a border, the bright ANSI pair for a raised surface, a blend
    /// of the text and the background for the metadata that must read as
    /// secondary and must not disappear.
    pub fn roles(&self) -> Roles {
        let c = &self.colors;
        let stated = &self.roles;
        let surface = stated.surface.unwrap_or(c.regular0);
        let accent = stated.accent.unwrap_or(c.regular6);
        Roles {
            surface,
            surface_alt: stated.surface_alt.unwrap_or(c.bright0),
            hover: stated
                .hover
                .unwrap_or_else(|| surface.mix(c.background, 0.35)),
            faint: stated
                .faint
                .unwrap_or_else(|| c.foreground.mix(c.background, 0.60)),
            danger_bg: stated
                .danger_bg
                .unwrap_or_else(|| stated.danger.unwrap_or(c.regular1).mix(c.background, 0.82)),
            border: stated.border.unwrap_or(accent),
            border_focus: stated.border_focus.unwrap_or(accent),
            text: stated.text.unwrap_or(c.foreground),
            strong: stated.strong.unwrap_or(c.bright7),
            dim: stated
                .dim
                .unwrap_or_else(|| c.foreground.mix(c.background, 0.42)),
            accent,
            accent_alt: stated.accent_alt.unwrap_or(c.regular4),
            ok: stated.ok.unwrap_or(c.regular2),
            warn: stated.warn.unwrap_or(c.regular3),
            danger: stated.danger.unwrap_or(c.regular1),
            info: stated.info.unwrap_or(c.regular4),
            selection: stated.selection.unwrap_or(surface),
        }
    }

    /// Every name a template may write: the 18 colours a terminal asks for by
    /// name plus the semantic roles. The two sets are disjoint on purpose - a
    /// template that wants the terminal's `regular1` says so, and one that wants
    /// "the colour a warning is drawn in" says `danger`.
    pub fn table(&self) -> BTreeMap<&'static str, Hex> {
        let c = self.colors;
        let ansi = [
            ("background", c.background),
            ("foreground", c.foreground),
            ("regular0", c.regular0),
            ("regular1", c.regular1),
            ("regular2", c.regular2),
            ("regular3", c.regular3),
            ("regular4", c.regular4),
            ("regular5", c.regular5),
            ("regular6", c.regular6),
            ("regular7", c.regular7),
            ("bright0", c.bright0),
            ("bright1", c.bright1),
            ("bright2", c.bright2),
            ("bright3", c.bright3),
            ("bright4", c.bright4),
            ("bright5", c.bright5),
            ("bright6", c.bright6),
            ("bright7", c.bright7),
        ];
        let mut table = self.roles().table();
        table.extend(ansi);
        table
    }

    /// The roles that have to be readable on the background, with the floor
    /// each one has to clear. `dim` is deliberately not here: being quieter
    /// than the text is its job.
    fn contrast_floor(&self) -> [(&'static str, f32); 6] {
        [
            ("text", 4.5),
            ("accent", 3.0),
            ("danger", 3.0),
            ("warn", 3.0),
            ("ok", 3.0),
            ("info", 3.0),
        ]
    }

    /// A palette that cannot be read is not a theme. The ratio is named so a
    /// theme author knows which pair to change, and a name that does not match
    /// its file is refused because the active theme is stored by name.
    pub fn validate(&self) -> Result<(), String> {
        let roles = self.roles();
        if self.name.trim().is_empty() {
            return Err("`name` is empty: it is what `lazyos theme set` takes".to_string());
        }
        for (role, floor) in self.contrast_floor() {
            let colour = match role {
                "text" => roles.text,
                "accent" => roles.accent,
                "danger" => roles.danger,
                "warn" => roles.warn,
                "ok" => roles.ok,
                _ => roles.info,
            };
            let ratio = crate::contrast::ratio(colour.0, self.colors.background.0);
            if ratio < floor {
                return Err(format!(
                    "`{role}` is {:.1}:1 against `background`, below the {floor:.1}:1 floor",
                    ratio
                ));
            }
        }
        Ok(())
    }

    /// The palette the UI crate draws with (spec 002's tokens, filled).
    pub fn palette(&self) -> crate::Palette {
        let roles = self.roles();
        crate::Palette {
            bg: self.colors.background.0,
            surface: roles.surface.0,
            surface_alt: roles.surface_alt.0,
            text: roles.text.0,
            dim: roles.dim.0,
            accent: roles.accent.0,
            accent_alt: roles.accent_alt.0,
            ok: roles.ok.0,
            warn: roles.warn.0,
            danger: roles.danger.0,
            selection: roles.selection.0,
        }
    }

    /// Terminal OSC sequences for setting palette colors dynamically on active terminals.
    ///
    /// Formats OSC 10 (foreground), OSC 11 (background), OSC 12 (cursor / accent),
    /// and OSC 4;0..15 (16 ANSI colors) with string terminators (`\x1b\\`).
    pub fn osc_sequences(&self) -> String {
        let c = &self.colors;
        let mut s = String::with_capacity(512);
        s.push_str(&format!("\x1b]10;{}\x1b\\", c.foreground.hashed()));
        s.push_str(&format!("\x1b]11;{}\x1b\\", c.background.hashed()));
        s.push_str(&format!("\x1b]12;{}\x1b\\", self.roles().accent.hashed()));
        let ansi = [
            c.regular0, c.regular1, c.regular2, c.regular3, c.regular4, c.regular5, c.regular6,
            c.regular7, c.bright0, c.bright1, c.bright2, c.bright3, c.bright4, c.bright5,
            c.bright6, c.bright7,
        ];
        for (idx, col) in ansi.iter().enumerate() {
            s.push_str(&format!("\x1b]4;{idx};{}\x1b\\", col.hashed()));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A theme with no `[roles]` at all, which is the shape a community theme
    /// has when it only carries the 16 colours.
    fn strip_roles(text: &str) -> String {
        let mut out = String::new();
        let mut in_roles = false;
        for line in text.lines() {
            if line.trim_start().starts_with('[') {
                in_roles = line.trim() == "[roles]";
            }
            if !in_roles {
                out.push_str(line);
                out.push('\n');
            }
        }
        out
    }

    fn solarized() -> Theme {
        Theme::parse(crate::builtin::SOLARIZED).expect("the built-in parses")
    }

    #[test]
    fn a_colour_is_written_the_way_the_surfaces_write_it() {
        let hex = Hex::parse("#2aa198").unwrap();
        assert_eq!(hex.bare(), "2aa198");
        assert_eq!(hex.hashed(), "#2aa198");
        assert_eq!(Hex::parse("2aa198").unwrap(), hex);
        assert!(Hex::parse("#2aa19").is_err());
        assert!(Hex::parse("#zzzzzz").is_err());
    }

    #[test]
    fn solarized_is_the_session_that_exists_today() {
        // The one test that keeps the engine honest: the built-in has to be the
        // palette the hand-written files already carry, or the first `theme
        // set` on a healthy card is not the empty diff spec 004 promises.
        assert_eq!(solarized().palette(), crate::Palette::solarized_dark());
        let roles = solarized().roles();
        assert_eq!(roles.accent.hashed(), "#2aa198");
        assert_eq!(roles.dim.hashed(), "#586e75");
        assert_eq!(roles.selection.hashed(), "#073642");
    }

    #[test]
    fn every_builtin_parses_and_clears_its_own_contrast_floor() {
        for (slug, text) in crate::builtin::ALL {
            let theme = Theme::parse(text)
                .unwrap_or_else(|error| panic!("built-in `{slug}` does not parse: {error}"));
            assert!(
                !theme.wallpapers.is_empty(),
                "built-in `{slug}` has no wallpaper: `theme set` would leave the old one"
            );
        }
        assert_eq!(crate::builtin::ALL.len(), 8, "the plan's eight themes");
    }

    #[test]
    fn an_unknown_key_names_itself_and_not_a_silent_default() {
        let text = crate::builtin::SOLARIZED.replace("regular1", "regular_one");
        let error = Theme::parse(&text).unwrap_err();
        assert!(
            error.contains("regular_one"),
            "the error must name the key that is wrong: {error}"
        );
        let text = crate::builtin::SOLARIZED.replace("[roles]", "[role]");
        let error = Theme::parse(&text).unwrap_err();
        assert!(
            error.contains("role"),
            "the error must name the table that is wrong: {error}"
        );
    }

    #[test]
    fn a_missing_colour_is_refused_because_a_surface_would_have_to_guess() {
        let text = crate::builtin::SOLARIZED
            .lines()
            .filter(|line| !line.starts_with("bright5"))
            .collect::<Vec<_>>()
            .join("\n");
        let error = Theme::parse(&text).unwrap_err();
        assert!(
            error.contains("bright5"),
            "the error must name the gap: {error}"
        );
    }

    #[test]
    fn a_palette_that_cannot_be_read_is_refused_with_the_pair_and_the_ratio() {
        let text = crate::builtin::SOLARIZED.replace("text = \"#93a1a1\"", "text = \"#0b2b36\"");
        // `parse` validates as part of parsing: a theme that cannot be read is
        // not a theme, and a `set` that half-applied one would be the bug.
        let error = Theme::parse(&text).expect_err("an unreadable palette is refused");
        assert!(error.contains("text"), "the failing role is named: {error}");
        assert!(error.contains(":1"), "the ratio is named: {error}");
    }

    #[test]
    fn roles_derive_from_the_sixteen_when_a_theme_states_none() {
        let text = strip_roles(crate::builtin::CATPPUCCIN);
        let theme = Theme::parse(&text).unwrap();
        let roles = theme.roles();
        assert_eq!(roles.accent, theme.colors.regular6);
        assert_eq!(roles.border, theme.colors.regular6);
        assert_eq!(roles.ok, theme.colors.regular2);
        assert_eq!(roles.danger, theme.colors.regular1);
        assert_eq!(roles.surface, theme.colors.regular0);
        assert_ne!(roles.dim, roles.text, "dim is secondary, not the text");
    }

    #[test]
    fn osc_sequences_format_foreground_background_and_palette() {
        let theme = solarized();
        let osc = theme.osc_sequences();
        assert!(osc.contains("\x1b]10;#93a1a1\x1b\\"));
        assert!(osc.contains("\x1b]11;#002b36\x1b\\"));
        assert!(osc.contains("\x1b]12;#2aa198\x1b\\"));
        assert!(osc.contains("\x1b]4;0;#073642\x1b\\"));
        assert!(osc.contains("\x1b]4;15;#fdf6e3\x1b\\"));
    }
}
