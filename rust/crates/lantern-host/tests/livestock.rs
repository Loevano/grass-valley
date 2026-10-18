use lantern_core::{
    ids::{ActorId, BuildingId, EntityIdentity, Owner, TownId, WorldId},
    model::*,
    search::{Outcome as SearchOutcome, Search, Terrain},
    spatial::*,
    units::*,
};
use lantern_host::{
    dto::{decode_save, encode_save},
    livestock::*,
    progression::Worker,
    resources::*,
    state::{ModelWorld, PrivateCredentials},
};
use std::{collections::BTreeMap, rc::Rc};
fn owner() -> Owner {
    Owner {
        world: WorldId::new("herds").unwrap(),
        town: TownId::new("one").unwrap(),
    }
}
fn a(n: u64) -> ActorId {
    ActorId::from_saved(EntityIdentity::from_saved(owner(), 100 + n).unwrap())
}
fn b(n: u64) -> BuildingId {
    BuildingId::from_saved(EntityIdentity::from_saved(owner(), n).unwrap())
}
fn t(x: i32, y: i32) -> Tile {
    Tile { x, y }
}
fn p(x: f64, y: f64) -> Position {
    Position::new(x, y).unwrap()
}
fn q(n: f64) -> Quantity {
    Quantity::new(n).unwrap()
}
fn sec(n: f64) -> Seconds {
    Seconds::new(n).unwrap()
}
fn key(s: &str) -> ResourceKey {
    ResourceKey::parse(s).unwrap()
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}
fn species(s: &str) -> Species {
    match s {
        "chicken" => Species::Chicken,
        "sheep" => Species::Sheep,
        "pig" => Species::Pig,
        "cow" => Species::Cow,
        _ => panic!("species"),
    }
}
fn data() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/livestock/python.json")).unwrap()
}
fn herd(species: Species, ages: Vec<f64>) -> HerdInput {
    HerdInput {
        species,
        count: ages.len(),
        ages,
        names: vec![],
    }
}
struct Fixture {
    town: Town,
    ledger: Ledger,
    stock: Livestock,
    worker: Worker,
    extra: Vec<Resource>,
    fences: bool,
    shop_size: (u32, u32),
    nav: u64,
}
impl Fixture {
    fn new(herds: Vec<HerdInput>) -> Self {
        let town = Town::restore(
            owner(),
            vec![
                Building {
                    id: b(1),
                    location: t(12, 12),
                    construction: ConstructionState::Complete,
                },
                Building {
                    id: b(2),
                    location: t(5, 12),
                    construction: ConstructionState::Complete,
                },
            ],
            (1..=8)
                .map(|i| ActorState::new(a(i), p(5., 11.), Cargo::default()))
                .collect(),
        )
        .unwrap();
        let ledger = Ledger::new(
            owner(),
            q(100.),
            vec![
                Store::new(b(1), StoragePolicy::new("barn")),
                Store::new(b(2), StoragePolicy::new("butcher")),
            ],
            vec![],
        )
        .unwrap();
        let stock = Livestock::admit(&town, &ledger, vec![(b(1), herds)], 0).unwrap();
        let mut worker = Worker::new("butchery").unwrap();
        worker
            .set_practice("husbandry", Practice::new(2.).unwrap())
            .unwrap();
        Self {
            town,
            ledger,
            stock,
            worker,
            extra: vec![],
            fences: false,
            shop_size: (1, 1),
            nav: 1,
        }
    }
    fn scene(&self) -> Scene {
        let mut fps = BTreeMap::new();
        let structures = self
            .town
            .buildings()
            .iter()
            .map(|b| {
                let size = if b.id == self::b(2) {
                    Some(self.shop_size)
                } else {
                    None
                };
                let footprint = Footprint::saved(b.location, size).unwrap();
                fps.insert(b.id.clone(), footprint);
                Structure {
                    footprint,
                    kind: if b.id == self::b(2) {
                        "butcher"
                    } else if self.fences {
                        "barn"
                    } else {
                        "home"
                    }
                    .into(),
                    construction: Construction::Complete,
                    fence_orientation: 0,
                }
            })
            .collect::<Vec<_>>();
        Scene {
            index: Rc::new(SpatialIndex::derive(&structures, &self.extra, &[], &[], 32).unwrap()),
            terrain: Terrain::default(),
            footprints: fps,
        }
    }
    fn roster(&self, i: u64) -> Vec<(ActorId, Worker)> {
        vec![(a(i), self.worker.clone())]
    }
    fn start(&mut self, s: Species) {
        let scene = self.scene();
        let roster = self.roster(1);
        self.stock
            .butcher_animal(
                &mut self.town,
                &self.ledger,
                &scene,
                &b(2),
                &b(1),
                s,
                &roster,
                NavigationRevision(self.nav),
            )
            .unwrap();
    }
    fn resolve_butcher(&mut self, i: u64) {
        self.town
            .resume_travel(&a(i), NavigationRevision(self.nav))
            .unwrap();
        let RouteState::Pending(r) = self.town.actor(&a(i)).unwrap().route().clone() else {
            return;
        };
        let mut search = Search::new(Rc::new(
            self.stock
                .butcher_query(&self.town, &self.scene(), &b(2), &a(i))
                .unwrap(),
        ));
        while search.outcome() == &SearchOutcome::Pending {
            search.advance(500);
        }
        let result = match search.into_outcome() {
            SearchOutcome::Arrived => RouteOutcome::Arrived,
            SearchOutcome::Unreachable => RouteOutcome::Unreachable,
            SearchOutcome::Completed { path, .. } => RouteOutcome::Ready(
                path.into_iter()
                    .map(|t| p(t.x as f64, t.y as f64))
                    .collect(),
            ),
            _ => panic!("search"),
        };
        self.stock
            .accept_butcher_route(&mut self.town, &b(2), &a(i), &r, r.navigation(), result)
            .unwrap();
    }
    fn advance(&mut self, i: u64, n: f64) -> Outcome {
        let mut remaining = n;
        for _ in 0..5 {
            if self.stock.order(&b(2)).unwrap().is_some_and(|o| {
                matches!(
                    o.leg(),
                    ButcherLeg::Pickup | ButcherLeg::Drag | ButcherLeg::ToShop
                )
            }) {
                self.resolve_butcher(i);
            }
            let scene = self.scene();
            let result = self
                .stock
                .advance_butcher(
                    &mut self.town,
                    &mut self.ledger,
                    &scene,
                    &b(2),
                    &a(i),
                    &mut self.worker,
                    sec(remaining),
                    NavigationRevision(self.nav),
                    |_, _, s| Some(s),
                )
                .unwrap();
            if let Outcome::RouteRequested(rest) = result {
                remaining = rest.get();
            } else {
                return result;
            }
        }
        panic!("legs");
    }
    fn reach(&mut self, leg: ButcherLeg) {
        for _ in 0..1000 {
            if self
                .stock
                .order(&b(2))
                .unwrap()
                .is_some_and(|o| o.leg() == leg)
            {
                return;
            }
            self.advance(1, 0.1);
        }
        panic!("leg {leg:?}");
    }
    fn reload(&mut self) {
        let s = self.stock.encode(&self.town).unwrap();
        let world = ModelWorld::new(
            owner().world,
            TerrainSeed(0),
            108,
            vec![self.town.clone()],
            PrivateCredentials::default(),
        )
        .unwrap();
        let loaded = decode_save(&encode_save(&world).unwrap()).unwrap();
        let town = &loaded.towns()[0];
        self.town = Town::restore(
            town.owner().clone(),
            town.buildings().to_vec(),
            town.actors().iter().map(|a| a.state().clone()).collect(),
        )
        .unwrap();
        self.stock = Livestock::decode(&s, &self.town, &self.ledger).unwrap();
    }
    fn keeper_roster(&self, n: usize) -> Vec<(ActorId, Worker)> {
        (1..=n as u64)
            .map(|i| (a(i), Worker::new("husbandry").unwrap()))
            .collect()
    }
}
#[test]
fn python_growth_rates_clipped_outputs_maturity_and_independent_clock() {
    for c in data()["growth"].as_array().unwrap() {
        let s = species(c["species"].as_str().unwrap());
        let mut f = Fixture::new(vec![herd(s, vec![c["age"].as_f64().unwrap()])]);
        let n = c["keepers"].as_u64().unwrap() as usize;
        let roster = f.keeper_roster(n);
        f.stock
            .set_keepers(
                &mut f.town,
                &f.ledger,
                &b(1),
                &roster,
                NavigationRevision(1),
            )
            .unwrap();
        let room = c["room"].as_f64().unwrap();
        if room < 100. {
            let product = match s {
                Species::Chicken => "eggs",
                Species::Sheep => "wool",
                Species::Pig => "meat",
                Species::Cow => "milk",
            };
            f.ledger.configure_cap(key(product), Some(q(room))).unwrap();
            f.ledger
                .configure_cap(key("manure"), Some(q(room)))
                .unwrap();
        }
        f.stock
            .grow(
                &f.town,
                &mut f.ledger,
                &b(1),
                sec(c["elapsed"].as_f64().unwrap()),
                false,
                1,
            )
            .unwrap();
        close(
            f.stock.animals(&b(1), s).unwrap()[0].age().get(),
            c["grown"].as_f64().unwrap(),
        );
        for k in ["eggs", "wool", "milk", "meat", "manure"] {
            let expected = c["output"][k].as_f64().unwrap_or(0.);
            close(
                f.ledger
                    .store(&b(1))
                    .unwrap()
                    .output
                    .quantity(&key(k))
                    .get(),
                expected,
            );
        }
        f.reload();
        let before = f.ledger.clone();
        f.stock
            .grow(&f.town, &mut f.ledger, &b(1), sec(3.), false, 1)
            .unwrap();
        assert_eq!(f.ledger, before);
    }
}
#[test]
fn python_names_and_count_prefixes_survive_clear_and_reload() {
    let mut f = Fixture::new(vec![
        herd(Species::Chicken, vec![10., 60.]),
        herd(Species::Sheep, vec![40.]),
    ]);
    for c in data()["names"].as_array().unwrap() {
        if c["op"] == "count" {
            f.stock
                .set_count(
                    &f.town,
                    &f.ledger,
                    &b(1),
                    Species::Chicken,
                    c["count"].as_u64().unwrap() as usize,
                    5,
                )
                .unwrap();
        }
        for s in [Species::Chicken, Species::Sheep] {
            let animals = f.stock.animals(&b(1), s).unwrap();
            let names = c["state"]["names"][s.name()].as_array().unwrap();
            let ages = c["state"]["ages"][s.name()].as_array().unwrap();
            assert_eq!(animals.len(), names.len());
            for (i, a) in animals.iter().enumerate() {
                assert_eq!(a.name(), names[i].as_str().unwrap());
                close(a.age().get(), ages[i].as_f64().unwrap());
            }
        }
        f.reload();
    }
}
#[test]
fn python_last_species_replacement_and_selected_names() {
    for s in [Species::Chicken, Species::Sheep, Species::Pig, Species::Cow] {
        let mut f = Fixture::new(vec![herd(s, vec![s.duration(); 3])]);
        for c in data()["replacement"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["species"] == s.name())
        {
            f.start(s);
            assert_eq!(
                f.stock.order(&b(2)).unwrap().unwrap().name(),
                c["name"].as_str().unwrap()
            );
            let animals = f.stock.animals(&b(1), s).unwrap();
            let expected = &c["state"];
            assert_eq!(
                animals.len(),
                expected["animals"][s.name()].as_u64().unwrap() as usize
            );
            for (i, a) in animals.iter().enumerate() {
                assert_eq!(a.name(), expected["names"][s.name()][i].as_str().unwrap());
                close(
                    a.age().get(),
                    expected["ages"][s.name()][i].as_f64().unwrap(),
                );
            }
            f.reload();
            for _ in 0..30 {
                if f.stock.order(&b(2)).unwrap().is_none() {
                    break;
                }
                f.advance(1, 20.);
            }
            assert!(f.stock.order(&b(2)).unwrap().is_none());
            let costs = Stock::from_entries(
                ["meat", "hides"]
                    .into_iter()
                    .map(|k| (key(k), f.ledger.free(&key(k)).unwrap())),
            )
            .unwrap();
            f.ledger.spend(&costs).unwrap();
            f.reload();
        }
    }
}
#[test]
fn walk_pickup_drag_and_process_are_host_clocks_with_exact_once_rewards() {
    let mut f = Fixture::new(vec![herd(Species::Pig, vec![240.])]);
    f.shop_size = (2, 2);
    f.fences = true;
    f.start(Species::Pig);
    let replacement = f.stock.animals(&b(1), Species::Pig).unwrap()[0].id();
    for leg in [
        ButcherLeg::Pickup,
        ButcherLeg::Finding,
        ButcherLeg::Drag,
        ButcherLeg::Processing,
    ] {
        f.reach(leg);
        f.reload();
        if leg != ButcherLeg::Processing {
            close(
                f.stock.order(&b(2)).unwrap().unwrap().remaining().get(),
                60.,
            );
        }
    }
    let name = f.stock.animals(&b(1), Species::Pig).unwrap()[0]
        .name()
        .to_owned();
    for _ in 0..21 {
        f.advance(1, 3.);
    }
    assert!(f.stock.order(&b(2)).unwrap().is_none());
    close(f.ledger.free(&key("meat")).unwrap().get(), 8.);
    close(f.ledger.free(&key("hides")).unwrap().get(), 2.);
    assert_eq!(
        f.stock.animals(&b(1), Species::Pig).unwrap()[0].id(),
        replacement
    );
    assert_eq!(
        f.stock.animals(&b(1), Species::Pig).unwrap()[0].name(),
        name
    );
    let skill = f.worker.practice_value("butchery");
    f.advance(1, 100.);
    close(f.worker.practice_value("butchery"), skill);
    close(f.ledger.free(&key("meat")).unwrap().get(), 8.);
}
impl Fixture {
    fn resolve_keeper(&mut self, i: u64) {
        self.town
            .resume_travel(&a(i), NavigationRevision(self.nav))
            .unwrap();
        let RouteState::Pending(r) = self.town.actor(&a(i)).unwrap().route().clone() else {
            return;
        };
        let mut search = Search::new(Rc::new(
            self.stock
                .keeper_query(&self.town, &self.scene(), &a(i))
                .unwrap(),
        ));
        while search.outcome() == &SearchOutcome::Pending {
            search.advance(500);
        }
        let result = match search.into_outcome() {
            SearchOutcome::Arrived => RouteOutcome::Arrived,
            SearchOutcome::Unreachable => RouteOutcome::Unreachable,
            SearchOutcome::Completed { path, .. } => RouteOutcome::Ready(
                path.into_iter()
                    .map(|t| p(t.x as f64, t.y as f64))
                    .collect(),
            ),
            _ => panic!("search"),
        };
        self.town
            .accept_route(&a(i), &r, r.navigation(), result)
            .unwrap();
    }
    fn keeper(&mut self, i: u64, n: f64) -> Outcome {
        let mut remaining = n;
        for _ in 0..4 {
            if matches!(
                self.town.actor(&a(i)).unwrap().state().task,
                Task::Work {
                    phase: WorkPhase::Outbound | WorkPhase::Returning,
                    ..
                }
            ) {
                self.resolve_keeper(i);
            }
            let scene = self.scene();
            let result = self
                .stock
                .advance_keeper(
                    &mut self.town,
                    &scene,
                    &a(i),
                    sec(remaining),
                    NavigationRevision(self.nav),
                    |_, _, s| Some(s),
                )
                .unwrap();
            match result {
                Outcome::RouteRequested(r) | Outcome::Unloaded(r) if r.get() > 1e-9 => {
                    remaining = r.get()
                }
                other => return other,
            }
        }
        Outcome::Advanced
    }
}
#[test]
fn growth_continues_with_paused_work_full_output_no_feed_and_upgrades() {
    let mut f = Fixture::new(vec![herd(Species::Sheep, vec![0.])]);
    let roster = f.keeper_roster(3);
    f.stock
        .set_keepers(
            &mut f.town,
            &f.ledger,
            &b(1),
            &roster,
            NavigationRevision(1),
        )
        .unwrap();
    let mut stores = f.ledger.stores().to_vec();
    stores[0].inputs = Stock::from_entries([(key("fodder"), q(40.))]).unwrap();
    f.ledger = Ledger::new(owner(), q(100.), stores, vec![]).unwrap();
    f.ledger.configure_cap(key("wool"), Some(q(0.))).unwrap();
    f.stock
        .grow(&f.town, &mut f.ledger, &b(1), sec(3.), false, 1)
        .unwrap();
    close(
        f.stock.animals(&b(1), Species::Sheep).unwrap()[0]
            .age()
            .get(),
        12.,
    );
    close(
        f.ledger
            .store(&b(1))
            .unwrap()
            .inputs
            .quantity(&key("fodder"))
            .get(),
        40.,
    );
    close(f.ledger.free(&key("wool")).unwrap().get(), 0.);
    let mut policy = f.ledger.store(&b(1)).unwrap().policy.clone();
    policy.construction = true;
    f.ledger.update_storage(&b(1), policy).unwrap();
    f.town
        .set_construction(
            &b(1),
            ConstructionState::InProgress {
                remaining: Work::new(30.).unwrap(),
            },
        )
        .unwrap();
    f.stock
        .grow(&f.town, &mut f.ledger, &b(1), sec(3.), true, 2)
        .unwrap();
    close(
        f.stock.animals(&b(1), Species::Sheep).unwrap()[0]
            .age()
            .get(),
        12.,
    );
    f.ledger.configure_cap(key("wool"), None).unwrap();
    f.stock
        .grow(&f.town, &mut f.ledger, &b(1), sec(3.), false, 3)
        .unwrap();
    close(
        f.stock.animals(&b(1), Species::Sheep).unwrap()[0]
            .age()
            .get(),
        24.,
    );
    assert!(f.ledger.store(&b(1)).unwrap().policy.construction);
    assert!(
        f.ledger
            .store(&b(1))
            .unwrap()
            .output
            .quantity(&key("wool"))
            .get()
            > 0.
    );
    f.reload();
}
#[test]
fn keepers_reserve_distinct_pen_rounds_without_growing_or_mutating_stock() {
    let mut f = Fixture::new(vec![herd(Species::Chicken, vec![30., 60.])]);
    f.fences = true;
    for i in 1..=6 {
        f.town.reposition_actor(&a(i), p(11., 12.)).unwrap();
    }
    let roster = f.keeper_roster(6);
    f.stock
        .set_keepers(
            &mut f.town,
            &f.ledger,
            &b(1),
            &roster,
            NavigationRevision(1),
        )
        .unwrap();
    let animals = f.stock.animals(&b(1), Species::Chicken).unwrap().to_vec();
    let ledger = f.ledger.clone();
    let mut targets = std::collections::BTreeSet::new();
    let mut saw_deposit = false;
    for tick in 0..350 {
        let mut active = std::collections::BTreeSet::new();
        for i in 1..=6 {
            f.keeper(i, 0.2);
            let state = f.town.actor(&a(i)).unwrap().state();
            assert!(
                (state.position.x.get() - 12.).abs() <= 3.
                    && (state.position.y.get() - 12.).abs() <= 3.
            );
            if let Task::Work {
                phase,
                target: Destination::Position(at),
                ..
            } = state.task
                && matches!(phase, WorkPhase::Outbound | WorkPhase::Working)
            {
                let tile = (at.x.get() as i32, at.y.get() as i32);
                assert!(active.insert(tile));
                targets.insert(tile);
            }
            saw_deposit |= matches!(
                state.task,
                Task::Work {
                    phase: WorkPhase::Depositing,
                    ..
                }
            );
        }
        if tick % 75 == 0 {
            f.reload();
        }
    }
    assert!(saw_deposit);
    assert!(targets.len() > 3);
    assert_eq!(f.ledger, ledger);
    assert_eq!(f.stock.animals(&b(1), Species::Chicken).unwrap(), animals);
}
#[test]
fn keeper_practice_and_growth_have_separate_exact_once_clocks() {
    let mut f = Fixture::new(vec![herd(Species::Chicken, vec![60.])]);
    let roster = f.keeper_roster(1);
    f.stock
        .set_keepers(
            &mut f.town,
            &f.ledger,
            &b(1),
            &roster,
            NavigationRevision(1),
        )
        .unwrap();
    let mut worker = roster[0].1.clone();
    assert!(
        !f.stock
            .practice_keeper(&f.town, &a(1), &mut worker, 1)
            .unwrap()
    );
    f.resolve_keeper(1);
    assert!(
        f.stock
            .practice_keeper(&f.town, &a(1), &mut worker, 2)
            .unwrap()
    );
    close(worker.practice_value("husbandry"), 0.128);
    f.reload();
    assert!(
        !f.stock
            .practice_keeper(&f.town, &a(1), &mut worker, 2)
            .unwrap()
    );
    let before = f.ledger.clone();
    f.stock
        .grow(&f.town, &mut f.ledger, &b(1), sec(20.), false, 1)
        .unwrap();
    assert_eq!(f.ledger, before);
    f.town.set_task(&a(1), Task::Idle).unwrap();
    f.stock
        .set_keepers(
            &mut f.town,
            &f.ledger,
            &b(1),
            &roster,
            NavigationRevision(1),
        )
        .unwrap();
    assert!(matches!(
        f.town.actor(&a(1)).unwrap().state().task,
        Task::Work { .. }
    ));
}
#[test]
fn blocked_dragging_freezes_processing_and_resumes_after_reload() {
    let mut f = Fixture::new(vec![herd(Species::Cow, vec![360.])]);
    f.start(Species::Cow);
    f.reach(ButcherLeg::Drag);
    f.extra = (0..32)
        .map(|y| Resource {
            footprint: Footprint::saved(t(8, y), None).unwrap(),
            kind: "rock".into(),
        })
        .collect();
    for _ in 0..50 {
        f.advance(1, 0.2);
    }
    let position = f.town.actor(&a(1)).unwrap().state().position;
    close(
        f.stock.order(&b(2)).unwrap().unwrap().remaining().get(),
        60.,
    );
    close(f.ledger.free(&key("meat")).unwrap().get(), 0.);
    f.reload();
    f.advance(1, 10.);
    assert_eq!(f.town.actor(&a(1)).unwrap().state().position, position);
    f.extra.clear();
    f.nav = 2;
    for _ in 0..100 {
        if matches!(f.advance(1, 2.), Outcome::Completed) {
            break;
        }
    }
    assert!(f.stock.order(&b(2)).unwrap().is_none());
    close(f.ledger.free(&key("meat")).unwrap().get(), 10.);
}
#[test]
fn replacement_collects_the_saved_fractional_drag_position() {
    let mut f = Fixture::new(vec![herd(Species::Pig, vec![240.])]);
    f.start(Species::Pig);
    f.reach(ButcherLeg::Drag);
    f.advance(1, 0.37);
    let at = f.stock.order(&b(2)).unwrap().unwrap().position().unwrap();
    assert!(at.x.get().fract() != 0. || at.y.get().fract() != 0.);
    let moved = Task::Move {
        destination: p(20., 20.),
    };
    f.town.set_task(&a(1), moved.clone()).unwrap();
    let roster = f.roster(2);
    let scene = f.scene();
    f.stock
        .reconcile_butchers(
            &mut f.town,
            &f.ledger,
            &scene,
            &b(2),
            &roster,
            NavigationRevision(1),
        )
        .unwrap();
    f.reload();
    let mut found = false;
    for _ in 0..200 {
        f.advance(2, 0.1);
        if f.stock.order(&b(2)).unwrap().unwrap().leg() == ButcherLeg::Finding {
            assert_eq!(f.town.actor(&a(2)).unwrap().state().position, at);
            found = true;
            break;
        }
    }
    assert!(found);
    assert_eq!(f.town.actor(&a(1)).unwrap().state().task, moved);
    for _ in 0..100 {
        if matches!(f.advance(2, 2.), Outcome::Completed) {
            break;
        }
    }
    close(f.ledger.free(&key("meat")).unwrap().get(), 8.);
}
#[test]
fn processing_waits_at_zero_for_meat_and_hides_room_and_helpers_do_not_accelerate() {
    let mut f = Fixture::new(vec![herd(Species::Pig, vec![240.])]);
    f.start(Species::Pig);
    f.reach(ButcherLeg::Processing);
    let scene = f.scene();
    let mut helper = f.worker.clone();
    let before = f.stock.order(&b(2)).unwrap().unwrap().remaining();
    assert_eq!(
        f.stock
            .advance_butcher(
                &mut f.town,
                &mut f.ledger,
                &scene,
                &b(2),
                &a(2),
                &mut helper,
                sec(60.),
                NavigationRevision(1),
                |_, _, s| Some(s)
            )
            .unwrap(),
        Outcome::Waiting
    );
    assert_eq!(f.stock.order(&b(2)).unwrap().unwrap().remaining(), before);
    f.ledger.configure_cap(key("hides"), Some(q(0.))).unwrap();
    f.advance(1, 70.);
    close(f.stock.order(&b(2)).unwrap().unwrap().remaining().get(), 0.);
    close(f.ledger.free(&key("meat")).unwrap().get(), 0.);
    f.reload();
    f.ledger.configure_cap(key("hides"), None).unwrap();
    assert_eq!(f.advance(1, 0.), Outcome::Completed);
    close(f.worker.practice_value("butchery"), 2.);
    close(helper.practice_value("butchery"), 0.);
}
#[test]
fn moved_barn_and_shop_keep_named_orders_ages_and_actor_positions() {
    let mut f = Fixture::new(vec![herd(Species::Sheep, vec![120.])]);
    f.start(Species::Sheep);
    let name = f.stock.order(&b(2)).unwrap().unwrap().name().to_owned();
    let before = f.town.actor(&a(1)).unwrap().state().position;
    f.town.relocate_building(&b(1), t(14, 14)).unwrap();
    let roster = f.roster(1);
    let scene = f.scene();
    f.stock
        .reconcile_butchers(
            &mut f.town,
            &f.ledger,
            &scene,
            &b(2),
            &roster,
            NavigationRevision(1),
        )
        .unwrap();
    assert_eq!(f.town.actor(&a(1)).unwrap().state().position, before);
    f.reload();
    f.reach(ButcherLeg::Processing);
    f.advance(1, 10.);
    let remaining = f.stock.order(&b(2)).unwrap().unwrap().remaining();
    let before = f.town.actor(&a(1)).unwrap().state().position;
    f.town.relocate_building(&b(2), t(3, 8)).unwrap();
    let scene = f.scene();
    f.stock
        .reconcile_butchers(
            &mut f.town,
            &f.ledger,
            &scene,
            &b(2),
            &roster,
            NavigationRevision(1),
        )
        .unwrap();
    assert_eq!(f.town.actor(&a(1)).unwrap().state().position, before);
    assert_eq!(
        f.stock.order(&b(2)).unwrap().unwrap().remaining(),
        remaining
    );
    assert_eq!(f.stock.order(&b(2)).unwrap().unwrap().name(), name);
    f.reload();
    for _ in 0..100 {
        if matches!(f.advance(1, 2.), Outcome::Completed) {
            break;
        }
    }
    close(f.ledger.free(&key("meat")).unwrap().get(), 4.);
}
#[test]
fn python_manual_rewards_and_blocked_completions_match() {
    for c in data()["rewards"].as_array().unwrap() {
        let s = species(c["species"].as_str().unwrap());
        let mut f = Fixture::new(vec![herd(s, vec![s.duration()])]);
        f.start(s);
        f.reach(ButcherLeg::Processing);
        if c["cap"] == true {
            f.ledger.configure_cap(key("meat"), Some(q(0.))).unwrap();
            f.ledger.configure_cap(key("hides"), Some(q(0.))).unwrap();
        }
        for _ in 0..20 {
            f.advance(1, 3.);
        }
        for k in ["meat", "hides"] {
            close(
                f.ledger
                    .store(&b(2))
                    .unwrap()
                    .output
                    .quantity(&key(k))
                    .get(),
                c["output"][k].as_f64().unwrap_or(0.),
            );
        }
        close(
            f.worker.practice_value("butchery"),
            c["practice"].as_f64().unwrap(),
        );
        assert_eq!(
            f.stock.order(&b(2)).unwrap().is_some(),
            c["pending"].as_bool().unwrap()
        );
        f.reload();
    }
}
#[test]
fn count_unlock_capacity_buy_and_rejected_butchery_are_atomic() {
    let mut f = Fixture::new(vec![herd(Species::Chicken, vec![12., 15.])]);
    let before = f.stock.clone();
    for (s, n, hall) in [
        (Species::Sheep, 1, 2),
        (Species::Pig, 3, 5),
        (Species::Chicken, usize::MAX, 5),
    ] {
        assert!(
            f.stock
                .set_count(&f.town, &f.ledger, &b(1), s, n, hall)
                .is_err()
        );
        assert_eq!(f.stock, before);
    }
    f.stock
        .buy_animal(&f.town, &mut f.ledger, &b(1), Species::Pig, 5)
        .unwrap();
    close(
        f.stock.animals(&b(1), Species::Pig).unwrap()[0].age().get(),
        0.,
    );
    close(f.ledger.free(&key("coins")).unwrap().get(), 100.);
    let scene = f.scene();
    let mut bad = Worker::new("butchery").unwrap();
    bad.set_practice("husbandry", Practice::new(0.).unwrap())
        .unwrap();
    let roster = vec![(a(1), bad)];
    let before = f.stock.clone();
    assert!(
        f.stock
            .butcher_animal(
                &mut f.town,
                &f.ledger,
                &scene,
                &b(2),
                &b(1),
                Species::Pig,
                &roster,
                NavigationRevision(1)
            )
            .is_err()
    );
    assert_eq!(f.stock, before);
    f.stock
        .set_count(&f.town, &f.ledger, &b(1), Species::Pig, 0, 5)
        .unwrap();
    f.stock
        .grow(&f.town, &mut f.ledger, &b(1), sec(999.), false, 1)
        .unwrap();
    assert!(f.stock.animals(&b(1), Species::Pig).unwrap().is_empty());
}
#[test]
fn legacy_names_are_unique_reserved_in_advance_and_survive_generations() {
    let mut f = Fixture::new(vec![]);
    f.town
        .insert_building(Building {
            id: b(3),
            location: t(22, 22),
            construction: ConstructionState::Complete,
        })
        .unwrap();
    let mut stores = f.ledger.stores().to_vec();
    stores.push(Store::new(b(3), StoragePolicy::new("barn")));
    f.ledger = Ledger::new(owner(), q(100.), stores, vec![]).unwrap();
    f.stock = Livestock::admit(
        &f.town,
        &f.ledger,
        vec![
            (
                b(1),
                vec![HerdInput {
                    species: Species::Chicken,
                    count: 3,
                    ages: vec![10., 60.],
                    names: vec![],
                }],
            ),
            (
                b(3),
                vec![HerdInput {
                    species: Species::Pig,
                    count: 40,
                    ages: vec![],
                    names: vec!["Poppy".into()],
                }],
            ),
        ],
        0,
    )
    .unwrap();
    let mut names = std::collections::BTreeSet::new();
    for (id, s) in [(b(1), Species::Chicken), (b(3), Species::Pig)] {
        for a in f.stock.animals(&id, s).unwrap() {
            assert!(names.insert(a.name().to_owned()));
        }
    }
    assert_eq!(names.len(), 43);
    assert_eq!(
        f.stock.animals(&b(3), Species::Pig).unwrap()[0].name(),
        "Poppy"
    );
    assert_eq!(
        f.stock.animals(&b(1), Species::Chicken).unwrap()[0].name(),
        "Clover"
    );
    close(
        f.stock.animals(&b(1), Species::Chicken).unwrap()[2]
            .age()
            .get(),
        60.,
    );
    f.reload();
}
#[test]
fn malformed_owner_saves_reject_ids_clocks_names_orders_and_targets() {
    let mut f = Fixture::new(vec![herd(Species::Pig, vec![240.])]);
    f.start(Species::Pig);
    let json = f.stock.encode(&f.town).unwrap();
    for edit in 0..12 {
        let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
        match edit {
            0 => v["world"] = "other".into(),
            1 => {
                let b = v["barns"][0].clone();
                v["barns"].as_array_mut().unwrap().push(b);
            }
            2 => v["barns"][0]["herds"][0]["animals"][0]["age"] = 241.into(),
            3 => v["next_animal"] = 1.into(),
            4 => v["orders"][0]["handler"] = 999.into(),
            5 => v["orders"][0]["remaining"] = 61.into(),
            6 => v["orders"][0]["leg"] = "processing".into(),
            7 => v["orders"][0]["barn"] = 0.into(),
            8 => {
                let o = v["orders"][0].clone();
                v["orders"].as_array_mut().unwrap().push(o);
            }
            9 => v["orders"][0]
                .as_object_mut()
                .unwrap()
                .remove("position")
                .map(|_| ())
                .unwrap(),
            10 => v["barns"][0]["herds"][0]["animals"][0]["name"] = "".into(),
            _ => v["extra"] = true.into(),
        };
        assert!(
            Livestock::decode(&v.to_string(), &f.town, &f.ledger).is_err(),
            "{edit}"
        );
    }
}
#[test]
fn completing_with_overflowing_practice_rolls_back_goods_order_and_worker() {
    let mut f = Fixture::new(vec![herd(Species::Cow, vec![360.])]);
    f.start(Species::Cow);
    f.reach(ButcherLeg::Processing);
    f.worker
        .set_practice("butchery", Practice::new(f64::MAX).unwrap())
        .unwrap();
    f.worker
        .set_talents(BTreeMap::from([("husbandry".into(), f64::MAX)]))
        .unwrap();
    let before = (
        f.stock.clone(),
        f.town.clone(),
        f.ledger.clone(),
        f.worker.clone(),
    );
    let scene = f.scene();
    assert!(
        f.stock
            .advance_butcher(
                &mut f.town,
                &mut f.ledger,
                &scene,
                &b(2),
                &a(1),
                &mut f.worker,
                sec(90.),
                NavigationRevision(1),
                |_, _, s| Some(s)
            )
            .is_err()
    );
    assert_eq!((f.stock, f.town, f.ledger, f.worker), before);
}
#[test]
fn manual_order_suspends_and_resumes_partial_processor_work() {
    use lantern_host::{
        logistics::LogisticsTrips,
        processing::{IronDeposits, Processors},
        progression::Workplace,
    };
    let mut f = Fixture::new(vec![herd(Species::Pig, vec![240.])]);
    let mut stores = f.ledger.stores().to_vec();
    stores[1].inputs = Stock::from_entries([(key("meat"), q(4.))]).unwrap();
    f.ledger = Ledger::new(owner(), q(100.), stores, vec![]).unwrap();
    let mut processors = Processors::default();
    let workplace = Workplace::new("butcher");
    processors
        .start(
            &mut f.town,
            &mut f.ledger,
            &a(1),
            &b(2),
            &f.worker,
            &workplace,
            NavigationRevision(1),
        )
        .unwrap();
    let RouteState::Pending(r) = f.town.actor(&a(1)).unwrap().route().clone() else {
        panic!("route");
    };
    f.town
        .accept_route(&a(1), &r, r.navigation(), RouteOutcome::Arrived)
        .unwrap();
    processors
        .advance(
            &mut f.town,
            &mut f.ledger,
            &mut IronDeposits::default(),
            &a(1),
            &mut f.worker,
            &workplace,
            sec(1.),
            NavigationRevision(1),
            |_, _, s| Some(s),
        )
        .unwrap();
    let partial = processors.elapsed(&a(1)).unwrap();
    assert!(partial.get() > 0.);
    let roster = f.roster(1);
    let scene = f.scene();
    f.stock
        .interrupt_for_butchery(
            &mut f.town,
            &mut f.ledger,
            &processors,
            &mut LogisticsTrips::default(),
            &scene,
            &b(2),
            &b(1),
            Species::Pig,
            &roster,
            &b(1),
            NavigationRevision(1),
        )
        .unwrap();
    let saved = processors.encode(&f.town).unwrap();
    f.reload();
    processors = Processors::decode(&saved, &f.town, &f.ledger).unwrap();
    assert_eq!(processors.elapsed(&a(1)).unwrap(), partial);
    for _ in 0..100 {
        if matches!(f.advance(1, 2.), Outcome::Completed) {
            break;
        }
    }
    processors
        .start(
            &mut f.town,
            &mut f.ledger,
            &a(1),
            &b(2),
            &f.worker,
            &workplace,
            NavigationRevision(1),
        )
        .unwrap();
    assert_eq!(processors.elapsed(&a(1)).unwrap(), partial);
    close(
        f.ledger
            .store(&b(2))
            .unwrap()
            .inputs
            .quantity(&key("meat"))
            .get(),
        4.,
    );
}
#[test]
fn dragging_speed_matches_python_and_does_not_start_processing_early() {
    let mut f = Fixture::new(vec![herd(Species::Pig, vec![240.])]);
    f.start(Species::Pig);
    let at = f.town.actor(&a(1)).unwrap().state().position;
    f.advance(1, 0.1);
    let walk = f.town.actor(&a(1)).unwrap().state().position;
    let walking = (walk.x.get() - at.x.get()).hypot(walk.y.get() - at.y.get());
    close(walking, 0.18);
    f.reach(ButcherLeg::Drag);
    let at = f.town.actor(&a(1)).unwrap().state().position;
    f.advance(1, 0.1);
    let drag = f.town.actor(&a(1)).unwrap().state().position;
    close(
        (drag.x.get() - at.x.get()).hypot(drag.y.get() - at.y.get()) / walking,
        0.65,
    );
    close(
        f.stock.order(&b(2)).unwrap().unwrap().remaining().get(),
        60.,
    );
}
#[test]
fn lifetime_products_stop_after_maturity_and_explicit_empty_species_stay_empty() {
    for s in [Species::Chicken, Species::Sheep, Species::Cow] {
        let mut f = Fixture::new(vec![herd(s, vec![0.])]);
        let product = match s {
            Species::Chicken => "eggs",
            Species::Sheep => "wool",
            _ => "milk",
        };
        for batch in 1..=(s.duration() / 3.) as u64 {
            f.stock
                .grow(&f.town, &mut f.ledger, &b(1), sec(3.), false, batch)
                .unwrap();
            let costs = Stock::from_entries(
                [product, "manure"]
                    .into_iter()
                    .map(|k| (key(k), f.ledger.free(&key(k)).unwrap())),
            )
            .unwrap();
            f.ledger.spend(&costs).unwrap();
            if batch % 10 == 0 {
                f.reload();
            }
        }
        close(f.ledger.produced().quantity(&key(product)).get(), 25.);
        let before = f.ledger.clone();
        f.stock
            .grow(&f.town, &mut f.ledger, &b(1), sec(1000.), false, 1000)
            .unwrap();
        assert_eq!(f.ledger, before);
        f.stock
            .set_count(&f.town, &f.ledger, &b(1), s, 0, 5)
            .unwrap();
        f.stock
            .grow(&f.town, &mut f.ledger, &b(1), sec(1000.), false, 1001)
            .unwrap();
        assert!(f.stock.animals(&b(1), s).unwrap().is_empty());
    }
}
#[test]
fn keeper_relocation_preserves_herd_clocks_goods_and_personal_position() {
    let mut f = Fixture::new(vec![herd(Species::Chicken, vec![30.])]);
    let roster = f.keeper_roster(1);
    f.stock
        .set_keepers(
            &mut f.town,
            &f.ledger,
            &b(1),
            &roster,
            NavigationRevision(1),
        )
        .unwrap();
    f.keeper(1, 1.);
    let at = f.town.actor(&a(1)).unwrap().state().position;
    let herd = f.stock.animals(&b(1), Species::Chicken).unwrap().to_vec();
    let goods = f.ledger.clone();
    f.town.relocate_building(&b(1), t(17, 17)).unwrap();
    f.stock
        .relocate_keepers(&mut f.town, &b(1), NavigationRevision(2))
        .unwrap();
    assert_eq!(f.town.actor(&a(1)).unwrap().state().position, at);
    assert_eq!(f.stock.animals(&b(1), Species::Chicken).unwrap(), herd);
    assert_eq!(f.ledger, goods);
    f.reload();
}
#[test]
fn no_assigned_butcher_pauses_shared_work_and_failed_interruption_is_atomic() {
    use lantern_host::{logistics::LogisticsTrips, processing::Processors};
    let mut f = Fixture::new(vec![herd(Species::Pig, vec![0.])]);
    let roster = f.roster(1);
    let scene = f.scene();
    let (mut trips, processors) = (LogisticsTrips::default(), Processors::default());
    let before = (
        f.stock.clone(),
        f.town.clone(),
        f.ledger.clone(),
        trips.clone(),
    );
    assert!(
        f.stock
            .interrupt_for_butchery(
                &mut f.town,
                &mut f.ledger,
                &processors,
                &mut trips,
                &scene,
                &b(2),
                &b(1),
                Species::Pig,
                &roster,
                &b(1),
                NavigationRevision(1)
            )
            .is_err()
    );
    assert_eq!(
        (
            f.stock.clone(),
            f.town.clone(),
            f.ledger.clone(),
            trips.clone()
        ),
        before
    );
    let mut f = Fixture::new(vec![herd(Species::Pig, vec![240.])]);
    f.start(Species::Pig);
    f.reach(ButcherLeg::Processing);
    f.advance(1, 10.);
    let remaining = f.stock.order(&b(2)).unwrap().unwrap().remaining();
    let scene = f.scene();
    f.stock
        .reconcile_butchers(
            &mut f.town,
            &f.ledger,
            &scene,
            &b(2),
            &[],
            NavigationRevision(1),
        )
        .unwrap();
    assert_eq!(f.advance(1, 500.), Outcome::Waiting);
    assert_eq!(
        f.stock.order(&b(2)).unwrap().unwrap().remaining(),
        remaining
    );
    f.reload();
}
#[test]
fn construction_helpers_pause_keeper_rounds_without_corrupting_growth_or_reload() {
    let mut f = Fixture::new(vec![herd(Species::Chicken, vec![0.])]);
    let roster = f.keeper_roster(1);
    f.stock
        .set_keepers(
            &mut f.town,
            &f.ledger,
            &b(1),
            &roster,
            NavigationRevision(1),
        )
        .unwrap();
    f.town
        .set_task(
            &a(1),
            Task::Construction {
                site: b(2),
                phase: ConstructionPhase::Building,
                elapsed: sec(2.),
            },
        )
        .unwrap();
    f.stock
        .grow(&f.town, &mut f.ledger, &b(1), sec(3.), false, 1)
        .unwrap();
    close(
        f.stock.animals(&b(1), Species::Chicken).unwrap()[0]
            .age()
            .get(),
        3.,
    );
    let before = f.town.actor(&a(1)).unwrap().state().task.clone();
    f.town.relocate_building(&b(1), t(18, 18)).unwrap();
    f.stock
        .relocate_keepers(&mut f.town, &b(1), NavigationRevision(2))
        .unwrap();
    assert_eq!(f.town.actor(&a(1)).unwrap().state().task, before);
    f.reload();
    f.town.set_task(&a(1), Task::Idle).unwrap();
    f.stock
        .set_keepers(
            &mut f.town,
            &f.ledger,
            &b(1),
            &roster,
            NavigationRevision(2),
        )
        .unwrap();
    f.stock
        .grow(&f.town, &mut f.ledger, &b(1), sec(3.), false, 2)
        .unwrap();
    close(
        f.stock.animals(&b(1), Species::Chicken).unwrap()[0]
            .age()
            .get(),
        9.,
    );
}
#[test]
fn selecting_the_first_adult_keeps_younger_animals_and_clearing_a_replacement_is_final() {
    let mut f = Fixture::new(vec![herd(Species::Pig, vec![10., 240.])]);
    let young = f.stock.animals(&b(1), Species::Pig).unwrap()[0].clone();
    f.start(Species::Pig);
    assert_eq!(f.stock.animals(&b(1), Species::Pig).unwrap(), &[young]);
    for _ in 0..100 {
        if matches!(f.advance(1, 2.), Outcome::Completed) {
            break;
        }
    }
    f.stock
        .set_count(&f.town, &f.ledger, &b(1), Species::Pig, 0, 5)
        .unwrap();
    f.stock
        .grow(&f.town, &mut f.ledger, &b(1), sec(999.), false, 1)
        .unwrap();
    assert!(f.stock.animals(&b(1), Species::Pig).unwrap().is_empty());
}
#[test]
fn growth_subdivisions_preserve_clocks_and_clipped_goods_across_reload() {
    let herds = || {
        vec![
            herd(Species::Chicken, vec![59.]),
            herd(Species::Sheep, vec![115.]),
            herd(Species::Pig, vec![237.]),
            herd(Species::Cow, vec![351.]),
        ]
    };
    let mut whole = Fixture::new(herds());
    let mut split = Fixture::new(herds());
    let roster = whole.keeper_roster(2);
    whole
        .stock
        .set_keepers(
            &mut whole.town,
            &whole.ledger,
            &b(1),
            &roster,
            NavigationRevision(1),
        )
        .unwrap();
    split
        .stock
        .set_keepers(
            &mut split.town,
            &split.ledger,
            &b(1),
            &roster,
            NavigationRevision(1),
        )
        .unwrap();
    whole
        .stock
        .grow(&whole.town, &mut whole.ledger, &b(1), sec(3.), false, 1)
        .unwrap();
    for i in 1..=12 {
        split
            .stock
            .grow(&split.town, &mut split.ledger, &b(1), sec(0.25), false, i)
            .unwrap();
        split.reload();
    }
    for s in [Species::Chicken, Species::Sheep, Species::Pig, Species::Cow] {
        assert_eq!(
            whole.stock.animals(&b(1), s).unwrap(),
            split.stock.animals(&b(1), s).unwrap()
        );
    }
    for k in ["eggs", "wool", "milk", "manure"] {
        close(
            whole.ledger.free(&key(k)).unwrap().get(),
            split.ledger.free(&key(k)).unwrap().get(),
        );
    }
}
