//! Fixed-command founder movement, independent of worker routes, tasks and cargo.
//! Scene geometry is derived, terrain is a read-only host query. Shared-world
//! assembly and blocked-start recovery belong to the host, never prediction.
use crate::{
    spatial::{SpatialIndex, fence_blocked},
    units::{Position, Tile},
};

pub const RADIUS: f64 = 0.18;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MovementError {
    InvalidDirection,
    InvalidBatch,
    InvalidAccess,
    NeedsRecovery,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Direction {
    dx: i8,
    dy: i8,
}
impl Direction {
    pub fn new(dx: i32, dy: i32) -> Result<Self, MovementError> {
        if !(-1..=1).contains(&dx) || !(-1..=1).contains(&dy) {
            return Err(MovementError::InvalidDirection);
        }
        Ok(Self {
            dx: dx as i8,
            dy: dy as i8,
        })
    }
}
/// Numeric boat levels are not clamped to catalog tiers: Python retains saved
/// numeric levels, including fractional/negative values. Nonfinite and levels
/// whose derived speed overflows explicitly reject at this typed boundary.
#[derive(Clone, Copy, Debug)]
pub struct FounderAccess {
    boat: bool,
    water_speed: f64,
    mounted: bool,
    swamp_boots: bool,
}
impl FounderAccess {
    pub fn new(
        boat: bool,
        boat_level: f64,
        mounted: bool,
        swamp_boots: bool,
    ) -> Result<Self, MovementError> {
        let water_speed = if boat {
            1.0 + 0.25 * (boat_level - 1.0).max(0.0)
        } else {
            1.0
        };
        if !boat_level.is_finite() || !water_speed.is_finite() {
            return Err(MovementError::InvalidAccess);
        }
        Ok(Self {
            boat,
            water_speed,
            mounted,
            swamp_boots,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FounderState {
    pub position: Position,
    pub facing: i8,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovementResult {
    pub state: FounderState,
    /// Apply only to activity/activity_status, preserving all other private
    /// founder fields. A directional input clears activity even against a wall.
    pub clear_activity: bool,
}
/// Same screen metric as Python/JS; deliberately different from worker sqrt(2).
pub fn screen_distance(dx: f64, dy: f64) -> f64 {
    libm::hypot(dx - dy, (dx + dy) * 0.5) / core::f64::consts::SQRT_2
}
fn tile_at(p: Position) -> Tile {
    Tile {
        x: libm::floor(p.x.get() + 0.5) as i32,
        y: libm::floor(p.y.get() + 0.5) as i32,
    }
}
fn protected(tile: Tile, biome: Option<char>, access: FounderAccess) -> bool {
    // Reduce before multiplication so every supported i32 map size is safe.
    let (x, y) = (
        i64::from(tile.x).rem_euclid(7),
        i64::from(tile.y).rem_euclid(7),
    );
    biome != Some('b') || (x * 17 + y * 31 + x * y * 7).rem_euclid(7) % 3 != 2 || access.swamp_boots
}
/// Local scene. None means absent terrain in an ordinary legacy map, as Python;
/// a chunk adapter must explicitly return 'v' for unavailable tiles (slice 3).
pub struct FounderScene<'a, T: Fn(Tile) -> Option<char>> {
    pub spatial: &'a SpatialIndex,
    pub terrain: T,
}
impl<T: Fn(Tile) -> Option<char>> FounderScene<'_, T> {
    pub fn walkable(&self, p: Position, access: FounderAccess) -> bool {
        let (x, y, size) = (p.x.get(), p.y.get(), f64::from(self.spatial.size()));
        if x < RADIUS - 0.5
            || y < RADIUS - 0.5
            || x > size - 0.5 - RADIUS
            || y > size - 0.5 - RADIUS
            || fence_blocked(p, self.spatial.fences(), RADIUS)
        {
            return false;
        }
        let center = tile_at(p);
        for tx in i64::from(center.x) - 1..=i64::from(center.x) + 1 {
            for ty in i64::from(center.y) - 1..=i64::from(center.y) + 1 {
                let t = Tile {
                    x: tx as i32,
                    y: ty as i32,
                };
                let biome = (self.terrain)(t);
                let hazard = !self.spatial.water().contains(&t) && !protected(t, biome, access);
                if (self.spatial.founder_occupied(t, access.boat)
                    || (tx >= 0
                        && ty >= 0
                        && tx < i64::from(self.spatial.size())
                        && ty < i64::from(self.spatial.size())
                        && biome == Some('v'))
                    || hazard)
                    && (x - tx as f64).abs() < 0.5 + RADIUS
                    && (y - ty as f64).abs() < 0.5 + RADIUS
                {
                    return false;
                }
            }
        }
        true
    }
    pub fn walk(
        &self,
        mut state: FounderState,
        direction: Direction,
        access: FounderAccess,
    ) -> Result<MovementResult, MovementError> {
        if !self.walkable(state.position, access) {
            return Err(MovementError::NeedsRecovery);
        }
        let (dx, dy) = (f64::from(direction.dx), f64::from(direction.dy));
        let length = screen_distance(dx, dy);
        if length != 0.0 {
            for _ in 0..5 {
                let tile = tile_at(state.position);
                let speed = if self.spatial.water().contains(&tile) {
                    access.water_speed
                } else {
                    (if self.spatial.roads().contains(&tile) {
                        1.5
                    } else {
                        1.0
                    }) * if access.mounted { 1.75 } else { 1.0 }
                };
                let x = Position::new(
                    state.position.x.get() + dx / length * 0.06 * speed,
                    state.position.y.get(),
                )
                .map_err(|_| MovementError::InvalidAccess)?;
                if self.walkable(x, access) {
                    state.position = x;
                }
                let y = Position::new(
                    state.position.x.get(),
                    state.position.y.get() + dy / length * 0.06 * speed,
                )
                .map_err(|_| MovementError::InvalidAccess)?;
                if self.walkable(y, access) {
                    state.position = y;
                }
            }
            if dx != dy {
                state.facing = if dx > dy { 1 } else { -1 };
            }
        }
        Ok(MovementResult {
            state,
            clear_activity: length != 0.0,
        })
    }
    /// Validate the whole raw batch before movement. No time parameter: the
    /// existing command is five fixed substeps, and accepts 1–5 ordered inputs.
    /// The future shared host must refresh its local scene between commands.
    pub fn walk_batch(
        &self,
        state: FounderState,
        inputs: &[(i32, i32)],
        access: FounderAccess,
    ) -> Result<MovementResult, MovementError> {
        if inputs.is_empty() || inputs.len() > 5 {
            return Err(MovementError::InvalidBatch);
        }
        for &(dx, dy) in inputs {
            Direction::new(dx, dy)?;
        }
        let mut result = MovementResult {
            state,
            clear_activity: false,
        };
        for &(dx, dy) in inputs {
            let next = self.walk(result.state, Direction::new(dx, dy)?, access)?;
            result.state = next.state;
            result.clear_activity |= next.clear_activity;
        }
        Ok(result)
    }
}
