import copy
import json
import unittest

from server import command, ensure_player, new_town, walkable
from shared_world import bind_map, export_world, new_world


class Equipment(unittest.TestCase):
    def setUp(self):
        self.town = new_town('Explorer')
        self.town.update(terrain=['g' * 120] * 120, map_resources=[], coins=50,
                         npcs=[{'name': 'Tobin', 'x': 5, 'y': 8}])

    def action(self, action, item='swamp_boots', **extra):
        return command(self.town, {'action': action, 'item': item, 'npc_index': 0, **extra})

    def reject(self, action, item='swamp_boots', **extra):
        before = copy.deepcopy(self.town)
        with self.assertRaises(ValueError):
            self.action(action, item, **extra)
        self.assertEqual(self.town, before)

    def test_buy_equip_sell_and_validate_atomic_rejections(self):
        self.reject('equip_item')
        self.reject('buy_equipment', 'unknown')
        self.reject('buy_equipment', npc_index=True)
        self.town['player']['x'] = 20
        self.reject('buy_equipment')
        self.town['player']['x'] = 5
        self.town['coins'] = 14
        self.reject('buy_equipment')
        self.town['coins'] = 50
        self.action('buy_equipment')
        self.assertEqual(self.town['coins'], 35)
        self.action('equip_item')
        self.reject('sell_equipment')
        self.action('buy_equipment')
        self.action('sell_equipment')
        self.assertEqual(self.town['coins'], 27)
        self.action('unequip_item')
        self.action('sell_equipment')
        self.assertEqual(self.town['coins'], 34)
        self.reject('equip_item')
        self.reject('sell_equipment')

    def test_biome_access_requires_equipping_and_keeps_boat_rules(self):
        for biome, key in [('b', 'swamp_boots'), ('s', 'desert_clothing')]:
            self.town['terrain'][7] = 'g' * 6 + biome + 'g' * 113
            self.assertFalse(walkable(self.town, 6, 7))
            self.action('buy_equipment', key)
            self.assertFalse(walkable(self.town, 6, 7))
            self.action('equip_item', key)
            self.assertTrue(walkable(self.town, 6, 7))
            self.town['player'].update(x=5.4, y=7)
            self.reject('unequip_item', key)
            self.town['player'].update(x=5, y=7)
            self.action('unequip_item', key)
        self.town['map_resources'] = [{'kind': 'water', 'x': 6, 'y': 7}]
        self.assertFalse(walkable(self.town, 6, 7))
        self.town['player']['boat'] = True
        self.assertTrue(walkable(self.town, 6, 7))
        self.town['map_resources'].append({'kind': 'rock', 'x': 6, 'y': 7})
        self.assertFalse(walkable(self.town, 6, 7))

    def test_legacy_player_on_hazard_is_moved_to_safe_ground(self):
        self.town['terrain'][7] = 'g' * 5 + 'b' + 'g' * 114
        ensure_player(self.town)
        self.assertTrue(walkable(self.town, self.town['player']['x'], self.town['player']['y']))
        self.assertNotEqual(self.town['player']['x'], 5)

    def test_treasure_is_once_shared_but_gear_is_personal_and_saved(self):
        world = new_world(42)
        a, b = new_town('A', world), new_town('B', world)
        world['towns'].update(a=a, b=b)
        chest = next(r for r in world['shared_map']['map_resources'] if r['kind'] == 'chest')
        # Equip for the sandy island before collecting its reward.
        a['player']['items'] = {'desert_clothing': 1}
        a['player']['equipment'] = {'body': 'desert_clothing'}
        a['player'].update(x=chest['x']-1, y=chest['y'])
        before = sum(a['player']['items'].values())
        command(a, {'action': 'gather'}, world)
        self.assertEqual(sum(a['player']['items'].values()), before + 1)
        self.assertNotIn('items', b['player'])
        self.assertNotIn(chest, world['shared_map']['map_resources'])
        saved = json.loads(json.dumps(export_world(world)))
        bind_map(saved)
        self.assertEqual(saved['towns']['a']['player'], a['player'])
        self.assertNotIn('items', saved['towns']['b']['player'])
