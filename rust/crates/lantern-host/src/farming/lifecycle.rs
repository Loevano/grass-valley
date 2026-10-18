use super::*;
impl Fields {
    /// Python applies a farm crop choice to linked acres, including unfinished
    /// ones; sown crops/yields, claims, water and carried goods remain untouched.
    pub fn set_crop(
        &mut self,
        town: &Town,
        ledger: &Ledger,
        id: &BuildingId,
        crop: &str,
        hall_level: u8,
    ) -> Result<(), Error> {
        town.building(id)?;
        let kind = &ledger.store(id)?.policy.kind;
        progression::crop_unlocked(kind, crop, hall_level)?;
        self.field(id)?;
        let crop = ResourceKey::parse(crop)?;
        for f in &mut self.fields {
            if (kind == "acre" && f.id == *id) || (kind == "farm" && f.farm.as_ref() == Some(id)) {
                f.selected_crop = crop.clone();
            }
        }
        Ok(())
    }
    /// Apply after R12 validated the structural move on a private model copy;
    /// commit that copy together with this owner. This never validates placement
    /// or moves buildings itself, and preserves all ledger/crop ownership.
    pub fn reconcile_relocation(
        &mut self,
        before: &Town,
        after: &mut Town,
        ledger: &Ledger,
        nav: NavigationRevision,
    ) -> Result<(), Error> {
        if before.owner() != after.owner() {
            return Err(Error::Invalid);
        }
        let (mut owner, mut model) = (self.clone(), after.clone());
        for (a, c) in self.claims.clone() {
            let source = before.building(&c.field)?.location != model.building(&c.field)?.location;
            let home =
                before.building(&c.workplace)?.location != model.building(&c.workplace)?.location;
            if !(source || home) || !Self::matches(&model, &a, &c) {
                continue;
            }
            let p = phase(&model, &a)?;
            let mut next = c.clone();
            if source {
                next.destination = work_destination(&model, ledger, owner.field(&c.field)?)?;
            }
            let water_trip = matches!(p, WorkPhase::FetchingWater | WorkPhase::CollectingWater);
            let returning = matches!(p, WorkPhase::Returning | WorkPhase::Depositing);
            if (source && !water_trip) || (home && returning) {
                let phase = if returning || pending(ledger, &a)?.as_ref() == Some(&c.workplace) {
                    WorkPhase::Returning
                } else if p == WorkPhase::ReturningWater {
                    WorkPhase::ReturningWater
                } else {
                    WorkPhase::Outbound
                };
                let goal = if phase == WorkPhase::Returning {
                    Destination::Building(c.workplace.clone())
                } else {
                    next.destination.clone()
                };
                next.clock = 0.;
                model.set_task(&a, task(&next, phase))?;
                model.request_route(&a, goal, nav)?;
            } else {
                let travel = model.actor(&a)?.state().travel.clone();
                model.restore_task_intent(&a, task(&next, p), travel)?;
            }
            owner.claims.insert(a, next);
        }
        for previous in owner.previous.values_mut() {
            if let (Ok(old), Ok(new)) = (
                before.building(&previous.field),
                model.building(&previous.field),
            ) && old.location != new.location
            {
                previous.destination =
                    work_destination(&model, ledger, self.field(&previous.field)?)?;
            }
        }
        *self = owner;
        *after = model;
        Ok(())
    }
    /// Admit just the legacy agricultural building facts through an explicit
    /// index-to-ID map. Movement, cargo and credentials are owned by their import
    /// adapters; no load-time production or rewriting of the source occurs here.
    pub fn admit_legacy(
        town: &Town,
        ledger: &Ledger,
        buildings: &[serde_json::Value],
        ids: &[BuildingId],
        assignments: BTreeMap<ActorId, BuildingId>,
    ) -> Result<Self, Error> {
        if buildings.len() != ids.len() {
            return Err(Error::Invalid);
        }
        let mut seen = BTreeSet::new();
        let mut fields = Vec::new();
        for (b, id) in buildings.iter().zip(ids) {
            town.building(id)?;
            if !seen.insert(id.clone())
                || b["kind"].as_str() != Some(ledger.store(id)?.policy.kind.as_str())
            {
                return Err(Error::Invalid);
            }
            let kind = b["kind"].as_str().ok_or(Error::Invalid)?;
            if !matches!(kind, "farm" | "acre") {
                continue;
            }
            let parent = if let Some(index) = b.get("farm_index") {
                Some(
                    ids.get(
                        usize::try_from(index.as_u64().ok_or(Error::Invalid)?)
                            .map_err(|_| Error::Invalid)?,
                    )
                    .ok_or(Error::Invalid)?
                    .clone(),
                )
            } else {
                None
            };
            let selected = ResourceKey::parse(match b.get("crop") {
                Some(s) => s.as_str().ok_or(Error::Invalid)?,
                None => "vegetables",
            })?;
            let state = if let Some(raw) = b.get("field") {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Legacy {
                    stage: String,
                    elapsed: f64,
                    progress: f64,
                    cycle: u64,
                    crop: Option<String>,
                    yield_remaining: Option<f64>,
                }
                let s: Legacy = serde_json::from_value(raw.clone()).map_err(|_| Error::Invalid)?;
                let stage = save::parse_stage(&s.stage)?;
                FieldState {
                    stage,
                    elapsed: Seconds::new(s.elapsed).map_err(|_| Error::Invalid)?,
                    progress: Seconds::new(s.progress).map_err(|_| Error::Invalid)?,
                    cycle: s.cycle,
                    crop: if matches!(stage, Stage::Unplowed | Stage::Plowed) {
                        s.crop.map(|s| ResourceKey::parse(&s)).transpose()?
                    } else {
                        Some(
                            s.crop
                                .map(|s| ResourceKey::parse(&s))
                                .transpose()?
                                .unwrap_or_else(|| selected.clone()),
                        )
                    },
                    yield_remaining: s
                        .yield_remaining
                        .map(|n| Quantity::new(n).map_err(|_| Error::Invalid))
                        .transpose()?,
                }
            } else {
                FieldState::default()
            };
            fields.push(Field {
                id: id.clone(),
                farm: parent,
                selected_crop: selected,
                state,
            });
        }
        Self::new(town, ledger, fields, assignments)
    }
}
pub(super) fn work_destination(
    town: &Town,
    ledger: &Ledger,
    field: &Field,
) -> Result<Destination, Error> {
    let t = town.building(&field.id)?.location;
    let (mut x, mut y) = (f64::from(t.x), f64::from(t.y));
    if ledger.store(&field.id)?.policy.kind == "acre"
        && matches!(
            field.state.stage,
            Stage::Unplowed | Stage::Plowed | Stage::Thirsty
        )
    {
        if t.x > 0 {
            x -= 0.45;
            y += 0.1;
        } else if t.y > 0 {
            x += 0.1;
            y -= 0.45;
        } else {
            x += 0.45;
            y += 0.1;
        }
    }
    Ok(Destination::Position(
        Position::new(x, y).map_err(|_| Error::Invalid)?,
    ))
}

impl Fields {
    /// Import legacy per-worker cans separately from movement/cargo admission.
    /// Old waterReady animations are upgraded only with a matching field cycle.
    /// Actor IDs correspond to the original legacy roster, never current ordering.
    pub fn admit_legacy_rounds(
        &mut self,
        town: &Town,
        ledger: &Ledger,
        workers: &[serde_json::Value],
        actors: &[ActorId],
    ) -> Result<(), Error> {
        if workers.len() != actors.len() {
            return Err(Error::Invalid);
        }
        let mut owner = self.clone();
        let mut seen = BTreeSet::new();
        let resolve = |kind: &str, x: i64, y: i64| -> Result<BuildingId, Error> {
            let ids: Vec<_> = town
                .buildings()
                .iter()
                .filter(|b| {
                    i64::from(b.location.x) == x
                        && i64::from(b.location.y) == y
                        && ledger.store(&b.id).is_ok_and(|s| s.policy.kind == kind)
                })
                .map(|b| b.id.clone())
                .collect();
            if ids.len() != 1 {
                return Err(Error::Invalid);
            }
            Ok(ids[0].clone())
        };
        for (raw, actor) in workers.iter().zip(actors) {
            town.actor(actor)?;
            if !seen.insert(actor.clone()) {
                return Err(Error::Invalid);
            }
            let round = if let Some(round) = raw.get("watering_round") {
                let r = round.as_array().ok_or(Error::Invalid)?;
                if r.len() != 4 {
                    return Err(Error::Invalid);
                }
                Some((
                    resolve(
                        r[0].as_str().ok_or(Error::Invalid)?,
                        r[1].as_i64().ok_or(Error::Invalid)?,
                        r[2].as_i64().ok_or(Error::Invalid)?,
                    )?,
                    r[3].as_u64().ok_or(Error::Invalid)?,
                ))
            } else if raw["work_animation"]["waterReady"] == true
                && raw["work_animation"]["task"][0] == "farming"
            {
                let task = raw["work_animation"]["task"]
                    .as_array()
                    .ok_or(Error::Invalid)?;
                if task.len() < 9 || task[task.len() - 2] != "field_cycle" {
                    return Err(Error::Invalid);
                }
                let cycle = task[task.len() - 1].as_u64().ok_or(Error::Invalid)?;
                let workplace = resolve(
                    task[1].as_str().ok_or(Error::Invalid)?,
                    task[2].as_i64().ok_or(Error::Invalid)?,
                    task[3].as_i64().ok_or(Error::Invalid)?,
                )?;
                let field = resolve(
                    task[4].as_str().ok_or(Error::Invalid)?,
                    task[5].as_i64().ok_or(Error::Invalid)?,
                    task[6].as_i64().ok_or(Error::Invalid)?,
                )?;
                (owner.field(&field)?.state.cycle == cycle).then_some((workplace, cycle))
            } else {
                None
            };
            if let Some((workplace, cycle)) = round {
                owner.field(&workplace)?;
                owner.rounds.insert(actor.clone(), (workplace, cycle));
            }
        }
        *self = owner;
        Ok(())
    }
}
