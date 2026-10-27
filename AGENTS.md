# AGENTS.md

## Project

Build a **Debian arm64** operating system (suite `forky`) for the **Google Hayato** Chromebook
(MediaTek MT8192, board codename `google-asurada`), reusing Nura's hardware support — kernel,
device tree, patches, modules and boot flow — instead of Alpine Linux.

This repo started empty; the scaffolding below is being created incrementally. Treat anything not
yet present in `scripts/` as not implemented.

## Language convention

- **All versioned artifacts are written in English**: source code, comments, commit messages,
  docs, `AGENTS.md`, scripts.
- Conversation with the maintainer may be Spanish and/or English. Artifacts are English only.
- Everything generated is versioned here (build scripts, kernel packaging, device package,
  installer, docs). Do not leave useful work only in `/tmp`.

## Feature rules

- **Icons first, never forced.** Icons and readable TUI tools are the house taste, not a garnish:
  a menu row gets its `UI_I_*` glyph from `~/.config/bin/lib-rofi.sh`, a notification its name from
  the same constants, a `rofi-files` row a `thumbnail://` or a name from the file-browser table, a
  `.desktop` entry an `Icon=` the **LazyOS** theme actually draws, and a terminal tool is picked for
  the same taste (`eza --icons`, `bat`, `glow`, fzf's preview pane, `btop`, `lazygit`) when a
  plainer one only prints the same facts. The order is fixed so the choice is testable: **a glyph
  already in the alphabet, then a name already in a `scripts/make-icons.sh` table, then a new table
  entry with the PNGs regenerated on the build host, then text alone.**
  **"If possible" is part of the rule.** No glyph where the surface cannot render it (an fzf row is
  foot, whose fallback is Symbols Nerd Font Mono, so a Font Awesome codepoint there is a tofu box -
  that feature's picture is its preview pane), no name the theme does not carry (it draws nothing
  and logs nothing), no package or daemon that exists only to draw, nothing that costs alignment,
  legibility or a warning line in `check-system.sh`, and no picture in `check-system.sh`'s own
  report, which other scripts read. A feature with no picture is finished when its text says the
  same thing; a broken row is worse than a plain one, and "this surface cannot draw it" is a reason
  the commit states, not one it leaves implicit.
  `check-system.sh` verifies the theme's files against the names the scripts ask for **and** that a
  font still carries every `UI_I_*` codepoint: a name nobody renders draws nothing at all (a hole
  on the row, a text-only card) and logs nothing. The tables and the mako `icon-path` trap are in
  the UI section, under *The launcher* and *The file browser*.
- **A feature ships with its three witnesses**: a `check-system.sh` line that fails when its
  dependency disappears, a paragraph in `usr/share/doc/lazyos/` (the manual is what a person
  reads on the device), and a bullet in this file that says why it is built this way.
- **Nothing resident.** Menus are one-shot rofi scripts and a viewer is the viewer's own window,
  so the session starts nothing that stays. Two agreed exceptions, both because only something
  watching can answer the question: `cliphist`'s watcher (no history without one) and `workstyle`
  (which windows are in which workspace, see the UI section). A slow action gets a `busy` card
  **and** a bar spinner, says whether it can be stopped, and **ends by showing its own result in
  the task window** instead of a toast that expires.
  The rule is a test, not only a paragraph: `check-system.sh` FAILs when a **declined daemon** is
  running (`udiskie`, `swaync`, `dunst`, `picom`, `blueman-applet`, `packagekitd`,
  `localsearch-3`, `tracker-miner-fs-3` - each is a package the comments and `docs/omarchy-gap.md`
  already answered "no" to) and WARNs when a user unit outside the session's own set (PipeWire,
  WirePlumber, the portals, gvfs, dconf, mpris) appears. The two agreed exceptions are `exec`'d by
  the sway configuration rather than running as units, which is why their checks are process
  checks and not unit checks.
- **The session UI moves to one Rust framework, and this bullet records the direction while it is
  built.** [`docs/plan-ui.md`](docs/plan-ui.md) designs it: Ratatui in foot, one `lazyos`
  multi-call binary, every old path a symlink (the ~55 bindings and the `.desktop` entries keep
  working), a TaskView that keeps the user in the loop (a slow action ends by showing its own
  result, not a toast), a design system, and a theme engine that owns the palette files. Three
  rules change when a surface is ported, and only then: a **TUI row draws a Nerd Font codepoint**
  (Font Awesome does not render in foot, which is why the `UI_I_*` alphabet stays for rofi/Pango
  surfaces); the **palette files are generated** from `theme.toml` (a hand edit is caught by the
  idempotent-regeneration check, never silently overwritten); and **the AI proposes, the person
  confirms** - `lazyos mcp` never exposes an apply-level destructive tool (install, format, swap,
  power), so the MCP manifest is the witness. The framework itself does not exist yet (the
  sixel spike in `rust/crates/spike-sixel` is the one validated piece); `docs/plan-ui.md` §16 is
  the order, and each tier lands with its three witnesses like everything else.
- **Ratatui is not the expensive part, and that is measured rather than assumed**
  ([`docs/optimization.md`](docs/optimization.md) §13). Against a crossterm-only app doing the same
  job it adds 23.6 s to a clean build, 89 MB of `target/`, 328 KB of stripped binary, **no**
  measurable startup and no measurable PSS (233 kB either way; a text-only ratatui app in foot is
  1.0 MB next to foot's 21 MB), and a 152x45 frame costs 154 µs empty - 0.15 % of a core at the
  plan's 10 fps tick. So **there is no ratatui-lite**: the saving would be a third of a megabyte
  and a fraction of a percent of CPU, paid for with Unicode width, wide cells, wrapping, the
  `Layout` solver and terminal quirks, which is exactly the class of silent rendering bug this port
  keeps fixing. What the rule does require: **event-driven redraw (10 fps only while something
  moves)** and **the widget tree built per state, not per frame** (rebuilding 40 formatted rows
  costs 445 µs against 231 µs for one changing cell). Feature-pruning and thin LTO were measured
  too and are not adopted (0 KB, and -9 % for +13 s of build). The cost that *was* real is the
  `image` crate's default codecs: 147 s of build and 2.17 MB of binary against 49 s and 0.73 MB for
  `png` + `jpeg`, so **a codec is opt-in and the commit names the file that needs it** (the spike's
  clean build went 228 s → 115 s and its binary 8.6 → 2.21 MB with that one change).
- **The framework is one multi-call binary, and its shape is settled in four decisions.**
  `lazyos` (spec 001) resolves a command from **`argv[0]` or from the words after it** - a
  symlink named `rofi-files` and `lazyos menu files` reach the same handler, longest word match
  first, unknown names exit 2 with the list - because that is what keeps the ~64 old paths
  working while each surface is ported: the binding keeps its name, the program behind it
  changes. **`LAZYOS_UI=1` is the one guard**: the host sets it on the `foot` it spawns, and a
  process that carries it never spawns another window (with no terminal it reports, exit 3) -
  a guard in each command is a guard one command will forget. **Exit codes are the shell's
  contract**: 0 did something, 1 Escape (nothing happened), 2 usage, 3 failed with the card
  already shown, so `|| exit 0` stays correct. And **the window's shape is table data**: the
  command row names `pick`/`task`/`full`, the host turns it into `foot --app-id=lazyos-<shape>
  --window-size-pixels=<size>`, and sway floats and centres that app-id from
  `config.d/18-framework-windows.conf` - the app cannot do it with `swaymsg`, because it runs
  before foot's first commit (measured on the card: the criteria matched no node and the window
  stayed tiled), and the size is foot's because a `resize set` at map time is overwritten by
  foot's own commit (900x600 rule, 700x496 window, centre still computed from 900).
  **Terminal hygiene is one module**: raw mode, alternate screen, cursor, mouse and sixel are
  undone by `Drop`, by a panic hook and by a `SIGINT`/`SIGTERM`/`SIGHUP` handler (`tcsetattr` +
  `write` + `_exit`, all async-signal-safe - a pty left in raw mode is the same bug in another
  costume); SIGKILL promises nothing. The binary's `--version` is the committed `VERSION`,
  compiled in, so `--version` and `/etc/debian-aarch64-revision` cannot disagree, and
  `check-system.sh` compares them. Cargo's package versions are placeholders for that reason.
- **The design system is a vocabulary, and the tests are what keep it one** (spec 002).
  `lazyos-ui::design` holds `ColorRole`/`Space`/`TextStyle`/`Border`/`Density`; a widget asks for
  `ColorRole::Accent`, never a hex value, and `lazyos-ui/tests/colours.rs` fails when
  `Color::Rgb` appears outside `design.rs` - a widget that names a colour cannot follow the theme
  engine (spec 004), and the palette itself stays in `lazyos-theme` (`colours.rs` also refuses a
  hex literal in `design.rs`). Density (normal/tablet) is read once from
  `$XDG_RUNTIME_DIR/lazyos-tablet` and decides the row stride and the bands, so the touch target
  is a second value and not a second theme. **The shell owns the window**: `App` draws the
  header, the body and the key bar, a command implements `Screen` and draws into `BodyCtx`, and
  `lazyos-cli/tests/frame.rs` (plus the same grep in `check-system.sh`) fails when `lazyos-cli`
  mentions `ratatui::Frame` - one shape instead of thirty. `Screen::key` and `Screen::hit`
  receive the `Design` instead of caching one, because a hit region computed with a stale
  density opens the row above the finger (`tests/app.rs` clicks the third row at both densities
  and the blank line between two touch rows, which must do nothing). Widgets copy cached rows
  into the buffer: 40 rows at 152x45 cost **424 µs** a tick in release and 6.2 ms unoptimised,
  with **0 allocations per frame** (`tests/no_alloc.rs` counts them with a global allocator - the
  445 µs in `optimization.md` §13 was `format!` per row per frame, not ratatui), and the golden
  snapshots (`lazyos-ui/tests/snapshots/`, every widget at 80x24 and 152x45,
  `UPDATE_SNAPSHOTS=1` to regenerate) are what makes a moved border visible in review. Motion is
  one tachyonfx sweep of at most **300 ms** at a 10 fps tick, skipped by
  `LAZYOS_REDUCED_MOTION=1`; nothing loops, and the loop draws on events only - the 10 fps tick
  exists only while the opening animation runs. `lazyos --glyphs` prints the codepoints the
  widgets draw (the frame's and, since spec 003, the row alphabet's) and `check-system.sh`
  asks the fonts for each one: a glyph no font has is a hole in the frame that logs nothing,
  the same failure the menu alphabet already had in another surface.
- **A row's picture is a Nerd Font codepoint, and the table behind it is generated**
  (spec 003). `lazyos_ui::glyphs::Glyph` is the handle a command names
  (`Glyph::Bluetooth`), and `scripts/make-glyphs.sh` is where a name becomes a codepoint:
  it writes `glyphs/table.rs` (variant → codepoint) and `glyphs/font.pin` (family, file,
  SHA256, Nerd Fonts version, drawn/fallback counts) from Nerd Fonts 3.5.1, filtered
  against the character map of `Symbols Nerd Font Mono` with `fc-query`. **The filter is
  the point**: `fonts-nerd-symbols` is DFSG-stripped - no Font Awesome, no MDI plane 15 -
  so eight names (Bluetooth, Wi-Fi, battery, keyboard, mouse, calculator, qr, crop) have no
  codepoint at all and draw a short token instead (`bt`, `wifi`, `bat`, `kbd`, `pad`, `=`,
  `qr`, `crop`), because a box is a hole in the row that logs nothing. `Glyph::cell()`
  therefore always answers something printable, the codepoints stay `\u{...}` escapes in
  the source (a literal private-use character has not survived an editor here before), and
  **the glyph column belongs to the list**: `ListState` reserves the widest cell among its
  rows, so one text token does not shift the labels beside it (the widget tests render a
  list of all three kinds and compare the column the labels start at). `--glyphs` prints
  `U+XXXX<TAB>name` for the frame's characters *and* the alphabet, and `--glyph-font` the
  pin; `check-system.sh` FAILs when the symbol family is missing, when the installed file's
  SHA256 is not the pinned one, when a codepoint is not in that font's own character map,
  and when a fallback is not short text - the frame's characters are asked of the union of
  the installed fonts instead, because they are text and any font that draws them will do.
  The eight fallbacks are documented in `01-welcome.md` § *The windows the framework draws*
  and `06-troubleshooting.md` § *Icons render as boxes*, and that page corrects a claim
  this port carried: foot **does** search fontconfig's dynamic fallback (`foot.ini(5)`), so
  a Font Awesome codepoint renders here when `fonts-font-awesome*` is installed - the
  alphabet is Nerd Font because a row needs the one symbol font the image guarantees and a
  single-cell advance, not because the alternative is a box. A font upgrade that drops a
  codepoint is caught by the pin, and `scripts/make-glyphs.sh` is how that row becomes text
  again.
  **A ported surface is redesigned for the terminal, not re-skinned**: the taste is Omarchy's (one
  accent, soft cards, dim metadata that never competes with the name, a selection that reads as a
  row) drawn with box-drawing, blocks and one Nerd Font glyph per row, at the size the window
  actually opens in - so a menu may come out shorter, wider, two-column or with a preview pane,
  and what a port keeps is its job and its keys, never rofi's one-line row with a 26 px icon.
  **And every list filters as you type with case folded** (`readme` finds `README.md`, `año` its
  `AÑO`): the needle is folded once per keystroke rather than once per row, an ASCII needle takes
  the allocation-free byte path and an accented one the Unicode path, and a growing needle is a
  `retain` over the visible set instead of a rescan - measured 372 µs against 1.74 ms for 5,000
  rows, with allocations that do not scale with the list (`lazyos-ui/tests/filter.rs`,
  `optimization.md` §13). Two matchers would be two case rules, so a surface with a search field
  uses `ListState` and nothing else.
- **The palette is one file, and every surface is generated from it** (spec 004).
  `lazyos-theme` parses `theme.toml` - the 18 colours a terminal asks for by name plus the
  semantic roles a surface asks for (`text`, `accent`, `danger`, `hover`, `faint`, …) - ships
  **eight built-ins** (solarized and the plan's seven), derives a role a theme leaves out, and
  **refuses a palette that cannot be read** (4.5:1 for `text`, 3:1 for the signal roles, the
  failing pair and its ratio named). Eleven surfaces are rendered from templates
  (`rust/crates/lazyos-theme/templates/*.tpl`, placeholders `{{ role }}`, `{{ role:bare }}` for
  `foot.ini`, `{{ role:rgb }}` for `rgba()`, `{{ header }}`): foot, waybar, mako, rofi, starship,
  btop, lazygit, nvim, fzf, swaylock, and the sway wallpaper+frame drop-in. **Solarized's output is
  the session that existed before the engine** - a unit test pins its `Palette` to
  `Palette::solarized_dark()` - so the first `set` on a healthy card is an empty diff, and
  `lazyos theme diff` is that witness on the device (`check-system.sh` FAILs on a non-empty one).
  Writes are **atomic and idempotent**: `<target>.new` in the same directory then a rename, an
  unchanged surface is not rewritten at all (a running foot watches its file), and the last
  generated body is kept in `~/.local/state/lazyos/theme/` so **a hand edit is reported, not
  overwritten** (exit 1 with the differing lines; `--force` replaces it). **A file the engine did
  not generate has no header and is never replaced in silence** - the first line is the engine's
  claim of ownership. `Alt+Shift+T` and `Alt+Shift+W` run `lazyos theme next` and
  `lazyos theme wallpaper next`, which open no window: a command whose whole job is to write a
  file and print a line is `Shape::Direct` in the command table, so neither the `LAZYOS_UI` guard
  nor `foot` has anything to do with it. The templates live in the crate (`include_str!`) so the
  binary works on a checkout with no image tree, and the image ships the rendered solarized files
  because it is staged with `git archive HEAD` - `scripts/make-theme.sh` is the host tool that
  regenerates them, like `make-icons.sh`. Two surfaces stay hand-written on purpose: the
  screensaver's profile (it carries the brand's cyan) and the GTK stylesheets (they are the
  session's, not a terminal's palette).
- **Work ships as a spec, one branch and one PR each.** [`specs/`](specs/README.md) turns
  [`docs/plan-ui.md`](docs/plan-ui.md) §16 into 14 mergeable specs, each naming its scope,
  acceptance criteria, three witnesses, tests and the `opencode go` model that builds it. The
  branch is `spec/NNN-slug`, the PR body is `.github/PULL_REQUEST_TEMPLATE.md`, and **the AI
  never merges its own**: a person reads the diff, the tests and the witnesses. CI is three
  workflows: `Rust` (fmt, clippy `-D warnings`, `cargo test`, release build), `Specs` (every
  spec carries its fields, its witnesses and an index entry, and the branch names its spec) and
  `Release` (a tag checks `VERSION` and the notes, tests, builds aarch64 binaries and publishes
  them; the disk image is deliberately not built in CI because it needs a kernel, a loop device
  and root). A criterion that cannot name how it is observed is not a criterion, and editing
  the criteria to match the code is how a spec stops being one.

## Critical upstream facts

- **Nura is postmarketOS renamed** (rebrand 2026). Old names `postmarketOS`, `pmOS`, `pmaports`,
  `pmbootstrap` refer to the same ecosystem; expect them in sources.
- The live wiki (`wiki.nura.eco`, `wiki.postmarketos.org`) is behind **Anubis** (proof-of-work
  anti-bot): `webfetch` returns a challenge page, not content. Use `git clone` from
  `gitlab.postmarketos.org` (the git protocol is **not** blocked) or the Wayback Machine.
- The `gitlab.com/postmarketOS/*` mirror is **stale**. Always prefer
  `gitlab.postmarketos.org/postmarketOS/*`.

## Target hardware (the install machine)

| Item | Value |
|---|---|
| Model | Google Hayato rev1 (ASUS Chromebook Detachable / Flip CM3 CM3200) |
| SoC | MediaTek MT8192 "Asurada" (4×A76 + 4×A55), Mali-G57 MC5 (Panfrost) |
| RAM | 4 GB |
| Storage | eMMC 58.2 GB (`mmcblk0`) + microSD (`mmcblk1`) |
| WiFi/BT | Realtek RTL8822CE (`rtw88_8822ce` + `rtl_bt`) |
| Input | `cros-ec-keyb` + Elan `04F3:2E70` (i2c-hid) |
| Firmware | stock coreboot/depthcharge, developer mode, vboot2 |

## Boot chain (do not get this wrong)

- Stock ChromeOS firmware (**depthcharge**) loads a **signed FIT "kpart"** from the GPT partition
  whose PARTUUID equals the `kern_guid` in the kernel command line (`p1` in Nura's layout).
- The kpart is built with **`depthchargectl build --board mt8192`** (`depthcharge-tools`) and
  signed with the **vboot developer keys** (`/usr/share/vboot/devkeys`).
- Partitioning uses **`cgpt`** (ChromeOS GPT). The 128 MB kernel partition is required.
- **Never flash the SPI firmware.** Only touch GPT partitions. Write-protect is disabled on this
  device, so a firmware write could brick it.
- Boot from microSD/USB requires `crossystem dev_boot_usb=1 dev_boot_signed_only=0`; the firmware
  then boots the external disk when it is picked in the developer-mode menu (see below).

Nura's proven disk layout (reuse it, and this repository reproduces it): `p1 pmOS_kernel` 128 MB
(kpart), `p2 pmOS_boot` 512 MB (ext2, where `/boot` lives), `p3 pmOS_root` root. It is created in
exactly one place, `scripts/lib-cgpt.sh`, which `write-sd.sh`, `install-emmc.sh` and
`install-kpart.sh` share; the kernel partition flags (`-S 1 -T 5 -P 10`), the partition types
(`kernel`, `efi`, `data`), the labels and the 512 MB boot size are the ones pmbootstrap uses for
ChromeOS (`pmb/install/partition.py`, whose `Config.boot_size` default is 512).

## Repository layout (device-agnostic)

The build is generic on purpose: any device supported by Nura can be ported by adding a
directory, and kernels are shared between devices.

```
devices/<codename>/        one directory per device
  deviceinfo               pmaports-style device description (sourceable shell)
  kernel                   kernel flavor to use (name under kernels/)
  kernel-cmdline.conf      base kernel command line
  modules-initfs           modules forced into the initramfs
  packages.list            device-specific Debian packages
  APKBUILD.reference       upstream device package (reference only)
kernels/<flavor>/          one directory per kernel
  version                  upstream version (e.g. 7.2.7)
  config.aarch64           kernel config (CONFIG_LOCALVERSION gives the release suffix)
  patches/                 *.patch + a `series` file with the apply order
  APKBUILD.reference       upstream kernel package (reference only)
compositors/<name>/        one directory per compositor built from source
  version                  upstream version (e.g. 0.6)
  scenefx-version          the effects library it links against, when it is a fork
  SHA256SUMS               checksums of the upstream tarballs, verified on build
  README.md                what it is, why it is built here, its effects policy
tools/<name>/              one directory per pinned upstream tool the image
                           carries: workstyle (built here from its crate) and
                           opencode (fetched as a platform binary); version +
                           SHA256SUMS, the same shape as compositors/
uis/<ui>/                  one directory per graphical shell
  packages.list            UI packages (Debian names)
  files/                   tree overlaid onto the rootfs; `etc/` is copied as
                           root, `home/` is merged into /home/$USERNAME
  README.md                what the shell ships and how it deviates upstream
brand/                     the mark, as an SVG source: scripts/make-brand.sh
                           renders the icon and the wordmark from it
packages/                  device-agnostic package lists (base.list: what the
                           system needs; tools.list: the CLI tools)
scripts/                   lib.sh (device/kernel/UI resolution), lib-cgpt.sh
                           (the ChromeOS GPT layout), lib-dist.sh (what a
                           published artifact must not carry) + build/install
                           scripts, including make-image.sh / make-release.sh and
                           the two host tools, make-icons.sh / make-brand.sh
reports/                   check-system.sh output captured on real hardware, kept
                           as evidence for the checks, plus the maintainer's
                           own session feedback from a booted card (see
                           reports/README.md)
specs/                     one spec per mergeable change (scope, acceptance,
                           the three witnesses, its tests) and the opencode-go
                           model that builds it; specs/README.md is the process
rust/                      the Rust workspace: lazyos-ui/-theme/-cli and the
                           sixel spike that validated foot before anything
                           depended on it (rust/README.md)
.github/workflows/         Rust (fmt/clippy/tests/build), Specs (the contract),
                           Release (tag -> binaries) and Pages (the site)
docs/                      plan.md, recon.md, optimization.md (measured device
                           and kernel-config findings), plan-rofi.md (what of
                           Omarchy's menus can move onto rofi), plan-agents.md
                           (the agents feature, phased), plan-ui.md (the session
                           framework this port becomes), test-checklist.md (what
                           to try on a fresh card before capturing a report),
                           releases/<version>.md
                           (release notes, reused as the tag message and the
                           GitHub release body), screenshots/
```

Select a device with `DEVICE=<codename> scripts/...`; with a single device it is picked
automatically. Select the graphical shell with `UI=<name>` (default `sway`).

### The image carries this repository

`build-rootfs.sh` stages the **committed** tree (`git archive HEAD`, so never an untested working
copy) into the image at `/home/$USERNAME/Dev/<repo directory name>`, next to a `REVISION` file, and
writes the same information to `/etc/debian-aarch64-revision` (commit, device, UI, kernel,
version, date).
It also installs `check-system.sh` as `/usr/local/sbin/check-system.sh`. This is not decoration:
`install-emmc.sh`, `install-kpart.sh` and the package checks in `check-system.sh` are meant to run
**on the device**, they source `scripts/lib.sh`, and the card that installs to the eMMC is the same
system that has to rewrite its own kernel partition later. A copy found on a card can therefore be
told apart from the checkout it came from - check `REVISION` before trusting it. It is a snapshot:
update it by rebuilding, not by editing it in place. The script warns when the working tree has
uncommitted changes, since those do not travel into the image.

## Vendored reference (pinned)

- Upstream: `pmaports` commit `0e458a2417d6f5ed61c7b70621d5632e30136091` (2026-10-15).
- `kernels/mediatek-mt81/` — mainline Linux **7.2.7** from `cdn.kernel.org` + patches + config.
  Built with `make LLVM=1` (clang/lld).
- `compositors/swayfx/` — **swayfx 0.6** (a fork of sway 1.12.0, which is forky's sway) on top of
  **scenefx 0.5** (tagged `0.5`, meson version `0.5.0`), from `github.com/WillPower3309/swayfx` and
  `github.com/wlrfx/scenefx`; it needs `wlroots >=0.20 <0.21`, exactly forky's 0.20.2.
- `devices/google-asurada/` — `deviceinfo`, `kernel-cmdline.conf`, `modules-initfs`.

## Conventions and gotchas

- The shell in this environment is **ash (busybox)**: no brace expansion (`{a,b}`), no `[[ ]]`.
  Write POSIX sh, or call `bash` explicitly.
- Build/run natively on this aarch64 machine; no cross-compiler needed.
- **`/tmp` is a ~1.9 GiB tmpfs** — too small for kernel/rootfs work. Keep large artifacts on
  the real filesystem; the scripts default to `$HOME/.cache/debian-aarch64`
  (`BUILD_ROOT`, `WORK_DIR`, `SRC_DIR`). `/tmp/opencode` is fine only for small scratch files.
- **Never edit a script while it is executing.** The shell re-reads the file as it goes and can
  die with `unexpected EOF while looking for matching '"'`. Write the new version elsewhere and
  `mv` it into place.
- **Every script under `uis/sway/files/home/.config/bin/` must be committed `100755`.** The image
  is staged with `git archive HEAD`, which takes the mode from the index, so a `100644` script
  arrives in the image readable but not executable; the binding `exec`s it, gets `EACCES`, and the
  error goes to the journal instead of anywhere a person looks, which reads exactly like "the key
  does not work". Three menus of the batch that answered the report (`rofi-power`, `rofi-capture`,
  `rofi-qr`) were committed that way, `check-system.sh` passed them because its menu checks asked
  for `-r`, and only a built rootfs showed it. Both are fixed at the source: `rootfs-setup.sh`
  refuses the build when a file in that directory is not executable, and `check-system.sh` asks
  for `-x` on every one of them, once, instead of in each menu list. `lib-rofi.sh` is sourced, so
  it is the one exception.
- **`pipefail` is set in every script here, so never test the status of `producer | grep -q PAT`**:
  `grep -q` exits on the first match, the producer then dies of SIGPIPE (141) and the pipeline -
  and with it the `if` - reports failure although the pattern *was* found. It depends on how far
  the producer got before the pipe closed, so the same input passes on a fast machine and fails on
  a slow one (`check-system.sh`'s firmware check reported a loaded firmware as missing on the card
  and as present on the host). Capture the output first and use `grep -q PAT <<<"$var"`, or a
  single `grep -rqs PAT <path>`.
- **`pkill -f PAT` also matches the shell that runs it**: `swayidle` executes each `timeout`,
  `before-sleep` and `after-resume` argument with `/bin/sh -c`, so that shell's own command line
  contains `PAT`. An unbracketed `pkill -f org.lazyos.screensaver` kills the shell, and anything
  after the `;` in the same chain never runs (the `night-light resume` added to `after-resume` was
  the first command to notice; the old line had nothing after the `pkill`, which is why it looked
  fine). Write the pattern as `pkill -f "[o]rg.lazyos.screensaver"`: the regex matches the target
  but not the literal text in its own command line. `check-system.sh` FAILs on the unbracketed form
  in the idle drop-in.

### Host toolchain

- `scripts/00-deps.sh` installs the same tool set from either package manager, because the
  reference machine runs this port (Debian) as well as having built it from Alpine: `apk` is used
  when present, `apt-get` otherwise. The Debian names differ in a few places (`rust-bindgen` ->
  `bindgen`, `elfutils-dev` -> `libelf-dev`, `pahole` -> `dwarves`, `vboot-utils` ->
  `cgpt` + `vboot-kernel-utils`, `dtc` -> `device-tree-compiler`, `gptfdisk` -> `gdisk`,
  `musl-dev` -> glibc via `build-essential`). On Debian the kernel's rust needs (`rustc`,
  `rust-src`, `bindgen`, `libclang-dev`) and BTF (`dwarves`) are part of that list too.
- The kernel build also needs the **`openssl`** binary (module signing) and **`findutils`** (the
  kernel's initramfs scripts use `find -printf`, which BusyBox find lacks).

### Debian naming

- `deviceinfo_arch` uses distro/Alpine names (`aarch64`), but **debootstrap requires the Debian
  name** (`arm64`). `lib.sh` maps between them; passing `aarch64` yields
  `Invalid Release file, no entry for main/binary-aarch64/Packages`.

### Kernel staging

- `make zinstall` installs the image as **`vmlinuz-<release>`** only; there is no plain `vmlinuz`
  in the staged tree.
- **`build-kernel.sh` syncs the config on every run.** `kernels/<flavor>/config.aarch64` is copied
  into the source tree unconditionally (kbuild only reads `.config`, and the extracted tree is
  reused), so editing the shipped config cannot be silently ignored - the same class of bug as a
  working tree that never reached the image. The `.prepared` marker holds a hash of
  `patches/series` + every `*.patch` instead of being an empty flag: editing a patch re-extracts,
  while a config-only change rebuilds in place.
- `CONFIG_MMC_MTK`, `CONFIG_DRM_MEDIATEK` and `CONFIG_EXT4_FS` are built-in (`=y`), so the root
  filesystem and the display work without any initramfs module.
- `CONFIG_DRM_PANFROST` (the driver this MT8192 uses) and `CONFIG_DRM_PANTHOR` (newer Mali) are both
  built-in, so every `update-initramfs` prints
  `W: Possible missing firmware /lib/firmware/arm/mali/arch14.8/mali_csffw.bin for built-in driver
  panthor`. It is noise, not a missing piece: initramfs-tools scans built-in modules for firmware
  requirements, and panthor never drives the Mali-G57 here.
- `CONFIG_EXT2_FS` is **not set**, and the `pmOS_boot` partition is formatted ext2 anyway
  (`scripts/lib-cgpt.sh`). That works because `CONFIG_EXT4_USE_FOR_EXT2=y`: the ext4 driver registers
  the `ext2` and `ext3` types, so `mount -t ext2` is served by ext4 and `/proc/filesystems` lists
  ext2/ext3/ext4. Nura's own `/boot` is mounted the same way. Do not "fix" this by enabling
  CONFIG_EXT2_FS.
- Not every `Direct firmware load ... failed` line is a missing firmware. `CONFIG_MTK_SCP=m`, and
  the SCP remoteproc asks for `mediatek/mt8192/scp.img` while only the initramfs exists, so every
  boot logs two `remoteproc remoteproc0: Direct firmware load for mediatek/mt8192/scp.img failed
  with error -2` and one `request_firmware failed: -2`; three seconds later the module is loaded
  from the real root filesystem and the log continues with
  `Booting fw image mediatek/mt8192/scp.img, size 309164` and `SCP is ready. FW version
  asurada_scp_v2.0.20536`. The file is installed (`firmware-mediatek`) and Nura loads the very same
  size, so this is the module being probed after the rootfs, not a missing piece; every function
  this port uses (display, audio, Wi-Fi, Bluetooth, suspend) works with the SCP coming up late.
  `check-system.sh` therefore reports the outcome of the load, not the first attempt.

### Compositor (swayfx)

The session does not run Debian's sway. Plain sway cannot round a window's corners, draw a shadow
under it or blur what is behind it, and this port keeps the effects Deboma gets from
Hyprland/Quickshell: `compositors/swayfx/` pins **swayfx 0.6** (a fork of sway 1.12.0, which is
exactly what forky ships) and **scenefx 0.5**, and `scripts/build-swayfx.sh` builds both into
`$WORK_DIR/swayfx-stage/*.deb`. Nothing in forky provides either, and swayfx needs
`wlroots >=0.20 <0.21`, which forky's 0.20.2 satisfies - no other package is rebuilt.

- **The packages keep sway's names.** `swayfx` installs `/usr/bin/sway`, `/usr/bin/swaymsg`,
  `swaymsg`/`swaynag`/`swaybar`, man pages and `/usr/share/wayland-sessions/sway.desktop`, and it is
  `Provides`/`Conflicts`/`Replaces: sway`. greetd still runs `sway`, `/etc/sway` still holds the
  configuration, and apt swaps it for Debian's sway (and back) instead of refusing.
- **`uis/sway/packages.list` does not list `sway`**, because that would install Debian's plain one.
  A rootfs is therefore useless until `scripts/install-swayfx.sh` has run: the order is
  `build-rootfs.sh` → `install-kernel.sh` → `install-swayfx.sh` → `install-workstyle.sh` →
  `make-kpart.sh` (`build.sh` does all five). On a running system `install-swayfx.sh /` and
  `install-workstyle.sh /` are the counterparts of `install-kpart.sh`.
- **The .deb deliberately does not carry `/etc/sway/config`.** Upstream installs its sample there,
  and `install-swayfx.sh` runs *after* `rootfs-setup.sh` has overlaid `uis/sway/files/`, so a
  packaged copy would replace the solarized, tested configuration. The build deletes `etc/` from the
  staged tree for that reason.
- **scenefx is linked from the staging tree, not from the host.** Its pkg-config file is copied with
  `prefix` rewritten to `$WORK_DIR/swayfx-stage/scenefx-root/usr`, so meson finds the headers and the
  freshly built `.so` without installing anything into the build machine; the soname is unversioned
  (`libscenefx-0.5.so`) and there is no rpath, so the installed binary uses the packaged copy in
  `/usr/lib/<triplet>`. That soname also means there is no ABI symlink and no `-dev` package:
  upgrading scenefx means rebuilding swayfx.
- Both projects set `werror=true`, and `meson -Dwerror=false` overrides it; `--libdir=lib/<triplet>`
  is needed because meson's `libdir` is relative to the prefix (a bare `<triplet>` installs into
  `/usr/<triplet>`, which is where the first build put the `.pc` file and nothing could find it).
  `meson install --strip` strips the binaries.
- **`Depends` is generated, not hand-written.** `dpkg-shlibdeps` wants a `debian/` directory, so each
  `DT_NEEDED` soname is resolved to the real file with `ldconfig -p` and that exact path is asked of
  `dpkg -S`. Asking dpkg for the bare soname is subtly wrong: `libc.so.6` also matches the linker
  script of that name in `libc6-dev`, and a `-dev` package would end up as a dependency of the image.
  `/sbin/ldconfig` is not on a normal user's `PATH`, hence the explicit path in the script.
- **`config.d/26-swayfx.conf` is swayfx-only** (`corner_radius`, `smart_corner_radius`, `shadows`,
  `blur`, `layer_effects`, `default_dim_inactive`): plain sway answers *Unknown/invalid command* and
  `sway -C` fails, which is the cheapest way to notice that Debian's sway is back. **Blur is on**,
  and what makes it affordable is which of SceneFX's two paths a window takes: a tiled window that
  cannot overlap samples a buffer blurred once from the wallpaper (one bilinear tap per pixel,
  0.07-0.11 ms full screen on the Mali-G57), while a floating one is re-blurred from what is behind
  it (2.1-2.2 ms full screen at radius 4 / 2 passes, scaling with its area, only while visible).
  `blur_xray disable` keeps that difference - enabled, every surface samples the cached wallpaper
  and a floating window shows the wallpaper through itself instead of the window behind (verified
  with screenshots). `blur_passes 2`/`blur_radius 4` is a 32 px sample instead of the shipped 80 px
  and is also how far a live blur grows its damage; `blur_noise 0` with neutral
  brightness/contrast/saturation skips SceneFX's second full-screen pass (0.6 ms/frame, and the
  visible grain). Blur is drawn *behind* a surface, so the effect only exists under one with
  alpha < 1: foot 0.87, rofi 0.82 (0.88 on the selected row), waybar 0.71 (0.80 on the
  cards) and mako 0.88 are the carriers, and
  `layer_effects` gives the bar the cached path (only the wallpaper is behind it) and the launcher
  the live one (it floats over windows). Numbers, method and the renderer facts are in
  `docs/optimization.md` §7: PanVK answers *Mali-G57 not supported*, and SceneFX creates its own
  GLES renderer (`fx_renderer_create`), so `WLR_RENDERER` never applies. Corners are cut when the
  buffer is committed and a shadow is a texture; `animation_duration_ms 150` is the only animation
  setting swayfx 0.6 has (no per-event directives, one ease-out-cubic), and it is what sends a
  window down the live path for the length of its animation. greetd starts `sway` with no
  `WLR_*`/`MESA_*`/`LIBGL_*` override, the image ships no `mesa-vulkan-drivers`, and
  `check-system.sh` FAILs when `eglinfo -B` does not name Panfrost - a silent drop to llvmpipe is
  otherwise invisible.
- `sway --version` prints `swayfx version 0.6 (based on sway 1.12.0)`; plain sway prints
  `sway version 1.12.0`. `check-system.sh` FAILs on anything that is not swayfx, and the bar's
  `sway -C` check then validates the effects drop-in along with the rest.

### Userspace / UI (Debian forky)

- **The brand is one mark and one wordmark, and each surface asks for the form
  it can draw.** `brand/README.md` is the brand (mark, wordmark, palette, rules).
  `brand/lazyos.svg` is the hand-drawn **z_** prompt (a cyan `z` and an orange
  cursor on a base02 tile; the wordmark is `lazyOS_`), and `scripts/make-brand.sh`
  renders it into `lazyos.png` in the LazyOS theme's `48x48/apps` (the About row
  and anything that asks for `Icon=lazyos`), the SVG in `scalable/apps`, a
  1366x912 `Pictures/Wallpapers/lazyos.png` from `brand/wallpaper.svg` (the
  panel's real mode, so nothing is resampled: a keyboard in the dark with one
  lit key carrying the mark, and `lazyOS_`, `lazy by design` and the arcade line
  bottom left), `brand/social-preview.png` (the repository's card), and the two
  text forms of the mark - `~/.config/lazyos/wordmark.txt` (fastfetch's logo,
  `$1..$3` colours) and `~/.config/lazyos/screensaver.txt` (the idle screen) -
  the z_ in full and half blocks, every row padded to the same width because
  the idle screen centres line by line. **The
  PNGs are committed**, because the image is staged with `git archive HEAD` and
  a build host without `rsvg-convert` still has to build; only an edit to
  `brand/` needs the script, which is a host tool like `make-icons.sh` - hence
  `librsvg2-bin`/`fonts-jetbrains-mono` in `00-deps.sh`. **The lock screen keeps
  `mountains.jpg`**: the desktop is the surface the brand owns, and the
  photograph is what is up when nobody is looking. **Each surface gets the form
  it can draw**: the PNGs and the SVG where pictures go, the block-character z_
  in the idle screen and fastfetch (box and block characters lay out the same in
  every monospace font; the old ANSI Shadow letters did not), and plain text in
  `/etc/issue`. `os-release` (a regular file: Debian's is a
  base-files-owned symlink, and `check-system.sh` FAILs when the image is back
  to calling itself Debian), `/etc/issue`, the greeter's `--greeting` and the
  `lazyos` hostname are the distribution saying its own name, and the version
  they carry comes from the committed `VERSION` file, read by `build-rootfs.sh`,
  forwarded through sudo into the chroot and recorded as `version` in
  `/etc/debian-aarch64-revision` - a build cannot say 0.2 while the tag says
  v0.3.0. The root menu's `ai` group (Agents and Dictation together, not
  scattered between the openers and the session actions) is the other statement
  the brand makes.
- **greetd**: Debian's package creates the **`_greetd`** system user (upstream docs say
  `greetd`), and its unit declares `Conflicts=getty@tty7.service`, i.e. it expects `vt = 7` in
  `/etc/greetd/config.toml`. With the upstream name the service dies with
  `configured default session user 'greetd' not found` and the machine silently falls back to a
  console login on tty1.
  **greetd does not pass its own environment to the session**: its worker execs
  `/bin/sh -c "exec <cmd>"` with the PAM environment alone (`pam_getenvlist()`), and Debian's
  `common-session` carries only `pam_systemd` here. Without `pam_env` in `/etc/pam.d/greetd` the
  session starts with no `LANG` even though `/etc/locale.conf` sets one - every terminal prints
  `warning: foot: 'C' is not a UTF-8 locale, falling back to 'C.UTF-8'`, and sorting, matching and
  non-ASCII text are on the C locale. `rootfs-setup.sh` appends the two `pam_env.so` lines Debian's
  `/etc/pam.d/login` uses (`readenv=1`, `envfile=/etc/default/locale`), and `pam_systemd` passes the
  result on to the user manager. Do not "fix" this in the foot configuration: foot is right, the
  session is what lacks the variable.
- **SSH host keys**: Debian's `sshd-keygen.service` is gated on `ConditionFirstBoot=yes`, i.e. it
  generates keys only when `/etc/machine-id` was *empty at boot*. Every system here is a *copy* of
  an image (`write-sd.sh`, `install-emmc.sh`), and the copy is written from a tree that already
  generated its id, so the condition is never true again, `ssh-keygen` never runs and
  `ssh.service` dies on every boot with `sshd: no hostkeys available -- exiting.` (hit on both the
  card and the eMMC install; the card kept failing for as long as it had been booted).
  `rootfs-setup.sh` therefore drops in
  `/etc/systemd/system/sshd-keygen.service.d/90-run-when-keys-are-missing.conf`, which resets the
  inherited condition list (`ConditionFirstBoot=`) and requires the ed25519 host key to be
  **absent** instead: the unit generates whatever key type is missing on the first boot that lacks
  them and is a single `stat` from then on. `build-rootfs.sh` ships `/etc/machine-id` empty and no
  host keys, and `install-emmc.sh` blanks the id and the keys of the copy, so each card and each
  install gets its own identity instead of sharing one journal, one DHCP identity and one SSH
  host key (`lib-dist.sh` makes the same choice for published artifacts).
- **Remote login is on by design, and it is documented rather than hidden.** `openssh-server` is in
  `packages/base.list` and Debian enables `ssh.service`, so the machine answers a login prompt on
  every network it joins - with the build's default `brunux`/`debian` and passwordless `sudo`
  until the password is changed. That is the agreed decision (the maintainer keeps it as the
  recovery path: a session that will not start, or a screen that is not usable, is still reachable,
  and a card can put a working kernel partition back over ssh), so it is not to be "cleaned up" by
  disabling the unit without asking. What the port owes it is honesty: `05-system.md` § *Remote
  login (ssh)* says it is enabled, gives the default credentials and the `passwd` /
  `systemctl disable --now ssh` commands, and `check-system.sh` prints an info line when it is
  enabled (a WARN when it is enabled but not running), because a machine whose ssh is expected to
  answer and does not is the other surprise.
- `nmtui` lives in the separate **`network-manager-tui`** package (no longer part of
  `network-manager`).
- `wireless-regdb` provides `regulatory.db`; without it cfg80211 logs
  `failed to load regulatory.db` and the WiFi stays on the world regulatory domain. `iw` is in
  `packages/base.list` for `check-system.sh`: without it the wireless interface and the regulatory
  domain are invisible to the report (it SKIPs both checks). The world domain (`country 00`) on its
  own is *not* a failure - it is what remains until NetworkManager or the access point's 802.11d
  country IE sets one (Nura reaches `country MX` from the AP), so the report says so instead of
  warning.
- `zstd` lets initramfs-tools compress the initramfs with zstd; otherwise it warns
  `No zstd in /usr/bin:/sbin:/bin, using gzip`.
- **The screen can be looked at from another machine, read-only and only through ssh.**
  `wayvnc` (312 kB + libneatvnc 218 kB + libaml 82 kB) serves the session as VNC, started and
  stopped from `~/.config/bin/rofi-vnc` (Settings → *Share this screen*, and the root menu); the
  bar's `custom/vnc` eye appears while it runs (`waybar` reads a state file every ten seconds plus
  `RTMIN+8` when it is toggled, so it is not a daemon), and a right click on the eye stops it.
  **Two decisions are the feature.** It binds **127.0.0.1 only**: wayvnc 0.10 authenticates only
  through `~/.config/wayvnc/config`, where `enable_auth` wants a certificate, a private key and a
  password, and this image ships no such file - a server on the network address would be an
  unauthenticated desktop on every network the machine joins, while the loopback address is reached
  through the ssh login the image already answers (`openssh-server` in `packages/base.list`, with
  `ssh.service` enabled by Debian), which makes the tunnel the authentication and uses the part that
  is already configured and tested. It runs
  **`--disable-input`**: remote input would arrive in this session as a keyboard nobody can
  attribute, and "see what is on the screen" never needs to touch it. Everything else is the house
  shape: `nohup` so the server outlives the menu **and `$!` is wayvnc's own pid** (`setsid` forks
  when it is already a process-group leader, which would leave the pid file naming a process that
  has exited), pid and log in `$XDG_RUNTIME_DIR/lazyos-vnc` so a session that dies takes the state
  with it, the pid trusted only after `kill -0` **and** `/proc/PID/comm` saying `wayvnc` (a reused
  pid is not killed by mistake), and an `ss -ltnH "sport = :5900"` read-back instead of believing
  the pid - which is what separates "started" from "serving" and what the menu's *Not serving* row
  reports. **`busy` is deliberately
  not used**: `bar-busy` drops a card older than five minutes as stale, so a share that lasts an
  afternoon would lose its stop button; the permanent state is the bar's eye and the menu's row.
  Witnesses: `check-system.sh` FAILs on the binary, the script, `--disable-input`, a bind that is
  not `127.0.0.1` and the rows in Settings/root menu, WARNs when the tunnel command is missing from
  the manual, when `style.css` lost `#custom-vnc.empty`, and when a `wayvnc` runs that `rofi-vnc`
  does not own (the menu would say off while the screen is being served). The manual is
  `05-system.md` § *Looking at the screen from another machine*, the way in is `02-rofi.md`, and the
  symptom list is `06-troubleshooting.md` § *The screen is being shared but the viewer shows
  nothing*.
- **Bluetooth**: `libspa-0.2-bluetooth` provides PipeWire's BlueZ SPA plugins, but those are
  only a D-Bus client of `org.bluez`. The `bluez` package is what supplies `bluetoothd`,
  `bluetoothctl`/`btmgmt` and the `bluetooth.service` unit; without it the HCI controller
  (`hci0`, bound to `serial0-0` via `hci_uart` in H5 mode, RTL8822CS firmware loaded) is
  never powered up and there is no Bluetooth at all — no scanning, no pairing, and the SPA
  plugins have nothing to talk to, so BT audio silently does not exist. `bluez` is therefore
  in `uis/sway/packages.list` alongside the SPA plugin. Debian 5.87 has **no `bluez-utils`**
  (the CLI tools ship in `bluez`; `bluetoothd` is `/usr/libexec/bluetooth/bluetoothd`).
  Verify with `bluetoothctl show` (*Powered: yes*) and `btmgmt info`.
- **Audio (MT8192)**: the card `mt8192_mt6359_rt1015p_rt5682` is only usable once its UCM
  **boot sequence** has run. That sequence (`MediaTek/mt8192/.../init.conf`) connects the
  front-end and back-end DAIs (`I2S3_CH1 DL1_CH1`, …); alsa-lib runs `BootSequence` **only**
  on an explicit `set _boot` — never at open or on a verb set — so something must drive it.
  `/usr/sbin/alsactl` does: on card add `90-alsa-restore.rules` and `alsa-restore.service`
  run `alsactl restore`, which for a card with no saved state falls back to the UCM init.
  Without **`alsa-utils`** the PCMs fail to open with `ASoC: no backend DAIs enabled for
  Playback_*`, ACP marks the `HiFi` profile unsupported, WirePlumber picks profile `off`, and
  the only sink is a **Dummy Output** (volume keys change nothing, no speaker sound).
  `alsa-utils` is therefore in `packages/base.list`. Verify with `pactl list cards` (Active
  Profile `HiFi`) and `wpctl status` (a *Built-in Audio Speaker* sink, not *Dummy Output*).
  Confirmed on hardware: the speaker plays a video.
  The same class of bug will hit any card whose routing lives in a UCM `BootSequence`.
- **ALSA-only applications**: Debian's stock alsa-lib configuration has no usable `default`
  PCM; that device is supplied by **`pipewire-alsa`** (in `uis/sway/packages.list`). Without
  it, `aplay`/`arecord` without an explicit `-D`, emulators and some Java/game audio fail
  with *audio open error: No such file or directory* while PipeWire clients (and therefore
  everything using PulseAudio) work fine, which makes the gap easy to overlook.
- **Clock**: Debian enables no NTP client, so a fresh image never synchronises
  (`timedatectl` says *System clock synchronized: no*, *NTP service: n/a*) and only the RTC
  keeps time. `systemd-timesyncd` is in `packages/base.list` for this.
- **Battery gauge**: the fuel gauge (`sbs-battery 8-000b`) registers fine (`battery gas gauge
  device registered`) and its capacity is readable, but it reports `Unknown chemistry: OTC0` and
  the kernel repeatedly logs `power_supply sbs-8-000b: driver failed to report
  'status'/'health'/'time_to_empty_now' property: -5/-6`. This is **upstream, not this port**:
  Nura's own log carries the same messages (1296 of them). What waybar shows is the capacity,
  which works, because its `battery` module reads `/sys/class/power_supply` directly
  (`"bat": "sbs-8-000b"`, see `waybar-battery(5)`) — `upower` is *not* what draws it.
  `upower` is in `uis/sway/packages.list` anyway: WirePlumber asks it for the battery
  percentage, and without the daemon every start logs
  `default: Failed to get percentage from UPower: NameHasNoOwner`.
- **Backlight keys**: `brightnessctl` needs write access to
  `/sys/class/backlight/*/brightness`. Debian's `brightnessctl` ships no udev rule and
  nothing else in the system grants it, so `/sys/class/backlight` stays root-only (0644) and
  the sway `XF86MonBrightness*` bindings fail with *Permission denied* for the session user.
  `uis/sway/files/etc/udev/rules.d/90-brightnessctl.rules` (group `video`, `g+w`) fixes it.
- **Lock key**: the right-most key of the top row emits `KEY_SLEEP`, and that is precisely the
  key systemd-logind turns into a suspend on its own — its button handling consults no polkit and
  no session, which makes it the only suspend path that always works.
  `uis/sway/files/etc/keyd/default.conf` therefore leaves the key alone, and
  `uis/sway/files/etc/sway/config.d/21-lock-screen.conf` binds the keysym the same keycode
  produces (**`XF86Sleep`**) to `swaylock`: the key locks the screen and logind suspends the
  machine ~0.3 s later.
  Nura instead maps `sleep = coffee` — `KEY_COFFEE`, the kernel's lock/screensaver key, whose
  xkb keysym is `XF86ScreenSaver`. Because keyd grabs the keyboard, logind then never sees
  `KEY_SLEEP`, and since nothing consumes `KEY_COFFEE` either, the Lock key there does
  *nothing* (elogind handles only `KEY_SLEEP`, `KEY_SUSPEND`, `KEY_POWER`, `KEY_POWER2`,
  `KEY_RESTART` and `KEY_ESC`). **That remap is what broke suspend on this image**: the first
  boot suspended on the key, and it stopped the moment keyd started grabbing the keyboard
  (`journalctl` shows `Suspend key pressed short.` in the early boots and not one line of it
  afterwards, while `keyd.rvaiya` logs `DEVICE: match ... (cros_ec)`), so the mapping is left
  out.
  Confirmed on hardware: the first report from a booted card records `5 suspend/resume cycle(s)`
  and the Lock key locks the screen and suspends the machine.
  The session can still suspend by itself — `Alt+Shift+Escape` (lock, then sleep; Nura's
  shortcut) and `Alt+Ctrl+Escape` (sleep without locking) run `~/.config/bin/sway-suspend` —
  but that goes through `loginctl suspend`, a D-Bus method guarded by polkitd (stock
  `org.freedesktop.login1.suspend` has `allow_active=yes`), which fails with *Access denied*
  and no output from a session logind does not consider active. The helper therefore walks a
  fallback chain (`sudo -n loginctl suspend`, then `echo mem > /sys/power/state`) and logs
  every attempt to `~/.cache/sway-suspend.log`. Suspending through logind needs the daemon
  (D-Bus activated; Debian keeps `systemd-logind` inside the `systemd` package).
  Reading a keymap dump is misleading here: `xkbcli compile-keymap` prints **X** keycodes,
  which are evdev + 8, so `KEY_SLEEP` (evdev 142) shows up as keycode 150 and
  `KEY_BRIGHTNESSDOWN` (evdev 224) as keycode 232.

- **`uis/sway/`** ports the maintainer's Nura session (solarized theme, Alt as `$mod`, Search
  key opens the file search, waybar, foot) on top of the compositor described above. `rootfs-setup.sh`
  rewrites `/home/brunux` inside the shipped text files to `$USERNAME`, and merges
  `uis/sway/files/home/` into that home.
- **tuigreet**: Debian ships the *original* tuigreet **0.9.1**, which has no `--config`/TOML
  support and no background animations (Nura runs the fork, which has both). The greeter's
  personalisation is therefore expressed as flags in `/etc/greetd/config.toml`; the `matrix`
  backdrop and the `custom_title`/widget positions have no 0.9.1 equivalent. Unknown
  components or colour names in `--theme` are silently ignored (they do not break the
  greeter), and `--remember*` needs `/var/cache/tuigreet` owned by the greeter user.
- **The Chromebook keyboard has no F1-F10**, and that is what breaks virtual terminals:
  `mt8192-asurada-hayato-r1.dts` maps the top row to action keys (`KEY_BACK`, `KEY_FORWARD`,
  `KEY_REFRESH`, `KEY_FULL_SCREEN`, `KEY_SCALE`, brightness, mute, volume). Switching VTs needs a
  real F-key keycode: the kernel maps `Ctrl+Alt+F<n>` to `Console_<n>`, and sway reacts to the
  `XF86Switch_VT_<n>` keysym that the same keycode produces (`srvr_ctrl(fkey2vt)` is part of the
  default xkb keymap). `uis/sway/files/etc/keyd/default.conf` (package `keyd`) supplies them:
  `Ctrl+Alt`+top row = VT 1-10, `Search`+top row = F1-F10. Debian names the binary
  **`/usr/bin/keyd.rvaiya`** (the package enables `keyd.service`, and `/etc/keyd/` ships empty).
  keyd reads its config **only at startup** — `keyd.rvaiya reload` after editing — and
  `backspace+escape+enter` makes it quit and release the keyboard.
- **keyd must not grab `cros_ec_buttons` or the headset jack**: both share the keyboard's
  `0000:0000` id, and keyd counts them as keyboards because they report `KEY_VOLUMEUP/DOWN` (its
  heuristic for laptops whose hotkeys live on a separate node). keyd's virtual device does not
  forward `EV_SW`, so grabbing them would silently kill suspend-on-lid-close, the tablet-mode
  switch and the power button (logind loses it). `default.conf` therefore excludes them; the
  exclusions must precede `k:0000:0000`, because keyd returns on the first matching entry, and
  they are hashes of the device name, its key count and the abs/rel masks (`keyd.rvaiya monitor`
  prints them, and `journalctl -u keyd` logs what got matched).
- **Tablet mode and the sensors are already in the kernel** - nothing has to be written:
  `cros_ec_buttons` reports `SW=3` (`SW_LID` + `SW_TABLET_MODE`), so sway's **`bindswitch
  tablet:on`/`tablet:off`** works (the compositor has `bindswitch`/`unbindswitch`); `iio:device1`
  and `iio:device4` are `cros-ec-accel` (`in_accel_{x,y,z}_raw`), `iio:device2` is
  `cros-ec-lid-angle` (`in_angl_raw`), `iio:device3` a gyro. Orientation is therefore readable from
  sysfs at whatever rate is wanted, and **on-screen keyboards work**: `libwlroots-0.20` exposes
  `zwp_virtual_keyboard_manager_v1` and sway creates it (`wlr_virtual_keyboard_manager_v1_create`),
  so `wvkbd` (826 kB, binary `wvkbd-mobintl`, cairo/pango only) types into the focused client.
  `iio-sensor-proxy` (236 kB) is packaged if the D-Bus orientation API is preferred over reading
  sysfs, and it stays out: the poller this port uses costs nothing outside tablet mode.
- **Tablet mode is `~/.config/bin/tablet-mode`, driven by two `bindswitch` lines**
  (`etc/sway/config.d/28-tablet.conf`): `on` starts `wvkbd-mobintl` (solarized colours, `JetBrains
  Mono`, `-H 300`/`-L 240`, `-R 8`, **opaque**: no `--alpha`, because with transparency the window
  underneath shows through the keys), turns the touchpad off (`swaymsg input
  type:touchpad events disabled` - it is on the back of the screen now) **and the typing keyboards
  too** (`keyboard_ids` enumerates `swaymsg -t get_inputs -r` and names each identifier that is not
  also a switch, `enabled` on exit; the folded keys are on the table and a keystroke there is a
  keystroke), and starts its own **1 Hz poller**
  (`tablet-mode poller`) over the accelerometer; `off` kills both, re-enables **both** input types
  and puts `transform normal` back. The state is a directory in `$XDG_RUNTIME_DIR/lazyos-tablet`
  holding the keyboard pid, the poller pid and the output name, so a session that dies removes all
  three. **`off` restores the rotation and both inputs even when the state directory is gone**: the
  output name comes from `swaymsg -t get_outputs`, not from `$state`, a leftover `wvkbd-mobintl` is
  stopped by name, and both input types are re-enabled - that path is the one a person cannot fix
  without a mouse, so it exists. **The keyboard is switched off only after `start_kbd`
  has confirmed the process is alive**, and never after a failed start: from that moment the
  touchscreen is the only input, so a wvkbd that did not start must leave the physical keyboard on.
  The keyboards are named one at a time because `cros_ec_buttons` is **both a keyboard and the
  tablet switch**: it reports the power/volume keys and `SW=3` on one libinput device, wlroots
  builds both halves over the same handle, and send-events belongs to that handle, so a type-wide
  `input type:keyboard events disabled` closes the fd the switch is read from
  (`evdev_device_suspend`) and `tablet:off` never arrives - the 2026-10-22 card could be folded
  into the mode and not out of it. `keyboard_ids` therefore skips every identifier a
  `type: "switch"` device also carries (the headset jack next to it), and `check-system.sh` FAILs
  both on a type-wide disable in the script and on a switch whose `libinput.send_events` is
  `disabled`. The internal keyboard is `cros_ec`, a device of its own, so the folded keys still go
  quiet. The on-screen keyboard is unaffected by that switch because it is **not an input device** -
  it is a Wayland client of `zwp_virtual_keyboard_manager_v1`; and the **Lock key still suspends**
  because logind reads `/dev/input` itself, so a send-events change in sway never reaches it.
  **The keyboard belongs to tablet mode and nowhere else** - `check-system.sh` warns when
  `wvkbd-mobintl` runs while the mode is off, because nothing else starts it. `wvkbd` 0.15 is the
  version in forky: it has `-H`, `-L`,
  `-R`, `--fn`, the colour flags and `--alpha`, and **no `--dock`/`--ratio`/`--auto`** (later
  releases); the compositor places it as a layer-shell surface at the bottom edge. Its layout is the
  mobile-international one, and there is **no `latin-american` layer** among `--list-layers` (`full`,
  `fullwide`, `special`, `simple`, `dialer`, cyrillic/arabic/persian/greek/georgian/hebrew, `emoji`,
  `nav`, `landscape`, …) - accents come from its `Cmp` key, which opens that key's variants for a
  tap (`Cmp`+`a` → a row of `á à â ä …`; `Cmp`+`?` → `¿`), a variant layer and **not** a dead-key
  compose sequence. **The rotation mapping is a table in `orient()`, and the card's first report
  said the screen came up upside down**: the accelerometer in this panel sits 180 degrees around
  from the frame the first table assumed, so an upright device reads **-Y**, not +Y, and the four
  in-plane rules are the mirror image of the assumption (the flat rule is unchanged). Screen-up
  gives `+Z`, so flat `+Z` → `normal`, upright `-Y` → `normal`, `+Y` → `180`, `+X` → `90`
  (`transform` is clockwise), `-X` → `270`, and no axis dominant → keep the current one; the
  thresholds are `0.85` (flat) and `0.75` (an axis, ~41° off vertical) so tilting does not flip
  the screen. **Because a sign in a comment is a guess until it is checked on a unit, there is a
  calibration instead of a hardcoded frame**: `tablet-mode calibrate` (a row in Settings while the
  mode is on, "Calibrate rotation") reads the accelerometer in the pose the person is holding -
  upright, screen facing them - and writes the direction that must look upright to
  `$XDG_CONFIG_HOME/lazyos/tablet.conf`, one line, `normal=y-`. `orient()` reads that file on every
  pass and derives all four positions from the single reference: the readings run around the device
  in the counter-clockwise order `+X, +Y, -X, -Y`, so each 90 degrees the reading turns is 90 of
  transform, the reference getting `normal` and the rest 90/180/270 in that order - which is
  exactly the table above for a `-Y` reference and needs nothing new for a unit that reads `+Y` or
  is held upright in portrait (`x+`/`x-`). The file is **optional** (no file means the table above),
  a malformed one is ignored silently (`orient()` runs once a second from the poller, where a
  notification would be the wrong answer), and deleting it is the way back. `tablet-mode
  calibration` prints which mapping is in use, `calibrate` refuses to store anything while the
  device is flat or halfway between two positions (both say "hold the screen upright"), and it
  applies `normal` at once, because the pose that was just calibrated is the one that has to look
  upright. Nothing of this is resident: the poller is still one process while the mode is on and
  nothing afterwards. **The acceptance test is on hardware** (the 2026-10-21 card folded once, the
  screen came up upside down and the physical keyboard kept typing): the inverted default, the
  keyboard switch, the calibration and the restore paths are what to try, and the manual section is
  `03-shortcuts.md` § Tablet mode.
- **Nerd Fonts / waybar icons**: Debian has no Nerd-patched Adwaita Mono, so the theme uses
  `JetBrains Mono` for the text. Beware `fonts-nerd-symbols`: it is **DFSG-stripped** and only
  ships Octicons, Codicons, Devicons, Seti and Powerline. It has **no Font Awesome
  (U+F000-U+F2FF) and no Material Design Icons (plane 15)** — which is exactly what the theme's
  icons are — so on its own every icon renders as a *tofu* box with the hex codepoint inside.
  `fonts-font-awesome-legacy` and `fonts-materialdesignicons-legacy` supply those two sets and
  fontconfig's fallback then resolves every glyph, with no change to the configuration.
  Verify with the engine waybar actually uses:
  `pango-view --no-display --font="JetBrains Mono 13" --output=/tmp/i.png <text>` (package
  `pango1.0-tools`) and look at the PNG.
  **The same class of box is text in another script**, and it is a different package:
  `fonts-noto-core` (42.5 MB) is in `uis/sway/packages.list` because `fc-list :lang=ja`, `:lang=hi`
  and `:lang=th` answered nothing and a file name, a web page or an EPUB in Arabic, Hebrew, an
  Indic script or Thai was a row of boxes. It covers the non-CJK Noto set; `fonts-noto-cjk`
  (91 MB) is deliberately out and documented as an `apt install` in `04-software.md` (§ *Text in
  other languages*) - it is the largest item left after the browser and a Latin card should not pay
  for it. `check-system.sh` WARNs when `fc-list :lang=ar` is empty (one line, not a font-by-font
  audit).
- **Wi-Fi profiles**: Nura and Debian name the wireless interface differently (Nura: `wlan0`,
  Debian/systemd: `wlp1s0`), so a NetworkManager profile copied verbatim is never activated.
  `scripts/wifi.sh import` drops `interface-name=` to bind the profile to any Wi-Fi device. The
  profiles hold the PSK in clear, so they live in `$WIFI_DIR` (`$BUILD_ROOT/wifi`) and are
  git-ignored; `build-rootfs.sh` imports them automatically when that directory exists.
- **sway `-C` validates headlessly** even inside a chroot (useful before booting):
  `WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 sway -C -c /etc/sway/config`.
- **Xwayland**: sway starts it on its own (it is not in the sway configuration, so the "every
  command the configuration invokes is installed" check does not see it). Without the `xwayland`
  package each session logs exactly two red lines —
  `[wlr] cannot find xwayland binary "/usr/bin/Xwayland"` and
  `[sway] Failed to start Xwayland` — and X11-only programs (emulators, Wine, some Java/Steam
  games) cannot run at all, while everything Wayland-native works. Nura ships Xwayland, so
  `xwayland` is in `uis/sway/packages.list` (25 extra packages: the X libraries);
  `check-system.sh` warns when it is missing, unless the session deliberately disables it with
  `xwayland disable`.
- **workstyle gives every workspace pill the icons of what is in it**, and it is the session's
  second resident process after `cliphist`'s clipboard watcher. Each workspace is renamed to
  `"<number>: <icons>"` (bare `<number>` when it is empty), `~/.config/waybar/config.jsonc` draws
  `"{value}"` - the whole name, because `{name}` is only what follows the colon and would drop the
  number - and the keys keep agreeing with the pills because every binding says
  `workspace number 3`. Debian packages no workspace renamer, upstream's answer is
  `cargo install workstyle`, and the binary Nura ran was linked against musl (it cannot run against
  this image's glibc); `tools/workstyle/` therefore pins the published **0.8.2** crate (git master
  still says 0.3.0 against `swayipc` 2.7, and 0.8.2 is the release that has the TOML config and
  `deduplicate_icons`) and `scripts/build-workstyle.sh` builds it into a `.deb` with
  `cargo --release --locked` - the tarball carries its `Cargo.lock`, so the fetched crates are the
  versions that release was tested with - and a generated `Depends`. It earns the exception the way
  the clipboard watcher does, and no more: nothing can answer "which windows are in this workspace"
  without watching, it blocks on sway's event stream (0 CPU ticks in 60 s), it costs **1 MB of PSS**
  (2.4 MB of RSS, 4.7 MB at peak), and it re-reads its config on every event, so an edit lands on the
  next switch (method and numbers: `docs/optimization.md` §11). **Three patched
  lines** (`tools/workstyle/patches/`) are each a failure this image would otherwise live with: an
  unmapped window logged at `error!` becomes `debug!` (the fallback glyph is the visible signal,
  and any app nobody mapped would otherwise add a journal line per workspace event); a second
  instance that finds the lockfile taken exits 0 at debug level instead of 1, because `exec_always`
  reaches that line on *every* `swaymsg reload`; and the reconnect loop - which exists for
  `swaymsg restart` - gives up after 60 tries, so a daemon orphaned by a logout cannot retry once a
  second until the next boot. **The bar's `persistent-workspaces` turned out to be dead config**:
  `"*": [1, 2, 3, 4, 5]` is the *Hyprland* module's shape and the sway module matches that list by
  workspace *name*, so it drew nothing at all - and with workstyle it would draw a duplicate pill
  beside every renamed workspace, which is why it is gone rather than fixed. **The glyphs are the
  `UI_I_*` alphabet** from `lib-rofi.sh` (terminal, globe, folder, image, video, document, and a
  window outline for "nobody mapped this one"), written as `\uXXXX` escapes so the file stays
  ASCII - a literal private-use character does not survive every editor, which is how an earlier
  glyph table in this repository shipped as a row of empty spans - and `check-system.sh` FAILs on a
  codepoint the alphabet does not carry, because nothing else checks whether a font draws it and a
  pill with no glyph logs nothing. Patterns are case-insensitive substrings of the title, then the
  app_id, then the class, first in the file wins: `firefox` sits above `foot` so a browser tab whose
  title mentions a terminal is still a browser, and `popup`/`lazyos-find`/`lazyos-welcome` are foot
  windows with their own app_id and are named above `foot` for the same reason.
  `deduplicate_icons = true` keeps a pill the width of a number. Renaming is what breaks
  name-addressed workspaces, so the same check FAILs when a binding or command names a workspace
  instead of using `number`. `install-workstyle.sh` installs with `dpkg --root` (no chroot: its only
  dependencies are the C library and the compiler runtime), and the manual paragraph is
  `01-welcome.md` § *What is on the screen*.
- **The launcher, the window switcher and the bar's dmenu are all `rofi` 2.0** (Debian's `rofi`;
  +6 packages / 1.75 MB over the previous set). `$mod+d` = `rofi -show drun` (the
  `.desktop` entries, including the port's own `usr/share/applications/org.lazyos.*`), `$mod+Tab` =
  `rofi -show window` (wlroots foreign-toplevel; sway has no switcher of its own), `$mod+Shift+d` =
  `rofi -show run` (`$PATH`; **Shift+Enter** runs the choice in foot - rofi's own binding, and the
  only way the TUIs from `packages/tools.list` start, since a TUI without a tty exits), and
  `bar-pick` is `rofi -dmenu`. It replaced tofi (launcher) and fuzzel (bar popups), so the session
  has one picker instead of two; measured PSS while open: drun 15.7-17.1 MB, dmenu 14.2-15.6 MB,
  window 16.7 MB, against **40.5 MB for each fuzzel popup** it replaced and 62.1 MB for wofi.
  **The Chromebook Search key opens the applications** (the same `$launcher` as `$mod+d`), and
  **`Alt+Search` is the file search** (`rofi-files --search`: one bounded `fd` run over `$HOME`).
  The two were the other way around for a while, and this is the maintainer asking for the
  magnifier to show all apps again: the file list moved one chord over to `Alt+Search`, the
  terminal filter went back to `Search+Shift`, and the windowed manager (favnyr) took
  `Alt+Ctrl+Shift+F`, the Shift variant of the Files key. The Search-key bindings
  live in `~/.config/sway/config`, not in `19-launcher.conf` (which documents the launcher
  alone), and both release bindings are `--release --no-repeat` because the top row doubles as
  F1-F10 with Search held down (keyd). `Alt+d` stays the launcher and was never unbound: the
  overlay used to carry `unbindsym Mod1+d`, which is why the key did nothing, and it is gone.
  **`Alt+Search` has two paths so either press order works**: keyd's `[alt]` layer maps the Search
  key to `F20` (`leftmeta = f20`), and sway binds `F20` on the press, so holding Alt and tapping
  Search opens the file list immediately and keyd consumes Search (no release binding fires).
  Pressed the other way around, Search reaches sway as `Super_L` and the `--release Super_L+Mod1`
  binding catches it when Search comes up with Alt still down; without Alt it falls through to
  the launcher. A missing `fd` makes `rofi-files` say so and browse `$HOME` instead, so the chord
  is never a key that does nothing.
  **Icons: a font glyph in the lists, an image in the launcher.** The lists this port draws put a
  Font Awesome codepoint in the row text (`~/.config/bin/lib-rofi.sh`), so they cost nothing, follow
  the palette and are filtered away by typing; the launcher and the window switcher show the real
  `.desktop` icon, **13.2 MB of PSS for ten rows** (the earlier 57.1 MB figure came from a run that
  decoded far more). **rofi 2.0.0-0.2 decodes those icons with gdk-pixbuf, not glycin**: the binary
  links `libgdk-pixbuf-2.0-0` and has no `glycin`/`sandbox`/`bwrap` string in it, and forky ships
  no glycin package here, so the `bubblewrap` that `uis/sway/packages.list` used to carry "for
  glycin to sandbox the decoder" was sandboxing nothing and is gone. A rofi `-show-icons` row is
  decoded when it is drawn, so the picture costs memory only while the window is open.
  **Adwaita 51 ships no application icons** - only monochrome `-symbolic` files,
  and not one of this session's names - so the launcher showed a hole where an icon should be, and
  `papirus-icon-theme` is 190 MB installed. `scripts/make-icons.sh` renders the same Font Awesome
  codepoints into 48x48 PNGs that ship as the small **LazyOS** theme
  (`uis/sway/files/usr/share/icons/LazyOS`), whose `index.theme` inherits Adwaita and hicolor, so an
  application that brings its own icon still shows it in colour. The theme carries **three**
  tables now, not two: the launcher's `.desktop` names, the command palette's binary names, and
  the **names the rest of the session asks for by freedesktop name** - the file browser's row
  icons (`folder go-up view-hidden text-x-generic text-html application-pdf image-x-generic
  video-x-generic audio-x-generic application-x-executable package-x-generic x-office-document
  dialog-warning`) and the ones a notification draws (`dialog-information dialog-warning
  dialog-error network-server display-brightness input-touchpad input-keyboard
  preferences-desktop-theme preferences-desktop-effects preferences-system-windows`). rofi's
  dmenu resolves `\0icon\x1f<name>` through `icon-theme: "LazyOS"`, so the launcher and the file
  list share the alphabet. **mako does not**: it reads no theme metadata, it walks the roots of
  its `icon-path` (`mako(5)`: *"This approximates the search algorithm used by the XDG Icon Theme
  Specification, but does not support any of the theme metadata"*), and `/usr/share/icons` is not
  a theme root - so `~/.config/mako/config` has to name
  `icon-path=/usr/share/icons/LazyOS:/usr/share/icons/hicolor` or every notification icon stays
  blank while `notify-send -i folder` looks perfectly correct on the command line. The call has to
  ask for the picture as well: a `notify-send -a APP` without `-i` is a text-only card with no log
  line, and the file browser's eight messages were the last ones without one, so `check-system.sh`
  warns when an application-named call in `~/.config/bin` has no icon. Two traps that
  cost a round each:
  a literal private-use character does not survive every editor or transport (the first attempt
  shipped a row of empty spans), and `printf '\uXXXX'` needs bash - dash has no `\u`, so the first
  `make-icons.sh` under `#!/bin/sh` rendered nothing; the script is bash and now fails loudly on an
  empty canvas. `bar-pick` passes `-no-show-icons`. rofi 2.0 is a
  native Wayland layer-shell client - no Xwayland - and, like tofi, not a daemon: nothing stays
  after the window. Two behaviours the configuration depends on were verified, not assumed:
  `rofi -dmenu` exits 1 on Escape (`bar-pick`'s callers' `|| exit 0`), and drun honours
  `Terminal=true` through the `terminal` setting (`foot`).
  **A menu has to grab the keyboard, and a layer surface does not do that by itself**: sway
  evaluates its bindings *before* it forwards a key to the focused surface
  (`sway/input/keyboard.c`, `handle_key_event`), and the only exemption is an active
  `zwp_keyboard_shortcuts_inhibitor_v1`. rofi asks for one only when `global-kb` is on, and that
  defaults to off (`config/config.c`), so before `config.rasi` set `global-kb: true` every binding
  of the session kept working *behind* the open menu: `Alt+1` switched to workspace 1 instead of
  reaching `-kb-custom-1`, and `Alt+q` closed the window behind the list. It is one line for the
  whole session instead of one per script, the inhibition lasts exactly as long as the surface has
  focus, and `check-system.sh` warns when the line is missing - the failure is otherwise silent,
  because the menu still opens and answers with the wrong action. The theme is
  `~/.config/rofi/{config,solarized}.rasi`, and **a parse error is only reported when a window
  opens** (a dialog naming file, line and column); `rofi -dump-config` does not report theme parse
  errors, so the check is `rofi -show drun` in the session. `bar-pick` has two modes because rofi's
  dmenu has to be told which one it is: a list (`-no-custom`, only the offered lines can be picked),
  free text (`-free`, a search term) and the Wi-Fi password (`-password`, with stdin from /dev/null -
  otherwise rofi waits on the tty for EOF and the prompt never appears). **A rofi script mode is
  called once at startup and again on selection, never while the query changes** (verified with a
  script that logged its calls), which is why the calculator is a one-shot prompt and not a live
  preview; the plan that assumed otherwise is corrected in `docs/plan-rofi.md`. `grimshot` is a package of its own. `pactl` (the
  XF86Audio* keys) comes from `pulseaudio-utils`, not from `pipewire-pulse`.
- **The file browser is a script over rofi's dmenu, and every row carries an icon.**
  `rofi -show filebrowser` *does* exist in 2.0.0-0.2 (`sorting-method`, `directories-first`,
  `show-hidden`, even a `recursivebrowser`), but with `-show-icons` it generated **no**
  thumbnails here - `~/.cache/thumbnails` stayed untouched and every row kept its mimetype icon -
  and it would replace the markup rows and the `bat`/popup behaviour below, so the dmenu path is
  the one that works. `rofi-files` (`Alt+Ctrl+F`, "Files" in the root menu and the launcher)
  lists one directory per run with `find -L ... -printf` as
  `rofi -dmenu -markup-rows -show-icons`, and Enter reopens the script in the chosen directory,
  reads the file with `bat --paging=always` in the floated `popup` foot window (`BAT_THEME=ansi`,
  since foot runs the command directly and never sources the profile) or hands it to the program
  that fits it. **The browser remembers where it was**: the last directory browsed, or the folder
  of the last file opened, is one line in
  `${XDG_STATE_HOME:-$HOME/.local/state}/lazyos/files-last`; a missing file starts in `$HOME`,
  never in `$PWD`, which is wherever the session happened to be and read as the browser
  forgetting. `--search` is defined to start at `$HOME` and carries a `from_search` flag, so its
  startup line does not overwrite the memory - otherwise every search would send the next
  `Alt+Ctrl+F` home. `remember()` ignores every write failure: a read-only state directory must
  not stop a file from opening.
  **Where it goes is the extension, not a guess**: a **document to `papers`** (PDF,
  DjVu, a comic: search, zoom, printing and annotations, where a PDF used to be a 280 MB browser
  tab; poppler only feeds the row's thumbnail), a **picture to `imv`** (the GPU viewer, with
  zoom/pan/rotate and a slideshow of the folder) with **`chafa`'s sixels as the fallback** for a
  GPU where imv gets no EGL context (~16 MB of PSS for a 6000x4000 photo against a browser window
  of hundreds of MB), text to `bat`, a **video to mpv** (a container is what chafa answers with
  *unknown file format*, so a real player was always the answer: `setsid mpv --no-config
  --no-resume-playback --force-window=yes --keep-open=no --title=<name> -- <path>`, no
  `~/.config/mpv` so a profile from another machine cannot decide anything, **plus
  `--script=/etc/mpv/scripts/mpris.so`** because `--no-config` also empties the script directory
  mpv-mpris installs into - without it playerctl, the Media menu and the bar see no player, which
  is silent), an HTML file still opens as text because it *is* text, and a `.deb` is the browser's
  download prompt. **The image family is decided before the text test**, or an SVG - which is a
  text file - would open as source in `bat` and could never be looked at. The details that make
  the rows work: `rofi -dmenu`
  draws a row's icon from the `\0icon\x1f<value>` suffix, and a value that starts with
  `thumbnail://<path>` makes rofi call `-preview-cmd` (`lazyos-thumbnail`) for the rows it is
  about to draw and keep the result in `~/.cache/thumbnails/normal/<md5 of the path>.png`,
  skipping the call when that file exists (verified: two runs, one call); **every** row also
  carries a theme icon name when it has no thumbnail, because a row without one loses the 32 px
  icon column and its name starts further left - a directory of source files was a bare,
  unstyled list, which is the bug the report named. The name comes from the extension plus the
  executable bit, both already in the listing (`%m`), never from a `file` per row: `folder` for a
  directory, `go-up` for the parent, `view-hidden` for the dotfile switch, `dialog-warning` for
  a broken link, `thumbnail://` for jpg/jpeg/png/gif/webp/bmp/tif/tiff/avif, the video containers
  (mp4/mkv/webm/mov/avi/m4v/mpg/mpeg/ogv/wmv/flv/3gp) and pdf, `application-pdf` for the
  formats papers reads but lazyos-thumbnail has no decoder for (djvu/djv/cbz/cbr/cb7/cbt/ai),
  `text-html` for
  html/htm/xhtml/shtml, `x-office-document` for odt/ods/odp/odg/doc/docx/xls/xlsx/ppt/pptx/rtf/
  epub, `application-x-executable` for sh/bash/zsh/fish/py/pl/rb/lua/awk/deb/appimage/run/bin/
  exe/elf/o/so/a/ko and for anything unrecognised, `package-x-generic` for tar/gz/tgz/bz2/tbz2/
  xz/txz/zst/zstd/zip/7z/rar/lz/lz4/lzma/iso/img, `audio-x-generic` for mp3/flac/ogg/oga/opus/
  wav/m4a/aac/wma/aiff/aif/mid/midi, `image-x-generic` for an image format ffmpeg cannot turn
  into a thumbnail (svg/heic/heif/jxl/raw/cr2/cr3/nef/arw/dng/psd/xcf/ico/icns - asking for a
  thumbnail there would never produce one), `text-x-generic` for txt/log/out/err/nfo/md/rst/csv/
  tsv/json/yml/yaml/toml/ini/cfg/conf/env/lock/list/xml/rasi/desktop/service/rules/sql/diff/
  patch and for the common source extensions, and for a name with no extension the executable
  bit decides (`application-x-executable` for a program, `text-x-generic` for a README). Every
  one of those names is a PNG in the LazyOS theme (`scripts/make-icons.sh`, third table). dmenu
  matches the **markup-stripped** text, so the dim size/date do not get in the way of typing
  (verified: `-filter span` matches nothing); `-L` makes a symlink to a directory navigate and
  leaves only broken links as `l`; the name column is padded to the longest name because the
  theme is monospace; and text is told from binary by the first 4 kB, because bat would render a
  binary as an empty page. **`rofi-files --search` is Alt+Search's mode**: one bounded `fd`
  run over `$HOME` (`--type f --max-results 500`, `--exclude .cache .git node_modules
  .local/share/containers`, hidden only with `--hidden` - Debian's binary is `fdfind` and the
  image also has `/usr/local/bin/fd`), one `stat` for the metadata of whatever it found, sorted
  newest first, with the `~/relative/path` as the row's name so typing filters it and Enter
  opening the file exactly like a browser row; Escape closes either mode with nothing, and a
  missing fd falls back to browsing with a notification. The cost is bounded by construction:
  501 rows (500 matches + the dotfile switch) of a 700-file `$HOME`, 92 ms end to end on this
  machine, no subprocess per row.
- **`Alt+1` in the file browser acts on the row, and it needs two halves.** `rofi -dmenu
  -kb-custom-1 Alt+1` exits **10** instead of 0 when that key is used, so `rofi-files` reads `rc`
  (`rc=0; choice=$(...) || rc=$?; case $rc in 0|10) ;; *) exit 0 ;; esac`) and only then decides
  between Enter and the action; the key is resolved *after* a typed path has been turned into a
  row, so both routes reach the same dispatch. `~/.config/bin/rofi-file-actions FILE` is the other
  half: a note naming the path, then only the rows that fit it - `du -sh` for a directory, pack
  into `name.tar.xz` beside it, unpack an archive into a folder named after it, `udisksctl
  loop-setup` + `mount` for an image (and the browser opens where it landed), ffmpeg to MP4 or MP3
  for a video or a song, the path to the clipboard, a terminal in the directory. Three rules hold
  it together: **nothing is touched until a row is picked** (the menu is built from the listing, no
  probe runs per row), **an existing name is refused rather than overwritten** - including the
  conversion whose output would be its own input, which becomes `name-converted.ext` instead of
  writing over the film - and **the failed output is removed**, so a half-written file is never
  left looking finished. The four long ones (`du` on a tree, packing, unpacking, converting) are
  `busy` cards with the bar spinner and each carries its own `BUSY_STOP` pattern (`pkill -f
  "[t]ar -caf"; pkill -f "[x]z"`), because the script is blocked on that child and a pattern is
  the one handle it can leave behind: the click kills the tool, the step fails, `busy cancelled`
  answers 0 and the menu reports nothing instead of half a result. The rest (copy the path, a
  terminal, the `udisksctl` mount) finish in milliseconds and would only flash a card. `rofi-files` asks for
  `$HOME/.config/bin/rofi-file-actions` and not `$dir`, the same convention it already uses for
  `lazyos-thumbnail`, because a row must not depend on which directory is being browsed.
- **A thumbnail is rofi's own cache plus one script.** A row is
  `text\0icon\x1fthumbnail://<abs path>`; with `-show-icons` and
  `-preview-cmd 'lazyos-thumbnail {input} {output} {size}'` rofi calls that script for the rows it
  is about to draw and keeps the result in `~/.cache/thumbnails/normal/<md5 of the path>.png`
  (128 px), **skipping the call when the file already exists** (verified: two runs, one call), so
  a picture is decoded once, ever. `~/.config/bin/lazyos-thumbnail` is the whole generator -
  ffmpeg for an image or a video frame (`-ss 1`, one frame, retried at 0 for a clip shorter than
  a second), `pdftocairo` page 1 for a PDF, nothing for a format it does not know (that row then
  has no icon) - and three things about it are not obvious. rofi starts **one process per visible
  row at the same time** (8 within 30 ms), which is what makes the 8 cores useful (ten 1280x800
  PNGs in 0.97 s through the gate against 2.0 s one after another), but a 24 MP PNG decodes into
  ~131 MB, so the script gates itself to **four** decoders with `flock` slots (the kernel releases
  a slot when the process dies, so a killed run cannot leave the browser waiting), passes
  `-threads 1 -filter_threads 1` (the parallelism is across files, not inside one) and `-lowres
  2` for a JPEG (53 ms / 50 MB against 179 ms / 83 MB for 24 MP). It writes a `.part` file and
  renames, because rofi reads the cache path directly and a killed decode must not leave a
  truncated PNG that looks cached. And the rows are built by awk: **mawk truncates a format
  string at `\0`** (`printf "a\0b"` prints only `a`), so the separator is
  `sprintf("%cicon%c", 0, 31)`, the one form that works in both mawk and gawk (the image has
  mawk). rofi hands back the row text up to the NUL, so the table-to-choice mapping still works
  with the suffix present (verified with `-auto-select`). The 32 px is **per menu**
  (`-theme-str element-icon { size: 32px }`); the launcher and the bar popups keep the theme's
  26 px. Cost: 23 MB of PSS for a window of fourteen thumbnail rows against 17 MB for the same
  window without them, and nothing afterwards; the numbers are in `docs/optimization.md`.
- **The picture fallback is `chafa`, and `xdg-open` is the shim.** When imv gets no EGL context,
  `foot --hold --app-id=popup -e sh -c 'sleep 0.2; exec chafa -f sixels "$1"'` draws the picture
  with sixels, which foot renders natively: ~16 MB of PSS for a 6000x4000 photo, against a
  browser window of hundreds of MB (280 MB installed). The `sleep` is not decoration - a terminal
  starts at 80x24 and resizes only after the window is mapped, so chafa would measure the wrong
  size and paint into a corner (the same lesson as the screensaver), and `--hold` keeps the
  window up after chafa has drawn once and exited. `uis/sway/files/usr/local/bin/xdg-open` is the
  two dozen lines that send a URL to `firefox-esr` and a path to `lazyos-view` (the same rule
  table a double click uses, so a file opened from a script lands where it would from the
  desktop); it is what foot's ctrl+click and the CLI tools expect, and there is no desktop portal
  here. `check-system.sh` fails when the browser, the shim, `imv`, `papers`, `mpv`, the
  `mpris.so` plugin, `ffmpeg`, `pdftocairo` or `chafa` is missing, and warns when the thumbnail
  cache is unwritable or `rofi-files` stops passing `-show-icons`/`thumbnail://` - a dropped flag
  makes the list silently text-only.
- **Files exists twice on purpose: `rofi-files` is the list and `favnyr` is the window.** The
  file list (`Alt+Search`) and `Alt+Ctrl+F` open the list (one process, no window);
  **`Alt+Ctrl+Shift+F`** opens favnyr, and the launcher and the root menu carry both rows - **Files**
  and **Files (window)**. favnyr is one statically linked Slint binary (Brunux/Favnyr-aarm64
  v0.3.6, 36 MB at `/usr/bin/favnyr`), committed in `uis/sway/files/usr/bin/` rather than
  installed from a package: it links no GTK and no libgio, so it is not in `uis/sway/packages.list`
  and no dependency pulls it in. `gvfs` still adds the trash (`gvfsd-trash`) and the volume
  monitor, and `gvfs-backends` is deliberately out (that is where `samba-libs` and the
  mtp/gphoto backends come from), so the Network places are off. **It has no `terminal=`
  setting**: "Open terminal here" runs `$TERMINAL` and then a list of known emulators (kgx,
  konsole, gnome-terminal, kitty, alacritty, wezterm, xfce4-terminal, lxterminal, tilix, xterm)
  that does **not** include foot, so the image ships `TERMINAL=foot` in `/etc/environment` - the
  file `pam_env` reads, in greetd's session stack and in sudo's - because a window started by the
  compositor inherits the PAM environment and not a login shell's `profile.d`. Without it the menu
  item answers "no terminal detected ($TERMINAL or known candidates)" and starts nothing.
  **The report must never run it to ask its version**: `favnyr --version` parses the flag as a
  startup location ("WARN favnyr: invalid startup location location=--version") and then opens its
  Slint event loop, so `check-system.sh` stopped on that line and every check after it - the whole
  framework section included - never ran. `--help` opens the window too. Presence and the exec bit
  are what the report asks.
- **A double click in favnyr resolves through `/etc/xdg/mimeapps.list` ->
  `org.lazyos.view.desktop` -> `~/.config/bin/lazyos-view`**, which applies exactly the rules
  `rofi-files` applies (text in `bat`; a document - PDF, DjVu, a comic - in `papers`; a picture
  in `imv` with `chafa`'s sixels as the fallback when imv gets no EGL context; a video in `mpv`;
  and an HTML page, an EPUB or a `.deb` in `firefox-esr`), so a file opens the same way from the
  list and from the window - with one deliberate exception, an HTML file, which the list reads as
  text in `bat` and a double click takes to the browser, because that is what a page is.
  `inode/directory` stays `pcmanfm.desktop`, which is now a **name** and not a program: the
  system entry is shadowed by `~/.local/share/applications/pcmanfm.desktop` whose `Exec` is
  `favnyr`, so a GIO program that asks for the directory handler starts the window, and the
  launcher row it draws is called *Files (window)*. There is no `xdg-desktop-portal` in the image,
  so this pair *is* the open-with mechanism for every GIO program. **GIO silently drops a
  `.desktop` whose `Exec`
  program does not exist**: with `lazyos-view` not executable (or absent) `gio mime image/png`
  answers "no default application" instead of an error, which looks like a broken association -
  so the `check-system.sh` line is both the association test and the mode test
  (`LC_ALL=C gio mime image/png` must say `Default application: org.lazyos.view.desktop`, and
  `gio mime inode/directory` must say `pcmanfm.desktop`). That CLI is **`libglib2.0-bin`**
  (0.9 MB, no daemon) and it is in `uis/sway/packages.list` for exactly this: `gvfs` links libgio
  but ships no `/usr/bin/gio`, and without it the check can only read
  `mimeapps.list` - it would pass while GIO refused the entry. No `mimeinfo.cache` regeneration is
  needed for that to resolve (verified), and a user `~/.config/mimeapps.list` overrides the
  system file, so nothing is locked in.
- **The three viewers are `papers`, `imv` and `mpv`, and each one is a package with its own
  reason**: a document (PDF, DjVu, a comic) goes to **`papers`** - a reader with search, zoom,
  printing and annotations, where a PDF used to be a 280 MB browser tab, and poppler stays only
  as the thumbnailer; a picture goes to **`imv`**, which draws it on the GPU and gives zoom, pan,
  rotation and a slideshow of the folder, with `chafa`'s sixels kept as the **fallback** for a
  GPU where imv gets no EGL context; a container goes to **`mpv`**. **imv has no on-screen help**,
  so `bin/lazyos-image-hint` says its keys in one notification the first time a picture opens in a
  session (`$XDG_RUNTIME_DIR/lazyos-imv-hint`, written before the toast is queued) and never again;
  it is called only from the image+imv branches of `lazyos-view` and `rofi-files`, each followed
  by `|| :` so a missing hint never fails the open. **`papers` does not open
  EPUB** - its `MimeType=` has no `application/epub+zip`, whatever `docs/omarchy-gap.md` once
  said - so an EPUB (and a `.deb`) still goes to the browser, and `foliate` (12.4 MB, in forky)
  is the package that would change that. Two packaging facts cost a round each: **Debian's `imv`
  installs no `/usr/bin/imv`** (the package ships `imv-wayland`/`imv-x11` and a
  `/usr/libexec/imv/imv` dispatcher), so `rootfs-setup.sh` adds the short name the way it does
  for `bat` and `fd` - without it every `command -v imv` fails and a photo silently lands in
  chafa; and **`mpv --no-config` empties the script directory too**, so `mpv-mpris` (which is
  what makes `playerctl`, the Media menu and the bar see the film at all) has to be passed as
  `--script=/etc/mpv/scripts/mpris.so`, guarded by `-r` so a tree without the plugin still
  plays. `rofi-files` and `lazyos-view` carry the same table on purpose (the menu does not start a
  process to decide what to start), so a change to one belongs in the other; `check-system.sh`
  FAILs on a missing `imv`/`papers`/`mpv`, on the missing `/usr/local/bin/imv`, on a missing
  `mpris.so`, and warns when `application/pdf` does not resolve to
  `org.gnome.Papers.desktop`.
- **The camera needed no driver, only a window, and that is the whole of this feature.**
  The Hayato's camera is a **USB UVC device on the internal hub** (`lsusb`: `0408:30d5 Quanta
  USB2.0 HD UVC WebCam`, at `1-1.4`), so `uvcvideo` - a module of this kernel
  (`CONFIG_USB_VIDEO_CLASS=m`) - binds it and udev creates `/dev/video0`; the tablet's
  `/dev/video2`/`video3` are the SoC's `mtk_vcodec` encoder and decoder, which is why
  `lazyos-camera` selects the node **by driver** and never by number (a replug reorders the
  minors, the same lesson as `disk_layout_parts()`), and why a report of "there is no driver"
  is answered with `lsusb` + `dmesg | grep -i uvc` + one `ffmpeg -f v4l2` frame rather than a
  package. The device tree carries **no MIPI sensor and no `seninf`**, so the rear camera a
  detachable is expected to have is *not* a missing module: it has no sensor to drive, and
  WirePlumber's `api.libcamera.enum.manager could not be loaded` line on every boot is about
  that path, not about this one (its v4l2 monitor still publishes the UVC node, `media.role
  Camera`). The window is **guvcview** - 4.7 MB with libguvcview/libportaudio2/libwebcam0/
  libgsl, GTK3 so it inherits `gtk-3.0/gtk.css` and looks like the rest of the session, and the
  one thing a bare preview is not: the camera's own UVC controls (exposure, gain, white
  balance), which is what a dark frame in a dim room actually needs. `~/.config/bin/lazyos-camera`
  is the whole wrapper and it exists for five reasons: the node choice above; guvcview's default
  photo and video paths are **`$HOME`** (`photo_path`, `video_path`, so the first capture would
  land among the dotfiles - the bug grimshot had), so it passes `-i ~/Pictures/Camera/photo.jpg`
  and `-j ~/Videos/camera.mkv` (guvcview appends `-N` and continues the numbering across runs,
  so nothing is overwritten); a window that dies at once sends its stderr to
  `~/.cache/lazyos-camera.log` and raises a card, because "the row does nothing" is what a silent
  failure looks like (a window *killed* with `Alt+Q` is SIGTERM and is not a failure, hence the
  elapsed-time half of the test); **`-g gtk3 -r sdl` is passed on every start**, because
  guvcview *remembers* the gui and render api in `~/.config/guvcview2/<device>` and a headless
  run (`-g none -r none`, which is how one captures a photo from a script - and how this feature
  was tested) stores exactly that, after which every later launch reads `gui=none` back, opens
  **no window at all** and still holds the camera - measured here, and it reads as a dead row with
  a live process behind it; and a second instance would do the same, so when `pgrep -x guvcview`
  answers the wrapper runs `swaymsg '[app_id="guvcview"] focus'` and says the camera is already
  open instead of starting an invisible one (the device opens exclusively - the same fact the
  troubleshooting entry states). It is the only viewer here that is **not** handed a
  file by `lazyos-view`: there is no camera *file* to double click. Rows: the root menu's
  `Camera` (with `UI_I_VIDEO`, because `UI_I_CAMERA` is Capture's), the launcher entry
  `org.lazyos.camera.desktop` (whose `Icon=camera-web` is a **name** in the first table of
  `scripts/make-icons.sh`, rendered from the same `f030` as Capture - rofi resolves a `.desktop`
  `Icon=` by name, so a glyph that exists under another name is a hole on that row).
  **The camera portal is the road that is deliberately closed**: `org.freedesktop.portal.Camera`
  needs a backend, the only one here is `xdg-desktop-portal-wlr` (Screenshot and ScreenCast), and
  the portal therefore logs *The camera portal requires an access impl* once per session - which
  is a limitation `04-software.md` states and `check-system.sh` prints as an `info` line, because
  the alternative is a resident portal daemon for one boolean, the answer the GTK4 section already
  gave. Native programs do not care: **Firefox reads V4L2 directly** (verified: a headless
  `getUserMedia` page enumerated a `videoinput` and opened 640x480 at 30 fps with the portal
  missing), so a video call works while a Flatpak app sees no camera. Witnesses: `check-system.sh`
  FAILs on a missing `guvcview`, `lazyos-camera`, `org.lazyos.camera.desktop`, `camera-web.png`
  and Camera row in `bar-menu`, FAILs when `uvcvideo` is loaded and no node is bound to it, and
  SKIPs when there is no UVC device at all; the manual is `04-software.md` § *The camera* and the
  symptom list is `06-troubleshooting.md` § *The camera shows nothing, or an app cannot find it*.
- **An external monitor is the device tree plus one register write per plug, and
  the second half is a udev one-shot.** The port shipped no external-display
  pipeline at all - `dpi@14016000` disabled, no second ANX7625 - so the kernel
  created one connector and `card1-DP-1` did not exist;
  `kernels/mediatek-mt81/patches/mt8192-asurada-external-display.patch` is the
  three nodes ChromeOS's asurada DTSI carries (the DPI output with its 16 pins
  at 6 mA, the bridge on `i2c@11d00000` with the `pp*_dpbrdg` rails, GPIO 8
  power / 9 reset / 7 IRQ and a DPI input endpoint `bus-type = <7>`, and the
  pinmux groups), applied to the shared asurada dtsi because that is where
  ChromeOS puts it. The bridge then has **one DP transmitter and two downstream
  Type-C ports**, and `TCPC_SWITCH_0/1` (7-bit `0x2c` on `i2c-7`) pick which one
  it reaches: `0x21` is the left port, `0x12` the right, and the chip powers up
  on the left. ChromeOS writes those from a typec-switch mux on every alt-mode
  notification (`anx7625_set_crosspoint_switch`); mainline registers a typec
  port and no mux, and its cros-ec-typec has no DT port endpoints to bind one
  to, so nothing ever writes them - measured: a monitor on the right port
  reports HPD, answers EDID over AUX and gets 1920x1080@60 while the panel shows
  its no-signal pattern, and the same monitor on the left port works. The fix is
  `/usr/local/sbin/lazyos-dp-mux`, run by
  `/etc/udev/rules.d/90-lazyos-dp-mux.rules` when a port's DisplayPort alt mode
  appears: it finds the bridge's bus (the anx7625 whose `port@0` endpoint has
  `bus-type`; the panel's has an `aux-bus`) and writes the two registers with
  `i2cset -f` (i2c-tools; the driver has claimed the TCPC address) after the
  page sync ChromeOS's `i2c_access_workaround()` does. **The timing is the
  feature**: the connector's hotplug arrives before the alt-mode device (46 ms,
  measured), and the chip trains the link when it is enabled, so a write
  afterwards changes nothing and only a fresh enable retrains it - the rule has
  to react to the alt-mode event, and it does so before sway commits the mode
  (verified by presetting the wrong crosspoint and watching a replug bring the
  picture up on its own). Nothing stays: one process while the plug is handled,
  and no daemon. **A resume is the half udev cannot see**: the bridge powers down
  with the display pipeline and re-initialises on the way back, so the
  crosspoint returns to its power-on default - measured `0x12` before the sleep
  and `0x00` then `0x21` after it - while the EC re-detects the partner without
  re-adding the alt-mode device the rule matches (`typec port1.0: EC does not
  support AP driven mode entry`, a -95 logged on every resume). The connector
  still reports connected, EDID is intact and sway keeps the mode, so the
  session paints into a link that now points at the empty port until the cable
  is replugged. `~/.config/bin/lazyos-dp-resume` runs from the idle chain's
  `after-resume` and does the two things that fix it: the crosspoint for the
  port that has DisplayPort alt mode, then **one modeset** - a 1280x720 round
  trip, and not a disable/enable, because the modeset is what retrains the link
  (a crosspoint write after training changes nothing, measured) while disabling
  the output moved that workspace to the internal panel (workspace 2 on DP-1
  came back on eDP-1). **Where it sits is Settings → Display arrangement**
  (`~/.config/bin/rofi-display`): right/left/above/below/mirrored, resolution and
  scale, and `--status` is the single copy of the answer - the Settings row's
  detail reads it, so the row and the menu cannot disagree. A runtime position
  dies with the session, so every arrangement is also written to
  `~/.config/sway/config.d/40-display.conf`, which `~/.config/sway/config`
  includes. The panel is never moved - the monitor is placed relative to it, so
  "on the left" is a negative x, and `swaymsg --` is what makes that legal:
  without it swaymsg's own option parser reads `-1920` as a flag (`invalid option
  -- '1'`). Only the monitor is written, because it is the output that can be
  absent; **Only the monitor**/**Only the internal panel** are deliberately not
  saved, since a config that disables a screen boots a card with nothing on it.
  **wdisplays** is the manual row (188 kB, every dependency already in the image)
  and opens on workspace 11 - the session's keys stop at 10 - because its two
  rectangles want the width; a second copy is focused instead of started.
  **The way back has two halves, because a black screen is two cases.** "Only
  the monitor" turns the internal panel off, so a cable that comes out leaves no
  picture. (1) While a screen can still draw - the monitor is present but
  disabled, or the panel is off with the monitor on - the menu's no-monitor
  branch offers **Bring the internal panel back**, and `--status` says `internal
  panel off` instead of `no external monitor`. (2) With *every* output off there
  is nowhere to draw a menu at all, because sway permits turning the last output
  off (measured: eDP-1 `active=false` with no mode, and `output eDP-1 enable`
  restores 1366x912) - so `$mod+Ctrl+Shift+D` runs
  `~/.config/bin/rofi-display --panel-on`, a sway command with no surface
  (`24-keys.conf`), which is the one key in this session expected to be used
  blind. Both are tests, not paragraphs: `check-system.sh` runs the menu against
  a stub swaymsg (only eDP-1, inactive) and FAILs when the row is gone, and
  FAILs when the binding or the `--panel-on` mode disappears. The failure they
  prevent is invisible - the machine is alive, the screen is black, and the only
  exit left is the power key. A short power-key press and a clean poweroff is
  what that boot looks like in the journal: no oops, no segfault, no core, and
  nothing else to read, which is exactly why it reads as a crash.
  **The night light cannot reach it**: `modetest -M
  mediatek -p` has `GAMMA_LUT` on the 1366x912 CRTC and nothing on the 1920x1080
  one (MT8192's second path OVL1→RDMA1→DPI has no gamma block bound; the SoC's
  one block is in the first path), so wlsunset logs `gamma control of output ...
  failed` and the monitor keeps its own colour temperature. Witnesses:
  `check-system.sh` FAILs on a missing DP connector (the patch), the helper,
  the udev rule, i2c-tools, the resume hook and its modeset step, the
  arrangement menu, its Settings row, the `config.d` include, `wdisplays`,
  the menu's way back with the panel off and no monitor, and the blind
  `--panel-on` key and mode,
  and on a crosspoint that does not name the port whose alt mode is active; the
  manual is `04-software.md` § *An external monitor* and the symptom list is
  `06-troubleshooting.md` § *The external monitor shows no signal*.
- **Removable media is `udisks2`, and mounting it is a row in Places.** An unprivileged session
  cannot `mount(8)` anything - that is a root operation - so a USB stick used to be unusable:
  no mount point, no `udisksctl`, and the file manager's device list had nothing to talk to.
  `uis/sway/packages.list` carries **`udisks2`** (D-Bus activated on the system bus, so nothing
  runs until something asks and nothing stays resident) and `rofi-places` grew two row kinds:
  `disk:` for every mounted real filesystem (`findmnt`, minus `/` and `/boot`), and `mount:` for
  every partition `lsblk` reports with a filesystem and no mount point, which
  `udisksctl mount -b` mounts and then opens in `rofi-files`. **`Alt+1` on a `disk:` row
  unmounts it**, because a stick mounted from the menu stays mounted otherwise - and pulling one
  that is still mounted is how a filesystem gets corrupted. Three details: `udisksctl mount`
  prints `Mounted /dev/sda1 at /run/media/<user>/<LABEL>.` **with a full stop**, so the path
  handed to the browser is `${out##* at }` with `${mp%.}` after it, or the menu opens a
  directory that does not exist; `lsblk -r` escapes a space in a label as `\x20`, so the row
  unescapes that one sequence (a label is what a person recognises); and the `mount:` rows skip
  `swap` and `crypto_LUKS` on purpose - udisks can unlock LUKS, but it needs a passphrase prompt
  this menu does not have, and a swap partition has nothing to show. **`udiskie` is deliberately
  not installed**: it is a resident Python daemon (`python3-gi` + `gir1.2-gtk-3.0` +
  `python3-docopt`/`python3-yaml`, none of them in this image) whose remaining job would be a
  tray icon this bar has no host for; on-demand `udisksctl` is the same result with no daemon.
  No userspace filesystem helper is needed either: the kernel has `VFAT_FS=y` and
  `EXFAT_FS=m`/`NTFS3_FS=m`, so a stick from any other machine mounts, and `dosfstools` is
  already in `packages/base.list` for the `fsck.vfat` udisks runs before mounting one.
- **A plugged USB drive mounts itself, and the whole feature is one udev event.** The user asked
  for it in those words ("usb drives should be auto mount when plugged in"), and it is built the
  way the no-daemon rule allows: `90-lazyos-automount.rules` matches `ACTION=="add"`,
  `SUBSYSTEMS=="usb"` and `ID_FS_USAGE=="filesystem"` and wants
  `lazyos-automount@<partition>.service`, a `Type=oneshot` unit that runs
  `/usr/local/sbin/lazyos-automount` once and exits. One process per plug, nothing between two
  plugs, and no `udiskie` (that bullet above is why). Five decisions are the feature and each one
  is a way it silently fails:
  **It mounts as the desktop user**, because udisks2 writes the *caller's* uid/gid into the mount
  options of vfat/exFAT: a root mount is `/run/media/root/<label>` with `uid=0`, a stick nobody
  can write to. `setpriv --reuid --regid --init-groups` is the whole of it (no PAM session, no
  logind session, no second login), and it is verified on hardware by reading
  `findmnt -o OPTIONS` back for `uid=1000`.
  **Polkit has to be told, and by group.** A caller with no session is `auth_admin` and never
  `allow_active` - udisksd cannot ask "is this user active?" about a process that has no session -
  so `/etc/polkit-1/rules.d/49-lazyos-automount.rules` grants
  `org.freedesktop.udisks2.filesystem-mount` **and** `-other-seat` (udisksd picks between them per
  *user*, through `sd_uid_is_on_seat`, so a session on another VT takes the second) to
  `subject.isInGroup('plugdev')`, and `rootfs-setup.sh` adds `plugdev` to `useradd -G`. `-system`
  is deliberately absent: that is the internal disk's action. Measured: `subject.local` is false
  in a session-less process, which is why the group is what the rule matches.
  **Only USB.** The match is the transport, so the eMMC and the internal microSD slot are never
  candidates - they are the two disks this machine boots and rescues from, and `mmcblk1` is where
  the rescue card lives. An SD card stays a **Mount** row in Places, which is also the honest
  amount of ceremony for a card about to be repartitioned.
  **The helper refuses more than the rule matches**, because it is also a command a person can
  run: non-block, a disk that is not removable (`removable=1` **or** `device/removable=removable`
  **or** `TRAN=usb`, the same triple as `ui_disk_system`, since `/sys/block/mmcblk1/removable` is
  `0` on this machine), no filesystem or one of `swap`/`crypto_LUKS`/`linux_raid_member`/
  `LVM2_member`, already mounted, and **any device the running system came from** (`root=`,
  `resume=`, `kern_guid=` resolved through `findfs`, every `/etc/fstab` source, every
  `/proc/mounts` source, compared with `[ "$src" -ef "$DEV" ]`). That last one is not
  hypothetical: on a machine booted from a USB stick, udev's `add` for the root partition arrives
  *before* systemd mounts it, so `findmnt` still says "not mounted" and mounting `/` a second time
  under `/run/media` is one plug away.
  **A switch, because a feature with no off is a feature somebody edits udev to escape**:
  `~/.config/lazyos/automount.conf`, one line `enabled=no`, missing means on, read by the helper
  and toggled by Settings' **Auto-mount USB drives** row (`rofi-quick`, `UI_I_DISK`). Both parse
  the file the same way on purpose, or the row would lie.
  Two smaller things that cost a round each: **udisksd learns about a device from its own udev
  monitor**, a different queue from the one that runs the unit, so the helper retries for up to
  five seconds instead of believing the first "No such object path"; and **`udisksctl mount`
  prints `Mounted /dev/sda1 at /run/media/<user>/<label>.` with a full stop**, so the path is
  `${out##* at }` and then `${mp%.}`. The notification is `notify-send` on the user bus (with
  `XDG_RUNTIME_DIR` and `DBUS_SESSION_BUS_ADDRESS` handed to it by the same `setpriv` wrapper,
  since a system unit has neither), it names the label and the path, and it adds `(read-only)`
  when the mount options carry an exact `ro` field - a dirty vfat mounts `ro` rather than
  refusing, and that is otherwise indistinguishable from a broken stick.
  One behaviour follows from the account rule and is worth knowing: **a stick already inserted
  when the machine booted waits in Places**, because udev's coldplug event arrives before anyone
  is logged in and the helper then has no account to mount for - unplug and replug, or pick the row.
  Witnesses: `check-system.sh` FAILs on any of the four files, the helper's exec bit, a udev rule
  that lost `SUBSYSTEMS=="usb"` (it would mount the internal disks) or its `SYSTEMD_WANTS`, a
  helper that lost `setpriv`, a polkit rule that lost `filesystem-mount` or `isInGroup('plugdev')`,
  an account missing from `plugdev` and the Settings row, and WARNs when `polkitd` is not running.
  The manual is `04-software.md` § *USB sticks and SD cards*, `02-rofi.md`'s Places paragraph, and
  the symptom list is `06-troubleshooting.md` § *A plugged USB drive does not mount*. The
  acceptance row is `docs/test-checklist.md` § *USB sticks and SD cards*.
- **Format a disk is the one destructive menu, so the list is the safety, not the question.**
  `rofi-format` is reached from the last row of Places and offers only disks where **nothing is
  mounted**, **no partition carries the ChromeOS kernel type**
  (`fe3a2a5d-4f32-41a7-b725-accc3285a309`, p1 of the layout in `scripts/lib-cgpt.sh`), and
  **`ui_disk_system` in `lib-rofi.sh` allows it**. That function is the one copy of the two rules
  that keep the internal disk out, shared with `rofi-swap`, and both menus ask it again after the
  pick because that step is the one that repartitions. **A fixed disk is not a candidate**: on this
  Chromebook `/sys/block/mmcblk0/removable` and `/sys/block/mmcblk1/removable` are both `0`, so the
  boot card looks exactly like the eMMC there; the device-level
  `/sys/block/<disk>/device/removable` is `fixed` for the eMMC and `removable` for the SD, and
  `TRAN=usb` counts too because a USB enclosure may report `0`. **The disk the system depends on is
  not a candidate either**: root, `/boot`, the kernel partition named by `kern_guid=` and every
  active swap resolve to their disk with `lsblk -s`, and that disk is refused - the kernel-partition
  rule alone would reopen the hole the moment the eMMC's table is wiped or `/boot` is unmounted,
  which is the case the old list missed. The list also skips zram, loop, ram, optical and pseudo
  devices, **`mtd*`** (the SPI flash this port must never write) and **`mmcblk*boot*`** (the eMMC's
  4 MB hardware boot partitions) - see the next bullet for the hardware that made those two
  necessary. What is left has nothing this system needs, and the empty-list card names the fourth
  reason the list enforces - `carries a bootable kernel partition (the card that installed this
  system)` - because a removable, unmounted card that installed the eMMC is a candidate on every
  other count, so the three reasons the message used to give did not describe it and the refusal
  read as a broken menu; Format a disk's card says the same. The
  confirmation still names the device, its model and its size, with **Cancel as the first row** so a
  stray Enter does nothing; the filesystem is **exFAT** (label `LAZYOS`, via `mkfs.exfat -F -L`,
  from `exfatprogs`) or ext4, and the whole disk becomes one partition (`parted -s mktable gpt` +
  `mkpart primary exfat`, then `mkfs`). exFAT rather than FAT32 because FAT32 refuses a file over
  4 GB and Windows, macOS and Linux all read exFAT; the eMMC can only be written by
  `install-emmc.sh`, so this menu is for sticks and cards. **It is not stoppable on purpose**, like
  the apt install: a killed `mkfs` leaves a disk that looks formatted and is not, which is worse
  than a wait. The step chain is linear (`rc`/`step`) rather than nested `if`s because `set -e`
  exits from inside an `if` *body* on a failed command - a failing `mkfs` would leave the `busy`
  card spinning with no reason in it. `parted` and `mkfs.ext4` are in `packages/base.list`,
  `mkfs.exfat` comes from `exfatprogs` in `uis/sway/packages.list`, and `check-system.sh` FAILs
  when any of them, the script, or the guard in `lib-rofi.sh` is missing.
- **zram is sized to all of RAM, with page-cluster=0 and IRQ balancing on MT8192.** `rootfs-setup.sh`
  writes `zram-size = min(ram, 4096)` (3.7 GiB here), keeping heavy workloads entirely in compressed
  RAM so pages never spill to the slow microSD card at priority 10 (~1-2 MB/s random writes). The
  earlier latency regression with a full-RAM cap was tracked to three kernel settings rather than the
  capacity itself: (1) `vm.page-cluster=3` (the kernel default) decompressed 8 pages (32 KB) on every
  swap-in, completely blowing out the 32 KB L1 data cache on the in-order Cortex-A55 LITTLE cores;
  (2) `vm.vfs_cache_pressure=50` hoarded directory/inode caches and forced application memory into swap;
  and (3) `vm.watermark_boost_factor=15000` triggered premature kswapd churn during allocation bursts.
  With `vm.page-cluster=0` (single-page decompress, ~1 us on Cortex-A76), `vm.vfs_cache_pressure=100`,
  `vm.watermark_boost_factor=0`, and `lz4`, swapping into zram is fast and zero-latency. Furthermore,
  storage interrupts (MMC) are routed away from CPU0 to CPU2/3 (`lazyos-irq-balance`), so heavy disk
  I/O never starves the ChromeOS EC (keyboard/trackpad) on CPU0. **The resident limit is the trap**:
  at the limit `zram_can_store_page()` refuses the store and `zram_bio_write` returns `BLK_STS_IOERR`,
  while the swap allocator picks a device by *slot exhaustion* and not by write success, so it keeps
  choosing zram (whose nominal slots are free) and keeps failing - the overflow becomes an I/O error on
  swapped-out data instead of a page handed to the card at priority 10, and the auxiliary swap above
  would become hibernate-only. Only the nominal cap falls through cleanly. Witnesses: `check-system.sh`
  reads the file, the live `disksize`, `page-cluster`, `watermark_boost_factor`, `vfs_cache_pressure`,
  and MMC IRQ routing; the manual paragraph is `05-system.md` § *The kernel*, and the numbers are
  `docs/optimization.md` § *zram sizing*.
- **Auxiliary swap turns a card into extra memory, and it shares Format's list on purpose.**
  `rofi-swap` (Places, the row before Format a disk, and the root menu's *system* section) picks a
  disk, then how much of it - the whole card first, then 2/4/8 GiB only when they fit with 64 MiB of
  margin - then confirms with **Cancel first**. The candidates are the same `ui_disks` function as
  Format (`PREFIX` per caller, `UI_KERN_TYPE` beside it) plus `ui_disk_system`, and the guard is
  asked again after the pick, so the two menus cannot disagree about which disk is a system disk.
  **Sharing it fixed two holes the card's own `lsblk` shows**: `-d`
  lists the SPI flash as `mtdblock0` (8 MB) and the eMMC's hardware boot partitions as
  `mmcblk0boot0`/`mmcblk0boot1` (4 MB each), none mounted and none carrying a kernel partition, so
  Format used to offer all three - and writing `mtdblock0` is the one thing this port must never do.
  `ui_disks` excludes `mtd*` and `mmcblk*boot*`/`mmcblk*rpmb` by name (plus `dm-*`, `md*`, `nbd*`,
  `rbd*`), so both menus are fixed at once and no device name is hardcoded.
  Setup is `parted -s mktable gpt` + `mkpart primary linux-swap 1MiB <end>` (the partition type is
  Linux swap, so Places skips it instead of offering it as a Mount), then
  `mkswap -f -L lazyos_swap` - `-f` because the new partition starts where an old filesystem's
  signature may still be - and one marked block in `/etc/fstab`:
  `LABEL=lazyos_swap none swap sw,nofail,x-systemd.device-timeout=15s,pri=10 0 0`. `nofail` keeps a
  boot without the card from failing, the 15 s timeout keeps it from waiting the default 90 s, and
  `pri=10` is **below zram's 100** (`rootfs-setup.sh`) so compressed RAM is used first and the card
  is the spillover it is good for; equal priorities would round-robin between RAM and a card. The
  label is the one `resume=LABEL=lazyos_swap` already names, so hibernate and this menu talk about
  one card; the menu does not add `resume=`, which is a kernel command line change and a rebuilt
  kpart.
  The block is **replaced, not appended** (markers `# LazyOS auxiliary swap` / `# end LazyOS
  auxiliary swap`), and removal drops any non-comment line whose source is `LABEL=lazyos_swap`, so
  the manual recipe in `sway-hibernate` is undoable from the menu too. The write is `install` to
  `/etc/.fstab.lazyos-swap` then `mv` over `/etc/fstab`: a `cat >` in place would truncate the file
  first. Activation is `swapon -p 10 <part>` (not `swapon -a`, whose exit status another fstab entry
  can spoil) followed by a read of `swapon --show=NAME --noheadings` **joined with `tr '\n' ' '`**:
  the list is one line per device and **zram is always first**, so `case " $list " in *" $part "*)`
  looked for a space where the card's own line began with a newline, and the read-back called every
  successful setup a failure while the swap was on (`check-system.sh` had the same line and WARNed
  that a configured card was inactive). **A failed activation removes the fstab block again**,
  because a card that is not doing anything should not be configured at boot either, and turns the
  partition back off when `swapon` itself answered 0 but the read-back could not see it: a swap that
  is on with no fstab entry is a state nothing explains. Both modes refuse an unreadable
  `/etc/fstab` up front, since both rewrite it. **Undo is the state surface**: when a block or an
  active labelled swap exists, the menu opens on the status (device, size, priority) with *Turn it
  off* - `swapoff` first (if that fails the fstab block stays, which is the state that was there),
  then the block and `systemctl daemon-reload`. The partition is left labelled and inactive, and
  Format a disk is how it is erased. **One label has to mean one card** (fstab and `resume=` both
  resolve `LABEL=` to whichever device carries it), so setup refuses a disk while a different
  device already carries the label. Not stoppable, like Format: a killed `parted`/`mkswap` leaves
  half a card. `parted`, `mkswap`/`swapon` (util-linux) and `blkid` are already in the image, so the
  menu adds no package - but they live in `/sbin`, and the session's `PATH` is the one greetd hands
  down (the PAM environment; Debian keeps `/usr/sbin` and `/sbin` for root), so the first card
  answered `Missing: parted mkswap swapon swapoff blkid` with all five installed, and Format a disk
  had the same hole. **The fix is one line where every menu already looks**: `lib-rofi.sh` appends
  `/usr/sbin:/sbin` to `PATH` once (appended, so a system package still wins over a user copy), and
  `rofi-install`'s copy of that block is gone with it - `check-system.sh` had grown the same block
  for its own report, and it now also asks the shared file in a session-shaped `PATH` and FAILs
  while the line is missing, because the failure is silent in every other witness: the tools are
  present, the row draws, and only the answer is "Missing". `check-system.sh` also FAILs on a
  missing tool or script and when Places loses the row; when the fstab block exists it WARNs if
  `nofail`, the device timeout or a priority below 100 went away, SKIPs when the card is not
  inserted, and WARNs when it is inserted but not active.
- **Install LazyOS is a front end, not a second installer, and the delegation is what
  `check-system.sh` asks of the file.** The root menu's red row (and
  `org.lazyos.install.desktop`) opens `~/.config/bin/rofi-install`, one rofi process that only
  *reads*: the target is fixed (`DEVICE_BLOCK`, `/dev/mmcblk0`, no picker - a list of disks is a
  list of ways to erase the wrong one), and the menu reports the machine before offering anything -
  the target and the room the copy needs (`df` on `/` + `/boot`, minus `$WORK_DIR`, because on a
  machine that built this port the build tree is bigger than the system it built), the charger and
  the battery (below half says so, below 20% on battery refuses), `/lib/modules/<release>` +
  `vmlinuz-` + `initrd.img-` (the kpart is built from them *after* the copy, so missing them was a
  failure with the disk already formatted), the installer's tools and a `sudo` that would not
  prompt - the latter two being cases where the row disappears instead of a doomed write starting.
  Those rows carry `none` as their key, so Enter re-checks instead of closing (the same trick
  `bar-menu`'s headings use), which is what makes "Cannot install yet" an honest row and not a
  dead one. **Cancel is the first row and it is selected**, and the last gate is the target's own
  name typed in full (`mmcblk0`) through `bar-pick -free`: Enter on a list row is exactly the
  gesture that must not erase a disk, so a wrong answer is a critical toast and nothing else.
  Then `setsid ~/.config/bin/lazyos-install-run TARGET REPO` - the runner is bash, detached, and
  **the only writer**: it calls `scripts/install-emmc.sh` once with `CONFIRM=yes` after `env`
  (`env`, not a bare `VAR=value` list: with no `systemd-inhibit` prefix the empty `"$@"` made bash
  read `DEVICE=` as the command name - "Failed to execute 'DEVICE='" - which is a second, silent
  way for an install to die before it starts), and `check-system.sh` FAILs if `parted`, `mkfs`,
  `cgpt`, `rsync` or `dd` ever appears in that file, which is what makes "byte for byte the
  documented install" a test instead of a promise.
  **Its preparation is exactly the two refusals `disk_release()` has**, done before the call so
  that refusal is unreachable instead of surprising: `systemctl stop udisks2` **and
  `mask --runtime`** - stopping is not enough, because udisks2 is D-Bus-activated and the new
  table alone is what makes it mount p2/p3 mid-write (`write-sd.sh`'s documented failure) - and
  `umount` of every mounted partition of the target; a failure in either aborts before a byte is
  written, and a trap unmasks and starts udisks2 again on **every** exit path, which is the manual
  procedure the port already documents. `systemd-inhibit --what=sleep --mode=block` keeps an idle
  machine from suspending into a half-written disk (the install still runs without the binary).
  **Not stoppable on purpose**, like Format a disk and the apt install: no `BUSY_STOP` is
  registered and the card says a killed copy leaves the disk half written; progress is the
  installer's own `==> ` lines (ANSI stripped) turning into `busy update` cards under a waybar
  spinner, and the log is `$XDG_STATE_HOME/lazyos/install-<timestamp>.log`, opened by
  `lazyos-install-log` in the floated foot (`less -R +F` while it runs, `+G` when it is finished).
  **Success is read back, not believed**: the three `PARTLABEL`s are asked one at a time (the
  `partx` round reorders minors) and `futility show` on p1 must say `Body verification
  succeeded` - an exit status 0 with a disk that has no bootable kernel partition is the one
  failure a copy this size must not hide - while a failure carries the installer's **last**
  `ERROR:` line, ANSI stripped and its prefix dropped because `fail()` puts one back ("ERROR:
  ERROR: …" is what a card looks like when it is not). The menu reads `$state/status` back the
  next time it opens (running/failed/did-not-finish), and the icon language is
  `system-software-install` (Font Awesome f132, table 1 of `scripts/make-icons.sh`) with
  `UI_I_SHIELD`/`UI_I_CHECK`. Nothing is resident: one rofi process while the menu is open, then
  the detached runner. On a loop device with the real pmOS layout and a signed kpart copied from
  the eMMC's p1 the whole path was exercised - success, failure, the udisks2 window
  (`inactive`/`masked-runtime` while writing, `active`/`enabled` after, no leftover mask) - which
  is how the `env` bug and the doubled `ERROR:` prefix were found.
- **The GTK3 half of the palette is two files** in `uis/sway/files/home/.config/gtk-3.0/`:
  `settings.ini` (`Adwaita-dark`, `gtk-application-prefer-dark-theme=1`, `LazyOS`,
  `JetBrains Mono 10`, animations off) and `gtk.css`, which repaints the surfaces with explicit
  selectors of the same specificity Adwaita uses and is parsed after it. The fills are **opaque**
  base03 - a translucent window shows the wallpaper through its own text. The GTK3 *window* the
  image ships is **guvcview** (the camera), and waybar is GTK3 as well but paints the bar from its
  own stylesheet; foot is pixman and rofi draws its own surfaces. The file manager used to be the
  other GTK3 window and is now `favnyr`, one Slint binary that links no GTK - but it still **reads
  this file**, because it takes its icon theme from `gtk-icon-theme-name=`, so the pair is a
  stylesheet plus an icon-theme pointer rather than a theme package. Adwaita 51 ships no
  full-colour `folder`,
  `user-home` or `drive-harddisk` at all, which is why `scripts/make-icons.sh` grew a **fourth
  table** (50 names: the places, the actions and the mimetypes the sidebar and the icon view ask
  for by name; the theme is 225 files in each of `48x48/apps` and `scalable/apps` - 224 from
  `make-icons.sh`, plus the mark from `make-brand.sh`), and `check-system.sh`
  asks the theme for the names favnyr draws instead of trusting the picture.
  **GTK4 is a different settings store and a different pair of files**, which is why `papers` - the
  one GTK4/libadwaita window in the image - opened white: libadwaita asks the settings portal for
  the desktop colour scheme, this image runs no Settings portal (`busctl --user` lists only the wlr
  backend, Screenshot/ScreenCast), and it fell back to its light default.
  `gtk-4.0/settings.ini` states `gtk-application-prefer-dark-theme=1`, the LazyOS icons and
  JetBrains Mono 10; `gtk-4.0/gtk.css` redefines the **libadwaita named colours**
  (`window_bg_color`, `view_bg_color`, `headerbar_bg_color`, `card_bg_color`, the cyan
  `accent_*`, `destructive`/`success`/`warning`/`error`, `sidebar_*`, `thumbnail_*`) with the same
  solarized values - named colours rather than the GTK3 file's selectors, because libadwaita paints
  from them while an equal-specificity selector only reaches the widgets it happens to match.
  `xdg-desktop-portal-gtk` (the other answer) is deliberately not installed: a resident daemon for
  one boolean. `check-system.sh` WARNs when either file is missing or the dark preference line is
  gone.
- **Two caches are built into the image, not paid on the first session.** After the UI tree is
  overlaid, `rootfs-setup.sh` runs `fc-cache -f -s` and
  `gtk-update-icon-cache -q -t -f /usr/share/icons/LazyOS`, so `/var/cache/fontconfig` and
  `/usr/share/icons/LazyOS/icon-theme.cache` travel in the image. Without them the first terminal
  walks every font directory and the first `rofi -show drun` scans 220 files - a one-time cost that
  reads as "the launcher is slow", and the icon half comes back silently after a rebuilt theme
  unless the cache is refreshed. Both tools are already in the chroot (`fontconfig`;
  `gtk-update-icon-cache`, which `adwaita-icon-theme` pulls in through `libgtk-3-0`), and a missing
  tool or a failed run is a warning, not a failed build: the image is still correct, just slower on
  first use. `check-system.sh` WARNs when either cache is absent; the manual is `05-system.md`
  § *The first launch*.
- **The window is `Alt+Ctrl+Shift+F`, the Shift variant of the Files key**: `Alt+Ctrl+F` opens the
  list and its Shift chord the window, so the file family sits together in `24-keys.conf` where
  `$mod+Ctrl+letter` opens a menu and `$mod+Ctrl+Shift+letter` the variant it stands for (the same
  shape as `Alt+Ctrl+D`/`Alt+Ctrl+Shift+D` and `Alt+Ctrl+A`/`Alt+Ctrl+Shift+A`). The Search key
  keeps the three lists - launcher, `Alt+Search`, `Search+Shift` - so a key that says "find" never
  opens a window, and `Search+Shift` now carries the terminal search (next bullet).
  Nothing of this is resident: the menu is one rofi process while it is open, and the window is
  the window.
- **`Search+Shift` is the same question in a terminal** (`~/.config/bin/lazyos-find`): `fd`
  streams the whole tree under `$HOME` into **fzf**, the fruit of typing narrows it while the walk is
  still running, the right 55 % is a preview pane, and Enter hands the choice to **`lazyos-view`**
  - the same opener a double click in favnyr uses - so the list, the window and the key never
  disagree about where a file goes. The two searches are deliberately different questions:
  `rofi-files --search` is **bounded** (500 rows, sorted newest, icons and thumbnails, up in a
  menu), which is what a file whose name you half-remember wants; this one is **uncapped** with a
  preview, which is what "I will know it when I see it" wants. Dotfiles are **included** here and
  hidden in the menu, because the whole tree is the pool and `~/.config` is a place people look
  for things; the four excludes are the same (`.cache`, `.git`, `node_modules`,
  `.local/share/containers` - `.cache` alone is 330k of the 350k files in this home) and
  `.gitignore` keeps vendor trees out.
  **fzf is the picker because it is already here** (the shell's Ctrl-R/Ctrl-T/Alt-C and `ff`,
  `packages/tools.list`) **and it filters as the producer streams instead of after it**;
  `--read0`/`--print0` make a name with a space, a tab or a newline one row and one argument.
  Rejected, each at its own line in `docs/optimization.md` §9: `skim` (a second finder with the
  same behaviour and no measurement to justify it), `television`/`tv` (not packaged in forky, so
  it would need a cargo build for a tool that has to be there on the first boot), `fzy` (slower
  and no preview) and fzf's own `--walker` (it does not read `.gitignore`, so `node_modules`
  comes back). Measured on the build host: **2,020 visible files walk in 21 ms**, the 19.4k of
  hidden-plus-excludes in **43 ms**, **first paint in 24 ms**, a keystroke repaints in **5-35 ms**,
  35.6 MB of PSS while open - and the unfiltered tree (348k files) costs 1.6 s of walk and 86.7
  MB, which is why the excludes are not optional.
  Four things here are traps, all of them found by running it. **`fd --print0` prefixes every
  relative path with `./`** and `--strip-cwd-prefix` is the only thing that removes it - without
  the flag every row is shifted two characters nobody types and every path handed to the opener
  is wrong. **The preview draws with `chafa -f symbols`, never sixels**: the pane is part of
  fzf's own terminal, not a window, so a sixel image is painted over by the next redraw, while a
  symbols image survives it (and reuses the same `lazyos-thumbnail` cache, so a picture is
  decoded once, as in the browser). **No glyph column**: foot cannot render a Font Awesome
  codepoint (its fallback font is Symbols Nerd Font Mono), so a row carries no icon and the
  picture is **the preview pane** - which is why this feature satisfies the picture rule without
  a name in `scripts/make-icons.sh`; the notification that a failed search raises does carry
  `system-search`, and `check-system.sh` warns when the theme loses it. **`--no-multi`** because
  the opener takes one file: a Tab that marked three rows would open the first and say nothing
  about the other two. The window is a foot with `--app-id=lazyos-find`, floated and centred by
  `etc/sway/config.d/29-file-search.conf`, and started from a shell it runs in that shell
  instead - the key and the command are the same program. The script starts `foot` itself and
  wraps the search instead of `exec`'ing it, so a missing `fd`/`fzf`/opener prints its reason and
  waits for Enter instead of the window closing at once, which is what "the key does nothing"
  looked like. `check-system.sh` FAILs on a missing
  binding, on a script that lost its executable bit (the window would flash and close), on a
  missing `fzf` (an empty list reads as "no such file", never as a missing package) and on a
  missing `lazyos-view`, and warns on the icon. **It is a release binding**, like the launcher on
  `Search` and the list on `Alt+Search`: sway matches a release binding against the modifiers held
  when the key comes up, so Search released with Shift still down lands here, Search alone falls
  through to the launcher, and none of the three fires on the press - the top row doubles as
  F1-F10 while Search is held (keyd), so a press binding would open a menu on the way to an F-key.
  Releasing Shift first gives the launcher rather than the search, which answers a narrower
  question instead of the wrong one. Nothing is resident: one fzf process while the
  window is open.
- **`Alt+Ctrl+G` searches the text inside the files** (`rofi-grep`, and "Search inside files" in
  the root menu). It is the third question in the family and each one is a different tool on
  purpose: `rofi-files --search` asks *which file was it* (bounded, 500 rows, icons), `lazyos-find`
  asks *I will know it when I see it* (uncapped, fzf, a preview), and this asks *where did I write
  this* - one `rg --line-number --no-heading` run over `$HOME`, one row per match with the matching
  **text** on the row (the text is what tells two matches apart) and `path:line` in the dim
  colour. It is bounded the same way the list is (`--max-count 3` per file, the same four
  excludes, `--max-columns 200`, 400 rows) because a home directory is not a repository: a file
  that says "error" forty times is one row of context and thirty-nine of noise. **Enter opens the
  match, not the file**: `bat --highlight-line N` colours the line but bat's own pager always
  starts at the top, so bat writes the page to a real file and `less -R "+N"` opens *that file* -
  less cannot seek a pipe, and the old `bat | less +N` answered *Cannot seek to line number N*
  and left the match below the screen; `-R` keeps the colours bat wrote, and the file is removed
  when less exits - inside the floated `popup` foot window. A pattern
  ripgrep cannot parse is reported in ripgrep's own words (`regex parse error`) instead of an
  empty list, and the walk is a `busy` card whose `BUSY_STOP` is `pkill -f "[r]g ..."`, so a
  search that is taking too long is one click away from stopping. The binding lives in
  `etc/sway/config.d/24-keys.conf` next to the Files keys and not in `19-launcher.conf`, because
  it is a file question, not the launcher. The rows carry no glyph - a dmenu row without
  `-show-icons` has no icon column, and foot cannot draw a Font Awesome codepoint anyway - so the
  picture this feature has is the text on the row, and the notification that says "nothing
  matches" carries `system-search` from the theme.
- **Capture is a menu over commands that already exist** (`rofi-capture`, `Alt+Ctrl+C`, "Capture"
  in the root menu and the launcher entry `org.lazyos.capture.desktop`):
  `screenshot -n save|copy screen|area|window` for the screenshot, where
  `~/.config/bin/screenshot` is the one place that decides where a picture lands -
  `~/Pictures/Screenshots/<the date>/`, one folder per day, under grimshot's own `date -Ins`
  name and handed to grimshot as its **third positional argument**, the only channel a
  `~/.config/user-dirs.dirs` cannot override (exporting `XDG_SCREENSHOTS_DIR` loses to the file
  grimshot re-sources after reading the environment, which is why the variable-only fix looked
  correct on an image that ships no such file and wrong on a machine that has one). Without it
  grimshot wrote `$HOME` - `XDG_PICTURES_DIR` has no `user-dirs.dirs` to come from - and every
  screenshot landed among the dotfiles (the card's report). The three keys and the capture menu
  all go through the wrapper, `check-system.sh` fails when the wrapper is not executable, and the
  notification prints the path and carries the image. `swappy -f FILE` annotates, which writes the edits back over the same file
  and exits when its window closes, `slurp -d` + `grim -g` for one region that `tesseract FILE
  stdout -l eng` (English is the only `tesseract-ocr-*` the image ships) or `zbarimg -q --raw`
  reads, the result copied with `ui_copy` and the first line in the notification.
  **`wf-recorder` is the one action that outlives the menu**, so it is started and stopped from the
  same list and its pid and file name live in `$XDG_RUNTIME_DIR/lazyos-capture`: a session that dies
  takes the state with it (a stale pid is `kill -0`-tested), the `.mp4` stays. x264 on the CPU
  (`-c libx264 -p preset=veryfast -p crf=28 -r 30`) because the MT8192 has no hardware encoder and
  four cores are the frame budget, **no audio track** because that needs a PipeWire monitor source
  nothing here wires (`-a` would record the microphone), and Stop is SIGINT, which is what
  finalises the container; a recorder that had to be SIGTERMed leaves a file the notification calls
  empty instead of claiming success. The `busy` toast is the persistent one and the bar shows the
  spinner while it runs. `rofi-qr` (root menu, the Wi-Fi panel's last row) draws
  `WIFI:T:WPA;S:<ssid>;P:<password>;;` with `\`, `;`, `,` and `:` escaped as ZXing expects, the
  password read with `sudo -n nmcli -s`, the payload written to a `0600` temp file and drawn by
  `qrencode -t ANSIUTF8` inside the floated `popup` foot window - no image viewer, no file left.
  The active connection is resolved **by device name** (`nmcli -g GENERAL.CONNECTION device show`)
  because a connection name may contain a colon and `nmcli -t` escapes it. `check-system.sh` FAILs
  when any of the tools or the two scripts is missing.
- **Download is `yt-dlp` with three decisions** (`rofi-download`, the root menu's "Download" row
  and the launcher entry `org.lazyos.download.desktop`): a link is pasted (or taken from the
  clipboard when it holds one), the kind is video or **audio only**, and the file lands in
  `~/Downloads` under yt-dlp's own `Title [id].ext` name. `--no-playlist` keeps a shared link to a
  video inside a playlist from turning into a thousand files on a 58 GB eMMC; `--no-mtime` keeps
  the newest download the newest file (yt-dlp otherwise stamps it with the upload date); and
  `--no-simulate` is **not** optional even though `--print after_move:filepath` implies a dry run
  on its own - without it yt-dlp prints the path and downloads nothing, which reads as a silent
  success. `ffmpeg` (in `packages/tools.list`) merges the streams, and **node is why YouTube
  offers the high-quality formats at all**: yt-dlp 2026 needs a JS runtime for its "EJS"
  challenges, enables only `deno` by default and Debian ships none, so `/etc/yt-dlp.conf` names
  `--js-runtimes node` and the debug line goes from `JS runtimes: none` to
  `[jsc:node] Solving JS challenges using node` - the visible symptom without it is *some formats
  may be missing*, which reads as a site problem, never as a missing package. A download is a
  `busy` card whose `BUSY_STOP` is `pkill -f "[y]t-dlp"`; stopped, the card says the `.part` file
  is still in `~/Downloads` instead of claiming a file, and a failure is the last lines of
  yt-dlp's own message rather than a traceback. `~/Downloads` is created by `rootfs-setup.sh` (git
  cannot carry an empty directory) and by the script itself if it is gone.
- **The card's shortcut feedback, fixed where it was** (the 2026-10-20 report): `Alt+Escape` and
  `Alt+Ctrl+P` open the **power menu** (`rofi-power`: lock, suspend, hibernate, log out, reboot,
  shut down), which is what the manual claimed they did - both keys ran `bar-power`, the *battery*
  banner, so the key looked dead. The three destructive rows go through a `bar-pick` yes/no, and the
  root menu's `Log out`/`Reboot`/`Power off` rows are gone for the same reason: a stray Enter there
  rebooted the machine with no question. `Alt+Shift+F`/`Alt+Shift+S`/`Alt+Ctrl+S` run `grimshot -n`
  (the flag that prints and notifies the file, or that the region is on the clipboard); without it
  the screenshot keys are silent. `Alt+Shift+Return` opens `firefox-esr`. **Night light**: one
  helper, `~/.config/bin/night-light`, owns the command line and the mode, because the login line,
  the Settings switch and the resume hook have to agree about what `wlsunset` is running and three
  copies of the coordinates cannot. `login` starts the sunset schedule
  (`-l 20.67 -L -103.35 -t 4000`, 4000 K because 5500 K was close enough to daylight that the switch
  looked broken), `start` forces the warm end (`-T 4000 -t 4000`) for the switch - a toggle that
  only takes effect after sunset does nothing when it is tried at 17:00 - and the mode is one line
  in `$XDG_RUNTIME_DIR/lazyos-night-light`. The idle drop-in's `after-resume` runs
  `night-light resume`, because the panel comes back with its own gamma table and a client that
  survived the sleep no longer has any effect; with no state file it does nothing, so a night light
  that was off stays off. The screensaver kill in that chain is bracketed - see the `pkill -f`
  gotcha below. **Time zone**: `timedatectl set-timezone` rewrites `/etc/localtime` and nothing
  else, and glibc caches the zone the first time a process calls `localtime()`, so the bar kept the
  old time and the menu looked inert; `rofi-tz` restarts `waybar` and puts the new local time in the
  notification. **Settings** (`rofi-quick`): six probe groups render in parallel (it was the sum of
  two `bluetoothctl` clients, two `nmcli` calls and three `swaymsg` round trips), the SSID comes from
  `iw dev <if> link` because `nmcli device wifi` blocks until a scan already in progress finishes,
  and the sink from one `wpctl get-volume` instead of three `pactl` calls. **Command palette**
  (`Alt+Shift+D`): rofi's run mode asks the icon theme for an icon *named after the binary*
  (`rofi_icon_fetcher_query(str[0], …)`), which Adwaita has for none of them, so `make-icons.sh`
  renders a second table under those names; the element is a fixed 26 px column, so the binaries
  without an icon keep their text aligned. `check-system.sh` warns, never FAILs, when those PNGs are
  missing.
- **The key scheme mirrors Omarchy's**, with `Alt` where Omarchy uses `Super`: the panels are
  `Alt+Ctrl+A/B/W` (audio, Bluetooth, network - the same scripts the bar's indicators run),
  `Alt+Ctrl+O` is **Settings** (`rofi-quick`: the radio switches, night light, do-not-disturb,
  touchpad, tablet mode, rotation), `Alt+Ctrl+N` the **Notifications** history, `Alt+Ctrl+Q` the
  **calculator**, `Alt+Ctrl+V` the **clipboard history** and `Alt+Ctrl+E` the **emoji picker**,
  the root menu is `Alt+Space` (Omarchy's `Super+Space`; sway's `focus mode_toggle` had that key and
  was dropped), the scratchpad is `Alt+grave` (`Alt+minus` stays as an alias), lock is
  `Alt+Ctrl+L`, and `Alt+Escape` **and** `Alt+Ctrl+P` open `rofi-power` (Omarchy's system menu and
  Super+Ctrl+P), while `bar-power` stays the *battery* panel behind the bar's indicator - a banner of
  numbers under a key documented as a menu of actions read as "does not work". Three deliberate
  deviations, each because a sway default owns Omarchy's letter: close is `Alt+Q` (`Alt+W` is
  tabbed), the key view is `Alt+Ctrl+K` (`Alt+K` is vim focus up, `Alt+Shift+K` moves the window), and there is no "cycle layout"
  key (sway's layouts are stacking/tabbed/split, not dwindle/master/scrolling).
  `bindswitch`/`wtype`/`iio` facts for the tablet work are in the sensor bullet above; the full
  mapping is `usr/share/doc/lazyos/03-shortcuts.md`. **Every binding carries its description on a
  comment line above**, and it covers the bindings below it until a blank line or another comment,
  because `rofi-keys` shows those comments - a binding without one appears as its raw command in the
  key view, and a group header with no blank line after it would label the first binding instead.
  Bindings inside a `mode "..."` block are shown with the mode's name.
- **The touchpad's four three-finger swipes: two move the session, two type into Firefox.**
  Left and right are `bindgesture swipe:3:left|right workspace next_on_output|prev_on_output`
  (`config.d/22-workspace-gestures.conf`, the file that already held them); up and down go to the
  next / previous tab of the **focused window when it is Firefox**, because they type **Ctrl+Tab**
  and **Ctrl+Shift+Tab** into it. sway has no command that hands a key to a client - `swaymsg`
  speaks to the compositor, not to the window - so the two vertical bindings are
  `exec wtype -M ctrl [-M shift] -k Tab -m ...`, the virtual keyboard protocol the emoji picker and
  dictation already use: the window receives a real key press. The modifiers are pressed **and
  released in the binding** (`-M` holds one for whatever follows, `-m` lets it go): `wtype` also
  releases them when it exits, and the written pair is what makes the chord readable and
  independent of that exit path - the emoji picker's `-M shift -k Insert -m shift` is the same
  form. **The Firefox gate is a criteria prefix on the command, not a helper script**:
  `[app_id="(?i)^firefox" con_id="__focused__"]`, and both halves are load-bearing. Measured on
  the maintainer's session: `[app_id="firefox-esr"] exec …` ran while a terminal was focused - a
  criteria matches *every* window of that app, not the focused one - and adding
  `con_id="__focused__"` stopped it, which is the half that makes the swipe do nothing outside
  Firefox. `(?i)^firefox` covers `firefox-esr` (Debian's app id here) and upstream `firefox`. The
  prefix is also the fast form, and that is why it is not a script: it runs inside the compositor,
  so a `nop` through IPC measured 4.1 ms without criteria, 3.8 ms with `con_id="__focused__"` and
  4.4 ms with the regex - noise, i.e. no measurable cost - and in another application it starts
  **no process at all**; `wtype` itself costs 11.9 ms (measured with `wtype ""`, so it is process
  startup plus the Wayland connect, not the key), which is the floor and under one 60 Hz frame.
  The script alternative (`swaymsg -t get_tree | jq | grep`) measured 12.1 ms per swipe in *every*
  application and about 24 ms in Firefox; a resident injector holding the keymap open would reach
  ~0.2 ms and is exactly the daemon this port does not keep. Three silent failures are what the
  check exists for: a `wtype` that is not installed makes the swipe complete with `Command not
  found` in the journal while the binding reads correctly; a session binding on Ctrl+Tab would
  swallow the injected keys before the window sees them (there is none - Alt is `$mod`, and
  `Alt+Tab` is the window switcher, not a tab switch); and a binding that loses the criteria prefix
  types Ctrl+Tab into whatever is focused. `check-system.sh` FAILs when any of the four bindings
  (the criteria included) or `wtype` is missing; the manual table and paragraph are in
  `03-shortcuts.md` § Windows, the symptom is `06-troubleshooting.md` § *The three-finger swipes
  do nothing up or down*, and `rofi-keys` lists the four with the comment above each as their
  description, because it parses `bindgesture` like every other binding. **A duplicate
  `bindgesture` is not a silent override like a duplicate `bindsym`**: the same gesture in a user
  drop-in and in the system one still applies, `sway -C` and a fresh sway accept both files, and
  `swaymsg reload` counts it as a config error and puts the red *There are errors in your config
  file* bar across the top - the only visible cause being a line that reads correctly. That is how
  the temporary diagnostic behind these two swipes was found, and it is why `check-system.sh` WARNs
  when a user drop-in repeats a system gesture (the device's own `~/.config/sway/config.d` is the
  person's to edit) instead of editing `/etc/sway/config.d/22-workspace-gestures.conf`; the symptom
  and the `/etc` alternative are in `06-troubleshooting.md` § *The red bar*. It is also why the
  Firefox filter is one regex inside the one binding and not a second `bindgesture`: the criteria
  cannot say "app_id or class" in a single bracket, and a Firefox forced onto Xwayland
  (`MOZ_ENABLE_WAYLAND=0`) has no app_id and therefore no gesture - a deliberate limit the
  troubleshooting page states instead of a second binding that would raise the bar.
- **The menus are one-shot rofi scripts, and slow actions say so twice.** `~/.config/bin/rofi-*`
  are the menus behind the root menu (`rofi-quick`, `rofi-notify`, `rofi-clean`, `rofi-kill`,
  `rofi-tz`, `rofi-places`, `rofi-media`, `rofi-calc`, `rofi-layout`, `rofi-learn`, `rofi-emoji`,
  `rofi-clip`, `rofi-about`, `rofi-dns`, `rofi-pkg`, `rofi-color`, `rofi-remind`, `rofi-download`,
  `rofi-grep`, `rofi-format`, `rofi-swap`, `rofi-file-actions`); they share
  `lib-rofi.sh` (`ui_row` glyph+bold+dim row, `ui_pick` table-and-key round trip because rofi hands
  back the rendered row, `ui_pick_alt` when a second action sits on Alt+1 - it returns `alt:KEY` and
  rofi exits 10 for that, `ui_say` for results) and each is one process while open. **A slow action
  needs visible progress, and the progress is the stop button**: `busy start TAG LABEL MESSAGE
  [ICON]` opens a mako toast that does not expire, `busy end`/`fail` turn the same card into the
  result (same `x-canonical-private-synchronous` tag, so mako replaces instead of stacking), and
  `bar-busy` is the waybar `custom/busy` module that shows a braille spinner plus the label in
  yellow while a state file exists in `$XDG_RUNTIME_DIR/lazyos-busy` - zero width when idle, and a
  state file older than five minutes is dropped so a killed script cannot leave a spinner forever.
  **Clicking that spinner stops the task it names**: the producer sets `BUSY_STOP` to the one shell
  line that ends its own slow step (`pkill -f "[n]mcli.*device wifi list"`, a pattern, not a pid
  this script would have to guess from the label), `busy start` writes it to `<tag>.stop` with mode
  0600 because something executes it, and the module's `on-click` (`bar-busy stop`) turns the card
  into `Stopped <label>` and then runs that line - the card first, so a producer that starts a card
  of its own finds an empty state instead of racing the removal of its own. The producer then asks
  `busy cancelled TAG`, which answers 0 once the tag has no card, and reports **nothing**: without
  that question a killed command reads as a result (half a scan drawn as a network list) or as a
  failure (`Wi-Fi: ` in a critical toast). A task with no `BUSY_STOP` says so when clicked and
  keeps running; a `.stop` from a previous run with the same tag is never inherited; the card with
  one ends its tooltip with *click to stop*. Deliberately **not** stoppable: an `apt` install
  (dpkg's lock outlives it), the DNS switch (it has already written the profile), and the radio
  toggles (they finish in milliseconds). Wired into the Wi-Fi scan/join, the Bluetooth scan, the
  `du` walk in Free space and the dictation recording; a new slow action gets the same two lines
  and, when it can be interrupted safely, one more.
  **The root menu's Password row is `openssl rand -hex 16`**, and the three choices in it are
  deliberate: **hex** has no shift key, no `+`, `/` or `=` to re-quote when it is pasted into a
  file, a URL or a `.env`, and 32 characters of hex is still 128 bits; the value goes **straight
  into the clipboard** and **never into the notification body**, because mako keeps a history and
  a password printed in a toast is a password in `~/.cache/mako` - the card says only that it is
  copied. Nothing is stored: the row is one `openssl` process, and the clipboard is the clipboard
  (where `rofi-clip`'s last row clears it).
- **Clipboard history and emoji are the two exceptions that were worth it.** `rofi-clip`
  (`Alt+Ctrl+V`) lists what `cliphist` kept, which is fed by
  `exec wl-paste --watch cliphist store` in `config.d/27-clipboard.conf` - **the only resident
  process the session has** (~2 MB). There is no history without a watcher: `wl-clipboard` alone
  only knows the current selection, which is what the "no daemons" rule had to give way to. Rows
  are text, link or image by content; an image is listed by what `cliphist` knows about it, not by
  a thumbnail (a rofi row is one line), and "Save the newest image" decodes it into
  `~/Pictures/clipboard/<date>.<ext>` (extension from `file --mime-type`) and opens it with
  `xdg-open`, which routes a path through `lazyos-view` and therefore shows it in `imv` now that
  the viewer is in the image. The database is `~/.cache/cliphist/db` and
  holds everything copied, secrets included, so the menu's last row clears it.
  `rofi-emoji` (`Alt+Ctrl+E`) reads `~/.config/emojis.tsv`, **generated on the build host by
  `scripts/make-emojis.sh`** from Python's `unicodedata` (name, glyph, block; the emoji blocks plus
  Dingbats and Miscellaneous Symbols, skin tones and ZWJ sequences left out because they are
  combinations of rows that already exist) **and then filtered against the character map of the
  font fontconfig resolves for `emoji`** (`fc-match -f '%{file}' emoji` + `fc-query
  --format='%{charset}'`): 692 of the 1988 rows named codepoints `fonts-noto-color-emoji` does not
  draw (chess pieces, arrows, alchemical signs, several ballot boxes), and every one of them was a
  tofu box in the picker. The table is 1296 rows and pure data - no comment line - and the sidecar
  `~/.config/emojis.font` pins the font's SHA256 and family, which is what `check-system.sh`
  compares with the installed font; a font upgrade means re-running the script on a host with
  `fontconfig` and the font. It is committed, so the image carries no Python and no Unicode
  package. Insertion is Omarchy's: `wl-copy` the glyph and type a virtual
  **Shift+Insert** with `wtype` (`wtype -t` would need a keysym for the codepoint, which most emoji
  have not); Alt+1 is the clipboard write alone, for a surface that refuses a virtual keyboard.
  `fonts-noto-color-emoji` is what draws them - without it every glyph is tofu - and
  `check-system.sh` asks `fc-match emoji` and compares the pin. The other five lists
  (`rofi-dns`, `rofi-pkg`, `rofi-color`, `rofi-remind`, `rofi-about`) are plain one-shot
  scripts and no resident process:
  DNS sets `ipv4.dns`/`ipv4.ignore-auto-dns` on the connection that is up and brings it back
  (`nmcli connection up` - hence the busy card and then the read-back, not a claim), packages are
  `apt-cache search` then `apt-get -y` behind the same card, colour is `slurp -p` + `grim` 1x1 +
  `ffmpeg -f rawvideo` (no ImageMagick) plus the solarized palette as swatch rows, reminders are
  transient `systemd-run --user --collect` timers that fire `notify-send` once and are listed back
  from systemd (nothing on disk, and they die with the session's user manager), and about reads
  procfs/sysfs. **A reminder is not a countdown**: the flow is a title, then an **optional** note
  (Enter or Escape there means "no note", not "forget it" - the timing menu is where a reminder is
  abandoned, so one keystroke cannot cost the title), then when - the five delays plus a `repeat`
  group (on a date and time, every day at a time, every weekday, every week on a chosen weekday,
  every month on a day 1-28, 28 rather than 31 so no month is skipped). **Every custom expression
  goes through `systemd-analyze calendar --iterations=1` before anything is created**, with
  `LC_ALL=C` so its `Next elapse:`/`Normalized form:` labels parse, and a rejection echoes the
  tool's own words (`Failed to parse calendar specification '*-*-* 25:00': Invalid argument`) and
  returns to the When menu with the title and note still known; the confirmation shows systemd's
  own next occurrence. The title and the note live in the timer unit's `Description`, which is what
  the list shows on a second, dimmer line, and cancelling reads `LoadState`/`Description`/
  `TimersCalendar` before claiming anything. The bar's `sbs-8-000b` gauge is the
  battery there; a `hid-*` power supply belongs to the touchpad and reads 0%.
- **Translate is a key, not a right-click, and the selection is the text.** `Alt+Ctrl+T`
  (`~/.config/bin/translate`) sends the mouse selection - the primary selection, which a drag
  already writes - or the clipboard into one language and copies the answer; a long answer opens in
  a read window. foot *could* have run it from the mouse: 1.28 here has `pipe-selected`,
  `[mouse-bindings]` takes any key-binding action, and `pipe-selected=[sh -c "…"] BTN_RIGHT` parses
  once `select-extend=none` frees the button (`foot -C -c …` is the check). It is a key because that
  binding would take right-click away from every TUI under foot and right-drag-select with it, and
  because the key works from the browser, a document and the file manager too. **The engine is
  chosen by measurement, not taste**: Google's `translate_a/single` - the one `translate-shell`
  uses - answers this machine with a captcha page, while
  `clients5.google.com/translate_a/t?client=dict-chrome-ex` answers plain JSON, auto-detects the
  source and echoes it, so the card can say `Spanish → English` and an English selection can answer
  **Already English**; it takes GET only (POST 302s to `/sorry`), which is why the query rides in
  the URL and the selection is capped at 3000 characters. MyMemory (no key, but 500 characters per
  request, so the text is folded into 450-character pieces) is the fallback, and when neither
  answers the card says so instead of showing half a translation. `translate-shell` is contrib
  (277 kB) and would have been a command that never works here. `~/.config/lazyos/translate.conf`
  holds `target=en` (the default when the file is absent) and `fallback_source=`, the language
  MyMemory is told when it cannot detect one; **Settings → Translate** (`rofi-quick`) is the list,
  and `translate config` is the same menu from a terminal. It is a `busy` card with the bar's
  spinner and a `BUSY_STOP` (both endpoints), so a slow network is visible and a click stops it;
  nothing is resident. Privacy is stated rather than hidden: the result is on the clipboard and
  therefore in `cliphist`, and the card is in mako's history. Witnesses: `check-system.sh` FAILs on
  the script, the binding, the Settings row, `curl`/`jq`/`wl-paste`, the `globe` icon, an unknown
  language code and `03-shortcuts.md`'s row, WARNs on an endpoint that is not 200, and SKIPs
  offline; the manual pages are `02-rofi.md` § Translate, `04-software.md` § Translate and
  `06-troubleshooting.md` § *The translation says the engine refused*; the welcome card and
  `03-shortcuts.md` teach the key.
- **The Developer menu is the account as an archive, and three decisions make it
  safe.** `rofi-dev` (`Alt+Ctrl+D`, the root menu's *Developer* row, the launcher
  entry `org.lazyos.dev.desktop`) is a three-row front end over
  `~/.config/bin/lazyos-profile`, and the helper is usable from a terminal - this
  image answers ssh, and a machine whose session will not start still has a
  profile worth saving - so the passphrase is `read` with `stty -echo` on a
  terminal and `bar-pick -password` in the session, and the destructive
  confirmation is the typed word `restore` there and two rofi rows (**Cancel
  first**) here. **A terminal is not `[ -t 0 ]`**: sway's `exec` inherits the VT
  the compositor was started on (`/dev/tty7` here), so a menu opened by a key has
  a tty on stdin that nobody is looking at - the first card that tried it showed
  `Packing the profile…` while `read` waited on that VT for ever and the session
  showed no prompt at all (reported on hardware; the fix is `on_terminal()`,
  which also requires the process to be the terminal's *foreground* group, `ps -o
  tpgid=` equal to `ps -o pgid=`, with `bar-pick` as the fallback whenever
  `WAYLAND_DISPLAY` is there). `have_prompt` uses the same predicate, so a
  scripted run is refused before the tar is built and not after, and the card
  says `Waiting for the passphrase…` before the prompt opens. **The
  archive is a tar, not a copy**: the disks this menu is pointed at are exFAT,
  which drops modes and symlinks, so a copied `~/.ssh/id_ed25519` would lose its
  `0600` and ssh would refuse it on the machine that restored it; the modes ride
  inside the tar. **The encryption is gpg symmetric**, because the archive
  carries `~/.ssh` and `~/.gnupg` and a lost stick must not be a lost identity;
  `age -p` was the smaller first choice and cannot take a passphrase from a
  program (it reads a TTY), while gpg's `--passphrase-fd` can, with
  `--pinentry-mode loopback` making the fd authoritative in 2.1+ - the
  passphrase is a `0600` file in the private staging dir handed over as fd 3,
  never argv, and the plaintext tar is removed by a trap on every exit path
  (which is also why `build_tar` writes tar to a file and runs zstd on it
  instead of piping: `pipefail` is not set in sh, so a tar that failed halfway
  would look like a good archive). `gnupg` (not the bare `gpg` package) is in
  `packages/tools.list`, for the agent that makes `git commit -S` work - and an
  export does start `gpg-agent.service`, because gpg 2.x routes even a symmetric
  passphrase through it; the packaged socket activates it, nothing enables it,
  and it idles out, which is why `check-system.sh`'s expected user units name
  it and its own round-trip kills the agent it started. **The
  profile list is the feature**: `~/.ssh`, `~/.gnupg`, the shell and the configs
  of the tools the image ships, `~/.local/bin`, the user applications and the
  agents - the menu's account directories (`~/.local/share/lazyos/agents`, which
  hold the sign-ins), OpenCode's config and the CLIs' own homes (`~/.claude`,
  `~/.codex`, `~/.gemini`, `~/.copilot`, `~/.config/github-copilot`), whose
  credentials and settings travel but whose conversation transcripts and caches
  (`.claude/projects`, `.codex/sessions`, `.copilot/session-state`,
  `plugins/repos`) are excluded by pattern - and the **Firefox profile**
  (`~/.config/mozilla`, the XDG path Debian's Firefox uses here, or `~/.mozilla`
  with `MOZ_LEGACY_HOME=1`: `places.sqlite`, `logins.db` + `key4.db`, `cert9.db`,
  cookies and `extensions/`, minus `storage/`, `security_state`,
  `sessionstore-backups` and the `-wal`/`-shm` checkpoints, which is how a
  238 MB profile becomes a few MB) - plus a plaintext manifest with the
  revision, `apt-mark showmanual` and `npm ls -g`; caches, `~/.cache/mozilla`,
  `~/Dev` and `~/.local/share/opencode/opencode.db` (the OpenCode sign-in and
  49 MB of session history here) are deliberately out, and the manual says so. **Restore backs up first** to
  `~/.cache/lazyos/backups/profile-before-restore-<date>.tar.zst` (unencrypted,
  `0600`, same machine and same account) and refuses an archive with an absolute
  path or a `..` before extracting anything; it is not stoppable (a killed
  extract is a half-restored home) while the export is, with
  `BUSY_STOP='pkill -f "[l]azyos-profile\../archive\.tar"'` matching the
  staging path and never the script's own command line. The diagnostics bundle
  (`report`) is the system report, this boot's journal, `dmesg` and one page of
  hardware/package facts, unencrypted because nothing in it is a secret, and its
  README says the journal still names networks and paths. Witnesses:
  `check-system.sh` FAILs on the tools, the two scripts, the gpg round-trip (the
  exact `--pinentry-mode loopback --passphrase-fd` pipeline on a throwaway
  passphrase), `export --dry-run` naming `.ssh` and `.gnupg`, and the three ways
  in, and WARNs when the launcher icon or the manual section goes; the manual is
  `04-software.md` § *Your profile, on a stick`, the key is in
  `03-shortcuts.md`, and the two symptom lists are `06-troubleshooting.md`
  §§ *The Developer menu does not offer the stick* and *The restored profile did
  not take effect*.
- **The About windows name `Alt+Q`, not foot's close key.** `Ctrl+Shift+W` is foot's
  built-in and did not reach the held windows on this keyboard; `Alt+Q` is sway's own `kill`
  and closes them. The three footers (fastfetch, boot time, devices) and
  `docs/test-checklist.md` say the same key, and `03-shortcuts.md` explains why.
- **The root menu is a launcher entry too.** `org.lazyos.menu.desktop` (`Name=LazyOS
  Menu`, `Icon=lazyos`) runs `~/.config/bin/bar-menu`, so the list Alt+Space opens is
  reachable by typing. Its `Exec` is a `$HOME` script, so `check-system.sh` asks for the
  script's executable bit, not only the `.desktop` file.
- **favnyr unmounts through `udisksctl`, and `fuser` is what names the blocker.** The
  removal action is the same tool `rofi-places` calls, so the two cannot disagree about a
  disk; when udisks refuses because something holds the mount, the window runs
  `fuser -m <mountpoint>` to fill its *In use by: …* sentence, which is why `psmisc` is in
  `uis/sway/packages.list` and `check-system.sh` FAILs without it. **`eject` is no longer a
  dependency**: it was the GTK3 file manager's, which exec'd it by name, and no program in
  this image does any more - the package left the list with the manager.
- **The reminder When menu carries its own help row.** "How do I fill these in?" answers
  in a notification and `continue`s, so the menu reopens with the title and note still
  known (they live outside the loop); the date/time prompts also name their formats
  (`YYYY-MM-DD HH:MM`, 24-hour `HH:MM`). A row without its `help)` arm is a dead key.
- **zoxide is initialised last in `~/.bashrc`.** It appends to `PROMPT_COMMAND`, and
  starship replaces that variable wholesale, so init must come after starship and after
  `~/.bash_aliases`; otherwise the hook is dropped and the first `cd -` prints
  `zoxide: detected a possible configuration issue`. `zd` sends a first argument of
  `-` to `builtin cd -` (which prints the path), not to zoxide.
- **The first boot gets one welcome window, and it is once per install, not once per login.**
  `~/.config/bin/lazyos-welcome` (started by `~/.config/sway/config.d/30-welcome.conf`, which also
  floats and centres it) opens one `foot --app-id=lazyos-welcome` page with the nine keys a new
  install needs - `Alt+Space` (menu), `Search`/`Alt+D` (launcher), `Alt+Search` (file search),
  `Alt+Ctrl+F` (file list), `Alt+Ctrl+T` (translate), `Alt+Return` (terminal), `Alt+Q` (close),
  `Alt+Escape` (power) and `Alt+Ctrl+K` (bindings) - and `read` closes it on Enter. The marker is
  `${XDG_STATE_HOME:-$HOME/.local/state}/lazyos/welcome` and holds `/etc/machine-id`; it is written
  **after** the window closes, so a missing or failing `foot` leaves no marker and the next login
  tries again, while a session restart with the same machine id stays quiet. A rebuilt card and an
  `install-emmc.sh` copy get a fresh machine id (the script blanks it), which is what makes it "once
  per install" rather than "once per home". The page is plain ASCII on purpose: foot falls back to
  Symbols Nerd Font Mono, where a Font Awesome codepoint is a tofu box, and the window is a card to
  read, not a terminal to work in. Nothing stays resident: one foot process while the window is
  open. The manual section is `01-welcome.md` § First boot; `check-system.sh` runs the script
  against a stub `foot` in a temp state directory to prove the marker is not written on failure.

- **The manual is rendered, not printed.** `usr/share/doc/lazyos/*.md` (seven pages: welcome, rofi,
  shortcuts, software, the system, troubleshooting, agents) is opened by `rofi-learn` with **glow**
  (`packages/tools.list`), styled by `~/.config/glow/solarized.json` - a *glamour* style, unrelated
  to the rofi theme; glow lives in `~/.config/glow/glow.yml`, whose `style:` must be an absolute
  path because glow treats a bare name as one of its built-ins. The style gives the headings a `▌`
  bar and carries a full `chroma` map for fenced code: without one glamour paints code blocks in the
  document colour, so a page with commands in it was a wall of base1 text (the "ugly" rendering).
  **Every colour in it must carry its `#`**, and that is the second version of the same bug: glamour
  parses `#rrggbb`, but a bare `839496` is taken as a *256-colour index*, so it emits
  `\x1b[38;5;839496m`, the terminal ignores the invalid index and the page falls back to its own
  colours with no error anywhere. The style shipped that way - 23 values bare, 28 hashed, which is
  why the headings looked right while every inline code span (the `code` colour) was an empty white
  block and the chroma map was half applied. `check-system.sh` warns when a value loses its `#`;
  the rendered result is verified by capturing glow's output in a pty and checking for `38;2;`
  (truecolour) instead of `38;5;<six hex digits>`.
  The first row of the menu searches
  every page with `rg`; the second lists the machine's **own** manuals - `apropos -- "$q"` over
  the whatis database man-db builds, one row per page with `name (section)` and its description,
  and Enter running `man SECTION NAME` in foot, where `MANPAGER` renders it in the same palette.
  That second row is what `man-db` in `packages/tools.list` is for, and why
  `check-system.sh` FAILs without `apropos`/`man`: the row would otherwise answer a search that
  found nothing. Nothing stays
  after the page closes. `rofi-keys` (Alt+Ctrl+K,
  Omarchy's Super+K) is *generated* from `/etc/sway/config`, `config.d/*.conf` and
  `~/.config/sway/config`: it expands `set $var`, takes the comment above each binding as its
  description, and shows the list with `rofi -dmenu -markup-rows` (bold keys, dim descriptions); the
  row-to-command mapping lives in a temp TSV because a markup string cannot be parsed back, and Enter
  copies the command. **Every row carries a picture**, not only its section line: rofi drops the icon
  column for a row without one, so that row's text starts 34 px to the left of its neighbours - which
  is what the maintainer's report ("no icons in Alt+Ctrl+K") was looking at. The name is the tool the
  binding runs (the theme has one per binary: `rofi-power`, `rofi-files`, `rofi-clip`, `bar-audio`,
  `dictate`, `sway-lock`…), the action it sends (`window-close` for a kill, `view-refresh` for a
  reload, `application-exit` for an exit, and the `go-*` arrow of each direction of `focus`/`move`), or
  the section's own picture for everything else (`preferences-system-windows` for the workspace and
  window bindings). The tool step exists so the picture says what the row does, the action step
  because a list of 43 identical window outlines earns nothing; every name is one
  `scripts/make-icons.sh` renders, and `check-system.sh` reads them out of the awk (the `return`
  names and the `split(..., tl)` tool list) and FAILs when the theme stops carrying one, since a name
  nobody draws is a row that silently loses its column and logs nothing.
  A hand-written list would drift the first time someone edits the configuration,
  which is why this one is parsed. `dictate` (Alt+Ctrl+X, Omarchy's Super+Ctrl+X) records with
  `parecord` at 16 kHz, **normalises the level with `ffmpeg -af dynaudnorm`** (the internal DMIC is
  quiet and whisper is very sensitive to it, which is what "a little bit low" meant),
  transcribes with `whisper-cli -t $(nproc)` and types with `wtype` (clipboard as the
  fallback when the focused surface refuses a virtual keyboard); the model is fetched by
  `dictate-model` on first use - 78 MB tiny.en, 148 MB base.en - **verified against a pinned SHA256**
  and kept in `~/.local/share/whisper/`, so it survives a rebuilt image. Recording stops itself after
  60 s and its state is a pid file in `$XDG_RUNTIME_DIR`, never a watcher. Both silent phases
  (recording, transcribing) are `busy` cards: the toast stays up saying "Alt+Ctrl+X stops it", turns
  into "Transcribing…" and then into the typed text or the reason it failed, and the bar spins while
  either runs - the seconds of silence were the other half of the complaint. Stopping is **SIGINT**
  (what makes `parecord` write the final wav header), with SIGTERM after two seconds so the menu
  never hangs, and the root menu keeps a **Dictation** row so the mouse can stop it too.
  Omarchy runs voxtype (a Rust daemon) for the same interaction. `sway-hibernate` refuses unless three things hold, each with
  its own message: `/sys/power/state` has `disk`, a **disk** swap (not zram) of at least 2/5 of RAM
  exists, and the kernel command line carries `resume=`. **The size it checks is `/proc/swaps`
  column 3 (`Size`), never column 4 (`Used`), and that is a bug this port shipped**: an idle swap
  reads `0` in `Used`, so a freshly created 8 GB partition on the microSD was refused as "0 MiB"
  while `swapon --show` was reporting 8G. `--status` exists for that reason - it prints the
  conditions (`kernel=`, `swap=`, `swap_kb=`, `need_kb=`, `resume=`, `resume_broken=`,
  `hibernate=`) and does nothing else, so `check-system.sh` can ask it what size it read and FAIL
  when the answer is not the kernel's own, without a report ever being able to hibernate the
  machine. `resume=LABEL=lazyos_swap`
  is the intended wiring (initramfs-tools resolves the label through udev, so an SD swap partition
  labelled `lazyos_swap` is enough) - a swap *file* cannot be used for resume because it has no
  label, and `resume=` is not in the image's own command line on purpose: a machine with no
  `lazyos_swap` partition would wait 30 s in the initramfs (`local_device_setup` in
  `local-premount/resume`) before mounting root, so the port keeps it opt-in -
  `CONFIRM=yes RESUME=yes scripts/install-kpart.sh` adds it once on the machine, `install-kpart.sh`
  carries an existing `resume=` across every later kernel update (dropping it is silent: the machine
  hibernates and comes back cold, with the session gone), and `RESUME=no` removes it.
  **A `resume=` that names nothing on this boot is a refusal of its own** (`resume_broken=yes`):
  the image would be written and the next boot would come up cold, which reads as hibernation losing
  the session rather than as a spec that points at a card that is gone - so the helper names both
  ways out (`swaplabel` the card, or `RESUME=no` on the partition) instead of hibernating. And
  because it runs from a key with no terminal, `systemctl hibernate`'s own refusal (*Not enough swap
  space*, *Access denied*) is captured and becomes the card rather than a journal line, which is
  indistinguishable from the key doing nothing. Turning `resume=` on needs the kernel partition
  rebuilt and nothing else: the running initramfs already carries
  `scripts/local-premount/resume`. The default image has no disk swap, so the menu entry explains
  instead of hibernating into a swap the machine cannot resume from. **Auxiliary swap** (Places and
  the root menu) prepares that partition and the fstab line that activates it at every boot;
  `resume=LABEL=lazyos_swap` still has to be added to the kernel command line and needs the kpart
  rebuilt. `check-system.sh` checks the manual, `rofi-keys`, the dictation tools,
  the model (SKIP when absent) and the hibernate conditions (SKIP/WARN, never FAIL, plus the
  `--status` size agreement above).
- **Agents** (`rofi-agents`, Alt+Ctrl+Shift+A = Omarchy's Super+Shift+Ctrl+A) is the local half of
  Omarchy's agents feature: launch (in foot, from `~/Work`, because the agents refuse to remember
  trust in `$HOME`), install (`npm install -g --prefix ~/.local <pkg>` - an explicit prefix, so npm
  never needs root and never touches `/usr/local`, and `~/.local/bin` is already on PATH), default
  agent, and accounts. **The root menu's AI group opens with the action, not the list**: its
  `Agent session` row runs `rofi-agents --default`, which starts the agent the menu marks `●` - the
  recorded default, OpenCode on a fresh card because the image ships it, then the menu's own order
  when the recorded one is no longer installed - in `~/Work`, so a session is one click instead of a
  click and a pick of the same row. With no agent CLI at all it raises a card ("the Agents menu
  installs one with npm"), because a foot window on a missing binary closes at once and reads as a
  dead key. `check-system.sh` FAILs when either half goes (`--default` in the script, the row in
  `bar-menu`), and the manual paragraph is `07-agents.md` § *A session in one click*. An "account"
  is a directory under
  `~/.local/share/lazyos/agents/<agent>/<name>` used through the variable the CLI itself documents:
  **`CLAUDE_CONFIG_DIR`** for Claude Code and **`CODEX_HOME`** for Codex. Gemini and
  Copilot have no such variable, and OpenCode's `OPENCODE_CONFIG_DIR` moves only its
  configuration (the sign-ins live in the database under the data directory, which two
  accounts would share), so the menu says so instead of pretending. A new account is created
  empty on purpose - the first launch there runs the CLI's own sign-in, so two subscriptions never
  share a token. **Every row carries the session's glyph** from `lib-rofi.sh` - a code sign for
  an agent, two people for its accounts, a download arrow for an install, a gear for the default,
  a terminal for the shell, a bin for removing an account and an undo arrow for going back - and a
  display/key TSV maps the rendered row back to its action, because rofi hands back the row and
  markup cannot be parsed back; `-markup-rows` is what makes the glyphs draw instead of printing
  their tags. **The usage meters are not ported**: Omarchy's bar panel reads provider APIs (and
  local session logs) that change shape and need credentials, and a broken meter is worse than none;
  the shape that would fit here is a stdlib `python3` script printing one line for a waybar `custom`
  module, not a daemon and not a new language. Nothing in this feature is resident: the menu is rofi
  (~16 MB while open) and then the agent is the agent.
- **OpenCode is the one agent the image ships, installed at `~/.opencode/bin`.** The other CLIs are
  the user's (`npm install -g --prefix ~/.local`); OpenCode is the agent this port is built with, so
  a fresh card should not have to download it before it can work. `tools/opencode/` pins the arm64
  tarball of the npm platform package (`@opencode/cli-linux-arm64`, which is just
  `package/bin/opencode`, so no npm runs at build time), `build-rootfs.sh` fetches it into
  `$SRC_DIR`, verifies the committed SHA256 and stages it, and `rootfs-setup.sh` extracts it into the
  user's home. **Not `/usr/local/bin` on purpose**: `opencode upgrade` picks its update method from
  where the binary lives - *Using method: curl* from `~/.opencode/bin`, where it rewrites the file in
  place as the user, and *installation method not found* from anywhere else, which in an agent that
  changes every week leaves a version only a rebuilt image can move (both measured on v2.1.3). It is
  also what keeps the image **signed out**: the credentials live in `~/.local/share/opencode`, which
  the build never creates and `lib-dist.sh` now excludes from anything published (its database holds
  provider sign-ins). The directory is appended to `PATH` in `90-cli-tools.sh` and prepended in
  `rofi-agents`, because sway's environment has neither it nor `~/.local/bin` - and the menu's
  install line passes `--allow-scripts=<package>`, since npm 12 blocks the postinstall that picks
  OpenCode's platform binary, leaving a stub that `command -v` would call installed. `check-system.sh`
  FAILs when the binary is missing or does not answer `--version` (with a throwaway `$HOME`: the
  binary creates its data directory and a log on startup, and the report changes nothing) and when
  the menu loses the PATH line. Bumping it is `tools/opencode/README.md`; the manual section is
  `07-agents.md`.
- **The bar's menus are short-lived `rofi -dmenu` popups, not panels.** Deboma answers a click on
  the bar with a Quickshell panel, and that shell is 361 MB of PSS - more than half of the Deboma
  session. `uis/sway/files/home/.config/bin/bar-*` reproduce the same interactions through
  `bar-pick` (rofi's dmenu, themed by `~/.config/rofi/solarized.rasi`), with `nmtui`,
  `bluetoothctl` and the apt updater in `foot --app-id=popup` (floated by
  `config.d/25-bar-panels.conf`) and `swaynag` for the power panel, so nothing stays resident;
  `jq` is the only package the bar itself added (rofi came with the launcher). Two
  things follow from that and both are easy to get wrong: `custom` modules only re-run on an
  interval, so every `bar-*` ends with `pkill -RTMIN+8 waybar` and every such module declares
  `"signal": 8`; and a module with nothing to say prints `{"class":"empty"}` so that
  `#custom-updates.empty` / `#custom-keyboard-layout.empty` collapse it, because waybar would
  otherwise leave an empty card on the bar. waybar's `backlight` module names its device
  (`backlight-lcd0`, not `backlight`); a wrong name shows nothing and logs nothing, so
  `check-system.sh` compares it with `/sys/class/backlight`.
- **Notifications are `mako`, and Debian's package is `mako-notifier`.** The name `mako` belongs to
  a Python template library (`apt install mako` fails with *Unable to locate package*), while the
  binary is still `/usr/bin/mako`; `uis/sway/packages.list` and the drop-in must use the package
  name. `config.d/20-notifications.conf` runs `exec mako` once per session and the theme is
  `~/.config/mako/config` (base03/base1/cyan, `outer-margin=40,10,10,10` so nothing covers the 30 px
  bar, `layer=overlay` so a critical one still shows over a fullscreen app). The config has no
  `--check`, but `mako -c <file>` parses it before connecting: a bad key prints
  `Failed to parse option '<key>'`, a good file gets as far as
  `Failed to acquire service name: File exists`. A daemon that is not running loses notifications
  with no log line at all, so `check-system.sh` asks the session bus for
  `org.freedesktop.Notifications` (`busctl --user list`) instead of trusting the package.
- **The idle screen is LazyOS's, and it loops cheaply.** Deboma runs `omarchy-screensaver` 150 s after
  the last input and locks at 300 s (`shell.json`); that screensaver is a fullscreen terminal looping
  `ttfx` at 120 fps - measured here, 78 MB of Python and 74-86 % of one core on a 200x60 canvas, i.e.
  120 full 1920x1200 repaints per second through panfrost, which is what the battery notices. The same
  chain runs here (`config.d/23-lazyos-screensaver.conf`: `swayidle -w` with 150 s screen, 300 s lock,
  **360 s `swaymsg "output * power off"`** - DPMS, because the lock covers the screen but leaves the
  panel lit, which on this IPS panel is the largest idle consumer; `resume` turns it back on for an
  idle period and `after-resume` for a suspend, which is not an idle timeout and would otherwise
  wake to a dark screen - `before-sleep` lock, and `after-resume` cleanup plus `night-light resume`),
  with Debian's `tte`
  instead of `ttfx`, at **30 fps**,
  looping with `--reuse-canvas` over a curated list (`decrypt middleout print sweep wipe slide
  expand`; the effects that end with the text sitting still, so the next one starts from the wordmark).
  The art is `~/.config/lazyos/screensaver.txt`: the **z_** mark in block characters, `lazyOS_`,
  `:: lazy by design ::` and the arcade line `» press any key to play «` - the lazy-tools idea
  (lazygit/lazydocker: the tool does the boring part, the thing to press is on the screen) plus the
  arcade screen an idle machine deserves. It is the surface that carries the **wordmark**: the art
  is `~/.config/lazyos/screensaver.txt`, the script is `~/.config/bin/lazyos-screensaver`, the app
  id `org.lazyos.screensaver`, and its drop-in and launcher entry are named after it. The mark is
  `scripts/make-brand.sh`'s; the terminal gets the text art because that is what a terminal can
  draw. **The frame rate is the whole cost.** Measured on this host with a pty and the real art,
  one complete cycle of the seven effects is **70 s of wall time and 8.5 s of CPU
  (~12 % of one core)**, peaking at **49 MB** in `tte` on top of foot's ~25 MB, and costing nothing
  after it ends; the same cycle at 120 fps takes 20 s and ~6.4 s of CPU, i.e. ~32 % of a core, for
  an animation three times faster than a screensaver needs. `tte` has no global foreground flag,
  so the colour lives in the terminal profile (`~/.config/foot/screensaver.ini`,
  `foreground=2aa198`, the brand's cyan) for the effects that paint in the terminal's foreground,
  and the static frame prints its own SGR.
  Five things are not obvious. **`tte`'s stdout is the animation** - the first version redirected it
  to `/dev/null` while silencing the log and the screen stayed black for the whole run ("the
  screensaver does not work, nothing moves"). **A terminal allocates an 80x24 pty and resizes it only
  after the compositor maps the window**, so the script waits for `stty size` to change before it
  starts (`tte` measures the terminal once, and would paint an 80x24 canvas into the corner).
  **`tte` reads its own stdin**, so the "any key" poll (`read -rsn1 -t 1` in bash, raw mode via
  `stty -icanon -echo min 1`) only worked at effect boundaries until `tte` was started with stdin on
  `/dev/null`; before that a key press was swallowed and the screen survived it. And **the art is
  measured in characters, not bytes** - block and box-drawing glyphs are three bytes each - so
  `center_logo` runs `LC_ALL=C awk` and strips UTF-8 continuation bytes before `length()`, which is
  the one form that behaves the same in mawk (the image) and gawk (a build host); a byte-range regex
  on lifted characters is a *collation error* under a UTF-8 locale, not a width.
  A fifth trap is **the pointer**: a pty hears the mouse only when the program inside asks for it,
  so the dismissal on a mouse move is `\033[?1003h\033[?1006h` (all-motion tracking, SGR encoding -
  foot maps them to `MOUSE_MOTION`/`MOUSE_SGR` in `csi.c` and reports button-0 motion in
  `term_mouse_motion`), printed with the raw-mode setup and turned off (`?1006l ?1003l ?25h`) from an
  `EXIT` trap so `--run` in a real shell does not leave the prompt writing escapes. Without the
  enable the loop saw key bytes only, because motion and clicks are the terminal's own business (a
  left click starts a selection, and a middle click pastes the primary selection into the pty - the
  one pointer event that could end the screen at all): the card's report - "the screensaver does
  not end with mouse movement" - is that line, and `check-system.sh` FAILs when it goes. A
  touchscreen tap counts, because foot maps a tap to a left click. Verified on this machine with a
  throwaway uinput pointer (a device with `REL_X`/`REL_Y` and `BTN_LEFT`, created and destroyed
  around the test): the real idle screen, started by the launcher, ended on the first motion event.
  A sixth trap is the **sway pointer itself**: hiding it while the wordmark is up is done with
  `swaymsg seat - hide_cursor 1`, which is a session-wide seat setting, not a property of the window
  - and sway offers no IPC read for it (`get_seats` reports only name, capabilities, focus and
  devices), so the `EXIT` trap recovers the previous value from the merged config
  (`swaymsg -t get_config`) and puts it back on the way out. Without that restore the seat keeps
  hiding the pointer after foot closes, and `hide_cursor 1` is clamped to **100 ms** (sway raises
  any non-zero timeout under 100 to 100), so the desktop pointer vanishes a tenth of a second after
  every movement - the report this port got ("after the screensaver stops, the mouse pointer
  disappears quickly"). `check-system.sh` FAILs when the capture or the restore line goes.
  **The lock is `~/.config/bin/sway-lock`, and its reason is the readiness line.** The idle timer,
  `Alt+Ctrl+L`, the Lock key and the two suspend shortcuts all call it instead of `swaylock`
  directly: it passes `-R, --ready-fd` (swaylock's own handshake, written once the compositor
  guarantees nothing sensitive is visible, and written *before* `-f` daemonizes) through a FIFO in
  `$XDG_RUNTIME_DIR`, so `before-sleep` waits 4 s (inside logind's 5 s inhibitor budget) instead of
  returning while the lock is still drawing; the ordinary call waits 2 s. It refuses to stack a
  second surface when `pgrep -x swaylock` already answers, **kills the idle screen first** (a lock
  put up with the shortcut at 100 s left the 150 s timer free to start the animation underneath it,
  where it burned CPU invisibly until the 300 s timer) and puts the session's
  own wallpaper (`~/Pictures/Wallpapers/mountains.jpg`, `-s fill`) on base03 instead of a flat
  colour; if swaylock does not report back and is not running at all, it raises a critical
  `dialog-error` notification, because a lock that failed quietly is the one failure a person must
  hear. `check-system.sh` FAILs on a missing `tte`/`swayidle`/art/launcher/`sway-lock`/terminal
  profile, reads the drop-in for the 150 s, 300 s, 360 s, the `resume` and `after-resume` wake
  lines, `before-sleep` and `after-resume` **and** that `swayidle` is running (a missing chain
  means no lock at all), WARNs when an idle screen is found running under `swaylock`, and FAILs
  when the pointer enable or the launcher's early return goes (see below).
  **`swayidle -w` waits for the command it runs, so the 150 s command must return at once.** `-w` is
  load-bearing for sleep: swayidle holds logind's delay inhibitor for the whole session and releases
  it in `prepare_for_sleep` *after* `before-sleep` returns, which is how the lock is up before the
  machine goes down (`sway-lock --wait` is the bounded wait that makes that true), and without `-w`
  the release happens as soon as the command is forked - the desktop visible on the way down, the bug
  the readiness line was built for. But it waits in the event loop itself (`main.c`, `cmd_exec` ->
  `waitpid`), so a 150 s command that stayed in the foreground held the 300 s lock, the 360 s DPMS,
  the `resume` lines and `before-sleep` for as long as the wordmark animated: a session left alone
  never locked and the panel never went dark, and a lid close slept after logind's 5 s inhibitor
  timeout with no lock in front of it. Measured here on a headless sway: with `-w` a
  `timeout 1 'sleep 100'` kept a 2 s timeout from firing within 5 s, and without `-w` it fired at
  2 s. `lazyos-screensaver` therefore starts its foot and exits (`foot ... </dev/null &`), and
  `check-system.sh` runs the launcher against a stub `foot` and FAILs when it does not return.
  **The lock itself is pointer-blind and that is not a bug**: `swaylock`'s `seat.c` answers motion,
  buttons and the wheel with `// Who cares`, so typing is the unlock and a mouse move at five
  minutes only turns the panel back on.
  Deboma's launcher (`hyprctl monitors`, a special workspace per
  monitor, its socket2 event stream) has no sway equivalent worth building: there is one output,
  and the drop-in fullscreens `[app_id="org.lazyos.screensaver"]` on the current workspace.
  Deboma's `ttfx` shim exists only because Omarchy's scripts call the Rust port by that name; this
  script calls Debian's `tte` directly, so no shim is shipped.
- **A film survives the idle chain, and the bar keeps a switch for everything else.**
  `config.d/32-idle-inhibit.conf` is two `for_window` criteria plus one: a **fullscreen** Firefox
  window, a **fullscreen** `papers` window and a **visible** mpv window each get
  `inhibit_idle`. The Firefox and papers rules exist because both ask
  `org.freedesktop.portal.Inhibit`, and this image runs no portal backend for that interface
  (`busctl --user` lists only the wlr impl, which is Screenshot/ScreenCast); mpv needs nothing
  (`--stop-screensaver` defaults to yes and it holds `zwp_idle_inhibitor_v1` itself), and the rule
  only makes that independent of a profile. `fullscreen` is the deliberate scope - a browser merely
  open must not hold the machine awake all day. The manual switch is waybar's own
  **`idle_inhibitor`** module, the coffee cup between the backlight and the battery: it holds one
  `zwp_idle_inhibitor_v1` on the bar's surface, so the session gains **no helper process and no
  script**, the cup is always visible (dim = the screen may sleep, yellow = it may not) because a
  control has to be visible to be turned on, and `start-activated` stays false so a login never
  inherits it. Sway's own `inhibit_idle` is not it: it has no `toggle` (only
  `focus|fullscreen|open|none|visible`) and its user inhibitors belong to a *view*, which would tie
  "keep awake" to a window; for the same reason there is no root-menu row - a menu cannot share
  waybar's inhibitor without a second mechanism. `check-system.sh` FAILs on a missing rule and
  WARNs on a missing module or its `.activated` style; the manual paragraphs are `03-shortcuts.md`
  § *Panels, lock and power* and § *What was copied from Omarchy*, `04-software.md` § *Playing a
  video*, and `06-troubleshooting.md` § *The screen turns off while you are watching something*.
- **The volume and brightness keys draw an overlay without adding a program.**
  `~/.config/bin/osd` is one POSIX sh script behind four bindings in `config.d/20-brightness.conf`
  and `21-volume.conf`: it runs the same `pactl`/`brightnessctl` step the keys used to run and then
  a `notify-send` card, so the session gains no daemon, no package and no process after the key. It
  is a mako notification with two hints: `x-canonical-private-synchronous:lazyos-osd` makes a held
  key move one card instead of stacking twenty (mako groups by that tag, the same trick `busy`
  uses), and `int:value:N` is what makes mako draw the progress bar (`mako(5)`: "Progress can be
  indicated ... by setting a hint, 'value' to an integer between 0 and 100"; `~/.config/mako/config`
  already had `progress-color=over #2aa198ff`). The level picks the icon: Font Awesome's
  volume-off/down/up (`f026`/`f027`/`f028`) rendered by `scripts/make-icons.sh` as
  **`audio-volume-muted`/`-low`/`-high`**, plus the theme's existing `display-brightness` (`f185`);
  `< 50 %` is the low glyph, muted ignores the level. Volume over 100 % (pactl allows it) is
  clamped to the bar's 100. The overlay is a *card*, not `swayosd` - that one is packaged in forky
  (0.3.0-4) but is a resident GTK4 server for the same picture. **The brightness delta carries its
  sign after the number** (`5%-`, the form `brightnessctl --help` lists), never in front:
  `brightnessctl` parses its own argv with getopt, which reads `-5%` as an option
  (`invalid option -- '5'`) and exits 1, so `osd brightness down` changed nothing and raised
  *could not change the backlight* while `up` worked - one direction, and the helper reads
  perfectly. `pactl` has no such trap (`set-sink-volume ... -5%` is fine), and the bar's own
  `on-scroll-up/down` in `config.jsonc` already wrote the brightness pair in the suffix form.
  `check-system.sh` FAILs on the helper, `notify-send`, either binding or any of the four icons,
  FAILs when the helper hands brightnessctl a step that starts with a hyphen (it runs it against a
  stub, so no backlight is touched) and WARNs when mako lost `icons=1` or `progress-color`
  (text-only card); the manual is `03-shortcuts.md` § *Sound, light and screenshots* and
  `06-troubleshooting.md` § *The volume or brightness keys change the value but show nothing* and
  § *The brightness keys do nothing*.
- **The CLI tools are Deboma's, in `packages/tools.list`** (a third list next to base and the UI
  one, installed by `rootfs-setup.sh` between the two). It is deliberately *only* tools: everything
  Deboma installs that is a service, a daemon or a timer stays out, which is why `plocate` is
  absent (it is `locate` **plus** `plocate-updatedb.timer`, a systemd timer that indexes the disk),
  and why the editor/keybindings tooling (`tree-sitter-cli`, `lua5.4`), Omarchy's typing helper
  (`wtype`) and `gawk` (Deboma needs awk's `RT` in its theme renderer; the theme here is a static
  file) are absent. Two Debian renames are the whole trap: **`bat` installs `batcat`** and
  **`fd-find` installs `fdfind`**, so `rootfs-setup.sh` adds `/usr/local/bin/bat` and
  `/usr/local/bin/fd` symlinks (guarded, so a tree without the list still builds) because the
  aliases, `MANPAGER` and `FZF_DEFAULT_COMMAND` call them by their upstream names. Which tool
  gets the list is where the *Icons first, never forced* rule starts: one that draws and reads
  well in foot (`eza --icons`, `bat`, `glow`, `btop`, `lazygit`) wins over a plainer one that
  prints the same facts, but nothing is added for looks alone and nothing resident (the reason
  `plocate` stays out is its timer, not its output). Three kinds of gap were closed at once:
  **`ip`, `ping` and `ss`** are `iproute2` + `iputils-ping` in `packages/base.list` (base, not
  tools: Debian's base system no longer pulls them in and the first troubleshooting question, "is
  the network up", answered `command not found`, which reads as a broken system); **`zip`** is the
  writing half of `unzip`; **`strace`** is the debugger a device with no gdb reaches for; **`ncdu`**
  is the `du` walk a 58 GB eMMC needs when it fills up. `check-system.sh` FAILs on a missing
  `ip`/`ping`/`ss`.
- **The editors are `nano` (in `packages/base.list`, the one that is always there) plus
  `neovim` and `vim` (in `packages/tools.list`)**, and `EDITOR`/`VISUAL` name `nvim` - guarded on
  the binary, because an `EDITOR` pointing at a program that is not installed makes `git commit`,
  `crontab -e` and `eff` fail instead of falling back to nano. **`vim` alone provides
  `/usr/bin/vi`**: its postinst runs `update-alternatives --install /usr/bin/vi vi
  /usr/bin/vim.basic 30` (it also registers `editor`, `ex`, `view` and `vimdiff` at the same
  priority), so `vim-tiny` is not needed - and `check-system.sh` asks for `vi` by name, because a
  `vim` whose postinst never ran leaves `vi` missing while `vim` works, which only the script
  that calls `vi` notices. `neovim` brings its own LuaJIT and tree-sitter library, which is why
  the standalone `tree-sitter-cli`/`lua5.4` that Deboma installs stay out.
- **The shell wiring is one file plus a symlink**: `uis/sway/files/etc/profile.d/90-cli-tools.sh`
  holds the environment (`MANPAGER='sh -c "col -bx | bat -l man -p"'` - `bsdextrautils` supplies
  `col`, `man-db` supplies `man` itself, which base.list does not pull in - `BAT_THEME=ansi` so bat
  paints with the terminal's own 16 colours, i.e. the solarized palette, and `$HOME/.local/bin`
  appended to `PATH`; plus `EDITOR`/`VISUAL` naming `nvim`, see the editors bullet), and it is
  sourced from `uis/sway/files/home/.bashrc`, which replaces Debian's
  skel file and defines everything else: `ls`/`lsa`/`lt`/`lta` as `eza` (`--icons=auto`), `cat` as
  `bat --style=plain --paging=never`, a `cd` wrapper that tries the real path first and only then
  `z`, `zd`/`ff`/`eff`, the fzf **key bindings only** (its `completion.bash` rewraps ~76 commands'
  completion on every shell start, ~25 ms before the prompt), and `starship init bash`. Every alias
  is guarded with `command -v eza`-style tests so the file is harmless without the packages. **The
  terminal needs the font fallback spelled out**: foot does not use fontconfig fallback for missing
  glyphs, so `~/.config/foot/foot.ini`'s `font=` lists `Symbols Nerd Font Mono` after JetBrains
  Mono - `fonts-nerd-symbols` provides it - or eza's icons, fastfetch and the starship git glyphs
  (U+F00C, U+EA71, U+EBAB, codicons) would all be boxes. waybar gets away without that because it
  is Pango, and a terminal is not. `check-system.sh` fails on each missing binary, on the two
  wiring files, and warns when an interactive `bash -ic 'alias ls'` does not answer `eza`.
- **kitty is the second terminal, and its configuration is the session's theme rather than a second
  theme.** Nothing starts it (`$term`, the bar's popups and every menu that needs a terminal open
  **foot**) and no package list installs it, but
  `uis/sway/files/home/.config/kitty/kitty.conf` ships with the image so that `apt install kitty`
  does not put a default-palette terminal beside a solarized one. Every line is what `foot.ini`
  already decided: solarized dark (the same 16 values), JetBrains Mono 11 with
  `symbol_map U+E000-U+F8FF` and plane 15 to **Symbols Nerd Font Mono** - kitty's fontconfig
  fallback would find the *non*-Mono variant, whose glyph is two cells wide, and every
  `eza --icons` row would stop lining up - 6 px of padding, 1000 lines of scrollback, no bell and
  `background_opacity 0.87`. Four things are deliberate and easy to undo by mistake:
  **`background_blur` stays 0**, because kitty only blurs when the compositor speaks
  `org_kde_kwin_blur` or `ext_background_effect_v1` and swayfx advertises neither
  (`WAYLAND_DEBUG=1` lists `wl_compositor`, `xdg_wm_base` and the KDE decoration manager, nothing
  about blur) - the blur under that window is swayfx's, behind the alpha; **`hide_window_decorations
  yes`**, because swayfx is the frame (2 px, cyan when focused) and kitty would paint its own title
  bar over it; **`confirm_os_window_close 0`**, because `Alt+Q` is sway's `kill`, a close request
  that cannot answer a dialog; and **`kitty_mod` stays `ctrl+shift`**, because sway sees every Alt
  chord before the window does, so a kitty on Alt would lose its keys to the session (which is also
  why prompt jumping is `Ctrl+Shift+Z`/`X`, not the obvious left/right). Ligatures are the one
  rendering difference from foot - kitty draws JetBrains Mono's `calt`, foot cannot - and nothing
  else about the window is. `check-system.sh` asks kitty's own parser what the file *resolves to*
  (`kitty +runpy` with `kitty.config.load_config`: ~0.1 s, no window, writes nothing, and the
  `--debug-config` of older releases is gone in 0.48), because an unknown key is one stderr line and
  a value that does not parse keeps the default in silence; `font_family` is a `FontSpec` namedtuple,
  so the family is its `.system`. SKIP when kitty is not installed, and the manual section is
  `04-software.md` § *kitty, if you install it*.
- **The prompt is the powerline one, and its glyphs are the reason it looks the same in foot and in
  kitty.** `uis/sway/files/home/.config/starship.toml` is the maintainer's Deboma config recoloured
  to the session's palette (the role names are upstream's, the values are solarized's: base3 on the
  saturated segments, base02/base01 for time and the containers, cyan for git, blue for languages,
  orange for the user), and it is the distro's prompt rather than the earlier four-line one because
  directory-and-git alone is what the bar already says. Three decisions are load-bearing. **There is
  no `$os` segment**: it asked for Nerd Fonts v3 `nf-md` glyphs (Linux `U+F033D`, Debian `U+F08DA`,
  Arch `U+F08C7`, …), which live in plane 15; `fonts-nerd-symbols` is DFSG-stripped (Octicons,
  Codicons, Devicons, Seti, Powerline) and `fonts-materialdesignicons-legacy` carries the *older*
  MDI numbering, so 12 of the 14 codepoints the segment could show have no font at all - measured
  with `fc-list :charset=<cp>`, which is also what the check asks. **The separators are the triangles
  `U+E0B0`/`U+E0B2`, not the rounded caps `U+E0B6`/`U+E0B4`**: JetBrains Mono draws the triangles
  itself, so they tile with the cell at any size, while the caps exist only in the symbol font, which
  foot draws at *that font's* size and baseline - on the same line, kitty's caps reached the segment
  and foot's stopped short (both terminals were screenshotted before the choice, and the mismatch is
  what "kitty looks good, foot does not" was). **Docker and Java use `U+E7B0` and `U+E624`** because
  `U+F308` and `U+E256` are in no font here. The other half of the same bug is `foot.ini`'s `font=`
  chain, which is now `JetBrains Mono` followed by `Symbols Nerd Font Mono`, `Font Awesome 6 Free
  Solid` and `Material Design Icons version 6`, each icon font **one point larger than the text**:
  foot searches the listed fonts in order and draws the first that has the glyph *at that font's
  size*, so a stripped-set glyph at 11 pt sat a fifth short of the cell; the old comment claiming
  foot has no fontconfig fallback at all was wrong (foot.ini(5): *"the dynamic fallback list from
  fontconfig … is searched"*) and was what hid the sizing half. `check-system.sh` reads the prompt,
  asks fontconfig which family draws each private-use codepoint and FAILs when the answer is none
  (the box that logs nothing) and WARNs when the only families that have it are not in foot's chain
  (the glyph renders small from the fallback), naming the codepoint; the manual section is
  `04-software.md` § *The prompt*.
- **`/etc/yt-dlp.conf` names Node as yt-dlp's JS runtime**, for the same reason node is installed at
  all: yt-dlp 2026 needs a JS runtime for YouTube's JS challenges ("EJS"), enables only `deno` by
  default and Debian ships none, so a stock install logs `[debug] JS runtimes: none` and warns that
  *some formats may be missing*. `--js-runtimes node` in the system config takes the debug line to
  `JS runtimes: node-24.21.0` (and `[jsc:node] Solving JS challenges using node`). The file lives in
  the UI overlay only because that is the etc/ tree the build stages; it configures a
  `packages/tools.list` tool, not the session.

### ChromeOS GPT (`cgpt`)

- **`depthcharge-tools.service` must keep running** (`depthchargectl bless`, 0.66 s of boot).
  ChromeOS kernel partitions carry `tries`/`successful` attributes (`-S 1 -T 5 -P 10` in
  `lib-cgpt.sh`) and the firmware spends one try per boot until something marks the partition
  successful. Masking that unit - it looks like a leftover from the package - would leave the
  kernel partition unbootable after five boots, which reads as a firmware problem. The image ships
  it enabled by the package (`basic.target.wants/`).
- **Do not pass `-z` to `cgpt create`**: it zeroes the GPT areas *after* writing them, so the
  following `cgpt add` fails with `GptSanityCheck() returned 2: GPT_ERROR_INVALID_HEADERS`.
- **`cgpt` does not write a protective MBR**, and the Linux kernel refuses to parse a GPT without
  one (`0xAA55` signature plus a `0xEE` entry). Run `parted -s <dev> mktable gpt` first — this is
  why pmbootstrap does it too — then `cgpt create` / `cgpt add` on top of it.
- After writing the table, refresh the kernel's view with `partx -d` + `partx -a`
  (`blockdev --rereadpt` returns `EBUSY` on this microSD card).
- **`udisks2` mounts the partitions `write-sd.sh` has just created**, and that is what the
  confusing failure looks like: the GPT is rewritten, `partx -a` presents p2/p3, the session's
  udisks2 auto-mounts both within a second, and `mkfs.ext4` stops the run with
  `/dev/mmcblk1p3 is mounted; will not make a filesystem here!` — after the table was already
  replaced, leaving a half-written card. It can fail one step earlier, which is worse: with p2
  mounted `parted`/`cgpt` answer *Partition(s) 2 ... have been written, but we have been unable to
  inform the kernel of the change, probably because it/they are in use*, the kernel keeps the old
  table and the run stops before anything is formatted (hit on this machine). **`systemctl mask
  --runtime udisks2` is not enough**: it does not stop a daemon that is already running, and that
  is the one that mounts the card. `sudo systemctl stop udisks2` is, and it can be started again
  afterwards (or left to the next boot). Both entry points now refuse rather than half-write:
  `disk_release()` before the table exists, `disk_layout_mount()` after `partx -a`, each naming the
  command. Seen while writing this build on the machine itself, where the port's own session is
  the thing mounting the card.
- `lsblk -nro PATH <disk>` lists the **disk first**, then its partitions: skip the first line when
  resolving `p1`/`p2`/`p3`. A wrong index here formats the whole disk instead of a partition.
- The tools needed **on the device** are three separate Debian packages and it is easy to install
  the wrong one: `cgpt` provides `cgpt`, **`vboot-kernel-utils`** provides `futility` (the
  similarly named `vboot-utils` ships `crossystem` and symlinks to `futility` but not `futility`
  itself), and `depthcharge-tools` provides `depthchargectl`/`mkdepthcharge`. All three are in
  `devices/google-asurada/packages.list`, together with `rsync`, which `install-emmc.sh` needs.
- **`depthcharge-tools`' system hooks are disabled in the image** by
  `/etc/depthcharge-tools/config` (`[depthchargectl/write] enable-system-hooks = False`, written by
  `rootfs-setup.sh`). The package ships `/etc/kernel/postinst.d/zz-depthcharge-tools` and
  `/etc/initramfs/post-update.d/depthcharge-tools`, which build a depthcharge image and write it to
  a kernel partition on every kernel or initramfs change. They know nothing about this port (the
  board `mt8192` and the command line come from our `deviceinfo`, applied by `make-kpart.sh`, and
  the write is `install-kpart.sh`), and enabled they also break the **build**: `update-initramfs`
  runs in the chroot, where `/proc/cmdline` is the host's — Nura's, which contains `cros_secure` —
  so the hook thinks it is on a depthcharge system, finds no kernel partition and exits 1, and
  `run-parts` then fails `install-kernel.sh`. The `enable-system-hooks` check reads section
  `depthchargectl/write`, not the `[depthcharge-tools]` section the package's own `config.ini` uses.
- A kernel that boots from a ChromeOS kernel partition lives **inside** that partition, so
  installing a new kernel is not finished until p1 is rewritten: `scripts/install-kpart.sh` does
  that (the Debian counterpart of boot-deploy's `flash_updated_depthcharge_kernel`, which reads
  `kern_guid` from `/proc/cmdline`, resolves `findfs PARTUUID=<guid>`, checks the image fits the
  partition and `dd`s it). On the installed system the sequence is
  `scripts/install-kernel.sh /` (staged kernel into the running rootfs) followed by
  `scripts/install-kpart.sh`; without the second step the machine keeps booting the old kernel
  while `/boot` and `/lib/modules` move on.
  Its chroot guard must keep `stat -L` **and** the comparison in that sense: without `-L`, `stat`
  reports the procfs symlink itself (device `0:28`), which can never equal `/`, so the guard passed
  silently; and *equal* is the state of a real system (this root is PID 1's root), so a chroot is when
  the two **differ**. As it was written the guard never fired, and a chroot would have rewritten the
  *host's* kernel partition from the host's `kern_guid`. Both sides are now read before refusing, so a
  system where `/proc` hides PID 1 is not mistaken for a chroot.

### pmbootstrap parity (the install method that is known to boot here)

`scripts/lib-cgpt.sh` reproduces pmbootstrap's ChromeOS install path part by part, because that
method is the one proven on this Chromebook. Read the upstream before changing it:
`pmb/install/partition.py` (`partition_cgpt`, `sanity_check_boot_size`), `pmb/install/format.py`
(`format_partition_with_filesystem`, `format_and_mount_*`), `pmb/install/_install.py`
(`create_fstab`, `copy_files_from_chroot`, `write_cgpt_kpart`, `partprobe`), `pmb/install/blockdevice.py`
(`mount_disk`) and `boot-deploy-functions.sh` (`create_depthcharge_kernel_image`,
`flash_updated_depthcharge_kernel`, `is_in_chroot`, `check_image_size`). Verified on a loop device:
the resulting geometry is identical (`p1 8192+262144`, `p2 270336+1048576`, `p3 1318912+rest-33`,
PMBR present, kernel/efi/data types, `pmOS_*` labels, `-S 1 -T 5 -P 10`).

- The **root partition size is `dev_size - root_start - 33`**, not `- 2048`: those 33 sectors are
  the secondary GPT (32) plus its header, and pmbootstrap sizes the partition so the backup table
  survives. `partprobe` runs after the `cgpt add`s, as in `install_system_image()`.
- **`mkfs.ext2 -F -q -L pmOS_boot` and `mkfs.ext4 -F -q -L pmOS_root`**, the lines
  `format_partition_with_filesystem()` builds for this device (its schema default for
  `boot_filesystem` is ext2, so `BOOT_FS=ext2` is compliance, not a quirk).
- **`/etc/fstab` options are pmbootstrap's**: `defaults` for the root and `nodev,nosuid,noexec` for
  `/boot` — which is also how the running Nura mounts its `/boot`. Do not "improve" them without a
  reason; the `pmos_rootfsopts`-equivalent a pmOS user would see comes from that column.
- **`disk_release` refuses when a partition of the target is mounted**, like `mount_disk()`, rather
  than unmounting it silently. Active swap is turned off (pmbootstrap does not check it, but `mkfs`
  on a live swap partition fails) and no `wipefs` is done: parted writes both GPT tables and
  `mkfs -F` overwrites signatures anyway.
- **`mountpoint -q <device>` does not test whether a device is mounted** — it answers whether that
  *path* is a mount point, which a device node never is. It answered "no" for a mounted partition, so
  the old release step never unmounted anything and the new refusal would never have fired. Test with
  `findmnt -rn -S <device>` (or `/proc/mounts`), never with `mountpoint`, when the argument is a
  device; `mountpoint -q <directory>` (as in `disk_layout_umount`) is fine.
- **Partitions are resolved by number, never by their position in `lsblk`.** `lsblk` lists a disk's
  partitions in the order the kernel handed out their minors, and the `partx -d`/`-a` round that
  `disk_layout_create()` runs to make the kernel re-read the table it was just given reorders them.
  Measured over 16 attaches of a 2048 MiB loop: 3 came back as `3 1 2` or `2 1 3` with no line
  missing, so the positional code would format `p2` as the root filesystem and write the kpart into
  `p3` — a card written wrong a fifth of the time, silently. It is not `partprobe`: in an A/B of 8
  attaches each, the order drifted once with it and twice without it. `disk_layout_parts()` uses
  `PATH,PARTN` and asserts the three `pmOS_*` labels, so a wrong resolution dies instead of touching
  the wrong partition: 16 of 16 attach correctly, including the two that came back reordered.

Intentional deviations, all of them at the line that makes them: the GPT areas are zeroed before
`parted` (pmbootstrap leaves it to parted; it costs nothing and makes `cgpt create` immune to a stale
header), `mkfs` does not get `-i 8192` (that is for pmbootstrap's image builds; our images are 4 GiB
with ~90k files), the fstab pass column is `0 1`/`0 2` instead of `0 0` (systemd-fsck orders them),
the kernel command line carries `root=UUID=... rootwait rw` instead of pmOS's `pmos_root_uuid=...`
(Debian resolves its root from `root=`), and `install-emmc.sh` copies `/home` with the rest of the
running system instead of seeding it from `/etc/skel`, because the point of that script is to clone
the card that has already been tested.

### depthcharge FIT

- Build with `deviceinfo_depthcharge_board="mt8192"` (not `asurada`: the hayato DTB has no
  `google,asurada` compatible). `mkdepthcharge` prepends `kern_guid=%U` by itself, so the cmdline
  passed to `depthchargectl` must not add it again.
- Verify with `futility show <kpart>`: expect a dev-signed key block, preamble 2.2 and
  `Body verification succeeded`.
- The FIT carries one config per DTB. With `--board mt8192` no config matches by `compatible`, so
  depthcharge falls back to the **default config, which is the first DTB**: hayato must stay
  first (the alphabetical order of `deviceinfo_dtb="mediatek/mt8192-asurada*"` does that).
- The output is silent by design (`quiet loglevel=2`). For a verbose boot without touching
  `kernel-cmdline.conf`:
  `KERNEL_CMDLINE_OVERRIDE="console=tty0 ignore_loglevel loglevel=7" scripts/make-kpart.sh`
  and write the resulting kpart to the kernel partition (or `scripts/install-kpart.sh` on the
  running system). Expect the `sbs-8-000b` gauge spam on screen while verbose. Never append a
  bare `debug`: initramfs-tools then redirects its own script output to
  `/run/initramfs/initramfs.debug` (a file *inside* the initramfs) instead of the console, so the
  messages that identify a boot failure (`run-init: ...`, `Target filesystem doesn't have ...`,
  `ALERT! ...`) never reach the screen. `fbcon` is built into this kernel, which is what makes
  `console=tty0` useful here.

## Commands

All scripts live in `scripts/` and read shared config from `scripts/lib.sh`.
Large artifacts go to `$HOME/.cache/debian-aarch64` (`WORK_DIR`, `SRC_DIR`).

```sh
scripts/00-deps.sh          # install host build deps (apk or apt), re-execs via sudo
scripts/build-kernel.sh     # mainline 7.2.7 + patches + config  -> $WORK_DIR/kernel-stage
scripts/build-swayfx.sh     # scenefx + swayfx from source     -> $WORK_DIR/swayfx-stage
scripts/build-workstyle.sh  # workstyle from its pinned crate  -> $WORK_DIR/workstyle-stage
scripts/build-rootfs.sh     # debootstrap Debian forky rootfs    -> $WORK_DIR/rootfs
scripts/install-kernel.sh   # copy staged kernel into the rootfs, depmod, initramfs
scripts/install-swayfx.sh   # install the compositor .debs (rootfs by default, `/` for a running system)
scripts/install-workstyle.sh # install the workspace renamer .deb (rootfs by default, `/` for a running system)
scripts/make-kpart.sh       # signed depthcharge FIT kpart       -> $WORK_DIR/vmlinuz.kpart
scripts/make-icons.sh       # Font Awesome glyphs -> the LazyOS icon theme PNGs (host tool, committed output)
scripts/make-brand.sh       # brand/lazyos.svg -> the icon, the SVG and the wordmark (host tool, committed output)
scripts/make-emojis.sh      # Python's Unicode database -> ~/.config/emojis.tsv (host tool, committed output)
scripts/write-sd.sh [dev]   # partition+populate a microSD/USB (default /dev/mmcblk1)
scripts/make-image.sh       # distributable disk image from the rootfs (no card needed)
scripts/make-release.sh     # every release asset + SHA256SUMS -> $WORK_DIR/release-<device>
scripts/install-emmc.sh     # install the running system onto /dev/mmcblk0 (CONFIRM=yes)
scripts/install-kpart.sh    # rewrite the kernel partition this system booted from
scripts/backup-system.sh    # kernel partition + GPT + /boot + rootfs tars into a directory
scripts/restore-system.sh   # put such a backup back onto a disk (root only, from the live SD)
scripts/wifi.sh export      # saved NetworkManager Wi-Fi profiles -> $WIFI_DIR
scripts/wifi.sh import      # $WIFI_DIR -> a rootfs (drops interface-name=)
scripts/check-system.sh     # (on the booted image) hardware + configuration report

scripts/build.sh            # full pipeline (kernel + compositor + workstyle + rootfs + kernel + compositor + workstyle + kpart)
```

`scripts/lib-cgpt.sh` holds the one copy of the ChromeOS GPT layout, used by `write-sd.sh`,
`install-emmc.sh`, `install-kpart.sh`, `make-image.sh` and `make-release.sh`; `BOOT_SIZE_MIB` (512)
and `BOOT_FS` (ext2) override its boot partition.

`scripts/make-image.sh` reuses that layout on a loop device, so an image is built exactly like a
card is written, with one difference: its size is an argument (`IMAGE_SIZE_MIB`, default 4096)
instead of the device's, and the copy it publishes is sanitized. `scripts/lib-dist.sh` is the only
place that decides what a published artifact may contain, and `dist_assert_clean` refuses the
artifact while anything is left; the release rule is:

- **Never publish an artifact straight out of `$ROOTFS`.** The build rootfs carries the
  maintainer's Wi-Fi profiles (PSKs in clear, imported from `$WIFI_DIR`), the SSH host keys and
  the machine id. `make-image.sh` and `make-release.sh` sanitize a copy and assert it clean;
  anything that publishes a rootfs has to go through `lib-dist.sh` too.
- The released image therefore comes up with no known Wi-Fi network, `ssh` host keys are generated
  on first boot (`sshd-keygen.service`) and `systemd` gets a fresh `machine-id`.
- A release is: `scripts/make-release.sh`, notes in `docs/releases/<version>.md` (committed, and
  reused as the annotated tag message and the GitHub release body), and the assets attached to the
  tag. `docs/releases/v0.1.0.md` is the first one and shows the shape.

`scripts/backup-system.sh` is the counterpart of `install-emmc.sh`: it writes the kernel partition
the system booted from (resolved from `kern_guid`, like `install-kpart.sh`), the GPT, `/boot` and
the root filesystem into a directory, the filesystems as gzip tars with owners, ACLs and xattrs
preserved, plus a `README.md` with the restore steps and a `SHA256SUMS`. The destination is
normally a removable disk with **exFAT**, the one filesystem both macOS and Linux read; exFAT
cannot hold a Linux root filesystem by itself, which is why the trees travel as tars. Caches,
build directories and the agent's session data are excluded by pattern (`EXCLUDES`), and nothing
in the backup needs them: they are downloaded or rebuilt again.

`scripts/restore-system.sh <backup-directory> [disk]` is its other half, and the one script that
insists on being run as root: it does not re-exec through sudo, because silently escalating to
rewrite a disk is the wrong default. It refuses to run from the disk it would overwrite (that is
the "live SD" requirement), refuses a disk of a different size than the backup came from (the raw
GPT dumps are exact byte copies, and the same eMMC is the case that matters), checks SHA256SUMS
before writing anything, and takes the filesystem UUIDs from the archive's own `/etc/fstab`, so the
restored system boots with the fstab and kernel command line it had. `CONFIRM=yes` is required;
`VERIFY=no` skips the checksums and `DEEP=yes` adds a full `tar -d` comparison afterwards.

`build-rootfs.sh` installs `$WIFI_DIR` into the image when that directory exists, so the machine
comes up already knowing the Wi-Fi networks (see `scripts/wifi.sh`).

Its rootfs inputs are environment variables: `TARGET_HOSTNAME`, `USERNAME`, `USERPASS`,
`LOCALE` (default `en_US.UTF-8`) and `TIMEZONE` (default `America/Mexico_City`, e.g.
`TIMEZONE=UTC`). The script re-execs itself through `sudo`, which does not preserve
the environment, so it forwards every resolved input explicitly — anything not forwarded is
silently replaced by its default. A locale that is not an entry of `/etc/locale.gen`, or a
timezone that is not under `/usr/share/zoneinfo`, is reported and replaced by the default.

`write-sd.sh` and `install-emmc.sh` are destructive to block devices and refuse to touch the
internal eMMC from `write-sd.sh` or the running root from either. Both build the same three
partitions (see `scripts/lib-cgpt.sh`) and both write a signed kpart into p1;
`install-emmc.sh` copies the **running** system (rsync) instead of `$ROOTFS`, which is why it
needs the card to be a system worth cloning. That copy excludes the *contents* of `/proc`,
`/sys`, `/dev`, `/run`, `/mnt`, `/media`, `/tmp`, `/var/cache` and `/var/log/journal` — but
`rsync` also drops the excluded directories themselves, and a root filesystem needs them: they
are mount points, and the initramfs's `run-init` moves its own `devtmpfs` `/dev` (plus `/proc`
and `/sys`) into them before handing over to systemd. A copy without them boots the kernel,
mounts the root filesystem read-write and then stops at `run-init: opening console: No such file
or directory` / `Target filesystem doesn't have requested /sbin/init` / `No init found. Try
passing init= bootarg.`; with `quiet` on the command line that is a black screen, and since the
kpart and its payload are byte-identical to the card's it reads as a firmware or kernel problem.
The script now recreates `dev:755 proc:555 sys:555 run:755 mnt:755 media:755` after the copy and
refuses to write p1 unless `/dev` exists and `/sbin/init` resolves.

`install-kpart.sh` is the third one: it does not partition anything, it only rewrites the kernel
partition the running system was booted from (resolved from `kern_guid` in `/proc/cmdline`), so
that a kernel update does not leave p1 behind. It backs up what it replaces to
`$WORK_DIR/kpart-backup-<timestamp>.img` and requires `CONFIRM=yes`.

`check-system.sh` is their counterpart: it runs **on the booted image**, not on the build host, so
it sources nothing and changes nothing, and the image also ships it as
`/usr/local/sbin/check-system.sh` so no checkout is needed to run it. With a checkout it also
verifies that every package declared in `packages/*.list`, `devices/*/packages.list` and
`uis/*/packages.list` is installed (`check-system.sh ~/Dev/<repo>`). Run it as the desktop user;
from a root shell it re-executes
itself as that user, and the privileged reads go through `sudo -n`. Every check maps to a bug this
port has already hit — keyd swallowing `KEY_SLEEP`, greetd's `vt`/user, the UCM boot sequence, ALSA's
missing `default` PCM, the backlight udev rule, the font fallback, `interface-name=` in the Wi-Fi
profiles, the clock, the `fstab` UUID, the kernel partition holding a signed FIT — so a FAIL there is
a regression, not a hypothetical. It
exits non-zero when something FAILs.

### Boot from microSD

1. `crossystem dev_boot_usb=1 dev_boot_signed_only=0` (from ChromeOS developer mode; already set
   on this machine).
2. `scripts/write-sd.sh /dev/mmcblk1`
3. Insert the card, power on and pick **external disk** in the firmware menu.

### The firmware menu (this machine)

In developer mode the stock firmware on this Chromebook does not wait for the Ctrl+D / Ctrl+U
screens the older documentation describes: it presents a **menu** on its own. Its entries were
verified on hardware: **internal disk = the eMMC, external disk = the microSD**. That is the
mechanism to boot the eMMC after `install-emmc.sh`; the microSD stays the rescue system (the card
is untouched by an eMMC install, so it can always be selected again).

Default credentials of the built image: user `brunux` / password `debian`, passwordless sudo.

