use super::*;
impl Fixture {
    fn reload_fields(&mut self) {
        let json = self.fields.encode(&self.town, &self.ledger).unwrap();
        let mut buildings = self.town.buildings().to_vec();
        buildings.reverse();
        self.town = Town::restore(
            owner(),
            buildings,
            self.town
                .actors()
                .iter()
                .map(|a| a.state().clone())
                .collect(),
        )
        .unwrap();
        self.fields = Fields::decode(&json, &self.town, &self.ledger).unwrap();
    }
}
#[test]
fn reload_every_farming_phase_preserves_rounds_partial_work_and_cargo() {
    let mut f = Fixture::new(&[Stage::Thirsty]);
    f.assign(1);
    f.reload_fields();
    f.route(1, RouteOutcome::Arrived);
    let water = WaterSources::new(vec![Tile { x: 8, y: 4 }], vec![]);
    assert!(matches!(f.water(1, 0., &water), Outcome::TravelStarted(_)));
    f.reload_fields();
    f.route(1, RouteOutcome::Arrived);
    f.water(1, 0.3, &water);
    f.reload_fields();
    assert!(matches!(
        f.water(1, 0.35, &water),
        Outcome::WaterCollected { .. }
    ));
    f.reload_fields();
    f.route(1, RouteOutcome::Arrived);
    f.water(1, 0.75, &WaterSources::default());
    f.reload_fields();
    assert!(matches!(
        f.water(1, 1.25, &WaterSources::default()),
        Outcome::Completed {
            action: Action::Water,
            ..
        }
    ));
    f.fields.grow(&f.town, &f.ledger, sec(15.));
    f.ready(1);
    f.advance(1, 0.5);
    f.reload_fields();
    f.advance(1, 1.5);
    f.assign(1);
    f.reload_fields();
    f.route(1, RouteOutcome::Unreachable);
    f.reload_fields();
    assert_eq!(f.advance(1, 100.), Outcome::Waiting(Wait::Travel));
    f.route(1, RouteOutcome::Arrived);
    f.advance(1, 0.25);
    f.reload_fields();
    assert!(matches!(f.advance(1, 0.4), Outcome::Unloaded { .. }));
    f.reload_fields();
    close(f.ledger.free(&key("wheat")).unwrap().get(), 10.);
    close(f.ledger.produced().quantity(&key("wheat")).get(), 10.);
}
#[test]
fn crop_edits_keep_sown_yields_work_and_pending_crops() {
    let mut f = Fixture::new(&[Stage::Ripe, Stage::Unplowed]);
    let before = f.fields.clone();
    assert!(
        f.fields
            .set_crop(&f.town, &f.ledger, &b(1), "fodder", 1)
            .is_err()
    );
    assert_eq!(f.fields, before);
    f.fields
        .set_crop(&f.town, &f.ledger, &b(1), "carrots", 10)
        .unwrap();
    assert_eq!(
        f.fields.field(&b(2)).unwrap().state.crop,
        Some(key("wheat"))
    );
    assert_eq!(f.fields.field(&b(3)).unwrap().selected_crop, key("carrots"));
    // Existing ripe crop stays wheat even when its next selection is carrots.
    f.fields
        .set_assignment(&mut f.town, &f.ledger, &a(1), Some(&b(2)))
        .unwrap();
    f.fields
        .assign(&mut f.town, &f.ledger, &a(1), &b(2), NavigationRevision(1))
        .unwrap();
    f.route(1, RouteOutcome::Arrived);
    f.advance(1, 2.);
    f.reload_fields();
    close(f.ledger.produced().quantity(&key("wheat")).get(), 10.);
    assert_eq!(f.ledger.produced().quantity(&key("carrots")), q(0.));
    f.fields
        .set_crop(&f.town, &f.ledger, &b(2), "vegetables", 10)
        .unwrap();
    close(
        f.ledger.store(&b(2)).unwrap().pending[&PendingOwner::Worker(a(1))]
            .quantity(&key("wheat"))
            .get(),
        10.,
    );
}
fn moved(f: &mut Fixture, dx: i32, dy: i32) {
    let before = f.town.clone();
    let mut after = f.town.clone();
    for b in before.buildings() {
        after
            .relocate_building(
                &b.id,
                Tile {
                    x: b.location.x + dx,
                    y: b.location.y + dy,
                },
            )
            .unwrap();
    }
    f.fields
        .reconcile_relocation(&before, &mut after, &f.ledger, NavigationRevision(20))
        .unwrap();
    f.town = after;
}
#[test]
fn linked_move_during_collection_preserves_can_clock_and_retargets_return() {
    let mut f = Fixture::new(&[Stage::Thirsty]);
    f.ready(1);
    let water = WaterSources::new(vec![Tile { x: 8, y: 4 }], vec![]);
    f.water(1, 0., &water);
    f.route(1, RouteOutcome::Arrived);
    f.water(1, 0.3, &water);
    let position = f.town.actor(&a(1)).unwrap().state().position;
    moved(&mut f, 2, 1);
    assert_eq!(f.town.actor(&a(1)).unwrap().state().position, position);
    f.reload_fields();
    assert!(matches!(
        f.water(1, 0.35, &water),
        Outcome::WaterCollected { .. }
    ));
    f.reload_fields();
    let TravelIntent::Active(Destination::Position(p)) =
        f.town.actor(&a(1)).unwrap().state().travel
    else {
        panic!("water return")
    };
    assert_eq!(p, pos(4.55, 5.1));
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
fn moved_partial_field_work_and_harvest_returns_are_coherent() {
    let mut f = Fixture::new(&[Stage::Unplowed]);
    f.ready(1);
    f.advance(1, 0.75);
    moved(&mut f, 2, 1);
    f.reload_fields();
    f.route(1, RouteOutcome::Arrived);
    let Outcome::Completed { remaining, .. } = f.advance(1, 2.) else {
        panic!("plow")
    };
    close(remaining.get(), 0.75);
    let mut f = Fixture::new(&[Stage::Ripe]);
    f.ready(1);
    f.advance(1, 2.);
    f.assign(1);
    moved(&mut f, 2, 1);
    f.reload_fields();
    f.route(1, RouteOutcome::Arrived);
    assert!(matches!(f.advance(1, 0.65), Outcome::Unloaded { .. }));
    close(f.ledger.free(&key("wheat")).unwrap().get(), 10.);
}
#[test]
fn strict_field_reload_rejects_duplicate_foreign_incoherent_and_unknown_facts() {
    let mut f = Fixture::new(&[Stage::Thirsty]);
    f.ready(1);
    let original = f.fields.encode(&f.town, &f.ledger).unwrap();
    for mode in 0..8 {
        let mut v: serde_json::Value = serde_json::from_str(&original).unwrap();
        match mode {
            0 => v["version"] = 99.into(),
            1 => v["world"] = "foreign".into(),
            2 => {
                let c = v["claims"][0].clone();
                v["claims"].as_array_mut().unwrap().push(c);
            }
            3 => v["claims"][0]["clock"] = 1.into(),
            4 => v["fields"][1]["stage"] = "unknown".into(),
            5 => {
                v["fields"][1].as_object_mut().unwrap().remove("crop");
            }
            6 => v["claims"][0]["destination"]["position"]["position"]["x"] = 99.into(),
            7 => v["extra"] = true.into(),
            _ => unreachable!(),
        };
        assert!(
            Fields::decode(&v.to_string(), &f.town, &f.ledger).is_err(),
            "mode {mode}"
        );
    }
}
#[test]
fn legacy_field_facts_preserve_sown_crop_partial_yield_and_explicit_links() {
    let f = Fixture::new(&[Stage::Ripe]);
    let raw = vec![
        serde_json::json!({"kind":"farm"}),
        serde_json::json!({"kind":"acre","farm_index":0,"crop":"carrots","field":{"stage":"ripe","elapsed":0,"progress":2,"cycle":7,"crop":"wheat","yield_remaining":3.5}}),
    ];
    let fields =
        Fields::admit_legacy(&f.town, &f.ledger, &raw, &[b(1), b(2)], BTreeMap::new()).unwrap();
    let acre = fields.field(&b(2)).unwrap();
    assert_eq!(acre.farm, Some(b(1)));
    assert_eq!(acre.selected_crop, key("carrots"));
    assert_eq!(acre.state.crop, Some(key("wheat")));
    close(acre.state.yield_remaining.unwrap().get(), 3.5);
    let mut bad = raw.clone();
    bad[1]["farm_index"] = true.into();
    assert!(
        Fields::admit_legacy(&f.town, &f.ledger, &bad, &[b(1), b(2)], BTreeMap::new()).is_err()
    );
}

#[test]
fn paired_python_farm_and_acre_crop_edit_scope() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/farming/orchard_cases.json")).unwrap();
    for c in data["edits"].as_array().unwrap() {
        let mut f = Fixture::with(
            &[Stage::Ripe, Stage::Unplowed],
            |v| {
                v[1].state.progress = sec(2.);
                v[1].state.cycle = 5;
                v[1].state.yield_remaining = Some(q(3.));
                v[2].selected_crop = key("vegetables");
            },
            |_| {},
            BTreeMap::new(),
        );
        f.fields
            .set_crop(
                &f.town,
                &f.ledger,
                &b(c["target"].as_u64().unwrap()),
                "carrots",
                10,
            )
            .unwrap();
        f.reload_fields();
        for (i, crop) in c["crops"].as_array().unwrap().iter().enumerate() {
            assert_eq!(
                f.fields.field(&b(i as u64 + 1)).unwrap().selected_crop,
                key(crop.as_str().unwrap())
            );
        }
        let state = &f.fields.field(&b(2)).unwrap().state;
        assert_eq!(state.crop, Some(key("wheat")));
        assert_eq!(state.yield_remaining, Some(q(3.)));
    }
}

#[test]
fn legacy_can_rounds_and_old_filled_animations_upgrade_without_recollecting() {
    let mut f = Fixture::new(&[Stage::Thirsty]);
    let raw = vec![
        serde_json::json!({"work_animation":{"waterReady":true,"task":["farming","farm",3,3,"acre",3,4,"field_cycle",0]}}),
    ];
    f.fields
        .admit_legacy_rounds(&f.town, &f.ledger, &raw, &[a(1)])
        .unwrap();
    assert!(f.fields.water_ready(&a(1), &b(1), 0));
    f.reload_fields();
    f.ready(1);
    assert!(matches!(
        f.advance(1, 2.),
        Outcome::Completed {
            action: Action::Water,
            ..
        }
    ));
    let mut bad = raw;
    bad[0]["watering_round"] = serde_json::json!(["farm", 3, 3, true]);
    let before = f.fields.clone();
    assert!(
        f.fields
            .admit_legacy_rounds(&f.town, &f.ledger, &bad, &[a(1)])
            .is_err()
    );
    assert_eq!(f.fields, before);
}

#[test]
fn move_between_completed_harvest_and_return_planning_retargets_saved_history() {
    let mut f = Fixture::new(&[Stage::Ripe]);
    f.ready(1);
    f.advance(1, 2.);
    moved(&mut f, 2, 1);
    f.reload_fields();
    f.assign(1);
    f.reload_fields();
    f.route(1, RouteOutcome::Arrived);
    assert!(matches!(f.advance(1, 0.65), Outcome::Unloaded { .. }));
    close(f.ledger.free(&key("wheat")).unwrap().get(), 10.);
}
