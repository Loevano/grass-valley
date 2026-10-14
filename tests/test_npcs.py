import copy
import json
import unittest
from unittest.mock import patch

import progression as economy

from npcs import advance_trades, ensure_npcs, local_key
from server import command, ensure_resources, new_town, prepare_landscapes, tick
from shared_world import bind_map, new_world
from chunk_world import chunk_key


class BiomeLocals(unittest.TestCase):
    def test_saved_clothing_locals_explain_perks_without_moving_or_resetting_visits(self):
        town = new_town('Clothing')
        locals_ = [npc for npc in town['npcs'] if npc['name'] in {'Dara', 'Petra'}]
        for npc in locals_:
            npc['dialogue'] = 'Old clothing introduction'
        town['local_visits'] = {local_key(npc): {'gift_claimed': True} for npc in locals_}
        before = copy.deepcopy(town)
        ensure_npcs(town)
        self.assertEqual(town['local_visits'], before['local_visits'])
        for npc, old in zip(town['npcs'], before['npcs']):
            self.assertEqual((npc['x'], npc['y']), (old['x'], old['y']))
            if npc['name'] in {'Dara', 'Petra'}:
                self.assertIn('20% more', npc['dialogue'])

    def test_chunked_relocation_reads_saved_resources_and_preserves_visits(self):
        world = new_world(seed=42, explorable=True)
        world['towns']['one'] = new_town('one', world)
        landscape = world['shared_map']
        npc = next(n for n in landscape['npcs'] if n['name'] == 'Sylva')
        old_key = local_key(npc)
        world['towns']['one']['local_visits'] = {old_key: {'gift_claimed': True}}
        landscape.pop('npc_placement_version')
        key = chunk_key(npc['x'], npc['y'])
        record = world['_chunks'].read(key)
        tree = {'kind': 'tree', 'x': npc['x'], 'y': npc['y']}
        record['map_resources'].append(tree)
        if key in world['_chunks'].records:
            landscape['map_resources'].append(tree)
        prepare_landscapes(world)
        self.assertNotEqual(local_key(npc), old_key)
        self.assertEqual(world['towns']['one']['local_visits'], {local_key(npc): {'gift_claimed': True}})

    def test_shared_relocation_preserves_every_towns_visit_progress(self):
        world = new_world(seed=42)
        world['towns'] = {name: new_town(name, world) for name in ('one', 'two')}
        landscape = world['shared_map']
        npc = next(n for n in landscape['npcs'] if n['name'] == 'Sylva')
        old_key = local_key(npc)
        for town in world['towns'].values():
            town['local_visits'] = {old_key: {'met': True, 'story_stage': 2}}
        landscape.pop('npc_placement_version')
        landscape['map_resources'].append({'kind': 'tree', 'x': npc['x'], 'y': npc['y']})
        prepare_landscapes(world)
        bind_map(world)
        self.assertNotEqual(local_key(npc), old_key)
        for town in world['towns'].values():
            self.assertEqual(town['local_visits'], {local_key(npc): {'met': True, 'story_stage': 2}})

    def test_canopies_and_regrowing_trees_do_not_cover_new_or_saved_locals(self):
        town = {'terrain': ['g' * 24] * 24, 'buildings': [],
                'map_resources': [{'kind': 'tree', 'x': 15, 'y': 15}],
                'tree_stumps': [{'kind': 'tree', 'x': 20, 'y': 20}],
                'npcs': [{'name': 'Tobin', 'biome': 'g', 'x': 14, 'y': 14}],
                'npc_roster_version': 3,
                'local_visits': {'Tobin:14:14': {'met': True, 'gift_claimed': True, 'story_stage': 2}}}
        ensure_npcs(town)
        npc = town['npcs'][0]
        self.assertNotEqual((npc['x'], npc['y']), (14, 14))
        self.assertEqual(town['local_visits'], {local_key(npc):
                         {'met': True, 'gift_claimed': True, 'story_stage': 2}})
        saved = copy.deepcopy(town)
        ensure_npcs(town)
        self.assertEqual(town, saved)
        del town['npcs']
        ensure_npcs(town)
        for npc in town['npcs']:
            for tree in town['map_resources'] + town['tree_stumps']:
                dx, dy = tree['x'] - npc['x'], tree['y'] - npc['y']
                self.assertFalse(dx >= 0 and dy >= 0 and abs(dx - dy) <= 1 and dx + dy <= 5)

    def test_locals_match_biomes_and_spawn_on_clear_tiles(self):
        town = new_town('Locals')
        occupied = {(b['x'], b['y']) for b in town['buildings'] + town['map_resources']}
        positions = set()
        for npc in town['npcs']:
            tile = npc['x'], npc['y']
            self.assertNotIn(tile, occupied | positions)
            self.assertEqual(town['terrain'][npc['y']][npc['x']], npc['biome'])
            self.assertTrue(npc['name'] and npc['role'] and npc['dialogue'])
            positions.add(tile)
        self.assertEqual({n['role'] for n in town['npcs'] if n['biome'] == 'b'},
                         {'Swamp herbalist', 'Marsh fisher', 'Marsh rod maker', 'Marsh fish trader'})

    def test_spawns_are_saved_deterministic_and_migrate_without_changing_town(self):
        town = new_town('Saved')
        before = copy.deepcopy(town)
        del town['npcs']
        ensure_npcs(town)
        self.assertEqual(town, before)
        saved = json.loads(json.dumps(town))
        ensure_resources(saved)
        self.assertEqual(saved, town)

    def test_locals_do_not_count_as_workers_or_consume_food(self):
        town = new_town('Quiet')
        without = copy.deepcopy(town)
        without['npcs'] = []
        tick({'towns': {'with': town, 'without': without}})
        for key in ('food', 'wood', 'coins', 'villagers'):
            self.assertEqual(town[key], without[key])

    def test_building_and_relocation_cannot_cover_a_local(self):
        town = new_town('Reserved')
        npc = town['npcs'][0]
        before = copy.deepcopy(town)
        for action in ('build', 'move'):
            with self.assertRaisesRegex(ValueError, 'local'):
                command(town, {'action': action, 'kind': 'home', 'index': 0,
                               'x': npc['x'], 'y': npc['y']})
            self.assertEqual(town, before)

    def test_fully_occupied_biome_has_no_spawn(self):
        town = {'terrain': ['b' * 14] * 14, 'buildings': [],
                'map_resources': [{'x': x, 'y': y} for x in range(14) for y in range(14)]}
        ensure_npcs(town)
        self.assertEqual(town['npcs'], [])


class LocalOffers(unittest.TestCase):
    def setUp(self):
        self.town = new_town('Gifts')
        self.town['npcs'] = [{'name': name, 'x': 5, 'y': 8}
                             for name in ('Sylva', 'Reed', 'Tobin', 'Clover', 'Moss')]

    def action(self, action, **data):
        return command(self.town, {'action': action, **data})

    def reject(self, action, **data):
        before = copy.deepcopy(self.town)
        with self.assertRaises(ValueError):
            self.action(action, **data)
        self.assertEqual(self.town, before)

    def test_talking_requires_proximity_and_saves_personal_discovery(self):
        self.reject('interact_local', npc_index=True)
        self.town['player']['x'] = 30
        self.reject('interact_local', npc_index=0)
        self.town['player']['x'] = 5
        other = copy.deepcopy(self.town)
        self.action('interact_local', npc_index=0)
        key = local_key(self.town['npcs'][0])
        self.assertTrue(self.town['local_visits'][key]['met'])
        self.assertNotIn(key, other.get('local_visits', {}))
        self.town = json.loads(json.dumps(self.town))
        self.action('interact_local', npc_index=0)
        self.assertEqual(self.town['local_visits'][key], {'met': True})

    def test_gifts_require_proximity_and_are_once_per_town_after_reload(self):
        self.reject('claim_local_gift', npc_index=True)
        self.reject('claim_local_gift', npc_index=4)
        self.town['player']['x'] = 30
        self.reject('claim_local_gift', npc_index=0)
        self.town['player']['x'] = 5
        other = copy.deepcopy(self.town)
        other['npcs'] = self.town['npcs']
        for index, key in ((0, 'fine_axe'), (1, 'fine_fishing_rod')):
            self.action('claim_local_gift', npc_index=index)
            self.assertEqual(self.town['player']['worker_tools'][key], 1)
            command(other, {'action': 'claim_local_gift', 'npc_index': index})
            self.assertEqual(other['player']['worker_tools'][key], 1)
        self.town = json.loads(json.dumps(self.town))
        self.reject('claim_local_gift', npc_index=0)
        self.reject('claim_local_gift', npc_index=1)

    def test_tools_transfer_without_duplication(self):
        self.reject('give_worker_tool', index=0, item='fine_axe')
        self.action('claim_local_gift', npc_index=0)
        self.reject('give_worker_tool', index=True, item='fine_axe')
        self.reject('give_worker_tool', index=0, item=[])
        self.action('give_worker_tool', index=0, item='fine_axe')
        self.reject('give_worker_tool', index=1, item='fine_axe')
        self.action('claim_local_gift', npc_index=1)
        self.reject('give_worker_tool', index=0, item='fine_fishing_rod')
        self.town = json.loads(json.dumps(self.town))
        self.assertEqual(self.town['villagers'][0]['worker_tool'], 'fine_axe')
        self.action('take_worker_tool', index=0)
        self.reject('take_worker_tool', index=0)
        self.action('give_worker_tool', index=1, item='fine_axe')
        self.assertEqual(self.town['player']['worker_tools']['fine_axe'], 0)
        self.assertNotIn('worker_tool', self.town['villagers'][0])

    def test_direct_tool_transfer_is_atomic_and_preserves_inventory(self):
        self.action('claim_local_gift', npc_index=0)
        self.action('give_worker_tool', index=0, item='fine_axe')
        for source, target, item in ((True, 1, 'fine_axe'), (0, True, 'fine_axe'),
                                    (0, -1, 'fine_axe'), (0, 99, 'fine_axe'),
                                    (0, 0, 'fine_axe'), (1, 0, 'fine_axe'),
                                    (0, 1, 'fine_fishing_rod'), (0, 1, [])):
            self.reject('transfer_worker_tool', index=source, target_index=target, item=item)
        self.action('claim_local_gift', npc_index=1)
        self.action('give_worker_tool', index=1, item='fine_fishing_rod')
        self.reject('transfer_worker_tool', index=0, target_index=1, item='fine_axe')
        self.action('take_worker_tool', index=1)
        pool = copy.deepcopy(self.town['player']['worker_tools'])
        self.action('transfer_worker_tool', index=0, target_index=1, item='fine_axe')
        self.assertNotIn('worker_tool', self.town['villagers'][0])
        self.assertEqual(self.town['villagers'][1]['worker_tool'], 'fine_axe')
        self.assertEqual(self.town['player']['worker_tools'], pool)
        self.reject('transfer_worker_tool', index=0, target_index=1, item='fine_axe')
        self.town = json.loads(json.dumps(self.town))
        ensure_resources(self.town)
        self.assertEqual(self.town['villagers'][1]['worker_tool'], 'fine_axe')
        self.action('transfer_worker_tool', index=1, target_index=0, item='fine_axe')
        self.assertEqual(self.town['player']['worker_tools'], pool)

    def test_barter_is_atomic_and_restocking_is_saved_and_personal(self):
        self.reject('trade_local', npc_index=1)
        self.town['player']['x'] = 30
        self.reject('trade_local', npc_index=2)
        self.town['player']['x'] = 5
        self.town['wood'] = 9
        self.reject('trade_local', npc_index=2)
        self.town['wood'] = 65
        economy.ensure_inventory(self.town)
        hall = self.town['buildings'][0]
        hall['stored_goods']['vegetables'] = 100
        self.town['food'] = 100
        self.reject('trade_local', npc_index=2)
        hall['stored_goods'].pop('vegetables')
        self.town['food'] = 40
        other = copy.deepcopy(self.town)
        self.action('trade_local', npc_index=2)
        self.assertEqual((self.town['wood'], self.town['food']), (55, 48))
        self.assertEqual(hall['stored_goods']['vegetables'], 8)
        self.reject('trade_local', npc_index=2)
        command(other, {'action': 'trade_local', 'npc_index': 2})
        self.action('trade_local', npc_index=3)
        self.town = json.loads(json.dumps(self.town))
        tick({'towns': {'own': self.town}})
        self.assertEqual(self.town['local_visits'][local_key(self.town['npcs'][2])]['trade_remaining'], 297)
        advance_trades(self.town, 296)
        self.reject('trade_local', npc_index=2)
        advance_trades(self.town, 1)
        wood, food = self.town['wood'], self.town['food']
        self.action('trade_local', npc_index=2)
        self.assertEqual((self.town['wood'], self.town['food']), (wood - 10, food + 8))

    def test_gift_tools_boost_matching_production_only(self):
        for job, kind, item, output, rate in (
                ('fishing', 'fishing_hut', 'fine_fishing_rod', 'fish', 1),
                ('woodcutting', 'woodcutter', 'fine_axe', 'wood', .75)):
            with self.subTest(job=job):
                town = new_town('Tools')
                for worker in town['villagers']:
                    worker['talents'] = {}
                town['buildings'].append({'kind': kind, 'x': 6, 'y': 2})
                worker = town['villagers'][0]
                worker.update(job=job, worker_tool=item)
                with patch('server.secrets.randbelow', return_value=1):
                    tick({'towns': {'own': town}})
                self.assertAlmostEqual(town['buildings'][1]['pending_output'][output], rate * 1.25)
                worker['worker_tool'] = 'fine_axe' if job == 'fishing' else 'fine_fishing_rod'
                with patch('server.secrets.randbelow', return_value=1):
                    tick({'towns': {'own': town}})
                self.assertAlmostEqual(town['buildings'][1]['pending_output'][output], rate * 2.25)

    def test_departing_worker_returns_gift_tool(self):
        town = self.town
        town['villagers'][0]['worker_tool'] = 'fine_axe'
        town['food'] = 0
        town['food_shortage_seconds'] = 60
        with patch('server.secrets.randbelow', return_value=0):
            tick({'towns': {'own': town}})
        self.assertEqual(town['player']['worker_tools']['fine_axe'], 1)


if __name__ == '__main__':
    unittest.main()
