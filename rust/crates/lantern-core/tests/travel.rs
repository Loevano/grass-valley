use lantern_core::{
    ids::{ActorId, IdAllocator, Owner, TownId, WorldId},
    model::*,
    travel::{Speed, Status},
    units::{Position, Quantity, Seconds, Tile},
};

fn p(x: f64, y: f64) -> Position {
    Position::new(x, y).unwrap()
}
fn seconds(value: f64) -> Seconds {
    Seconds::new(value).unwrap()
}
fn fixture() -> (Town, ActorId) {
    let owner = Owner {
        world: WorldId::new("world").unwrap(),
        town: TownId::new("town").unwrap(),
    };
    let mut ids = IdAllocator::new(owner.world.clone());
    let workplace = ids.allocate(&owner).unwrap();
    let id = ids.allocate(&owner).unwrap();
    let mut town = Town::restore(
        owner,
        vec![Building {
            id: workplace,
            location: Tile { x: 2, y: 1 },
            construction: ConstructionState::Complete,
        }],
        vec![ActorState::new(
            id.clone(),
            p(0.25, 0.0),
            Cargo::from_entries([("wood".into(), Quantity::new(1.125).unwrap())]).unwrap(),
        )],
    )
    .unwrap();
    town.set_task(
        &id,
        Task::Move {
            destination: p(2.0, 1.0),
        },
    )
    .unwrap();
    (town, id)
}
fn request(town: &mut Town, id: &ActorId) -> RouteRequest {
    town.request_route(
        id,
        Destination::Position(p(2.0, 1.0)),
        NavigationRevision(1),
    )
    .unwrap()
}
fn ready(town: &mut Town, id: &ActorId, path: Vec<Position>) {
    let r = request(town, id);
    town.accept_route(id, &r, r.navigation(), RouteOutcome::Ready(path))
        .unwrap();
}
fn walk(town: &mut Town, id: &ActorId, elapsed: f64) -> lantern_core::travel::Advance {
    town.advance_travel(id, seconds(elapsed), 100, |_, _| Speed::new(1.0))
        .unwrap()
}
fn assert_facts(before: &ActorState, after: &ActorState) {
    assert_eq!(before.cargo, after.cargo);
    assert_eq!(before.task, after.task);
    assert_eq!(before.task_generation, after.task_generation);
}

#[test]
fn fractional_progress_and_arrival_once_with_unused_time() {
    let (mut town, id) = fixture();
    ready(&mut town, &id, vec![p(0.25, 0.0), p(1.0, 0.0), p(2.0, 0.0)]);
    let facts = town.actor(&id).unwrap().state().clone();
    let a = walk(&mut town, &id, 0.5);
    assert_eq!(a.status, Status::Walking);
    assert!(!a.arrived_now);
    assert_eq!(town.actor(&id).unwrap().state().position, p(0.75, 0.0));
    let a = walk(&mut town, &id, 2.0);
    assert_eq!(a.status, Status::Arrived);
    assert!(a.arrived_now);
    assert_eq!(a.remaining, seconds(0.75));
    assert_eq!(town.actor(&id).unwrap().state().position, p(2.0, 0.0));
    let a = walk(&mut town, &id, 5.0);
    assert!(!a.arrived_now);
    assert_eq!(a.remaining, seconds(5.0));
    assert_facts(&facts, town.actor(&id).unwrap().state());
}

#[test]
fn waiting_unreachable_cancelled_and_not_requested_never_move_or_bank_time() {
    let (mut town, id) = fixture();
    let before = town.actor(&id).unwrap().state().clone();
    assert_eq!(walk(&mut town, &id, 100.0).status, Status::NotRequested);
    let r = request(&mut town, &id);
    let a = walk(&mut town, &id, 100.0);
    assert_eq!(a.status, Status::Waiting);
    assert_eq!(a.remaining, seconds(100.0));
    town.accept_route(&id, &r, r.navigation(), RouteOutcome::Unreachable)
        .unwrap();
    assert_eq!(walk(&mut town, &id, 100.0).status, Status::Unreachable);
    town.cancel_route(&id).unwrap();
    assert_eq!(walk(&mut town, &id, 100.0).status, Status::Cancelled);
    assert_eq!(before.position, town.actor(&id).unwrap().state().position);
    ready(&mut town, &id, vec![p(1.0, 0.0)]);
    walk(&mut town, &id, 0.25);
    assert_eq!(town.actor(&id).unwrap().state().position, p(0.5, 0.0));
    assert_facts(&before, town.actor(&id).unwrap().state());
}

#[test]
fn diagonals_road_speeds_and_subdivision_use_actual_distance() {
    let (mut town, id) = fixture();
    let path = vec![p(1.25, 1.0), p(2.25, 1.0)];
    ready(&mut town, &id, path.clone());
    let mut divided = Town::restore(
        town.owner().clone(),
        vec![],
        vec![town.actor(&id).unwrap().state().clone()],
    )
    .unwrap();
    ready(&mut divided, &id, path);
    let speed =
        |_: Position, target: Position| Speed::new(if target.x.get() > 2.0 { 1.5 } else { 1.0 });
    town.advance_travel(&id, seconds(1.7), 20, speed).unwrap();
    for _ in 0..17 {
        divided
            .advance_travel(&id, seconds(0.1), 20, speed)
            .unwrap();
    }
    let a = town.actor(&id).unwrap().state().position;
    let b = divided.actor(&id).unwrap().state().position;
    let expected = 1.25 + (1.7 - 2.0_f64.sqrt()) * 1.5;
    assert!((a.x.get() - expected).abs() < 1e-12);
    assert_eq!(a.y.get(), 1.0);
    assert!((a.x.get() - b.x.get()).abs() < 1e-12);
}

#[test]
fn current_segment_guard_preserves_fractional_position_on_block() {
    let (mut town, id) = fixture();
    ready(&mut town, &id, vec![p(1.0, 0.0), p(2.0, 0.0)]);
    walk(&mut town, &id, 0.25);
    let before = town.actor(&id).unwrap().state().clone();
    let a = town
        .advance_travel(&id, seconds(10.0), 10, |from, to| {
            assert_eq!(from, p(0.5, 0.0));
            assert_eq!(to, p(1.0, 0.0));
            None
        })
        .unwrap();
    assert_eq!(a.status, Status::Blocked);
    assert!(!a.arrived_now);
    assert_eq!(a.remaining, seconds(10.0));
    assert_eq!(&before, town.actor(&id).unwrap().state());
    assert_eq!(walk(&mut town, &id, 5.0).status, Status::Blocked);
}

#[test]
fn segment_budget_bounds_duplicates_and_keeps_the_cursor() {
    let (mut town, id) = fixture();
    ready(&mut town, &id, vec![p(0.25, 0.0); 4]);
    let a = town
        .advance_travel(&id, seconds(1.0), 2, |_, _| panic!("zero distance"))
        .unwrap();
    assert_eq!(a.segments, 2);
    assert_eq!(a.status, Status::Walking);
    assert_eq!(a.remaining, seconds(1.0));
    let a = town
        .advance_travel(&id, seconds(1.0), 2, |_, _| panic!("zero distance"))
        .unwrap();
    assert_eq!(a.segments, 2);
    assert!(a.arrived_now);
}

#[test]
fn zero_time_and_zero_budget_do_not_move() {
    let (mut town, id) = fixture();
    ready(&mut town, &id, vec![p(1.0, 0.0)]);
    town.advance_travel(&id, seconds(0.0), 10, |_, _| panic!("zero time"))
        .unwrap();
    town.advance_travel(&id, seconds(10.0), 0, |_, _| panic!("zero budget"))
        .unwrap();
    assert_eq!(town.actor(&id).unwrap().state().position, p(0.25, 0.0));
}

#[test]
fn speed_validation_and_overflowing_route_reject_before_movement() {
    for invalid in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(Speed::new(invalid), None);
    }
    let (mut town, id) = fixture();
    town.reposition_actor(&id, p(-f64::MAX, 0.0)).unwrap();
    let r = request(&mut town, &id);
    assert_eq!(
        town.accept_route(
            &id,
            &r,
            r.navigation(),
            RouteOutcome::Ready(vec![p(f64::MAX, 0.0)])
        ),
        Err(ModelError::InvalidRouteGeometry)
    );
    assert_eq!(town.actor(&id).unwrap().travel_status(), Status::Waiting);
}

#[test]
fn accepted_already_arrived_is_not_a_repeated_movement_event() {
    let (mut town, id) = fixture();
    let r = request(&mut town, &id);
    town.accept_route(&id, &r, r.navigation(), RouteOutcome::Arrived)
        .unwrap();
    let a = walk(&mut town, &id, 10.0);
    assert_eq!(a.status, Status::Arrived);
    assert!(!a.arrived_now);
    assert_eq!(a.remaining, seconds(10.0));
}

#[test]
fn finite_slow_speed_with_overflowing_duration_still_advances() {
    let (mut town, id) = fixture();
    ready(&mut town, &id, vec![p(1e300, 0.0)]);
    town.advance_travel(&id, seconds(1e200), 1, |_, _| Speed::new(1e-100))
        .unwrap();
    let x = town.actor(&id).unwrap().state().position.x.get();
    assert!((x / 1e100 - 1.0).abs() < 1e-12);
}

#[test]
fn return_leg_retains_cargo_phase_and_work_clock_through_block_and_arrival() {
    let (mut town, id) = fixture();
    let workplace = town.buildings()[0].id.clone();
    town.set_task(
        &id,
        Task::Work {
            workplace: workplace.clone(),
            target: Destination::Position(p(10.0, 2.0)),
            phase: WorkPhase::Returning,
            elapsed: seconds(0.375),
        },
    )
    .unwrap();
    ready(&mut town, &id, vec![p(1.0, 0.0), p(2.0, 0.0)]);
    let facts = town.actor(&id).unwrap().state().clone();
    let a = town
        .advance_travel(&id, seconds(2.0), 10, |_, target| {
            if target.x.get() > 1.0 {
                None
            } else {
                Speed::new(1.0)
            }
        })
        .unwrap();
    assert_eq!(a.status, Status::Blocked);
    assert_eq!(a.remaining, seconds(1.25));
    assert_eq!(town.actor(&id).unwrap().state().position, p(1.0, 0.0));
    assert_facts(&facts, town.actor(&id).unwrap().state());
    // Explicit new intent is required; a later slice owns automatic recovery.
    let r = town
        .request_route(&id, Destination::Building(workplace), NavigationRevision(2))
        .unwrap();
    town.accept_route(
        &id,
        &r,
        r.navigation(),
        RouteOutcome::Ready(vec![p(2.0, 0.0)]),
    )
    .unwrap();
    assert!(walk(&mut town, &id, 1.0).arrived_now);
    assert_facts(&facts, town.actor(&id).unwrap().state());
}
