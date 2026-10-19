//! Experimental R13 rule evaluation over explicit personal and workplace facts.
//! No elapsed-work scheduler, routes, resident assignment or live-save conversion.
//! Completed batches re-evaluate current ledger inputs/capacity/caps atomically.
use crate::resources::{self, Ledger, ResourceKey, Stock};
use lantern_core::{
    ids::{ActorId, BuildingId},
    units::{Practice, Quantity, Seconds},
};
use std::collections::{BTreeMap, BTreeSet};
mod hall;
pub use hall::{
    BuildingStatus, EquipmentOffer, HallBuilding, HallContext, HallEvidence, HallOffer,
    construction_evidence, equipment_offer, hall_milestone, hall_offer, workplace_from_ledger,
};

#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    UnknownJob,
    InvalidTalent,
    UnknownRecipe,
    UnknownCrop,
    WrongWorkplace,
    WorkshopWaiting,
    Qualification(Vec<MissingPractice>),
    RecipePractice { job: String, required: f64 },
    NoQualifiedWorker,
    Locked,
    UnknownUpgrade,
    PreviousTier,
    AlreadyUnlocked,
    NoCatch,
    InvalidPopulation,
    Overflow,
    Resource(resources::ResourceError),
}
impl From<resources::ResourceError> for Error {
    fn from(e: resources::ResourceError) -> Self {
        Self::Resource(e)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct MissingPractice {
    pub job: String,
    pub required: f64,
    pub actual: f64,
}
fn rules() -> &'static serde_json::Value {
    &resources::catalog()["rules"]
}
fn job_known(job: &str) -> bool {
    rules()["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v.as_str() == Some(job))
}
fn number(v: f64) -> Result<f64, Error> {
    if v.is_finite() && v >= 0.0 {
        Ok(v)
    } else {
        Err(Error::Overflow)
    }
}
fn q(v: f64) -> Result<Quantity, Error> {
    Quantity::new(v).map_err(|_| Error::Overflow)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Worker {
    job: String,
    skills: BTreeMap<String, Practice>,
    talents: Option<BTreeMap<String, f64>>,
    pub sandbox: bool,
    pub tool: Option<String>,
}
impl Worker {
    pub fn new(job: &str) -> Result<Self, Error> {
        if !job_known(job) {
            return Err(Error::UnknownJob);
        }
        Ok(Self {
            job: job.into(),
            skills: BTreeMap::new(),
            talents: None,
            sandbox: false,
            tool: None,
        })
    }
    pub fn job(&self) -> &str {
        &self.job
    }
    pub fn set_job(&mut self, job: &str) -> Result<(), Error> {
        self.qualification(job)?;
        self.job = job.into();
        Ok(())
    }
    pub fn skills(&self) -> &BTreeMap<String, Practice> {
        &self.skills
    }
    pub fn talents(&self) -> Option<&BTreeMap<String, f64>> {
        self.talents.as_ref()
    }
    pub fn set_practice(&mut self, job: &str, value: Practice) -> Result<(), Error> {
        if !job_known(job) {
            return Err(Error::UnknownJob);
        }
        self.skills.insert(job.into(), value);
        Ok(())
    }
    pub fn set_talents(&mut self, talents: BTreeMap<String, f64>) -> Result<(), Error> {
        // Explicitly reject values that would create negative/nonfinite rates.
        if talents.values().any(|n| !n.is_finite() || *n < -4.0) {
            return Err(Error::InvalidTalent);
        }
        self.talents = Some(talents);
        Ok(())
    }
    /// Only absent traits get deterministic legacy defaults; saved empty maps stay empty.
    pub fn ensure_talents(&mut self, index: usize) {
        if self.talents.is_none() {
            let roots = rules()["talents"].as_array().unwrap();
            self.talents = Some(BTreeMap::from([
                (roots[index % roots.len()].as_str().unwrap().into(), 1.0),
                (
                    roots[(index % roots.len() + 2) % roots.len()]
                        .as_str()
                        .unwrap()
                        .into(),
                    -1.0,
                ),
            ]));
        }
    }
    pub fn practice_value(&self, job: &str) -> f64 {
        self.skills.get(job).map_or(0.0, |p| p.get())
    }
    pub fn aptitude(&self, job: &str) -> f64 {
        let root = resources::public_catalog()["talent_roots"][job]
            .as_str()
            .unwrap_or(job);
        self.talents
            .as_ref()
            .and_then(|t| t.get(root))
            .copied()
            .unwrap_or(0.0)
    }
    pub fn efficiency(&self, job: &str) -> Result<f64, Error> {
        if !job_known(job) {
            return Err(Error::UnknownJob);
        }
        // Python float floor division uses the remainder, avoiding quotient rounding
        // across an exact 20-point boundary before floor is taken.
        let value = self.practice_value(job);
        let div = (value - value % 20.0) / 20.0;
        let floor = div.floor();
        let tier = if div - floor > 0.5 {
            floor + 1.0
        } else {
            floor
        };
        number((1.0 + tier) * (1.0 + 0.25 * self.aptitude(job)))
    }
    pub fn qualification(&self, job: &str) -> Result<(), Error> {
        if !job_known(job) {
            return Err(Error::UnknownJob);
        }
        if self.sandbox {
            return Ok(());
        }
        let missing: Vec<_> = resources::public_catalog()["job_requirements"][job]
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(skill, n)| {
                let required = n.as_f64().unwrap();
                let actual = self.practice_value(skill);
                (actual < required).then(|| MissingPractice {
                    job: skill.clone(),
                    required,
                    actual,
                })
            })
            .collect();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(Error::Qualification(missing))
        }
    }
    pub fn practice(&mut self, job: &str, seconds: Seconds) -> Result<(), Error> {
        if !job_known(job) {
            return Err(Error::UnknownJob);
        }
        let rate = rules()["practice_rates"][job].as_f64().unwrap();
        let gain = rate * seconds.get() / 3.0 * (1.0 + 0.25 * self.aptitude(job));
        let value = number(self.practice_value(job) + gain)?;
        // Decimal formatting rounds the binary64 value itself, avoiding the extra
        // rounding introduced by multiply/round/divide. Inventory is never rounded.
        let rounded = format!("{value:.3}")
            .parse::<f64>()
            .map_err(|_| Error::Overflow)?;
        self.skills.insert(
            job.into(),
            Practice::new(rounded).map_err(|_| Error::Overflow)?,
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub recipe: String,
    pub remaining: Quantity,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Workplace {
    pub kind: String,
    pub recipe: Option<String>,
    pub equipment: BTreeSet<String>,
    /// Caller supplies the current workshop head; the R15 queue owner advances it.
    pub request: Option<Request>,
}
impl Workplace {
    pub fn new(kind: &str) -> Self {
        Self {
            kind: kind.into(),
            recipe: None,
            equipment: BTreeSet::new(),
            request: None,
        }
    }
    pub fn job(&self) -> Result<&'static str, Error> {
        resources::public_catalog()["buildings"][&self.kind]["job"]
            .as_str()
            .ok_or(Error::WrongWorkplace)
    }
    pub fn selected_recipe(&self) -> Result<Recipe, Error> {
        let c = resources::public_catalog();
        if let Some(recipes) = c["profession_recipes"][&self.kind].as_array() {
            let selected = if self.kind == "workshop" {
                self.request
                    .as_ref()
                    .map(|r| r.recipe.as_str())
                    .or(self.recipe.as_deref())
            } else {
                self.recipe.as_deref()
            };
            let spec = recipes
                .iter()
                .find(|r| r["output"].as_str() == selected)
                .unwrap_or(&recipes[0]);
            recipe_from(spec)
        } else {
            let spec = c["recipes"][&self.kind]
                .as_array()
                .ok_or(Error::UnknownRecipe)?;
            Ok(Recipe {
                inputs: stock_from(&spec[0])?,
                output: ResourceKey::parse(spec[1].as_str().unwrap())?,
                amount: q(spec[2].as_f64().unwrap())?,
                level: 1.0,
            })
        }
    }
    pub fn recipe_requirement(&self, worker: &Worker) -> Result<(), Error> {
        if self.kind == "workshop" && self.request.is_none() {
            return Err(Error::WorkshopWaiting);
        }
        if worker.sandbox {
            return Ok(());
        }
        let recipe = self.selected_recipe()?;
        let job = self.job()?;
        if recipe.level > 1.0 && worker.practice_value(job) < recipe.level {
            return Err(Error::RecipePractice {
                job: job.into(),
                required: recipe.level,
            });
        }
        Ok(())
    }
    pub fn processing_efficiency(&self, worker: &Worker) -> Result<f64, Error> {
        let tool = worker.tool.as_deref().unwrap_or("");
        let spec = &resources::public_catalog()["crafted_worker_tools"][tool];
        let mut bonus = if spec["job"].as_str() == Some(worker.job()) {
            spec["bonus"].as_f64().unwrap_or(0.0)
        } else {
            0.0
        };
        if self.equipment.contains("steel_pickaxe") {
            bonus += 0.25;
        }
        let mut speed = worker.efficiency(worker.job())?
            * (1.0 + bonus)
            * equipment_multiplier(&self.kind, &self.equipment);
        if self.kind == "workshop"
            && let Some(request) = &self.request
        {
            speed = speed.min(request.remaining.get() / self.selected_recipe()?.amount.get());
        }
        number(speed)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Recipe {
    pub inputs: Stock,
    pub output: ResourceKey,
    pub amount: Quantity,
    pub level: f64,
}
impl Recipe {
    pub fn seconds(&self) -> Seconds {
        Seconds::new(
            resources::public_catalog()["production_seconds"][self.output.as_str()]
                .as_f64()
                .unwrap(),
        )
        .unwrap()
    }
}
fn stock_from(v: &serde_json::Value) -> Result<Stock, Error> {
    Ok(Stock::from_entries(
        v.as_object()
            .unwrap()
            .iter()
            .map(|(k, n)| Ok((ResourceKey::parse(k)?, q(n.as_f64().unwrap())?)))
            .collect::<Result<Vec<_>, Error>>()?,
    )?)
}
fn recipe_from(spec: &serde_json::Value) -> Result<Recipe, Error> {
    Ok(Recipe {
        inputs: stock_from(&spec["inputs"])?,
        output: ResourceKey::parse(spec["output"].as_str().unwrap())?,
        amount: q(spec["amount"].as_f64().unwrap())?,
        level: spec["level"].as_f64().unwrap(),
    })
}
pub fn equipment_multiplier(kind: &str, equipment: &BTreeSet<String>) -> f64 {
    equipment
        .iter()
        .filter_map(|key| {
            let s = &resources::public_catalog()["upgrades"][key];
            (s["building"].as_str() == Some(kind)).then(|| s["throughput"].as_f64().unwrap_or(1.0))
        })
        .reduce(f64::max)
        .unwrap_or(1.0)
}
#[derive(Clone, Debug, PartialEq)]
pub struct Batch {
    pub costs: Stock,
    pub outputs: Stock,
    pub seconds: Seconds,
    pub speed: f64,
}
/// Evaluate one complete recipe against current personal qualification and input species.
pub fn evaluate_batch(
    worker: &Worker,
    workplace: &Workplace,
    inputs: &Stock,
) -> Result<Batch, Error> {
    worker.qualification(worker.job())?;
    if workplace.job()? != worker.job() {
        return Err(Error::WrongWorkplace);
    }
    workplace.recipe_requirement(worker)?;
    let recipe = workplace.selected_recipe()?;
    let speed = workplace.processing_efficiency(worker)?;
    let mut costs = Stock::default();
    for (k, n) in recipe.inputs.entries() {
        if workplace.kind == "smokehouse" && k.as_str() == "fish" {
            continue;
        }
        costs.credit(k.clone(), q(n.get() * speed)?)?;
    }
    let mut outputs = Stock::default();
    if workplace.kind == "smokehouse" {
        let mut remaining = recipe.inputs.quantity(&ResourceKey::parse("fish")?).get() * speed;
        for name in rules()["fish_order"].as_array().unwrap() {
            let k = ResourceKey::parse(name.as_str().unwrap())?;
            let take = remaining.min(inputs.quantity(&k).get());
            if take > 0.0 {
                costs.credit(k, q(take)?)?;
                remaining -= take;
            }
        }
        if remaining > 0.0 {
            costs.credit(ResourceKey::parse("fish")?, q(remaining)?)?;
        }
        for (k, n) in costs
            .entries()
            .iter()
            .filter(|(k, _)| resources::public_catalog()["fish"][k.as_str()].is_object())
        {
            outputs.credit(ResourceKey::parse(&format!("smoked_{}", k.as_str()))?, *n)?;
        }
    } else {
        outputs.credit(recipe.output.clone(), q(recipe.amount.get() * speed)?)?;
    }
    Ok(Batch {
        costs,
        outputs,
        seconds: recipe.seconds(),
        speed,
    })
}
/// The scheduler calls this once after work completes. No clock/queue is advanced.
/// Re-evaluation and ledger admission precede all skill mutation; failure is atomic.
pub fn complete_batch(
    ledger: &mut Ledger,
    id: &BuildingId,
    actor: &ActorId,
    worker: &mut Worker,
    workplace: &Workplace,
) -> Result<Batch, Error> {
    let store = ledger.store(id)?;
    if store.policy.kind != workplace.kind || store.policy.equipment != workplace.equipment {
        return Err(Error::WrongWorkplace);
    }
    let batch = evaluate_batch(worker, workplace, &store.inputs)?;
    let mut trained = worker.clone();
    trained.practice(worker.job(), batch.seconds)?;
    ledger.produce(id, Some(actor), &batch.costs, &batch.outputs)?;
    *worker = trained;
    Ok(batch)
}
/// Selection uses only supplied assigned workers, never the founder or town maximum.
pub fn choose_recipe(
    kind: &str,
    output: &str,
    sandbox: bool,
    assigned: &[Worker],
) -> Result<Recipe, Error> {
    let spec = resources::public_catalog()["profession_recipes"][kind]
        .as_array()
        .and_then(|r| r.iter().find(|r| r["output"].as_str() == Some(output)))
        .ok_or(Error::UnknownRecipe)?;
    let recipe = recipe_from(spec)?;
    let workplace = Workplace::new(kind);
    let job = workplace.job()?;
    if !sandbox
        && recipe.level > 1.0
        && !assigned.iter().any(|w| {
            w.job() == job && w.qualification(job).is_ok() && w.practice_value(job) >= recipe.level
        })
    {
        return Err(Error::NoQualifiedWorker);
    }
    Ok(recipe)
}
pub fn crop_unlocked(kind: &str, crop: &str, hall_level: u8) -> Result<(), Error> {
    if !matches!(kind, "farm" | "acre") {
        return Err(Error::WrongWorkplace);
    }
    let level = resources::public_catalog()["crops"][crop]
        .as_u64()
        .ok_or(Error::UnknownCrop)?;
    if u64::from(hall_level) < level {
        Err(Error::Locked)
    } else {
        Ok(())
    }
}
pub fn fishing_boat_level(equipment: &BTreeSet<String>) -> u8 {
    ["boat", "improved_boat", "master_boat"]
        .iter()
        .rposition(|k| equipment.contains(*k))
        .map_or(0, |i| i as u8 + 1)
}
pub fn fishing_boat_capacity(equipment: &BTreeSet<String>) -> u64 {
    let level = fishing_boat_level(equipment);
    if level == 0 {
        0
    } else {
        resources::public_catalog()["upgrades"]
            [["boat", "improved_boat", "master_boat"][level as usize - 1]]["capacity"]
            .as_u64()
            .unwrap()
    }
}
pub fn fishing_catch(equipment: &BTreeSet<String>, depth: u8) -> Result<ResourceKey, Error> {
    let level = fishing_boat_level(equipment);
    for name in rules()["fish_order"].as_array().unwrap().iter().rev() {
        let name = name.as_str().unwrap();
        let s = &resources::public_catalog()["fish"][name];
        if u64::from(level) >= s["boat"].as_u64().unwrap()
            && u64::from(depth) >= s["depth"].as_u64().unwrap()
        {
            return Ok(ResourceKey::parse(name)?);
        }
    }
    Err(Error::NoCatch)
}
pub fn gathering_bonus(equipment: &BTreeMap<String, String>, good: &ResourceKey) -> f64 {
    rules()["equipment_items"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(k, s)| {
            s["gather_good"].as_str() == Some(good.as_str())
                && equipment
                    .get(s["slot"].as_str().unwrap())
                    .is_some_and(|v| v == *k)
        })
        .map(|(_, s)| s["gather_bonus"].as_f64().unwrap())
        .sum()
}
pub fn food_demand(total: usize, working: usize) -> Result<Quantity, Error> {
    if working > total {
        return Err(Error::InvalidPopulation);
    }
    q((total as f64 * rules()["idle_food"].as_f64().unwrap()
        + working as f64 * rules()["worker_food"].as_f64().unwrap())
        * 60.0
        / rules()["food_tick_seconds"].as_f64().unwrap())
}

/// Staffing upgrades are independent of structural/storage/equipment levels.
/// Assignment and resident lifecycle remain with the population owner.
pub fn worker_capacity(kind: &str, tier: u8) -> Result<u64, Error> {
    if tier == 0 {
        return Err(Error::WrongWorkplace);
    }
    if matches!(kind, "hall" | "orchard_tree") {
        return Ok(0);
    }
    if let Some(tiers) = resources::public_catalog()["worker_capacities"][kind].as_array() {
        return Ok(tiers[(tier as usize).min(tiers.len()) - 1]
            .as_u64()
            .unwrap());
    }
    rules()["base_staffing"][kind]
        .as_u64()
        .map(|n| n * u64::from(tier))
        .ok_or(Error::WrongWorkplace)
}
