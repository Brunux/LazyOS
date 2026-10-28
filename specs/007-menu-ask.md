# Spec 007 — Root menu, command palette and the Ask strip

- **Status**: approved (2026-10-28, with the Ask strip on the default agent)
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
- [ ] the ranking test: `th` ranks `theme` above `themes install` and `theme` above a
      subsequence-only match; frecency moves a repeated choice up after three launches.
- [ ] the destructive rows (reboot, power off, format) open a confirm with Cancel selected;
      nothing runs on a stray Enter.
- [ ] the Ask row opens a prompt, runs the default agent once (Claude Code or OpenCode, as
      above), and shows the answer inline with a copy action; a failure shows the agent's own
      error line, not a traceback.
- [ ] no process of the agent's group (`opencode serve`, `claude`) exists after the Ask window
      closes.
- [ ] witness: `scripts/check-system.sh` FAILs when `lazyos menu` is missing from `--help`,
      when the root-menu groups lose a row that the old `bar-menu` had (a table comparison),
      when the Ask row is gone, and when an Ask run leaves `opencode serve` (or `claude`) behind.
- [ ] witness: a paragraph in `uis/sway/files/usr/share/doc/lazyos/02-rofi.md` describes the
      menu, the palette and the Ask strip (what it sends, what it costs).
- [ ] witness: an `AGENTS.md` bullet records that a row is an enum action, not a shell string.
- [ ] unit tests: ranking (prefix vs subsequence vs frecency), group composition against a
      fixture table, `Action` dispatch (no shell), frecency pruning, prompt cancellation.

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
