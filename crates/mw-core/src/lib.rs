//! mini-world deterministic kernel.
//!
//! Owns all simulation truth and the contracts the rest of the platform plugs
//! into. The load-bearing guarantee is determinism: given the same seed and the
//! same validated-intent log, a run reproduces bitwise-identical state on any
//! machine — that is what makes AFK fast-forward, replay, and multiplayer
//! verification possible (see `DESIGN.md`, load-bearing decision 1).

mod contracts;
mod entity;
mod hash;
mod intent;
mod pack;
mod rng;
mod world;

pub use contracts::{
    ActionManifest, ArgKind, ArgSchema, NeighborSlot, Observation, ScenarioPack, SoulPolicy,
    SpeakRequest, TextBackend, ToolDescriptor, K_NEAREST,
};
pub use entity::{Entity, EntityId, StatRegistry};
pub use intent::{Event, Intent, LoggedIntent, RejectReason};
pub use pack::KernelPack;
pub use rng::{agent_rng, stream, AgentRng, StreamTag};
pub use world::World;
