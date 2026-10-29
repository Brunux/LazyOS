# rofi: the launcher, the window list and every menu

One binary, a handful of jobs. It exists while its window is on screen and
nothing of it stays afterwards (13-17 MB of memory while open; there is no
daemon).

| You press | What opens | What it lists |
|---|---|---|
| **Alt+D** | `rofi -show drun` | the applications: the `.desktop` entries, including this image's own (LazyOS Menu, Settings, Wi-Fi, Bluetooth, audio, media, notifications, power, Files, Camera, Learn, Keys, Dictate, Agents…) |
| **Alt+Tab** | `rofi -show window` | the open windows (sway has no switcher of its own; this uses wlroots' foreign-toplevel protocol) |
| **Alt+Shift+D** | `rofi -show run` | every executable in `$PATH`. **Enter** runs it, **Shift+Enter** runs it in foot |
| **Alt+Space** | `lazyos menu` (`bar-menu`) | the root menu: what opens something, the AI rows (Ask among them), the system panels, and the session actions - type to narrow it |
| **Alt+Ctrl+K** | `rofi-keys` | every key binding this session defines, parsed from `/etc/sway` and `~/.config/sway`. Enter copies the command |
| **Alt+Ctrl+F** | `rofi-files` | the filesystem: directories first, sizes and dates dimmed. Enter opens a directory or reads a text file in foot; the dotfiles are one row away |
| **Alt+Ctrl+G** | `rofi-grep` | **search inside the files**: ripgrep over your home, one row per match; Enter opens the file at that line |
| **Alt+Ctrl+D** | `rofi-dev` | **developer**: export your profile (keys, configs, package list) into an encrypted archive on a disk, restore one, or save a diagnostics bundle |
| **Download** (root menu, launcher) | `rofi-download` | a link - or the one in the clipboard - kept in `~/Downloads`, as a video or as audio |
| **Alt+Ctrl+C** | `rofi-capture` | **capture**: a screenshot (screen, area or window), annotate it, read the text or a QR code out of a region, and start or stop a recording |
| **Alt+Ctrl+O** | `rofi-quick` | **Settings**: the switches (Wi-Fi and Bluetooth radios, night light, do-not-disturb, touchpad, tablet mode, rotation, display arrangement, screen sharing, auto-mount of USB drives) and the panels, in one list |
| **Alt+Ctrl+P** or **Alt+Escape** | `rofi-power` | **power**: lock, suspend, hibernate, log out, reboot, shut down - the last three ask once more |
| **Alt+Ctrl+N** | `rofi-notify` | the notification history, with do-not-disturb and "copy this notification" |
| **Alt+Ctrl+Q** | `rofi-calc` | a calculator: type `(3*4)/2`, Enter copies `6` |
| **Alt+Ctrl+V** | `rofi-clip` | **clipboard history**: what was copied, Enter puts it back, Alt+1 deletes that entry |
| **Alt+Ctrl+E** | `rofi-emoji` | **emoji**: type a name, Enter inserts the glyph at the cursor, Alt+1 only copies it |
| **Alt+Ctrl+T** | `translate` | **translate** the selected text (or the clipboard) into the language Settings sets, English until it is set; the answer is copied, and a long one opens in a read window |
| any bar module click | `bar-pick` → `rofi -dmenu` | the fixed list that module hands it (Wi-Fi networks, sinks, devices, power actions, …) |
| **Learn** (root menu) | `rofi-learn` | these pages, rendered with glow, plus a search across all of them |

The root menu is also a launcher entry: **LazyOS Menu** (`Alt+D`, type
`lazy`). It runs the same `bar-menu` that **Alt+Space** opens, so the list is
one row away for someone who does not know the key yet, and the launcher is not
only applications - the bar's left-most button and this entry open the same
window.

## The root menu, the palette and Ask

**The root menu is a window of its own now, not a rofi list.** `bar-menu` is a
link to `lazyos`, so **Alt+Space**, the bar's button and the launcher entry open
the framework's menu: the same rows in the same four groups (*open*, *ai*,
*system*, *session*, the group's name dim on the right of each row), a glyph per
row, and a filter line at the top. **Type to narrow it** - case does not matter -
and the best match moves to the top and is selected, so `wifi` Enter opens the
Wi-Fi list and `lock` Enter locks: a name that starts with what you typed beats
one where a word of it does, which beats one that only contains the letters in
order. A group's name narrows to its rows (`system`). What you open often moves
up, but only from the third time (a single slip does not reorder the menu); that
history is `~/.local/state/lazyos/frecency.json`, it keeps 200 entries, and
deleting it - or a damaged copy - just starts the ranking over. Arrows move,
**Enter** opens, **Escape** closes and changes nothing. A row starts its program
directly, never through a shell, after the menu's window is gone.

**Power** opens the three rows that end the session - log out, reboot, shut
down - inside the same window, and each one asks once more with **Cancel**
selected: a stray Enter answers Cancel. Escape goes back one step (from the
question to the list, from the list to the menu) before it closes anything.

**The command palette** (`lazyos menu palette`) is the same window over every
row the menu has, plus the commands that need no question: one row per theme
(*Theme: Nord* sets it), *Next theme* and *Next wallpaper*. Typing ranks it the
same way. The applications join it later, with their icons.

**Ask** (the *ai* group, or the **Ask** launcher entry, or `lazyos ask` in a
terminal) is one question to an AI, answered in the same window. The header
says **AI** and names the agent before anything is sent: the default the Agents
menu marks with `●` when that is Claude Code or OpenCode, and otherwise
OpenCode, with a line saying why (Codex, Gemini and Copilot have no one-question
mode here). **What leaves the machine is the question, and nothing else**: the
agent runs once in an empty private directory under `$XDG_RUNTIME_DIR`, so it
has no files to read, and Claude Code runs with no tools at all and saves no
session. **Enter** sends it; the window shows the agent working, then the answer
- **c** copies it, the arrows scroll - or, when it fails, the agent's own error
line (not signed in, no network, a rate limit). **What it costs** is what the
agent's own account charges for one question: the image ships no model and no
key, the answer comes from whatever you signed the agent in with. Nothing stays
afterwards: the agent, and anything it started (an `opencode serve`), is ended
when the answer is in, when you press **Escape** or **Ctrl+C**, and when the
window is closed in any other way. `check-system.sh` fails if one was left
behind. It is one question, not a conversation: the next one starts fresh.

The rest of the menus live in the root menu and the launcher: **Places**

(home, documents, downloads, pictures, every mounted disk, and a **Mount** row
for each unmounted one - an SD card in the internal slot, or a stick when
Settings' *Auto-mount USB drives* is off - where Enter mounts it and opens it in
Files and **Alt+1** unmounts a disk; a USB stick is already in that list by the
time you open it, because plugging one in mounts it; its last two rows are the disk
actions, **Auxiliary swap** (a card as more memory, activated on every boot) and
**Format a disk**, and both list only disks with nothing mounted on them, no
ChromeOS kernel partition, and that are neither built into the machine nor the
disk this system runs from, so the eMMC and the card this system boots from are
never candidates - and Format asks twice with Cancel as the first answer, and
writes exFAT (no 4 GB file limit, and Windows, macOS and Linux all read it) or
ext4),
**Media** (what is
playing, play/pause/next/previous), **Time zone** (with network time),
**Free space** (what is using the disk, one action per row), **End a process**
(the heaviest twenty), **Calculator**, **Password** (32 random characters from
the kernel's generator, straight into the clipboard: hex, so there is no shift key and no `+`,
`/` or `=` to re-quote when it is pasted into a file or a URL, and the value never
appears in a notification - mako keeps a history, and a password in it is a
password on disk), **SSH** (hosts from `~/.ssh/config`),
**Share this screen** (Settings and the root menu: `wayvnc` serves the session
read-only on the loopback address, so another machine can look at it through the
ssh login — the menu carries the tunnel command, and `05-system.md` has the how
and the why),
**DNS** (which resolver this connection uses), **Packages** (search the archive
and install one), **Developer** (Alt+Ctrl+D: your profile into an encrypted
archive on a stick, the way back from it, and a diagnostics bundle for a bug
report - `04-software.md` has the detail), **Colour** (the hex of any pixel, and
the palette this session is drawn with), **Reminders** (be told later), **About**
(kernel, uptime, memory, battery, the revision this image was built from, and the
errors since this boot).

## The menus are becoming one program

Everything in the table above is a shell script that starts rofi and exits. The
next version of the session replaces that plumbing with **one binary**,
`/usr/bin/lazyos`, and the old names become symlinks to it: the binding, the
`.desktop` entry and the manual keep saying `rofi-files` while the program
behind the name is the framework. The point is not fewer files - it is that a
menu becomes a *window that changes state* (a list, then a prompt, then a
progress row, then the result) instead of a process that can only answer once.

You can call it yourself, today:

```sh
lazyos --help        # the commands this build carries, from the same table
                     # the session's checks and the installer read
lazyos --version     # the version this image was built from
lazyos menu          # opens one floating window; Esc closes it
```

From the session (a sway binding, a `.desktop` entry, `rofi` replaced by the
frame) that last line asks `foot` for the window itself, with the app-id and the
size its command table declares, and returns immediately - so the key is never
blocked by a window. From a terminal that already is one, it draws in place;
`LAZYOS_UI=1` is how a process tells the framework "I am already the UI, do not
open another window", and the framework sets it for the child it starts. Run
`lazyos menu` inside foot and press Escape: the prompt comes back with the
cursor where it was, which is the whole terminal contract in one try.

Which menus answer today: the two the table lists (`menu` and `menu files`),
each with an empty-state card naming the version that fills it in. They are the
skeleton, not the product - and `check-system.sh` fails if the binary is missing,
if a symlink the installer creates points at a command `--help` does not list,
or if sway lost the rule that floats the window.

## Something is happening

A scan or a connection takes seconds, and a menu that answers only when it is
finished looks like a click that was never registered. Every slow action says so
in two places at once:

* a **mako notification** that does not expire while the action runs, and then
  becomes the result - the same card reads "Looking for networks…", "9 networks
  in range" or "Connected to Home". Errors are the same card in red (critical);
* a **spinner on the bar**, left of the updates indicator: a braille frame with
  the name of the task in yellow, so it is visible even when the notification has
  already scrolled away.

Both come from `~/.config/bin/busy` (`start`, `update`, `stop`, `end`, `fail`)
and `~/.config/bin/bar-busy`; nothing of either stays after the action. Wiring a
new action in is two lines:

```sh
busy start wifi Wi-Fi 'Looking for networks…' network-wireless
out=$(nmcli device wifi list --rescan yes)
busy end wifi '9 networks in range'      # or: busy fail wifi "$out"
```

The state is a file per task in `$XDG_RUNTIME_DIR/lazyos-busy`, and the bar card
disappears on its own if a script dies without ending it.

The spinner is also a button: **click it to stop what it names**. The card and
its notification turn into "Stopped Wi-Fi" (or Bluetooth, Free space, Dictation)
instead of a second toast appearing, and the menu that was waiting opens nothing
- a stopped scan does not show you half a list of networks.

| Card | Clicking it |
|---|---|
| **Wi-Fi** · scanning or connecting | stops waiting. The scan is NetworkManager's, and a join it has already started finishes on its own, so the network may still come up a moment later |
| **Bluetooth** · scanning | stops the three-second scan |
| **Free space** | stops measuring, so nothing is listed (nothing was deleted) |
| **Dictation** | stops the recording and does what the shortcut's second press does: transcribes and types what you said |
| **Packages**, **DNS**, **Wi-Fi** on/off, **Bluetooth** on/off or connecting | says that it cannot be stopped, and keeps going - an `apt` install cut in the middle leaves dpkg's lock behind, and the DNS switch has already written the profile by then |

The tooltip of a stoppable card ends with "click to stop"; the others show the
task alone.

## The task screen

A slow action that takes more than a moment gets a **task window** instead of a
toast. It is part of the same `lazyos` binary the menus are moving to, and it
keeps the person in the loop: the checklist, the progress bar and the live log
stay visible until the action ends, and the result is shown in the same window.
Run `lazyos task attach <id>` to reopen a task after closing it; the state lives
in `~/.local/state/lazyos/tasks/<id>.json` and the log in
`~/.local/state/lazyos/tasks/<id>.log`, so reattaching after the action finishes
still shows what happened. While a task runs, **s** stops it if it is stoppable
(SIGINT first, SIGTERM after two seconds, and the final state is always
`Stopped`, never `Failed`); **Esc** or **q** closes the window without cancelling
the task. The bar's spinner is the same busy state the old menus use, so a click
on it stops a stoppable task the same way **s** does.

## Browsing files

`rofi-files` (Alt+Ctrl+F, "Files" in the root menu and the launcher) is a shell
script over `rofi -dmenu`, because rofi 2.0 has no filebrowser mode:

* **every row has an icon**: a 32 px picture of the file (a photo, a video's
  frame, a PDF's first page) where one can be decoded and a glyph from the
  system's icon theme where it cannot - a folder, a text file, a page, a
  document, an archive, an executable, a song - so the names line up under one
  column and the list says what things are before you read them;
* directories come first, in cyan and with a trailing `/`; then files, with the
  size and the date in the dim colour; a symlink to a directory navigates like a
  directory, and a broken one says so (with a warning icon);
* **Enter** opens a directory (the listing reopens there), reads a text file with
  `bat` (paged, in the terminal's own solarized colours, `BAT_THEME=ansi`, inside
  the floated `popup` window), opens a **document** (PDF, DjVu, a comic) in
  **papers** - a reader with search, zoom, printing and annotations, not a
  browser tab - draws an **image** with **imv** on the GPU (zoom, pan, rotate,
  and the arrow keys walk the folder; `chafa`'s sixels in a foot window are the
  fallback when imv gets no graphics context), and plays a **video** with **mpv**
  in its own window (seek, subtitles, sound; press `q` to close it). A `.deb` or
  anything else binary asks Firefox to open it, which is a download prompt -
  still an answer;
* the first row is `↑ parent directory`, and the last one toggles the dotfiles
  (hidden ones are out of the way until asked for);
* typing filters the list; a path that matches no row is taken literally, so
  `/etc/sway/config.d` or `~/notes.md` can be typed and opened with
  **Ctrl+Return** (or plain Enter once nothing matches);
* the line under the prompt says where you are, how many directories and files
  are there and how much space is left.

**It opens where you left it.** The first time, Files starts in your home
directory; after that it starts in the directory you were last browsing, and
opening a file - from a row or from the file search - remembers that file's
folder instead, so the next **Alt+Ctrl+F** is already where the file lives. The
memory is one line in `~/.local/state/lazyos/files-last` (under `$XDG_STATE_HOME`
when that is set); deleting the file sends the browser back to your home
directory. The file search itself still searches all of `$HOME` and does not move
that memory.

**Alt+1 acts on the row instead of opening it.** Enter *opens* what is under the
cursor; Alt+1 asks what else can be done with it, and the list offers only what
fits: a directory can be measured (`du -sh`, as a card with the stop button) and
packed into `name.tar.xz` next to itself, an archive unpacked into a folder named
after it, a disk image mounted through `udisksctl` and opened in the browser, a
video or a song converted with ffmpeg (MP4, or MP3 for the sound alone), and any
row can be copied as a path or opened in a terminal. Nothing is touched until you
pick something, an existing name is refused rather than overwritten, and every
long action is a `busy` card with the bar's spinner - with the stop button where
stopping is safe (a killed `mkfs` would leave a disk that looks formatted).

**Alt+Search: the files, not the launcher.** Alt+Search (and
`rofi-files --search`) opens one list of every file under your home directory,
newest first, filtered as you type: type `notes` and the row list narrows to the
notes; Enter opens the choice exactly as the browser does. It is one `fd` run
capped at 500 matches with the caches and build directories left out
(`.cache`, `.git`, `node_modules`, `.local/share/containers`), so it is instant
even on a home directory with hundreds of thousands of files - if a query is too
broad, type one more letter rather than scrolling. The dotfiles are still one row
away, and Escape closes it without opening anything. With `fd` missing the
script says so and opens the browser instead.

**Search+Shift: the same files, in a terminal.** The 500-row cap is the list's
trade - it is what makes it open like a menu - so the question "where is the file
I would recognise but cannot name" has a different answer: `lazyos-find` streams
the whole tree through `fd` into `fzf`, with the same four exclusions and the
dotfiles included, a preview pane on the right (text read with `bat`, a picture
or a PDF page decoded once and drawn with `chafa`) and Enter handing the choice
to the same opener a double click in the window uses. Nothing is capped, the
prompt is up before the walk is finished, **Ctrl+Y** copies the chosen path, and
Escape closes it. Both keys are one process while their window is open and
nothing afterwards.

**Alt+Ctrl+G: the text inside the files.** The two keys above answer "which file
was it" from the name. `rofi-grep` answers the other question - "where did I write
this" - by running `ripgrep` over the same home directory and listing a row per
match: the matching **text** on the row (which is what tells two matches apart),
`path:line` in the dim colour, and Enter opening the file in `bat` scrolled to
that line. bat colours the match and writes the page to a real temporary file,
and `less -R +N` opens that file at the line - `-R` keeps the colours bat wrote,
and the file is removed when the window closes. A pipe would be the obvious
shape and is the one that does not work: `less` cannot seek it, and the old
`bat | less +N` answered "Cannot seek to line number N" instead of showing the
match. It is bounded like the
list: three matches per file, the same four trees left out, binaries dropped by
ripgrep itself, and 400 rows - type one more word to narrow it. A pattern ripgrep
cannot read says so in ripgrep's own words, and the walk is a `busy` card with the
stop button.

`xdg-open` exists too (`/usr/local/bin/xdg-open`, two dozen lines): a URL goes to
Firefox, and a path goes to the same rule table a double click in the window uses
(`lazyos-view`), so a file opened from a script lands where it would from the
desktop. That is what foot's ctrl+click and the CLI tools that print a URL call.

It is the same trade as everything else here: 16 MB while open, nothing
afterwards.

**Alt+Ctrl+Shift+F: the same files in a window.** `favnyr` (the root menu's **Files
(window)** row, `Files (window)` in the launcher) is a real file manager for the
things a list cannot do - drag a file onto a folder, select several, rename in
place, send to the trash - and it opens a file with exactly the rules above: a
double click reaches `~/.config/bin/lazyos-view` through
`/etc/xdg/mimeapps.list`, so text goes to `bat`, a document to `papers`, a picture
to `imv` (or `chafa`'s sixels when imv cannot start), a video to `mpv` and an
HTML page, an EPUB or a `.deb` to Firefox (the list itself reads an HTML file as
text). Its colours are the session's too (base03 window, cyan selection,
JetBrains Mono, the same icons). It is the *only* GTK3 window in
the image, which is why it has a stylesheet and not a theme of its own, and it
needs `gvfs` for the trash; SMB and phone mounts are not installed, so its
Network places are off. `04-software.md` § *Files in a window* has the rest.

## Capture: the screen as a picture, as text, or as a video

**Alt+Ctrl+C** (`rofi-capture`, and "Capture" in the root menu and the launcher)
is the list behind the screenshot keys. Everything on it is one command that
ends before the list closes again:

| Row | What it does |
|---|---|
| **Screenshot** · the whole screen / an area / a window | `grimshot`, through `~/.config/bin/screenshot`, saves it to `~/Pictures/Screenshots/<today>/` and names the file in a notification, which also carries the image |
| **Screenshot · an area, to the clipboard** | the same, straight to the clipboard (what **Alt+Ctrl+S** also does) |
| **Annotate an area** | `grimshot` saves it and **swappy** opens it: arrows, rectangles, text, blur, crop. Enter saves back over the same file |
| **Text from an area** | `slurp` picks the region, `tesseract` reads it, and the text lands in the clipboard, with the first line in the notification. English is what the image ships (`tesseract-ocr-eng`); other languages are one `apt install` away |
| **QR code from an area** | `zbarimg` reads a QR code (or an EAN/Code-128 barcode) from the region and copies what it says |
| **Make a QR code** | the Wi-Fi network's password (read from the saved profile, so it is not typed again) or any text, drawn in a floating terminal by `qrencode`; nothing is left on disk |
| **Record** · the whole screen / an area | `wf-recorder` starts writing an `.mp4` into `~/Videos`, and the same menu row turns into **Stop recording** |

The recording is the one thing that outlives the list, so it says so twice: the
mako card stays until it is stopped, and the bar shows the spinner with
`Recording` in it. It encodes with x264 on the CPU because this SoC has no
hardware encoder (`veryfast`, crf 28, 30 fps - the point where a full-screen
recording still leaves the compositor its frame budget). There is **no audio
track**: that needs a PipeWire monitor source and nothing here wires one, so
`wf-recorder -a` would record the microphone instead. The file is finalised on
SIGINT, which is what Stop sends; a recorder that had to be killed leaves a file
the notification calls empty rather than claiming success.

The QR the Wi-Fi row shows is the phone-readable `WIFI:T:WPA;S:…;P:…;;`, with
`\`, `;`, `,` and `:` escaped. It contains the password, so the temporary file it
is drawn from is created `0600` and removed when the window closes.

Everything this menu needs is already in the image: `grimshot`, `swappy`,
`tesseract`, `zbar-tools`, `qrencode` and `wf-recorder`; `check-system.sh`
fails when one of them is missing, because a row that does nothing is worse than
a row that is not there.

Every screenshot this menu takes - like every one the keys take - lands in
`~/Pictures/Screenshots/<the date>/`, one folder per day, under grimshot's own
file name; the last row opens that folder (or `~/Pictures` before the first
screenshot).

## Download

**Download** (`rofi-download`, in the root menu and the launcher) takes a link and
leaves the file in `~/Downloads`: paste it, or pick **the link in the clipboard**
when the clipboard holds one. Then choose **video** (the best picture and sound it
can merge) or **audio only** (an `.m4a`, without the picture). It is `yt-dlp`,
which needs no daemon and no configuration: one process per link, and it exits
when the file is on disk.

Three things it does not do. It does not take a playlist (`--no-playlist`), so a
shared link to a video inside a list downloads that video and not the thousand
next to it. It does not date the file by the upload (`--no-mtime`), so the newest
download is the newest file. And it does not pretend: the notification names the
file it wrote, and a failure is the last lines of yt-dlp's own message
("Video unavailable"), not a traceback. A download can take minutes, so it is a
`busy` card with a stop button — and stopping it says the `.part` file is still in
`~/Downloads` instead of claiming a file. `ffmpeg` merges the streams, and `node`
is the JavaScript runtime `/etc/yt-dlp.conf` names, which is what makes YouTube
offer the high-quality formats at all (`[debug] JS runtimes: node-24.21.0`).

## Clipboard history

**Alt+Ctrl+V** (`rofi-clip`, and "Clipboard" in the root menu and the launcher)
lists what has been copied since the session started. Enter puts an entry back on
the clipboard, ready to paste; Alt+1 deletes that one entry.

The history is kept by `cliphist`, which is fed by `wl-paste --watch cliphist
store` — one line in `etc/sway/config.d/27-clipboard.conf`, and the only
long-running process this session has (about 2 MB). `wl-clipboard` alone only
ever knows the current selection, so a history needs something watching it; there
is no way to have one without that watcher. The database is
`~/.cache/cliphist/db`, it holds **everything** copied — a password from a
password manager included — and the last row of the menu clears it.

Rows are classified by their content: text, a link (starts with `http`) or an
image. An image is listed by what `cliphist` knows about it (`[[ binary data 64000
png 640x480 ]]`) rather than by a thumbnail, because a rofi row is one line of
text. **Save the newest image** decodes it into `~/Pictures/clipboard/` with the
extension its MIME type implies and hands it to `xdg-open`, which is the same
`lazyos-view` a double click in favnyr uses — so a picture opens in **imv**, not
in a browser tab; the decoded file is what survives, and the entry itself stays
in the history until deleted or cleared.

## Emoji

**Alt+Ctrl+E** (`rofi-emoji`, and "Emoji" in the root menu and the launcher) is
the same list shape over `~/.config/emojis.tsv`: about 1300 rows of *name*,
*glyph* and *block*. Typing filters by any word of the name, and since the block
is in the row too, `faces`, `transport` or `dingbats` narrow it to a category.
The table is generated on the build host by `scripts/make-emojis.sh` from
Python's Unicode database and shipped as a file, so the image needs no Python and
no extra package for it. It is filtered against the font that draws it: only the
codepoints `fonts-noto-color-emoji` actually carries become rows, because a
symbol the font does not draw is inserted as an empty box. `~/.config/emojis.font`
next to the table pins that font's SHA256 and family, which is how
`check-system.sh` notices that a font upgrade made the table stale - re-running
the script on a build host with `fontconfig` and the font rebuilds it.

Enter inserts the glyph at the cursor the way Omarchy does it: the glyph goes to
the clipboard and a virtual **Shift+Insert** is typed with `wtype`, which every
toolkit here understands as paste. Typing the character itself would need a
keysym for its codepoint, and most emoji have none in the current keymap. Alt+1
is the clipboard write alone, for a program that refuses a virtual keyboard. What
draws the glyph is `fonts-noto-color-emoji`; without it an emoji is a tofu box,
which is why `check-system.sh` asks `fc-match emoji`.

## Translate

**Alt+Ctrl+T** is not a menu: it translates **the text you selected with the
mouse** (or, with nothing selected, the clipboard) into one language, copies the
answer and names the direction in a notification - `Spanish → English`. The only
thing to configure is which language that one is, and it is a row in
**Settings**: **Translate** shows the current one and opens the list of the
twenty-six languages both engines take, with the how-to on the same screen
(`~/.config/bin/translate config` is that list from a terminal). Until it is set,
everything goes into **English**.

The engine is chosen for this machine, not by taste: Google's JSON endpoint first
(`clients5`, the one that also detects the language), MyMemory without a key
second (in 450-character pieces; 500 is its ceiling), and a card that says the
network is the reason when neither answers. `translate-shell` is not installed
because the endpoint it uses answers this machine with a captcha page. A
translation longer than a few lines opens in a read window instead of a toast;
the full page is `04-software.md` § Translate, including why a translated secret
is a secret in the clipboard history.

## Colour, DNS, packages, reminders and about

Five more lists in the root menu, each one screen of answers about the machine:

* **Colour** (`rofi-color`) reads a pixel with `slurp -p` and `grim`, turns the
  1×1 PNG into three bytes with `ffmpeg` — no ImageMagick, no extra package — and
  copies `#rrggbb`. The rest of the list is the sixteen solarized colours this
  session is drawn with, each row a swatch in that colour; Enter copies its hex.
* **DNS** (`rofi-dns`) points the connection that is up at Cloudflare, Google,
  Quad9, AdGuard, a list you type, or back at what DHCP hands out. It sets
  `ipv4.dns`/`ipv4.ignore-auto-dns` on the NetworkManager connection and brings it
  up again, which is why it shows a busy card and then reads back the servers in
  use instead of repeating what was asked for. It does not touch IPv6 DNS.
* **Packages** (`rofi-pkg`) searches the archive with `apt-cache search` and
  installs what is chosen with `apt-get -y` (noninteractive, keeping existing
  configuration files), reporting through the busy card; installed packages are
  marked before they are picked. The last row updates the package lists, which is
  what to run when a search finds nothing.
* **Reminders** (`rofi-remind`) writes nothing to disk: it starts a transient
  `systemd-run --user --collect` timer that fires a `notify-send` and is
  collected, and the list is read back from systemd, so choosing a reminder
  cancels it and a reminder lives as long as the session's user manager does. It
  asks for a title, then for an optional note — Enter or Escape there leaves it
  empty, and neither cancels the reminder — then for when. Besides the five
  delays it can repeat: on a date and time, every day at a time, every weekday
  (Monday to Friday), every week on a chosen weekday, or every month on a day
  1-28. Every date is put through `systemd-analyze calendar` before anything is
  created, so a typo is refused with systemd's own message and the next
  occurrence is shown for confirmation; the reminder then keeps the title and the
  note in the timer's own description, which is what the list shows on a second,
  dimmer line. "At 09:00" means the next nine in the morning, once.
* **About** (`rofi-about`) is one screen of read-only facts — model, the revision
  this image was built from, kernel, uptime, memory and swap, disk, battery,
  network, compositor, process count — plus two rows: copy the summary as text,
  or open `fastfetch` in a terminal for the pretty version.

They are all scripts in `~/.config/bin/`, they all use the shared row style from
`lib-rofi.sh`, and none of them stays running.

**Reminders: what the prompts expect.** The When menu has a **How do I fill
these in?** row that says this in a notification and reopens the menu, so the
answer is one keystroke away while the title and note are still known. The five
delays need nothing typed. A date is `YYYY-MM-DD HH:MM`, for example
`2026-12-24 19:00` (anything `date -d` understands also works). A time is
24-hour `HH:MM`, for example `09:00` or `18:30`. Weekdays and days of the month
are picked from a list, not typed. A repeating reminder repeats while the
session's user manager lives - logging out stops it, the same lifetime as the
one-shot ones.

## The manual, rendered

`rofi-learn` lists `usr/share/doc/lazyos/*.md` and opens the chosen page with
**glow**, so it is a document (headings, tables, inline code, lists) instead of
markdown source. The style is `~/.config/glow/solarized.json`, the same palette
as everything else: headings carry a cyan/blue bar, inline code and fenced
blocks sit on base02, and code is highlighted with the solarized token colours.
The first row searches all pages with ripgrep and opens the one that matches.

The second row is the *other* manual on the machine: **A program's manual** asks
for a word and lists every installed manual page whose description mentions it
(`apropos`), one row per page with the section in brackets and the description in
the dim colour; Enter opens `man` in a terminal, where `MANPAGER` (`col -bx | bat
-l man -p`, from `etc/profile.d/90-cli-tools.sh`) renders the page in the same
palette as everything else. The same name in two sections (`printf (1)` and
`printf (3)`) is two rows, because `man` has to be told which one.

`glow -p` pages through `less`, so the page scrolls with the usual keys and
leaves nothing behind when it closes.

## The theme and the icons

Two files, and everything - launcher, window list, bar panels, key view, the
manual - reads the same theme:

* `~/.config/rofi/config.rasi` - the settings: the modes, `terminal: "foot"`,
  fuzzy matching, history, `show-icons: true`, `icon-theme: "LazyOS"`.
* `~/.config/rofi/solarized.rasi` - the look: base03 window, 2 px cyan rim,
  JetBrains Mono 12, base02 card on the selected row, the box shrinking to its
  entries.

Three kinds of picture, because rofi has three:

* the lists this session draws itself show a **font glyph** in the row text
  (Font Awesome, the same alphabet as the bar). It costs nothing, follows the
  palette, is filtered away by typing, and is what `lib-rofi.sh` puts in every
  `rofi-*` menu;
* the launcher, the window switcher and the **key view** show an **image icon**,
  which rofi resolves through the icon theme. The launcher and the switcher ask
  for the application's own icon and cost about 13 MB of memory for ten rows
  while the window is open; the key view has a picture on **every row**, in three
  steps: the tool the binding runs (`rofi-power`, `rofi-files`, `rofi-clip`,
  `dictate`, `sway-lock`…), the action it sends (the arrow of a focus or move
  binding, the cross of a close, the refresh of a reload, the door of an exit), and
  for everything else its section (windows, gear, lock, sun, speaker, touchpad…).
  Every row and not only the section line, because rofi drops the icon column for a
  row that has no picture: that row's text starts 34 px to the left of its
  neighbours, which is what the list looked like before;
* **notifications** show a picture too - `clipboard-history`, `dialog-error`,
  `user-trash`, `folder`… - which mako looks up on its `icon-path`. A name the
  theme does not carry draws nothing: the card is text-only, with no error
  anywhere.

Debian's Adwaita ships only monochrome symbolic icons and none of this session's
names, so the image carries a small **LazyOS** icon theme: the same Font Awesome
codepoints, rendered once to 48×48 PNGs by `scripts/make-icons.sh`. Its
`index.theme` inherits Adwaita and hicolor, so an application that brings its own
icon (foot, btop, htop, anything installed later) still shows it in colour.

A row is one line and rofi cannot scroll it sideways, so the second half of a row
(a path, a revision, an emoji's block) is drawn a little smaller and, past sixty
characters, cut with an ellipsis - a path keeps its end, because that is the part
that identifies the file. The list grows wider instead when it can: 62% for a
name and a sentence, 76% and 88% for the longest rows the session has (an emoji
name is 76 characters). The full value is always in the notification that
choosing the row raises, so nothing is lost - the list is the index, the card is
the text.

The bar's popups pass `-no-show-icons`: they draw a glyph in the text and do not
need an icon column.

## Adding things to the launcher

* **An application**: drop a `.desktop` file in `~/.local/share/applications/`.
  For a terminal program, `Terminal=true` is enough - rofi starts it in foot.
  This is how `org.lazyos.*` entries are made (they live in
  `/usr/share/applications/`, shipped by the image).
* **An icon for it**: put the name in its `Icon=`, and either let the
  application ship the image or add a line to `scripts/make-icons.sh` (a Font
  Awesome codepoint) and run it on the build host.
* **A menu**: `bar-pick "Prompt" < list-of-lines` gives you a rofi dmenu with the
  session's theme and the same "Escape means cancel" contract; the `rofi-*`
  scripts add the shared row style from `lib-rofi.sh` (`ui_row`, `ui_pick`,
  `ui_say`).
* **A dynamic list**: a script mode (`-modi "name:script"`) is called when the
  mode starts and when a row is chosen - *not* on every keystroke, which is why
  the calculator is a one-shot prompt and not a live preview. The file browser
  does not need it either: it lists one directory per run and reopens in the
  chosen one.

## When rofi misbehaves

* **A dialog in the middle of the screen naming a file, a line and a column**:
  that is a theme parse error. `rofi` only reports it when a window opens, so
  `rofi -dump-config` is not a syntax check.
* **The window opens empty**: the list handed to it was empty. An empty menu is
  indistinguishable from a hang; the scripts are written to exit instead.
* **A row of empty space where an icon should be**: the `.desktop` names an icon
  that no installed theme has. `check-system.sh` verifies the LazyOS theme and
  every `org.lazyos.*` icon.
* **A list opens no window at all, and the journal has a line about a binding**:
  the menu was given an `Alt+1` action whose value was a sentence.
  `-kb-custom-1` accepts a keybinding and nothing else; `lib-rofi.sh` passes one
  word and puts the sentence in the message line.
* **A row ends in `…` but the file name is not there**: the value did not fit the
  window, and `ui_trim` kept the head because it contains no `/`. For a path the
  tail is kept instead. Either way the whole value is in the notification.
* **Nothing happens on a bar click**: `bar-pick` is not executable, or rofi is
  missing. `check-system.sh` fails for both.
