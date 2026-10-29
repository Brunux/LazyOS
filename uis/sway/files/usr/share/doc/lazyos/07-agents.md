# Coding agents

Omarchy ships an "agents" feature: a bar panel with subscription usage meters,
plus a set of scripts that keep several accounts per provider and can switch
accounts automatically near a rate limit. This image carries the local half of
that - launching and account profiles - and not the usage meters.

**The menu**: `rofi-agents`, on **Alt+Ctrl+Shift+A** (Omarchy's `Super+Shift+Ctrl+A`),
or "Agents" in the root menu / the launcher.

**A session in one click.** The root menu's AI group (`Alt+Space`) opens with
**Agent session**: it starts the agent the list marks `●` - OpenCode on a fresh
card, because the image ships it - in `~/Work`, without opening the list first.
The list below it is where another agent is picked, installed or given a second
account, and `Set the default agent…` is the row the menu's first row follows.
From a shell the same thing is `rofi-agents --default`; when the recorded default
is not installed any more it starts OpenCode instead of failing, and with no
agent CLI installed at all it raises a card ("the Agents menu installs one with
npm") rather than opening a terminal that closes at once.

**One question, without a session.** The AI group's **Ask** row (also the
**Ask** launcher entry, and `lazyos ask` in a terminal) sends one question to
the same default agent - when it is Claude Code or OpenCode; any other default
is answered by OpenCode, and the window says so - and shows the answer in the
window it was asked in, with **c** to copy it. Only the question is sent: the
agent runs in an empty private directory, Claude Code with no tools and no saved
session, and nothing of it stays running afterwards. `02-rofi.md` § *The root
menu, the palette and Ask* has the details.

| Row | Does |
|---|---|
| `<Agent>` | runs it in foot, in `~/Work` when that directory exists (agents remember trust per directory, and they refuse `$HOME`) |
| `<Agent> — install …` | `npm install -g --prefix ~/.local <package>` in a terminal, so nothing is written to `/usr/local` and no root prompt appears |
| `<Agent> accounts…` | create, switch and remove accounts (see below) |
| `Set the default agent…` | marks the agent shown with `●` |
| `Open a terminal in ~/Work` | a shell where the agent binaries are on `PATH` |

Every row carries the glyph the rest of the session's menus use (`lib-rofi.sh`):
a code sign for an agent, two people for its accounts, a download arrow for an
install, a gear for the default, a terminal for the shell, a bin for removing an
account and an undo arrow for going back. The default agent is still the one
marked `●`, next to its glyph.

The agents it knows about, and their packages:

| Agent | Binary | npm package |
|---|---|---|
| Claude Code | `claude` | `@anthropic-ai/claude-code` |
| Codex | `codex` | `@openai/codex` |
| OpenCode | `opencode` | `@opencode/cli` |
| Gemini CLI | `gemini` | `@google/gemini-cli` |
| GitHub Copilot | `copilot` | `@github/copilot` |

`node` and `npm` are already installed (they arrived for exactly this), and
`~/.local/bin` is on `PATH` for every shell, so a CLI installed here is visible
in the next terminal without logging out.

**OpenCode is already installed.** It is the one agent the image ships: the build
fetches the pinned arm64 tarball from `tools/opencode/`, verifies its checksum and
extracts it to `~/.opencode/bin`, so its row on a fresh card says *launch* instead
of offering an install. That directory is the upstream installer's layout rather
than npm's, and that is what keeps `opencode upgrade` usable with no root: it
prints *Using method: curl* for a binary there and rewrites it in place, while
from `/usr/local` it answers *installation method not found* and asks for a
package manager. `~/.opencode/bin` is on `PATH` for every shell and for the menu
itself, and the image ships it **signed out** - the sign-ins live in
`~/.local/share/opencode`, which the build never writes.

The package in the table is the V2 one; `opencode-ai` is V1. npm 12 blocks
lifecycle scripts by default, and OpenCode's postinstall is the step that picks
its platform binary, so the menu's install line passes `--allow-scripts=<package>`
(and without it the CLI answers *postinstall script was not run* while `command -v`
finds it happily). Bumping the pinned version is `tools/opencode/README.md`.

## Accounts

Claude Code and Codex both support an alternative configuration directory -
`CLAUDE_CONFIG_DIR` and `CODEX_HOME` - and that is all an "account" is here: a
directory per account under `~/.local/share/lazyos/agents/<agent>/<account>/`,
with the last one used recorded next to it. Creating one is empty on purpose:
the first launch in that account runs the CLI's normal sign-in and the profile is
created there, so two subscriptions never share a session or a token.

From the shell, the same thing without the menu:

```sh
eval "$(rofi-agents --env 'Claude Code' work)"   # export CLAUDE_CONFIG_DIR=…
claude
```

OpenCode, Gemini and Copilot have no variable that moves everything an account
needs, so their menu entry says so instead of pretending: OpenCode v2 does read
`OPENCODE_CONFIG_DIR`, but its sign-ins live in the database under the data
directory, so an account built on that variable alone would share the login.

## What is not here, and why

* **Usage meters.** Omarchy's bar panel reads subscription usage through
  provider APIs and local session logs. Those change shape when a provider
  changes, they need credentials, and a broken meter is worse than none. If they
  come back, the shape that fits this port is one `python3` script (standard
  library only, already in the image) printing a single line for a waybar
  `custom` module on an interval - no daemon and no new language.
* **Automatic account switching** near a limit: it depends on the meters above.

## Cost

The menu is a rofi window (about 16 MB while open) and a foot window for the
agent itself. The agents are Node programs: 150-400 MB resident while they run,
which is a lot on 4 GB. Run one at a time, and close it when you are done - the
bar's power panel shows memory and swap if you want to check. OpenCode is the
exception there too: one compiled binary (236 MB **on disk**, in
`~/.opencode/bin`) instead of a Node install, so it costs the image that space and
nothing while it is not running.
