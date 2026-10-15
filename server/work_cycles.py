"""Host-owned workplace animation and drop-off timing."""
import copy
import math

import progression as economy
import farming
import barns
from navigation import routes
from spatial import SpatialIndex, occupied_tiles


def fishing_crew(town, building):
    """One boat crew space per boat tier; other hut workers use the dock."""
    tier = economy.fishing_boat_level(building)
    if not tier:
        return []
    owner = next((i for i, b in enumerate(town['buildings']) if b is building), None)
    huts = [i for i, b in enumerate(town['buildings']) if b['kind'] == 'fishing_hut'
            and ('construction' not in b or 'upgrade' in b['construction'])
            and any(r['kind'] == 'water' and abs(r['x'] - b['x']) + abs(r['y'] - b['y'])
                    <= 2 + b.get('level', 1) + (3 + 2 * (economy.fishing_boat_level(b) - 1)
                                               if economy.fishing_boat_level(b) else 0)
                    for r in town['map_resources'])]
    crew = [i for i, v in enumerate(town['villagers']) if v['job'] == 'fishing'
            and v.get('building_index', huts[i % len(huts)] if huts else None) == owner]
    return crew[:tier]


def fishing_boating(town, building, worker_index):
    return worker_index in fishing_crew(town, building)


def boat_load(town, building):
    return sum(amount for index in fishing_crew(town, building)
               for key, amount in town['villagers'][index].get('pending_output', {}).items() if key in economy.FISH)


def fishing_water(building, water, occupied, reach, terrain=None, depths=None):
    """Water reachable by boat from the hut's nearest bank, within its reach."""
    tiles = {(r['x'], r['y']): r for r in water
             if (r['x'], r['y']) not in occupied
             and (depths is None or depths.get((r['x'], r['y']), 1) <= economy.fishing_boat_level(building) + 1)
             and (not terrain or terrain[r['y']][r['x']] != 'v')
             and abs(r['x'] - building['x']) + abs(r['y'] - building['y']) <= reach}
    if not tiles:
        return []
    berth = min(tiles, key=lambda p: (abs(p[0] - building['x']) + abs(p[1] - building['y']), p[1], p[0]))
    queue, seen = [berth], {berth}
    for x, y in queue:
        for tile in ((x + 1, y), (x, y + 1), (x - 1, y), (x, y - 1)):
            if tile in tiles and tile not in seen:
                seen.add(tile)
                queue.append(tile)
    return [tiles[tile] for tile in queue]


def fishing_pier(building, landscape, blocked, size, terrain, bounds):
    occupied = occupied_tiles(landscape['buildings'] + landscape.get('npcs', []))
    water = sorted((r for r in landscape['map_resources'] if r['kind'] == 'water'
                    and (r['x'], r['y']) not in occupied
                    and abs(r['x'] - building['x']) + abs(r['y'] - building['y']) <= 3),
                   key=lambda r: (abs(r['x'] - building['x']) + abs(r['y'] - building['y']), r['y'], r['x']))
    pier_blocked = blocked - {(building['x'], building['y'])} - {(r['x'], r['y']) for r in water}
    for tile in water:
        deck = route(building, tile, pier_blocked, size, terrain, False, bounds, diagonal=False,
                     fences=barns.fences(landscape['buildings'], size))
        if deck is not None and len(deck) <= 3:
            return [building, *deck]
    return []


def retained_pier(state, building, landscape, blocked, terrain, source=None):
    """Keep an existing deck while its hut, endpoint, and steps remain usable."""
    if not state or state['task'][:4] != ['fishing', building['kind'], building['x'], building['y']]:
        return None
    pier = state.get('pier')
    if not pier or (source and (pier[-1]['x'], pier[-1]['y']) != (source['x'], source['y'])):
        return None
    occupied = occupied_tiles(landscape['buildings'] + landscape.get('npcs', []))
    water = {(r['x'], r['y']) for r in landscape['map_resources'] if r['kind'] == 'water'}
    fences = barns.fences(landscape['buildings'], landscape.get('map_size', 120))
    for index, tile in enumerate(pier[1:], 1):
        p = tile['x'], tile['y']
        before = pier[index - 1]
        if (p in occupied or barns.crosses((before['x'], before['y']), p, fences)
                or (p in blocked and p not in water)
                or (terrain and (not terrain[p[1]][p[0]] or terrain[p[1]][p[0]] == 'v'))):
            return None
    return pier


def route(start, target, blocked, size, terrain, adjacent=True, bounds=None, roads=(), diagonal=True, fences=()):
    start = (round(start['x']), round(start['y']))
    return routes.route(start, target, blocked, size, terrain, adjacent, bounds, roads, diagonal=diagonal, fences=fences)


def walking_bounds(town, landscape):
    if not landscape.get('chunked'):
        return None
    size = landscape.get('map_size', 120)
    center = town.get('town_origin') or next(b for b in town['buildings'] if b['kind'] == 'hall')
    return (max(0, center['x'] - 60), min(size - 1, center['x'] + 60),
            max(0, center['y'] - 60), min(size - 1, center['y'] + 60))


def starting_position(town, landscape, villager, index, blocked):
    """Use the same safe saved position for target selection and animation."""
    hall = next(b for b in town['buildings'] if b['kind'] == 'hall')
    position = (villager.get('work_animation') or villager.get('logistics_position')
                or villager.get('construction_position')
                or {'x': hall['x'] - 2 + index % 5, 'y': hall['y'] + 3 + index // 5})
    x, y = round(position['x']), round(position['y'])
    size, terrain = landscape.get('map_size', 120), landscape.get('terrain', [])
    if (x, y) in blocked or not (0 <= x < size and 0 <= y < size) or (terrain and terrain[y][x] == 'v'):
        candidates = ((cx, cy) for cx in range(max(0, x - 60), min(size, x + 61))
                      for cy in range(max(0, y - 60), min(size, y + 61)))
        spot = min((p for p in candidates if p not in blocked and (not terrain or terrain[p[1]][p[0]] != 'v')),
                   key=lambda p: abs(p[0] - x) + abs(p[1] - y), default=None)
        return {'x': spot[0], 'y': spot[1]} if spot is not None else None
    return {'x': position['x'], 'y': position['y']}


def advance(town, landscape, villager, building, source, index, seconds):
    """Record a compact, confirmed movement timeline for delayed browser playback."""
    if villager['job'] == 'fishing':
        crew = fishing_crew(town, building)
        if index in crew and index != crew[0]:
            captain = town['villagers'][crew[0]].get('work_animation')
            if captain:
                # One hut owns one voyage; extra crew never sail independently.
                villager['work_animation'] = copy.deepcopy(captain)
                return False
    previous = villager.get('work_animation', {}).get('playback')
    finished, _ = _advance(town, landscape, villager, building, source, index, seconds)
    state = villager.get('work_animation')
    if state and state.get('playback') is not previous:
        state['playback'].append(_sample(state, seconds, source))
    return finished


def advance_farming(town, landscape, villager, building, source, index, seconds):
    """Stop at an action or delivery so the caller can use the remaining time."""
    finished, remaining = _advance(town, landscape, villager, building, source, index,
                                   seconds, stop_at_delivery=True)
    state = villager.get('work_animation')
    if state:
        state['playback'].append(_sample(state, seconds - remaining, source))
    return finished, remaining


def _sample(state, time, source):
    sample = {key: state.get(key) for key in ('x', 'y', 'phase', 'elapsed', 'facing',
              'waterReady', 'waterSource', 'farmAction')}
    sample['time'] = time
    if state.get('butcherAnimal'):
        sample.update(butcherAnimal=state['butcherAnimal'], butcherPickup=state.get('butcherPickup'))
    if state['task'][0] == 'farming' and source['kind'] == 'acre':
        sample['farmField'] = _field_view(source)
    return sample


def _field_view(source):
    field = farming.field_state(source)
    return {'x': source['x'], 'y': source['y'],
            **{key: field[key] for key in ('stage', 'elapsed', 'progress', 'cycle', 'crop')
               if key in field}}


def field_work_spot(source):
    """A tool-length approach inside the acre's walkable tile."""
    if farming.field_state(source)['stage'] not in ('unplowed', 'plowed', 'thirsty'):
        return {'x': source['x'], 'y': source['y']}
    if source['x'] > 0:
        return {'x': source['x'] - .45, 'y': source['y'] + .1}
    if source['y'] > 0:
        return {'x': source['x'] + .1, 'y': source['y'] - .45}
    return {'x': source['x'] + .45, 'y': source['y'] + .1}


def pause_farming(villager, source, seconds):
    """Publish a resting pose when storage prevents work from advancing."""
    state = villager.get('work_animation')
    if state:
        state.update(phase='waiting_storage', path=[], elapsed=0, farmAction=None)
        state['version'] += 1
        state['playback'] = [_sample(state, 0, source), _sample(state, seconds, source)]


def finish_harvest(state, source):
    """Place the emptied field at the confirmed end of the sickle animation."""
    for sample in state.get('playback', []):
        if sample['phase'] == 'harvest_ready':
            sample['farmField'] = _field_view(source)


def _advance(town, landscape, villager, building, source, index, seconds, stop_at_delivery=False):
    """Return True only when the worker finishes unloading at its workplace."""
    boating = villager['job'] == 'fishing' and fishing_boating(town, building, index)
    spec = [villager['job'], building['kind'], building['x'], building['y'], source['kind'], source['x'], source['y']]
    if villager['job'] == 'fishing':
        spec.extend([building.get('level', 1), economy.fishing_boat_level(building), boating, 'boat_cargo_cycle'])
    state = villager.get('work_animation')
    farming_job = villager['job'] == 'farming'
    barn_keeper = villager['job'] == 'husbandry' and building['kind'] == 'barn'
    butcher = building.get('butchering') if villager['job'] == 'butchery' else None
    butcher_trip = butcher and butcher.get('pickup')
    if butcher_trip:
        spec.extend(['butcher_animal', butcher['id']])
    water_round = None
    if farming_job:
        field = farming.field_state(source)
        water_round = farming.watering_round(building, source)
        spec.extend(['field_cycle', field['cycle']])
        # Upgrade a saved filled can without sending its owner back to water.
        if (state and state.get('waterReady') and state['task'][:4] == spec[:4]
                and state['task'][-2:] == ['field_cycle', field['cycle']]
                and 'watering_round' not in villager):
            villager['watering_round'] = water_round
        if villager.get('pending_output') and state and state['task'][:7] == spec[:7]:
            spec = state['task']
    if (state and state['phase'] in ('fetching_water', 'collecting_water')
            and (state.get('waterSource') or {}).get('kind') == 'well'
            and not any(b['kind'] == 'well' and 'construction' not in b
                        and b['x'] == state['waterSource']['x'] and b['y'] == state['waterSource']['y']
                        for b in landscape['buildings'])):
        state.update(path=[], phase='waiting_water', elapsed=0)
    size, terrain = landscape.get('map_size', 120), landscape.get('terrain', [])
    pen_bounds = (max(0, building['x'] - 3), min(size - 1, building['x'] + 3),
                  max(0, building['y'] - 3), min(size - 1, building['y'] + 3)) if barn_keeper else None
    roads = {(r['x'], r['y']) for r in landscape.get('roads', []) if 'construction' not in r}
    blocked = occupied_tiles(landscape['buildings'] + landscape['map_resources'] + landscape.get('npcs', []))
    fences = barns.fences(landscape['buildings'], size)
    bounds = walking_bounds(town, landscape)
    if farming_job:
        blocked.difference_update((b['x'], b['y']) for b in landscape['buildings'] if b['kind'] == 'acre' and 'construction' not in b)
    work_spot = source if farming_job and source['kind'] == 'acre' else None
    pier = []
    if boating:
        reach = 2 + building.get('level', 1) + 3 + 2 * (economy.fishing_boat_level(building) - 1)
        occupied = occupied_tiles(landscape['buildings'] + landscape.get('npcs', []))
        depths = landscape.get('_spatial_index', SpatialIndex(landscape['map_resources'])).water_depths()
        water = fishing_water(building, [r for r in landscape['map_resources'] if r['kind'] == 'water'], occupied, reach, terrain, depths)
        water_tiles = {(r['x'], r['y']) for r in water}
        pier = retained_pier(state, building, landscape, blocked, terrain)
        if pier is None:
            pier = fishing_pier(building, landscape, blocked, size, terrain, bounds)
        dock = pier[-1] if pier else building
        deck = {(p['x'], p['y']) for p in pier}
        berths = [r for r in water if abs(r['x'] - dock['x']) + abs(r['y'] - dock['y']) == 1
                  and (r['x'], r['y']) not in deck]
        berth = min(berths, key=lambda r: (abs(r['x'] - building['x']) + abs(r['y'] - building['y']), r['y'], r['x'])) if berths else dock
        land_blocked = blocked - deck - {(building['x'], building['y']), (berth['x'], berth['y'])}
        boarding_blocked = land_blocked | ({(berth['x'], berth['y'])} if (dock['x'], dock['y']) != (berth['x'], berth['y']) else set())
        sea_bounds = (max(0, building['x'] - reach), min(size - 1, building['x'] + reach),
                      max(0, building['y'] - reach), min(size - 1, building['y'] + reach))
        sea_blocked = {(x, y) for x in range(sea_bounds[0], sea_bounds[1] + 1)
                       for y in range(sea_bounds[2], sea_bounds[3] + 1) if (x, y) not in water_tiles}
        blocked.difference_update((r['x'], r['y']) for r in water)
        blocked.discard((building['x'], building['y']))
        work_spot = source
    elif villager['job'] == 'fishing':
        # The hut's pier permits walking onto a reachable water tile.
        pier_blocked = blocked - {(building['x'], building['y'])}
        pier_blocked.difference_update((r['x'], r['y']) for r in landscape['map_resources']
                                      if r['kind'] == 'water' and abs(r['x'] - building['x']) + abs(r['y'] - building['y']) <= 3)
        pier = retained_pier(state, building, landscape, blocked, terrain, source)
        if pier is None and abs(source['x'] - building['x']) + abs(source['y'] - building['y']) <= 3:
            deck = route(building, source, pier_blocked, size, terrain, False, bounds, diagonal=False, fences=fences)
            pier = [building, *deck] if deck is not None and len(deck) <= 3 else []
        if pier:
            blocked.discard((building['x'], building['y']))
            blocked.difference_update((p['x'], p['y']) for p in pier)
            work_spot = source
    def travel(start, target, adjacent=True):
        if boating:
            tile = (round(start['x']), round(start['y']))
            afloat = tile in water_tiles and (tile != (dock['x'], dock['y']) or tile == (berth['x'], berth['y']))
            outbound = target is source
            if outbound and afloat:
                return route(start, source, sea_blocked, size, terrain, False, sea_bounds, diagonal=False, fences=fences)
            if outbound:
                approach = route(start, dock, boarding_blocked, size, terrain, False, bounds, roads, fences=fences)
                if approach is not None and (dock['x'], dock['y']) != (berth['x'], berth['y']):
                    approach.append({'x': berth['x'], 'y': berth['y']})
                sailing = route(berth, source, sea_blocked, size, terrain, False, sea_bounds, diagonal=False, fences=fences)
            else:
                approach = route(start, berth, sea_blocked, size, terrain, False, sea_bounds, diagonal=False, fences=fences) if afloat else []
                sailing = [] if afloat else route(start, berth, land_blocked, size, terrain, False, bounds, roads, fences=fences)
            return None if approach is None or sailing is None else approach + sailing
        local_bounds = pen_bounds if barn_keeper and (
            pen_bounds[0] <= start['x'] <= pen_bounds[1]
            and pen_bounds[2] <= start['y'] <= pen_bounds[3]) else bounds
        path = route(start, target, blocked, size, terrain, adjacent, local_bounds, roads, fences=fences)
        if path is not None and butcher_trip and target is animal_target and butcher.get('position'):
            position = butcher['position']
            if (start['x'], start['y']) == (position['x'], position['y']):
                return []
            if (target['x'], target['y']) != (position['x'], position['y']):
                path.append(dict(position))
        if path is not None and farming_job and target is work_spot:
            destination = field_work_spot(source)
            if (start['x'], start['y']) == (destination['x'], destination['y']):
                return []
            if path:
                path.pop()
            path.append(destination)
        return path
    # The reserved animal remains at its saved position if its butcher changes.
    animal_target, animal_adjacent = butcher_trip, True
    if butcher_trip and butcher.get('position'):
        animal_target = {k: round(butcher['position'][k]) for k in ('x', 'y')}
        animal_adjacent = False
    if (not state or state['task'] != spec or state['phase'] in ('waiting', 'waiting_storage')
            or (farming_job and state['phase'] == 'waiting_field' and not farming.claimed(town, source, index))):
        resume_phase = state.get('resume_phase') if state and state['task'] == spec else None
        if villager['job'] == 'woodcutting' and villager.get('pending_output'):
            # Older saves can be waiting on an unreachable or vanished tree
            # with gathered wood. Deliver that cargo before trying more work.
            resume_phase = 'returning'
        position = starting_position(town, landscape, villager, index, blocked)
        if position is None:
            return False, 0
        x, y = round(position['x']), round(position['y'])
        water_ready = bool(state and state['task'] == spec and state.get('waterReady'))
        if farming_job:
            # A filled can covers this farmer's entire farm pass, including
            # field changes and intervening logistics trips.
            water_ready = villager.get('watering_round') == water_round
        state = {'task': spec, 'x': position['x'],
                 'y': position['y'], 'elapsed': 0, 'waterReady': water_ready,
                 'version': (state or {}).get('version', 0), 'facing': 1}
        if resume_phase:
            target = building if resume_phase in ('returning', 'dragging') else animal_target or work_spot or source
            adjacent = animal_adjacent if target is animal_target else not work_spot if target is source else True
            path = [] if resume_phase == 'fetching_water' else travel(state, target, adjacent)
            state.update(path=path or [], phase='waiting' if path is None else resume_phase if path else
                         {'returning': 'depositing', 'dragging': 'working',
                          'walking': 'finding_animal' if butcher_trip else 'working'}.get(resume_phase, 'working'))
            if path is None:
                state['resume_phase'] = resume_phase
            villager['work_animation'] = state
        elif farming_job and villager.get('pending_output') and farming.harvest_room(town, building, source) <= 1e-9:
            # Finish pre-existing harvest cargo, including saves from the old cycle.
            state.update(phase='harvest_ready', farmAction=None)
            state['path'] = []
            villager['work_animation'] = state
        elif farming_job and farming.claimed(town, source, index):
            state.update(path=[], phase='waiting_field')
            villager['work_animation'] = state
        else:
            target = building if butcher_trip and butcher.get('stage') == 'working' else animal_target or work_spot or source
            adjacent = animal_adjacent if target is animal_target else not work_spot
            path = travel(state, target, adjacent)
            # A cardinal step stays inside the two walkable tiles, even from
            # a fractional work position. Recentering first makes farmers
            # walk back across the acre they have just finished.
            direct_departure = (farming_job and source['kind'] == 'acre' and path is not None
                                and (not path or abs(round(path[0]['x']) - x)
                                     + abs(round(path[0]['y']) - y) <= 1))
            if path is not None and (state['x'], state['y']) != (x, y) and not direct_departure:
                path.insert(0, {'x': x, 'y': y})
            arrival = 'finding_animal' if butcher_trip and butcher.get('stage') != 'working' else 'working'
            state.update(path=path or [], phase='waiting' if path is None else 'walking' if path else arrival)
            villager['work_animation'] = state
    if butcher_trip:
        state['butcherAnimal'] = butcher['animal']
        if state['phase'] in ('walking', 'finding_animal') and butcher.get('stage') != 'working':
            state['butcherPickup'] = {k: (state['path'][-1] if state['path'] else state)[k] for k in ('x', 'y')}
        if state['phase'] == 'working':
            butcher['stage'] = 'working'
    if villager['job'] == 'fishing':
        state['pier'] = [{'x': p['x'], 'y': p['y']} for p in pier or []]
    state['version'] += 1
    remaining = seconds
    finished = False
    state['playback'] = [_sample(state, 0, source)]
    last_phase, last_length, last_action = state['phase'], len(state['path']), state.get('farmAction')
    while remaining > 1e-9:
        if state['phase'] != last_phase or len(state['path']) != last_length or state.get('farmAction') != last_action:
            state['playback'].append(_sample(state, seconds - remaining, source))
            last_phase, last_length, last_action = state['phase'], len(state['path']), state.get('farmAction')
        if state['path']:
            target = state['path'][0]
            tile_x, tile_y = round(target['x']), round(target['y'])
            if ((tile_x, tile_y) in blocked
                    or barns.crosses((state['x'], state['y']), (target['x'], target['y']), fences)
                    or not (0 <= target['x'] < size and 0 <= target['y'] < size)
                    or (terrain and (terrain[tile_y][tile_x] == 'v'
                                     or (bounds and not terrain[tile_y][tile_x])))):
                state.update(path=[], resume_phase=state['phase'], phase='waiting', elapsed=0)
                return finished, 0
            dx, dy = target['x'] - state['x'], target['y'] - state['y']
            distance = math.hypot(dx, dy)
            speed = 1.8 * (1.5 if (math.floor(state['x'] + .5), math.floor(state['y'] + .5)) in roads else 1)
            if state['phase'] == 'dragging':
                speed *= .65
            # Match the browser's small movement steps and road boundaries.
            movement = min(distance, .05, remaining * speed)
            remaining -= movement / speed
            if dx != dy:
                state['facing'] = 1 if dx - dy > 0 else -1
            if distance <= movement + 1e-9:
                state.update(x=target['x'], y=target['y'])
                state['path'].pop(0)
            else:
                state['x'] += dx / distance * movement
                state['y'] += dy / distance * movement
            if butcher_trip and state['phase'] == 'dragging':
                butcher['position'] = {k: state[k] for k in ('x', 'y')}
            if not state['path']:
                phase = state['phase']
                state['phase'] = {'returning': 'depositing', 'fetching_water': 'collecting_water'}.get(phase, 'working')
                if butcher_trip:
                    if phase == 'walking' and butcher.get('stage') != 'working':
                        state['phase'] = 'finding_animal'
                    elif phase == 'dragging':
                        butcher['stage'] = 'working'
                state['elapsed'] = 0
                if phase == 'returning_water':
                    state['waterReady'] = True
                    state['elapsed'] = field['progress'] if farming_job else 0
            continue
        phase = state['phase']
        if phase == 'waiting':
            # Retry a blocked route on the next tick, rather than looping with
            # unchanged remaining time while the host holds the world lock.
            return finished, 0
        if farming_job and phase not in ('depositing', 'returning', 'collecting_water'):
            # Keep harvesting while another ripe acre fits the reserved farm
            # allowance. Once returning, finish the trip before accepting work.
            if (villager.get('pending_output') and not finished
                    and farming.harvest_room(town, building, source) <= 1e-9):
                path = [] if source is building else travel(state, building)
                state.update(path=path or [], phase='waiting' if path is None else 'returning' if path else 'depositing',
                             elapsed=0, farmAction=None)
                continue
            if farming.claimed(town, source, index):
                state.update(phase='waiting_field', farmAction=None)
                return finished, 0
            if field['stage'] in ('sprouting', 'growing'):
                state.update(phase='growing', elapsed=0, farmAction=None)
                return finished, 0
            if field['stage'] == 'plowed' and not farming.can_sow(town, building, field['cycle']):
                state.update(phase='waiting_plowing', elapsed=0, farmAction=None)
                return finished, 0
            cycle, stage = farming.work_pass(town, building)
            carrying_water = field['stage'] == 'thirsty' and state['waterReady']
            continuing_harvest = field['stage'] == 'ripe' and villager.get('pending_output')
            if not (carrying_water or continuing_harvest) and (field['cycle'] != cycle or field['stage'] != stage):
                state.update(phase='waiting_pass', elapsed=0, farmAction=None)
                return finished, 0
            if field['stage'] == 'thirsty' and not state['waterReady']:
                banks = {}
                for water in economy.water_sources(landscape):
                    for dx, dy in ((1, 0), (0, 1), (-1, 0), (0, -1)):
                        bank = {'x': water['x'] + dx, 'y': water['y'] + dy}
                        if (bank['x'], bank['y']) not in blocked:
                            banks[(bank['x'], bank['y'])] = water
                path = travel(state, set(banks), False) if banks else None
                if path is None:
                    state.update(phase='waiting_water', farmAction=None)
                    return finished, 0
                bank = path[-1] if path else state
                water = banks[(round(bank['x']), round(bank['y']))]
                state.update(path=path, waterSource=water, phase='fetching_water' if path else 'collecting_water',
                             elapsed=0, farmAction=None)
                continue
            if work_spot is not None:
                destination = field_work_spot(source)
                if math.hypot(state['x'] - destination['x'], state['y'] - destination['y']) > 1e-9:
                    path = travel(state, work_spot, False)
                    state.update(path=path or [], phase='waiting' if path is None else 'walking',
                                 elapsed=0, farmAction=None)
                    continue
            state.update(phase='working', farmAction=farming.ACTIONS[field['stage']], elapsed=field['progress'])
            if state.get('farmAction') != last_action or state['phase'] != last_phase:
                state['playback'].append(_sample(state, seconds - remaining, source))
                last_phase, last_action = state['phase'], state.get('farmAction')
            if field['stage'] == 'ripe' and field['progress'] >= farming.WORK_SECONDS:
                state.update(phase='harvest_ready', farmAction=None)
                state['playback'].append(_sample(state, seconds - remaining, source))
                return finished, remaining
            phase = state['phase']
        if phase == 'waiting_water':
            return finished, 0
        if butcher_trip and phase == 'finding_animal':
            take = min(remaining, max(0, 1.2 - state['elapsed']))
            state['elapsed'] += take
            remaining -= take
            if state['elapsed'] + 1e-9 < 1.2:
                break
            path = travel(state, building)
            butcher.update(stage='working' if path == [] else 'dragging',
                           position={k: state[k] for k in ('x', 'y')})
            state.update(path=path or [], phase='waiting' if path is None else 'dragging' if path else 'working', elapsed=0)
            if path is None:
                state['resume_phase'] = 'dragging'
            continue
        if phase == 'working' and building.get('butchering'):
            # The animal's shared processing timer keeps the butcher at the shop.
            state['elapsed'] += remaining
            return finished, 0
        duration = .65 if phase in ('depositing', 'collecting_water') else farming.WORK_SECONDS if farming_job else 3.2
        take = min(remaining, max(0, duration - state['elapsed']))
        state['elapsed'] += take
        remaining -= take
        if farming_job and phase == 'working':
            field['progress'] = state['elapsed']
            economy.practice(villager, 'farming', take)
        if state['elapsed'] + 1e-9 < duration:
            break
        if phase == 'depositing':
            if stop_at_delivery:
                state.update(phase='idle', elapsed=0, farmAction=None)
                return True, remaining
            path = travel(state, work_spot or source, not work_spot)
            if barn_keeper and any(building.get('animals', {}).values()):
                # Alternate short feeding rounds with unloading beside the barn.
                spots = [(2, 0), (0, 2), (-2, 0), (0, -2)]
                cursor = state.get('barn_round', index)
                for step in range(len(spots)):
                    dx, dy = spots[(cursor + step) % len(spots)]
                    target = {'x': building['x'] + dx, 'y': building['y'] + dy}
                    round_path = route(state, target, blocked, size, terrain, False,
                                       pen_bounds, roads, fences=fences)
                    if round_path:
                        path = round_path
                        state['barn_round'] = (cursor + step + 1) % len(spots)
                        break
            state.update(path=path or [], phase='waiting' if path is None else 'walking' if path else 'working', elapsed=0, waterReady=False)
            # Report the delivery to the caller without discarding the rest of
            # this tick. Otherwise playback inserts up to three seconds of
            # standing still before the next trip starts.
            finished = True
            continue
        if phase == 'collecting_water':
            path = travel(state, work_spot or source, not work_spot)
            state.update(path=path or [], phase='waiting' if path is None else 'returning_water' if path else 'working', elapsed=0)
            state['waterReady'] = True
            if farming_job:
                villager['watering_round'] = water_round
        elif farming_job:
            if field['stage'] == 'ripe':
                state.update(phase='harvest_ready', farmAction=None)
            else:
                farming.complete_action(source)
                state.update(elapsed=0, farmAction=None,
                             phase='growing' if field['stage'] in ('sprouting', 'growing') else
                             'idle' if farming.can_sow(town, building, field['cycle']) else 'waiting_plowing')
            state['playback'].append(_sample(state, seconds - remaining, source))
            return finished, remaining
        else:
            if boating:
                load = boat_load(town, building)
                full = load >= economy.fishing_boat_capacity(building) - 1e-9
                if not load or (not full and not villager.get('boat_return_requested') and source.get('supply', 1) > 0):
                    state['elapsed'] = 0
                    continue
            elif villager['job'] == 'fishing' and not any(villager.get('pending_output', {}).values()):
                state['elapsed'] = 0
                continue
            path = [] if source is building and not barn_keeper else travel(state, building)
            state.update(path=path or [], phase='waiting' if path is None else 'returning' if path else 'depositing', elapsed=0)
    return finished, 0
