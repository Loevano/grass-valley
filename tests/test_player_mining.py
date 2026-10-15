import copy
import json
import unittest

from fixtures import flat_town
import progression as economy
from server import command, prepare_landscapes, walkable
from shared_world import bind_map, export_world


class PlayerMining(unittest.TestCase):
    def setUp(self):
        self.town = flat_town()
        self.town['map_resources'] = []
        self.town['player'].update(x=5, y=7)
        economy.ensure_inventory(self.town)

    def test_clears_only_nearest_grassland_rock_and_stores_ten_stone(self):
        for biome in 'gm':
            for ore in (None, 'iron'):
                with self.subTest(biome=biome, ore=ore):
                    self.town['player'].update(x=5.25, y=7)
                    self.town['terrain'] = [biome * 16] * 16
                    near = {'kind': 'rock', 'x': 6, 'y': 7, 'supply': 60}
                    if ore:
                        near['ore'] = ore
                    diagonal = {'kind': 'rock', 'x': 6, 'y': 8}
                    self.town['map_resources'] = [diagonal, near]
                    self.town['player']['activity'] = {'job': 'fishing'}
                    before = copy.deepcopy(self.town)
                    self.assertFalse(walkable(self.town, 6, 7))
                    result = command(self.town, {'action': 'mine'})
                    self.assertEqual(self.town['map_resources'], [diagonal])
                    self.assertTrue(walkable(self.town, 6, 7))
                    self.assertNotIn('activity', result['player'])
                    self.assertEqual(economy.quantity(self.town, 'stone'),
                                     economy.quantity(before, 'stone') + 10)
                    self.assertEqual(self.town['buildings'][0]['stored_goods']['stone'],
                                     economy.quantity(self.town, 'stone'))
                    for key in before.keys() - {'player', 'map_resources', 'goods', 'buildings', 'produced'}:
                        self.assertEqual(self.town[key], before[key], key)
                    command(self.town, {'action': 'mine'})
                    self.assertEqual(self.town['map_resources'], [])

    def test_full_storage_or_production_cap_keeps_rock_and_stock_unchanged(self):
        rock = {'kind': 'rock', 'x': 6, 'y': 7}
        self.town['map_resources'] = [rock]
        for cap in (0, 9):
            self.town['rules'] = {'production_caps': {'stone': cap}}
            before = copy.deepcopy(self.town)
            with self.assertRaisesRegex(ValueError, 'production cap'):
                command(self.town, {'action': 'mine'})
            self.assertEqual(self.town, before)
        self.town.pop('rules')
        economy.deliver(self.town, 'stone', 95, 200)
        before = copy.deepcopy(self.town)
        with self.assertRaisesRegex(ValueError, 'storage room'):
            command(self.town, {'action': 'mine'})
        self.assertEqual(self.town, before)

    def test_rejects_distant_stones_other_biomes_and_other_resources(self):
        for biome, kind, x in [('g', 'rock', 8), ('m', 'tree', 6)] + [
                (biome, 'rock', 6) for biome in 'fbsrdiv']:
            with self.subTest(biome=biome, kind=kind, x=x):
                self.town['terrain'] = [biome * 16] * 16
                self.town['map_resources'] = [{'kind': kind, 'x': x, 'y': 7}]
                before = copy.deepcopy(self.town)
                with self.assertRaisesRegex(ValueError, 'grassland stone'):
                    command(self.town, {'action': 'mine'})
                self.assertEqual(self.town, before)

    def test_shared_removal_survives_save_reload_and_regeneration(self):
        rock = {'kind': 'rock', 'x': 6, 'y': 7}
        landscape = {key: copy.deepcopy(self.town[key]) for key in
                     ('terrain', 'map_size', 'terrain_seed', 'tree_stumps', 'npcs')}
        landscape.update(map_resources=[rock], mineral_version=1, industry_version=2)
        world = {'towns': {'own': self.town, 'friend': flat_town('Friend')},
                 'shared_map': landscape}
        bind_map(world)
        command(self.town, {'action': 'mine'}, world)
        self.assertEqual(world['towns']['friend']['map_resources'], [])
        restored = json.loads(json.dumps(export_world(world)))
        bind_map(restored)
        for _ in range(3):
            prepare_landscapes(restored, regenerate=True)
        self.assertFalse(any(r['kind'] == 'rock' for r in restored['shared_map']['map_resources']))
        self.assertTrue(walkable(restored['towns']['own'], 6, 7))
        self.assertEqual(economy.quantity(restored['towns']['own'], 'stone'), 10)
        self.assertEqual(economy.quantity(restored['towns']['friend'], 'stone'), 0)


if __name__ == '__main__':
    unittest.main()
