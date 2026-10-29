# Spec 009 — The three search questions: files, find, grep

- **Status**: in progress (approved 2026-10-29: the three old scripts become links to the binary in this
  PR, `lazyos-view` becomes `lazyos open` behind a link rather than a generated script, and the
  work is built on `claude-opus-5-5` - one tier above the assignment, recorded as a deviation for
  §6.2 rather than a re-route)
- **Phase**: 3
- **Depends on**: 001, 002, 003, 005
- **Model**: `claude-sonnet-5-5` (effort `high`) — an async walk, a preview pipeline and three
  different row builders, each with tests: dense implementation against a clear shape. Fallback
  `claude-opus-5-5`.
- **Branch**: `spec/009-search`
- **Plan**: [`docs/plan-ui.md`](../docs/plan-ui.md) §5.3 (files), §5.4 (find), §5.5 (grep)
- **Old paths**: `rofi-files`, `lazyos-find` (fzf), `rofi-grep` and `lazyos-view`, each a link to
  `/usr/bin/lazyos` in `~/.config/bin` from this PR on (the bar-menu shape of spec 007), so the
  bindings, `org.lazyos.files.desktop`, `org.lazyos.view.desktop` and the `xdg-open` shim keep
  their paths

## Problem

The three search surfaces exist as three scripts with three different behaviours and three
copies of the opener table (`rofi-files` and `lazyos-view` already carry the same rules twice,
by hand). The list has no preview beyond a 128 px thumbnail, the fzf window cannot be themed,
and a grep match opens a `less` that cannot seek a pipe.

## Scope

**In (`lazyos menu files`, `lazyos menu find`, `lazyos menu grep`):**

- **Files**: one directory per screen with the theme's glyph per row, a **right-hand preview
  pane** (the 128 px thumbnail from `lazyos-thumbnail`'s cache, text excerpt, or metadata),
  `Alt+1` file actions as a second menu, the remembered directory from
  `~/.local/state/lazyos/files-last`, and the dotfile/parent rows.
- **Find**: `fd` streaming into the in-process fuzzy filter (the framework replaces fzf for
  this surface), uncapped, with the same preview pane (image symbols, not sixels — the pane is
  part of the frame); Esc cancels the walk.
- **Grep**: `rg` over `$HOME` with the same excludes and limits, the matching text on the row,
  Enter opens the file **at the line** in a viewer that can seek (the framework's own log
  pane), not `bat | less`.
- **One opener table** in the crate: extension → action, used by all three and by spec 012's
  double-click; `lazyos open PATH` applies it, and `lazyos-view` is a link to the binary that
  resolves to that command, so there is no second copy to keep in step.

**Out:** the file manager window (favnyr stays, and its double click reaches `lazyos open`
through `org.lazyos.view.desktop`), the thumbnail generator itself (the existing
`lazyos-thumbnail` is reused), clipboard actions (spec 011).

## Design

- The preview pane is a widget: `Preview::for(path)` returns an image protocol, a text excerpt
  or metadata, with a per-type cache; images use the existing cache file, so a picture is
  decoded once ever.
- Find's walk is a thread feeding a channel; the UI cancels by dropping the receiver and
  killing the child `fd` — no `pkill` pattern needed.
- The opener table is data (`[(glob, Action)]`), so a test can assert that the list, the window
  and the new surface agree.
- A missing `fd`/`rg` degrades to a message row that says which package provides it, never an
  empty list.

## Acceptance criteria

- [x] Files: navigating a directory, opening a text file (viewer), a picture (preview then the
      image viewer) and a video (player) works; the remembered directory survives a restart.
  - *Observed*: `tests/files.rs` drives the screen with keys: Enter into `Photos`, Backspace
    back up, `notes` + Enter ends on `Chosen::Open(notes.md)`, and `files-last` holds the
    folder each time (`navigating_opening_and_remembering_where_it_was`); the opener behind it
    is `lazyos_view_starts_what_the_table_names` - the `lazyos-view` link with fake viewers on
    `PATH` starts `imv --` for the picture, `foot --app-id=popup … bat` for the text, `mpv
    --no-config` for the film, a `foot --app-id=lazyos-pick … menu files -- DIR` for a folder,
    refuses the broken link with exit 3, and decodes a `file://` URI. The picture's preview is
    the `files-picture` snapshot (half blocks); in a pty answering like foot the same window
    drew sixel, and in one that does not answer it drew half blocks after 300 ms, with the real
    `lazyos-thumbnail` writing the 256 px thumbnail. `start_dir` survives a restart and falls
    back to home when the folder is gone (`remembered_directory_fallback`).
- [ ] `Alt+1` on a row opens the action menu; a destructive action asks first; a packed file
      that would overwrite is refused (the existing rules).
  - *Observed, in part*: the row's action key opens `rofi-file-actions` for that path
    (`a_typed_path_opens_when_no_row_matches_and_tab_asks_for_the_actions`: Tab, and Alt+1
    when it arrives), and that script is unchanged, so its confirmations and its refusal to
    overwrite are the existing ones. **Alt+1 itself does not reach the window in the
    session**: sway binds it to workspace 1 and foot asks for no shortcut inhibitor (rofi
    could use it because `global-kb` inhibits), so the key the session can deliver is Tab.
    Whether that is the answer, or the criterion wants a sway rule that frees Alt+1 for
    `lazyos-pick`, is the maintainer's call; it is left unchecked for that.
- [x] Find: typing narrows while `fd` is still walking; Esc stops the walk (no `fd` child
      survives); a 100k-file home tree paints the first result under 100 ms.
  - *Observed*: a fake `fd` that prints 301 paths and keeps running: all 301 are listed while
    it runs, `notes` + Enter opens the match, and dropping the window kills and reaps it
    (`typing_narrows_while_the_walk_runs_and_escape_ends_it`); in a real pty with the real
    `fdfind` 9.0.0 Escape closed the window and no `fdfind` was left. With the real `fd` on
    the build host (x86_64, 4 cores, release) a 100,000-file tree painted its first row after
    **12.6 ms** and finished in 103 ms; a keystroke over all of it cost 1-67 ms
    (`a_hundred_thousand_files_paint_the_first_row_under_100_ms`, `--ignored`). Not yet
    measured on the Hayato.
- [x] Grep: a match opens at its line highlighted; an invalid regex reports ripgrep's own
      message; the walk stops on Esc.
  - *Observed*: `grep-at-the-line` is line 5 of the file marked and in view (the test reads
    `target == 4`), `grep-bad-pattern` is ripgrep's own `regex parse error` lines, and Escape
    during a search kills the `rg` the window started while the window stays
    (`escape_stops_a_search_that_is_still_running`); in a real pty with the real `rg` the
    window opened a match, went back, and closed with nothing left running.
- [x] witness: `scripts/check-system.sh` FAILs when `fd`/`rg`/the opener binary is missing,
      when the three subcommands are absent, when `lazyos-view` or one of the three old scripts
      is not a link to the binary, and when the preview cache is not writable.
  - *Observed*: the block was run on the build host against the debug binary in a scratch
    home: OK for the three links, FAIL for a `lazyos-view` that is a script again; the
    framework section already FAILs when `lazyos` itself is missing, and each name also FAILs
    when its command leaves `--help`. `fd` and `rg` are FAILs, and the cache check judges
    the nearest existing folder so the report still writes nothing.
- [x] witness: a paragraph in `uis/sway/files/usr/share/doc/lazyos/02-rofi.md` explains the
      three surfaces and when to use which.
  - *Observed*: § *Browsing files* opens with the four keys as a table of questions and when
    to use which; `03-shortcuts.md` and `04-software.md` follow.
- [x] witness: an `AGENTS.md` bullet says the opener table is one table and the preview is the
      picture.
  - *Observed*: "The three file questions are one binary with one opener table, and the
    preview is the picture".
- [x] unit tests: opener table, row builder per type, grep row parsing, find chunk handling,
      remembered-directory fallback, preview selection by extension.
  - *Observed*: `opener_table_is_total`, `opener_table_has_no_duplicate`,
    `a_row_says_what_the_table_says`, `grep_parses_path_line_text`, `find_chunk_handling`,
    `find_survives_a_directory_that_disappears`, `remembered_directory_fallback`,
    `preview_picks_text_for_text`, `the_terminal_answer_names_sixel_and_the_cell`, and
    `rows_that_arrive_later_are_filtered_and_keep_the_cursor` (`cargo test --workspace`).

## Tests

- **Unit** (`cargo test`): `opener_table_is_total` (every extension has a default), `preview_picks_text_for_text`,
  `find_survives_a_directory_that_disappears`, `grep_parses_path_line_text`.
- **Snapshot**: files (icon, thumbnail, text), preview pane at two widths, empty states.
- **Integration**: a fixture tree in a temp dir; drive the list with a scripted key sequence;
  assert the opener command for a picture, a text file, a directory and a broken link.
- **Hardware**: browse the card's `$HOME`, preview a photo, open a PDF, search for a word and
  open the match; the preview is legible and the walk stops on Esc.

## Risks

- **The preview decodes a 24 MP picture on the UI thread** — the existing four-slot `flock`
  gate and the 128 px cache are reused; the spec's budget is "first paint under 100 ms".
- **Find holds a growing result set** — the channel is bounded and the filter works on the
  stream, so a huge tree costs memory in `fd`, not in the UI.
