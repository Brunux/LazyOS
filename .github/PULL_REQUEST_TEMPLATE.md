## Spec

<!-- The unit of work. One spec, one branch, one PR (specs/README.md). -->

- Spec: `specs/NNN-slug.md` (number, title)
- Model that did the work: `claude-…`, effort `…`, harness (Claude Code / OpenCode) — the spec's
  assignment or the documented re-route (specs/README.md §6.3); passed on the first review: yes/no
- Branch: `spec/NNN-slug`

## What changed

<!-- Two or three sentences a person can read without the diff. -->

## Witnesses (all three, as every feature here ships them)

- [ ] `scripts/check-system.sh` FAILs when this feature's dependency disappears — line:
- [ ] manual paragraph in `uis/sway/files/usr/share/doc/lazyos/<page>.md`:
- [ ] `AGENTS.md` bullet:

## Tests

- [ ] unit tests added or changed (`cargo test --workspace`, names in the spec's §Tests)
- [ ] snapshot tests for every visual state this PR touches (`TestBackend`)
- [ ] integration/protocol tests with committed fixtures
- [ ] hardware observation recorded in `reports/` (when the change is visible on the device)

## Acceptance criteria

<!-- Paste the spec's checkboxes; each one names how it was observed. -->

- [ ]

## Compatibility

- [ ] the old path still works (symlink/subcommand/generated file) or the spec explains why
      it cannot yet
- [ ] no drive-by change: anything else this touched is its own spec or a `fix:` commit

## Review

- [ ] the diff was read by a person (the AI does not merge its own PR)
- [ ] CI is green: `Rust` and `Specs`
