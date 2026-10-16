//! Persistent identity primitives, independent of location and collection order.
//!
//! One allocator belongs to one world, shared by every town and entity kind.
//! Persist its high-water mark even after deletion. Copy-based import must assign
//! identities through this allocator in recorded order and remap legacy references.
//! These internal types deliberately have no serialization implementation.

use alloc::{collections::BTreeSet, string::String};
use core::marker::PhantomData;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityError {
    EmptyOpaqueId,
    ZeroSerial,
    ForeignWorld,
    ForeignTown,
    DuplicateSerial,
    CounterBehindIssuedId,
    Exhausted,
}

macro_rules! opaque_id {
    ($name:ident) => {
        /// An existing opaque identifier, preserved verbatim (including Unicode).
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, IdentityError> {
                let value = value.into();
                if value.is_empty() {
                    return Err(IdentityError::EmptyOpaqueId);
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

opaque_id!(WorldId);
opaque_id!(TownId);

/// Ownership is world-qualified: an identical town key in another world is foreign.
/// This is a scope check, not authentication; the host must obtain it from credentials.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Owner {
    pub world: WorldId,
    pub town: TownId,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityIdentity {
    owner: Owner,
    serial: u64,
}

impl EntityIdentity {
    /// Used by explicit save conversion, never by an unvalidated client command.
    pub fn from_saved(owner: Owner, serial: u64) -> Result<Self, IdentityError> {
        if serial == 0 {
            return Err(IdentityError::ZeroSerial);
        }
        Ok(Self { owner, serial })
    }

    pub fn owner(&self) -> &Owner {
        &self.owner
    }

    pub fn serial(&self) -> u64 {
        self.serial
    }

    pub fn validate_owner(&self, owner: &Owner) -> Result<(), IdentityError> {
        if self.owner.world != owner.world {
            return Err(IdentityError::ForeignWorld);
        }
        if self.owner.town != owner.town {
            return Err(IdentityError::ForeignTown);
        }
        Ok(())
    }
}

/// The kind parameter prevents accidental building/actor/animal/NPC ID interchange.
///
/// ```compile_fail
/// use lantern_core::ids::{ActorId, BuildingId};
/// fn assign_worker(_: ActorId) {}
/// fn wrong(id: BuildingId) { assign_worker(id); }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PersistentId<K> {
    identity: EntityIdentity,
    kind: PhantomData<K>,
}

impl<K> PersistentId<K> {
    pub fn from_saved(identity: EntityIdentity) -> Self {
        Self {
            identity,
            kind: PhantomData,
        }
    }

    pub fn identity(&self) -> &EntityIdentity {
        &self.identity
    }
}

macro_rules! entity_kinds {
    ($($kind:ident => $alias:ident),+ $(,)?) => {$(
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum $kind {}
        pub type $alias = PersistentId<$kind>;
    )+};
}

entity_kinds!(Building => BuildingId, Actor => ActorId, Animal => AnimalId, Npc => NpcId);

#[derive(Debug)]
pub struct IdAllocator {
    world: WorldId,
    high_water: u64,
}

impl IdAllocator {
    pub fn new(world: WorldId) -> Self {
        Self {
            world,
            high_water: 0,
        }
    }

    /// Validate all live identities across all towns/kinds before accepting a save.
    /// Deleted IDs are protected by the persisted high-water mark, not this list.
    pub fn restore(
        world: WorldId,
        high_water: u64,
        issued: &[EntityIdentity],
    ) -> Result<Self, IdentityError> {
        let mut seen = BTreeSet::new();
        for id in issued {
            if id.owner.world != world {
                return Err(IdentityError::ForeignWorld);
            }
            if id.serial > high_water {
                return Err(IdentityError::CounterBehindIssuedId);
            }
            if !seen.insert(id.serial) {
                return Err(IdentityError::DuplicateSerial);
            }
        }
        Ok(Self { world, high_water })
    }

    pub fn high_water(&self) -> u64 {
        self.high_water
    }

    pub fn allocate<K>(&mut self, owner: &Owner) -> Result<PersistentId<K>, IdentityError> {
        if owner.world != self.world {
            return Err(IdentityError::ForeignWorld);
        }
        let serial = self
            .high_water
            .checked_add(1)
            .ok_or(IdentityError::Exhausted)?;
        self.high_water = serial;
        Ok(PersistentId::from_saved(EntityIdentity {
            owner: owner.clone(),
            serial,
        }))
    }
}
