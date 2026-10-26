# Specs: the unit of work

[`docs/plan-ui.md`](../docs/plan-ui.md) decides **what the session becomes**. A spec in this
directory decides **what one mergeable change is**, how it is tested, and how we know it is
finished. The plan is the map; a spec is one leg of the trip.

Nothing here replaces the plan. Every spec links to the plan section it implements and adds only
the decisions a builder needs: scope, acceptance criteria, the three witnesses, the tests, and
which model builds it.

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

`opencode go` serves the models below. The assignment in each spec is a **starting hypothesis**
based on the shape of the work and the price, not on a benchmark run against this repository.
Each PR records the model that actually did the work and whether the acceptance criteria and
tests passed on the first review; two failed attempts on a spec re-route it, and the table is
revised by that record rather than by marketing.

| Model | `opencode-go/…` | $/M in–out | Good at (route here for) |
|---|---|---|---|
| DeepSeek V4 Pro (New) | `deepseek-v4-pro` | 0.66–1.98 | architecture, protocol surfaces, cross-crate design |
| Kimi K2.7 Code | `kimi-k2.7-code` | 0.95–4.00 | dense implementation with tests, state machines |
| GLM-5.3 | `glm-5.3` | 1.40–4.40 | visual/taste work, layout judgement |
| GPT-6 Luna | `gpt-6-luna` | 0.10–0.50 (272k tier 0.20–0.75) | repo-wide sweeps, data tables, long context, docs+witnesses |
| Claude Haiku 5.5 | `claude-haiku-5-5` | 0.10–0.50 | small mechanical ports, repetitive fixtures |
| MiniMax M3.1 Flash | `minimax-m3.1-flash` | 0.15–0.60 | bulk repetition where tests already exist |
| MiMo-V2.6-Flash | `mimo-v2.6-flash` | 0.14–0.28 | cheapest bulk edits |

Reserve `gemini-4-argon-contributor` ($4–20) and `grok-4.7` ($2–6) for a spec that has already
defeated two cheaper models. `beam-free` is not used for committed work (no cost signal, no
reproducible assignment).

| Spec | Work shape | Primary | Fallback |
|---|---|---|---|
| 001 core | architecture + small code | `deepseek-v4-pro` | `kimi-k2.7-code` |
| 002 design system | visual taste + widgets | `glm-5.3` | `kimi-k2.7-code` |
| 003 glyphs | data table + font verification | `gpt-6-luna` | `claude-haiku-5-5` |
| 004 themes | schema + generators + idempotence | `deepseek-v4-pro` | `glm-5.3` |
| 005 TaskView | state machine + snapshots | `kimi-k2.7-code` | `deepseek-v4-pro` |
| 006 notifications | small, mechanical | `claude-haiku-5-5` | `minimax-m3.1-flash` |
| 007 menu + Ask | taste + ranking | `glm-5.3` | `gpt-6-luna` |
| 008 MCP | protocol + security tiers | `deepseek-v4-pro` | `kimi-k2.7-code` |
| 009 search family | async walk + preview | `kimi-k2.7-code` | `gpt-6-luna` |
| 010 system pickers | many small dialogs + tests | `kimi-k2.7-code` | `claude-haiku-5-5` |
| 011 media pickers | repetition over a fixed shape | `claude-haiku-5-5` | `minimax-m3.1-flash` |
| 012 launcher/reference | parsing + IPC + many surfaces | `gpt-6-luna` | `kimi-k2.7-code` |
| 013 themes + greeter | palette data + generated files | `gpt-6-luna` | `glm-5.3` |
| 014 perf + witnesses | measurement + repo sweep | `deepseek-v4-pro` | `gpt-6-luna` |
| 015 browser bar | protocol client + profile/data | `deepseek-v4.1-flash` | `kimi-k2.7-code` |

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
| 004 | [`004-theme-engine.md`](004-theme-engine.md) — `theme.toml` and the generators | 2 | in progress |
| 005 | [`005-taskview.md`](005-taskview.md) — the task surface | 2 | merged |
| 006 | [`006-notifications.md`](006-notifications.md) — cards and icons | 2 | in progress |
| 007 | [`007-menu-ask.md`](007-menu-ask.md) — root menu, palette, Ask strip | 2 | draft |
| 008 | [`008-mcp.md`](008-mcp.md) — `lazyos mcp` and the OpenCode contract | 2 | draft |
| 009 | [`009-search.md`](009-search.md) — files, find, grep | 3 | draft |
| 010 | [`010-system-pickers.md`](010-system-pickers.md) — system dialogs | 3 | draft |
| 011 | [`011-media-pickers.md`](011-media-pickers.md) — media and utilities | 3 | draft |
| 012 | [`012-launcher-reference.md`](012-launcher-reference.md) — launcher, window, run, reference | 4 | draft |
| 013 | [`013-themes-greeter.md`](013-themes-greeter.md) — 7 themes, wallpapers, greeter | 5 | draft |
| 014 | [`014-perf-witnesses.md`](014-perf-witnesses.md) — budget, ledger, closure | 5 | draft |
| 015 | [`015-browser-bar.md`](015-browser-bar.md) — the browser bar (optional, not on the plan) | - | draft |
