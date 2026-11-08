{{ header }}
# foot: the session terminal, in the active theme's palette.
#
# foot is the default terminal here instead of Deboma's (and this port's
# original) alacritty for two reasons: it is around half the resident memory of
# an alacritty window (it has no GPU renderer to feed - it draws with pixman),
# and it is a native Wayland client with no glib and no Rust runtime, which
# matters on a 4 GB machine whose whole point is that the compositor and the bar
# fit around the applications.  The bar's popups (nmtui, bluetoothctl, the apt
# updater) run here too, with `--app-id=popup`.
#
# Syntax check: foot --check-config -c <this file>.
#
# The palette comes from the active theme (spec 004): this file is generated,
# and `lazyos theme set NAME` is how it changes.
[main]
# The font list is also the icon policy, and the order is the point: foot searches
# these in order and draws the first font that has the glyph, *at that font's own
# size*. Only when none of them has it does it ask fontconfig (foot.ini(5): "If a
# glyph cannot be found in any of the fallback fonts, the dynamic fallback list
# from fontconfig ... is searched"). A glyph that comes from a fallback therefore
# renders smaller than the cell unless its font is listed at a size that matches,
# which is why the three icon fonts here are one point larger than the text. This
# was written the other way once, on the belief that foot had no fallback at all;
# the result was icons a fifth short of the line and a prompt that looked wrong
# beside kitty's, so the sizes are the fix and the man page is the reason.
#
# JetBrains Mono is first because it is the cell. Symbols Nerd Font Mono is what
# makes eza --icons, fastfetch, the starship branch/clock/character glyphs and the
# language icons render at all - `fonts-nerd-symbols` is the DFSG-stripped set
# (Octicons, Codicons, Devicons, Seti, Powerline): no Font Awesome, no Material
# Design Icons, no Nerd Fonts v3 `nf-md` plane 15. Font Awesome and Material
# Design Icons carry what that set leaves out, which here is the prompt's folder
# substitutions (U+F019, U+F03E, U+F0219, U+F075A, U+F0C8B) and the Debian mark
# (U+F306). The names are the families `fc-list : family` prints, "version 6"
# included.
#
# The prompt's separators are the triangles U+E0B0/U+E0B2, which JetBrains Mono
# draws itself and which therefore tile with the cell at any size. The rounded
# caps U+E0B6/U+E0B4 exist only in the symbol font, and drawn at its own size they
# sit short of the segment they close - which is why ~/.config/starship.toml uses
# triangles rather than caps.
font=JetBrains Mono:size=11, Symbols Nerd Font Mono:size=12, Font Awesome 6 Free Solid:size=12, Material Design Icons version 6:size=12
pad=6x6
dpi-aware=no

# Auto-copy selected text directly to both the PRIMARY selection and the system CLIPBOARD.
selection-target=both

[scrollback]
lines=1000

[colors-dark]
# 0.87 rather than opaque: swayfx draws a window's blur *behind* the window
# (see /etc/sway/config.d/26-swayfx.conf), so a fully opaque terminal would
# hide the effect completely. alpha-mode is left at its `default`, which applies
# the alpha only to cells using the default background - a program that paints
# its own background (btop's panels, a coloured banner, a selection) keeps its
# exact colour and contrast. 0.87 is as far as this goes before base1 text over
# a bright wallpaper stops being comfortable to read.
alpha=0.87
foreground={{ foreground:bare }}
background={{ background:bare }}
regular0={{ regular0:bare }}
regular1={{ regular1:bare }}
regular2={{ regular2:bare }}
regular3={{ regular3:bare }}
regular4={{ regular4:bare }}
regular5={{ regular5:bare }}
regular6={{ regular6:bare }}
regular7={{ regular7:bare }}
bright0={{ bright0:bare }}
bright1={{ bright1:bare }}
bright2={{ bright2:bare }}
bright3={{ bright3:bare }}
bright4={{ bright4:bare }}
bright5={{ bright5:bare }}
bright6={{ bright6:bare }}
bright7={{ bright7:bare }}

[csd]
preferred=server
