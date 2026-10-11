"""Host-owned workplace animation and drop-off timing."""
from collections import deque
import math

import progression as economy


def fishing_water(building, water, occupied, reach, terrain=None):
    """Water reachable by boat from the hut's nearest bank, within its reach."""
    tiles = {(r['x'], r['y']): r for r in water
             if (r['x'], r['y']) not in occupied
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


def route(start, target, blocked, size, terrain, adjacent=True, bounds=None):
    start = (round(start['x']), round(start['y']))
    queue, previous = deque([start]), {start: None}
    while queue:
        tile = queue.popleft()
        reached = tile in target if isinstance(target, set) else abs(tile[0] - target['x']) + abs(tile[1] - target['y']) == int(adjacent)
        if reached:
            path = []
            while previous[tile] is not None:
                path.append({'x': tile[0], 'y': tile[1]})
                tile = previous[tile]
            return list(reversed(path))
        x, y = tile
        for next_tile in ((x + 1, y), (x, y + 1), (x - 1, y), (x, y - 1)):
            nx, ny = next_tile
            if bounds and not (bounds[0] <= nx <= bounds[1] and bounds[2] <= ny <= bounds[3]):
                continue
            if (0 <= nx < size and 0 <= ny < size and next_tile not in blocked
                    and next_tile not in previous and (not terrain or terrain[ny][nx] != 'v')
                    and (not bounds or terrain[ny][nx])):
                previous[next_tile] = tile
                queue.append(next_tile)
    return None


def advance(town, landscape, villager, building, source, index, seconds):
    """Return True only when the worker finishes unloading at its workplace."""
    spec = [villager['job'], building['kind'], building['x'], building['y'], source['kind'], source['x'], source['y']]
    if villager['job'] == 'fishing':
        spec.extend([building.get('level', 1), economy.fishing_boat_level(building)])
    state = villager.get('work_animation')
    size, terrain = landscape.get('map_size', 120), landscape.get('terrain', [])
    blocked = {(b['x'], b['y']) for b in landscape['buildings'] + landscape['map_resources'] + landscape.get('npcs', [])}
    farming = villager['job'] == 'farming'
    hall = next(b for b in town['buildings'] if b['kind'] == 'hall')
    center = town.get('town_origin', hall)
    bounds = (max(0, center['x'] - 60), min(size - 1, center['x'] + 60),
              max(0, center['y'] - 60), min(size - 1, center['y'] + 60)) if landscape.get('chunked') else None
    if farming:
        blocked.difference_update((b['x'], b['y']) for b in landscape['buildings'] if b['kind'] == 'acre' and 'construction' not in b)
    work_spot = source if farming and source['kind'] == 'acre' else None
    boating = villager['job'] == 'fishing' and economy.fishing_boat_level(building)
    if boating:
        reach = 2 + building.get('level', 1) + 3 + 2 * (economy.fishing_boat_level(building) - 1)
        occupied = {(b['x'], b['y']) for b in landscape['buildings'] + landscape.get('npcs', [])}
        water = fishing_water(building, [r for r in landscape['map_resources'] if r['kind'] == 'water'], occupied, reach, terrain)
        water_tiles = {(r['x'], r['y']) for r in water}
        berth = water[0] if water else source
        land_blocked = blocked - {(building['x'], building['y']), (berth['x'], berth['y'])}
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
        pier = route(building, source, pier_blocked, size, terrain, False, bounds)
        if pier is not None and len(pier) <= 3:
            blocked.discard((building['x'], building['y']))
            blocked.difference_update((p['x'], p['y']) for p in pier)
            work_spot = source
    def travel(start, target, adjacent=True):
        if boating:
            afloat = (round(start['x']), round(start['y'])) in water_tiles
            outbound = target is source
            if outbound and afloat:
                return route(start, source, sea_blocked, size, terrain, False, sea_bounds)
            if outbound:
                approach = route(start, berth, land_blocked, size, terrain, False, bounds)
                sailing = route(berth, source, sea_blocked, size, terrain, False, sea_bounds)
            else:
                approach = route(start, berth, sea_blocked, size, terrain, False, sea_bounds) if afloat else []
                sailing = route(berth if afloat else start, building, land_blocked, size, terrain, True, bounds)
            return None if approach is None or sailing is None else approach + sailing
        return route(start, target, blocked, size, terrain, adjacent, bounds)
    if not state or state['task'] != spec or state['phase'] in ('waiting', 'waiting_water'):
        position = (villager.get('logistics_position') or state or villager.get('construction_position')
                    or {'x': hall['x'] - 2 + index % 5, 'y': hall['y'] + 3 + index // 5})
        x, y = round(position['x']), round(position['y'])
        if (x, y) in blocked or not (0 <= x < size and 0 <= y < size) or (terrain and terrain[y][x] == 'v'):
            candidates = ((cx, cy) for cx in range(max(0, x - 60), min(size, x + 61))
                          for cy in range(max(0, y - 60), min(size, y + 61)))
            spot = min((p for p in candidates if p not in blocked and (not terrain or terrain[p[1]][p[0]] != 'v')),
                       key=lambda p: abs(p[0] - x) + abs(p[1] - y), default=None)
            if spot is None:
                return False
            x, y = spot
        state = {'task': spec, 'x': x, 'y': y, 'elapsed': 0, 'waterReady': False,
                 'version': (state or {}).get('version', 0), 'facing': 1}
        path = travel(state, work_spot or source, not work_spot)
        state.update(path=path or [], phase='waiting' if path is None else 'walking' if path else 'working')
        villager['work_animation'] = state
    state['version'] += 1
    roads = {(r['x'], r['y']) for r in landscape.get('roads', []) if 'construction' not in r}
    remaining = seconds
    while remaining > 1e-9:
        if state['path']:
            target = state['path'][0]
            if (target['x'], target['y']) in blocked or (terrain and terrain[target['y']][target['x']] == 'v'):
                villager.pop('work_animation', None)
                return False
            dx, dy = target['x'] - state['x'], target['y'] - state['y']
            distance = math.hypot(dx, dy)
            speed = 1.8 * (1.5 if (math.floor(state['x'] + .5), math.floor(state['y'] + .5)) in roads else 1)
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
            if not state['path']:
                phase = state['phase']
                state['phase'] = {'returning': 'depositing', 'fetching_water': 'collecting_water'}.get(phase, 'working')
                state['elapsed'] = 0
                if phase == 'returning_water':
                    state['waterReady'] = True
                    state['elapsed'] = 4
            continue
        phase = state['phase']
        if phase in ('waiting', 'waiting_water'):
            return False
        duration = .65 if phase in ('depositing', 'collecting_water') else 4 if farming and not state['waterReady'] else 8 if farming else 3.2
        take = min(remaining, max(0, duration - state['elapsed']))
        state['elapsed'] += take
        remaining -= take
        if state['elapsed'] + 1e-9 < duration:
            break
        if phase == 'depositing':
            path = travel(state, work_spot or source, not work_spot)
            state.update(path=path or [], phase='waiting' if path is None else 'walking' if path else 'working', elapsed=0, waterReady=False)
            return True
        if phase == 'collecting_water':
            path = travel(state, work_spot or source, not work_spot)
            state.update(path=path or [], phase='waiting_water' if path is None else 'returning_water' if path else 'working', elapsed=4)
            state['waterReady'] = path is not None and not path
        elif farming and not state['waterReady']:
            banks = {}
            for water in landscape['map_resources']:
                if water['kind'] != 'water':
                    continue
                for dx, dy in ((1, 0), (0, 1), (-1, 0), (0, -1)):
                    bank = {'x': water['x'] + dx, 'y': water['y'] + dy}
                    if (bank['x'], bank['y']) not in blocked:
                        banks[(bank['x'], bank['y'])] = water
            path = travel(state, set(banks), False) if banks else None
            if path is None:
                state['phase'] = 'waiting_water'
                return False
            bank = path[-1] if path else state
            water = banks[(round(bank['x']), round(bank['y']))]
            state.update(path=path, waterSource=water, phase='fetching_water' if path else 'collecting_water', elapsed=0)
        else:
            if villager['job'] == 'fishing' and not any(villager.get('pending_output', {}).values()):
                state['elapsed'] = 0
                continue
            path = [] if source is building else travel(state, building)
            state.update(path=path or [], phase='waiting' if path is None else 'returning' if path else 'depositing', elapsed=0)
    return False
