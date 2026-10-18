//! Experimental producer input trips. The ledger alone owns cargo; Town owns
//! movement. Navigation remains disposable and must use the shared route service.
//! DTO reload is offline admission, not a durable save or playable host adapter.
use crate::{
    progression::{self, Worker, Workplace},
    resources::{self, CargoKind, Ledger, ResourceKey, Stock},
};
use lantern_core::{
    ids::{ActorId, BuildingId, EntityIdentity},
    model::{
        ConstructionState, Destination, ModelError, NavigationRevision, Task, Town, TransportPhase,
        TravelIntent,
    },
    travel::Speed,
    units::{Position, Quantity, Seconds},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, PartialEq)]
pub enum Error {
    Resource(resources::ResourceError),
    Progression(progression::Error),
    Model(ModelError),
    InvalidSave,
    Busy,
    Exhausted,
}
impl From<resources::ResourceError> for Error {
    fn from(e: resources::ResourceError) -> Self {
        Self::Resource(e)
    }
}
impl From<progression::Error> for Error {
    fn from(e: progression::Error) -> Self {
        Self::Progression(e)
    }
}
impl From<ModelError> for Error {
    fn from(e: ModelError) -> Self {
        Self::Model(e)
    }
}
fn q(n: f64) -> Quantity {
    Quantity::new(n).expect("bounded quantity")
}
fn sum(s: &Stock) -> f64 {
    s.entries().values().map(|n| n.get()).sum()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Endpoint {
    id: u64,
    x: i32,
    y: i32,
}
impl Endpoint {
    fn capture(town: &Town, id: &BuildingId) -> Result<Self, Error> {
        let b = town.building(id)?;
        Ok(Self {
            id: id.identity().serial(),
            x: b.location.x,
            y: b.location.y,
        })
    }
    fn id(&self, town: &Town) -> Result<BuildingId, Error> {
        Ok(BuildingId::from_saved(
            EntityIdentity::from_saved(town.owner().clone(), self.id)
                .map_err(|_| Error::InvalidSave)?,
        ))
    }
    fn available(&self, town: &Town, ledger: &Ledger) -> bool {
        self.id(town).is_ok_and(|id| {
            town.building(&id).is_ok_and(|b| {
                b.location.x == self.x
                    && b.location.y == self.y
                    && b.construction == ConstructionState::Complete
            }) && ledger.store(&id).is_ok_and(|s| !s.policy.construction)
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
enum Phase {
    Pickup,
    Delivery,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Trip {
    id: u64,
    source: Endpoint,
    destination: Endpoint,
    phase: Phase,
    trained: bool,
    cargo_order: Vec<String>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InputTrips {
    next: u64,
    trips: BTreeMap<ActorId, Trip>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    version: u8,
    world: String,
    town: String,
    next: u64,
    trips: Vec<(u64, Trip, BTreeMap<String, f64>)>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    Idle,
    Travelling,
    /// Service the new return leg, then reuse this unconsumed simulation time.
    PickedUp(Seconds),
    WaitingStorage,
    Completed(Seconds),
    Cancelled,
}

/// Explicit production facts from the recipe/animal owners; this module does
/// not advance production work, animal clocks, or manual butchering.
#[derive(Clone, Debug)]
pub struct Producer {
    pub workplace: Workplace,
    pub animal_feed: Quantity,
    pub butchering: bool,
}
impl Producer {
    pub fn recipe(workplace: Workplace) -> Self {
        Self {
            workplace,
            animal_feed: q(0.),
            butchering: false,
        }
    }
}
fn ingredient_order(workplace: &Workplace) -> Result<Vec<ResourceKey>, Error> {
    if matches!(workplace.kind.as_str(), "barn" | "stable") {
        return Ok(vec![
            ResourceKey::parse("fodder")?,
            ResourceKey::parse("vegetables")?,
        ]);
    }
    if workplace.kind == "smokehouse" {
        let mut keys: Vec<_> = resources::catalog()["rules"]["fish_order"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| ResourceKey::parse(v.as_str().unwrap()).unwrap())
            .collect();
        keys.push(ResourceKey::parse("wood")?);
        return Ok(keys);
    }
    static ORDER: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    let order = ORDER.get_or_init(|| {
        serde_json::from_str(include_str!("logistics/input_order.json"))
            .expect("reviewed ingredient order")
    });
    let recipe = workplace.selected_recipe()?;
    order[&workplace.kind][recipe.output.as_str()]
        .as_array()
        .ok_or(Error::InvalidSave)?
        .iter()
        .map(|v| Ok(ResourceKey::parse(v.as_str().ok_or(Error::InvalidSave)?)?))
        .collect()
}
fn request_with_inputs(
    ledger: &Ledger,
    destination: &BuildingId,
    worker: &Worker,
    producer: &Producer,
    inputs: &mut Stock,
) -> Result<Stock, Error> {
    let workplace = &producer.workplace;
    let local = &ledger.store(destination)?.inputs;
    let needs = if matches!(workplace.kind.as_str(), "barn" | "stable") {
        if producer.animal_feed.get() == 0. {
            return Ok(Stock::default());
        }
        let key =
            if local.quantity(&ResourceKey::parse("fodder")?).get() >= producer.animal_feed.get() {
                "fodder"
            } else {
                "vegetables"
            };
        Stock::from_entries([(ResourceKey::parse(key)?, producer.animal_feed)])?
    } else {
        if (workplace.kind == "butcher" && producer.butchering)
            || workplace.recipe_requirement(worker).is_err()
        {
            return Ok(Stock::default());
        }
        let mut costs = Stock::default();
        let speed = workplace.processing_efficiency(worker)?;
        for (key, n) in workplace.selected_recipe()?.inputs.entries() {
            if workplace.kind == "smokehouse" && key.as_str() == "fish" {
                let mut remaining = n.get() * speed;
                for name in resources::catalog()["rules"]["fish_order"]
                    .as_array()
                    .unwrap()
                {
                    let fish = ResourceKey::parse(name.as_str().unwrap())?;
                    let take = remaining.min(local.quantity(&fish).get());
                    if take > 0. {
                        costs.credit(fish, q(take))?;
                        remaining -= take;
                    }
                }
                if remaining > 0. {
                    costs.credit(key.clone(), q(remaining))?;
                }
            } else {
                costs.credit(
                    key.clone(),
                    Quantity::new(n.get() * speed).map_err(|_| Error::InvalidSave)?,
                )?;
            }
        }
        costs
    };
    if needs
        .entries()
        .iter()
        .all(|(k, n)| inputs.quantity(k).get() + 1e-9 >= n.get())
    {
        return Ok(Stock::default());
    }
    let mut grouped = needs.clone();
    if matches!(workplace.kind.as_str(), "barn" | "stable") {
        let feed = producer.animal_feed;
        grouped = Stock::from_entries([(
            ResourceKey::parse(
                if ledger.free(&ResourceKey::parse("fodder")?)?.get() >= feed.get() {
                    "fodder"
                } else {
                    "vegetables"
                },
            )?,
            feed,
        )])?;
    }
    if workplace.kind == "smokehouse" {
        let mut total = 0.;
        for name in resources::catalog()["rules"]["fish_order"]
            .as_array()
            .unwrap()
        {
            let k = ResourceKey::parse(name.as_str().unwrap())?;
            total += needs.quantity(&k).get();
            grouped.spend(&Stock::from_entries([(k.clone(), needs.quantity(&k))])?)?;
        }
        grouped.credit(ResourceKey::parse("fish")?, q(total))?;
    }
    let total = sum(&grouped);
    if total == 0. {
        return Ok(Stock::default());
    }
    let size = ledger.store(destination)?.policy.input_size();
    let mut batches = size / total;
    if workplace.kind == "workshop"
        && let Some(head) = &workplace.request
    {
        batches = batches.min(
            head.remaining.get()
                / (workplace.selected_recipe()?.amount.get()
                    * workplace.processing_efficiency(worker)?),
        );
    }
    let mut room = (size - sum(inputs)).max(0.);
    let mut request = Stock::default();
    let order = if workplace.kind == "smokehouse" {
        vec![ResourceKey::parse("fish")?, ResourceKey::parse("wood")?]
    } else {
        ingredient_order(workplace)?
    };
    for key in &order {
        let n = grouped.quantity(key);
        let keys = if workplace.kind == "smokehouse" && key.as_str() == "fish" {
            resources::catalog()["rules"]["fish_order"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| ResourceKey::parse(v.as_str().unwrap()).unwrap())
                .collect()
        } else {
            vec![key.clone()]
        };
        let mut wanted = (n.get() * batches
            - keys.iter().map(|k| inputs.quantity(k).get()).sum::<f64>())
        .max(0.);
        for k in keys {
            let available = ledger
                .stores()
                .iter()
                .filter(|s| &s.id != destination && !s.policy.construction)
                .try_fold(0., |a, s| {
                    Ok::<_, Error>(a + s.inventory()?.quantity(&k).get())
                })?;
            let take = room.min(wanted).min(available);
            if take > 1e-9 {
                request.credit(k, q(take))?;
                room -= take;
                wanted -= take;
            }
        }
    }
    Ok(request)
}
#[derive(Debug, PartialEq)]
pub enum ProductionGate {
    Ready,
    CollectInputs(Stock),
    BufferFull,
}
impl InputTrips {
    /// Work schedulers consult this before production. Supply collection takes
    /// precedence over full output buffers, preserving processing of local inputs.
    pub fn production_gate(
        &self,
        town: &Town,
        ledger: &Ledger,
        destination: &BuildingId,
        worker: &Worker,
        producer: &Producer,
    ) -> Result<ProductionGate, Error> {
        let request = self.request(town, ledger, destination, worker, producer)?;
        if sum(&request) > 0. {
            return Ok(ProductionGate::CollectInputs(request));
        }
        let s = ledger.store(destination)?;
        let mut output = s.inventory()?;
        for (k, n) in s.pending_stock()?.entries() {
            output.credit(k.clone(), *n)?;
        }
        if output
            .entries()
            .iter()
            .any(|(k, n)| n.get() > 0. && s.local_room(k.as_str()).is_ok_and(|r| r <= 1e-9))
        {
            return Ok(ProductionGate::BufferFull);
        }
        Ok(ProductionGate::Ready)
    }
    pub fn trip_id(&self, actor: &ActorId) -> Option<u64> {
        self.trips.get(actor).map(|t| t.id)
    }
    pub fn request(
        &self,
        town: &Town,
        ledger: &Ledger,
        destination: &BuildingId,
        worker: &Worker,
        producer: &Producer,
    ) -> Result<Stock, Error> {
        let store = ledger.store(destination)?;
        if store.policy.kind != producer.workplace.kind {
            return Err(Error::InvalidSave);
        }
        let mut inputs = store.inputs.clone();
        for (actor, trip) in &self.trips {
            if trip.destination.id(town)? == *destination
                && trip.phase == Phase::Delivery
                && let Some(cargo) = ledger.held().get(actor)
            {
                for (k, n) in cargo.goods.entries() {
                    inputs.credit(k.clone(), *n)?;
                }
            }
        }
        request_with_inputs(ledger, destination, worker, producer, &mut inputs)
    }
    /// Stable nearest-first candidates; the caller tries shared navigation in this
    /// order and starts the first source with reachable outward AND return legs.
    pub fn sources(
        &self,
        town: &Town,
        ledger: &Ledger,
        destination: &BuildingId,
        worker: &Worker,
        producer: &Producer,
    ) -> Result<Vec<BuildingId>, Error> {
        let request = self.request(town, ledger, destination, worker, producer)?;
        let origin = town.building(destination)?.location;
        let mut sources = Vec::new();
        for s in ledger.stores() {
            if &s.id != destination
                && !s.policy.construction
                && town.building(&s.id).is_ok()
                && request
                    .entries()
                    .keys()
                    .any(|k| s.inventory().is_ok_and(|i| i.quantity(k).get() > 0.))
            {
                sources.push(s.id.clone());
            }
        }
        sources.sort_by_key(|id| {
            let p = town.building(id).unwrap().location;
            (i64::from(p.x) - i64::from(origin.x)).abs()
                + (i64::from(p.y) - i64::from(origin.y)).abs()
        });
        Ok(sources)
    }
    pub fn start(
        &mut self,
        town: &mut Town,
        ledger: &Ledger,
        actor: &ActorId,
        source: &BuildingId,
        destination: &BuildingId,
        navigation: NavigationRevision,
    ) -> Result<u64, Error> {
        if self.trips.contains_key(actor)
            || ledger.held().contains_key(actor)
            || !town.actor(actor)?.state().cargo.goods().is_empty()
        {
            return Err(Error::Busy);
        }
        let id = self.next.checked_add(1).ok_or(Error::Exhausted)?;
        let trip = Trip {
            id,
            source: Endpoint::capture(town, source)?,
            destination: Endpoint::capture(town, destination)?,
            phase: Phase::Pickup,
            trained: false,
            cargo_order: Vec::new(),
        };
        if !trip.source.available(town, ledger)
            || !trip.destination.available(town, ledger)
            || source == destination
        {
            return Err(Error::InvalidSave);
        }
        let mut next = town.clone();
        next.set_task(
            actor,
            Task::Transport {
                source: source.clone(),
                destination: destination.clone(),
                phase: TransportPhase::ToPickup,
            },
        )?;
        next.request_route(actor, Destination::Building(source.clone()), navigation)?;
        *town = next;
        self.next = id;
        self.trips.insert(actor.clone(), trip);
        Ok(id)
    }
    /// Atomic rule/movement update. Segment admission comes from current shared
    /// collision/road rules. None blocks movement without dropping carried goods.
    #[allow(clippy::too_many_arguments)]
    pub fn advance(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        actor: &ActorId,
        worker: &mut Worker,
        producer: &Producer,
        hall: &BuildingId,
        elapsed: Seconds,
        navigation: NavigationRevision,
        mut segment: impl FnMut(Position, Position, Speed) -> Option<Speed>,
    ) -> Result<Outcome, Error> {
        let (mut trips, mut model, mut goods, mut person) =
            (self.clone(), town.clone(), ledger.clone(), worker.clone());
        let result = trips.advance_inner(
            &mut model,
            &mut goods,
            actor,
            &mut person,
            producer,
            hall,
            elapsed,
            navigation,
            &mut segment,
        )?;
        *self = trips;
        *town = model;
        *ledger = goods;
        *worker = person;
        Ok(result)
    }
    #[allow(clippy::too_many_arguments)]
    fn advance_inner(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        actor: &ActorId,
        worker: &mut Worker,
        producer: &Producer,
        hall: &BuildingId,
        elapsed: Seconds,
        navigation: NavigationRevision,
        segment: &mut impl FnMut(Position, Position, Speed) -> Option<Speed>,
    ) -> Result<Outcome, Error> {
        let workplace = &producer.workplace;
        let Some(trip) = self.trips.get(actor).cloned() else {
            return Ok(Outcome::Idle);
        };
        let expected = Task::Transport {
            source: trip.source.id(town)?,
            destination: trip.destination.id(town)?,
            phase: if trip.phase == Phase::Pickup {
                TransportPhase::ToPickup
            } else {
                TransportPhase::ToDropoff
            },
        };
        if town.actor(actor)?.state().task != expected
            || !trip.source.available(town, ledger)
            || !trip.destination.available(town, ledger)
        {
            self.cancel(town, ledger, actor, hall)?;
            return Ok(Outcome::Cancelled);
        }
        let speed =
            Speed::new(1.8 * (1. + 0.25 * (worker.practice_value("hauling") / 20.).floor()))
                .ok_or(Error::InvalidSave)?;
        town.resume_travel(actor, navigation)?;
        let advance = town.advance_travel(actor, elapsed, 4096, |a, b| segment(a, b, speed))?;
        let target = if trip.phase == Phase::Pickup {
            trip.source.id(town)?
        } else {
            trip.destination.id(town)?
        };
        if town.actor(actor)?.state().travel != TravelIntent::Arrived(Destination::Building(target))
        {
            return Ok(Outcome::Travelling);
        }
        if trip.phase == Phase::Pickup {
            let request =
                self.request(town, ledger, &trip.destination.id(town)?, worker, producer)?;
            let inventory = ledger.store(&trip.source.id(town)?)?.inventory()?;
            let equipment = if matches!(worker.job(), "milling" | "baking") {
                progression::equipment_multiplier(&workplace.kind, &workplace.equipment)
            } else {
                1.
            };
            let mut room = 50. * worker.efficiency("hauling")? * equipment;
            let mut cargo = Stock::default();
            for k in &ingredient_order(workplace)? {
                let n = request.quantity(k);
                let take = n.get().min(inventory.quantity(k).get()).min(room);
                if take > 0. {
                    cargo.credit(k.clone(), q(take))?;
                    room -= take;
                }
            }
            if sum(&cargo) > 0. {
                ledger.pickup(actor, &trip.source.id(town)?, CargoKind::Supply, &cargo)?;
            }
            self.trips.get_mut(actor).unwrap().cargo_order = ingredient_order(workplace)?
                .iter()
                .filter(|k| cargo.quantity(k).get() > 0.)
                .map(|k| k.as_str().into())
                .collect();
            self.trips.get_mut(actor).unwrap().phase = Phase::Delivery;
            town.set_task(
                actor,
                Task::Transport {
                    source: trip.source.id(town)?,
                    destination: trip.destination.id(town)?,
                    phase: TransportPhase::ToDropoff,
                },
            )?;
            town.request_route(
                actor,
                Destination::Building(trip.destination.id(town)?),
                navigation,
            )?;
            return Ok(Outcome::PickedUp(advance.remaining));
        }
        if ledger.held().contains_key(actor) {
            let order: Vec<_> = trip
                .cargo_order
                .iter()
                .map(|k| ResourceKey::parse(k))
                .collect::<Result<_, _>>()?;
            let moved = ledger.deposit_ordered(actor, &trip.destination.id(town)?, &order)?;
            if sum(&moved) > 0. && !trip.trained {
                worker.practice("hauling", Seconds::new(3.).unwrap())?;
                self.trips.get_mut(actor).unwrap().trained = true;
            }
            if ledger.held().contains_key(actor) {
                return Ok(Outcome::WaitingStorage);
            }
        }
        self.trips.remove(actor);
        town.set_task(actor, Task::Idle)?;
        Ok(Outcome::Completed(advance.remaining))
    }
    pub fn cancel(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        actor: &ActorId,
        hall: &BuildingId,
    ) -> Result<(), Error> {
        let Some(trip) = self.trips.get(actor) else {
            return Ok(());
        };
        let mut next = ledger.clone();
        let mut model = town.clone();
        if next.held().contains_key(actor) {
            let destination = if trip.source.available(town, ledger) {
                trip.source.id(town)?
            } else {
                hall.clone()
            };
            if destination == *hall
                && (!town
                    .building(hall)
                    .is_ok_and(|b| b.construction == ConstructionState::Complete)
                    || next.store(hall)?.policy.kind != "hall")
            {
                return Err(Error::InvalidSave);
            }
            next.restore_cargo(actor, &destination)?;
        }
        if matches!(model.actor(actor)?.state().task, Task::Transport {ref source, ref destination, ..} if *source==trip.source.id(town)? && *destination==trip.destination.id(town)?)
        {
            model.set_task(actor, Task::Idle)?;
        }
        *ledger = next;
        *town = model;
        self.trips.remove(actor);
        Ok(())
    }
    pub fn encode(&self, town: &Town, ledger: &Ledger) -> Result<String, Error> {
        serde_json::to_string(&Saved {
            version: 1,
            world: town.owner().world.as_str().into(),
            town: town.owner().town.as_str().into(),
            next: self.next,
            trips: self
                .trips
                .iter()
                .map(|(a, t)| {
                    (
                        a.identity().serial(),
                        t.clone(),
                        ledger
                            .held()
                            .get(a)
                            .map(|c| {
                                c.goods
                                    .entries()
                                    .iter()
                                    .map(|(k, n)| (k.as_str().into(), n.get()))
                                    .collect()
                            })
                            .unwrap_or_default(),
                    )
                })
                .collect(),
        })
        .map_err(|_| Error::InvalidSave)
    }
    /// Admit alongside restored model/ledger facts; reject phase/cargo mismatch.
    /// Routes are rebuilt by Town::resume_travel, never serialized here.
    pub fn decode(json: &str, town: &Town, ledger: &Ledger) -> Result<Self, Error> {
        let saved: Saved = serde_json::from_str(json).map_err(|_| Error::InvalidSave)?;
        if saved.version != 1
            || saved.world != town.owner().world.as_str()
            || saved.town != town.owner().town.as_str()
        {
            return Err(Error::InvalidSave);
        }
        let mut result = Self {
            next: saved.next,
            trips: BTreeMap::new(),
        };
        let mut ids = std::collections::BTreeSet::new();
        for (serial, trip, cargo) in saved.trips {
            let actor = ActorId::from_saved(
                EntityIdentity::from_saved(town.owner().clone(), serial)
                    .map_err(|_| Error::InvalidSave)?,
            );
            let source = trip.source.id(town)?;
            let destination = trip.destination.id(town)?;
            let expected = Task::Transport {
                source: source.clone(),
                destination,
                phase: if trip.phase == Phase::Pickup {
                    TransportPhase::ToPickup
                } else {
                    TransportPhase::ToDropoff
                },
            };
            let actual: BTreeMap<String, f64> = ledger
                .held()
                .get(&actor)
                .map(|c| {
                    c.goods
                        .entries()
                        .iter()
                        .map(|(k, n)| (k.as_str().into(), n.get()))
                        .collect()
                })
                .unwrap_or_default();
            let keys: std::collections::BTreeSet<_> = trip.cargo_order.iter().collect();
            if trip
                .cargo_order
                .iter()
                .any(|k| ResourceKey::parse(k).is_err())
                || keys.len() != trip.cargo_order.len()
                || cargo.keys().any(|k| !keys.contains(k))
                || (trip.phase == Phase::Pickup && !keys.is_empty())
                || cargo != actual
                || trip.source.id == trip.destination.id
                || trip.id == 0
                || trip.id > result.next
                || !ids.insert(trip.id)
                || (matches!(&town.actor(&actor)?.state().task, Task::Transport { source: s, destination: d, .. } if *s == source && *d == trip.destination.id(town)?)
                    && town.actor(&actor)?.state().task != expected)
                || !town.actor(&actor)?.state().cargo.goods().is_empty()
                || (trip.phase == Phase::Pickup
                    && (trip.trained || ledger.held().contains_key(&actor)))
                || ledger
                    .held()
                    .get(&actor)
                    .is_some_and(|c| c.kind != CargoKind::Supply || c.source != source)
                || result.trips.insert(actor, trip).is_some()
            {
                return Err(Error::InvalidSave);
            }
        }
        Ok(result)
    }
}
