"""Saved, host-owned field progress; growth does not require a worker."""

GROWTH_SECONDS = 15
WORK_SECONDS = 2
ACTIONS = {'unplowed': 'plowing', 'plowed': 'sowing',
           'thirsty': 'watering', 'ripe': 'harvesting'}


def field_state(field):
    return field.setdefault('field', {'stage': 'unplowed', 'elapsed': 0, 'progress': 0, 'cycle': 0})


def acres(town, farm):
    owner = next(i for i, b in enumerate(town['buildings']) if b is farm)
    return [b for b in town['buildings'] if b['kind'] == 'acre' and b.get('farm_index') == owner]


def can_sow(town, farm):
    return all(field_state(a)['stage'] != 'unplowed' for a in acres(town, farm))


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


def claimed(town, field, index):
    """An acre has one active tender, even with upgraded worker capacity."""
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


def choose_field(town, farm, fields, index):
    worker = town['villagers'][index]
    animation = worker.get('work_animation', {})
    task = animation.get('task', [])
    previous = next((a for a in fields if len(task) >= 7
                     and task[:4] == ['farming', farm['kind'], farm['x'], farm['y']]
                     and task[4:7] == [a['kind'], a['x'], a['y']]), None)
    # Harvest cargo must return to its farm before this worker changes fields.
    if worker.get('pending_output') and previous is not None:
        return previous
    ready = [a for a in fields if field_state(a)['stage'] in ACTIONS
             and (field_state(a)['stage'] != 'plowed' or can_sow(town, farm))
             and not claimed(town, a, index)]
    if previous in ready and (animation.get('phase') in ('walking', 'working', 'fetching_water',
                                                        'collecting_water', 'returning_water', 'harvest_ready')
                              or (field_state(previous)['stage'] == 'thirsty' and animation.get('waterReady'))):
        return previous
    if ready:
        priorities = {'unplowed': 0, 'thirsty': 1, 'ripe': 2, 'plowed': 3}
        return min(ready, key=lambda a: (priorities[field_state(a)['stage']],
                                        abs(a['x'] - farm['x']) + abs(a['y'] - farm['y'])))
    return previous if previous is not None else fields[index % len(fields)]


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
    if phase in ('returning', 'depositing'):
        return 'Returning harvested crops'
    state = field_state(field)
    if state['stage'] in ('sprouting', 'growing'):
        return f"{state['stage'].capitalize()} · {GROWTH_SECONDS - state['elapsed']:g}s remaining"
    if phase == 'walking':
        return 'Walking to acre'
    return ACTIONS.get(state['stage'], 'Waiting for acre').capitalize()
