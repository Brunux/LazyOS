# CLAUDE.md

`AGENTS.md` is this repository's instructions file, and it is the one every agent reads: the
image ships OpenCode, which reads it by that name, and the specs are built with Claude models
(`specs/README.md` §6). This file only makes Claude Code load the same text, so there is one set
of rules and no second copy to drift.

@AGENTS.md

When the work is a spec, `specs/README.md` is the process (branch, commits, the three witnesses,
the definition of done) and the spec file is the task. The PR body records the model, the effort
and the harness that did the work (`.github/PULL_REQUEST_TEMPLATE.md`), and the AI never merges
its own PR.
