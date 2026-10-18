//! Pure hall requirements and explicit evidence adapters; no construction/save mutation.
use super::{Error, Worker, rules};
use crate::{
    building::Progression,
    resources::{Ledger, Stock},
};
use lantern_core::{
    ids::BuildingId,
    units::{Quantity, Seconds, Tile},
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildingStatus {
    Complete,
    InitialConstruction,
    Upgrade,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HallBuilding {
    pub kind: String,
    pub tile: Tile,
    pub status: BuildingStatus,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HallContext {
    pub buildings: Vec<HallBuilding>,
    pub natural_water: Vec<Tile>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct HallEvidence {
    pub food: Quantity,
    pub produced: Stock,
    pub completed_trades: u64,
    pub market_sales: Quantity,
}
impl HallEvidence {
    pub fn from_ledger(
        ledger: &Ledger,
        completed_trades: u64,
        market_sales: Quantity,
    ) -> Result<Self, Error> {
        Ok(Self {
            food: ledger.food()?,
            produced: ledger.produced().clone(),
            completed_trades,
            market_sales,
        })
    }
}
pub fn hall_milestone(level: u8, e: &HallEvidence, context: &HallContext) -> bool {
    if level == 2 {
        let housing = context
            .buildings
            .iter()
            .any(|b| b.kind == "home" && b.status != BuildingStatus::InitialConstruction);
        let sources = context
            .natural_water
            .iter()
            .copied()
            .chain(
                context
                    .buildings
                    .iter()
                    .filter(|b| b.kind == "well" && b.status == BuildingStatus::Complete)
                    .map(|b| b.tile),
            )
            .collect::<Vec<_>>();
        let food = context.buildings.iter().any(|b| {
            matches!(b.kind.as_str(), "farm" | "fishing_hut")
                && b.status != BuildingStatus::InitialConstruction
                && sources.iter().any(|w| {
                    (i64::from(b.tile.x) - i64::from(w.x)).abs()
                        + (i64::from(b.tile.y) - i64::from(w.y)).abs()
                        <= 10
                })
        });
        return housing && food && e.food.get() >= 20.0;
    }
    let Some(upgrade) = rules()["hall_upgrades"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["level"].as_u64() == Some(u64::from(level)))
    else {
        return false;
    };
    if upgrade["production"]
        .as_object()
        .unwrap()
        .iter()
        .any(|(k, n)| {
            e.produced
                .entries()
                .iter()
                .find(|(key, _)| key.as_str() == k)
                .map_or(0.0, |(_, n)| n.get())
                < n.as_f64().unwrap()
        })
    {
        return false;
    }
    if level == 5 {
        let processed = rules()["processed_food"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                e.produced
                    .entries()
                    .iter()
                    .find(|(k, _)| Some(k.as_str()) == v.as_str())
                    .map_or(0.0, |(_, n)| n.get())
            })
            .sum::<f64>();
        return processed >= upgrade["processed_food"].as_f64().unwrap();
    }
    level != 10 || e.completed_trades >= 1 || e.market_sales.get() >= 10.0
}
/// Derived progression observations for the existing R12 building boundary.
/// This does not synchronize or duplicate R12 material inventory; the integration
/// owner must use one authoritative ledger when connecting building commands.
pub fn construction_evidence(
    ledger: &Ledger,
    workers: &[Worker],
    player: Option<&Worker>,
    completed_trades: u64,
    market_sales: Quantity,
) -> Result<Progression, Error> {
    let mut skills: BTreeMap<String, Quantity> = BTreeMap::new();
    for worker in workers.iter().chain(player) {
        for (name, value) in worker.skills() {
            let value = Quantity::new(value.get()).map_err(|_| Error::Overflow)?;
            if skills
                .get(name)
                .is_none_or(|current| value.get() > current.get())
            {
                skills.insert(name.clone(), value);
            }
        }
    }
    Ok(Progression {
        skills,
        produced: ledger
            .produced()
            .entries()
            .iter()
            .map(|(k, n)| (k.as_str().into(), *n))
            .collect(),
        food: Some(ledger.food()?),
        completed_trades,
        market_sales: Some(market_sales),
    })
}
#[derive(Clone, Debug, PartialEq)]
pub struct HallOffer {
    pub level: u8,
    pub costs: Stock,
    pub seconds: Seconds,
    pub reward: Quantity,
}
pub fn hall_offer(level: u8, sandbox: bool) -> Result<HallOffer, Error> {
    let spec = rules()["hall_upgrades"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["level"].as_u64() == Some(u64::from(level)))
        .ok_or(Error::Locked)?;
    Ok(HallOffer {
        level,
        costs: if sandbox {
            Stock::default()
        } else {
            super::stock_from(&rules()["hall_costs"][level.to_string()])?
        },
        seconds: Seconds::new(spec["seconds"].as_f64().unwrap()).unwrap(),
        reward: super::q(spec["coins"].as_f64().unwrap())?,
    })
}
#[derive(Clone, Debug, PartialEq)]
pub struct EquipmentOffer {
    pub costs: Stock,
    pub seconds: Seconds,
    pub instant: bool,
}
pub fn equipment_offer(
    workplace: &super::Workplace,
    key: &str,
    hall_level: u8,
    skills: &BTreeMap<String, Quantity>,
    sandbox: bool,
) -> Result<EquipmentOffer, Error> {
    let spec = &crate::resources::public_catalog()["upgrades"][key];
    if spec["building"].as_str() != Some(&workplace.kind) {
        return Err(Error::UnknownUpgrade);
    }
    let required = spec["xp"].as_f64().unwrap_or(0.0);
    let skill = spec["skill"].as_str().unwrap_or("");
    if u64::from(hall_level) < spec["level"].as_u64().unwrap()
        || (!sandbox && skills.get(skill).map_or(0.0, |p| p.get()) < required)
    {
        return Err(Error::Locked);
    }
    if spec["requires"]
        .as_str()
        .is_some_and(|k| !workplace.equipment.contains(k))
    {
        return Err(Error::PreviousTier);
    }
    if workplace.equipment.contains(key) {
        return Err(Error::AlreadyUnlocked);
    }
    Ok(EquipmentOffer {
        costs: if sandbox {
            Stock::default()
        } else {
            super::stock_from(&rules()["upgrade_costs"][key])?
        },
        seconds: Seconds::new(rules()["build_seconds"].as_f64().unwrap()).unwrap(),
        instant: sandbox || key == "boat",
    })
}
/// Keep an explicit building identity when deriving policy input for the batch bridge.
pub fn workplace_from_ledger(
    ledger: &Ledger,
    id: &BuildingId,
    recipe: Option<String>,
    request: Option<super::Request>,
) -> Result<super::Workplace, Error> {
    let s = ledger.store(id)?;
    Ok(super::Workplace {
        kind: s.policy.kind.clone(),
        equipment: s.policy.equipment.clone(),
        recipe,
        request,
    })
}
