//! Binary64 units preserve Python/browser fractional values without quantization.
//! No implicit arithmetic/conversions: rules must choose the resulting unit and
//! validate it. Comparison tolerances are not storage rounding instructions.
//!
//! ```compile_fail
//! use lantern_core::units::{Quantity, Seconds};
//! fn advance(_: Seconds) {}
//! advance(Quantity::new(0.05).unwrap());
//! ```

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitError {
    NonFinite,
    Negative,
}

macro_rules! finite_unit {
    ($name:ident, $nonnegative:expr) => {
        #[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
        pub struct $name(f64);

        impl $name {
            pub fn new(value: f64) -> Result<Self, UnitError> {
                if !value.is_finite() {
                    return Err(UnitError::NonFinite);
                }
                if $nonnegative && value < 0.0 {
                    return Err(UnitError::Negative);
                }
                Ok(Self(value))
            }

            pub fn get(self) -> f64 {
                self.0
            }
        }
    };
}

finite_unit!(Quantity, true);
finite_unit!(Work, true);
finite_unit!(Practice, true);
finite_unit!(Seconds, true);
finite_unit!(TileCoordinate, false);

/// Continuous position, independent of route/task state and cargo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Position {
    pub x: TileCoordinate,
    pub y: TileCoordinate,
}

impl Position {
    pub fn new(x: f64, y: f64) -> Result<Self, UnitError> {
        Ok(Self {
            x: TileCoordinate::new(x)?,
            y: TileCoordinate::new(y)?,
        })
    }
}

/// Discrete tile location; conversion from continuous position requires a rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tile {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Count(pub u64);

/// Kept exact internally; a future JSON boundary must use a decimal string when
/// crossing JavaScript's safe-integer limit, rather than converting through f64.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TerrainSeed(pub u64);
