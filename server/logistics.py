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


def configure_market_sale(town, data):
    if not any(b['kind'] == 'market' and 'construction' not in b for b in town['buildings']):
        raise ValueError('Finish a market stall first.')
    key, cap = data.get('resource'), data.get('cap')
    if key not in economy.SELLABLE_GOODS:
        raise ValueError('Choose goods that can be sold at the market.')
    enabled = data.get('enabled', cap is not None)
    if type(enabled) is not bool:
        raise ValueError('Enable or disable automatic sales for this product.')
    rules = town.get('rules', {}).get('market_sales', {})
    previous = rules.get(key, {})
    if cap is None and not enabled:
        cap = previous.get('cap', 10)
    if cap is not None and (type(cap) not in (int, float) or not math.isfinite(cap) or cap < 1):
        raise ValueError('Use a sale cap of at least 1.')
    if cap is None:
        raise ValueError('Set a sale cap before enabling this product.')
    amount = data.get('amount', previous.get('amount', 10))
    if type(amount) is not int or not 1 <= amount <= 10:
        raise ValueError('Choose a sale batch from 1 to 10 goods.')
    rules = town.setdefault('rules', {}).setdefault('market_sales', {})
    rules[key] = {'enabled': enabled, 'cap': cap, 'amount': amount}
    return {}


def sale_cargo(town, building, villager):
    """Plan one product's sale against available stock, never reserved inputs."""
    for key, rule in town.get('rules', {}).get('market_sales', {}).items():
        if (key not in economy.SELLABLE_GOODS or not rule.get('enabled', True)
                or economy.quantity(town, key) + 1e-9 < rule['cap']):
            continue
        amount = math.floor(min(rule['amount'], production_stock(building, key), carry_capacity(villager)) + 1e-9)
        if amount > 0:
            return {key: amount}
    return {}


def carry_capacity(villager):
    return BUFFER_SIZE * economy.efficiency(villager, 'hauling')


def travel_speed(villager):
    return 1.8 * (1 + .25 * (villager.get('hauling', 0) // 20))


def buffered(building):
    return sum(economy.inventory(building).values()) + sum(building.get('pending_output', {}).values())


def buffer_size(building):
    return economy.WORKPLACE_STORAGE_SIZES.get(building['kind'], BUFFER_SIZE) * economy.storage_multiplier(building)


def production_stock(building, key):
    return max(0, economy.inventory(building).get(key, 0))


def output_fits(building, outputs):
    """Check each resource's whole batch, including returning output."""
    target = {**building, 'pending_output': dict(building.get('pending_output', {}))}
    for key, amount in outputs.items():
        if amount > economy.local_room(target, key) + 1e-9:
            return False
        target['pending_output'][key] = target['pending_output'].get(key, 0) + amount
    return True


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


def return_sale_cargo(town, trip, cargo):
    """Restore unsold cargo to its source, or salvage it at the hall."""
    source = next((b for b in town['buildings'] if all(b.get(k) == v for k, v in trip['workplace'].items())
                   and 'construction' not in b), None)
    if source is None:
        source = next(b for b in town['buildings'] if b['kind'] == 'hall')
    for key, amount in cargo.items():
        if amount > 0:
            stock = source.setdefault('stored_goods', {})
            stock[key] = stock.get(key, 0) + amount
            economy.add_stock(town, key, amount)


def cancel_trip(town, villager):
    trip = villager.get('logistics_trip')
    if trip and trip['operation'] == 'sell' and trip.get('picked_up') and not trip['delivered']:
        return_sale_cargo(town, trip, trip['cargo'])
    villager.pop('logistics_trip', None)
    villager.pop('logistics_target', None)


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
        speed = economy.processing_efficiency(villager, building)
        return economy.production_costs(building, speed, building.get('input_buffer', {}))
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
    elif building['kind'] == 'smokehouse':
        # All raw fish share the recipe's ingredient space, including mixed batches.
        fish = sum(needs.pop(key, 0) for key in economy.FISH)
        needs = {'fish': fish, **needs}
    # Reserve each ingredient's share so separate storage workers can supply a recipe.
    batches = buffer_size(building) / sum(needs.values()) if needs else 0
    room = max(0, buffer_size(building) - sum(inputs.values()))
    request = {}
    for key, amount in needs.items():
        keys = economy.FISH if building['kind'] == 'smokehouse' and key == 'fish' else (key,)
        wanted = max(0, amount * batches - sum(inputs.get(k, 0) for k in keys))
        for ingredient in keys:
            available = sum(production_stock(b, ingredient) for b in town['buildings']
                            if b is not building and 'construction' not in b)
            take = min(room, wanted, available)
            if take > 1e-9:
                request[ingredient] = take
                room -= take
                wanted -= take
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
    status = economy.extra_worker(local, worker, building, buffer_size(building))
    produced = local.get('produced', {})
    if not output_fits(building, produced):
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
    hides = {'sheep': 1, 'pig': 2, 'cow': 3}.get(task['animal'], 0)
    outputs = {'meat': amount, **({'hides': hides} if hides else {})}
    if not output_fits(building, outputs):
        building['buffer_full'] = True
        return 'Workplace buffer full'
    if hides > economy.production_room(town, 'hides') + 1e-9:
        return 'Waiting for hide buffer space or production cap'
    record_output(town, building, 'meat', amount)
    if hides:
        record_output(town, building, 'hides', hides)
    economy.practice(villager, 'butchery', economy.BUTCHER_SECONDS)
    del building['butchering']
    return 'Working'


def storage_for(town, keys, operation):
    return [b for b in town['buildings'] if 'construction' not in b
            and any((economy.local_room(b, k) if operation == 'collect' else production_stock(b, k)) > 0
                    for k in keys)]


def planned_cargo(town, building, storage, operation, villager):
    """Describe the batch this trip can deliver without reserving town stock."""
    if operation == 'sell':
        return sale_cargo(town, building, villager)
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
                                 'pickup_step': len(outward) - 1 if (operation in {'collect', 'sell'} and hauler)
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
        cancel_trip(town, villager)
        villager['production_status'] = 'Delivery cancelled: building moved or unavailable'
        return True
    if 'pickup_step' not in trip:
        selling = trip['operation'] == 'sell'
        pickup = building if selling or (trip['operation'] == 'collect' and villager.get('job') == 'hauling') else storage
        fetching = selling or (trip['operation'] == 'collect') == (villager.get('job') == 'hauling')
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
    if (trip['operation'] == 'sell' and step >= trip['pickup_step']
            and not trip.get('picked_up') and not trip['delivered']):
        trip['cargo'] = pickup_sale(town, building, villager)
        trip['picked_up'] = True
    remaining = elapsed + trip.get('travel_seconds', 0)
    while step < len(path) - 1:
        tile = tuple(path[step + 1])
        before = path[step]
        corners = ((tile[0], before[1]), (before[0], tile[1])) if tile[0] != before[0] and tile[1] != before[1] else ()
        if (tile in blocked or (terrain and terrain[tile[1]][tile[0]] == 'v')
                or any(p in blocked or (terrain and terrain[p[1]][p[0]] == 'v') for p in corners)):
            cancel_trip(town, villager)
            villager['production_status'] = 'Delivery blocked; finding another route'
            return True
        duration = math.hypot(tile[0]-before[0], tile[1]-before[1]) / (travel_speed(villager) * (1.5 if tile in roads else 1))
        if remaining < duration:
            break
        remaining -= duration
        step += 1
        if step == trip.get('pickup_step') and not trip['delivered']:
            if trip['operation'] == 'sell':
                trip['cargo'] = pickup_sale(town, building, villager)
                trip['picked_up'] = True
            else:
                trip['cargo'] = planned_cargo(town, building, storage, trip['operation'], villager)
        if step >= trip['delivery_step'] and not trip['delivered']:
            trip['cargo'] = transfer(town, building, storage, trip['operation'], villager, trip['cargo'])
            trip['delivered'] = True
    # Already adjacent trips still perform their transfer once.
    if step >= trip['delivery_step'] and not trip['delivered']:
        trip['cargo'] = transfer(town, building, storage, trip['operation'], villager, trip['cargo'])
        trip['delivered'] = True
    trip.update(step=step, travel_seconds=remaining)
    villager['logistics_position'] = {'x': path[step][0], 'y': path[step][1]}
    villager['logistics_target'] = {'kind': 'delivery', 'x': path[min(step + 1, len(path) - 1)][0],
                                    'y': path[min(step + 1, len(path) - 1)][1]}
    if trip['operation'] == 'sell':
        villager['production_status'] = 'Collecting goods for sale' if step < trip['pickup_step'] else 'Returning to market to sell'
    else:
        villager['production_status'] = 'Hauling outputs' if trip['operation'] == 'collect' else 'Delivering supplies'
    if step == len(path) - 1:
        villager.pop('logistics_trip', None)
        villager.pop('logistics_target', None)
        return not resume_work
    return True


def pickup_sale(town, building, villager, planned=None):
    """Remove the actual sale batch from available stock when it is collected."""
    economy.ensure_inventory(town)
    cargo = sale_cargo(town, building, villager)
    if planned is not None:
        cargo = {key: min(amount, planned.get(key, 0)) for key, amount in cargo.items()
                 if planned.get(key, 0) > 0}
    for key, amount in cargo.items():
        economy.withdraw(town, key, amount, source=building)
        economy.add_stock(town, key, -amount)
    return cargo


def transfer(town, building, storage, operation, villager, planned=None):
    economy.ensure_inventory(town)
    if operation == 'sell':
        trip = villager.get('logistics_trip', {})
        held = (planned or {}) if trip.get('picked_up') else pickup_sale(town, building, villager, planned or {})
        cargo = {}
        unsold = {}
        for key, wanted in held.items():
            rule = town.get('rules', {}).get('market_sales', {}).get(key)
            if rule is None or not rule.get('enabled', True):
                unsold[key] = wanted
                continue
            amount = math.floor(min(wanted, rule['amount']) + 1e-9)
            unsold[key] = wanted - amount
            if amount <= 0:
                continue
            economy.record_sale(town, key, amount)
            economy.practice(villager, 'trading')
            economy.practice(villager, 'hauling')
            cargo[key] = amount
        return_sale_cargo(town, {'workplace': {k: building[k] for k in ('kind', 'x', 'y')}}, unsold)
        return cargo
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
    if villager['job'] == 'farming':
        # Farms store several crops independently. Full stock from another
        # crop (or a stale buffer_full flag) must not stop this farmer's pass.
        field = villager.get('work_target') or building
        crop = field.get('field', {}).get('crop', field.get('crop', 'vegetables'))
        collect = economy.local_room(building, crop) <= 1e-9
    elif building['kind'] in economy.RECIPES:
        # Full ingredient storage must not prevent processing those supplies.
        output = economy.production_recipe(building)[1]
        collect = building.get('buffer_full', False) or (
            collection_inventory(building).get(output, 0) > 0
            and economy.local_room(building, output) <= 1e-9)
    else:
        collect = building.get('buffer_full', False) or any(
            amount > 0 and economy.local_room(building, key) <= 1e-9
            for key, amount in collection_inventory(building).items())
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


def merchant(town, landscape, villager, market, route, elapsed):
    villager.pop('work_animation', None)
    if advance_trip(town, landscape, villager, elapsed):
        return
    sources = [b for b in town['buildings'] if b['kind'] != 'market'
               and 'construction' not in b and sale_cargo(town, b, villager)]
    sources.sort(key=lambda b: abs(market['x'] - b['x']) + abs(market['y'] - b['y']))
    for source in sources:
        if start_trip(town, landscape, villager, source, market, 'sell', route, hauler=True):
            advance_trip(town, landscape, villager, elapsed)
            return
    villager['production_status'] = ('Waiting for a reachable source of sale goods' if sources
                                     else 'Waiting for stock to reach an automatic sale trigger')
