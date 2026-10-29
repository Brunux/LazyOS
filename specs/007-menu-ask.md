# Spec 007 — Root menu, command palette and the Ask strip

- **Status**: in progress (approved 2026-10-28, with the Ask strip on the default agent)
- **Phase**: 2
- **Depends on**: 002, 003, 005
- **Model**: `claude-opus-5-5` (effort `high`) — the menu is the session's face: grouping,
  hierarchy and the first three screens the taste is judged on, plus the one rule a mistake here
  breaks for every later spec (a row is an enum action, never a shell string). Fallback
  `claude-sonnet-5-5`.
- **Branch**: `spec/007-menu-ask`
- **Plan**: [`docs/plan-ui.md`](../docs/plan-ui.md) §5.1 (root menu), §5.2 (palette), §13.4
  (Ask strip), §16 phase 2
- **Old paths**: `bar-menu` (the root menu) and `rofi -show drun` (the launcher, ported in 012)

## Problem

`bar-menu` is a rofi dmenu with 60 rows and no hierarchy beyond headings; its result is a
command string that another script has to interpret, and the palette (`rofi -show drun`) is a
separate program with a separate ranking. The Ask strip does not exist: asking the AI anything
means leaving the desktop for a terminal.

## Scope

**In (`lazyos menu`):**

- **Root menu**: the existing groups (openers, AI, session, system) rendered by the design
  system's `List`, with the glyph per row, `●` markers for defaults, and *Cancel first* on
  anything destructive. Selecting a row dispatches a subcommand (no shell strings).
- **Fuzzy filter** on the same surface (type to narrow, arrows, Enter), with the ranking
  shared with the palette: exact prefix > word prefix > subsequence, then frecency.
- **Command palette**: all `lazyos` subcommands plus the `.desktop` launcher entries (spec
  012 fills the icons and the launch path; this spec defines the ranking and the surface).
- **Ask strip v1**: a row (and a launcher entry) opens a prompt; the question runs **once,
  through the default agent** - the one the Agents menu marks with `●`
  (`~/.config/lazyos/agents/default`, the rule `rofi-agents --default` already applies) - and
  the answer renders **inline in a task window** (spec 005), with a copy action. Two agents
  have a one-shot mode wired here: Claude Code (`claude -p --output-format json`, with no tools
  and no session saved) and OpenCode (`opencode run --format json`). OpenCode answers when no
  default is set, when the default is not installed, or when the default has no one-shot mode
  here (Codex, Gemini, Copilot), and the header names the agent that will answer. No server,
  no streaming — v2 (spec 013+) keeps the conversation.
- **Frecency**: `~/.local/state/lazyos/frecency.json`, written on launch, capped and pruned;
  an unreadable file starts empty (never an error).

**Out:** the launcher's `.desktop` parsing and icons (012), the window switcher and run (012),
the AI chat with streaming (later), MCP (008).

## Design

- The menu is data: `MenuItem { key, label, glyph, hint, action: Action }`, where `Action` is
  an enum (subcommand, shell-free), so a row cannot invent a command line.
- The Ask row is marked `AI` in the header and says what leaves the machine before it runs -
  **the question, and nothing else**: the agent runs in an empty private directory under
  `$XDG_RUNTIME_DIR` (Claude Code with `--tools ""`), so there is no working directory to read.
  Enter is the confirmation.
- The agent runs in its own process group, and the group is killed when the answer is in or
  the window closes: a helper the agent left behind (`opencode serve`) cannot outlive the
  question.
- The palette shows the same `Glyph` alphabet; the `.desktop` icons arrive in 012 and a row
  without one keeps the column (spec 002/003 rule).
- Every row that is slow runs through TaskView (spec 005), so the menu closes into a result,
  never into a toast.

## Acceptance criteria

- [ ] the root menu opens on `Alt+Space`, lists the groups, filters while typing, and runs the
      selected action; Escape closes with exit 1 and no state change.
  - *Observed*: `Alt+Space` still runs `~/.config/bin/bar-menu`, which is now a link to `/usr/bin/lazyos` and resolves to `menu` (`command.rs`, `--symlinks`). The four groups and the filter line are in the golden snapshots (`menu`, `menu-filtered`, `menu-empty` at 80x24 and 152x45, `lazyos-cli/tests/snapshots/`); `wifi` + Enter ends on `~/.config/bin/bar-net` and nothing else (`tests/menu.rs::typing_a_name_and_enter_runs_that_row_as_a_program_and_its_words`); Escape is exit 1 with no state directory written (`escape_closes_with_exit_one_and_writes_nothing`, and in a real pty `tests/cli.rs::escape_returns_one_and_draws_the_root_menu` / `a_menu_writes_nothing_to_disk`). On the build host (x86_64, release) the open menu is 2.9 MB PSS / 4.4 MB RSS, against rofi's 15-17 MB.
  - *Pending on the Hayato*: `Alt+Space`, type `wifi`, Enter → the Wi-Fi picker (the spec's hardware test), and the window floating as `lazyos-pick`.
- [x] the ranking test: `th` ranks `theme` above `themes install` and `theme` above a
      subsequence-only match; frecency moves a repeated choice up after three launches.
  - *Observed*: `lazyos-ui/tests/ranking.rs::ranking_prefers_prefix` (and `every_tier_lands_in_its_place`, `a_boost_orders_equal_matches_and_never_beats_a_better_tier`); `lazyos-cli/src/menu/frecency.rs::frecency_promotes_after_three` - two launches leave the order alone, the third moves `themes install` above `theme`.
- [x] the destructive rows (reboot, power off, format) open a confirm with Cancel selected;
      nothing runs on a stray Enter.
  - *Observed*: every Power row (log out, reboot, shut down) followed by Enter answers Cancel and runs nothing, Escape backs out one level at a time, and only Right + Enter returns the session action (`tests/menu.rs::a_stray_enter_on_power_runs_nothing`; the `menu-confirm` snapshot shows Cancel selected).
  - *Deviation*: *format* is not a root-menu row and never was (`bar-menu` reached it through Places → Format a disk, where `rofi-format` already puts Cancel first); the root menu's one disk-writing row, *Install LazyOS*, opens `rofi-install`, which has Cancel first and a typed gate. Neither is ported here, so their confirms are unchanged.
- [ ] the Ask row opens a prompt, runs the default agent once (Claude Code or OpenCode, as
      above), and shows the answer inline with a copy action; a failure shows the agent's own
      error line, not a traceback.
  - *Observed*: with a fake `opencode` in `~/.opencode/bin` and a fake `claude` in `~/.local/bin` (the agents' `PATH`), the answer is drawn in the window with `[c] copy` (`opencode_answers_inline_and_leaves_nothing_behind`, `claude_code_answers_with_no_tools_and_no_session` - which also reads back the exact argv: `--tools ""`, `--no-session-persistence`, `--` before the question); the question is one argument, unexpanded (`what is $HOME; rm -rf ~` arrives verbatim); a JSON error event shows its own message and a crash its stderr line, never `ERROR:` (`a_failure_shows_the_agents_own_error_line`); the agent choice and its note are `ask.rs::the_default_agent_answers_when_it_can` and `opencode_answers_for_a_default_it_cannot_ask_and_says_why`. Snapshots: `ask-prompt`, `ask-no-agent`, `ask-running`.
  - *Pending on the Hayato*: a real question with a signed-in OpenCode and Claude Code, and one with the network off (the spec's hardware test); the copy through the real `wl-copy`.
- [ ] no process of the agent's group (`opencode serve`, `claude`) exists after the Ask window
      closes.
  - *Observed*: the fake agent starts one helper in its group and one that leaves it with `setsid`; neither is alive after the answer (`opencode_answers_inline_and_leaves_nothing_behind`), after Ctrl+C in the window (`stopping_a_question_ends_the_agents_group`), or after the asking window's process is killed without a word (`a_window_that_dies_takes_the_question_with_it`, which also finds no process left with the question's `LAZYOS_ASK` mark).
  - *Pending on the Hayato*: the real `opencode run` (whether and how it starts `opencode serve`) and `claude -p`, with `pgrep -f opencode` after the window closes.
- [x] witness: `scripts/check-system.sh` FAILs when `lazyos menu` is missing from `--help`,
      when the root-menu groups lose a row that the old `bar-menu` had (a table comparison),
      when the Ask row is gone, and when an Ask run leaves `opencode serve` (or `claude`) behind.
  - *Observed*: `--help` without `menu` was already a FAIL (spec 001), and `ask` is a new one. The framework section compares `lazyos menu --rows` with bar-menu's 43 keys (spec 016 added Local model) and FAILs naming each lost one, FAILs without a `menu:ask` row, and FAILs when `~/.config/bin/bar-menu` is not the link; a process carrying `LAZYOS_ASK=<id>` whose `--ask-run <id>` is gone is a FAIL with its pid and name. The blocks were extracted and run on the build host against the debug binary: OK on the clean tree, FAIL with a planted `env LAZYOS_ASK=ask-test-1 sleep 30`. The checks that grepped the old script (Developer, Install, Camera, Share this screen, Agent session, the five menus of `02-rofi.md`) now read `--rows`, and "a row with no action" became "a row whose program is missing".
- [x] witness: a paragraph in `uis/sway/files/usr/share/doc/lazyos/02-rofi.md` describes the
      menu, the palette and the Ask strip (what it sends, what it costs).
  - *Observed*: § *The root menu, the palette and Ask* (typing, the ranking, frecency and its file, Power's confirms, the palette's rows, what Ask sends, what it costs, and that nothing stays); `07-agents.md` points to it from the AI group, and `03-shortcuts.md` says the menu filters.
- [x] witness: an `AGENTS.md` bullet records that a row is an enum action, not a shell string.
  - *Observed*: "A menu row is an enum action, never a shell string, and the root menu is that table" in the feature rules (with why `bar-menu` is a link, why the checks read `--rows`, and the three mechanisms behind "nothing outlives the question").
- [x] unit tests: ranking (prefix vs subsequence vs frecency), group composition against a
      fixture table, `Action` dispatch (no shell), frecency pruning, prompt cancellation.
  - *Observed*: `ranking_prefers_prefix`, `frecency_promotes_after_three`; `the_root_menu_keeps_every_bar_menu_row` and `the_groups_read_in_bar_menus_order` against `BAR_MENU_KEYS`; `action_never_contains_a_shell_metacharacter`, `a_row_becomes_its_program_and_its_words`, `rows_say_what_they_run_in_the_words_the_check_matches`; `frecency_prunes_to_cap`, `an_unreadable_file_starts_empty`; `prompt_cancellation_goes_back_and_sends_nothing` (`cargo test --workspace`).

## Tests

- **Unit** (`cargo test`): `ranking_prefers_prefix`, `frecency_promotes_after_three`, `menu_table_has_no_dupes`,
  `action_never_contains_a_shell_metacharacter`, `frecency_prunes_to_cap`.
- **Snapshot**: menu (unfiltered, filtered, empty result), confirm, Ask prompt, Ask running.
- **Integration**: drive the menu with a scripted key sequence; assert the chosen action id and
  the process it would spawn (a fake `claude` and a fake `opencode` on `PATH`).
- **Hardware**: `Alt+Space`, type `wifi`, Enter → the Wi-Fi picker (spec 010) opens; ask a
  question with the network off and read the failure.

## Risks

- **The menu becomes the place every feature hides** — the group table is compared with the
  old `bar-menu` in check-system, and new rows need their feature's spec.
- **The Ask strip leaks the working directory or a secret to the model** — the header states
  what is sent, the prompt is single-shot, and the tool never includes a file unless the person
  asked for one.
