# Spec 017 — Remaining Interactive Surfaces: Network, Places, Cleaner, Agents, and Utilities

- **Status**: draft
- **Phase**: 4
- **Depends on**: 007, 010, 012
- **Model**: `claude-sonnet-5-5` (effort `high`) — multi-surface interactive port covering NetworkManager IPC/nmcli, process signals, safety confirmations, disk reclamation, and agent model switching. Fallback `claude-opus-5-5`.
- **Branch**: `spec/017-remaining-surfaces`
- **Plan**: [`docs/plan-ui.md`](../docs/plan-ui.md) §16 phase 4 (the final rofi replacement surfaces before closure)
- **Old paths**: `bar-net`, `rofi-agents`, `rofi-places`, `rofi-clean`, `rofi-kill`, `rofi-download`, `rofi-layout`, `rofi-file-actions`, `translate`, `dictate`

## Problem

While 35 surfaces have migrated to native Rust/Ratatui (`lazyos`), 9 user-facing scripts still call `rofi -dmenu` and source [`lib-rofi.sh`](../uis/sway/files/home/.config/bin/lib-rofi.sh). Because of these remaining scripts, the `rofi` package cannot yet be removed from `uis/sway/packages.list`, keeping rofi's styling files, layer-effects configuration, and Pango glyph dependencies in the shipping OS. These remaining scripts also lack the unified theme, case-folded filtering, Nerd Font single-advance glyphs, and responsive TaskView progress reporting of the new framework.

## Scope

**In:**

- **`lazyos menu net` (Wi-Fi & Network Manager)**:
  - Replaces `bar-net`.
  - Scans available Wi-Fi networks using `nmcli dev wifi list`.
  - Displays SSID, signal gauge, security status, and active connection indicator.
  - Interactive password prompt via `Input` widget for secured networks.
  - Toggles Wi-Fi radio and airplane mode.
- **`lazyos menu agents` (AI Agent Selector)**:
  - Replaces `rofi-agents`.
  - Lists configured agents (Claude Code, OpenCode, local Ollama models) with active default indicator.
  - Selecting an agent updates `~/.config/lazyos/agents/default` and OpenCode configuration.
- **`lazyos menu places` (Bookmarks & Disks Jump)**:
  - Replaces `rofi-places`.
  - Lists user directories (`~/Downloads`, `~/Documents`, `~/Pictures`, `~/Videos`, `~/Music`).
  - Lists mounted and mountable block storage devices (internal eMMC/SD, external USB sticks) with filesystem usage and mountpoints.
  - Enter opens path via `lazyos open` (or `favnyr` for directory window).
- **`lazyos menu clean` (Interactive Disk Cleaner)**:
  - Replaces `rofi-clean`.
  - Scans reclaimable sizes: apt cache (`/var/cache/apt/archives`), systemd journal logs (`journalctl --disk-usage`), thumbnail cache (`~/.cache/thumbnails`), and user trash (`~/.local/share/Trash`).
  - Toggleable category selection and confirmation card before deletion.
- **`lazyos menu kill` (Process Manager & Killer)**:
  - Replaces `rofi-kill`.
  - Lists current user processes with PID, command name, CPU%, and memory usage.
  - Filter as you type; Enter sends `SIGTERM`, `Shift+Enter` sends `SIGKILL`.
- **`lazyos menu download` (Media Downloader)**:
  - Replaces `rofi-download`.
  - Prompts for video/audio URL using `Input` widget (auto-populated if clipboard contains a valid URL).
  - Prompts for download profile (video vs audio-only).
  - Spawns background download task running `yt-dlp` into `~/Downloads/`.
- **`lazyos menu layout` (Keyboard Layout Switcher)**:
  - Replaces `rofi-layout`.
  - Lists supported keyboard layouts (`us`, `es`, `latam`, etc.) and applies choice via Sway IPC (`swaymsg input type:keyboard xkb_layout <layout>`).
- **`lazyos menu file-actions` (Contextual File Actions)**:
  - Replaces `rofi-file-actions`.
  - Accepts a path argument and presents secondary actions: copy absolute path, rename, trash, open with terminal, or reveal in file manager.
- **`lazyos menu translate` & `lazyos menu dictate`**:
  - Replaces `translate` and `dictate`.
  - Interactive language selector and prompt input for translation; recording and model trigger for dictation.
- **Symlinks & Legacy Path Support**:
  - All 9 legacy script names under `~/.config/bin/` become symlinks to `/usr/bin/lazyos`.
  - Commands added to `command.rs` dispatch table and `lazyos --symlinks`.

**Out:**

- **The eMMC installation wizard**: moved to dedicated Spec 018 (`018-tui-installer.md`), which builds a full TUI installer with user setup, password, location/timezone, and TaskView progress bars.
- Removal of `rofi` package from `packages.list` and deletion of `lib-rofi.sh` (this is the scope of spec 014, which enforces closure and ledger verification).
- Web browser toolbar Marionette integration (spec 015).

## Design

- **Network Picker (`menu net`)**:
  - Runs headless query mode `lazyos menu net --status` to provide waybar `custom/net` with compact SSID/signal payload.
  - In-window mode renders an interactive `ListState` table with signal bars (` ▂▃▄▅`), lock glyphs for encrypted SSIDs, and an active radio toggle row at top.
  - Password prompt uses `lazyos_ui::widgets::Input` with masked character display (`*`).
- **Places & Drives (`menu places`)**:
  - Queries `gio` mount points and `/proc/mounts`.
  - Partitions show disk label, filesystem type, total/used capacity gauge, and mount status.
  - Removable devices offer an explicit unmount action via `udisksctl unmount -b <device>`.
- **Cleaner (`menu clean`)**:
  - Safe discovery: calculates bytes without modifying disk state until explicit user confirmation.
  - Each reclaimable target has an estimated reclaimable size in MB/GB and a checkbox toggle.
- **Process Killer (`menu kill`)**:
  - Reads `/proc/[0-9]*/stat` and status for user processes.
  - Formats rows with PID, process name, memory footprint, and CPU activity.

## Acceptance criteria

- [ ] `lazyos menu net` lists visible Wi-Fi networks with signal strength and encryption status; selecting an unconfigured protected network prompts for password and connects via `nmcli`.
- [ ] `lazyos menu net --status` prints formatted status JSON suitable for waybar polling without opening a window.
- [ ] `lazyos menu agents` displays available agents, marks the current active default, and selecting a new agent persists choice to configuration.
- [ ] `lazyos menu places` lists standard folders and connected external drives; selecting a folder opens it in file browser, selecting an unmounted drive mounts it via udisks2.
- [ ] `lazyos menu clean` shows calculated reclaimable space for apt, journals, thumbnails, and trash; confirms before purging selected caches.
- [ ] `lazyos menu kill` lists running processes; typing filters by process name, and selecting a process terminates it with `SIGTERM`.
- [ ] `lazyos menu download` prompts for URL (auto-fills from clipboard), prompts format, and spawns download task in `~/Downloads`.
- [ ] `lazyos menu layout` lists keyboard layouts and switches the active Sway keyboard mapping on selection.
- [ ] compatibility: all 9 legacy paths (`bar-net`, `rofi-agents`, `rofi-places`, `rofi-clean`, `rofi-kill`, `rofi-download`, `rofi-layout`, `rofi-file-actions`, `translate`, `dictate`) exist as symlinks to `lazyos` and dispatch to their corresponding commands.
- [ ] witness: `scripts/check-system.sh` verifies every ported surface exists in `lazyos --symlinks` and `lazyos --help`, and passes behavioral tests for `menu net --status`, `menu clean --dry-run`, and `menu places --rows`.
- [ ] witness: a paragraph in `uis/sway/files/usr/share/doc/lazyos/02-rofi.md` documents the migrated interactive surfaces under their `lazyos menu <subcommand>` paths.
- [ ] witness: an `AGENTS.md` bullet documents the completion of the interactive surface migration and the retirement of `rofi -dmenu` workflows.
- [ ] unit tests: network output parser, process table parser, cleaner target calculation, and places drive detection are covered by `cargo test`.

## Tests

- **Unit** (`cargo test`):
  - `parse_nmcli_wifi`: parses tabular nmcli output into structured SSID records with signal and security.
  - `parse_proc_stat`: parses `/proc` stat files into process entries with memory calculations.
  - `cleaner_reclaim_calc`: validates size calculations for mock directory trees.
- **Snapshot**:
  - `lazyos-ui` golden snapshot tests for `menu-net`, `menu-places`, `menu-clean`, and `menu-kill` at 80x24 and 152x45 terminals.
- **Integration**:
  - Verify `lazyos menu net --status` outputs valid waybar JSON.
  - Verify `lazyos menu layout` emits valid `swaymsg` IPC commands.
- **Hardware**:
  - Connect to WPA-PSK Wi-Fi network from `lazyos menu net` on Hayato Chromebook.
  - Mount/unmount USB thumbdrive from `lazyos menu places`.
  - Reclaim package cache from `lazyos menu clean`.

## Risks

- **Password exposure in terminal logs** — Wi-Fi WPA keys and passphrases must be handled strictly in memory via masked `Input` fields and never logged or serialized to disk.
- **Accidental process termination** — `menu kill` must require explicit Enter with confirmation for non-user system processes.
