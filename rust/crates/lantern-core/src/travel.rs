//! Generic authoritative travel. Task rules own arrival actions; presentation
//! samples and search state never own position or cargo.
use crate::units::Seconds;

/// Validated speed in tiles per second (Euclidean worker metric).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Speed(f64);
impl Speed {
    pub fn new(value: f64) -> Option<Self> {
        (value.is_finite() && value > 0.0).then_some(Self(value))
    }
    pub fn get(self) -> f64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    NotRequested,
    /// Includes navigation admission/resource waits, never a final failure.
    Waiting,
    Walking,
    Arrived,
    Unreachable,
    Blocked,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Advance {
    pub status: Status,
    /// Unused time is returned, never banked while waiting or blocked. The caller
    /// may use it for a subsequent task phase in the same simulation update.
    pub remaining: Seconds,
    /// True once per accepted route, only when movement consumes its final point.
    /// This is a travel notification, not a pickup/unload/work authorization.
    pub arrived_now: bool,
    pub segments: usize,
}
