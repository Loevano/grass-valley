"""Building inventories and host-controlled transport between workplaces and storage."""
import math

import progression as economy

BUFFER_SIZE = 50
PRODUCTION_KINDS = {'woodcutter', 'farm', 'fishing_hut', 'barn', 'stable', 'orchard', *economy.RECIPES}
PRODUCTION_GOODS = tuple(dict.fromkeys(('wood', *economy.CROPS, *economy.EDIBLE,
                                       *(r[1] for r in economy.RECIPES.values()), 'wool', 'manure', 'herbs', *economy.CRAFTED_GOODS, *economy.LATE_GOODS)))


def configure_rules(town, data):
    key, cap = data.get('resource'), data.get('cap')
    if key not in PRODUCTION_GOODS:
        raise ValueError('Choose a production resource.')
    if cap is not None and (type(cap) not in (int, float) or not math.isfinite(cap) or cap < 0):
        raise ValueError('Use a non-negative production cap, or leave it blank for unlimited.')
    caps = town.setdefault('rules', {}).setdefault('production_caps', {})
    if cap is None:
        caps.pop(key, None)
    else:
        caps[key] = cap
    return {}


def carry_capacity(villager):
    return BUFFER_SIZE * economy.efficiency(villager, 'hauling')


def travel_speed(villager):
    return 1.8 * (1 + .25 * (villager.get('hauling', 0) // 20))


def buffered(building):
    return sum(economy.inventory(building).values()) + sum(building.get('pending_output', {}).values())


def production_stock(building, key):
    return max(0, economy.inventory(building).get(key, 0))


def record_output(town, building, key, amount, villager=None, orphan_room=None):
    economy.ensure_inventory(town)
    buffer = building.setdefault('pending_output' if villager is not None else 'output_buffer', {})
    buffer[key] = buffer.get(key, 0) + amount
    if villager is None:
        economy.add_stock(town, key, amount)
    else:
        pending = villager.setdefault('pending_output', {})
        orphaned = building.get('orphaned_output', {})
        for orphan_key, orphan_amount in list(orphaned.items()):
            take = orphan_amount if orphan_room is None else min(orphan_amount, max(0, orphan_room))
            pending[orphan_key] = pending.get(orphan_key, 0) + take
            if orphan_room is not None:
                orphan_room -= take
            if take >= orphan_amount:
                del orphaned[orphan_key]
            else:
                orphaned[orphan_key] -= take
        pending[key] = pending.get(key, 0) + amount
        villager['pending_workplace'] = {k: building[k] for k in ('kind', 'x', 'y')}
    totals = town.setdefault('produced', {})
    totals[key] = totals.get(key, 0) + amount
    if key == 'wood':
        town['harvested'] += amount


def drop_off(town, building, villager):
    """Make only this worker's completed delivery available to the town."""
    pending = building.get('pending_output', {})
    for key, amount in villager.pop('pending_output', {}).items():
        amount = min(amount, pending.get(key, 0))
        pending[key] = pending.get(key, 0) - amount
        buffer = building.setdefault('output_buffer', {})
        buffer[key] = buffer.get(key, 0) + amount
        economy.add_stock(town, key, amount)
    villager.pop('pending_workplace', None)


def cancel_output(town, villager):
    villager.pop('production_work', None)
    spec = villager.pop('pending_workplace', None)
    goods = villager.pop('pending_output', {})
    building = next((b for b in town['buildings'] if spec and all(b.get(k) == v for k, v in spec.items())), None)
    if building:
        orphaned = building.setdefault('orphaned_output', {})
        for key, amount in goods.items():
            orphaned[key] = orphaned.get(key, 0) + amount
    villager.pop('work_animation', None)


def collection_inventory(building):
    result = economy.inventory(building)
    for key, amount in building.get('orphaned_output', {}).items():
        result[key] = result.get(key, 0) + amount
    return result


def requirements(building, villager):
    kind = building['kind']
    if kind == 'butcher' and building.get('butchering'):
        return {}
    if kind in economy.RECIPES:
        if economy.recipe_requirement(villager, building):
            return {}
        speed = economy.efficiency(villager, villager['job'])
        speed *= 1.25 if any(k in building.get('profession_upgrades', []) for k in ('saw', 'steel_pickaxe')) else 1
        return {key: amount * speed for key, amount in economy.production_recipe(building)[0].items()}
    if kind in {'barn', 'stable'}:
        feed = sum(economy.ANIMALS[a]['feed'] * n for a, n in building.get('animals', {}).items())
        if not feed:
            return {}
        inputs = building.get('input_buffer', {})
        return {'fodder' if inputs.get('fodder', 0) >= feed else 'vegetables': feed}
    return {}


def supply_request(town, building, villager):
    needs = requirements(building, villager)
    inputs = building.get('input_buffer', {})
    if all(inputs.get(k, 0) + 1e-9 >= n for k, n in needs.items()):
        return {}
    if building['kind'] in {'barn', 'stable'}:
        feed = next(iter(needs.values()), 0)
        needs = {'fodder' if economy.quantity(town, 'fodder') >= feed else 'vegetables': feed}
    # Reserve each ingredient's share so separate storage workers can supply a recipe.
    batches = BUFFER_SIZE / sum(needs.values()) if needs else 0
    room = max(0, BUFFER_SIZE - sum(inputs.values()))
    request = {}
    for key, amount in needs.items():
        available = sum(production_stock(b, key) for b in town['buildings']
                        if b is not building and 'construction' not in b)
        take = min(room, max(0, amount * batches - inputs.get(key, 0)), available)
        if take > 1e-9:
            request[key] = take
            room -= take
    return request


def produce(town, building, villager, elapsed=3):
    """Reuse profession recipes against local inputs; outputs never enter town stock."""
    reason = economy.qualification(villager, villager['job'])
    if building['kind'] in economy.RECIPES:
        reason = reason or economy.recipe_requirement(villager, building)
    if reason:
        return reason
    deposit = None
    if building['kind'] == 'mine':
        deposits = [r for r in town['map_resources'] if r['kind'] == 'rock' and r.get('ore') == 'iron'
                    and economy.distance(building, r) <= 10]
        if not deposits:
            return 'Needs an iron vein within 10 tiles'
        deposit = max(deposits, key=lambda r: r.get('supply', 60))
    inputs = building.get('input_buffer', {})
    needs = requirements(building, villager)
    missing = [k for k, n in needs.items() if inputs.get(k, 0) + 1e-9 < n]
    if missing:
        return 'Waiting for delivery: ' + ', '.join(missing)
    local = {'wood': inputs.get('wood', 0), 'food': sum(n * economy.EDIBLE.get(k, 0) for k, n in inputs.items()),
             'goods': {k: n for k, n in inputs.items() if k != 'wood'}, 'harvested': 0,
             '_local_production': True}
    worker = dict(villager)
    status = economy.extra_worker(local, worker, building, BUFFER_SIZE)
    produced = local.get('produced', {})
    if buffered(building) + sum(produced.values()) > BUFFER_SIZE + 1e-9:
        building['buffer_full'] = True
        return 'Workplace buffer full'
    if deposit and deposit.get('supply', 60) + 1e-9 < produced.get('iron_ore', 0):
        return 'Waiting for ore to regenerate'
    if status != 'Working':
        return status
    if any(amount > economy.production_room(town, key) + 1e-9 for key, amount in produced.items()):
        return 'Production cap reached'
    if building['kind'] in economy.RECIPES:
        output = economy.production_recipe(building)[1]
        task = [villager['job'], building['kind'], building['x'], building['y'], output]
        progress = villager.get('production_work', {})
        if progress.get('task') != task:
            progress = {'task': task, 'elapsed': 0}
        duration = economy.production_seconds(building)
        progress['elapsed'] = min(duration, progress['elapsed'] + elapsed)
        villager['production_work'] = progress
        if progress['elapsed'] < duration:
            return f"Producing {output.replace('_', ' ')} · {math.ceil(duration - progress['elapsed'])}s remaining"
        villager.pop('production_work', None)
    if deposit:
        economy.take_supply(town, deposit, produced['iron_ore'])
    building['input_buffer'] = {k: max(0, economy.quantity(local, k) - produced.get(k, 0)) for k in inputs}
    for key, amount in produced.items():
        record_output(town, building, key, amount, villager)
    villager[villager['job']] = worker[villager['job']]
    return status


def butcher_animal(town, building, villager, seconds):
    """One shared processing job; completion enters the normal hauling buffer."""
    task = building['butchering']
    if task['remaining'] > 0:
        state = villager.get('work_animation', {})
        if state.get('phase') != 'working':
            return 'Waiting for butcher at workplace'
        task['remaining'] = max(0, task['remaining'] - seconds)
    if task['remaining'] > 0:
        return f"Butchering {task['animal']} · {math.ceil(task['remaining'])}s remaining"
    amount = task['amount']
    if amount > economy.production_room(town, 'meat') + 1e-9:
        return 'Production cap reached'
    if buffered(building) + amount > BUFFER_SIZE + 1e-9:
        building['buffer_full'] = True
        return 'Workplace buffer full'
    hides = {'sheep': 1, 'pig': 2, 'cow': 3}.get(task['animal'], 0)
    if hides and (buffered(building) + amount + hides > BUFFER_SIZE + 1e-9
                  or hides > economy.production_room(town, 'hides') + 1e-9):
        return 'Waiting for hide buffer space or production cap'
    record_output(town, building, 'meat', amount)
    if hides:
        record_output(town, building, 'hides', hides)
    economy.practice(villager, 'butchery')
    del building['butchering']
    return 'Working'


def storage_for(town, keys, operation):
    return [b for b in town['buildings'] if 'construction' not in b
            and any((economy.local_room(b, k) if operation == 'collect' else production_stock(b, k)) > 0
                    for k in keys)]


def planned_cargo(town, building, storage, operation, villager):
    """Describe the batch this trip can deliver without reserving town stock."""
    remaining = carry_capacity(villager)
    cargo = {}
    if operation == 'collect':
        target = {**storage, 'stored_goods': dict(storage.get('stored_goods', {}))}
        for field in ('output_buffer', 'stored_goods', 'orphaned_output'):
            for key, amount in building.get(field, {}).items():
                moved = min(amount, economy.local_room(target, key), remaining)
                if moved > 0:
                    cargo[key] = cargo.get(key, 0) + moved
                    target['stored_goods'][key] = target['stored_goods'].get(key, 0) + moved
                    remaining -= moved
    else:
        producer = next((v for v in town['villagers'] if v.get('building_index') is not None
                         and town['buildings'][v['building_index']] is building
                         and v['job'] in economy.JOB_BUILDINGS), villager)
        for key, amount in supply_request(town, building, producer).items():
            moved = min(amount, production_stock(storage, key), remaining)
            if moved > 0:
                cargo[key] = moved
                remaining -= moved
    return cargo


def start_trip(town, landscape, villager, building, storage, operation, route, hauler=False):
    # Finished fields are walkable, including when carrying goods through a farm.
    landscape = {**landscape, 'buildings': [b for b in landscape['buildings']
                 if b['kind'] != 'acre' or 'construction' in b]}
    position = villager.get('logistics_position', villager.get('construction_position', storage if hauler else building))
    first, last = (building, storage) if hauler else (storage, building)
    outward = route(landscape, (round(position['x']), round(position['y'])), first)
    if outward is None:
        return False
    back = route(landscape, outward[-1], last)
    if back is None:
        return False
    villager['logistics_trip_id'] = villager.get('logistics_trip_id', 0) + 1
    villager['logistics_trip'] = {'id': villager['logistics_trip_id'], 'operation': operation, 'workplace': {k: building[k] for k in ('kind', 'x', 'y')},
                                 'storage': {k: storage[k] for k in ('kind', 'x', 'y')},
                                 'path': [list(p) for p in outward + back[1:]],
                                 'pickup_step': len(outward) - 1 if (operation == 'collect' and hauler)
                                 or (operation == 'supply' and not hauler) else 0,
                                 'cargo': planned_cargo(town, building, storage, operation, villager),
                                 'delivery_step': len(outward) - 1 if (operation == 'collect' and not hauler)
                                 or (operation == 'supply' and hauler) else len(outward + back[1:]) - 1,
                                 'delivered': False}
    villager['logistics_position'] = {'x': outward[0][0], 'y': outward[0][1]}
    return True


def advance_trip(town, landscape, villager, elapsed, resume_work=False):
    trip = villager.get('logistics_trip')
    if not trip:
        return False
    find = lambda spec: next((b for b in town['buildings'] if all(b.get(k) == v for k, v in spec.items())
                             and 'construction' not in b), None)
    building, storage = find(trip['workplace']), find(trip['storage'])
    if not building or not storage:
        villager.pop('logistics_trip', None)
        villager.pop('logistics_target', None)
        villager['production_status'] = 'Delivery cancelled: building moved or unavailable'
        return True
    if 'pickup_step' not in trip:
        pickup = building if trip['operation'] == 'collect' and villager.get('job') == 'hauling' else storage
        fetching = (trip['operation'] == 'collect') == (villager.get('job') == 'hauling')
        trip['pickup_step'] = next((i for i, (x, y) in enumerate(trip['path'])
                                   if abs(x - pickup['x']) + abs(y - pickup['y']) == 1), 0) if fetching else 0
    if 'cargo' not in trip:
        trip['cargo'] = planned_cargo(town, building, storage, trip['operation'], villager)
    obstacles = [b for b in landscape['buildings'] if b['kind'] != 'acre' or 'construction' in b]
    spatial = landscape.get('_spatial_index')
    blocked = (spatial.blocked(dict(landscape, buildings=obstacles)) if spatial is not None else
               {(b['x'], b['y']) for b in obstacles + landscape['map_resources'] + landscape.get('npcs', [])})
    terrain = landscape.get('terrain', [])
    roads = {(r['x'], r['y']) for r in landscape.get('roads', []) if 'construction' not in r}
    path = trip['path']
    step = trip.get('step', 0)
    remaining = elapsed + trip.get('travel_seconds', 0)
    while step < len(path) - 1:
        tile = tuple(path[step + 1])
        if tile in blocked or (terrain and terrain[tile[1]][tile[0]] == 'v'):
            villager.pop('logistics_trip', None)
            villager.pop('logistics_target', None)
            villager['production_status'] = 'Delivery blocked; finding another route'
            return True
        duration = 1 / (travel_speed(villager) * (1.5 if tile in roads else 1))
        if remaining < duration:
            break
        remaining -= duration
        step += 1
        if step == trip.get('pickup_step') and not trip['delivered']:
            trip['cargo'] = planned_cargo(town, building, storage, trip['operation'], villager)
        if step >= trip['delivery_step'] and not trip['delivered']:
            trip['cargo'] = transfer(town, building, storage, trip['operation'], villager)
            trip['delivered'] = True
    # Already adjacent trips still perform their transfer once.
    if step >= trip['delivery_step'] and not trip['delivered']:
        trip['cargo'] = transfer(town, building, storage, trip['operation'], villager)
        trip['delivered'] = True
    trip.update(step=step, travel_seconds=remaining)
    villager['logistics_position'] = {'x': path[step][0], 'y': path[step][1]}
    villager['logistics_target'] = {'kind': 'delivery', 'x': path[min(step + 1, len(path) - 1)][0],
                                    'y': path[min(step + 1, len(path) - 1)][1]}
    villager['production_status'] = 'Hauling outputs' if trip['operation'] == 'collect' else 'Delivering supplies'
    if step == len(path) - 1:
        villager.pop('logistics_trip', None)
        villager.pop('logistics_target', None)
        return not resume_work
    return True


def transfer(town, building, storage, operation, villager):
    economy.ensure_inventory(town)
    remaining = carry_capacity(villager)
    total_moved = 0
    cargo = {}
    if operation == 'collect':
        for field in ('output_buffer', 'stored_goods', 'orphaned_output'):
            buffer = building.get(field, {})
            for key, amount in list(buffer.items()):
                moved = min(amount, economy.local_room(storage, key), remaining)
                if moved > 0:
                    target = storage.setdefault('stored_goods', {})
                    target[key] = target.get(key, 0) + moved
                    buffer[key] -= moved
                    if field == 'orphaned_output':
                        building['pending_output'][key] -= moved
                        economy.add_stock(town, key, moved)
                    remaining -= moved
                    total_moved += moved
                    cargo[key] = cargo.get(key, 0) + moved
                    building.pop('buffer_full', None)
    else:
        # The assigned producer's practice determines the recipe, not the hauler's.
        producer = next((v for v in town['villagers'] if v.get('building_index') is not None
                         and town['buildings'][v['building_index']] is building
                         and v['job'] in economy.JOB_BUILDINGS), villager)
        request = supply_request(town, building, producer)
        request = {k: min(n, production_stock(storage, k)) for k, n in request.items()
                   if production_stock(storage, k) > 0}
        # Share one carrying limit across all ingredients in the batch.
        for key, amount in request.items():
            request[key] = min(amount, remaining)
            remaining -= request[key]
            total_moved += request[key]
        if request:
            for key, amount in request.items():
                economy.withdraw(town, key, amount, source=storage)
                economy.add_stock(town, key, -amount)
            inputs = building.setdefault('input_buffer', {})
            for key, amount in request.items():
                inputs[key] = inputs.get(key, 0) + amount
            cargo = {key: amount for key, amount in request.items() if amount > 0}
    if total_moved > 0:
        economy.practice(villager, 'hauling')
    return cargo


def worker(town, landscape, villager, building, route, elapsed):
    if villager.get('logistics_trip'):
        # Use room freed by delivery before another producer refills the buffer.
        return advance_trip(town, landscape, villager, elapsed, resume_work=True)
    collect = buffered(building) >= BUFFER_SIZE - 1e-9 or building.get('buffer_full', False)
    request = supply_request(town, building, villager) if not collect else {}
    if not collect and not request:
        return False
    keys = collection_inventory(building) if collect else request
    stores = [b for b in storage_for(town, keys, 'collect' if collect else 'supply') if b is not building]
    stores.sort(key=lambda b: abs(b['x'] - building['x']) + abs(b['y'] - building['y']))
    for storage in stores:
        if collect and not any(economy.local_room(storage, k) > 0 for k, n in keys.items() if n):
            continue
        if start_trip(town, landscape, villager, building, storage, 'collect' if collect else 'supply', route):
            return advance_trip(town, landscape, villager, elapsed, resume_work=True)
    if collect:
        villager['production_status'] = 'Workplace buffer full: storage full, disabled, or unreachable'
        return True
    villager['production_status'] = 'Waiting for reachable storage with supplies'
    return True


def haul(town, landscape, villager, storage, route, elapsed):
    if advance_trip(town, landscape, villager, elapsed):
        return
    candidates = []
    for i, b in enumerate(town['buildings']):
        if b is storage or b['kind'] == 'storehouse' or 'construction' in b:
            continue
        producers = [v for v in town['villagers'] if v.get('building_index') == i
                     and v['job'] in economy.JOB_BUILDINGS]
        for producer in producers:
            request = supply_request(town, b, producer)
            if request and storage in storage_for(town, request, 'supply'):
                candidates.append((0, b, 'supply'))
                break
        outputs = {k: n for k, n in collection_inventory(b).items() if n > 0
                   and economy.local_room(storage, k) > 0}
        if outputs and storage in storage_for(town, outputs, 'collect'):
            candidates.append((1, b, 'collect'))
    candidates.sort(key=lambda c: (c[0], -buffered(c[1]), abs(storage['x'] - c[1]['x']) + abs(storage['y'] - c[1]['y'])))
    for _, building, operation in candidates:
        if start_trip(town, landscape, villager, building, storage, operation, route, hauler=True):
            advance_trip(town, landscape, villager, elapsed)
            return
    villager['production_status'] = 'Waiting for goods or enabled storage resources'
