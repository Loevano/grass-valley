//! Experimental R13 quantities, reviewed catalogs and single-owner town accounting.
//!
//! A Stock owns physical item quantities, never the legacy aggregate `food` score.
//! Ledger derives spendable stock, food value, reserves and pending totals from
//! its containers. Recipe evaluation, live Cargo/BuildingWorld integration, HTTP
//! and legacy save conversion remain later work. A standalone Stock grants no
//! permission to spend another owner's cargo.
mod activity;
mod ledger;
mod storage;
pub use activity::{Activity, Bucket, Direction};
pub use ledger::{CargoKind, HeldCargo, Ledger, PendingOwner, ResourceRow, Store};
pub use storage::StoragePolicy;

use lantern_core::units::Quantity;
use serde_json::Value;
use std::{collections::BTreeMap, sync::OnceLock};

/// Full Python public metadata, data only. Rules and HTTP export are separate.
pub fn public_catalog() -> &'static Value {
    &catalog()["public"]
}

fn catalog() -> &'static Value {
    static CATALOG: OnceLock<Value> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("resources/catalog.json"))
            .expect("reviewed resource catalog")
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceError {
    UnknownResource,
    AggregateFood,
    DuplicateResource,
    InsufficientStock,
    Overflow,
    InvalidStorage,
    InvalidCap,
    Ownership,
    DuplicateIdentity,
    MissingBuilding,
    MissingCargo,
    MissingPending,
    CargoExists,
    Unfinished,
    Capacity,
    ProductionCap,
}

/// Validated canonical item key, preserving species and crafted-good names.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ResourceKey(String);
impl ResourceKey {
    pub fn parse(key: &str) -> Result<Self, ResourceError> {
        if key == "food" {
            return Err(ResourceError::AggregateFood);
        }
        if !catalog()["resources"]
            .as_array()
            .expect("resource list")
            .iter()
            .any(|value| value.as_str() == Some(key))
        {
            return Err(ResourceError::UnknownResource);
        }
        Ok(Self(key.into()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn food_value(&self) -> f64 {
        public_catalog()["edible"][self.as_str()]
            .as_f64()
            .unwrap_or(0.0)
    }
    pub fn sale_price(&self) -> f64 {
        public_catalog()["fish"][self.as_str()]["coins"]
            .as_f64()
            .or_else(|| public_catalog()["sale_prices"][self.as_str()].as_f64())
            .unwrap_or(1.0)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stock(BTreeMap<ResourceKey, Quantity>);
impl Stock {
    pub fn from_entries(
        entries: impl IntoIterator<Item = (ResourceKey, Quantity)>,
    ) -> Result<Self, ResourceError> {
        let mut stock = Self::default();
        for (key, amount) in entries {
            if stock.0.insert(key, amount).is_some() {
                return Err(ResourceError::DuplicateResource);
            }
        }
        Ok(stock)
    }
    pub fn entries(&self) -> &BTreeMap<ResourceKey, Quantity> {
        &self.0
    }
    pub fn quantity(&self, key: &ResourceKey) -> Quantity {
        self.0
            .get(key)
            .copied()
            .unwrap_or_else(|| Quantity::new(0.0).unwrap())
    }
    /// Python afford tolerance applies only to spending, never to item transfers.
    pub fn can_afford(&self, costs: &Self) -> bool {
        costs
            .0
            .iter()
            .all(|(key, cost)| self.quantity(key).get() + 1e-9 >= cost.get())
    }
    /// Check every item before mutation; tiny roundoff shortfalls clamp to zero.
    pub fn spend(&mut self, costs: &Self) -> Result<(), ResourceError> {
        if !self.can_afford(costs) {
            return Err(ResourceError::InsufficientStock);
        }
        for (key, cost) in &costs.0 {
            let remainder = (self.quantity(key).get() - cost.get()).max(0.0);
            self.0
                .insert(key.clone(), Quantity::new(remainder).unwrap());
        }
        Ok(())
    }
    /// Credit validated quantities without quantization; overflow leaves stock intact.
    pub fn credit(&mut self, key: ResourceKey, amount: Quantity) -> Result<(), ResourceError> {
        let sum = Quantity::new(self.quantity(&key).get() + amount.get())
            .map_err(|_| ResourceError::Overflow)?;
        self.0.insert(key, sum);
        Ok(())
    }
    /// Transfer actual available items. No affordability epsilon may create cargo.
    /// Validate both resulting quantities before changing either container.
    pub fn transfer_to(
        &mut self,
        destination: &mut Self,
        key: &ResourceKey,
        amount: Quantity,
    ) -> Result<(), ResourceError> {
        let available = self.quantity(key).get();
        if available < amount.get() {
            return Err(ResourceError::InsufficientStock);
        }
        let received = Quantity::new(destination.quantity(key).get() + amount.get())
            .map_err(|_| ResourceError::Overflow)?;
        let remaining = Quantity::new(available - amount.get()).unwrap();
        self.0.insert(key.clone(), remaining);
        destination.0.insert(key.clone(), received);
        Ok(())
    }
}
