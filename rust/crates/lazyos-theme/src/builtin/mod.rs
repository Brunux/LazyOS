//! The sixteen built-in themes, compiled in.
//!
//! They are `include_str!`ed rather than read from `/usr/share/lazyos/themes/`
//! for one reason: the binary has to be able to say `theme set solarized` on a
//! machine where the image tree is missing (a dev checkout, a rescue boot), and
//! a theme is data with no state. The shipped copies exist too, because a user
//! theme is a directory beside them and `theme list` shows both.

/// The slug is the file name and what `lazyos theme set` takes; the display
/// name is inside the file, because two themes may share a slug only by being
/// the same theme.
pub const SOLARIZED: &str = include_str!("solarized.toml");
pub const CATPPUCCIN: &str = include_str!("catppuccin.toml");
pub const TOKYO_NIGHT: &str = include_str!("tokyo-night.toml");
pub const ROSE_PINE: &str = include_str!("rose-pine.toml");
pub const NORD: &str = include_str!("nord.toml");
pub const GRUVBOX: &str = include_str!("gruvbox.toml");
pub const EVERFOREST: &str = include_str!("everforest.toml");
pub const KANAGAWA: &str = include_str!("kanagawa.toml");
pub const DRACULA: &str = include_str!("dracula.toml");
pub const ONE_DARK: &str = include_str!("one-dark.toml");
pub const HACKERMAN: &str = include_str!("hackerman.toml");
pub const LUMON: &str = include_str!("lumon.toml");
pub const MATTE_BLACK: &str = include_str!("matte-black.toml");
pub const OMACON: &str = include_str!("omacon.toml");
pub const CATPPUCCIN_LATTE: &str = include_str!("catppuccin-latte.toml");
pub const FLEXOKI_LIGHT: &str = include_str!("flexoki-light.toml");

/// Every built-in, in the order `Alt+Shift+T` cycles them: solarized first,
/// because that is where a fresh install starts and the cycle should return to
/// it in one direction.
pub const ALL: [(&str, &str); 16] = [
    ("solarized", SOLARIZED),
    ("catppuccin", CATPPUCCIN),
    ("tokyo-night", TOKYO_NIGHT),
    ("rose-pine", ROSE_PINE),
    ("nord", NORD),
    ("gruvbox", GRUVBOX),
    ("everforest", EVERFOREST),
    ("kanagawa", KANAGAWA),
    ("dracula", DRACULA),
    ("one-dark", ONE_DARK),
    ("hackerman", HACKERMAN),
    ("lumon", LUMON),
    ("matte-black", MATTE_BLACK),
    ("omacon", OMACON),
    ("catppuccin-latte", CATPPUCCIN_LATTE),
    ("flexoki-light", FLEXOKI_LIGHT),
];

/// A built-in by slug, for a machine with no theme directory.
pub fn get(slug: &str) -> Option<&'static str> {
    ALL.iter()
        .find(|(name, _)| *name == slug)
        .map(|(_, text)| *text)
}

/// The slugs, in cycle order.
pub fn slugs() -> impl Iterator<Item = &'static str> {
    ALL.iter().map(|(slug, _)| *slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_slug_is_its_file_and_the_cycle_has_no_repeats() {
        let unique: std::collections::BTreeSet<_> = slugs().collect();
        assert_eq!(unique.len(), ALL.len(), "a slug appears twice");
        for (slug, text) in ALL {
            assert!(!text.trim().is_empty(), "{slug} is empty");
            assert_eq!(get(slug), Some(text));
        }
        assert_eq!(slugs().next(), Some("solarized"));
        assert_eq!(get("no-such-theme"), None);
    }
}
