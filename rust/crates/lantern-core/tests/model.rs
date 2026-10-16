use lantern_core::{
    ids::{
        ActorId, BuildingId, EntityIdentity, IdAllocator, IdentityError, Owner, TownId, WorldId,
    },
    model::*,
    units::*,
};

const NAV: NavigationRevision = NavigationRevision(7);

fn position(x: f64, y: f64) -> Position {
    Position::new(x, y).unwrap()
}

fn owner(world: &str, town: &str) -> Owner {
    Owner {
        world: WorldId::new(world).unwrap(),
        town: TownId::new(town).unwrap(),
    }
}

fn cargo() -> Cargo {
    Cargo::from_entries([
        ("wood".into(), Quantity::new(1.23456789012345).unwrap()),
        (
            "fish_perch".into(),
            Quantity::new(f64::from_bits(1)).unwrap(),
        ),
    ])
    .unwrap()
}

struct Fixture {
    town: Town,
    worker: ActorId,
    other: ActorId,
    workplace: BuildingId,
    store: BuildingId,
}

fn fixture() -> Fixture {
    let owner = owner("world", "town");
    let mut ids = IdAllocator::new(owner.world.clone());
    let workplace = ids.allocate(&owner).unwrap();
    let store = ids.allocate(&owner).unwrap();
    let worker = ids.allocate(&owner).unwrap();
    let other = ids.allocate(&owner).unwrap();
    let buildings = vec![
        Building {
            id: workplace.clone(),
            location: Tile { x: 9, y: 3 },
            construction: ConstructionState::Complete,
        },
        Building {
            id: store.clone(),
            location: Tile { x: 2, y: 2 },
            construction: ConstructionState::InProgress {
                remaining: Work::new(0.125).unwrap(),
            },
        },
    ];
    let actors = vec![
        ActorState::new(worker.clone(), position(11.250000000123, -2.125), cargo()),
        ActorState::new(other.clone(), position(0.25, 1.0), Cargo::default()),
    ];
    Fixture {
        town: Town::restore(owner, buildings, actors).unwrap(),
        worker,
        other,
        workplace,
        store,
    }
}

fn returning(f: &Fixture) -> Task {
    Task::Work {
        workplace: f.workplace.clone(),
        target: Destination::Position(position(12.0, 3.0)),
        phase: WorkPhase::Returning,
        elapsed: Seconds::new(0.4).unwrap(),
    }
}

fn begin(f: &mut Fixture) -> RouteRequest {
    f.town.set_task(&f.worker, returning(f)).unwrap();
    f.town
        .request_route(&f.worker, Destination::Building(f.workplace.clone()), NAV)
        .unwrap()
}

fn restore(town: &Town) -> Town {
    Town::restore(
        town.owner().clone(),
        town.buildings().to_vec(),
        town.actors().iter().map(|a| a.state().clone()).collect(),
    )
    .unwrap()
}

#[test]
fn reassignment_and_cancellation_preserve_actor_facts() {
    let mut f = fixture();
    let before = f.town.actor(&f.worker).unwrap().state().clone();
    let old = begin(&mut f);
    f.town
        .set_task(
            &f.worker,
            Task::Construction {
                site: f.store.clone(),
                phase: ConstructionPhase::Outbound,
                elapsed: Seconds::new(0.0).unwrap(),
            },
        )
        .unwrap();
    f.town.set_task(&f.worker, Task::Idle).unwrap();
    let actor = f.town.actor(&f.worker).unwrap();
    assert_eq!(actor.state().position, before.position);
    assert_eq!(actor.state().cargo, before.cargo);
    assert_eq!(actor.state().id, before.id);
    assert_eq!(actor.state().task_generation.get(), 3);
    assert_eq!(actor.route(), &RouteState::NotRequested);
    assert_eq!(
        f.town
            .accept_route(&f.worker, &old, NAV, RouteOutcome::Arrived),
        Err(ModelError::StaleRoute)
    );
    assert_eq!(
        f.town
            .request_route(&f.worker, Destination::Building(f.workplace), NAV),
        Err(ModelError::NoTask)
    );
}

#[test]
fn pending_unreachable_blocked_and_cancelled_retain_return_leg_and_cargo() {
    let mut f = fixture();
    let request = begin(&mut f);
    let facts = f.town.actor(&f.worker).unwrap().state().clone();
    assert!(matches!(
        f.town.actor(&f.worker).unwrap().route(),
        RouteState::Pending(_)
    ));
    assert_eq!(
        f.town
            .accept_route(&f.worker, &request, NAV, RouteOutcome::Ready(vec![])),
        Err(ModelError::EmptyRoute)
    );
    assert!(matches!(
        f.town.actor(&f.worker).unwrap().route(),
        RouteState::Pending(_)
    ));
    f.town
        .accept_route(&f.worker, &request, NAV, RouteOutcome::Unreachable)
        .unwrap();
    assert!(matches!(
        f.town.actor(&f.worker).unwrap().route(),
        RouteState::Unreachable(_)
    ));
    assert_eq!(f.town.actor(&f.worker).unwrap().state(), &facts);
    let retry = f
        .town
        .request_route(&f.worker, Destination::Building(f.workplace.clone()), NAV)
        .unwrap();
    f.town
        .accept_route(
            &f.worker,
            &retry,
            NAV,
            RouteOutcome::Ready(vec![position(10.5, 3.0)]),
        )
        .unwrap();
    f.town.block_route(&f.worker).unwrap();
    assert!(matches!(
        f.town.actor(&f.worker).unwrap().route(),
        RouteState::Blocked(_)
    ));
    f.town.cancel_route(&f.worker).unwrap();
    assert_eq!(
        f.town.actor(&f.worker).unwrap().route(),
        &RouteState::Cancelled
    );
    let state = f.town.actor(&f.worker).unwrap().state();
    assert_eq!(state.task, facts.task);
    assert_eq!(state.cargo, facts.cargo);
    assert_eq!(state.position, facts.position);
    assert_eq!(
        f.town
            .accept_route(&f.worker, &retry, NAV, RouteOutcome::Arrived),
        Err(ModelError::StaleRoute)
    );
}

#[test]
fn superseded_duplicate_and_cross_actor_results_are_rejected() {
    let mut f = fixture();
    let old = begin(&mut f);
    let current = f
        .town
        .request_route(&f.worker, Destination::Building(f.workplace.clone()), NAV)
        .unwrap();
    assert_ne!(old, current);
    f.town.set_task(&f.other, returning(&f)).unwrap();
    f.town
        .request_route(&f.other, Destination::Building(f.workplace), NAV)
        .unwrap();
    assert_eq!(
        f.town
            .accept_route(&f.worker, &old, NAV, RouteOutcome::Arrived),
        Err(ModelError::StaleRoute)
    );
    assert_eq!(
        f.town
            .accept_route(&f.other, &current, NAV, RouteOutcome::Arrived),
        Err(ModelError::StaleRoute)
    );
    f.town
        .accept_route(&f.worker, &current, NAV, RouteOutcome::Arrived)
        .unwrap();
    let after = f.town.actor(&f.worker).unwrap().clone();
    assert!(matches!(after.route(), RouteState::Arrived(_)));
    assert_eq!(
        f.town
            .accept_route(&f.worker, &current, NAV, RouteOutcome::Unreachable),
        Err(ModelError::StaleRoute)
    );
    assert_eq!(f.town.actor(&f.worker).unwrap(), &after);
}

#[test]
fn relevant_navigation_change_and_position_recovery_reject_old_results() {
    let mut f = fixture();
    let request = begin(&mut f);
    let before = f.town.actor(&f.worker).unwrap().clone();
    assert_eq!(
        f.town.accept_route(
            &f.worker,
            &request,
            NavigationRevision(8),
            RouteOutcome::Arrived
        ),
        Err(ModelError::StaleRoute)
    );
    assert_eq!(f.town.actor(&f.worker).unwrap(), &before);
    // Repositioning another actor does not invalidate this request.
    f.town
        .reposition_actor(&f.other, position(0.5, 1.5))
        .unwrap();
    f.town
        .accept_route(&f.worker, &request, NAV, RouteOutcome::Arrived)
        .unwrap();
    let request = f
        .town
        .request_route(&f.worker, Destination::Building(f.workplace), NAV)
        .unwrap();
    f.town
        .reposition_actor(&f.worker, position(11.250000000124, -2.125))
        .unwrap();
    assert_eq!(
        f.town
            .accept_route(&f.worker, &request, NAV, RouteOutcome::Arrived),
        Err(ModelError::StaleRoute)
    );
    assert_eq!(
        f.town.actor(&f.worker).unwrap().state().cargo,
        before.state().cargo
    );
    assert_eq!(
        f.town.actor(&f.worker).unwrap().state().task,
        before.state().task
    );
}

#[test]
fn moving_workplace_keeps_identity_actor_progress_and_unrelated_route() {
    let mut f = fixture();
    let request = begin(&mut f);
    f.town
        .accept_route(
            &f.worker,
            &request,
            NAV,
            RouteOutcome::Ready(vec![position(10.5, 3.0)]),
        )
        .unwrap();
    let before = f.town.actor(&f.worker).unwrap().state().clone();
    f.town
        .set_task(
            &f.other,
            Task::Transport {
                source: f.workplace.clone(),
                destination: f.store.clone(),
                phase: TransportPhase::ToDropoff,
            },
        )
        .unwrap();
    let other_route = f
        .town
        .request_route(&f.other, Destination::Building(f.store.clone()), NAV)
        .unwrap();
    let other_before = f.town.actor(&f.other).unwrap().clone();
    f.town
        .relocate_building(&f.workplace, Tile { x: 8, y: 4 })
        .unwrap();
    assert_eq!(f.town.actor(&f.worker).unwrap().state(), &before);
    assert_eq!(
        f.town.actor(&f.worker).unwrap().route(),
        &RouteState::NotRequested
    );
    assert_eq!(f.town.actor(&f.other).unwrap(), &other_before);
    assert_eq!(
        f.town
            .accept_route(&f.worker, &request, NAV, RouteOutcome::Arrived),
        Err(ModelError::StaleRoute)
    );
    let retry = f
        .town
        .request_route(&f.worker, Destination::Building(f.workplace.clone()), NAV)
        .unwrap();
    assert_eq!(retry.origin(), before.position);
    assert_eq!(retry.goal(), position(8.0, 4.0));
    assert_eq!(retry.destination(), &Destination::Building(f.workplace));
    f.town
        .accept_route(&f.other, &other_route, NAV, RouteOutcome::Arrived)
        .unwrap();
}

#[test]
fn reordering_and_restoration_keep_facts_rebuild_routes_and_do_not_reuse_requests() {
    let mut f = fixture();
    let old = begin(&mut f);
    let mut actors: Vec<_> = f.town.actors().iter().map(|a| a.state().clone()).collect();
    actors.reverse();
    let mut buildings = f.town.buildings().to_vec();
    buildings.reverse();
    let mut reloaded = Town::restore(f.town.owner().clone(), buildings, actors).unwrap();
    assert_eq!(
        reloaded.actor(&f.worker).unwrap().state(),
        f.town.actor(&f.worker).unwrap().state()
    );
    assert_eq!(
        reloaded.building(&f.workplace).unwrap(),
        f.town.building(&f.workplace).unwrap()
    );
    assert_eq!(
        reloaded.actor(&f.worker).unwrap().route(),
        &RouteState::NotRequested
    );
    let new = reloaded
        .request_route(&f.worker, Destination::Building(f.workplace), NAV)
        .unwrap();
    assert_ne!(new, old);
    assert_eq!(
        reloaded.accept_route(&f.worker, &old, NAV, RouteOutcome::Arrived),
        Err(ModelError::StaleRoute)
    );
    reloaded
        .accept_route(&f.worker, &new, NAV, RouteOutcome::Arrived)
        .unwrap();
}

#[test]
fn foreign_and_missing_references_fail_atomically() {
    let mut f = fixture();
    begin(&mut f);
    for foreign in [owner("world", "foreign"), owner("elsewhere", "town")] {
        let expected = if foreign.world == f.town.owner().world {
            IdentityError::ForeignTown
        } else {
            IdentityError::ForeignWorld
        };
        let mut ids = IdAllocator::new(foreign.world.clone());
        let building: BuildingId = ids.allocate(&foreign).unwrap();
        let actor: ActorId = ids.allocate(&foreign).unwrap();
        let before = restore(&f.town);
        let route = f.town.actor(&f.worker).unwrap().route().clone();
        assert_eq!(
            f.town.building(&building),
            Err(ModelError::Identity(expected))
        );
        assert_eq!(f.town.actor(&actor), Err(ModelError::Identity(expected)));
        let tasks = [
            Task::Work {
                workplace: f.workplace.clone(),
                target: Destination::Building(building.clone()),
                phase: WorkPhase::Outbound,
                elapsed: Seconds::new(0.0).unwrap(),
            },
            Task::Transport {
                source: building.clone(),
                destination: f.store.clone(),
                phase: TransportPhase::ToPickup,
            },
            Task::Transport {
                source: f.store.clone(),
                destination: building.clone(),
                phase: TransportPhase::ToPickup,
            },
            Task::Construction {
                site: building.clone(),
                phase: ConstructionPhase::Building,
                elapsed: Seconds::new(0.1).unwrap(),
            },
        ];
        for task in tasks {
            assert_eq!(
                f.town.set_task(&f.worker, task),
                Err(ModelError::Identity(expected))
            );
        }
        assert_eq!(
            f.town
                .request_route(&f.worker, Destination::Building(building.clone()), NAV),
            Err(ModelError::Identity(expected))
        );
        assert_eq!(
            f.town.relocate_building(&building, Tile { x: 0, y: 0 }),
            Err(ModelError::Identity(expected))
        );
        assert_eq!(restore(&f.town), before);
        assert_eq!(f.town.actor(&f.worker).unwrap().route(), &route);
    }
    let missing_building =
        BuildingId::from_saved(EntityIdentity::from_saved(f.town.owner().clone(), 999).unwrap());
    let missing_actor =
        ActorId::from_saved(EntityIdentity::from_saved(f.town.owner().clone(), 998).unwrap());
    assert_eq!(
        f.town.building(&missing_building),
        Err(ModelError::MissingBuilding)
    );
    assert_eq!(
        f.town.set_task(&missing_actor, Task::Idle),
        Err(ModelError::MissingActor)
    );
    assert_eq!(
        f.town
            .request_route(&f.worker, Destination::Building(missing_building), NAV),
        Err(ModelError::MissingBuilding)
    );
}

#[test]
fn restore_rejects_foreign_duplicate_and_dangling_entities() {
    let mut f = fixture();
    begin(&mut f);
    let state = f.town.actor(&f.worker).unwrap().state().clone();
    let buildings = f.town.buildings().to_vec();
    let own = f.town.owner().clone();
    assert_eq!(
        Town::restore(
            own.clone(),
            buildings.clone(),
            vec![state.clone(), state.clone()]
        )
        .unwrap_err(),
        ModelError::Identity(IdentityError::DuplicateSerial)
    );
    let mut collision = state.clone();
    collision.id = ActorId::from_saved(f.workplace.identity().clone());
    assert_eq!(
        Town::restore(own.clone(), buildings.clone(), vec![collision]).unwrap_err(),
        ModelError::Identity(IdentityError::DuplicateSerial)
    );
    assert_eq!(
        Town::restore(own.clone(), vec![], vec![state.clone()]).unwrap_err(),
        ModelError::MissingBuilding
    );
    assert_eq!(
        Town::restore(
            owner("world", "foreign"),
            buildings.clone(),
            vec![state.clone()]
        )
        .unwrap_err(),
        ModelError::Identity(IdentityError::ForeignTown)
    );
    let mut dangling = state;
    dangling.task = Task::Construction {
        site: BuildingId::from_saved(
            EntityIdentity::from_saved(owner("world", "foreign"), 999).unwrap(),
        ),
        phase: ConstructionPhase::Building,
        elapsed: Seconds::new(0.0).unwrap(),
    };
    assert_eq!(
        Town::restore(own, buildings, vec![dangling]).unwrap_err(),
        ModelError::Identity(IdentityError::ForeignTown)
    );
}

#[test]
fn generation_exhaustion_rejects_changes_without_losing_facts() {
    let mut f = fixture();
    begin(&mut f);
    let mut state = f.town.actor(&f.worker).unwrap().state().clone();
    state.task_generation = TaskGeneration::from_saved(u64::MAX);
    state.route_generation = RouteGeneration::from_saved(u64::MAX);
    let mut town = Town::restore(
        f.town.owner().clone(),
        f.town.buildings().to_vec(),
        vec![state],
    )
    .unwrap();
    let before = restore(&town);
    assert_eq!(
        town.set_task(&f.worker, Task::Idle),
        Err(ModelError::GenerationExhausted)
    );
    assert_eq!(
        town.request_route(&f.worker, Destination::Building(f.workplace), NAV),
        Err(ModelError::GenerationExhausted)
    );
    assert_eq!(town, before);
}

#[test]
fn cargo_preserves_named_fractional_goods_and_rejects_ambiguous_keys() {
    let goods = cargo();
    assert_eq!(
        goods.goods()["wood"].get().to_bits(),
        1.23456789012345_f64.to_bits()
    );
    assert_eq!(goods.goods()["fish_perch"].get().to_bits(), 1);
    let q = Quantity::new(0.1).unwrap();
    assert_eq!(
        Cargo::from_entries([("".into(), q)]),
        Err(ModelError::InvalidCargoKey)
    );
    assert_eq!(
        Cargo::from_entries([("wood".into(), q), ("wood".into(), q)]),
        Err(ModelError::DuplicateCargoKey)
    );
    assert_eq!(
        Cargo::from_entries([("wood".into(), Quantity::new(0.0).unwrap())])
            .unwrap()
            .goods()
            .len(),
        1
    );
}
