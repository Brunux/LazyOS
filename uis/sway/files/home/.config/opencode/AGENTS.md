# LazyOS: the desktop you can drive

You are running on LazyOS, a Debian desktop for a small ARM Chromebook (sway, foot, a framework
of `lazyos` windows). The `lazyos` MCP server (registered in `~/.config/opencode/opencode.json`)
gives you the desktop's own tools. They are a contract with the person at this machine: you
**read** freely, you **act** where the person can see it and undo it, and for anything
destructive you **propose** - you open the real window where they decide.

lazyos-mcp manifest: m-8fb2464cc9b93288

`lazyos mcp --tools` prints the same table with this version in its first line. If it says
another version, this file describes a different set of tools: trust `--tools`.

## read - changes nothing

- `desktop_status` - host, uptime, load, memory, battery, disk, theme.
- `desktop_screenshot` - a PNG path (optionally one `output`). It shows whatever is on the
  screen, private windows included; the file is deleted when this session ends.
- `desktop_windows` - workspaces, windows, which is focused.
- `theme_current` - the active palette.
- `packages_installed` - installed Debian packages, optionally filtered.
- `system_report` - the machine's own health report (`check-system.sh`): counts and every
  FAIL/WARN line. Slow; once a minute at most.
- `clipboard_read` - the clipboard's text. It may be something the person did not mean to
  share; quote it only when they asked.
- `task_list` - recent slow actions and whether each can be stopped.

## act - visible and reversible

- `menu_open` - the root menu or the palette in front of the person, with a filter typed so
  the row you mean is first. They press Enter.
- `task_open` / `task_stop` - show a task's window; stop one whose record says it can be.
- `theme_set` - re-theme the desktop (`theme_current` says what is active).
- `notify_send` - a card. At most three a minute. **Prefer a window that holds the answer over
  a toast that expires**: open the file, the menu or the task rather than describing it.
- `view_open` - open a file in its viewer, or a directory in the file list.

## propose - the person decides

- `install_propose` - the Install LazyOS window; the person types the disk's name.
- `format_propose` - Format a disk; its list only holds removable disks this system does not
  use, and it asks again with Cancel first.
- `swap_propose` - Auxiliary swap; the person picks the card and the size.
- `power_propose` - log out, reboot or shut down, with Cancel selected.

A propose tool returns "proposed" as soon as the window is open. You do not see what the person
answers and must not assume it: ask them, or read the result afterwards (`desktop_status`,
`task_list`). Each proposal waits 30 seconds before it can be made again.

## Never exposed

There is no tool that runs a shell command, applies a format or an install, ends the session,
writes a partition or the kernel partition, or exports or restores a profile (`run`,
`format_apply`, `install_apply`, `swap_apply`, `power_apply`, `kpart_write`, `partition_write`,
`profile_*`). Calling one is refused with the menu to point the person at. Your own shell
session is your command line; on this machine, destructive work is the person's.

The manual is a resource (`lazyos://manual/*.md`) - the same pages the person reads in Learn.
