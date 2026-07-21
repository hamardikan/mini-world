# Browser simulation layer plan

Status: proposed future work. This document is an implementation-ready contract for the first web slice; it does not claim that a browser layer exists.

Related documents:

- [README](../README.md), for the shipped kernel, CLI, and Ratatui viewer.
- [DESIGN](../DESIGN.md), for the ratified kernel, replay, LOD, dialogue, and model contracts.

## 1. Scope and status vocabulary

This plan describes one local browser observatory and control surface for one host-owned village run. It adds no hosted service, cloud dependency, multiplayer protocol, or browser-owned simulation state.

- **Observed**: supported by the repository or the recorded review.
- **Decision**: the contract selected for implementation.
- **Future work**: not present in the repository today.
- **Target**: a budget or threshold to measure, not a current result.

The repository currently has no server, frontend, web assets, or `SimulationController`. Every endpoint, DTO, queue, and UI behavior below is **Future work** unless explicitly marked **Observed**. Existing v0/v0.5 scientific measurements remain unchanged.

## 2. Current versus planned

### Observed today

- `mw-core` owns a deterministic fixed-timestep world, seed, tick, entities, pack state, kernel event/intent logs, and canonical state hash.
- SOUL emits intents; the kernel validates and applies them. TEXT renders committed dialogue asynchronously and never mutates world state.
- `World::step_gated` enforces live LOD; cold analytic progress is represented by `FfSegment` in the validated world log and replay consumes it.
- `mw-sim::view::App` owns the current live world and Ratatui UI. Its fast-forward key is report-only and does not mutate the live viewer world.
- Current trajectory serialization is schema version 2. It is not a web Snapshot/Event/Command API.
- There is no HTTP server, SSE publisher, frontend, web bundle, or shared controller abstraction.

### Planned first vertical slice

The first slice is exactly two phases:

1. **Phase 0 — shared controller/hash parity.** Extract one host-owned `SimulationController` seam around the current simulation ownership, preserve the normal kernel pipeline, and prove controller/TUI direct-pipeline hash parity. Define the web DTO families and actor contracts, but expose no web UI requirement beyond the contracts.
2. **Phase 1 — minimal local web slice.** Construct one deterministic run on the actor thread; serve `GET /v1/snapshot`, authoritative `GET /v1/capabilities`, `POST /v1/commands` for only `step {ticks: 1}`, and `GET /v1/events` as SSE with reconnect. Render a 16x16 Canvas 2D map plus a DOM summary, keyboard agent selection, command/connection status, and reduced-motion behavior.

Phase 1 is complete only when its HTTP/SSE and browser acceptance tests pass. It does not include an automatic simulation loop.

## 3. Decisions and explicit deferrals

### 3.1 Selected architecture

- **One host-owned actor:** the actor owns `World`, `VillagePack`, SOUL, Director, validated intent log, ControlLog, idempotency table, and the latest immutable projection. The browser and adapters hold projections only.
- **Actor construction/threading:** the host creates the complete `SimulationController`/actor state inside the actor thread (or single simulation task) before publishing readiness. HTTP handlers, SSE publishers, and TUI code never construct or borrow `World` or `VillagePack` directly.
- **Multiplexed actor input:** one bounded FIFO actor queue carries `Command` envelopes, asynchronous TEXT results when introduced, and shutdown messages. The actor processes one message at a time; TEXT results cannot starve commands or shutdown because the queue has explicit message classes and a fairness rule (at most one result batch between command/shutdown opportunities).
- **HTTP + SSE:** synchronous HTTP `200` is selected for first-slice commands. A handler validates and sends one envelope to the actor over a oneshot; the actor returns the same command-result object before the handler responds and publishes the matching SSE event. SSE is one-way delivery; it is not a second command channel.
- **JSON and DTO families:** use React + TypeScript + Vite, JSON, loopback HTTP, and SSE. Use one accessible component system (Radix primitives with a small local CSS layer), native Canvas 2D for the map, and DOM for all controls, summaries, status, and accessibility. `CapabilitiesV1`, `SnapshotV1`, `EventV1`, `CommandV1`, `CommandResultV1`, and `ErrorV1` are one jointly versioned web DTO compatibility family: every member carries the same integer `schema_version`, and any breaking change to one member increments the joint version. `ControlLogV1` is a separately versioned persistent audit log. The current trajectory schema version 2 is unrelated.
- **Capabilities authority:** `GET /v1/capabilities` is authoritative for enabled command types, limits, and DTO versions. The UI must not infer capability from a button, route, or snapshot field.
- **Loopback boundary:** bind to `127.0.0.1` and fail closed for a non-loopback bind. No authentication or remote-exposure claim is made.

### 3.2 Explicitly deferred

The first slice explicitly defers pause/resume, fast-forward (preview or authoritative), focus commands, multi-step commands, inspector/detail fields, dialogue/TEXT backfill, policy hot-swap, reset/reseed, WebSocket, binary deltas, WASM, WebLLM/browser inference, remote exposure/authentication, cloud hosting, multiplayer, and any automatic wall-clock tick loop. `set_focus` is a later provenance-only control command; it is not part of Phase 1.

## 4. Minimal Phase 1 surface

### 4.1 `GET /v1/snapshot` — SnapshotV1

SnapshotV1 is a recovery baseline and rendering projection, not a second world model. It is intentionally minimal:

```json
{
  "schema_version": 1,
  "run_id": "local-01",
  "seed": "1",
  "scenario": {"id": "village", "version": 1},
  "tick": "120",
  "state_hash": "0x0123456789abcdef",
  "run_provenance": {
    "policy_id": "utility-v0",
    "model_hash": "sha256:…",
    "backend_id": "rust-utility",
    "expertise": "capable"
  },
  "grid": {"width": 16, "height": 16, "tiles": []},
  "agents": [
    {"id": {"index": 0, "generation": 0}, "position": [8, 8]}
  ],
  "event_seq": "240"
}
```

`tiles` uses the scenario's versioned compact tile representation; `agents` contains only stable identity and position in Phase 1. There are no needs, inventory, persona, opinions, memory, dialogue, inspector, feed, or Director details in this snapshot. Selection is UI-local and is not simulation focus. `event_seq` is the SSE baseline cursor.

Every Rust `u64` serialized into JSON, including `seed`, `tick`, `event_seq`, IDs when represented as `u64`, sequence values, and limits, is a decimal string. Hashes are lowercase hexadecimal strings. Coordinates and bounded dimensions may remain JSON numbers because they are fixed-width non-`u64` DTO values.

`run_provenance` is fixed at run creation and immutable for a `run_id`. `policy_id`, `model_hash`, `backend_id`, and `expertise` are always exposed together; a policy/backend change requires a new run and new `run_id`, never a hot swap.
`run_id` is generated from a timestamp plus a monotonic per-process counter (or a UUID) and is metadata, never a simulation input; it cannot collide for two runs in one process.

### 4.2 `GET /v1/capabilities` — CapabilitiesV1

CapabilitiesV1 is authoritative and participates in the jointly versioned web DTO family:

```json
{
  "schema_version": 1,
  "run_id": "local-01",
  "run_provenance": {
    "policy_id": "utility-v0",
    "model_hash": "sha256:…",
    "backend_id": "rust-utility",
    "expertise": "capable"
  },
  "dto_versions": {
    "capabilities": 1,
    "snapshot": 1,
    "event": 1,
    "command": 1,
    "command_result": 1,
    "error": 1,
    "control_log": 1
  },
  "scenario": {"id": "village", "version": 1},
  "commands": [{"type": "step", "ticks": {"min": 1, "max": 1}}],
  "limits": {"command_queue": "256", "event_ring": "4096"}
}
```

The only enabled command in Phase 1 is `step` with exactly one tick. Additive fields within the joint web DTO family are allowed when old clients can ignore them; changing required meaning or field types increments the joint family version. A client rejects an unsupported joint version rather than partially applying it.

### 4.3 `POST /v1/commands` — CommandV1 and CommandResultV1

Phase 1 accepts only:

```json
{
  "schema_version": 1,
  "run_id": "local-01",
  "command_id": "01J00000000000000000000000",
  "expected_tick": "120",
  "command": {"type": "step", "ticks": 1}
}
```

`command_id` is an opaque client id. `expected_tick` is required for every tick-advancing command; a later provenance-affecting but non-advancing command such as `set_focus` carries no `expected_tick` and is instead actor-sequenced in ControlLogV1 with its applied tick. The actor's processing order is:

1. Parse and validate the request and DTO version, including `run_id`, command shape, bounds, and required fields.
2. Perform the idempotency lookup using the validated request's exact validated body bytes (no semantic reserialization).
3. If the id exists with byte-identical request bytes, return the stored original result **even when its `expected_tick` is now stale**; do not reapply or re-check the current tick.
4. If the id exists with different payload/bytes, return `idempotency_conflict` and do not mutate state.
5. For a new id, compare `expected_tick` with the actor's current tick; mismatch returns `tick_conflict` without changing world state.
6. For a new id with a matching tick, execute exactly one normal gated tick, store the result, and publish its event.

This ordering is mandatory: validation precedes lookup, lookup precedes `expected_tick`. Every Rust `u64` in request/response JSON is string encoded; `ticks: 1` is a bounded DTO integer literal because Phase 1 permits only the fixed value 1.

The selected response is HTTP `200` with the actor's oneshot result. A newly accepted command returns one `CommandResultV1` and the matching event has the same `command_id`, applied tick, and state hash. Rejected commands return typed `ErrorV1` with HTTP `400` for malformed/schema errors and `409` for run, tick, or idempotency conflicts. No rejection changes tick, hash, pack state, intent log, or event cursor.

### 4.4 `GET /v1/events` — EventV1 SSE

SSE uses `id: <event_seq>`, `event: <event_type>`, and one JSON `data:` object. `Last-Event-ID` and `?after_seq=` are equivalent cursors. EventV1 carries the joint web-family `schema_version` and includes `run_id`, string `event_seq`, string `tick`, `state_hash`, `event_type`, and a typed payload. Phase 1 emits the accepted step's command result/tick event; heartbeats are comments and have no simulation meaning.

The event ring is bounded. If `after_seq` is older than the retained horizon, the server emits a typed `snapshot_required` recovery event (or equivalent typed gap response), and the browser fetches SnapshotV1 before reconnecting. It never interpolates missing ticks. Reconnect must preserve `run_id`, apply events strictly monotonically, ignore duplicates, and recover on a future gap. Idempotency-result retention must cover the reconnect/event-ring horizon: an idempotency row cannot be evicted while a matching command result could still be needed for a retained/reconnecting client; if retention policy cannot guarantee this, the server must retain the row for the active run instead.

## 5. Replay and provenance contracts

The **validated world log** remains authoritative. Authoritative log replay uses the run seed/config plus the validated intent log (including any future `FfSegment` records) and the scenario pack to reconstruct the state hash. It does not require re-running SOUL or trusting browser commands.

A distinct future **seed + ControlLog replay** starts from the seed/config and replays accepted controls to recompute Director inputs and policy decisions. It is diagnostic/re-simulation evidence, not authoritative replay: model/backend nondeterminism can produce a different intent stream and hash. A UI must label these modes differently and never present seed+ControlLog output as proof of canonical state.

ControlLogV1 is defined now for later provenance work. Each accepted actor control entry has `schema_version`, `run_id`, string `control_seq`, string `command_id`, string `applied_tick`, `control_type`, canonical command payload, and the resulting `state_hash` when a control changes simulation inputs. It is append-only, actor-sequenced, versioned independently alongside the validated intent log, and persisted with the run's replay metadata. It is supplementary to the validated world log. Later `set_focus {x,y}` is provenance-only: it does not advance a tick or change the canonical hash by itself, is sequenced through the actor, and is recorded in ControlLogV1 so a seed+ControlLog diagnostic replay can explain the Director input. It is not a Phase 1 command.

Run provenance is fixed per `run_id` and includes `policy_id`, `model_hash`, `backend_id`, and `expertise`; those values are included in CapabilitiesV1, SnapshotV1, and future replay metadata. Policy hot-swap is deferred and would require a new run.

## 6. Actor, TEXT, shutdown, and backpressure

The actor is constructed on its own thread/task and publishes its first immutable SnapshotV1 only after construction succeeds. Its input enum multiplexes `Command`, `TextResult` (when dialogue is later introduced), and `Shutdown`. Commands and shutdown have priority over render completions; a fairness bound prevents an unbounded TEXT-result burst from starving either. TEXT remains render-only and cannot advance ticks or mutate hashes.

HTTP handlers validate without touching the world, enqueue one message, and await a oneshot for the synchronous `200` result. SSE clients consume immutable events from a bounded ring; slow clients never block the actor. Proposed limits are **Target/pending measurement**, not current capacities: command queue 256, event ring 4,096 events, per-client queue 256 events or 1 MiB, and a 500-line UI tail only when a later feed exists. Measure before changing JSON, SSE, or retention.

Shutdown stops new commands, replies with typed stopping errors, emits `server_stopping`, drains/rejects queued work deterministically, and joins the actor. A new host start gets a new `run_id`; browser state is discarded rather than treated as simulation truth.

## 7. Browser slice

The Phase 1 page uses React + TypeScript + Vite and one eventual design system. Canvas 2D draws the 16x16 terrain and agent positions from the latest accepted SnapshotV1. Adjacent DOM text mirrors the map and selection; color is never the only distinction. The page provides semantic headings, keyboard-reachable agent selection, visible focus, connection/run/tick/hash status, command accepted/rejected status in an `aria-live` region, and a reduced-motion path. No inspector/detail panel, dialogue rail, feed, pause control, or focus command is required in Phase 1.

With `prefers-reduced-motion: reduce`, apply snapshots immediately, preserve focus, disable interpolation/pulsing/auto-scroll, and keep the same information and controls. Loading, snapshot error, SSE disconnect, sequence gap, run restart, and typed command error states must preserve the last known projection and explain recovery; no mutating command is silently retried with a new id.

## 8. Phase acceptance tests

### Phase 0 — shared controller/hash parity

- A controller-created run and the existing direct pipeline with identical seed/config and fixed ticks produce identical state hashes and pack state.
- Validation failure occurs before idempotency lookup; malformed requests do not consume an idempotency key.
- A first valid command is accepted, then a retry with identical canonical bytes returns the original result after the actor tick has advanced; it does not reapply, append a second intent, or fail stale `expected_tick`.
- Reusing a command id with any different payload returns `idempotency_conflict`, including a different `expected_tick` or command body.
- A new command with stale `expected_tick` returns `tick_conflict` and changes no tick, hash, pack state, intent log, ControlLog, or event sequence.
- When authoritative fast-forward is introduced, one `FfSegment` is appended and authoritative intent-log replay reproduces the final hash; seed+ControlLog replay is separately labeled and is not asserted hash-equal.
- Snapshot, capability, event, command, result, and error fixtures assert the joint DTO-family version, additive compatibility, breaking-version rejection; ControlLog fixtures assert its parallel version; all fixtures assert string encoding for every Rust `u64`, including seed.
- Run provenance exposes `policy_id`, `model_hash`, `backend_id`, and `expertise`, and remains byte-for-byte fixed for one `run_id`.
- Actor construction occurs on the actor thread; a delayed TEXT result cannot starve a command or shutdown, and TEXT completion does not alter tick/hash.
- TUI/controller command sequences and direct pipeline runs have matching hash transitions; no adapter owns a second world.

### Phase 1 — minimal local host and browser

- A fixed seed/config starts one actor and `GET /v1/snapshot` returns minimal SnapshotV1; `GET /v1/capabilities` is authoritative and advertises only `step {ticks:1}`. Health, if exposed, matches snapshot tick/hash after readiness.
- `POST /v1/commands` with valid `step {ticks:1}` and current `expected_tick` returns HTTP 200, advances exactly one tick, and emits one matching EventV1.
- Malformed JSON/schema, wrong run id, unsupported command, stale tick, and idempotency conflict return typed errors without world transition.
- Connect SSE, record `event_seq`, disconnect, advance once, reconnect with `after_seq`, and receive exactly the missing event. Force an event-ring miss and verify `snapshot_required`, fresh snapshot replacement, and resume; no event is silently dropped or interpolated.
- Every Rust `u64` in all JSON bodies is a string; DTO-family version mismatch is rejected without partial application; run provenance is present and unchanged.
- Browser smoke renders the 16x16 Canvas map and equivalent DOM summary, selects an agent by keyboard, exposes tick/hash/connection/command status, and passes the no-motion behavior under reduced-motion preference.
- The actor remains responsive while an SSE client is slow; browser accessibility covers headings, labels, focus order, visible focus, non-color status, keyboard selection, live command status, loading, error, disconnected, gap, and restart states.

## 9. Later phases (not first-slice scope)

Later work may add shared TUI/browser controls and inspector fields, then authoritative fast-forward and render-only dialogue/TEXT backfill, then measured hardening. Those phases must retain the contracts above: `expected_tick` for tick advancement, ControlLogV1 for actor-sequenced provenance-only controls, authoritative intent-log replay, fixed per-run provenance, actor/TEXT isolation, bounded SSE recovery, and DTO-family compatibility. No later phase is implied to exist by this plan.

## 10. Rollout and rollback

Roll out behind an explicit opt-in local command or feature flag. Keep the Ratatui/TUI and CLI as the known-good default; do not silently change existing keys or viewer behavior. Land Phase 0 controller/hash-parity fixtures before enabling the host, then enable the Phase 1 host only after its readiness snapshot exists. If a DTO, controller, or hash defect appears, stop/disable the web host and use the TUI/CLI; replay the saved validated intent log with the prior known-good binary. Never repair a hash mismatch in the browser. A new host start creates a new `run_id`; reject in-flight commands during shutdown and clear the browser projection rather than merging stale state.

## 11. Risks and mitigations

| Risk | Mitigation |
| --- | --- |
| Duplicate retry advances the world twice | Validate, idempotency lookup, then `expected_tick`; byte-identical retry returns the stored result; conflicting payloads reject. |
| Browser diverges after an SSE gap | Bounded cursor plus `snapshot_required`; refetch a baseline and never interpolate. |
| TEXT or a slow client blocks simulation | Actor multiplex fairness, render-only TEXT results, bounded queues, and disconnect/recover behavior. |
| Canvas hides required state | DOM summary/list mirrors identity, position, selection, status, and hash; status is not color-only. |
| Local endpoint is exposed | Loopback bind, fail-closed non-loopback handling, same-origin behavior, and no remote/auth claim. |
| Unsupported DTO change corrupts a client | Joint web DTO-family versioning, additive-only compatibility, and fixture rejection of breaking versions. |

## 12. Explicit non-goals

No cloud/remote deployment, authentication, accounts, multiplayer, browser-owned simulation, client prediction, local hash authority, WebSocket, binary deltas, WebGL, WASM, WebLLM/browser inference, automatic wall-clock ticking, pause/resume, fast-forward, focus command, multi-step command, inspector/detail fields, dialogue/TEXT backfill, policy hot-swap, reset/reseed, arbitrary command execution, model management, filesystem browsing, or scientific/training claims belong to the first slice.

## 13. Pending measurement targets

The following are **Target** values, not observations: local release page interactive within 2 seconds; SnapshotV1 below 1 MiB for the approximately 50-agent village; Canvas/DOM update within a 16 ms frame; one-tick HTTP acknowledgement under 100 ms p95 excluding future TEXT work; each event under 64 KiB; SSE bandwidth under 1 MiB/s. Measure on the target workstation and a modest laptop before treating any target as a result or changing retention.

## 14. Links and implementation boundary

The current known-good default remains the Ratatui/TUI and CLI. The browser is opt-in future work and must use the host-owned controller described here. No implementation, dependency, configuration, browser artifact, scientific result, or public deployment is created by this documentation change.
