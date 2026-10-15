"""Persistent fruit trees and automatic orchard harvests, owned by the host."""

STAGE_SECONDS = {'sapling': 60, 'blossom': 30, 'fruit': 60, 'harvested': 30}
NEXT_STAGE = {'sapling': 'blossom', 'blossom': 'fruit', 'fruit': 'ripe', 'harvested': 'blossom'}
HARVEST_YIELD = 8
PICK_SECONDS = 3.2
REACH = 3


def tree_state(tree):
    return tree.setdefault('tree', {'stage': 'sapling', 'elapsed': 0, 'fruit': HARVEST_YIELD})


def trees(town, orchard):
    owner = next(i for i, b in enumerate(town['buildings']) if b is orchard)
    return [b for b in town['buildings'] if b['kind'] == 'orchard_tree'
            and b.get('orchard_index') == owner]


def grow(town, seconds):
    for tree in town['buildings']:
        if tree['kind'] != 'orchard_tree' or 'construction' in tree:
            continue
        state = tree_state(tree)
        remaining = seconds
        while state['stage'] in STAGE_SECONDS and remaining > 0:
            duration = STAGE_SECONDS[state['stage']]
            take = min(remaining, duration - state['elapsed'])
            state['elapsed'] += take
            remaining -= take
            if state['elapsed'] >= duration:
                state.update(stage=NEXT_STAGE[state['stage']], elapsed=0)
                if state['stage'] == 'ripe':
                    state['fruit'] = HARVEST_YIELD


def choose_tree(town, orchard, index):
    worker = town['villagers'][index]
    animation = worker.get('work_animation', {})
    task = animation.get('task', [])
    previous = next((b for b in trees(town, orchard) if task[4:7] == [b['kind'], b['x'], b['y']]), None)
    # Keep the delivery leg even after the picked tree is removed.
    if worker.get('pending_output'):
        return previous or (dict(zip(('kind', 'x', 'y'), task[4:7])) if len(task) >= 7 else orchard)
    claimed = {tuple(v['work_animation']['task'][4:7]) for i, v in enumerate(town['villagers'])
               if i != index and v['job'] == 'orcharding' and not v.get('construction_target')
               and v.get('work_animation', {}).get('phase') in ('walking', 'working', 'waiting')
               and len(v['work_animation']['task']) >= 7}
    ready = [b for b in trees(town, orchard) if tree_state(b)['stage'] == 'ripe'
             and (b['kind'], b['x'], b['y']) not in claimed]
    if previous in ready:
        return previous
    position = animation or worker.get('logistics_position') or orchard
    return min(ready, key=lambda b: abs(b['x'] - position['x']) + abs(b['y'] - position['y'])) if ready else orchard


def harvest(town, orchard, tree, worker):
    import logistics
    import progression as economy
    state = tree_state(tree)
    amount = min(state['fruit'], economy.local_room(orchard, 'fruit'), economy.production_room(town, 'fruit'))
    if state['stage'] != 'ripe' or amount <= 1e-9:
        return False
    logistics.record_output(town, orchard, 'fruit', amount, worker)
    state['fruit'] = max(0, state['fruit'] - amount)
    if state['fruit'] <= 1e-9:
        state.update(stage='harvested', elapsed=0)
    return True


def advance(town, landscape, worker, orchard, source, index, seconds):
    import logistics
    import progression as economy
    import work_cycles
    if worker.get('work_animation', {}).get('phase') == 'idle':
        animation = worker.pop('work_animation')
        worker['logistics_position'] = {k: animation[k] for k in ('x', 'y')}
    if not worker.get('pending_output'):
        reason = ('Plant fruit trees around this orchard' if not trees(town, orchard) else
                  'Waiting for ripe fruit' if source is orchard else
                  'Production cap reached' if economy.production_room(town, 'fruit') <= 1e-9 else
                  'Workplace buffer full' if economy.local_room(orchard, 'fruit') <= 1e-9 else None)
        if reason:
            animation = worker.pop('work_animation', None)
            if animation:
                worker['logistics_position'] = {k: animation[k] for k in ('x', 'y')}
            worker['production_status'] = reason
            return
    if work_cycles.advance(town, landscape, worker, orchard, source, index, seconds):
        logistics.drop_off(town, orchard, worker)
    animation = worker.get('work_animation', {})
    worker['production_status'] = {'walking': 'Walking to fruit tree', 'working': 'Picking fruit',
        'returning': 'Carrying fruit to orchard', 'depositing': 'Unloading fruit',
        'waiting': 'Waiting for a route', 'waiting_storage': 'Waiting for storage space',
        'idle': 'Harvest delivered'}.get(animation.get('phase'), 'Waiting for ripe fruit')
