# Spec NNN — Title

- **Status**: draft
- **Phase**: N (plan §16)
- **Depends on**: spec NNN (or "none")
- **Model**: `claude-<tier>-<version>` (effort) — why this model; fallback `claude-<tier>-<version>`
  (specs/README.md §6)
- **Branch**: `spec/NNN-slug`
- **Plan**: [`docs/plan-ui.md`](../docs/plan-ui.md) §N
- **Old paths**: which of the ~64 scripts and ~55 bindings this replaces or keeps working

## Problem

Two or three sentences: what is wrong today, in terms of what a person sees or what the code
cannot do. Link the evidence (`docs/omarchy-gap.md` §, a report in `reports/`, a script name).

## Scope

**In:**

- what this spec builds, at the level of surfaces and APIs.

**Out:**

- what is deliberately a later spec (name it), so the PR cannot grow.

## Design

Only the decisions a builder needs that the plan does not already fix: data shapes, file
paths, keys, failure behaviour, the one trade-off that was chosen and why.

## Acceptance criteria

Each line must be observable. A criterion that cannot be demonstrated is not a criterion.

- [ ] behaviour: observable result (what the person does, what they see).
- [ ] failure: what happens when the dependency is missing, and what the person reads.
- [ ] compatibility: the old path/command still works (symlink, subcommand, generated file).
- [ ] witness: a `scripts/check-system.sh` line FAILs when the dependency is removed — say
      which line.
- [ ] witness: a paragraph in `uis/sway/files/usr/share/doc/lazyos/<page>.md` says how to use
      it and how to undo it.
- [ ] witness: an `AGENTS.md` bullet says why it is built this way.
- [ ] unit tests: the logic is covered by `cargo test` (name the tests in §Tests).

## Tests

- **Unit** (`cargo test`): the pure functions and parsers, with their fixtures.
- **Snapshot**: which `TestBackend` states are asserted (one per visual state).
- **Integration**: protocol/file generation exercised end to end, in a temp dir.
- **Hardware**: what to try on the Hayato, and what a failure looks like (goes to `reports/`).

## Risks

- the failure this spec is most likely to ship silently, and the test that catches it.
