# Software

Debian **forky** with a normal `apt`. There is one repository
(`deb.debian.org/debian forky main contrib non-free non-free-firmware`) and
`sudo` does not ask for a password.

```sh
sudo apt update                # refresh the package lists
sudo apt install <package>     # add something
sudo apt remove <package>      # remove it (add --purge for its configuration)
apt search <word>              # find packages
apt show <package>             # details, size, dependencies
sudo apt full-upgrade          # the whole system (see "The system" page: a new
                               # kernel also needs the boot partition rewritten)
```

Nothing about this machine depends on a package manager other than apt: no
Flatpak, no Snap, no AUR. If a program is not packaged for Debian, compiling it
in your home is the way (that is how the compositor got here), not a third-party
repository.

## What is installed, and where the lists are

The image is built from four package lists in the repository, copied on the
machine at `~/Dev/LazyOS/`:

| List | Holds |
|---|---|
| `packages/base.list` | what the system needs: systemd, NetworkManager, ssh, filesystems, audio plumbing, zram, the clock |
| `packages/tools.list` | the terminal tools (below) |
| `uis/sway/packages.list` | the session: sway, waybar, foot, rofi, mako, swayfx's dependencies, fonts |
| `devices/google-asurada/packages.list` | this hardware: the firmware, cgpt, vboot, depthcharge-tools, rsync |

Editing a list does not change the running machine; it changes the next image
that is built. For the machine in front of you, `apt install` is the answer.

## The terminal

**foot**, solarized, JetBrains Mono with a Nerd Font fallback for glyphs.

| Keys | Does |
|---|---|
| Ctrl+Shift+C / Ctrl+Shift+V | copy / paste |
| Ctrl+Shift+N | new window |
| Ctrl+Plus / Ctrl+Minus | text size |
| Ctrl+Shift+PageUp / PageDown | scrollback |
| Shift+PageUp | scrollback too |

### kitty, if you install it

`sudo apt install kitty` gives a second terminal that arrives already in this
session's palette: `~/.config/kitty/kitty.conf` ships with the image and carries
the same solarized colours as foot, JetBrains Mono 11 with the Nerd Font
fallback, 6 px of padding, 1000 lines of scrollback and 0.87 alpha, so the
compositor's blur shows through it exactly as it does through foot. Nothing here
starts it - `Alt+Return` and every menu open foot - so the file exists for one
reason: installing a terminal should not put a second, unthemed one beside the
first.

| Keys | Does |
|---|---|
| Ctrl+Shift+C / Ctrl+Shift+V | copy / paste |
| Ctrl+Shift+T | new tab (the tab bar appears with the second one) |
| Ctrl+Shift+N | new window |
| Ctrl+Shift+Z / Ctrl+Shift+X | previous / next shell prompt |
| Ctrl+Shift+Plus / Minus | text size (`Ctrl+Shift+Backspace` back to 11) |
| Ctrl+Shift+Up / Down | scrollback, one line |
| Alt+Q | close the window, which is sway's key and not kitty's |

Alt is the session's modifier and sway sees every Alt chord first, which is why
kitty keeps its own shortcuts on Ctrl+Shift. The one respect in which it differs
from foot: kitty draws JetBrains Mono's ligatures (`->`, `!=`), the same bytes
in a narrower column.

### The prompt

**starship**, in the shape of a powerline: one coloured block per question, left
to right - who you are, where you are, which branch and what the tree looks like,
which languages this directory has, which container, what time - then a second
line that is nothing but the character: cyan `❯` when the last command worked,
red `✗` when it did not. The blocks are solarized (base3 on orange for the user,
base02/base01 for time and containers, cyan for git, blue for the languages) and
the separators are the triangles `U+E0B0`/`U+E0B2`, which is the form that looks
the same in foot and in kitty.

Two things are worth knowing before editing `~/.config/starship.toml`:

- Most of what you see in the prompt is a *private-use* character, and a
  character no installed font has is a box - silently, in some terminals and not
  in others. This image ships the DFSG-stripped Nerd Fonts set, which has no
  Nerd Fonts v3 `nf-md` glyphs at all, so the OS logo the upstream prompt carried
  is not here (and neither are the distro logos). Every glyph the shipped prompt
  uses comes from JetBrains Mono, Symbols Nerd Font Mono, Font Awesome or
  Material Design Icons, all named in `~/.config/foot/foot.ini`.
- `check-system.sh` reads the prompt, asks fontconfig which font draws each
  codepoint and FAILs when the answer is "none", or when the only family that has
  it is not one foot is given (that one renders small, from fontconfig's
  fallback). Add a glyph, run the report: it names the codepoint.

`starship prompt` prints what the prompt would look like, and `starship preset
nerd-font-symbols` is a starting point if you would rather build another one.

The shell is bash with a set of aliases and tools that are worth knowing:

| Instead of | Use | Note |
|---|---|---|
| `ls` | **eza** | `ls`, `lsa`, `lt` (tree), `lta` |
| `cat` | **bat** | `cat file`, with colours; `bat file` pages it |
| `find` | **fd** | `fd name` |
| `grep` | **rg** | not aliased: `rg pattern` |
| `cd` | **zoxide** | `cd` still works for paths; `z name` jumps by frecency |
| `Ctrl+R` | **fzf** history | `Ctrl+T` files, `Alt+C` directories |
| `ff` / `eff` | fzf + bat | pick a file with a preview / edit the pick |
| `t` | tmux | attach to the `main` session or start it |
| the prompt | **starship** | user, directory, git, language, container, time |

Also there: `btop`, `lazygit`, `tmux`, `gum`, `tldr` (cached cheat sheets),
`fastfetch`, `inxi`, `tmux`, `yt-dlp` (with `ffmpeg` to merge streams), `node`
and `npm`, `tesseract` (OCR), `zbarimg` (QR/barcode), `qrencode`, `socat`,
`inotifywait`, `whois`, `unzip`, `jq`, `less`, `man` (pages go through bat).
`swappy` annotates a screenshot and `wf-recorder` records the screen, both
through the capture menu (Alt+Ctrl+C).

**`cd -` is bash's, not zoxide's.** `cd` is the wrapper `zd`, which keeps real
paths working and jumps by frecency otherwise; a first argument of `-` goes
straight to bash's own `builtin cd -`, so it returns to the previous directory
and prints it instead of asking zoxide for a match. The shell initialises zoxide
at the very end of `~/.bashrc`, after starship and after `~/.bash_aliases`:
starship replaces `PROMPT_COMMAND`, and zoxide's directory-tracking hook has to
be appended to it last - initialised earlier, the hook is dropped and the first
`cd -` prints "zoxide: detected a possible configuration issue".

`man <command>` and `tldr <command>` are the two fastest ways to learn a tool;
`tldr` needs one fetch the first time (`tldr --update`).

## Editors

**nano** is the editor that is always there (`nano file`).

**nvim** is what `$EDITOR` names, so `eff`, `git commit` and `crontab -e` open
Neovim: `nvim file` for one file, `nvim` for the empty buffer. It brings its own
Lua runtime and tree-sitter, and it starts with no configuration - plugins, a
language server and a theme live in `~/.config/nvim`, which the image does not
write. **vim** is the other one (`vim file`), and it is what makes `/usr/bin/vi`
exist: `vi file` is the same editor, reading `~/.vimrc`.

The variable comes from `/etc/profile.d/90-cli-tools.sh` (a login shell and this
terminal read it) and only when nothing has set it already, so an `EDITOR` you
export yourself wins.

## Emoji

**Alt+Ctrl+E** opens the emoji picker (the root menu and the launcher carry the
same row): type a name, Enter inserts the glyph at the cursor, Alt+1 only copies
it. The list is `~/.config/emojis.tsv`, and **every row is a glyph the shipped
font draws**: on the build host the table is filtered against the character map
of `fonts-noto-color-emoji`, so a symbol the font does not carry is never offered
instead of being inserted as an empty box. `~/.config/emojis.font` next to it
pins the SHA256 and the family of the font the filter used, which is how
`check-system.sh` notices a font upgrade; after one, re-running
`scripts/make-emojis.sh` on a host with `fontconfig` and the font rebuilds the
table. Nothing of this runs on the machine: the picker is one rofi window and the
table is a file.

## Working in the terminal all day

* `tmux` (alias `t`) keeps a session across terminal windows; `Ctrl+B` is its
  prefix.
* `ssh` keys: generate your own with `ssh-keygen -t ed25519`; the machine also
  runs `sshd`, so you can log in from another computer.
* `journalctl -b -p err` shows this boot's errors; `dmesg` the kernel's.
* `/usr/share/doc/<package>/` and `man` are local; `tldr` is the shortcut.
* The network's three questions are `ip -br addr`, `ping` and `ss -tunlp` (the
  first two packages are `iproute2` and `iputils-ping`, in the base list because
  no "is it up" answer starts without them).
* `zip` is the writing half of `unzip`, `strace` is the debugger when a program
  fails without a message, and `ncdu` is the disk walk to reach for when this
  58 GB eMMC fills (`du` prints the same numbers as a wall of text).

## The browser

**firefox-esr** is the one application outside the session that this image
installs on purpose (280 MB on disk, none of it resident while it is closed). It
is the browser, and it is also the last resort for a file: a downloaded page, an
EPUB, a `.deb`, or anything whose own viewer is missing.

* **Files** (Alt+Ctrl+F) hands it an **EPUB** or a `.deb`, the formats nothing
  else here opens. A PDF, a DjVu page or a comic goes to **papers** (the reader),
  a picture to **imv**, a video to **mpv** and an HTML page to `bat`, because an
  HTML page *is* text - each of those is cheaper than a browser window (see
  below). A **URL** is the browser's job, which is what `xdg-open` below is
  about.
* `xdg-open` exists (`/usr/local/bin/xdg-open`, two dozen lines): a URL goes to
  Firefox, and a path goes to the same rule table a double click in favnyr uses
  (`lazyos open`, linked as `lazyos-view`), so a file opened from a script lands where it would land from
  the desktop. That is what foot's ctrl+click and the command-line tools that
  print a URL call - there is no desktop portal in this image.
* It is a normal Wayland client here (no Xwayland needed), and its icon is its
  own: the launcher shows Firefox's icon, not a substitute.
* **Alt+Shift+Return** opens it from anywhere - the one program that earns a key
  of its own (Omarchy has none; its launcher is the way there).

## Playing a video

**mpv** is the video player, and the only program the file browser needs for
`video`: **Files** (Alt+Ctrl+F) starts it on the file you chose and leaves it
running after the list closes. It seeks, shows subtitles, follows the screen's
refresh rate and takes the keyboard while its window has the focus - `q` quits,
`space` pauses, the arrow keys seek, `f` goes fullscreen.

It is deliberately plain: the file list starts it with `--no-config`, so nothing
in `~/.config/mpv` (a shader, a GUI driver, a profile copied from another
machine) changes what a click does - write that file if you want to, but the
session's behaviour does not depend on it. A still picture does not start mpv:
**imv** draws it on the GPU (zoom, pan, rotate, and a slideshow of the folder),
and `chafa`'s sixels in a foot window are the answer when imv cannot get an EGL
context - both cost a fraction of a player, and a picture looks like a picture
rather than like a video that has not started.

**The first picture comes with its keys.** imv has no on-screen help, so the
first picture of a session raises one notification with the bindings its
`/etc/imv_config` defines: `i` and `o` zoom, `h` `j` `k` `l` pan, **Ctrl+R**
rotates, **Left** and **Right** walk the folder, `f` is fullscreen and `q` quits.
It is one card per session (the state file is
`$XDG_RUNTIME_DIR/lazyos-imv-hint`), it appears only when a picture actually
opens in imv - not for text, a document or a video - and it is queued in the
background, so it never delays the picture.

**A film keeps the screen awake.** mpv asks the compositor for an idle inhibitor
while its window is visible, so the idle screen and the lock do not arrive in the
middle of one. Firefox and the document reader ask the same thing through their
own interface and get it from the compositor (see the next section); the bar's
coffee cup is the manual switch for everything else - a long download, a recipe,
a page you are reading from across the room.

## The camera

**The camera works because the kernel drives it, not because something was
installed for it.** It is a USB device on the internal hub (`lsusb`:
`0408:30d5 Quanta USB2.0 HD UVC WebCam`, at `1-1.4`), and `uvcvideo` - a module
of this kernel - turns it into `/dev/video0`. `/dev/video2` and `/dev/video3`
are the SoC's hardware video encoder and decoder, not cameras. This device tree
carries no MIPI sensor, so there is no second (rear) camera to open: a machine
with one would need a kernel and an image pipeline, not a package.

**To open it: Camera, in the root menu (Alt+Space) and the launcher.** It runs
`~/.config/bin/lazyos-camera`, which finds the node bound to `uvcvideo` - by
driver, because the numbering is what a replug reorders - sends a notification
when there is no camera at all, and starts **guvcview**: the preview, the
camera's own controls, and two capture buttons. **Alt+Q** closes the window
(sway's kill; guvcview's own quit is the **Quit** button). The device opens
exclusively, so pressing **Camera** again while it is open brings that window
back instead of starting a second one.

**The controls are half of what it is for.** The preview and **Image Controls**
are two windows: exposure, gain, brightness, contrast, white balance and
sharpness come from the camera itself through UVC, and a camera that looks black
in a dim room is usually exposure, not a broken driver. Auto exposure takes a
moment to settle, so the first frame after the window appears is darker than the
second one - that is the sensor ramping, not a fault. **Cap. Image (I)** writes
`~/Pictures/Camera/photo-N.jpg`, **Cap. Video (V)** writes
`~/Videos/camera-N.mkv`; guvcview continues that numbering across runs, so
nothing is overwritten, and a clip carries the internal microphone.

**Without the window**, from a terminal:
`ffmpeg -f v4l2 -i /dev/video0 -frames:v 1 /tmp/cam.png` captures one frame, and
`ffmpeg -f v4l2 -list_formats all -i /dev/video0` lists what this camera offers
(MJPEG and YUYV, from 1280x720 down to 160x120). That is the test that separates
"the camera is broken" from "the window is broken", and it is the same device
`check-system.sh` reports on.

**A browser reads the camera directly.** A video call in Firefox needs none of
the above: Firefox asks the kernel for the device and prompts for permission in
its own window. **The camera *portal* is what this image does not have** -
`org.freedesktop.portal.Camera` needs a portal backend that implements it, the
only backend here is the wlr one (Screenshot and ScreenCast), and
`xdg-desktop-portal` therefore logs *The camera portal requires an access impl*
once per session. Native programs are unaffected; a Flatpak application, which
may only reach a camera through that portal, sees none at all. That is a stated
limit rather than a reason to run one more daemon.

## An external monitor

**Both USB-C ports carry DisplayPort, and the display is the DTB plus one
udev one-shot.** This port's device tree described no external-display pipeline
at all: `dpi@14016000` was disabled and the second ANX7625 bridge - the one on
`i2c@11d00000`, not the internal panel's - was missing, so the kernel created a
single connector and there was nowhere for a monitor to go. The patch
`kernels/mediatek-mt81/patches/mt8192-asurada-external-display.patch` describes
them (the DPI output with its pins, the bridge with its rails, its power and
reset GPIOs and its DPI input), and that is what makes `card1-DP-1` exist.

**The chip has one DisplayPort transmitter and two Type-C ports**, and which one
it reaches is a crosspoint (`TCPC_SWITCH_0/1`, 7-bit address `0x2c` on
`i2c-7`): `0x21` is the left port, `0x12` the right one, and the chip powers up
pointing at the left one. ChromeOS sets those registers from a typec-switch mux;
mainline's driver has no such code, so a monitor on the right port reports HPD,
answers EDID and gets a mode - and shows its no-signal test pattern, because the
lanes never reach it. The fix is not in the session: `/usr/local/sbin/lazyos-dp-mux`,
run by `/etc/udev/rules.d/90-lazyos-dp-mux.rules` when a port enters DisplayPort
alt mode, writes the two registers with `i2cset` (package `i2c-tools`). It has to
happen before the compositor enables the output, which is why it is a udev rule
and not a login hook: the Type-C event arrives about a second before the
connector's hotplug. Nothing is resident - one process while the plug is handled.

**A monitor can be plugged in at any time and into either port.** That is sway's
own output handling: the new connector gets a workspace and windows go there
with `Alt+Shift+<arrow>`; unplugging takes its workspaces back to the internal
panel. The chip drives one DisplayPort stream, so two monitors are one monitor
followed by the other, not two at once.

**Where it sits is Settings → Display arrangement** (`Alt+Ctrl+O`,
`~/.config/bin/rofi-display`): monitor on the right, on the left, above, below,
mirrored, plus the monitor's resolution and scale, and **Open wdisplays** for
what those rows do not cover (a custom position, a rotation) - that window opens
on a workspace of its own, because its two rectangles need the width. The
Settings row's detail is the menu's own `--status`, so the two cannot disagree.

**A position given at runtime is not remembered, and the menu is what remembers
it.** sway keeps `output position` until the session ends, so each arrangement is
also written to `~/.config/sway/config.d/40-display.conf`, which the session's
`~/.config/sway/config` includes: the monitor's resolution, scale and position,
and nothing else. The panel is never moved - the monitor is placed relative to
it, which is why "on the left" is a negative x (`swaymsg -- output DP-1 position
-1920 0`; the `--` is not optional, or swaymsg reads `-1920` as one of its own
flags). Only the monitor is written because it is the output that can be absent,
so a boot without it is untouched. **Only the monitor** and **Only the internal
panel** are deliberately not saved - a config that disables a screen would boot a
card with nothing on it - so they last until the session ends and the menu says
how to undo them. Delete the file to go back to sway's default (on the right).

**If the panel is the screen that is off, there are two ways back.**
"Only the monitor" turns the internal panel off for the session, so if the cable
then comes out - or the monitor loses its link - there is no picture at all.
**`Alt+Ctrl+Shift+D`** turns the panel back on and needs no picture to do it: it
runs `~/.config/bin/rofi-display --panel-on`, which is the reason the key exists,
because a menu cannot be drawn when every output is off - and sway allows
turning the last one off. When a screen *can* still draw (the monitor is present
but disabled, or the panel is off and the monitor is on), `Alt+Ctrl+O` →
**Display arrangement** carries the same row, **Bring the internal panel back**,
and the menu offers it even with no monitor plugged in. Over ssh,
`swaymsg output eDP-1 enable` is the same command, and a reboot clears it too,
because the choice is never written to the saved arrangement.

**The night light does not reach it.** `wlsunset` asks the compositor for a
gamma table per output, and this kernel has one only for the internal panel's
CRTC: `modetest -M mediatek -p` shows `GAMMA_LUT` on the 1366x912 CRTC and
nothing on the 1920x1080 one, because MT8192's second display path
(OVL1 → RDMA1 → DPI) has no gamma block bound to it in mainline, and the SoC's
one gamma block sits in the first path. `wlsunset -t 3000` says so itself:
*gamma control of output ... failed*. Use the monitor's own colour setting; a
warm external display would need the driver to carry a gamma block on that CRTC.

**A suspend resets the bridge's crosspoint, and the session puts it back.** The
bridge powers down with the rest of the display pipeline and re-initialises on
the way back, so the crosspoint returns to its power-on default (the left port)
- measured on this machine: `0x12` before the sleep, `0x00` and then `0x21`
after it - while the kernel's Type-C stack only re-detects the partner. The
connector still says `connected` with its EDID and sway still has the mode, so
the picture goes to the empty port and the panel shows its no-signal pattern
until the cable is replugged. `~/.config/bin/lazyos-dp-resume` runs from the
idle chain's `after-resume` and does the two things that fix it: it writes the
crosspoint for the port that has DisplayPort alt mode, then forces one modeset
(1280x720 and back), because a crosspoint write alone changes nothing - the link
was already trained when it ran. The mode change is deliberate: disabling the
output would move that output's workspaces to the internal panel, and a resume
hook should not rearrange the desktop. Nothing is resident, and
`journalctl -t lazyos-dp-resume` says what it did.

## Files in a window

**Alt+Ctrl+Shift+F** opens **favnyr**: the files **Alt+Search** and **Alt+Ctrl+F**
list, with a mouse. It is a second entry and not a replacement, because the two
answer different questions - a list is one keystroke and no window, a window is
what drag and drop, several files at once, renaming in place and the trash need.
The launcher (**Alt+D**) has both rows, **Files** and **Files (window)**, the
root menu (**Alt+Space**) lists them the same way, and the window is a normal
one: tile it, move it to another workspace, close it with **Alt+Q**.

* **A double click opens what the Files menu opens.** `/etc/xdg/mimeapps.list`
  decides, and it names the same programs the list does: a document (PDF, DjVu, a
  comic) goes to `papers`, a directory to favnyr, a page or a `.deb` to Firefox,
  and text, pictures and video to `org.lazyos.view.desktop` ->
  `~/.config/bin/lazyos-view` (`lazyos open`), the one table the file list follows too (text
  in `bat`, a picture in `imv` - sixels in a foot window when imv cannot start - a
  video in `mpv`, an EPUB in Firefox; the list reads an HTML file as text where a
  double click opens the page). There is no desktop portal in this image, so those
  entries *are* the "open with" mechanism - for favnyr, for `gio open` and for
  any application that asks to open something.
* **A terminal from inside it is foot** (`Shift+F4`, "Open terminal here"): the
  window runs `$TERMINAL` and then a built-in list of emulators that does not
  include foot, so the image exports `TERMINAL=foot` in `/etc/environment` -
  `pam_env` reads that file for a session started by the compositor, where
  `/etc/profile.d` is never read. Without it the menu item answers *no terminal
  detected* and starts nothing.
* **A disk it cannot unmount says why.** The removal action goes through
  `udisksctl`, and when udisks refuses because something is using the mount, the
  window asks `fuser` which process it is and names it (*In use by: …*). Both
  come from packages the image installs (`udisks2`, `psmisc`), so the message
  arrives with the answer instead of only with the refusal.
* **The trash works** (`gvfs` provides `gvfsd-trash`), so a deleted file lands in
  `~/.local/share/Trash` and can be restored. SMB and phone mounts do not,
  because `gvfs-backends` (31 MB of `samba-libs`) is deliberately not installed
  and the Network places are off; a USB stick appears in the sidebar once
  `udisks2` mounts it.
* **Its colours are the session's, and not through GTK.** favnyr is one Slint
  binary that draws its own widgets - it links no GTK - so the window follows the
  palette by itself, and what it takes from the GTK files is the **icon theme**:
  it reads `gtk-icon-theme-name=` from `~/.config/gtk-3.0/settings.ini` and asks
  that theme for the folder, trash and drive names it draws, which is why the
  **LazyOS** theme carries them (`scripts/make-icons.sh`'s fourth table). The
  `gtk-3.0/gtk.css` next to it paints GTK3 programs instead - the camera window
  (`guvcview`), waybar, and anything a person installs later.
* **The document reader gets the same palette, from a different pair of files.**
  `papers` is GTK4/libadwaita, which does not read `gtk-3.0/` at all: it asks the
  *settings portal* for the desktop's colour scheme, and this image runs no
  Settings portal (the only backend is the wlr one, for screenshots and screen
  casting). It therefore opened in libadwaita's light default, a white rectangle
  in a base03 session. `~/.config/gtk-4.0/settings.ini` states the dark
  preference and `~/.config/gtk-4.0/gtk.css` paints the libadwaita named colours
  in the same solarized values as the GTK3 file next door. Any other GTK4 or
  libadwaita application you install inherits it, and no portal daemon is
  installed for one boolean.
* **A document can be read, searched, zoomed and annotated; printing needs a
  print service.** "Print" in `papers` answers that it cannot find one, because
  the image ships no `cups` daemon (it is a system service, not a viewer). If you
  print, install it (`sudo apt install cups`) and configure a queue once.

## USB sticks and SD cards

A USB stick **mounts itself when you plug it in**: a notification says *label
mounted at /run/media/<you>/<label>*, and it is then a row in **Places** (the
root menu, Alt+Space, and the Places entry in the launcher) and in favnyr's
sidebar. **Alt+1** on a mounted disk in Places unmounts it; eject before pulling
the stick out - a filesystem that is still mounted when it disappears is how one
gets corrupted. An SD card in the internal slot is a **Mount** row instead; the
list below says why.

* **Auto-mount is one udev event, not a daemon.** Plugging in a stick makes udev
  start `lazyos-automount@<partition>.service`, which runs
  `/usr/local/sbin/lazyos-automount` once and exits. There is no `udiskie`: that
  is a resident Python daemon (and a tray icon this bar has no host for), and an
  on-demand `udisksctl` is the same result without it. Nothing runs between two
  plugs. A stick that was **already inserted when the machine booted** waits in
  Places instead: the event fires before anyone is logged in, and the helper has
  no account to mount for (the same rule that keeps it out of the root
  filesystem), so unplug it and plug it back in - or pick the Mount row.
* **It mounts as you, not as root**, because udisks2 writes the *caller's* uid
  and gid into the mount options of a vfat or exFAT filesystem: a root mount is
  `/run/media/root/<label>` with `uid=0`, a stick you cannot write to. The helper
  drops to your account with `setpriv` (no PAM, no logind session, no second
  login), and a polkit rule
  (`/etc/polkit-1/rules.d/49-lazyos-automount.rules`) authorizes that
  session-less caller through the `plugdev` group - which is why the account is
  created with it.
* **Only USB disks.** The rule matches the USB transport, so the eMMC and the
  internal microSD slot are never touched: they are the two disks this machine
  boots and rescues from, and `mmcblk1` is where the rescue card lives. A card in
  that slot is a **Mount** row in Places, one click - also the honest amount of
  ceremony for a card you may be about to repartition.
* **Settings → Auto-mount USB drives** turns it off: one line, `enabled=no`, in
  `~/.config/lazyos/automount.conf`, and a missing file means on. With it off a
  stick waits as a **Mount** row, which is what an image without this feature
  did.
* A dirty vfat or exFAT stick mounts read-only and the notification says
  `(read-only)`: udisks does that rather than refusing it, and a stick nobody can
  write to looks broken until you know why. Unplug it, or run `sudo fsck.vfat -a
  /dev/sdX1` (with nothing mounted from it) and plug it back in.
* Mounting is **udisks2**'s job, because an ordinary session cannot mount
  anything itself (that is a root operation). It is D-Bus activated: nothing of
  it runs until a mount is asked for, and nothing stays afterwards.
* The same disks are in favnyr's sidebar (**Alt+Ctrl+Shift+F**), and a stick shows
  up there as soon as it is mounted.
* **Unmounting from the window goes through `udisksctl`, like the Places menu.**
  When udisks refuses because something still has the disk open, favnyr runs
  `fuser -m` to find out what and says so (*In use by: python3, mousepad*) - so
  `psmisc` is in `uis/sway/packages.list` and the refusal arrives with a reason.
  `eject` is not involved: it was pcmanfm's dependency and no program in this
  image execs it any more.
* `vfat`, `exFAT` and `NTFS` all work - the kernel has those drivers - so a
  stick from a camera or from Windows needs no extra package.
* **Auxiliary swap** is the row above it: it takes a card that is not a system card
  and turns the whole thing - or 2, 4 or 8 GiB of it - into a swap partition
  labelled `lazyos_swap`, adds the `/etc/fstab` line that activates it at every
  boot, and turns it on now. "Not a system card" is the list: nothing mounted on
  it, no bootable kernel partition, and not the disk this system runs from. The
  card that installed the eMMC has one of those kernel partitions, so with it
  attached the row answers *Nothing to offer* - that is the guard, not a fault,
  and the same sentence names the reason. Repurposing such a card is a deliberate
  root action (`sudo wipefs -a /dev/mmcblk1`, which erases the rescue system too),
  never a menu row. The priority is 10 against zram's 100, so the
  compressed RAM is used first and the card is the spillover; `nofail` plus a 15 s
  device timeout keep a boot without the card from waiting or failing. It is a
  dedicated swap card, not a filesystem, so it never appears as a Mount row.
  Pulling it while it is active loses whatever pages live there: open the same row
  and choose **Turn it off** first (`swapoff`, then the fstab entry is removed; the
  partition stays, and **Format a disk** erases it). `resume=LABEL=lazyos_swap` is
  the extra piece hibernate needs - see **The system** page.
* **Format a disk** is the last row of Places: it offers only disks with nothing
  mounted on them, no ChromeOS kernel partition, and that are neither built into
  the machine nor the disk this system is running from - so the eMMC and the card
  this system boots from are never candidates, even after a partition table is
  wiped - and it asks twice with Cancel first. The filesystem is exFAT (`LAZYOS`,
  for a camera, a TV or another computer: Windows, macOS and Linux all read it,
  and unlike FAT32 it has no 4 GB file limit) or ext4 (for a Linux machine), and
  the whole disk becomes one partition. The internal eMMC is only written by
  `scripts/install-emmc.sh`; this menu is for sticks and cards. It is not
  stoppable: a killed `mkfs` leaves a disk that looks formatted and is not.
* A failed mount says why in the notification, in udisks' own words: "not
  authorized" is a policy problem, "already mounted" means it is already in the
  list above.

## Your profile, on a stick

**Alt+Ctrl+D** - also the root menu's **Developer** row and the launcher entry -
opens three actions: export your profile to a disk, restore an exported profile,
and save a diagnostics bundle. They are about the *account*, not the machine: the
keys, the dotfiles and the configuration the tools in this image read at login,
in one archive you can carry to another computer.

**Export my profile** asks for a disk (a mounted one, a partition it can mount
with udisks - the same mechanism as Places - or `~/Downloads` when no stick is
in), then for a passphrase, twice. The prompt is a masked **rofi** window when
the menu is opened from the session, and the terminal's own when the command is
run from a shell or over ssh. It writes
`lazyos-profile-<host>-<date>.tar.zst.gpg` and a `.txt` manifest beside it.

* **It is encrypted, and it has to be**: the archive carries `~/.ssh` (private
  keys), `~/.gnupg` (secret keys) and the Firefox password store
  (`logins.db` + `key4.db`), and a stick that is lost must not be an identity
  that is lost. The passphrase is typed into rofi, kept in a private
  staging file and handed to `gpg` through a file descriptor, so it never
  appears on a command line; the plaintext tar is removed on every exit path,
  including a stop from the bar.
* **It is a tar, not a copy of the files**, because the disks this menu is
  pointed at are usually exFAT (that is what **Format a disk** writes): exFAT
  has no permission bits and no symlinks, so a plain copy would leave
  `~/.ssh/id_ed25519` with no `0600`, and ssh refuses a key like that. The modes
  travel inside the tar, where the filesystem underneath cannot drop them.
* **What is inside**: `~/.ssh`, `~/.gnupg`, the shell files (`.bashrc`,
  `.bash_aliases`, `.profile`, `.bash_logout`), `.gitconfig`, the configuration
  of the tools this image ships (`foot`, `kitty`, `nvim`, `vim`,
  `starship.toml`, `glow`, `sway` drop-ins, `mako`, `rofi`, `gtk-3.0`,
  `gtk-4.0`, `lazyos`, `favnyr`, `mpv`, `yt-dlp`, `go`, `pip`, `cargo`,
  `rustup`, `mimeapps.list`, `user-dirs.dirs`), `~/.local/bin`, the user's own
  applications, and the **agents**: the account directories of the Agents menu
  (`~/.local/share/lazyos/agents`, sign-ins included), OpenCode's configuration
  (`~/.config/opencode`) and the CLIs' own homes - `~/.claude`, `~/.codex`,
  `~/.gemini`, `~/.copilot` and `~/.config/github-copilot` - with their
  credentials and settings; and the **Firefox profile** (`~/.config/mozilla`,
  the XDG path this image's Firefox uses, or `~/.mozilla` with
  `MOZ_LEGACY_HOME=1`): bookmarks and history in `places.sqlite`, passwords in
  `logins.db` with `key4.db`, the certificate and permission overrides, cookies,
  the add-ons in `extensions/`, the daily backup in `bookmarkbackups/` and
  `prefs.js`.
* **What is deliberately not**: caches, the Firefox *site data* - `storage/`
  (202 MB of IndexedDB and localStorage here), `security_state`,
  `sessionstore-backups`, `gmp-*`, the SQLite `-wal`/`-shm` checkpoints (a copy
  of a live profile is the database at its last checkpoint, so close Firefox
  first if the last few seconds matter) and `~/.cache/mozilla` (468 MB, never in
  the archive) - `node_modules`, the
  code in `~/Dev` (git remotes are its backup), the conversation transcripts and
  caches inside the agents' homes (`~/.claude/projects`, `~/.codex/sessions`,
  `~/.copilot/session-state`, `plugins/repos`…: history, not configuration), and
  `~/.local/share/opencode/opencode.db` - OpenCode keeps its sign-in there and on
  this machine it is 49 MB of session history, so sign in again on the machine
  that restored the profile; `~/.local/share/opencode/auth.json` travels when a
  layout keeps the credentials outside the database.
* The **manifest** (`...txt`, plaintext, no secrets) names the date, the host,
  the image revision, the paths, `apt-mark showmanual` and `npm ls -g`, so a
  stick can be read without the passphrase.
* From a terminal - including over ssh, which this machine answers - the same
  work is one command:
  `~/.config/bin/lazyos-profile export /run/media/$USER/STICK`. It asks for the
  passphrase on the terminal, and `--passphrase-file FILE` makes it scriptable.

**Restore a profile** lists every archive it can see (mounted disks and
`~/Downloads`), asks for the passphrase, and says how many files it is about to
overwrite - **Cancel is the first row**. Before extracting anything it writes
the current profile to
`~/.cache/lazyos/backups/profile-before-restore-<date>.tar.zst`, so a restore
that turns out to be the wrong archive is itself undoable. It then extracts into
your home with the modes from the archive and says to log out and back in: the
shell, the session and the agents read their configuration at login. It cannot
be stopped once it starts - a killed extraction is a half-restored home.
By hand the archive is ordinary gpg and tar:

```sh
gpg -d lazyos-profile-20261023-1200.tar.zst.gpg | tar --zstd -x -C ~
```

**Save a diagnostics bundle** writes `lazyos-report-<host>-<date>.tar.zst` with
the output of `check-system.sh`, the last 3000 lines of this boot's journal, the
kernel ring buffer and one page of hardware and package facts. It is not
encrypted because it holds no keys; the journal does name Wi-Fi networks, paths
and errors, which is why the README inside says to read it before posting it. It
is the file to attach to a bug report - the same report `docs/test-checklist.md`
asks for after a fresh card.

## The night light

The session starts `wlsunset` at login: from sunset it warms the screen to
4000 K, and by day it leaves it alone. **Settings → Night light** (`Alt+Ctrl+O`)
turns it off and on by hand; turning it on forces the warm end immediately
instead of waiting for sunset, because a switch that does nothing at 17:00 reads
as a broken switch. The command line and the remembered mode live in
`~/.config/bin/night-light` (`login`, `start`, `stop`, `resume`, `status`), and
the mode is one line in `$XDG_RUNTIME_DIR/lazyos-night-light`, so it dies with
the session.

Suspend and resume are the part that used to lose it: the panel comes back with
its own gamma table, so a running `wlsunset` no longer has any effect and the
screen wakes up at daylight colour while the switch still says on. The idle
drop-in's `after-resume` hook asks the helper to restart the mode that was
running (`night-light resume`), and it does nothing when the night light was off
before the sleep. `night-light status` prints `on schedule`, `on warm` or `off`
from a terminal.

**An external monitor stays at its own colour temperature**: the kernel has a
gamma table only for the internal panel's CRTC, so the warm ramp cannot reach
it - see § *An external monitor*.

## Themes

The colours of this session come from **one file**, not from a dozen edited
configs: `theme.toml` holds the 16 terminal colours and the roles a surface asks
for (background, text, accent, danger…), and every other file is *generated*
from it — foot, waybar, mako, rofi, btop, lazygit, nvim, fzf, the lock screen,
the prompt (starship) and the wallpaper and window frame sway uses.

```
lazyos theme list           # every theme, the active one marked *
lazyos theme set gruvbox    # regenerate everything and reload the session
lazyos theme next           # the next one in the cycle      (Alt+Shift+T)
lazyos theme wallpaper next # the next wallpaper of a theme  (Alt+Shift+W)
lazyos theme validate       # parse every built-in and check its contrast
lazyos theme diff           # is every file still what the theme says?
```

Solarized Dark is the default and the brand; the others are Catppuccin Mocha,
Tokyo Night, Rose Pine, Nord, Gruvbox, Everforest Dark and Kanagawa.

**A generated file says so on its first line.** If you edit one of them, the
next switch will *report* the difference and leave your edit alone instead of
silently replacing it: `lazyos theme set NAME` prints the differing lines and
exits 1, and `--force` replaces them. To keep a change, put it in the template
(`/usr/share/lazyos/themed/` or `~/.config/lazyos/themed/`) — that is what the
generator reads.

Two things a switch does not do. **A terminal that is already open keeps its
palette**, because foot reads its configuration when a window is created: open a
new window and it has the new colours. And a theme's own wallpaper art arrives
with the release that ships it; until then every theme names the same picture.

`lazyos theme set NAME --headless` writes the files and asks nothing to reload,
which is what to use over ssh. The last generated body is kept in
`~/.local/state/lazyos/theme/`, which is how "the template moved on" is told from
"a person edited this file".

## Translate

**Alt+Ctrl+T** translates **the text you selected with the mouse** - in the
terminal, in the browser, in any window - or, when nothing is selected, what is
on the clipboard. Nothing is opened first: the selection *is* the question, and
the key answers it in place.

The source language is always detected; everything goes into the language
**Settings → Translate** sets. Until that is set it is **English**, so an image
that was never configured answers the common case with one key. The Settings row
also says which language is in use, and the menu it opens lists the twenty-six
languages both engines take with the how-to on the same screen
(`~/.config/bin/translate config` from a terminal). The setting is one line in
`~/.config/lazyos/translate.conf`:

    target=es

The translation is **copied to the clipboard** and shown in a card that names
the direction it went (`Spanish → English`). When it is longer than a few lines
the card is only the summary: a read window opens with the whole translation and
the selection underneath it, paged with `bat`, and Alt+Q closes it. A selection
cut at 3000 characters says so on the card. English into English is the one
direction that answers something already known, so that case says **Already
English** instead of echoing the text and points at Settings.

**The network** decides the rest. Google's JSON endpoint is asked first
(`clients5` with `client=dict-chrome-ex` - the same answer that detects the
language), and MyMemory without a key is the fallback, in 450-character pieces
because 500 is its ceiling. When neither answers, the card says so - *No answer
from the translator: is the network up?* - instead of showing half a
translation. The request is one `curl`; a `busy` card and the bar's spinner say
it is running, and **a click on the spinner stops it**.

Nothing is stored by this feature: no history file, no cache. The result is on
the clipboard, which means it is in the clipboard history (`Alt+Ctrl+V`) like
anything else copied, and the card is in mako's history - so a secret translated
is a secret in both, and the clipboard menu's last row clears that history.

`translate-shell` is deliberately not installed. It would have been one package
(277 kB), but its engine is Google's *other* endpoint, which answers this
machine with a captcha page; the package would be a command that never works
here. `check-system.sh` reports the tools this script does need (`curl`, `jq`,
`wl-paste`) and SKIPs the network probe when the machine is offline.

## A local model

**Alt+Ctrl+M** opens a model that runs on this machine: **Qwen3-1.7B**, four
cores, **no network and no account**. The window is an ordinary chat - type a
question, Enter sends it, Ctrl+C stops a long answer - and **Alt+Q** closes it.
It is also the root menu's **Local model** row, the launcher entry of the same
name, and `lazyos-model` in a terminal (with several models it asks which one
with `fzf`).

The weights are **not in the image**: they are 1.2 GB each, and shipping two of
them would be a third of the disk. `lazyos-model --fetch` downloads the
recommended one and checks it against a pinned SHA256 before keeping it
(`~/.config/lazyos/models.sha256`), the way `dictate-model` fetches whisper's;
`--verify` re-checks the files that are already there. They land in
`~/.local/share/lazyos/models/`, so a rebuilt image keeps them. From the launcher
the same command runs in the model window, with the progress bar on screen.

Three measured numbers are the whole configuration, and the wrapper passes them
so no invocation has to remember them (`lazyos-model --flags` prints them,
`--bench` re-measures `-t 2/4/8` on the machine; the full table is
`~/.local/share/lazyos/models/BENCH.md`):

- **four threads, not eight.** llama.cpp asks the kernel for the core count and
  uses all of it: that measured **6.67 tokens/s**, because the four A55 cores are
  seven times slower each and drag every barrier. The A76 cluster alone measured
  **11.19**. `taskset` keeps the desktop off those four cores too, which is why
  the window that asks the question stays responsive.
- **Q4_0 rather than Q4_K_M.** The smaller quant pre-fills a prompt at **84.7
  tokens/s against 46.2**, so a 1800-token question answers in **31 s** instead
  of 49 s. Q4_K_M is a slightly better model and a noticeably slower one.
- **a q8_0 KV cache behind flash attention**: 4k of context peaks at **2356 MiB**
  instead of 2584, which is the difference between fitting next to the session and
  swapping.

Qwen3 thinks before it answers, and a thinking block is nearly a minute of
silence at eleven tokens a second, so **reasoning is off** by default; the
comment at `REASONING=` in `~/.config/bin/lazyos-model` is the switch for
questions that deserve the wait.

What this machine will not do, so the window does not pretend: the Mali-G57 has
no compute path here (Panfrost is OpenGL 3.1, PanVK answers *Mali-G57 not
supported*) and there is no NPU, so nothing is offloaded to the GPU; and with no
`i8mm` the IQ-quants are out. A 3B model does not fit in 4 GB beside a browser.
Eleven tokens a second is a paragraph in about ten seconds - fast enough to ask a
question, too slow to read a page.

## Text in other languages


`fonts-noto-core` is installed, so Arabic, Hebrew, Devanagari, Thai, Armenian,
Georgian and the rest of Noto's non-CJK families have a face: a file name, a web
page or an EPUB in one of those scripts is text instead of a row of boxes, in the
terminal and in the browser alike. Nothing has to be configured - fontconfig's
fallback finds them.

Chinese, Japanese and Korean are **not** installed. `fonts-noto-cjk` is 91 MB
installed, the largest single item left after the browser, and a card that only
needs Latin should not pay for it. If you read those scripts:

```sh
sudo apt install fonts-noto-cjk
```

No logout is needed; fontconfig picks the new faces up on the next lookup, and
`fc-cache -f` forces it if an application cached the miss. `fc-list :lang=ja`
answers whether a Japanese face is there, and `check-system.sh` reports the
non-Latin case with `fc-list :lang=ar`.

## What is deliberately not installed

A GUI editor, an office suite and any Electron-style application. The image is
2.9 GB and every one of those would add hundreds of megabytes and a resident
process to a machine with 4 GB of RAM. If you want one, `apt search` it - the
session is a normal Wayland desktop and any GTK/Qt application works. The file
manager above is the one GUI exception, because it does two things a list
cannot; it costs 4.9 MB and starts no daemon.
