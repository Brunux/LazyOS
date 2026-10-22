#!/bin/bash
# The lazyOS brand, rendered from brand/*.svg into the files the image and the
# repository ship.  brand/README.md is the brand itself: the mark, the
# wordmark, the palette and the rules.
#
# Usage: scripts/make-brand.sh
#
#   brand/lazyos.svg                                        the mark (source)
#   brand/wallpaper.svg                                     the wallpaper (source)
#   brand/social-preview.svg                                GitHub's social card (source)
#
#   .../icons/LazyOS/48x48/apps/lazyos.png                  the launcher, the About row
#   .../icons/LazyOS/scalable/apps/lazyos.svg               GTK at larger sizes
#   .../home/Pictures/Wallpapers/lazyos.png                 1366x912, the default background
#   .../home/.config/lazyos/wordmark.txt                    fastfetch's logo
#   .../home/.config/lazyos/screensaver.txt                 the idle screen's art
#   brand/social-preview.png                                1280x640, the repository's card
#   site/apple-touch-icon.png                               180x180, the landing page's touch icon
#
# The outputs are committed, because build-rootfs.sh stages `git archive HEAD`
# and a build host without rsvg-convert must still be able to build an image.
# Only an edit to brand/ needs this script to run.  The landing page (site/)
# takes its pictures straight from brand/ when the Pages workflow assembles it.
#
# Host tool, like make-icons.sh: rsvg-convert is `librsvg2-bin` on Debian and
# `librsvg` on Alpine (scripts/00-deps.sh installs it); the text is drawn with
# JetBrains Mono, which is `fonts-jetbrains-mono` on Debian.
set -eu

here=$(cd "$(dirname "$0")/.." && pwd)
brand="$here/brand"
theme="$here/uis/sway/files/usr/share/icons/LazyOS"
wall="$here/uis/sway/files/home/Pictures/Wallpapers/lazyos.png"
conf="$here/uis/sway/files/home/.config/lazyos"
wordmark="$conf/wordmark.txt"
saver="$conf/screensaver.txt"

die() {
	echo "make-brand.sh: $*" >&2
	exit 1
}

command -v rsvg-convert >/dev/null 2>&1 ||
	die "rsvg-convert is not installed (Debian: librsvg2-bin, Alpine: librsvg)"

# fc-match always answers, with a fallback when the family is missing; ask it
# for the family it resolved and refuse a substitute, because a wallpaper
# typeset in DejaVu is not the brand.
family=$(fc-match -f '%{family}' 'JetBrains Mono' 2>/dev/null || true)
case "$family" in
"JetBrains Mono" | "JetBrains Mono,"*) ;;
*) die "JetBrains Mono is not installed (Debian: fonts-jetbrains-mono)" ;;
esac

for _f in lazyos.svg wallpaper.svg social-preview.svg; do
	[ -r "$brand/$_f" ] || die "missing brand/$_f"
done

mkdir -p "$theme/48x48/apps" "$theme/scalable/apps" "$(dirname "$wall")" "$conf"

echo "  mark 48x48  $(basename "$theme")/48x48/apps/lazyos.png"
rsvg-convert -w 48 -h 48 -o "$theme/48x48/apps/lazyos.png" "$brand/lazyos.svg"

echo "  mark svg    $(basename "$theme")/scalable/apps/lazyos.svg"
install -m 0644 "$brand/lazyos.svg" "$theme/scalable/apps/lazyos.svg"

# The panel's own mode (docs/optimization.md): generating at the size it is
# shown at avoids a resample, and `fill` in the sway config crops rather than
# stretches if the output ever differs.  The size lives in the SVG.
echo "  wallpaper   1366x912  Pictures/Wallpapers/lazyos.png"
rsvg-convert -w 1366 -h 912 -b '#002b36' -o "$wall" "$brand/wallpaper.svg"

echo "  social      1280x640  brand/social-preview.png"
rsvg-convert -w 1280 -h 640 -b '#002b36' -o "$brand/social-preview.png" "$brand/social-preview.svg"

# iOS masks the icon itself and shows transparency as black, so the touch icon
# is the mark on an opaque base03 square.
echo "  touch icon  180x180   site/apple-touch-icon.png"
rsvg-convert -w 180 -h 180 -b '#002b36' -o "$here/site/apple-touch-icon.png" "$brand/lazyos.svg"

# The text forms of the mark.  A terminal cannot draw the SVG, so each terminal
# surface gets the z_ in full and half blocks, which every monospace font lays
# out the same way (the old ANSI Shadow letters detached their shadow columns
# outside a terminal).  The z is 11 columns, the cursor 5, and every row of the
# picture is padded to the same 17 columns: lazyos-screensaver centres the art
# line by line, so rows of different widths would shear the letter.

# fastfetch's logo: `"type": "file"`, so $1..$3 are the colours config.jsonc
# names (cyan, orange, base2) and fastfetch measures the width without them.
echo "  wordmark    .config/lazyos/wordmark.txt"
cat >"$wordmark" <<'EOF'
$1███████████
$1      ▄███▀
$1   ▄███▀
$1▄███▀
$1███████████ $2▄▄▄▄▄

$3lazyOS$2_
$1lazy by design
EOF

# The idle screen's art: one colour (the terminal's foreground, cyan), so no
# escapes; tte paints its own palette over it.  The picture's rows are quoted
# because their trailing spaces are the padding, and an editor that strips
# trailing whitespace would otherwise shear the z without anyone noticing.
echo "  idle art    .config/lazyos/screensaver.txt"
printf '%s\n' \
	"███████████      " \
	"      ▄███▀      " \
	"   ▄███▀         " \
	"▄███▀            " \
	"███████████ ▄▄▄▄▄" \
	"" \
	"lazyOS_" \
	"" \
	":: lazy by design ::" \
	"» press any key to continue «" >"$saver"

echo "Done."
