//! No offer reservations. Acceptance spends both outgoing stocks before checking
//! incoming enabled capacity and commits both ledgers/receipts together.
use super::*;
use lantern_core::ids::{TownId, WorldId};
#[derive(Clone)]
pub struct TradeTown {
    pub level: u8,
    pub ledger: Ledger,
}
impl TradeTown {
    fn market(&self) -> bool {
        self.level >= 5
            && self
                .ledger
                .stores()
                .iter()
                .any(|s| s.policy.kind == "market" && !s.policy.construction)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Offer {
    pub id: String,
    pub target_town: String,
    pub give_resource: String,
    pub give_amount: u64,
    pub want_resource: String,
    pub want_amount: u64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Trading {
    world: WorldId,
    offers: BTreeMap<String, Vec<Offer>>,
    completed: BTreeMap<String, u64>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    version: u8,
    world: String,
    offers: BTreeMap<String, Vec<Offer>>,
    completed: BTreeMap<String, u64>,
}
impl Trading {
    pub fn restore(
        world: WorldId,
        offers: BTreeMap<String, Vec<Offer>>,
        completed: BTreeMap<String, u64>,
        towns: &BTreeMap<TownId, TradeTown>,
    ) -> Result<Self, Error> {
        let s = Self {
            world,
            offers,
            completed,
        };
        s.validate(towns)?;
        Ok(s)
    }
    pub fn offers(&self) -> &BTreeMap<String, Vec<Offer>> {
        &self.offers
    }
    pub fn completed(&self, town: &TownId) -> u64 {
        self.completed.get(town.as_str()).copied().unwrap_or(0)
    }
    fn validate(&self, towns: &BTreeMap<TownId, TradeTown>) -> Result<(), Error> {
        for (id, t) in towns {
            if t.ledger.owner().world != self.world
                || &t.ledger.owner().town != id
                || t.level == 0
                || t.level > 10
            {
                return Err(reject("invalid trade town owner or level"));
            }
        }
        let mut ids = std::collections::BTreeSet::new();
        for (source, offers) in &self.offers {
            let id = TownId::new(source).map_err(|_| reject("invalid offer owner"))?;
            if !towns.contains_key(&id) {
                return Err(reject("absent offer owner"));
            }
            for o in offers {
                let target =
                    TownId::new(&o.target_town).map_err(|_| reject("invalid offer target"))?;
                if source == &o.target_town
                    || !towns.contains_key(&target)
                    || o.id.is_empty()
                    || !ids.insert(&o.id)
                {
                    return Err(reject("invalid offer reference or duplicate ID"));
                }
                terms(&o.give_resource, o.give_amount)?;
                terms(&o.want_resource, o.want_amount)?;
                if o.give_resource == o.want_resource {
                    return Err(reject("identical trade goods"));
                }
            }
        }
        if self
            .completed
            .keys()
            .any(|k| TownId::new(k).is_err() || !towns.keys().any(|id| id.as_str() == k))
        {
            return Err(reject("absent trade counter owner"));
        }
        Ok(())
    }
    pub fn encode(&self, towns: &BTreeMap<TownId, TradeTown>) -> Result<String, Error> {
        self.validate(towns)?;
        serde_json::to_string(&Saved {
            version: 1,
            world: self.world.as_str().into(),
            offers: self.offers.clone(),
            completed: self.completed.clone(),
        })
        .map_err(|_| reject("invalid trade snapshot"))
    }
    pub fn decode(
        world: &WorldId,
        json: &str,
        towns: &BTreeMap<TownId, TradeTown>,
    ) -> Result<Self, Error> {
        crate::legacy::validate_social_snapshot(json.as_bytes())
            .map_err(|_| reject("invalid trade snapshot"))?;
        let s: Saved = serde_json::from_str(json).map_err(|_| reject("invalid trade snapshot"))?;
        if s.version != 1 || s.world != world.as_str() {
            return Err(reject("version or world mismatch"));
        }
        Self::restore(world.clone(), s.offers, s.completed, towns)
    }
    pub fn change(
        &mut self,
        owner: &Owner,
        towns: &mut BTreeMap<TownId, TradeTown>,
        data: &Value,
        next_id: &mut impl FnMut() -> String,
    ) -> Result<(), Error> {
        self.validate(towns)?;
        if owner.world != self.world || !towns.get(&owner.town).is_some_and(TradeTown::market) {
            return Err(reject("own completed market at hall five required"));
        }
        let (mut s, mut t) = (self.clone(), towns.clone());
        s.apply(owner, &mut t, data, next_id)?;
        s.validate(&t)?;
        *self = s;
        *towns = t;
        Ok(())
    }
    fn apply(
        &mut self,
        owner: &Owner,
        towns: &mut BTreeMap<TownId, TradeTown>,
        d: &Value,
        next_id: &mut impl FnMut() -> String,
    ) -> Result<(), Error> {
        let action = d["action"]
            .as_str()
            .ok_or_else(|| reject("unknown trade command"))?;
        let own = owner.town.as_str();
        if action == "create_trade" {
            let target = d["target_town"]
                .as_str()
                .and_then(|s| TownId::new(s).ok())
                .ok_or_else(|| reject("invalid target town"))?;
            if target == owner.town || !towns.get(&target).is_some_and(TradeTown::market) {
                return Err(reject("target market unavailable"));
            }
            let (give, amount) = parse_terms(d, "give")?;
            let (want, count) = parse_terms(d, "want")?;
            if give == want
                || !towns[&owner.town]
                    .ledger
                    .can_afford(&stock(&give, amount as f64)?)?
            {
                return Err(reject("invalid goods or insufficient offered stock"));
            }
            if self.offers.get(own).is_some_and(|v| v.len() >= 10) {
                return Err(reject("offer limit"));
            }
            let id = next_id();
            if id.is_empty() || self.offers.values().flatten().any(|o| o.id == id) {
                return Err(reject("duplicate offer ID"));
            }
            self.offers.entry(own.into()).or_default().push(Offer {
                id,
                target_town: target.as_str().into(),
                give_resource: give,
                give_amount: amount,
                want_resource: want,
                want_amount: count,
            });
            return Ok(());
        }
        let id = d["offer_id"]
            .as_str()
            .ok_or_else(|| reject("offer unavailable"))?;
        if action == "cancel_trade" {
            let offers = self
                .offers
                .get_mut(own)
                .ok_or_else(|| reject("offer unavailable"))?;
            let i = offers
                .iter()
                .position(|o| o.id == id)
                .ok_or_else(|| reject("offer unavailable"))?;
            offers.remove(i);
            return Ok(());
        }
        if action != "accept_trade" {
            return Err(reject("unknown trade command"));
        }
        let source = d["source_town"]
            .as_str()
            .and_then(|s| TownId::new(s).ok())
            .ok_or_else(|| reject("source unavailable"))?;
        if source == owner.town || !towns.get(&source).is_some_and(TradeTown::market) {
            return Err(reject("source unavailable"));
        }
        let offer = self
            .offers
            .get(source.as_str())
            .and_then(|v| v.iter().find(|o| o.id == id))
            .filter(|o| o.target_town == own)
            .cloned()
            .ok_or_else(|| reject("offer unavailable to this town"))?;
        towns
            .get_mut(&source)
            .unwrap()
            .ledger
            .spend(&stock(&offer.give_resource, offer.give_amount as f64)?)?;
        towns
            .get_mut(&owner.town)
            .unwrap()
            .ledger
            .spend(&stock(&offer.want_resource, offer.want_amount as f64)?)?;
        receive(
            &mut towns.get_mut(&source).unwrap().ledger,
            &offer.want_resource,
            offer.want_amount,
        )?;
        receive(
            &mut towns.get_mut(&owner.town).unwrap().ledger,
            &offer.give_resource,
            offer.give_amount,
        )?;
        self.offers
            .get_mut(source.as_str())
            .unwrap()
            .retain(|o| o.id != id);
        for participant in [source.as_str(), own] {
            let n = self.completed.entry(participant.into()).or_default();
            *n = n
                .checked_add(1)
                .ok_or_else(|| reject("trade counter overflow"))?;
        }
        Ok(())
    }
}
fn terms(k: &str, n: u64) -> Result<(), Error> {
    if !(1..=100).contains(&n)
        || !crate::resources::public_catalog()["tradable_goods"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str() == Some(k))
    {
        Err(reject("invalid whole trade terms"))
    } else {
        Ok(())
    }
}
fn parse_terms(v: &Value, p: &str) -> Result<(String, u64), Error> {
    let k = v[format!("{p}_resource")]
        .as_str()
        .ok_or_else(|| reject("invalid trade good"))?;
    let n = v[format!("{p}_amount")]
        .as_u64()
        .ok_or_else(|| reject("invalid whole trade amount"))?;
    terms(k, n)?;
    Ok((k.into(), n))
}
fn receive(l: &mut Ledger, k: &str, n: u64) -> Result<(), Error> {
    if k == "coins" {
        l.credit_coins(q(n as f64)?)?;
    } else {
        if l.storage_room(k)? + 1e-9 < (n as f64) {
            return Err(reject("incoming enabled capacity unavailable"));
        }
        l.deliver(&key(k)?, q(n as f64)?, false)?;
    }
    Ok(())
}
