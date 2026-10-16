//! Read-only local founder scenarios. Exact source bytes remain private and
//! untouched; results describe movement only, never a lossy rewritten player.
use super::{PrivateSpatialCopy, json::Node};
use crate::BoundaryError;
use lantern_core::{
    founder::{FounderAccess, FounderScene, FounderState, MovementResult},
    units::{Position, Tile},
};

/// No implicit Debug/Serialize: this contains arbitrary private source fields.
/// ```compile_fail
/// use lantern_host::legacy::PrivateFounderCopy;
/// fn leak(value: &PrivateFounderCopy) { std::format!("{value:?}"); }
/// ```
pub struct PrivateFounderCopy {
    pub(super) geometry: PrivateSpatialCopy,
    terrain: Vec<Vec<char>>,
    pub(super) state: FounderState,
    pub(super) access: FounderAccess,
}
impl PrivateFounderCopy {
    pub fn decode(source: &[u8]) -> Result<Self, BoundaryError> {
        let p = "$.founder_copy";
        let root = Node::parse(source, p)?;
        // Chunk terrain requires host assembly and explicit missing-tile rules.
        if root
            .optional("chunked", p)?
            .map(|v| v.boolean(p))
            .transpose()?
            .unwrap_or(false)
        {
            return Err(BoundaryError::invalid(
                p,
                "chunked founder scene requires assembly",
            ));
        }
        let player = root.required("player", p)?;
        let pp = "$.founder_copy.player";
        let x = number(player.required("x", pp)?, "$.founder_copy.player.x")?;
        let y = number(player.required("y", pp)?, "$.founder_copy.player.y")?;
        let facing = player
            .optional("facing", pp)?
            .map(|v| v.tile(pp))
            .transpose()?
            .unwrap_or(1);
        if ![-1, 1].contains(&facing) {
            return Err(BoundaryError::invalid(pp, "unsupported facing"));
        }
        let flag = |key| {
            player
                .optional(key, pp)?
                .map(|v| v.boolean(pp))
                .transpose()
                .map(|v| v.unwrap_or(false))
        };
        let boat_level = player
            .optional("boat_level", pp)?
            .map(|v| number(v, "$.founder_copy.player.boat_level"))
            .transpose()?
            .unwrap_or(1.0);
        let boots = player
            .optional("equipment", pp)?
            .map(|equipment| {
                equipment
                    .optional("feet", pp)?
                    .map(|v| v.string(pp))
                    .transpose()
                    .map(|v| v.as_deref() == Some("swamp_boots"))
            })
            .transpose()?
            .unwrap_or(false);
        let access = FounderAccess::new(flag("boat")?, boat_level, flag("mounted")?, boots)
            .map_err(|_| BoundaryError::invalid(pp, "unsupported movement access"))?;
        let mut terrain = Vec::new();
        if let Some(rows) = root.optional("terrain", p)?.filter(|v| !v.is_null()) {
            for row in rows.array(p)? {
                let chars = match row {
                    Node::Array(cells) => cells
                        .iter()
                        .map(|cell| {
                            let text = cell.string(p)?;
                            let mut chars = text.chars();
                            let value = chars
                                .next()
                                .ok_or_else(|| BoundaryError::invalid(p, "empty terrain cell"))?;
                            if chars.next().is_some() {
                                return Err(BoundaryError::invalid(p, "unsupported terrain cell"));
                            }
                            Ok(value)
                        })
                        .collect::<Result<Vec<_>, BoundaryError>>()?,
                    _ => row.string(p)?.chars().collect(),
                };
                terrain.push(chars);
            }
        }
        Ok(Self {
            geometry: PrivateSpatialCopy::decode(source)?,
            terrain,
            state: FounderState {
                position: Position::new(x, y)
                    .map_err(|_| BoundaryError::invalid(pp, "unsupported position"))?,
                facing: facing as i8,
            },
            access,
        })
    }
    pub fn private_bytes(&self) -> &[u8] {
        self.geometry.private_bytes()
    }
    pub fn state(&self) -> FounderState {
        self.state
    }
    pub(super) fn terrain_at(&self, tile: Tile) -> Option<char> {
        usize::try_from(tile.y)
            .ok()
            .and_then(|y| self.terrain.get(y))
            .and_then(|row| usize::try_from(tile.x).ok().and_then(|x| row.get(x)))
            .copied()
    }
    pub fn walkable(&self, position: Position) -> Result<bool, BoundaryError> {
        let index = self.geometry.derive_index()?;
        Ok(FounderScene {
            spatial: &index,
            terrain: |t| self.terrain_at(t),
        }
        .walkable(position, self.access))
    }
    /// No mutation of the source or starting state. Callers explicitly retain
    /// the returned position and clear activity/status only when requested.
    pub fn walk_batch(
        &self,
        state: FounderState,
        inputs: &[(i32, i32)],
    ) -> Result<MovementResult, BoundaryError> {
        let index = self.geometry.derive_index()?;
        FounderScene {
            spatial: &index,
            terrain: |t| self.terrain_at(t),
        }
        .walk_batch(state, inputs, self.access)
        .map_err(|error| {
            BoundaryError::invalid(
                "$.founder_copy",
                match error {
                    lantern_core::founder::MovementError::NeedsRecovery => {
                        "founder requires position recovery"
                    }
                    _ => "invalid movement input or access",
                },
            )
        })
    }
}
fn number(value: &Node, path: &str) -> Result<f64, BoundaryError> {
    let Node::Scalar(raw) = value else {
        return Err(BoundaryError::invalid(path, "expected finite number"));
    };
    raw.get()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| BoundaryError::invalid(path, "expected finite number"))
}
