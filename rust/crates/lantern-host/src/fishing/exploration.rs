//! Exploration boat ownership/costs are independent of every hut's equipment.
use super::*;
use lantern_core::{founder::FounderAccess, ids::Owner};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoatPurchase {
    Buy,
    Upgrade,
    Build,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExplorationBoat {
    owner: Owner,
    tier: u8,
}
impl ExplorationBoat {
    pub fn new(owner: Owner) -> Self {
        Self { owner, tier: 0 }
    }
    pub fn tier(&self) -> u8 {
        self.tier
    }
    pub(super) fn validate(&self, town: &Town) -> Result<(), Error> {
        if self.owner != *town.owner() {
            Err(Error::Invalid)
        } else {
            Ok(())
        }
    }
    /// The caller supplies town fishing practice (maximum across founder and
    /// residents), matching Python. Buying is paid even in sandbox; building is
    /// free there. Hut equipment stays in R12's upgrade/construction owner.
    pub fn purchase(
        &mut self,
        town: &Town,
        ledger: &mut Ledger,
        action: BoatPurchase,
        hall: u8,
        town_fishing: Quantity,
        sandbox: bool,
    ) -> Result<(), Error> {
        self.validate(town)?;
        for s in ledger.stores() {
            s.id.identity()
                .validate_owner(&self.owner)
                .map_err(|_| Error::Invalid)?;
        }
        if (action == BoatPurchase::Upgrade && self.tier == 0)
            || (action != BoatPurchase::Upgrade && self.tier > 0)
            || self.tier >= 3
        {
            return Err(Error::Invalid);
        }
        let costs = if action == BoatPurchase::Build {
            if hall < 3 {
                return Err(Error::Invalid);
            }
            if sandbox {
                Stock::default()
            } else {
                Stock::from_entries([
                    (ResourceKey::parse("wood")?, q(20.)?),
                    (ResourceKey::parse("planks")?, q(10.)?),
                ])?
            }
        } else {
            let spec = &crate::resources::public_catalog()["boats"][(self.tier + 1).to_string()];
            if u64::from(hall) < spec["level"].as_u64().ok_or(Error::Invalid)?
                || town_fishing.get() < spec["xp"].as_f64().ok_or(Error::Invalid)?
            {
                return Err(Error::Invalid);
            }
            Stock::from_entries(
                ["wood", "planks", "coins"]
                    .iter()
                    .filter_map(|key| spec[*key].as_f64().map(|n| (*key, n)))
                    .map(|(key, n)| Ok((ResourceKey::parse(key)?, q(n)?)))
                    .collect::<Result<Vec<_>, Error>>()?,
            )?
        };
        ledger.spend(&costs)?;
        self.tier += 1;
        Ok(())
    }
    /// R08 already owns collision, water speed and land speed rules.
    pub fn movement_access(
        &self,
        mounted: bool,
        swamp_boots: bool,
    ) -> Result<FounderAccess, Error> {
        FounderAccess::new(self.tier > 0, f64::from(self.tier), mounted, swamp_boots)
            .map_err(|_| Error::Invalid)
    }
    pub fn encode(&self) -> Result<String, Error> {
        serde_json::to_string(&Saved {
            version: 1,
            world: self.owner.world.as_str().into(),
            town: self.owner.town.as_str().into(),
            tier: self.tier,
        })
        .map_err(|_| Error::Invalid)
    }
    pub fn decode(json: &str, town: &Town) -> Result<Self, Error> {
        let s: Saved = serde_json::from_str(json).map_err(|_| Error::Invalid)?;
        if s.version != 1
            || s.world != town.owner().world.as_str()
            || s.town != town.owner().town.as_str()
            || s.tier > 3
        {
            return Err(Error::Invalid);
        }
        Ok(Self {
            owner: town.owner().clone(),
            tier: s.tier,
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    version: u8,
    world: String,
    town: String,
    tier: u8,
}
