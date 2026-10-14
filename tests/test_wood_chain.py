from fixtures import flat_town
import copy
import json
import unittest

import logistics
import npcs
import progression as economy
import server


class WoodChain(unittest.TestCase):
    def setUp(self):
        self.town = flat_town('Wood chain')
        self.town.update(level=4, terrain=['g' * 16] * 16, map_size=16,
                         map_resources=[], npcs=[], town_origin={'x': 5, 'y': 5})
        for worker in self.town['villagers']:
            worker['talents'] = {}

    def add(self, kind, **extra):
        building = dict(kind=kind, x=7, y=5, **extra)
        self.town['buildings'].append(building)
        return building, len(self.town['buildings']) - 1

    def test_worker_upgrades_are_separate_from_storage_and_stop_at_limits(self):
        for kind, capacities in [('woodcutter', [1, 2, 3]), ('sawpit', [1, 2]), ('workshop', [1])]:
            with self.subTest(kind=kind):
                self.setUp()
                building, index = self.add(kind)
                self.town['wood'] = 100
                for tier, capacity in enumerate(capacities, 1):
                    self.assertEqual(server.settler_capacity(building), capacity)
                    self.assertEqual(economy.storage_size(building), 50)
                    if tier < len(capacities):
                        before = self.town['wood']
                        server.command(self.town, {'action': 'upgrade_capacity', 'index': index})
                        self.assertEqual(self.town['wood'], before - 20 * tier)
                        self.assertEqual(building['construction']['total'], 30 * tier)
                        self.assertEqual(server.settler_capacity(building), capacity)
                        for _ in range(100):
                            server.construct(self.town, self.town)
                            if 'construction' not in building:
                                break
                        self.assertNotIn('construction', building)
                before = copy.deepcopy(self.town)
                with self.assertRaises(ValueError):
                    server.command(self.town, {'action': 'upgrade_capacity', 'index': index})
                self.assertEqual(self.town, before)

    def test_storage_upgrade_allows_processing_past_base_output_limit(self):
        building, _ = self.add('sawpit', level=2, input_buffer={'wood': 20},
                               output_buffer={'planks': 20})
        worker = dict(job='woodworking', woodcutting=10, talents={})
        self.assertEqual(economy.storage_allocations(building), {'wood': 37.5, 'planks': 25})
        self.assertEqual(logistics.produce(self.town, building, worker, elapsed=6), 'Working')
        self.assertEqual(worker['pending_output'], {'planks': 1})
        self.assertEqual(building['input_buffer'], {'wood': 18})
        self.assertEqual(server.production_bonus(building), 0)
        for _ in range(4):
            self.assertEqual(logistics.produce(self.town, building, worker, elapsed=6), 'Working')
        self.assertEqual(worker['pending_output'], {'planks': 5})
        self.assertEqual(logistics.produce(self.town, building, worker, elapsed=6), 'Workplace buffer full')
        self.assertEqual(building['input_buffer'], {'wood': 10})

    def test_crafted_saw_scales_real_inputs_without_accelerating_training(self):
        building, _ = self.add('sawpit', input_buffer={'wood': 5})
        worker = dict(job='woodworking', woodcutting=10, talents={}, worker_tool='crafted_saw')
        self.assertEqual(logistics.requirements(building, worker), {'wood': 2.5})
        self.assertEqual(logistics.produce(self.town, building, worker, elapsed=6), 'Working')
        self.assertEqual(building['input_buffer'], {'wood': 2.5})
        self.assertEqual(worker['pending_output'], {'planks': 1.25})
        self.assertEqual(worker['woodworking'], .08)
        self.assertEqual(npcs.tool_bonus({'worker_tool': 'crafted_axe'}, 'woodcutting'), .25)
        self.assertEqual(npcs.tool_bonus({'worker_tool': 'crafted_axe'}, 'fishing'), 0)

    def test_tool_assignment_transfer_and_return_preserve_exactly_one_item(self):
        self.town['goods'] = {'crafted_saw': 1}
        economy.ensure_inventory(self.town)
        server.command(self.town, dict(action='give_worker_tool', index=0, item='crafted_saw'))
        self.assertEqual(economy.quantity(self.town, 'crafted_saw'), 0)
        with self.assertRaises(ValueError):
            server.command(self.town, dict(action='give_worker_tool', index=1, item='crafted_saw'))
        server.command(self.town, dict(action='transfer_worker_tool', index=0, target_index=1, item='crafted_saw'))
        self.assertNotIn('worker_tool', self.town['villagers'][0])
        server.command(self.town, dict(action='take_worker_tool', index=1))
        self.assertEqual(self.town['player']['worker_tools']['crafted_saw'], 1)
        server.command(self.town, dict(action='give_worker_tool', index=0, item='crafted_saw'))
        self.assertEqual(self.town['player']['worker_tools']['crafted_saw'], 0)
        self.assertEqual(self.town['villagers'][0]['worker_tool'], 'crafted_saw')

    def test_legacy_migration_preserves_residents_pending_goods_and_existing_tools(self):
        self.town.pop('wood_chain_version')
        building, index = self.add('sawpit', settler_level=3, profession_upgrades=['saw'],
                                   input_buffer={'wood': 5}, construction={'progress': 10, 'total': 30,
                                   'upgrade': {'profession_upgrades': ['saw']}})
        for worker in self.town['villagers'][:3]:
            worker.update(job='woodworking', building_index=index, woodcutting=10)
        self.town['villagers'][0]['worker_tool'] = 'fine_axe'
        logistics.record_output(self.town, building, 'planks', 1, self.town['villagers'][2])
        residents, wood = len(self.town['villagers']), self.town['wood']
        server.migrate_wood_chain(self.town)
        self.assertEqual(server.settler_capacity(building), 2)
        self.assertEqual(server.assigned_settlers(self.town, index), [0, 1])
        self.assertEqual(self.town['villagers'][2]['job'], 'idle')
        self.assertEqual(self.town['villagers'][2]['woodcutting'], 10)
        self.assertEqual(self.town['villagers'][0]['worker_tool'], 'fine_axe')
        self.assertEqual(self.town['villagers'][1]['worker_tool'], 'crafted_saw')
        self.assertEqual(self.town['player']['worker_tools']['crafted_saw'], 1)
        self.assertEqual(logistics.collection_inventory(building)['planks'], 1)
        self.assertEqual(building['input_buffer'], {'wood': 5})
        self.assertEqual(building['construction']['progress'], 10)
        self.assertEqual((len(self.town['villagers']), self.town['wood']), (residents, wood))
        restored = json.loads(json.dumps(self.town))
        before = copy.deepcopy(restored)
        server.migrate_wood_chain(restored)
        self.assertEqual(restored, before)

    def test_decorations_consume_furniture_and_reuse_placement_and_removal_rules(self):
        for kind, good in [('wooden_bench', 'furniture'), ('wooden_statue', 'carved_furniture')]:
            with self.subTest(kind=kind):
                self.setUp()
                before = copy.deepcopy(self.town)
                with self.assertRaises(ValueError):
                    server.command(self.town, dict(action='build', kind=kind, x=7, y=5))
                self.assertEqual(self.town, before)
                self.town['goods'] = {good: 1}
                server.command(self.town, dict(action='build', kind=kind, x=7, y=5))
                building = self.town['buildings'][-1]
                self.assertNotIn('construction', building)
                self.assertEqual(economy.quantity(self.town, good), 0)
                self.assertEqual(economy.storage_size(building), 0)
                self.assertEqual(server.settler_capacity(building), 0)
                for action in ('upgrade_capacity', 'upgrade_building'):
                    with self.assertRaises(ValueError):
                        server.command(self.town, dict(action=action, index=1))
                with self.assertRaises(ValueError):
                    server.command(self.town, dict(action='build', kind='home', x=7, y=5))
                server.command(self.town, dict(action='move', index=1, x=8, y=5))
                self.assertEqual(building['x'], 8)
                server.command(self.town, dict(action='destroy', index=1))
                self.assertEqual(economy.quantity(self.town, good), 0)


if __name__ == '__main__':
    unittest.main()
