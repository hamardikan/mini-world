//! Per-entity RNG streams.
//!
//! Every draw is derived from `(world_seed, entity_id, stream_tag, tick)`, so an
//! entity's randomness depends only on *its own* identity and the current tick —
//! never on how many other entities were processed first. That makes an entity's
//! *draws* order-free.
//!
//! Reorder independence is NOT claimed for shared-cell *effects* (Give/Pickup/
//! Drop, where two actors race for the same tile): those genuinely depend on who
//! resolves first. The kernel therefore ratifies a **canonical apply order** —
//! `World::apply_intents` sorts each tick's batch by entity id before executing —
//! so the outcome is deterministic regardless of submission order.

use crate::entity::EntityId;
use crate::hash::{splitmix64, FnvHasher};
use rand_core::RngCore;
use rand_pcg::Pcg64Mcg;

/// Distinguishes independent streams belonging to the same entity (e.g. policy
/// decisions vs. effect resolution) so they never draw the same numbers.
pub type StreamTag = u64;

pub mod stream {
    use super::StreamTag;
    /// Stream the policy uses to pick an intent.
    pub const SOUL: StreamTag = 0;
    /// Stream the executor uses to resolve stochastic effects.
    pub const EFFECT: StreamTag = 1;
}

pub struct AgentRng(Pcg64Mcg);

impl AgentRng {
    pub fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0.next_u64()
    }

    /// Uniform integer in `0..n` (Lemire's method — no modulo bias). `n` must be
    /// non-zero.
    pub fn range_u32(&mut self, n: u32) -> u32 {
        debug_assert!(n != 0);
        let mut m = (self.next_u32() as u64).wrapping_mul(n as u64);
        let mut low = m as u32;
        if low < n {
            let threshold = n.wrapping_neg() % n;
            while low < threshold {
                m = (self.next_u32() as u64).wrapping_mul(n as u64);
                low = m as u32;
            }
        }
        (m >> 32) as u32
    }
}

/// Build a fresh, stateless RNG stream for one entity/tag at one tick.
pub fn agent_rng(seed: u64, entity: EntityId, tag: StreamTag, tick: u64) -> AgentRng {
    let mut h = FnvHasher::new();
    h.write_u64(seed);
    h.write_u32(entity.index());
    h.write_u32(entity.generation());
    h.write_u64(tag);
    h.write_u64(tick);
    let key = h.finish();
    let hi = splitmix64(key) as u128;
    let lo = splitmix64(key ^ 0xD1B5_4A32_D192_ED03) as u128;
    AgentRng(Pcg64Mcg::new((hi << 64) | lo))
}
