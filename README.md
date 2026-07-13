# mini-world

A sandbox, semi-AFK simulation platform where **every character is a small on-device AI model**. Characters act through a deterministic world, build memories and relationships, and keep moving while the player is away.

**Status: v0 vertical slice complete.** The shipped slice is a deterministic village with a utility-AI SOUL stub, shared TEXT rendering, latent dialogue, live LOD, analytic fast-forward, replay, and a Ratatui viewer. The architecture and ratified contracts are documented in [DESIGN.md](DESIGN.md).

## Architecture at a glance

SOUL and TEXT have deliberately different jobs. SOUL is the per-character decision policy: it reads a fixed observation and emits an **intent**, never a world mutation. The kernel validates intents, applies them in canonical entity-id order, records the validated log, and owns all simulation truth. The shared TEXT model only verbalizes a committed `speak` act; it is not in the tick loop and cannot change simulation state.

Dialogue is **latent**: an off-screen conversation still applies its mechanical relationship outcome, but costs no TEXT render. When the player focuses the scene, the viewer renders and caches the line; an unobserved row can also be backfilled on demand. This makes dialogue cost track attention rather than population.

The workspace is split into small seams:

| Crate | Responsibility |
| --- | --- |
| `mw-core` | Deterministic integer tick kernel, intent validation/execution/logging, canonical hash, and the minimal `Observation`/`ScenarioPack`/`SoulPolicy`/`TextBackend` contracts. |
| `mw-village` | Village scenario pack: needs, inventory and ground items, action affordances, validation, outcomes, and analytic pack fast-forward/hash state. |
| `mw-agents` | Persona generation, structured memory/opinions, the ratified rich `AgentObs` schema, and the utility-AI SOUL implementation. |
| `mw-text` | Managed llama.cpp `llama-server` bridge for the shared Qwen3-0.6B Q4_0 TEXT backend, with prompt/KV-slot reuse. |
| `mw-sim` | Village wiring, live Director/LOD gates, analytic AFK fast-forward and digest, latent-dialogue demo, soak runner, and Ratatui TUI. |

## Verified results

These are the v0 measurements and gates, not projections:

| Area | Verified result |
| --- | --- |
| Determinism and replay | Same seed for 10,000 ticks produces an identical hash. Replaying `(seed, intent log)` — including `FfSegment` records — reproduces the full state hash, including pack state. |
| Live simulation | 50 agents at **12,893 ticks/s** in release on an M4 Pro; the largest action-histogram share is **37.9%**. |
| Analytic fast-forward | One in-game week (**604,800 ticks**) in **0.014 s** (about **43M ticks/s** analytic); drift against the hot reference is **≤4%** with a **15%** bound, and the digest is deterministic. |
| TEXT latency and cache | Qwen3-0.6B Q4_0 (**359 MiB**, via llama.cpp): warm render **79 ms**; prompt-token work falls from **104 → 1** with KV-slot reuse. |
| TEXT throughput | M4 Pro Metal: **pp512 2691 t/s**, **tg128 193 t/s**; CPU-only: **pp512 388 t/s**, **tg128 76 t/s**. |
| Latent dialogue | Unobserved conversations make **0 `TextBackend` calls** while relationship deltas still apply. Retroactive backfill renders act-coherent lines, caches them, and TEXT never mutates sim state. |
| Gates | **49** unit/integration tests and **4** live-model tests green; `clippy -D warnings` clean; `scripts/demo.sh` exits 0. Ratatui TUI was verified in a real PTY, and `view --smoke` exits 0 headless. |

## Quickstart

### Prerequisites

- Rust and Cargo.
- Optional live TEXT: install llama.cpp and download the documented Qwen3-0.6B Q4_0 GGUF. The default path is `~/.cache/mini-world/models/`, and `MW_MODEL_PATH` overrides it.

```sh
brew install llama.cpp
mkdir -p "$HOME/.cache/mini-world/models"
curl -L --fail --retry 3 \
  -o "$HOME/.cache/mini-world/models/Qwen3-0.6B-Q4_0.gguf" \
  "https://huggingface.co/unsloth/Qwen3-0.6B-GGUF/resolve/main/Qwen3-0.6B-Q4_0.gguf?download=true"
```

The default demos use the offline dialogue renderer, so the model is not needed for the kernel, soak, fast-forward, or non-live viewer checks.

### Run the slice

```sh
cargo test --workspace
bash scripts/demo.sh
cargo run -p mw-sim -- soak
cargo run -p mw-sim -- view
```

`soak` runs the village loop. `scripts/demo.sh` builds release, runs a live day, fast-forwards a week, and exercises observed plus backfilled latent dialogue. `view` opens the interactive Ratatui viewer; use `cargo run -p mw-sim -- view --smoke` for one headless CI-safe frame.

Viewer keys: **arrows** move focus, **Tab** selects an agent, **j/k** move through conversations, **ENTER** backfills the selected latent row, **Space** pauses/resumes, **1** selects 1× speed, **8** selects 8× speed, **F** fast-forwards one day, and **q** quits. Set `MW_TEXT_LIVE=1` when opening `dialogue` or `view` to use the live TEXT backend.

Live model gates are opt-in because they spawn `llama-server`:

```sh
cargo test -- --ignored
```

## Roadmap

**Next: SOUL v1 distillation.** Generate LLM roleplay trajectories and distill them into a **1–5M parameter** policy that plugs into the existing `SoulPolicy` socket.

Backlog:

- SOUL feeding calibration (**~8/50 starve — emergent, real**).
- Async viewer rendering.
- SIGKILL orphan handling.
- Asymmetric opinion deltas.
