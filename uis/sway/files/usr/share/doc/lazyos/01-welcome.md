# Welcome to LazyOS

LazyOS is Debian (**forky**, arm64) running on a Google **Hayato**: the ASUS
Chromebook Detachable CM3 with a MediaTek **MT8192** ("Asurada") SoC, 4 GB of RAM
and a 58 GB eMMC. It is not a Debian remix: the hardware support comes from
Nura (the postmarketOS-based system this port started from), and Debian supplies
everything above the kernel.

## The first five minutes

* Log in with the user account the image was built with (`brunux` by default,
  password `debian`). Change the password now: `passwd`.
* `sudo` does not ask for a password on this machine. Anything that needs root
  goes through it, including the scripts that rewrite the boot partition.
* Open a terminal: **Alt+Return** (or the bar's menu → Terminal).
* Open the launcher: the **Search key** (the magnifier, left of Tab) or
  **Alt+D**. Type part of a name, Enter.
* See every key binding: **Alt+Ctrl+K**.
* This manual: the **Learn** entry in the bar menu (**Alt+Space**), or
  `rofi-learn` in a terminal.

## What is on the screen

| Piece | What it is |
|---|---|
| the bar (top) | waybar: menu, workspaces, clock, updates, tray, keyboard layout, Bluetooth, network, volume, brightness, battery. Left/right click and scroll do different things on each |
| compositor | **swayfx**, a fork of sway that draws rounded corners, shadows and a blurred backdrop under the translucent terminal, launcher and bar |
| the terminal | **foot**: solarized palette, no GPU renderer, about half the memory of an Alacritty window |
| the launcher | **rofi**: applications, windows, commands, and the bar's own menus |
| notifications | **mako**, bottom of the top-right corner, under the bar |
| the session | greetd starts sway at boot; swayidle runs the idle screen at 150 s and the lock at 300 s |

**The workspace pills say what is in them.** Each pill is the number of the
workspace plus one glyph per *kind* of window open there - a terminal, a
browser, a file manager, a picture, a video, a document - and a workspace with
nothing in it keeps its bare number. The glyphs are the same alphabet the menus
draw with, so a terminal in the bar and a terminal in the power menu are the
same picture. It is the one thing on this system that keeps running in order to
do it (`workstyle`, about 1 MB), because "where did I leave that window" is a
question only something watching can answer. The table is
`~/.config/workstyle/config.toml`: one line per application, matched against its
title or its app_id, and the next window or workspace change picks it up - no
restart, and an app with no line of its own shows a plain window outline.

## The windows the framework draws

Some of the session's menus are already one program (`lazyos`, the same binary
under every old name), and every window it opens has the same three bands, so a
window you have never seen is readable the first time:

| Band | What it says |
|---|---|
| top | what this is, in cyan; a second line of context when there is one; and the one number worth showing while it runs, right-aligned over a rule |
| middle | the list or the body. The selected row has a cyan bar down its left edge and its whole width filled; everything else is dim |
| bottom | the keys that are not obvious, in brackets: `[Enter] open   [Esc] close` |

The keys are letters and Enter, never Alt chords: sway sees every Alt chord
before the window does, so an Alt hint in a window would be a hint the session
swallows. Type to filter the list - **upper and lower case are the same**, so
`readme` finds `README.md` and `año` finds `AÑO`, and the list narrows while you
type instead of waiting for Enter; `Up`/`Down` (or `k`/`j`) to move, `Enter` to
open, `Esc` to close with nothing done. A click on a row does what moving to it
and pressing `Enter` does; fold the keyboard back and the rows become two lines
tall so a finger can hit one, and a click on the blank line between two of them
does nothing at all.

The pictures in these windows are **Nerd Font** codepoints - one cell each,
from the same symbol font the prompt's glyphs come from - not the Font Awesome
images the launcher draws. A launcher row asks rofi's icon theme for a file, so
the picture can be any size the theme has; a window row has to sit in a fixed
column beside a label, so its picture comes from the one symbol font the image
guarantees: `fonts-nerd-symbols`, the package `foot.ini` names after JetBrains
Mono.

That package is DFSG-stripped, and eight of the session's names are Font Awesome
codepoints it does not carry: **Bluetooth**, **Wi-Fi**, the **battery**, the
**keyboard**, the **touchpad**, the **calculator**, a **QR code** and **crop**
show the short words `bt`, `wifi`, `bat`, `kbd`, `pad`, `=`, `qr` and `crop` on
the row instead. That is the design, not a missing font: a name is only a picture
while some font draws it, and a box is a hole in the row that logs nothing. Every
other picture is a codepoint of that font, and `check-system.sh` asks it for each
one.

A window fades in over a third of a second and nothing else animates;
`LAZYOS_REDUCED_MOTION=1` starts it at its final frame.

## The mark

The mark is a prompt: **z_**, a cyan `z` and an orange cursor, and the
wordmark is `lazyOS_`. The wallpaper is a keyboard in the dark with one lit
key carrying the mark - the key to press - and `lazyOS_`, `lazy by design` and
the arcade line in the corner. The colours are the session's own Solarized,
so nothing else had to be repainted. The launcher icon, the wallpaper and the
**About LazyOS** card are the same drawing - the card is **Alt+Space** >
**About**, and its last row opens `fastfetch` for the full picture. The lock screen keeps the mountain photograph
instead: the desktop is the surface the mark owns, and the photograph is what is
up when nobody is looking.

## The idle screen

150 s without input and the screen becomes **lazyOS_**: the z_ mark, the wordmark, `:: lazy by
design ::` and the arcade line `» press any key to play «`, drawn in cyan. One terminal-text
effect rebuilds the letters, then the next one does it differently, and so on while the machine waits. Any
key, any movement of the mouse or trackpad, or a tap on the touchscreen returns you to the
session, and the 150 s starts over.

At 300 s the screen locks instead: the wallpaper inside a base03 frame. The lock stops the animation
first and refuses to draw a second time over itself, and the suspend shortcuts lock the screen
*before* the machine sleeps, so nothing behind the lock is ever on screen.

The name is on purpose. **LazyOS** is the distribution, "lazy by design" is what it promises, and
the idea is the one the lazy tools share - lazygit, lazydocker: the tool does the boring part, and
the thing to press is on the screen.

## Where things live

| Path | Contents |
|---|---|
| `/etc/sway/` | the system half of the session: `config` plus `config.d/*.conf` drop-ins |
| `~/.config/sway/` | your half: it includes the system one and adds your own settings |
| `~/.config/bin/` | the commands the bar and the keys call (symlinks to `/usr/bin/lazyos` for ported surfaces, plus shell helpers) |
| `~/.config/{waybar,foot,rofi,mako}/` | the look of each piece |
| `/usr/share/doc/lazyos/` | these pages |
| `~/Dev/LazyOS/` | a **snapshot** of the repository this image was built from, with a `REVISION` file naming the commit. The directory is named after the repository: if it is ever renamed, the copy takes the new name |

That last one matters: the image carries the tested tree, and the scripts that
rewrite the kernel partition (`scripts/install-kpart.sh`) or back the system up
(`backup-system.sh`) are meant to be run *from it*, on the machine. If you see a
`REVISION` that does not match what you expect, the card or the install is older
than you think.

## Is everything healthy?

```sh
check-system.sh ~/Dev/LazyOS
```

It prints one line per check (`OK`, `WARN`, `FAIL`, `SKIP`), and every check maps
to a bug this port has already hit - a missing firmware, a key that a remapper
swallowed, the mixer's boot sequence, the backlight permissions, the clock. Run it
after a boot that behaved strangely, and keep the output when you ask for help.

## Power

* **Lock key** (top row, right-most): locks the screen, and systemd-logind
  suspends the machine a moment later. That path needs no polkit and no session
  state, which is why it always works. **Alt+Ctrl+L** locks without suspending
  (Omarchy's `Super+Ctrl+L`).
* **Alt+Escape** opens the same system panel as the battery module (Omarchy's
  `Super+Escape` system menu): lock, suspend, log out, reboot and power off.
* **Alt+Shift+Escape**: lock, then suspend. **Alt+Ctrl+Escape**: suspend without
  locking.
* The bar's battery module opens the power panel (charge, draw, time left,
  charger, uptime, memory and swap), and its **root menu** (**Alt+Space**) has log
  out, reboot and power off.
* Hibernate is offered only when a disk swap and `resume=` are both present; see
  **The system** page.
