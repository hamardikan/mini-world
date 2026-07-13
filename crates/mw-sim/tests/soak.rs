//! Soak gate: the 50-agent / 10k-tick village run must be deterministic, keep
//! the whole population alive and acting (no starvation deadlock, no all-idle
//! paralysis), and produce a non-degenerate action mix.

use mw_sim::soak::{self, SoakConfig};

const CFG: SoakConfig = SoakConfig {
    seed: 1,
    agents: 50,
    ticks: 10_000,
};

#[test]
fn soak_is_deterministic_for_a_fixed_seed() {
    let a = soak::run(CFG);
    let b = soak::run(CFG);
    assert_eq!(
        a.final_hash, b.final_hash,
        "same seed must reproduce the hash"
    );
    assert_eq!(
        a.histogram, b.histogram,
        "same seed must reproduce decisions"
    );
}

#[test]
fn soak_hash_is_seed_sensitive() {
    let a = soak::run(CFG);
    let b = soak::run(SoakConfig { seed: 2, ..CFG });
    assert_ne!(a.final_hash, b.final_hash, "a different seed must diverge");
}

#[test]
fn soak_has_no_paralysis_or_starvation_deadlock() {
    let r = soak::run(CFG);
    // Every living agent decides every tick; zero deaths means the whole
    // population stayed functional — no all-idle paralysis, no deadlock.
    assert_eq!(r.deaths, 0, "the village must not starve to death");
    assert_eq!(
        r.total_actions(),
        CFG.ticks * CFG.agents as u64,
        "every agent must act every tick (no paralysis)"
    );
}

#[test]
fn soak_histogram_is_non_degenerate() {
    let r = soak::run(CFG);
    // No single tool may dominate the run...
    assert!(
        r.max_share() < 0.80,
        "no tool may exceed 80% (was {:.1}%)",
        100.0 * r.max_share()
    );
    // ...and the village exercises a broad slice of its body, not two tools.
    let used = r.histogram.iter().filter(|&&n| n > 0).count();
    assert!(
        used >= 6,
        "expected a varied action mix, got {used} tools used"
    );
    // Survival tools are actually reached, not just social ones.
    let idx = |a: mw_village::Action| a as usize;
    for a in [
        mw_village::Action::Eat,
        mw_village::Action::Sleep,
        mw_village::Action::Work,
    ] {
        assert!(
            r.histogram[idx(a)] > 0,
            "{a:?} should occur in a living village"
        );
    }
}
