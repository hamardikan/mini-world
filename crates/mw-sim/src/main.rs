//! Headless functional check: run N ticks over M entities driven by a trivial
//! per-entity random-walk policy, then print the final canonical state hash.

use clap::Parser;
use mw_core::{AgentRng, Intent, KernelPack, Observation, SoulPolicy, World};

#[derive(Parser)]
#[command(about = "mini-world headless kernel runner")]
struct Args {
    /// Number of ticks to simulate.
    #[arg(long, default_value_t = 10_000)]
    ticks: u64,
    /// Number of entities to spawn.
    #[arg(long, default_value_t = 32)]
    entities: i32,
    /// World seed.
    #[arg(long, default_value_t = 1)]
    seed: u64,
}

/// Picks one of four unit steps from the entity's own RNG stream. It ignores
/// the observation entirely — enough to exercise the per-entity RNG and the
/// intent pipeline.
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

/// Deterministic starting layout so a given `entities` count always begins the
/// same way.
fn start_positions(count: i32) -> Vec<(i32, i32)> {
    (0..count).map(|i| (i % 16, i / 16)).collect()
}

fn main() {
    let args = Args::parse();

    let pack = KernelPack::new();
    let mut world = World::with_pack(args.seed, &pack);
    for pos in start_positions(args.entities) {
        world.spawn(pos);
    }

    let mut policy = RandomWalk;
    for _ in 0..args.ticks {
        world.step(&pack, &mut policy);
    }

    println!(
        "seed={} entities={} ticks={} hash={:#018x}",
        args.seed,
        world.entity_count(),
        world.tick(),
        world.state_hash(),
    );
}
