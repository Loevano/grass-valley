use lantern_core::{
    ids::{ActorId, BuildingId, EntityIdentity, Owner, TownId, WorldId},
    model::*,
    units::*,
};
use lantern_host::{farming::*, progression::Worker, resources::*};
use std::collections::BTreeMap;
fn owner() -> Owner {
    Owner {
        world: WorldId::new("fields").unwrap(),
        town: TownId::new("one").unwrap(),
    }
}
fn b(n: u64) -> BuildingId {
    BuildingId::from_saved(EntityIdentity::from_saved(owner(), n).unwrap())
}
fn a(n: u64) -> ActorId {
    ActorId::from_saved(EntityIdentity::from_saved(owner(), 100 + n).unwrap())
}
fn sec(n: f64) -> Seconds {
    Seconds::new(n).unwrap()
}
fn q(n: f64) -> Quantity {
    Quantity::new(n).unwrap()
}
fn key(s: &str) -> ResourceKey {
    ResourceKey::parse(s).unwrap()
}
fn pos(x: f64, y: f64) -> Position {
    Position::new(x, y).unwrap()
}
fn close(x: f64, y: f64) {
    assert!((x - y).abs() < 1e-9, "{x} != {y}");
}
fn stage(s: &str) -> Stage {
    match s {
        "unplowed" => Stage::Unplowed,
        "plowed" => Stage::Plowed,
        "sprouting" => Stage::Sprouting,
        "thirsty" => Stage::Thirsty,
        "growing" => Stage::Growing,
        "ripe" => Stage::Ripe,
        _ => panic!("stage"),
    }
}
fn state(s: Stage) -> FieldState {
    FieldState {
        stage: s,
        crop: matches!(
            s,
            Stage::Sprouting | Stage::Thirsty | Stage::Growing | Stage::Ripe
        )
        .then(|| key("wheat")),
        ..FieldState::default()
    }
}
struct Fixture {
    town: Town,
    ledger: Ledger,
    fields: Fields,
    worker: Worker,
    revision: u64,
}
impl Fixture {
    fn new(stages: &[Stage]) -> Self {
        Self::with(
            stages,
            |_| {},
            |_| {},
            BTreeMap::from([(a(1), b(1)), (a(2), b(1)), (a(3), b(1))]),
        )
    }
    fn with(
        stages: &[Stage],
        edit: impl FnOnce(&mut Vec<Field>),
        stores: impl FnOnce(&mut Vec<Store>),
        assignments: BTreeMap<ActorId, BuildingId>,
    ) -> Self {
        let mut buildings = vec![Building {
            id: b(1),
            location: Tile { x: 3, y: 3 },
            construction: ConstructionState::Complete,
        }];
        let mut fields = vec![Field {
            id: b(1),
            farm: None,
            selected_crop: key("vegetables"),
            state: FieldState::default(),
        }];
        let mut stock = vec![
            Store::new(b(1), StoragePolicy::new("farm")),
            Store::new(b(50), StoragePolicy::new("hall")),
        ];
        for (i, s) in stages.iter().enumerate() {
            let id = b(i as u64 + 2);
            buildings.push(Building {
                id: id.clone(),
                location: Tile {
                    x: 3 + (i % 2) as i32,
                    y: 4 + (i / 2) as i32,
                },
                construction: ConstructionState::Complete,
            });
            stock.push(Store::new(id.clone(), StoragePolicy::new("acre")));
            fields.push(Field {
                id,
                farm: Some(b(1)),
                selected_crop: key("wheat"),
                state: state(*s),
            });
        }
        edit(&mut fields);
        stores(&mut stock);
        let town = Town::restore(
            owner(),
            buildings,
            (1..=3)
                .map(|i| ActorState::new(a(i), pos(3., 4.), Cargo::default()))
                .collect(),
        )
        .unwrap();
        let ledger = Ledger::new(owner(), q(0.), stock, vec![]).unwrap();
        let fields = Fields::new(&town, &ledger, fields, assignments).unwrap();
        Self {
            town,
            ledger,
            fields,
            worker: Worker::new("farming").unwrap(),
            revision: 1,
        }
    }
    fn assign(&mut self, i: u64) -> Option<BuildingId> {
        self.fields
            .assign(
                &mut self.town,
                &self.ledger,
                &a(i),
                &b(1),
                NavigationRevision(1),
            )
            .unwrap()
    }
    fn route(&mut self, i: u64, outcome: RouteOutcome) {
        self.revision += 1;
        self.town
            .resume_travel(&a(i), NavigationRevision(self.revision))
            .unwrap();
        let RouteState::Pending(r) = self.town.actor(&a(i)).unwrap().route().clone() else {
            panic!("route")
        };
        self.town
            .accept_route(&a(i), &r, r.navigation(), outcome)
            .unwrap();
    }
    fn advance(&mut self, i: u64, n: f64) -> Outcome {
        self.fields
            .advance(
                &mut self.town,
                &mut self.ledger,
                &a(i),
                &mut self.worker,
                sec(n),
                NavigationRevision(self.revision),
                |_, _, s| Some(s),
            )
            .unwrap()
    }
    fn ready(&mut self, i: u64) {
        self.assign(i).unwrap();
        self.route(i, RouteOutcome::Arrived);
    }
}
#[test]
fn shared_progress_survives_cancel_and_replacement_without_duplicate_completion() {
    let mut f = Fixture::new(&[Stage::Unplowed]);
    f.ready(1);
    assert_eq!(f.advance(1, 0.75), Outcome::Progress);
    assert_eq!(f.assign(2), None);
    f.fields.cancel(&mut f.town, &a(1)).unwrap();
    f.ready(2);
    let Outcome::Completed {
        action,
        state,
        remaining,
        ..
    } = f.advance(2, 3.)
    else {
        panic!("complete")
    };
    assert_eq!(action, Action::Plow);
    assert_eq!(state.stage, Stage::Plowed);
    close(remaining.get(), 1.75);
    assert_eq!(f.advance(2, 100.), Outcome::Waiting(Wait::NoClaim));
    f.ready(1);
    let Outcome::Completed { action, state, .. } = f.advance(1, 2.) else {
        panic!("sow")
    };
    assert_eq!(action, Action::Sow);
    assert_eq!(state.crop, Some(key("wheat")));
    f.fields.grow(&f.town, &f.ledger, sec(15.));
    assert_eq!(state.elapsed, sec(0.)); // Completed event is immutable.
    assert_eq!(f.fields.field(&b(2)).unwrap().state.stage, Stage::Thirsty);
}
#[test]
fn paired_python_growth_selection_and_passes() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/farming/python.json")).unwrap();
    for c in data["growth"].as_array().unwrap() {
        let mut f = Fixture::new(&[stage(c["stage"].as_str().unwrap())]);
        for n in c["parts"].as_array().unwrap() {
            f.fields.grow(&f.town, &f.ledger, sec(n.as_f64().unwrap()));
        }
        assert_eq!(
            f.fields.field(&b(2)).unwrap().state.stage,
            stage(c["state"]["stage"].as_str().unwrap())
        );
        close(
            f.fields.field(&b(2)).unwrap().state.elapsed.get(),
            c["state"]["elapsed"].as_f64().unwrap(),
        );
    }
    for c in data["selection"].as_array().unwrap() {
        let stages: Vec<_> = c["stages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| stage(s.as_str().unwrap()))
            .collect();
        let mut f = Fixture::new(&stages);
        assert_eq!(
            f.fields.work_pass(&f.town, &f.ledger, &b(1)).unwrap(),
            (
                c["work_pass"][0].as_u64().unwrap(),
                stage(c["work_pass"][1].as_str().unwrap())
            )
        );
        for i in 1..=2 {
            assert_eq!(f.assign(i), c["chosen"][i as usize - 1].as_u64().map(b));
        }
    }
}
#[test]
fn paired_python_crop_yields_tools_levels_manure_and_caps() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/farming/python.json")).unwrap();
    for c in data["yields"].as_array().unwrap() {
        let crop = key(c["crop"].as_str().unwrap());
        let mut f = Fixture::with(
            &[Stage::Ripe],
            |fields| {
                fields[1].selected_crop = crop.clone();
                fields[1].state.crop = Some(crop.clone());
                fields[1].state.progress = sec(2.);
            },
            |stores| {
                stores[0].policy.level = c["level"].as_u64().unwrap() as u8;
                stores[1].stored =
                    Stock::from_entries([(key("manure"), q(c["manure"].as_f64().unwrap()))])
                        .unwrap();
            },
            BTreeMap::from([(a(1), b(1))]),
        );
        f.worker
            .set_practice(
                "farming",
                Practice::new(c["skill"].as_f64().unwrap()).unwrap(),
            )
            .unwrap();
        f.worker
            .set_talents(BTreeMap::from([(
                "farming".into(),
                c["talent"].as_f64().unwrap(),
            )]))
            .unwrap();
        f.worker.tool = c["tool"].as_str().map(str::to_owned);
        f.ledger
            .configure_cap(crop.clone(), c["cap"].as_f64().map(q))
            .unwrap();
        f.ready(1);
        let result = f.advance(1, 0.);
        if c["success"] == true {
            let Outcome::Completed {
                action,
                produced,
                state,
                ..
            } = result
            else {
                panic!("{c}: {result:?}")
            };
            assert_eq!(action, Action::Harvest);
            close(produced.get(), c["produced"].as_f64().unwrap());
            assert_eq!(state.stage, stage(c["state"]["stage"].as_str().unwrap()));
            assert_eq!(state.cycle, c["state"]["cycle"].as_u64().unwrap());
            if let Some(n) = c["state"]["yield_remaining"].as_f64() {
                close(state.yield_remaining.unwrap().get(), n);
            }
        } else {
            assert_eq!(result, Outcome::Waiting(Wait::Storage));
        }
        close(
            f.ledger.free(&key("manure")).unwrap().get(),
            c["manure_left"].as_f64().unwrap(),
        );
        close(
            f.ledger.produced().quantity(&crop).get(),
            c["produced"].as_f64().unwrap(),
        );
        close(f.ledger.free(&crop).unwrap().get(), 0.);
    }
}
#[test]
fn construction_barriers_and_harvested_next_pass_do_not_replow_early() {
    let mut f = Fixture::with(
        &[Stage::Unplowed, Stage::Plowed],
        |_| {},
        |s| s[2].policy.construction = true,
        BTreeMap::from([(a(1), b(1))]),
    );
    assert_eq!(f.assign(1), None); // Construction acreage still blocks sowing.
    let mut f = Fixture::with(
        &[Stage::Unplowed, Stage::Ripe],
        |v| v[1].state.cycle = 1,
        |_| {},
        BTreeMap::from([(a(1), b(1))]),
    );
    assert_eq!(f.assign(1), Some(b(3)));
    f.route(1, RouteOutcome::Arrived);
    let Outcome::Completed { state, .. } = f.advance(1, 2.) else {
        panic!("harvest")
    };
    assert_eq!(state.cycle, 1);
    assert_eq!(f.assign(1), Some(b(3))); // Finish cargo return before the next pass.
    f.route(1, RouteOutcome::Arrived);
    assert!(matches!(f.advance(1, 0.65), Outcome::Unloaded { .. }));
    assert_eq!(f.assign(1), Some(b(2)));
}
#[test]
fn claims_use_complete_roster_direct_acre_priority_and_actor_order() {
    let mut f = Fixture::with(
        &[Stage::Unplowed, Stage::Unplowed],
        |_| {},
        |_| {},
        BTreeMap::from([(a(1), b(1)), (a(2), b(2)), (a(3), b(2))]),
    );
    assert_eq!(f.assign(1), Some(b(3)));
    assert_eq!(
        f.fields
            .assign(&mut f.town, &f.ledger, &a(3), &b(2), NavigationRevision(1))
            .unwrap(),
        None
    );
    assert_eq!(
        f.fields
            .assign(&mut f.town, &f.ledger, &a(2), &b(2), NavigationRevision(1))
            .unwrap(),
        Some(b(2))
    );
    assert_eq!(f.advance(2, 100.), Outcome::Waiting(Wait::Travel));
    assert_eq!(f.fields.field(&b(2)).unwrap().state.progress, sec(0.));
}
#[test]
fn pending_blocked_and_unreachable_routes_never_work_and_can_resume() {
    let mut f = Fixture::new(&[Stage::Unplowed]);
    f.assign(1);
    assert_eq!(f.advance(1, 100.), Outcome::Waiting(Wait::Travel));
    f.route(1, RouteOutcome::Ready(vec![pos(2.55, 4.1)]));
    let result = f
        .fields
        .advance(
            &mut f.town,
            &mut f.ledger,
            &a(1),
            &mut f.worker,
            sec(100.),
            NavigationRevision(1),
            |_, _, _| None,
        )
        .unwrap();
    assert_eq!(result, Outcome::Waiting(Wait::Travel));
    close(f.worker.practice_value("farming"), 0.);
    f.route(1, RouteOutcome::Unreachable);
    assert_eq!(f.advance(1, 100.), Outcome::Waiting(Wait::Travel));
    f.route(1, RouteOutcome::Arrived);
    assert!(matches!(
        f.advance(1, 2.),
        Outcome::Completed {
            action: Action::Plow,
            ..
        }
    ));
}
#[test]
fn continuous_approach_time_is_removed_before_stationary_work() {
    let mut f = Fixture::new(&[Stage::Unplowed]);
    f.assign(1);
    f.route(1, RouteOutcome::Ready(vec![pos(2.55, 4.1)]));
    let Outcome::Completed { remaining, .. } = f.advance(1, 3.) else {
        panic!("plow")
    };
    close(remaining.get(), 1. - 0.45_f64.hypot(0.1) / 1.8);
    assert_eq!(
        f.town.actor(&a(1)).unwrap().state().position,
        pos(2.55, 4.1)
    );
}
#[test]
fn partial_yield_is_frozen_and_cannot_be_double_claimed_or_retrained() {
    let mut f = Fixture::new(&[Stage::Ripe]);
    f.ledger.configure_cap(key("wheat"), Some(q(2.))).unwrap();
    f.ready(1);
    let Outcome::Completed {
        state, produced, ..
    } = f.advance(1, 2.)
    else {
        panic!("harvest")
    };
    close(produced.get(), 2.);
    close(state.yield_remaining.unwrap().get(), 8.);
    assert_eq!(f.assign(2), None);
    let trained = f.worker.practice_value("farming");
    assert_eq!(f.advance(1, 100.), Outcome::TravelStarted(sec(100.)));
    f.route(1, RouteOutcome::Arrived);
    assert!(matches!(f.advance(1, 0.65), Outcome::Unloaded { .. }));
    f.worker.tool = Some("masterwork_sickle".into());
    f.ledger.configure_cap(key("wheat"), None).unwrap();
    f.ready(1);
    let Outcome::Completed { produced, .. } = f.advance(1, 100.) else {
        panic!("rest")
    };
    close(produced.get(), 8.);
    close(f.worker.practice_value("farming"), trained);
    close(f.ledger.produced().quantity(&key("wheat")).get(), 10.);
    assert_eq!(f.advance(1, 100.), Outcome::Waiting(Wait::NoClaim));
}
#[test]
fn independent_subdivisions_preserve_fields_yields_and_action_counts() {
    for parts in [vec![2.], vec![1., 1.], vec![0.25; 8]] {
        let mut f = Fixture::new(&[Stage::Ripe]);
        f.ready(1);
        let mut completed = 0;
        for n in parts {
            if matches!(f.advance(1, n), Outcome::Completed { .. }) {
                completed += 1;
            }
        }
        assert_eq!(completed, 1);
        assert_eq!(
            f.fields.field(&b(2)).unwrap().state,
            FieldState {
                cycle: 1,
                ..FieldState::default()
            }
        );
        close(f.ledger.produced().quantity(&key("wheat")).get(), 10.);
    }
}
#[test]
fn farm_capacity_and_atomic_completion_errors_preserve_all_owners() {
    let mut f = Fixture::with(
        &[Stage::Ripe],
        |v| v[1].state.cycle = u64::MAX,
        |_| {},
        BTreeMap::from([(a(1), b(1))]),
    );
    f.ready(1);
    let before = (
        f.fields.clone(),
        f.town.clone(),
        f.ledger.clone(),
        f.worker.clone(),
    );
    assert!(
        f.fields
            .advance(
                &mut f.town,
                &mut f.ledger,
                &a(1),
                &mut f.worker,
                sec(2.),
                NavigationRevision(1),
                |_, _, s| Some(s)
            )
            .is_err()
    );
    assert_eq!((f.fields, f.town, f.ledger, f.worker), before);
    for (level, room) in [(1, 480.), (2, 1080.), (3, 1920.)] {
        let mut p = StoragePolicy::new("farm");
        p.level = level;
        close(
            p.room("wheat", &Stock::default(), &Stock::default(), 0.),
            room,
        );
    }
}

#[test]
fn subdivisions_preserve_yield_at_skill_thresholds() {
    let mut outputs = Vec::new();
    for parts in [vec![2.], vec![1., 1.], vec![0.1; 20]] {
        let mut f = Fixture::new(&[Stage::Ripe]);
        f.worker
            .set_practice("farming", Practice::new(19.946).unwrap())
            .unwrap();
        f.ready(1);
        for n in parts {
            f.advance(1, n);
        }
        outputs.push((
            f.ledger.produced().quantity(&key("wheat")).get(),
            f.worker.practice_value("farming"),
        ));
    }
    for (output, practice) in outputs {
        close(output, 10.);
        close(practice, 19.999);
    }
}

#[test]
fn fallback_farm_works_and_unfinished_acres_do_not_grow_or_get_claimed() {
    let mut f = Fixture::new(&[]);
    f.ready(1);
    assert!(
        matches!(f.advance(1,2.),Outcome::Completed{field,action:Action::Plow,..} if field==b(1))
    );
    let mut f = Fixture::with(
        &[Stage::Growing],
        |_| {},
        |s| s[2].policy.construction = true,
        BTreeMap::from([(a(1), b(1))]),
    );
    f.fields.grow(&f.town, &f.ledger, sec(100.));
    assert_eq!(f.fields.field(&b(2)).unwrap().state.stage, Stage::Growing);
    assert_eq!(f.assign(1), Some(b(1)));
}

#[test]
fn cancelled_harvest_goods_remain_owned_and_retries_never_mint_them_again() {
    let mut f = Fixture::new(&[Stage::Ripe, Stage::Ripe]);
    f.ready(1);
    let Outcome::Completed { produced, .. } = f.advance(1, 2.) else {
        panic!("harvest")
    };
    close(produced.get(), 10.);
    f.fields.cancel(&mut f.town, &a(1)).unwrap();
    assert_eq!(
        f.ledger.store(&b(1)).unwrap().pending[&PendingOwner::Worker(a(1))].quantity(&key("wheat")),
        q(10.)
    );
    f.ready(1);
    f.advance(1, 2.);
    close(f.ledger.produced().quantity(&key("wheat")).get(), 20.);
    close(
        f.ledger.store(&b(1)).unwrap().pending[&PendingOwner::Worker(a(1))]
            .quantity(&key("wheat"))
            .get(),
        20.,
    );
    // Existing ledger operation conserves the combined cargo across repeated unload.
    f.ledger.drop_off(&b(1), &a(1)).unwrap();
    assert!(f.ledger.drop_off(&b(1), &a(1)).is_err());
    close(f.ledger.free(&key("wheat")).unwrap().get(), 20.);
}

#[test]
fn paired_python_plowing_sowing_shared_clocks_and_unused_time() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/farming/python.json")).unwrap();
    for c in data["actions"].as_array().unwrap() {
        let mut f = Fixture::with(
            &[stage(c["stage"].as_str().unwrap())],
            |v| v[1].state.progress = sec(c["progress"].as_f64().unwrap()),
            |_| {},
            BTreeMap::from([(a(1), b(1))]),
        );
        f.ready(1);
        let result = f.advance(1, c["seconds"].as_f64().unwrap());
        if let Outcome::Completed { remaining, .. } = result {
            close(remaining.get(), c["remaining"].as_f64().unwrap());
        }
        let s = &f.fields.field(&b(2)).unwrap().state;
        assert_eq!(s.stage, stage(c["state"]["stage"].as_str().unwrap()));
        close(s.progress.get(), c["state"]["progress"].as_f64().unwrap());
        close(
            f.worker.practice_value("farming"),
            c["practice"].as_f64().unwrap(),
        );
    }
}

#[test]
fn invalid_links_crops_clocks_duplicate_fields_and_rosters_are_rejected() {
    let f = Fixture::new(&[Stage::Ripe]);
    let originals = vec![
        f.fields.field(&b(1)).unwrap().clone(),
        f.fields.field(&b(2)).unwrap().clone(),
    ];
    for mode in 0..6 {
        let mut fields = originals.clone();
        let mut roster = BTreeMap::from([(a(1), b(1))]);
        match mode {
            0 => fields.push(fields[1].clone()),
            1 => fields[1].farm = Some(b(2)),
            2 => fields[1].selected_crop = key("wood"),
            3 => fields[1].state.progress = sec(3.),
            4 => fields[1].state.elapsed = sec(1.),
            5 => {
                roster.insert(a(99), b(1));
            }
            _ => unreachable!(),
        }
        assert!(Fields::new(&f.town, &f.ledger, fields, roster).is_err());
    }
    let mut f = Fixture::new(&[Stage::Unplowed; 48]);
    let mut fields: Vec<_> = (1..=49)
        .map(|i| f.fields.field(&b(i)).unwrap().clone())
        .collect();
    f.town
        .insert_building(Building {
            id: b(70),
            location: Tile { x: 10, y: 10 },
            construction: ConstructionState::Complete,
        })
        .unwrap();
    let mut stores = f.ledger.stores().to_vec();
    stores.push(Store::new(b(70), StoragePolicy::new("acre")));
    f.ledger = Ledger::new(owner(), q(0.), stores, vec![]).unwrap();
    fields.push(Field {
        id: b(70),
        farm: Some(b(1)),
        selected_crop: key("wheat"),
        state: FieldState::default(),
    });
    assert!(Fields::new(&f.town, &f.ledger, fields, BTreeMap::new()).is_err());
}

#[test]
fn roster_reassignment_releases_direct_priority_and_preserves_work_and_cargo() {
    let mut f = Fixture::new(&[Stage::Unplowed]);
    f.ready(1);
    f.advance(1, 0.75);
    f.fields
        .set_assignment(&mut f.town, &f.ledger, &a(2), Some(&b(2)))
        .unwrap();
    assert_eq!(f.fields.target(&a(1)), None);
    assert_eq!(f.assign(1), None);
    assert_eq!(
        f.fields
            .assign(&mut f.town, &f.ledger, &a(2), &b(2), NavigationRevision(1))
            .unwrap(),
        Some(b(2))
    );
    f.route(2, RouteOutcome::Arrived);
    assert!(matches!(
        f.advance(2, 1.25),
        Outcome::Completed {
            action: Action::Plow,
            ..
        }
    ));
    f.fields
        .set_assignment(&mut f.town, &f.ledger, &a(2), None)
        .unwrap();
    assert_eq!(f.assign(1), Some(b(2)));
    let before = (f.fields.clone(), f.town.clone());
    assert!(
        f.fields
            .set_assignment(&mut f.town, &f.ledger, &a(1), Some(&b(99)))
            .is_err()
    );
    assert_eq!((f.fields, f.town), before);
    let mut f = Fixture::new(&[Stage::Ripe]);
    f.ready(1);
    f.advance(1, 2.);
    let before = f.ledger.clone();
    f.fields
        .set_assignment(&mut f.town, &f.ledger, &a(1), None)
        .unwrap();
    assert_eq!(f.ledger, before);
}

#[path = "farming/trips.rs"]
mod trips;

#[path = "farming/lifecycle.rs"]
mod lifecycle;
