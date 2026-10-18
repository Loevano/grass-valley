use lantern_core::{
    ids::{ActorId, BuildingId, EntityIdentity, Owner, TownId, WorldId},
    model::*,
    units::*,
};
use lantern_host::{gathering::*, progression::Worker, resources::*};
use std::collections::BTreeMap;
fn owner() -> Owner {
    Owner {
        world: WorldId::new("forestry").unwrap(),
        town: TownId::new("one").unwrap(),
    }
}
fn a(n: u64) -> ActorId {
    ActorId::from_saved(EntityIdentity::from_saved(owner(), 100 + n).unwrap())
}
fn b(n: u64) -> BuildingId {
    BuildingId::from_saved(EntityIdentity::from_saved(owner(), n).unwrap())
}
fn p(x: f64, y: f64) -> Position {
    Position::new(x, y).unwrap()
}
fn sec(n: f64) -> Seconds {
    Seconds::new(n).unwrap()
}
fn q(n: f64) -> Quantity {
    Quantity::new(n).unwrap()
}
fn wood() -> ResourceKey {
    ResourceKey::parse("wood").unwrap()
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}
struct Fixture {
    town: Town,
    ledger: Ledger,
    trees: Trees,
    cutters: Woodcutters,
    worker: Worker,
}
impl Fixture {
    fn new() -> Self {
        let town = Town::restore(
            owner(),
            vec![
                Building {
                    id: b(1),
                    location: Tile { x: 5, y: 3 },
                    construction: ConstructionState::Complete,
                },
                Building {
                    id: b(2),
                    location: Tile { x: 6, y: 3 },
                    construction: ConstructionState::Complete,
                },
            ],
            (1..=3)
                .map(|i| ActorState::new(a(i), p(5., 1.), Cargo::default()))
                .collect(),
        )
        .unwrap();
        Self {
            town,
            ledger: Ledger::new(
                owner(),
                q(0.),
                vec![
                    Store::new(b(1), StoragePolicy::new("woodcutter")),
                    Store::new(b(2), StoragePolicy::new("woodcutter")),
                ],
                vec![],
            )
            .unwrap(),
            trees: Trees::new(vec![
                Tree {
                    tile: Tile { x: 5, y: 2 },
                    supply: q(8.),
                },
                Tree {
                    tile: Tile { x: 7, y: 2 },
                    supply: q(8.),
                },
            ])
            .unwrap(),
            cutters: Woodcutters::default(),
            worker: Worker::new("woodcutting").unwrap(),
        }
    }
    fn assign(&mut self, i: u64, camp: u64) -> Option<Tile> {
        self.cutters
            .assign(
                &mut self.town,
                &mut self.ledger,
                &self.trees,
                &a(i),
                &b(camp),
                &self.worker,
                NavigationRevision(1),
                |_| Access::Reachable,
            )
            .unwrap()
    }
    fn ready(&mut self, i: u64, outcome: RouteOutcome) {
        self.town
            .resume_travel(&a(i), NavigationRevision(1))
            .unwrap();
        let RouteState::Pending(r) = self.town.actor(&a(i)).unwrap().route().clone() else {
            panic!("pending route required");
        };
        self.town
            .accept_route(&a(i), &r, r.navigation(), outcome)
            .unwrap();
    }
    fn advance(&mut self, i: u64, n: f64) -> Outcome {
        self.cutters
            .advance(
                &mut self.town,
                &mut self.ledger,
                &mut self.trees,
                &a(i),
                &mut self.worker,
                sec(n),
                NavigationRevision(1),
                |_, _, s| Some(s),
            )
            .unwrap()
    }
    fn set_phase(&mut self, phase: WorkPhase, n: f64) {
        self.town
            .set_task(
                &a(1),
                Task::Work {
                    workplace: b(1),
                    target: Destination::Position(p(5., 2.)),
                    phase,
                    elapsed: sec(n),
                },
            )
            .unwrap();
    }
    fn phase(&self, i: u64) -> (WorkPhase, f64) {
        let Task::Work { phase, elapsed, .. } = self.town.actor(&a(i)).unwrap().state().task else {
            panic!("work");
        };
        (phase, elapsed.get())
    }
    fn work(&mut self, n: f64) -> Outcome {
        self.assign(1, 1);
        self.ready(1, RouteOutcome::Arrived);
        self.advance(1, n)
    }
    fn total(&self) -> f64 {
        self.ledger.overview().unwrap()["wood"].total
    }
}
#[test]
fn python_rates_tools_skills_talents_supply_caps_and_practice() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/gathering/python.json")).unwrap();
    for c in data["rates"].as_array().unwrap() {
        let mut f = Fixture::new();
        f.worker
            .set_practice(
                "woodcutting",
                Practice::new(c["skill"].as_f64().unwrap()).unwrap(),
            )
            .unwrap();
        f.worker
            .set_talents(BTreeMap::from([(
                "woodcutting".into(),
                c["talent"].as_f64().unwrap(),
            )]))
            .unwrap();
        f.worker.tool = c["tool"].as_str().map(str::to_owned);
        f.trees = Trees::new(vec![Tree {
            tile: Tile { x: 5, y: 2 },
            supply: q(c["supply"].as_f64().unwrap()),
        }])
        .unwrap();
        f.ledger
            .configure_cap(wood(), Some(q(c["room"].as_f64().unwrap())))
            .unwrap();
        let Outcome::Advanced { produced, .. } = f.work(c["elapsed"].as_f64().unwrap()) else {
            panic!("work");
        };
        close(produced.get(), c["amount"].as_f64().unwrap());
        close(
            f.worker.practice_value("woodcutting"),
            c["practice"].as_f64().unwrap(),
        );
        close(f.total(), produced.get());
        close(f.ledger.produced().quantity(&wood()).get(), produced.get());
    }
}
#[test]
fn python_work_and_unloading_clocks() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/gathering/python.json")).unwrap();
    for c in data["clocks"].as_array().unwrap().iter().take(4) {
        let mut f = Fixture::new();
        f.assign(1, 1);
        let phase = if c["phase"] == "working" {
            WorkPhase::Working
        } else {
            WorkPhase::Depositing
        };
        f.set_phase(phase, c["clock"].as_f64().unwrap());
        let result = f.advance(1, c["elapsed"].as_f64().unwrap());
        if c["finished"] == true {
            let Outcome::Unloaded(remaining) = result else {
                panic!("unloaded");
            };
            close(remaining.get(), c["remaining"].as_f64().unwrap());
            assert_eq!(f.phase(1), (WorkPhase::Outbound, 0.));
        } else {
            let expected = match c["result_phase"].as_str().unwrap() {
                "working" => WorkPhase::Working,
                "returning" => WorkPhase::Returning,
                "depositing" => WorkPhase::Depositing,
                _ => panic!("phase"),
            };
            assert_eq!(f.phase(1).0, expected);
            close(f.phase(1).1, c["result_clock"].as_f64().unwrap());
        }
    }
}
#[test]
fn separate_claims_across_camps_and_pending_workers() {
    let mut f = Fixture::new();
    assert_eq!(f.assign(1, 1), Some(Tile { x: 5, y: 2 }));
    assert_eq!(f.assign(2, 2), Some(Tile { x: 7, y: 2 }));
    assert_eq!(f.assign(3, 1), None);
    for i in 1..=3 {
        assert_eq!(f.advance(i, 100.), Outcome::Waiting);
    }
    close(f.total(), 0.);
    close(f.worker.practice_value("woodcutting"), 0.);
    f.cutters.cancel(&mut f.town, &mut f.ledger, &a(1)).unwrap();
    assert_eq!(f.assign(3, 1), Some(Tile { x: 5, y: 2 }));
}
#[test]
fn stable_target_unreachable_replacement_and_reassigned_release() {
    let mut f = Fixture::new();
    f.assign(1, 1);
    f.assign(2, 1);
    f.cutters.cancel(&mut f.town, &mut f.ledger, &a(1)).unwrap();
    assert_eq!(f.assign(2, 1), Some(Tile { x: 7, y: 2 }));
    f.ready(2, RouteOutcome::Unreachable);
    let result = f
        .cutters
        .assign(
            &mut f.town,
            &mut f.ledger,
            &f.trees,
            &a(2),
            &b(1),
            &f.worker,
            NavigationRevision(1),
            |t| {
                if t.x == 7 {
                    Access::Unreachable
                } else {
                    Access::Reachable
                }
            },
        )
        .unwrap();
    assert_eq!(result, Some(Tile { x: 5, y: 2 }));
    f.town
        .set_task(
            &a(2),
            Task::Move {
                destination: p(0., 0.),
            },
        )
        .unwrap();
    assert_eq!(f.assign(1, 1), Some(Tile { x: 5, y: 2 }));
    assert_eq!(f.advance(2, 3.), Outcome::Cancelled);
    assert!(matches!(
        f.town.actor(&a(2)).unwrap().state().task,
        Task::Move { .. }
    ));
}
#[test]
fn travel_partial_arrival_and_blocking_do_not_bank_productive_time() {
    let mut f = Fixture::new();
    f.assign(1, 1);
    f.ready(1, RouteOutcome::Ready(vec![p(5., 0.)]));
    let result = f
        .cutters
        .advance(
            &mut f.town,
            &mut f.ledger,
            &mut f.trees,
            &a(1),
            &mut f.worker,
            sec(10.),
            NavigationRevision(1),
            |_, _, _| None,
        )
        .unwrap();
    assert_eq!(result, Outcome::Waiting);
    close(f.total(), 0.);
    close(f.worker.practice_value("woodcutting"), 0.);
    f.town.resume_travel(&a(1), NavigationRevision(2)).unwrap();
    let RouteState::Pending(r) = f.town.actor(&a(1)).unwrap().route().clone() else {
        panic!("pending");
    };
    f.town
        .accept_route(
            &a(1),
            &r,
            r.navigation(),
            RouteOutcome::Ready(vec![p(5., 0.)]),
        )
        .unwrap();
    let Outcome::Advanced {
        productive,
        produced,
        ..
    } = f.advance(1, 3.)
    else {
        panic!("work");
    };
    close(productive.get(), 3. - 1. / 1.8);
    close(produced.get(), 0.25 * (3. - 1. / 1.8));
    close(f.worker.practice_value("woodcutting"), 0.065);
}
#[test]
fn depletion_returns_before_retargeting_and_unload_preserves_time() {
    let mut f = Fixture::new();
    f.trees = Trees::new(vec![
        Tree {
            tile: Tile { x: 5, y: 2 },
            supply: q(0.1),
        },
        Tree {
            tile: Tile { x: 7, y: 2 },
            supply: q(8.),
        },
    ])
    .unwrap();
    let Outcome::Advanced { depleted, .. } = f.work(3.) else {
        panic!("work");
    };
    assert_eq!(depleted, Some(Tile { x: 5, y: 2 }));
    assert_eq!(f.trees.trees().len(), 1);
    assert_eq!(f.assign(1, 1), Some(Tile { x: 5, y: 2 }));
    assert_eq!(f.advance(1, 100.), Outcome::Waiting);
    close(f.total(), 0.1);
    f.ready(1, RouteOutcome::Arrived);
    assert_eq!(f.advance(1, 3.), Outcome::Unloaded(sec(2.35)));
    close(
        f.ledger
            .store(&b(1))
            .unwrap()
            .output
            .quantity(&wood())
            .get(),
        0.1,
    );
    assert_eq!(f.assign(1, 1), Some(Tile { x: 7, y: 2 }));
}
#[test]
fn removed_and_unreachable_tree_cargo_finishes_without_practice() {
    for removed in [true, false] {
        let mut f = Fixture::new();
        f.work(1.);
        let skill = f.worker.practice_value("woodcutting");
        f.set_phase(WorkPhase::Outbound, 0.);
        f.town
            .request_route(
                &a(1),
                Destination::Position(p(5., 2.)),
                NavigationRevision(1),
            )
            .unwrap();
        if removed {
            f.trees.remove(Tile { x: 5, y: 2 });
        } else {
            f.ready(1, RouteOutcome::Unreachable);
        }
        assert_eq!(f.advance(1, 3.), Outcome::Waiting);
        assert_eq!(f.phase(1).0, WorkPhase::Returning);
        close(f.total(), 0.25);
        close(f.worker.practice_value("woodcutting"), skill);
        f.ready(1, RouteOutcome::Arrived);
        f.advance(1, 3.);
        close(
            f.ledger
                .store(&b(1))
                .unwrap()
                .output
                .quantity(&wood())
                .get(),
            0.25,
        );
    }
}
#[test]
fn cancellation_preserves_finished_goods_once() {
    for kind in [0, 1, 2, 3] {
        let mut f = Fixture::new();
        f.work(1.);
        if kind == 0 {
            f.town
                .relocate_building(&b(1), Tile { x: 6, y: 5 })
                .unwrap();
        }
        if kind == 1 {
            f.town.remove_building(&b(1)).unwrap();
        }
        if kind == 2 {
            f.town.set_task(&a(1), Task::Idle).unwrap();
        }
        if kind == 3 {
            let mut policy = StoragePolicy::new("woodcutter");
            policy.construction = true;
            f.ledger.update_storage(&b(1), policy).unwrap();
        }
        let json = f.cutters.encode(&f.town).unwrap();
        f.cutters = Woodcutters::decode(&json, &f.town, &f.ledger).unwrap();
        f.advance(1, 3.);
        f.cutters.cancel(&mut f.town, &mut f.ledger, &a(1)).unwrap();
        close(f.total(), 0.25);
        close(f.ledger.produced().quantity(&wood()).get(), 0.25);
        close(
            f.ledger.store(&b(1)).unwrap().pending[&PendingOwner::Orphan]
                .quantity(&wood())
                .get(),
            0.25,
        );
    }
}
#[test]
fn reload_all_phases_and_reject_bad_claims() {
    for phase in [
        WorkPhase::Outbound,
        WorkPhase::Working,
        WorkPhase::Returning,
        WorkPhase::Depositing,
    ] {
        let mut f = Fixture::new();
        f.work(0.5);
        f.set_phase(phase, 0.);
        let json = f.cutters.encode(&f.town).unwrap();
        assert_eq!(
            Woodcutters::decode(&json, &f.town, &f.ledger).unwrap(),
            f.cutters
        );
        let mut bad: serde_json::Value = serde_json::from_str(&json).unwrap();
        let duplicate = bad["claims"][0].clone();
        bad["claims"].as_array_mut().unwrap().push(duplicate);
        assert!(Woodcutters::decode(&bad.to_string(), &f.town, &f.ledger).is_err());
        bad = serde_json::from_str(&json).unwrap();
        bad["world"] = "foreign".into();
        assert!(Woodcutters::decode(&bad.to_string(), &f.town, &f.ledger).is_err());
        f.set_phase(phase, 4.);
        assert!(Woodcutters::decode(&json, &f.town, &f.ledger).is_err());
    }
}
#[test]
fn caps_and_full_buffers_stop_practice_and_competing_workers_share_limits() {
    let mut f = Fixture::new();
    f.ledger.configure_cap(wood(), Some(q(0.9))).unwrap();
    f.work(3.);
    f.assign(2, 1);
    f.ready(2, RouteOutcome::Arrived);
    f.advance(2, 3.);
    close(f.total(), 0.9);
    let skill = f.worker.practice_value("woodcutting");
    f.advance(1, 0.1);
    close(f.worker.practice_value("woodcutting"), skill);
    close(f.total(), 0.9);
    let mut f = Fixture::new();
    let output = Stock::from_entries([(wood(), q(50.))]).unwrap();
    f.ledger
        .produce(&b(1), None, &Stock::default(), &output)
        .unwrap();
    f.work(3.);
    close(f.total(), 50.);
    close(f.worker.practice_value("woodcutting"), 0.);
}
#[test]
fn work_boundary_returns_time_for_next_trip() {
    let mut f = Fixture::new();
    let Outcome::Advanced { remaining, .. } = f.work(10.) else {
        panic!("work");
    };
    close(remaining.get(), 6.8);
    f.ready(1, RouteOutcome::Arrived);
    let Outcome::Unloaded(remaining) = f.advance(1, remaining.get()) else {
        panic!("unload");
    };
    close(remaining.get(), 6.15);
    close(f.ledger.free(&wood()).unwrap().get(), 0.8);
    f.ready(1, RouteOutcome::Arrived);
    let Outcome::Advanced { produced, .. } = f.advance(1, remaining.get()) else {
        panic!("work");
    };
    close(produced.get(), 0.8);
    close(f.total(), 1.6);
}
#[test]
fn overflow_rolls_back_model_supply_and_practice() {
    let mut f = Fixture::new();
    f.worker
        .set_practice("woodcutting", Practice::new(f64::MAX).unwrap())
        .unwrap();
    f.worker
        .set_talents(BTreeMap::from([("woodcutting".into(), f64::MAX)]))
        .unwrap();
    f.assign(1, 1);
    f.ready(1, RouteOutcome::Arrived);
    let before = (
        f.town.clone(),
        f.ledger.clone(),
        f.trees.clone(),
        f.worker.clone(),
        f.cutters.clone(),
    );
    assert!(
        f.cutters
            .advance(
                &mut f.town,
                &mut f.ledger,
                &mut f.trees,
                &a(1),
                &mut f.worker,
                sec(3.),
                NavigationRevision(1),
                |_, _, s| Some(s)
            )
            .is_err()
    );
    assert_eq!((f.town, f.ledger, f.trees, f.worker, f.cutters), before);
}

#[test]
fn next_gatherer_adopts_cancelled_output_without_minting_or_training_for_it() {
    let mut f = Fixture::new();
    f.work(1.);
    f.cutters.cancel(&mut f.town, &mut f.ledger, &a(1)).unwrap();
    let skill = f.worker.practice_value("woodcutting");
    f.assign(2, 1);
    f.ready(2, RouteOutcome::Arrived);
    f.advance(2, 1.);
    close(f.total(), 0.5);
    close(f.ledger.produced().quantity(&wood()).get(), 0.5);
    close(f.worker.practice_value("woodcutting") - skill, 0.027);
    let pending = &f.ledger.store(&b(1)).unwrap().pending;
    assert!(!pending.contains_key(&PendingOwner::Orphan));
    close(
        pending[&PendingOwner::Worker(a(2))].quantity(&wood()).get(),
        0.5,
    );
}

#[test]
fn stalled_wood_chain_resumes_when_downstream_reserves_inputs() {
    let mut f = Fixture::new();
    let mut stores = f.ledger.stores().to_vec();
    stores.push(Store::new(b(3), StoragePolicy::new("sawpit")));
    f.ledger = Ledger::new(owner(), q(0.), stores, vec![]).unwrap();
    f.ledger.configure_cap(wood(), Some(q(0.8))).unwrap();
    f.work(3.2);
    f.ready(1, RouteOutcome::Arrived);
    f.advance(1, 0.65);
    f.ready(1, RouteOutcome::Arrived);
    f.advance(1, 1.);
    close(f.total(), 0.8);
    let skill = f.worker.practice_value("woodcutting");
    let inputs = Stock::from_entries([(wood(), q(0.8))]).unwrap();
    f.ledger.reserve(&b(1), &b(3), &inputs).unwrap();
    // Finish the empty return/unload, then use a fresh productive interval.
    f.ready(1, RouteOutcome::Arrived);
    f.advance(1, 0.65);
    f.ready(1, RouteOutcome::Arrived);
    f.advance(1, 1.);
    close(f.worker.practice_value("woodcutting") - skill, 0.027);
    close(
        f.ledger
            .store(&b(3))
            .unwrap()
            .inputs
            .quantity(&wood())
            .get(),
        0.8,
    );
    close(f.total(), 1.05);
    close(f.ledger.produced().quantity(&wood()).get(), 1.05);
}

#[test]
fn python_interruption_accounting_trace() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/gathering/python.json")).unwrap();
    let mut f = Fixture::new();
    for state in data["accounting"].as_array().unwrap() {
        match state["op"].as_str().unwrap() {
            "work" => {
                f.work(1.);
            }
            "cancel" => {
                f.cutters.cancel(&mut f.town, &mut f.ledger, &a(1)).unwrap();
            }
            "replacement" => {
                f.assign(2, 1);
                f.ready(2, RouteOutcome::Arrived);
                f.advance(2, 1.);
            }
            "dropoff" => {
                f.town
                    .set_task(
                        &a(2),
                        Task::Work {
                            workplace: b(1),
                            target: Destination::Position(p(5., 2.)),
                            phase: WorkPhase::Depositing,
                            elapsed: sec(0.),
                        },
                    )
                    .unwrap();
                f.advance(2, 0.65);
            }
            "retry" => {
                f.advance(2, 3.);
            }
            _ => panic!("operation"),
        }
        let store = f.ledger.store(&b(1)).unwrap();
        close(
            f.ledger.free(&wood()).unwrap().get(),
            state["free"].as_f64().unwrap(),
        );
        close(
            store.pending_stock().unwrap().quantity(&wood()).get(),
            state["pending"].as_f64().unwrap(),
        );
        for (owner, key) in [
            (PendingOwner::Orphan, "orphan"),
            (PendingOwner::Worker(a(2)), "worker"),
        ] {
            close(
                store
                    .pending
                    .get(&owner)
                    .map_or(0., |s| s.quantity(&wood()).get()),
                state[key].as_f64().unwrap(),
            );
        }
        close(
            f.ledger.produced().quantity(&wood()).get(),
            state["produced"].as_f64().unwrap(),
        );
    }
}
