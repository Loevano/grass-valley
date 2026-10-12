"""Host-timed cooperative courier course; valley positions stay untouched."""
import math

LAYOUT = ['#############', '#...........#', '#...........#', '#...........#', '#############']
REWARD = 25
KEEPSAKE = 'Courier lantern badge'
CATALOG = {'title': 'The Falling Lanterns', 'layout': LAYOUT,
           'reward': REWARD, 'keepsake': KEEPSAKE}


def inside(world, town):
    key = next((k for k, t in world.get('towns', {}).items() if t is town), None)
    return bool(world.get('lanterns', {}).get('members', {}).get(key, {}).get('inside'))


def playing(quest):
    return not quest['completed'] and sum(m['inside'] for m in quest['members'].values()) >= 2


def hazards(quest):
    result = []
    for start, end, offset in ((3, 5, 0), (7, 10, 1.2)):
        phase = quest['elapsed'] + offset
        wave = math.floor(phase / 3.4)
        beat = phase % 3.4
        state = 'warning' if beat < 2 else 'strike' if beat < 2.6 else 'clear'
        safe_row = wave % 3 + 1
        if state != 'clear':
            result.extend({'x': x, 'y': y, 'state': state}
                          for x in range(start, end + 1) for y in range(1, 4) if y != safe_row)
    return result


def hit(quest, member):
    if any(h['state'] == 'strike' and (h['x'], h['y']) == (member['x'], member['y'])
           for h in hazards(quest)):
        member.update(x=member['checkpoint'], y=2, hits=member['hits'] + 1)
        quest['message'] = 'Falling debris! Back to your checkpoint; watch the warning tiles and try again.'


def advance(world, seconds):
    quest = world.get('lanterns')
    if not quest or not playing(quest):
        return
    # Small slices preserve collision checks even after a delayed host update.
    remaining = min(max(seconds, 0), .25)
    while remaining > 0:
        step = min(remaining, .05)
        quest['elapsed'] += step
        for member in quest['members'].values():
            if member['inside']:
                hit(quest, member)
        remaining -= step


def snapshot(world):
    quest = world.get('lanterns')
    return dict(quest, hazards=hazards(quest), playing=playing(quest)) if quest else None


def change(town, data, world):
    if not world:
        raise ValueError('Open a saved world first.')
    key = next((k for k, t in world['towns'].items() if t is town), None)
    if key is None:
        raise ValueError('Join this world first.')
    action = data['action']
    quest = world.get('lanterns')
    if action == 'join_lanterns':
        if quest and quest['completed']:
            raise ValueError('The courier satchel has already been recovered.')
        if world.get('quarry', {}).get('members', {}).get(key, {}).get('inside'):
            raise ValueError('Leave the quarry first.')
        if quest is None:
            quest = world['lanterns'] = {'members': {}, 'elapsed': 0, 'completed': False,
                                         'message': 'Two founders must enter the courier path.'}
        member = quest['members'].get(key)
        if member and member['inside']:
            raise ValueError('You are already on the courier path.')
        if member:
            member['inside'] = True
        else:
            quest['members'][key] = {'x': 1, 'y': 2, 'inside': True, 'checkpoint': 1, 'hits': 0}
        town['player'].pop('activity', None)
        town['player'].pop('activity_status', None)
        quest['message'] = 'Recover the satchel together. Amber ! warns of debris; red × means dodge now.'
        return {}
    member = quest and quest['members'].get(key)
    if not member or not member['inside']:
        raise ValueError('Enter the courier path first.')
    if action == 'leave_lanterns':
        member['inside'] = False
        return {}
    if not playing(quest):
        raise ValueError('The course needs two founders inside and an unrecovered satchel.')
    if action == 'lanterns_walk':
        dx, dy = data.get('dx'), data.get('dy')
        if type(dx) is not int or type(dy) is not int or (dx, dy) not in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            raise ValueError('Choose one tile up, down, left, or right.')
        x, y = member['x'] + dx, member['y'] + dy
        if not (1 <= x <= 11 and 1 <= y <= 3):
            raise ValueError('The ruined wall blocks your path.')
        if quest['elapsed'] - member.get('last_step', -1) < .12:
            raise ValueError('Wait a moment before the next step.')
        member.update(x=x, y=y, last_step=quest['elapsed'])
        if x == 6:
            member['checkpoint'] = 6
        hit(quest, member)
        return {}
    if action != 'lanterns_collect':
        raise ValueError('Choose a courier path action.')
    finishers = [m for m in quest['members'].values() if m['inside'] and m['x'] == 11]
    if member['x'] != 11 or len(finishers) < 2:
        raise ValueError('At least two founders must reach the satchel at the far end together.')
    quest['completed'] = True
    quest['message'] = 'The lost satchel is safe! Every expedition member receives a courier lantern badge.'
    for town_id in quest['members']:
        recipient = world['towns'][town_id]
        recipient['coins'] += REWARD
        recipient['player'].setdefault('keepsakes', []).append(KEEPSAKE)
    return {}
