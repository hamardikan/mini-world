//! Intents, the validated-intent log, events, and rejection reasons.
//!
//! Brains never touch the world; they emit [`Intent`]s. The kernel validates,
//! then executes, then logs. The validated-intent log is the replay ground
//! truth — re-applying it reproduces the world without re-running any policy.

use crate::entity::EntityId;

/// Kernel-level intent variants. Scenario packs extend behavior through their
/// action manifest + validation hooks rather than by adding enum variants here.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Intent {
    /// Single-step move; the kernel rejects magnitudes greater than one cell.
    Move {
        dx: i32,
        dy: i32,
    },
    Interact {
        target: EntityId,
        verb: u32,
    },
    Speak {
        target: EntityId,
        act: u32,
        topic: u32,
    },
    Idle,
}

/// One validated intent as recorded in the log: everything replay needs.
#[derive(Clone, Debug)]
pub struct LoggedIntent {
    pub tick: u64,
    pub actor: EntityId,
    pub intent: Intent,
}

/// Outcomes emitted by the executor. The event log is the outcome ground truth
/// (memory, digests, and dialogue rendering are downstream consumers).
#[derive(Clone, Debug)]
pub enum Event {
    Moved {
        tick: u64,
        actor: EntityId,
        to: (i32, i32),
    },
    Interacted {
        tick: u64,
        actor: EntityId,
        target: EntityId,
        verb: u32,
    },
    Spoke {
        tick: u64,
        actor: EntityId,
        target: EntityId,
        act: u32,
        topic: u32,
    },
    Rejected {
        tick: u64,
        actor: EntityId,
        reason: RejectReason,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RejectReason {
    UnknownTool,
    OutOfRange,
    InvalidTarget,
    NotAfforded,
    /// A required need/resource was too low.
    Depleted,
}
