//! Village scenario pack — skeleton.
//!
//! Wires up the [`ScenarioPack`] contract it will implement (action manifest,
//! validation rules, apply hooks, needs/stat registration). No behavior yet:
//! the v0 village rules land in a later step.

use mw_core::{ActionManifest, EntityId, Intent, RejectReason, ScenarioPack, StatRegistry, World};

#[derive(Default)]
pub struct VillagePack {
    manifest: ActionManifest,
}

impl VillagePack {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ScenarioPack for VillagePack {
    fn manifest(&self) -> &ActionManifest {
        &self.manifest
    }

    fn validate(
        &self,
        _world: &World,
        _actor: EntityId,
        _intent: &Intent,
    ) -> Result<(), RejectReason> {
        todo!("village validation rules")
    }

    fn apply(&self, _world: &mut World, _actor: EntityId, _intent: &Intent) {
        todo!("village effect hooks")
    }

    fn register(&self, _registry: &mut StatRegistry) {
        todo!("register village needs/stats")
    }
}
