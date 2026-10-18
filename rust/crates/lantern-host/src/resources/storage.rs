//! Current Python storage rules. Saved filters affect admission, never existing goods.
use super::{ResourceError, ResourceKey, Stock, catalog, public_catalog};
use lantern_core::units::Quantity;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq)]
pub struct StoragePolicy {
    pub kind: String,
    pub level: u8,
    /// Python excludes both new construction and in-progress upgrades from storage.
    pub construction: bool,
    pub equipment: BTreeSet<String>,
    pub enabled: Option<BTreeMap<String, bool>>,
    pub legacy_allocations: BTreeMap<String, Quantity>,
}
impl StoragePolicy {
    /// Explicit facts may be edited before admission; validate them before using
    /// size/allocation helpers. Ledger validates every admitted policy.
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            level: 1,
            construction: false,
            equipment: BTreeSet::new(),
            enabled: None,
            legacy_allocations: BTreeMap::new(),
        }
    }
    pub fn validate(&self) -> Result<(), ResourceError> {
        if self.level == 0
            || (self.kind == "storehouse"
                && self.level as usize
                    > catalog()["rules"]["storehouse_capacities"]
                        .as_array()
                        .unwrap()
                        .len())
            || (self.kind != "hall" && !crate::building::catalog().contains_key(&self.kind))
        {
            return Err(ResourceError::InvalidStorage);
        }
        for key in self
            .enabled
            .iter()
            .flat_map(|m| m.keys())
            .chain(self.legacy_allocations.keys())
        {
            if key != "food" {
                ResourceKey::parse(key)?;
            }
        }
        // Unknown/mismatched saved equipment is ignored by Python's multiplier.
        Ok(())
    }
    pub fn accepts(&self, key: &str) -> bool {
        public_catalog()["storage_goods"][&self.kind]
            .as_array()
            .is_some_and(|keys| keys.iter().any(|k| k.as_str() == Some(key)))
    }
    pub fn multiplier(&self) -> f64 {
        if !public_catalog()["storage_upgrade_kinds"]
            .as_array()
            .unwrap()
            .iter()
            .any(|k| k.as_str() == Some(&self.kind))
        {
            return 1.0;
        }
        let equipment = self
            .equipment
            .iter()
            .filter_map(|name| {
                let spec = &public_catalog()["upgrades"][name];
                (spec["building"].as_str() == Some(&self.kind))
                    .then(|| spec["throughput"].as_f64().unwrap_or(1.0))
            })
            .reduce(f64::max)
            .unwrap_or(1.0);
        (1.0 + 0.25 * (self.level - 1) as f64) * equipment
    }
    pub fn size(&self) -> f64 {
        match self.kind.as_str() {
            "farm" | "acre" => self.allocations().values().sum(),
            "hall" => 200.0,
            "storehouse" => catalog()["rules"]["storehouse_capacities"][self.level as usize - 1]
                .as_f64()
                .unwrap(),
            "home" | "market" | "well" | "orchard_tree" => 0.0,
            kind if public_catalog()["decorations"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v.as_str() == Some(kind)) =>
            {
                0.0
            }
            _ => {
                public_catalog()["workplace_storage_sizes"][&self.kind]
                    .as_f64()
                    .unwrap_or(50.0)
                    * self.multiplier()
            }
        }
    }
    pub fn allocations(&self) -> BTreeMap<String, f64> {
        if self.kind == "storehouse" {
            let enabled = self.enabled.clone().unwrap_or_else(|| {
                self.legacy_allocations
                    .iter()
                    .map(|(k, v)| (k.clone(), v.get() > 0.0))
                    .collect()
            });
            return public_catalog()["storage_keys"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|v| {
                    let key = v.as_str().unwrap();
                    self.accepts(key).then(|| {
                        let edible = public_catalog()["edible"][key].is_number();
                        let active = enabled.get(key).copied().unwrap_or_else(|| {
                            edible && enabled.get("food").copied().unwrap_or(false)
                        });
                        (key.into(), if active { self.size() } else { 0.0 })
                    })
                })
                .collect();
        }
        public_catalog()["fixed_storage"][&self.kind]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(key, value)| {
                let amount = if self.kind == "farm" || self.kind == "acre" {
                    if public_catalog()["crops"][key].is_number() || key == "food" {
                        let harvest = public_catalog()["farming_harvest_yield"].as_f64().unwrap();
                        if self.kind == "acre" {
                            harvest
                        } else {
                            let scale = 1.0 + 0.5 * (self.level.clamp(1, 3) - 1) as f64;
                            catalog()["rules"]["max_acres"].as_f64().unwrap()
                                * (harvest * scale)
                                * scale
                        }
                    } else {
                        value.as_f64().unwrap()
                            * if self.kind == "acre" {
                                1.0
                            } else {
                                self.level as f64
                            }
                    }
                } else {
                    value.as_f64().unwrap() * self.multiplier()
                };
                (key.clone(), amount)
            })
            .collect()
    }
    /// Inventory excludes processor inputs; pending output reserves room once.
    pub fn room(&self, key: &str, inventory: &Stock, pending: &Stock, provisions: f64) -> f64 {
        if !self.accepts(key) {
            return 0.0;
        }
        let occupied = if key == "food" {
            provisions
        } else {
            let k = ResourceKey::parse(key).expect("validated storage key");
            inventory.quantity(&k).get() + pending.quantity(&k).get()
        };
        let allowance = (self.allocations().get(key).copied().unwrap_or(0.0) - occupied).max(0.0);
        if self.kind == "storehouse" || key == "food" || public_catalog()["edible"][key].is_number()
        {
            return allowance;
        }
        let materials: f64 = inventory
            .entries()
            .iter()
            .chain(pending.entries())
            .filter(|(k, _)| k.food_value() == 0.0)
            .map(|(_, v)| v.get())
            .sum();
        allowance.min((self.size() - materials).max(0.0))
    }
    pub fn input_size(&self) -> f64 {
        public_catalog()["workplace_storage_sizes"][&self.kind]
            .as_f64()
            .unwrap_or(50.0)
            * self.multiplier()
    }
}
