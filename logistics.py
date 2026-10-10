"""Workplace buffers and host-controlled transport to shared town storage."""
import progression as economy

BUFFER_SIZE = 50
PRODUCTION_KINDS = {'woodcutter', 'farm', 'fishing_hut', 'barn', 'orchard', *economy.RECIPES}


def buffered(building):
    return sum(building.get('output_buffer', {}).values())


def record_output(town, building, key, amount):
    buffer = building.setdefault('output_buffer', {})
    buffer[key] = buffer.get(key, 0) + amount
    totals = town.setdefault('produced', {})
    totals[key] = totals.get(key, 0) + amount
    if key == 'wood':
        town['harvested'] += amount


def requirements(building, villager):
    kind = building['kind']
    if kind in economy.RECIPES:
        speed = economy.efficiency(villager, villager['job'])
        speed *= 1.25 if 'saw' in building.get('profession_upgrades', []) else 1
        return {key: amount * speed for key, amount in economy.RECIPES[kind][0].items()}
    if kind == 'barn':
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
    if building['kind'] == 'barn':
        feed = next(iter(needs.values()), 0)
        needs = {'fodder' if economy.quantity(town, 'fodder') >= feed else 'vegetables': feed}
    # Reserve each ingredient's share so separate storage workers can supply a recipe.
    batches = BUFFER_SIZE / sum(needs.values()) if needs else 0
    room = max(0, BUFFER_SIZE - sum(inputs.values()))
    request = {}
    for key, amount in needs.items():
        take = min(room, max(0, amount * batches - inputs.get(key, 0)), economy.quantity(town, key))
        if take > 1e-9:
            request[key] = take
            room -= take
    return request


def produce(town, building, villager):
    """Reuse profession recipes against local inputs; outputs never enter town stock."""
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
    if status != 'Working':
        return status
    building['input_buffer'] = {k: max(0, economy.quantity(local, k) - produced.get(k, 0)) for k in inputs}
    for key, amount in produced.items():
        record_output(town, building, key, amount)
    villager[villager['job']] = worker[villager['job']]
    return status


def storage_for(town, keys, operation):
    group_keys = {'food' if k in economy.EDIBLE else k for k in keys}
    return [b for b in town['buildings'] if b['kind'] in {'hall', 'storehouse'}
            and 'construction' not in b
            and any(economy.storage_allocations(b).get(k, 0) for k in group_keys)]


def start_trip(town, landscape, villager, building, storage, operation, route, hauler=False):
    position = villager.get('logistics_position', villager.get('construction_position', storage if hauler else building))
    first, last = (building, storage) if hauler else (storage, building)
    outward = route(landscape, (round(position['x']), round(position['y'])), first)
    if outward is None:
        return False
    back = route(landscape, outward[-1], last)
    if back is None:
        return False
    villager['logistics_trip'] = {'operation': operation, 'workplace': {k: building[k] for k in ('kind', 'x', 'y')},
                                 'storage': {k: storage[k] for k in ('kind', 'x', 'y')},
                                 'path': [list(p) for p in outward + back[1:]],
                                 'delivery_step': len(outward) - 1 if (operation == 'collect' and not hauler)
                                 or (operation == 'supply' and hauler) else len(outward + back[1:]) - 1,
                                 'delivered': False}
    villager['logistics_position'] = {'x': outward[0][0], 'y': outward[0][1]}
    return True


def advance_trip(town, landscape, villager, elapsed):
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
    blocked = {(b['x'], b['y']) for b in landscape['buildings'] + landscape['map_resources'] + landscape.get('npcs', [])}
    terrain = landscape.get('terrain', [])
    roads = {(r['x'], r['y']) for r in landscape.get('roads', [])}
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
        duration = 1 / (1.8 * (1.5 if tile in roads else 1))
        if remaining < duration:
            break
        remaining -= duration
        step += 1
        if step >= trip['delivery_step'] and not trip['delivered']:
            transfer(town, building, storage, trip['operation'], villager)
            trip['delivered'] = True
    # Already adjacent trips still perform their transfer once.
    if step >= trip['delivery_step'] and not trip['delivered']:
        transfer(town, building, storage, trip['operation'], villager)
        trip['delivered'] = True
    trip.update(step=step, travel_seconds=remaining)
    villager['logistics_position'] = {'x': path[step][0], 'y': path[step][1]}
    villager['logistics_target'] = {'kind': 'delivery', 'x': path[min(step + 1, len(path) - 1)][0],
                                    'y': path[min(step + 1, len(path) - 1)][1]}
    villager['production_status'] = 'Hauling outputs' if trip['operation'] == 'collect' else 'Delivering supplies'
    if step == len(path) - 1:
        villager.pop('logistics_trip', None)
        villager.pop('logistics_target', None)
    return True


def transfer(town, building, storage, operation, villager):
    if operation == 'collect':
        buffer = building.get('output_buffer', {})
        for key, amount in list(buffer.items()):
            group = 'food' if key in economy.EDIBLE else key
            if economy.storage_allocations(storage).get(group, 0):
                moved = economy.deliver(town, key, amount, sum(economy.storage_size(b) for b in town['buildings']
                                        if b['kind'] in {'hall', 'storehouse'} and 'construction' not in b), record=False)
                buffer[key] = max(0, amount - moved)
                if moved:
                    building.pop('buffer_full', None)
    else:
        # The assigned producer's practice determines the recipe, not the hauler's.
        producer = next((v for v in town['villagers'] if v.get('building_index') is not None
                         and town['buildings'][v['building_index']] is building
                         and v['job'] in economy.JOB_BUILDINGS), villager)
        request = supply_request(town, building, producer)
        request = {k: n for k, n in request.items() if economy.storage_allocations(storage).get('food' if k in economy.EDIBLE else k, 0)}
        if request:
            economy.spend(town, request)
            inputs = building.setdefault('input_buffer', {})
            for key, amount in request.items():
                inputs[key] = inputs.get(key, 0) + amount


def worker(town, landscape, villager, building, route, elapsed):
    if advance_trip(town, landscape, villager, elapsed):
        return True
    collect = buffered(building) >= BUFFER_SIZE - 1e-9 or building.get('buffer_full', False)
    request = supply_request(town, building, villager) if not collect else {}
    if not collect and not request:
        return False
    keys = building.get('output_buffer', {}) if collect else request
    stores = storage_for(town, keys, 'collect' if collect else 'supply')
    stores.sort(key=lambda b: abs(b['x'] - building['x']) + abs(b['y'] - building['y']))
    for storage in stores:
        if collect and not any(economy.storage_room(town, k) > 0 for k, n in keys.items() if n):
            continue
        if start_trip(town, landscape, villager, building, storage, 'collect' if collect else 'supply', route):
            advance_trip(town, landscape, villager, elapsed)
            return True
    if collect:
        villager['production_status'] = 'Workplace buffer full: storage full, unallocated, or unreachable'
        return True
    villager['production_status'] = 'Waiting for reachable storage with supplies'
    return True


def haul(town, landscape, villager, storage, route, elapsed):
    if advance_trip(town, landscape, villager, elapsed):
        return
    candidates = []
    for i, b in enumerate(town['buildings']):
        if b['kind'] not in PRODUCTION_KINDS or 'construction' in b:
            continue
        producers = [v for v in town['villagers'] if v.get('building_index') == i
                     and v['job'] in economy.JOB_BUILDINGS]
        for producer in producers:
            request = supply_request(town, b, producer)
            if request and storage in storage_for(town, request, 'supply'):
                candidates.append((0, b, 'supply'))
                break
        outputs = {k: n for k, n in b.get('output_buffer', {}).items() if n > 0
                   and economy.storage_room(town, k) > 0}
        if outputs and storage in storage_for(town, outputs, 'collect'):
            candidates.append((1, b, 'collect'))
    candidates.sort(key=lambda c: (c[0], -buffered(c[1]), abs(storage['x'] - c[1]['x']) + abs(storage['y'] - c[1]['y'])))
    for _, building, operation in candidates:
        if start_trip(town, landscape, villager, building, storage, operation, route, hauler=True):
            advance_trip(town, landscape, villager, elapsed)
            return
    villager['production_status'] = 'Waiting for goods or storage allocations'
