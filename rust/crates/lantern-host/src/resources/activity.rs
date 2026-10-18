//! Host simulation-time flows. Transfers/reservations never call record.
use super::{ResourceError, ResourceKey};
use lantern_core::units::{Quantity, Seconds};
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    In,
    Out,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bucket {
    pub at: f64,
    pub incoming: BTreeMap<String, f64>,
    pub outgoing: BTreeMap<String, f64>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Activity {
    seconds: f64,
    buckets: VecDeque<Bucket>,
}
impl Activity {
    pub fn seconds(&self) -> f64 {
        self.seconds
    }
    pub fn buckets(&self) -> &VecDeque<Bucket> {
        &self.buckets
    }
    pub fn advance(&mut self, elapsed: Seconds) -> Result<(), ResourceError> {
        let next = self.seconds + elapsed.get();
        if !next.is_finite() {
            return Err(ResourceError::Overflow);
        }
        self.seconds = next;
        while self.buckets.front().is_some_and(|b| b.at <= next - 60.0) {
            self.buckets.pop_front();
        }
        Ok(())
    }
    pub fn record(
        &mut self,
        key: &ResourceKey,
        amount: Quantity,
        direction: Direction,
    ) -> Result<(), ResourceError> {
        self.record_value(key.as_str(), amount.get(), key.food_value(), direction)
    }
    pub(super) fn record_food(
        &mut self,
        amount: f64,
        direction: Direction,
    ) -> Result<(), ResourceError> {
        self.record_value("food", amount, 0.0, direction)
    }
    fn record_value(
        &mut self,
        key: &str,
        amount: f64,
        food_value: f64,
        direction: Direction,
    ) -> Result<(), ResourceError> {
        if amount <= 1e-9 {
            return Ok(());
        }
        let mut next = self.clone();
        if next.buckets.back().is_none_or(|b| b.at != next.seconds) {
            next.buckets.push_back(Bucket {
                at: next.seconds,
                ..Bucket::default()
            });
        }
        let bucket = next.buckets.back_mut().unwrap();
        let values = match direction {
            Direction::In => &mut bucket.incoming,
            Direction::Out => &mut bucket.outgoing,
        };
        add(values, key, amount)?;
        if food_value != 0.0 {
            add(values, "food", amount * food_value)?;
        }
        next.totals()?;
        *self = next;
        Ok(())
    }
    pub fn totals(&self) -> Result<Bucket, ResourceError> {
        let mut total = Bucket {
            at: self.seconds,
            ..Bucket::default()
        };
        for bucket in &self.buckets {
            for (key, amount) in &bucket.incoming {
                add(&mut total.incoming, key, *amount)?;
            }
            for (key, amount) in &bucket.outgoing {
                add(&mut total.outgoing, key, *amount)?;
            }
        }
        Ok(total)
    }
}
fn add(values: &mut BTreeMap<String, f64>, key: &str, amount: f64) -> Result<(), ResourceError> {
    let sum = values.get(key).copied().unwrap_or(0.0) + amount;
    if !sum.is_finite() {
        return Err(ResourceError::Overflow);
    }
    values.insert(key.into(), sum);
    Ok(())
}
