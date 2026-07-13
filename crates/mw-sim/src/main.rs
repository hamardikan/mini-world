//! mini-world headless runner.
//!
//! Two subcommands: `run` drives a trivial random-walk policy through the bare
//! kernel (a determinism smoke test), and `soak` runs the full village +
//! utility-SOUL + memory loop and reports throughput, an action histogram,
//! deaths, and a final state hash.

use clap::{Parser, Subcommand};
use mw_core::{AgentRng, Intent, KernelPack, Observation, SoulPolicy, World};
use mw_sim::soak::{self, SoakConfig};

#[derive(Parser)]
#[command(about = "mini-world headless kernel runner")]
struct Args {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Bare-kernel random-walk run; prints the final canonical state hash.
    Run {
        #[arg(long, default_value_t = 10_000)]
        ticks: u64,
        #[arg(long, default_value_t = 32)]
        entities: i32,
        #[arg(long, default_value_t = 1)]
        seed: u64,
    },
    /// Village social-sim soak with the utility SOUL.
    Soak {
        #[arg(long, default_value_t = 10_000)]
        ticks: u64,
        #[arg(long, default_value_t = 50)]
        agents: i32,
        #[arg(long, default_value_t = 1)]
        seed: u64,
    },
}

/// Picks one of four unit steps from the entity's own RNG stream. It ignores the
/// observation entirely — enough to exercise the per-entity RNG and the intent
/// pipeline.
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

fn start_positions(count: i32) -> Vec<(i32, i32)> {
    (0..count).map(|i| (i % 16, i / 16)).collect()
}

fn run_kernel(ticks: u64, entities: i32, seed: u64) {
    let pack = KernelPack::new();
    let mut world = World::with_pack(seed, &pack);
    for pos in start_positions(entities) {
        world.spawn(pos);
    }
    let mut policy = RandomWalk;
    for _ in 0..ticks {
        world.step(&pack, &mut policy);
    }
    println!(
        "seed={} entities={} ticks={} hash={:#018x}",
        seed,
        world.entity_count(),
        world.tick(),
        world.state_hash(),
    );
}

fn run_soak(ticks: u64, agents: i32, seed: u64) {
    let report = soak::run(SoakConfig {
        seed,
        agents,
        ticks,
    });
    println!(
        "soak seed={} agents={} ticks={}",
        report.cfg.seed, report.cfg.agents, report.cfg.ticks
    );
    println!(
        "ticks/sec={:.0} actions={} deaths={} (starvation)",
        report.ticks_per_sec(),
        report.total_actions(),
        report.deaths,
    );
    println!("final_hash={:#018x}", report.final_hash);
    println!(
        "action histogram (max share {:.1}%):",
        100.0 * report.max_share()
    );
    for line in report.histogram_lines() {
        println!("{line}");
    }
}

fn main() {
    match Args::parse().cmd {
        Command::Run {
            ticks,
            entities,
            seed,
        } => run_kernel(ticks, entities, seed),
        Command::Soak {
            ticks,
            agents,
            seed,
        } => run_soak(ticks, agents, seed),
    }
}
