//! Experimental barn herds, independent growth, keeper rounds and manual butchery.
//! R12 owns construction/placement; R18 admits resident rosters. No wall clock,
//! feed consumption, client animation rewards or durable/live-save adapter.
mod butchery;
mod keepers;
mod save;
use crate::{
    gathering::Error,
    progression::Worker,
    resources::{Ledger, ResourceKey, Stock},
};
pub use butchery::{ButcherLeg, Order};
use lantern_core::{
    ids::{ActorId, BuildingId, EntityIdentity, Owner},
    model::{
        ConstructionState, Destination, NavigationRevision, RouteOutcome, RouteRequest, RouteState,
        Task, Town, TravelIntent, WorkPhase,
    },
    search::{Access, Query, Target, Terrain},
    spatial::{self, AccessProfile, Footprint, LandAccess, RoadMode, SpatialIndex},
    travel::Speed,
    units::{Position, Quantity, Seconds, Tile},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
fn sec(n: f64) -> Seconds {
    Seconds::new(n).expect("validated livestock clock")
}
fn quantity(n: f64) -> Result<Quantity, Error> {
    Quantity::new(n).map_err(|_| Error::Invalid)
}
fn key(s: &str) -> ResourceKey {
    ResourceKey::parse(s).unwrap()
}
fn tile(p: [i32; 2]) -> Tile {
    Tile { x: p[0], y: p[1] }
}
fn pair(t: Tile) -> [i32; 2] {
    [t.x, t.y]
}
fn pos(t: Tile) -> Position {
    Position::new(f64::from(t.x), f64::from(t.y)).unwrap()
}
fn point(p: [f64; 2]) -> Result<Position, Error> {
    Position::new(p[0], p[1]).map_err(|_| Error::Invalid)
}
fn xy(p: Position) -> [f64; 2] {
    [p.x.get(), p.y.get()]
}
fn rounded(p: Position) -> Tile {
    Tile {
        x: p.x.get().round_ties_even() as i32,
        y: p.y.get().round_ties_even() as i32,
    }
}
fn bid(town: &Town, n: u64) -> Result<BuildingId, Error> {
    Ok(BuildingId::from_saved(
        EntityIdentity::from_saved(town.owner().clone(), n).map_err(|_| Error::Invalid)?,
    ))
}
fn aid(town: &Town, n: u64) -> Result<ActorId, Error> {
    Ok(ActorId::from_saved(
        EntityIdentity::from_saved(town.owner().clone(), n).map_err(|_| Error::Invalid)?,
    ))
}
fn complete(town: &Town, ledger: &Ledger, id: &BuildingId, kind: &str) -> bool {
    town.building(id)
        .is_ok_and(|b| b.construction == ConstructionState::Complete)
        && ledger
            .store(id)
            .is_ok_and(|s| s.policy.kind == kind && !s.policy.construction)
}
const NAMES: [&str; 32] = [
    "Poppy",
    "Clover",
    "Hazel",
    "Biscuit",
    "Mabel",
    "Pip",
    "Daisy",
    "Fern",
    "Buttercup",
    "Pebble",
    "Maple",
    "Honey",
    "Moss",
    "Nellie",
    "Pumpkin",
    "Coco",
    "Dottie",
    "Juniper",
    "Pickle",
    "Rosie",
    "Bramble",
    "Olive",
    "Chestnut",
    "Sunny",
    "Bonnie",
    "Acorn",
    "Ginger",
    "Marigold",
    "Basil",
    "Bluebell",
    "Toffee",
    "Pearl",
];
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Species {
    Chicken,
    Sheep,
    Pig,
    Cow,
}
impl Species {
    pub fn name(self) -> &'static str {
        match self {
            Self::Chicken => "chicken",
            Self::Sheep => "sheep",
            Self::Pig => "pig",
            Self::Cow => "cow",
        }
    }
    fn spec(self) -> &'static serde_json::Value {
        &crate::resources::public_catalog()["animals"][self.name()]
    }
    pub fn duration(self) -> f64 {
        self.spec()["growth_seconds"].as_f64().unwrap()
    }
    fn meat(self) -> f64 {
        crate::resources::public_catalog()["animal_meat"][self.name()]
            .as_f64()
            .unwrap()
    }
    fn hides(self) -> f64 {
        match self {
            Self::Chicken => 0.,
            Self::Sheep => 1.,
            Self::Pig => 2.,
            Self::Cow => 3.,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Animal {
    id: u64,
    name: String,
    age: f64,
}
impl Animal {
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn age(&self) -> Seconds {
        sec(self.age)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Herd {
    species: Species,
    animals: Vec<Animal>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Barn {
    id: u64,
    herds: Vec<Herd>,
    last_growth: u64,
}
/// Explicit legacy barn facts: missing ages mean adults; missing/duplicate/blank
/// names receive deterministic names in admitted barn/species order.
pub struct HerdInput {
    pub species: Species,
    pub count: usize,
    pub ages: Vec<f64>,
    pub names: Vec<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Livestock {
    owner: Owner,
    barns: Vec<Barn>,
    keepers: Vec<keepers::Keeper>,
    orders: Vec<Order>,
    name_sequence: u64,
    next_animal: u64,
    next_order: u64,
}
/// Shared derived geometry, with actual saved footprints rather than catalog
/// dimensions. Missing footprints mean the legacy 1x1 default.
pub struct Scene {
    pub index: Rc<SpatialIndex>,
    pub terrain: Terrain,
    pub footprints: BTreeMap<BuildingId, Footprint>,
}
impl Scene {
    fn footprint(&self, town: &Town, id: &BuildingId) -> Result<Footprint, Error> {
        let at = town.building(id)?.location;
        let fp = self
            .footprints
            .get(id)
            .copied()
            .unwrap_or(Footprint::saved(at, None).map_err(|_| Error::Invalid)?);
        if fp.origin() != at {
            return Err(Error::Invalid);
        }
        Ok(fp)
    }
    fn open(&self, t: Tile) -> bool {
        spatial::search_terrain_open(
            t,
            self.index.size(),
            self.terrain.bounds,
            self.terrain.cells.as_ref().map(|m| m.get(&t).copied()),
        ) && !self.index.occupied_for(
            t,
            &AccessProfile {
                land: LandAccess::Ordinary,
                exemptions: &BTreeSet::new(),
                only: None,
            },
        )
    }
    fn segment_safe(&self, a: Position, b: Position) -> bool {
        let (x, y) = (rounded(a), rounded(b));
        self.open(x)
            && (x == y && self.open(y) || self.index.edge_open(x, y, true, |t| self.open(t)))
            && !spatial::crosses(a, b, self.index.fences())
    }
    fn query(
        &self,
        start: Position,
        target: Target,
        bounds: Option<(i32, i32, i32, i32)>,
    ) -> Result<Query, Error> {
        let mut terrain = self.terrain.clone();
        if let Some((a, b, c, d)) = bounds {
            terrain.bounds = Some(if let Some((x, y, z, w)) = terrain.bounds {
                (a.max(x), b.min(y), c.max(z), d.min(w))
            } else {
                (a, b, c, d)
            });
        }
        Query::new(
            self.index.clone(),
            terrain,
            Access::default(),
            rounded(start),
            target,
            true,
            RoadMode::Smooth,
        )
        .map_err(|_| Error::Invalid)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Waiting,
    Advanced,
    RouteRequested(Seconds),
    Unloaded(Seconds),
    Completed,
    NeedsReconcile,
}
impl Livestock {
    fn validate(&self, town: &Town) -> Result<(), Error> {
        if self.owner != *town.owner() {
            return Err(Error::Invalid);
        }
        Ok(())
    }
    fn barn(&self, id: &BuildingId) -> Result<&Barn, Error> {
        id.identity()
            .validate_owner(&self.owner)
            .map_err(|_| Error::Invalid)?;
        self.barns
            .iter()
            .find(|b| b.id == id.identity().serial())
            .ok_or(Error::Invalid)
    }
    fn barn_mut(&mut self, id: &BuildingId) -> Result<&mut Barn, Error> {
        id.identity()
            .validate_owner(&self.owner)
            .map_err(|_| Error::Invalid)?;
        self.barns
            .iter_mut()
            .find(|b| b.id == id.identity().serial())
            .ok_or(Error::Invalid)
    }
    pub fn animals(&self, id: &BuildingId, species: Species) -> Result<&[Animal], Error> {
        Ok(self
            .barn(id)?
            .herds
            .iter()
            .find(|h| h.species == species)
            .map_or(&[], |h| h.animals.as_slice()))
    }
    fn name(&mut self, reserved: &mut BTreeSet<String>) -> Result<String, Error> {
        loop {
            let n = self.name_sequence;
            self.name_sequence = n.checked_add(1).ok_or(Error::Invalid)?;
            let base = NAMES[(n % 32) as usize];
            let name = if n < 32 {
                base.into()
            } else {
                format!("{base} {}", n / 32 + 1)
            };
            if reserved.insert(name.clone()) {
                return Ok(name);
            }
        }
    }
    fn fresh(&mut self, reserved: &mut BTreeSet<String>) -> Result<Animal, Error> {
        let id = self.next_animal;
        self.next_animal = id.checked_add(1).ok_or(Error::Invalid)?;
        Ok(Animal {
            id,
            name: self.name(reserved)?,
            age: 0.,
        })
    }
    fn reserved(&self) -> BTreeSet<String> {
        self.barns
            .iter()
            .flat_map(|b| {
                b.herds
                    .iter()
                    .flat_map(|h| h.animals.iter().map(|a| a.name.clone()))
            })
            .chain(self.orders.iter().map(|o| o.animal.name.clone()))
            .collect()
    }
    pub fn admit(
        town: &Town,
        ledger: &Ledger,
        inputs: Vec<(BuildingId, Vec<HerdInput>)>,
        name_sequence: u64,
    ) -> Result<Self, Error> {
        let mut result = Self {
            owner: town.owner().clone(),
            barns: vec![],
            keepers: vec![],
            orders: vec![],
            name_sequence,
            next_animal: 1,
            next_order: 1,
        };
        let mut reserved: BTreeSet<_> = inputs
            .iter()
            .flat_map(|(_, hs)| {
                hs.iter()
                    .flat_map(|h| h.names.iter().filter(|n| !n.trim().is_empty()).cloned())
            })
            .collect();
        let mut seen = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for (id, herds) in inputs {
            town.building(&id)?;
            if ledger.store(&id)?.policy.kind != "barn" || !ids.insert(id.clone()) {
                return Err(Error::Invalid);
            }
            let mut output = vec![];
            let mut species = BTreeSet::new();
            for h in herds {
                if !species.insert(h.species) {
                    return Err(Error::Invalid);
                }
                let mut animals = vec![];
                for i in 0..h.count {
                    let age = h.ages.get(i).copied().unwrap_or(h.species.duration());
                    if !age.is_finite() {
                        return Err(Error::Invalid);
                    }
                    let serial = result.next_animal;
                    result.next_animal = serial.checked_add(1).ok_or(Error::Invalid)?;
                    let saved = h
                        .names
                        .get(i)
                        .filter(|n| !n.trim().is_empty() && !seen.contains(*n));
                    let name = if let Some(n) = saved {
                        n.clone()
                    } else {
                        result.name(&mut reserved)?
                    };
                    seen.insert(name.clone());
                    reserved.insert(name.clone());
                    animals.push(Animal {
                        id: serial,
                        name,
                        age: age.clamp(0., h.species.duration()),
                    });
                }
                output.push(Herd {
                    species: h.species,
                    animals,
                });
            }
            result.barns.push(Barn {
                id: id.identity().serial(),
                herds: output,
                last_growth: 0,
            });
        }
        Ok(result)
    }
    pub fn register_barn(
        &mut self,
        town: &Town,
        ledger: &Ledger,
        id: &BuildingId,
    ) -> Result<(), Error> {
        self.validate(town)?;
        town.building(id)?;
        if ledger.store(id)?.policy.kind != "barn"
            || self.barns.iter().any(|b| b.id == id.identity().serial())
        {
            return Err(Error::Invalid);
        }
        self.barns.push(Barn {
            id: id.identity().serial(),
            herds: vec![],
            last_growth: 0,
        });
        Ok(())
    }
    pub fn set_count(
        &mut self,
        town: &Town,
        ledger: &Ledger,
        id: &BuildingId,
        species: Species,
        count: usize,
        hall: u8,
    ) -> Result<(), Error> {
        self.validate(town)?;
        if !complete(town, ledger, id, "barn")
            || u64::from(hall) < species.spec()["level"].as_u64().unwrap()
        {
            return Err(Error::Invalid);
        }
        let barn = self.barn(id)?;
        let others: usize = barn
            .herds
            .iter()
            .filter(|h| h.species != species)
            .map(|h| h.animals.len())
            .sum();
        if count.checked_add(others).ok_or(Error::Invalid)?
            > 4 * usize::from(ledger.store(id)?.policy.level)
        {
            return Err(Error::Invalid);
        }
        let mut owner = self.clone();
        let mut animals = owner.animals(id, species)?.to_vec();
        animals.truncate(count);
        let mut reserved = owner.reserved();
        while animals.len() < count {
            animals.push(owner.fresh(&mut reserved)?);
        }
        let barn = owner.barn_mut(id)?;
        if let Some(h) = barn.herds.iter_mut().find(|h| h.species == species) {
            h.animals = animals;
        } else {
            barn.herds.push(Herd { species, animals });
        }
        *self = owner;
        Ok(())
    }
    pub fn buy_animal(
        &mut self,
        town: &Town,
        ledger: &mut Ledger,
        id: &BuildingId,
        species: Species,
        hall: u8,
    ) -> Result<(), Error> {
        let (mut owner, mut goods) = (self.clone(), ledger.clone());
        let count = owner
            .animals(id, species)?
            .len()
            .checked_add(1)
            .ok_or(Error::Invalid)?;
        owner.set_count(town, &goods, id, species, count, hall)?;
        let price = species.spec()["coins"].as_f64().unwrap();
        goods.spend(&Stock::from_entries([(key("coins"), quantity(price)?)])?)?;
        *self = owner;
        *ledger = goods;
        Ok(())
    }
    /// Called once per barn per explicit host update. Existing barns keep base
    /// growth during upgrades; R12 supplies initial_construction classification.
    /// Keeper route/cap/buffer pauses never slow this independent growth clock.
    #[allow(clippy::too_many_arguments)]
    pub fn grow(
        &mut self,
        town: &Town,
        ledger: &mut Ledger,
        id: &BuildingId,
        elapsed: Seconds,
        initial_construction: bool,
        update: u64,
    ) -> Result<Stock, Error> {
        self.validate(town)?;
        town.building(id)?;
        let last = self.barn(id)?.last_growth;
        if update == 0 {
            return Err(Error::Invalid);
        }
        if update <= last {
            return Ok(Stock::default());
        }
        let (mut owner, mut goods) = (self.clone(), ledger.clone());
        owner.barn_mut(id)?.last_growth = update;
        if initial_construction {
            *self = owner;
            return Ok(Stock::default());
        }
        let keepers = self
            .keepers
            .iter()
            .filter(|k| k.barn == id.identity().serial())
            .filter(|k| {
                aid(town, k.actor).is_ok_and(|a| {
                    town.actor(&a)
                        .is_ok_and(|a| !matches!(a.state().task, Task::Construction { .. }))
                })
            })
            .count();
        let step = Seconds::new(elapsed.get() * (1. + keepers as f64))
            .map_err(|_| Error::Invalid)?
            .get();
        let mut outputs = vec![];
        let mut growing = 0.;
        for h in &mut owner.barn_mut(id)?.herds {
            let mut advanced = 0.;
            for a in &mut h.animals {
                let take = step.min((h.species.duration() - a.age).max(0.));
                a.age += take;
                advanced += take;
            }
            growing += advanced;
            let n = crate::resources::public_catalog()["animal_products"][h.species.name()]
                .as_f64()
                .unwrap_or(0.)
                * advanced
                / h.species.duration();
            if n > 0. {
                outputs.push((
                    ResourceKey::parse(h.species.spec()["output"].as_str().unwrap())?,
                    n,
                ));
            }
        }
        outputs.push((key("manure"), 0.1 * growing / 3.));
        let mut clipped = vec![];
        for (k, n) in outputs {
            let n = n
                .min(goods.store(id)?.local_room(k.as_str())?)
                .min(goods.production_room(&k)?);
            if n > 1e-9 {
                clipped.push((k, quantity(n)?));
            }
        }
        let output = Stock::from_entries(clipped)?;
        if !output.entries().is_empty() {
            goods.produce_barn_growth(id, &output)?;
        }
        *self = owner;
        *ledger = goods;
        Ok(output)
    }
}
