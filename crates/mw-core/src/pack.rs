//! The kernel's built-in scenario pack.
//!
//! `KernelPack` is the minimal pack the bare kernel (sim harness, tests) runs
//! against: it publishes the placeholder action manifest and adds no rules or
//! effects beyond the kernel's own. Real scenarios (e.g. `mw-village`) provide
//! their own [`ScenarioPack`].

use crate::contracts::{ActionManifest, ArgKind, ArgSchema, ScenarioPack, ToolDescriptor};
use crate::entity::{EntityId, StatRegistry};
use crate::intent::{Intent, RejectReason};
use crate::world::World;

pub struct KernelPack {
    manifest: ActionManifest,
}

impl KernelPack {
    pub fn new() -> Self {
        let tools = vec![
            ToolDescriptor {
                id: 0,
                name: "move".to_owned(),
                args: vec![
                    ArgSchema {
                        name: "dx".to_owned(),
                        kind: ArgKind::Scalar,
                    },
                    ArgSchema {
                        name: "dy".to_owned(),
                        kind: ArgKind::Scalar,
                    },
                ],
            },
            ToolDescriptor {
                id: 1,
                name: "interact".to_owned(),
                args: vec![
                    ArgSchema {
                        name: "target".to_owned(),
                        kind: ArgKind::EntityRef,
                    },
                    ArgSchema {
                        name: "verb".to_owned(),
                        kind: ArgKind::Enum { variants: 4 },
                    },
                ],
            },
            ToolDescriptor {
                id: 2,
                name: "speak".to_owned(),
                args: vec![
                    ArgSchema {
                        name: "target".to_owned(),
                        kind: ArgKind::EntityRef,
                    },
                    ArgSchema {
                        name: "act".to_owned(),
                        kind: ArgKind::Enum { variants: 4 },
                    },
                    ArgSchema {
                        name: "topic".to_owned(),
                        kind: ArgKind::Scalar,
                    },
                ],
            },
            ToolDescriptor {
                id: 3,
                name: "idle".to_owned(),
                args: Vec::new(),
            },
        ];
        Self {
            manifest: ActionManifest { tools },
        }
    }
}

impl Default for KernelPack {
    fn default() -> Self {
        Self::new()
    }
}

impl ScenarioPack for KernelPack {
    fn manifest(&self) -> &ActionManifest {
        &self.manifest
    }

    fn validate(
        &self,
        _world: &World,
        _actor: EntityId,
        _intent: &Intent,
    ) -> Result<(), RejectReason> {
        // No scenario rules beyond the kernel's base validation.
        Ok(())
    }

    fn apply(&self, _world: &mut World, _actor: EntityId, _intent: &Intent) {}

    fn register(&self, _registry: &mut StatRegistry) {}
}
