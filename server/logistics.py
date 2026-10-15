"""Building inventories and host-controlled transport between workplaces and storage."""
from spatial import occupied_tiles
import math
import barns
from navigation import routes

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
    buy = town.get('rules', {}).get('market_purchases', {}).get(key)
    if enabled and buy is not None and buy >= cap:
        raise ValueError('The sale cap must be higher than Buy at.')
    rules = town.setdefault('rules', {}).setdefault('market_sales', {})
    rules[key] = {'enabled': enabled, 'cap': cap, 'amount': amount}
    if previous.get('strict'):
        rules[key]['strict'] = True
    return {}


def configure_resource_trade(town, data):
    """Configure both thresholds atomically; leave a gap to avoid trade loops."""
    if not any(b['kind'] == 'market' and 'construction' not in b for b in town['buildings']):
        raise ValueError('Finish a market stall first.')
    key, buy, sell = data.get('resource'), data.get('buy_below'), data.get('sell_above')
    if key not in economy.STORAGE_KEYS or key == 'food':
        raise ValueError('Choose a resource that can be bought.')
    for value in (buy, sell):
        if value is not None and (type(value) not in (int, float) or not math.isfinite(value) or value < 0):
            raise ValueError('Use a non-negative threshold, or leave it blank to disable.')
    if sell is not None and key not in economy.SELLABLE_GOODS:
        raise ValueError('This resource cannot be sold automatically.')
    if buy is not None and sell is not None and buy >= sell:
        raise ValueError('Sell at must be higher than Buy at.')
    rules = town.setdefault('rules', {})
    purchases = rules.setdefault('market_purchases', {})
    if buy is None:
        purchases.pop(key, None)
    else:
        purchases[key] = buy
    sales = rules.setdefault('market_sales', {})
    if sell is None:
        if key in sales:
            sales[key]['enabled'] = False
    else:
        sales[key] = {**sales.get(key, {}), 'enabled': True, 'cap': sell,
                      'amount': sales.get(key, {}).get('amount', 10), 'strict': True}
    return {}


def sale_cargo(town, building, villager):
    """Plan one product's sale against available stock, never reserved inputs."""
    for key, rule in town.get('rules', {}).get('market_sales', {}).items():
        if (key not in economy.SELLABLE_GOODS or not rule.get('enabled', True)
                or economy.quantity(town, key) + 1e-9 < rule['cap']):
            continue
        surplus = max(0, economy.quantity(town, key) - rule['cap']) if rule.get('strict') else float('inf')
        amount = math.floor(min(rule['amount'], surplus, production_stock(building, key), carry_capacity(villager)) + 1e-9)
        if amount > 0:
            return {key: amount}
    return {}


def carry_capacity(villager, building=None):
    equipment = economy.grain_equipment_multiplier(building) if building and villager.get('job') in {'milling', 'baking'} else 1
    return BUFFER_SIZE * economy.efficiency(villager, 'hauling') * equipment


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
    economy.record_activity(town, key, amount, 'in')
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
    animation = villager.get('work_animation')
    if animation and not villager.get('logistics_trip'):
        villager['logistics_position'] = {k: animation[k] for k in ('x', 'y')}
    villager.pop('production_work', None)
    spec = villager.pop('pending_workplace', None)
    goods = villager.pop('pending_output', {})
    building = next((b for b in town['buildings'] if spec and all(b.get(k) == v for k, v in spec.items())), None)
    if building:
        orphaned = building.setdefault('orphaned_output', {})
        for key, amount in goods.items():
            orphaned[key] = orphaned.get(key, 0) + amount
    villager.pop('work_animation', None)


def return_cargo(town, trip, cargo):
    """Restore cancelled cargo to its source, or salvage it at the hall."""
    spec = trip['storage'] if trip.get('operation') == 'supply' else trip['workplace']
    source = next((b for b in town['buildings'] if all(b.get(k) == v for k, v in spec.items())
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
    if trip and trip.get('picked_up') and not trip['delivered']:
        return_cargo(town, trip, trip['cargo'])
    villager.pop('logistics_trip', None)
    villager.pop('logistics_target', None)


def advance_barn(town, building, seconds, keepers=0):
    """Grow animals to adulthood without keepers, feed, routes, or output room."""
    elapsed = seconds * (1 + keepers)
    outputs, growth = {}, {}
    growing_seconds = 0
    for animal in building.get('animals', {}):
        if animal not in economy.ANIMAL_MEAT:
            continue
        duration = economy.animal_duration(animal)
        ages, animal_seconds = [], 0
        for age in economy.animal_growth(building, animal):
            age = min(duration, max(0, age))
            advanced = min(elapsed, duration - age)
            animal_seconds += advanced
            ages.append(age + advanced)
        growth[animal] = ages
        growing_seconds += animal_seconds
        product = economy.ANIMAL_PRODUCTS.get(animal, 0) * animal_seconds / duration
        if product:
            key = economy.ANIMALS[animal]['output']
            outputs[key] = outputs.get(key, 0) + product
    building['animal_growth'] = growth
    outputs['manure'] = .1 * growing_seconds / 3
    for key, amount in outputs.items():
        # A full buffer or production cap limits goods, never the growth clock.
        amount = min(amount, economy.local_room(building, key), economy.production_room(town, key))
        if amount > 1e-9:
            record_output(town, building, key, amount)


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
        feed = economy.animal_feed(building)
        if not feed:
            return {}
        inputs = building.get('input_buffer', {})
        return {'fodder' if inputs.get('fodder', 0) >= feed else 'vegetables': feed}
    return {}


def advance_workshop_queue(town, building):
    """Release obsolete reservations and unfinished work when the head changes."""
    spec = economy.selected_recipe(building) if building.get('craft_queue') else None
    economy.ensure_inventory(town)
    for key in list(building.get('input_buffer', {})):
        if spec is None or key not in spec['inputs']:
            amount = building['input_buffer'].pop(key)
            buffer = building.setdefault('output_buffer', {})
            buffer[key] = buffer.get(key, 0) + amount
            economy.add_stock(town, key, amount)
    index = next(i for i, candidate in enumerate(town['buildings']) if candidate is building)
    for worker in town['villagers']:
        trip = worker.get('logistics_trip', {})
        if (trip.get('operation') == 'supply'
                and all(building.get(k) == v for k, v in trip['workplace'].items())):
            cancel_trip(town, worker)
        if worker.get('building_index') == index:
            worker.pop('production_work', None)
    building.pop('buffer_full', None)


def supply_request(town, building, villager):
    needs = requirements(building, villager)
    inputs = dict(building.get('input_buffer', {}))
    for worker in town['villagers']:
        trip = worker.get('logistics_trip', {})
        if (trip.get('operation') == 'supply' and trip.get('picked_up') and not trip['delivered']
                and all(building.get(k) == v for k, v in trip['workplace'].items())):
            for key, amount in trip['cargo'].items():
                inputs[key] = inputs.get(key, 0) + amount
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
    if building['kind'] == 'workshop' and building.get('craft_queue') and needs:
        spec = economy.selected_recipe(building)
        speed = economy.processing_efficiency(villager, building)
        batches = min(batches, building['craft_queue'][0]['remaining'] / (spec['amount'] * speed))
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
            queued = building.get('grain_work_queue', [])
            saved = next((p for p in queued if p.get('task', [None])[-1] == output), None)
            if saved:
                progress['elapsed'] = saved['elapsed']
                queued.remove(saved)
        duration = economy.production_seconds(building)
        progress['elapsed'] = min(duration, progress['elapsed'] + elapsed)
        villager['production_work'] = progress
        if progress['elapsed'] < duration:
            return f"Producing {output.replace('_', ' ')} · {math.ceil(duration - progress['elapsed'])}s remaining"
        villager.pop('production_work', None)
    if deposit:
        economy.take_supply(town, deposit, produced['iron_ore'])
    building['input_buffer'] = {k: max(0, economy.quantity(local, k) - produced.get(k, 0)) for k in inputs}
    for key, amount in inputs.items():
        economy.record_activity(town, key, amount - building['input_buffer'].get(key, 0), 'out')
    for key, amount in produced.items():
        record_output(town, building, key, amount, villager)
    if building['kind'] == 'workshop':
        queue = building['craft_queue']
        queue[0]['remaining'] = max(0, queue[0]['remaining'] - sum(produced.values()))
        if queue[0]['remaining'] <= 1e-9:
            queue.pop(0)
            advance_workshop_queue(town, building)
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
    remaining = carry_capacity(villager, building)
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
    if routes.active and operation == 'supply':
        routes.priority = min(routes.priority, 1)
    start = (round(position['x']), round(position['y']))
    source_first = (operation == 'supply' and hauler) or (operation == 'collect' and not hauler)
    source = storage if operation == 'supply' else building
    approach = [start]
    if source_first and not any(abs(start[0] - x) + abs(start[1] - y) == 1
                                for x, y in occupied_tiles([source])):
        approach = route(landscape, start, source)
        if approach is None:
            return False
    outward = route(landscape, approach[-1], first)
    if outward is None:
        return False
    outward = approach[:-1] + outward
    back = route(landscape, outward[-1], last)
    if back is None:
        return False
    villager['logistics_trip_id'] = villager.get('logistics_trip_id', 0) + 1
    villager['logistics_trip'] = {'id': villager['logistics_trip_id'], 'operation': operation, 'workplace': {k: building[k] for k in ('kind', 'x', 'y')},
                                 'storage': {k: storage[k] for k in ('kind', 'x', 'y')},
                                 'path': [list(p) for p in outward + back[1:]],
                                 'turn_step': len(outward) - 1,
                                 'pickup_step': len(outward) - 1 if (operation in {'collect', 'sell'} and hauler)
                                 or (operation == 'supply' and not hauler) else len(approach) - 1,
                                 'cargo': planned_cargo(town, building, storage, operation, villager),
                                 'delivery_step': len(outward) - 1 if (operation == 'collect' and not hauler)
                                 or (operation == 'supply' and hauler) else len(outward + back[1:]) - 1,
                                 'delivered': False}
    villager['logistics_position'] = {'x': outward[0][0], 'y': outward[0][1]}
    return True


def replan_trip(landscape, villager, building, storage, blocked, roads, fences):
    """Replace only the remaining legs, preserving pickup and delivery state."""
    trip = villager['logistics_trip']
    step, path = trip.get('step', 0), trip['path']
    turn = trip.get('turn_step', trip['pickup_step'] or trip['delivery_step'])
    hauler = villager['job'] in ('hauling', 'trading')
    first, last = (building, storage) if hauler else (storage, building)
    start = tuple(path[step])
    size, terrain = landscape.get('map_size', 120), landscape.get('terrain', [])

    def leg(position, target):
        points = routes.route(position, target, blocked, size, terrain, roads=roads,
                              road_mode='arrival', diagonal=True, fences=fences)
        return None if points is None else [position] + [(p['x'], p['y']) for p in points]

    source_first = (trip['operation'] == 'supply' and hauler) or (trip['operation'] == 'collect' and not hauler)
    approach = [start]
    if source_first and not trip.get('picked_up') and step < trip['pickup_step']:
        approach = leg(start, storage if trip['operation'] == 'supply' else building)
        if approach is None:
            return
    outward = leg(approach[-1], first) if step < turn else [start]
    if outward is None:
        return
    outward = approach[:-1] + outward
    back = leg(outward[-1], last)
    if back is None:
        return
    new_path = outward + back[1:]
    trip.update(path=[list(p) for p in new_path], step=0, travel_seconds=0,
                pickup_step=(len(approach) - 1 if source_first else len(outward) - 1)
                if step < trip['pickup_step'] else 0,
                delivery_step=(0 if trip['delivered'] else len(outward) - 1 if trip['delivery_step'] == turn
                               and step < turn else len(new_path) - 1),
                turn_step=len(outward) - 1)


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
               occupied_tiles(obstacles + landscape['map_resources'] + landscape.get('npcs', [])))
    terrain = landscape.get('terrain', [])
    roads = {(r['x'], r['y']) for r in landscape.get('roads', []) if 'construction' not in r}
    path = trip['path']
    fences = barns.fences(landscape['buildings'], landscape.get('map_size', 120))
    step = trip.get('step', 0)
    def pickup():
        if not trip.get('picked_up') and not trip['delivered']:
            trip['cargo'] = pickup_cargo(town, building, storage, trip['operation'], villager)
            trip['picked_up'] = True

    def deliver():
        if trip['delivered']:
            return True
        if trip['operation'] == 'sell':
            trip['cargo'] = transfer(town, building, storage, 'sell', villager, trip['cargo'])
            trip['delivered'] = True
            return True
        moved, held = deposit_cargo(town, building, storage, trip['operation'], trip['cargo'])
        if moved and not trip.get('trained'):
            economy.practice(villager, 'hauling')
            trip['trained'] = True
        trip['delivered'] = not held
        trip['cargo'] = held if held else moved
        return trip['delivered']

    if step >= trip['pickup_step']:
        pickup()
    remaining = elapsed + trip.get('travel_seconds', 0)
    waiting = step >= trip['delivery_step'] and not deliver()
    while step < len(path) - 1:
        if waiting:
            break
        tile = tuple(path[step + 1])
        before = path[step]
        corners = ((tile[0], before[1]), (before[0], tile[1])) if tile[0] != before[0] and tile[1] != before[1] else ()
        if (tile in blocked or barns.crosses(before, tile, fences)
                or (terrain and terrain[tile[1]][tile[0]] == 'v')
                or any(p in blocked or (terrain and terrain[p[1]][p[0]] == 'v') for p in corners)):
            # Save movement already completed in this update before a queued
            # replan can suspend. Held cargo and the original assignment
            # stay on the trip until delivery or explicit cancellation.
            trip.update(step=step, travel_seconds=0)
            villager['logistics_position'] = {'x': path[step][0], 'y': path[step][1]}
            villager['production_status'] = 'Delivery blocked; finding another route'
            routes.priority = 0
            replan_trip(landscape, villager, building, storage, blocked, roads, fences)
            return True
        duration = math.hypot(tile[0]-before[0], tile[1]-before[1]) / (travel_speed(villager) * (1.5 if tile in roads else 1))
        if remaining < duration:
            break
        remaining -= duration
        step += 1
        if step == trip.get('pickup_step') and not trip['delivered']:
            pickup()
        if step >= trip['delivery_step'] and not trip['delivered']:
            waiting = not deliver()
    trip.update(step=step, travel_seconds=0 if waiting else remaining)
    villager['logistics_position'] = {'x': path[step][0], 'y': path[step][1]}
    villager['logistics_target'] = {'kind': 'delivery', 'x': path[min(step + 1, len(path) - 1)][0],
                                    'y': path[min(step + 1, len(path) - 1)][1]}
    if waiting:
        villager['production_status'] = 'Waiting for delivery storage'
        villager['logistics_target'] = {'kind': 'delivery', **villager['logistics_position']}
    elif trip['operation'] == 'sell':
        villager['production_status'] = 'Collecting goods for sale' if step < trip['pickup_step'] else 'Returning to market to sell'
    else:
        villager['production_status'] = 'Hauling outputs' if trip['operation'] == 'collect' else 'Delivering supplies'
    if step == len(path) - 1 and trip['delivered']:
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
            surplus = max(0, economy.quantity(town, key) + wanted - rule['cap']) if rule.get('strict') else float('inf')
            amount = math.floor(min(wanted, rule['amount'], surplus) + 1e-9)
            unsold[key] = wanted - amount
            if amount <= 0:
                continue
            economy.record_activity(town, key, amount, 'out')
            economy.record_sale(town, key, amount)
            economy.practice(villager, 'trading')
            economy.practice(villager, 'hauling')
            cargo[key] = amount
        return_cargo(town, {'workplace': {k: building[k] for k in ('kind', 'x', 'y')}}, unsold)
        return cargo
    held = pickup_cargo(town, building, storage, operation, villager)
    cargo, unused = deposit_cargo(town, building, storage, operation, held)
    if unused:
        return_cargo(town, {'operation': operation,
                               'workplace': {k: building[k] for k in ('kind', 'x', 'y')},
                               'storage': {k: storage[k] for k in ('kind', 'x', 'y')}}, unused)
    if cargo:
        economy.practice(villager, 'hauling')
    return cargo


def pickup_cargo(town, building, storage, operation, villager):
    """Move a real batch from its source into the worker's trip inventory."""
    if operation == 'sell':
        return pickup_sale(town, building, villager)
    economy.ensure_inventory(town)
    cargo = planned_cargo(town, building, storage, operation, villager)
    source = building if operation == 'collect' else storage
    for key, amount in cargo.items():
        remaining = amount
        for field in ('output_buffer', 'stored_goods', 'orphaned_output'):
            stock = source.get(field, {})
            take = min(remaining, stock.get(key, 0))
            if take > 0:
                stock[key] -= take
                if field == 'orphaned_output':
                    source['pending_output'][key] -= take
                else:
                    economy.add_stock(town, key, -take)
                remaining -= take
        source.pop('buffer_full', None)
    return cargo


def deposit_cargo(town, building, storage, operation, cargo):
    """Unload held goods only; leave anything that no longer fits on the worker."""
    moved, held = {}, {}
    destination = storage if operation == 'collect' else building
    field = 'stored_goods' if operation == 'collect' else 'input_buffer'
    target = destination.get(field, {})
    for key, amount in cargo.items():
        room = (economy.local_room(destination, key) if operation == 'collect' else
                max(0, buffer_size(destination) - sum(target.values())))
        take = min(amount, room)
        if take > 0:
            destination[field] = target
            target[key] = target.get(key, 0) + take
            if operation == 'collect':
                economy.add_stock(town, key, take)
            moved[key] = take
        if amount > take:
            held[key] = amount - take
    return moved, held


def worker(town, landscape, villager, building, route, elapsed):
    """Producers fetch required inputs; storehouse haulers move their outputs."""
    if villager.get('logistics_trip', {}).get('operation') == 'collect':
        # Older saves may contain producer output trips. Cancellation restores
        # any goods already collected before releasing the assignment.
        cancel_trip(town, villager)
    if villager.get('logistics_trip'):
        return advance_trip(town, landscape, villager, elapsed, resume_work=True)
    if villager['job'] == 'farming':
        # Other crops' full stores must not block this farmer's current pass.
        field = villager.get('work_target') or building
        crop = field.get('field', {}).get('crop', field.get('crop', 'vegetables'))
        full = economy.local_room(building, crop) <= 1e-9
    elif building['kind'] in economy.RECIPES:
        # Full ingredient storage still allows processing those supplies.
        outputs = economy.SMOKED_FISH if building['kind'] == 'smokehouse' else (economy.production_recipe(building)[1],)
        full = building.get('buffer_full', False) or any(
            collection_inventory(building).get(output, 0) > 0
            and economy.local_room(building, output) <= 1e-9 for output in outputs)
    else:
        full = building.get('buffer_full', False) or any(
            amount > 0 and economy.local_room(building, key) <= 1e-9
            for key, amount in collection_inventory(building).items())
    request = supply_request(town, building, villager)
    if not request:
        if full:
            villager['production_status'] = 'Workplace buffer full'
            return True
        return False
    stores = [b for b in storage_for(town, request, 'supply') if b is not building]
    stores.sort(key=lambda b: abs(b['x'] - building['x']) + abs(b['y'] - building['y']))
    for storage in stores:
        if start_trip(town, landscape, villager, building, storage, 'supply', route):
            return advance_trip(town, landscape, villager, elapsed, resume_work=True)
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


def purchase_batch(town, villager, elapsed):
    """Buy at most ten items per merchant per three seconds, directly into storage."""
    candidates = []
    for key, threshold in town.get('rules', {}).get('market_purchases', {}).items():
        if key not in economy.STORAGE_KEYS or key == 'food':
            continue
        price = 2 * economy.sale_price(key)
        amount = math.floor(min(10, threshold - economy.quantity(town, key),
                                economy.storage_room(town, key), town['coins'] / price) + 1e-9)
        if amount > 0:
            candidates.append((key, amount, price))
    if not candidates:
        villager.pop('market_purchase_work', None)
        return False
    work = villager.get('market_purchase_work', 0) + elapsed
    if work < 3:
        villager['market_purchase_work'] = work
        villager['production_status'] = 'Buying supplies at the market'
        return True
    villager.pop('market_purchase_work', None)
    # Rotate products so a large target cannot starve the other purchases.
    previous = villager.get('market_last_purchase')
    keys = [key for key, _, _ in candidates]
    index = (keys.index(previous) + 1) % len(keys) if previous in keys else 0
    key, amount, price = candidates[index]
    delivered = economy.deliver(town, key, amount, float('inf'), record=False)
    economy.spend(town, {'coins': delivered * price})
    villager['market_last_purchase'] = key
    economy.practice(villager, 'trading')
    villager['production_status'] = f'Bought {delivered:g} {key.replace("_", " ")}'
    return True


def merchant(town, landscape, villager, market, route, elapsed):
    villager.pop('work_animation', None)
    if advance_trip(town, landscape, villager, elapsed):
        return
    if purchase_batch(town, villager, elapsed):
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
