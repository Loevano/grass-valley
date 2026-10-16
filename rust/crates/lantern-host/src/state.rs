//! Host-owned facts. No JSON derives, no filesystem access, no authentication.

use std::collections::{BTreeMap, BTreeSet};

use lantern_core::{
    ids::{IdAllocator, Owner, PersistentId, TownId, WorldId},
    model::Town,
    units::TerrainSeed,
};

use crate::{BoundaryError, ErrorKind};

/// Opaque existing credential material. Hash verification/normalization is R20.
/// No Debug or Serialize: neither logs nor public DTOs may embed this record.
pub struct PrivateAccount {
    pub name: String,
    pub town_id: TownId,
    pub salt: String,
    pub pin_hash: String,
}

#[derive(Default)]
pub struct PrivateCredentials {
    pub tokens: BTreeMap<String, TownId>,
    pub players: BTreeMap<String, PrivateAccount>,
}

pub struct ModelWorld {
    pub(crate) id: WorldId,
    pub(crate) seed: TerrainSeed,
    pub(crate) allocator: IdAllocator,
    pub(crate) towns: Vec<Town>,
    pub(crate) credentials: PrivateCredentials,
}

impl ModelWorld {
    pub fn new(
        id: WorldId,
        seed: TerrainSeed,
        high_water: u64,
        towns: Vec<Town>,
        credentials: PrivateCredentials,
    ) -> Result<Self, BoundaryError> {
        let mut seen = BTreeSet::new();
        let mut issued = Vec::new();
        for (index, town) in towns.iter().enumerate() {
            let path = format!("$.towns[{index}]");
            if town.owner().world != id {
                return Err(BoundaryError::invalid(path, "foreign world"));
            }
            if !seen.insert(town.owner().town.clone()) {
                return Err(BoundaryError::invalid(path, "duplicate town"));
            }
            issued.extend(town.buildings().iter().map(|b| b.id.identity().clone()));
            issued.extend(
                town.actors()
                    .iter()
                    .map(|a| a.state().id.identity().clone()),
            );
        }
        let allocator = IdAllocator::restore(id.clone(), high_water, &issued)
            .map_err(|e| BoundaryError::at("$.high_water", ErrorKind::Identity(e)))?;
        for (index, (token, town_id)) in credentials.tokens.iter().enumerate() {
            if token.is_empty() || !seen.contains(town_id) {
                return Err(BoundaryError::invalid(
                    format!("$.credentials.tokens[{index}]"),
                    "invalid credential owner",
                ));
            }
        }
        for (index, (key, account)) in credentials.players.iter().enumerate() {
            if key.is_empty() || !seen.contains(&account.town_id) {
                return Err(BoundaryError::invalid(
                    format!("$.credentials.players[{index}]"),
                    "invalid credential owner",
                ));
            }
        }
        Ok(Self {
            id,
            seed,
            allocator,
            towns,
            credentials,
        })
    }

    pub fn id(&self) -> &WorldId {
        &self.id
    }
    pub fn seed(&self) -> TerrainSeed {
        self.seed
    }
    pub fn high_water(&self) -> u64 {
        self.allocator.high_water()
    }
    pub fn towns(&self) -> &[Town] {
        &self.towns
    }
    pub fn credentials(&self) -> &PrivateCredentials {
        &self.credentials
    }

    pub fn town_mut(&mut self, id: &TownId) -> Option<&mut Town> {
        self.towns.iter_mut().find(|town| &town.owner().town == id)
    }

    pub fn allocate<K>(&mut self, town: &TownId) -> Result<PersistentId<K>, BoundaryError> {
        if !self.towns.iter().any(|t| &t.owner().town == town) {
            return Err(BoundaryError::invalid("$.town_id", "missing town"));
        }
        self.allocator
            .allocate(&Owner {
                world: self.id.clone(),
                town: town.clone(),
            })
            .map_err(|e| BoundaryError::at("$.high_water", ErrorKind::Identity(e)))
    }
}
