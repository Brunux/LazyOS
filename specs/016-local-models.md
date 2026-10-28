# Spec 016 — A local model on this machine

- **Status**: in progress
- **Phase**: not in `docs/plan-ui.md` §16 — the plan's fourteen specs port the session as it is.
  This is a new capability, written after the measurement campaign of 2026-10-28 and the
  maintainer's decision to keep it.
- **Depends on**: `llama.cpp-examples` (forky), `foot`, `rofi` (the root menu), and the LazyOS
  theme's `dictate-model` name. Nothing from the framework: the window is foot's, as every other
  one-shot surface here is.
- **Model**: `claude-sonnet-5-5` (high) — measurement-driven flag selection plus a wrapper, a pin
  file and three witnesses; the numbers are the hard part and each one is a test. Fallback
  `claude-haiku-5-5`.
- **Branch**: `spec/016-local-models`

## Problem

The obvious answer to "can this Chromebook run a local model" is no: 3.8 GB of RAM, no NPU, no
GPU compute (Panfrost is OpenGL 3.1 and PanVK answers *Mali-G57 not supported*). It was measured
anyway, and it is yes — a 1.7B model at **11 tok/s**, which is a paragraph in about ten seconds.
That is slow enough to be useless for reading and fast enough to ask a question.

The problem is that the number is not a property of the machine, it is a property of **four flags
nobody would guess**, and every one of them is invisible at the call site:

| call | what it measured |
|---|---|
| `llama-cli -m model.gguf` | **6.67 tok/s** — llama.cpp asks for `nproc` (8) and the four A55 cores, 7.8× slower each, drag every barrier |
| `-t 4` | 11.19 tok/s |
| `-t 4` with Q4_K_M | 11.14 tok/s, and **49 s** to the first token of an 1800-token prompt |
| `-t 4` with **Q4_0** | **11.74 tok/s**, **31 s** to first token |
| `-t 4 -fa on -ctk/-ctv q8_0` | 2356 MiB peak RSS instead of 2584, against **2544 MiB free** with the session up |

So the difference between "this machine cannot" and "this machine can" is a default flag, a quant
choice and a KV cache type — and a person who runs `llama-cli` once concludes the first, correctly,
because that is what they measured.

## Scope

**In** (one wrapper, one pin file, three surfaces, three witnesses):

- **`~/.config/bin/lazyos-model`** — the one place the flags live (`THREADS`, `CTX`, `SAMPLING`,
  `REASONING`), plus `--list`, `--fetch`, `--verify`, `--bench` and `--flags`. It opens its own
  foot window (`--app-id=lazyos-model`) when it has no terminal, so the launcher row, the menu row
  and the key are the same program as the terminal command.
- **`~/.config/lazyos/models.sha256`** — the pinned SHA256 of the two measured GGUFs, and the only
  place a hash lives (`--verify` reads it; `--fetch` checks against it).
- **Three surfaces**: `Alt+Ctrl+M` (`24-keys.conf`, with its description comment), the root menu's
  AI group (`Local model`, `UI_I_CHIP`), and `org.lazyos.model.desktop` (`Icon=dictate-model`, a
  name the theme already draws).
- **`config.d/34-local-model.conf`** — floats and centres the window at 1000x700 (997x696 measured
  on the 1366x912 panel).
- **`llama.cpp-examples`** in `packages/tools.list`.

**Out**:

- **The weights in the image.** 2.4 GB, a third of the disk, and stale the moment a better 1.7B
  appears. `--fetch` downloads the recommended one, verified against the pin, the way
  `dictate-model` fetches whisper's — and lands it in `~/.local/share/lazyos/models/`, which a
  rebuilt image keeps.
- **A vendored `tools/llama.cpp/`.** Measured: a platform build (`-DGGML_CPU_ARM_ARCH=armv8.2-a+dotprod+fp16`,
  `-DGGML_OPENMP=OFF`, LTO) took 396 s and measured **11.14** against the package's **10.95** at
  correct settings, because `libggml0` already ships runtime-dispatched backends and
  `llama-bench` loads `libggml-cpu-armv8.2_2.so` (1867 `sdot`) on this chip. +1.7 % is not a
  package to maintain.
- **A server, an API, or a daemon.** One process while the window is open; `llama-cli` exits with
  the window. Nothing here answers a socket.
- **`ollama`.** It is installed on the maintainer's machine, outside every package list, and it
  inherits `nproc`, so it sits in the 6.67 tok/s regime unless it is given
  `PARAMETER num_thread 4`. This spec does not manage it: the CLI covers the same ground and the
  unit is the user's to keep or drop.
- **A port to `lazyos`.** The command is 200 lines of shell that execs another program and reads a
  pin file. Spec 001's rule is that a surface moves when it *needs* the framework; a chat REPL
  drawn by `llama-cli` is not one the framework can draw.

## Design

### Why each flag, and what it costs to lose it

`-t 4` is the whole performance story (+64 % over the default). The cliff is not subtle: the A76
cluster is 31.9 GMAC/s on one core and 103.5 on four, the A55 cluster 4.1 and 15.7, and eight
threads unpinned measured 66.8 GMAC/s — *slower* than four. Debian's package feels it twice
because it is built with OpenMP and libgomp spins its thread pool across the clusters; a local
`-DGGML_OPENMP=OFF` build held 9.98 at `-t 8` where the package fell to 6.67. So `-t 8` is not
"a bit slower", it is the difference between a usable answer and waiting twice.

`taskset -c 4-7` is kept and buys **nothing** (11.19 unpinned against 11.14 pinned — the scheduler
already lands four threads on the A76s). It stays because it keeps the *desktop* off those four
cores, and it is skipped when `nproc` is under 8, so it cannot pin a smaller machine to cores that
do not exist.

Q4_0 over Q4_K_M is the cheaper model winning on this chip: +83 % prefill (84.66 against 46.16
tok/s) and +5 % decode, because Q4_0 is the shape the repacked dotprod kernels are fastest at, and
it is 1.14 GiB instead of 1.19. The trade is quant quality, and at 1.7B that is real — the manual
says so instead of hiding it.

`-fa on -ctk q8_0 -ctv q8_0` is the memory half: 2356 MiB peak at 4k context with an 1800-token
prompt actually processed, against 2584 with f16 KV, against 2544 MiB available with the session
up. It is not a micro-optimisation, it is whether the model fits beside a browser. Things that did
**not** help, measured and therefore not shipped: `-b 256 -ub 128` (2358, inside noise) and a
smaller `-c` — the floor is the 1280 MiB of weights.

`-rea off` is the one decision that is about the reader rather than the machine: Qwen3 emits a
thinking block first, several hundred tokens, i.e. ~45 s of silence before the answer at 11 tok/s.
It is one variable at the top of the script, and the manual names it.

### Where the hash lives

`--fetch` builds the address from a table in the script and asks `models.sha256` for the expected
hash. Two files, two facts, no duplication: a download cannot disagree with `--verify`, and
`check-system.sh` reads the same pins. A `.part` file is verified and then renamed, so a killed
download never leaves something that looks like a model; a mismatch removes it and says so. There
is no `--fetch` of an unpinned model — an address without a pin is a file of unknown provenance.

### Degrading, not lying

| state | the window does |
|---|---|
| no model downloaded | exits 3 after a card naming `lazyos-model --fetch`; a terminal also gets the reason on stderr |
| `llama-cli` missing | exits 3 with a card naming `apt install llama.cpp-examples` |
| a model not matching its pin | `--verify` names the file and fails; nothing is deleted behind the person's back |
| the float rule missing | the window tiles: ugly, working, and `check-system.sh` says why |

### A key has no terminal, and `[ -t 0 ]` does not say so

The first version of `window()` decided "I already have a terminal" with `[ -t 0 ]`,
and under the key it did: **sway's `exec` inherits the VT the compositor was
started on** (`fd/0 -> /dev/tty7`), so the script skipped `foot` and started
`llama-cli` on a screen nobody was looking at. That is the same trap
`lazyos-profile` documents for `rofi-dev`'s prompt, found here in its window
decision. The predicate is `on_terminal()`: a tty **and** this process is its
foreground process group (`ps -o tpgid=` equals `ps -o pgid=`), which is true in
a shell, an ssh session and inside `foot`, and false for a child of the
compositor. Both paths are exercised - `swaymsg exec` produces one floated window,
`script -qec` runs in place - and `check-system.sh` FAILs on a wrapper that went
back to the bare test, because the failure is otherwise a key that does nothing.

## Acceptance criteria

- [x] `Alt+Ctrl+M`, the root menu's **Local model** row and the launcher entry all open a floated
      997x696 window running the model — observed on this Hayato, reading the compositor
      (`swaymsg -t get_tree`: `floating: "user_on"`, `997x696` at `183,123`; foot's client-side
      decoration takes the 3 px of the 1000x700 the rule asks for).
- [x] the key's path and a real terminal are told apart: `swaymsg exec -- 'lazyos-model …'`
      produces exactly one window, and the same command under `script -qec` runs in place with no
      second window — observed, after the first version of `window()` mistook the inherited VT for
      a terminal and started the model with no window at all.
- [x] generation through the wrapper measures **10.5–11.3 tok/s**, not the default's 6.67 —
      observed as llama.cpp's own `[ Prompt: 70.0 t/s | Generation: 10.5 t/s ]` and
      `[ Prompt: 80.9 t/s | Generation: 11.2 t/s ]` lines.
- [x] the wrapper answers `--list`, `--flags`, `--verify`, `--fetch`, `--bench`, `--help`, and
      exits 0/1/2/3 as documented — observed under `dash`, from a terminal and from a stub `foot`.
- [x] `--fetch` refuses a file whose SHA256 is not the pinned one and keeps nothing — observed
      with a fixture and a stub `curl` in a throwaway `$HOME` (`files left: 0`, rc=1), and an
      unpinned name is refused before anything is downloaded.
- [x] `--verify` reports both measured GGUFs `OK` against `models.sha256` — observed on the device.
- [x] witness: `scripts/check-system.sh` FAILs when `llama-cli`, the wrapper's execute bit, the
      pins file, the float rule, the binding, the menu row, the `.desktop` entry or
      `dictate-model.png` disappears, when the wrapper decides "a terminal" with a bare `-t 0`,
      and when a downloaded model does not match its pin; WARNs when the flags lose `-t 4` or
      `-fa on`/q8_0; SKIPs when no model is downloaded. Both directions were run.
- [x] witness: a paragraph in `uis/sway/files/usr/share/doc/lazyos/04-software.md` § *A local
      model* carries the three numbers, the fetch, and what the machine will not do; § *Panels,
      lock and power* in `03-shortcuts.md` lists the key.
- [x] witness: an `AGENTS.md` bullet records why there is no `tools/llama.cpp/`, the four flags
      with their measurements, the pin-only hash, the VT trap and the witnesses.
- [x] `cargo test --workspace` stays green: this spec adds no Rust, and `rust/` is untouched, so
      the obligation is that nothing else moves.
- [x] the hardware observation is in `reports/2026-10-28-local-model-016.md`.

## Tests

- **Unit** (`cargo test --workspace`): no new Rust. The spec adds shell, pins and configuration, so
  the only Rust obligation is that the workspace stays green — CI runs it, and a red
  `cargo test` here means this spec touched something it said it would not.
- **Shell**: `sh -n` under `dash` (the session's `/bin/sh`), and every documented exit path
  exercised by hand and by `check-system.sh`: `--help` 0, an unknown flag 2, a missing model 2, no
  models at all 3, Escape in `fzf` 1.
- **The two launch paths**: `swaymsg exec -- 'lazyos-model <model>'` must produce exactly one
  `foot` with `app_id=lazyos-model`, and the same command under `script -qec` (a real pty) must run
  in place and open nothing. The first is what the key does; the second is what a terminal does.
  `check-system.sh` FAILs if the wrapper goes back to deciding that with a bare `[ -t 0 ]`, since
  the VT sway's `exec` inherits makes that test true and the window then never opens.
- **`--fetch`**: a stub `curl` that copies a fixture, in a throwaway `$HOME` whose pins file names
  the fixture's hash — the file lands, the `.part` is gone; then a pins file whose hash is wrong,
  and nothing is kept. This is the test that matters, because the real one moves 1.2 GB.
- **Integration**: `check-system.sh` on the device, with the new lines read in both directions —
  passing, and failing against a `$HOME` with no wrapper (the FAIL text names what is missing).
- **Hardware**: the window's geometry and float (above), the generation rate, and a session restart
  (the row, the key and the launcher entry come back with no model downloaded, and say so).

## Risks

- **1.7B is a small model, and it answers like one.** The honest framing is in the manual: this is
  a question you did not want to send to a server, not a replacement for the agent CLIs, which run
  in a window of their own and are someone else's compute.
- **The memory wall is 188 MiB away** (2356 used at 4k context against 2544 free). A bigger `-c`,
  a second browser window or a model with a larger vocabulary puts the machine into zram during
  generation, which is not corruption but is a stall. The manual says what to close, and the flag
  set is one file to change.
- **forky's `llama.cpp` is a snapshot.** A model whose architecture is newer than 1:0.7.0 loads as
  "unknown architecture" rather than answering. That is the one reason to revisit the build
  decision, and it is a two-line change to `packages/tools.list` plus a `tools/` directory, not a
  redesign.
- **The weights come from an unofficial repo** (`bartowski/Qwen_Qwen3-1.7B-GGUF`; upstream ships
  only Q8_0). The pin makes the file the one that was measured; it cannot make the provenance
  better than it is, and `models.sha256` says where they came from in the manual.
