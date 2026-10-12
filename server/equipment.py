"""Player clothing, exploration equipment, and local trading."""
import math

from terrain import ground_variant


ITEMS = {
    'swamp_boots': {'name': 'Swamp boots', 'slot': 'feet', 'biome': 'b', 'price': 15,
                    'gather_good': 'herbs', 'gather_bonus': .2},
    'desert_clothing': {'name': 'Desert clothing', 'slot': 'body', 'price': 20,
                        'gather_good': 'berries', 'gather_bonus': .2},
}


def trader_items(npc):
    if npc.get('name') == 'Dara' or npc.get('village') == 'Sunwell':
        return ('desert_clothing',)
    if npc.get('name') == 'Petra' or npc.get('village') == 'Reedhaven':
        return ('swamp_boots',)
    return ()


def gathering_bonus(player, good):
    return sum(item['gather_bonus'] for key, item in ITEMS.items()
               if item['gather_good'] == good
               and player.get('equipment', {}).get(item['slot']) == key)


def protected(player, biome, x, y):
    if biome == 'b' and ground_variant(x, y) != 2:
        return True
    return all(player.get('equipment', {}).get(item['slot']) == key
               for key, item in ITEMS.items() if 'biome' in item and item['biome'] == biome)


def find_item(player, chest):
    key = list(ITEMS)[(chest['x'] + chest['y']) % len(ITEMS)]
    player.setdefault('items', {})[key] = player.get('items', {}).get(key, 0) + 1
    return ITEMS[key]['name']


def change(town, data):
    action, key = data['action'], data.get('item')
    if not isinstance(key, str) or key not in ITEMS:
        raise ValueError('Choose a known equipment item.')
    player, item = town['player'], ITEMS[key]
    count = player.get('items', {}).get(key, 0)
    equipped = player.get('equipment', {}).get(item['slot']) == key
    if action in {'buy_equipment', 'sell_equipment'}:
        index = data.get('npc_index')
        locals_ = town.get('npcs', [])
        if type(index) is not int or not 0 <= index < len(locals_):
            raise ValueError('Choose a local trader.')
        trader = locals_[index]
        if key not in trader_items(trader):
            raise ValueError('This local does not trade that clothing.')
        position = trader.get('routine', trader)
        x, y = position['x'], position['y']
        if math.hypot(player['x'] - x, player['y'] - y) > 1.8:
            raise ValueError('Walk beside the local to trade.')
        if action == 'buy_equipment':
            if town['coins'] < item['price']:
                raise ValueError('Not enough coins for this item.')
            town['coins'] -= item['price']
            player.setdefault('items', {})[key] = count + 1
        else:
            if count <= int(equipped):
                raise ValueError('Unequip the item before selling it.')
            player['items'][key] = count - 1
            town['coins'] += item['price'] // 2
    elif action == 'equip_item':
        if count < 1:
            raise ValueError('Find or buy this item first.')
        player.setdefault('equipment', {})[item['slot']] = key
    else:
        if not equipped:
            raise ValueError('This item is not equipped.')
        # Keep the character safely equipped while any part touches hazardous ground.
        terrain = town.get('terrain', [])
        for y in range(math.floor(player['y'] + .5) - 1, math.floor(player['y'] + .5) + 2):
            for x in range(math.floor(player['x'] + .5) - 1, math.floor(player['x'] + .5) + 2):
                if (0 <= y < len(terrain) and 0 <= x < len(terrain[y])
                        and terrain[y][x] == item.get('biome')
                        and (terrain[y][x] != 'b' or ground_variant(x, y) == 2)
                        and abs(player['x'] - x) < .68 and abs(player['y'] - y) < .68):
                    raise ValueError('Return to safe ground before removing this item.')
        del player['equipment'][item['slot']]
    return {'player': dict(player)}
