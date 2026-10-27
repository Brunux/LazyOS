# Spec 005 — The task surface (TaskView)

- **Status**: in progress
- **Phase**: 2
- **Depends on**: 002
- **Model**: `opencode-go/kimi-k2.7-code` — a state machine with process control, stop/reattach
  and a snapshot per state; dense implementation with tests is exactly its shape. Fallback
  `opencode-go/deepseek-v4-pro`.
- **Branch**: `spec/005-taskview`
- **Plan**: [`docs/plan-ui.md`](../docs/plan-ui.md) §10 (TaskView), §11 (the loop), §14 (perf)

## Problem

The kit's slow actions (`busy` cards in `lib-rofi.sh`: the Wi-Fi scan, the Bluetooth scan, an
apt install, a dictation) hand their result to a toast that expires. A person who looked away
misses the result, a failure leaves a card with no reason, and nothing can be stopped from the
window except by killing a pkill pattern. The installer has the same problem at a larger scale.

The plan's answer is a task surface that **keeps the person in the loop**: the action ends by
showing its own result, in the same window, with the log it produced.

## Scope

**In (`lazyos-ui::task` + `lazyos task`):**

- **Model**: `Task { title, steps: Vec<Step>, state, progress, log, stoppable, started_at }`;
  `Step` has a label, a state (waiting/running/ok/failed/skipped) and optional detail. State
  transitions are the only way to change the view.
- **View**: header (title + elapsed), checklist (glyph per step), spinner + gauge while
  running, a **live log tail** (last N lines, follow), footer with the one action that applies
  (`[s] stop`, `[c] copy`, `[r] rerun`, `[q] close`) and the result block when it ends.
- **Process execution**: a task runs one child at a time, captures stdout/stderr line by line,
  and owns the stop contract: **SIGINT first, SIGTERM after 2 s**, then `Stopped` (not
  `Failed`); the exit status of a stopped child is never reported as a result.
- **Reattach**: state is a file in `~/.local/state/lazyos/tasks/<id>.json` (phase, percent,
  pid, log path, stoppable), written atomically; `lazyos task attach` reopens a running task
  after the window closed. A pid whose `/proc/<pid>/comm` no longer matches is not touched.
- **The bridge**: while a task runs, it also writes the existing `busy` protocol
  (`$XDG_RUNTIME_DIR/lazyos-busy/<tag>`) so the bar's spinner keeps working until the bar is
  ported (spec 014), including the `BUSY_STOP` line the click runs.
- **Results inline**: success renders the result in the window (path, count, URL, table);
  failure renders the last `ERROR:`-style line in `danger`, with the full log one key away.

**Out:** the actual tasks (Wi-Fi, apt, dictation — their pickers' specs), the notification
daemon, the installer's screens (they *use* this).

## Design

- A task is **not stoppable by default**; the spec of each task says whether it is. Format,
  install and DNS in the shell menus are deliberately not stoppable — the same rule carries
  over, and the view says so.
- The log is a ring buffer in memory plus a file; the view reads each frame, so a killed window
  does not lose the reason.
- The tick is 100 ms only while a task runs (spinner/progress); idle is event-only, so a task
  left open costs nothing.
- `Task::spawn(cmd)` takes an argv array, never a shell string, so a pattern from a menu cannot
  become a shell injection; the stop pattern is data, not a command line.

## Acceptance criteria

- [x] a scripted task (3 steps, 2 s each) renders waiting → running → done with the checklist,
      a moving spinner and a gauge; the snapshots are asserted. **Observed**:
      `lazyos-ui/tests/snapshots/task-queued.txt`, `task-running.txt`, `task-done.txt`,
      `task-running-stoppable.txt` at 80x24 and 152x45, plus `task::tests::success_finishes_done`
      driving a real `/bin/sh` child through the state machine.
- [x] a task that fails on step 2 shows step 1 ok, step 2 failed with its stderr line, and the
      window stays open on the result. **Observed**:
      `lazyos-ui/tests/snapshots/task-failed.txt` and
      `task::tests::failure_keeps_the_error_line` (the last `ERROR:` line becomes `error_line`,
      the `danger` row), and `tests/task.rs::a_failed_detached_task_keeps_its_error_line` for
      the detached path.
- [x] pressing `s` on a stoppable task ends it as `Stopped` with the last line kept; the same
      key on a non-stoppable task says why and does nothing. **Observed**:
      `task::tests::stop_is_not_failure`, `task::tests::ignore_sigint_is_stopped` (SIGTERM after
      two seconds), `task::tests::late_output_after_stop_is_dropped`,
      `task::tests::stop_key_on_non_stoppable_says_why`, snapshot `task-stopped.txt`.
- [x] closing the window during a task leaves the task running and reattachable; `attach`
      after it finished shows the saved result. **Observed**: the runner is a separate,
      `setsid`'d process (`task_cmd::spawn_runner`), so the viewer owns no child;
      `lazyos-cli/tests/task.rs::a_detached_task_runs_to_done_without_a_viewer` starts it and
      never owns it, `a_viewer_stops_a_detached_task_through_the_request_file` stops it through
      the request file, and `task::tests::attach_after_finish_shows_the_saved_result` reads the
      finished record back. A reattached task is only as honest as its log, so that half has
      its own witness: `task::tests::done_waits_for_the_output_pipes_to_close` (a child that
      exits while a grandchild still holds the pipe; `Done` must not arrive before the last
      line is on disk) and `task::tests::a_finished_task_carries_every_line_the_child_wrote`
      (5,000 lines, all three copies: memory, file, reloaded record).
- [x] the bar spinner appears and disappears with the task, and a click on it stops a stoppable
      task exactly like `s`. **Observed**: `task::tests::busy_tag_matches_stop` (the tag, label
      and `BUSY_STOP` line match `bin/busy`'s format) and
      `task::tests::non_stoppable_task_writes_no_stop_file`; the runner clears the tag in
      `finalize` and on any exit.
- [x] witness: `scripts/check-system.sh` FAILs when a task declared stoppable has no STOP
      handler, when a non-stoppable task command lists `pkill`, and when a task writes a
      `busy` tag without the matching `BUSY_STOP` rule. **Observed**: the spec-005 row FAILs
      when `lazyos --help` stops listing `task attach` (the symlink then points at nothing);
      the stop/BUSY_STOP rules are the unit tests above, because no shell menu declares a task
      yet (they arrive with the pickers' specs).
- [x] witness: a paragraph in `uis/sway/files/usr/share/doc/lazyos/02-rofi.md` describes the
      task screen, the stop key and where the log lives. **Observed**: § *The task screen*.
- [x] witness: an `AGENTS.md` bullet says a slow action ends by showing its result.
      **Observed**: the *Nothing resident* bullet now ends with "**ends by showing its own
      result in the task window** instead of a toast that expires".
- [x] unit tests: state machine transitions (no result after stop, no stop after done),
      log ring, atomic state file, PID identity check, `BUSY_STOP` mirroring. **Observed**:
      `stop_is_not_failure`, `late_output_after_stop_is_dropped`,
      `stopping_a_finished_task_does_nothing`, `the_log_is_a_ring_that_drops_the_oldest_lines`,
      `state_file_is_atomic`, `pid_identity_rejects_reuse`, `busy_tag_matches_stop`.

## Tests

- **Unit** (`cargo test`): `stop_is_not_failure`, `late_output_after_stop_is_dropped`,
  `state_file_is_atomic`, `pid_identity_rejects_reuse`, `busy_tag_matches_stop`,
  `done_waits_for_the_output_pipes_to_close`, `a_finished_task_carries_every_line_the_child_wrote`.
- **Snapshot**: queued/running/done/failed/stopped, with and without a stoppable action.
- **Integration**: run `/bin/sh -c` fixtures (success, failure, long sleep, ignore-SIGINT) in
  a temp state dir and assert the final state, exit code and log tail.
- **Hardware**: run a real `nmcli device wifi list` and an `apt install` through it; stop the
  scan and confirm the bar spinner disappears and no `nmcli` survives.

## Risks

- **A stopped child leaves a half-written artifact** (the note db, a download) — the task's
  spec owns cleanup; this spec only guarantees the state says `Stopped` and the log says what
  was interrupted.
- **Reattach across a reboot** — the state file carries the boot id; a mismatch shows "did not
  finish" instead of pretending.
