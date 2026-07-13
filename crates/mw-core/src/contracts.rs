//! Shared trait contracts implemented by the other crates.
//!
//! These are the versioned seams between the kernel and everything that plugs
//! into it: scenario packs (rules + manifest), soul policies (brains), and the
//! text backend (dialogue rendering). Kept deliberately minimal — v0 only needs
//! the shapes fixed, not the implementations.

use crate::entity::{EntityId, StatRegistry};
use crate::hash::FnvHasher;
use crate::intent::{Intent, RejectReason};
use crate::rng::AgentRng;
use crate::world::World;

/// Typed argument kinds for a tool descriptor.
#[derive(Clone, Debug)]
pub enum ArgKind {
    /// Pointer into observed entities.
    EntityRef,
    /// Bounded scalar parameter.
    Scalar,
    /// Discrete choice with `variants` options.
    Enum { variants: u32 },
}

#[derive(Clone, Debug)]
pub struct ArgSchema {
    pub name: String,
    pub kind: ArgKind,
}

/// One entry in the action manifest — an MCP-tool-shaped action the body
/// affords, with a typed arg schema the brain's pointer/param heads target.
#[derive(Clone, Debug)]
pub struct ToolDescriptor {
    pub id: u32,
    pub name: String,
    pub args: Vec<ArgSchema>,
}

/// The scenario's digital-body API surface. The kernel executes these; brains
/// only ever pick among the currently afforded subset.
#[derive(Clone, Debug, Default)]
pub struct ActionManifest {
    pub tools: Vec<ToolDescriptor>,
}

impl ActionManifest {
    pub fn empty() -> Self {
        Self::default()
    }
}

/// A scenario pack: schemas + rules layered on the kernel. Implemented by
/// crates like `mw-village`. The kernel applies its own base rules first, then
/// defers to [`ScenarioPack::validate`] / [`ScenarioPack::apply`] for
/// scenario-specific behavior.
pub trait ScenarioPack {
    fn manifest(&self) -> &ActionManifest;

    /// Scenario rules on top of the kernel's base validation.
    fn validate(&self, world: &World, actor: EntityId, intent: &Intent)
        -> Result<(), RejectReason>;

    /// Scenario-specific effects, applied after the kernel's base effect.
    fn apply(&self, world: &mut World, actor: EntityId, intent: &Intent);

    /// Declare the needs/stats this pack tracks. Called once at world init.
    fn register(&self, registry: &mut StatRegistry);

    /// Bitmask of tools `entity`'s body currently affords (bit `i` = tool id
    /// `i`). The kernel builds one [`Observation`] per decide and passes it in,
    /// so the pack reads neighbor proximity from `obs` instead of re-observing —
    /// the mask and the observation are computed from a single scan. Default:
    /// the whole manifest is afforded; packs override per body state.
    fn afforded_tools(&self, _world: &World, _entity: EntityId, _obs: &Observation) -> u32 {
        let n = self.manifest().tools.len();
        if n >= 32 {
            u32::MAX
        } else {
            (1u32 << n) - 1
        }
    }

    /// Fold this pack's per-entity state into the world's canonical hash, in
    /// canonical (entity-id / cell) order. The kernel hashes only positions and
    /// the tick; a pack owns the rest (needs, inventories, ground items), so
    /// without this seam replay verification would miss it. Default: no-op (a
    /// stateless pack contributes nothing).
    fn hash_state(&self, _h: &mut FnvHasher) {}

    /// Advance this pack's per-entity state analytically from `from_tick` to
    /// `to_tick` (the cold LOD ring / AFK fast-forward): no per-tick intents,
    /// just closed-form need integration. Must be a pure function of
    /// `(seed, from, to)` so replay reproduces it exactly. Default: no-op.
    fn fast_forward(&self, _world: &mut World, _from_tick: u64, _to_tick: u64) {}
}

/// A character's brain. Given a fixed-shape observation and its own RNG stream,
/// it returns one intent. The kernel decides whether that intent is legal.
pub trait SoulPolicy {
    fn decide(&mut self, observation: &Observation, rng: &mut AgentRng) -> Intent;
}

/// One K-nearest neighbor slot. Always present in the fixed-size array; empty
/// slots carry `present = false`.
#[derive(Clone, Copy, Debug)]
pub struct NeighborSlot {
    pub present: bool,
    pub id: EntityId,
    /// Relative position from the observer.
    pub dx: i32,
    pub dy: i32,
}

/// Number of nearest neighbors surfaced per observation (v0 value).
pub const K_NEAREST: usize = 4;

/// Fixed-size structured observation — the versioned API between game and
/// brain. Its shape is independent of world population: a bigger world fills
/// the same slots, it never grows the struct. This is what lets a SOUL net be
/// retrained without touching the sim.
#[derive(Clone, Debug)]
pub struct Observation {
    pub tick: u64,
    pub self_pos: (i32, i32),
    pub neighbors: [NeighborSlot; K_NEAREST],
    /// Rolling summary of world activity (event-log length so far). Placeholder
    /// for the richer per-entity event summary the encoder will add later.
    pub event_count: u64,
    /// Bitmask of currently afforded tools (bit `i` = tool id `i`).
    pub tool_mask: u32,
}

/// A committed speak act awaiting rendering. Assembled by the kernel; consumed
/// by a [`TextBackend`].
#[derive(Clone, Copy, Debug)]
pub struct SpeakRequest<'a> {
    /// Persona/genome handle of the speaker.
    pub persona: u64,
    pub act: u32,
    pub topic: u32,
    /// Free-form context (recent events, relationship, scene).
    pub context: &'a str,
}

/// Renders dialogue for an act SOUL has *already* committed — TEXT verbalizes,
/// it never decides. Sync-agnostic: a blocking implementation returns the line
/// directly; a queuing implementation may return a placeholder and fill it in
/// asynchronously.
pub trait TextBackend {
    fn render(&self, request: &SpeakRequest<'_>) -> String;
}
