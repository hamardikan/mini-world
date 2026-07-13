//! Functional gate: drive 1000 ticks of scripted intents through the full
//! kernel pipeline (observe → decide → validate → execute → log) and assert
//! that valid intents apply, invalid ones are rejected with the right
//! `RejectReason`, and the run replays bit-identically.

use mw_core::{AgentRng, Event, Intent, LogEntry, Observation, RejectReason, SoulPolicy, World};
use mw_village::{verb, Action, Item, VillagePack, MAX_NEED};

const TICKS: u64 = 1000;

/// Feeds a fixed per-entity program. The kernel calls `decide` once per entity
/// each tick in stable slot order, so a call counter reset on every tick change
/// recovers "which entity am I" without the observation carrying an id.
struct Scripted {
    programs: Vec<Box<dyn FnMut(u64) -> Intent>>,
    tick: u64,
    idx: usize,
}

impl SoulPolicy for Scripted {
    fn decide(&mut self, obs: &Observation, _rng: &mut AgentRng) -> Intent {
        if obs.tick != self.tick {
            self.tick = obs.tick;
            self.idx = 0;
        }
        let i = self.idx;
        self.idx += 1;
        (self.programs[i])(obs.tick)
    }
}

fn positions() -> Vec<(i32, i32)> {
    vec![
        (8, 8),   // 0 baker  — eats free at the bakery every tick (always valid)
        (3, 3),   // 1 starver — eats with no food off-bakery (Depleted, then starves)
        (15, 7),  // 2 edger   — walks into the east wall (OutOfRange)
        (10, 10), // 3 sleeper — sleeps off a home tile (NotAfforded)
        (5, 5),   // 4 selfish — gives to itself (InvalidTarget)
        (2, 2),   // 5 glitch  — calls an unknown tool id (UnknownTool)
        (5, 10),  // 6 walker  — walks east until the wall (valid moves, then reject)
    ]
}

fn build_world() -> (World, VillagePack, Vec<mw_core::EntityId>) {
    let pack = VillagePack::new();
    let mut world = World::with_pack(7, &pack);
    let ids: Vec<_> = positions().into_iter().map(|p| world.spawn(p)).collect();
    (world, pack, ids)
}

fn run(world: &mut World, pack: &VillagePack, ids: &[mw_core::EntityId]) {
    let me = ids.to_vec();
    let programs: Vec<Box<dyn FnMut(u64) -> Intent>> = vec![
        {
            let s = me[0];
            Box::new(move |_| Intent::Interact {
                target: s,
                verb: verb(Action::Eat, Item::Food),
            })
        },
        {
            let s = me[1];
            Box::new(move |_| Intent::Interact {
                target: s,
                verb: verb(Action::Eat, Item::Food),
            })
        },
        Box::new(|_| Intent::Move { dx: 1, dy: 0 }),
        {
            let s = me[3];
            Box::new(move |_| Intent::Interact {
                target: s,
                verb: verb(Action::Sleep, Item::Food),
            })
        },
        {
            let s = me[4];
            Box::new(move |_| Intent::Interact {
                target: s, // self-give
                verb: verb(Action::Give, Item::Food),
            })
        },
        {
            let s = me[5];
            Box::new(move |_| Intent::Interact {
                target: s,
                verb: 250, // no tool has this id
            })
        },
        Box::new(|_| Intent::Move { dx: 1, dy: 0 }),
    ];

    let mut policy = Scripted {
        programs,
        tick: 0,
        idx: 0,
    };
    for _ in 0..TICKS {
        world.step(pack, &mut policy);
    }
}

/// Rejections grouped by actor, in tick order.
fn rejections(world: &World, actor: mw_core::EntityId) -> Vec<RejectReason> {
    world
        .event_log()
        .iter()
        .filter_map(|e| match e {
            Event::Rejected {
                actor: a, reason, ..
            } if *a == actor => Some(*reason),
            _ => None,
        })
        .collect()
}

#[test]
fn scripted_run_validates_and_applies_over_1k_ticks() {
    let (mut world, pack, ids) = build_world();
    run(&mut world, &pack, &ids);
    assert_eq!(world.tick(), TICKS);

    // Baker: never rejected, hunger stays high (eats free every tick).
    assert!(
        rejections(&world, ids[0]).is_empty(),
        "the baker's eats are always valid"
    );
    assert!(pack.needs(ids[0]).hunger(world.tick()) > MAX_NEED / 2);
    assert!(!pack.is_dead(&world, ids[0]));

    // Starver: eating is Depleted while alive, then it starves to death and the
    // reason flips to NotAfforded (the dead afford nothing).
    let starver = rejections(&world, ids[1]);
    assert!(starver.contains(&RejectReason::Depleted));
    assert!(starver.contains(&RejectReason::NotAfforded));
    assert!(pack.is_dead(&world, ids[1]), "prolonged starvation kills");

    // Edger: every move is out of bounds, so its position never changes.
    assert!(rejections(&world, ids[2]).contains(&RejectReason::OutOfRange));
    assert_eq!(world.entity(ids[2]).unwrap().pos, (15, 7));

    // Sleeper off a home tile: not afforded.
    assert!(rejections(&world, ids[3]).contains(&RejectReason::NotAfforded));

    // Self-give: invalid target.
    assert!(rejections(&world, ids[4]).contains(&RejectReason::InvalidTarget));

    // Unknown tool id (checked before death sets in).
    assert!(rejections(&world, ids[5]).contains(&RejectReason::UnknownTool));

    // Walker: valid moves apply until it hits the east wall at x=15.
    assert_eq!(world.entity(ids[6]).unwrap().pos, (15, 10));
    let logged_moves = world
        .intent_log()
        .iter()
        .filter_map(|e| match e {
            LogEntry::Intent(l) => Some(l),
            LogEntry::Ff(_) => None,
        })
        .filter(|l| l.actor == ids[6] && matches!(l.intent, Intent::Move { .. }))
        .count();
    assert_eq!(logged_moves, 10, "5->15 is ten single steps, rest rejected");

    // The glitch actor's unknown intents never enter the ground-truth log.
    assert!(!world.intent_log().iter().any(|e| matches!(
        e,
        LogEntry::Intent(l) if l.actor == ids[5]
    )));
}

#[test]
fn run_is_deterministic_and_replayable() {
    let (mut a, pack_a, ids) = build_world();
    run(&mut a, &pack_a, &ids);

    // Same seed + script → identical canonical hash.
    let (mut b, pack_b, ids_b) = build_world();
    run(&mut b, &pack_b, &ids_b);
    assert_eq!(a.state_hash(&pack_a), b.state_hash(&pack_b));

    // Replaying the validated-intent log on a fresh pack reproduces the hash
    // without re-running any policy.
    let log: Vec<LogEntry> = a.intent_log().to_vec();
    let pack_r = VillagePack::new();
    let replayed = World::replay(a.seed(), &positions(), TICKS, &log, &pack_r);
    assert_eq!(a.state_hash(&pack_a), replayed.state_hash(&pack_r));
}

/// Everyone idles; needs simply decay. Exercises the fast-forward replay path
/// without depending on the utility SOUL (which lives in mw-sim).
struct AllIdle;
impl SoulPolicy for AllIdle {
    fn decide(&mut self, _obs: &Observation, _rng: &mut AgentRng) -> Intent {
        Intent::Idle
    }
}

#[test]
fn cold_fast_forward_is_reconstructible_from_the_log() {
    // A week of AFK cold time, bracketed by live ticks. Because the fast-forward
    // is recorded as an `FfSegment`, replaying `(seed, log)` reproduces the exact
    // full state hash — needs, starvation clock, inventories and all — without
    // re-running the analytic advance from scratch.
    const WEEK: u64 = 7 * 86_400;
    let (mut w, pack, _ids) = build_world();
    let mut policy = AllIdle;
    for _ in 0..50 {
        w.step(&pack, &mut policy); // live
    }
    w.fast_forward(&pack, WEEK); // cold AFK span
    for _ in 0..50 {
        w.step(&pack, &mut policy); // live again
    }

    let total = w.tick();
    assert_eq!(total, 100 + WEEK);
    let hash = w.state_hash(&pack);
    let log: Vec<LogEntry> = w.intent_log().to_vec();
    assert!(
        log.iter().any(|e| matches!(e, LogEntry::Ff(_))),
        "the fast-forward must be recorded in the log"
    );

    let pack_r = VillagePack::new();
    let replayed = World::replay(w.seed(), &positions(), total, &log, &pack_r);
    assert_eq!(
        hash,
        replayed.state_hash(&pack_r),
        "replay reproduces the FF span"
    );
}
