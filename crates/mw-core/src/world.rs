//! The deterministic world kernel.
//!
//! Fixed-timestep tick loop, integer-only state, per-entity RNG streams, a
//! validate → execute → log pipeline, and a canonical state hash. There is no
//! wall-clock anywhere in here: the only notion of time is the `u64` tick.

use crate::contracts::{NeighborSlot, Observation, ScenarioPack, SoulPolicy, K_NEAREST};
use crate::entity::{Entity, EntityId, EntityStore, StatRegistry};
use crate::hash::FnvHasher;
use crate::intent::{Event, Intent, LoggedIntent, RejectReason};
use crate::rng::{self, stream, AgentRng, StreamTag};

pub struct World {
    seed: u64,
    tick: u64,
    entities: EntityStore,
    intent_log: Vec<LoggedIntent>,
    event_log: Vec<Event>,
    stats: StatRegistry,
}

impl World {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            tick: 0,
            entities: EntityStore::new(),
            intent_log: Vec::new(),
            event_log: Vec::new(),
            stats: StatRegistry::default(),
        }
    }

    /// Build a world and let the pack register its needs/stats.
    pub fn with_pack<P: ScenarioPack>(seed: u64, pack: &P) -> Self {
        let mut world = Self::new(seed);
        pack.register(&mut world.stats);
        world
    }

    pub fn spawn(&mut self, pos: (i32, i32)) -> EntityId {
        self.entities.spawn(Entity { pos })
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    pub fn entity(&self, id: EntityId) -> Option<&Entity> {
        self.entities.get(id)
    }

    pub fn intent_log(&self) -> &[LoggedIntent] {
        &self.intent_log
    }

    pub fn event_log(&self) -> &[Event] {
        &self.event_log
    }

    pub fn stats(&self) -> &StatRegistry {
        &self.stats
    }

    /// A stateless RNG stream for `entity`/`tag` at the current tick. Because it
    /// is keyed by the tick and never carries state across ticks, entity
    /// processing order within a tick cannot affect any entity's draws.
    pub fn agent_rng(&self, entity: EntityId, tag: StreamTag) -> AgentRng {
        rng::agent_rng(self.seed, entity, tag, self.tick)
    }

    /// Fixed-shape observation for one entity. K nearest others by integer
    /// squared distance, ties broken by slot index so the result is canonical.
    pub fn observe(&self, me: EntityId) -> Observation {
        let self_pos = self.entities.get(me).map(|e| e.pos).unwrap_or((0, 0));

        let mut candidates: Vec<(i64, u32, EntityId, (i32, i32))> = Vec::new();
        for (id, e) in self.entities.iter() {
            if id == me {
                continue;
            }
            let ddx = (e.pos.0 - self_pos.0) as i64;
            let ddy = (e.pos.1 - self_pos.1) as i64;
            candidates.push((ddx * ddx + ddy * ddy, id.index(), id, e.pos));
        }
        candidates.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

        let empty = NeighborSlot {
            present: false,
            id: me,
            dx: 0,
            dy: 0,
        };
        let mut neighbors = [empty; K_NEAREST];
        for (slot, c) in neighbors.iter_mut().zip(candidates.iter().take(K_NEAREST)) {
            *slot = NeighborSlot {
                present: true,
                id: c.2,
                dx: c.3 .0 - self_pos.0,
                dy: c.3 .1 - self_pos.1,
            };
        }

        Observation {
            tick: self.tick,
            self_pos,
            neighbors,
            event_count: self.event_log.len() as u64,
            // v0 affords every kernel tool; packs will mask this per body state.
            tool_mask: 0b1111,
        }
    }

    /// Advance one tick: every entity observes, its policy decides, then the
    /// batch runs through validate → execute → log.
    pub fn step<P: ScenarioPack, S: SoulPolicy>(&mut self, pack: &P, policy: &mut S) {
        let ids: Vec<EntityId> = self.entities.ids().collect();
        let mut batch = Vec::with_capacity(ids.len());
        for id in ids {
            let observation = self.observe(id);
            let mut rng = self.agent_rng(id, stream::SOUL);
            batch.push((id, policy.decide(&observation, &mut rng)));
        }
        self.apply_intents(pack, &batch);
    }

    /// Re-run a world purely from `(seed, intent log)`, bypassing all policies.
    /// Feeding the logged intents back through the same validate → execute path
    /// reproduces the exact state hash. `total_ticks` is replayed in full so
    /// the tick counter lands where the original run left it even across ticks
    /// that logged nothing.
    pub fn replay<P: ScenarioPack>(
        seed: u64,
        init_positions: &[(i32, i32)],
        total_ticks: u64,
        log: &[LoggedIntent],
        pack: &P,
    ) -> World {
        let mut world = World::with_pack(seed, pack);
        for &pos in init_positions {
            world.spawn(pos);
        }

        // The log is globally ordered by tick, so a single cursor slices out
        // each tick's batch in O(n).
        let mut cursor = 0;
        for t in 0..total_ticks {
            debug_assert_eq!(world.tick, t);
            let start = cursor;
            while cursor < log.len() && log[cursor].tick == t {
                cursor += 1;
            }
            let batch: Vec<(EntityId, Intent)> = log[start..cursor]
                .iter()
                .map(|l| (l.actor, l.intent.clone()))
                .collect();
            world.apply_intents(pack, &batch);
        }
        world
    }

    /// Canonical state hash: stable across runs, machines, and architectures.
    /// Iterates entities in canonical slot order and hashes integer state only
    /// via fixed-width little-endian bytes — no `HashMap` iteration, no float
    /// bit patterns.
    pub fn state_hash(&self) -> u64 {
        let mut h = FnvHasher::new();
        h.write_u64(self.tick);
        for (id, e) in self.entities.iter() {
            h.write_u32(id.index());
            h.write_u32(id.generation());
            h.write_i32(e.pos.0);
            h.write_i32(e.pos.1);
        }
        h.finish()
    }

    fn apply_intents<P: ScenarioPack>(&mut self, pack: &P, batch: &[(EntityId, Intent)]) {
        for (actor, intent) in batch {
            match self.validate(pack, *actor, intent) {
                Ok(()) => {
                    self.intent_log.push(LoggedIntent {
                        tick: self.tick,
                        actor: *actor,
                        intent: intent.clone(),
                    });
                    self.execute(pack, *actor, intent);
                }
                Err(reason) => self.event_log.push(Event::Rejected {
                    tick: self.tick,
                    actor: *actor,
                    reason,
                }),
            }
        }
        self.tick += 1;
    }

    fn validate<P: ScenarioPack>(
        &self,
        pack: &P,
        actor: EntityId,
        intent: &Intent,
    ) -> Result<(), RejectReason> {
        self.base_validate(actor, intent)?;
        pack.validate(self, actor, intent)
    }

    /// Kernel base rules that hold for every scenario.
    fn base_validate(&self, _actor: EntityId, intent: &Intent) -> Result<(), RejectReason> {
        match *intent {
            Intent::Move { dx, dy } => {
                if dx.abs() > 1 || dy.abs() > 1 {
                    Err(RejectReason::OutOfRange)
                } else {
                    Ok(())
                }
            }
            Intent::Interact { target, .. } | Intent::Speak { target, .. } => {
                if self.entities.get(target).is_some() {
                    Ok(())
                } else {
                    Err(RejectReason::InvalidTarget)
                }
            }
            Intent::Idle => Ok(()),
        }
    }

    fn execute<P: ScenarioPack>(&mut self, pack: &P, actor: EntityId, intent: &Intent) {
        match *intent {
            Intent::Move { dx, dy } => {
                let moved_to = self.entities.get_mut(actor).map(|e| {
                    e.pos.0 += dx;
                    e.pos.1 += dy;
                    e.pos
                });
                if let Some(to) = moved_to {
                    self.event_log.push(Event::Moved {
                        tick: self.tick,
                        actor,
                        to,
                    });
                }
            }
            Intent::Interact { target, verb } => self.event_log.push(Event::Interacted {
                tick: self.tick,
                actor,
                target,
                verb,
            }),
            Intent::Speak { target, act, topic } => self.event_log.push(Event::Spoke {
                tick: self.tick,
                actor,
                target,
                act,
                topic,
            }),
            Intent::Idle => {}
        }
        pack.apply(self, actor, intent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::KernelPack;

    // Reorder independence is the headline determinism property: applying a
    // tick's intents in a different order must not change the outcome.
    #[test]
    fn intent_order_does_not_affect_state() {
        let pack = KernelPack::new();
        let mut forward = World::with_pack(1, &pack);
        let mut reverse = World::with_pack(1, &pack);
        let ids: Vec<EntityId> = (0..8).map(|i| forward.spawn((i, 0))).collect();
        for i in 0..8 {
            reverse.spawn((i, 0));
        }

        let mut batch: Vec<(EntityId, Intent)> = ids
            .iter()
            .map(|&id| (id, Intent::Move { dx: 1, dy: 0 }))
            .collect();
        forward.apply_intents(&pack, &batch);
        batch.reverse();
        reverse.apply_intents(&pack, &batch);

        assert_eq!(forward.state_hash(), reverse.state_hash());
    }
}
