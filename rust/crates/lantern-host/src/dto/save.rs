use super::{
    facts::*,
    wire::{Decimal, Number, UniqueMap},
};
use crate::{
    BoundaryError, ErrorKind,
    state::{ModelWorld, PrivateAccount, PrivateCredentials},
};
use lantern_core::{
    ids::{Owner, TownId, WorldId},
    model::{ActorState, Building, Cargo, RouteGeneration, TaskGeneration, Town},
    units::{Quantity, TerrainSeed, Tile},
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
enum Format {
    #[serde(rename = "lantern-model")]
    Model,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveWorldDto {
    format: Format,
    version: u32,
    world_id: String,
    seed: Decimal,
    high_water: Decimal,
    towns: Vec<SaveTownDto>,
    credentials: CredentialsDto,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveTownDto {
    town_id: String,
    buildings: Vec<BuildingDto>,
    actors: Vec<ActorDto>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BuildingDto {
    id: EntityIdDto,
    x: i32,
    y: i32,
    construction: ConstructionDto,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ActorDto {
    id: EntityIdDto,
    position: PositionDto,
    cargo: UniqueMap<Number>,
    task: TaskDto,
    task_generation: Decimal,
    route_generation: Decimal,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AccountDto {
    name: String,
    town_id: String,
    salt: String,
    pin_hash: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialsDto {
    tokens: UniqueMap<String>,
    players: UniqueMap<AccountDto>,
}

impl From<&ModelWorld> for SaveWorldDto {
    fn from(world: &ModelWorld) -> Self {
        Self {
            format: Format::Model,
            version: 1,
            world_id: world.id.as_str().into(),
            seed: Decimal(world.seed.0),
            high_water: Decimal(world.high_water()),
            towns: world
                .towns
                .iter()
                .map(|town| SaveTownDto {
                    town_id: town.owner().town.as_str().into(),
                    buildings: town
                        .buildings()
                        .iter()
                        .map(|b| BuildingDto {
                            id: EntityIdDto::from_id(&b.id),
                            x: b.location.x,
                            y: b.location.y,
                            construction: (&b.construction).into(),
                        })
                        .collect(),
                    actors: town
                        .actors()
                        .iter()
                        .map(|actor| {
                            let a = actor.state();
                            ActorDto {
                                id: EntityIdDto::from_id(&a.id),
                                position: a.position.into(),
                                cargo: UniqueMap(
                                    a.cargo
                                        .goods()
                                        .iter()
                                        .map(|(k, q)| (k.clone(), Number::core(q.get())))
                                        .collect(),
                                ),
                                task: (&a.task).into(),
                                task_generation: Decimal(a.task_generation.get()),
                                route_generation: Decimal(a.route_generation.get()),
                            }
                        })
                        .collect(),
                })
                .collect(),
            credentials: CredentialsDto {
                tokens: UniqueMap(
                    world
                        .credentials
                        .tokens
                        .iter()
                        .map(|(key, town)| (key.clone(), town.as_str().into()))
                        .collect(),
                ),
                players: UniqueMap(
                    world
                        .credentials
                        .players
                        .iter()
                        .map(|(key, a)| {
                            (
                                key.clone(),
                                AccountDto {
                                    name: a.name.clone(),
                                    town_id: a.town_id.as_str().into(),
                                    salt: a.salt.clone(),
                                    pin_hash: a.pin_hash.clone(),
                                },
                            )
                        })
                        .collect(),
                ),
            },
        }
    }
}

impl SaveWorldDto {
    fn into_world(self) -> Result<ModelWorld, BoundaryError> {
        if self.version != 1 {
            return Err(BoundaryError::invalid(
                "$.version",
                "unsupported model schema",
            ));
        }
        let world_id = WorldId::new(self.world_id)
            .map_err(|e| BoundaryError::at("$.world_id", ErrorKind::Identity(e)))?;
        let mut towns = Vec::new();
        for (ti, town) in self.towns.into_iter().enumerate() {
            let path = format!("$.towns[{ti}]");
            let owner = Owner {
                world: world_id.clone(),
                town: TownId::new(town.town_id).map_err(|e| {
                    BoundaryError::at(format!("{path}.town_id"), ErrorKind::Identity(e))
                })?,
            };
            let mut buildings = Vec::new();
            for (bi, b) in town.buildings.into_iter().enumerate() {
                let bp = format!("{path}.buildings[{bi}]");
                let id = b.id.into_id(&format!("{bp}.id"))?;
                id.identity()
                    .validate_owner(&owner)
                    .map_err(|e| BoundaryError::at(format!("{bp}.id"), ErrorKind::Identity(e)))?;
                buildings.push(Building {
                    id,
                    location: Tile { x: b.x, y: b.y },
                    construction: b.construction.into_core(&format!("{bp}.construction"))?,
                });
            }
            let mut actors = Vec::new();
            for (ai, a) in town.actors.into_iter().enumerate() {
                let ap = format!("{path}.actors[{ai}]");
                let mut goods = Vec::new();
                for (qi, (key, amount)) in a.cargo.0.into_iter().enumerate() {
                    let quantity = Quantity::new(amount.get()).map_err(|e| {
                        BoundaryError::at(format!("{ap}.cargo[{qi}]"), ErrorKind::Unit(e))
                    })?;
                    goods.push((key, quantity));
                }
                let id = a.id.into_id(&format!("{ap}.id"))?;
                id.identity()
                    .validate_owner(&owner)
                    .map_err(|e| BoundaryError::at(format!("{ap}.id"), ErrorKind::Identity(e)))?;
                actors.push(ActorState {
                    id,
                    position: a.position.into_core(&format!("{ap}.position"))?,
                    cargo: Cargo::from_entries(goods).map_err(|e| {
                        BoundaryError::at(format!("{ap}.cargo"), ErrorKind::Model(e))
                    })?,
                    task: a.task.into_core(&format!("{ap}.task"))?,
                    task_generation: TaskGeneration::from_saved(a.task_generation.0),
                    route_generation: RouteGeneration::from_saved(a.route_generation.0),
                });
            }
            towns.push(
                Town::restore(owner, buildings, actors)
                    .map_err(|e| BoundaryError::at(&path, ErrorKind::Model(e)))?,
            );
        }
        let mut credentials = PrivateCredentials::default();
        for (index, (token, town)) in self.credentials.tokens.0.into_iter().enumerate() {
            credentials.tokens.insert(
                token,
                TownId::new(town).map_err(|e| {
                    BoundaryError::at(
                        format!("$.credentials.tokens[{index}]"),
                        ErrorKind::Identity(e),
                    )
                })?,
            );
        }
        for (index, (key, a)) in self.credentials.players.0.into_iter().enumerate() {
            credentials.players.insert(
                key,
                PrivateAccount {
                    name: a.name,
                    town_id: TownId::new(a.town_id).map_err(|e| {
                        BoundaryError::at(
                            format!("$.credentials.players[{index}].town_id"),
                            ErrorKind::Identity(e),
                        )
                    })?,
                    salt: a.salt,
                    pin_hash: a.pin_hash,
                },
            );
        }
        ModelWorld::new(
            world_id,
            TerrainSeed(self.seed.0),
            self.high_water.0,
            towns,
            credentials,
        )
    }
}

fn json_error(error: serde_json::Error) -> BoundaryError {
    BoundaryError::at(
        "$",
        ErrorKind::Json {
            line: error.line(),
            column: error.column(),
        },
    )
}

/// Contains private credential material. Never send this output to a browser.
pub fn encode_save(world: &ModelWorld) -> Result<String, BoundaryError> {
    serde_json::to_string(&SaveWorldDto::from(world)).map_err(json_error)
}

/// In-memory R05 format only. Structural errors report $ + source line/column;
/// semantic errors report schema paths. Raw parser messages/values are withheld.
pub fn decode_save(json: &str) -> Result<ModelWorld, BoundaryError> {
    serde_json::from_str::<SaveWorldDto>(json)
        .map_err(json_error)?
        .into_world()
}
