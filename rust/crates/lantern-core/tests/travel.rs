use lantern_core::{
    ids::{ActorId, IdAllocator, Owner, TownId, WorldId},
    model::*,
    travel::{Speed, Status},
    units::{Position, Quantity, Seconds, Tile},
};

#[test]
fn exhausted_recovery_generation_preserves_blocked_intent_atomically() {
    let (town, id) = fixture();
    let mut state = town.actor(&id).unwrap().state().clone();
    state.route_generation = RouteGeneration::from_saved(u64::MAX - 1);
    let mut town =
        Town::restore(town.owner().clone(), town.buildings().to_vec(), vec![state]).unwrap();
    ready(&mut town, &id, vec![p(2.0, 0.0)]);
    town.block_route(&id).unwrap();
    let before = town.actor(&id).unwrap().clone();
    assert_eq!(
        town.resume_travel(&id, NavigationRevision(2)),
        Err(ModelError::GenerationExhausted)
    );
    assert_eq!(town.actor(&id).unwrap(), &before);
}

#[test]
fn refreshed_pending_failure_waits_for_a_change_after_the_latest_query_revision() {
    let (mut town, id) = fixture();
    let r = request(&mut town, &id);
    assert_eq!(
        town.resume_travel(&id, NavigationRevision(2)).unwrap(),
        Some(r.clone())
    );
    // R10 refreshes the current query while preserving the request token.
    town.accept_route(&id, &r, r.navigation(), RouteOutcome::Unreachable)
        .unwrap();
    assert_eq!(
        town.resume_travel(&id, NavigationRevision(2)).unwrap(),
        None
    );
    assert!(
        town.resume_travel(&id, NavigationRevision(3))
            .unwrap()
            .is_some()
    );
}

fn p(x: f64, y: f64) -> Position {
    Position::new(x, y).unwrap()
}

#[test]
fn relocation_after_reload_retargets_active_leg_but_does_not_restart_completed_leg() {
    let (mut town, id) = fixture();
    let building = town.buildings()[0].id.clone();
    let r = town
        .request_route(
            &id,
            Destination::Building(building.clone()),
            NavigationRevision(1),
        )
        .unwrap();
    let state = town.actor(&id).unwrap().state().clone();
    let mut restored =
        Town::restore(town.owner().clone(), town.buildings().to_vec(), vec![state]).unwrap();
    restored
        .relocate_building(&building, Tile { x: 5, y: 5 })
        .unwrap();
    let new = restored
        .resume_travel(&id, NavigationRevision(2))
        .unwrap()
        .unwrap();
    assert_eq!(new.goal(), p(5.0, 5.0));
    assert!(
        restored
            .accept_route(&id, &r, r.navigation(), RouteOutcome::Arrived)
            .is_err()
    );
    restored
        .accept_route(&id, &new, new.navigation(), RouteOutcome::Arrived)
        .unwrap();
    let state = restored.actor(&id).unwrap().state().clone();
    let mut completed = Town::restore(
        restored.owner().clone(),
        restored.buildings().to_vec(),
        vec![state],
    )
    .unwrap();
    completed
        .relocate_building(&building, Tile { x: 6, y: 6 })
        .unwrap();
    assert_eq!(
        completed.resume_travel(&id, NavigationRevision(3)).unwrap(),
        None
    );
    assert_eq!(
        completed.actor(&id).unwrap().travel_status(),
        Status::Arrived
    );
    assert!(!walk(&mut completed, &id, 1.0).arrived_now);
}

#[test]
fn confirmed_playback_keeps_corners_timing_facing_and_stationary_arrival_tail() {
    use lantern_core::travel::Facing;
    let (mut town, id) = fixture();
    ready(&mut town, &id, vec![p(1.0, 0.0), p(1.0, 1.0), p(2.0, 1.0)]);
    walk(&mut town, &id, 3.0);
    let actor = town.actor(&id).unwrap();
    let playback = actor.confirmed_playback();
    assert_eq!(
        playback
            .samples()
            .iter()
            .map(|s| s.time.get())
            .collect::<Vec<_>>(),
        vec![0.0, 0.75, 1.75, 2.75, 3.0]
    );
    let middle = playback.pose_at(seconds(1.25));
    assert_eq!(middle.position, p(1.0, 0.5));
    assert_eq!(middle.facing, Facing::Left);
    assert_eq!(middle.status, Status::Walking);
    assert_eq!(playback.pose_at(seconds(2.9)).status, Status::Arrived);
    assert_eq!(playback.pose_at(seconds(999.0)), actor.pose());
    assert_eq!(actor.state().facing, Facing::Right);
    assert_eq!(playback.duration(), seconds(3.0));
}

#[test]
fn blocked_playback_confirms_completed_steps_then_holds_and_invalidations_drop_old_samples() {
    let (mut town, id) = fixture();
    ready(&mut town, &id, vec![p(1.0, 0.0), p(2.0, 0.0)]);
    town.advance_travel(&id, seconds(3.0), 10, |_, target| {
        if target.x.get() > 1.0 {
            None
        } else {
            Speed::new(1.0)
        }
    })
    .unwrap();
    let playback = town.actor(&id).unwrap().confirmed_playback();
    assert_eq!(playback.pose_at(seconds(0.5)).position, p(0.75, 0.0));
    assert_eq!(playback.pose_at(seconds(1.0)).status, Status::Blocked);
    assert_eq!(playback.pose_at(seconds(3.0)).position, p(1.0, 0.0));
    town.set_task(&id, Task::Idle).unwrap();
    let actor = town.actor(&id).unwrap();
    assert_eq!(actor.confirmed_playback().samples().len(), 1);
    assert_eq!(actor.confirmed_playback().samples()[0].pose, actor.pose());
}

#[test]
fn facing_survives_diagonal_motion_wait_reassignment_and_restore() {
    use lantern_core::travel::Facing;
    let (mut town, id) = fixture();
    ready(&mut town, &id, vec![p(-1.0, 0.0)]);
    walk(&mut town, &id, 0.25);
    assert_eq!(town.actor(&id).unwrap().state().facing, Facing::Left);
    ready(&mut town, &id, vec![p(1.0, 1.0)]);
    walk(&mut town, &id, 0.25); // dx == dy retains the previous facing.
    assert_eq!(town.actor(&id).unwrap().state().facing, Facing::Left);
    town.set_task(&id, Task::Idle).unwrap();
    walk(&mut town, &id, 100.0);
    let facts = town.actor(&id).unwrap().state().clone();
    let restored = Town::restore(
        town.owner().clone(),
        town.buildings().to_vec(),
        vec![facts.clone()],
    )
    .unwrap();
    assert_eq!(restored.actor(&id).unwrap().state(), &facts);
    assert_eq!(restored.actor(&id).unwrap().state().facing, Facing::Left);
}

#[test]
fn reload_rebuilds_active_leg_but_does_not_replay_completed_or_cancelled_arrival() {
    let (mut town, id) = fixture();
    ready(&mut town, &id, vec![p(2.0, 0.0)]);
    walk(&mut town, &id, 0.125);
    let facts = town.actor(&id).unwrap().state().clone();
    let mut restored = Town::restore(
        town.owner().clone(),
        town.buildings().to_vec(),
        vec![facts.clone()],
    )
    .unwrap();
    assert_eq!(
        restored.actor(&id).unwrap().travel_status(),
        Status::Waiting
    );
    assert_eq!(
        restored
            .actor(&id)
            .unwrap()
            .confirmed_playback()
            .samples()
            .len(),
        1
    );
    let r = restored
        .resume_travel(&id, NavigationRevision(4))
        .unwrap()
        .unwrap();
    assert_eq!(r.origin(), facts.position);
    assert_eq!(Some(r.destination()), facts.travel.destination());
    restored
        .accept_route(
            &id,
            &r,
            r.navigation(),
            RouteOutcome::Ready(vec![p(2.0, 0.0)]),
        )
        .unwrap();
    assert!(walk(&mut restored, &id, 5.0).arrived_now);
    for cancelled in [false, true] {
        if cancelled {
            restored.cancel_route(&id).unwrap();
        }
        let facts = restored.actor(&id).unwrap().state().clone();
        let mut reloaded = Town::restore(
            restored.owner().clone(),
            restored.buildings().to_vec(),
            vec![facts.clone()],
        )
        .unwrap();
        assert_eq!(
            reloaded.actor(&id).unwrap().travel_status(),
            if cancelled {
                Status::Cancelled
            } else {
                Status::Arrived
            }
        );
        assert_eq!(
            reloaded.resume_travel(&id, NavigationRevision(5)).unwrap(),
            None
        );
        assert!(!walk(&mut reloaded, &id, 5.0).arrived_now);
        assert_eq!(reloaded.actor(&id).unwrap().state(), &facts);
    }
}

#[test]
fn missed_observations_rejoin_at_walking_speed_without_mutating_authority() {
    use lantern_core::travel::rejoin;
    let (mut town, id) = fixture();
    ready(&mut town, &id, vec![p(2.0, 0.0)]);
    let visible = town.actor(&id).unwrap().pose();
    walk(&mut town, &id, 10.0);
    let actor = town.actor(&id).unwrap().clone();
    let confirmed = actor.confirmed_playback().pose_at(seconds(1000.0));
    let mut view = visible;
    for _ in 0..40 {
        let next = rejoin(view, confirmed, seconds(0.05), Speed::new(1.0).unwrap()).unwrap();
        assert!((next.position.x.get() - view.position.x.get()).abs() <= 0.05 + 1e-12);
        view = next;
    }
    assert_eq!(view, confirmed);
    assert_eq!(town.actor(&id).unwrap(), &actor);
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

#[test]
fn task_replacement_clears_old_leg_without_discarding_fractional_pose_or_cargo() {
    let (mut town, id) = fixture();
    let old = request(&mut town, &id);
    town.accept_route(
        &id,
        &old,
        old.navigation(),
        RouteOutcome::Ready(vec![p(2.0, 0.0)]),
    )
    .unwrap();
    walk(&mut town, &id, 0.125);
    let facts = town.actor(&id).unwrap().state().clone();
    town.set_task(
        &id,
        Task::Move {
            destination: p(0.0, 2.0),
        },
    )
    .unwrap();
    assert_eq!(town.actor(&id).unwrap().state().position, facts.position);
    assert_eq!(town.actor(&id).unwrap().state().cargo, facts.cargo);
    assert_eq!(town.actor(&id).unwrap().travel_destination(), None);
    assert_eq!(
        town.resume_travel(&id, NavigationRevision(2)).unwrap(),
        None
    );
    assert_eq!(
        town.accept_route(&id, &old, old.navigation(), RouteOutcome::Arrived),
        Err(ModelError::StaleRoute)
    );
    let new = town
        .request_route(
            &id,
            Destination::Position(p(0.0, 2.0)),
            NavigationRevision(2),
        )
        .unwrap();
    assert_eq!(new.origin(), facts.position);
    assert_ne!(new.task_generation(), old.task_generation());
    town.set_task(&id, Task::Idle).unwrap();
    assert_eq!(
        town.resume_travel(&id, NavigationRevision(3)).unwrap(),
        None
    );
    assert_eq!(town.actor(&id).unwrap().state().cargo, facts.cargo);
}

#[test]
fn relocation_keeps_outbound_source_but_retargets_return_from_fractional_position() {
    let (mut town, id) = fixture();
    let workplace = town.buildings()[0].id.clone();
    let target = Destination::Position(p(8.0, 0.0));
    town.set_task(
        &id,
        Task::Work {
            workplace: workplace.clone(),
            target: target.clone(),
            phase: WorkPhase::Outbound,
            elapsed: seconds(0.375),
        },
    )
    .unwrap();
    let outbound = town
        .request_route(&id, target.clone(), NavigationRevision(1))
        .unwrap();
    town.accept_route(
        &id,
        &outbound,
        outbound.navigation(),
        RouteOutcome::Ready(vec![p(8.0, 0.0)]),
    )
    .unwrap();
    walk(&mut town, &id, 0.125);
    let actor = town.actor(&id).unwrap().clone();
    town.relocate_building(&workplace, Tile { x: 3, y: 1 })
        .unwrap();
    assert_eq!(town.actor(&id).unwrap(), &actor);
    assert_eq!(
        town.resume_travel(&id, NavigationRevision(2)).unwrap(),
        None
    );
    town.set_task(
        &id,
        Task::Work {
            workplace: workplace.clone(),
            target,
            phase: WorkPhase::Returning,
            elapsed: seconds(0.375),
        },
    )
    .unwrap();
    let old = town
        .request_route(
            &id,
            Destination::Building(workplace.clone()),
            NavigationRevision(2),
        )
        .unwrap();
    town.accept_route(
        &id,
        &old,
        old.navigation(),
        RouteOutcome::Ready(vec![p(3.0, 1.0)]),
    )
    .unwrap();
    walk(&mut town, &id, 0.125);
    let facts = town.actor(&id).unwrap().state().clone();
    town.relocate_building(&workplace, Tile { x: 4, y: 1 })
        .unwrap();
    assert_eq!(town.actor(&id).unwrap().state(), &facts);
    assert_eq!(
        town.actor(&id).unwrap().travel_destination(),
        Some(&Destination::Building(workplace.clone()))
    );
    let retry = town
        .resume_travel(&id, NavigationRevision(3))
        .unwrap()
        .unwrap();
    assert_eq!(retry.origin(), facts.position);
    assert_eq!(retry.goal(), p(4.0, 1.0));
    assert_eq!(retry.destination(), &Destination::Building(workplace));
    assert_eq!(
        town.resume_travel(&id, NavigationRevision(4)).unwrap(),
        Some(retry.clone())
    );
    assert_eq!(
        town.accept_route(&id, &old, old.navigation(), RouteOutcome::Arrived),
        Err(ModelError::StaleRoute)
    );
    town.accept_route(&id, &retry, retry.navigation(), RouteOutcome::Arrived)
        .unwrap();
    assert_eq!(
        town.resume_travel(&id, NavigationRevision(5)).unwrap(),
        None
    );
    assert_eq!(town.actor(&id).unwrap().state().position, facts.position);
    assert_facts(&facts, town.actor(&id).unwrap().state());
}

#[test]
fn moved_source_rejects_pending_answer_and_resolves_new_goal_on_same_leg() {
    let (mut town, id) = fixture();
    let source = town.buildings()[0].id.clone();
    let old = town
        .request_route(
            &id,
            Destination::Building(source.clone()),
            NavigationRevision(1),
        )
        .unwrap();
    let before = town.actor(&id).unwrap().state().clone();
    town.relocate_building(&source, Tile { x: 5, y: 2 })
        .unwrap();
    let retry = town
        .resume_travel(&id, NavigationRevision(1))
        .unwrap()
        .unwrap();
    assert_eq!(retry.goal(), p(5.0, 2.0));
    assert_ne!(old.route_generation(), retry.route_generation());
    assert_eq!(
        town.accept_route(&id, &old, old.navigation(), RouteOutcome::Arrived),
        Err(ModelError::StaleRoute)
    );
    assert_facts(&before, town.actor(&id).unwrap().state());
}

#[test]
fn blocked_leg_retries_once_and_unchanged_unreachable_does_not_churn() {
    let (mut town, id) = fixture();
    ready(&mut town, &id, vec![p(2.0, 0.0)]);
    walk(&mut town, &id, 0.125);
    let before = town.actor(&id).unwrap().state().clone();
    town.advance_travel(&id, seconds(1.0), 10, |_, _| None)
        .unwrap();
    let retry = town
        .resume_travel(&id, NavigationRevision(1))
        .unwrap()
        .unwrap();
    assert_eq!(retry.origin(), before.position);
    for _ in 0..10 {
        assert_eq!(
            town.resume_travel(&id, NavigationRevision(1)).unwrap(),
            Some(retry.clone())
        );
    }
    town.accept_route(&id, &retry, retry.navigation(), RouteOutcome::Unreachable)
        .unwrap();
    for _ in 0..10 {
        assert_eq!(
            town.resume_travel(&id, NavigationRevision(1)).unwrap(),
            None
        );
    }
    assert_eq!(
        town.actor(&id).unwrap().travel_status(),
        Status::Unreachable
    );
    let reopened = town
        .resume_travel(&id, NavigationRevision(2))
        .unwrap()
        .unwrap();
    assert_eq!(reopened.destination(), retry.destination());
    assert_eq!(reopened.origin(), before.position);
    town.accept_route(
        &id,
        &reopened,
        reopened.navigation(),
        RouteOutcome::Ready(vec![p(2.0, 0.0)]),
    )
    .unwrap();
    assert!(walk(&mut town, &id, 10.0).arrived_now);
    assert_eq!(
        town.resume_travel(&id, NavigationRevision(3)).unwrap(),
        None
    );
    assert!(!walk(&mut town, &id, 10.0).arrived_now);
    assert_facts(&before, town.actor(&id).unwrap().state());
}

#[test]
fn recovery_repositioning_keeps_intent_but_cancellation_is_explicit() {
    let (mut town, id) = fixture();
    ready(&mut town, &id, vec![p(2.0, 0.0)]);
    let destination = town.actor(&id).unwrap().travel_destination().cloned();
    town.reposition_actor(&id, p(0.5, 1.25)).unwrap();
    let retry = town
        .resume_travel(&id, NavigationRevision(2))
        .unwrap()
        .unwrap();
    assert_eq!(retry.origin(), p(0.5, 1.25));
    assert_eq!(Some(retry.destination()), destination.as_ref());
    town.cancel_route(&id).unwrap();
    assert_eq!(
        town.resume_travel(&id, NavigationRevision(3)).unwrap(),
        None
    );
    assert_eq!(town.actor(&id).unwrap().travel_status(), Status::Cancelled);
    assert_eq!(town.actor(&id).unwrap().travel_destination(), None);
    town.reposition_actor(&id, p(0.75, 1.25)).unwrap();
    assert_eq!(
        town.resume_travel(&id, NavigationRevision(4)).unwrap(),
        None
    );
}
