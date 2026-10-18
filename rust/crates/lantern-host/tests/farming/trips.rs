use super::*;
impl Fixture {
    fn pump(&mut self, i: u64) {
        if let RouteState::Pending(r) = self.town.actor(&a(i)).unwrap().route().clone() {
            let path = if r.origin() == r.goal() {
                RouteOutcome::Arrived
            } else {
                RouteOutcome::Ready(vec![r.goal()])
            };
            self.town
                .accept_route(&a(i), &r, r.navigation(), path)
                .unwrap();
        }
    }
    pub(super) fn water(&mut self, i: u64, n: f64, sources: &WaterSources) -> Outcome {
        self.fields
            .advance_with_water(
                &mut self.town,
                &mut self.ledger,
                &a(i),
                &mut self.worker,
                sec(n),
                NavigationRevision(self.revision),
                sources,
                |p, banks| {
                    banks
                        .iter()
                        .min_by(|a, b| {
                            let d = |bank: &WaterBank| {
                                (f64::from(bank.tile.x) - p.x.get())
                                    .hypot(f64::from(bank.tile.y) - p.y.get())
                            };
                            d(a).total_cmp(&d(b))
                                .then((a.tile.x, a.tile.y).cmp(&(b.tile.x, b.tile.y)))
                        })
                        .cloned()
                },
                |_, _, s| Some(s),
            )
            .unwrap()
    }
    // The caller owns navigation and consumes boundary remainders. No time is
    // banked across pending/blocked query waits, just as for R14/R15 actors.
    fn run(&mut self, i: u64, n: f64, sources: &WaterSources) -> Vec<(f64, Outcome)> {
        let mut left = n;
        let mut events = Vec::new();
        for _ in 0..1000 {
            self.assign(i);
            self.pump(i);
            let outcome = self.water(i, left, sources);
            let remaining = match &outcome {
                Outcome::Completed { remaining, .. }
                | Outcome::WaterCollected { remaining, .. }
                | Outcome::Unloaded { remaining, .. }
                | Outcome::TravelStarted(remaining) => Some(remaining.get()),
                _ => None,
            };
            events.push((n - remaining.unwrap_or(0.), outcome));
            let Some(next) = remaining else {
                return events;
            };
            if next <= 1e-9 {
                return events;
            }
            left = next;
        }
        panic!("no-progress farming loop");
    }
}
fn water() -> WaterSources {
    WaterSources::new(vec![Tile { x: 8, y: 4 }], vec![])
}
fn count_water(events: &[(f64, Outcome)]) -> usize {
    events
        .iter()
        .filter(|(_, e)| matches!(e, Outcome::WaterCollected { .. }))
        .count()
}
#[test]
fn one_filled_can_covers_whole_farm_cycle_but_not_another_farmer() {
    let mut f = Fixture::new(&[Stage::Thirsty, Stage::Thirsty]);
    let events = f.run(1, 30., &water());
    assert_eq!(count_water(&events), 1);
    assert_eq!(
        events
            .iter()
            .filter(|(_, e)| matches!(
                e,
                Outcome::Completed {
                    action: Action::Water,
                    ..
                }
            ))
            .count(),
        2
    );
    assert!(f.fields.water_ready(&a(1), &b(1), 0));
    assert!(!f.fields.water_ready(&a(1), &b(1), 1));
    close(f.worker.practice_value("farming"), 0.106);
    // A separate worker has no right to the first worker's can.
    let mut f = Fixture::new(&[Stage::Thirsty, Stage::Thirsty]);
    f.ready(1);
    f.assign(2);
    assert_eq!(count_water(&f.run(1, 30., &water())), 1);
    assert_eq!(count_water(&f.run(2, 30., &water())), 1);
}
#[test]
fn watering_round_survives_cancelled_animation_and_missing_water() {
    let mut f = Fixture::new(&[Stage::Thirsty, Stage::Thirsty]);
    f.ready(1);
    assert!(matches!(
        f.water(1, 0., &water()),
        Outcome::TravelStarted(_)
    ));
    f.route(1, RouteOutcome::Arrived);
    assert!(matches!(
        f.water(1, 0.65, &water()),
        Outcome::WaterCollected { .. }
    ));
    f.fields.cancel(&mut f.town, &a(1)).unwrap();
    let events = f.run(1, 10., &WaterSources::default());
    assert_eq!(count_water(&events), 0);
    assert_eq!(f.fields.field(&b(2)).unwrap().state.stage, Stage::Growing);
    assert_eq!(f.fields.field(&b(3)).unwrap().state.stage, Stage::Growing);
}
#[test]
fn blocked_water_return_retains_collection_and_retries_without_practice() {
    let mut f = Fixture::new(&[Stage::Thirsty]);
    f.ready(1);
    f.water(1, 0., &water());
    f.route(1, RouteOutcome::Arrived);
    assert_eq!(f.water(1, 0.2, &water()), Outcome::Progress);
    assert!(!f.fields.water_ready(&a(1), &b(1), 0));
    assert!(
        matches!(f.water(1,0.45,&water()),Outcome::WaterCollected{remaining,..} if remaining==sec(0.))
    );
    f.route(1, RouteOutcome::Ready(vec![pos(2.55, 4.1)]));
    let outcome = f
        .fields
        .advance_with_water(
            &mut f.town,
            &mut f.ledger,
            &a(1),
            &mut f.worker,
            sec(100.),
            NavigationRevision(1),
            &WaterSources::default(),
            |_, _| panic!("must not recollect"),
            |_, _, _| None,
        )
        .unwrap();
    assert_eq!(outcome, Outcome::Waiting(Wait::Travel));
    close(f.worker.practice_value("farming"), 0.);
    assert!(f.fields.water_ready(&a(1), &b(1), 0));
    f.route(1, RouteOutcome::Unreachable);
    assert_eq!(
        f.water(1, 100., &WaterSources::default()),
        Outcome::Waiting(Wait::Travel)
    );
    f.route(1, RouteOutcome::Arrived);
    assert!(matches!(
        f.water(1, 2., &WaterSources::default()),
        Outcome::Completed {
            action: Action::Water,
            ..
        }
    ));
}
#[test]
fn missing_removed_and_unfinished_wells_cannot_fill_a_can() {
    let mut f = Fixture::new(&[Stage::Thirsty]);
    f.ready(1);
    assert_eq!(
        f.water(1, 30., &WaterSources::default()),
        Outcome::Waiting(Wait::Water)
    );
    f.town
        .insert_building(Building {
            id: b(60),
            location: Tile { x: 8, y: 4 },
            construction: ConstructionState::Complete,
        })
        .unwrap();
    let mut stores = f.ledger.stores().to_vec();
    stores.push(Store::new(b(60), StoragePolicy::new("well")));
    f.ledger = Ledger::new(owner(), q(0.), stores, vec![]).unwrap();
    let sources = WaterSources::new(vec![], vec![b(60)]);
    assert!(matches!(
        f.water(1, 0., &sources),
        Outcome::TravelStarted(_)
    ));
    f.route(1, RouteOutcome::Arrived);
    f.water(1, 0.3, &sources);
    let mut policy = StoragePolicy::new("well");
    policy.construction = true;
    f.ledger.update_storage(&b(60), policy).unwrap();
    assert_eq!(f.water(1, 10., &sources), Outcome::Waiting(Wait::Water));
    assert!(!f.fields.water_ready(&a(1), &b(1), 0));
    f.ledger
        .update_storage(&b(60), StoragePolicy::new("well"))
        .unwrap();
    f.water(1, 0., &sources);
    f.route(1, RouteOutcome::Arrived);
    assert_eq!(f.water(1, 0.3, &sources), Outcome::Progress); // Old collection was reset.
    f.town.remove_building(&b(60)).unwrap();
    assert_eq!(f.water(1, 10., &sources), Outcome::Waiting(Wait::Water));
}
#[test]
fn navigation_selects_only_admitted_banks_and_bad_choice_rolls_back() {
    let mut f = Fixture::new(&[Stage::Thirsty]);
    f.ready(1);
    let before = (
        f.fields.clone(),
        f.town.clone(),
        f.ledger.clone(),
        f.worker.clone(),
    );
    assert!(
        f.fields
            .advance_with_water(
                &mut f.town,
                &mut f.ledger,
                &a(1),
                &mut f.worker,
                sec(2.),
                NavigationRevision(1),
                &water(),
                |_, _| Some(WaterBank {
                    source: WaterSource::Surface(Tile { x: 8, y: 4 }),
                    tile: Tile { x: 99, y: 99 }
                }),
                |_, _, s| Some(s)
            )
            .is_err()
    );
    assert_eq!(
        (
            f.fields.clone(),
            f.town.clone(),
            f.ledger.clone(),
            f.worker.clone()
        ),
        before
    );
    // The query may choose a farther bank if the nearest is blocked or costly.
    let mut chosen = None;
    let result = f
        .fields
        .advance_with_water(
            &mut f.town,
            &mut f.ledger,
            &a(1),
            &mut f.worker,
            sec(2.),
            NavigationRevision(1),
            &water(),
            |_, banks| {
                chosen = banks
                    .iter()
                    .find(|b| b.tile == Tile { x: 9, y: 4 })
                    .cloned();
                chosen.clone()
            },
            |_, _, s| Some(s),
        )
        .unwrap();
    assert!(matches!(result, Outcome::TravelStarted(_)));
    assert!(chosen.is_some());
}
#[test]
fn mixed_crop_harvest_crosses_passes_and_unloads_only_after_final_acre() {
    let mut f = Fixture::with(
        &[Stage::Ripe, Stage::Ripe, Stage::Unplowed],
        |v| {
            v[2].selected_crop = key("vegetables");
            v[2].state.crop = Some(key("vegetables"));
            v[2].state.cycle = 2;
            v[3].state.cycle = 1;
        },
        |_| {},
        BTreeMap::from([(a(1), b(1))]),
    );
    let events = f.run(1, 15., &WaterSources::default());
    let harvests: Vec<_> = events
        .iter()
        .filter(|(_, e)| {
            matches!(
                e,
                Outcome::Completed {
                    action: Action::Harvest,
                    ..
                }
            )
        })
        .collect();
    let unload = events
        .iter()
        .find(|(_, e)| matches!(e, Outcome::Unloaded { .. }))
        .unwrap();
    assert_eq!(harvests.len(), 2);
    assert!(unload.0 > harvests[1].0);
    close(f.ledger.free(&key("wheat")).unwrap().get(), 10.);
    close(f.ledger.free(&key("vegetables")).unwrap().get(), 4.8);
}
#[test]
fn crop_return_blockage_keeps_mixed_cargo_and_finishes_before_new_work() {
    let mut f = Fixture::with(
        &[Stage::Ripe, Stage::Ripe],
        |v| {
            v[2].state.crop = Some(key("vegetables"));
        },
        |_| {},
        BTreeMap::from([(a(1), b(1))]),
    );
    f.ready(1);
    f.advance(1, 2.);
    f.ready(1);
    f.advance(1, 2.);
    let source = f.assign(1);
    assert!(source.is_some());
    f.route(1, RouteOutcome::Ready(vec![pos(3., 3.)]));
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
    let snapshot = f.ledger.clone();
    f.assign(1);
    assert_eq!(f.ledger, snapshot);
    f.route(1, RouteOutcome::Arrived);
    assert_eq!(f.advance(1, 0.3), Outcome::Progress);
    close(f.ledger.free(&key("wheat")).unwrap().get(), 0.);
    assert!(matches!(f.advance(1,0.35),Outcome::Unloaded{remaining,..} if remaining==sec(0.)));
    close(f.ledger.free(&key("wheat")).unwrap().get(), 10.);
    close(f.ledger.free(&key("vegetables")).unwrap().get(), 4.8);
    assert_eq!(f.advance(1, 100.), Outcome::Waiting(Wait::NoClaim));
}
#[test]
fn buffer_and_cap_clip_yields_and_force_cargo_home_before_resuming() {
    let mut f = Fixture::new(&[Stage::Ripe, Stage::Ripe]);
    f.ledger.configure_cap(key("wheat"), Some(q(15.))).unwrap();
    let events = f.run(1, 20., &WaterSources::default());
    assert_eq!(
        events
            .iter()
            .filter(|(_, e)| matches!(e, Outcome::Unloaded { .. }))
            .count(),
        1
    );
    close(f.ledger.produced().quantity(&key("wheat")).get(), 15.);
    assert_eq!(f.fields.field(&b(3)).unwrap().state.stage, Stage::Ripe);
    close(
        f.fields
            .field(&b(3))
            .unwrap()
            .state
            .yield_remaining
            .unwrap()
            .get(),
        5.,
    );
    f.ledger.configure_cap(key("wheat"), None).unwrap();
    f.run(1, 20., &WaterSources::default());
    close(f.ledger.produced().quantity(&key("wheat")).get(), 20.);
    close(f.ledger.free(&key("wheat")).unwrap().get(), 20.);
}
#[test]
fn subdivisions_preserve_water_harvest_delivery_and_event_times() {
    let mut results = Vec::new();
    for parts in [vec![30.], vec![1.; 30], vec![0.25; 120]] {
        let mut f = Fixture::new(&[Stage::Thirsty, Stage::Thirsty]);
        let mut events = Vec::new();
        let mut time = 0.;
        for n in &parts {
            events.extend(
                f.run(1, *n, &water())
                    .into_iter()
                    .map(|(t, e)| (time + t, e)),
            );
            time += n;
        }
        assert_eq!(count_water(&events), 1);
        f.fields.grow(&f.town, &f.ledger, sec(15.));
        let mut harvests = Vec::new();
        time = 0.;
        for n in parts {
            harvests.extend(
                f.run(1, n, &WaterSources::default())
                    .into_iter()
                    .filter_map(|(t, e)| match e {
                        Outcome::Completed {
                            action: Action::Harvest,
                            ..
                        }
                        | Outcome::Unloaded { .. } => Some(time + t),
                        _ => None,
                    }),
            );
            time += n;
        }
        results.push((
            f.ledger.produced().quantity(&key("wheat")).get(),
            f.ledger.free(&key("wheat")).unwrap().get(),
            harvests,
        ));
    }
    for (produced, free, times) in &results {
        close(*produced, 20.);
        close(*free, 20.);
        assert_eq!(times.len(), 3);
        for (a, b) in times.iter().zip(&results[0].2) {
            close(*a, *b);
        }
    }
}

#[test]
fn paired_python_water_rounds_mixed_harvest_caps_and_buffers() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/farming/trips.json")).unwrap();
    for case in data.as_array().unwrap() {
        let spec = &case["spec"];
        let stages: Vec<_> = spec["stages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| stage(s.as_str().unwrap()))
            .collect();
        let mut f = Fixture::with(
            &stages,
            |fields| {
                for (i, field) in fields.iter_mut().skip(1).enumerate() {
                    let crop = key(spec["crops"][i].as_str().unwrap());
                    field.selected_crop = crop.clone();
                    field.state.crop = Some(crop);
                    field.state.cycle = spec["cycles"][i].as_u64().unwrap_or(0);
                }
            },
            |stores| {
                if let Some(n) = spec["stored"].as_f64() {
                    stores[0].output = Stock::from_entries([(key("wheat"), q(n))]).unwrap();
                }
            },
            BTreeMap::from([(a(1), b(1))]),
        );
        if let Some(n) = spec["cap"].as_f64() {
            f.ledger.configure_cap(key("wheat"), Some(q(n))).unwrap();
        }
        if let Some(n) = spec["pending"].as_f64() {
            f.ledger
                .produce(
                    &b(1),
                    Some(&a(1)),
                    &Stock::default(),
                    &Stock::from_entries([(key("wheat"), q(n))]).unwrap(),
                )
                .unwrap();
        }
        for snapshot in case["snapshots"].as_array().unwrap() {
            let collections = if snapshot["action"] == "grow" {
                f.fields.grow(&f.town, &f.ledger, sec(15.));
                0
            } else {
                count_water(&f.run(1, 30., &water()))
            };
            assert_eq!(
                collections as u64,
                snapshot["collections"].as_u64().unwrap(),
                "{}",
                spec["name"]
            );
            for (i, s) in snapshot["fields"].as_array().unwrap().iter().enumerate() {
                let actual = &f.fields.field(&b(i as u64 + 2)).unwrap().state;
                assert_eq!(
                    actual.stage,
                    stage(s["stage"].as_str().unwrap()),
                    "{}",
                    spec["name"]
                );
                assert_eq!(actual.cycle, s["cycle"].as_u64().unwrap());
                close(actual.progress.get(), s["progress"].as_f64().unwrap());
                close(actual.elapsed.get(), s["elapsed"].as_f64().unwrap());
                if let Some(n) = s["yield_remaining"].as_f64() {
                    close(actual.yield_remaining.unwrap().get(), n);
                }
            }
            for crop in spec["crops"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s.as_str().unwrap())
            {
                close(
                    f.ledger.free(&key(crop)).unwrap().get(),
                    snapshot["free"][crop].as_f64().unwrap(),
                );
                close(
                    f.ledger.produced().quantity(&key(crop)).get(),
                    snapshot["produced"][crop].as_f64().unwrap(),
                );
                let pending = f
                    .ledger
                    .store(&b(1))
                    .unwrap()
                    .pending
                    .get(&PendingOwner::Worker(a(1)))
                    .map_or(0., |s| s.quantity(&key(crop)).get());
                close(pending, snapshot["pending"][crop].as_f64().unwrap_or(0.));
            }
            if let Some(round) = snapshot["round"].as_array() {
                assert!(
                    f.fields
                        .water_ready(&a(1), &b(1), round[3].as_u64().unwrap())
                );
            }
        }
    }
}

#[test]
fn reassigned_farmer_delivers_original_cargo_before_working_at_new_farm() {
    let mut f = Fixture::with(
        &[Stage::Ripe, Stage::Ripe, Stage::Unplowed],
        |v| {
            v[2].farm = Some(b(4));
            v[3].farm = None;
        },
        |s| s[4].policy = StoragePolicy::new("farm"),
        BTreeMap::from([(a(1), b(1))]),
    );
    f.ready(1);
    f.advance(1, 2.);
    f.fields
        .set_assignment(&mut f.town, &f.ledger, &a(1), Some(&b(4)))
        .unwrap();
    f.fields
        .assign(&mut f.town, &f.ledger, &a(1), &b(4), NavigationRevision(1))
        .unwrap();
    assert!(
        matches!(f.town.actor(&a(1)).unwrap().state().task,Task::Work{ref workplace,phase:WorkPhase::Returning,..} if workplace==&b(1))
    );
    f.route(1, RouteOutcome::Arrived);
    assert!(matches!(f.advance(1,0.65),Outcome::Unloaded{workplace,..} if workplace==b(1)));
    close(
        f.ledger
            .store(&b(1))
            .unwrap()
            .output
            .quantity(&key("wheat"))
            .get(),
        10.,
    );
    assert_eq!(
        f.fields
            .assign(&mut f.town, &f.ledger, &a(1), &b(4), NavigationRevision(1))
            .unwrap(),
        Some(b(3))
    );
}
#[test]
fn failed_water_route_can_choose_another_source_and_foreign_wells_are_landscape_facts() {
    let mut f = Fixture::new(&[Stage::Thirsty]);
    f.ready(1);
    f.water(1, 0., &water());
    f.route(1, RouteOutcome::Unreachable);
    let foreign = BuildingId::from_saved(
        EntityIdentity::from_saved(
            Owner {
                world: owner().world,
                town: TownId::new("neighbor").unwrap(),
            },
            70,
        )
        .unwrap(),
    );
    let sources =
        WaterSources::from_landscape(vec![], vec![(foreign.clone(), Tile { x: 9, y: 4 })]);
    let result =
        f.fields
            .advance_with_water(
                &mut f.town,
                &mut f.ledger,
                &a(1),
                &mut f.worker,
                sec(1.),
                NavigationRevision(1),
                &sources,
                |_, banks| {
                    assert!(banks.iter().all(
                        |bank| matches!(&bank.source,WaterSource::Well{id,..} if id==&foreign)
                    ));
                    banks.first().cloned()
                },
                |_, _, s| Some(s),
            )
            .unwrap();
    assert!(matches!(result, Outcome::TravelStarted(_)));
    f.route(1, RouteOutcome::Arrived);
    f.water(1, 0.3, &sources);
    assert_eq!(
        f.water(1, 100., &WaterSources::default()),
        Outcome::Waiting(Wait::Water)
    );
    assert!(!f.fields.water_ready(&a(1), &b(1), 0));
}
#[test]
fn return_arrival_subtracts_movement_and_water_and_harvest_events_are_not_early() {
    let mut f = Fixture::new(&[Stage::Ripe]);
    let events = f.run(1, 1.9, &WaterSources::default());
    assert!(events.iter().all(|(_, e)| !matches!(
        e,
        Outcome::Completed {
            action: Action::Harvest,
            ..
        }
    )));
    let rest = f.run(1, 10., &WaterSources::default());
    let harvest = rest
        .iter()
        .find(|(_, e)| {
            matches!(
                e,
                Outcome::Completed {
                    action: Action::Harvest,
                    ..
                }
            )
        })
        .unwrap();
    close(harvest.0, 0.1); // Completion occurs at exactly two seconds of work.
    let unload = rest
        .iter()
        .find(|(_, e)| matches!(e, Outcome::Unloaded { .. }))
        .unwrap();
    close(unload.0, 0.1 + 1. / 1.8 + 0.65);
}

#[test]
fn failed_collection_completion_rolls_back_round_clock_movement_and_events() {
    let mut f = Fixture::new(&[Stage::Thirsty]);
    f.ready(1);
    f.water(1, 0., &water());
    f.route(1, RouteOutcome::Arrived);
    f.water(1, 0.3, &water());
    let mut actors: Vec<_> = f.town.actors().iter().map(|a| a.state().clone()).collect();
    actors[0].route_generation = RouteGeneration::from_saved(u64::MAX);
    f.town = Town::restore(owner(), f.town.buildings().to_vec(), actors).unwrap();
    let before = (
        f.fields.clone(),
        f.town.clone(),
        f.ledger.clone(),
        f.worker.clone(),
    );
    assert!(
        f.fields
            .advance_with_water(
                &mut f.town,
                &mut f.ledger,
                &a(1),
                &mut f.worker,
                sec(0.35),
                NavigationRevision(1),
                &water(),
                |_, banks| banks.first().cloned(),
                |_, _, s| Some(s)
            )
            .is_err()
    );
    assert_eq!((f.fields, f.town, f.ledger, f.worker), before);
}
