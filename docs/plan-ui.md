# Plan: The LazyOS UI framework

> Design for the session UI framework that replaces rofi, mako-as-UI and the bash plumbing of
> `uis/sway/files/home/.config/bin/` with one Rust framework, hosted in foot, built on Ratatui.
> It extends [`plan.md`](plan.md) §3/§5 (Rust first, the multi-call `lazyos` binary) and turns
> the "port the 50 scripts" work into "build the framework that hosts them". The concrete
> visual language follows the current ratatui design canon (yazi, lazygit, gitui, bottom,
> atuin, television, tachyonfx) and Omarchy's theming patterns.
>
> Linked from [`plan.md`](plan.md) §3/§5 and §10; the witnesses land in
> `scripts/check-system.sh` as each tier ships.

**Status: proposed.** The installer TUI of v0.4.0 ([`plan-installer.md`](plan-installer.md)) is
the first consumer; this document is the session-wide framework around it.

---

## 1. Why: the problem being replaced

The session is three disconnected surfaces today:

| Surface | What it does | Cost of the model |
|---|---|---|
| rofi (dmenu/drun/window/run/ssh/calc) | lists; one line of text per action | cannot show progress, panes or live results; "result" = a notification |
| mako | transient toasts | a *different* loop: the action runs and the user watches the desktop |
| bash + foot popups | logic and long tasks | the user **falls out of the loop**: install = menu → detached runner → toasts + bar spinner + a *second* log window |

The design bug is that **the result of a slow action never returns to the window that started
it**. Ratatui fixes it at the root: a window is a UI state that can transition (list →
checklist → spinner/gauge → live log → result) without closing.

## 2. The principle

**One window = one transitioning state; the answer appears where the question was asked.**
Long actions stop being "menu, then notifications, then another window"; they become a task
view that ends by showing its own result inline.

- **Ratatui (crossterm) inside foot.** Foot is not replaced — Ratatui needs a terminal and foot
  is the session's terminal. Replaced: rofi, mako-as-UI, the bash plumbing.
- **Nothing resident.** One `lazyos` process while a window is open, nothing after. mako is the
  only notification daemon until phase 5's `lazyos-notify` swaps it 1:1.
- **Multi-call binary.** `lazyos <subcommand>`; the 64 old paths under `~/.config/bin/` become
  symlinks to `/usr/bin/lazyos`, so the ~55 sway bindings, the 31 `.desktop` `Exec` lines and
  the manual keep working unchanged (dispatch by `argv[0]`, busybox-style). A dangling link
  fails `-x` in `check-system.sh`.

## 3. Surfaces (window shapes)

| Shape | app-id | Geometry (new `for_window` rules in `config.d/`) | Use |
|---|---|---|---|
| **pick** | `lazyos-pick` | floating, centered, width by content class (45/62/76/88 % of the panel, the `ui_width` classes) | menus, lists, forms |
| **task** | `lazyos-task` | floating 940×620, centered (the current `popup` rule) | long actions: scans, installs, downloads |
| **full** | `lazyos-full` | fullscreen | installer (v0.4), greeter |

- `lazyos menu X` / `lazyos task X` spawn foot themselves (the `lazyos-find` pattern):
  `foot --app-id=lazyos-<shape> -e lazyos ui <mode>`, with `LAZYOS_UI=1` in the environment so
  a re-exec from an existing terminal does not open a second window; guard: if the app-id is
  already live, `swaymsg '[app_id=…] focus'` (the `rofi-vnc` pattern).
- swayfx blur already applies to floating windows with alpha (foot 0.92); `layer_effects` is
  layer-shell-only and needs nothing. Window open/close animations (150 ms) come free.
- Terminal geometry at 940×620 with JetBrains Mono 11 ≈ 92 cols × 46 rows — comfortable for the
  task layout.

## 4. Architecture (crates)

```
rust/Cargo.toml            workspace, Cargo.lock committed
rust/crates/lazyos-ui      the framework: design system (tokens, anatomy, motion), widgets
                           (list, task, form, confirm, image, osd), launch (foot spawn per
                           shape), task protocol, notification bridge (lazyos notify)
rust/crates/lazyos-icons   the row-glyph alphabet (enum Glyph → Nerd Font codepoint, filtered
                           at build time against the installed font charset) and theme-PNG
                           resolution (name → path)
rust/crates/lazyos-theme   the theme engine: theme.toml parsing, semantic role derivation,
                           {{ }} template renderer, per-surface generators, apply/reload
rust/crates/lazyos         multi-call binary: menu/task/form/osd/act/table modes + logic tools
rust/crates/lazyos-ai      the OpenCode interfaces: `opencode serve` client, the chat widget,
                           and the `lazyos mcp` server (rmcp)
rust/crates/lazyos-installer  (planned, plan-installer.md) consumes lazyos-ui widgets
rust/crates/lazyos-notify  phase 5: the notification daemon (replaces mako)
```

`lazyos-core` / `lazyos-disk` / `lazyos-identity` / `lazyos-install` stay as planned in
`plan.md`; `lazyos-ui` never touches disks.

**Dependencies**: `ratatui` 0.31 + `crossterm`, `tachyonfx` (motion), `ratatui-image` (sixel),
`ratatui-macros` (declarative layouts), `throbber-widgets-tui` (spinners), `tui-big-text`
(hero, optional), `fuzzy-matcher` (the fzf algorithm), `swayipc` (already a workstyle dep),
`serde`/`serde_json`/`toml`, `unicode-width`, `greetd_ipc` (the greeter), `rmcp` (the MCP
server). All pure-Rust, aarch64 native, built `--release --locked` under `$WORK_DIR`.

## 5. Design system (locked)

The canonical references: **yazi** (async walk + preview pane + task manager with progress),
**lazygit** (footer key hints, statusline, modal popups — the interaction canon), **bottom**
(gauges, sparklines, sortable tables, mouse), **gitui** (lean cmdbar, focus borders),
**television** (streaming fuzzy finder), **atuin** (instant history search), **tachyonfx**
(official ratatui effects library), **btop** (rounded boxes set the visual bar).

1. **Anatomy.** Every window: `header` (glyph + title + state/breadcrumb) / `body` (panes) /
   `footer` (action bar) / `statusline` (context). No window without all four.
2. **Borders.** Rounded, 1 px, title embedded (` glyph Files`); focus = accent, inactive = dim
   (`Block::bordered().rounded().title()`).
3. **Rows.** The current row language (glyph + bold name + dim detail) + soft inverted
   selection + scrollbar + mouse hover.
4. **Action bar.** `[enter] open  [a] action  [d] delete  [esc] close` — recomputed per mode,
   inactive keys dimmed (gitui cmdbar). This replaces `ui_pick_alt`'s `Alt+1` second action.
5. **Statusline.** pick: `12 rows · 3 marked`; task: `phase 3/9 · 42% · 00:47`; dashboards:
   clock/battery.
6. **Motion.** tachyonfx: fade-in on open (~150 ms), cyan sweep on task-phase completion,
   dissolve on close. **Hard rule: effects run only during transitions (≤300 ms), never loop;
   the tick stops when the effect ends.** Cost on the Mali-G57: one repaint per frame while an
   effect lives — measured before it is committed.
7. **Hero.** Welcome/installer/greeter open with `tui-big-text` of the block z_ (the committed
   art is already this form) + splash.
8. **Bento.** About is a card grid (CPU sparkline, memory gauge, battery, disk, kernel),
   bottom-style; Free space gets a per-directory bar chart; Kill/Packages are sortable tables.
9. **Popups.** Every destructive confirm (power, format, install, restore) is a centered modal
   with a danger border + typed gate; the list behind is dimmed.
10. **Preview.** Files = list left + preview pane right (128 px thumbnail sixel + metadata +
    actions), yazi-style; Find = async walk with live filtering + sixel preview.
11. **Mouse first.** Hover, click on rows/buttons, wheel on lists and logs; footer buttons are
    clickable. Foot reports SGR mouse (proven by the screensaver).
12. **Gradients.** `Gradient` fills in task gauges (solarized base03→accent); optional, late.
13. **Hyperlinks.** OSC 8 in Learn (verify foot support in the spike).
14. **The layout is ours, not rofi's.** A port keeps the *job* - the list, the switch, the
    confirm - not the shape. The taste it aims at is **Omarchy's**: wide margins, one accent,
    soft cards, a selection that reads as a row rather than a highlight, and metadata dim
    enough that it never competes with the name. None of that is copied from rofi, because
    rofi's row is a single line of text with a 26 px icon and no space between a name and its
    detail - on a 152x45 canvas that throws the canvas away. Every surface is designed at the
    size it actually opens in (§3), and `pick`/`task`/`full` are the grid it is designed
    against, so a ported menu may come out shorter, wider, two-column, or with a preview pane
    beside the list. What a port must keep is the job and the keys, never the old geometry.
15. **Every list filters as you type, and case does not matter.** A search that is not
    case-insensitive is not a search (`readme` must find `README.md`, `año` its `AÑO`), and
    the narrowing happens on the keystroke, not on `Enter`. `ListState` is the only
    implementation: the needle folds once per change, an ASCII needle takes an allocation-free
    byte scan, an accented one the Unicode path, and a growing needle tests only the rows that
    are already visible. A surface that adds a search field gets the behaviour by using the
    widget - never a second matcher, because two matchers are two case rules. The numbers and
    the rejected alternative are in [`optimization.md`](optimization.md) §13.

## 6. Icons and pictures

- **Rows = Nerd Font glyphs.** The current `UI_I_*` alphabet is Font Awesome codepoints, which
  foot cannot draw (its fallback is Symbols Nerd Font Mono — the documented fzf/welcome rule).
  Measured while building spec 003: this was too strong — foot does search fontconfig's dynamic
  fallback (`foot.ini(5)`), so a Font Awesome codepoint renders once `fonts-font-awesome*` is
  installed. The alphabet is Nerd Font anyway, for the reason that survives: a row's picture has
  to come from the one symbol font the image guarantees (`fonts-nerd-symbols`) and occupy one
  cell, not from whichever family answers.
  The framework carries an `enum Glyph` mapping the alphabet 1:1 to Nerd Font codepoints
  (Octicons/Codicons/Devicons/Seti/Powerline, from `fonts-nerd-symbols`). The table is
  **generated filtered, not hand-written**: a host tool (`scripts/make-glyphs.sh`, sibling of
  `make-emojis.sh`) checks every codepoint against the font's charset with `fc-query` and
  substitutes what does not exist. `check-system.sh` re-verifies per codepoint. The AGENTS.md
  alphabet rule becomes "a glyph in the Nerd Font alphabet".
- **Big surfaces = sixel.** foot renders sixels natively (the chafa path). `ratatui-image`
  draws the theme's 48 px PNGs (heroes, launcher rows) and the 128 px thumbnails from
  `~/.cache/thumbnails/normal/` (file previews — better than rofi, which never generated
  thumbnails). Protocol encoding cached per image; redrawn only when the frame repaints.
- `scripts/make-icons.sh` is untouched: it keeps feeding PNGs for toasts, launcher,
  pcmanfm and the sixel surfaces.
- **The spike (`rust/crates/spike-sixel`) settled the API**: foot 1.28 answers `Sixel` to
  `Picker::from_query_stdio()` with a 9x20 cell, and both a 48 px theme PNG and a 1366x912
  screenshot render at full size. Three facts the framework depends on: the fixed `Image`
  widget **silently draws nothing** when the protocol does not fit the area, so pictures use
  `StatefulImage` + `new_resize_protocol` (fit at render time); `Resize::Fit` only shrinks,
  so a hero that has to fill its card uses `Resize::Scale`; and `ratatui-image`'s default
  features want **`libchafa-dev`**, which sixel does not need — the crate is built with
  `default-features = false` (`crossterm` + `image-defaults`) and the build host stays free
  of it.

## 7. Keymap

- **No Alt chords inside the TUI** (sway evaluates its bindings before the focused surface —
  `plan-installer.md` §8). Letters for actions (footer hints), `Enter`, `Esc` closes,
  `Ctrl+C` stops, `Ctrl+L` toggles the log. F-keys exist (Search+top row) but are secondary.
- **Mouse complete** (see §5.11).
- **New cycle keys** (Omarchy's pattern, Alt where Omarchy uses Super; both verified free
  against the 55 existing bindings and sway's defaults, reserved in `config.d/` with the
  description comment `rofi-keys` shows):
  - `Alt+Shift+T` — cycle theme (Omarchy: Super+Shift+T);
  - `Alt+Shift+W` — cycle wallpaper of the active theme (Omarchy: Super+Ctrl+Space).
- The `ui_pick_alt` second actions become footer letters.

## 8. The TaskView and the task protocol (keep-in-the-loop)

Every slow action is a task with a declarative spec:
`{ id, label, icon, stop: Signal | Pattern, progress, result }`.

- **State** in `$XDG_RUNTIME_DIR/lazyos/tasks/<id>/` (the busy/tablet/vnc pattern):
  `state.json` written atomically (tmp + rename, `plan-installer.md` §6) with phase, percent,
  label, log path, exit code, result. **The runner is detached** (`setsid`, the
  `lazyos-install-run` pattern): closing the window does not kill the task;
  `lazyos task view <id>` re-attaches.
- **The window** shows: an animating preflight checklist (the "Cannot install yet" rows become
  checks that tick), braille spinner + label, a **real progress gauge** (rsync/yt-dlp
  percent), a **live, scrollable log tail** (replaces `lazyos-install-log`), and controls:
  `Ctrl+C` stops (only when the spec allows — the deliberately-not-stoppable list stands: apt,
  mkfs, the post-first-write copy, with the button disabled and the reason written) and `q`
  sends to background.
- **The result appears in the same window**: a Wi-Fi scan ends and the network list appears
  *inside* the task. No second window, no toast.
- **Bar compatibility**: the TaskView writes the same `<tag>.label/.stop/.icon` state files
  `bar-busy` reads (+ percent), so waybar works unchanged during the transition; later the
  `custom/busy` module reads task state directly.
- Tasks: install, wifi, bt, pkg, download, clean, grep, dns, format, swap, dictate
  (record → transcribe phases), profile export/restore, updates.

## 9. The notification daemon (phase 5)

`lazyos-notify` replaces mako 1:1 — same resident, better surface:

- **Layer-shell client** (top-right, overlay), software-rendered with **tiny-skia +
  cosmic-text** (no GTK4, no GPU; small cards repainted only on notification), theme PNGs
  decoded directly.
- Implements `org.freedesktop.Notifications` (notify-send and apps keep working) + extension
  hints: **progress with percentage and per-task color**, **real action buttons**, animations,
  own grouping; history in `$XDG_STATE_HOME/lazyos/notify/` read directly by the history mode
  (adios `makoctl`).
- Until then the `lazyos notify` bridge talks to whatever implements the spec (mako first),
  so the swap is invisible to the rest of the framework.
- Why not mako forever: it parses actions but renders no buttons, has no numeric progress or
  per-task colors, no fine animations, and its history DB is private.

## 10. The theme engine

Omarchy's patterns, copied concretely (from `omacom/omarchy` `docs/theming.md` and the manual):

| Omarchy pattern | Our copy |
|---|---|
| A theme = a directory whose `colors.toml` is the single source of truth (`background, foreground, accent, red, color0..color15`); everything else is *generated* | `~/.config/lazyos/themes/<name>/theme.toml`; built-ins in `/usr/share/lazyos/themes/` |
| Generation from `.tpl` templates with `{{ background }}`, `{{ accent }}`, `{{ color0 }}`…; user templates override built-ins; a hand-written file is **never overwritten** | `/usr/share/lazyos/themed/*.tpl` + `~/.config/lazyos/themed/`, same placeholders + derived semantic roles |
| 22 built-in themes | 8: **solarized** (the default, the brand — our Tokyo Night) + Catppuccin Mocha, Tokyo Night, Rose Pine, Nord, Gruvbox, Everforest, Kanagawa (~10 KB TOML each) |
| Selector with preview; cycle keys; wallpapers per theme (3-4, cycled) | `lazyos menu theme` (Style group of the root menu + a Settings row) with **live preview** — the TUI re-renders itself in the selected palette *before* applying; `Alt+Shift+T` / `Alt+Shift+W`; 2-3 wallpapers per theme, **gradient-generated** (the `omarchy-wsl2` make-wallpaper pattern) so 8 themes do not cost 200 MB; `mountains.jpg` stays the solarized wallpaper (brand rule) |
| Apply = generate + restart (`omarchy-theme-set`); headless escape hatch | `lazyos theme set X` → generate all surfaces → `swaymsg reload` + restart waybar + `makoctl reload` (fallback: re-exec mako); `--headless` for TTY/ssh |
| CLI (`list/set/current/next`, `omarchy-theme-bg-next`) | `lazyos theme list/set/current/next/wallpaper` |
| Make your own: copy a directory and edit `colors.toml` | identical |
| Dynamic from wallpaper (matugen → Material You → colors.toml, pywal/wallust style) | later phase: `lazyos theme from-wallpaper IMG` — the `theme.toml` interface makes it an addition, not a redesign |
| Per-theme unlock screens | our equivalent: the **greeter** + swaylock + wallpapers, themed per theme |
| Community themes, install by URL (`omarchytheme.com`, 355 themes) | `lazyos theme install <dir|url>` — one directory is the format; safety: only TOML + images are fetched and validated, **never executed** |

- **Semantic roles**: `theme.toml` = base palette (16 colors, Omarchy's model) + derived roles
  (bg, bg-elevated, border, border-focus, text, strong, dim, accent, danger, warn, good, info,
  selection) + explicit overrides. The design system's tokens are filled from these.
- **Motion token**: `motion = full | reduced | off`, and a battery rule — below 20 % on
  battery, animations step down automatically.
- **Surfaces generated** (today the look lives in 6+ hand-maintained files; the engine
  produces them all from `theme.toml`, atomically — write `.new`, `mv` — and idempotently):
  the framework TUI (native read at open; the selector re-renders live), waybar
  `style.css.tpl` (the current file becomes the solarized template), foot `foot.ini.tpl`
  (alpha 0.92 and the font chain stay untouched), mako `mako.ini.tpl` (until the daemon),
  rofi `config.rasi.tpl` (until rofi dies), sway `90-theme.conf.tpl` (client.* colors), GTK3
  and GTK4 `gtk.css.tpl` ×2, wvkbd flags (the colors hardcoded in `tablet-mode` become theme
  reads), glow `glow-style.tpl` (the `#` rule preserved), starship `starship.toml.tpl`, kitty
  `kitty.conf.tpl`, the greeter's `--theme` flags, swaylock colors + per-theme wallpaper.
- **Shipped TUIs get themed too** (Omarchy generates `btop.theme` and `neovim.lua` for exactly
  this reason): four more templates — `btop.theme`, `lazygit/config.yml` (theme section), a
  generated nvim colorscheme `.lua`, and `FZF_DEFAULT_OPTS` colors. A terminal session then
  matches the desktop in every tool; that coherence is the point.
- **The switch is a moment**: on apply, a short tachyonfx sweep through the palette and a
  themed toast naming the new theme; a bar module (an accent dot; click = selector) marks the
  active theme.
- **Out of theme scope (brand rule)**: the idle screen art (`screensaver.txt` is the mark),
  the LazyOS mark, the font chain; v1 keeps the icon theme as-is (note: a neutral PNG set for
  light themes is a later decision).

## 11. Complete inventory (what becomes what)

Every entry point survives: the ~55 bindings, the 4 gestures, the 2 bindswitch lines, the idle
chain, the 13 waybar modules, the 30 root-menu rows and the 31 `.desktop` entries keep their
paths (symlinks). The 64 scripts and 5 rofi built-ins become modes; 12 planned features become
generic table modes.

| Kind | Modes |
|---|---|
| `lazyos menu` (pick) | menu (command palette), files (sixel preview), find (async walk + preview — the framework *is* the fuzzy finder, television-style; fzf and the old "not packaged" objection die), grep, settings, power, media, notify, clip, emoji, calc (own parser), keys, learn, about (bento), colour, tz, dns, vnc, places, kill, pkg, dev, agents, display, capture, qr, layout, ssh, **launcher** (drun: `.desktop` parsing + sixel icons + `Terminal=true`→foot + frecency), **window** (switcher via sway IPC), **run** (PATH runner, Shift+Enter→foot), password, theme |
| `lazyos task` | wifi, bt, pkg, download, clean, dns, format, swap, install, dictate, profile, updates |
| `lazyos form` | remind, download, dev, swap-undo, agents-account, format-fs |
| `lazyos osd` | volume/brightness up/down/mute (notify protocol) |
| `lazyos act` | lock, suspend, hibernate, screenshot, night-light, dp-resume, tablet on/off/calibrate, view, install-log, welcome, camera |
| `lazyos table` (generic spec) | snippets, pass, units, worldclock, weather, speedtest (the 12 planned features) |
| `lazyos notify` | title/body/icon/progress/buttons (daemon protocol) |

Stays as-is (not TUI territory): waybar (themed), imv/papers/mpv/guvcview/pcmanfm/wvkbd/
wdisplays, swaylock/swayidle/tte (art surface), firefox, foot.

## 12. Outstanding surfaces (the look-and-feel claim)

A. **First impressions**
1. **`lazyos-greeter`** — the greeter becomes the first framework window: a greetd session
   running the TUI fullscreen in foot (`greetd_ipc` crate; the greeter is a TUI, so hero,
   themes, wallpapers, clock and mouse all apply). Replaces `tuigreet` 0.9.1 (no TOML, no
   animation) — the ugliest moment of the boot becomes the best one. No new resident: greetd
   already spawns a greeter.
2. **Onboarding** — the first-boot welcome becomes a bento + **theme carousel**: pick the
   theme before the desktop is seen.

B. **Daily drivers**
3. **The unified launcher (command center)** — one input that answers apps + files + **calc
   inline** (result under the query as you type) + web search + our actions + **Ask (AI)** +
   frecency ranking. Every piece exists as a mode; the launcher is the frame. This is the
   feature that reads as "this distro is different".
4. **Bar elevation** — floating pill bar (margins, rounded per-module cards), plus the modules
   Omarchy has and we lack: active window title, **MPRIS now-playing card with album art via
   sixel** (`mpris:artUrl`; playerctl + mpv-mpris already ship), **weather card** (one
   `wttr.in` call as a card), **calendar popup** on the clock. Each popup is a framework
   window, not a waybar tooltip.
5. **Capture post-actions** — after a screenshot, a small card with the thumbnail (sixel) +
   annotate/copy/save/OCR, over the tools that already exist.

C. **Coherence**
6. Theme templates for btop/lazygit/nvim/fzf (§10).
7. Animated theme switch + bar theme dot (§10).
8. Community themes: `lazyos theme install <dir|url>` (§10).

D. **Feel**
9. Motion tokens + power-aware motion (§10).
10. **Sound design** (optional, off by default): 3-4 CC0 sounds in `/usr/share/sounds/lazyos`,
    a `lazyos sound` helper over `paplay` (already in `pulseaudio-utils`), wired to theme
    switch, notification arrive (with the daemon), capture, dictation start/stop. One Settings
    row. Zero new packages, zero residents.

E. **Optional moonshots (flagged, not committed)**
- Custom lock screen (clock + media on lock, hyprlock-style) — a real project with a security
  surface; meanwhile swaylock gets per-theme wallpapers + colors.
- which-key style hold overlay (press and hold `Alt+Ctrl+K`) — cheap once `keys` is a mode.

Deliberately **not** added: power-profiles, indexers, GTK4 shells, animated wallpapers, a
second terminal — each violates a standing rule and the gap docs already answered "no".

## 13. AI in the loop (OpenCode as the default harness)

The image already ships OpenCode (`~/.opencode/bin`, signed out, managed by the Agents menu), so
it is the default harness by construction: free, OSS, and the same login every other agent
surface uses. Two standard interfaces, zero changes to OpenCode itself:

- **`lazyos mcp`** — a local stdio MCP server, registered in `~/.config/opencode/opencode.json`
  as `type: local`. OpenCode spawns it when a session starts and kills it when the session
  ends — the MCP lifecycle gives the nothing-resident rule for free. The AI's hands on the
  desktop are MCP tools.
- **Chat and Ask surfaces** — `lazyos menu ask` (a chat window) and the launcher's Ask strip
  speak to `opencode serve` over its local HTTP API; the server is spawned on demand and torn
  down when the window closes. The conversation renders in our design system — markdown, code
  blocks, icons, hero — not in a bare terminal.

### 13.1 The three channels

| Channel | Interface | What it gives the AI |
|---|---|---|
| **hands** | `lazyos mcp` tools | act on the desktop; every effect is a window the person sees |
| **eyes** | `desktop.screenshot` / `desktop.region` (grim), `desktop.ocr_region` (tesseract), `desktop.windows` (sway IPC), `system.status` | see the screen as an image, read text off it, know windows/state |
| **voice** | `menu ask` (chat window), the launcher's Ask strip, dictation → Ask, `lazyos ask "…"` (TTY/ssh, text answer) | the person talks to the AI from every surface |

### 13.2 The permission tiers (the contract)

| Tier | Meaning | Enforced at |
|---|---|---|
| **read** | no change; text, JSON or an image path back | tool boundary |
| **act** | visual, reversible, no data loss; executes and the result shows | tool boundary |
| **propose** | opens the real, gated window; the person confirms (types the token, picks the row, presses Enter) | the window's own gate — the AI never sees the token |
| **refuse** | the tool does not exist; the instructions tell the AI which menu to point at | the manifest: `lazyos mcp --tools` never lists it |

### 13.3 The complete tool manifest

Every tool is a thin wrapper over the same mode handler the menus call — one implementation
per action, never a second copy. Tiers per §13.2; "visual result" is what the person sees.
The names below are the concepts; the served names use `_` (`desktop_status`), because the
model providers accept `[A-Za-z0-9_-]` in a tool name (spec 008), and phase 2 ships the cut
spec 008 lists.

**Desktop & windows**

| Tool | Tier | Visual result |
|---|---|---|
| `desktop.screenshot` | read | PNG path back (the eyes) |
| `desktop.region` | read | region PNG (the person draws the area) |
| `desktop.ocr_region` | read | the text on the screen, back as text |
| `desktop.windows` | read | tree: workspaces, titles, focus |
| `desktop.focus <win>` | act | the window focused |
| `desktop.close_focused` | act | the window closes (Alt+Q) |
| `desktop.workspace <n>` | act | workspace switch |
| `desktop.launch <app>` | act | the app opens |
| `desktop.float_toggle` | act | focused window floats |

**Appearance — the visual-est domain**

| Tool | Tier | Visual result |
|---|---|---|
| `theme.list` / `theme.current` | read | — |
| `theme.set <name>` | act | full re-theme: sweep + toast naming it |
| `theme.preview <name>` | act | the selector open, live-previewing that theme |
| `wallpaper.next` | act | wallpaper cycles |
| `nightlight.on/off/status` | act | the screen warms |
| `screen.transform <deg>` | act | rotation |
| `display.status` | read | monitors, modes, arrangement |
| `display.open` | act | arrangement menu (the person arranges) |

**Files**

| Tool | Tier | Visual result |
|---|---|---|
| `file.open <path>` | act | the file opens in its viewer (lazyos-view) |
| `file.browse <dir>` | act | files picker at that directory |
| `file.search <query>` | act | search results picker |
| `file.reveal <path>` | act | pcmanfm at the path |
| `grep.search <pattern>` | act | matches task → picker, hit opens at its line |
| `file.actions <path>` | act | the row-actions menu for that file |

**Media & audio**

| Tool | Tier | Visual result |
|---|---|---|
| `media.status` | read | — |
| `media.play/pause/next/prev` | act | OSD / media card |
| `volume.set <n>` / `volume.mute` | act | the OSD bar |
| `sinks.open` | act | audio output picker |
| `bluetooth.scan` | act | task window → device list |
| `bluetooth.connect <dev>` | propose | the picker; pairing is the person's |

**Network**

| Tool | Tier | Visual result |
|---|---|---|
| `wifi.status` | read | — |
| `wifi.scan` | act | task window ends in the network list |
| `wifi.connect <ssid>` | propose | the picker opens; the password prompt is typed by the person |
| `dns.status` | read | — |
| `dns.open` | act | resolver picker |
| `qr.show` | act | the Wi-Fi QR window |

**System**

| Tool | Tier | Visual result |
|---|---|---|
| `system.status` | read | JSON: cpu/mem/battery/disk/theme/uptime |
| `system.about` | act | the About bento |
| `system.processes` | act | the kill picker (the person picks) |
| `system.packages <q>` | act | package picker with the query |
| `pkg.install <name>` | propose | picker pre-selected; the person confirms apt |
| `updates.check` | read | count |
| `updates.open` | propose | the updater task window |
| `clean.open` | act | Free space task (its delete rows are the person's) |

**Capture & camera**

| Tool | Tier | Visual result |
|---|---|---|
| `capture.screen` | act | post-actions card with the thumbnail |
| `capture.ocr` | read | region → text back |
| `capture.record_start/stop` | act | the recording indicator |
| `camera.open` | act | the camera window |

**Text, knowledge & sessions**

| Tool | Tier | Visual result |
|---|---|---|
| `clipboard.read` / `clipboard.copy <text>` | read / act | — / copied card |
| `clipboard.history` | act | history picker |
| `emoji.find <q>` | act | emoji picker |
| `translate <text>` | act | the result card |
| `calc <expr>` | read | the number back |
| `docs.search <q>` | act | Learn picker |
| `keys.show` | act | the key view |
| `password.generate` | act | a password in the clipboard (card) |
| `reminder.create` | propose | the form pre-filled; the person confirms |
| `timezone.open` | act | timezone picker |
| `notify.send <title> <body> [icon]` | act (rate-capped: 3/min) | a card — windows over toasts is the instruction |
| `menu.open` / `settings.open` / `agents.open` | act | the palette / switches panel / agents menu |
| `ssh.connect <host>` | act | a terminal with ssh |
| `session.lock` | act | the lock screen (visible, safe) |
| `screensaver.start` | act | the idle art |
| `tablet.toggle` | act | tablet mode on/off, on-screen keyboard |

**Propose-only and refused**

| Tool | Tier | Why |
|---|---|---|
| `install.open` | propose | opens the gated task window; the typed token is still typed by the person |
| `power.propose` (log out / reboot / shut down) | propose | the Power confirm opens with Cancel selected; an AI never ends the session, the person's second answer does (spec 008: propose, not refuse) |
| `format.propose` / `swap.propose` | propose | the disk menu opens; its list and its Cancel-first question are the gate (spec 008) |
| `format.apply` / `swap.apply` / `power.apply` | refuse | never exposed; the apply step lives in the menu |
| `profile.*` | refuse | the passphrase is the person's, read on their terminal |
| `run <cmd>` | **not exposed** | the agent's own shell session is its command line; no second exec surface |

### 13.4 The visual result loop

1. A tool call renders in the chat as an **action card**: glyph + label + what changed
   (`theme.set catppuccin` → "the desktop re-themed · Catppuccin").
2. The effect opens or updates the framework window the person watches.
3. Where it pays (theme changes), the card can carry a **before/after sixel thumbnail** via
   `desktop.screenshot` read-back — the conversation is visual, not just the desktop.
4. Tool results include "what is now on screen" (which window opened), so the AI's next action
   is grounded in the state it actually produced.

### 13.5 Where the person talks to the AI

Ask strip (launcher), `menu ask` (streaming chat), dictation → Ask (voice), `lazyos ask "…"`
(ssh/TTY), and the root menu's AI group (agent session / agents / dictation). One brain
(OpenCode), five doors.

### 13.6 The checkable contract

`lazyos mcp --tools` is the manifest, and it is what the checks read, not prose:
- **no tier leak**: nothing apply-level destructive ever appears (refuse set is absent from
  the manifest);
- **coverage**: every `lazyos menu X` / `lazyos task X` mode has at least one corresponding
  tool — the AI can reach everything the person can (the one deliberate exception: `run`);
- the manifest version is pinned in the instructions file (`~/.config/opencode/
  instructions/lazyos.md`), so the agent's vocabulary and the tools cannot drift;
- the notify rate cap exists in the manifest description.

New crate: `lazyos-ai` — the `opencode serve` client, the chat widget, and the MCP server
(`rmcp`, pure Rust). No new resident: the serve process lives only while an AI window is open,
the MCP server only inside an agent session. Credentials stay in `~/.local/share/opencode`
(already excluded from published artifacts by `lib-dist.sh`).

## 14. Performance budget (measured, not promised)

- Event-driven redraw; tick at 10-30 fps **only** while a spinner/animation/progress is live;
  tachyonfx effects ≤300 ms and never loop.
- Target: the `lazyos` process ≤ rofi's measured 15-17 MB PSS; a window ≈ 40 MB with foot;
  0 after close. Measured with the `optimization.md` method and watched by `check-system.sh`.
- Sixels: protocol encoding cached per image; thumbnails already cached by
  `lazyos-thumbnail` (reused as-is). **Measured on the device** (spike, release build, foot
  1.28, 150x45 cells): the first frame carrying a 1366x912 picture takes **169 ms** (resize +
  quantize + encode + write), and every later repaint of the same frame **1-2 ms**, because
  the cached protocol is an unchanged cell and ratatui's diff skips it. So a ticking UI is
  free once a picture is up, and the one visible cost is opening a picture — which is where
  a pre-scaled 128 px thumbnail (or encoding off the UI thread) keeps the window responsive.
- Tablet mode: layout tokens enlarge rows/buttons when the tablet state dir is present
  (touch-first pickers, no daemon).
- **Ratatui stays; a "ratatui-lite" was evaluated and rejected on measurement.** On this device,
  against a crossterm-only app doing the same job: +23.6 s of clean build, +89 MB of `target/`,
  +328 KB of stripped binary, **no startup difference** (23 ms both, the `script` floor) and
  **233 kB of PSS either way** - a text-only ratatui app inside foot is 1.0 MB PSS next to foot's
  21 MB and a picture's 13.3 MB. A frame at 152x45 costs **154 µs empty** (ratatui's reset + diff
  floor), 231 µs with one changing cell and 445 µs when 43 rows are `format!`ed again, i.e. 0.15 %
  of a core at the plan's 10 fps tick. A home-grown renderer would save that and own Unicode width,
  wide cells, wrapping, the `Layout` solver and terminal quirks - the class of silent rendering bug
  this port keeps fixing. The two rules that follow are the ones to enforce: **event-driven redraw
  (10 fps only while something moves)** and **the widget tree is built per state, not per frame**.
  Feature-pruning (`default-features = false`) measured 0 s / 0 KB of gain, and thin LTO + one codegen
  unit -9 % of binary for +13 s of build, so neither is adopted. The measured table and method are
  `optimization.md` §13.
- **Image codecs are opt-in, not default**: `image` with its default features is 147 s of build,
  272 MB of `target/` and 2.17 MB of binary for decoders nothing opens; `png` + `jpeg` is 49 s,
  49 MB and 0.73 MB. The spike's clean build went 228 s → 115 s and its binary 8.6 → 2.21 MB
  (1.58 MB stripped) with that one change. A codec is added when a real file needs it, and the
  commit says which file.

## 15. Witnesses

- Each ported tool: the old path becomes a symlink → `/usr/bin/lazyos`; its `check-system.sh`
  line asks for the binary + the subcommand + the mode's shape rule (a task declared stoppable
  has its stop; a destructive confirm has its gate).
- New framework lines: foot sixel capability, `fonts-nerd-symbols` present, every alphabet
  codepoint in the font's charset (`fc-query`, the prompt-check method), the `for_window`
  rules per app-id, the cycle bindings, the greeter session entry.
- Theme lines: the active theme exists, `lazyos theme set` regenerates **idempotently** (a
  regeneration diff must be empty — a hand-edited file is caught instead of silently breaking),
  each generated surface carries the active palette, the 8 built-ins ship, the default is
  solarized. The report prints the active theme as an info line.
- AI lines: `lazyos mcp --tools` lists no apply-level destructive tool and covers every
  `menu`/`task` mode (the manifest is read, not assumed), the manifest version matches the pin
  in the instructions file, the `opencode.json` MCP registration exists, the instructions file
  ships, the Ask strip rows exist in the launcher and the root menu, no stray `opencode serve`
  survives a closed ask window, and `grim`/`tesseract` are present for the eyes. Manual:
  `07-agents.md` § *The desktop the agent can drive*.
- Manual paragraph + `AGENTS.md` bullet per feature, as always; the AGENTS.md alphabet rule is
  updated to the Nerd Font alphabet.

## 16. Roadmap

Each numbered phase below is one or more **specs** in [`../specs/`](../specs/README.md): the
scope, acceptance criteria, tests, three witnesses and the `opencode go` model that builds it.
One branch (`spec/NNN-slug`) and one PR per spec; [`specs/README.md`](../specs/README.md) holds
the workflow and the routing.

1. **v0.4.0** — installer TUI fullscreen (already planned): first TaskView + typed-gate
   confirm + splash/hero.
2. **Framework core + first loop** — `lazyos-ui` (design system, glyphs, list, launch, notify
   bridge), `lazyos-theme` with **solarized as theme #1** and all surface generators,
   `lazyos` dispatch; ports: root menu (command palette + Ask strip), TaskView, wifi/bt/pkg
   (+ download/clean/grep/dns). The loop closes where it hurts most. AI lands here too:
   **`lazyos mcp`** + the `opencode.json` registration + the instructions file, and the Ask
   strip v1 (single-shot `opencode run` rendered in a task window).
3. **Pickers** — files with sixel preview, settings, power, media, notify, clip, emoji, calc,
   learn, about (bento), keys, colour, tz, vnc, places, kill, agents, ssh, capture (+
   post-actions popup), qr, dev, remind, display, swap, format; theme selector with live
   preview; cycle keys.
4. **rofi closure** — launcher command center, window switcher, run, find; welcome onboarding;
   rofi leaves the image. AI surface upgrades here: **`menu ask` chat with streaming** via
   `opencode serve`, the `desktop.screenshot` vision loop, and dictation → Ask.
5. **v0.5.x** — `lazyos-notify` daemon (mako leaves), `lazyos-greeter`, the 7 extra themes +
   gradient wallpapers, bar elevation (MPRIS/weather/calendar), theme templates for
   btop/lazygit/nvim/fzf, `lazyos check`, `theme install`; then the host migration of
   [`plan.md`](plan.md).
6. **Later** — `theme from-wallpaper` (matugen-style), sound design (if enabled), custom lock
   and which-key overlay (if un-parked).

**First technical step (the only spike)**: done — `rust/crates/spike-sixel` rendered a 48 px
theme PNG and a document picture via sixel in real foot on the device (commit `spike: …`),
with the timings in §14. `ratatui-image` 11.1.0 + `ratatui` 0.30.2, no chafa. The half-blocks
fallback stays in `ratatui-image` and is what any other terminal gets for free.

## 17. Risks and mitigations

| Risk | Mitigation |
|---|---|
| sixel in foot | **validated on hardware** (spike: both pictures render, 169 ms first frame, 1-2 ms cached repaint); half-blocks fallback works in any terminal |
| sway steals Alt chords | zero Alt chords inside the TUI; footer letters; checked by design |
| theme switch breaks a surface silently | idempotent regeneration diff is the witness; atomic writes |
| old paths vs new binary | symlinks + dispatch by argv[0]; `check-system.sh` `-x` + dangling-link checks |
| notification daemon scope | last phase; mako stays the spec-compliant fallback; the bridge decouples |
| greeter is a security surface | greetd already sandboxes the greeter process; ours is a dumb client of `greetd_ipc` |
| performance regression | measured PSS/tick budget in §14, watched by the report |
| the AI drives the desktop | tools are read/safe only; destructive is propose-only at the tool boundary; the `--tools` manifest check fails a regression |
| `opencode serve` left resident | spawned on demand by the ask window, killed on close/idle; `check-system.sh` watches for a stray serve |

## 18. Open questions

- Community theme install by URL: fetch-and-validate now or directory-only first?
- Sound design: ship the 3-4 sounds in v0.5 or gate behind the "optional" flag until asked?
- The unified launcher's Ask strip: keyword routing offline (v1) vs opencode invocation (v2)?
- The MCP v1 manifest is specified in §13.3; the open question is only the cut: which tools
  ship with phase 2's first cut vs the phase 4 upgrade — and whether `desktop.screenshot`
  requires a vision-capable model before it is exposed, or always ships and returns the path.
- The ask window's model/provider comes from the user's own OpenCode login; no model ships in
  the image (size). Confirm that "free harness" means the harness, not a bundled model.
- Neutral icon set for light themes: when?
- Custom lock and which-key overlay: un-park decision gate.

## 19. References

- [`plan.md`](plan.md) §3/§5 — Rust-first policy, the migration tiers, the multi-call shape.
- [`plan-installer.md`](plan-installer.md) §8 — the TUI's key-collision and idle-chain constraints.
- [`omarchy-gap.md`](omarchy-gap.md) §5 and [`plan-rofi.md`](plan-rofi.md) — the 12 planned
  features and batch history.
- [`optimization.md`](optimization.md) — measurement method and device budget.
- Ratatui 0.31 (core/widgets/macros), `tachyonfx`, `ratatui-image`, `throbber-widgets-tui`,
  `tui-big-text`; apps: yazi, lazygit, gitui, bottom, atuin, television.
- Omarchy theming: `omacom/omarchy` `docs/theming.md`, the Omarchy manual (Themes, Making
  your own theme), `omarchytheme.com`, `omarchy-auto-theme` (matugen).
- OpenCode (`tools/opencode/`): `opencode serve` HTTP API, MCP `type: local` config in
  `opencode.json`, instructions files; `rmcp` (Rust MCP SDK); `greetd_ipc` (Rust greetd
  protocol client).
