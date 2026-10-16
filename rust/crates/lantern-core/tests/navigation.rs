use lantern_core::{ids::*, model::*, navigation::*, search::*, spatial::*, units::*};
use std::rc::Rc;
fn p(x: f64, y: f64) -> Position {
    Position::new(x, y).unwrap()
}
fn fixture(n: usize) -> (Town, Vec<ActorId>) {
    let owner = Owner {
        world: WorldId::new("world").unwrap(),
        town: TownId::new("town").unwrap(),
    };
    let mut ids = IdAllocator::new(owner.world.clone());
    let actors: Vec<_> = (0..n)
        .map(|_| {
            ActorState::new(
                ids.allocate(&owner).unwrap(),
                p(0., 0.),
                Cargo::from_entries([("wood".into(), Quantity::new(1.25).unwrap())]).unwrap(),
            )
        })
        .collect();
    let keys = actors.iter().map(|a| a.id.clone()).collect();
    let mut town = Town::restore(owner, vec![], actors).unwrap();
    for id in &keys {
        town.set_task(
            id,
            Task::Move {
                destination: p(63., 63.),
            },
        )
        .unwrap();
    }
    (town, keys)
}
fn request(town: &mut Town, id: &ActorId) -> RouteRequest {
    town.request_route(
        id,
        Destination::Position(p(63., 63.)),
        NavigationRevision(1),
    )
    .unwrap()
}
fn query(size: u32) -> Rc<Query> {
    Rc::new(
        Query::new(
            Rc::new(SpatialIndex::derive(&[], &[], &[], &[], size).unwrap()),
            Terrain::default(),
            Access::default(),
            Tile { x: 0, y: 0 },
            Target::Exact(Tile { x: 63, y: 63 }),
            false,
            RoadMode::Smooth,
        )
        .unwrap(),
    )
}
fn submit(s: &mut Service, t: &Town, r: &RouteRequest, q: Rc<Query>, priority: Priority) -> Status {
    s.submit(t, r, NavigationRevision(1), q, priority)
}
fn advance(s: &mut Service, t: &Town, n: usize) -> usize {
    s.advance(n, |r| t.route_is_current(r, NavigationRevision(1)))
}
#[test]
fn stale_replacement_and_late_cancellation_preserve_actor_facts() {
    let (mut t, ids) = fixture(1);
    let id = &ids[0];
    let mut s = Service::new(Limits::default());
    let q = query(64);
    let old = request(&mut t, id);
    let state = t.actor(id).unwrap().state().clone();
    assert_eq!(
        submit(&mut s, &t, &old, q.clone(), Priority::Ordinary),
        Status::Searching
    );
    assert_eq!(advance(&mut s, &t, 9), 9);
    let new = request(&mut t, id);
    assert_eq!(
        submit(&mut s, &t, &new, q.clone(), Priority::Blocked),
        Status::Searching
    );
    s.cancel(&old);
    assert_eq!(s.resident_jobs(), 1);
    assert_eq!(
        submit(&mut s, &t, &old, q, Priority::Ordinary),
        Status::Stale
    );
    t.cancel_route(id).unwrap();
    assert_eq!(advance(&mut s, &t, 9), 0);
    assert_eq!(s.resident_jobs(), 0);
    let now = t.actor(id).unwrap().state();
    assert_eq!(now.position, state.position);
    assert_eq!(now.cargo, state.cargo);
    assert_eq!(now.task, state.task);
}
#[test]
fn terminal_result_is_guarded_and_holds_capacity_until_taken() {
    let (mut t, ids) = fixture(2);
    let r = request(&mut t, &ids[0]);
    let other = request(&mut t, &ids[1]);
    let mut s = Service::new(Limits {
        jobs: 1,
        ..Limits::default()
    });
    let q = query(2); // finite exhausted search, not a capacity failure
    submit(&mut s, &t, &r, q.clone(), Priority::Ordinary);
    advance(&mut s, &t, 100);
    assert_eq!(s.status(&r), Some(Status::Ready));
    assert_eq!(
        submit(&mut s, &t, &other, q, Priority::Blocked),
        Status::Waiting(Waiting::Capacity)
    );
    let done = s.take(&r).unwrap();
    assert_eq!(done.outcome(), &Outcome::Unreachable);
    let new = request(&mut t, &ids[0]);
    let facts = t.actor(&ids[0]).unwrap().state().clone();
    assert_eq!(
        done.apply(&mut t, NavigationRevision(1)),
        Err(ModelError::StaleRoute)
    );
    assert_eq!(t.actor(&ids[0]).unwrap().state(), &facts);
    assert!(t.route_is_current(&new, NavigationRevision(1)));
}
#[test]
fn changed_navigation_rejects_completion_and_prunes_reservation() {
    let (mut t, ids) = fixture(2);
    let r = request(&mut t, &ids[0]);
    let other = request(&mut t, &ids[1]);
    let mut s = Service::new(Limits {
        jobs: 1,
        ..Limits::default()
    });
    submit(&mut s, &t, &r, query(2), Priority::Ordinary);
    advance(&mut s, &t, 100);
    submit(&mut s, &t, &other, query(2), Priority::Ordinary);
    let done = s.take(&r).unwrap();
    assert_eq!(
        done.apply(&mut t, NavigationRevision(2)),
        Err(ModelError::StaleRoute)
    );
    assert_eq!(
        s.advance(1, |r| t.route_is_current(r, NavigationRevision(2))),
        0
    );
    assert!(!s.has_reservation());
}
#[test]
fn hard_allowance_and_resource_wait_release_query_without_false_failure() {
    let (mut t, ids) = fixture(1);
    let r = request(&mut t, &ids[0]);
    let q = query(64);
    let mut s = Service::new(Limits {
        work_per_job: 9,
        quantum: 8,
        ..Limits::default()
    });
    submit(&mut s, &t, &r, q.clone(), Priority::Ordinary);
    assert_eq!(advance(&mut s, &t, 0), 0);
    assert_eq!(advance(&mut s, &t, 3), 3);
    assert_eq!(advance(&mut s, &t, 100), 6);
    assert_eq!(s.status(&r), Some(Status::Waiting(Waiting::ResourceLimit)));
    assert_eq!(Rc::strong_count(&q), 1);
    assert!(s.take(&r).is_none());
    assert!(t.route_is_current(&r, NavigationRevision(1)));
    s.cancel(&r);
    assert_eq!(s.resident_jobs(), 0);
}
#[test]
fn saturation_reserves_retry_against_new_and_replacing_streams() {
    let (mut t, ids) = fixture(4);
    let rs: Vec<_> = ids.iter().map(|id| request(&mut t, id)).collect();
    let mut s = Service::new(Limits {
        jobs: 1,
        ..Limits::default()
    });
    submit(&mut s, &t, &rs[0], query(2), Priority::Blocked);
    assert_eq!(
        submit(&mut s, &t, &rs[1], query(2), Priority::Ordinary),
        Status::Waiting(Waiting::Capacity)
    );
    advance(&mut s, &t, 100);
    s.take(&rs[0]).unwrap();
    for r in [&rs[2], &rs[3], &rs[0]] {
        assert_eq!(
            submit(&mut s, &t, r, query(2), Priority::Blocked),
            Status::Waiting(Waiting::Reserved)
        );
    }
    assert_eq!(
        submit(&mut s, &t, &rs[1], query(2), Priority::Ordinary),
        Status::Searching
    );
    // Cancelling the reservation lets the next caller reserve; no overflow exists.
    submit(&mut s, &t, &rs[2], query(2), Priority::Blocked);
    s.cancel(&rs[2]);
    submit(&mut s, &t, &rs[3], query(2), Priority::Cargo);
    assert!(s.has_reservation());
}
#[test]
fn all_lanes_and_fifo_receive_service_under_continuous_priority_load() {
    let (mut t, ids) = fixture(6);
    let rs: Vec<_> = ids.iter().map(|id| request(&mut t, id)).collect();
    let queries: Vec<_> = (0..6).map(|_| query(64)).collect();
    let mut s = Service::new(Limits {
        quantum: 1,
        work_per_job: 4,
        ..Limits::default()
    });
    for (i, r) in rs.iter().enumerate() {
        submit(
            &mut s,
            &t,
            r,
            queries[i].clone(),
            [Priority::Blocked, Priority::Cargo, Priority::Ordinary][i / 2],
        );
    }
    // Each seven-operation round gives 4/2/1 operations. Two ordinary jobs each
    // exhaust four operations within eight rounds, despite saturated other lanes.
    for _ in 0..8 {
        assert!(advance(&mut s, &t, 7) <= 7);
    }
    for (i, r) in rs.iter().enumerate() {
        assert_eq!(s.status(r), Some(Status::Waiting(Waiting::ResourceLimit)));
        assert_eq!(Rc::strong_count(&queries[i]), 1);
    }
}
#[test]
fn retries_do_not_reset_work_and_input_limits_retain_nothing() {
    let (mut t, ids) = fixture(1);
    let r = request(&mut t, &ids[0]);
    let q = query(64);
    let mut s = Service::new(Limits {
        work_per_job: 5,
        ..Limits::default()
    });
    submit(&mut s, &t, &r, q.clone(), Priority::Ordinary);
    advance(&mut s, &t, 3);
    submit(&mut s, &t, &r, q.clone(), Priority::Blocked);
    assert_eq!(advance(&mut s, &t, 9), 2);
    let mut tiny = Service::new(Limits {
        identity_bytes: 1,
        ..Limits::default()
    });
    assert_eq!(
        submit(&mut tiny, &t, &r, q, Priority::Ordinary),
        Status::Waiting(Waiting::IdentityLimit)
    );
    assert_eq!(tiny.resident_jobs(), 0);
    let q = Rc::new(
        Query::new(
            Rc::new(SpatialIndex::derive(&[], &[], &[], &[], 64).unwrap()),
            Terrain::default(),
            Access {
                exemptions: [(Tile { x: 1, y: 1 })].into(),
                ..Access::default()
            },
            Tile { x: 0, y: 0 },
            Target::Exact(Tile { x: 63, y: 63 }),
            false,
            RoadMode::Smooth,
        )
        .unwrap(),
    );
    let mut tiny = Service::new(Limits {
        query_records: 1,
        ..Limits::default()
    });
    assert_eq!(
        submit(&mut tiny, &t, &r, q.clone(), Priority::Ordinary),
        Status::Waiting(Waiting::QueryLimit)
    );
    assert_eq!(Rc::strong_count(&q), 1);
    assert!(!tiny.has_reservation());
}
#[test]
fn exact_weighted_fifo_and_late_streams_cannot_overtake_ordinary() {
    let (mut t, ids) = fixture(12);
    let rs: Vec<_> = ids.iter().map(|id| request(&mut t, id)).collect();
    let mut s = Service::new(Limits {
        quantum: 1,
        ..Limits::default()
    });
    for (i, r) in rs[..6].iter().enumerate() {
        submit(
            &mut s,
            &t,
            r,
            query(64),
            [Priority::Blocked, Priority::Cargo, Priority::Ordinary][i / 2],
        );
    }
    assert_eq!(advance(&mut s, &t, 7), 7);
    assert_eq!(
        rs[..6]
            .iter()
            .map(|r| s.work(r).unwrap())
            .collect::<Vec<_>>(),
        [2, 2, 1, 1, 1, 0]
    );
    // Sustained replacements plus fresh blocked/cargo work cannot restart the
    // ordinary lane clock or overtake either existing ordinary request.
    for round in 0..30 {
        let i = round % 4;
        let replacement = request(&mut t, &ids[i]);
        submit(
            &mut s,
            &t,
            &replacement,
            query(64),
            if i < 2 {
                Priority::Blocked
            } else {
                Priority::Cargo
            },
        );
        let extra = 6 + round % 6;
        let replacement = request(&mut t, &ids[extra]);
        submit(&mut s, &t, &replacement, query(64), Priority::Blocked);
        advance(&mut s, &t, 7);
    }
    assert_eq!(s.work(&rs[4]), Some(16));
    assert_eq!(s.work(&rs[5]), Some(15));
}
#[test]
fn finite_256_request_retry_stream_completes_without_overflow() {
    let (mut t, ids) = fixture(256);
    let rs: Vec<_> = ids.iter().map(|id| request(&mut t, id)).collect();
    let mut done = vec![false; 256];
    let mut s = Service::new(Limits::default());
    for _ in 0..20 {
        for (i, r) in rs.iter().enumerate().filter(|(i, _)| !done[*i]) {
            submit(
                &mut s,
                &t,
                r,
                query(2),
                [Priority::Blocked, Priority::Cargo, Priority::Ordinary][i % 3],
            );
        }
        advance(&mut s, &t, 2048);
        for (i, r) in rs.iter().enumerate() {
            if let Some(result) = s.take(r) {
                assert_eq!(result.outcome(), &Outcome::Unreachable);
                done[i] = true;
            }
        }
        assert!(s.resident_jobs() <= 128);
        if done.iter().all(|v| *v) {
            break;
        }
    }
    assert!(done.iter().all(|v| *v));
    assert_eq!(s.resident_jobs(), 0);
    assert!(!s.has_reservation());
}
#[test]
fn pending_priority_promotes_without_reset_or_demotion() {
    let (mut t, ids) = fixture(3);
    let rs: Vec<_> = ids.iter().map(|id| request(&mut t, id)).collect();
    let q = query(64);
    let mut s = Service::new(Limits {
        quantum: 1,
        ..Limits::default()
    });
    for r in &rs {
        submit(&mut s, &t, r, q.clone(), Priority::Ordinary);
    }
    advance(&mut s, &t, 7);
    let before = s.work(&rs[0]).unwrap();
    submit(&mut s, &t, &rs[0], q.clone(), Priority::Blocked);
    submit(&mut s, &t, &rs[0], q, Priority::Ordinary);
    advance(&mut s, &t, 7);
    assert_eq!(s.work(&rs[0]), Some(before + 6));
}
#[test]
fn abandoned_current_reservation_expires_without_an_overflow_registry() {
    let (mut t, ids) = fixture(3);
    let rs: Vec<_> = ids.iter().map(|id| request(&mut t, id)).collect();
    let mut s = Service::new(Limits {
        jobs: 1,
        ..Limits::default()
    });
    submit(&mut s, &t, &rs[0], query(2), Priority::Ordinary);
    submit(&mut s, &t, &rs[1], query(2), Priority::Ordinary);
    advance(&mut s, &t, 100);
    s.take(&rs[0]).unwrap();
    assert_eq!(
        submit(&mut s, &t, &rs[2], query(2), Priority::Blocked),
        Status::Waiting(Waiting::Reserved)
    );
    // The second request is still model-current but stopped retrying.
    advance(&mut s, &t, 0);
    assert!(!s.has_reservation());
    assert_eq!(
        submit(&mut s, &t, &rs[2], query(2), Priority::Blocked),
        Status::Searching
    );
}
