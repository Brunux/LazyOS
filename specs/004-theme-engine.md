# Spec 004 — The theme engine

- **Status**: merged (PR #9)
- **Phase**: 2
- **Depends on**: 001
- **Model**: `opencode-go/deepseek-v4-pro` — a schema, a set of generators, atomic and
  idempotent writes, and a CLI surface: the correctness is in the state machine. Fallback
  `opencode-go/glm-5.3`.
- **Branch**: `spec/004-theme-engine`
- **Plan**: [`docs/plan-ui.md`](../docs/plan-ui.md) §8 (themes), §9 (cycle keys), §16 phase 5

## Problem

The palette is copied by hand into `foot.ini`, `style.css`, `solarized.rasi`, `mako/config`,
`starship.toml`, btop/lazygit/nvim/fzf configs and `brand` assets. Changing one value means
seven edits, and a theme switch is impossible without a script that rewrites files under the
user's home — the exact class of change that already bit this repo (a hand edit that looked
applied, an icon cache that never refreshed).

## Scope

**In (`lazyos-theme` + `lazyos theme`):**

- **`theme.toml`**: the palette (16 ANSI + semantic roles), the accent pair, wallpaper list,
  icon-set hint, and the surface table. Validated on load (contrast floor, required roles,
  known keys); an unknown key is an error naming the line.
- **Built-ins**: solarized (the current session, byte-identical output at first install) plus
  the seven from the plan (Catppuccin Mocha, Tokyo Night, Rose Pine, Nord, Gruvbox, Everforest,
  Kanagawa) as data files.
- **Generators**: `foot.ini`, waybar `style.css`, the legacy rofi `solarized.rasi` (until rofi
  leaves), mako `config` (until spec 013), btop, lazygit, nvim, fzf, starship, swaylock, and
  the wallpaper selection. Each generator is a pure function `Palette -> String`.
- **Writes**: every generated file goes through `tmp + rename`, and regeneration is
  **idempotent**: a second run with the same theme produces zero-byte diffs. A file the user
  edited by hand is never silently overwritten: `lazyos theme set` prints the diff and asks
  (spec 002 `Confirm`), `--force` answers.
- **Commands**: `lazyos theme list`, `set NAME`, `validate`, `preview` (palette card, sixel
  optional later), `current`.
- **Cycle keys**: `Alt+Shift+T` (next theme) and `Alt+Shift+W` (next wallpaper of the active
  theme), reserved in `etc/sway/config.d/` with the description comment `rofi-keys` reads.

**Out:** the extra themes' *wallpaper art* (spec 013), the greeter (013), the notification
daemon (013), the icon theme (the PNGs stay as they are).

## Design

- The active theme is one line in `~/.config/lazyos/theme` (name + version); generated files
  carry a header with the theme name, so `lazyos theme current` can verify what a surface
  actually contains.
- `check-system.sh` regenerates into a temp directory and compares with the installed files
  (**the idempotence witness**), and fails when a surface lost its header or a role.
- The current session's colours are the solarized built-in's expected output; the first `set`
  is therefore a no-op diff on a healthy machine — the fastest way to spot a generator bug.
- The cycle keys run `lazyos theme cycle` and raise no window (a one-shot write + a
  notification card), so they cost nothing when unused.

## Acceptance criteria

- [x] `lazyos theme set solarized` on a stock card changes nothing (empty diff on every
      surface); `set catppuccin` changes the footer, the bar, mako, btop and starship, and
      `set solarized` restores byte-identical files.
- [x] `lazyos theme set` refuses an unknown name (exit 2, list shown); `validate` rejects a
      palette below the contrast floor with the failing pair named.
- [x] an edited hand-written generated file is reported and not overwritten without
      confirmation; `--force` overwrites and says so.
- [x] `Alt+Shift+T`/`Alt+Shift+W` cycle and persist; a session restart keeps the choice.
- [x] witness: `scripts/check-system.sh` FAILs when the active theme file is missing, when the
      regeneration diff is non-empty, when a surface lacks the palette header, or when the two
      cycle bindings are absent.
- [x] witness: a paragraph in `uis/sway/files/usr/share/doc/lazyos/04-software.md` § Themes
      says how to switch, what changes, and how to undo a hand edit.
- [x] witness: an `AGENTS.md` bullet records that palette files are generated and the
      idempotence check is the witness.
- [x] unit tests: schema parsing (valid/invalid), every generator's golden output per built-in,
      idempotence, atomic failure injection, theme resolution and cycle order.

## Tests

- **Unit** (`cargo test -p lazyos-theme`), 25 tests in the crate:
  `solarized_is_the_session_that_exists_today` (the built-in's `Palette` equals
  `Palette::solarized_dark()`, which is the byte-identical promise),
  `every_builtin_parses_and_clears_its_own_contrast_floor`,
  `an_unknown_key_names_itself_and_not_a_silent_default`,
  `a_missing_colour_is_refused_because_a_surface_would_have_to_guess`,
  `a_palette_that_cannot_be_read_is_refused_with_the_pair_and_the_ratio`,
  `roles_derive_from_the_sixteen_when_a_theme_states_none`, the renderer's four
  (`a_role_is_written_hashed_or_bare_and_nothing_else`,
  `an_unknown_role_names_itself_and_the_roles_that_exist`,
  `a_theme_switch_moves_every_placeholder`, `the_header_names_the_theme_and_the_slug`,
  `an_unclosed_placeholder_is_an_error_not_the_end_of_the_file`), the writer's
  seven (`the_first_write_lands_and_the_second_is_unchanged`,
  `a_theme_change_moves_the_colours_and_the_header`,
  `a_hand_edit_is_refused_until_force_and_then_replaced`,
  `a_file_the_engine_never_generated_is_not_overwritten_in_silence`,
  `a_failed_write_leaves_the_old_file_whole_and_no_temp_behind`,
  `the_active_theme_is_one_line_that_round_trips`,
  `a_diff_summary_shows_the_change_and_counts_the_rest`) and contrast's three.
- **Golden** (`tests/surfaces.rs`): `the_golden_files_are_current` compares 8
  themes x 11 surfaces with `tests/fixtures/themes/` (`UPDATE_SNAPSHOTS=1`
  regenerates); `every_surface_renders_for_every_theme_without_a_literal_or_a_placeholder`
  fails on a surviving `{{`, on a colour that is not one of the theme's own (a
  hardcoded hex) and on a header that is not a comment in the file's language;
  `every_surface_changes_with_the_theme_or_it_is_not_reading_the_palette`.
- **Integration**: `a_second_set_is_an_empty_diff_and_a_round_trip_is_byte_identical`
  writes every surface into a temp `HOME`, runs it twice (the second run is all
  `Unchanged`, which also means no mtime moves), switches to gruvbox and back,
  and compares the bytes.
- **Hardware**: `reports/2026-10-23-theme-004.md` - every surface fed to its own
  parser (`foot --check-config`, `mako -c`, waybar, `rofi -dump-theme`, nvim,
  starship, `sway -C`), btop/lazygit/nvim opened, and the solarized/gruvbox
  round trip photographed. `check-system.sh`'s theme section is 6/6 on the card.

## Deviations

Recorded where the build differs from the sketch above, with the reason - the
criteria themselves were not edited to match the code.

- **The active theme is `~/.local/state/lazyos/theme`, not `~/.config/lazyos/theme`.**
  It is machine state (which palette was last applied), not user configuration;
  the generated files are the configuration, and a person who edits the state
  file would get a `theme current` that disagrees with every surface. The same
  directory holds the last generated body of each surface, which is what tells a
  hand edit from a template change.
- **`preview` is a truecolour card in the terminal, not a sixel card.**
  `lazyos theme preview [NAME]` prints every role with its swatch and hex, the
  16 ANSI colours and the contrast ratios `validate` enforces. It works over
  ssh, needs no window and no GPU; sixel art is the plan's later step and would
  change nothing about what the card says.
- **A hand edit is answered with `--force`, not a `Confirm` dialog (spec 002).**
  The command is `Shape::Direct` on purpose: it opens no window, which is what
  lets `Alt+Shift+T` cycle a theme without stealing focus. The prompt is the
  diff on stderr plus exit 1; `--force` is the answer, and `check-system.sh`
  prints the two lines a person needs.
- **No snapshot tests.** The two screens the sketch wanted (a palette card, a
  confirm dialog) are a printed card and a stderr diff, so there is nothing
  drawn to snapshot; the golden fixture per surface per theme (88 files) is the
  visual contract instead, and `UPDATE_SNAPSHOTS=1` regenerates it.
- **The idempotence witness renders in-process.** `lazyos theme diff --quiet`
  renders every surface and compares it with the installed file, instead of
  generating into a temp directory: same question, no writes, and the exit
  status is what `check-system.sh` reads.
- **`fzf` is generated as `~/.config/lazyos/fzf.env`,** sourced from `~/.bashrc`,
  because fzf has no colour file - `FZF_DEFAULT_OPTS` is the only channel.
- **Two surfaces stayed out of the generator**: `foot/screensaver.ini` carries
  the brand's cyan (the brand rule owns it) and the GTK stylesheets are the
  session's, not a terminal's palette. Both are named in `AGENTS.md`.

## Risks

- **A generator drifts from the hand-written file** and the first switch breaks a surface —
  the byte-identical first `set` and the golden tests are the catch.
- **A write interrupts a session** (rename across filesystems) — the temp file is created in
  the target directory, and a test injects a failure mid-write.
