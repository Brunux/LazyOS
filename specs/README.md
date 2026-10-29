# Specs: the unit of work

[`docs/plan-ui.md`](../docs/plan-ui.md) decides **what the session becomes**. A spec in this
directory decides **what one mergeable change is**, how it is tested, and how we know it is
finished. The plan is the map; a spec is one leg of the trip.

Nothing here replaces the plan. Every spec links to the plan section it implements and adds only
the decisions a builder needs: scope, acceptance criteria, the three witnesses, the tests, and
which model builds it (§6).

## 1. Why specs

The plan is 600 lines and the port is ~60 commands. A single branch that "ports the session"
produces an unreadable diff, an unverifiable claim and a reviewer who cannot say no to half of
it. A spec is small enough that:

- the reviewer can hold the whole change in their head;
- the acceptance criteria are observable on the device, not "looks done";
- the tests and the three witnesses land **in the same PR** as the code that needs them;
- a model can be given a bounded, checkable task instead of "build the UI".

## 2. The lifecycle

| Status | Means | Who moves it |
|---|---|---|
| `draft` | written, not reviewed | the spec's author |
| `approved` | scope and criteria accepted | the maintainer |
| `in progress` | the branch exists | the builder |
| `merged` | the PR is merged, witnesses pass on hardware | the maintainer |
| `dropped` | superseded or rejected, with the reason in the file | the maintainer |

A spec is edited **before** the branch, in its own PR if the change is material (scope, criteria,
model). Editing acceptance criteria to match the code is how a spec stops being a spec.

## 3. Branch and PR rules

One spec, one branch, one PR, one reviewer. The branch is created from `main`:

```sh
git switch -c spec/004-theme-engine origin/main
# work, commit, push
gh pr create --base main --head spec/004-theme-engine \
  --title "spec 004: the theme engine" --body-file .github/PULL_REQUEST_TEMPLATE.md
```

- **Branch name**: `spec/NNN-slug`, the number and slug of the spec file.
- **Commit subject**: `spec NNN: what changed`, one concern per commit; the spec number makes
  the history auditable (`git log --grep 'spec 004'`).
- **No drive-by changes**: a fix the spec did not ask for is its own spec (or a plain `fix:`
  commit on `main` when it is a one-liner).
- **The model used is written in the PR body**, even when it differs from the spec's assignment
  (see §6).
- **The AI never merges its own PR.** A human reviews the diff, the tests and the witnesses.

## 4. Definition of done

A spec is done when **all** of these are true:

1. Every acceptance-criteria checkbox is checked, and each one names how it was observed.
2. **Unit tests** cover the new logic (see §5) and `cargo test --workspace` is green in CI.
3. **The three witnesses** exist, as every feature in this repository ships them:
   - a `scripts/check-system.sh` line that **FAILs** when the feature's dependency disappears
     (a binary, a generated file, a binding, a symlink, a manifest entry);
   - a paragraph in `uis/sway/files/usr/share/doc/lazyos/` (the manual a person reads on the
     device);
   - a bullet in `AGENTS.md` that says **why** it is built that way.
4. The **old path still works** while the port is in flight: the spec adds the symlink (or the
   documented replacement) in the same PR, or explains why it cannot yet.
5. CI is green: `Rust` (fmt, clippy, tests, build) and `Specs` (this directory's contract).
6. A hardware observation is recorded in `reports/` when the change is visible on the device
   (the report is the evidence the criterion was met on the Hayato, not on the build host).

## 5. Tests

The Rust side has four levels; a spec says which ones it uses:

- **Unit tests** (`#[cfg(test)]`): pure logic — parsers, rankers, dispatch tables, path
  resolution, theme validation, protocol framing.
- **Snapshot tests** (`ratatui::backend::TestBackend`): every widget and every TaskView state
  renders to a buffer, and the buffer is asserted. A layout change that moves a border is a
  failing test, not a review comment.
- **Protocol/integration tests**: the MCP manifest, the `theme.toml` → file generation, the
  `.desktop` parser, the sway IPC client against recorded JSON.
- **Hardware checks**: on the device, captured in `reports/` (the manual and `check-system.sh`
  are the runtime half). CI cannot boot a Chromebook, so the device evidence stays manual by
  design — the same split `docs/test-checklist.md` already uses.

No test may depend on a running session, the network, or `$HOME` state; fixtures live in the
crate (`tests/fixtures/`) and are committed.

## 6. Model routing

Specs are built with **Claude models** (the API IDs below). They were built with `opencode go`
until spec 006; that record stays in each merged spec and PR, and §6.4 keeps what it taught. The
IDs are the same whatever runs them: Claude Code (`claude --model claude-sonnet-5-5`), or OpenCode
with the Anthropic provider (`anthropic/claude-sonnet-5-5`). The PR names the harness as well as
the model.

### 6.1 The models

| Model | ID | $/M in–out | Batch in–out | Route here for |
|---|---|---|---|---|
| Claude Fable 5.1 | `claude-fable-5-1` | 10–50 | 5–25 | **escalation only**: a spec that has defeated two attempts on Opus |
| Claude Opus 5.5 | `claude-opus-5-5` | 4–20 | 2–10 | architecture, security and protocol boundaries, destructive or irreversible actions, the session's taste |
| Claude Sonnet 5.5 | `claude-sonnet-5-5` | 2–10 | 1–5 | dense implementation with tests: parsers, IPC, async pipelines, state machines, many dialogs of one shape |
| Claude Haiku 5.5 | `claude-haiku-5-5` | 0.10–0.50 (prompt ≤ 100k) · 0.50–2.50 (> 100k) | half | a fixed shape repeated where the pattern and tests already exist, fixtures, golden regeneration |

Prices are Anthropic's list prices (platform.claude.com/docs/en/about-claude/pricing,
2026-10-28); every model has a 1M-token context at one price, except Haiku's tier.

**Haiku's tier is the one that applies here.** A session that loads `AGENTS.md` (~58k tokens) and
`docs/plan-ui.md` (~11k) is past 100k as soon as it reads the crate it changes, so budget Haiku at
0.50–2.50: still a quarter of Sonnet, and the right price for 011-shaped work.

**Effort** is part of the assignment. Opus defaults to `medium` and Sonnet to `high`; a spec that
names an effort is built at that effort, and the PR says so. `xhigh` is reserved for a security
boundary (008).

### 6.2 Assignments

| Spec | Work shape | Primary | Fallback |
|---|---|---|---|
| 001 core | — merged (`opencode-go/deepseek-v4-pro`) | — | — |
| 002 design system | — merged (`opencode-go/glm-5.3`) | — | — |
| 003 glyphs | — merged (`opencode-go/gpt-6-luna`) | — | — |
| 004 themes | criteria met, hardware report filed; waits for the maintainer | — | — |
| 005 TaskView | — merged (`opencode-go/kimi-k2.7-code`) | — | — |
| 006 notifications | code merged; three observations on the Hayato remain | the maintainer, on the device | — |
| 007 menu + Ask | taste + ranking + enum dispatch + a child's lifetime | `claude-opus-5-5` (high) | `claude-sonnet-5-5` |
| 008 MCP | protocol + security tiers | `claude-opus-5-5` (xhigh) | `claude-fable-5-1` |
| 009 search family | async walk + preview | `claude-sonnet-5-5` (high) | `claude-opus-5-5` |
| 010 system pickers | many dialogs, destructive edges, parity tests | `claude-sonnet-5-5` (high) | `claude-opus-5-5` |
| 011 media pickers | repetition over a fixed shape | `claude-haiku-5-5` | `claude-sonnet-5-5` |
| 012 launcher/reference | parsing + IPC + many surfaces | `claude-sonnet-5-5` (high) | `claude-opus-5-5` |
| 013 themes + greeter | palette data + generated wallpapers judged by eye | `claude-sonnet-5-5` (high) | `claude-opus-5-5` |
| 014 perf + witnesses | budget harness + sweep + the closure | `claude-opus-5-5` (high) | `claude-sonnet-5-5` |
| 015 browser bar | protocol client + profile/data | `claude-sonnet-5-5` | `claude-opus-5-5` |
| 016 local model | measured flags + a wrapper + a pin | `claude-sonnet-5-5` (high) | `claude-haiku-5-5` |

The assignment is still a **starting hypothesis**: each PR records the model, the effort, the
harness and whether the criteria and tests passed on the first review, and the table is revised
by that record.

### 6.3 Re-routing

- **Two failed attempts re-route the spec one tier up** (Haiku → Sonnet → Opus → Fable), never
  down: a spec that failed on a model failed on the shape of the work, and a cheaper model has
  less room for it.
- **A usage limit is not a failure.** The spec resumes on the same model later, or on the same
  model through the API; it is not handed to another model mid-spec.
- **Haiku is never a fallback for a destructive edge** (format, swap, power, the MCP tiers), even
  when the rest of the spec is repetitive.

### 6.4 What the `opencode go` record taught

| Spec | Assigned | Built by | Note |
|---|---|---|---|
| 001 | deepseek-v4-pro | same | no re-route |
| 002 | glm-5.3 | same | no re-route |
| 003 | gpt-6-luna | same | measured the spec's own premise false, and recorded it |
| 004 | deepseek-v4-pro | deepseek-v4.1-flash | re-routed before the work |
| 005 | kimi-k2.7-code | kimi, then deepseek-v4.1-flash | **usage limit** mid-spec (runner, criteria pass) |
| 006 | claude-haiku-5-5 | Haiku, then deepseek-v4.1-flash | **usage limit** after all five commits (rebase, verification) |

Two of the last three specs changed model mid-flight for a reason that had nothing to do with the
work, so their "first review" result describes two models at once. That is the reason for §6.3's
second rule, and part of the reason for the migration.

## 7. CI

| Workflow | Runs on | Does |
|---|---|---|
| [`Rust`](../.github/workflows/rust.yml) | push, PR (`rust/**`) | `cargo fmt --check`, `clippy -D warnings`, `cargo test`, release build |
| [`Specs`](../.github/workflows/specs.yml) | push, PR (`specs/**`) | every spec has its contract fields, witnesses and index entry |
| [`Release`](../.github/workflows/release.yml) | tag `v*` | checks `VERSION` and the notes, tests, builds aarch64 binaries, publishes the release |
| [`Pages`](../.github/workflows/pages.yml) | push to `main` (`site/**`, `brand/**`, screenshots) | deploys the landing page |

Cargo artifacts are cached by `Swatinem/rust-cache`; the workspace lives in `rust/`.

## 8. Writing a spec

Copy [`000-template.md`](000-template.md), number it, link the plan section, and keep it short:
scope, acceptance, tests, witnesses, model. If a spec needs more than two pages, it is two
specs.

## 9. The index

| # | Spec | Phase (plan §16) | Status |
|---|---|---|---|
| 001 | [`001-core.md`](001-core.md) — the `lazyos` binary and its host | 2 | merged |
| 002 | [`002-design-system.md`](002-design-system.md) — tokens, widgets, snapshots | 2 | merged |
| 003 | [`003-glyphs.md`](003-glyphs.md) — the Nerd Font alphabet | 2 | merged |
| 004 | [`004-theme-engine.md`](004-theme-engine.md) — `theme.toml` and the generators | 2 | merged |
| 005 | [`005-taskview.md`](005-taskview.md) — the task surface | 2 | merged |
| 006 | [`006-notifications.md`](006-notifications.md) — cards and icons | 2 | merged |
| 007 | [`007-menu-ask.md`](007-menu-ask.md) — root menu, palette, Ask strip | 2 | in progress |
| 008 | [`008-mcp.md`](008-mcp.md) — `lazyos mcp` and the OpenCode contract | 2 | in progress |
| 009 | [`009-search.md`](009-search.md) — files, find, grep | 3 | draft |
| 010 | [`010-system-pickers.md`](010-system-pickers.md) — system dialogs | 3 | draft |
| 011 | [`011-media-pickers.md`](011-media-pickers.md) — media and utilities | 3 | draft |
| 012 | [`012-launcher-reference.md`](012-launcher-reference.md) — launcher, window, run, reference | 4 | draft |
| 013 | [`013-themes-greeter.md`](013-themes-greeter.md) — 7 themes, wallpapers, greeter | 5 | draft |
| 014 | [`014-perf-witnesses.md`](014-perf-witnesses.md) — budget, ledger, closure | 5 | draft |
| 015 | [`015-browser-bar.md`](015-browser-bar.md) — the browser bar (optional, not on the plan) | - | draft |
| 016 | [`016-local-models.md`](016-local-models.md) — a local model, measured and pinned | - | in progress |
