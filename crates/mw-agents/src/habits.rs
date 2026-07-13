//! Deterministic per-character habit cache (DESIGN.md §4).
//!
//! The cache sits at the `SoulPolicy` seam.  It keys a decision by coarse state
//! bands and re-checks cheap predicates before replaying it; the kernel remains
//! the final authority on whether the replayed intent is legal.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use mw_core::{AgentRng, EntityId, Event, Intent, Observation, SoulPolicy};

/// Number of needs represented by the v0 observation encoder.
pub const N_NEEDS: usize = 3;
/// Four coarse bands retain routine decisions without making the cache brittle.
pub const NEED_BANDS: u8 = 4;
/// Maximum entries retained by one character.
pub const HABIT_CAPACITY: usize = 32;

/// Compact, deterministic context key for one cached decision.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HabitKey {
    /// Quantized need levels, low to high.
    pub need_bands: [u8; N_NEEDS],
    /// Scenario-defined cell class (the village uses its tile id).
    pub cell_class: u8,
    /// Affordance mask at the point of decision.
    pub tool_mask: u32,
    /// Goal/priority slot from the rich observation layer.
    pub goal: u8,
}

impl HabitKey {
    /// Construct a key from fixed-point needs in `[0, need_max]`.
    pub fn from_needs(
        needs: [i16; N_NEEDS],
        need_max: i16,
        cell_class: u8,
        tool_mask: u32,
        goal: u8,
    ) -> Self {
        let max = need_max.max(1) as i32;
        let bands = needs.map(|n| {
            (((n.max(0) as i32) * NEED_BANDS as i32) / max).min(NEED_BANDS as i32 - 1) as u8
        });
        Self {
            need_bands: bands,
            cell_class,
            tool_mask,
            goal,
        }
    }

    /// Stable compact fingerprint useful for telemetry and tests.
    pub fn fingerprint(self) -> u64 {
        let mut h = DefaultHasher::new();
        self.hash(&mut h);
        h.finish()
    }
}

/// Context supplied by the scenario before a gated step.  The kernel's
/// observation does not own pack needs, so this keeps the cache pack-agnostic.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HabitContext {
    pub needs: [i16; N_NEEDS],
    pub need_max: i16,
    pub cell_class: u8,
    pub goal: u8,
}

impl HabitContext {
    pub fn key(self, tool_mask: u32) -> HabitKey {
        HabitKey::from_needs(
            self.needs,
            self.need_max,
            self.cell_class,
            tool_mask,
            self.goal,
        )
    }
}

#[derive(Clone, Debug)]
struct Entry {
    key: HabitKey,
    intent: Intent,
    stamp: u64,
}

#[derive(Clone, Debug, Default)]
struct AgentCache {
    entries: Vec<Entry>,
    context: Option<HabitContext>,
}
/// Cache counters exposed in soak/TUI telemetry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HabitStats {
    pub hits: u64,
    pub misses: u64,
    pub invalidations: u64,
    pub scoring_calls_skipped: u64,
}

impl HabitStats {
    pub fn decisions(self) -> u64 {
        self.hits + self.misses
    }
    pub fn hit_rate(self) -> f64 {
        let n = self.decisions();
        if n == 0 {
            0.0
        } else {
            self.hits as f64 / n as f64
        }
    }
}

/// A `SoulPolicy` wrapper that replays valid routine intents.
///
/// `ids` must use the same stable entity-slot order as the kernel.  The
/// optional hit hook is used by cursor-based policies (the v0 UtilitySoul) to
/// advance their call cursor without running observe/score work.
pub struct HabitSoul<P: SoulPolicy> {
    inner: P,
    caches: Vec<AgentCache>,
    contexts: Vec<HabitContext>,
    last_tick: Option<u64>,
    cursor: usize,
    stamp: u64,
    stats: HabitStats,
    hit_hook: Option<fn(&mut P, &Observation, &Intent)>,
}

/// Naming used by the design block; `HabitSoul` is the policy-facing wrapper.
pub type HabitCache<P> = HabitSoul<P>;

impl<P: SoulPolicy> HabitSoul<P> {
    pub fn new(inner: P, ids: Vec<EntityId>) -> Self {
        let n = ids.len();
        Self {
            inner,
            caches: vec![AgentCache::default(); n],
            contexts: vec![HabitContext::default(); n],
            last_tick: None,
            cursor: 0,
            stamp: 0,
            stats: HabitStats::default(),
            hit_hook: None,
        }
    }

    /// Construct with a callback that advances a cursor-only inner policy on a hit.
    pub fn with_hit_hook(
        inner: P,
        ids: Vec<EntityId>,
        hook: fn(&mut P, &Observation, &Intent),
    ) -> Self {
        let mut this = Self::new(inner, ids);
        this.hit_hook = Some(hook);
        this
    }
    pub fn inner(&self) -> &P {
        &self.inner
    }
    pub fn inner_mut(&mut self) -> &mut P {
        &mut self.inner
    }

    pub fn stats(&self) -> HabitStats {
        self.stats
    }
    pub fn cache_size(&self, slot: usize) -> usize {
        self.caches.get(slot).map_or(0, |c| c.entries.len())
    }
    pub fn cache_sizes(&self) -> Vec<usize> {
        self.caches.iter().map(|c| c.entries.len()).collect()
    }
    pub fn total_cache_size(&self) -> usize {
        self.caches.iter().map(|c| c.entries.len()).sum()
    }

    /// Set pack-owned context before stepping. A need-band edge flushes the
    /// affected character immediately; other key fields naturally miss.
    pub fn set_context(&mut self, entity: EntityId, context: HabitContext) {
        let slot = entity.index() as usize;
        if let Some(cache) = self.caches.get_mut(slot) {
            let old = cache.context.replace(context);
            if old.is_some_and(|old| {
                HabitKey::from_needs(old.needs, old.need_max, old.cell_class, 0, old.goal)
                    .need_bands
                    != HabitKey::from_needs(
                        context.needs,
                        context.need_max,
                        context.cell_class,
                        0,
                        context.goal,
                    )
                    .need_bands
            }) {
                cache.entries.clear();
            }
        }
        if let Some(ctx) = self.contexts.get_mut(slot) {
            *ctx = context;
        }
    }

    pub fn set_contexts(&mut self, contexts: &[(EntityId, HabitContext)]) {
        for &(id, ctx) in contexts {
            self.set_context(id, ctx);
        }
    }

    /// Flush one character's habits (e.g. a death or nearby attack).
    pub fn invalidate_agent(&mut self, entity: EntityId) {
        if let Some(cache) = self.caches.get_mut(entity.index() as usize) {
            cache.entries.clear();
        }
    }

    pub fn invalidate_all(&mut self) {
        for cache in &mut self.caches {
            cache.entries.clear();
        }
    }

    /// Event-driven invalidation of affected characters. Routine movement is
    /// left intact; social/economic events can swing opinions or targets.
    pub fn observe_events(&mut self, events: &[Event]) {
        for ev in events {
            let (actor, target) = match *ev {
                Event::Rejected { actor, .. } => (Some(actor), None),
                Event::Interacted { actor, target, .. } | Event::Spoke { actor, target, .. } => {
                    (Some(actor), Some(target))
                }
                Event::Moved { .. } => (None, None),
            };
            if let Some(actor) = actor {
                self.invalidate_agent(actor);
            }
            if let Some(target) = target {
                self.invalidate_agent(target);
            }
        }
    }

    fn context_for(&self, slot: usize, obs: &Observation) -> HabitKey {
        let Some(cache) = self.caches.get(slot) else {
            return HabitContext::from(obs)
                .key(obs.tool_mask)
                .with_location_fallback(obs);
        };
        if let Some(context) = cache.context {
            context.key(obs.tool_mask)
        } else {
            HabitContext::from(obs)
                .key(obs.tool_mask)
                .with_location_fallback(obs)
        }
    }

    /// Stable digest of one cache's contents for determinism/plasticity tests.
    pub fn cache_fingerprint(&self, slot: usize) -> u64 {
        let mut h = DefaultHasher::new();
        if let Some(cache) = self.caches.get(slot) {
            for entry in &cache.entries {
                entry.key.hash(&mut h);
                format!("{:?}", entry.intent).hash(&mut h);
            }
        }
        h.finish()
    }

    fn slot_for(&mut self, tick: u64) -> usize {
        if self.last_tick != Some(tick) {
            self.last_tick = Some(tick);
            self.cursor = 0;
        }
        let slot = self.cursor;
        self.cursor = self.cursor.saturating_add(1);
        slot
    }

    fn valid(intent: &Intent, obs: &Observation) -> bool {
        if obs.tool_mask == 0 {
            return false;
        }
        match intent {
            Intent::Interact { target, .. } | Intent::Speak { target, .. } => obs
                .neighbors
                .iter()
                .any(|n| n.present && n.id == *target && n.dx.abs().max(n.dy.abs()) <= 1),
            // A move carries only a direction, not a destination. Re-score it
            // rather than replaying a direction that can strand a character.
            Intent::Move { .. } => false,
            Intent::Idle => true,
        }
    }
}

impl HabitKey {
    fn with_location_fallback(self, obs: &Observation) -> Self {
        // Context supplied by a pack wins; zero is also a valid tile class, so
        // only use the observation-derived location when no context was set.
        if self.cell_class == 0 {
            let (x, y) = obs.self_pos;
            Self {
                cell_class: (x as u32)
                    .wrapping_mul(31)
                    .wrapping_add(y as u32)
                    .wrapping_mul(17) as u8,
                ..self
            }
        } else {
            self
        }
    }
}

impl<P: SoulPolicy> SoulPolicy for HabitSoul<P> {
    fn decide(&mut self, obs: &Observation, rng: &mut AgentRng) -> Intent {
        let slot = self.slot_for(obs.tick);
        if obs.tool_mask == 0 {
            // Gated cold entities must still advance any cursor-only inner soul.
            if let Some(hook) = self.hit_hook {
                hook(&mut self.inner, obs, &Intent::Idle);
            }
            return Intent::Idle;
        }
        let key = self.context_for(slot, obs);
        let hit = self.caches.get_mut(slot).and_then(|c| {
            c.entries
                .iter_mut()
                .find(|e| e.key == key && Self::valid(&e.intent, obs))
        });
        if let Some(entry) = hit {
            self.stats.hits += 1;
            self.stats.scoring_calls_skipped += 1;
            self.stamp = self.stamp.wrapping_add(1);
            entry.stamp = self.stamp;
            if let Some(hook) = self.hit_hook {
                hook(&mut self.inner, obs, &entry.intent);
            }
            return entry.intent.clone();
        }
        if self
            .caches
            .get(slot)
            .is_some_and(|c| c.entries.iter().any(|e| e.key == key))
        {
            self.stats.invalidations += 1;
        }
        self.stats.misses += 1;
        let intent = self.inner.decide(obs, rng);
        if let Some(cache) = self.caches.get_mut(slot) {
            self.stamp = self.stamp.wrapping_add(1);
            if cache.entries.len() >= HABIT_CAPACITY {
                let evict = cache
                    .entries
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, e)| (e.stamp, e.key))
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                cache.entries.remove(evict);
            }
            cache.entries.push(Entry {
                key,
                intent: intent.clone(),
                stamp: self.stamp,
            });
        }
        intent
    }
}

/// Build a context from a minimal observation when no pack-specific context is available.
impl From<&Observation> for HabitContext {
    fn from(_obs: &Observation) -> Self {
        Self {
            needs: [0; N_NEEDS],
            need_max: 1,
            cell_class: 0,
            goal: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mw_core::{agent_rng, KernelPack, NeighborSlot, World, K_NEAREST};

    struct AlwaysIdle;
    impl SoulPolicy for AlwaysIdle {
        fn decide(&mut self, _: &Observation, _: &mut AgentRng) -> Intent {
            Intent::Idle
        }
    }

    fn id() -> EntityId {
        let pack = KernelPack::new();
        let mut world = World::with_pack(1, &pack);
        world.spawn((0, 0))
    }

    fn obs(tick: u64, mask: u32, id: EntityId) -> Observation {
        Observation {
            tick,
            self_pos: (0, 0),
            neighbors: [NeighborSlot {
                present: false,
                id,
                dx: 0,
                dy: 0,
            }; K_NEAREST],
            event_count: 0,
            tool_mask: mask,
        }
    }

    #[test]
    fn warm_context_replays_and_capacity_is_bounded() {
        let id = id();
        let mut h = HabitSoul::new(AlwaysIdle, vec![id]);
        let mut r = agent_rng(1, id, 0, 0);
        let o = obs(0, 1, id);
        h.decide(&o, &mut r);
        h.decide(&Observation { tick: 1, ..o }, &mut r);
        assert_eq!(h.stats().hits, 1);
        assert_eq!(h.cache_size(0), 1);
        assert!(h.total_cache_size() <= HABIT_CAPACITY);
    }

    #[test]
    fn event_flushes_history() {
        let id = id();
        let mut h = HabitSoul::new(AlwaysIdle, vec![id]);
        let mut r = agent_rng(1, id, 0, 0);
        h.decide(&obs(0, 1, id), &mut r);
        h.observe_events(&[Event::Moved {
            tick: 0,
            actor: id,
            to: (1, 0),
        }]);
        assert_eq!(h.cache_size(0), 1);
        h.observe_events(&[Event::Rejected {
            tick: 0,
            actor: id,
            reason: mw_core::RejectReason::InvalidTarget,
        }]);
        assert_eq!(h.cache_size(0), 0);
    }

    #[test]
    fn different_histories_diverge_cache_contents() {
        let pack = KernelPack::new();
        let mut world = World::with_pack(1, &pack);
        let a = world.spawn((0, 0));
        let b = world.spawn((1, 0));
        let mut h = HabitSoul::new(AlwaysIdle, vec![a, b]);
        let mut ra = agent_rng(1, a, 0, 0);
        let mut rb = agent_rng(1, b, 0, 0);
        h.decide(&obs(0, 1, a), &mut ra);
        h.decide(&obs(0, 1, b), &mut rb);
        h.observe_events(&[Event::Rejected {
            tick: 0,
            actor: a,
            reason: mw_core::RejectReason::InvalidTarget,
        }]);
        assert_ne!(h.cache_fingerprint(0), h.cache_fingerprint(1));
    }
}
