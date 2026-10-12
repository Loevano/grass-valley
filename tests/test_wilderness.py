import copy
import json
import unittest
from math import ceil
from collections import deque

import progression as economy
from server import command, new_town, prepare_landscapes, tick, walkable
from shared_world import bind_map, export_world, new_world


class Wilderness(unittest.TestCase):
    def landscape(self, biome='f', species=None):
        tree = {'kind': 'tree', 'x': 2, 'y': 2}
        if species:
            tree['species'] = species
        return {'terrain': [biome * 5] * 5, 'map_resources': [tree]}, tree

    def test_felling_leaves_stumps_and_regrows_on_every_ground(self):
        for biome in 'fgmsrb':
            landscape, tree = self.landscape(biome)
            maximum = economy.source_limits(landscape, tree)[0]
            self.assertEqual(economy.take_supply(landscape, tree, maximum), maximum)
            self.assertEqual(landscape['map_resources'], [])
            self.assertEqual(economy.take_supply(landscape, tree, 1), 0)
            saved = json.loads(json.dumps(landscape))
            self.assertEqual(len(saved['tree_stumps']), 1)
            seconds = saved['tree_stumps'][0]['remaining']
            for _ in range(ceil(seconds / 2.7) - 1):
                economy.renew(saved)
            self.assertEqual(saved['map_resources'], [])
            economy.renew(saved)
            self.assertEqual(saved['map_resources'][0]['supply'], maximum)
            self.assertEqual(saved['tree_stumps'], [])

    def test_buildings_roads_and_occupants_pause_regrowth(self):
        landscape, tree = self.landscape()
        economy.take_supply(landscape, tree, 8)
        for kind in ('home', 'acre', 'road', 'player'):
            for _ in range(101):
                economy.renew(landscape, [{'kind': kind, 'x': 2, 'y': 2}])
            self.assertEqual(landscape['tree_stumps'][0]['remaining'], 300)
        for _ in range(112):
            economy.renew(landscape)
        self.assertEqual(len(landscape['map_resources']), 1)

    def test_existing_stumps_regrow_ten_percent_slower(self):
        landscape = {'map_resources': [], 'tree_stumps': [
            {'kind': 'tree', 'x': 2, 'y': 2, 'remaining': 27}]}
        for _ in range(9):
            economy.renew(landscape)
        self.assertEqual(landscape['map_resources'], [])
        self.assertAlmostEqual(landscape['tree_stumps'][0]['remaining'], 2.7)
        economy.renew(landscape)
        self.assertEqual(len(landscape['map_resources']), 1)

    def test_special_trees_take_five_times_the_work_and_never_refill_while_cutting(self):
        for biome, species in [('b', None), ('f', 'ancient')]:
            landscape, tree = self.landscape(biome, species)
            economy.renew(landscape)
            economy.take_supply(landscape, tree, 8)
            for _ in range(20):
                economy.renew(landscape)
            self.assertEqual(tree['supply'], 32)
            economy.take_supply(landscape, tree, 32)
            self.assertEqual(landscape['tree_stumps'][0]['remaining'], 900)

    def test_shared_building_blocks_saved_regrowth(self):
        world = new_world(42)
        town = new_town('Woods', world)
        world['towns']['a'] = town
        tree = next(r for r in town['map_resources'] if r['kind'] == 'tree' and town['terrain'][r['y']][r['x']] == 'f')
        economy.take_supply(world['shared_map'], tree, 100)
        town['buildings'].append({'kind': 'home', 'x': tree['x'], 'y': tree['y']})
        saved = json.loads(json.dumps(export_world(world)))
        bind_map(saved)
        prepare_landscapes(saved, regenerate=True)
        self.assertEqual(saved['shared_map']['tree_stumps'][0]['remaining'], 300 if tree.get('species') != 'ancient' else 900)
        self.assertIs(saved['towns']['a']['tree_stumps'], saved['shared_map']['tree_stumps'])

    def test_boat_requires_level_materials_and_preserves_collision(self):
        town = new_town('Sailor')
        town.update(npcs=[], map_resources=[{'kind': 'water', 'x': 6, 'y': 7}], terrain=['g' * 120] * 120)
        for level, planks in [(2, 10), (3, 9)]:
            town.update(level=level, goods={'planks': planks})
            before = copy.deepcopy(town)
            with self.assertRaises(ValueError):
                command(town, {'action': 'build_boat'})
            self.assertEqual(town, before)
        town.update(level=3, goods={'planks': 10})
        wood = town['wood']
        self.assertFalse(walkable(town, 6, 7))
        command(town, {'action': 'build_boat'})
        self.assertEqual(town['wood'], wood - 20)
        self.assertEqual(town['goods']['planks'], 0)
        self.assertTrue(walkable(town, 6, 7))
        command(town, {'action': 'walk', 'dx': 1, 'dy': 0})
        self.assertGreater(town['player']['x'], 5)
        town['map_resources'].append({'kind': 'rock', 'x': 7, 'y': 7})
        self.assertFalse(walkable(town, 7, 7))
        saved = json.loads(json.dumps(town))
        self.assertTrue(saved['player']['boat'])
        before = copy.deepcopy(town)
        with self.assertRaises(ValueError):
            command(town, {'action': 'build_boat'})
        self.assertEqual(town, before)

    def test_large_lake_islands_require_water_crossing_and_loot_is_shared_once(self):
        for seed in range(12):
            world = new_world(seed)
            landscape = world['shared_map']
            water = {(r['x'], r['y']) for r in landscape['map_resources'] if r['kind'] == 'water'}
            chests = [r for r in landscape['map_resources'] if r['kind'] == 'chest']
            self.assertEqual(len(chests), 3)
            for chest in chests:
                start = (chest['x'], chest['y'])
                seen, queue = {start}, deque([start])
                while queue:
                    x, y = queue.popleft()
                    for p in [(x+1,y), (x-1,y), (x,y+1), (x,y-1)]:
                        if p not in seen and p not in water:
                            seen.add(p)
                            queue.append(p)
                    self.assertLess(len(seen), 30, 'Island must be enclosed by water')
                self.assertGreater(len(water), 500)
            town = new_town('Explorer', world)
            world['towns']['a'] = town
            chest = chests[0]
            town['player'].update(x=chest['x']-1, y=chest['y'])
            coins = town['coins']
            command(town, {'action': 'gather'}, world)
            self.assertEqual(town['coins'], coins + 25)
            self.assertNotIn(chest, landscape['map_resources'])
            friend = new_town('Friend', world)
            world['towns']['b'] = friend
            friend['player'].update(x=chest['x']-1, y=chest['y'])
            with self.assertRaises(ValueError):
                command(friend, {'action': 'gather'}, world)
            saved = json.loads(json.dumps(export_world(world)))
            self.assertNotIn(chest, saved['shared_map']['map_resources'])
            self.assertEqual(town['coins'], coins + 25)

    def test_players_and_settlers_both_fell_trees(self):
        for worker in (False, True):
            town = new_town('Woodland')
            tree = {'kind': 'tree', 'x': 6, 'y': 7}
            town.update(map_resources=[tree], terrain=['f' * 120] * 120,
                        npcs=[], forage_version=1, forage_halls=[[5, 5]])
            world = {'towns': {'a': town}}
            if worker:
                town['buildings'].append({'kind': 'woodcutter', 'x': 7, 'y': 7})
                command(town, {'action': 'assign_settler', 'index': 0, 'building_index': 1})
            else:
                town['villagers'] = []
                prepare_landscapes(world)
                command(town, {'action': 'chop'})
            for _ in range(80):
                tick(world)
                if tree not in town['map_resources']:
                    break
            self.assertNotIn(tree, town['map_resources'])
            self.assertEqual(len(town['tree_stumps']), 1)
