//! Persona: the character sheet the SOUL is conditioned on (DESIGN.md §8).
//!
//! A fixed-size trait vector plus per-need weights, all `i16` fixed-point, built
//! deterministically from `(seed, entity_id)` through a kernel RNG stream. No
//! persona bytes are ever stored: identity lives in the seed, so replay and AFK
//! fast-forward regrow the exact same population (DESIGN.md, load-bearing
//! decision 2 — "identity lives in data, not weights").

use mw_core::{agent_rng, EntityId};

/// Fixed-point scale: `PERSONA_ONE` represents 1.0.
pub const PERSONA_ONE: i16 = 1000;

/// Number of persona trait slots — the versioned layout the SOUL reads.
pub const N_TRAITS: usize = 5;
/// Number of need-weight slots (one per tracked need: hunger, energy, social).
pub const N_WEIGHTS: usize = 3;

/// Faction buckets a persona sorts into (a cheap derived label, no stored state).
pub const N_FACTIONS: u8 = 4;

/// Persona-generation stream tag — distinct from `SOUL`/`EFFECT` so trait draws
/// never collide with decision or effect randomness. ("PERSONA\0" as bytes.)
const PERSONA_TAG: u64 = 0x5045_5253_4f4e_4100;

/// Named indices into [`Persona::traits`]. Fixed layout: reordering is a
/// schema-version change for any trained SOUL that reads it.
pub mod trait_idx {
    pub const AGGRESSION: usize = 0;
    pub const SOCIABILITY: usize = 1;
    pub const INDUSTRIOUSNESS: usize = 2;
    pub const GREED: usize = 3;
    pub const CAUTION: usize = 4;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Persona {
    /// Bounded trait vector, each in `[0, PERSONA_ONE]`.
    pub traits: [i16; N_TRAITS],
    /// Need-relief weights, each in `[1, PERSONA_ONE]`. Never zero, so every
    /// character still cares at least a little about staying alive.
    pub weights: [i16; N_WEIGHTS],
}

impl Persona {
    /// Deterministic persona for `entity` in a world seeded with `seed`.
    pub fn new(seed: u64, entity: EntityId) -> Self {
        let mut rng = agent_rng(seed, entity, PERSONA_TAG, 0);
        let mut traits = [0i16; N_TRAITS];
        for t in traits.iter_mut() {
            *t = rng.range_u32(PERSONA_ONE as u32 + 1) as i16; // [0, PERSONA_ONE]
        }
        let mut weights = [0i16; N_WEIGHTS];
        // Weights span [PERSONA_ONE/4, PERSONA_ONE]: personas vary in how much
        // they prioritize each need, but every character still cares enough that
        // it will act to stay alive rather than socialize itself to a standstill.
        const FLOOR: u32 = PERSONA_ONE as u32 / 4;
        for w in weights.iter_mut() {
            *w = (FLOOR + rng.range_u32(PERSONA_ONE as u32 + 1 - FLOOR)) as i16;
        }
        Self { traits, weights }
    }

    /// A stable faction bucket derived purely from the trait vector, so an
    /// observer can label a neighbor without any extra stored state.
    pub fn faction(&self) -> u8 {
        let s: u32 = self.traits.iter().map(|&x| x as u32).sum();
        (s % N_FACTIONS as u32) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mw_core::{KernelPack, World};

    #[test]
    fn persona_is_deterministic_from_seed_and_id() {
        let pack = KernelPack::new();
        let mut a = World::with_pack(9, &pack);
        let mut b = World::with_pack(9, &pack);
        let ea = a.spawn((0, 0));
        let eb = b.spawn((0, 0));
        assert_eq!(Persona::new(9, ea), Persona::new(9, eb));
        // A different seed yields a different sheet.
        assert_ne!(Persona::new(9, ea), Persona::new(10, eb));
    }

    #[test]
    fn traits_and_weights_stay_in_range() {
        let pack = KernelPack::new();
        let mut w = World::with_pack(3, &pack);
        for i in 0..64 {
            let e = w.spawn((i, 0));
            let p = Persona::new(3, e);
            assert!(p.traits.iter().all(|&t| (0..=PERSONA_ONE).contains(&t)));
            assert!(p.weights.iter().all(|&x| (1..=PERSONA_ONE).contains(&x)));
            assert!(p.faction() < N_FACTIONS);
        }
    }
}
