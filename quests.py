"""Saved cooperative quarry puzzle; positions are separate from the valley."""

LAYOUT = [
    '###########',
    '#....#....#',
    '#.P..#.SLM#',
    '#....G....#',
    '#....#.I.T#',
    '#....#....#',
    '###########',
]
SEQUENCE = ['sun', 'leaf', 'moon']
RUNES = {'sun': (7, 2), 'leaf': (8, 2), 'moon': (9, 2)}
KEEPSAKE = 'Quarry lantern stone'
REWARD = 20
CATALOG = {'title': 'The Sealed Quarry', 'layout': LAYOUT, 'runes': RUNES,
           'reward': REWARD, 'keepsake': KEEPSAKE,
           'clue': 'First comes the dawn. Then the leaf drinks its light. Last, the moon watches over the sleeping quarry.'}


def inside(world, town):
    key = next((key for key, value in world.get('towns', {}).items() if value is town), None)
    return world.get('quarry', {}).get('members', {}).get(key, {}).get('inside', False)


def gate_open(quest):
    return quest['gate_latched'] or any(
        member['inside'] and (member['x'], member['y']) == (2, 2)
        for member in quest['members'].values())


def change(town, data, world):
    if not world:
        raise ValueError('Open a saved world before visiting the quarry.')
    key = next((key for key, value in world['towns'].items() if value is town), None)
    if key is None:
        raise ValueError('Join this world first.')
    action = data['action']
    quest = world.get('quarry')
    if action == 'join_quarry':
        if quest and quest['completed']:
            raise ValueError('This expedition is complete.')
        if quest is None:
            quest = world['quarry'] = {'members': {}, 'started': False, 'completed': False,
                                       'gate_latched': False, 'sequence': [], 'runes_solved': False,
                                       'message': 'Gather at least two founders to begin.'}
        member = quest['members'].get(key)
        if member and member['inside']:
            raise ValueError('You are already in the quarry.')
        quest['members'][key] = {'x': 1, 'y': 3, 'inside': True}
        town['player'].pop('activity', None)
        town['player'].pop('activity_status', None)
        if sum(m['inside'] for m in quest['members'].values()) >= 2 and not quest['started']:
            quest['started'] = True
            quest['message'] = 'The expedition begins. The floor plate is linked to the iron gate.'
        return {}
    member = quest and quest['members'].get(key)
    if not member or not member['inside']:
        raise ValueError('Enter the quarry first.')
    if action == 'leave_quarry':
        member['inside'] = False
        if quest['started'] and not quest['gate_latched'] and sum(m['inside'] for m in quest['members'].values()) < 2:
            # A departing companion must never strand the remaining player.
            quest['gate_latched'] = True
            quest['message'] = 'A companion leaves. Flint’s safety counterweight secures the gate open.'
        return {}
    if quest['completed']:
        raise ValueError('The quarry lantern has already been recovered.')
    if not quest['started']:
        raise ValueError('Wait for a second founder to enter.')
    if action == 'quarry_walk':
        dx, dy = data.get('dx'), data.get('dy')
        if (type(dx) is not int or type(dy) is not int
                or (dx, dy) not in ((0, 1), (0, -1), (1, 0), (-1, 0))):
            raise ValueError('Choose one tile up, down, left, or right.')
        x, y = member['x'] + dx, member['y'] + dy
        if not (0 <= y < len(LAYOUT) and 0 <= x < len(LAYOUT[0])) or LAYOUT[y][x] == '#':
            raise ValueError('The quarry wall blocks your path.')
        if LAYOUT[y][x] == 'G' and not gate_open(quest):
            raise ValueError('Ask a companion to stand on the floor plate.')
        member.update(x=x, y=y)
        if x == 6 and not quest['gate_latched']:
            quest['gate_latched'] = True
            quest['message'] = 'You secure the gate’s catch. Both companions can now cross safely.'
        return {}
    target = data.get('target')
    position = RUNES.get(target) if isinstance(target, str) else None
    if target == 'inscription':
        position = (7, 4)
    elif target == 'treasure':
        position = (9, 4)
    if position is None:
        raise ValueError('Choose a quarry object.')
    if abs(member['x'] - position[0]) + abs(member['y'] - position[1]) > 1:
        raise ValueError('Walk beside this object first.')
    if target == 'inscription':
        quest['message'] = CATALOG['clue']
    elif target == 'treasure':
        if not quest['runes_solved']:
            raise ValueError('Three carved runes seal the lantern chest.')
        quest['completed'] = True
        quest['message'] = 'The lantern stone glows again. Every expedition member receives their own reward.'
        for town_id in quest['members']:
            recipient = world['towns'][town_id]
            recipient['coins'] += REWARD
            recipient['player'].setdefault('keepsakes', []).append(KEEPSAKE)
    elif quest['runes_solved']:
        raise ValueError('The rune seal is already open.')
    else:
        if target == SEQUENCE[len(quest['sequence'])]:
            quest['sequence'].append(target)
            quest['runes_solved'] = len(quest['sequence']) == len(SEQUENCE)
            quest['message'] = ('The rune seal opens. Recover the lantern stone from the chest.'
                                if quest['runes_solved'] else 'A rune glows. The next carving waits.')
        else:
            quest['sequence'] = []
            quest['message'] = 'The carvings go dark. Read the inscription and try again; nothing is lost.'
    return {}
