//! Founder fishing activity; position, equipment and personal practice stay with
//! their existing owners. Movement's clear_activity result calls stop().
use super::*;
use lantern_core::{founder::FounderState, ids::Owner, units::Practice};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Idle,
    Working,
    NoCatch,
    Cap,
    Storage,
    Supply,
}
#[derive(Clone, Debug, PartialEq)]
pub struct LandAngler {
    owner: Owner,
    source: Option<Tile>,
    history: CatchHistory,
    status: Status,
}
fn range(p: Position, t: Tile) -> f64 {
    (p.x.get() - f64::from(t.x)).abs() + (p.y.get() - f64::from(t.y)).abs()
}
impl LandAngler {
    pub fn new(owner: Owner) -> Self {
        Self {
            owner,
            source: None,
            history: CatchHistory::default(),
            status: Status::Idle,
        }
    }
    fn validate(&self, town: &Town) -> Result<(), Error> {
        if self.owner != *town.owner() {
            Err(Error::Invalid)
        } else {
            Ok(())
        }
    }
    pub fn source(&self) -> Option<Tile> {
        self.source
    }
    pub fn status(&self) -> Status {
        self.status
    }
    pub fn stop(&mut self) {
        self.source = None;
        self.status = Status::Idle;
    }
    /// Same action toggles the nearest water source off. Manhattan range 1.8;
    /// failed acquisition leaves an existing activity and facing untouched.
    pub fn toggle(
        &mut self,
        town: &Town,
        water: &Waters,
        founder: &mut FounderState,
    ) -> Result<(), Error> {
        self.validate(town)?;
        let source = water
            .water
            .iter()
            .filter(|w| range(founder.position, w.tile) <= 1.8)
            .min_by(|a, b| {
                range(founder.position, a.tile).total_cmp(&range(founder.position, b.tile))
            })
            .ok_or(Error::Invalid)?
            .tile;
        if self.source == Some(source) {
            self.stop();
        } else {
            self.source = Some(source);
            self.status = Status::Working;
            let dx = f64::from(source.x) - founder.position.x.get();
            let dy = f64::from(source.y) - founder.position.y.get();
            if dx != 0. || dy != 0. {
                founder.facing = if dx > dy { 1 } else { -1 };
            }
        }
        Ok(())
    }
    /// One explicit three-second founder production batch. Chance is 4/5,
    /// practice is +6 per attempt, no villager talents/tools/nets apply. Boat
    /// fishing uses attempt_afloat with the same saved activity and batch history.
    #[allow(clippy::too_many_arguments)]
    pub fn attempt(
        &mut self,
        town: &Town,
        ledger: &mut Ledger,
        water: &mut Waters,
        position: Position,
        practice: &mut Practice,
        batch: u64,
        roll: u8,
    ) -> Result<Catch, Error> {
        self.attempt_with_bonus(town, ledger, water, position, practice, batch, roll, 1.)
    }
    /// Same activity owner and batch history afloat; only an owned exploration
    /// boat on an actual water tile supplies the equipment bonus.
    #[allow(clippy::too_many_arguments)]
    pub fn attempt_afloat(
        &mut self,
        town: &Town,
        ledger: &mut Ledger,
        water: &mut Waters,
        position: Position,
        practice: &mut Practice,
        batch: u64,
        roll: u8,
        boat: &super::ExplorationBoat,
    ) -> Result<Catch, Error> {
        boat.validate(town)?;
        let at = Tile {
            x: (position.x.get() + 0.5) as i32,
            y: (position.y.get() + 0.5) as i32,
        };
        let bonus = if boat.tier() > 0 && water.water.iter().any(|w| w.tile == at) {
            1. + 0.25 * f64::from(boat.tier().saturating_sub(1))
        } else {
            1.
        };
        self.attempt_with_bonus(town, ledger, water, position, practice, batch, roll, bonus)
    }
    #[allow(clippy::too_many_arguments)]
    fn attempt_with_bonus(
        &mut self,
        town: &Town,
        ledger: &mut Ledger,
        water: &mut Waters,
        position: Position,
        practice: &mut Practice,
        batch: u64,
        roll: u8,
        bonus: f64,
    ) -> Result<Catch, Error> {
        self.validate(town)?;
        for store in ledger.stores() {
            store
                .id
                .identity()
                .validate_owner(&self.owner)
                .map_err(|_| Error::Invalid)?;
        }
        if batch == 0 || roll >= 5 {
            return Err(Error::Invalid);
        }
        if batch <= self.history.last_batch {
            return Ok(Catch::Skipped);
        }
        let mut owner = self.clone();
        owner.history.last_batch = batch;
        let Some(source) = owner.source else {
            *self = owner;
            return Ok(Catch::Skipped);
        };
        let Some(w) = water
            .water
            .iter()
            .find(|w| w.tile == source && range(position, source) <= 1.8)
        else {
            owner.stop();
            *self = owner;
            return Ok(Catch::Skipped);
        };
        let cap = ledger.production_room(&fish())?;
        let storage = ledger.storage_room("fish")?;
        let blocked = if cap <= 0. {
            Some(Status::Cap)
        } else if storage <= 0. {
            Some(Status::Storage)
        } else if w.supply.get() <= 0. {
            Some(Status::Supply)
        } else {
            None
        };
        if let Some(status) = blocked {
            owner.status = status;
            *self = owner;
            return Ok(Catch::Skipped);
        }
        let (mut goods, mut waters) = (ledger.clone(), water.clone());
        let amount = cap
            .min(storage)
            .min((1. + (practice.get() / 20.).floor()) * bonus);
        let consumed = waters.take(source, amount);
        let caught = roll != 0;
        if caught && consumed > 0. {
            goods.deliver(&fish(), q(consumed)?, true)?;
        }
        let skill = Practice::new(practice.get() + 6.).map_err(|_| Error::Invalid)?;
        owner.history.attempts = owner
            .history
            .attempts
            .checked_add(1)
            .ok_or(Error::Invalid)?;
        owner.history.caught = caught;
        owner.status = if caught {
            Status::Working
        } else {
            Status::NoCatch
        };
        *self = owner;
        *ledger = goods;
        *water = waters;
        *practice = skill;
        Ok(Catch::Attempt {
            caught,
            amount: q(if caught { consumed } else { 0. })?,
            consumed: q(consumed)?,
        })
    }
    pub fn encode(&self) -> Result<String, Error> {
        serde_json::to_string(&SavedAngler {
            version: 1,
            world: self.owner.world.as_str().into(),
            town: self.owner.town.as_str().into(),
            source: self.source.map(pair),
            history: self.history.clone(),
            status: self.status,
        })
        .map_err(|_| Error::Invalid)
    }
    pub fn decode(json: &str, town: &Town) -> Result<Self, Error> {
        let s: SavedAngler = serde_json::from_str(json).map_err(|_| Error::Invalid)?;
        if s.version != 1
            || s.world != town.owner().world.as_str()
            || s.town != town.owner().town.as_str()
            || s.history.attempts > s.history.last_batch
            || (s.history.attempts == 0 && s.history.caught)
            || (s.source.is_none()) != (s.status == Status::Idle)
        {
            return Err(Error::Invalid);
        }
        Ok(Self {
            owner: town.owner().clone(),
            source: s.source.map(tile),
            history: s.history,
            status: s.status,
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedAngler {
    version: u8,
    world: String,
    town: String,
    #[serde(deserialize_with = "required_source")]
    source: Option<[i32; 2]>,
    history: CatchHistory,
    status: Status,
}
fn required_source<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<[i32; 2]>, D::Error> {
    Option::<[i32; 2]>::deserialize(d)
}
