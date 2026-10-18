//! Experimental R12 building commands over explicit synthetic model facts.
//! No HTTP, legacy import, playable saves, or inferred missing feature fields.
//! The model owns locations/actors; these records own construction metadata and
//! inventory. Derived geometry is rebuilt from those facts, never written back.
use std::collections::{BTreeMap, BTreeSet};

use lantern_core::{
    ids::{ActorId, BuildingId, EntityIdentity, Owner, TownId},
    model::{Building, ConstructionState, Town},
    spatial::{self, Construction, Footprint, Resource, Structure},
    units::{Position, Quantity, Tile, Work},
};
use serde::Deserialize;

use crate::state::ModelWorld;

mod construction;
mod save;
mod upgrades;
pub use construction::{Builder, Claim, Plan, PlanResult, Role, Suspended, Walk};
pub use save::{decode_building_save, encode_building_save};
pub use upgrades::{ChildState, Progression, Upgrade};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    MissingTown,
    Ownership,
    InvalidFacts,
    Unsupported,
    MissingBuilding,
    InvalidOrientation,
    Unfinished,
    Occupied,
    Boundary,
    Range,
    Territory,
    Pen,
    Player,
    Local,
    HallSeparation,
    HallProtected,
    NoChange,
    Locked,
    Materials,
    Exhausted,
    Model,
    Quantity,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    pub level: u8,
    pub costs: BTreeMap<String, f64>,
    pub decoration: bool,
}
/// Committed data is generated from Python, not a second hand-maintained catalog.
pub fn catalog() -> BTreeMap<String, Spec> {
    serde_json::from_str(include_str!("building_catalog.json"))
        .expect("reviewed construction catalog")
}

#[derive(Clone, Debug, PartialEq)]
pub struct Site {
    pub kind: String,
    /// Saved, already-oriented dimensions, including compact legacy footprints.
    pub dimensions: (u32, u32),
    pub orientation: u8,
    pub fence_orientation: u8,
    pub stored: BTreeMap<String, Quantity>,
    pub pending: BTreeMap<String, Quantity>,
    pub parent: Option<BuildingId>,
    pub child: Option<ChildState>,
    pub level: u8,
    pub capacity_level: u8,
    pub equipment: BTreeSet<String>,
    pub upgrade: Option<Upgrade>,
}
impl Site {
    pub fn new(kind: impl Into<String>, dimensions: (u32, u32)) -> Self {
        Self {
            kind: kind.into(),
            dimensions,
            orientation: 0,
            fence_orientation: 0,
            stored: BTreeMap::new(),
            pending: BTreeMap::new(),
            parent: None,
            child: None,
            level: 1,
            capacity_level: 1,
            equipment: BTreeSet::new(),
            upgrade: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Road {
    pub tile: Tile,
    pub remaining: Option<Work>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Rules {
    pub level: u8,
    pub sandbox: bool,
    pub origin: Tile,
    pub player: Position,
    pub coins: Quantity,
    pub sites: BTreeMap<BuildingId, Site>,
    pub roads: Vec<Road>,
    pub progression: Progression,
}
#[derive(Clone)]
pub struct Landscape {
    pub size: u32,
    /// Explicit unavailable/void cells. Caller must materialize the command area;
    /// this slice has no chunk loading or private legacy-world mutation bridge.
    pub unavailable: BTreeSet<Tile>,
    pub resources: Vec<Resource>,
    pub locals: Vec<Footprint>,
    /// Actual routine positions for the fence safety check.
    pub local_positions: Vec<Position>,
}

pub enum Command {
    BuildChild {
        kind: String,
        tile: Tile,
        parent: BuildingId,
        orientation: u8,
    },
    UpgradeStructure(BuildingId),
    UpgradeCapacity(BuildingId),
    UpgradeEquipment {
        id: BuildingId,
        key: String,
    },
    UpgradeHall,
    Build {
        kind: String,
        tile: Tile,
        orientation: u8,
    },
    Move {
        id: BuildingId,
        tile: Tile,
    },
    Destroy {
        id: BuildingId,
    },
    Rotate {
        id: BuildingId,
        orientation: u8,
        decoration: bool,
    },
    RotateFence {
        id: BuildingId,
        orientation: u8,
    },
    BuildRoad(Tile),
    RemoveRoad(Tile),
}

/// Exclusive command owner. Construction records must accompany every model
/// building on restoration: no unsupported field is silently defaulted.
pub struct BuildingWorld {
    model: ModelWorld,
    rules: BTreeMap<TownId, Rules>,
    landscape: Landscape,
    shared: bool,
    catalog: BTreeMap<String, Spec>,
    builders: BTreeMap<ActorId, Builder>,
}
impl BuildingWorld {
    pub fn restore(
        model: ModelWorld,
        rules: BTreeMap<TownId, Rules>,
        landscape: Landscape,
        shared: bool,
    ) -> Result<Self, Error> {
        if landscape.size == 0
            || landscape.size > i32::MAX as u32
            || rules.len() != model.towns().len()
        {
            return Err(Error::InvalidFacts);
        }
        // A non-shared world contains exactly one independent landscape/town.
        if !shared && model.towns().len() != 1 {
            return Err(Error::InvalidFacts);
        }
        let catalog = catalog();
        for town in model.towns() {
            let r = rules.get(&town.owner().town).ok_or(Error::InvalidFacts)?;
            if !(1..=10).contains(&r.level) || r.sites.len() != town.buildings().len() {
                return Err(Error::InvalidFacts);
            }
            let mut halls = 0;
            for building in town.buildings() {
                let site = r.sites.get(&building.id).ok_or(Error::InvalidFacts)?;
                if site.kind == "hall" {
                    halls += 1;
                } else if !catalog.contains_key(&site.kind) {
                    return Err(Error::Unsupported);
                }
                if site.orientation > 3 || site.fence_orientation > 3 {
                    return Err(Error::InvalidOrientation);
                }
                if site
                    .stored
                    .keys()
                    .chain(site.pending.keys())
                    .any(|k| k.is_empty() || k == "coins")
                {
                    return Err(Error::InvalidFacts);
                }
                let fp = Footprint::saved(building.location, Some(site.dimensions))
                    .map_err(|_| Error::InvalidFacts)?;
                if fp.tiles().any(|t| !inside(t, landscape.size)) {
                    return Err(Error::InvalidFacts);
                }
            }
            upgrades::validate(town, r)?;
            if halls != 1 {
                return Err(Error::InvalidFacts);
            }
            let mut roads = BTreeSet::new();
            if r.roads
                .iter()
                .any(|r| !inside(r.tile, landscape.size) || !roads.insert(r.tile))
            {
                return Err(Error::InvalidFacts);
            }
        }
        Ok(Self {
            model,
            rules,
            landscape,
            shared,
            catalog,
            builders: BTreeMap::new(),
        })
    }
    pub fn model(&self) -> &ModelWorld {
        &self.model
    }
    pub fn rules(&self) -> &BTreeMap<TownId, Rules> {
        &self.rules
    }
    pub fn landscape(&self) -> &Landscape {
        &self.landscape
    }
    /// Explicit facts for offline restoration. Durable save integration is R19;
    /// the generic lantern-model DTO does not contain these extra facts.
    pub fn into_parts(
        self,
    ) -> (
        ModelWorld,
        BTreeMap<TownId, Rules>,
        Landscape,
        bool,
        BTreeMap<ActorId, Builder>,
    ) {
        (
            self.model,
            self.rules,
            self.landscape,
            self.shared,
            self.builders,
        )
    }

    /// Owner is authenticated by the future host boundary, never client supplied.
    /// Stage candidate facts, then commit once. A failed command cannot consume
    /// IDs, coins, inventory, forage or actor generations.
    pub fn execute(
        &mut self,
        owner: &Owner,
        command: Command,
    ) -> Result<Option<BuildingId>, Error> {
        if &owner.world != self.model.id() {
            return Err(Error::Ownership);
        }
        let index = self
            .model
            .towns
            .iter()
            .position(|t| t.owner() == owner)
            .ok_or(Error::MissingTown)?;
        let mut town = self.model.towns[index].clone();
        let mut rules = self
            .rules
            .get(&owner.town)
            .ok_or(Error::MissingTown)?
            .clone();
        let mut clear = BTreeSet::new();
        let mut created = None;
        let mut builders = self.builders.clone();
        match command {
            Command::Build {
                kind,
                tile,
                orientation,
            } => {
                if orientation > 3 {
                    return Err(Error::InvalidOrientation);
                }
                if kind == "acre" || kind == "orchard_tree" {
                    return Err(Error::Unsupported);
                }
                let spec = self.catalog.get(&kind).ok_or(Error::Unsupported)?;
                let dimensions = spatial::placement_dimensions(&kind, i32::from(orientation));
                let site = Site {
                    orientation,
                    ..Site::new(kind, dimensions)
                };
                let fp = footprint(tile, &site)?;
                self.validate(&town, &rules, &site, fp, &[])?;
                if rules.level < spec.level {
                    return Err(Error::Locked);
                }
                if !rules.sandbox {
                    spend(&town, &mut rules, &spec.costs)?;
                }
                let serial = self
                    .model
                    .high_water()
                    .checked_add(1)
                    .ok_or(Error::Exhausted)?;
                let id = BuildingId::from_saved(
                    EntityIdentity::from_saved(owner.clone(), serial)
                        .map_err(|_| Error::Exhausted)?,
                );
                let construction = if rules.sandbox || spec.decoration {
                    ConstructionState::Complete
                } else {
                    ConstructionState::InProgress {
                        remaining: Work::new(30.0).unwrap(),
                    }
                };
                town.insert_building(Building {
                    id: id.clone(),
                    location: tile,
                    construction,
                })
                .map_err(|_| Error::Model)?;
                rules.sites.insert(id.clone(), site);
                clear.extend(fp.tiles());
                created = Some(id);
            }
            Command::Move { id, tile } => {
                clear.extend(upgrades::relocate(self, &mut town, &mut rules, &id, tile)?);
            }
            Command::Destroy { id } => {
                upgrades::destroy(&mut town, &mut rules, &id)?;
            }
            Command::BuildChild {
                kind,
                tile,
                parent,
                orientation,
            } => {
                if orientation > 3 {
                    return Err(Error::InvalidOrientation);
                }
                let mut site = upgrades::child(&town, &rules, &kind, &parent, tile)?;
                site.orientation = orientation;
                let fp = footprint(tile, &site)?;
                self.validate(&town, &rules, &site, fp, &[])?;
                let spec = &self.catalog[&kind];
                if rules.level < spec.level {
                    return Err(Error::Locked);
                }
                if !rules.sandbox {
                    spend(&town, &mut rules, &spec.costs)?;
                }
                let serial = self
                    .model
                    .high_water()
                    .checked_add(1)
                    .ok_or(Error::Exhausted)?;
                let id = BuildingId::from_saved(
                    EntityIdentity::from_saved(owner.clone(), serial)
                        .map_err(|_| Error::Exhausted)?,
                );
                town.insert_building(Building {
                    id: id.clone(),
                    location: tile,
                    construction: ConstructionState::Complete,
                })
                .map_err(|_| Error::Model)?;
                rules.sites.insert(id.clone(), site);
                clear.extend(fp.tiles());
                created = Some(id);
            }
            command @ (Command::UpgradeStructure(_)
            | Command::UpgradeCapacity(_)
            | Command::UpgradeEquipment { .. }
            | Command::UpgradeHall) => {
                self.start_upgrade(&mut town, &mut rules, &command)?;
            }
            Command::Rotate {
                id,
                orientation,
                decoration,
            } => {
                if orientation > 3 {
                    return Err(Error::InvalidOrientation);
                }
                let b = lookup(&town, &id)?;
                if b.construction != ConstructionState::Complete {
                    return Err(Error::Unfinished);
                }
                let mut site = rules.sites[&id].clone();
                let is_decoration = self.catalog.get(&site.kind).is_some_and(|s| s.decoration);
                if decoration && !is_decoration {
                    return Err(Error::Unsupported);
                }
                let before = footprint(b.location, &site)?;
                if orientation % 2 != site.orientation % 2 {
                    site.dimensions = (site.dimensions.1, site.dimensions.0);
                }
                site.orientation = orientation;
                let after = footprint(b.location, &site)?;
                if before != after {
                    self.validate(&town, &rules, &site, after, std::slice::from_ref(&id))?;
                    clear.extend(after.tiles());
                }
                rules.sites.insert(id, site);
            }
            Command::RotateFence { id, orientation } => {
                if orientation > 3 {
                    return Err(Error::InvalidOrientation);
                }
                let b = lookup(&town, &id)?;
                if b.construction != ConstructionState::Complete
                    && rules.sites[&id].upgrade.is_none()
                {
                    return Err(Error::Unfinished);
                }
                if rules.sites[&id].kind != "barn" {
                    return Err(Error::Unsupported);
                }
                let pen = spatial::pen(b.location, self.landscape.size, i32::from(orientation))
                    .map_err(|_| Error::Boundary)?;
                if pen.orientation != usize::from(orientation) {
                    return Err(Error::Boundary);
                }
                let mut positions = self.landscape.local_positions.clone();
                for t in self
                    .model
                    .towns()
                    .iter()
                    .filter(|t| self.shared || t.owner() == owner)
                {
                    positions.push(self.rules[&t.owner().town].player);
                    positions.extend(t.actors().iter().map(|a| a.state().position));
                }
                if positions
                    .into_iter()
                    .any(|p| spatial::fence_blocked(p, &pen.segments, 0.18))
                {
                    return Err(Error::Player);
                }
                rules.sites.get_mut(&id).unwrap().fence_orientation = orientation;
            }
            Command::BuildRoad(tile) => {
                self.validate_road(&town, &rules, tile)?;
                if !rules.roads.iter().any(|r| r.tile == tile) {
                    rules.roads.push(Road {
                        tile,
                        remaining: (!rules.sandbox).then(|| Work::new(10.0).unwrap()),
                    });
                }
                clear.insert(tile);
            }
            Command::RemoveRoad(tile) => {
                if !inside(tile, self.landscape.size) {
                    return Err(Error::Boundary);
                }
                rules.roads.retain(|r| r.tile != tile);
            }
        }
        construction::reconcile(&mut town, &rules, &mut builders)?;
        if let Some(id) = &created {
            let allocated: BuildingId = self
                .model
                .allocate(&owner.town)
                .map_err(|_| Error::Exhausted)?;
            debug_assert_eq!(&allocated, id);
        }
        self.model.towns[index] = town;
        self.rules.insert(owner.town.clone(), rules);
        self.builders = builders;
        self.landscape
            .resources
            .retain(|r| !forage(&r.kind) || !clear.contains(&r.footprint.origin()));
        Ok(created)
    }

    /// Read-only geometry projection for navigation; no index owns gameplay facts.
    pub fn structures(&self) -> Vec<Structure> {
        self.model
            .towns()
            .iter()
            .flat_map(|t| {
                let r = &self.rules[&t.owner().town];
                t.buildings().iter().map(|b| structure(b, &r.sites[&b.id]))
            })
            .collect()
    }

    fn validate(
        &self,
        town: &Town,
        rules: &Rules,
        site: &Site,
        fp: Footprint,
        moving: &[BuildingId],
    ) -> Result<(), Error> {
        let others = self
            .model
            .towns()
            .iter()
            .filter(|t| self.shared || t.owner() == town.owner())
            .flat_map(|t| {
                let r = &self.rules[&t.owner().town];
                t.buildings()
                    .iter()
                    .filter(|b| !moving.contains(&b.id))
                    .map(|b| structure(b, &r.sites[&b.id]))
            })
            .collect::<Vec<_>>();
        if spatial::violates_pen(fp, site.kind == "barn", &others) {
            return Err(Error::Pen);
        }
        let occupied = others
            .iter()
            .flat_map(|b| b.footprint.tiles())
            .collect::<BTreeSet<_>>();
        let multi = fp.dimensions() != (1, 1);
        for tile in fp.tiles() {
            self.validate_tile(town, rules, tile, true)?;
            if occupied.contains(&tile) {
                return Err(Error::Occupied);
            }
            if multi
                && self.rules.iter().any(|(id, r)| {
                    (self.shared || id == &town.owner().town)
                        && r.roads.iter().any(|r| r.tile == tile)
                })
            {
                return Err(Error::Occupied);
            }
        }
        Ok(())
    }
    fn validate_tile(
        &self,
        town: &Town,
        rules: &Rules,
        tile: Tile,
        own_player: bool,
    ) -> Result<(), Error> {
        if !inside(tile, self.landscape.size) || self.landscape.unavailable.contains(&tile) {
            return Err(Error::Boundary);
        }
        let hall = town
            .buildings()
            .iter()
            .find(|b| rules.sites[&b.id].kind == "hall")
            .ok_or(Error::InvalidFacts)?;
        let reach = 30 + 5 * i64::from(rules.level - 1);
        if distance(tile, hall.location) > reach {
            return Err(Error::Range);
        }
        if self.shared && distance(tile, rules.origin) > reach.max(15) {
            return Err(Error::Territory);
        }
        if self
            .landscape
            .resources
            .iter()
            .any(|r| !forage(&r.kind) && r.footprint.tiles().any(|t| t == tile))
        {
            return Err(Error::Occupied);
        }
        if self
            .landscape
            .locals
            .iter()
            .any(|n| n.tiles().any(|t| t == tile))
        {
            return Err(Error::Local);
        }
        // Standalone roads permit the local founder; shared placement checks all
        // founders. Mobile workers do not reserve placement tiles in Python.
        for (id, r) in &self.rules {
            if (self.shared || (own_player && id == &town.owner().town)) && near(r.player, tile) {
                return Err(Error::Player);
            }
        }
        if self.shared {
            for other in self
                .model
                .towns()
                .iter()
                .filter(|t| t.owner() != town.owner())
            {
                if other.buildings().iter().any(|b| {
                    footprint(b.location, &self.rules[&other.owner().town].sites[&b.id])
                        .unwrap()
                        .tiles()
                        .any(|t| t == tile)
                }) {
                    return Err(Error::Occupied);
                }
            }
        }
        Ok(())
    }
    fn validate_road(&self, town: &Town, rules: &Rules, tile: Tile) -> Result<(), Error> {
        self.validate_tile(town, rules, tile, false)?;
        for b in town.buildings() {
            if footprint(b.location, &rules.sites[&b.id])?
                .tiles()
                .any(|t| t == tile)
            {
                return Err(Error::Occupied);
            }
        }
        Ok(())
    }
}
fn lookup<'a>(town: &'a Town, id: &BuildingId) -> Result<&'a Building, Error> {
    if id.identity().owner() != town.owner() {
        return Err(Error::Ownership);
    }
    town.building(id).map_err(|_| Error::MissingBuilding)
}
fn inside(t: Tile, size: u32) -> bool {
    t.x >= 0 && t.y >= 0 && (t.x as u32) < size && (t.y as u32) < size
}
fn distance(a: Tile, b: Tile) -> i64 {
    (i64::from(a.x) - i64::from(b.x)).abs() + (i64::from(a.y) - i64::from(b.y)).abs()
}
fn near(p: Position, t: Tile) -> bool {
    (p.x.get() - f64::from(t.x)).abs() < 0.68 && (p.y.get() - f64::from(t.y)).abs() < 0.68
}
fn forage(kind: &str) -> bool {
    kind == "berry_bush" || kind == "herb_patch"
}
fn quantity(value: f64) -> Result<Quantity, Error> {
    Quantity::new(value).map_err(|_| Error::Quantity)
}
fn footprint(tile: Tile, site: &Site) -> Result<Footprint, Error> {
    Footprint::saved(tile, Some(site.dimensions)).map_err(|_| Error::Boundary)
}
fn structure(b: &Building, s: &Site) -> Structure {
    Structure {
        kind: s.kind.clone(),
        footprint: footprint(b.location, s).expect("validated geometry"),
        construction: if b.construction == ConstructionState::Complete {
            Construction::Complete
        } else if s.upgrade.is_some() {
            Construction::Upgrade
        } else {
            Construction::Site
        },
        fence_orientation: i32::from(s.fence_orientation),
    }
}
/// R12's explicit inventory-only cost boundary. Pending output and actor cargo
/// are never spendable. R13 owns reservations, edible aggregates and activity DTOs.
fn spend(town: &Town, rules: &mut Rules, costs: &BTreeMap<String, f64>) -> Result<(), Error> {
    for (key, amount) in costs {
        let available: f64 = if key == "coins" {
            rules.coins.get()
        } else {
            town.buildings()
                .iter()
                .map(|b| rules.sites[&b.id].stored.get(key).map_or(0.0, |q| q.get()))
                .sum()
        };
        if available + 1e-9 < *amount {
            return Err(Error::Materials);
        }
    }
    for (key, amount) in costs {
        if key == "coins" {
            rules.coins = quantity((rules.coins.get() - amount).max(0.0))?;
            continue;
        }
        let mut remaining = *amount;
        for b in town.buildings() {
            if let Some(stock) = rules.sites.get_mut(&b.id).unwrap().stored.get_mut(key) {
                let taken = stock.get().min(remaining);
                *stock = quantity(stock.get() - taken)?;
                remaining = (remaining - taken).max(0.0);
            }
        }
    }
    Ok(())
}
