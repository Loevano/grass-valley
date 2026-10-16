//! Legacy index boundary: require the ID from the client's observed slot, validate
//! before queuing, then retain only the stable ID. Index-only references fail
//! deserialization. The caller supplies the authenticated owner, never client data.

use crate::{BoundaryError, ErrorKind, dto::EntityIdDto};
use lantern_core::{
    ids::{ActorId, BuildingId, IdentityError, Owner},
    model::{Actor, Building, Town},
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexedReferenceDto {
    pub index: usize,
    pub expected_id: EntityIdDto,
}

fn authorize(town: &Town, owner: &Owner) -> Result<(), BoundaryError> {
    let error = if town.owner().world != owner.world {
        Some(IdentityError::ForeignWorld)
    } else if town.owner().town != owner.town {
        Some(IdentityError::ForeignTown)
    } else {
        None
    };
    match error {
        Some(error) => Err(BoundaryError::at("$.owner", ErrorKind::Identity(error))),
        None => Ok(()),
    }
}

macro_rules! reference {
    ($name:ident, $id:ty, $entity:ty, $admit:ident, $resolve:ident, $collection:ident, $lookup:ident, $get_id:expr) => {
        pub struct $name {
            id: $id,
        }
        impl $name {
            pub fn id(&self) -> &$id {
                &self.id
            }
        }

        pub fn $admit(
            town: &Town,
            owner: &Owner,
            input: IndexedReferenceDto,
        ) -> Result<$name, BoundaryError> {
            authorize(town, owner)?;
            let id: $id = input.expected_id.into_id("$.expected_id")?;
            id.identity()
                .validate_owner(owner)
                .map_err(|e| BoundaryError::at("$.expected_id", ErrorKind::Identity(e)))?;
            if !town
                .$collection()
                .get(input.index)
                .is_some_and(|entity| ($get_id)(entity) == &id)
            {
                return Err(BoundaryError::invalid(
                    "$.index",
                    "stale index or mismatched ID",
                ));
            }
            Ok($name { id })
        }

        pub fn $resolve<'a>(
            town: &'a Town,
            owner: &Owner,
            reference: &$name,
        ) -> Result<&'a $entity, BoundaryError> {
            authorize(town, owner)?;
            reference
                .id
                .identity()
                .validate_owner(owner)
                .map_err(|e| BoundaryError::at("$.id", ErrorKind::Identity(e)))?;
            town.$lookup(&reference.id)
                .map_err(|e| BoundaryError::at("$.id", ErrorKind::Model(e)))
        }
    };
}

fn building_id(building: &Building) -> &BuildingId {
    &building.id
}
fn actor_id(actor: &Actor) -> &ActorId {
    &actor.state().id
}

reference!(
    BuildingReference,
    BuildingId,
    Building,
    admit_building,
    resolve_building,
    buildings,
    building,
    building_id
);
reference!(
    ActorReference,
    ActorId,
    Actor,
    admit_actor,
    resolve_actor,
    actors,
    actor,
    actor_id
);
