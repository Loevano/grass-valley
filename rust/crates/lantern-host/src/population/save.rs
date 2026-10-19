use super::*;
use lantern_core::units::Practice;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedResident {
    actor: u64,
    name: String,
    job: String,
    #[serde(deserialize_with = "required")]
    assignment: Option<u64>,
    practice: BTreeMap<String, f64>,
    #[serde(deserialize_with = "required")]
    talents: Option<BTreeMap<String, f64>>,
    sandbox: bool,
    #[serde(deserialize_with = "required")]
    tool: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    version: u8,
    world: String,
    town: String,
    sandbox: bool,
    residents: Vec<SavedResident>,
    history: History,
    returned_tools: BTreeMap<String, u64>,
}
impl Population {
    pub fn encode(&self, town: &Town) -> Result<String, Error> {
        self.validate(town)?;
        serde_json::to_string(&Saved {
            version: 1,
            world: self.owner.world.as_str().into(),
            town: self.owner.town.as_str().into(),
            sandbox: self.sandbox,
            residents: self
                .residents
                .iter()
                .map(|r| SavedResident {
                    actor: r.actor.identity().serial(),
                    name: r.name.clone(),
                    job: r.worker.job().into(),
                    assignment: r.assignment.as_ref().map(|b| b.identity().serial()),
                    practice: r
                        .worker
                        .skills()
                        .iter()
                        .map(|(j, n)| (j.clone(), n.get()))
                        .collect(),
                    talents: r.worker.talents().cloned(),
                    sandbox: r.worker.sandbox,
                    tool: r.worker.tool.clone(),
                })
                .collect(),
            history: self.history.clone(),
            returned_tools: self.returned_tools.clone(),
        })
        .map_err(|_| Error::Invalid)
    }
    /// Offline reload does not perform meals, names/talent migration, autofill or
    /// arrivals. Explicit import admission is `restore`; wire/file hydration R19.
    pub fn decode(json: &str, town: &Town) -> Result<Self, Error> {
        let s: Saved = serde_json::from_str(json).map_err(|_| Error::Invalid)?;
        if s.version != 1 {
            return Err(Error::Invalid);
        }
        if s.world != town.owner().world.as_str() || s.town != town.owner().town.as_str() {
            return Err(Error::Ownership);
        }
        let residents = s
            .residents
            .into_iter()
            .map(|r| {
                let id = |n| {
                    EntityIdentity::from_saved(town.owner().clone(), n).map_err(|_| Error::Invalid)
                };
                let mut worker = Worker::new(&r.job).map_err(|_| Error::Invalid)?;
                for (job, n) in r.practice {
                    worker
                        .set_practice(&job, Practice::new(n).map_err(|_| Error::Invalid)?)
                        .map_err(|_| Error::Invalid)?;
                }
                if let Some(talents) = r.talents {
                    worker.set_talents(talents).map_err(|_| Error::Invalid)?;
                }
                worker.sandbox = r.sandbox;
                worker.tool = r.tool;
                if r.name.trim().is_empty() || (s.sandbox && !worker.sandbox) {
                    return Err(Error::Invalid);
                }
                Ok(Resident {
                    actor: ActorId::from_saved(id(r.actor)?),
                    name: r.name,
                    worker,
                    assignment: r
                        .assignment
                        .map(|n| id(n).map(BuildingId::from_saved))
                        .transpose()?,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        if s.returned_tools.keys().any(|k| !known_tool(k)) {
            return Err(Error::Invalid);
        }
        let result = Self {
            owner: town.owner().clone(),
            sandbox: s.sandbox,
            residents,
            history: s.history,
            returned_tools: s.returned_tools,
        };
        result.validate(town)?;
        Ok(result)
    }
}

pub(super) fn required<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(d)
}
