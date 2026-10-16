use lantern_core::{ids::*, units::*};

fn owner(world: &str, town: &str) -> Owner {
    Owner {
        world: WorldId::new(world).unwrap(),
        town: TownId::new(town).unwrap(),
    }
}

#[test]
fn opaque_keys_are_preserved_without_normalization() {
    assert_eq!(WorldId::new("legacy").unwrap().as_str(), "legacy");
    assert_eq!(TownId::new("  Stad-é\0 ").unwrap().as_str(), "  Stad-é\0 ");
    assert_eq!(TownId::new(""), Err(IdentityError::EmptyOpaqueId));
    assert_eq!(WorldId::new(""), Err(IdentityError::EmptyOpaqueId));
}

#[test]
fn allocation_is_world_wide_across_imported_towns_and_entity_kinds() {
    let a = owner("legacy", "a");
    let b = owner("legacy", "b");
    let mut ids = IdAllocator::new(a.world.clone());
    let building: BuildingId = ids.allocate(&a).unwrap();
    let worker: ActorId = ids.allocate(&b).unwrap();
    let animal: AnimalId = ids.allocate(&a).unwrap();
    let npc: NpcId = ids.allocate(&b).unwrap();
    assert_eq!(building.identity().serial(), 1);
    assert_eq!(worker.identity().serial(), 2);
    assert_eq!(animal.identity().serial(), 3);
    assert_eq!(npc.identity().serial(), 4);
    assert_eq!(worker.identity().owner(), &b);
    let mut reloaded = IdAllocator::restore(
        a.world.clone(),
        ids.high_water(),
        &[building.identity().clone(), worker.identity().clone()],
    )
    .unwrap();
    // Even deleted animals/NPCs consume serials permanently.
    let next: BuildingId = reloaded.allocate(&b).unwrap();
    assert_eq!(next.identity().serial(), 5);
}

#[test]
fn movement_reorder_deletion_and_reconstruction_do_not_retarget_identity() {
    let town = owner("world", "town");
    let mut ids = IdAllocator::new(town.world.clone());
    let first: BuildingId = ids.allocate(&town).unwrap();
    let target: BuildingId = ids.allocate(&town).unwrap();
    let reference = target.clone();
    let mut buildings = vec![
        (first.clone(), Tile { x: 0, y: 0 }),
        (target, Tile { x: 1, y: 1 }),
    ];
    buildings[1].1 = Tile { x: -10, y: 20 };
    buildings.reverse();
    buildings.retain(|(id, _)| id != &first);
    // A save adapter reconstructs identity explicitly, independently of list slots.
    let restored = BuildingId::from_saved(
        EntityIdentity::from_saved(
            reference.identity().owner().clone(),
            reference.identity().serial(),
        )
        .unwrap(),
    );
    assert_eq!(
        buildings.iter().find(|(id, _)| id == &restored).unwrap().1,
        Tile { x: -10, y: 20 }
    );
    // A queued reference to the deleted first entity cannot select the next slot.
    assert!(buildings.iter().all(|(id, _)| id != &first));
}

#[test]
fn foreign_world_and_town_are_rejected_without_consuming_ids() {
    let a = owner("world", "a");
    let mut ids = IdAllocator::new(a.world.clone());
    let id: ActorId = ids.allocate(&a).unwrap();
    assert_eq!(
        id.identity().validate_owner(&owner("world", "b")),
        Err(IdentityError::ForeignTown)
    );
    assert_eq!(
        id.identity().validate_owner(&owner("elsewhere", "a")),
        Err(IdentityError::ForeignWorld)
    );
    assert_eq!(
        ids.allocate::<Actor>(&owner("elsewhere", "a")),
        Err(IdentityError::ForeignWorld)
    );
    assert_eq!(ids.high_water(), 1);
}

#[test]
fn restore_rejects_collisions_including_different_towns_and_stale_counters() {
    let a = owner("world", "a");
    let b = owner("world", "b");
    let id = EntityIdentity::from_saved(a.clone(), 8).unwrap();
    let collision = EntityIdentity::from_saved(b, 8).unwrap();
    assert_eq!(
        IdAllocator::restore(a.world.clone(), 8, &[id.clone(), collision]).unwrap_err(),
        IdentityError::DuplicateSerial
    );
    assert_eq!(
        IdAllocator::restore(a.world.clone(), 7, std::slice::from_ref(&id)).unwrap_err(),
        IdentityError::CounterBehindIssuedId
    );
    assert_eq!(
        IdAllocator::restore(WorldId::new("foreign").unwrap(), 8, &[id]).unwrap_err(),
        IdentityError::ForeignWorld
    );
    assert_eq!(
        EntityIdentity::from_saved(a.clone(), 0),
        Err(IdentityError::ZeroSerial)
    );
}

#[test]
fn serials_above_js_precision_remain_exact_and_exhaustion_never_wraps() {
    let town = owner("world", "town");
    let mut ids = IdAllocator::restore(town.world.clone(), 1 << 53, &[]).unwrap();
    let id: ActorId = ids.allocate(&town).unwrap();
    assert_eq!(id.identity().serial(), (1 << 53) + 1);
    let mut ids = IdAllocator::restore(town.world.clone(), u64::MAX - 1, &[]).unwrap();
    let last: ActorId = ids.allocate(&town).unwrap();
    assert_eq!(last.identity().serial(), u64::MAX);
    assert_eq!(ids.allocate::<Actor>(&town), Err(IdentityError::Exhausted));
    assert_eq!(ids.high_water(), u64::MAX);
    assert_eq!(TerrainSeed(u64::MAX).0, u64::MAX);
    assert_eq!(Count(u64::MAX).0, u64::MAX);
}

#[test]
fn fractional_units_preserve_bits_without_changing_rates() {
    for value in [
        0.0,
        -0.0,
        f64::MIN_POSITIVE,
        f64::from_bits(1),
        0.1,
        1.0 / 3.0,
        65.12345678901234,
        f64::MAX,
    ] {
        assert_eq!(
            Quantity::new(value).unwrap().get().to_bits(),
            value.to_bits()
        );
        assert_eq!(Work::new(value).unwrap().get().to_bits(), value.to_bits());
        assert_eq!(
            Practice::new(value).unwrap().get().to_bits(),
            value.to_bits()
        );
        assert_eq!(
            Seconds::new(value).unwrap().get().to_bits(),
            value.to_bits()
        );
        assert_eq!(
            TileCoordinate::new(value).unwrap().get().to_bits(),
            value.to_bits()
        );
    }
    let rate = 0.7;
    let elapsed = Seconds::new(0.05).unwrap();
    assert_eq!(
        Work::new(rate * elapsed.get()).unwrap().get().to_bits(),
        (rate * 0.05).to_bits()
    );
    let position = Position::new(-1.23456789012345, 0.0000000001).unwrap();
    assert_eq!(
        position.x.get().to_bits(),
        (-1.23456789012345_f64).to_bits()
    );
    assert_eq!(position.y.get().to_bits(), 0.0000000001_f64.to_bits());
}

#[test]
fn external_nonfinite_and_negative_amounts_are_rejected() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(Quantity::new(value), Err(UnitError::NonFinite));
        assert_eq!(Work::new(value), Err(UnitError::NonFinite));
        assert_eq!(Practice::new(value), Err(UnitError::NonFinite));
        assert_eq!(Seconds::new(value), Err(UnitError::NonFinite));
        assert_eq!(TileCoordinate::new(value), Err(UnitError::NonFinite));
        assert_eq!(Position::new(0.0, value), Err(UnitError::NonFinite));
        assert_eq!(Position::new(value, 0.0), Err(UnitError::NonFinite));
    }
    assert_eq!(Quantity::new(-0.01), Err(UnitError::Negative));
    assert_eq!(Work::new(-0.01), Err(UnitError::Negative));
    assert_eq!(Practice::new(-0.01), Err(UnitError::Negative));
    assert_eq!(Seconds::new(-0.01), Err(UnitError::Negative));
    assert_eq!(TileCoordinate::new(-0.01).unwrap().get(), -0.01);
}
