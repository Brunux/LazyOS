# Spec 014 — Performance, witnesses and the migration ledger

- **Status**: draft
- **Phase**: 5-6
- **Depends on**: 007-013
- **Model**: `claude-opus-5-5` (effort `high`) — budget design, a measurement harness, a
  repo-wide sweep of check-system/manual/AGENTS that must stay consistent, and the closure that
  removes rofi and mako from the image (the one irreversible step of the port). Fallback
  `claude-sonnet-5-5`.
- **Branch**: `spec/014-perf-witnesses`
- **Plan**: [`docs/plan-ui.md`](../docs/plan-ui.md) §14 (budget), §15 (witnesses), §16 phase 5
- **Old paths**: the remaining rofi/mako entries in `uis/sway/packages.list` and the scripts
  the earlier specs left as symlinks

## Problem

The port can finish "working" and still fail the point of it: a framework that costs more than
rofi, a report that no longer names the surfaces it checks, and a set of symlinks nobody can
audit. The plan promises a budget (≤ rofi's 15-17 MB PSS, 0 after close) and a closure (rofi
leaves the image). Neither has a harness yet.

## Scope

**In:**

- **A benchmark binary** (`lazyos bench` or a `benches/` target): measures cold start, PSS of
  each window, tick cost with and without a picture, theme generation time, and the sixel
  first-frame/repaint numbers from the spike — prints a table the check can read.
- **CI budget check**: a test that fails when a committed baseline is exceeded (PSS/startup
  thresholds live in one file), so a regression is a red build, not a report nobody reads.
- **`lazyos check`**: the framework's own half of `check-system.sh` (subcommands, symlinks,
  theme idempotence, glyph pins, MCP manifest) as a Rust command, so the report and the
  framework agree about what exists.
- **The migration ledger** (`docs/migration.md`): every old path, its new subcommand, the spec
  that ported it and its status; check-system fails when a ledger entry claims `ported` but
  the symlink/subcommand is missing, or vice versa.
- **Closure**: remove rofi, mako (as UI), the ported scripts and their packages from
  `uis/sway/packages.list`; keep mako until spec 013's daemon lands if it has not.
- **The report**: `check-system.sh` sections renamed/regrouped to match the framework, with the
  manual and AGENTS updated in the same PR.

**Out:** the image build changes (`scripts/make-image.sh` etc. stay), the site.

## Design

- The budget file is committed data (`rust/benches/budget.toml`) and the test reads it; the
  maintainer updates it deliberately, with the measured number in the commit message.
- `lazyos check` prints the same `PASS/FAIL/SKIP/WARN` vocabulary `check-system.sh` prints, so
  the two halves can be concatenated by the report.
- The ledger is generated from the command table and the check reads it, so a new subcommand
  cannot be added without a ledger row.
- Closure is staged: a package leaves `packages.list` only after its replacement has a spec's
  check line; the PR lists each removal and the witness that allows it.

## Acceptance criteria

- [ ] the benchmark prints a stable table across three runs (variance stated); the numbers are
      appended to `docs/optimization.md` § UI.
- [ ] the budget test fails when a window's PSS or a cold start crosses the committed
      threshold; a deliberate regression in a test branch is caught.
- [ ] `lazyos check` answers for every subcommand and symlink; it FAILs on the same defects
      the shell report does (same fixture).
- [ ] the ledger covers every old path in the plan's inventory; a wrong status fails the check.
- [ ] the ported scripts are gone (symlinks only), and rofi/mako packages are out of
      `uis/sway/packages.list`; the session still runs with the framework alone.
- [ ] witness: `scripts/check-system.sh` FAILs on a missing `lazyos check`, on a ledger
      mismatch, on a remaining rofi/mako package in the UI list (when the ledger says ported),
      and on a budget file that does not match the measured baseline.
- [ ] witness: a paragraph in `uis/sway/files/usr/share/doc/lazyos/05-system.md`
      § *The check* explains the two halves of the report and how to read a FAIL.
- [ ] witness: an `AGENTS.md` bullet says the budget is committed data and the ledger is the
      closure witness.
- [ ] unit tests: ledger/command-table consistency, budget parsing, check vocabulary, benchmark
      table formatting, threshold comparison.

## Tests

- **Unit** (`cargo test`): ledger ↔ table, budget parse/compare, check line formatting.
- **Snapshot**: the check output (PASS/FAIL/SKIP/WARN) and the benchmark table.
- **Integration**: run the check against the real tree with a missing subcommand injected;
  assert FAIL; with a ledger row flipped; assert FAIL.
- **Hardware**: the full report on the card after the last port; every FAIL has an owner.

## Risks

- **The budget is gamed by measuring an idle window** — the benchmark measures the heaviest
  surface (the palette with sixel icons) and the check records the method alongside the number.
- **Closure removes a package a still-present script needs** — the ledger's `ported` status is
  the gate, and the check runs the session before the package list changes.
