//! Experimental producer and hauler trips. The ledger alone owns cargo; Town owns
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
mod market;
pub use market::{PurchaseOutcome, SaleRule};

#[derive(Debug, PartialEq)]
pub enum Error {
    Resource(resources::ResourceError),
    Progression(progression::Error),
    Model(ModelError),
    InvalidSave,
    Busy,
    Exhausted,
    MarketRule,
    InvalidUpdate,
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
    Returning,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Trip {
    id: u64,
    #[serde(default)]
    operation: HaulOperation,
    #[serde(default)]
    return_to: Option<Endpoint>,
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
    market: market::MarketState,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    version: u8,
    world: String,
    town: String,
    next: u64,
    trips: Vec<(u64, Trip, BTreeMap<String, f64>)>,
    #[serde(default)]
    market: Option<market::MarketState>,
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
    /// Delivery completed; service the hauler's empty return leg with this time.
    Delivered(Seconds),
}

/// Shared trip owner; the original producer API remains available as InputTrips.
pub type LogisticsTrips = InputTrips;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HaulOperation {
    #[default]
    Supply,
    Collect,
    Sell,
}
impl HaulOperation {
    fn cargo(self) -> CargoKind {
        match self {
            Self::Supply => CargoKind::Supply,
            Self::Collect => CargoKind::Collect,
            Self::Sell => CargoKind::Sell,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HaulCandidate {
    pub workplace: BuildingId,
    pub operation: HaulOperation,
}
/// Current producer assignments in resident order, provided by the resident owner.
pub struct ProducerAssignment<'a> {
    pub building: &'a BuildingId,
    pub worker: &'a Worker,
    pub producer: &'a Producer,
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
fn carry_capacity(worker: &Worker, workplace: &Workplace) -> Result<f64, Error> {
    let equipment = if matches!(worker.job(), "milling" | "baking") {
        progression::equipment_multiplier(&workplace.kind, &workplace.equipment)
    } else {
        1.
    };
    let capacity = 50. * worker.efficiency("hauling")? * equipment;
    if !capacity.is_finite() {
        return Err(Error::InvalidSave);
    }
    Ok(capacity)
}
/// Preview a real collection batch without mutation. Owned worker pending output
/// never enters the pickup pool. Each buffer retains its insertion order.
fn collection_plan(
    ledger: &Ledger,
    source: &BuildingId,
    destination: &BuildingId,
    worker: &Worker,
) -> Result<(Stock, Vec<ResourceKey>), Error> {
    let source = ledger.store(source)?;
    let mut target = ledger.store(destination)?.clone();
    let mut remaining = carry_capacity(worker, &Workplace::new(&source.policy.kind))?;
    let mut cargo = Stock::default();
    let mut order = Vec::new();
    for field in [&source.output, &source.stored]
        .into_iter()
        .chain(source.pending.get(&resources::PendingOwner::Orphan))
    {
        for (key, n) in field.ordered_entries() {
            let take = n.get().min(target.local_room(key.as_str())?).min(remaining);
            if take > 0. {
                if !order.contains(key) {
                    order.push(key.clone());
                }
                cargo.credit(key.clone(), q(take))?;
                target.stored.credit(key.clone(), q(take))?;
                remaining -= take;
            }
        }
    }
    Ok((cargo, order))
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
                && trip.operation == HaulOperation::Supply
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
            operation: HaulOperation::Supply,
            return_to: None,
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
    /// Producer input collection; preserve the slice-1 API.
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
        segment: impl FnMut(Position, Position, Speed) -> Option<Speed>,
    ) -> Result<Outcome, Error> {
        let producer_worker = worker.clone();
        self.advance_rules(
            town,
            ledger,
            actor,
            worker,
            producer,
            &producer_worker,
            None,
            hall,
            elapsed,
            navigation,
            segment,
        )
    }
    /// Supplies use the first assigned producer's current recipe/skill facts;
    /// carry speed, capacity, talents and training always belong to the hauler.
    #[allow(clippy::too_many_arguments)]
    pub fn advance_hauler(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        actor: &ActorId,
        worker: &mut Worker,
        assignments: &[ProducerAssignment<'_>],
        hall: &BuildingId,
        elapsed: Seconds,
        navigation: NavigationRevision,
        segment: impl FnMut(Position, Position, Speed) -> Option<Speed>,
    ) -> Result<Outcome, Error> {
        if worker.job() != "hauling" {
            return Err(Error::InvalidSave);
        }
        let Some(trip) = self.trips.get(actor) else {
            return Ok(Outcome::Idle);
        };
        let workplace = if trip.operation == HaulOperation::Supply {
            trip.destination.id(town)?
        } else {
            trip.source.id(town)?
        };
        // A removed endpoint cancels before any rule evaluation or lookup.
        let fallback = Producer::recipe(Workplace::new(
            ledger
                .store(&workplace)
                .map(|s| s.policy.kind.as_str())
                .unwrap_or(""),
        ));
        let fallback_worker = worker.clone();
        let (producer_worker, producer) = assignments
            .iter()
            .find(|a| *a.building == workplace)
            .map(|a| (a.worker, a.producer))
            .unwrap_or((&fallback_worker, &fallback));
        self.advance_rules(
            town,
            ledger,
            actor,
            worker,
            producer,
            producer_worker,
            None,
            hall,
            elapsed,
            navigation,
            segment,
        )
    }
    /// Both route legs must be admitted by the navigation owner before starting.
    pub fn start_haul(
        &mut self,
        town: &mut Town,
        ledger: &Ledger,
        actor: &ActorId,
        storage: &BuildingId,
        candidate: &HaulCandidate,
        navigation: NavigationRevision,
    ) -> Result<u64, Error> {
        if ledger.store(storage)?.policy.kind != "storehouse" {
            return Err(Error::InvalidSave);
        }
        let (source, destination) = match candidate.operation {
            HaulOperation::Supply => (storage, &candidate.workplace),
            HaulOperation::Collect => (&candidate.workplace, storage),
            HaulOperation::Sell => return Err(Error::InvalidSave),
        };
        let id = self.start(town, ledger, actor, source, destination, navigation)?;
        let trip = self.trips.get_mut(actor).unwrap();
        trip.operation = candidate.operation;
        if candidate.operation == HaulOperation::Supply {
            trip.return_to = Some(trip.source.clone());
        }
        Ok(id)
    }
    /// Python priority: supplies first, then fuller workplaces, then nearest;
    /// stable ties preserve building order. Try reachable candidates in this order.
    pub fn hauler_candidates(
        &self,
        town: &Town,
        ledger: &Ledger,
        storage: &BuildingId,
        assignments: &[ProducerAssignment<'_>],
    ) -> Result<Vec<HaulCandidate>, Error> {
        let storage_store = ledger.store(storage)?;
        if storage_store.policy.kind != "storehouse"
            || storage_store.policy.construction
            || town.building(storage)?.construction != ConstructionState::Complete
        {
            return Err(Error::InvalidSave);
        }
        let position = town.building(storage)?.location;
        let inventory = storage_store.inventory()?;
        let mut ranked = Vec::new();
        for s in ledger.stores() {
            if s.id == *storage
                || s.policy.kind == "storehouse"
                || s.policy.construction
                || !town
                    .building(&s.id)
                    .is_ok_and(|b| b.construction == ConstructionState::Complete)
            {
                continue;
            }
            let mut supply = false;
            for a in assignments.iter().filter(|a| *a.building == s.id) {
                let request = self.request(town, ledger, &s.id, a.worker, a.producer)?;
                if request
                    .entries()
                    .keys()
                    .any(|k| inventory.quantity(k).get() > 0.)
                {
                    supply = true;
                    break;
                }
            }
            let available = s.inventory()?;
            let orphan = s.pending.get(&resources::PendingOwner::Orphan);
            let collect = available
                .entries()
                .iter()
                .chain(orphan.into_iter().flat_map(|s| s.entries()))
                .any(|(k, n)| {
                    n.get() > 0. && storage_store.local_room(k.as_str()).is_ok_and(|r| r > 0.)
                });
            let buffered = sum(&available) + sum(&s.pending_stock()?);
            let p = town.building(&s.id)?.location;
            let distance = (i64::from(position.x) - i64::from(p.x)).abs()
                + (i64::from(position.y) - i64::from(p.y)).abs();
            for (enabled, operation) in [
                (supply, HaulOperation::Supply),
                (collect, HaulOperation::Collect),
            ] {
                if enabled {
                    ranked.push((
                        HaulCandidate {
                            workplace: s.id.clone(),
                            operation,
                        },
                        buffered,
                        distance,
                    ));
                }
            }
        }
        ranked.sort_by(|a, b| {
            let priority = |c: &HaulCandidate| {
                if c.operation == HaulOperation::Supply {
                    0
                } else {
                    1
                }
            };
            priority(&a.0)
                .cmp(&priority(&b.0))
                .then_with(|| b.1.total_cmp(&a.1))
                .then_with(|| a.2.cmp(&b.2))
        });
        Ok(ranked
            .into_iter()
            .map(|(candidate, _, _)| candidate)
            .collect())
    }
    /// Atomic rule/movement update. Segment admission comes from current shared
    /// collision/road rules. None blocks movement without dropping carried goods.
    #[allow(clippy::too_many_arguments)]
    fn advance_rules(
        &mut self,
        town: &mut Town,
        ledger: &mut Ledger,
        actor: &ActorId,
        worker: &mut Worker,
        producer: &Producer,
        producer_worker: &Worker,
        market_rules: Option<&market::Rules>,
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
            producer_worker,
            market_rules,
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
        producer_worker: &Worker,
        market_rules: Option<&market::Rules>,
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
            } else if trip.phase == Phase::Returning {
                TransportPhase::Returning
            } else {
                TransportPhase::ToDropoff
            },
        };
        if town.actor(actor)?.state().task != expected
            || !trip.source.available(town, ledger)
            || !trip.destination.available(town, ledger)
            || trip
                .return_to
                .as_ref()
                .is_some_and(|e| !e.available(town, ledger))
        {
            self.cancel(town, ledger, actor, hall)?;
            return Ok(Outcome::Cancelled);
        }
        let speed =
            Speed::new(1.8 * (1. + 0.25 * (worker.practice_value("hauling") / 20.).floor()))
                .ok_or(Error::InvalidSave)?;
        town.resume_travel(actor, navigation)?;
        let advance = town.advance_travel(actor, elapsed, 4096, |a, b| segment(a, b, speed))?;
        let target = if trip.phase == Phase::Returning {
            trip.return_to
                .as_ref()
                .ok_or(Error::InvalidSave)?
                .id(town)?
        } else if trip.phase == Phase::Pickup {
            trip.source.id(town)?
        } else {
            trip.destination.id(town)?
        };
        if town.actor(actor)?.state().travel != TravelIntent::Arrived(Destination::Building(target))
        {
            return Ok(Outcome::Travelling);
        }
        if trip.phase == Phase::Pickup {
            let (cargo, order) = if trip.operation == HaulOperation::Sell {
                let cargo = market_rules.ok_or(Error::InvalidSave)?.sale_cargo(
                    ledger,
                    &trip.source.id(town)?,
                    worker,
                )?;
                let order = cargo.ordered_entries().map(|(k, _)| k.clone()).collect();
                (cargo, order)
            } else if trip.operation == HaulOperation::Collect {
                collection_plan(
                    ledger,
                    &trip.source.id(town)?,
                    &trip.destination.id(town)?,
                    worker,
                )?
            } else {
                let request = self.request(
                    town,
                    ledger,
                    &trip.destination.id(town)?,
                    producer_worker,
                    producer,
                )?;
                let inventory = ledger.store(&trip.source.id(town)?)?.inventory()?;
                let mut room = carry_capacity(worker, workplace)?;
                let mut cargo = Stock::default();
                let order = ingredient_order(workplace)?;
                for k in &order {
                    let take = request
                        .quantity(k)
                        .get()
                        .min(inventory.quantity(k).get())
                        .min(room);
                    if take > 0. {
                        cargo.credit(k.clone(), q(take))?;
                        room -= take;
                    }
                }
                (cargo, order)
            };
            if sum(&cargo) > 0. {
                ledger.pickup(
                    actor,
                    &trip.source.id(town)?,
                    trip.operation.cargo(),
                    &cargo,
                )?;
            }
            self.trips.get_mut(actor).unwrap().cargo_order = order
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
        if trip.phase == Phase::Delivery
            && trip.operation == HaulOperation::Sell
            && ledger.held().contains_key(actor)
        {
            let cargo = ledger.held()[actor].goods.clone();
            let sold = market_rules
                .ok_or(Error::InvalidSave)?
                .sale_at_arrival(ledger, &cargo)?;
            if sum(&sold) > 0. {
                ledger.sell_held(actor, &sold)?;
                for _ in sold.entries().values().filter(|n| n.get() > 0.) {
                    worker.practice("trading", Seconds::new(3.).unwrap())?;
                    worker.practice("hauling", Seconds::new(3.).unwrap())?;
                }
            }
            if ledger.held().contains_key(actor) {
                ledger.restore_cargo(actor, &trip.source.id(town)?)?;
            }
        }
        if trip.phase == Phase::Delivery && ledger.held().contains_key(actor) {
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
        if trip.phase == Phase::Delivery
            && let Some(endpoint) = &trip.return_to
        {
            self.trips.get_mut(actor).unwrap().phase = Phase::Returning;
            town.set_task(
                actor,
                Task::Transport {
                    source: trip.source.id(town)?,
                    destination: trip.destination.id(town)?,
                    phase: TransportPhase::Returning,
                },
            )?;
            town.request_route(actor, Destination::Building(endpoint.id(town)?), navigation)?;
            return Ok(Outcome::Delivered(advance.remaining));
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
            version: 3,
            market: Some(self.market.clone()),
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
        if !matches!(saved.version, 1..=3)
            || saved.world != town.owner().world.as_str()
            || saved.town != town.owner().town.as_str()
        {
            return Err(Error::InvalidSave);
        }
        if (saved.version < 3 && saved.market.is_some())
            || (saved.version == 3 && saved.market.is_none())
        {
            return Err(Error::InvalidSave);
        }
        let market = saved.market.unwrap_or_default();
        market.validate(town)?;
        let mut result = Self {
            next: saved.next,
            trips: BTreeMap::new(),
            market,
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
                } else if trip.phase == Phase::Returning {
                    TransportPhase::Returning
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
                || (saved.version == 1
                    && (trip.operation != HaulOperation::Supply
                        || trip.return_to.is_some()
                        || trip.phase == Phase::Returning))
                || (saved.version < 3 && trip.operation == HaulOperation::Sell)
                || (trip.operation == HaulOperation::Sell
                    && ledger
                        .store(&trip.destination.id(town)?)
                        .is_ok_and(|s| s.policy.kind != "market"))
                || (trip.operation != HaulOperation::Supply && trip.return_to.is_some())
                || trip.return_to.as_ref().is_some_and(|e| e != &trip.source)
                || (trip.phase == Phase::Returning
                    && (trip.return_to.is_none() || ledger.held().contains_key(&actor)))
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
                    .is_some_and(|c| c.kind != trip.operation.cargo() || c.source != source)
                || result.trips.insert(actor, trip).is_some()
            {
                return Err(Error::InvalidSave);
            }
        }
        Ok(result)
    }
}
