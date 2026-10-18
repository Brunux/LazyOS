# PROVENANCE — the vendored Omarchy tree

`uis/omarchy/files/usr/share/omarchy/` is a **curated subset** of upstream Omarchy,
vendored so the port is reproducible and the image can be built offline.

| Field | Value |
|---|---|
| Upstream | <https://github.com/omacom/omarchy> |
| Branch | `quattro` |
| Commit | `da2db1a2fe022089165b9edca3326886ad78dae2` |
| Commit date | 2026-10-18 |
| `version` | `4.0.0.alpha` |
| License | MIT |
| Installed at | `/usr/share/omarchy` (upstream's `$OMARCHY_PATH` default) |

## What is vendored

| Tree | Why |
|---|---|
| `config/` | Upstream's stock configuration, the source `omarchy-refresh-config` copies into `~/.config`. |
| `shell/` | The Quickshell desktop shell (bar, launcher, notifications, OSD, lock screen). |
| `applications/` | `.desktop` entries and icons. |
| `etc/` | System-level files (units, sysctl, udev, xkb, sudoers, portals). |
| `bin/` | The `omarchy-*` commands. Vendored whole as reference; the port ships only the subset the desktop reaches (see `README.md`). |
| `default/` | The runtime defaults Hyprland loads by path (`default/hypr/bootstrap.lua`, `default/hypr/omarchy*.lua`), the home seed, the Wayland session entry and UWSM/environment defaults. |
| `themes/` | All theme definitions (`colors.toml`, `*.theme`, `icons.theme`, previews) **and the Tokyo Night backgrounds only** — Tokyo Night is Omarchy's default theme (`install/user/theme.sh`). |
| `version` | `4.0.0.alpha`. |
| `icon.png` | The tree root icon, which `default/chromium/extensions/copy-url/icon.png` symlinks to. |

## What is deliberately excluded

| Excluded | Reason |
|---|---|
| `themes/*/backgrounds/` except `tokyo-night/` | ~164 MB of videos/images; only the default theme's backgrounds ship. Fetch a theme's backgrounds from the pinned commit if a wallpaper is wanted. |
| `default/i18n/<lang>/` except `en/` | ~8 MB of translations; the shell loads `en/shell.json` + `locales.json`, and other languages fall back to English. |
| `install/` | The Arch installer/provisioner (pacman, SDDM, LUKS, platform setup). This port does not run it. |
| `migrations/`, `manual/`, `test/`, `docs/`, `agents/`, `plans/`, `.github/` | Not part of the desktop; documentation, tests and history of upstream. |

Excluded on purpose, not forgotten: `docs/plan-omarchy.md` §4 records what is dropped
and why, and `README.md` lists the same from the UI's point of view.

## Refreshing from upstream

The tree is copied from a `git clone --depth 1 --filter=blob:none` with a sparse
checkout of the trees above. To move to a newer `quattro` commit: re-clone, record the new
commit here, copy the same trees (same exclusions), and update the pin in
`docs/plan-omarchy.md` and `README.md`. Anything this port changes is kept as a patch in
`patches/`, never as a silent edit to the vendored files.
