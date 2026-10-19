//! Founder arbitration, gathering, stone/chest collection and explicit batches.
//! Shared supply belongs to this admitted landscape copy; R07 alone renews it.
use super::*;
use crate::fishing::{Water, Waters};
use lantern_core::{ids::WorldId, units::Tile};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ground {
    pub tile: [i32; 2],
    pub biome: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    pub kind: String,
    pub tile: [i32; 2],
    #[serde(deserialize_with = "required")]
    pub supply: Option<f64>,
    #[serde(deserialize_with = "required")]
    pub species: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stump {
    pub tile: [i32; 2],
    #[serde(deserialize_with = "required")]
    pub species: Option<String>,
    pub remaining: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Activity {
    pub job: String,
    pub kind: String,
    pub tile: [i32; 2],
}
impl Activity {
    pub(super) fn validate(&self) -> Result<(), Error> {
        if matches!(
            (self.job.as_str(), self.kind.as_str()),
            ("fishing", "water")
                | ("woodcutting", "tree")
                | ("gathering", "berry_bush" | "herb_patch")
        ) {
            Ok(())
        } else {
            Err(reject("invalid founder activity"))
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct FounderLand {
    world: WorldId,
    version: u8,
    ground: Vec<Ground>,
    resources: Vec<Resource>,
    stumps: Vec<Stump>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    version: u8,
    world: String,
    terrain_version: u8,
    ground: Vec<Ground>,
    resources: Vec<Resource>,
    stumps: Vec<Stump>,
}
impl FounderLand {
    pub fn restore(
        world: WorldId,
        version: u8,
        ground: Vec<Ground>,
        resources: Vec<Resource>,
        stumps: Vec<Stump>,
    ) -> Result<Self, Error> {
        let result = Self {
            world,
            version,
            ground,
            resources,
            stumps,
        };
        result.validate()?;
        Ok(result)
    }
    fn validate(&self) -> Result<(), Error> {
        if !(1..=7).contains(&self.version) {
            return Err(reject("unknown terrain version"));
        }
        let mut seen = std::collections::BTreeSet::new();
        for r in &self.resources {
            if !seen.insert((&r.kind, r.tile)) || r.supply.is_some_and(|n| !n.is_finite() || n < 0.)
            {
                return Err(reject("invalid shared source"));
            }
        }
        let mut tiles = std::collections::BTreeSet::new();
        for g in &self.ground {
            if !tiles.insert(g.tile) || g.biome.chars().count() != 1 {
                return Err(reject("invalid ground facts"));
            }
        }
        if self
            .stumps
            .iter()
            .any(|s| !s.remaining.is_finite() || s.remaining < 0.)
        {
            return Err(reject("invalid stump clock"));
        }
        Ok(())
    }
    pub fn resources(&self) -> &[Resource] {
        &self.resources
    }
    pub fn stumps(&self) -> &[Stump] {
        &self.stumps
    }
    fn biome(&self, t: [i32; 2]) -> Option<&str> {
        self.ground
            .iter()
            .find(|g| g.tile == t)
            .map(|g| g.biome.as_str())
    }
    pub fn encode(&self) -> Result<String, Error> {
        self.validate()?;
        serde_json::to_string(&Saved {
            version: 1,
            world: self.world.as_str().into(),
            terrain_version: self.version,
            ground: self.ground.clone(),
            resources: self.resources.clone(),
            stumps: self.stumps.clone(),
        })
        .map_err(|_| reject("invalid land snapshot"))
    }
    pub fn decode(world: &WorldId, json: &str) -> Result<Self, Error> {
        crate::legacy::validate_social_snapshot(json.as_bytes())
            .map_err(|_| reject("invalid land snapshot"))?;
        let s: Saved = serde_json::from_str(json).map_err(|_| reject("invalid land snapshot"))?;
        if s.version != 1 || s.world != world.as_str() {
            return Err(reject("version or world mismatch"));
        }
        Self::restore(
            world.clone(),
            s.terrain_version,
            s.ground,
            s.resources,
            s.stumps,
        )
    }
    fn take(&mut self, index: usize, amount: f64) -> Result<f64, Error> {
        let r = &self.resources[index];
        if r.kind == "water" {
            let mut waters = Waters::new(
                self.resources
                    .iter()
                    .filter(|r| r.kind == "water")
                    .map(|r| {
                        Ok(Water {
                            tile: Tile {
                                x: r.tile[0],
                                y: r.tile[1],
                            },
                            supply: q(r.supply.unwrap_or(12.))?,
                        })
                    })
                    .collect::<Result<Vec<_>, Error>>()?,
            )
            .map_err(|_| reject("invalid water supply"))?;
            let source = Tile {
                x: r.tile[0],
                y: r.tile[1],
            };
            let members = waters.component(source);
            let taken = waters.take(source, amount);
            for r in self.resources.iter_mut().filter(|r| {
                r.kind == "water"
                    && members.contains(&Tile {
                        x: r.tile[0],
                        y: r.tile[1],
                    })
            }) {
                r.supply = Some(
                    waters
                        .water()
                        .iter()
                        .find(|w| [w.tile.x, w.tile.y] == r.tile)
                        .unwrap()
                        .supply
                        .get(),
                );
            }
            return Ok(taken);
        }
        let maximum = match r.kind.as_str() {
            "tree" => {
                if self.biome(r.tile) == Some("b") || r.species.as_deref() == Some("ancient") {
                    40.
                } else {
                    8.
                }
            }
            "berry_bush" => 6.,
            "herb_patch" => 4.,
            _ => return Err(reject("not a gathering source")),
        };
        let remaining = r.supply.unwrap_or(maximum);
        let taken = amount.min(remaining);
        self.resources[index].supply = Some(remaining - taken);
        if self.resources[index].kind == "tree" && remaining - taken <= 1e-9 {
            let r = self.resources.remove(index);
            if self.version < 6 || self.biome(r.tile).is_some_and(|b| matches!(b, "f" | "b")) {
                self.stumps.push(Stump {
                    tile: r.tile,
                    species: r.species,
                    remaining: if maximum == 8. { 300. } else { 900. },
                });
            }
        }
        Ok(taken)
    }
}
fn distance(p: Position, t: [i32; 2]) -> f64 {
    (p.x.get() - f64::from(t[0])).abs() + (p.y.get() - f64::from(t[1])).abs()
}
impl Social {
    /// Failed acquisition leaves existing work intact. Chest/mine commands are
    /// immediate and remove the real shared object before another founder can claim.
    #[allow(clippy::too_many_arguments)]
    pub fn start_activity(
        &mut self,
        owner: &Owner,
        town: &Town,
        ledger: &mut Ledger,
        land: &mut FounderLand,
        position: Position,
        action: &str,
        inside_adventure: bool,
    ) -> Result<(), Error> {
        self.authorize(owner, town, ledger)?;
        if land.world != owner.world || (inside_adventure && action != "stop_activity") {
            return Err(reject("world mismatch or leave adventure first"));
        }
        let (mut s, mut l, mut map) = (self.clone(), ledger.clone(), land.clone());
        s.start(&mut l, &mut map, position, action)?;
        s.validate()?;
        map.validate()?;
        *self = s;
        *ledger = l;
        *land = map;
        Ok(())
    }
    fn start(
        &mut self,
        l: &mut Ledger,
        map: &mut FounderLand,
        p: Position,
        action: &str,
    ) -> Result<(), Error> {
        if action == "stop_activity" {
            self.player.activity = None;
            self.player.activity_status = None;
            return Ok(());
        }
        if action == "gather"
            && let Some(i) = map
                .resources
                .iter()
                .position(|r| r.kind == "chest" && distance(p, r.tile) <= 1.8)
        {
            let chest = map.resources.remove(i);
            l.credit_coins(q(25.)?)?;
            let k = if (i64::from(chest.tile[0]) + i64::from(chest.tile[1])).rem_euclid(2) == 0 {
                "swamp_boots"
            } else {
                "desert_clothing"
            };
            increment(&mut self.player.items, k)?;
            self.player.activity = None;
            self.player.activity_status = Some(format!(
                "Found 25 coins and {}",
                catalog()["equipment"][k]["name"]
                    .as_str()
                    .unwrap()
                    .to_lowercase()
            ));
            return Ok(());
        }
        let mut nearest: Option<(usize, f64)> = None;
        for (i, r) in map.resources.iter().enumerate() {
            let eligible = match action {
                "mine" => {
                    r.kind == "rock" && map.biome(r.tile).is_some_and(|b| matches!(b, "g" | "m"))
                }
                "chop" => r.kind == "tree",
                "gather" => matches!(r.kind.as_str(), "berry_bush" | "herb_patch"),
                "fish" => r.kind == "water",
                _ => return Err(reject("unknown founder activity")),
            };
            let d = distance(p, r.tile);
            if eligible && d <= 1.8 && nearest.is_none_or(|(_, old)| d < old) {
                nearest = Some((i, d));
            }
        }
        let i = nearest.ok_or_else(|| reject("no reachable source"))?.0;
        if action == "mine" {
            let stone = key("stone")?;
            if l.production_room(&stone)? < 10. || l.storage_room("stone")? < 10. {
                return Err(reject("stone cap or storage insufficient"));
            }
            l.deliver(&stone, q(10.)?, true)?;
            map.resources.remove(i);
            self.player.activity = None;
            self.player.activity_status = Some("Found 10 stone; stone permanently cleared".into());
            return Ok(());
        }
        let r = &map.resources[i];
        let job = match action {
            "fish" => "fishing",
            "chop" => "woodcutting",
            _ => "gathering",
        };
        let activity = Activity {
            job: job.into(),
            kind: r.kind.clone(),
            tile: r.tile,
        };
        if self.player.activity.as_ref() == Some(&activity) {
            self.player.activity = None;
            self.player.activity_status = None;
        } else {
            self.player.activity = Some(activity);
            self.player.activity_status = Some("Working".into());
            let (dx, dy) = (
                f64::from(r.tile[0]) - p.x.get(),
                f64::from(r.tile[1]) - p.y.get(),
            );
            if dx != 0. || dy != 0. {
                self.player.facing = if dx > dy { 1 } else { -1 };
            }
        }
        Ok(())
    }
    /// One economic batch, not arbitrary elapsed time. Idempotency includes
    /// supply consumed by missed catches and practice awarded for attempts.
    #[allow(clippy::too_many_arguments)]
    pub fn activity_tick(
        &mut self,
        owner: &Owner,
        town: &Town,
        ledger: &mut Ledger,
        land: &mut FounderLand,
        position: Position,
        batch: u64,
        roll: u8,
        boat_tier: u8,
    ) -> Result<(), Error> {
        self.authorize(owner, town, ledger)?;
        if land.world != owner.world || batch == 0 || roll >= 5 || boat_tier > 3 {
            return Err(reject("invalid founder batch"));
        }
        if batch <= self.player.last_batch {
            return Ok(());
        }
        let (mut s, mut l, mut map) = (self.clone(), ledger.clone(), land.clone());
        s.player.last_batch = batch;
        s.produce(&mut l, &mut map, position, roll, boat_tier)?;
        s.validate()?;
        map.validate()?;
        *self = s;
        *ledger = l;
        *land = map;
        Ok(())
    }
    fn produce(
        &mut self,
        l: &mut Ledger,
        map: &mut FounderLand,
        p: Position,
        roll: u8,
        boat: u8,
    ) -> Result<(), Error> {
        let Some(a) = self.player.activity.clone() else {
            return Ok(());
        };
        let Some(i) = map
            .resources
            .iter()
            .position(|r| r.kind == a.kind && r.tile == a.tile && distance(p, r.tile) <= 1.8)
        else {
            self.player.activity = None;
            return Ok(());
        };
        let good = match a.kind.as_str() {
            "tree" => "wood",
            "water" => "fish",
            "berry_bush" => "berries",
            "herb_patch" => "herbs",
            _ => return Err(reject("invalid founder source")),
        };
        let cap = l.production_room(&key(good)?)?;
        let storage = l.storage_room(good)?;
        let blocked = if cap <= 0. {
            Some("Production cap reached")
        } else if storage <= 0. {
            Some("Storage full")
        } else if map.resources[i].supply.unwrap_or(0.) <= 0. {
            Some("Waiting for supply to regenerate")
        } else {
            None
        };
        if let Some(status) = blocked {
            self.player.activity_status = Some(status.into());
            return Ok(());
        }
        let practice = self.player.practice.get(&a.job).copied().unwrap_or(0.);
        let afloat = map.resources.iter().any(|r| {
            r.kind == "water" && r.tile == [(p.x.get() + 0.5) as i32, (p.y.get() + 0.5) as i32]
        });
        let boat_bonus = if a.job == "fishing" && afloat {
            0.25 * f64::from(boat.saturating_sub(1))
        } else {
            0.
        };
        let clothing = if a.job == "gathering" {
            crate::progression::gathering_bonus(&self.player.equipment, &key(good)?)
        } else {
            0.
        };
        let base = if good == "wood" { 0.75 } else { 1. };
        let mut personal =
            crate::progression::Worker::new(&a.job).map_err(|_| reject("invalid personal job"))?;
        personal
            .set_practice(
                &a.job,
                lantern_core::units::Practice::new(practice)
                    .map_err(|_| reject("invalid practice"))?,
            )
            .map_err(|_| reject("invalid practice"))?;
        let efficiency = personal
            .efficiency(&a.job)
            .map_err(|_| reject("invalid personal efficiency"))?;
        let amount = map.take(
            i,
            cap.min(storage)
                .min(base * efficiency * (1. + boat_bonus) * (1. + clothing)),
        )?;
        let caught = a.job != "fishing" || roll != 0;
        if caught {
            l.deliver(&key(good)?, q(amount)?, true)?;
        }
        let next = practice + if a.job == "fishing" { 6. } else { 2. };
        if !next.is_finite() {
            return Err(reject("practice overflow"));
        }
        self.player.practice.insert(a.job, next);
        self.player.activity_status = Some(
            if caught {
                "Working"
            } else {
                "No catch — trying again"
            }
            .into(),
        );
        Ok(())
    }
}
