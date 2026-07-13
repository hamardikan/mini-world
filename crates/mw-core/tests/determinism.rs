//! The determinism gate: same-seed reproducibility, intent-log replay, and
//! seed divergence — all over 10k ticks through the public kernel API.

use mw_core::{AgentRng, Intent, KernelPack, Observation, SoulPolicy, World};

const TICKS: u64 = 10_000;
const ENTITIES: i32 = 32;

struct RandomWalk;

impl SoulPolicy for RandomWalk {
    fn decide(&mut self, _observation: &Observation, rng: &mut AgentRng) -> Intent {
        match rng.range_u32(4) {
            0 => Intent::Move { dx: 1, dy: 0 },
            1 => Intent::Move { dx: -1, dy: 0 },
            2 => Intent::Move { dx: 0, dy: 1 },
            _ => Intent::Move { dx: 0, dy: -1 },
        }
    }
}

fn start_positions() -> Vec<(i32, i32)> {
    (0..ENTITIES).map(|i| (i % 16, i / 16)).collect()
}

fn run(seed: u64) -> World {
    let pack = KernelPack::new();
    let mut world = World::with_pack(seed, &pack);
    for pos in start_positions() {
        world.spawn(pos);
    }
    let mut policy = RandomWalk;
    for _ in 0..TICKS {
        world.step(&pack, &mut policy);
    }
    world
}

#[test]
fn determinism_same_seed() {
    assert_eq!(run(42).state_hash(), run(42).state_hash());
}

#[test]
fn replay_reproduces_hash() {
    let original = run(42);
    let pack = KernelPack::new();
    let replayed = World::replay(42, &start_positions(), TICKS, original.intent_log(), &pack);
    assert_eq!(original.state_hash(), replayed.state_hash());
}

#[test]
fn seed_divergence_changes_hash() {
    assert_ne!(run(42).state_hash(), run(43).state_hash());
}
