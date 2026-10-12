"""Host-owned ambient routines for wilderness villagers; no player production."""
from collections import deque
import math

from villages import BASE_BUILDINGS, EXTRA_BUILDINGS, LIFE_BUILDINGS, PLANTINGS

SPEED = .65


def work_stop(npc, village):
    role = npc['role']
    if village.get('life_version') != 1:
        return (2, 3), 'idle', None
    if 'woodworker' in role or 'boatwright' in role:
        return (-4, -4), 'woodcutting', (-5, -4)
    if 'fisher' in role:
        return (6, -7), 'fishing', (7, -7)
    if 'herbalist' in role or 'apprentice' in role:
        return (-4, 6), 'gathering', (-5, 6)
    if any(word in role for word in ('keeper', 'warden', 'tender')) and 'innkeeper' not in role:
        return (-6, -7), 'husbandry', (-7, -7)
    if 'innkeeper' in role:
        return (2, 8), 'baking', (3, 8)
    if any(word in role for word in ('weaver', 'dyer', 'mason')):
        return (-2, -7), 'crafting', (-3, -7)
    return (2, 3), 'idle', (3, 3)


def blocked_tiles(world, village):
    vx, vy = village['x'], village['y']
    buildings = BASE_BUILDINGS + (EXTRA_BUILDINGS if village.get('layout_version') == 2 else ())
    blocked = {(vx+dx, vy+dy) for _, dx, dy in buildings}
    if village.get('life_version') == 1:
        blocked.update((vx+dx, vy+dy) for _, dx, dy, _, _ in LIFE_BUILDINGS)
        blocked.update((vx+dx, vy+dy) for _, dx, dy in PLANTINGS)
    # Read only already active chunks. Sleeping village life never loads terrain.
    manager = world.get('_chunks')
    if manager:
        for cy in range((vy-9)//40, (vy+9)//40+1):
            for cx in range((vx-9)//40, (vx+9)//40+1):
                record = manager.records.get(f'{cx},{cy}')
                if record is None:
                    continue
                blocked = {p for p in blocked if not (p[0]//40 == cx and p[1]//40 == cy)}
                blocked.update((r['x'], r['y']) for r in record['map_resources'])
    blocked.update((b['x'], b['y']) for town in world['towns'].values()
                   for b in town['buildings'])
    blocked.update((n['x'], n['y']) for n in world['shared_map']['npcs'] if not n.get('village'))
    return blocked


def route(start, goal, blocked, village):
    radius = 9 if village.get('layout_version') == 2 else 5
    queue = deque([start])
    parents = {start: None}
    while queue:
        x, y = queue.popleft()
        if (x, y) == goal:
            path = []
            while parents[(x, y)] is not None:
                path.append([x, y])
                x, y = parents[(x, y)]
            return path[::-1]
        for dx, dy in ((1, 0), (0, 1), (-1, 0), (0, -1)):
            next_ = x+dx, y+dy
            if (next_ in parents or next_ in blocked
                    or max(abs(next_[0]-village['x']), abs(next_[1]-village['y'])) > radius):
                continue
            parents[next_] = (x, y)
            queue.append(next_)
    return []


def advance(world, seconds):
    landscape = world.get('shared_map', {})
    if seconds <= 0 or not landscape.get('villages'):
        return
    players = [t['player'] for t in world['towns'].values() if 'player' in t]
    for village in landscape['villages']:
        residents = [n for n in landscape['npcs'] if n.get('village') == village['name']]
        blocked = blocked_tiles(world, village)
        for index, npc in enumerate(residents):
            state = npc.setdefault('routine', dict(x=npc['x'], y=npc['y'], path=[],
                stop=-1, wait=index*.8, elapsed=0, phase='idle', job='idle', facing=1,
                origin=[npc['x'], npc['y']]))
            # Locals stop beside visitors so talking and trading remain practical.
            state['paused'] = any(math.hypot(p['x']-state['x'], p['y']-state['y']) <= 1.8 for p in players)
            if state['paused']:
                continue
            remaining = seconds
            transitions = 0
            while remaining > 0 and transitions < 32:
                transitions += 1
                if state['path']:
                    target = state['path'][0]
                    if tuple(target) in blocked:
                        # Finish no further steps until a new safe route is found.
                        origin = tuple(state.get('origin', (npc['x'], npc['y'])))
                        detour = route(origin, tuple(state['goal']), blocked, village)
                        returning = [[*origin]] if (state['x'], state['y']) != origin else []
                        state['path'] = returning + detour
                        if not detour:
                            state['job'], state['source'] = 'idle', None
                        if not state['path']:
                            state['phase'] = 'idle'
                            break
                        target = state['path'][0]
                    distance = math.hypot(target[0]-state['x'], target[1]-state['y'])
                    duration = distance/SPEED
                    used = min(remaining, duration)
                    fraction = used/duration if duration else 1
                    dx, dy = target[0]-state['x'], target[1]-state['y']
                    state['x'] += dx*fraction
                    state['y'] += dy*fraction
                    state['facing'] = 1 if dx-dy >= 0 else -1
                    state['phase'] = 'walking'
                    remaining -= used
                    if used < duration:
                        break
                    state['path'].pop(0)
                    state['origin'] = list(target)
                    if not state['path']:
                        state['phase'] = 'working' if state['job'] != 'idle' and state['stop'] == 0 else 'idle'
                        state['elapsed'] = 0
                    continue
                if state['wait'] > 0:
                    used = min(remaining, state['wait'])
                    state['wait'] -= used
                    state['elapsed'] += used
                    remaining -= used
                    continue
                work, job, source = work_stop(npc, village)
                if source and (village['x']+source[0], village['y']+source[1]) not in blocked:
                    job, source = 'idle', None
                vx, vy = village['x'], village['y']
                hall = (vx-2, vy+8) if village.get('life_version') == 1 else (vx-2, vy-2)
                stops = [(vx+work[0], vy+work[1]), (vx+2, vy+3), hall, (npc['x'], npc['y'])]
                state['stop'] = (state['stop']+1) % len(stops)
                state['goal'] = list(stops[state['stop']])
                state['job'] = job if state['stop'] == 0 else 'idle'
                state['source'] = dict(x=vx+source[0], y=vy+source[1]) if source and state['stop'] == 0 else None
                state['wait'] = (8, 4, 3, 5)[state['stop']]
                state['elapsed'] = 0
                state['path'] = route((round(state['x']), round(state['y'])), tuple(state['goal']), blocked, village)
                state['phase'] = 'walking' if state['path'] else 'idle'
            state['x'], state['y'] = round(state['x'], 6), round(state['y'], 6)
