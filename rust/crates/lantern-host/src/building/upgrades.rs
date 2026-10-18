use super::*;
use lantern_core::{
    model::{Cargo, Task},
    units::{Seconds, Work},
};

#[derive(Clone, Debug, PartialEq)]
pub enum Upgrade {
    Structure(u8),
    Capacity(u8),
    Equipment(String),
    Hall { level: u8, reward: Quantity },
}
/// Explicit observations owned by the future R13 economy, not derived from
/// imported stock or fabricated by construction. Construction never adds output.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Progression {
    pub skills: BTreeMap<String, Quantity>,
    pub produced: BTreeMap<String, Quantity>,
    pub food: Option<Quantity>,
    pub completed_trades: u64,
    pub market_sales: Option<Quantity>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum ChildState {
    Field {
        stage: String,
        elapsed: Seconds,
        progress: Work,
        cycle: u64,
    },
    Tree {
        stage: String,
        elapsed: Seconds,
        fruit: Quantity,
    },
}
#[derive(Deserialize)]
pub(super) struct UpgradeCatalog {
    pub capacity_levels: BTreeMap<String, u8>,
    pub hall: Vec<HallSpec>,
    pub equipment: BTreeMap<String, EquipmentSpec>,
    pub processed_food: Vec<String>,
}
#[derive(Deserialize)]
pub(super) struct HallSpec {
    pub level: u8,
    pub costs: BTreeMap<String, f64>,
    pub seconds: f64,
    pub reward: f64,
    pub production: BTreeMap<String, f64>,
    pub processed_food: f64,
}
#[derive(Deserialize)]
pub(super) struct EquipmentSpec {
    pub building: String,
    pub level: u8,
    pub skill: String,
    pub xp: f64,
    pub requires: Option<String>,
    pub costs: BTreeMap<String, f64>,
}
pub(super) fn catalog() -> UpgradeCatalog {
    serde_json::from_str(include_str!("upgrade_catalog.json")).expect("reviewed upgrade catalog")
}
fn structural_max(kind: &str) -> u8 {
    match kind {
        "storehouse" => 8,
        "home" => 4,
        _ => 3,
    }
}
fn upgradable(s: &Site) -> bool {
    !matches!(s.kind.as_str(), "hall" | "well" | "acre" | "orchard_tree")
        && !super::catalog().get(&s.kind).is_some_and(|s| s.decoration)
}

pub(super) fn validate(town: &Town, rules: &Rules) -> Result<(), Error> {
    let specs = catalog();
    for b in town.buildings() {
        let s = &rules.sites[&b.id];
        if s.level == 0
            || s.level > structural_max(&s.kind)
            || s.capacity_level == 0
            || s.capacity_level > *specs.capacity_levels.get(&s.kind).unwrap_or(&1)
        {
            return Err(Error::InvalidFacts);
        }
        for key in &s.equipment {
            if !specs.equipment.get(key).is_some_and(|e| {
                e.building == s.kind && e.requires.as_ref().is_none_or(|r| s.equipment.contains(r))
            }) {
                return Err(Error::InvalidFacts);
            }
        }
        match (&s.parent, &s.child, s.kind.as_str()) {
            (Some(parent), Some(child), "acre" | "orchard_tree") => {
                let p = town.building(parent).map_err(|_| Error::InvalidFacts)?;
                let expected = if s.kind == "acre" { "farm" } else { "orchard" };
                if rules.sites[parent].kind != expected
                    || chebyshev(b.location, p.location) > 3
                    || b.construction != ConstructionState::Complete
                    || s.level != 1
                    || s.capacity_level != 1
                    || !s.equipment.is_empty()
                {
                    return Err(Error::InvalidFacts);
                }
                match child {
                    ChildState::Field { stage, .. }
                        if s.kind == "acre"
                            && [
                                "unplowed",
                                "plowed",
                                "sprouting",
                                "thirsty",
                                "growing",
                                "ripe",
                            ]
                            .contains(&stage.as_str()) => {}
                    ChildState::Tree { stage, .. }
                        if s.kind == "orchard_tree"
                            && ["sapling", "blossom", "fruit", "ripe", "harvested"]
                                .contains(&stage.as_str()) => {}
                    _ => return Err(Error::InvalidFacts),
                }
            }
            (None, None, k) if k != "acre" && k != "orchard_tree" => {}
            _ => return Err(Error::InvalidFacts),
        }
        if let Some(u) = &s.upgrade {
            if b.construction == ConstructionState::Complete {
                return Err(Error::InvalidFacts);
            }
            let valid = match u {
                Upgrade::Structure(l) => {
                    upgradable(s) && *l == s.level + 1 && *l <= structural_max(&s.kind)
                }
                Upgrade::Capacity(l) => {
                    upgradable(s)
                        && *l == s.capacity_level + 1
                        && *l <= *specs.capacity_levels.get(&s.kind).unwrap_or(&1)
                }
                Upgrade::Equipment(key) => specs.equipment.get(key).is_some_and(|e| {
                    e.building == s.kind
                        && !s.equipment.contains(key)
                        && e.requires.as_ref().is_none_or(|r| s.equipment.contains(r))
                }),
                Upgrade::Hall { level, reward } => {
                    s.kind == "hall"
                        && *level == rules.level + 1
                        && specs
                            .hall
                            .iter()
                            .any(|h| h.level == *level && h.reward == reward.get())
                }
            };
            if !valid {
                return Err(Error::InvalidFacts);
            }
        } else if s.kind == "hall" && b.construction != ConstructionState::Complete {
            return Err(Error::InvalidFacts);
        }
    }
    Ok(())
}
pub(super) fn child(
    town: &Town,
    rules: &Rules,
    kind: &str,
    parent: &BuildingId,
    tile: Tile,
) -> Result<Site, Error> {
    let expected = match kind {
        "acre" => "farm",
        "orchard_tree" => "orchard",
        _ => return Err(Error::Unsupported),
    };
    let p = lookup(town, parent)?;
    if rules.sites[parent].kind != expected
        || p.construction != ConstructionState::Complete
        || chebyshev(tile, p.location) > 3
    {
        return Err(Error::InvalidFacts);
    }
    let mut site = Site::new(kind, (1, 1));
    site.parent = Some(parent.clone());
    site.child = Some(if kind == "acre" {
        ChildState::Field {
            stage: "unplowed".into(),
            elapsed: Seconds::new(0.).unwrap(),
            progress: Work::new(0.).unwrap(),
            cycle: 0,
        }
    } else {
        ChildState::Tree {
            stage: "sapling".into(),
            elapsed: Seconds::new(0.).unwrap(),
            fruit: quantity(8.)?,
        }
    });
    Ok(site)
}
pub(super) fn group(town: &Town, rules: &Rules, id: &BuildingId) -> Result<Vec<BuildingId>, Error> {
    lookup(town, id)?;
    Ok(town
        .buildings()
        .iter()
        .filter(|b| &b.id == id || rules.sites[&b.id].parent.as_ref() == Some(id))
        .map(|b| b.id.clone())
        .collect())
}
fn chebyshev(a: Tile, b: Tile) -> i64 {
    (i64::from(a.x) - i64::from(b.x))
        .abs()
        .max((i64::from(a.y) - i64::from(b.y)).abs())
}

impl BuildingWorld {
    pub(super) fn start_upgrade(
        &self,
        town: &mut Town,
        rules: &mut Rules,
        command: &Command,
    ) -> Result<(), Error> {
        let specs = catalog();
        let id = match command {
            Command::UpgradeStructure(id)
            | Command::UpgradeCapacity(id)
            | Command::UpgradeEquipment { id, .. } => id.clone(),
            Command::UpgradeHall => town
                .buildings()
                .iter()
                .find(|b| rules.sites[&b.id].kind == "hall")
                .ok_or(Error::InvalidFacts)?
                .id
                .clone(),
            _ => return Err(Error::Unsupported),
        };
        if lookup(town, &id)?.construction != ConstructionState::Complete {
            return Err(Error::Unfinished);
        }
        let site = &rules.sites[&id];
        let (upgrade, costs, seconds) = match command {
            Command::UpgradeStructure(_) => {
                if !upgradable(site) || site.level >= structural_max(&site.kind) {
                    return Err(Error::Locked);
                }
                let wood = self.catalog[&site.kind]
                    .costs
                    .get("wood")
                    .copied()
                    .unwrap_or(0.)
                    * f64::from(site.level);
                let mut costs = BTreeMap::from([("wood".into(), wood)]);
                if site.kind == "home" && site.level == 3 {
                    costs.insert("stone".into(), 30.);
                }
                (
                    Upgrade::Structure(site.level + 1),
                    costs,
                    30. * f64::from(site.level),
                )
            }
            Command::UpgradeCapacity(_) => {
                if !upgradable(site) || site.capacity_level >= specs.capacity_levels[&site.kind] {
                    return Err(Error::Locked);
                }
                let cost = 20. * f64::from(site.capacity_level);
                // Capacity uses Python's strict wood comparison, not afford's tolerance.
                if !rules.sandbox && stored(town, rules, "wood") < cost {
                    return Err(Error::Materials);
                }
                (
                    Upgrade::Capacity(site.capacity_level + 1),
                    BTreeMap::from([("wood".into(), cost)]),
                    30. * f64::from(site.capacity_level),
                )
            }
            Command::UpgradeEquipment { key, .. } => {
                let e = specs
                    .equipment
                    .get(key)
                    .filter(|e| e.building == site.kind)
                    .ok_or(Error::Unsupported)?;
                if rules.level < e.level
                    || (!rules.sandbox
                        && rules
                            .progression
                            .skills
                            .get(&e.skill)
                            .map_or(0., |q| q.get())
                            < e.xp)
                    || site.equipment.contains(key)
                    || e.requires
                        .as_ref()
                        .is_some_and(|r| !site.equipment.contains(r))
                {
                    return Err(Error::Locked);
                }
                (
                    Upgrade::Equipment(key.clone()),
                    e.costs.clone(),
                    if key == "boat" { 0. } else { 30. },
                )
            }
            Command::UpgradeHall => {
                let h = specs
                    .hall
                    .iter()
                    .find(|h| h.level == rules.level + 1)
                    .ok_or(Error::Locked)?;
                if !rules.sandbox && !milestone(town, rules, &self.landscape, h, &specs) {
                    return Err(Error::Locked);
                }
                (
                    Upgrade::Hall {
                        level: h.level,
                        reward: quantity(h.reward)?,
                    },
                    h.costs.clone(),
                    h.seconds,
                )
            }
            _ => return Err(Error::Unsupported),
        };
        if !rules.sandbox {
            spend(town, rules, &costs)?;
        }
        rules.sites.get_mut(&id).unwrap().upgrade = Some(upgrade);
        town.set_construction(
            &id,
            ConstructionState::InProgress {
                remaining: Work::new(seconds).unwrap(),
            },
        )
        .map_err(|_| Error::Model)?;
        if rules.sandbox || seconds == 0. {
            finish(town, rules, &id)?;
        }
        Ok(())
    }
}
pub(super) fn finish(town: &mut Town, rules: &mut Rules, id: &BuildingId) -> Result<(), Error> {
    if let Some(upgrade) = rules
        .sites
        .get_mut(id)
        .ok_or(Error::MissingBuilding)?
        .upgrade
        .take()
    {
        match upgrade {
            Upgrade::Structure(level) => rules.sites.get_mut(id).unwrap().level = level,
            Upgrade::Capacity(level) => rules.sites.get_mut(id).unwrap().capacity_level = level,
            Upgrade::Equipment(key) => {
                rules.sites.get_mut(id).unwrap().equipment.insert(key);
            }
            Upgrade::Hall { level, reward } => {
                rules.coins = quantity(rules.coins.get() + reward.get())?;
                rules.level = level;
            }
        }
    }
    town.set_construction(id, ConstructionState::Complete)
        .map_err(|_| Error::Model)
}
fn stored(town: &Town, rules: &Rules, key: &str) -> f64 {
    town.buildings()
        .iter()
        .map(|b| rules.sites[&b.id].stored.get(key).map_or(0., |q| q.get()))
        .sum()
}
fn milestone(
    town: &Town,
    r: &Rules,
    land: &Landscape,
    h: &HallSpec,
    specs: &UpgradeCatalog,
) -> bool {
    let operational = |b: &Building| {
        b.construction == ConstructionState::Complete || r.sites[&b.id].upgrade.is_some()
    };
    if h.level == 2 {
        let housing = town
            .buildings()
            .iter()
            .any(|b| r.sites[&b.id].kind == "home" && operational(b));
        let water = land
            .resources
            .iter()
            .filter(|r| r.kind == "water")
            .map(|r| r.footprint.origin())
            .chain(
                town.buildings()
                    .iter()
                    .filter(|b| {
                        r.sites[&b.id].kind == "well"
                            && b.construction == ConstructionState::Complete
                    })
                    .map(|b| b.location),
            )
            .collect::<Vec<_>>();
        let food = town.buildings().iter().any(|b| {
            matches!(r.sites[&b.id].kind.as_str(), "farm" | "fishing_hut")
                && operational(b)
                && water.iter().any(|w| distance(b.location, *w) <= 10)
        });
        return housing && food && r.progression.food.is_some_and(|q| q.get() >= 20.);
    }
    if h.production
        .iter()
        .any(|(k, n)| r.progression.produced.get(k).map_or(0., |q| q.get()) < *n)
    {
        return false;
    }
    if h.processed_food > 0.
        && specs
            .processed_food
            .iter()
            .map(|k| r.progression.produced.get(k).map_or(0., |q| q.get()))
            .sum::<f64>()
            < h.processed_food
    {
        return false;
    }
    h.level != 10
        || r.progression.completed_trades >= 1
        || r.progression.market_sales.is_some_and(|q| q.get() >= 10.)
}
/// Model transport tasks name the actual source. Refund only explicit carried
/// transport cargo; generic work cargo has no imported logistics semantics yet.
pub(super) fn cancel_transports(
    town: &mut Town,
    rules: &mut Rules,
    changed: &[BuildingId],
    removed: bool,
) -> Result<(), Error> {
    let hall = town
        .buildings()
        .iter()
        .find(|b| rules.sites[&b.id].kind == "hall")
        .ok_or(Error::InvalidFacts)?
        .id
        .clone();
    let actors = town
        .actors()
        .iter()
        .filter_map(|a| match &a.state().task {
            Task::Transport {
                source,
                destination,
                ..
            } if changed.contains(source) || changed.contains(destination) => Some((
                a.state().id.clone(),
                source.clone(),
                a.state().cargo.clone(),
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    for (actor, source, cargo) in actors {
        let destination = if (!removed || !changed.contains(&source))
            && town
                .building(&source)
                .is_ok_and(|b| b.construction == ConstructionState::Complete)
        {
            source
        } else {
            hall.clone()
        };
        let stock = &mut rules.sites.get_mut(&destination).unwrap().stored;
        for (key, amount) in cargo.goods() {
            if key == "coins" {
                return Err(Error::InvalidFacts);
            }
            let total = stock.get(key).map_or(0., |q| q.get()) + amount.get();
            stock.insert(key.clone(), quantity(total)?);
        }
        town.set_cargo(&actor, Cargo::default())
            .map_err(|_| Error::Model)?;
        town.set_task(&actor, Task::Idle)
            .map_err(|_| Error::Model)?;
    }
    Ok(())
}

pub(super) fn relocate(
    world: &BuildingWorld,
    town: &mut Town,
    rules: &mut Rules,
    id: &BuildingId,
    tile: Tile,
) -> Result<BTreeSet<Tile>, Error> {
    let original = lookup(town, id)?.location;
    if original == tile {
        return Err(Error::NoChange);
    }
    let ids = group(town, rules, id)?;
    let dx = i64::from(tile.x) - i64::from(original.x);
    let dy = i64::from(tile.y) - i64::from(original.y);
    let mut proposals = vec![];
    let mut clear = BTreeSet::new();
    for key in &ids {
        let b = town.building(key).map_err(|_| Error::Model)?;
        let nx = i32::try_from(i64::from(b.location.x) + dx).map_err(|_| Error::Boundary)?;
        let ny = i32::try_from(i64::from(b.location.y) + dy).map_err(|_| Error::Boundary)?;
        let location = Tile { x: nx, y: ny };
        let mut site = rules.sites[key].clone();
        site.dimensions = spatial::placement_dimensions(&site.kind, i32::from(site.orientation));
        let fp = footprint(location, &site)?;
        world.validate(town, rules, &site, fp, &ids)?;
        if fp.tiles().any(|t| !clear.insert(t)) {
            return Err(Error::Occupied);
        }
        proposals.push((key.clone(), location, site));
    }
    let s = &rules.sites[id];
    if let Some(parent) = &s.parent {
        child(town, rules, &s.kind, parent, tile)?;
    }
    if s.kind == "hall" && world.shared {
        for other in world
            .model
            .towns()
            .iter()
            .filter(|t| t.owner() != town.owner())
        {
            for b in other
                .buildings()
                .iter()
                .filter(|b| world.rules[&other.owner().town].sites[&b.id].kind == "hall")
            {
                let dx = f64::from(b.location.x) - f64::from(tile.x);
                let dy = f64::from(b.location.y) - f64::from(tile.y);
                if dx * dx + dy * dy < 1600. {
                    return Err(Error::HallSeparation);
                }
            }
        }
    }
    cancel_transports(town, rules, &ids, false)?;
    for (key, location, site) in proposals {
        town.relocate_building(&key, location)
            .map_err(|_| Error::Model)?;
        rules.sites.get_mut(&key).unwrap().dimensions = site.dimensions;
    }
    if !rules.sandbox {
        rules.coins = quantity((rules.coins.get() - 1.).max(0.))?;
    }
    Ok(clear)
}
pub(super) fn destroy(town: &mut Town, rules: &mut Rules, id: &BuildingId) -> Result<(), Error> {
    lookup(town, id)?;
    if rules.sites[id].kind == "hall" {
        return Err(Error::HallProtected);
    }
    let ids = group(town, rules, id)?;
    let hall = town
        .buildings()
        .iter()
        .find(|b| rules.sites[&b.id].kind == "hall")
        .ok_or(Error::InvalidFacts)?
        .id
        .clone();
    cancel_transports(town, rules, &ids, true)?;
    for key in &ids {
        let site = &rules.sites[key];
        let salvage = site
            .stored
            .iter()
            .chain(site.pending.iter())
            .map(|(k, v)| (k.clone(), *v))
            .collect::<Vec<_>>();
        for (resource, amount) in salvage {
            let stock = &mut rules.sites.get_mut(&hall).unwrap().stored;
            let total = stock.get(&resource).map_or(0., |q| q.get()) + amount.get();
            stock.insert(resource, quantity(total)?);
        }
        town.remove_building(key).map_err(|_| Error::Model)?;
        rules.sites.remove(key);
    }
    Ok(())
}
