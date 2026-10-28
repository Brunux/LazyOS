# Shortcuts

`$mod` is **Alt** on this port. Omarchy (the desktop this session is ported from)
uses Super, so the same actions usually keep the same letter and change the
modifier; the table at the end says so where it matters.

The live list is **Alt+Ctrl+K** (`rofi-keys`): it is parsed from the
configuration files, so it is never stale, it can be searched, and Enter copies
the command. Every row carries a picture as well - the tool the binding runs, the
action it sends, or its section - so the list is scanned by shape before it is
read. This page is the tour.

## Getting around

| Keys | Does |
|---|---|
| Search key | launcher: every application, narrowed by typing (the same menu as Alt+D) |
| Alt+Search | file search: every file under your home, narrowed by typing (Enter opens it, Enter on a folder browses there) |
| Alt+Ctrl+Shift+F | the file manager in a window (favnyr): drag and drop, several files at once, rename, the trash |
| Alt+D | launcher: applications (the Search key opens the same menu) |
| Alt+Space | the root menu (the bar's menu button, on the keyboard) |
| Alt+Tab | window switcher |
| Alt+Shift+D | command palette (`$PATH`; Shift+Enter runs it in foot) |
| Alt+Ctrl+K | every key binding, searchable, Enter copies the command |
| Alt+Ctrl+F | files: browse a directory, Enter reads a text file in foot, Alt+1 acts on the row |
| Search+Shift | the file search in a terminal (`lazyos-find`): live filtering as you type, a preview on the right, Enter opens the file |
| Alt+Ctrl+G | search inside the files: ripgrep over your home, Enter opens the file at the matching line |
| Alt+Ctrl+D | developer: export your profile (keys, configs, packages) encrypted to a disk, restore one, or save a diagnostics bundle |
| Alt+Ctrl+T | translate the selected text (or the clipboard): the answer is copied, and the card names the direction |
| Alt+Ctrl+O | settings: the radio switches, night light, touchpad, rotation, display arrangement |
| Alt+Ctrl+Shift+D | bring the internal panel back when the screen is black (no menu needed: it works with nothing on screen) |
| Alt+Ctrl+N | notifications: the history, do-not-disturb, copy one |
| Alt+Ctrl+Q | calculator: type it, Enter copies the result |
| Alt+Ctrl+V | clipboard history: Enter puts an entry back, Alt+1 deletes it |
| Alt+Ctrl+E | emoji: type a name, Enter inserts the glyph at the cursor |
| Alt+Return | terminal (foot) |
| Alt+Shift+Return | the browser (firefox-esr; Omarchy has no key for it) |
| Alt+Q (or Alt+Shift+Q) | close the focused window |
| Alt+Shift+C | reload the configuration |
| Alt+Shift+E | exit the session (asks first) |
| Alt+grave / Alt+Shift+grave | scratchpad: show the hidden window / send the focused one there |
| Alt+R | resize mode (arrows, Return or Escape to leave) |

The **Search key** and **Alt+D** open the same launcher: every application, one
row each, narrowed as you type. The Search key is the magnifier; Alt+D is the
other way to the same menu, so a keyboard that is not this Chromebook's can still
reach it.

**Alt+Search is the file search.** It opens `rofi-files --search`: one `fd` run
over your home (queues, caches and build trees left out), capped at 500 rows,
listed with the same icons the file browser draws, so typing `invoice` finds the
file by its name wherever it is. Enter opens the choice with the program that
fits it, Enter on a folder browses there, and Escape closes the list without
opening anything.

Press **Alt first, then Search**: keyd turns that chord into `F20`, so the list
opens on the press. Pressing Search first also works - it reaches sway as the
Search key, and releasing it while Alt is still down opens the same list - but
the Alt-first order is the one that does not depend on which key comes up first.

**Search+Shift is the same key with Shift held: the file search in a terminal.**
It opens `lazyos-find`: `fzf` over every file under your home - the same four exclusions,
but with the dotfiles included, since the whole tree is the pool and `~/.config`
is a place people look for things. The list narrows as you type and the pane on
the right previews the row under the cursor: text is read in place, a picture or
the first page of a PDF is decoded once and drawn in the terminal, anything else
is named by its type and size. Enter opens the choice with the same rules the
file browser uses (text in `bat`, a picture in `imv`, a video in `mpv`, a PDF in
the reader); **Ctrl+Y** copies its path instead, which is what a search in a
terminal can do that a menu cannot, and Escape closes it.

The two searches answer different questions. Alt+Search opens a *menu* - rows
with icons and thumbnails, ready before you can type, one bounded run over the
tree, and a list you scroll. Search+Shift *filters everything*, uncapped,
while the walk is still running, and shows you the file before you open it.
Neither stays resident: one process while its window is open.

**Alt+Ctrl+Shift+F opens the file manager in a window** (**favnyr**), the Shift
variant of the Files key (`Alt+Ctrl+F` above) and the same window the root menu
(**Alt+Space**) and the launcher call **Files (window)**. It is what a list
cannot do - drag a file onto a folder, select several, rename in place, send to
the trash - and it opens a file with the rules above, so a double click lands
where Enter in either search lands.

The Search key's own three bindings are *release* bindings: Search alone opens
the launcher, Alt+Search the file list and Search+Shift the terminal search, and
which one fires is decided by the modifiers still down when Search comes up.
Release Shift first and the launcher opens, which answers a narrower question
rather than the wrong one.

**Alt+Ctrl+T translates what you selected.** The mouse selection *is* the text,
so the gesture is highlight-then-key in any window - foot, the browser, a
document - and with nothing selected it takes the clipboard. Everything goes
into the language **Settings → Translate** sets (English until it is set), the
translation is copied to the clipboard, and a long one opens in a read window
with the selection underneath it. `04-software.md` § Translate has the rest: the
two engines, what happens offline, and why `translate-shell` is not on the card.

**Alt+Ctrl+M is the local model.** It opens a model that runs on this machine -
no network, no account, one window - and the root menu's AI group carries the
same window as **Local model**. It runs four threads on purpose: eight measured
6.67 tokens/s on this chip against 11.19 for four. The weights are not shipped
(1.2 GB each), so the first use is `lazyos-model --fetch`, which downloads the
recommended one and checks it against its pin. `04-software.md` § A local model
has the numbers, what it will not do, and why the answer arrives without a
thinking block.

**Alt+Ctrl+D is the developer menu**, and it is about your account rather than
the machine. *Export my profile* packs `~/.ssh`, `~/.gnupg`, the dotfiles and the
configuration of the tools this image ships into one archive on a disk you pick -
encrypted with a passphrase, because those are private keys - and writes a
manifest beside it. *Restore a profile* is the way back: it lists the archives it
can see, backs up the current profile first, and overwrites only what the
archive carries, so the wrong archive is undoable. *Save a diagnostics bundle*
collects the system report, this boot's journal and the hardware facts into one
file to attach to a bug report. `04-software.md` § *Your profile, on a stick*
has what is inside, what is deliberately not, and the command to do it from a
terminal (or over ssh).

## Panels, lock and power

These are the bar's panels without the mouse, on the same letters Omarchy uses
with `Super+Ctrl`:

| Keys | Does |
|---|---|
| Alt+Ctrl+A | audio output (sinks, volume per application, and the two mutes) |
| Alt+Ctrl+B | Bluetooth (scan, connect, disconnect) |
| Alt+Ctrl+W | network (Wi-Fi list, strongest first) |
| Alt+Ctrl+P | power menu: lock, suspend, hibernate, log out, reboot, shut down |
| Alt+Escape | the same power menu (Omarchy's Super+Escape) |
| Alt+Ctrl+L | lock the screen (the Lock key does the same and then suspends) |
| Alt+Ctrl+X | dictation on/off (the notification stays while it records; press again to stop) |
| Alt+Ctrl+M | the local model: Qwen3-1.7B on this machine, offline (see `04-software.md` § A local model) |
| Alt+Ctrl+Shift+A | agents (launch, accounts, install) |
| Alt+Ctrl+O | settings: Wi-Fi and Bluetooth radios, night light, do-not-disturb, touchpad, tablet mode, rotation, display arrangement |
| Alt+Ctrl+Shift+D | the internal panel back on, with nothing on screen: the way out of a black screen (it runs a sway command, not a menu) |
| Alt+Ctrl+N | notifications: history, do-not-disturb, copy |
| Alt+Ctrl+Q | calculator |
| Alt+Ctrl+V | clipboard history (Enter copies back, Alt+1 deletes the entry) |
| Alt+Ctrl+E | emoji picker (Enter inserts, Alt+1 copies only) |
| Alt+Shift+Escape / Alt+Ctrl+Escape | lock then suspend / suspend |

**An idle screen is three steps, not one.** After 2½ minutes the LazyOS wordmark
starts looping; at five minutes the session locks; at six the panel itself goes
dark (DPMS), which is what actually saves the battery on a lit LCD. Any input
ends the wordmark at once - a key, a moving mouse, a tap on the screen - and the
same input brings the panel back on. The lock is the one step that answers
typing only: nothing on the lock surface reads the pointer, so there a mouse
move turns the screen back on and the password is what ends it.

Two applications are exempt while they are being watched: a **fullscreen video or
presentation** in Firefox or the document reader, and a video in **mpv**. They ask
the compositor to keep the screen awake, so the lock does not interrupt a film.

The **coffee cup on the bar** is the manual switch: while it is yellow the screen
stays awake (a long download, a recipe, a page you are reading from across the
room), and a click on it puts it back to dim, which means "let it sleep". It is
the bar's own module and it starts nothing.

## Windows

| Keys | Does |
|---|---|
| Alt+arrows (or Alt+H/J/K/L) | focus |
| Alt+Shift+arrows | move the window |
| Alt+1..0 | go to a workspace |
| Alt+Shift+1..0 | move the window there |
| Alt+B / Alt+V | split horizontally / vertically |
| Alt+S / Alt+W / Alt+E | stacking / tabbed / toggle split |
| Alt+F | fullscreen |
| Alt+T | float the window |
| Alt+Shift+Space | toggle floating (the sway default) |
| Alt+A | focus the parent container |
| three-finger swipe left/right | previous / next workspace (touchpad) |
| three-finger swipe up/down | next / previous tab in Firefox: Ctrl+Tab and Ctrl+Shift+Tab |

**Alt+Q closes the focused window**, and it is the key to remember for the
windows the About menu holds open: **The full picture** (fastfetch), **Boot
time** and **Devices** start a terminal that stays up until it is dismissed, and
each one ends with "close this window with Alt+Q". That is sway's own close
binding, the same one every other window answers to; foot's built-in close key
did not reach these windows on this keyboard, so the footer names the session's
key instead.

**The touchpad carries four three-finger swipes.** Left and right switch
workspaces. Up and down go to the next and the previous tab of **Firefox**,
because they type **Ctrl+Tab** and **Ctrl+Shift+Tab** into it, and they do
nothing at all in any other window: sway checks that the focused window is
Firefox before it starts anything, so the filter costs nothing when it is not.
That is deliberate - a swipe that typed Ctrl+Tab into every application would be
a keyboard nobody asked for, and in a terminal Ctrl+Tab means something else.

The keys themselves are typed with `wtype` (the same virtual keyboard the emoji
picker and dictation use), which is what makes them real for the focused window;
if it is missing the swipe completes and nothing happens, and `check-system.sh`
says so. A Firefox started with `MOZ_ENABLE_WAYLAND=0` runs under Xwayland, has
no Wayland app id, and therefore gets no gesture: run it the normal way and the
swipes work.

## Sound, light and screenshots

| Keys | Does |
|---|---|
| the volume keys (or F1/F2 with the Search key) | volume -5% / +5% |
| Mute | mute · **Shift+Mute**: switch the audio output |
| the brightness keys (or F3/F4 with Search) | brightness -5% / +5% |
| Alt+Shift+S / Alt+Shift+F / Alt+Ctrl+S | screenshot area / full screen / copy area (each one says so in a notification: the file, or the clipboard) |
| Alt+Ctrl+C | capture menu: screenshot, annotate, OCR, QR code, record the screen |

**The volume and brightness keys draw an overlay** - a notification card with the
level as a bar, the icon for what changed and "Muted" when that is the answer. It
replaces itself on every press (holding the key moves one bar instead of stacking
twenty) and it is the only feedback that survives a fullscreen window, which is
where the bar's own number is covered. The card is the session's own notification
system - no extra program runs - and pressing a key while an overlay is up only
updates it.

**Volume per application** is the audio panel's third row (`Alt+Ctrl+A`): it opens
`pavucontrol`, one slider per application and per input, for the case the sink
list cannot answer ("which window is that sound coming from"). It is a window,
not a mixer in the bar, and nothing starts it until the row is picked.

A saved screenshot goes to **`~/Pictures/Screenshots/<today>/`**, under the name
grimshot generates (`2026-10-21T18-04-11.png`), which is the name the
notification shows. The capture menu's last row opens that folder.

Dictation (`Alt+Ctrl+X`) records up to 60 seconds and types what it heard into
the focused window. It says what it is doing: the notification stays up while it
records ("Alt+Ctrl+X stops it"), becomes "Transcribing…" while whisper works, and
turns into the text it typed - or the reason it could not. Pressing the key again
**stops and transcribes**, and the root menu's **Dictation** row does the same
from the mouse. The text is also left on the clipboard, because a few surfaces
(a launcher, a fullscreen game) refuse typed input.

The top row of this keyboard sends action keys, not F1-F12: keyd turns
**Ctrl+Alt+top row** into virtual terminals 1-10 and **Search+top row** into
F1-F10. Without keyd there would be no way to reach a VT on this machine.

## Tablet mode

Fold the keyboard all the way back and the machine becomes a tablet: the
on-screen keyboard comes up over the bottom of the screen, the touchpad **and
the physical keyboard** are turned off (the touchpad is on the back now, and the
folded keys are under the screen, where a keystroke is a keystroke), and the
screen turns with the device. The rotation is read from the accelerometer once a
second and only while the keyboard is detached, so nothing about it runs in
laptop mode. Fold it back and all of it undoes itself, with the screen upright
again.

The keyboard is only switched off once the on-screen one is confirmed running,
so a keyboard that fails to start leaves you the physical one instead of only
the touchscreen. Lock still suspends the machine in tablet mode: logind reads
`/dev/input` itself, so the key reaches it even while sway has the keyboard
switched off.

The volume and power keys and the tablet switch are one device
(`cros_ec_buttons`), so the mode cannot simply switch "every keyboard" off: the
device would be suspended with the keys and folding the keyboard back would
never be seen. It names the typing keyboards one by one instead and leaves that
device alone, which is why the volume keys still work and the mode can always be
left. If it ever stays on after folding the keyboard back, **Settings → Tablet
mode** (or `~/.config/bin/tablet-mode off`, typed on the on-screen keyboard)
puts everything back, and `check-system.sh` FAILs when the switch itself is
suspended.

If the screen comes up **upside down**, or turns the wrong way, the accelerometer
in your unit's panel disagrees with the built-in mapping: **Settings → Calibrate
rotation** (only listed while tablet mode is on) reads the sensor in the pose you
are holding - screen facing you, upright - and stores it in
`~/.config/lazyos/tablet.conf`, so the four positions are derived from your unit
from then on. It needs no rebuild. `~/.config/bin/tablet-mode calibrate` is the
same thing from a terminal, `tablet-mode calibration` says which mapping is in
use, and deleting the file goes back to the built-in one.

| Action | Does |
|---|---|
| the keyboard folded back | on-screen keyboard up, touchpad and keyboard off, the screen follows the device |
| the keyboard back in place | keyboard gone, both inputs back, screen upright |
| Settings → Tablet mode | the same switch from the mouse, when the keyboard is already detached and the mode did not follow |
| Settings → Calibrate rotation | (tablet mode only) teach it which way your unit's accelerometer says is up |
| Settings → Rotation | turn the screen by hand, four positions (this is not tablet mode, and it stays) |

The keyboard is `wvkbd` drawn in the session's palette and font. Its letters
layer is the mobile-international one, so accented characters are one **Cmp**
away: `Cmp` then `a` opens that key's variants (`á`, `à`, `â`, `ä`…) and the one
you tap is what gets typed; `Cmp` then `?` is how `¿` is reached. It types into
the focused window through the virtual-keyboard protocol, which is why it works
in foot, Firefox and the rofi prompts alike.

The keyboard is **opaque**: with transparency the window underneath showed
through the keys, which is the opposite of what a keyboard is for. Folding it
back always leaves the screen upright, even after a session that died in tablet
mode - the rotation is undone by asking the compositor for the focused output,
not by reading the mode's state directory, which such a session takes with it.

## What was copied from Omarchy, and what differs

`Super` there is `Alt` here; the letters and Ctrl/Shift combinations are the same
wherever sway does not already own the key.

| Action | Omarchy | Here |
|---|---|---|
| terminal | Super+Return | Alt+Return |
| apps | Super+Alt+Space | Alt+D or the Search key |
| root menu | Super+Space | **Alt+Space** |
| window switcher | Alt+Tab | **Alt+Tab** (rofi's window mode) |
| keybindings | Super+K | **Alt+Ctrl+K** (Alt+K is vim focus up, Alt+Shift+K moves the window) |
| settings / control centre | Super+Ctrl+O | **Alt+Ctrl+O** (one list of switches and panels) |
| calculator | Super+Ctrl+Q | **Alt+Ctrl+Q** |
| notifications | bar panel | **Alt+Ctrl+N** |
| clipboard history | Super+Ctrl+V | **Alt+Ctrl+V** |
| emoji picker | Super+Ctrl+E | **Alt+Ctrl+E** |
| dictation | Super+Ctrl+X | Alt+Ctrl+X |
| agents | Super+Shift+Ctrl+A | Alt+Ctrl+Shift+A |
| floating | Super+T | Alt+T |
| close window | Super+Q (or Super+W) | Alt+Q (Alt+W is sway's tabbed layout) |
| scratchpad | Super+grave | Alt+grave |
| lock | Super+Ctrl+L | Alt+Ctrl+L (and the Lock key) |
| system menu | Super+Escape | Alt+Escape → the power menu |
| audio / Bluetooth / network panels | Super+Ctrl+A/B/W | Alt+Ctrl+A/B/W |
| power menu | Super+Ctrl+P | **Alt+Ctrl+P** |
| browser | no key (its launcher) | **Alt+Shift+Return** (firefox-esr) |
| screenshot | Super+Shift+S, Print | Alt+Shift+S, Alt+Shift+F, Alt+Ctrl+S |
| capture menu (screenshot, OCR, QR, recording) | Super+Ctrl+C | **Alt+Ctrl+C** |
| files | Super+Shift+F opens the file manager | Alt+Search: `rofi-files --search` (a list); **Alt+Ctrl+Shift+F**: favnyr, the same files in a window; **Search+Shift**: `lazyos-find`, the same files in a terminal; **Alt+Ctrl+F**: browse a directory |
| fullscreen | Super+F | Alt+F |
| volume, brightness, media keys | XF86 keys | the same keys |
| workspaces | Super+Tab cycles | Alt+1..0 and three-finger swipe |
| layouts | Super+L cycles | Alt+S/W/E (sway's three layouts) |

**Three deliberate deviations**, all because sway's defaults own the key Omarchy
uses: the key view is `Alt+Ctrl+K` (not `Alt+K`, which is vim focus up, nor `Alt+Shift+K`, which moves the window), closing
is `Alt+Q` (not `Alt+W`, which is the tabbed layout), and there is no single
"cycle layout" key because sway's layouts are stacking/tabbed/split rather than
dwindle/master/scrolling.

Clipboard history (`Alt+Ctrl+V`) and emoji (`Alt+Ctrl+E`) use the same letters as
Omarchy, and both are built: the history is `cliphist`, fed by `wl-paste --watch`
— the one resident process this session has, about 2 MB — and the picker is a
rofi list over the table `scripts/make-emojis.sh` generates, inserting with the
clipboard plus a virtual Shift+Insert. The capture menu `Alt+Ctrl+C` is built too
(screenshots, annotation, OCR, QR and recording). `docs/omarchy-gap.md` has the
full list.

Everything else lives in the root menu instead of on a key: Places, Media, Time
zone, Free space, End a process, SSH, DNS, Packages, Colour, Reminders, About and
the manual. `Alt+Space` then the first letter is usually enough.
