import copy
import json
import unittest

from npcs import NEW_LOCALS, ROSTER_VERSION, WORKER_TOOLS, ensure_npcs, local_key
from server import command, new_town, prepare_landscapes, tick
from shared_world import bind_map, export_world, new_world


class Toolsmiths(unittest.TestCase):
    def setUp(self):
        self.town = new_town('Tools')
        self.town['npcs'] = [{'name': name, 'x': 5, 'y': 8}
                             for name in ('Fern', 'Bram', 'Runa', 'Sylva')]

    def buy(self, index, item):
        command(self.town, {'action': 'buy_worker_tool', 'npc_index': index, 'item': item})

    def test_grower_gift_is_once_and_survives_reload(self):
        command(self.town, {'action': 'claim_local_gift', 'npc_index': 0})
        self.town = json.loads(json.dumps(self.town))
        before = copy.deepcopy(self.town)
        with self.assertRaises(ValueError):
            command(self.town, {'action': 'claim_local_gift', 'npc_index': 0})
        self.assertEqual(self.town, before)
        self.assertEqual(self.town['player']['worker_tools'], {'fine_sickle': 1})

    def test_shops_charge_for_each_copy_and_tools_can_be_assigned_and_returned(self):
        self.town['coins'] = 300
        for index, key in ((1, 'steel_axe'), (1, 'masterwork_axe'),
                           (2, 'steel_sickle'), (2, 'masterwork_sickle')):
            with self.subTest(item=key):
                coins = self.town['coins']
                self.buy(index, key)
                self.assertEqual(self.town['coins'], coins - WORKER_TOOLS[key]['price'])
                command(self.town, {'action': 'give_worker_tool', 'index': 0, 'item': key})
                self.town = json.loads(json.dumps(self.town))
                command(self.town, {'action': 'transfer_worker_tool', 'index': 0,
                                    'target_index': 1, 'item': key})
                command(self.town, {'action': 'take_worker_tool', 'index': 1})
                self.assertEqual(self.town['player']['worker_tools'][key], 1)
        self.buy(1, 'steel_axe')
        self.assertEqual(self.town['player']['worker_tools']['steel_axe'], 2)

    def test_invalid_purchases_are_atomic(self):
        self.town['coins'] = 100
        for index, key in ((True, 'steel_axe'), (-1, 'steel_axe'), (99, 'steel_axe'),
                           (1, []), (1, 'unknown'), (1, 'steel_sickle'),
                           (2, 'masterwork_axe'), (3, 'steel_axe')):
            before = copy.deepcopy(self.town)
            with self.assertRaises(ValueError):
                self.buy(index, key)
            self.assertEqual(self.town, before)
        for changes in ({'coins': 29}, {'coins': 100, 'player': {'x': 30, 'y': 8}}):
            self.town.update(changes)
            before = copy.deepcopy(self.town)
            with self.assertRaises(ValueError):
                self.buy(1, 'steel_axe')
            self.assertEqual(self.town, before)

    def test_tool_tiers_change_actual_matching_production(self):
        for key, tool in WORKER_TOOLS.items():
            if tool['job'] not in ('farming', 'woodcutting'):
                continue
            for job in ('farming', 'woodcutting'):
                with self.subTest(tool=key, job=job):
                    town = new_town('Production')
                    for worker in town['villagers']:
                        worker['talents'] = {}
                    kind = 'farm' if job == 'farming' else 'woodcutter'
                    town['buildings'].append({'kind': kind, 'x': 6, 'y': 2})
                    town['villagers'][0].update(job=job, worker_tool=key)
                    if job == 'farming':
                        town['buildings'][1]['field'] = {'stage': 'ripe', 'crop': 'vegetables',
                                                        'elapsed': 0, 'progress': 0, 'cycle': 0}
                        town['villagers'][0]['construction_position'] = {'x': 6, 'y': 3}
                    tick({'towns': {'own': town}})
                    output, rate = ('vegetables', 10) if job == 'farming' else ('wood', .75)
                    bonus = tool['bonus'] if tool['job'] == job else 0
                    self.assertAlmostEqual(town['buildings'][1]['pending_output'][output],
                                           rate * (1 + bonus))

    def test_existing_save_adds_locals_without_moving_old_npcs_or_resetting_visits(self):
        town = new_town('Legacy')
        town['npcs'] = [n for n in town['npcs'] if n['name'] not in NEW_LOCALS]
        town.pop('npc_roster_version')
        old = copy.deepcopy(town['npcs'])
        town['local_visits'] = {local_key(old[0]): {'gift_claimed': True, 'story_stage': 2}}
        progress = copy.deepcopy(town['local_visits'])
        ensure_npcs(town)
        self.assertEqual(town['npcs'][:len(old)], old)
        self.assertEqual({n['name'] for n in town['npcs'][len(old):]}, NEW_LOCALS)
        self.assertEqual(town['local_visits'], progress)
        saved = copy.deepcopy(town)
        ensure_npcs(town)
        self.assertEqual(town, saved)

    def test_shared_and_chunked_saves_migrate_and_purchases_remain_personal(self):
        for chunked in (False, True):
            with self.subTest(chunked=chunked):
                world = new_world(seed=42, explorable=chunked)
                world['towns'] = {name: new_town(name, world) for name in ('one', 'two')}
                landscape = world['shared_map']
                landscape['npcs'] = [n for n in landscape['npcs'] if n['name'] not in NEW_LOCALS]
                landscape.pop('npc_roster_version', None)
                bind_map(world)
                old = copy.deepcopy(landscape['npcs'])
                prepare_landscapes(world)
                bind_map(world)
                self.assertEqual(landscape['npcs'][:len(old)], old)
                self.assertEqual({n['name'] for n in landscape['npcs'][len(old):]}, NEW_LOCALS)
                self.assertEqual(landscape['npc_roster_version'], ROSTER_VERSION)
                index = next(i for i, npc in enumerate(landscape['npcs']) if npc['name'] == 'Bram')
                town = world['towns']['one']
                npc = landscape['npcs'][index]
                town['player'].update(x=npc['x'], y=npc['y'])
                town['coins'] = 30
                command(town, {'action': 'buy_worker_tool', 'npc_index': index,
                               'item': 'steel_axe'}, world)
                self.assertNotIn('worker_tools', world['towns']['two']['player'])
                restored = json.loads(json.dumps(export_world(world)))
                bind_map(restored)
                self.assertEqual(restored['towns']['one']['player']['worker_tools'], {'steel_axe': 1})


if __name__ == '__main__':
    unittest.main()
