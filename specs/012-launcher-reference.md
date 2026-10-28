# Spec 012 — Launcher, window switcher, run and the reference surfaces

- **Status**: draft
- **Phase**: 4
- **Depends on**: 001-007, 009
- **Model**: `claude-sonnet-5-5` (effort `high`) — `.desktop` parsing, sway IPC, generated keys
  and four reference surfaces over a long context (every current model has 1M tokens at one
  price, so context no longer picks the model). Fallback `claude-opus-5-5`.
- **Branch**: `spec/012-launcher-reference`
- **Plan**: [`docs/plan-ui.md`](../docs/plan-ui.md) §5.2 (launcher), §5.12 (reference), §16
  phase 4
- **Old paths**: `rofi -show drun`, `rofi -show window`, `rofi -show run`, `rofi-keys`,
  `rofi-learn`, `rofi-about`, `rofi-color`, `rofi-pkg`, `rofi-dev`

## Problem

The launcher, the window switcher and the run prompt are three rofi modes with a foreign look,
a ranking nobody can test and `-show-icons` decoding through gdk-pixbuf at open time. The
reference menus are separate scripts: `rofi-keys` parses the sway config with awk, `rofi-learn`
opens glow in a terminal, and the About screens are fastfetch in a foot window.

## Scope

**In (`lazyos menu apps|window|run|keys|learn|about|color|pkg|dev`):**

- **Apps**: `.desktop` parsing (`Exec`, `Name`, `Icon`, `Terminal`, `NoDisplay`,
  field codes `%f/%F/%u/%U`), sixel/PNG icons resolved through the LazyOS theme, frecency
  shared with the palette (spec 007), `Terminal=true` → foot.
- **Window**: the wlroots foreign-toplevel list (or sway IPC), filter by title/app, Enter
  focuses, `Alt+1` closes.
- **Run**: `$PATH` completion, Shift+Enter runs in foot, a confirmation for a command that is
  not on `PATH`.
- **Keys**: generated from `/etc/sway/config`, `config.d/*.conf` and the user's config, with
  the comment above each binding as its description, per-row glyphs, Enter copies the command.
- **Learn**: the seven manual pages rendered **in the window** (a markdown renderer with the
  theme's style; `glow` stays available from a terminal), plus `apropos` over the man pages.
- **About**: the bento (fastfetch-style facts as cards, sixel mark, build revision from
  `/etc/debian-aarch64-revision`), boot time, devices.
- **Colour**: the slurp pixel picker + the solarized swatches, copy on Enter.
- **Pkg** and **Dev**: the package search/install (with the not-stoppable task) and the
  profile export/restore surface, both as tasks.

**Out:** removing rofi from the image (014), the notification daemon (013), the greeter (013).

## Design

- The `.desktop` parser is a pure function with fixtures for the real files on this image
  (`firefox-esr`, `org.lazyos.*`, a `Terminal=true` entry, a `NoDisplay=true` entry).
- The window switcher uses `swaymsg -t get_tree` (the repo already speaks it) rather than the
  foreign-toplevel protocol, so it works with the current compositor and is testable against a
  recorded tree.
- Keys parses the same files `rofi-keys` parses today and shows the same descriptions; the
  parsing moves into the crate and its awk is deleted in the same PR.
- The markdown renderer handles the subset the manual uses (headings, lists, code fences,
  bold, tables) and renders code fences in the theme's code role — no `glow` subprocess.

## Acceptance criteria

- [ ] Apps: 30+ entries parse, each with its icon; typing `fire` launches Firefox; Shift+Enter
      on a TUI runs it in foot; a `NoDisplay` entry is absent; a `Terminal=true` app opens in
      foot.
- [ ] Window: lists every mapped window with its title, focuses on Enter, closes on `Alt+1`;
      an empty workspace shows an empty state.
- [ ] Run: completes a partial command, refuses an unknown one with the exact message; Escape
      cancels.
- [ ] Keys: every binding in the session's config appears with its comment; a binding without
      a comment shows the command (never an empty row); Enter copies.
- [ ] Learn: each manual page renders with headings, lists and code; `apropos` rows open the
      man page in the same renderer.
- [ ] About: the revision in the card equals `/etc/debian-aarch64-revision`; the mark is
      visible (sixel), the cards do not overflow at 80x24.
- [ ] witness: `scripts/check-system.sh` FAILs when a subcommand is missing, when `.desktop`
      parsing skips an installed entry, when `rofi-keys`' generated list and the new one differ
      in count, and when a launcher icon name is not in the theme.
- [ ] witness: paragraphs in `uis/sway/files/usr/share/doc/lazyos/02-rofi.md` (Apps/Window/Run)
      and `03-shortcuts.md` (Keys) and `04-software.md` (Learn/About).
- [ ] witness: an `AGENTS.md` bullet says the parser and the keys generator are pure functions
      with fixtures.
- [ ] unit tests: `.desktop` field codes, `NoDisplay`, `Terminal`, missing `Exec`; window tree
      flattening; path completion; keys parsing (set expansion, comments, modes); markdown
      subset; revision parsing.

## Tests

- **Unit** (`cargo test`): the parser and the keys generator against committed fixtures; `Exec` field-code
  expansion (`%f`, `%F`, `%u`, `%U`, `%%`).
- **Snapshot**: Apps (with icons), Window, Keys (a section with a description and one without),
  About at two widths, Learn (a page with a code fence).
- **Integration**: a recorded `swaymsg -t get_tree` JSON drives the window switcher; a stub
  `swaymsg` returns it.
- **Hardware**: launch Firefox and a TUI from the launcher, switch windows, copy a key
  command, read a manual page, and open About.

## Risks

- **A `.desktop` file with an `Exec` this system cannot run** — the launcher reports "not
  installed" on the row instead of failing at launch.
- **The keys list drifts from the config** — the check compares counts against the shell
  output before the shell script is deleted.
