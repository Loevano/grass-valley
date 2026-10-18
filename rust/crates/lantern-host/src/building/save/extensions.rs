use super::*;
use crate::dto::{
    facts::{TaskDto, TravelDto},
    wire::Decimal,
};
use lantern_core::units::Seconds;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Extensions {
    sites: Vec<SiteDetails>,
    progression: Vec<ProgressDetails>,
    helpers: Vec<HelperDetails>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SiteDetails {
    id: EntityIdDto,
    level: u8,
    capacity_level: u8,
    equipment: Vec<String>,
    #[serde(deserialize_with = "required_option")]
    parent: Option<EntityIdDto>,
    #[serde(deserialize_with = "required_option")]
    child: Option<ChildDto>,
    #[serde(deserialize_with = "required_option")]
    upgrade: Option<UpgradeDto>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum ChildDto {
    Field {
        stage: String,
        elapsed: Number,
        progress: Number,
        cycle: Decimal,
    },
    Tree {
        stage: String,
        elapsed: Number,
        fruit: Number,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum UpgradeDto {
    Structure { level: u8 },
    Capacity { level: u8 },
    Equipment { key: String },
    Hall { level: u8, reward: Number },
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProgressDetails {
    town_id: String,
    skills: UniqueMap<Number>,
    produced: UniqueMap<Number>,
    completed_trades: Decimal,
    #[serde(deserialize_with = "required_option")]
    food: Option<Number>,
    #[serde(deserialize_with = "required_option")]
    market_sales: Option<Number>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HelperDetails {
    id: EntityIdDto,
    #[serde(deserialize_with = "required_option")]
    workplace: Option<EntityIdDto>,
    #[serde(deserialize_with = "required_option")]
    suspended: Option<SuspendedDto>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SuspendedDto {
    task: TaskDto,
    travel: TravelDto,
}
impl Extensions {
    pub(super) fn from_world(w: &BuildingWorld) -> Self {
        Self {
            sites: w
                .rules
                .values()
                .flat_map(|r| {
                    r.sites.iter().map(|(id, s)| SiteDetails {
                        id: EntityIdDto::from_id(id),
                        level: s.level,
                        capacity_level: s.capacity_level,
                        equipment: s.equipment.iter().cloned().collect(),
                        parent: s.parent.as_ref().map(EntityIdDto::from_id),
                        child: s.child.as_ref().map(|c| match c {
                            ChildState::Field {
                                stage,
                                elapsed,
                                progress,
                                cycle,
                            } => ChildDto::Field {
                                stage: stage.clone(),
                                elapsed: Number::core(elapsed.get()),
                                progress: Number::core(progress.get()),
                                cycle: Decimal(*cycle),
                            },
                            ChildState::Tree {
                                stage,
                                elapsed,
                                fruit,
                            } => ChildDto::Tree {
                                stage: stage.clone(),
                                elapsed: Number::core(elapsed.get()),
                                fruit: Number::core(fruit.get()),
                            },
                        }),
                        upgrade: s.upgrade.as_ref().map(|u| match u {
                            Upgrade::Structure(level) => UpgradeDto::Structure { level: *level },
                            Upgrade::Capacity(level) => UpgradeDto::Capacity { level: *level },
                            Upgrade::Equipment(key) => UpgradeDto::Equipment { key: key.clone() },
                            Upgrade::Hall { level, reward } => UpgradeDto::Hall {
                                level: *level,
                                reward: Number::core(reward.get()),
                            },
                        }),
                    })
                })
                .collect(),
            progression: w
                .rules
                .iter()
                .map(|(id, r)| ProgressDetails {
                    town_id: id.as_str().into(),
                    skills: numbers(&r.progression.skills),
                    produced: numbers(&r.progression.produced),
                    completed_trades: Decimal(r.progression.completed_trades),
                    food: r.progression.food.map(|q| Number::core(q.get())),
                    market_sales: r.progression.market_sales.map(|q| Number::core(q.get())),
                })
                .collect(),
            helpers: w
                .builders
                .iter()
                .map(|(id, b)| HelperDetails {
                    id: EntityIdDto::from_id(id),
                    workplace: b.helper.as_ref().map(EntityIdDto::from_id),
                    suspended: b.suspended.as_ref().map(|s| SuspendedDto {
                        task: (&s.task).into(),
                        travel: (&s.travel).into(),
                    }),
                })
                .collect(),
        }
    }
    pub(super) fn apply(
        self,
        rules: &mut BTreeMap<TownId, Rules>,
        builders: &mut BTreeMap<ActorId, Builder>,
    ) -> Result<(), BoundaryError> {
        let expected = rules.values().map(|r| r.sites.len()).sum::<usize>();
        if self.sites.len() != expected
            || self.progression.len() != rules.len()
            || self.helpers.len() != builders.len()
        {
            return Err(invalid("incomplete construction extensions"));
        }
        let mut seen = BTreeSet::new();
        for e in self.sites {
            let id: BuildingId = e.id.into_id("$.extensions.sites.id")?;
            if !seen.insert(id.clone()) {
                return Err(invalid("duplicate extension identity"));
            }
            let site = rules
                .get_mut(&id.identity().owner().town)
                .and_then(|r| r.sites.get_mut(&id))
                .ok_or_else(|| invalid("unknown extension identity"))?;
            site.level = e.level;
            site.capacity_level = e.capacity_level;
            let len = e.equipment.len();
            site.equipment = e.equipment.into_iter().collect();
            if site.equipment.len() != len {
                return Err(invalid("duplicate equipment"));
            }
            site.parent = e
                .parent
                .map(|id| id.into_id("$.extensions.sites.parent"))
                .transpose()?;
            site.child = e
                .child
                .map(|c| {
                    Ok(match c {
                        ChildDto::Field {
                            stage,
                            elapsed,
                            progress,
                            cycle,
                        } => ChildState::Field {
                            stage,
                            elapsed: Seconds::new(elapsed.get())
                                .map_err(|_| invalid("invalid field clock"))?,
                            progress: Work::new(progress.get())
                                .map_err(|_| invalid("invalid field progress"))?,
                            cycle: cycle.0,
                        },
                        ChildDto::Tree {
                            stage,
                            elapsed,
                            fruit,
                        } => ChildState::Tree {
                            stage,
                            elapsed: Seconds::new(elapsed.get())
                                .map_err(|_| invalid("invalid tree clock"))?,
                            fruit: Quantity::new(fruit.get())
                                .map_err(|_| invalid("invalid fruit"))?,
                        },
                    })
                })
                .transpose()?;
            site.upgrade = e
                .upgrade
                .map(|u| {
                    Ok(match u {
                        UpgradeDto::Structure { level } => Upgrade::Structure(level),
                        UpgradeDto::Capacity { level } => Upgrade::Capacity(level),
                        UpgradeDto::Equipment { key } => Upgrade::Equipment(key),
                        UpgradeDto::Hall { level, reward } => Upgrade::Hall {
                            level,
                            reward: Quantity::new(reward.get())
                                .map_err(|_| invalid("invalid reward"))?,
                        },
                    })
                })
                .transpose()?;
        }
        let mut seen = BTreeSet::new();
        for p in self.progression {
            let id = TownId::new(p.town_id).map_err(|_| invalid("invalid progression owner"))?;
            if !seen.insert(id.clone()) {
                return Err(invalid("duplicate progression owner"));
            }
            rules
                .get_mut(&id)
                .ok_or_else(|| invalid("unknown progression owner"))?
                .progression = Progression {
                skills: quantities(p.skills)?,
                produced: quantities(p.produced)?,
                completed_trades: p.completed_trades.0,
                food: p
                    .food
                    .map(|n| Quantity::new(n.get()).map_err(|_| invalid("invalid food")))
                    .transpose()?,
                market_sales: p
                    .market_sales
                    .map(|n| Quantity::new(n.get()).map_err(|_| invalid("invalid sales")))
                    .transpose()?,
            };
        }
        let mut seen = BTreeSet::new();
        for h in self.helpers {
            let id: ActorId = h.id.into_id("$.extensions.helpers.id")?;
            if !seen.insert(id.clone()) {
                return Err(invalid("duplicate helper identity"));
            }
            let b = builders
                .get_mut(&id)
                .ok_or_else(|| invalid("unknown helper identity"))?;
            b.helper = h
                .workplace
                .map(|id| id.into_id("$.extensions.helpers.workplace"))
                .transpose()?;
            b.suspended = h
                .suspended
                .map(|s| {
                    Ok(Suspended {
                        task: s.task.into_core("$.extensions.helpers.task")?,
                        travel: s.travel.into_core("$.extensions.helpers.travel")?,
                    })
                })
                .transpose()?;
        }
        Ok(())
    }
}
