//! Read-only admission from a fully validated native generation. Admission is
//! all-or-nothing, including inactive worlds; unsupported state is never omitted.
use super::SnapshotQuery;
use crate::{
    BoundaryError,
    legacy::http_snapshot::{PublicFrame, TerrainView},
    persistence::RestoredRegistry,
};
use serde_json::{Value, json};

#[derive(Default)]
pub struct ReadRegistry {
    active: Option<String>,
    worlds: Vec<ReadWorld>,
}
struct ReadWorld {
    id: String,
    name: String,
    terrain: TerrainView,
}
pub(crate) enum ReadError {
    Query(&'static str),
    Coverage,
}
impl ReadRegistry {
    pub fn from_native(registry: RestoredRegistry) -> Result<Self, BoundaryError> {
        let mut worlds = Vec::new();
        for world in registry.worlds {
            if !world.resources.trees.trees().is_empty()
                || !world.resources.water.water().is_empty()
                || !world.resources.iron.veins().is_empty()
            {
                return Err(BoundaryError::invalid(
                    "$.http_read",
                    "missing populated-world HTTP coverage",
                ));
            }
            let id = world.model.model().id();
            let archive = world.terrain.as_ref().ok_or_else(|| {
                BoundaryError::invalid("$.http_read", "missing HTTP terrain coverage")
            })?;
            let terrain = TerrainView::new(&world, archive)?;
            worlds.push(ReadWorld {
                id: id.as_str().into(),
                name: world.name,
                terrain,
            });
        }
        let active = registry.active.map(|id| id.as_str().into());
        if active
            .as_ref()
            .is_some_and(|id| !worlds.iter().any(|world| &world.id == id))
        {
            return Err(BoundaryError::invalid(
                "$.http_read",
                "missing active world",
            ));
        }
        Ok(Self { active, worlds })
    }
    pub(crate) fn active(&self) -> Option<&str> {
        self.active.as_deref()
    }
    pub(crate) fn worlds(&self, can_manage: bool) -> Value {
        json!({"active_world": self.active, "can_manage":can_manage,
            "worlds":self.worlds.iter().map(|world| json!({"id":world.id,"name":world.name,
                "sandbox":world.terrain.sandbox(),"town_count":world.terrain.town_count()})).collect::<Vec<_>>()})
    }
    pub(crate) fn players(&self) -> Value {
        self.worlds
            .iter()
            .find(|w| Some(w.id.as_str()) == self.active())
            .map_or(json!({}), |w| w.terrain.players())
    }
    pub(crate) fn lanterns(&self) -> Value {
        self.worlds
            .iter()
            .find(|w| Some(w.id.as_str()) == self.active())
            .map_or(Value::Null, |w| w.terrain.lanterns())
    }
    pub(crate) fn frame(
        &self,
        query: &SnapshotQuery,
        sampled_at: f64,
    ) -> Result<PublicFrame, ReadError> {
        let world = self
            .worlds
            .iter()
            .find(|world| Some(world.id.as_str()) == self.active())
            .ok_or(ReadError::Coverage)?;
        world
            .terrain
            .validate_query(query)
            .map_err(ReadError::Query)?;
        world
            .terrain
            .frame(query, &world.name, sampled_at)
            .map_err(|_| ReadError::Coverage)
    }
}
