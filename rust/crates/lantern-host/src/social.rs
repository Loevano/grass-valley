//! Experimental R18 social commands over explicit personal/ledger facts. No HTTP,
//! wall clock, live save writes or inferred authority from a client town index.
use crate::{
    population::Population,
    resources::{Ledger, ResourceKey, Stock},
};
use lantern_core::{
    ids::Owner,
    model::Town,
    units::{Position, Quantity},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, sync::OnceLock};
pub mod activity;
pub mod trading;
pub use activity::{FounderLand, Ground, Resource, Stump};
#[derive(Debug, PartialEq)]
pub enum Error {
    Rejected(&'static str),
    Resource(crate::resources::ResourceError),
}
impl From<crate::resources::ResourceError> for Error {
    fn from(e: crate::resources::ResourceError) -> Self {
        Self::Resource(e)
    }
}
fn reject(s: &'static str) -> Error {
    Error::Rejected(s)
}
pub(crate) fn catalog() -> &'static Value {
    static C: OnceLock<Value> = OnceLock::new();
    C.get_or_init(|| serde_json::from_str(include_str!("social/catalog.json")).unwrap())
}
fn key(s: &str) -> Result<ResourceKey, Error> {
    Ok(ResourceKey::parse(s)?)
}
fn q(n: f64) -> Result<Quantity, Error> {
    Quantity::new(n).map_err(|_| reject("invalid quantity"))
}
fn stock(k: &str, n: f64) -> Result<Stock, Error> {
    Ok(Stock::from_entries([(key(k)?, q(n)?)])?)
}
fn index(v: &Value, k: &str, n: usize) -> Result<usize, Error> {
    v[k].as_u64()
        .filter(|i| *i < n as u64)
        .map(|i| i as usize)
        .ok_or_else(|| reject("invalid index"))
}
fn item(v: &Value) -> Result<&str, Error> {
    v["item"].as_str().ok_or_else(|| reject("invalid item"))
}
fn increment(m: &mut BTreeMap<String, u64>, k: &str) -> Result<(), Error> {
    let n = m.entry(k.into()).or_default();
    *n = n
        .checked_add(1)
        .ok_or_else(|| reject("inventory overflow"))?;
    Ok(())
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Player {
    pub items: BTreeMap<String, u64>,
    pub worker_tools: BTreeMap<String, u64>,
    pub equipment: BTreeMap<String, String>,
    pub keepsakes: Vec<String>,
    pub practice: BTreeMap<String, f64>,
    #[serde(deserialize_with = "required")]
    pub activity: Option<activity::Activity>,
    #[serde(deserialize_with = "required")]
    pub activity_status: Option<String>,
    pub facing: i8,
    pub last_batch: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Visit {
    #[serde(default)]
    pub met: bool,
    #[serde(default)]
    pub gift_claimed: bool,
    #[serde(default)]
    pub story_stage: u64,
    #[serde(default)]
    pub story_rocks: Vec<String>,
    #[serde(default)]
    pub trade_remaining: f64,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Local {
    pub name: String,
    pub village: Option<String>,
    pub key: String,
    pub position: Position,
}
pub struct Context<'a> {
    pub world: &'a lantern_core::ids::WorldId,
    pub town: &'a Town,
    pub position: Position,
    pub locals: &'a [Local],
    pub rocks: &'a [[i32; 2]],
    pub completed_roads: u64,
    pub ground: &'a [Ground],
}
#[derive(Clone, Debug, PartialEq)]
pub struct Social {
    owner: Owner,
    player: Player,
    visits: BTreeMap<String, Visit>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    version: u8,
    world: String,
    town: String,
    player: Player,
    visits: BTreeMap<String, Visit>,
}
impl Social {
    pub fn restore(
        owner: Owner,
        mut player: Player,
        visits: BTreeMap<String, Visit>,
    ) -> Result<Self, Error> {
        // Default-facing admission is explicit; preserve saved -1/1.
        if player.facing == 0 {
            player.facing = 1;
        }
        let s = Self {
            owner,
            player,
            visits,
        };
        s.validate()?;
        Ok(s)
    }
    fn validate(&self) -> Result<(), Error> {
        if !matches!(self.player.facing, -1 | 1)
            || self
                .player
                .practice
                .values()
                .any(|n| !n.is_finite() || *n < 0.)
            || self
                .visits
                .values()
                .any(|v| !v.trade_remaining.is_finite() || v.trade_remaining < 0.)
        {
            return Err(reject("invalid personal state"));
        }
        if self.visits.values().any(|v| {
            v.extra.keys().any(|k| {
                matches!(
                    k.as_str(),
                    "met" | "gift_claimed" | "story_stage" | "story_rocks" | "trade_remaining"
                )
            })
        }) {
            return Err(reject("duplicate visit fields"));
        }
        for (slot, k) in &self.player.equipment {
            if catalog()["equipment"][k]["slot"].as_str() != Some(slot) {
                return Err(reject("invalid equipped slot"));
            }
        }
        for k in self.player.items.keys() {
            if catalog()["equipment"].get(k).is_none() {
                return Err(reject("unsupported inventory item"));
            }
        }
        for k in self.player.worker_tools.keys() {
            if catalog()["tools"].get(k).is_none() {
                return Err(reject("unsupported worker tool"));
            }
        }
        if let Some(a) = &self.player.activity {
            a.validate()?;
        }
        Ok(())
    }
    pub fn player(&self) -> &Player {
        &self.player
    }
    pub fn visits(&self) -> &BTreeMap<String, Visit> {
        &self.visits
    }
    pub fn encode(&self) -> Result<String, Error> {
        self.validate()?;
        serde_json::to_string(&Saved {
            version: 1,
            world: self.owner.world.as_str().into(),
            town: self.owner.town.as_str().into(),
            player: self.player.clone(),
            visits: self.visits.clone(),
        })
        .map_err(|_| reject("invalid social snapshot"))
    }
    pub fn decode(json: &str, town: &Town) -> Result<Self, Error> {
        crate::legacy::validate_social_snapshot(json.as_bytes())
            .map_err(|_| reject("invalid social snapshot"))?;
        let s: Saved = serde_json::from_str(json).map_err(|_| reject("invalid social snapshot"))?;
        if s.version != 1
            || s.world != town.owner().world.as_str()
            || s.town != town.owner().town.as_str()
        {
            return Err(reject("version or ownership mismatch"));
        }
        let result = Self {
            owner: town.owner().clone(),
            player: s.player,
            visits: s.visits,
        };
        result.validate()?;
        Ok(result)
    }
    fn authorize(&self, owner: &Owner, town: &Town, ledger: &Ledger) -> Result<(), Error> {
        if owner != &self.owner || town.owner() != owner || ledger.owner() != owner {
            Err(reject("ownership mismatch"))
        } else {
            Ok(())
        }
    }
    /// Worker receipts are drained and credited in the same transaction.
    pub fn collect_departed_tools(
        &mut self,
        owner: &Owner,
        town: &Town,
        population: &mut Population,
    ) -> Result<(), Error> {
        if owner != &self.owner || town.owner() != owner {
            return Err(reject("ownership mismatch"));
        }
        population
            .encode(town)
            .map_err(|_| reject("invalid resident owner"))?;
        let (mut s, mut p) = (self.clone(), population.clone());
        for (k, n) in p
            .take_returned_tools(owner)
            .map_err(|_| reject("invalid resident owner"))?
        {
            let count = s.player.worker_tools.entry(k).or_default();
            *count = count
                .checked_add(n)
                .ok_or_else(|| reject("tool overflow"))?;
        }
        s.validate()?;
        *self = s;
        *population = p;
        Ok(())
    }
    pub fn change(
        &mut self,
        owner: &Owner,
        context: &Context<'_>,
        ledger: &mut Ledger,
        population: &mut Population,
        data: &Value,
    ) -> Result<(), Error> {
        self.authorize(owner, context.town, ledger)?;
        if context.world != &owner.world {
            return Err(reject("foreign social scene"));
        }
        population
            .encode(context.town)
            .map_err(|_| reject("invalid resident owner"))?;
        let (mut s, mut l, mut p) = (self.clone(), ledger.clone(), population.clone());
        s.apply(context, &mut l, &mut p, data)?;
        s.validate()?;
        *self = s;
        *ledger = l;
        *population = p;
        Ok(())
    }
    fn apply(
        &mut self,
        c: &Context<'_>,
        l: &mut Ledger,
        p: &mut Population,
        d: &Value,
    ) -> Result<(), Error> {
        let action = d["action"]
            .as_str()
            .ok_or_else(|| reject("unknown social command"))?;
        if matches!(
            action,
            "give_worker_tool" | "take_worker_tool" | "transfer_worker_tool"
        ) {
            return self.tool(p, l, d, action);
        }
        if action == "inspect_story_rock" {
            let x = d["x"]
                .as_i64()
                .and_then(|n| i32::try_from(n).ok())
                .ok_or_else(|| reject("invalid rock"))?;
            let y = d["y"]
                .as_i64()
                .and_then(|n| i32::try_from(n).ok())
                .ok_or_else(|| reject("invalid rock"))?;
            if !c.rocks.contains(&[x, y])
                || (c.position.x.get() - f64::from(x)).hypot(c.position.y.get() - f64::from(y))
                    > 1.8
            {
                return Err(reject("rock unavailable or distant"));
            }
            let npc = c
                .locals
                .iter()
                .find(|n| n.name == "Flint")
                .ok_or_else(|| reject("survey not started"))?;
            let progress = self
                .visits
                .get_mut(&npc.key)
                .filter(|v| v.story_stage == 1)
                .ok_or_else(|| reject("survey not started"))?;
            let key = format!("{x}:{y}");
            if progress.story_rocks.contains(&key) || progress.story_rocks.len() >= 3 {
                return Err(reject("outcrop already recorded or survey ready"));
            }
            progress.story_rocks.push(key);
            return Ok(());
        }
        if matches!(
            action,
            "buy_equipment" | "sell_equipment" | "equip_item" | "unequip_item"
        ) {
            return self.clothing(c, l, d, action);
        }
        let n = &c.locals[index(d, "npc_index", c.locals.len())?];
        near(c.position, n.position)?;
        let progress = self.visits.get(&n.key).cloned().unwrap_or_default();
        match action {
            "interact_local" => self.visits.entry(n.key.clone()).or_default().met = true,
            "claim_local_gift" => {
                let k = catalog()["gifts"][&n.name]
                    .as_str()
                    .ok_or_else(|| reject("no local gift"))?;
                if progress.gift_claimed {
                    return Err(reject("gift already claimed"));
                }
                increment(
                    if catalog()["tools"].get(k).is_some() {
                        &mut self.player.worker_tools
                    } else {
                        &mut self.player.items
                    },
                    k,
                )?;
                self.visits.entry(n.key.clone()).or_default().gift_claimed = true;
            }
            "buy_worker_tool" => {
                let k = item(d)?;
                if !catalog()["shops"][&n.name]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(k)))
                {
                    return Err(reject("tool not sold here"));
                }
                let price = catalog()["tools"][k]["price"].as_f64().unwrap();
                if l.free(&key("coins")?)?.get() < price {
                    return Err(reject("insufficient coins"));
                }
                l.spend(&stock("coins", price)?)?;
                increment(&mut self.player.worker_tools, k)?;
            }
            "trade_local" => {
                let offer = catalog()["barter"][&n.name]
                    .as_array()
                    .ok_or_else(|| reject("no local barter"))?;
                if progress.trade_remaining > 0. {
                    return Err(reject("local restocking"));
                }
                let wood = offer[0].as_f64().unwrap();
                let good = offer[1].as_str().unwrap();
                let amount = offer[2].as_f64().unwrap();
                if l.free(&key("wood")?)?.get() < wood || l.storage_room(good)? < amount {
                    return Err(reject("barter stock or capacity"));
                }
                l.spend(&stock("wood", wood)?)?;
                l.deliver(&key(good)?, q(amount)?, false)?;
                self.visits
                    .entry(n.key.clone())
                    .or_default()
                    .trade_remaining = 300.;
            }
            "advance_local_story" => {
                let story = catalog()["stories"]
                    .get(&n.name)
                    .ok_or_else(|| reject("no local story"))?;
                let stages = story["stages"].as_array().unwrap();
                let stage = progress.story_stage;
                if stage >= stages.len() as u64 - 1 {
                    return Err(reject("story already complete"));
                }
                let objective = &stages[stage as usize];
                if c.completed_roads < objective["roads"].as_u64().unwrap_or(0)
                    || progress.story_rocks.len()
                        < (objective["rocks"].as_u64().unwrap_or(0) as usize)
                {
                    return Err(reject("story objective incomplete"));
                }
                spend_costs(l, &objective["costs"])?;
                self.visits.entry(n.key.clone()).or_default().story_stage = stage + 1;
                if stage + 1 == stages.len() as u64 - 1 {
                    l.credit_coins(q(story["reward"].as_f64().unwrap())?)?;
                    self.player
                        .keepsakes
                        .push(story["keepsake"].as_str().unwrap().into());
                }
            }
            _ => return Err(reject("unknown social command")),
        }
        Ok(())
    }
    fn tool(
        &mut self,
        p: &mut Population,
        l: &mut Ledger,
        d: &Value,
        action: &str,
    ) -> Result<(), Error> {
        let i = index(d, "index", p.residents().len())?;
        let actor = p.residents()[i].actor.clone();
        let old = p.residents()[i].worker.tool.clone();
        match action {
            "give_worker_tool" => {
                let k = item(d)?;
                if catalog()["tools"].get(k).is_none()
                    || old.as_ref().is_some_and(|s| !s.is_empty())
                {
                    return Err(reject("invalid tool or worker occupied"));
                }
                if self.player.worker_tools.get(k).copied().unwrap_or(0) >= 1 {
                    *self.player.worker_tools.get_mut(k).unwrap() -= 1;
                } else if crate::resources::public_catalog()["crafted_worker_tools"]
                    .get(k)
                    .is_some()
                {
                    l.spend(&stock(k, 1.)?)?;
                } else {
                    return Err(reject("tool unavailable"));
                }
                p.worker_mut(&actor)
                    .map_err(|_| reject("missing worker"))?
                    .tool = Some(k.into());
            }
            "take_worker_tool" => {
                let k = old
                    .filter(|k| catalog()["tools"].get(k).is_some())
                    .ok_or_else(|| reject("no tool to return"))?;
                increment(&mut self.player.worker_tools, &k)?;
                p.worker_mut(&actor).unwrap().tool = None;
            }
            _ => {
                let j = index(d, "target_index", p.residents().len())?;
                let k = old
                    .filter(|k| {
                        catalog()["tools"].get(k).is_some() && d["item"].as_str() == Some(k)
                    })
                    .ok_or_else(|| reject("stale source tool"))?;
                if i == j
                    || p.residents()[j]
                        .worker
                        .tool
                        .as_ref()
                        .is_some_and(|s| !s.is_empty())
                {
                    return Err(reject("invalid tool recipient"));
                }
                let target = p.residents()[j].actor.clone();
                p.worker_mut(&target).unwrap().tool = Some(k);
                p.worker_mut(&actor).unwrap().tool = None;
            }
        }
        Ok(())
    }
    fn clothing(
        &mut self,
        c: &Context<'_>,
        l: &mut Ledger,
        d: &Value,
        action: &str,
    ) -> Result<(), Error> {
        let k = item(d)?;
        let spec = catalog()["equipment"]
            .get(k)
            .ok_or_else(|| reject("unknown clothing"))?;
        let slot = spec["slot"].as_str().unwrap();
        let count = self.player.items.get(k).copied().unwrap_or(0);
        let equipped = self.player.equipment.get(slot).is_some_and(|v| v == k);
        match action {
            "equip_item" => {
                if count < 1 {
                    return Err(reject("item unavailable"));
                }
                self.player.equipment.insert(slot.into(), k.into());
            }
            "unequip_item" => {
                if !equipped {
                    return Err(reject("item not equipped"));
                }
                if spec["biome"].as_str().is_some_and(|biome| {
                    c.ground.iter().any(|g| {
                        g.biome == biome
                            && (biome != "b" || variant(g.tile) == 2)
                            && (c.position.x.get() - f64::from(g.tile[0])).abs() < 0.68
                            && (c.position.y.get() - f64::from(g.tile[1])).abs() < 0.68
                    })
                }) {
                    return Err(reject("unsafe clothing removal"));
                }
                self.player.equipment.remove(slot);
            }
            _ => {
                let n = &c.locals[index(d, "npc_index", c.locals.len())?];
                let offered = if n.name == "Dara" || n.village.as_deref() == Some("Sunwell") {
                    Some("desert_clothing")
                } else if n.name == "Petra" || n.village.as_deref() == Some("Reedhaven") {
                    Some("swamp_boots")
                } else {
                    None
                };
                if offered != Some(k) {
                    return Err(reject("clothing not sold here"));
                }
                near(c.position, n.position)?;
                let price = spec["price"].as_f64().unwrap();
                if action == "buy_equipment" {
                    if l.free(&key("coins")?)?.get() < price {
                        return Err(reject("insufficient coins"));
                    }
                    l.spend(&stock("coins", price)?)?;
                    increment(&mut self.player.items, k)?;
                } else {
                    if count <= u64::from(equipped) {
                        return Err(reject("equipped item protected"));
                    }
                    self.player.items.insert(k.into(), count - 1);
                    l.credit_coins(q((price / 2.).floor())?)?;
                }
            }
        }
        Ok(())
    }
    /// Explicit economic interval; loading and unrelated commands never restock.
    pub fn advance_restocks(
        &mut self,
        owner: &Owner,
        seconds: lantern_core::units::Seconds,
    ) -> Result<(), Error> {
        if owner != &self.owner {
            return Err(reject("ownership mismatch"));
        }
        for v in self.visits.values_mut() {
            if v.trade_remaining != 0. {
                v.trade_remaining = (v.trade_remaining - seconds.get()).max(0.);
            }
        }
        Ok(())
    }
}
fn near(a: Position, b: Position) -> Result<(), Error> {
    if (a.x.get() - b.x.get()).hypot(a.y.get() - b.y.get()) > 1.8 {
        Err(reject("local too distant"))
    } else {
        Ok(())
    }
}
fn spend_costs(l: &mut Ledger, v: &Value) -> Result<(), Error> {
    if let Some(costs) = v.as_object() {
        let food = costs.get("food").and_then(Value::as_f64).unwrap_or(0.);
        let goods = Stock::from_entries(
            costs
                .iter()
                .filter(|(k, _)| k.as_str() != "food")
                .map(|(k, v)| {
                    Ok((
                        key(k)?,
                        q(v.as_f64().ok_or_else(|| reject("invalid cost"))?)?,
                    ))
                })
                .collect::<Result<Vec<_>, Error>>()?,
        )?;
        if l.food()?.get() + 1e-9 < food || !l.can_afford(&goods)? {
            return Err(reject("story supplies unavailable"));
        }
        l.spend(&goods)?;
        if food > 0. {
            l.eat(q(food)?)?;
        }
    }
    Ok(())
}
fn variant(t: [i32; 2]) -> i128 {
    let (x, y) = (i128::from(t[0]), i128::from(t[1]));
    ((x * 17 + y * 31 + x * y * 7).rem_euclid(7)).rem_euclid(3)
}
fn required<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(d)
}
