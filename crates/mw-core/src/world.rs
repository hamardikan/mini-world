//! The deterministic world kernel.
//!
//! Fixed-timestep tick loop, integer-only state, per-entity RNG streams, a
//! validate → execute → log pipeline, and a canonical state hash. There is no
//! wall-clock anywhere in here: the only notion of time is the `u64` tick.

use crate::contracts::{NeighborSlot, Observation, ScenarioPack, SoulPolicy, K_NEAREST};
use crate::entity::{Entity, EntityId, EntityStore, StatRegistry};
use crate::hash::FnvHasher;
use crate::intent::{Event, FfSegment, Intent, LogEntry, LoggedIntent, RejectReason};
use crate::rng::{self, stream, AgentRng, StreamTag};

pub struct World {
    seed: u64,
    tick: u64,
    entities: EntityStore,
    intent_log: Vec<LogEntry>,
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

    pub fn intent_log(&self) -> &[LogEntry] {
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
            // Base observation: affordances are a pack concern, so the mask is
            // left empty here and filled by `step` via `ScenarioPack::afforded_tools`
            // (the seam). Packs may call `observe` to read neighbor proximity
            // without recursing back into affordance masking.
            tool_mask: 0,
        }
    }

    /// Build the single per-decide observation for one entity: one neighbor scan,
    /// reused by the pack to compute the tool mask (no second scan). This is the
    /// only place the kernel constructs an [`Observation`] for a policy.
    fn observe_for_policy<P: ScenarioPack>(&self, pack: &P, id: EntityId) -> Observation {
        let mut obs = self.observe(id);
        obs.tool_mask = pack.afforded_tools(self, id, &obs);
        obs
    }

    /// Advance one tick: every entity observes, its policy decides, then the
    /// batch runs through validate → execute → log.
    pub fn step<P: ScenarioPack, S: SoulPolicy>(&mut self, pack: &P, policy: &mut S) {
        self.step_gated(pack, policy, |_, _| true);
    }

    /// Advance one tick with a per-entity policy gate (the Director's LOD ring:
    /// hot every tick, warm on cadence, cold never). For a gated-off entity the
    /// kernel skips the observe + affordance + scoring work and hands the policy
    /// a zero-mask observation — the policy idle-extrapolates cheaply while its
    /// per-tick call cursor still advances, so entities that *do* run this tick
    /// still resolve to the right slot.
    pub fn step_gated<P, S, G>(&mut self, pack: &P, policy: &mut S, gate: G)
    where
        P: ScenarioPack,
        S: SoulPolicy,
        G: Fn(EntityId, u64) -> bool,
    {
        let ids: Vec<EntityId> = self.entities.ids().collect();
        let mut batch = Vec::with_capacity(ids.len());
        for id in ids {
            let observation = if gate(id, self.tick) {
                self.observe_for_policy(pack, id)
            } else {
                Observation {
                    tick: self.tick,
                    self_pos: self.entities.get(id).map(|e| e.pos).unwrap_or((0, 0)),
                    neighbors: [NeighborSlot {
                        present: false,
                        id,
                        dx: 0,
                        dy: 0,
                    }; K_NEAREST],
                    event_count: self.event_log.len() as u64,
                    tool_mask: 0, // afford nothing → the policy idles this tick
                }
            };
            let mut rng = self.agent_rng(id, stream::SOUL);
            batch.push((id, policy.decide(&observation, &mut rng)));
        }
        self.apply_intents(pack, &batch);
    }

    /// Advance the world by `duration` ticks analytically (the cold LOD ring):
    /// no per-tick intents, just the pack's closed-form state advance. The span
    /// is recorded as an [`FfSegment`] so replay reconstructs it from the log.
    pub fn fast_forward<P: ScenarioPack>(&mut self, pack: &P, duration: u64) {
        let start = self.tick;
        pack.fast_forward(self, start, start + duration);
        self.tick += duration;
        self.intent_log.push(LogEntry::Ff(FfSegment {
            start_tick: start,
            duration,
        }));
    }

    /// Re-run a world purely from `(seed, log)`, bypassing all policies. Feeding
    /// the log back through the same validate → execute path (and re-applying
    /// fast-forward segments analytically) reproduces the exact state hash.
    /// `total_ticks` is replayed in full so the tick counter lands where the
    /// original run left it even across ticks that logged nothing.
    pub fn replay<P: ScenarioPack>(
        seed: u64,
        init_positions: &[(i32, i32)],
        total_ticks: u64,
        log: &[LogEntry],
        pack: &P,
    ) -> World {
        let mut world = World::with_pack(seed, pack);
        for &pos in init_positions {
            world.spawn(pos);
        }

        // The log is globally ordered by tick; a single cursor walks it. Intent
        // entries are sliced per tick; a fast-forward entry jumps the clock.
        let mut cursor = 0;
        while world.tick < total_ticks {
            if let Some(LogEntry::Ff(seg)) = log.get(cursor) {
                debug_assert_eq!(world.tick, seg.start_tick);
                world.fast_forward(pack, seg.duration);
                cursor += 1;
                continue;
            }
            let t = world.tick;
            let start = cursor;
            while let Some(LogEntry::Intent(l)) = log.get(cursor) {
                if l.tick != t {
                    break;
                }
                cursor += 1;
            }
            let batch: Vec<(EntityId, Intent)> = log[start..cursor]
                .iter()
                .map(|e| match e {
                    LogEntry::Intent(l) => (l.actor, l.intent.clone()),
                    LogEntry::Ff(_) => unreachable!("sliced only intent entries"),
                })
                .collect();
            world.apply_intents(pack, &batch);
        }
        world
    }

    /// Canonical state hash: stable across runs, machines, and architectures.
    /// Iterates entities in canonical slot order and hashes integer state only
    /// via fixed-width little-endian bytes — no `HashMap` iteration, no float
    /// bit patterns — then folds in the pack's own per-entity state so the hash
    /// covers everything replay must reproduce, not just positions.
    pub fn state_hash<P: ScenarioPack>(&self, pack: &P) -> u64 {
        let mut h = FnvHasher::new();
        h.write_u64(self.tick);
        for (id, e) in self.entities.iter() {
            h.write_u32(id.index());
            h.write_u32(id.generation());
            h.write_i32(e.pos.0);
            h.write_i32(e.pos.1);
        }
        pack.hash_state(&mut h);
        h.finish()
    }

    fn apply_intents<P: ScenarioPack>(&mut self, pack: &P, batch: &[(EntityId, Intent)]) {
        // Canonical apply order: sort by entity id so the outcome is independent
        // of the order intents were submitted in — including shared-cell effects
        // (Give/Pickup/Drop) where two actors race for the same tile and only a
        // deterministic winner (lowest id) is correct. Per-entity RNG streams are
        // already order-free; this ratifies ordering for the *effects* too. Stable
        // sort, unique ids: no ambiguity.
        let mut ordered: Vec<(EntityId, Intent)> = batch.to_vec();
        ordered.sort_by_key(|(a, _)| (a.index(), a.generation()));
        for (actor, intent) in &ordered {
            match self.validate(pack, *actor, intent) {
                Ok(()) => {
                    self.intent_log.push(LogEntry::Intent(LoggedIntent {
                        tick: self.tick,
                        actor: *actor,
                        intent: intent.clone(),
                    }));
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

    // Submission order must not change the outcome, BECAUSE the kernel sorts each
    // batch into canonical (entity-id) order before applying. Per-entity RNG is
    // already order-free; this proves the *effect* order is ratified too.
    #[test]
    fn submission_order_does_not_affect_state_kernel_sorts() {
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

        assert_eq!(forward.state_hash(&pack), reverse.state_hash(&pack));
    }

    // A shared-cell conflict: two entities grab the one unit of loot on the same
    // cell in the SAME tick. Exactly one can win; the winner must be the lower
    // entity id regardless of the order the two grabs were submitted — that is
    // the canonical-order guarantee the kernel now enforces.
    #[test]
    fn shared_cell_conflict_resolves_to_lowest_id() {
        use crate::contracts::ActionManifest;
        use std::cell::Cell;

        struct LootPack {
            manifest: ActionManifest,
            loot: Cell<u32>,
            winner: Cell<Option<EntityId>>,
        }
        impl LootPack {
            fn new() -> Self {
                Self {
                    manifest: ActionManifest::default(),
                    loot: Cell::new(1), // exactly one unit on the shared cell
                    winner: Cell::new(None),
                }
            }
        }
        impl ScenarioPack for LootPack {
            fn manifest(&self) -> &ActionManifest {
                &self.manifest
            }
            fn validate(
                &self,
                _: &World,
                _: EntityId,
                intent: &Intent,
            ) -> Result<(), RejectReason> {
                match intent {
                    // Grab is legal only while loot remains.
                    Intent::Interact { .. } if self.loot.get() > 0 => Ok(()),
                    Intent::Interact { .. } => Err(RejectReason::Depleted),
                    _ => Ok(()),
                }
            }
            fn apply(&self, _: &mut World, actor: EntityId, intent: &Intent) {
                if let Intent::Interact { .. } = intent {
                    if self.loot.get() > 0 {
                        self.loot.set(self.loot.get() - 1);
                        self.winner.set(Some(actor));
                    }
                }
            }
            fn register(&self, _: &mut StatRegistry) {}
        }

        // Two entities minted at slots 0 and 1; a fresh world mints them the same
        // way, so these ids are valid actors inside `winner_for`'s own world.
        let mut probe = World::with_pack(1, &LootPack::new());
        let a = probe.spawn((0, 0));
        let b = probe.spawn((0, 0));
        let g = |actor: EntityId| {
            (
                actor,
                Intent::Interact {
                    target: actor,
                    verb: 0,
                },
            )
        };

        let winner_for = |batch: &[(EntityId, Intent)]| {
            let pack = LootPack::new();
            let mut w = World::with_pack(1, &pack);
            w.spawn((0, 0));
            w.spawn((0, 0));
            w.apply_intents(&pack, batch);
            pack.winner.get()
        };

        let forward = winner_for(&[g(a), g(b)]);
        let reverse = winner_for(&[g(b), g(a)]);
        assert_eq!(forward, Some(a), "lowest id wins");
        assert_eq!(
            reverse,
            Some(a),
            "and the winner is independent of submission order"
        );
    }
}
