"""Saved, host-owned field progress; growth does not require a worker."""

GROWTH_SECONDS = 15
WORK_SECONDS = 2
# Two growth periods replace ten of the old three-second production cycles.
HARVEST_YIELD = 2 * GROWTH_SECONDS / 3
ACTIONS = {'unplowed': 'plowing', 'plowed': 'sowing',
           'thirsty': 'watering', 'ripe': 'harvesting'}


def field_state(field):
    return field.setdefault('field', {'stage': 'unplowed', 'elapsed': 0, 'progress': 0, 'cycle': 0})


def watering_round(farm, field):
    """One filled can per farmer, farm, and crop pass; serializable in saves."""
    return [farm['kind'], farm['x'], farm['y'], field_state(field)['cycle']]


def acres(town, farm):
    owner = next(i for i, b in enumerate(town['buildings']) if b is farm)
    return [b for b in town['buildings'] if b['kind'] == 'acre' and b.get('farm_index') == owner]


def work_pass(town, farm):
    fields = [a for a in acres(town, farm) if 'construction' not in a] or [farm]
    cycle = min(field_state(a)['cycle'] for a in fields)
    stages = {field_state(a)['stage'] for a in fields if field_state(a)['cycle'] == cycle}
    stage = ('unplowed' if 'unplowed' in stages else 'plowed' if 'plowed' in stages else
             'thirsty' if stages & {'sprouting', 'thirsty'} else 'ripe')
    return cycle, stage


def can_sow(town, farm, cycle=None):
    if cycle is None:
        cycle = work_pass(town, farm)[0]
    # Already harvested acres belong to the next pass, and cannot block this one.
    return all(field_state(a)['stage'] != 'unplowed' or field_state(a)['cycle'] > cycle
               for a in acres(town, farm))


def grow(town, seconds):
    for field in town['buildings']:
        if field['kind'] not in ('acre', 'farm') or 'construction' in field:
            continue
        state = field_state(field)
        if state['stage'] in ('sprouting', 'growing'):
            state['elapsed'] = min(GROWTH_SECONDS, state['elapsed'] + seconds)
            if state['elapsed'] >= GROWTH_SECONDS:
                state.update(stage='thirsty' if state['stage'] == 'sprouting' else 'ripe',
                             elapsed=0, progress=0)


def claimed(town, field, index, reservations=None):
    """An acre has one active tender, even with upgraded worker capacity."""
    if any(owner != index and target is field for owner, target in (reservations or {}).items()):
        return True
    field_index = next((i for i, b in enumerate(town['buildings']) if b is field), None)
    for other_index, worker in enumerate(town['villagers']):
        if other_index == index or worker['job'] != 'farming' or worker.get('construction_target'):
            continue
        if field['kind'] == 'acre' and worker.get('building_index') == field_index:
            if town['villagers'][index].get('building_index') != field_index or other_index < index:
                return True
        animation = worker.get('work_animation', {})
        task = animation.get('task', [])
        if (field['kind'] == 'acre' and town['villagers'][index].get('building_index') == field_index
                and worker.get('building_index') != field_index):
            continue
        if (len(task) >= 7 and task[4:7] == [field['kind'], field['x'], field['y']]
                and (animation.get('phase') in ('walking', 'working', 'fetching_water',
                                               'collecting_water', 'returning_water', 'harvest_ready')
                     or (field_state(field)['stage'] == 'thirsty' and animation.get('waterReady')))):
            return True
    return False


def choose_field(town, farm, fields, index, reservations=None):
    worker = town['villagers'][index]
    animation = worker.get('work_animation', {})
    task = animation.get('task', [])
    previous = next((a for a in fields if len(task) >= 7
                     and task[:4] == ['farming', farm['kind'], farm['x'], farm['y']]
                     and task[4:7] == [a['kind'], a['x'], a['y']]), None)
    # Finish a return trip before accepting any newly available acres.
    returning = animation.get('phase') in ('returning', 'depositing') or animation.get('resume_phase') == 'returning'
    if worker.get('pending_output') and previous is not None and returning:
        return previous
    if previous is not None and field_state(previous)['stage'] == 'thirsty' and (
            animation.get('waterReady') or animation.get('phase') in
            ('fetching_water', 'collecting_water', 'returning_water')):
        return previous
    cycle, stage = work_pass(town, farm)
    ready = [a for a in fields if field_state(a)['cycle'] == cycle and field_state(a)['stage'] == stage
             and (stage != 'plowed' or can_sow(town, farm, cycle))
             and not claimed(town, a, index, reservations)
             and (not worker.get('pending_output') or harvest_room(town, farm, a) > 1e-9)]
    if previous in ready and (animation.get('phase') in ('walking', 'working', 'fetching_water',
                                                        'collecting_water', 'returning_water', 'harvest_ready')
                              or (field_state(previous)['stage'] == 'thirsty' and animation.get('waterReady'))):
        return previous
    if ready:
        position = animation or worker.get('logistics_position') or worker.get('construction_position') or farm
        rows = {}
        for field in ready:
            rows.setdefault(field['y'], []).append(field)
        # Keep workers on their row; other workers prefer separate rows when available.
        occupied_rows = {v['work_animation']['task'][6] for i, v in enumerate(town['villagers'])
                         if i != index and v['job'] == 'farming' and v.get('building_index') == worker.get('building_index')
                         and len(v.get('work_animation', {}).get('task', [])) >= 7
                         and v['work_animation']['task'][:4] == ['farming', farm['kind'], farm['x'], farm['y']]}
        occupied_rows.update(target['y'] for owner, target in (reservations or {}).items()
                             if owner != index and town['villagers'][owner].get('building_index') == worker.get('building_index'))
        available_rows = [y for y in rows if y not in occupied_rows] or list(rows)
        row = previous['y'] if previous is not None and previous['y'] in available_rows else min(
            available_rows, key=lambda y: (abs(y - position['y']), y))
        ends = [min(rows[row], key=lambda a: a['x']), max(rows[row], key=lambda a: a['x'])]
        return min(ends, key=lambda a: (abs(a['x'] - position['x']), a['x']))
    return previous if previous is not None else fields[index % len(fields)]


def harvest_room(town, farm, field):
    # Progression also uses field state, so import after module initialization.
    import progression as economy
    state = field_state(field)
    if state['stage'] != 'ripe':
        return 0
    crop = state.get('crop', field.get('crop', 'vegetables'))
    return min(economy.local_room(farm, crop), economy.production_room(town, crop))


def complete_action(field):
    state = field_state(field)
    stage = state['stage']
    if stage == 'unplowed':
        state.update(stage='plowed', progress=0)
    elif stage == 'plowed':
        state.update(stage='sprouting', crop=field.get('crop', 'vegetables'), elapsed=0, progress=0)
    elif stage == 'thirsty':
        state.update(stage='growing', elapsed=0, progress=0)


def status(field, animation):
    phase = animation.get('phase')
    if phase in ('fetching_water', 'collecting_water', 'returning_water'):
        return {'fetching_water': 'Walking to water', 'collecting_water': 'Collecting water',
                'returning_water': 'Carrying water to acre'}[phase]
    if phase == 'waiting_water':
        return 'Waiting for reachable water'
    if phase == 'waiting_plowing':
        return 'Waiting for all farm acres to be plowed'
    if phase == 'waiting_field':
        return 'Acre already being tended'
    if phase == 'waiting_pass':
        return 'Waiting for the current farm pass to finish'
    if phase in ('returning', 'depositing'):
        return 'Returning harvested crops'
    state = field_state(field)
    if state['stage'] in ('sprouting', 'growing'):
        return f"{state['stage'].capitalize()} · {GROWTH_SECONDS - state['elapsed']:g}s remaining"
    if phase == 'walking':
        return 'Walking to acre'
    return ACTIONS.get(state['stage'], 'Waiting for acre').capitalize()
